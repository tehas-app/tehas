//! `tehas-cycle sandbox`: the docker options of a container, for the commands
//! that still hold their docker line in bash. The lists are
//! tehas_core::sandbox's. Each verb prints its words NUL separated, so a
//! command reads them with `mapfile -d ''`.

use std::env;
use std::io::{self, Write};
use tehas_core::sandbox::{self, Identity, Kind, Paths};
use tehas_core::slice::{self, Load, Reading};

fn print(words: &[String]) -> i32 {
    let mut out = io::stdout().lock();
    for word in words {
        if out.write_all(word.as_bytes()).is_err() || out.write_all(b"\0").is_err() {
            return 1;
        }
    }
    0
}

fn limits(who: &str, what: &str) -> i32 {
    let taken = match sandbox::take(&Reading::probe(), &Load::from_env(), who, what) {
        Ok(taken) => taken,
        Err(line) => {
            eprintln!("{line}");
            return 78;
        }
    };
    let mut words = taken.limits;
    words.extend(taken.security);
    words.push(taken.slice.max.to_string());
    words.push(taken.slice.active);
    print(&words)
}

fn env_options(kind: Kind, tree: &str, name: &str, email: &str) -> i32 {
    let who = Identity { tree, name, email };
    print(&sandbox::env_options(
        kind,
        &who,
        &sandbox::ensemble(kind),
        &sandbox::tokens(),
    ))
}

fn mounts(kind: Kind, p: &Paths) -> i32 {
    match sandbox::prepared_mounts(kind, p) {
        Ok(words) => print(&words),
        Err(why) => {
            if !why.is_empty() {
                eprintln!("{why}");
            }
            1
        }
    }
}

fn verify(words: &[String]) -> i32 {
    match slice::verify(words, &Load::from_env()) {
        Some(n) => {
            println!("{n}");
            0
        }
        None => 1,
    }
}

fn ok() -> i32 {
    match sandbox::settings_ok() {
        Ok(()) => 0,
        Err(line) => {
            eprintln!("{line}");
            78
        }
    }
}

/// The exit status of one verb, None for words it does not take.
pub fn main(args: &[String]) -> Option<i32> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    Some(match a.as_slice() {
        ["ok"] => ok(),
        ["limits", who, what] => limits(who, what),
        ["hosts"] => print(&sandbox::hosts(
            &env::var("PI_ENSEMBLE_HOST_ALIASES").unwrap_or_default(),
        )),
        ["env", kind, tree, name, email] => env_options(Kind::parse(kind)?, tree, name, email),
        ["mounts", kind, tree, host, state, runs] => mounts(
            Kind::parse(kind)?,
            &Paths {
                tree,
                host,
                state,
                runs,
            },
        ),
        ["verify", ..] => verify(&args[1..]),
        _ => return None,
    })
}
