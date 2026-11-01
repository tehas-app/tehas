//! `tehas-cycle sandbox`: the docker options of a container, for the commands
//! that still hold their docker line in bash. The lists are
//! tehas_core::sandbox's. Each verb prints its words NUL separated, so a
//! command reads them with `mapfile -d ''`.

use std::env;
use std::io::{self, Write};
use std::path::Path;
use tehas_core::models;
use tehas_core::sandbox::{self, Identity, Kind, Paths};
use tehas_core::slice::{self, Host, Load};

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
    let taken = match sandbox::take(&Host::here(), &Load::from_env(), who, what) {
        Ok(taken) => taken,
        Err(line) => {
            eprintln!("{line}");
            return 78;
        }
    };
    let mut words = taken.limits;
    words.extend(taken.security);
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

// The options are held until the container's models.json and endpoints are
// written, so a run that could not write them is given none.
fn mounts(kind: Kind, p: &Paths) -> i32 {
    match sandbox::prepared_mounts(kind, p) {
        Ok(words) => {
            let state = Path::new(p.state);
            let out = state.join("models.json");
            match models::write_for_container(&out, &state.join("endpoints")) {
                Some(()) => print(&words),
                None => 1,
            }
        }
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

/// The containers tehas work starts, each row its command, its image and what
/// it runs, read off the words of the run itself. The runner stands as the
/// libexec scripts name it.
pub fn table() -> String {
    let image = "PI_ENSEMBLE_IMAGE";
    let none: [String; 0] = [];
    let runs = [
        sandbox::capabilities(image),
        sandbox::push_check(&none, "", &none, "", "", image),
        sandbox::cycle_run(&sandbox::CycleRun {
            name: "",
            limits: &none,
            security: &none,
            user: "",
            cid_file: "",
            tree: "",
            mounts: &none,
            images: &none,
            env: &none,
            hosts: &none,
            image,
            issue: 1,
            flags: &none,
        }),
    ];
    let rows = runs.iter().map(|run| {
        let at = run.iter().position(|w| w == image).unwrap_or(0);
        let cmd = match run.get(at + 1).map(String::as_str) {
            Some(sandbox::RUNNER) => "$runner",
            Some("push") => "git",
            Some(other) => other,
            None => "",
        };
        format!("work\t{image}\t{cmd}")
    });
    rows.collect::<Vec<_>>().join("\n")
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
        ["table"] => {
            println!("{}", table());
            0
        }
        ["retry", host] => {
            println!("{}", sandbox::retry_line(host));
            0
        }
        ["clone", src, tree, branch] => i32::from(!sandbox::clone(src, tree, branch)),
        ["seed", state, index] => i32::from(!sandbox::seed(state, index)),
        _ => return None,
    })
}
