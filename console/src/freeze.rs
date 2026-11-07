//! `tehas freeze <issue>` and `tehas thaw <issue>`: hold one running cycle
//! still without ending it, and let it go on (#1745). src/usage/freeze.txt
//! and src/usage/thaw.txt are the texts a person reads.
//!
//! The commands only write and remove the request,
//! `state/cycles/<issue>.freeze`. The watch of the cycle owns the container
//! and does the pausing (console/cycle/src/freeze.rs), and it writes
//! `state/cycles/<issue>.frozen` once the container is paused. So `frozen`
//! in an answer is what the watch did, and `asked` is what was asked of it.
//!
//! Both commands answer 0 when nothing was left to do, so a caller that asks
//! every minute changes nothing by asking again.

use crate::account;
use crate::flags::oneline;
use crate::stop::is_issue;
use serde_json::{json, Value};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use tehas_core::lock::{probe, Probe};
use tehas_core::time;

pub const FREEZE_USAGE: &str = include_str!("usage/freeze.txt");
pub const THAW_USAGE: &str = include_str!("usage/thaw.txt");

/// What the words of a run asked for.
#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    issue: u64,
    reason: String,
    json: bool,
}

/// The words of `tehas freeze` (`freeze` true) or `tehas thaw`, read left to
/// right. `Ok(None)` is help, printed here.
pub fn words(given: &[String], freeze: bool) -> Result<Option<Asked>, u8> {
    let (name, usage) = if freeze {
        ("freeze", FREEZE_USAGE)
    } else {
        ("thaw", THAW_USAGE)
    };
    let bad = |why: String| {
        eprintln!("tehas {name}: {why}");
        eprint!("{usage}");
        Err(64)
    };
    let (mut issue, mut reason, mut json) = (None, None, false);
    let mut it = given.iter();
    while let Some(word) = it.next() {
        match word.as_str() {
            "-h" | "--help" => {
                tehas_core::out!("{usage}");
                return Ok(None);
            }
            "--json" => json = true,
            "--reason" if freeze => match it.next() {
                Some(r) if reason.is_none() => reason = Some(r.clone()),
                Some(_) => return bad("one --reason at most".into()),
                None => return bad("--reason needs a text".into()),
            },
            w if w.starts_with('-') => return bad(format!("unknown argument: {w}")),
            w if issue.is_some() => return bad(format!("one issue number at most: {w}")),
            w if is_issue(w) => issue = w.parse().ok(),
            w => return bad(format!("not an issue number: {w}")),
        }
    }
    let Some(issue) = issue else {
        return bad("it needs an issue number".into());
    };
    Ok(Some(Asked {
        issue,
        reason: oneline(reason.as_deref().unwrap_or("")),
        json,
    }))
}

/// What the files of one issue's cycle say.
#[derive(Debug, PartialEq, Eq)]
struct Seen {
    /// The cycle holds its lock. A lock that cannot be probed counts as
    /// held, as it does for `tehas drain`.
    running: bool,
    asked: bool,
    frozen: bool,
}

fn seen(cycles: &Path, issue: u64) -> Seen {
    let f = |end: &str| cycles.join(format!("{issue}.{end}"));
    Seen {
        running: matches!(probe(&f("lock")), Probe::Held | Probe::Unknown(_)),
        asked: f("freeze").exists(),
        frozen: f("frozen").exists(),
    }
}

fn answer(issue: u64, s: &Seen, line: &str, json: bool) {
    if json {
        let v = json!({
            "api": 1,
            "issue": issue,
            "running": s.running,
            "asked": s.asked,
            "frozen": s.frozen,
        });
        tehas_core::outln!("{v}");
    } else {
        tehas_core::outln!("{line}");
    }
}

/// The request, written through a name of its own and moved into place at
/// mode 0600, so a reader finds the whole of it or nothing.
fn write_request(file: &Path, reason: &str) -> bool {
    let held = |text: &str| Value::String(text.to_string()).to_string();
    let doc = format!(
        "{{\"by\":{},\"at\":{},\"reason\":{}}}\n",
        held(&account::name()),
        held(&time::iso(time::now())),
        held(reason),
    );
    let tmp = file.with_extension("freeze.new");
    let _ = fs::remove_file(&tmp);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(doc.as_bytes()))
        .and_then(|()| fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)))
        .and_then(|()| fs::rename(&tmp, file));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written.is_ok()
}

fn cycles_dir() -> Option<PathBuf> {
    let from = |name: &str| match env::var_os(name) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    };
    Some(
        from("TEHAS_HOME")
            .or_else(|| from("HOME"))?
            .join("state/cycles"),
    )
}

/// `tehas freeze` in `cycles`.
fn freeze_in(cycles: &Path, a: &Asked) -> u8 {
    let n = a.issue;
    let before = seen(cycles, n);
    if !before.running {
        answer(
            n,
            &before,
            &format!("#{n}: no cycle is running, nothing to freeze"),
            a.json,
        );
        return 0;
    }
    if before.asked {
        let line = if before.frozen {
            format!("#{n} is frozen already")
        } else {
            format!("#{n}: a freeze is asked already: the watch of the cycle pauses it on its next look")
        };
        answer(n, &before, &line, a.json);
        return 0;
    }
    let file = cycles.join(format!("{n}.freeze"));
    if !write_request(&file, &a.reason) {
        eprintln!("tehas freeze: could not write {}", file.display());
        return 1;
    }
    let after = seen(cycles, n);
    let line = format!(
        "#{n}: freeze asked: the watch of the cycle pauses its container on its next look, and neither its stall nor its time limit counts until tehas thaw {n}"
    );
    answer(n, &after, &line, a.json);
    0
}

/// `tehas thaw` in `cycles`.
fn thaw_in(cycles: &Path, a: &Asked) -> u8 {
    let n = a.issue;
    let file = cycles.join(format!("{n}.freeze"));
    let before = seen(cycles, n);
    if !before.asked {
        answer(
            n,
            &before,
            &format!("#{n} is not frozen, nothing to thaw"),
            a.json,
        );
        return 0;
    }
    if let Err(e) = fs::remove_file(&file) {
        eprintln!("tehas thaw: could not remove {}: {e}", file.display());
        return 1;
    }
    let after = seen(cycles, n);
    let line = if after.frozen {
        format!("#{n}: thaw asked: the watch of the cycle unpauses its container on its next look")
    } else {
        format!("#{n}: thawed before its container was paused")
    };
    answer(n, &after, &line, a.json);
    0
}

fn main_of(given: &[String], freeze: bool) -> u8 {
    let a = match words(given, freeze) {
        Ok(Some(a)) => a,
        Ok(None) => return 0,
        Err(rc) => return rc,
    };
    let Some(cycles) = cycles_dir() else {
        eprintln!("console: neither TEHAS_HOME nor HOME is set");
        return 64;
    };
    if freeze {
        freeze_in(&cycles, &a)
    } else {
        thaw_in(&cycles, &a)
    }
}

pub fn freeze(given: &[String]) -> u8 {
    main_of(given, true)
}

pub fn thaw(given: &[String]) -> u8 {
    main_of(given, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tehas_core::testdir::TestDir;

    fn args(given: &[&str]) -> Vec<String> {
        given.iter().map(|s| s.to_string()).collect()
    }

    fn asked(issue: u64, reason: &str) -> Asked {
        Asked {
            issue,
            reason: reason.into(),
            json: false,
        }
    }

    fn dir(name: &str) -> TestDir {
        TestDir::new(&format!("freeze-{name}"))
    }

    /// The lock of a running cycle, held for as long as the file lives.
    fn running(d: &Path, n: u64) -> File {
        let f = File::create(d.join(format!("{n}.lock"))).unwrap();
        f.lock().unwrap();
        f
    }

    #[test]
    fn the_words_are_an_issue_a_reason_and_json() {
        assert_eq!(
            words(&args(&["7", "--reason", "disk\nfull", "--json"]), true),
            Ok(Some(Asked {
                issue: 7,
                reason: "disk full".into(),
                json: true
            }))
        );
        assert_eq!(words(&args(&["7"]), false), Ok(Some(asked(7, ""))));
    }

    #[test]
    fn a_word_that_cannot_be_right_is_64() {
        for w in [
            &[][..],
            &["07"],
            &["seven"],
            &["7", "8"],
            &["7", "--bogus"],
            &["7", "--reason"],
            &["7", "--reason", "a", "--reason", "b"],
        ] {
            assert_eq!(words(&args(w), true), Err(64), "{w:?}");
        }
        assert_eq!(words(&args(&["7", "--reason", "x"]), false), Err(64));
    }

    #[test]
    fn a_freeze_of_no_running_cycle_writes_nothing_and_is_0() {
        let d = dir("idle");
        assert_eq!(freeze_in(&d, &asked(7, "")), 0);
        assert!(!d.join("7.freeze").exists());
    }

    #[test]
    fn a_freeze_writes_the_request_once_at_mode_600() {
        let d = dir("ask");
        let _lock = running(&d, 7);
        assert_eq!(freeze_in(&d, &asked(7, "the host disk is full")), 0);
        let file = d.join("7.freeze");
        let doc: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(doc["reason"], "the host disk is full");
        assert!(doc["at"].as_str().is_some_and(|t| t.ends_with('Z')));
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!d.join("7.freeze.new").exists());
        // Asked again: the first request is kept as it was.
        assert_eq!(freeze_in(&d, &asked(7, "another reason")), 0);
        let again: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(again["reason"], "the host disk is full");
    }

    #[test]
    fn a_thaw_removes_the_request_and_a_second_thaw_is_0() {
        let d = dir("thaw");
        let _lock = running(&d, 7);
        assert_eq!(freeze_in(&d, &asked(7, "")), 0);
        fs::write(d.join("7.frozen"), "1600\n").unwrap();
        assert_eq!(thaw_in(&d, &asked(7, "")), 0);
        assert!(!d.join("7.freeze").exists());
        // The watch's word stays until the watch unpauses.
        assert!(seen(&d, 7).frozen);
        assert_eq!(thaw_in(&d, &asked(7, "")), 0);
    }

    #[test]
    fn what_the_files_say_of_a_cycle() {
        let d = dir("seen");
        assert_eq!(
            seen(&d, 7),
            Seen {
                running: false,
                asked: false,
                frozen: false
            }
        );
        let lock = running(&d, 7);
        fs::write(d.join("7.freeze"), "{}\n").unwrap();
        fs::write(d.join("7.frozen"), "1600\n").unwrap();
        assert_eq!(
            seen(&d, 7),
            Seen {
                running: true,
                asked: true,
                frozen: true
            }
        );
        drop(lock);
        assert!(!seen(&d, 7).running);
    }
}
