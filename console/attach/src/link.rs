//! The one link rule: which addresses tehas ever fetches for an issue. The
//! attachment checker, the dispatcher and the Protolab all read links by it.

use regex::Regex;
use std::sync::LazyLock;

/// Only GitHub's own storage: nothing else is ever fetched for an issue.
///
/// No part of the path past the folder is `.` or `..`, nor holds an escaped
/// `.`, `/` or `\`, so no link names a place outside the folder, however a
/// client or GitHub reads it. A link that does ends before that part.
pub const LINK_RULE: &str = r"https://(?:github\.com/user-attachments/(?:assets|files)/|github\.com/\.*[A-Za-z0-9_-][A-Za-z0-9_.-]*/\.*[A-Za-z0-9_-][A-Za-z0-9_.-]*/(?:assets|files)/[0-9]+/|user-images\.githubusercontent\.com/)\.*(?:[A-Za-z0-9_~&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))(?:[A-Za-z0-9._~?&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))*(?:/\.*(?:[A-Za-z0-9_~&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))(?:[A-Za-z0-9._~?&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))*)*";

static LINK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(LINK_RULE).unwrap());

const GITHUB: &str = "https://github.com/";
const IMAGES: &str = "https://user-images.githubusercontent.com/";

/// One whole link of GitHub's storage and nothing else.
pub fn is_link(s: &str) -> bool {
    LINK_RE
        .find(s)
        .is_some_and(|m| m.start() == 0 && m.end() == s.len())
}

/// The links of a text, each once, in the order written.
pub fn links(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in LINK_RE.find_iter(text) {
        if !out.iter().any(|have| have == m.as_str()) {
            out.push(m.as_str().to_string());
        }
    }
    out
}

/// Wider than [`LINK_RULE`], so no link it takes is missed: the check of a
/// caller that cannot run the attachment checker. The text may be JSON, which
/// may escape a slash.
pub fn may_link(text: &str) -> bool {
    let text = text.replace('\\', "");
    let after = text.find(GITHUB).map(|at| &text[at + GITHUB.len()..]);
    after.is_some_and(|rest| rest.contains("/assets/") || rest.contains("/files/"))
        || text.contains(IMAGES)
}

/// Whether a fetch of `link` carries the token: only to a whole link of an
/// attachment folder on github.com, never to GitHub's storage it redirects to.
pub fn sends_token(link: &str, token: &str) -> bool {
    link.starts_with(GITHUB) && is_link(link) && !token.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn a_text_gives_each_link_once_in_its_order() {
        let text = "a https://github.com/user-attachments/files/2/b.md, then https://user-images.githubusercontent.com/1/x.png and https://github.com/user-attachments/files/2/b.md again https://github.com/o/r/assets/9/c.png";
        assert_eq!(
            links(text),
            [
                "https://github.com/user-attachments/files/2/b.md",
                "https://user-images.githubusercontent.com/1/x.png",
                "https://github.com/o/r/assets/9/c.png",
            ]
        );
    }

    #[test]
    fn only_a_whole_link_is_a_link() {
        assert!(is_link("https://github.com/user-attachments/assets/1"));
        assert!(!is_link(" https://github.com/user-attachments/assets/1"));
        assert!(!is_link("https://github.com/user-attachments/assets/1 x"));
        assert!(!is_link("https://example.com/user-attachments/assets/1"));
        assert!(!is_link("http://github.com/user-attachments/assets/1"));
    }

    #[test]
    fn the_wide_check_takes_every_link_the_rule_takes() {
        for text in [
            "https://github.com/user-attachments/assets/1",
            "https://github.com/o/r/files/1/a.md",
            "https://user-images.githubusercontent.com/1/x.png",
            r#"{"body":"https:\/\/github.com\/user-attachments\/assets\/x"}"#,
        ] {
            assert!(may_link(text), "{text}");
        }
        for text in ["", "no link", "https://example.com/assets/1"] {
            assert!(!may_link(text), "{text}");
        }
    }

    #[test]
    fn the_token_goes_only_to_a_whole_link_on_github() {
        let link = "https://github.com/user-attachments/files/1/a.md";
        assert!(sends_token(link, "t"));
        assert!(!sends_token(link, ""));
        assert!(!sends_token(
            "https://user-images.githubusercontent.com/1/x.png",
            "t"
        ));
        assert!(!sends_token(&format!("{link} x"), "t"));
    }

    /// The rule is written once in the workspace: every reader of links asks
    /// this module.
    #[test]
    fn the_rule_is_written_once() {
        fn walk(dir: &Path, found: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                let name = entry.file_name();
                if path.is_dir() {
                    if name != "target" && name != "node_modules" {
                        walk(&path, found);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = fs::read_to_string(&path).unwrap_or_default();
                    if text.contains(r"user-images\.githubusercontent\.com/)") {
                        found.push(path.display().to_string());
                    }
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found = Vec::new();
        walk(&root, &mut found);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].ends_with("attach/src/link.rs"), "{found:?}");
    }
}
