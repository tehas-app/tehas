//! pack-release: the release tarball of a tag, from a tree and the programs a
//! build made. scripts/pack-release.sh execs it; its header is the usage, the
//! list of what is packed and the exit statuses, and `--help` prints it.
//!
//! The checks run in the order the script has always made them, so a bad
//! argument and a broken tree are named as before. Nothing is written until
//! every check has passed, and the tarball and its .sha256 reach the out dir
//! last, the .sha256 first.
//!
//! A relative --console, program or out dir is read from the working
//! directory while the arguments are checked and from the tree after that, as
//! the script read it once it had changed into the tree.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// A refusal: the exit status and what is said after "pack-release: ".
#[derive(Debug, PartialEq)]
pub struct Fail {
    pub code: u8,
    pub msg: String,
}

fn fail<T>(code: u8, msg: impl Into<String>) -> Result<T, Fail> {
    Err(Fail {
        code,
        msg: msg.into(),
    })
}

/// The programs besides the console: the option, and the name in libexec/.
pub const PROGRAMS: &[(&str, &str)] = &[
    ("--analytics", "analytics"),
    ("--mcp", "tehas-mcp"),
    ("--store", "store"),
    ("--attach", "attach"),
    ("--dispatch", "tehas-dispatch"),
    ("--cycle", "tehas-cycle"),
];

#[derive(Debug, Default, PartialEq)]
pub struct Args {
    pub tag: String,
    pub out_dir: String,
    pub console: String,
    /// The path of each of PROGRAMS, empty when it was not given.
    pub programs: [String; 6],
    pub arch: String,
    pub commit: String,
    pub rukas_commit: String,
    pub rukas_digest: String,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Help,
    Run(Box<Args>),
}

/// The arguments, as the script's loop read them: the first error in their
/// order, or --help wherever it comes before one.
pub fn parse(args: &[String]) -> Result<Parsed, Fail> {
    let mut a = Args {
        arch: "x86_64".into(),
        ..Args::default()
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let arg = arg.as_str();
        let mut value = |what: &str| match it.next() {
            Some(v) => Ok(v.clone()),
            None => fail(64, format!("{arg} needs {what}")),
        };
        match arg {
            "--console" => a.console = value("a path")?,
            "--arch" => a.arch = value("x86_64 or aarch64")?,
            "--commit" => a.commit = value("a commit")?,
            "--rukas-commit" => a.rukas_commit = value("a commit")?,
            "--rukas-digest" => a.rukas_digest = value("a digest")?,
            "-h" | "--help" => return Ok(Parsed::Help),
            _ => {
                if let Some(i) = PROGRAMS.iter().position(|(o, _)| *o == arg) {
                    a.programs[i] = value("a path")?;
                } else if arg.starts_with('-') {
                    return fail(64, format!("unknown argument: {arg}"));
                } else if a.tag.is_empty() {
                    a.tag = arg.into();
                } else if a.out_dir.is_empty() {
                    a.out_dir = arg.into();
                } else {
                    return fail(64, format!("too many arguments: {arg}"));
                }
            }
        }
    }
    Ok(Parsed::Run(Box::new(a)))
}

/// The help: the script's header, as `sed -n '2,/^[^#]/s/^# \{0,1\}//p'` gave it.
pub fn help(script: &str) -> String {
    let mut out = String::new();
    for line in script.lines().skip(1) {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            out.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            out.push('\n');
        } else {
            break;
        }
    }
    out
}

/// vX.Y.Z, each part one or more digits.
pub fn is_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| is_digits(p, |c| c.is_ascii_digit()))
}

fn is_digits(s: &str, ok: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.chars().all(ok)
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && is_digits(s, |c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// The two bytes of e_machine, at offset 18 of an ELF header, little endian.
pub fn want_machine(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("3e00"),
        "aarch64" => Some("b700"),
        _ => None,
    }
}

/// The bytes at offsets 18 and 19 as od -tx1 gives them, fewer when the file
/// stops before.
pub fn machine_of(path: &Path) -> io::Result<String> {
    let mut head = Vec::with_capacity(20);
    fs::File::open(path)?.take(20).read_to_end(&mut head)?;
    Ok(head.iter().skip(18).map(|b| format!("{b:02x}")).collect())
}

/// The entries of one [section] of lib/release.list, `#` lines and blank ones
/// skipped. An entry is the whole line, as awk printed it.
pub fn release_list(text: &str, section: &str) -> Vec<String> {
    let want = format!("[{section}]");
    let mut on = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.split_whitespace().next().is_none() {
            continue;
        }
        if line.starts_with('[') {
            on = line == want;
            continue;
        }
        if on {
            out.push(line.to_string());
        }
    }
    out
}

/// versions.env with every `<key>=` line taken out and `<key>=<value>` at the
/// end. None when no line is left, which grep -v took as a failure.
pub fn pin_env(text: &str, key: &str, value: &str) -> Option<String> {
    let prefix = format!("{key}=");
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let bare = line.strip_suffix('\n').unwrap_or(line);
        if bare.starts_with(&prefix) {
            continue;
        }
        out.push_str(bare);
        out.push('\n');
    }
    if out.is_empty() {
        return None;
    }
    out.push_str(&format!("{key}={value}\n"));
    Some(out)
}

/// `[ -f ]`: a regular file, after links.
fn is_file(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.is_file())
}

/// `[ -d ]`: a directory, after links.
fn is_dir(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.is_dir())
}

/// `[ -x ]`: this process may run it.
fn is_exec(p: &Path) -> bool {
    use rustix::fs::{accessat, Access, AtFlags, CWD};
    accessat(CWD, p, Access::EXEC_OK, AtFlags::EACCESS).is_ok()
}

/// `[ a -ef b ]`: the same file, after links.
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

/// `cmp -s`: the same bytes.
fn same_bytes(a: &Path, b: &Path) -> bool {
    match (fs::read(a), fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The path as `cd` leaves it in $PWD: absolute, `.` and `..` taken lexically.
pub fn logical(base: &Path, p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in base.join(p).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// The symbolic links under the paths, the paths themselves included, named
/// as `find <path>... -type l` names them.
fn links(root: &Path, paths: &[String]) -> Vec<String> {
    fn walk(root: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(m) = fs::symlink_metadata(root.join(rel)) else {
            return;
        };
        if m.file_type().is_symlink() {
            out.push(rel.to_string());
        } else if m.is_dir() {
            let Ok(entries) = fs::read_dir(root.join(rel)) else {
                return;
            };
            for e in entries.flatten() {
                let name = e.file_name();
                let child = format!("{}/{}", rel.trim_end_matches('/'), name.to_string_lossy());
                walk(root, &child, out);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        walk(root, p, &mut out);
    }
    out
}

/// A directory under $TMPDIR (or /tmp), removed when the value drops.
struct Stage(PathBuf);

impl Stage {
    fn new() -> io::Result<Stage> {
        let tmp = std::env::var_os("TMPDIR")
            .filter(|t| !t.is_empty())
            .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let mut last = io::Error::other("no name left");
        for n in 0..100u32 {
            let p = tmp.join(format!(
                "pack-release.{}{:06x}",
                std::process::id(),
                (seed ^ n.wrapping_mul(2_654_435_761)) & 0xff_ffff
            ));
            match fs::create_dir(&p) {
                Ok(()) => return Ok(Stage(p)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Run a copy as cp makes it, in the tree: cp keeps the modes install.sh
/// compares and keeps a link a link (-P).
fn cp(root: &Path, args: &[&str]) -> bool {
    Command::new("cp")
        .args(args)
        .current_dir(root)
        .status()
        .is_ok_and(|s| s.success())
}

fn chmod_755(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(0o755)).is_ok()
}

/// mv: a rename, or a copy and a removal across file systems.
fn mv(from: &Path, to: &Path) -> io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

/// What a pack wrote: the tarball's path and the line of its .sha256.
#[derive(Debug, PartialEq)]
pub struct Packed {
    pub tarball: String,
    pub sha_line: String,
}

/// Pack a release. `pack_root` is TEHAS_PACK_ROOT and `pwd` the working
/// directory, as the shell knew it.
pub fn pack(a: &Args, pack_root: Option<&str>, pwd: &Path) -> Result<Packed, Fail> {
    if a.tag.is_empty() {
        return fail(64, "name the tag");
    }
    if !is_tag(&a.tag) {
        return fail(64, format!("the tag must be vX.Y.Z, got: {}", a.tag));
    }
    if a.out_dir.is_empty() {
        return fail(64, "name the output directory");
    }
    let Some(want) = want_machine(&a.arch) else {
        return fail(
            64,
            format!("--arch must be x86_64 or aarch64, got: {}", a.arch),
        );
    };
    if a.console.is_empty() {
        return fail(64, "--console names the console to pack");
    }
    let here = |p: &str| pwd.join(p);
    if !is_file(&here(&a.console)) {
        return fail(64, format!("no console at: {}", a.console));
    }
    if !is_exec(&here(&a.console)) {
        return fail(64, format!("the console is not executable: {}", a.console));
    }
    for p in a.programs.iter().filter(|p| !p.is_empty()) {
        if !is_file(&here(p)) || !is_exec(&here(p)) {
            return fail(64, format!("not a program the pack can run: {p}"));
        }
    }
    // Git and a registry answer in either case; versions.env holds one shape.
    let commit = a.commit.to_lowercase();
    let rukas_commit = a.rukas_commit.to_lowercase();
    let rukas_digest = a.rukas_digest.to_lowercase();
    if !commit.is_empty() && !is_hex(&commit, 40) {
        return fail(1, format!("--commit must be 40 hex digits: {commit}"));
    }
    if !rukas_commit.is_empty() && !is_hex(&rukas_commit, 40) {
        return fail(
            1,
            format!("--rukas-commit must be 40 hex digits: {rukas_commit}"),
        );
    }
    if !rukas_digest.is_empty()
        && !rukas_digest
            .strip_prefix("sha256:")
            .is_some_and(|h| is_hex(h, 64))
    {
        return fail(
            1,
            format!("--rukas-digest must be sha256: and 64 hex digits: {rukas_digest}"),
        );
    }
    if !rukas_commit.is_empty() && rukas_digest.is_empty() {
        return fail(
            64,
            "--rukas-commit needs --rukas-digest: name both pins or neither",
        );
    }
    if rukas_commit.is_empty() && !rukas_digest.is_empty() {
        return fail(
            64,
            "--rukas-digest needs --rukas-commit: name both pins or neither",
        );
    }

    // The tree has to answer to tehas, not only hold an install.sh.
    let given = match pack_root {
        Some(r) if !r.is_empty() => r.to_string(),
        _ => pwd.display().to_string(),
    };
    let raw = here(&given);
    if ["install.sh", "bin/tehas", "lib/config.sh", "libexec/setup"]
        .iter()
        .any(|f| !is_file(&raw.join(f)))
    {
        return fail(64, format!("not a tehas tree to pack: {given}"));
    }
    let root = logical(pwd, Path::new(&given));
    if fs::read_dir(&root).is_err() {
        return fail(64, format!("cannot read the tree: {given}"));
    }
    let rd = root.display();
    let at = |p: &str| root.join(p);

    let Ok(list) = fs::read_to_string(at("lib/release.list")) else {
        return fail(1, format!("lib/release.list is missing from {rd}"));
    };
    let dirs = release_list(&list, "dirs");
    let plain = release_list(&list, "plain");
    let extra = release_list(&list, "extra");
    // Two empty lists would pass every check below without checking anything.
    if dirs.is_empty() || plain.is_empty() {
        return fail(1, format!("{rd}/lib/release.list names nothing to pack"));
    }
    for (list, ok) in [
        (&dirs, is_dir as fn(&Path) -> bool),
        (&plain, is_file),
        (&extra, is_dir),
    ] {
        if let Some(missing) = list.iter().find(|e| !ok(&at(e))) {
            return fail(1, format!("{missing} is missing from {rd}"));
        }
    }
    // What is packed is the --console build, never one a run of install.sh or
    // the gate left in the tree.
    let cached = at("libexec/console");
    if cached.exists()
        && (same_file(&at(&a.console), &cached) || same_bytes(&at(&a.console), &cached))
    {
        return fail(
            64,
            format!(
                "--console is the tree's own build of it: {}",
                cached.display()
            ),
        );
    }

    let name = format!("tehas-{}", a.tag);
    let tarball = format!("{name}-linux-{}.tar.gz", a.arch);
    // The architecture the tarball's name promises is read from the programs.
    let given_programs: Vec<&String> = std::iter::once(&a.console)
        .chain(a.programs.iter())
        .filter(|p| !p.is_empty())
        .collect();
    for p in &given_programs {
        let machine = machine_of(&at(p)).map_err(|e| Fail {
            code: 1,
            msg: format!("cannot read {p}: {e}"),
        })?;
        if machine != want {
            return fail(
                1,
                format!(
                    "not a {} program: {p} (e_machine {machine}, want {want})",
                    a.arch
                ),
            );
        }
    }
    // cp keeps a link and tar writes what it points at, so a link out of the
    // tree would put a file that is not in the tree inside a release.
    let found: Vec<String> = links(&root, &[dirs.clone(), extra.clone()].concat());
    if !found.is_empty() {
        let names: Vec<&str> = found.iter().take(10).map(String::as_str).collect();
        return fail(
            1,
            format!(
                "{rd} holds a symbolic link, which would be packed as the file it points at:\n{}",
                names.join("\n")
            ),
        );
    }
    let out_dir = at(&a.out_dir);
    let out_tarball = out_dir.join(&tarball);
    let out_sha = out_dir.join(format!("{tarball}.sha256"));
    if out_tarball.symlink_metadata().is_ok() || out_sha.symlink_metadata().is_ok() {
        return fail(
            1,
            format!(
                "{}/{tarball} already exists; remove it to repack",
                a.out_dir
            ),
        );
    }

    let stage = Stage::new().map_err(|_| Fail {
        code: 1,
        msg: "cannot make a staging directory".into(),
    })?;
    let stage_dir = stage.0.join(&name);
    if fs::create_dir_all(&stage_dir).is_err() || fs::create_dir_all(&out_dir).is_err() {
        return fail(64, format!("cannot make {}", a.out_dir));
    }
    let sd = stage_dir.display().to_string();
    let sd_slash = format!("{sd}/");
    let mut args: Vec<&str> = vec!["-R", "-P"];
    args.extend(dirs.iter().map(String::as_str));
    args.push(&sd_slash);
    if !cp(&root, &args) {
        return fail(1, "cannot copy the directories");
    }
    let mut args: Vec<&str> = plain.iter().map(String::as_str).collect();
    args.push(&sd_slash);
    if !cp(&root, &args) {
        return fail(1, "cannot copy the files");
    }
    if !extra.is_empty() {
        let mut args: Vec<&str> = vec!["-R", "-P", "--parents"];
        args.extend(extra.iter().map(String::as_str));
        args.push(&sd_slash);
        if !cp(&root, &args) {
            return fail(1, format!("cannot copy {}", extra.join(" ")));
        }
    }
    let libexec = stage_dir.join("libexec");
    if fs::copy(at(&a.console), libexec.join("console")).is_err() {
        return fail(1, "cannot copy the console");
    }
    if !chmod_755(&libexec.join("console")) {
        return fail(1, "cannot make the console executable");
    }
    for (p, (_, dest)) in a.programs.iter().zip(PROGRAMS) {
        if p.is_empty() {
            continue;
        }
        let to = libexec.join(dest);
        if fs::copy(at(p), &to).is_err() || !chmod_755(&to) {
            return fail(1, format!("cannot copy {p}"));
        }
    }

    // The checkout's versions.env stays the record of every other version;
    // the pins are the last lines, and the only ones of their keys.
    let env = stage_dir.join("host/versions.env");
    let mut pins: Vec<(&str, String)> = Vec::new();
    if !rukas_commit.is_empty() {
        let source = fs::read_to_string(at("host/versions.env")).unwrap_or_default();
        if !source.lines().any(|l| l.starts_with("PI_RUKAS_FOLLOW=")) {
            return fail(1, "host/versions.env has no PI_RUKAS_FOLLOW line to pin");
        }
        pins.push(("PI_RUKAS_FOLLOW", format!("commit:{rukas_commit}")));
    }
    if !rukas_digest.is_empty() {
        pins.push(("PI_RUKAS_IMAGE_DIGEST", rukas_digest.clone()));
    }
    for (key, value) in pins {
        let next = fs::read_to_string(&env)
            .ok()
            .and_then(|t| pin_env(&t, key, &value));
        let Some(next) = next else {
            return fail(1, "cannot rewrite host/versions.env");
        };
        if fs::write(&env, next).is_err() {
            return fail(1, "cannot rewrite host/versions.env");
        }
    }
    // install.sh reads these only from a tree that has no git of its own.
    if fs::write(stage_dir.join("VERSION"), format!("{}\n", a.tag)).is_err() {
        return fail(1, "cannot write VERSION");
    }
    if !commit.is_empty() && fs::write(stage_dir.join("COMMIT"), format!("{commit}\n")).is_err() {
        return fail(1, "cannot write COMMIT");
    }

    let partial = stage.0.join(format!("{tarball}.partial"));
    let tarred = Command::new("tar")
        .arg("-czf")
        .arg(&partial)
        .arg("-C")
        .arg(&stage.0)
        .arg(&name)
        .status()
        .is_ok_and(|s| s.success());
    if !tarred {
        return fail(1, "tar failed");
    }
    let Ok(bytes) = fs::read(&partial) else {
        return fail(1, "cannot read the tarball to hash it");
    };
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    // sha256sum -c reads it from the out dir: the hash, two spaces, the name.
    let sha_line = format!("{hash}  {tarball}");
    let staged_sha = stage.0.join(format!("{tarball}.sha256"));
    if fs::write(&staged_sha, format!("{sha_line}\n")).is_err() {
        return fail(1, "cannot write the hash line");
    }
    if mv(&staged_sha, &out_sha).is_err() {
        return fail(1, format!("cannot put the .sha256 in {}", a.out_dir));
    }
    if mv(&partial, &out_tarball).is_err() {
        let _ = fs::remove_file(&out_sha);
        return fail(1, format!("cannot put the tarball in {}", a.out_dir));
    }
    Ok(Packed {
        tarball: format!("{}/{tarball}", a.out_dir),
        sha_line,
    })
}

#[cfg(test)]
mod tests;
