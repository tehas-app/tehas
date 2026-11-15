//! `tehas start <issue>`: approve an issue for a start from the host (#1967).
//! console/src/usage/start.txt is the text a person reads.
//!
//!   tehas start <issue> [--json]
//!   tehas start <issue> --clear [--json]
//!
//! The approval is a row of the state store of its own kind, `host`, given by
//! no GitHub account. It binds the title and the text GitHub answers now. The
//! next poll starts the issue under the rules every start passes, and nothing
//! starts here. `--clear` takes back an approval no cycle has used.
//!
//! The clock is the host's. A debug build takes `TEHAS_STORE_TEST_NOW` when it
//! is set, the store's own test clock; a release build never reads it.
//!
//! Exit: 0 approved, withdrawn, or nothing to withdraw; 1 GitHub has no open
//! issue of that number or did not answer, or the store refused or could not be
//! read; 64 a bad argument, or a test clock that is not a Unix time, which
//! writes nothing.

use crate::store;
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use tehas_core::approval::{body_sha256, title_sha256};
use tehas_core::time;

const USAGE: &str = "usage: tehas start <issue> [--clear] [--json]";

/// The account and the how of the row: no GitHub login gave it.
const HOST: &str = "host";

#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    issue: u64,
    clear: bool,
    json: bool,
}

/// A positive number of at most ten digits, with no leading zero.
fn is_issue(word: &str) -> bool {
    let b = word.as_bytes();
    (1..=10).contains(&b.len()) && (b'1'..=b'9').contains(&b[0]) && b.iter().all(u8::is_ascii_digit)
}

fn bad(why: &str) -> u8 {
    eprintln!("tehas start: {why}");
    eprintln!("{USAGE}");
    64
}

/// The arguments, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let (mut issue, mut clear, mut json) = (None, false, false);
    for w in given {
        match w.as_str() {
            "--clear" if !clear => clear = true,
            "--json" if !json => json = true,
            w if w.starts_with('-') => return Err(bad(&format!("unknown argument: {w}"))),
            w if issue.is_some() => return Err(bad(&format!("one issue number at most: {w}"))),
            w if is_issue(w) => issue = w.parse().ok(),
            w => return Err(bad(&format!("not an issue number: {w}"))),
        }
    }
    let Some(issue) = issue else {
        return Err(bad("it needs an issue number"));
    };
    Ok(Asked { issue, clear, json })
}

/// The host's clock. A debug build -- the tests' -- takes the store's test
/// clock when it is set, read as the store reads it; a release build never
/// reads the variable. A value that is not a Unix time is the line that says
/// so, and the command stops with 64 rather than fall back to the real clock.
fn now() -> Result<i64, String> {
    #[cfg(debug_assertions)]
    let given = env::var_os(::store::TEST_NOW);
    #[cfg(not(debug_assertions))]
    let given: Option<std::ffi::OsString> = None;
    clock(given.as_deref())
}

/// The time `given` names, or the host's when it names none.
fn clock(given: Option<&std::ffi::OsStr>) -> Result<i64, String> {
    #[cfg(debug_assertions)]
    if let Some(t) = ::store::test_clock(given)? {
        return Ok(t);
    }
    #[cfg(not(debug_assertions))]
    let _ = given;
    Ok(time::now())
}

fn home() -> Option<PathBuf> {
    let from = |name: &str| match env::var_os(name) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    };
    from("TEHAS_HOME").or_else(|| from("HOME"))
}

/// The issue as GitHub's REST answer has it, or the line that says why it is
/// not one this may approve.
fn issue_of(repo: &str, n: u64) -> Result<Value, String> {
    let out = Command::new("timeout")
        .args(["20", "gh", "api", &format!("repos/{repo}/issues/{n}")])
        .output()
        .map_err(|e| format!("gh could not be run: {e}"))?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        let said = format!("{said}{}", String::from_utf8_lossy(&out.stdout));
        if said.contains("Not Found") || said.contains("HTTP 404") {
            return Err(format!("{repo} has no issue #{n}"));
        }
        let line = said.lines().next().unwrap_or("").trim();
        return Err(format!("GitHub did not answer for #{n}: {line}"));
    }
    let v: Value = serde_json::from_slice(&out.stdout)
        .map_err(|_| format!("GitHub's answer for #{n} could not be read"))?;
    if !v.is_object() {
        return Err(format!("GitHub's answer for #{n} could not be read"));
    }
    if v.get("pull_request").is_some_and(|p| !p.is_null()) {
        return Err(format!(
            "#{n} is a pull request, not an issue the factory works on"
        ));
    }
    match v.get("state").and_then(Value::as_str) {
        Some("open") => Ok(v),
        Some("closed") => Err(format!(
            "#{n} is closed, and a closed issue is never started"
        )),
        _ => Err(format!("GitHub's answer for #{n} could not be read")),
    }
}

/// An approval row of the store, as this command answers it.
fn shown(a: &Value) -> Value {
    json!({
        "at": a.get("at").and_then(Value::as_i64).map(time::iso),
        "by": a.get("by"),
        "how": a.get("how"),
        "body_sha256": a.get("body_sha256"),
        "title_sha256": a.get("title_sha256"),
    })
}

fn approve(here: &std::path::Path, repo: &str, a: &Asked) -> u8 {
    let n = a.issue;
    let at = match now() {
        Ok(t) => t.to_string(),
        Err(why) => {
            eprintln!("tehas start: {why}; nothing was recorded");
            return 64;
        }
    };
    let issue = match issue_of(repo, n) {
        Ok(v) => v,
        Err(why) => {
            eprintln!("tehas start: {why}; nothing was recorded");
            return 1;
        }
    };
    let (num, body) = (n.to_string(), body_sha256(&issue));
    let title = title_sha256(&issue);
    let mut args = vec![
        "approval-record",
        &num,
        "--at",
        &at,
        "--by",
        HOST,
        "--how",
        HOST,
        "--body-sha",
        &body,
    ];
    if let Some(t) = &title {
        args.extend(["--title-sha", t]);
    }
    match store::call(here, &args) {
        Ok(v) => {
            let approval = shown(&v["approval"]);
            if a.json {
                tehas_core::outln!(
                    "{}",
                    json!({"api": 1, "issue": n, "approval": approval, "cleared": false})
                );
            } else {
                tehas_core::outln!(
                    "approved #{n} from the host at {}: the next poll starts it when there is room and nothing holds it",
                    approval["at"].as_str().unwrap_or("?")
                );
            }
            0
        }
        Err(refused) => {
            eprintln!("tehas start: the store said: {}", refused.line);
            1
        }
    }
}

fn clear(here: &std::path::Path, a: &Asked) -> u8 {
    let n = a.issue;
    let num = n.to_string();
    let waiting = match store::call(here, &["approval-current", &num]) {
        Ok(v) => v["approval"].clone(),
        Err(refused) => {
            eprintln!("tehas start: the store said: {}", refused.line);
            return 1;
        }
    };
    let ours = waiting["how"] == HOST;
    if ours {
        let at = waiting["at"].as_i64().unwrap_or(0).to_string();
        if let Err(refused) = store::call(here, &["approval-record", &num, "--undo", "--at", &at]) {
            eprintln!("tehas start: the store said: {}", refused.line);
            return 1;
        }
    }
    let approval = if ours { shown(&waiting) } else { Value::Null };
    if a.json {
        tehas_core::outln!(
            "{}",
            json!({"api": 1, "issue": n, "approval": approval, "cleared": ours})
        );
    } else if ours {
        tehas_core::outln!(
            "withdrew the host's approval of #{n} of {}",
            approval["at"].as_str().unwrap_or("?")
        );
    } else {
        tehas_core::outln!("#{n} has no approval from the host waiting: nothing to withdraw");
    }
    0
}

pub fn main(given: &[String]) -> u8 {
    let asked = match words(given) {
        Ok(a) => a,
        Err(rc) => return rc,
    };
    let Some(here) = home() else {
        eprintln!("tehas start: neither TEHAS_HOME nor HOME is set");
        return 64;
    };
    if !store::ready(&here) {
        eprintln!(
            "tehas start: no state store in {}: run install.sh",
            here.display()
        );
        return 1;
    }
    if asked.clear {
        return clear(&here, &asked);
    }
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    approve(&here, &repo, &asked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_words_are_one_issue_and_two_switches() {
        assert_eq!(
            words(&w(&["12"])),
            Ok(Asked {
                issue: 12,
                clear: false,
                json: false
            })
        );
        assert_eq!(
            words(&w(&["--json", "12", "--clear"])),
            Ok(Asked {
                issue: 12,
                clear: true,
                json: true
            })
        );
        for bad in [
            &[][..],
            &["0"],
            &["012"],
            &["x"],
            &["12", "13"],
            &["12", "--clear", "--clear"],
            &["12", "--now"],
        ] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    /// A test clock that is not a Unix time is a line to say, never a panic,
    /// and with none set the clock is the host's. A release build reads none.
    #[cfg(debug_assertions)]
    #[test]
    fn a_bad_test_clock_is_a_line_and_not_a_panic() {
        use std::ffi::OsStr;
        assert_eq!(clock(Some(OsStr::new("1793404800"))), Ok(1_793_404_800));
        assert!(clock(None).is_ok_and(|t| t > 1_793_404_800));
        let why = clock(Some(OsStr::new("soon"))).unwrap_err();
        assert!(why.contains("TEHAS_STORE_TEST_NOW"), "{why}");
    }
}
