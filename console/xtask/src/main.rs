//! xtask: the development tasks of this repository, behind the scripts in
//! scripts/ that exec it.
//!
//! Usage:
//!   xtask select-tests --root <dir> <path>...
//!   xtask collect-changelog --script <path> <argument>...
//!   xtask pack-release --script <path> <argument>...
//!   xtask tag-release --script <path> <argument>...
//!   xtask polyfills --script <path> <argument>...
//!   xtask test --root <dir> [path...]
//!
//! select-tests is scripts/select-tests.sh: src/select.rs says what it prints.
//! pack-release is scripts/pack-release.sh, whose header (the --script) is its
//! usage: src/pack.rs. tag-release is scripts/tag-release.sh, the same way:
//! src/tag.rs; it works in the checkout two levels above the --script.
//! collect-changelog is scripts/collect-changelog.sh, the same way:
//! src/changelog.rs; its default changelog is the CHANGELOG.md two levels
//! above the --script. polyfills is scripts/polyfills.sh, the same way:
//! src/polyfills.rs; its default tree is the checkout two levels above the
//! --script. test
//! is scripts/test.sh: src/run.rs. Each check of its pool runs as
//! `xtask test-task`, a process of this program's own.
//!
//! Exit: what the task says; 64 a task or an argument it does not know.

use std::path::PathBuf;
use std::process::ExitCode;

mod changelog;
mod pack;
mod polyfills;
mod run;
mod select;
mod tag;

fn main() -> ExitCode {
    // A task's arguments are paths, taken as bytes.
    let os: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if os.first().is_some_and(|a| a == "test-task") {
        return ExitCode::from(run::task(&os[1..]));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("select-tests") => select_tests(&args[1..]),
        Some("pack-release") => pack_release(&args[1..]),
        Some("tag-release") => tag_release(&args[1..]),
        Some("collect-changelog") => collect_changelog(&args[1..]),
        Some("polyfills") => polyfills(&args[1..]),
        Some("test") => test(&args[1..]),
        _ => {
            eprintln!("xtask: usage: xtask select-tests --root <dir> <path>...");
            eprintln!("       xtask pack-release --script <path> <argument>...");
            eprintln!("       xtask tag-release --script <path> <argument>...");
            eprintln!("       xtask collect-changelog --script <path> <argument>...");
            eprintln!("       xtask polyfills --script <path> <argument>...");
            eprintln!("       xtask test --root <dir> [path...]");
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

fn collect_changelog(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: collect-changelog needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let a = match changelog::parse(rest) {
        Ok(changelog::Parsed::Run(a)) => a,
        Ok(changelog::Parsed::Help) => {
            let text = std::fs::read_to_string(&script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("collect-changelog: {msg}");
            return ExitCode::from(64);
        }
    };
    let default = pack::logical(&pwd(), &script.join("../../CHANGELOG.md"));
    ExitCode::from(changelog::run(&a, &default))
}

fn polyfills(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: polyfills needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let usage = || pack::help(&std::fs::read_to_string(&script).unwrap_or_default());
    let a = match polyfills::parse(rest) {
        Ok(polyfills::Parsed::Run(a)) => a,
        Ok(polyfills::Parsed::Help) => {
            tehas_core::out!("{}", usage());
            return ExitCode::SUCCESS;
        }
        Err(polyfills::Bad::Usage) => {
            eprint!("{}", usage());
            return ExitCode::from(64);
        }
        Err(polyfills::Bad::Say(msg)) => {
            eprintln!("polyfills: {msg}");
            return ExitCode::from(64);
        }
    };
    let root = match &a.root {
        Some(r) => pack::logical(&pwd(), r),
        None => pack::logical(&pwd(), &script.join("../..")),
    };
    ExitCode::from(polyfills::run(&a, &root))
}

fn test(args: &[String]) -> ExitCode {
    let (root, paths) = match args {
        [flag, root, paths @ ..] if flag == "--root" => (PathBuf::from(root), paths),
        _ => {
            eprintln!("xtask: test needs --root <dir>");
            return ExitCode::from(64);
        }
    };
    let Ok(me) = std::env::current_exe() else {
        eprintln!("test.sh: cannot name the program that runs the checks");
        return ExitCode::from(2);
    };
    ExitCode::from(run::main(&root, paths, &me))
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
