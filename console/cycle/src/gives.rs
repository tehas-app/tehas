//! What a cycle is given of its issue's files, for tehas work (src/work.rs): the
//! way to $TEHAS_HOME/libexec/attach (console/attach), which holds which links
//! count, who may attach, how much an issue is given and what a file is, and
//! fetches. Here are the settings it is passed and what a start reads of its
//! answers. lib/attachments.sh is the same way for tehas plan. Who approved is
//! the caller's word: who applied the label (AGENTS.md rule 5).
//!
//! A reason here may be put on the issue, so it names no host path.

use crate::runner;
use crate::start::{is_record, record_value};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::events;

/// Why nothing can be checked without the program.
const NO_ATTACH: &str =
    "the issue's files could not be checked (the attachment checker is not installed)";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn env_or(name: &str, default: &str) -> String {
    let v = env(name);
    if v.is_empty() {
        default.to_string()
    } else {
        v
    }
}

/// The program, when it is there to run.
fn attach(home: &Path) -> Option<PathBuf> {
    if home.as_os_str().is_empty() {
        return None;
    }
    let bin = home.join("libexec/attach");
    let m = fs::metadata(&bin).ok()?;
    (m.is_file() && m.permissions().mode() & 0o111 != 0).then_some(bin)
}

/// Wider than LINK_RULE of console/attach/src/given.rs, so no link it takes is
/// missed. The text may be JSON, which may escape a slash.
pub fn may_link(text: &str) -> bool {
    let text = text.replace('\\', "");
    let github = "https://github.com/";
    let after = text.find(github).map(|at| &text[at + github.len()..]);
    after.is_some_and(|rest| rest.contains("/assets/") || rest.contains("/files/"))
        || text.contains("https://user-images.githubusercontent.com/")
}

fn status_of(s: std::process::ExitStatus) -> i32 {
    s.code()
        .or_else(|| s.signal().map(|n| 128 + n))
        .unwrap_or(127)
}

/// One verb that fetches nothing, within 60 s: its answer when it is one line
/// with api 1, ok and that verb. What it says on its error stream is part of
/// the line, so a program that complains gives no answer.
fn ask(home: &Path, verb: &str, args: &[String]) -> Option<Value> {
    let bin = attach(home)?;
    let (mut reader, writer) = std::io::pipe().ok()?;
    let child = {
        let mut cmd = Command::new("timeout");
        cmd.args(["-k", "2", "60"])
            .arg(&bin)
            .arg(verb)
            .args(args)
            .stdin(Stdio::null())
            .stdout(writer.try_clone().ok()?)
            .stderr(writer);
        cmd.spawn()
    };
    let mut child = child.ok()?;
    let mut out = String::new();
    let _ = reader.read_to_string(&mut out);
    if !child.wait().ok()?.success() {
        return None;
    }
    let line = out.trim_end_matches('\n');
    if line.contains('\n') {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let ok = v.get("api").and_then(Value::as_f64) == Some(1.0)
        && v.get("ok") == Some(&Value::Bool(true))
        && v.get("verb").and_then(Value::as_str) == Some(verb);
    ok.then_some(v)
}

/// A field as `jq -r '.<key> // ""'` prints it.
fn word(v: &Value, key: &str) -> String {
    match v.get(key) {
        None | Some(Value::Null | Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// One verb that fetches, the issue on standard input. No bound of its own:
/// curl has one. What curl says goes to the cycle's log. Err is why nothing may
/// start, and a program that gives no answer is a reason too.
fn take(bin: &Path, verb: &str, args: &[String], input: &str) -> Result<(), String> {
    let child = Command::new(bin)
        .arg(verb)
        .args(args)
        .arg("--attach")
        .arg(bin)
        .args(["--images", &env_or("TEHAS_WORK_IMAGES", "1")])
        .args(["--files", &env_or("TEHAS_WORK_FILES", "0")])
        .args(["--zips", &env_or("TEHAS_WORK_ZIPS", "0")])
        .env("GH_TOKEN", env("GH_TOKEN"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn();
    let unchecked =
        |rc: i32| format!("the issue's files could not be checked (attach: {verb} exited {rc})");
    let Ok(mut child) = child else {
        return Err(unchecked(127));
    };
    if let Some(mut w) = child.stdin.take() {
        let input = input.to_string();
        std::thread::spawn(move || {
            let _ = w.write_all(input.as_bytes());
        });
    }
    let Ok(out) = child.wait_with_output() else {
        return Err(unchecked(127));
    };
    let rc = status_of(out.status);
    let answer = runner::values(&String::from_utf8_lossy(&out.stdout)).unwrap_or_default();
    let said = answer.iter().rev().find(|v| {
        v.get("api").and_then(Value::as_f64) == Some(1.0)
            && v.get("ok") == Some(&Value::Bool(true))
            && v.get("verb").and_then(Value::as_str) == Some(verb)
    });
    match said {
        Some(v) if rc == 0 => match word(v, "why") {
            why if why.is_empty() => Ok(()),
            why => Err(why),
        },
        _ => Err(unchecked(rc)),
    }
}

/// An array of a record, as `(.<key> // [])` reads it: None for any other kind.
fn array_or_none<'a>(v: &'a Value, key: &str) -> Option<&'a [Value]> {
    match v.get(key) {
        None | Some(Value::Null | Value::Bool(false)) => Some(&[]),
        Some(Value::Array(a)) => Some(a),
        Some(_) => None,
    }
}

/// With no program: <dir> with a manifest that gives nothing, when there is
/// nothing to give. A record's links and the words of a retry are what a start
/// with one is given, else the links of the issue's text.
fn gives_nothing(issue: u64, dir: &Path, record: &str, data: &str) -> Result<(), String> {
    let approval = if !record.is_empty() {
        let r = record_value(Path::new(record)).ok_or(NO_ATTACH)?;
        let mut given = 0;
        for key in ["links", "comment_links", "retry_context"] {
            given += array_or_none(&r, key).ok_or(NO_ATTACH)?.len();
        }
        if given != 0 {
            return Err(NO_ATTACH.to_string());
        }
        let field = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
        json!({"at": field("at"), "by": field("by"), "how": field("how")}).to_string()
    } else {
        let unread = "the issue could not be read for its images";
        let read;
        let data = if data.is_empty() {
            let repo = env("TEHAS_REPO");
            let out = Command::new("gh")
                .args(["api", &format!("repos/{repo}/issues/{issue}")])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .map_err(|_| unread)?;
            if !out.status.success() {
                return Err(unread.to_string());
            }
            read = String::from_utf8_lossy(&out.stdout).into_owned();
            read.as_str()
        } else {
            data
        };
        let v: Value = serde_json::from_str(data).map_err(|_| unread)?;
        let body_ok = matches!(v.get("body"), None | Some(Value::Null | Value::String(_)));
        if !v.is_object() || !body_ok {
            return Err(unread.to_string());
        }
        if may_link(data) {
            return Err(NO_ATTACH.to_string());
        }
        "null".to_string()
    };
    let files = if env("TEHAS_WORK_FILES") == "1" {
        "  \"files\": [],\n"
    } else {
        ""
    };
    let unmade = "the directory for the issue's images could not be made";
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).map_err(|_| unmade)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).map_err(|_| unmade)?;
    let manifest = dir.join("manifest.json");
    let text = format!(
        "{{\n  \"images\": [],\n{files}  \"leftOut\": [],\n  \"issue\": {issue},\n  \"approval\": {approval},\n  \"leftOutMore\": 0\n}}\n"
    );
    fs::write(&manifest, text).map_err(|_| unmade)?;
    fs::set_permissions(&manifest, fs::Permissions::from_mode(0o644)).map_err(|_| unmade)?;
    Ok(())
}

/// Remake <dir> from the issue's text, with a manifest.json; Err is why
/// nothing may start. `approved` says an allowed account approved the text
/// with its label: the images are then taken whoever opened the issue. With
/// the dispatcher's approval record the links are the record's. `data` is the
/// answer the start already read for the issue, so it is not read again.
pub fn issue_images(
    home: &Path,
    issue: u64,
    dir: &Path,
    approved: &str,
    record: &str,
    data: &str,
) -> Result<(), String> {
    let Some(bin) = attach(home) else {
        return gives_nothing(issue, dir, record, data);
    };
    let args: Vec<String> = [
        issue.to_string(),
        dir.to_string_lossy().into_owned(),
        "--approved".into(),
        approved.into(),
        "--record".into(),
        record.into(),
        "--repo".into(),
        env("TEHAS_REPO"),
        "--comments".into(),
        env_or("TEHAS_WORK_COMMENTS", "0"),
        "--allowed".into(),
        env("TEHAS_ALLOWED_LABELERS"),
        "--millwright".into(),
        env("TEHAS_MILLWRIGHT_LOGIN"),
    ]
    .to_vec();
    take(&bin, "issue", &args, &format!("{data}\n"))
}

/// Err, with the reason, when the kept manifest and <dir>'s are of the same
/// approval and a link gave other bytes this time. A program that cannot
/// answer has not held the link to its bytes, so the start stops.
pub fn changed(home: &Path, kept: &Path, dir: &Path) -> Result<(), String> {
    if !is_record(&kept.to_string_lossy()) {
        return Ok(());
    }
    let manifest = dir.join("manifest.json");
    // Nothing given has no bytes to hold to the kept manifest.
    if attach(home).is_none() {
        let given = fs::read_to_string(&manifest)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok());
        let empty = given.as_ref().is_some_and(|m| {
            let images = match m.get("images") {
                None | Some(Value::Null) => Some(0),
                Some(Value::Array(a)) => Some(a.len()),
                Some(_) => None,
            };
            let files = array_or_none(m, "files").map(<[Value]>::len);
            images.zip(files).is_some_and(|(i, f)| i + f == 0)
        });
        if empty {
            return Ok(());
        }
    }
    let args = [kept, &manifest].map(|p| p.to_string_lossy().into_owned());
    let Some(answer) = ask(home, "changed", &args) else {
        return Err("the kept manifest could not be compared".to_string());
    };
    match word(&answer, "link") {
        link if link.is_empty() => Ok(()),
        link => Err(format!(
            "{link} gave other bytes than when this approval was first fetched"
        )),
    }
}

/// What a cycle given no images and no files was not given.
pub enum Gap {
    /// The line that says the gap. The manifest at <file> says the same.
    Said(String),
    /// No record, an issue that links nothing, a link in place of the file.
    Nothing,
    /// A record that could not be read, or a manifest that could not be written.
    Unread,
}

pub fn left_out(home: &Path, issue: u64, file: &Path, record: &str) -> Gap {
    if !is_record(record) {
        return Gap::Nothing;
    }
    let args = [
        issue.to_string(),
        file.to_string_lossy().into_owned(),
        record.to_string(),
        "--images".into(),
        env_or("TEHAS_WORK_IMAGES", "0"),
        "--files".into(),
        env_or("TEHAS_WORK_FILES", "0"),
    ];
    match ask(home, "left-out", &args) {
        None => Gap::Unread,
        Some(a) => match word(&a, "gap") {
            gap if gap.is_empty() => Gap::Nothing,
            gap => Gap::Said(gap),
        },
    }
}

/// The comment that says what <what> was given and whether it was shown, or
/// nothing: `reads` is the image's own word ([`reads_images`]).
pub fn said(home: &Path, dir: &Path, what: &str, reads: &str) -> String {
    let args = [
        dir.to_string_lossy().into_owned(),
        what.to_string(),
        "--reads".into(),
        reads.to_string(),
        "--build".into(),
        home.join("sandbox/build.sh").to_string_lossy().into_owned(),
    ];
    ask(home, "said", &args).map_or(String::new(), |a| word(&a, "text"))
}

/// Whether the pi-rukas in the sandbox image reads the manifest tehas writes:
/// "1", "0", or nothing when the image's label does not say.
pub fn reads_images() -> String {
    let format =
        "{{index .Config.Labels \"tehas.rukas\"}}|{{index .Config.Labels \"tehas.rukas.images\"}}";
    let out = Command::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            format,
            &env("PI_ENSEMBLE_IMAGE"),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let text = out
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let line = text.lines().next().unwrap_or("");
    match line.split_once('|') {
        Some((_, w @ ("1" | "0"))) => w.to_string(),
        _ => String::new(),
    }
}

/// Whether <login> is one of TEHAS_ALLOWED_LABELERS.
pub fn allowed(login: &str) -> bool {
    events::allowed_in(&env("TEHAS_ALLOWED_LABELERS"), login)
}

/// Whether <login> is the millwright's account, with or without [bot].
pub fn millwright_is(login: &str) -> bool {
    same_account(&env("TEHAS_MILLWRIGHT_LOGIN"), login)
}

/// A GitHub App's login is <slug>[bot] to REST and <slug> to GraphQL.
fn same_account(millwright: &str, login: &str) -> bool {
    let bare = |l: &'_ str| l.strip_suffix("[bot]").unwrap_or(l).to_string();
    !millwright.is_empty() && bare(login) == bare(millwright)
}

fn shaped(v: &Value, key: &str, ok: impl Fn(&str) -> bool) -> bool {
    v.get(key)
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty() && ok(s))
}

/// The person's approval a record rests on, {at, by, how, body_sha256}: the
/// record's own when a person gave it, its rests_on when the millwright or a
/// refresh did. None for a missing record, a link, a shape it does not know, or
/// an account that is not allowed now.
pub fn millwright_base(record: &str) -> Option<Value> {
    if !is_record(record) {
        return None;
    }
    base(
        &record_value(Path::new(record))?,
        &env("TEHAS_ALLOWED_LABELERS"),
    )
}

fn base(r: &Value, allowed: &str) -> Option<Value> {
    let how = r.get("how").and_then(Value::as_str);
    let base = if matches!(how, Some("fixer" | "refresh")) {
        r.get("rests_on")?
    } else {
        r
    };
    let ok = shaped(base, "at", |s| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || b"T:.Z-".contains(&b))
    }) && shaped(base, "by", |s| {
        s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) && shaped(base, "how", |s| {
        matches!(s, "label" | "retry-tick" | "outage-retry")
    }) && shaped(base, "body_sha256", |s| {
        s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    });
    let by = base.get("by").and_then(Value::as_str)?;
    (ok && events::allowed_in(allowed, by)).then(|| {
        let field = |k: &str| base.get(k).cloned().unwrap_or(Value::Null);
        json!({"at": field("at"), "by": field("by"), "how": field("how"),
            "body_sha256": field("body_sha256")})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_to_an_attachment_may_be_one() {
        for yes in [
            "see https://github.com/user-attachments/assets/abc",
            "https://github.com/user-attachments/files/1/a.md",
            r#"{"body":"https:\/\/github.com\/user-attachments\/assets\/x"}"#,
            "https://user-images.githubusercontent.com/1/a.png",
        ] {
            assert!(may_link(yes), "{yes}");
        }
        for no in [
            "",
            "https://github.com/acme/widget/pull/1",
            "/assets/ before https://github.com/",
            "https://example.com/assets/a.png",
        ] {
            assert!(!may_link(no), "{no}");
        }
    }

    #[test]
    fn a_field_reads_as_jq_prints_it() {
        let v = json!({"s": "x", "n": 3, "null": null, "f": false});
        assert_eq!(word(&v, "s"), "x");
        assert_eq!(word(&v, "n"), "3");
        assert_eq!(word(&v, "null"), "");
        assert_eq!(word(&v, "f"), "");
        assert_eq!(word(&v, "none"), "");
    }

    #[test]
    fn the_millwright_is_its_account_in_both_forms_and_no_other() {
        for setting in ["acme-millwright", "acme-millwright[bot]"] {
            for (login, yes) in [
                ("acme-millwright", true),
                ("acme-millwright[bot]", true),
                ("acme-millwright[bot][bot]", false),
                ("acme-millwright2", false),
                ("acme-millwright2[bot]", false),
                ("acme-millwrigh", false),
                ("[bot]", false),
                ("bot", false),
                ("", false),
            ] {
                assert_eq!(same_account(setting, login), yes, "{setting} {login}");
            }
        }
        assert!(!same_account("", ""));
        assert!(!same_account("", "alice"));
    }

    fn person(by: &str) -> Value {
        json!({"at": "2026-10-01T00:00:00Z", "by": by, "how": "label",
            "body_sha256": "a".repeat(64), "links": []})
    }

    #[test]
    fn a_record_rests_on_the_persons_approval_in_each_shape_a_start_has() {
        let fixer = json!({"at": "2026-10-02T00:00:00Z", "by": "tehas-fixer", "how": "fixer",
            "body_sha256": "a".repeat(64), "fixer_starts": 1, "rests_on": person("alice")});
        let refresh = json!({"at": "2026-10-01T00:00:00Z", "by": "alice", "how": "refresh",
            "body_sha256": "a".repeat(64), "refreshes": 1, "rests_on": person("alice")});
        for mut shape in [person("alice"), fixer, refresh] {
            shape["driver"] = json!("lean");
            let got = base(&shape, "alice bob").expect("an approval");
            assert_eq!(got["by"], "alice");
            assert_eq!(got["how"], "label");
        }
    }

    #[test]
    fn an_approval_that_is_not_one_is_refused() {
        assert!(base(&person("alice"), "bob").is_none());
        assert!(base(&person("alice\n"), "alice").is_none());
        assert!(base(&person("al ice"), "al ice").is_none());
        assert!(base(&json!({"how": "fixer", "rests_on": null}), "alice").is_none());
        let mut odd = person("alice");
        odd["how"] = json!("fixer-tick");
        assert!(base(&odd, "alice").is_none());
        let mut short = person("alice");
        short["body_sha256"] = json!("abc");
        assert!(base(&short, "alice").is_none());
    }
}
