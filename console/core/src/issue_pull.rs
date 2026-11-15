//! Whether a pull request is the one of an issue. The runner, the report,
//! finish and the dispatcher all ask it, and all ask it here, so a pull request
//! one of them counts as the issue's is the issue's for the others too.

use serde_json::Value;

/// A pull request is the issue's when its head is the issue's branch,
/// `feature/issue-<n>` or `feature/issue-<n>-<slug>`, or its body closes the
/// issue. The head is GitHub's answer and is only compared, never handed to git.
pub fn names_issue(pr: &Value, issue: u64) -> bool {
    let branch = format!("feature/issue-{issue}");
    let named = pr
        .get("headRefName")
        .and_then(Value::as_str)
        .is_some_and(|h| h == branch || h.starts_with(&format!("{branch}-")));
    named
        || pr
            .get("body")
            .and_then(Value::as_str)
            .is_some_and(|b| closes_issue(b, issue))
}

/// Whether a pull request's body closes the issue: a word of closing, the
/// reference, and no longer number after it. The reading the shell gave a
/// jq program, held to the ASCII words the factory's own bodies use. `#70` does
/// not name 7, and a bare `#7` names nothing.
pub fn closes_issue(text: &str, issue: u64) -> bool {
    const WORDS: [&str; 9] = [
        "closes", "close", "closed", "fixes", "fix", "fixed", "resolves", "resolve", "resolved",
    ];
    let reference = format!("#{issue}");
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let at = reference.as_bytes();
    // What may stand after the reference: white space, or the punctuation a body
    // puts after it, `Fixes #7.` and `Closes #7)` among them. A digit would make
    // the number longer, `#7` of `#70` being no reference to 7 at all.
    let ends = |one: u8| {
        one.is_ascii_whitespace() || matches!(one, b'.' | b',' | b';' | b':' | b')' | b']')
    };
    bytes
        .windows(at.len())
        .enumerate()
        .filter(|(_, w)| *w == at)
        .any(|(i, _)| {
            if bytes.get(i + at.len()).is_some_and(|one| !ends(*one)) {
                return false;
            }
            // The reference is a word of its own: white space, or one of the
            // writer's marks, stands before its `#`. Where the `#` is written
            // against a word, `Fixes#7`, the writer left the number no room and
            // named nothing.
            match lower[..i].chars().last() {
                None => return false,
                Some(one) if one.is_whitespace() || one == ':' || one == '.' => {}
                Some(_) => return false,
            }
            // The body up to the reference, its trailing white space taken off:
            // the words that came before the number.
            let before = lower[..i].trim_end();
            // The words before the one that closes. A body that negates the
            // word names no pull request of the issue: `does not fix #7`,
            // `no test fixes #7` and `never fixed #7` say the pull request does
            // not close the issue, and one that does not close it is no reason
            // to refuse a finish. `half-fixed #7` is refused another way: the
            // word that closes is a word of its own, and what stands before it
            // leaves no room for one.
            if negated(before) {
                return false;
            }
            // Where the last word of the body begins: just after the last white
            // space, or at the body's own beginning.
            let head = match before.rfind(char::is_whitespace) {
                Some(k) => k + 1,
                None => 0,
            };
            // A word of closing is a word of its own: only white space, or the
            // body's own beginning, stands before it. `unfixes #7` is no word.
            if !before[..head].is_empty() && !before[..head].ends_with(char::is_whitespace) {
                return false;
            }
            // The word of closing, and the one mark a writer may leave on its end:
            // the colon that sets the number off, `close: #7`, and the period that
            // ends the body's sentence, `This fixes #7.`. Past the word and that
            // mark the body goes into nothing else — a letter, `fixesx #7`, would
            // make the word something the writer never wrote.
            WORDS.iter().any(|word| {
                let tail = &before[head..];
                if !tail.starts_with(*word) {
                    return false;
                }
                if tail.len() - word.len() > 1 {
                    return false;
                }
                match tail[word.len()..].chars().next() {
                    None => true,
                    Some(one) => one == ':' || one == '.',
                }
            })
        })
}

/// Whether the words a body puts before a closing word deny it. The word of
/// negation is read where the closing word is read: the word before it, the one
/// before that, or the end of the word before it when a writer wrote it with
/// `not` contracted onto it. Two words are read because `no test fixes #7` denies the closing
/// word with a noun of its own: the word of negation stands two back there, and
/// the noun of its own stands between. A negation further back names another
/// clause than the one that closes the issue, and leaves the reference standing
/// — `nothing here, but it fixes #7` is the issue's pull request.
pub fn negated(before: &str) -> bool {
    const NO: [&str; 3] = ["not", "no", "never"];
    before
        .rsplit(char::is_whitespace)
        .take(3)
        .any(|word| NO.contains(&word) || word.ends_with("n't"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_body_closes_an_issue_only_when_a_word_of_closing_names_it() {
        for (text, yes) in [
            ("Fixes #7.", true),
            ("Fixes #7", true),
            ("Closes #7", true),
            ("resolves #7 in one line", true),
            ("Resolved #7", true),
            ("close: #7", true),
            ("This fixes #7.", true),
            ("FIXED #7", true),
            ("Fixes #70.", false),
            ("see #7", false),
            ("Fixes #7x", false),
            ("Fixes#7", false),
            ("unfixes #7", false),
            ("some textfixes #7", false),
            ("This does not fix #7.", false),
            ("not fixed #7", false),
            ("no test fixes #7", false),
            ("never resolved #7", false),
            ("This doesn't fix #7.", false),
            ("it isn't fixed #7", false),
            // A word of nothing further back is the case's own claim and not a
            // denial of the closing word.
            ("nothing here, but it fixes #7", true),
            ("##7", false),
            ("", false),
        ] {
            assert_eq!(closes_issue(text, 7), yes, "{text:?} closes #7: {yes}");
        }
    }

    #[test]
    fn a_pull_request_is_the_issues_by_its_head_or_its_body() {
        assert!(names_issue(&json!({"headRefName": "feature/issue-7"}), 7));
        assert!(names_issue(&json!({"headRefName": "feature/issue-7-x"}), 7));
        assert!(!names_issue(&json!({"headRefName": "feature/issue-70"}), 7));
        assert!(!names_issue(&json!({"headRefName": "fix/issue-7"}), 7));
        assert!(names_issue(
            &json!({"headRefName": "other", "body": "Fixes #7"}),
            7
        ));
        assert!(!names_issue(
            &json!({"headRefName": "other", "body": "Closes #70"}),
            7
        ));
        assert!(!names_issue(&json!({"body": "does not fix #7"}), 7));
        assert!(!names_issue(&json!({"headRefName": null, "body": null}), 7));
        assert!(!names_issue(&json!({}), 7));
    }
}
