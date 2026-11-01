//! The record a cycle leaves for the improvement loop, the only thing the
//! loop reads of a cycle: a directory in tmp/improve/window. Built beside the
//! window and renamed into it, so a run never sees half of one and cycles
//! that end together do not collide.

use crate::host;
use crate::jq::J;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The most bytes of an attachment file a record takes: lib/attachments.sh's
/// tehas_manifest_bytes, which stays in bash. A test holds the two together.
pub const MANIFEST_BYTES: u64 = 262_144;

const ATTACHMENTS: [&str; 3] = ["manifest.json", "images.md", "retry-context.md"];

/// A file or directory of `prefix` and six more characters, made new.
pub fn fresh(
    dir: &Path,
    prefix: &str,
    mut make: impl FnMut(&Path) -> std::io::Result<()>,
) -> Option<PathBuf> {
    static N: AtomicU32 = AtomicU32::new(0);
    for _ in 0..100 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let mut seed = nanos
            ^ std::process::id().rotate_left(13)
            ^ N.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9e37_79b9);
        let tail: String = (0..6)
            .map(|_| {
                let c = b"abcdefghijklmnopqrstuvwxyz0123456789"[(seed % 36) as usize];
                seed = seed / 36 + 0x2f1b_3c4d;
                c as char
            })
            .collect();
        let p = dir.join(format!("{prefix}{tail}"));
        match make(&p) {
            Ok(()) => return Some(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// What one cycle ended with, as far as the record says it.
#[derive(Clone, Debug, Default)]
pub struct Ended {
    pub issue: u64,
    pub started: i64,
    pub rc: i64,
    pub status: String,
    pub step: String,
    pub pr: String,
    pub kills: String,
    pub image: String,
    pub rukas: String,
    pub cap: String,
    pub reason: String,
    pub build: String,
}

fn re(pattern: &str, s: &str) -> bool {
    Regex::new(pattern).is_ok_and(|r| r.is_match(s))
}

/// The characters of free text a record keeps.
fn prose(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " _.,:;/()=+-".contains(*c))
        .collect()
}

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// A plain file modified after `since`, as `find -type f -newermt @since`.
fn newer(p: &Path, since: i64) -> bool {
    let Ok(m) = fs::symlink_metadata(p) else {
        return false;
    };
    let Ok(at) = m.modified() else {
        return false;
    };
    let base = if since >= 0 {
        UNIX_EPOCH + Duration::from_secs(since as u64)
    } else {
        UNIX_EPOCH
    };
    m.is_file() && at > base
}

/// The log of this cycle: work-<issue>-*.jsonl, written since it began, the
/// last by its name.
fn log_of(home: &Path, issue: u64, started: i64) -> Option<PathBuf> {
    let prefix = format!("work-{issue}-");
    fs::read_dir(home.join("logs"))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(&prefix) && name.ends_with(".jsonl") && newer(p, started - 5)
        })
        .max()
}

/// One line of the stream as a model can read it, None for a line that is
/// not an object or is a delta. The id of a status event differs every time,
/// so it goes, and a status repeated in a row is then kept once.
pub fn condensed(line: &str) -> Option<String> {
    let mut v = J::parse(line)?;
    let kind = match &v {
        J::Obj(_) => v.get("type").cloned(),
        _ => return None,
    };
    if kind == Some(J::str("message_update")) {
        return None;
    }
    if kind == Some(J::str("extension_ui_request")) {
        v.remove("id");
    }
    v.each_string(&|s| {
        if s.chars().nth(2000).is_some() {
            *s = s.chars().take(2000).collect::<String>() + " [cut]";
        }
    });
    Some(v.compact())
}

fn condense(raw: &Path, to: &Path) -> std::io::Result<()> {
    let mut out = BufWriter::new(fs::File::create(to)?);
    let Ok(file) = fs::File::open(raw) else {
        return Ok(());
    };
    let mut r = BufReader::new(file);
    let mut line = vec![];
    let mut last: Option<String> = None;
    loop {
        line.clear();
        if r.read_until(b'\n', &mut line).unwrap_or(0) == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&line);
        let Some(c) = condensed(text.trim_end_matches('\n')) else {
            continue;
        };
        if last.as_ref() != Some(&c) {
            writeln!(out, "{c}")?;
            last = Some(c);
        }
    }
    out.flush()
}

fn copy_or_empty(from: &Path, to: &Path) {
    if fs::copy(from, to).is_err() {
        let _ = fs::write(to, "");
    }
}

/// Container, host and concurrentCycles of a settings-view.
pub fn ran_with(view: &Value) -> (J, J, J) {
    let part = |k: &str| view.get(k).map_or(J::null(), J::from);
    (part("container"), part("host"), part("concurrentCycles"))
}

/// Write the record of a cycle that has ended; `view` is the store's
/// settings-view of it. `Err` holds a line for standard error, when there is
/// one.
pub fn write(
    home: &Path,
    e: &Ended,
    view: &Result<Value, String>,
    now: i64,
) -> Result<(), Option<String>> {
    let base = home.join("tmp/improve");
    let Some(raw) = log_of(home, e.issue, e.started) else {
        return Err(Some(format!(
            "tehas: improve: no log of issue {} since the cycle began, no record written",
            e.issue
        )));
    };
    let name = raw
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let rec = name
        .strip_suffix(".jsonl")
        .and_then(|n| n.strip_prefix("work-"))
        .unwrap_or(&name)
        .to_string();
    fs::create_dir_all(base.join("window")).map_err(|_| None)?;
    let tmp = fresh(&base, ".new.", |p| {
        fs::DirBuilder::new().mode(0o700).create(p)
    })
    .ok_or(None)?;
    let view = view.as_ref().ok();
    let from = view
        .and_then(|v| v.get("rukas_commit"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let rukas = [e.rukas.as_str(), from]
        .into_iter()
        .find(|r| re(r"^[0-9a-f]{7,40}$", r))
        .unwrap_or("");
    let shape = view.and_then(|v| v.get("view")).filter(|v| v.is_object());
    let preflight = PathBuf::from(format!("{}.preflight", raw.display()));
    let retry = fs::read(&preflight)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let retry = retry
        .lines()
        .find(|l| l.starts_with("provider retry: "))
        .map_or(String::new(), |l| cut(&prose(l), 200));
    let cap: String = e
        .cap
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_.:-".contains(*c))
        .take(80)
        .collect();
    let reason = cut(&prose(&e.reason), 200);
    let kills = if re(r"^(0|[1-9][0-9]{0,8})$", &e.kills) {
        e.kills.as_str()
    } else {
        "0"
    };
    let image = if re(r"^sha256:[0-9a-f]{64}$", &e.image) {
        e.image.as_str()
    } else {
        ""
    };
    let build = if re(r"^[0-9a-f]{40}$", &e.build) {
        e.build.as_str()
    } else {
        ""
    };
    let (ran_with, concurrent) = match shape.map(ran_with) {
        Some((container, host, n)) => (J::obj(vec![("container", container), ("host", host)]), n),
        None => (J::null(), J::null()),
    };
    let doc = J::obj(vec![
        ("issue", J::int(e.issue as i64)),
        ("started", J::int(e.started)),
        ("seconds", J::int(now - e.started)),
        ("status", J::str(&e.status)),
        ("step", J::str(&e.step)),
        ("rc", J::int(e.rc)),
        ("pr", J::text_or_null(&e.pr)),
        ("log", J::str(&name)),
        ("oomKills", J::Lit(kills.into())),
        ("rukas", J::text_or_null(rukas)),
        ("buildCommit", J::text_or_null(build)),
        ("image", J::text_or_null(image)),
        ("cap", J::text_or_null(&cap)),
        ("capReason", J::text_or_null(&reason)),
        ("settings", ran_with),
        ("concurrentCycles", concurrent),
        ("providerRetry", J::text_or_null(&retry)),
    ]);
    let built = (|| {
        fs::write(tmp.join("cycle.json"), doc.pretty() + "\n")?;
        condense(&raw, &tmp.join("stream.jsonl"))
    })();
    if built.is_err() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(None);
    }
    copy_or_empty(
        &home.join(format!("live/console-{}.txt", e.issue)),
        &tmp.join("console.txt"),
    );
    copy_or_empty(
        Path::new(&format!("{}.err", raw.display())),
        &tmp.join("err.txt"),
    );
    copy_or_empty(&preflight, &tmp.join("preflight.txt"));
    // The driver's slow notices: none of a cycle whose children all came in
    // under the store's bound.
    let slow = tmp.join("slow.jsonl");
    let (r, o) = (raw.to_string_lossy(), slow.to_string_lossy());
    if host::store(home, &["slow", &r, "--out", &o]).is_err() {
        let _ = fs::remove_file(&slow);
    }
    // What the cycle was given of the issue's attachments, as tehas work kept
    // it. Only this cycle's, only plain files, none over the bound. Never an
    // image or a file.
    for f in ATTACHMENTS {
        let from = home.join(format!("state/cycles/{}.{f}", e.issue));
        let plain =
            fs::symlink_metadata(&from).is_ok_and(|m| m.is_file() && m.size() <= MANIFEST_BYTES);
        if plain && newer(&from, e.started - 5) && fs::copy(&from, tmp.join(f)).is_err() {
            let _ = fs::remove_file(tmp.join(f));
        }
    }
    let to = base.join("window").join(&rec);
    let _ = fs::remove_dir_all(&to);
    if fs::create_dir_all(base.join("window")).is_err() || fs::rename(&tmp, &to).is_err() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(None);
    }
    Ok(())
}

/// A file made new with mode 0600 under `dir`, holding `text`.
pub fn private_file(dir: &Path, prefix: &str, text: &str) -> Option<PathBuf> {
    let mut opened = None;
    let p = fresh(dir, prefix, |p| {
        opened = Some(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(p)?,
        );
        Ok(())
    })?;
    let wrote = opened.as_mut().map(|f| writeln!(f, "{text}"));
    if !matches!(wrote, Some(Ok(()))) {
        let _ = fs::remove_file(&p);
        return None;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// The filter of the record as bash had it, for the stream held to jq.
    const JQ: &str = r#"fromjson? | select(type == "object" and .type != "message_update")
      | if .type == "extension_ui_request" then del(.id) else . end
      | walk(if type == "string" and length > 2000 then .[0:2000] + " [cut]" else . end)"#;

    #[test]
    fn the_stream_is_condensed_as_jq_condensed_it() {
        let long = "y".repeat(2500);
        let exact = "z".repeat(2000);
        let text = [
            r#"{"type":"extension_ui_request","id":"a","method":"setStatus","statusText":"review"}"#.to_string(),
            r#"{"type":"extension_ui_request","id":"b","method":"setStatus","statusText":"review"}"#.to_string(),
            r#"{"type":"message_update","delta":"x"}"#.to_string(),
            "not json".to_string(),
            "[1,2]".to_string(),
            "7".to_string(),
            String::new(),
            format!(r#"{{"type":"message_end","text":"{long}","z":[1.50,{{"b":"short","a":"{exact}"}}],"a":null}}"#),
            r#"{"b":1,"a":"é\u007f\n\u0001 \/","id":"kept"}"#.to_string(),
            r#"{"b":1,"a":"é\u007f\n\u0001 \/","id":"kept"}"#.to_string(),
            r#"{"no":"type"}"#.to_string(),
        ]
        .join("\n");
        let mut jq = Command::new("jq")
            .args(["-c", "-R", JQ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the gate has jq");
        jq.stdin
            .take()
            .unwrap()
            .write_all(format!("{text}\n").as_bytes())
            .unwrap();
        let out = jq.wait_with_output().unwrap();
        let mut last: Option<&str> = None;
        let want: Vec<&str> = String::from_utf8(out.stdout)
            .unwrap()
            .leak()
            .lines()
            .filter(|l| {
                let new = last != Some(*l);
                last = Some(l);
                new
            })
            .collect();
        let dir = std::env::temp_dir().join(format!("tehas-condense-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("raw"), format!("{text}\n")).unwrap();
        condense(&dir.join("raw"), &dir.join("out")).unwrap();
        let got = fs::read_to_string(dir.join("out")).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(got.lines().collect::<Vec<_>>(), want);
    }

    /// Not held to jq: jq -R reads 4 KiB at a time and breaks a character that
    /// falls across two reads.
    #[test]
    fn a_long_text_is_cut_at_a_character() {
        let line = format!(r#"{{"text":"{}"}}"#, "é".repeat(2001));
        let want = format!(r#"{{"text":"{} [cut]"}}"#, "é".repeat(2000));
        assert_eq!(condensed(&line).unwrap(), want);
    }

    #[test]
    fn the_bound_of_an_attachment_is_the_one_bash_keeps() {
        let lib = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lib/attachments.sh");
        let text = fs::read_to_string(lib).unwrap();
        let body = text.split("tehas_manifest_bytes() {").nth(1).unwrap();
        let n: u64 = body
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .expect("tehas_manifest_bytes echoes a number");
        assert_eq!(n, MANIFEST_BYTES);
    }

    #[test]
    fn free_text_keeps_only_prose_and_is_cut() {
        assert_eq!(prose("Connection error. @x <b>`"), "Connection error. x b");
        assert_eq!(cut(&"a".repeat(300), 200).len(), 200);
    }
}
