//! `tehas order`: the order ready issues start in, kept in the state store
//! (store verbs `order-show`, `order-set`, `order-clear`; tehas_core::order
//! says what it holds).
//!
//!   tehas order [--json]
//!   tehas order set --issues N[,N...] [--only] [--for SECONDS | --until TIME]
//!       [--reason TEXT] [--json]
//!   tehas order clear [--json]
//!
//! `set` replaces the whole order and `clear` takes it off; each prints the
//! order as it then is. Setting the same order twice leaves the row as it
//! was, so a program that sets it on every poll changes nothing.
//!
//! The clock is the host's, or `TEHAS_STORE_TEST_NOW` when that names whole
//! seconds, the store's own test clock.
//!
//! Exit: 0 shown, set or cleared; 1 the store refused or could not be read;
//! 64 a bad argument, which writes nothing.

use crate::store;
use std::env;
use std::path::PathBuf;
use tehas_core::limit::{epoch, oneline};
use tehas_core::order::{list, Order, Reading};
use tehas_core::time;

const USAGE: &str = "usage: tehas order [--json] | set --issues N[,N...] [--only] [--for SECONDS | --until TIME] [--reason TEXT] [--json] | clear [--json]";

/// The widest `--for`, as for `tehas limit`.
const MAX_FOR: i64 = 999_999_999;

/// What was asked.
#[derive(Debug, PartialEq)]
pub enum Asked {
    Show { json: bool },
    Set { order: SetWords, json: bool },
    Clear { json: bool },
}

/// The words of `set`, before the clock turns `--for` into a time.
#[derive(Debug, PartialEq, Default)]
pub struct SetWords {
    issues: Vec<u64>,
    only: bool,
    for_: Option<i64>,
    until: Option<i64>,
    reason: Option<String>,
}

fn bad(why: &str) -> u8 {
    eprintln!("tehas order: {why}");
    eprintln!("{USAGE}");
    64
}

/// The arguments, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let json_only = |rest: &[String]| match rest {
        [] => Ok(false),
        [f] if f == "--json" => Ok(true),
        _ => Err(bad("--json, once, is all this takes")),
    };
    match given.split_first() {
        None => Ok(Asked::Show { json: false }),
        Some((w, rest)) if w == "--json" && rest.is_empty() => Ok(Asked::Show { json: true }),
        Some((w, rest)) if w == "show" => json_only(rest).map(|json| Asked::Show { json }),
        Some((w, rest)) if w == "clear" => json_only(rest).map(|json| Asked::Clear { json }),
        Some((w, rest)) if w == "set" => set_words(rest),
        Some(_) => Err(bad("show, set or clear")),
    }
}

fn set_words(rest: &[String]) -> Result<Asked, u8> {
    let mut want = SetWords::default();
    let (mut issues, mut json) = (false, false);
    let mut i = 0;
    while i < rest.len() {
        let value = rest.get(i + 1);
        match rest[i].as_str() {
            "--only" if !want.only => want.only = true,
            "--json" if !json => json = true,
            "--issues" if !issues => {
                let Some(v) = value else {
                    return Err(bad("--issues wants a list such as 12,8"));
                };
                want.issues = list(v).ok_or_else(|| bad("--issues wants a list such as 12,8"))?;
                issues = true;
                i += 1;
            }
            "--for" if want.for_.is_none() && want.until.is_none() => {
                let n = value
                    .filter(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit()))
                    .and_then(|v| v.parse::<i64>().ok())
                    .filter(|n| (1..=MAX_FOR).contains(n));
                want.for_ = Some(n.ok_or_else(|| bad("--for wants whole seconds, at least 1"))?);
                i += 1;
            }
            "--until" if want.for_.is_none() && want.until.is_none() => {
                let v = value
                    .and_then(|v| epoch(v))
                    .ok_or_else(|| bad("--until wants a time such as 2026-10-17T14:05:00Z"))?;
                want.until = Some(v);
                i += 1;
            }
            "--reason" if want.reason.is_none() => {
                let Some(v) = value else {
                    return Err(bad("--reason wants its text"));
                };
                let line = oneline(v);
                want.reason = (!line.is_empty()).then_some(line);
                i += 1;
            }
            _ => return Err(bad("an argument set does not know, or one given twice")),
        }
        i += 1;
    }
    if !issues {
        return Err(bad("set wants --issues; clear takes the order off"));
    }
    let probe = Order {
        issues: want.issues.clone(),
        ..Order::default()
    };
    if let Err(why) = probe.check() {
        return Err(bad(&why));
    }
    Ok(Asked::Set { order: want, json })
}

fn now() -> i64 {
    match env::var("TEHAS_STORE_TEST_NOW") {
        Ok(s) => s.parse().expect("TEHAS_STORE_TEST_NOW is not a Unix time"),
        Err(_) => time::now(),
    }
}

fn home() -> Option<PathBuf> {
    let from = |name: &str| match env::var_os(name) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    };
    from("TEHAS_HOME").or_else(|| from("HOME"))
}

/// The answer of each verb: the words, or the object with `api` first.
fn answer(r: &Reading, json: bool) -> String {
    if json {
        let body = r.json();
        return format!("{{\"api\":1,{}", &body[1..]);
    }
    r.said()
}

pub fn main(given: &[String]) -> u8 {
    let asked = match words(given) {
        Ok(a) => a,
        Err(rc) => return rc,
    };
    let Some(here) = home() else {
        eprintln!("tehas order: neither TEHAS_HOME nor HOME is set");
        return 64;
    };
    if !store::ready(&here) {
        eprintln!(
            "tehas order: no state store in {}: run install.sh",
            here.display()
        );
        return 1;
    }
    let mut args: Vec<String> = Vec::new();
    let json = match asked {
        Asked::Show { json } => {
            args.push("order-show".into());
            json
        }
        Asked::Clear { json } => {
            args.push("order-clear".into());
            json
        }
        Asked::Set { order: want, json } => {
            args.extend(["order-set".into(), "--issues".into()]);
            let issues: Vec<String> = want.issues.iter().map(u64::to_string).collect();
            args.push(issues.join(","));
            if want.only {
                args.push("--only".into());
            }
            if let Some(u) = want.until.or(want.for_.map(|s| now() + s)) {
                args.extend(["--until".into(), u.to_string()]);
            }
            if let Some(r) = want.reason {
                args.extend(["--reason".into(), r]);
            }
            json
        }
    };
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match store::call(&here, &words) {
        Ok(v) => {
            let r = v
                .get("order")
                .map(Reading::of)
                .unwrap_or(Reading::Unreadable);
            tehas_core::outln!("{}", answer(&r, json));
            0
        }
        Err(refused) => {
            eprintln!("tehas order: the store said: {}", refused.line);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn show_and_clear_take_json_alone() {
        assert_eq!(words(&[]), Ok(Asked::Show { json: false }));
        assert_eq!(words(&w(&["--json"])), Ok(Asked::Show { json: true }));
        assert_eq!(
            words(&w(&["clear", "--json"])),
            Ok(Asked::Clear { json: true })
        );
        assert_eq!(words(&w(&["clear", "x"])), Err(64));
        assert_eq!(words(&w(&["sort"])), Err(64));
    }

    #[test]
    fn set_reads_its_words() {
        let Ok(Asked::Set { order, json }) = words(&w(&[
            "set",
            "--issues",
            "12,8",
            "--only",
            "--for",
            "600",
            "--reason",
            "held\tback",
        ])) else {
            panic!("set");
        };
        assert!(!json);
        assert_eq!(order.issues, vec![12, 8]);
        assert!(order.only);
        assert_eq!(order.for_, Some(600));
        assert_eq!(order.reason.as_deref(), Some("held back"));
    }

    #[test]
    fn set_refuses_what_it_cannot_write() {
        for bad in [
            &["set"][..],
            &["set", "--issues"],
            &["set", "--issues", "1,,2"],
            &["set", "--issues", "1,1"],
            &["set", "--issues", "0"],
            &["set", "--issues", "-1"],
            &["set", "--issues", "1", "--for", "0"],
            &["set", "--issues", "1", "--for", "soon"],
            &[
                "set",
                "--issues",
                "1",
                "--for",
                "9",
                "--until",
                "2026-10-17T14:05:00Z",
            ],
            &["set", "--issues", "1", "--until", "tomorrow"],
            &["set", "--issues", "1", "--only", "--only"],
            &["set", "--issues", "1", "--issues", "2"],
        ] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
        let many: Vec<String> = (1..=201).map(|n| n.to_string()).collect();
        assert_eq!(words(&w(&["set", "--issues", &many.join(",")])), Err(64));
    }

    #[test]
    fn the_json_answer_puts_api_first() {
        let r = Reading::Ok(Order {
            issues: vec![3],
            ..Order::default()
        });
        assert_eq!(
            answer(&r, true),
            "{\"api\":1,\"state\":\"ok\",\"issues\":[3],\"only\":false}"
        );
        assert_eq!(
            answer(&Reading::None, true),
            "{\"api\":1,\"state\":\"none\"}"
        );
    }
}
