//! Files attached to an issue, for a command of this program: the way to
//! $TEHAS_HOME/libexec/attach (console/attach), as lib/attachments.sh is for the
//! commands still in bash. The approved word stays with the caller: it is who
//! applied the label (rule 5).

use serde_json::Value;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const NOT_INSTALLED: &str =
    "the issue's files could not be checked (the attachment checker is not installed)";

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

fn or(name: &str, default: &str) -> String {
    Some(var(name))
        .filter(|v| !v.is_empty())
        .unwrap_or(default.to_string())
}

fn program() -> Option<PathBuf> {
    let home = var("TEHAS_HOME");
    let bin = Path::new(&home).join("libexec/attach");
    let ready = !home.is_empty()
        && bin
            .metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    ready.then_some(bin)
}

// The answer of a verb, when it is one.
fn answer(text: &str, verb: &str) -> Option<Value> {
    let doc: Value = serde_json::from_str(text.trim_end_matches('\n')).ok()?;
    let good = doc.get("api") == Some(&Value::from(1))
        && doc.get("ok") == Some(&Value::Bool(true))
        && doc.get("verb") == Some(&Value::from(verb));
    good.then_some(doc)
}

// `.key // ""`.
fn word(doc: &Value, key: &str) -> String {
    match doc.get(key) {
        None | Some(Value::Null | Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

// What stops the start, or nothing, from what `attach issue` ended with.
fn take_said(rc: i32, out: &str) -> Option<String> {
    let doc = (rc == 0).then(|| answer(out, "issue")).flatten();
    let Some(doc) = doc else {
        return Some(format!(
            "the issue's files could not be checked (attach: issue exited {rc})"
        ));
    };
    Some(word(&doc, "why")).filter(|why| !why.is_empty())
}

// curl's own words go to the caller's terminal.
fn take(bin: &Path, issue: &str, dir: &str, approved: &str) -> Result<(), String> {
    let done = Command::new(bin)
        .args(["issue", issue, dir, "--approved", approved, "--record", ""])
        .args(["--repo", &var("TEHAS_REPO")])
        .args(["--comments", &or("TEHAS_WORK_COMMENTS", "0")])
        .args(["--allowed", &var("TEHAS_ALLOWED_LABELERS")])
        .args(["--millwright", &var("TEHAS_MILLWRIGHT_LOGIN")])
        .arg("--attach")
        .arg(bin)
        .args(["--images", &or("TEHAS_WORK_IMAGES", "1")])
        .args(["--files", &or("TEHAS_WORK_FILES", "0")])
        .args(["--zips", &or("TEHAS_WORK_ZIPS", "0")])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output();
    let (rc, out) = done.map_or((127, Vec::new()), |o| {
        let rc = o.status.code();
        let rc = rc.unwrap_or_else(|| 128 + o.status.signal().unwrap_or(0));
        (rc, o.stdout)
    });
    take_said(rc, &String::from_utf8_lossy(&out)).map_or(Ok(()), Err)
}

// Wider than LINK_RULE of console/attach/src/given.rs, so no link it takes is
// missed. The text may be JSON, which may escape a slash.
fn may_link(text: &str) -> bool {
    let text = text.replace('\\', "");
    let github = "https://github.com/";
    let after = text.find(github).map(|at| &text[at + github.len()..]);
    after.is_some_and(|rest| rest.contains("/assets/") || rest.contains("/files/"))
        || text.contains("https://user-images.githubusercontent.com/")
}

fn empty_manifest(issue: &str, files: bool) -> String {
    let files = if files { "  \"files\": [],\n" } else { "" };
    format!(
        "{{\n  \"images\": [],\n{files}  \"leftOut\": [],\n  \"issue\": {issue},\n  \"approval\": null,\n  \"leftOutMore\": 0\n}}\n"
    )
}

// With no program nothing can be checked, so a text that links a file stops and
// one that links nothing goes on. Err is why.
fn gives_nothing(issue: &str, dir: &str) -> Result<(), String> {
    let unread = "the issue could not be read for its images";
    let data = Command::new("gh")
        .args([
            "api",
            &format!("repos/{}/issues/{issue}", var("TEHAS_REPO")),
        ])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .ok_or(unread)?;
    let data = String::from_utf8_lossy(&data.stdout);
    let doc: Value = serde_json::from_str(&data).map_err(|_| unread)?;
    let body = doc.as_object().map(|o| o.get("body"));
    if !matches!(body, Some(None | Some(Value::Null | Value::String(_)))) {
        return Err(unread.to_string());
    }
    if may_link(&data) {
        return Err(NOT_INSTALLED.to_string());
    }
    let manifest = Path::new(dir).join("manifest.json");
    let mode = |path: &Path, bits| fs::set_permissions(path, fs::Permissions::from_mode(bits));
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir)
        .and_then(|()| mode(Path::new(dir), 0o755))
        .and_then(|()| {
            fs::write(
                &manifest,
                empty_manifest(issue, var("TEHAS_WORK_FILES") == "1"),
            )
        })
        .and_then(|()| mode(&manifest, 0o644))
        .map_err(|_| "the directory for the issue's images could not be made".to_string())
}

/// tehas_issue_images, with no approval record: remake `dir` from the issue's
/// text, with a manifest.json. `approved` is the word `approved` when an allowed
/// account applied the label. Err is why nothing may start.
pub fn issue_images(issue: &str, dir: &str, approved: &str) -> Result<(), String> {
    match program() {
        Some(bin) => take(&bin, issue, dir, approved),
        None => gives_nothing(issue, dir),
    }
}

fn label_said(line: &str) -> String {
    let images = line.lines().next().and_then(|l| l.split_once('|'));
    let images = images.map_or("", |(_, images)| images);
    let known = images == "1" || images == "0";
    if known { images } else { "" }.to_string()
}

/// tehas_rukas_reads_images: 1 the image's pi-rukas reads the manifest, 0 it does
/// not, nothing when the image does not say.
pub fn reads_images(image: &str) -> String {
    const LABELS: &str =
        r#"{{index .Config.Labels "tehas.rukas"}}|{{index .Config.Labels "tehas.rukas.images"}}"#;
    let line = Command::new("docker")
        .args(["image", "inspect", "--format", LABELS, image])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or(String::new(), |o| {
            String::from_utf8_lossy(&o.stdout).into_owned()
        });
    label_said(&line)
}

// One line that is an answer of `said`, and nothing on standard error.
fn said_text(out: &[u8], err: &[u8]) -> Option<String> {
    let out = String::from_utf8_lossy(out);
    let out = out.trim_end_matches('\n');
    if !err.is_empty() || out.contains('\n') {
        return None;
    }
    let text = word(&answer(out, "said")?, "text");
    let text = text.trim_end_matches('\n');
    (!text.is_empty()).then(|| text.to_string())
}

/// tehas_issue_images_said: what `dir` gave a `what`, within 60 s. Nothing when
/// there is nothing to say or no program to say it.
pub fn said(dir: &str, what: &str, reads: &str) -> Option<String> {
    let bin = program()?;
    let home = or("TEHAS_HOME", &format!("{}/tehas", var("HOME")));
    let done = Command::new("timeout")
        .args(["-k", "2", "60"])
        .arg(bin)
        .args(["said", dir, what, "--reads", reads])
        .args(["--build", &format!("{home}/sandbox/build.sh")])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    said_text(&done.stdout, &done.stderr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_stops_on_what_the_program_says_and_on_no_answer() {
        let good = r#"{"api":1,"ok":true,"verb":"issue","why":null}"#;
        assert_eq!(take_said(0, good), None);
        let why = r#"{"api":1,"ok":true,"verb":"issue","why":"a link failed"}"#;
        assert_eq!(take_said(0, why).unwrap(), "a link failed");
        for (rc, out) in [
            (3, good),
            (0, ""),
            (0, "nonsense"),
            (0, r#"{"api":2,"ok":true,"verb":"issue"}"#),
            (0, r#"{"api":1,"ok":false,"verb":"issue"}"#),
            (0, r#"{"api":1,"ok":true,"verb":"answer"}"#),
        ] {
            assert_eq!(
                take_said(rc, out).unwrap(),
                format!("the issue's files could not be checked (attach: issue exited {rc})")
            );
        }
    }

    #[test]
    fn a_text_that_may_link_a_file_is_known_as_the_shell_knew_it() {
        for link in [
            "see https://github.com/user-attachments/assets/1",
            "https://github.com/a/b/files/2/x.md",
            r#"{"body":"https:\/\/github.com\/a\/assets\/1"}"#,
            "https://user-images.githubusercontent.com/1/x.png",
        ] {
            assert!(may_link(link), "{link}");
        }
        for plain in [
            "",
            "https://github.com/a/b",
            "/assets/ before https://github.com/a",
            "https://example.com/x/assets/1",
        ] {
            assert!(!may_link(plain), "{plain}");
        }
    }

    #[test]
    fn a_manifest_that_gives_nothing_names_files_only_when_files_are_on() {
        let doc = |files| serde_json::from_str::<Value>(&empty_manifest("7", files)).unwrap();
        assert_eq!(doc(false)["issue"], 7);
        assert_eq!(doc(false).get("files"), None);
        assert_eq!(doc(true)["files"], Value::Array(Vec::new()));
        assert_eq!(doc(true)["approval"], Value::Null);
    }

    #[test]
    fn an_image_says_one_or_zero_or_nothing() {
        assert_eq!(label_said(&format!("{}|1\n", "a".repeat(40))), "1");
        assert_eq!(label_said("|0\n"), "0");
        for none in ["", "|", "abc|2\n", "abc\n", "abc|<no value>\n"] {
            assert_eq!(label_said(none), "", "{none}");
        }
    }

    #[test]
    fn the_said_text_is_one_answer_with_nothing_beside_it() {
        let good = br#"{"api":1,"ok":true,"verb":"said","text":"Given.\n"}"#;
        assert_eq!(said_text(good, b"").unwrap(), "Given.");
        assert_eq!(said_text(good, b"warning"), None);
        let none = br#"{"api":1,"ok":true,"verb":"said","text":null}"#;
        assert_eq!(said_text(none, b""), None);
        assert_eq!(said_text(b"{}\n{}", b""), None);
    }
}
