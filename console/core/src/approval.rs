//! The text a start is held to. The dispatcher records the hash of an issue's
//! body and title with its approval, and `tehas work` compares them as it
//! starts, so a body or a title edited after the label starts no cycle.

use serde_json::Value;
use sha2::{Digest, Sha256};

fn sum(text: &str) -> String {
    let sum = Sha256::digest(text.as_bytes());
    sum.iter().map(|b| format!("{b:02x}")).collect()
}

/// The sha256 of an issue's body. Carriage returns are dropped: the dispatcher
/// reads the body from GraphQL and tehas work from REST, and only line ends may
/// differ.
pub fn body_sha256(issue: &Value) -> String {
    sum(&issue
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('\r', ""))
}

/// The sha256 of an issue's title, without its line ends. None when the answer
/// names no title.
pub fn title_sha256(issue: &Value) -> Option<String> {
    Some(sum(&issue
        .get("title")?
        .as_str()?
        .replace(['\r', '\n'], "")))
}

fn all_of(s: &str, ok: fn(u8) -> bool) -> bool {
    !s.is_empty() && s.bytes().all(ok)
}

fn hex64(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// The shape of an approval. at, by and how are asserted by shape, not only by
/// type: they are printed to an operator and published in a cycle's record, and
/// a value with a newline in it moves the fields of a manifest read back by
/// line. The hashes are 64 lower-case hex characters and nothing else, so a
/// value with a newline on the end is refused as the malformed record it is. A
/// record written before titles were bound names none, and stays valid.
fn record_ok(r: &Value) -> bool {
    let text = |k: &str| r.get(k).and_then(Value::as_str);
    r.is_object()
        && r.get("links").is_some_and(Value::is_array)
        && text("at").is_some_and(|s| all_of(s, |b| b.is_ascii_digit() || b"T:.Z-".contains(&b)))
        && text("by").is_some_and(|s| all_of(s, |b| b.is_ascii_alphanumeric() || b == b'-'))
        && matches!(
            text("how"),
            Some("label" | "retry-tick" | "outage-retry" | "fixer" | "refresh")
        )
        && matches!(
            r.get("comment_links"),
            None | Some(Value::Null) | Some(Value::Array(_))
        )
        && hex64(r.get("body_sha256"))
        && (matches!(r.get("title_sha256"), None | Some(Value::Null))
            || hex64(r.get("title_sha256")))
}

/// Why a text is not the approved one.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The approval itself could not be read: said with its path to an
    /// operator, and only ever on the host.
    Record,
    /// The issue changed, or could not be read. None of these names a host
    /// path, so any may be put on the issue.
    Text(&'static str),
}

impl Refusal {
    pub fn why(&self) -> &'static str {
        match self {
            Refusal::Record => "the approval record could not be read",
            Refusal::Text(why) => why,
        }
    }
}

/// Hold the issue, as GitHub answers for it, to what the record approved. A
/// record that names a title is held to that title as it is to the body; an
/// answer that names no title is then a failed read. A record that names no
/// title is checked on the body alone.
pub fn hold(record: Option<&Value>, issue: &Value) -> Result<(), Refusal> {
    let Some(r) = record.filter(|r| record_ok(r)) else {
        return Err(Refusal::Record);
    };
    if !matches!(
        issue.get("body"),
        None | Some(Value::Null | Value::String(_))
    ) {
        return Err(Refusal::Text(
            "the issue's text could not be read for its hash",
        ));
    }
    if r.get("body_sha256").and_then(Value::as_str) != Some(&body_sha256(issue)) {
        return Err(Refusal::Text(
            "the issue's text is not the text that was approved: it changed after the approval",
        ));
    }
    let Some(bound) = r.get("title_sha256").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(title) = title_sha256(issue) else {
        return Err(Refusal::Text(
            "the issue's text could not be read for its title",
        ));
    };
    if title != bound {
        return Err(Refusal::Text(
            "the issue's title is not the title that was approved: it changed after the approval",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // printf 'Words.' | sha256sum
    const WORDS: &str = "44dcebcdcc564a642123c985ea211a70d843bbb02df908a1a46172c4ca15c11f";

    fn approved(body: &str, title: Option<&str>) -> Value {
        let mut r = json!({"at": "2026-10-14T00:00:00Z", "by": "alice", "how": "label", "links": [],
            "body_sha256": body_sha256(&json!({"body": body}))});
        if let Some(t) = title {
            r["title_sha256"] = json!(title_sha256(&json!({"title": t})).unwrap());
        }
        r
    }

    #[test]
    fn the_hashes_are_sha256sums_of_the_text_without_its_line_ends() {
        // printf '' | sha256sum
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(body_sha256(&json!({})), empty);
        assert_eq!(body_sha256(&json!({"body": null})), empty);
        assert_eq!(
            body_sha256(&json!({"body": "a\r\nb"})),
            body_sha256(&json!({"body": "a\nb"}))
        );
        assert_eq!(
            title_sha256(&json!({"title": "a\r\nb"})),
            title_sha256(&json!({"title": "ab"}))
        );
        assert_eq!(title_sha256(&json!({})), None);
        assert_eq!(title_sha256(&json!({"title": 7})), None);
        assert_eq!(body_sha256(&json!({"body": "Words."})), WORDS);
        assert_eq!(title_sha256(&json!({"title": "Words."})).unwrap(), WORDS);
    }

    #[test]
    fn the_approved_text_passes_and_another_does_not() {
        let r = approved("Words.", None);
        assert_eq!(hold(Some(&r), &json!({"body": "Words."})), Ok(()));
        assert_eq!(hold(Some(&r), &json!({"body": "Words.\r"})), Ok(()));
        assert_eq!(
            hold(Some(&r), &json!({"body": "Other words."})),
            Err(Refusal::Text(
                "the issue's text is not the text that was approved: it changed after the approval"
            ))
        );
    }

    #[test]
    fn a_record_with_a_title_holds_the_title_too() {
        let r = approved("Words.", Some("A title"));
        let ok = json!({"body": "Words.", "title": "A title"});
        assert_eq!(hold(Some(&r), &ok), Ok(()));
        let other = hold(Some(&r), &json!({"body": "Words.", "title": "Another"}));
        assert_eq!(
            other.unwrap_err().why(),
            "the issue's title is not the title that was approved: it changed after the approval"
        );
        let none = hold(Some(&r), &json!({"body": "Words."}));
        assert_eq!(
            none.unwrap_err().why(),
            "the issue's text could not be read for its title"
        );
    }

    #[test]
    fn a_record_of_another_shape_is_no_record() {
        let good = approved("Words.", None);
        let issue = json!({"body": "Words."});
        assert_eq!(hold(None, &issue), Err(Refusal::Record));
        let bad: [(&str, Value); 9] = [
            ("at", json!("2026\n")),
            ("by", json!("two words")),
            ("by", json!("alice\n")),
            ("how", json!("nobody")),
            ("links", json!("x")),
            ("comment_links", json!("x")),
            ("body_sha256", json!("abc")),
            ("body_sha256", json!(format!("{}\n", "a".repeat(64)))),
            ("title_sha256", json!(7)),
        ];
        for (k, v) in bad {
            let mut r = good.clone();
            r[k] = v;
            assert_eq!(hold(Some(&r), &issue), Err(Refusal::Record), "{k}");
        }
        assert_eq!(hold(Some(&json!([])), &issue), Err(Refusal::Record));
    }

    #[test]
    fn every_way_of_approving_is_a_shape_it_reads() {
        for how in ["label", "retry-tick", "outage-retry", "fixer", "refresh"] {
            let mut r = approved("Words.", None);
            r["how"] = json!(how);
            assert_eq!(hold(Some(&r), &json!({"body": "Words."})), Ok(()));
        }
    }
}
