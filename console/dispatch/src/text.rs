//! What the poll reads off an issue's own text: the hash an approval binds
//! (lib/attachments.sh), the files its body names (tehas_core::size), and the
//! attachments it links or promises (lib/attachments.sh).

use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// tehas_body_sha256: the body with carriage returns dropped.
pub fn body_sha256(issue: &Value) -> String {
    let body = issue.get("body").and_then(Value::as_str).unwrap_or("");
    let sum = Sha256::digest(body.replace('\r', "").as_bytes());
    sum.iter().map(|b| format!("{b:02x}")).collect()
}

/// The files count of the measure of a body.
pub fn size_files(body: &str, max_bytes: usize, max_lines: usize) -> usize {
    tehas_core::size::measure(body.as_bytes(), max_bytes, max_lines).files
}

/// tehas_attachment_links: every attachment link in a text.
pub fn links(text: &str) -> usize {
    link_list(text).len()
}

/// tehas_attachment_links' links, each once, in the order the text has them.
pub fn link_list(text: &str) -> Vec<String> {
    static LINKS: OnceLock<Regex> = OnceLock::new();
    let re = LINKS.get_or_init(|| {
        Regex::new(r"https://(?:github\.com/user-attachments/(?:assets|files)/|github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/(?:assets|files)/[0-9]+/|user-images\.githubusercontent\.com/)[A-Za-z0-9._~%/?&=+-]+").unwrap()
    });
    let mut out: Vec<String> = Vec::new();
    for m in re.find_iter(text) {
        if !out.iter().any(|l| l == m.as_str()) {
            out.push(m.as_str().to_string());
        }
    }
    out
}

/// tehas_title_sha256: the title without its line ends; None when the
/// answer names no title, which binds none.
pub fn title_sha256(issue: &Value) -> Option<String> {
    let title = issue.get("title")?.as_str()?.replace(['\r', '\n'], "");
    let sum = Sha256::digest(title.as_bytes());
    Some(sum.iter().map(|b| format!("{b:02x}")).collect())
}

/// What a plan's start binds: the title and the body together, without the
/// white space at their ends and the carriage returns, which GitHub may change
/// on its own.
pub fn plan_sum(issue: &Value) -> String {
    let part = |k: &str| {
        issue
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .replace('\r', "")
            .trim()
            .to_string()
    };
    let sum = Sha256::digest(json!([part("title"), part("body")]).to_string().as_bytes());
    sum.iter().map(|b| format!("{b:02x}")).collect()
}

/// tehas_attachment_placeholder: the first unfilled placeholder of a text.
fn placeholder(text: &str) -> Option<String> {
    static SAID: OnceLock<Regex> = OnceLock::new();
    static SPACE: OnceLock<Regex> = OnceLock::new();
    const WHAT: [&str; 17] = [
        "screenshot",
        "image",
        "picture",
        "figure",
        "shot",
        "mockup",
        "mock",
        "diagram",
        "attachment",
        "file",
        "video",
        "gif",
        "screencast",
        "thumbnail",
        "animation",
        "example",
        "prototype",
    ];
    let said = SAID.get_or_init(|| {
        // \s, not [[:space:]]: jq's class takes Unicode spaces, Rust's is ASCII.
        Regex::new(r"(?i)[a-z0-9_.-]+(?:\s+|_)goes(?:\s+|_)here").unwrap()
    });
    let space = SPACE.get_or_init(|| Regex::new(r"\s+").unwrap());
    said.find_iter(text)
        .map(|m| m.as_str())
        .find(|m| {
            let low = m.to_ascii_lowercase();
            let spaced = space.replace_all(&low, " ");
            WHAT.iter()
                .any(|w| format!("{w} goes here") == spaced || format!("{w}_goes_here") == low)
        })
        .map(str::to_string)
}

fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(m) => m.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// tehas_texts_hold_attachment: the word a text of the answer promises when
/// none of its texts links an attachment.
pub fn promise(answer: &Value) -> Option<String> {
    let mut texts = Vec::new();
    strings(answer, &mut texts);
    if texts.iter().any(|t| links(t) > 0) {
        return None;
    }
    // jq writes the word as JSON: a form feed comes out as the two
    // characters \f, and any other control character as an escape the bash
    // refuses.
    let word = texts
        .iter()
        .filter_map(|t| placeholder(t))
        .find(|p| !p.is_empty() && p.chars().count() <= 80 && !p.contains(['\t', '\r', '\n']))?;
    if word
        .chars()
        .any(|c| (c < ' ' && c != '\x0c') || c == '\x7f')
    {
        return None;
    }
    Some(word.replace('\x0c', "\\f"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_drops_carriage_returns() {
        assert_eq!(
            body_sha256(&json!({"body": "a\r\nb"})),
            body_sha256(&json!({"body": "a\nb"}))
        );
        // printf '' | sha256sum, printf 'Words.' | sha256sum, and the same of
        // a\nb.
        assert_eq!(
            body_sha256(&json!({})),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            body_sha256(&json!({"body": "Words."})),
            "44dcebcdcc564a642123c985ea211a70d843bbb02df908a1a46172c4ca15c11f"
        );
        assert_eq!(
            body_sha256(&json!({"body": "a\r\nb"})),
            "7e18f737311b2dc3b2f269dd78396b0351f14fb66efa879f768cb23181883c78"
        );
    }

    #[test]
    fn a_plans_sum_binds_title_and_body_as_github_may_trim_them() {
        let sum = plan_sum(&json!({"title": "Fast", "body": "## Plan\n\nDo it."}));
        for same in [
            json!({"title": " Fast\n", "body": "## Plan\r\n\r\nDo it.\n"}),
            json!({"title": "Fast", "body": "\n## Plan\n\nDo it.  "}),
        ] {
            assert_eq!(plan_sum(&same), sum, "{same}");
        }
        for other in [
            json!({"title": "Fast", "body": "## Plan\n\nDo it, and more."}),
            json!({"title": "Faster", "body": "## Plan\n\nDo it."}),
            // The break between the two is part of what is summed.
            json!({"title": "Fast\n## Plan", "body": "Do it."}),
            json!({"body": "## Plan\n\nDo it."}),
            json!({}),
        ] {
            assert_ne!(plan_sum(&other), sum, "{other}");
        }
    }

    #[test]
    fn files_are_paths_between_ticks() {
        let body = "Change `lib/a.sh` and `b.rs`, not `tehas work` or `notes.txt`.\n\
                    Again `lib/a.sh`.\nNot `lib/c.sh`.\n```\n`d/e`\n```";
        assert_eq!(size_files(body, 131072, 1000), 3);
        assert_eq!(size_files("`a/b`\n`c/d`\n`e/f`", 131072, 2), 2);
        assert_eq!(size_files("`a/b` `c/d`", 6, 1000), 1);
        assert_eq!(size_files("", 131072, 1000), 0);
    }

    #[test]
    fn a_placeholder_holds_only_where_nothing_is_linked() {
        assert_eq!(
            promise(&json!({"body": "x. SCREENSHOT GOES HERE."})).as_deref(),
            Some("SCREENSHOT GOES HERE")
        );
        assert_eq!(
            promise(&json!({"body": "IMAGE_GOES_HERE"})).as_deref(),
            Some("IMAGE_GOES_HERE")
        );
        assert_eq!(promise(&json!({"body": "the value goes here"})), None);
        // jq's [[:space:]] takes a no-break space and an em space.
        assert_eq!(
            promise(&json!({"body": "SCREENSHOT\u{a0}GOES HERE"})).as_deref(),
            Some("SCREENSHOT\u{a0}GOES HERE")
        );
        assert!(promise(&json!({"body": "image\u{2003}goes here"})).is_some());
        assert_eq!(promise(&json!({"body": "SCREENSHOT\u{b}GOES HERE"})), None);
        assert_eq!(
            promise(&json!({"body": "SCREENSHOT\u{c}GOES HERE"})).as_deref(),
            Some("SCREENSHOT\\fGOES HERE")
        );
        assert_eq!(
            promise(
                &json!({"body": "image goes here ![a](https://github.com/user-attachments/assets/0f3e-41)"})
            ),
            None
        );
    }
}
