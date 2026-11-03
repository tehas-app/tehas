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
//!
//!   console settings words <name>
//!
//! The words a setting takes, for what ships to be held against them.
//!
//!   console settings required
//!
//! The settings an instance has to set itself, one name per line: what
//! tehas_require_instance_settings checks, and what tehas_load hands a shell as
//! _tehas_required. test/config_test.sh holds the two side by side.
//!
//!   console settings checkout | branch
//!
//! tehas_checkout and tehas_branch, as tehas_core holds them: 0, or 78 with the
//! line. The same test file holds them against lib/config.sh.
//!
//! [`load_here`] and [`require`] are tehas_load and tehas_require for this
//! program, `console run` their caller, and [`loaded`] both for a command with no
//! shell before it.

use std::env;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::process::{Command, Stdio};
use tehas_core::sandbox;
use tehas_core::settings::load::{self, Host, Sourced, Vars};
use tehas_core::settings::rules::{self, Trees};
use tehas_core::settings::INSTANCE_SETTINGS;

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

/// The variables a load starts from.
pub(crate) fn this_environment() -> Vars {
    Vars::new(env::vars_os().map(|(k, v)| (k.into_vec(), v.into_vec())))
}

/// tehas_instance_dir.
pub(crate) fn instance_dir(vars: &Vars) -> String {
    let host = Real {
        bash: String::new(),
    };
    load::instance_dir(vars, &host)
}

/// tehas_load on `vars`, for a program that runs what follows itself. Err is
/// the line for standard error, without its `tehas: `.
pub(crate) fn load_into(vars: &mut Vars, uid: &str, bash: &str) -> Result<(), String> {
    let mut host = Real {
        bash: bash.to_string(),
    };
    load::load(vars, &mut host, uid)
}

fn run_load(uid: &str, bash: &str) -> u8 {
    let start = this_environment();
    let mut vars = start.clone();
    let got = load_into(&mut vars, uid, bash);
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

// tehas_require: the first of `names` that is not set, or holds no value of its
// rule. A name set to spaces only counts as not set.
fn refusal(names: &[&str], vars: &Vars) -> Option<String> {
    let bad = load::bad(vars);
    let dir = vars.text("TEHAS_INSTANCE_DIR");
    let file = format!("{}/instance.env", if dir.is_empty() { "?" } else { &dir });
    names.iter().find_map(|name| {
        if vars.text(name).trim_matches(' ').is_empty() {
            return Some(format!("tehas: {name} is not set (see {file})"));
        }
        let (_, reason) = bad.iter().find(|(bad, _)| bad == name)?;
        Some(format!("tehas: {name} is not valid: {reason} (see {file})"))
    })
}

/// tehas_load on this process's own environment, so all it starts is given the
/// settings. Call it before any thread. Err is the status, 78, the line said on
/// standard error. Ok is what was loaded, for [`require`].
pub(crate) fn load_here(uid: &str, bash: &str) -> Result<Vars, u8> {
    let start = this_environment();
    let mut vars = start.clone();
    let got = load_into(&mut vars, uid, bash);
    let (dropped, set) = load::changes(&start, &vars);
    for name in dropped {
        env::remove_var(OsStr::from_bytes(name));
    }
    for (name, value) in set {
        env::set_var(OsStr::from_bytes(name), OsStr::from_bytes(value));
    }
    match got {
        Ok(()) => Ok(vars),
        Err(line) => {
            eprintln!("tehas: {line}");
            Err(REFUSED)
        }
    }
}

/// tehas_require on what [`load_here`] loaded.
pub(crate) fn require(names: &[&str], vars: &Vars) -> Result<(), u8> {
    match refusal(names, vars) {
        Some(line) => {
            eprintln!("{line}");
            Err(REFUSED)
        }
        None => Ok(()),
    }
}

/// tehas_load then tehas_require, for a command with no shell before it to say
/// who it runs as.
pub fn loaded(names: &[&str]) -> Result<(), u8> {
    let uid = sandbox::user();
    let uid = uid.split(':').next().unwrap_or_default();
    require(names, &load_here(uid, "bash")?)
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
    let (pairs, _) = words.as_chunks::<2>();
    for [name, value] in pairs {
        let (name, value) = (
            String::from_utf8_lossy(name),
            String::from_utf8_lossy(value),
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
                tehas_core::outln!("{reason}");
                1
            }
            None => 0,
        },
        ["words", name] => match rules::words(name) {
            Some(words) => {
                tehas_core::outln!("{}", words.join(" "));
                0
            }
            None => 1,
        },
        ["required"] => {
            for name in INSTANCE_SETTINGS {
                tehas_core::outln!("{name}");
            }
            0
        }
        // The checkout and the branch, as tehas_core holds them, for
        // test/config_test.sh to hold against lib/config.sh.
        ["checkout"] => match tehas_core::settings::checkout() {
            Ok(()) => 0,
            Err(line) => {
                eprintln!("{line}");
                REFUSED
            }
        },
        ["branch"] => match tehas_core::settings::branch() {
            Ok(name) => {
                tehas_core::outln!("{name}");
                0
            }
            Err(line) => {
                eprintln!("{line}");
                REFUSED
            }
        },
        _ => {
            eprintln!(
                "usage: console settings load <euid> [<bash>] | valid [<name> <value>] | words <name> | required | checkout | branch"
            );
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
    fn a_required_setting_is_set_and_holds_a_value_of_its_rule() {
        let vars = |list: &[(&str, &str)]| {
            Vars::new(
                list.iter()
                    .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec())),
            )
        };
        let good = vars(&[("TEHAS_REPO", "acme/widget")]);
        assert_eq!(refusal(&["TEHAS_REPO"], &good), None);
        for empty in ["", "   "] {
            assert_eq!(
                refusal(&["TEHAS_REPO"], &vars(&[("TEHAS_REPO", empty)])).unwrap(),
                "tehas: TEHAS_REPO is not set (see ?/instance.env)"
            );
        }
        let bad = vars(&[("TEHAS_REPO", "no"), ("TEHAS_INSTANCE_DIR", "/i")]);
        let said = refusal(&["TEHAS_REPO"], &bad).unwrap();
        assert!(
            said.starts_with("tehas: TEHAS_REPO is not valid: "),
            "{said}"
        );
        assert!(said.ends_with(" (see /i/instance.env)"), "{said}");
    }

    #[test]
    fn a_line_with_two_lines_in_it_stays_one_value() {
        assert_eq!(pairs(b"A=1\n2\0"), [(b"A".to_vec(), b"1\n2".to_vec())]);
    }
}
