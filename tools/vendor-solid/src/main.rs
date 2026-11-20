//! Bundles the pinned Solid 2 packages into the kit's one library file.
//!
//!   cargo run --release --locked --manifest-path tools/vendor-solid/Cargo.toml [-- --check]
//!
//! Downloads each pinned tarball from the npm registry, checks it against its
//! pinned sha512 integrity, unpacks it into a scratch `node_modules` and
//! bundles one entry that re-exports `EXPORTS` with rolldown: one minified ES
//! module from the browser production entries. No package code runs. Writes
//! `host/fun/kit/vendor/solid.js`, the licences beside it in
//! `host/fun/kit/vendor/LICENSES` and the kit's `MANIFEST`.
//!
//! With `--check` it writes nothing and fails when the committed files differ
//! from what it would write.
//!
//! Exit: 0 written (or the same), 1 a download, check, bundle or write failed
//! or the files differ, 64 usage.

use base64::Engine;
use sha2::{Digest, Sha256, Sha512};
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

struct Pin {
    name: &'static str,
    version: &'static str,
    tarball: &'static str,
    integrity: &'static str,
}

const PINS: [Pin; 4] = [
    Pin {
        name: "solid-js",
        version: "2.0.3",
        tarball: "https://registry.npmjs.org/solid-js/-/solid-js-2.0.3.tgz",
        integrity: "sha512-XOHstv+GAJjfcSGZU9xfX5q6Mbqmv72IcdWh2JuMx3KvBbfpQPdy/Yl00A1ouk7A0etjqhNe799CPdr6i7koWg==",
    },
    Pin {
        name: "@solidjs/signals",
        version: "2.0.3",
        tarball: "https://registry.npmjs.org/@solidjs/signals/-/signals-2.0.3.tgz",
        integrity: "sha512-M6+kiJflqSvJRHRcmdsdIXpPLPkQrY817zWKKfxZ3z3ZRE5eEtddtWPrDXY4IM1Wz3zAqG+3Fx/HQIkAtpRwnA==",
    },
    Pin {
        name: "@solidjs/web",
        version: "2.0.3",
        tarball: "https://registry.npmjs.org/@solidjs/web/-/web-2.0.3.tgz",
        integrity: "sha512-XYl9JGOtQ8yJSZRnd6uC+5gYXsYvnIRNxhn89m+F1v6D4d4ZFdTJY2Lq/8XJ3YcjckkCrK/mNfd3kIiqqgnG7w==",
    },
    Pin {
        name: "@solidjs/html",
        version: "2.0.1",
        tarball: "https://registry.npmjs.org/@solidjs/html/-/html-2.0.1.tgz",
        integrity: "sha512-9tCCCUBkqyUonWX9rJW/hJwH2HJjI5I1X9MEhM4epFSNcTaCb8Iw2UCQi2Y204+8B9mQumI6WWa8Idik3V9fug==",
    },
];

/// The kit's Solid API: the names a page may import from `solid.js`
/// (host/docs/frontend.md). The bundle must export exactly these.
const EXPORTS: [(&str, &[&str]); 3] = [
    (
        "solid-js",
        &[
            "Errored",
            "For",
            "Loading",
            "Match",
            "Show",
            "Switch",
            "action",
            "createEffect",
            "createMemo",
            "createOptimistic",
            "createOptimisticStore",
            "createRenderEffect",
            "createRoot",
            "createSignal",
            "createStore",
            "flush",
            "getOwner",
            "isPending",
            "latest",
            "onCleanup",
            "onSettled",
            "refresh",
            "runWithOwner",
            "snapshot",
            "untrack",
        ],
    ),
    ("@solidjs/web", &["render"]),
    ("@solidjs/html", &["default as html"]),
];

/// The browser baseline of host/docs/frontend.md: the older of the last two
/// majors of each engine when this pin was made.
const TARGET: [&str; 4] = ["chrome142", "edge142", "firefox145", "safari26"];

/// Text the output may not hold. The kit uses no eval, so it works under a
/// CSP without 'unsafe-eval', and a bundle that reaches for node or for
/// another module did not bundle what it should.
const REFUSED: [&str; 5] = [
    "eval(",
    "new Function",
    "process.env",
    "require(",
    "import(",
];

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("vendor-solid: {message}");
    std::process::exit(1);
}

fn download(pin: &Pin) -> Vec<u8> {
    let mut response = ureq::get(pin.tarball)
        .call()
        .unwrap_or_else(|e| fail(format!("{}: {e}", pin.tarball)));
    let bytes = response
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_vec()
        .unwrap_or_else(|e| fail(format!("{}: {e}", pin.tarball)));
    let got = format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&bytes))
    );
    if got != pin.integrity {
        fail(format!(
            "{} {}: integrity {got}, pinned {}",
            pin.name, pin.version, pin.integrity
        ));
    }
    bytes
}

/// The tarball's `package/` folder into `to`: plain files and folders only,
/// and no path that climbs out.
fn unpack(pin: &Pin, tgz: &[u8], to: &Path) {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tgz));
    let entries = archive
        .entries()
        .unwrap_or_else(|e| fail(format!("{}: {e}", pin.name)));
    for entry in entries {
        let mut entry = entry.unwrap_or_else(|e| fail(format!("{}: {e}", pin.name)));
        let path = entry
            .path()
            .unwrap_or_else(|e| fail(format!("{}: {e}", pin.name)))
            .into_owned();
        let mut parts = path.components();
        if parts.next() != Some(Component::Normal("package".as_ref())) {
            fail(format!(
                "{}: {} is outside package/",
                pin.name,
                path.display()
            ));
        }
        let rest: PathBuf = parts.collect();
        if rest
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            fail(format!("{}: refused path {}", pin.name, path.display()));
        }
        let kind = entry.header().entry_type();
        let target = to.join(&rest);
        if kind.is_dir() {
            fs::create_dir_all(&target).unwrap_or_else(|e| fail(e));
        } else if kind.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|e| fail(e));
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .unwrap_or_else(|e| fail(format!("{}: {e}", pin.name)));
            fs::write(&target, bytes).unwrap_or_else(|e| fail(e));
        } else {
            fail(format!("{}: refused entry {}", pin.name, path.display()));
        }
    }
}

fn entry_source() -> String {
    let mut out = String::new();
    for (module, names) in EXPORTS {
        let _ = writeln!(out, "export {{ {} }} from {module:?};", names.join(", "));
    }
    out
}

fn exported_names() -> Vec<String> {
    let mut names: Vec<String> = EXPORTS
        .iter()
        .flat_map(|(_, n)| n.iter())
        .map(|n| n.rsplit(' ').next().unwrap_or(n).to_string())
        .collect();
    names.sort();
    names
}

async fn bundle(scratch: &Path) -> String {
    use rolldown::{
        Bundler, BundlerOptions, BundlerTransformOptions, Either, InputItem, LegalComments,
        OutputFormat, Platform, RawMinifyOptions, ResolveOptions,
    };
    let options = BundlerOptions {
        input: Some(vec![InputItem {
            name: Some("solid".to_string()),
            import: "./entry.js".to_string(),
        }]),
        cwd: Some(scratch.to_path_buf()),
        platform: Some(Platform::Browser),
        format: Some(OutputFormat::Esm),
        minify: Some(RawMinifyOptions::Bool(true)),
        legal_comments: Some(LegalComments::None),
        resolve: Some(ResolveOptions {
            condition_names: Some(
                ["browser", "production", "import", "default"]
                    .map(String::from)
                    .to_vec(),
            ),
            ..Default::default()
        }),
        transform: Some(BundlerTransformOptions {
            target: Some(Either::Right(TARGET.map(String::from).to_vec())),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut bundler = Bundler::new(options).unwrap_or_else(|e| fail(format!("{e:?}")));
    let output = bundler
        .generate()
        .await
        .unwrap_or_else(|e| fail(format!("bundle: {e:?}")));
    for warning in &output.warnings {
        eprintln!("vendor-solid: warning: {warning:?}");
    }
    let chunks: Vec<_> = output
        .assets
        .iter()
        .filter_map(|o| match o {
            rolldown_common::Output::Chunk(c) => Some(c.clone()),
            rolldown_common::Output::Asset(_) => None,
        })
        .collect();
    let [chunk] = chunks.as_slice() else {
        fail(format!("{} chunks, want one", chunks.len()));
    };
    if !chunk.imports.is_empty() || !chunk.dynamic_imports.is_empty() {
        fail(format!("the bundle imports {:?}", chunk.imports));
    }
    let mut got: Vec<String> = chunk.exports.iter().map(|e| e.to_string()).collect();
    got.sort();
    if got != exported_names() {
        fail(format!(
            "the bundle exports {got:?}, want {:?}",
            exported_names()
        ));
    }
    chunk.code.clone()
}

fn header() -> String {
    let mut out = String::from(
        "/*! The kit's Solid 2, made by tools/vendor-solid. Do not edit: run the tool.\n",
    );
    for pin in &PINS {
        let _ = writeln!(out, " * {} {} {}", pin.name, pin.version, pin.tarball);
    }
    out.push_str(" * MIT licence. The texts are in LICENSES beside this file. */\n");
    out
}

fn licences(scratch: &Path) -> String {
    let mut out =
        String::from("The licences of the packages bundled into solid.js by tools/vendor-solid.\n");
    for pin in &PINS {
        let dir = scratch.join("node_modules").join(pin.name);
        let manifest = fs::read_to_string(dir.join("package.json")).unwrap_or_else(|e| fail(e));
        let licence = manifest
            .split("\"license\"")
            .nth(1)
            .and_then(|r| r.split('"').nth(1))
            .unwrap_or("none stated")
            .to_string();
        let _ = write!(
            out,
            "\n== {} {} (package.json license: {licence})\n\n",
            pin.name, pin.version
        );
        match fs::read_to_string(dir.join("LICENSE")) {
            Ok(text) => out.push_str(text.trim_end()),
            Err(_) => out.push_str(
                "The package ships no licence file. It declares MIT and comes from the same\n\
                 project as @solidjs/signals, whose MIT licence text is in this file.",
            ),
        }
        out.push('\n');
    }
    out
}

/// The first line of the kit's manifest as it stands: `kit <n>`, the contract
/// number a person sets. A kit with no manifest yet starts at `kit 1`. Bundling
/// Solid is no reason to change the number, so a line of another shape stops
/// the tool rather than being replaced.
fn kit_line(kit: &Path) -> Result<String, String> {
    let Ok(text) = fs::read_to_string(kit.join("MANIFEST")) else {
        return Ok("kit 1".to_string());
    };
    let first = text.lines().next().unwrap_or("");
    match first.strip_prefix("kit ") {
        Some(n) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => Ok(first.to_string()),
        _ => Err(format!(
            "host/fun/kit/MANIFEST begins {first:?}, not kit <number>"
        )),
    }
}

/// The kit's manifest: its `kit <n>` line, kept, then
/// `file <sha256> <bytes> <name>` for each file under `kit` but the manifest,
/// by name, with `changed` standing in for the files on disk at those names.
fn manifest(kit: &Path, changed: &[(&str, &[u8])]) -> String {
    let mut files = Vec::new();
    walk(kit, Path::new(""), &mut files);
    for (name, _) in changed {
        files.push(PathBuf::from(name));
    }
    files.sort();
    files.dedup();
    let mut out = kit_line(kit).unwrap_or_else(|e| fail(e));
    out.push('\n');
    for f in files {
        let name = f.to_string_lossy().into_owned();
        if name == "MANIFEST" {
            continue;
        }
        let bytes = match changed.iter().find(|(n, _)| *n == name) {
            Some((_, b)) => b.to_vec(),
            None => fs::read(kit.join(&f)).unwrap_or_else(|e| fail(e)),
        };
        let hex: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let _ = writeln!(out, "file {hex} {} {name}", bytes.len());
    }
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root.join(dir)) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = dir.join(e.file_name());
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(root, &path, out);
        } else {
            out.push(path);
        }
    }
}

/// A new directory under `parent` that only this user can open, with a name
/// another process cannot guess ahead. It is never one that existed.
fn private_dir(parent: &Path) -> std::io::Result<PathBuf> {
    use std::hash::BuildHasher;
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    let mut tries = 0;
    loop {
        // RandomState is seeded from the OS's random source.
        let n = std::hash::RandomState::new().hash_one(std::process::id());
        let dir = parent.join(format!("vendor-solid-{n:016x}"));
        match builder.create(&dir) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && tries < 8 => tries += 1,
            r => return r.map(|()| dir),
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = match args.as_slice() {
        [] => false,
        [a] if a == "--check" => true,
        _ => {
            eprintln!("usage: vendor-solid [--check]");
            std::process::exit(64);
        }
    };
    let kit = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../host/fun/kit");

    let scratch = private_dir(&std::env::temp_dir()).unwrap_or_else(|e| fail(e));
    for pin in &PINS {
        let tgz = download(pin);
        unpack(pin, &tgz, &scratch.join("node_modules").join(pin.name));
        eprintln!("vendor-solid: {} {} checked", pin.name, pin.version);
    }
    fs::write(scratch.join("entry.js"), entry_source()).unwrap_or_else(|e| fail(e));

    let code = bundle(&scratch).await;
    for word in REFUSED {
        if code.contains(word) {
            fail(format!("the bundle holds {word:?}"));
        }
    }
    let solid = format!("{}{}\n", header(), code.trim_end());
    let licences = licences(&scratch);
    let _ = fs::remove_dir_all(&scratch);

    let written: [(&str, &[u8]); 2] = [
        ("vendor/solid.js", solid.as_bytes()),
        ("vendor/LICENSES", licences.as_bytes()),
    ];
    let manifest = manifest(&kit, &written);
    let all = [written[0], written[1], ("MANIFEST", manifest.as_bytes())];

    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    std::io::Write::write_all(&mut gz, solid.as_bytes()).unwrap_or_else(|e| fail(e));
    let gzipped = gz.finish().unwrap_or_else(|e| fail(e)).len();
    eprintln!(
        "vendor-solid: solid.js {} bytes, {gzipped} gzipped",
        solid.len()
    );

    if check {
        let mut differ = false;
        for (name, bytes) in all {
            if fs::read(kit.join(name)).ok().as_deref() != Some(bytes) {
                eprintln!("vendor-solid: host/fun/kit/{name} differs");
                differ = true;
            }
        }
        std::process::exit(i32::from(differ));
    }
    fs::create_dir_all(kit.join("vendor")).unwrap_or_else(|e| fail(e));
    for (name, bytes) in all {
        fs::write(kit.join(name), bytes).unwrap_or_else(|e| fail(e));
    }
    eprintln!("vendor-solid: wrote host/fun/kit/vendor and MANIFEST");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kit directory of a test's own, removed when it ends.
    struct Kit(PathBuf);

    impl Kit {
        fn new(name: &str) -> Kit {
            let dir = std::env::temp_dir()
                .join(format!("vendor-solid-test-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("island.js"), "export const island = 1;\n").unwrap();
            Kit(dir)
        }
    }

    impl Drop for Kit {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_scratch_is_a_new_private_directory() {
        use std::os::unix::fs::PermissionsExt;
        let parent = Kit::new("scratch");
        let a = private_dir(&parent.0).unwrap();
        let b = private_dir(&parent.0).unwrap();
        assert_ne!(a, b);
        for dir in [&a, &b] {
            let mode = fs::metadata(dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{}", dir.display());
            assert!(fs::read_dir(dir).unwrap().next().is_none());
        }
    }

    #[test]
    fn the_manifest_keeps_the_kit_number_it_has() {
        let kit = Kit::new("keep");
        fs::write(kit.0.join("MANIFEST"), "kit 3\nfile 00 1 old.js\n").unwrap();
        let out = manifest(&kit.0, &[("vendor/solid.js", b"x")]);
        assert!(out.starts_with("kit 3\nfile "), "{out}");
        assert!(out.contains(" island.js\n"), "{out}");
        assert!(out.contains(" vendor/solid.js\n"), "{out}");
        assert!(!out.contains("old.js"), "{out}");
    }

    #[test]
    fn a_kit_with_no_manifest_starts_at_1() {
        let kit = Kit::new("new");
        assert_eq!(kit_line(&kit.0).unwrap(), "kit 1");
    }

    #[test]
    fn a_first_line_of_another_shape_is_refused() {
        let kit = Kit::new("bad");
        for first in ["kit", "kit x", "kit 2 extra", "file 00 1 a.js", ""] {
            fs::write(kit.0.join("MANIFEST"), format!("{first}\n")).unwrap();
            assert!(kit_line(&kit.0).is_err(), "{first:?}");
        }
    }
}
