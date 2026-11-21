//! A drafted issue (plugin specification 1.0.0, the v3 part, "Drafted
//! issues, and the prototype reaching a cycle"). A plugin granted
//! `issue:draft` proposes a title, a body and one of its records; a person
//! reads it, may edit it, and files it under their own login. Here are the
//! rules on the draft, the line the filed body carries, and which records a
//! removed plugin keeps (guarantee 9).

use super::is_name;
use serde_json::Value;

/// The longest title, in characters.
pub const TITLE_MAX: usize = 256;
/// The largest body, in bytes. GitHub takes 65536; the files line needs room.
pub const BODY_MAX: usize = 60000;
/// The front of the line a filed body carries, which only tehas writes.
pub const FILES: &str = "tehas-files:";

/// A record's id: `a-z`, `0-9` and `-`, 1 to 32, not starting or ending with
/// `-`. The state store's verbs take the same.
pub fn is_record_id(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub title: String,
    pub body: String,
    /// The record whose kept files the issue links.
    pub record: String,
}

/// A draft as a plugin gives it: `{"draft": 1, "title", "body", "record"}`.
/// Every reason it is refused, in the order of the fields.
pub fn read(bytes: &[u8]) -> Result<Draft, Vec<String>> {
    let v: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Err(vec!["the draft is not JSON".to_string()]),
    };
    let Some(m) = v.as_object() else {
        return Err(vec!["the draft is not an object".to_string()]);
    };
    let mut out: Vec<String> = Vec::new();
    for key in m.keys() {
        if !["draft", "title", "body", "record"].contains(&key.as_str()) {
            out.push(format!("{key}: no field of draft version 1"));
        }
    }
    if m.get("draft") != Some(&Value::from(1)) {
        out.push("draft: expected 1".to_string());
    }
    let text = |key: &str| m.get(key).and_then(Value::as_str);
    let title = text("title").filter(|t| {
        (1..=TITLE_MAX).contains(&t.chars().count())
            && !t.trim().is_empty()
            && !t.chars().any(char::is_control)
    });
    if title.is_none() {
        out.push(format!(
            "title: expected one line of 1 to {TITLE_MAX} characters"
        ));
    }
    let body = text("body");
    match body {
        None => out.push("body: expected a string".to_string()),
        Some(b) if b.len() > BODY_MAX => out.push(format!("body: over {BODY_MAX} bytes")),
        Some(b) if b.chars().any(|c| c.is_control() && c != '\n' && c != '\t') => {
            out.push("body: a control character other than a newline or a tab".to_string())
        }
        Some(b) if b.lines().any(|l| l.trim_start().starts_with(FILES)) => out.push(format!(
            "body: a line starting {FILES}, which only tehas writes"
        )),
        Some(_) => {}
    }
    let record = text("record").filter(|r| is_record_id(r));
    if record.is_none() {
        out.push("record: expected a record's id, a-z, 0-9 and -, 1 to 32".to_string());
    }
    match (title, body, record) {
        (Some(t), Some(b), Some(r)) if out.is_empty() => Ok(Draft {
            title: t.to_string(),
            body: b.to_string(),
            record: r.to_string(),
        }),
        _ => Err(out),
    }
}

/// The body as filed: what the person kept of the draft, then the line that
/// names the record and the sha256 of its files.
pub fn filed_body(body: &str, plugin: &str, record: &str, sha256: &str) -> String {
    format!(
        "{}\n\n{FILES} {plugin}/{record} {sha256}\n",
        body.trim_end()
    )
}

/// One `tehas-files:` line of a filed body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Files {
    pub plugin: String,
    pub record: String,
    pub sha256: String,
}

/// The files lines of a body, each once, in order. A line that starts so but
/// is not exactly `tehas-files: <plugin>/<record> <sha256>` is no line.
pub fn files_lines(body: &str) -> Vec<Files> {
    let mut out: Vec<Files> = Vec::new();
    for line in body.lines() {
        let Some(rest) = line.strip_prefix(FILES).and_then(|r| r.strip_prefix(' ')) else {
            continue;
        };
        let mut parts = rest.split(' ');
        let (Some(key), Some(sha), None) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let Some((plugin, record)) = key.split_once('/') else {
            continue;
        };
        let hex = sha.len() == 64
            && sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if is_name(plugin) && is_record_id(record) && hex {
            let f = Files {
                plugin: plugin.to_string(),
                record: record.to_string(),
                sha256: sha.to_string(),
            };
            if !out.contains(&f) {
                out.push(f);
            }
        }
    }
    out
}

/// A record as retention reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub id: String,
    pub issue: Option<u64>,
    pub closed: bool,
}

/// The records of a removed plugin whose files stay (guarantee 9): those
/// filed in an issue that is still open, and not closed. Everything else of
/// the plugin goes with it.
pub fn stays(records: &[Kept], open: &[u64]) -> Vec<String> {
    records
        .iter()
        .filter(|r| !r.closed && r.issue.is_some_and(|n| open.contains(&n)))
        .map(|r| r.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn draft(edit: impl Fn(&mut serde_json::Map<String, Value>)) -> Result<Draft, Vec<String>> {
        let mut v = json!({"draft": 1, "title": "Home page: build prototype p-1", "body": "Make / match it.", "record": "p-1"});
        edit(v.as_object_mut().unwrap());
        read(v.to_string().as_bytes())
    }

    #[test]
    fn a_draft_is_a_title_a_body_and_a_record() {
        assert_eq!(
            draft(|_| {}).unwrap(),
            Draft {
                title: "Home page: build prototype p-1".into(),
                body: "Make / match it.".into(),
                record: "p-1".into(),
            }
        );
        assert_eq!(read(b"[]").unwrap_err(), ["the draft is not an object"]);
        assert_eq!(read(b"x").unwrap_err(), ["the draft is not JSON"]);
    }

    #[test]
    fn a_draft_refuses_what_it_cannot_keep_and_says_each_reason() {
        let cases: Vec<(Value, &str)> = vec![
            (json!({"draft": 2}), "draft: expected 1"),
            (json!({"title": ""}), "title: expected one line"),
            (json!({"title": "a\nb"}), "title: expected one line"),
            (
                json!({"title": "x".repeat(TITLE_MAX + 1)}),
                "title: expected one line",
            ),
            (json!({"body": "x".repeat(BODY_MAX + 1)}), "body: over"),
            (json!({"body": "a\u{0007}b"}), "body: a control character"),
            (
                json!({"body": "ok\n  tehas-files: specs/x 00"}),
                "body: a line starting tehas-files:",
            ),
            (json!({"record": "../p"}), "record: expected a record's id"),
            (
                json!({"labels": ["bug"]}),
                "labels: no field of draft version 1",
            ),
        ];
        for (edit, why) in cases {
            let got = draft(|m| {
                for (k, v) in edit.as_object().unwrap() {
                    m.insert(k.clone(), v.clone());
                }
            })
            .unwrap_err();
            assert!(got.iter().any(|g| g.starts_with(why)), "{edit}: {got:?}");
        }
        let got = draft(|m| {
            m.remove("body");
            m.insert("record".into(), json!(1));
        })
        .unwrap_err();
        assert_eq!(got.len(), 2, "{got:?}");
    }

    #[test]
    fn the_filed_body_carries_one_line_that_reads_back() {
        let sha = "a".repeat(64);
        let body = filed_body("Make / match it.\n\n", "pageticket", "p-1", &sha);
        assert_eq!(
            body,
            format!("Make / match it.\n\ntehas-files: pageticket/p-1 {sha}\n")
        );
        assert_eq!(
            files_lines(&body),
            [Files {
                plugin: "pageticket".into(),
                record: "p-1".into(),
                sha256: sha.clone(),
            }]
        );
        let twice = format!("{body}{body}");
        assert_eq!(files_lines(&twice).len(), 1);
        for bad in [
            format!("tehas-files: page-ticket/p-1 {sha}"),
            format!("tehas-files: pageticket/../x {sha}"),
            format!("tehas-files: pageticket/p-1 {}", "A".repeat(64)),
            format!("tehas-files: pageticket/p-1 {sha} more"),
            format!(" tehas-files: pageticket/p-1 {sha}"),
            format!("tehas-files:pageticket/p-1 {sha}"),
        ] {
            assert_eq!(files_lines(&bad), [], "{bad}");
        }
    }

    #[test]
    fn a_removed_plugin_keeps_only_what_an_open_issue_links() {
        let r = |id: &str, issue: Option<u64>, closed: bool| Kept {
            id: id.into(),
            issue,
            closed,
        };
        let records = [
            r("open", Some(12), false),
            r("shut-issue", Some(13), false),
            r("never-filed", None, false),
            r("closed", Some(12), true),
        ];
        assert_eq!(stays(&records, &[12]), ["open"]);
        assert_eq!(stays(&records, &[]), Vec::<String>::new());
    }
}
