//! xtask: the development tasks of this repository, behind the scripts in
//! scripts/ that exec it.
//!
//! Usage:
//!   xtask select-tests --root <dir> <path>...
//!   xtask pack-release --script <path> <argument>...
//!   xtask tag-release --script <path> <argument>...
//!
//! select-tests is scripts/select-tests.sh: src/select.rs says what it prints.
//! pack-release is scripts/pack-release.sh, whose header (the --script) is its
//! usage: src/pack.rs. tag-release is scripts/tag-release.sh, the same way:
//! src/tag.rs; it works in the checkout two levels above the --script.
//!
//! Exit: what the task says; 64 a task or an argument it does not know.

use std::path::PathBuf;
use std::process::ExitCode;

mod pack;
mod select;
mod tag;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("select-tests") => select_tests(&args[1..]),
        Some("pack-release") => pack_release(&args[1..]),
        Some("tag-release") => tag_release(&args[1..]),
        _ => {
            eprintln!("xtask: usage: xtask select-tests --root <dir> <path>...");
            eprintln!("       xtask pack-release --script <path> <argument>...");
            eprintln!("       xtask tag-release --script <path> <argument>...");
            ExitCode::from(64)
        }
    }
}

fn select_tests(args: &[String]) -> ExitCode {
    let (root, paths) = match args {
        [flag, root, paths @ ..] if flag == "--root" => (PathBuf::from(root), paths),
        _ => {
            eprintln!("xtask: select-tests needs --root <dir>");
            return ExitCode::from(64);
        }
    };
    if paths.is_empty() {
        eprintln!("usage: scripts/select-tests.sh <path>...");
        return ExitCode::from(64);
    }
    let metadata = || select::cargo_metadata(&root);
    for line in select::select(&root, paths, metadata) {
        tehas_core::outln!("{line}");
    }
    ExitCode::SUCCESS
}

fn pack_release(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (script, rest),
        _ => {
            eprintln!("xtask: pack-release needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let parsed = match pack::parse(rest) {
        Ok(p) => p,
        Err(f) => return refuse(f),
    };
    let a = match parsed {
        pack::Parsed::Help => {
            let text = std::fs::read_to_string(script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        pack::Parsed::Run(a) => a,
    };
    let root = std::env::var("TEHAS_PACK_ROOT").ok();
    match pack::pack(&a, root.as_deref(), &pwd()) {
        Ok(p) => {
            tehas_core::outln!("{}", p.tarball);
            tehas_core::outln!("{}", p.sha_line);
            ExitCode::SUCCESS
        }
        Err(f) => refuse(f),
    }
}

fn tag_release(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: tag-release needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let a = match tag::parse(rest, &pwd()) {
        Ok(tag::Parsed::Run(a)) => a,
        Ok(tag::Parsed::Help) => {
            let text = std::fs::read_to_string(&script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("tag-release: {msg}");
            return ExitCode::from(64);
        }
    };
    let root = pack::logical(&pwd(), &script.join("../.."));
    ExitCode::from(tag::run(&a, &root))
}

fn refuse(f: pack::Fail) -> ExitCode {
    eprintln!("pack-release: {}", f.msg);
    ExitCode::from(f.code)
}

/// The working directory as the shell names it: $PWD when that is this
/// directory, as `cd` left it, else the one the kernel gives.
fn pwd() -> PathBuf {
    let real = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match std::env::var_os("PWD").map(PathBuf::from) {
        Some(p) if p.is_absolute() && same_dir(&p, &real) => p,
        _ => real,
    }
}

fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}
