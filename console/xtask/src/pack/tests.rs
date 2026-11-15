//! The pack over scratch trees with stub programs: the arguments, the tree's
//! checks, the ELF check, the pins and what is written. test/pack_release_test.sh
//! runs the script on the real checkout.

use super::*;
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use tehas_core::testdir::TestDir;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|a| a.to_string()).collect()
}

fn run_args(v: &[&str]) -> Args {
    match parse(&s(v)).unwrap() {
        Parsed::Run(a) => *a,
        Parsed::Help => panic!("help"),
    }
}

/// A program of one e_machine and one line of text.
fn program(path: &Path, machine: [u8; 2], text: &str) {
    let mut b = b"\x7fELF".to_vec();
    b.extend([0u8; 14]);
    b.extend(machine);
    b.extend(format!("\n{text}\n").as_bytes());
    fs::write(path, b).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

const X86: [u8; 2] = [0x3e, 0];
const ARM: [u8; 2] = [0xb7, 0];
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

struct Tree {
    dir: TestDir,
}

impl Tree {
    /// The smallest tehas tree the pack takes, a stub console beside it.
    fn new() -> Tree {
        let dir = TestDir::new("xtask-pack");
        let t = Tree { dir };
        for f in [
            "tree/install.sh",
            "tree/bootstrap.sh",
            "tree/CHANGELOG.md",
            "tree/bin/tehas",
            "tree/lib/config.sh",
            "tree/libexec/setup",
            "tree/instances/example/settings.env",
        ] {
            t.file(f, "x\n");
        }
        t.file(
            "tree/host/versions.env",
            "A=1\nPI_RUKAS_FOLLOW=release\nB=2\n",
        );
        t.file(
            "tree/lib/release.list",
            "# the list\n\n[dirs]\nbin\nlibexec\nlib\nhost\n\n[plain]\ninstall.sh\nbootstrap.sh\nCHANGELOG.md\n\n[extra]\ninstances/example\n",
        );
        program(&t.path("console"), X86, "the console");
        t
    }

    fn path(&self, p: &str) -> PathBuf {
        self.dir.join(p)
    }

    fn file(&self, p: &str, text: &str) {
        let p = self.path(p);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn root(&self) -> String {
        self.path("tree").display().to_string()
    }

    /// A pack of v9.9.9 into out/, with the stub console and `more`.
    fn pack(&self, more: &[&str]) -> Result<Packed, Fail> {
        let out = self.path("out").display().to_string();
        let console = self.path("console").display().to_string();
        let mut v = vec!["v9.9.9", out.as_str(), "--console", console.as_str()];
        v.extend(more);
        pack(&run_args(&v), Some(&self.root()), self.dir.path())
    }

    /// The files of the tarball, as tar lists them.
    fn listing(&self, tarball: &str) -> Vec<String> {
        let out = Command::new("tar")
            .args(["-tzf", tarball])
            .output()
            .unwrap();
        let mut v: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        v.sort();
        v
    }

    fn unpacked(&self, tarball: &str, file: &str) -> String {
        let out = Command::new("tar")
            .args(["-xzOf", tarball, &format!("tehas-v9.9.9/{file}")])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

fn code(r: Result<Packed, Fail>) -> (u8, String) {
    let f = r.unwrap_err();
    (f.code, f.msg)
}

#[test]
fn parse_reads_the_arguments_in_order() {
    let a = run_args(&[
        "v1.2.3",
        "dist",
        "--console",
        "c",
        "--mcp",
        "m",
        "--cycle",
        "y",
        "--arch",
        "aarch64",
    ]);
    assert_eq!(a.tag, "v1.2.3");
    assert_eq!(a.out_dir, "dist");
    assert_eq!(a.console, "c");
    assert_eq!(a.arch, "aarch64");
    assert_eq!(a.programs[1], "m");
    assert_eq!(a.programs[5], "y");
    assert!(a.programs[0].is_empty());
    assert_eq!(run_args(&[]).arch, "x86_64");
}

#[test]
fn parse_names_the_first_bad_argument() {
    let e = |v: &[&str]| parse(&s(v)).unwrap_err();
    assert_eq!(e(&["--console"]).msg, "--console needs a path");
    assert_eq!(e(&["--store"]).msg, "--store needs a path");
    assert_eq!(e(&["--arch"]).msg, "--arch needs x86_64 or aarch64");
    assert_eq!(e(&["--rukas-digest"]).msg, "--rukas-digest needs a digest");
    assert_eq!(e(&["--consolex", "y"]).msg, "unknown argument: --consolex");
    assert_eq!(e(&["a", "b", "c"]).msg, "too many arguments: c");
    assert_eq!(e(&["a", "b", "c"]).code, 64);
    // --help before an error wins, after one it is never read.
    assert_eq!(parse(&s(&["a", "--help", "--bad"])).unwrap(), Parsed::Help);
    assert_eq!(e(&["--bad", "-h"]).msg, "unknown argument: --bad");
}

#[test]
fn help_is_the_header_of_the_script() {
    let script =
        "#!/usr/bin/env bash\n# Pack it.\n#\n#   --arch  x\n#no space\nset -u\n# not this\n";
    assert_eq!(help(script), "Pack it.\n\n  --arch  x\nno space\n");
}

#[test]
fn the_tag_is_vxyz() {
    assert!(is_tag("v0.7.6"));
    assert!(is_tag("v10.0.123"));
    for bad in [
        "0.7.6",
        "v0.7",
        "v0.7.6.1",
        "v0..6",
        "v0.7.x",
        "V0.7.6",
        "v0.7.6-rc1",
    ] {
        assert!(!is_tag(bad), "{bad}");
    }
}

#[test]
fn release_list_reads_one_section() {
    let text = "# c\n[dirs]\nbin\n\n  \n# lib\nlib\n[plain]\nx.sh\n[dirs]\nhost\n";
    assert_eq!(release_list(text, "dirs"), s(&["bin", "lib", "host"]));
    assert_eq!(release_list(text, "plain"), s(&["x.sh"]));
    assert!(release_list(text, "extra").is_empty());
}

#[test]
fn pin_env_puts_the_pin_last_and_once() {
    assert_eq!(
        pin_env("A=1\nK=old\nB=2\nK=x\n", "K", "new").unwrap(),
        "A=1\nB=2\nK=new\n"
    );
    // A last line without its newline is not glued to the pin.
    assert_eq!(pin_env("A=1", "K", "v").unwrap(), "A=1\nK=v\n");
    // A key that only starts another is kept.
    assert_eq!(pin_env("KEY=1\n", "K", "v").unwrap(), "KEY=1\nK=v\n");
    // Nothing left is a failure, as grep -v made it.
    assert_eq!(pin_env("K=1\n", "K", "v"), None);
    assert_eq!(pin_env("", "K", "v"), None);
}

#[test]
fn logical_takes_dots_as_cd_does() {
    let base = Path::new("/a/b");
    assert_eq!(
        logical(base, Path::new("c/../d/./")),
        PathBuf::from("/a/b/d")
    );
    assert_eq!(logical(base, Path::new("/x//y/..")), PathBuf::from("/x"));
}

#[test]
fn machine_is_read_at_offset_18() {
    let t = Tree::new();
    assert_eq!(machine_of(&t.path("console")).unwrap(), "3e00");
    t.file("short", "\x7fELF\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x3e");
    assert_eq!(machine_of(&t.path("short")).unwrap(), "3e");
    t.file("empty", "");
    assert_eq!(machine_of(&t.path("empty")).unwrap(), "");
}

#[test]
fn a_pack_writes_the_tarball_and_its_hash() {
    let t = Tree::new();
    let p = t.pack(&["--commit", COMMIT]).unwrap();
    let tarball = t.path("out/tehas-v9.9.9-linux-x86_64.tar.gz");
    assert_eq!(p.tarball, tarball.display().to_string());
    let bytes = fs::read(&tarball).unwrap();
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        p.sha_line,
        format!("{hash}  tehas-v9.9.9-linux-x86_64.tar.gz")
    );
    assert_eq!(
        fs::read_to_string(t.path("out/tehas-v9.9.9-linux-x86_64.tar.gz.sha256")).unwrap(),
        format!("{}\n", p.sha_line)
    );
    assert_eq!(fs::read_dir(t.path("out")).unwrap().count(), 2);
    let list = t.listing(&p.tarball);
    for want in [
        "tehas-v9.9.9/",
        "tehas-v9.9.9/VERSION",
        "tehas-v9.9.9/COMMIT",
        "tehas-v9.9.9/install.sh",
        "tehas-v9.9.9/bin/tehas",
        "tehas-v9.9.9/libexec/console",
        "tehas-v9.9.9/instances/example/settings.env",
    ] {
        assert!(list.contains(&want.to_string()), "{want} in {list:?}");
    }
    assert!(list.iter().all(|f| f.starts_with("tehas-v9.9.9/")));
    assert!(!list
        .iter()
        .any(|f| f.contains("release.list.") || f.ends_with("console.partial")));
    assert_eq!(t.unpacked(&p.tarball, "VERSION"), "v9.9.9\n");
    assert_eq!(t.unpacked(&p.tarball, "COMMIT"), format!("{COMMIT}\n"));
    assert!(t
        .unpacked(&p.tarball, "libexec/console")
        .contains("the console"));
    // No pins: versions.env is the tree's.
    assert_eq!(
        t.unpacked(&p.tarball, "host/versions.env"),
        "A=1\nPI_RUKAS_FOLLOW=release\nB=2\n"
    );
}

#[test]
fn the_programs_go_to_libexec_executable() {
    let t = Tree::new();
    program(&t.path("mcp"), X86, "mcp");
    fs::set_permissions(t.path("mcp"), fs::Permissions::from_mode(0o700)).unwrap();
    let mcp = t.path("mcp").display().to_string();
    let p = t.pack(&["--mcp", &mcp]).unwrap();
    assert!(t.unpacked(&p.tarball, "libexec/tehas-mcp").contains("mcp"));
    let out = Command::new("tar")
        .args(["-tvzf", &p.tarball])
        .output()
        .unwrap();
    let verbose = String::from_utf8(out.stdout).unwrap();
    let line = verbose
        .lines()
        .find(|l| l.ends_with("libexec/tehas-mcp"))
        .unwrap();
    assert!(line.starts_with("-rwxr-xr-x"), "{line}");
    assert!(!t
        .listing(&p.tarball)
        .iter()
        .any(|f| f.ends_with("libexec/store")));
}

#[test]
fn the_pins_are_the_last_lines_in_lower_case() {
    let t = Tree::new();
    let digest = format!("sha256:{}", "A".repeat(64));
    let p = t
        .pack(&[
            "--rukas-commit",
            &COMMIT.to_uppercase(),
            "--rukas-digest",
            &digest,
        ])
        .unwrap();
    assert_eq!(
        t.unpacked(&p.tarball, "host/versions.env"),
        format!(
            "A=1\nB=2\nPI_RUKAS_FOLLOW=commit:{COMMIT}\nPI_RUKAS_IMAGE_DIGEST=sha256:{}\n",
            "a".repeat(64)
        )
    );
}

#[test]
fn bad_values_are_refused_before_anything_is_written() {
    let t = Tree::new();
    let digest = format!("sha256:{}", "a".repeat(64));
    let cases: Vec<(Vec<&str>, u8, &str)> = vec![
        (
            vec!["--commit", "deadbeef"],
            1,
            "--commit must be 40 hex digits: deadbeef",
        ),
        (
            vec!["--rukas-commit", "xyz", "--rukas-digest", &digest],
            1,
            "40 hex digits",
        ),
        (
            vec!["--rukas-commit", COMMIT, "--rukas-digest", "sha256:ab"],
            1,
            "sha256: and 64 hex",
        ),
        (
            vec!["--rukas-commit", COMMIT],
            64,
            "name both pins or neither",
        ),
        (
            vec!["--rukas-digest", &digest],
            64,
            "name both pins or neither",
        ),
        (
            vec!["--arch", "riscv"],
            64,
            "--arch must be x86_64 or aarch64, got: riscv",
        ),
        (vec!["--arch", "aarch64"], 1, "not a aarch64 program"),
    ];
    for (more, want_code, want) in cases {
        let (c, msg) = code(t.pack(&more));
        assert_eq!(c, want_code, "{more:?}: {msg}");
        assert!(msg.contains(want), "{more:?}: {msg}");
        assert!(!t.path("out").exists(), "{more:?} wrote the out dir");
    }
}

#[test]
fn the_tag_out_dir_and_console_are_needed() {
    let t = Tree::new();
    let pwd = t.dir.path();
    let root = t.root();
    let try_ = |v: &[&str]| code(pack(&run_args(v), Some(&root), pwd));
    assert_eq!(try_(&[]), (64, "name the tag".into()));
    assert_eq!(
        try_(&["9.9.9"]),
        (64, "the tag must be vX.Y.Z, got: 9.9.9".into())
    );
    assert_eq!(try_(&["v9.9.9"]), (64, "name the output directory".into()));
    assert_eq!(
        try_(&["v9.9.9", "out"]),
        (64, "--console names the console to pack".into())
    );
    assert_eq!(
        try_(&["v9.9.9", "out", "--console", "nowhere"]),
        (64, "no console at: nowhere".into())
    );
    t.file("plain", "x");
    assert_eq!(
        try_(&["v9.9.9", "out", "--console", "plain"]),
        (64, "the console is not executable: plain".into())
    );
    assert_eq!(
        try_(&["v9.9.9", "out", "--console", "console", "--store", "plain"]),
        (64, "not a program the pack can run: plain".into())
    );
}

#[test]
fn a_tree_that_is_not_tehas_or_misses_a_file_is_refused() {
    let t = Tree::new();
    let (c, msg) = code(pack(
        &run_args(&["v9.9.9", "out", "--console", "console"]),
        Some("elsewhere"),
        t.dir.path(),
    ));
    assert_eq!(
        (c, msg.as_str()),
        (64, "not a tehas tree to pack: elsewhere")
    );
    // Without TEHAS_PACK_ROOT the tree is the working directory.
    let (c, msg) = code(pack(
        &run_args(&["v9.9.9", "out", "--console", "../../console"]),
        None,
        &t.path("tree/lib"),
    ));
    assert_eq!(c, 64);
    assert!(msg.ends_with("tree/lib"), "{msg}");

    fs::remove_file(t.path("tree/bootstrap.sh")).unwrap();
    let (c, msg) = code(t.pack(&[]));
    assert_eq!(
        (c, msg),
        (1, format!("bootstrap.sh is missing from {}", t.root()))
    );
    t.file("tree/lib/release.list", "[dirs]\n[plain]\n");
    let (c, msg) = code(t.pack(&[]));
    assert_eq!(
        (c, msg),
        (
            1,
            format!("{}/lib/release.list names nothing to pack", t.root())
        )
    );
    fs::remove_file(t.path("tree/lib/release.list")).unwrap();
    let (c, msg) = code(t.pack(&[]));
    assert_eq!(
        (c, msg),
        (1, format!("lib/release.list is missing from {}", t.root()))
    );
}

#[test]
fn the_trees_own_console_is_refused_by_path_and_by_content() {
    let t = Tree::new();
    program(&t.path("tree/libexec/console"), X86, "built in the tree");
    let own = t.path("tree/libexec/console").display().to_string();
    let (c, msg) = code(t.pack(&["--console", &own]));
    assert_eq!(c, 64);
    assert_eq!(
        msg,
        format!("--console is the tree's own build of it: {own}")
    );
    fs::copy(t.path("tree/libexec/console"), t.path("copy")).unwrap();
    let copy = t.path("copy").display().to_string();
    assert_eq!(code(t.pack(&["--console", &copy])).0, 64);
    // Another console is packed in its place.
    let p = t.pack(&[]).unwrap();
    assert!(t
        .unpacked(&p.tarball, "libexec/console")
        .contains("the console"));
}

#[test]
fn a_relative_console_is_read_from_the_tree_once_it_is_checked() {
    // The working directory and the tree both hold a console: the check reads
    // the first, the copy the second, as the script did after its cd.
    let t = Tree::new();
    program(&t.path("tree/console-build"), X86, "the tree's build");
    program(&t.path("console-build"), X86, "the working directory's");
    let p = pack(
        &run_args(&["v9.9.9", "dist", "--console", "console-build"]),
        Some(&t.root()),
        t.dir.path(),
    )
    .unwrap();
    assert_eq!(p.tarball, "dist/tehas-v9.9.9-linux-x86_64.tar.gz");
    let full = t
        .path("tree/dist/tehas-v9.9.9-linux-x86_64.tar.gz")
        .display()
        .to_string();
    assert!(t
        .unpacked(&full, "libexec/console")
        .contains("the tree's build"));
}

#[test]
fn a_program_of_the_other_architecture_is_refused() {
    let t = Tree::new();
    program(&t.path("arm"), ARM, "arm");
    let arm = t.path("arm").display().to_string();
    let (c, msg) = code(t.pack(&["--analytics", &arm]));
    assert_eq!(c, 1);
    assert_eq!(
        msg,
        format!("not a x86_64 program: {arm} (e_machine b700, want 3e00)")
    );
    let p = t.pack(&["--arch", "aarch64", "--console", &arm]).unwrap();
    assert!(p.tarball.ends_with("tehas-v9.9.9-linux-aarch64.tar.gz"));
}

#[test]
fn a_symbolic_link_in_the_tree_is_refused_and_named() {
    let t = Tree::new();
    std::os::unix::fs::symlink("/etc/hostname", t.path("tree/lib/out.sh")).unwrap();
    let (c, msg) = code(t.pack(&[]));
    assert_eq!(c, 1);
    assert!(msg.contains("holds a symbolic link"), "{msg}");
    assert!(msg.ends_with("\nlib/out.sh"), "{msg}");
    assert!(!t.path("out").exists());
}

#[test]
fn a_packed_tarball_is_not_overwritten() {
    let t = Tree::new();
    let first = t.pack(&[]).unwrap();
    let before = fs::read(&first.tarball).unwrap();
    let (c, msg) = code(t.pack(&[]));
    assert_eq!(c, 1);
    assert!(msg.ends_with("tehas-v9.9.9-linux-x86_64.tar.gz already exists; remove it to repack"));
    assert_eq!(fs::read(&first.tarball).unwrap(), before);
}

#[test]
fn an_out_dir_that_cannot_be_made_is_a_bad_argument() {
    let t = Tree::new();
    t.file("plain", "x");
    let out = t.path("plain/nested").display().to_string();
    let console = t.path("console").display().to_string();
    let (c, msg) = code(pack(
        &run_args(&["v9.9.9", &out, "--console", &console]),
        Some(&t.root()),
        t.dir.path(),
    ));
    assert_eq!((c, msg), (64, format!("cannot make {out}")));
}

/// The packages of the workspace that build a program and that no host is given,
/// each with why. Every other program is in each list below.
const NOT_INSTALLED: &[(&str, &str)] = &[
    ("tehas-xtask", "the development tasks of scripts/"),
    (
        "tehas-merge",
        "built and tested, but no installed command starts it yet",
    ),
];

/// The name of `[package]` in a manifest.
fn package_name(manifest: &str) -> String {
    let mut in_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if let Some(v) = line.strip_prefix("name = ").filter(|_| in_package) {
            return v.trim_matches('"').to_string();
        }
    }
    panic!("no package name in\n{manifest}");
}

/// Each word of `text` that follows `marker` up to the first of `ends`.
fn names_after(text: &str, marker: &str, ends: &[char]) -> BTreeSet<String> {
    text.split(marker)
        .skip(1)
        .map(|rest| rest.split(ends).next().unwrap().to_string())
        .collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn every_program_of_the_workspace_reaches_a_host_or_is_named_as_not_installed() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |p: &str| fs::read_to_string(repo.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let root = read("console/Cargo.toml");
    let members = root
        .lines()
        .find_map(|l| l.strip_prefix("members = ["))
        .expect("console/Cargo.toml: members");
    let mut dirs = vec![String::new()];
    dirs.extend(
        members
            .trim_end_matches(']')
            .split(',')
            .map(|m| format!("{}/", m.trim().trim_matches('"'))),
    );
    let mut programs = BTreeSet::new();
    for dir in &dirs {
        if repo.join(format!("console/{dir}src/main.rs")).is_file() {
            programs.insert(package_name(&read(&format!("console/{dir}Cargo.toml"))));
        }
    }
    for (name, why) in NOT_INSTALLED {
        assert!(
            programs.remove(*name),
            "{name} ({why}) is no program of the workspace"
        );
    }

    let install = read("install.sh");
    let compiled = install
        .lines()
        .find_map(|l| l.strip_prefix("compiled=("))
        .expect("install.sh: compiled=(");
    let compiled: BTreeSet<String> = compiled
        .trim_end_matches(')')
        .split_whitespace()
        .map(String::from)
        .collect();
    assert_eq!(compiled, programs, "install.sh compiled=(...)");

    let release = read(".github/workflows/release.yml");
    let between = |from: &str, to: &str| {
        let start = release
            .find(from)
            .unwrap_or_else(|| panic!("release.yml: {from}"));
        let rest = &release[start..];
        rest[..rest.find(to).unwrap_or_else(|| panic!("release.yml: {to}"))].to_string()
    };
    let built = names_after(&between("cargo build", "mkdir -p"), " -p ", &[' ', '\n']);
    assert_eq!(built, programs, "release.yml: cargo build -p");
    let copies = between("cp \"$target", "\"$RUNNER_TEMP/programs/\"");
    let copied = names_after(&copies, "/release/", &['"', ' ']);
    assert_eq!(copied, programs, "release.yml: cp");
    let flags: BTreeSet<String> = between("scripts/pack-release.sh \"$TAG\"", "--commit")
        .split_whitespace()
        .filter(|w| w.starts_with("--"))
        .map(String::from)
        .collect();
    let mut packed = set(&["console"]);
    let mut pack_flags = set(&["--console"]);
    for (flag, name) in PROGRAMS {
        packed.insert(name.to_string());
        pack_flags.insert(flag.to_string());
    }
    assert_eq!(packed, programs, "pack-release PROGRAMS");
    assert_eq!(flags, pack_flags, "release.yml: pack-release.sh flags");
}
