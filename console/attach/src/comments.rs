//! comments.json: the comments a retry is given, made from the approval record
//! and never fetched again, in the format of trail-openers/pi-rukas#2020. The
//! dispatcher chose them at the tick (console/dispatch/src/apply/start.rs):
//! allowed accounts that may write, up to the tick, not edited after it, not
//! the bot's. This takes from the record only what fits that shape, and only
//! for the start that may carry it: a comments retry's comments, the fixer's
//! note on a millwright's start, the task of a refresh.

use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// Each item's text, in characters, and the whole blob, in bytes.
pub const CUT_ITEM: usize = 4000;
pub const BLOB_MAX: usize = 64 * 1024;
pub const FILE: &str = "comments.json";

static AT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9T:.Z-]+\z").unwrap());
static BY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9-]+(\[bot\])?\z").unwrap());
/// A record written before #1492 names a comment `pr-comment:<id>`, and the
/// fixer's note `millwright:<id>` or `fixer:<id>`.
static OLD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(pr-comment|millwright|fixer):([0-9]{1,20})\z").unwrap());

/// The blob, and what it holds.
#[derive(Debug, PartialEq)]
pub struct Blob {
    pub text: String,
    pub bytes: usize,
    pub sha256: String,
    pub count: usize,
    pub dropped: usize,
}

fn no_bot(login: &str) -> &str {
    login.strip_suffix("[bot]").unwrap_or(login)
}

fn is(v: &Value, re: &Regex) -> bool {
    v.as_str().is_some_and(|s| re.is_match(s))
}

fn cut(text: &str) -> String {
    text.replace('\r', "").chars().take(CUT_ITEM).collect()
}

/// One entry of the record as an item of the blob, or None when it is of no
/// shape this start may be given.
fn item(e: &Value, how: &str, issue: u64, repo: &str, millwright: &str) -> Option<Value> {
    let (by, at, text) = (e.get("by")?, e.get("at")?, e.get("text")?.as_str()?);
    let source = e.get("source")?.as_str()?;
    let issue_url = format!("https://github.com/{repo}/issues/{issue}");
    let mut m = Map::new();
    if source == "refresh" {
        // The dispatcher's own words, on a refresh only.
        if how != "refresh" || !is(at, &AT) {
            return None;
        }
        m.insert("source".into(), json!("refresh"));
        m.insert("id".into(), json!(0));
        m.insert("by".into(), json!("tehas"));
        m.insert("at".into(), at.clone());
        m.insert("url".into(), json!(issue_url));
        m.insert("text".into(), json!(cut(text)));
        return Some(Value::Object(m));
    }
    if !is(by, &BY) || !is(at, &AT) {
        return None;
    }
    if let Some(c) = OLD.captures(source) {
        let id: u64 = c[2].parse().ok()?;
        let fixer = &c[1] != "pr-comment";
        if fixer {
            let mine = !millwright.is_empty()
                && no_bot(by.as_str()?) == no_bot(millwright)
                && matches!(how, "millwright" | "fixer");
            if !mine {
                return None;
            }
        } else if how != "retry-tick" {
            return None;
        }
        m.insert(
            "source".into(),
            json!(if fixer { "fixer" } else { "pr-comment" }),
        );
        m.insert("id".into(), json!(id));
        m.insert("by".into(), by.clone());
        m.insert("at".into(), at.clone());
        let url = if fixer {
            format!("{issue_url}#issuecomment-{id}")
        } else {
            String::new()
        };
        m.insert("url".into(), json!(url));
        m.insert("text".into(), json!(cut(text)));
        return Some(Value::Object(m));
    }
    // The shape of #1492, written for a comments retry alone.
    let on_pr = match source {
        "issue-comment" => false,
        "pr-comment" | "review" | "review-comment" => true,
        _ => return None,
    };
    if how != "retry-tick" {
        return None;
    }
    let id = e.get("id").filter(|i| i.is_u64())?;
    let url = e.get("url").filter(|u| u.is_string())?;
    m.insert("source".into(), json!(source));
    m.insert("id".into(), id.clone());
    if on_pr {
        m.insert("pr".into(), e.get("pr").filter(|p| p.is_u64())?.clone());
    }
    if source == "review-comment" {
        if let Some(p) = e.get("path").filter(|p| p.is_string()) {
            m.insert("path".into(), p.clone());
        }
        if let Some(l) = e.get("line").filter(|l| l.is_u64()) {
            m.insert("line".into(), l.clone());
        }
    }
    m.insert("by".into(), by.clone());
    m.insert("at".into(), at.clone());
    m.insert("url".into(), url.clone());
    m.insert("text".into(), json!(cut(text)));
    Some(Value::Object(m))
}

/// The blob of an approval record, or None when it gives nothing. Items are
/// oldest first; past `BLOB_MAX` the oldest are dropped and counted. The fixer's
/// note is its one comment by `millwright`.
pub fn blob(record: &Value, issue: u64, repo: &str, millwright: &str) -> Option<Blob> {
    let how = record.get("how").and_then(Value::as_str).unwrap_or("");
    let mut items: Vec<Value> = Vec::new();
    let mut fixer = false;
    for e in record
        .get("retry_context")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(i) = item(e, how, issue, repo, millwright) else {
            continue;
        };
        match i["source"].as_str() {
            Some("fixer") if fixer => continue,
            Some("fixer") => fixer = true,
            Some("refresh") if items.iter().any(|x| x["source"] == "refresh") => continue,
            _ => {}
        }
        items.push(i);
    }
    if items.is_empty() {
        return None;
    }
    items.sort_by(|a, b| a["at"].as_str().cmp(&b["at"].as_str()));
    let until = record.get("at").cloned().unwrap_or(Value::Null);
    let mut dropped = 0;
    loop {
        let mut v = json!({"version": 1, "issue": issue, "until": until, "items": items});
        if dropped > 0 {
            v["dropped"] = json!(dropped);
        }
        let text = v.to_string();
        if text.len() <= BLOB_MAX || items.is_empty() {
            if items.is_empty() {
                return None;
            }
            let sha256 = Sha256::digest(text.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            return Some(Blob {
                bytes: text.len(),
                sha256,
                count: items.len(),
                dropped,
                text,
            });
        }
        items.remove(0);
        dropped += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(source: &str, id: u64, at: &str, text: &str) -> Value {
        json!({"source": source, "id": id, "by": "alice", "at": at, "url": format!("u{id}"), "text": text})
    }

    fn of(how: &str, ctx: Value) -> Option<Blob> {
        blob(
            &json!({"how": how, "at": "2026-11-04T10:12:00Z", "retry_context": ctx}),
            42,
            "acme/widget",
            "mw[bot]",
        )
    }

    fn items(b: &Blob) -> Vec<Value> {
        let v: Value = serde_json::from_str(&b.text).unwrap();
        v["items"].as_array().unwrap().clone()
    }

    #[test]
    fn a_comments_retry_gives_each_source_oldest_first_in_the_format_of_the_runner() {
        let mut inline = said("review-comment", 126, "2026-11-03T09:03:00Z", "line");
        inline["pr"] = json!(57);
        inline["path"] = json!("src/x.rs");
        inline["line"] = json!(88);
        let mut review = said("review", 125, "2026-11-03T09:02:00Z", "rev");
        review["pr"] = json!(57);
        let mut talk = said("pr-comment", 124, "2026-11-03T09:01:00Z", "talk\r\nmore");
        talk["pr"] = json!(57);
        let b = of(
            "retry-tick",
            json!([
                inline,
                said("issue-comment", 123, "2026-11-03T09:00:00Z", "one"),
                review,
                talk
            ]),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&b.text).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["issue"], 42);
        assert_eq!(v["until"], "2026-11-04T10:12:00Z");
        assert!(v.get("dropped").is_none());
        let got = items(&b);
        assert_eq!(
            got[0],
            json!({"source": "issue-comment", "id": 123, "by": "alice", "at": "2026-11-03T09:00:00Z", "url": "u123", "text": "one"})
        );
        assert_eq!(got[1]["text"], "talk\nmore");
        assert_eq!(got[1]["pr"], 57);
        assert_eq!(got[2]["source"], "review");
        assert_eq!(
            got[3],
            json!({"source": "review-comment", "id": 126, "pr": 57, "path": "src/x.rs", "line": 88,
                "by": "alice", "at": "2026-11-03T09:03:00Z", "url": "u126", "text": "line"})
        );
        assert_eq!((b.count, b.dropped, b.bytes), (4, 0, b.text.len()));
        assert_eq!(b.sha256.len(), 64);
    }

    #[test]
    fn comments_go_only_with_a_comments_retry() {
        let ctx = json!([said("issue-comment", 1, "2026-11-03T09:00:00Z", "x")]);
        for how in ["label", "refresh", "millwright", "outage-retry"] {
            assert_eq!(of(how, ctx.clone()), None, "{how}");
        }
    }

    #[test]
    fn an_item_of_another_shape_is_left_out() {
        let mut no_pr = said("review", 2, "2026-11-03T09:00:00Z", "x");
        no_pr["pr"] = json!("57");
        for bad in [
            said("issue-comment", 1, "now", "x"),
            json!({"source": "issue-comment", "id": "1", "by": "alice", "at": "2026", "url": "u", "text": "x"}),
            json!({"source": "issue-comment", "id": 1, "by": "a b", "at": "2026", "url": "u", "text": "x"}),
            json!({"source": "issue-comment", "id": 1, "by": "alice", "at": "2026", "text": "x"}),
            json!({"source": "other", "id": 1, "by": "alice", "at": "2026", "url": "u", "text": "x"}),
            no_pr,
        ] {
            assert_eq!(of("retry-tick", json!([bad])), None, "{bad}");
        }
    }

    #[test]
    fn the_fixer_note_is_its_one_comment_by_the_millwright() {
        let note = |id: u64, by: &str| json!({"source": format!("millwright:{id}"), "by": by, "at": "2026-11-03T09:00:00Z", "text": format!("n{id}")});
        let b = of(
            "millwright",
            json!([note(1, "alice"), note(2, "mw"), note(3, "mw[bot]")]),
        )
        .unwrap();
        assert_eq!(
            items(&b),
            [
                json!({"source": "fixer", "id": 2, "by": "mw", "at": "2026-11-03T09:00:00Z",
                "url": "https://github.com/acme/widget/issues/42#issuecomment-2", "text": "n2"})
            ]
        );
        assert_eq!(of("retry-tick", json!([note(2, "mw")])), None);
    }

    #[test]
    fn a_refresh_task_goes_only_with_a_refresh() {
        let task = json!({"source": "refresh", "by": "tehas", "at": "2026-11-03T09:00:00Z", "text": "Merge it."});
        let b = of("refresh", json!([task])).unwrap();
        assert_eq!(items(&b)[0]["source"], "refresh");
        assert_eq!(items(&b)[0]["text"], "Merge it.");
        assert_eq!(of("label", json!([task])), None);
    }

    #[test]
    fn a_record_of_before_names_its_pull_request_comments_by_source() {
        let old = json!({"source": "pr-comment:7", "by": "alice", "at": "2026-11-03T09:00:00Z", "text": "x"});
        let b = of("retry-tick", json!([old])).unwrap();
        assert_eq!(items(&b)[0]["source"], "pr-comment");
        assert_eq!(items(&b)[0]["id"], 7);
    }

    #[test]
    fn each_item_is_cut_and_past_the_cap_the_oldest_are_dropped_and_counted() {
        let long = "\u{e4}".repeat(5000);
        let ctx: Vec<Value> = (0..40)
            .map(|i| {
                said(
                    "issue-comment",
                    i,
                    &format!("2026-11-03T09:{i:02}:00Z"),
                    &long,
                )
            })
            .collect();
        let b = of("retry-tick", json!(ctx)).unwrap();
        assert!(b.bytes <= BLOB_MAX);
        let v: Value = serde_json::from_str(&b.text).unwrap();
        let got = items(&b);
        assert_eq!(got[0]["text"].as_str().unwrap().chars().count(), CUT_ITEM);
        assert_eq!(b.count + b.dropped, 40);
        assert!(b.dropped > 0);
        assert_eq!(v["dropped"], b.dropped);
        assert_eq!(got.last().unwrap()["id"], 39);
        assert_eq!(got[0]["id"], b.dropped);
    }

    #[test]
    fn a_record_that_gives_nothing_has_no_blob() {
        assert_eq!(of("retry-tick", json!([])), None);
        assert_eq!(of("retry-tick", Value::Null), None);
    }
}
