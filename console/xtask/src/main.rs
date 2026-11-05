//! xtask: the development tasks of this repository, behind the scripts in
//! scripts/ that exec it.
//!
//! Usage:
//!   xtask select-tests --root <dir> <path>...
//!
//! select-tests is scripts/select-tests.sh: src/select.rs says what it prints.
//!
//! Exit: what the task says; 64 a task or an argument it does not know.

use std::path::PathBuf;
use std::process::ExitCode;

mod select;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("select-tests") => select_tests(&args[1..]),
        _ => {
            eprintln!("xtask: usage: xtask select-tests --root <dir> <path>...");
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
