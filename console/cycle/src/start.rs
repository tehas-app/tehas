//! `tehas-cycle start-check`: what a cycle's start is held to before anything of
//! it is written. Two steps, because libexec/work asks the image what it
//! supports between them and each refusal keeps its place in that order.
//!
//! `record` reads the approval of this start into a private file and prints its
//! path and, on a second line, why a start with no approval stops, if it does.
//! It refuses driver lean with 78 and the note `<issue>.lean-refused`.
//!
//! `text` reads the issue once, holds its words to the approval, and measures
//! them into state/sizes.jsonl. It says why a start stopped on the issue and
//! exits 4, or prints the path of a private file that holds GitHub's answer,
//! which is the one read the images and files are fetched from too. By hand,
//! with no record on this host, the run is its own approval: GitHub is asked for
//! nothing and the line it prints is empty.
//!
//! Under tehasbot cycle only the approval the attempt used counts, and a store
//! that cannot be read is never taken for a run by hand. Run on its own the
//! approval of the issue's latest start counts, and none is a run by hand.

use crate::host;
use crate::runner;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tehas_core::approval::{self, Refusal};
use tehas_core::call;

/// `[1-9][0-9]{0,8}`: an attempt as the store gives it.
pub fn attempt_shaped(a: &str) -> bool {
    (1..=9).contains(&a.len()) && a.bytes().all(|b| b.is_ascii_digit()) && !a.starts_with('0')
}

/// A new file only this account reads under $TEHAS_HOME/tmp, holding `text`.
pub fn private_file(home: &Path, name: &str, text: &[u8]) -> Option<PathBuf> {
    let dir = home.join("tmp");
    if !dir.exists() {
        let _ = fs::DirBuilder::new().mode(0o700).create(&dir);
    }
    let m = fs::symlink_metadata(&dir).ok()?;
    if !m.is_dir() {
        return None;
    }
    for i in 0..100u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .subsec_nanos();
        let file = dir.join(format!("{name}.{}{nanos}{i}", std::process::id()));
        let made = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file);
        match made {
            Ok(mut f) => {
                if f.write_all(text).is_err() {
                    let _ = fs::remove_file(&file);
                    return None;
                }
                return Some(file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// What the store has of an approval.
pub enum View {
    File(PathBuf),
    None,
    Unread,
}

/// tehas_approval_view of lib/store.sh.
fn view(home: &Path, issue: &str, attempt: Option<&str>) -> View {
    let mut args = vec!["approval-view", issue];
    if let Some(a) = attempt {
        args.extend(["--attempt", a]);
    }
    let Ok(answer) = host::store_said(home, &args) else {
        return View::Unread;
    };
    match answer.get("view") {
        None => View::Unread,
        Some(Value::Null) => View::None,
        Some(v) => match private_file(home, "approval", format!("{v}\n").as_bytes()) {
            Some(f) => View::File(f),
            None => View::Unread,
        },
    }
}

/// The record of this start and why a start without one stops.
fn read_record(home: &Path, issue: &str, attempt: &str) -> (Option<PathBuf>, &'static str) {
    if attempt.is_empty() {
        return match view(home, issue, None) {
            View::File(f) => (Some(f), ""),
            View::None => (None, ""),
            View::Unread => (None, "the approval of this issue could not be read"),
        };
    }
    if !attempt_shaped(attempt) {
        return (None, "its start names no attempt this host can read");
    }
    match view(home, issue, Some(attempt)) {
        View::File(f) => (Some(f), ""),
        View::Unread => (None, "the approval of this start could not be read"),
        // None used by this start: one by hand, as the store has it, or one
        // whose approval is gone.
        View::None => match host::store_said(home, &["cycle-show", issue]) {
            Err(_) => (None, "the approval of this start could not be read"),
            Ok(shown) => {
                let cycle = shown.get("cycle").filter(|c| !c.is_null());
                let this = cycle.filter(|c| {
                    c.get("attempt").and_then(Value::as_f64) == attempt.parse::<f64>().ok()
                });
                let how = this.and_then(|c| c.get("how")).and_then(Value::as_str);
                if how == Some("hand") {
                    (None, "")
                } else {
                    (None, "no approval of this start is on record")
                }
            }
        },
    }
}

/// Whether the approval says a driver: lean needs what this tehas cannot run
/// yet, and classic never runs in its place.
pub fn wants_driver(record: &Value) -> bool {
    !matches!(
        record.get("driver"),
        None | Some(Value::Null) | Some(Value::Bool(false))
    ) && record.get("driver") != Some(&Value::String(String::new()))
}

pub fn record_value(file: &Path) -> Option<Value> {
    let text = fs::read_to_string(file).ok()?;
    runner::values(&text)?.pop()
}

pub fn is_record(p: &str) -> bool {
    !p.is_empty() && fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
}

/// The `record` step. Its exit status.
pub fn record(home: &Path, issue: u64, attempt: &str) -> i32 {
    let n = issue.to_string();
    let (file, why) = read_record(home, &n, attempt);
    if let Some(f) = &file {
        if record_value(f).is_some_and(|r| wants_driver(&r)) {
            let _ = fs::remove_file(f);
            let _ = fs::File::create(home.join(format!("state/cycles/{n}.lean-refused")));
            eprintln!(
                "tehas work: this tehas cannot run the lean driver yet, not starting a cycle; take tehas:lean off #{n} to run it on classic"
            );
            return 78;
        }
    }
    let path = file.map(|f| f.to_string_lossy().into_owned());
    println!("{}\n{why}", path.unwrap_or_default());
    0
}

/// What the `text` step works with.
pub struct Text {
    pub issue: u64,
    pub repo: String,
    pub record: String,
    pub why: String,
    pub log: PathBuf,
    pub wait: u64,
    pub console: String,
}

/// The answer read for the issue, or why a start stops for it.
fn read_issue(t: &Text) -> Result<Vec<u8>, String> {
    let err = OpenOptions::new()
        .create(true)
        .append(true)
        .open(side(&t.log, ".err"))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    let out = Command::new("timeout")
        .args(["-k", "10", &t.wait.to_string(), "gh", "api"])
        .arg(format!("repos/{}/issues/{}", t.repo, t.issue))
        .stdin(Stdio::null())
        .stderr(err)
        .output();
    let refused = "the issue could not be read for its text: GitHub refused the read";
    let Ok(out) = out else {
        return Err(refused.to_string());
    };
    match out.status.code() {
        Some(0) => Ok(out.stdout),
        Some(124 | 137) => Err(format!(
            "the issue could not be read for its text: no answer within {}s",
            t.wait
        )),
        _ => Err(refused.to_string()),
    }
}

/// `<file>` with `end` on its name.
fn side(file: &Path, end: &str) -> PathBuf {
    let mut s = file.as_os_str().to_owned();
    s.push(end);
    PathBuf::from(s)
}

/// What a start stops for, in the answer for the issue. An answer that is a pull
/// request, or an issue nobody can start a cycle on, is refused before its text
/// is hashed.
fn held(t: &Text, answer: &[u8]) -> Result<Value, (String, bool)> {
    let unread = || {
        (
            "the issue could not be read for its text".to_string(),
            false,
        )
    };
    let text = String::from_utf8_lossy(answer);
    let mut values = runner::values(&text).ok_or_else(unread)?;
    if values.len() != 1 {
        return Err(unread());
    }
    let issue = values.remove(0);
    if !issue.is_object()
        || !matches!(
            issue.get("body"),
            None | Some(Value::Null | Value::String(_))
        )
    {
        return Err(unread());
    }
    let open = issue.get("state").and_then(Value::as_str) == Some("open");
    if !matches!(issue.get("pull_request"), None | Some(Value::Null)) || !open {
        return Err((
            format!(
                "#{} is not an open issue, as GitHub answers for it",
                t.issue
            ),
            false,
        ));
    }
    let record = record_value(Path::new(&t.record));
    match approval::hold(record.as_ref(), &issue) {
        Ok(()) => Ok(issue),
        Err(r) => Err((r.why().to_string(), r == Refusal::Record)),
    }
}

/// What the cycle starts with, measured. A line that cannot be written is one
/// line on the error stream: the measure is a record of the start, never a
/// reason to stop one.
fn measure(home: &Path, t: &Text, issue: &Value) {
    let ledger = home.join("state/sizes.jsonl");
    let _ = fs::create_dir_all(home.join("state"));
    let body = issue.get("body").and_then(Value::as_str).unwrap_or("");
    let input = format!("{}\n", body.trim_end_matches('\n'));
    let args = vec![
        "size".to_string(),
        "record".to_string(),
        t.issue.to_string(),
        ledger.to_string_lossy().into_owned(),
    ];
    let ran = call::run_input(&t.console, &args, Some(&input), Duration::from_secs(120));
    if !ran.ok() {
        let word = if ran.out.trim() == "ledger" {
            "ledger"
        } else {
            "measure"
        };
        eprintln!(
            "tehas work: could not record the size of #{} in {} ({word}), starting the cycle all the same",
            t.issue,
            ledger.display()
        );
    }
}

/// A comment that says why no cycle started. Its path stays on the host.
pub fn tell(issue: u64, repo: &str, wait: u64, why: &str) {
    let body = format!("Tehas started no cycle: {why}. To retry, apply `tehas:ready`.");
    let done = Command::new("timeout")
        .args(["-k", "10", &wait.to_string(), "gh", "issue", "comment"])
        .args([&issue.to_string(), "--repo", repo, "--body", &body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !done.is_ok_and(|s| s.success()) {
        eprintln!("tehas work: the comment on #{issue} failed");
    }
}

/// The `text` step. Its exit status.
pub fn text(home: &Path, t: &Text) -> i32 {
    let stop = |why: &str, path: bool| {
        let shown = if path {
            format!("{why} ({})", t.record)
        } else {
            why.to_string()
        };
        eprintln!("tehas work: {shown}, not starting a cycle");
        tell(t.issue, &t.repo, t.wait, why);
        4
    };
    if !t.why.is_empty() {
        return stop(&t.why, false);
    }
    if !is_record(&t.record) {
        eprintln!(
            "tehas work: no approval is recorded on this host for #{}; a run by hand is its own approval",
            t.issue
        );
        println!();
        return 0;
    }
    let answer = match read_issue(t) {
        Ok(a) => a,
        Err(why) => return stop(&why, false),
    };
    let issue = match held(t, &answer) {
        Ok(i) => i,
        Err((why, path)) => return stop(&why, path),
    };
    measure(home, t, &issue);
    match private_file(home, "issue", &answer) {
        Some(f) => {
            println!("{}", f.display());
            0
        }
        None => {
            eprintln!("tehas work: the issue's answer could not be kept, not starting a cycle");
            78
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;

    fn home(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tehas-start-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_attempt_is_digits_with_no_leading_zero() {
        for good in ["1", "42", "999999999"] {
            assert!(attempt_shaped(good), "{good}");
        }
        for bad in ["", "0", "01", "1234567890", "1a", "-1"] {
            assert!(!attempt_shaped(bad), "{bad}");
        }
    }

    #[test]
    fn a_private_file_is_the_accounts_alone() {
        let h = home("private");
        let f = private_file(&h, "approval", b"{}\n").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"{}\n");
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(h.join("tmp")).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn a_link_where_the_directory_goes_gives_no_file() {
        let h = home("link");
        let other = home("link-other");
        std::os::unix::fs::symlink(&other, h.join("tmp")).unwrap();
        assert!(private_file(&h, "approval", b"x").is_none());
    }

    #[test]
    fn lean_is_any_driver_the_record_names() {
        assert!(wants_driver(&json!({"driver": "lean"})));
        assert!(wants_driver(&json!({"driver": 1})));
        for no in [
            json!({}),
            json!({"driver": null}),
            json!({"driver": false}),
            json!({"driver": ""}),
        ] {
            assert!(!wants_driver(&no), "{no}");
        }
    }

    #[test]
    fn a_record_is_a_plain_file_and_no_link() {
        let h = home("record");
        let f = h.join("r");
        fs::write(&f, "{}").unwrap();
        assert!(is_record(f.to_str().unwrap()));
        let l = h.join("l");
        std::os::unix::fs::symlink(&f, &l).unwrap();
        assert!(!is_record(l.to_str().unwrap()));
        assert!(!is_record(h.to_str().unwrap()));
        assert!(!is_record(""));
    }
}
