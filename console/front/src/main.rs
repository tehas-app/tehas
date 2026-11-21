//! `tehas` on a machine with no factory: the usage, and the commands of the
//! parts of the tehas family. `tehas <part> [words]` runs `tehas-<part>`
//! from PATH by exec with the words, by the rule of tehas_core::parts, and
//! `tehas help <part>` runs it with `--help`.
//!
//! Exit: a part's own status; 0 the usage; 64 a word no part has; 1 a part
//! that could not be run; 141 standard output closed.

use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};
use tehas_core::parts;

const USAGE: &str = include_str!("usage.txt");

fn usage() -> String {
    let found = parts::list(std::env::var_os("PATH").as_deref(), &[]);
    if found.is_empty() {
        return format!("{USAGE}\nNo part is installed on this machine.\n");
    }
    format!("{USAGE}\nParts on this machine: {}\n", found.join(", "))
}

fn run<S: AsRef<OsStr>>(word: &str, args: &[S]) -> u8 {
    let Some(program) = parts::find(word, std::env::var_os("PATH").as_deref(), &[]) else {
        eprint!("tehas: no part named {word} on this machine\n{USAGE}");
        return 64;
    };
    let failed = Command::new(&program).args(args).exec();
    eprintln!("tehas {word}: {}: {failed}", program.display());
    1
}

fn main() -> ExitCode {
    let given: Vec<OsString> = std::env::args_os().skip(1).collect();
    let word = |i: usize| given.get(i).map(|w| w.to_string_lossy());
    let code = match (word(0).as_deref(), word(1).as_deref()) {
        (Some("help" | "-h" | "--help"), Some(part)) if part != "help" => run(part, &["--help"]),
        (Some(part), _) if !matches!(part, "help" | "-h" | "--help") => run(part, &given[1..]),
        _ => {
            tehas_core::out!("{}", usage());
            0
        }
    };
    ExitCode::from(code)
}
