//! What the poll reads off an issue's own text: the hash an approval binds
//! (lib/attachments.sh), the files its body names (lib/sizing.sh), and the
//! attachments it links or promises (lib/attachments.sh).

use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// tehas_body_sha256: the body with carriage returns dropped.
pub fn body_sha256(issue: &Value) -> String {
    let body = issue.get("body").and_then(Value::as_str).unwrap_or("");
    let sum = Sha256::digest(body.replace('\r', "").as_bytes());
    sum.iter().map(|b| format!("{b:02x}")).collect()
}

/// The files count of tehas_issue_size: distinct paths between two ticks on
/// one line, cut to the front of the body on both bounds.
pub fn size_files(body: &str, max_bytes: usize, max_lines: usize) -> usize {
    static TICKS: OnceLock<Regex> = OnceLock::new();
    static PATH: OnceLock<Regex> = OnceLock::new();
    static SKIP: OnceLock<Regex> = OnceLock::new();
    let ticks = TICKS.get_or_init(|| Regex::new("`([^`]+)`").unwrap());
    let path = PATH.get_or_init(|| Regex::new(r"/|[.](sh|rs|md|json|toml|yml)$").unwrap());
    let skip = SKIP.get_or_init(|| Regex::new("^(Not|Do not|Leave) ").unwrap());
    let cut = &body.as_bytes()[..body.len().min(max_bytes)];
    let cut = String::from_utf8_lossy(cut);
    let mut lines: Vec<&str> = cut.split('\n').collect();
    // head -n keeps that many lines, each with its newline.
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        lines.push("");
    }
    let mut paths: Vec<&str> = lines
        .iter()
        .filter(|l| !skip.is_match(l))
        .flat_map(|l| ticks.captures_iter(l).map(|c| c.get(1).unwrap().as_str()))
        .filter(|p| path.is_match(p))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths.len()
}

/// tehas_attachment_links: every attachment link in a text.
pub fn links(text: &str) -> usize {
    static LINKS: OnceLock<Regex> = OnceLock::new();
    LINKS
        .get_or_init(|| {
            Regex::new(r"https://(?:github\.com/user-attachments/(?:assets|files)/|github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/(?:assets|files)/[0-9]+/|user-images\.githubusercontent\.com/)[A-Za-z0-9._~%/?&=+-]+").unwrap()
        })
        .find_iter(text)
        .count()
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
        Regex::new(r"(?i)[a-z0-9_.-]+(?:[[:space:]]+|_)goes(?:[[:space:]]+|_)here").unwrap()
    });
    let space = SPACE.get_or_init(|| Regex::new("[[:space:]]+").unwrap());
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
    texts
        .iter()
        .filter_map(|t| placeholder(t))
        .find(|p| !p.is_empty() && p.len() <= 80 && !p.contains(['\t', '\r', '\n']))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_hash_drops_carriage_returns() {
        assert_eq!(
            body_sha256(&json!({"body": "a\r\nb"})),
            body_sha256(&json!({"body": "a\nb"}))
        );
        // printf '' | sha256sum
        assert_eq!(
            body_sha256(&json!({})),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
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
        assert_eq!(
            promise(
                &json!({"body": "image goes here ![a](https://github.com/user-attachments/assets/0f3e-41)"})
            ),
            None
        );
    }
}
