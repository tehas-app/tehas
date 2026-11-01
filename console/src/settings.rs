//! The settings of the instance, for lib/config.sh to take over.
//!
//!   console settings load <euid> [<bash>]
//!
//! What tehas_load does, as lines for a shell to eval: standard output holds
//! the exports, whether the load was refused or not. Exit 78 with one line on
//! standard error when it was refused.
//!
//! instance.env is shell, so a shell of its own runs it, on the environment
//! this load leaves after its sweep. The shell's own output goes to standard
//! error: standard output is what the caller evals. `<euid>` is the shell's
//! EUID, the default image's name; `<bash>` is the shell's BASH, so a load
//! needs nothing on PATH.
//!
//!   console settings valid <name> <value>
//!
//! Exit 1 with the reason on standard output when `value` is no value of
//! `name`. With no name and value, standard input holds pairs of NUL-ended
//! words, and each pair is answered with its status and reason, NUL-ended.

use std::env;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::process::{Command, Stdio};
use tehas_core::settings::load::{self, Host, Sourced, Vars};
use tehas_core::settings::rules::{self, Trees};

const REFUSED: u8 = 78;

/// fd 3 keeps the pipe from everything the file starts. `set -a` is off for the
/// list, so its own two names stay out of it. The list is made with builtins:
/// the file's PATH may hold nothing.
const SOURCE: &str = r#"exec 3>&1 1>&2
set -a
if . "$0" 3>&-; then set +a; __tehas_rc=0; else set +a; __tehas_rc=1; fi
compgen -e | while IFS= read -r __tehas_n; do printf '%s=%s\0' "$__tehas_n" "${!__tehas_n}" >&3; done
exit "$__tehas_rc"
"#;

struct Real {
    bash: String,
}

impl Host for Real {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut file = File::open(path).ok()?;
        let mut bytes = Vec::new();
        Some(file.read_to_end(&mut bytes).map_or(Vec::new(), |_| bytes))
    }

    fn source(&mut self, file: &str, vars: &Vars) -> Sourced {
        let env = vars
            .iter()
            .filter(|(name, _)| *name != b"BASH_ENV")
            .map(|(k, v)| (OsStr::from_bytes(k), OsStr::from_bytes(v)));
        let done = Command::new(&self.bash)
            .args(["-c", SOURCE, file])
            .env_clear()
            .envs(env)
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .output();
        match done {
            Ok(done) => Sourced {
                vars: pairs(&done.stdout),
                ok: done.status.success() && !done.stdout.is_empty(),
            },
            Err(_) => Sourced {
                vars: Vec::new(),
                ok: false,
            },
        }
    }

    fn cpus(&mut self) -> i64 {
        let said = Command::new("nproc")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        match said {
            Ok(said) if said.status.success() => String::from_utf8_lossy(&said.stdout)
                .trim()
                .parse()
                .unwrap_or(1),
            _ => 1,
        }
    }
}

/// NUL-ended NAME=value entries.
fn pairs(list: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    list.split(|b| *b == 0)
        .filter_map(|entry| {
            let at = entry.iter().position(|b| *b == b'=')?;
            Some((entry[..at].to_vec(), entry[at + 1..].to_vec()))
        })
        .collect()
}

fn run_load(uid: &str, bash: &str) -> u8 {
    let start = Vars::new(env::vars_os().map(|(k, v)| (k.into_vec(), v.into_vec())));
    let mut vars = start.clone();
    let mut host = Real {
        bash: bash.to_string(),
    };
    let got = load::load(&mut vars, &mut host, uid);
    if io::stdout()
        .write_all(&load::script(&start, &vars))
        .is_err()
    {
        return REFUSED;
    }
    match got {
        Ok(()) => 0,
        Err(line) => {
            eprintln!("tehas: {line}");
            REFUSED
        }
    }
}

fn why(name: &str, value: &str) -> Option<String> {
    let var = |k: &str| env::var(k).unwrap_or_default();
    let (home, instance, work_repo) = (
        var("TEHAS_HOME"),
        var("TEHAS_INSTANCE_DIR"),
        var("TEHAS_WORK_REPO"),
    );
    let trees = Trees {
        home: &home,
        instance: &instance,
        work_repo: &work_repo,
    };
    rules::check(name, value, &trees).err()
}

fn valid_stream() -> u8 {
    let mut input = Vec::new();
    if io::stdin().read_to_end(&mut input).is_err() {
        return 1;
    }
    let mut out = Vec::new();
    let words: Vec<&[u8]> = input.split(|b| *b == 0).collect();
    for pair in words.chunks_exact(2) {
        let (name, value) = (
            String::from_utf8_lossy(pair[0]),
            String::from_utf8_lossy(pair[1]),
        );
        let answer = match why(&name, &value) {
            Some(reason) => format!("1\0{reason}\0"),
            None => "0\0\0".to_string(),
        };
        out.extend_from_slice(answer.as_bytes());
    }
    u8::from(io::stdout().write_all(&out).is_err())
}

pub fn main(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["load", uid] => run_load(uid, "bash"),
        ["load", uid, bash] => run_load(uid, bash),
        ["valid"] => valid_stream(),
        ["valid", name, value] => match why(name, value) {
            Some(reason) => {
                println!("{reason}");
                1
            }
            None => 0,
        },
        _ => {
            eprintln!("usage: console settings load <euid> [<bash>] | valid [<name> <value>]");
            64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_of_a_shell_is_split_at_the_first_equals() {
        assert_eq!(
            pairs(b"A=1\0B=x=y\0C=\0"),
            [
                (b"A".to_vec(), b"1".to_vec()),
                (b"B".to_vec(), b"x=y".to_vec()),
                (b"C".to_vec(), Vec::new()),
            ]
        );
        assert!(pairs(b"").is_empty());
    }

    #[test]
    fn a_line_with_two_lines_in_it_stays_one_value() {
        assert_eq!(pairs(b"A=1\n2\0"), [(b"A".to_vec(), b"1\n2".to_vec())]);
    }
}
