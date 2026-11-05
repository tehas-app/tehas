//! What the poll reads off an issue's own text: the hash an approval binds
//! (tehas_core::approval), the files its body names (tehas_core::size), and the
//! attachments it links or promises (console/attach).

use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::{LazyLock, OnceLock};
pub use tehas_core::approval::{body_sha256, title_sha256};
/// The info string of the block, the one name the reader of a body and the
/// writer of one both use.
pub use tehas_core::plan::SPEC_INFO;

/// The files count of the measure of a body.
pub fn size_files(body: &str, max_bytes: usize, max_lines: usize) -> usize {
    tehas_core::size::measure(body.as_bytes(), max_bytes, max_lines).files
}

/// Every attachment link in a text.
pub fn links(text: &str) -> usize {
    link_list(text).len()
}

/// The attachment links, each once, in the order the text has them.
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

/// The `pi-spec v1` block a lean start needs, asked of an issue's body. The
/// reader itself is [`tehas_core::plan::spec_block`], the one function the poll
/// and the writer of a plan both ask, so the two cannot disagree about the same
/// body; this is the poll's name for its question. tehas asks only that one such
/// block is there and holds valid JSON; the fields are pi-rukas's to judge, and
/// lean's own `ready` step still runs.
pub fn has_spec(body: &str) -> bool {
    tehas_core::plan::spec_block(body).is_some()
}

/// What the poll says on an issue it will not start lean for the want of its
/// `pi-spec v1` block: the reason, and what a person does next. One sentence
/// for the log and the comment both.
pub fn no_spec_said(ask: &str) -> String {
    format!("its text holds no `{SPEC_INFO}` block for the lean driver to read: it is not there, or it is not the one block of valid JSON lean needs. A person writes that block, and applies `{ask}` again.")
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

/// The numbers a text names with a closing keyword: `Closes`, `Fixes` or
/// `Resolves` (any case, colon or none) in front of `#N`, and the references
/// that keyword carries on: `Closes #21 and #22` names both. A number is read
/// whole, so `#210` names 210 and never 21, and a keyword the words before it
/// deny names nothing: `does not fix #7` holds no issue.
///
/// The dispatcher's own reading, and wider than the single-reference one the
/// cycle and the apply step match (`console/cycle/src/runner.rs`,
/// `console/dispatch/src/apply/start.rs`, `lib/cycles.sh`): it also reads a run
/// of references behind one keyword. Only a hold is read here, never a start,
/// so the two cannot start work twice on one pull request.
pub fn closing_numbers(body: &str) -> Vec<u64> {
    // The keyword and the reference it names: `Closes`, `Fixes` or `Resolves`
    // (any case, colon or none) in front of `#` and a number. The `\b` before
    // the digits reaches into the reference's own `#`, and the `\b` after them
    // reads the number whole: `#210` is one reference and never `#21` with a
    // `0` left after it, and `#7and` is one word that names nothing at all.
    static KEYWORD: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?):?\s+#([0-9]{1,18})\b").unwrap()
    });
    // A separator between two references of one keyword: the white space and
    // commas they are held together by, and the words of a list, nothing else.
    // `(?i)` because the keyword above is any case.
    static SEPARATORS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)^(?:[\s,]+|\s*(?:and|or)\s*)+").unwrap());
    // The next reference of a run, where the separators left the text.
    static REFERENCE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)^#([0-9]{1,18})\b").unwrap());
    let mut out: Vec<u64> = Vec::new();
    // A number named twice, by two keywords of one body, is named once.
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    for c in KEYWORD.captures_iter(body) {
        let Some(m) = c.get(1) else {
            continue;
        };
        // A keyword the words before it deny names nothing, and neither does
        // its run: `does not fix #7` says the pull request leaves 7 open.
        if c.get(0).is_some_and(|k| denied(&body[..k.start()])) {
            continue;
        }
        // A reference too long for the type names no issue, and a reference
        // GitHub cannot link names nothing: neither ends the run.
        if let Ok(n) = m.as_str().parse() {
            if seen.insert(n) {
                out.push(n);
            }
        }
        // The references the same keyword carries on: the `#22` of
        // `Closes #21 and #22`. The keyword's own number is already pushed, so
        // the run starts after the whole match: separators, then a reference at
        // once, until the text is not that shape any more.
        let mut rest = &body[m.end()..];
        while let Some(sep) = SEPARATORS.find(rest) {
            let tail = &rest[sep.end()..];
            let Some(n) = REFERENCE.captures(tail) else {
                break;
            };
            let Some(num) = n.get(1) else {
                break;
            };
            // A reference too long for the type names no issue, and ends no run.
            if let Ok(y) = num.as_str().parse() {
                if seen.insert(y) {
                    out.push(y);
                }
            }
            // A reference of 19 digits or more is no number's: it names nothing
            // and ends the run, for the text after it is no separator.
            if tail[num.end()..].starts_with(|c: char| c.is_ascii_digit())
                || num.end() >= tail.len()
            {
                break;
            }
            rest = &tail[num.end()..];
        }
    }
    out
}

/// Whether the words before a closing keyword deny it: `not`, `no` or `never`
/// one or two words back, or a word with `n't` on its end. The reading of
/// `negated` in `console/cycle/src/finish/run.rs`, which the cycle's finish
/// reads a body with: `does not fix #7`, `no test fixes #7`, `doesn't fix #7`
/// and `never fixed #7` name no issue. A negation further back is another
/// clause's: `nothing here, but it fixes #7` names 7.
fn denied(before: &str) -> bool {
    const NO: [&str; 3] = ["not", "no", "never"];
    before
        .trim_end()
        .to_ascii_lowercase()
        .rsplit(char::is_whitespace)
        .take(2)
        .any(|word| NO.contains(&word) || word.ends_with("n't"))
}

/// The first unfilled placeholder of a text.
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

/// The word a text of the answer promises when
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
    fn an_unfilled_placeholder_in_a_text_that_links_nothing_is_a_promise() {
        assert_eq!(
            promise(&json!({"body": "The screenshot is the place. SCREENSHOT GOES HERE."}))
                .as_deref(),
            Some("SCREENSHOT GOES HERE")
        );
        assert_eq!(
            promise(&json!({"title": "IMAGE GOES HERE", "body": "A plain body."})).as_deref(),
            Some("IMAGE GOES HERE")
        );
        assert_eq!(
            promise(&json!({"body": "The Mockup goes here."})).as_deref(),
            Some("Mockup goes here")
        );
    }

    #[test]
    fn a_text_that_links_an_attachment_answers_for_them_all() {
        for v in [
            json!({"body": "SCREENSHOT GOES HERE: ![s](https://github.com/user-attachments/assets/aa)"}),
            json!({"title": "SCREENSHOT GOES HERE",
                "body": "See ![s](https://github.com/user-attachments/files/9/spec.md)."}),
        ] {
            assert_eq!(promise(&v), None, "{v}");
        }
    }

    #[test]
    fn the_name_of_a_file_before_goes_here_is_a_promise() {
        for words in [
            "SCREENSHOT GOES HERE",
            "SCREENSHOT_GOES_HERE",
            "IMAGE GOES HERE",
            "image goes here",
            "the picture goes here",
            "MOCKUP GOES HERE",
            "the file goes here",
            "the video goes here",
            "shot goes here",
            "the figure goes here",
            "the diagram goes here",
            "the screencast goes here",
            "the thumbnail goes here",
            "the animation goes here",
            "the attachment goes here",
            "the gif goes here",
            "the example goes here",
        ] {
            let text = format!("See ({words}) for the layout of the header.");
            assert_eq!(
                promise(&json!({"body": text})).as_deref(),
                Some(words.strip_prefix("the ").unwrap_or(words))
            );
        }
    }

    #[test]
    fn the_same_words_of_something_that_is_no_file_are_prose() {
        for words in [
            "the value goes here in the code",
            "goes here",
            "WORDS GOES HERE",
            "the plan goes here, then the build",
            "the name goes here",
            "the title goes here",
            "a sentence goes here",
            "nothing goes here at all",
        ] {
            let text = format!("In the form, {words}, and the rest is clear.");
            assert_eq!(promise(&json!({"body": text})), None, "{words}");
        }
        assert_eq!(
            promise(&json!({"body": "Plain words about the feature, with no gap in them."})),
            None
        );
        assert_eq!(
            promise(&json!({"body": "The feature works as the manual says."})),
            None
        );
    }

    #[test]
    fn the_promise_of_a_body_at_any_depth_of_the_answer() {
        // Spelled from parts: this file is read by no rule, but the fixture of
        // the shell test it replaces was, and the words stay as they were.
        let body = format!("The body reads IMAGE{}GOES{}HERE.", '_', '_');
        let v = json!({"data": {"repository": {"issue": {"lastEditedAt": null, "editor": null, "body": body}}}});
        assert_eq!(promise(&v).as_deref(), Some("IMAGE_GOES_HERE"));
    }

    #[test]
    fn a_promise_with_a_line_end_or_of_more_than_80_characters_is_no_answer() {
        assert_eq!(promise(&json!({"body": "SCREENSHOT GOES\nHERE"})), None);
        let long = format!("{} GOES HERE", "A".repeat(90));
        assert_eq!(promise(&json!({"body": long})), None);
    }

    #[test]
    fn an_answer_with_no_text_that_promises_promises_nothing() {
        for v in [
            json!({"body": 42}),
            json!({}),
            json!([]),
            Value::Null,
            json!(42),
            json!({"body": null}),
            json!(["a screenshot goes", 42]),
            json!({"number": 7, "state": "open"}),
        ] {
            assert_eq!(promise(&v), None, "{v}");
        }
    }

    #[test]
    fn a_body_names_every_issue_its_closing_keywords_name() {
        assert_eq!(closing_numbers("Closes #9"), [9]);
        assert_eq!(closing_numbers("Closes #9 and #8, fixes #7"), [9, 8, 7]);
        assert_eq!(closing_numbers("Resolves: #91"), [91]);
        // A longer number is no shorter one's, and a bare reference names
        // nothing.
        assert_eq!(closing_numbers("Closes #90"), [90]);
        assert!(closing_numbers("See #9").is_empty());
        assert!(closing_numbers("").is_empty());
    }

    #[test]
    fn a_keyword_the_words_before_it_deny_names_nothing() {
        // The negatives the cycle's finish reads (console/cycle's `negated`),
        // so a body the finish does not call the issue's holds nothing either.
        for body in [
            "This does not fix #7",
            "No test fixes #7",
            "It never fixed #7",
            "This doesn't fix #7",
            "Does NOT close #7 and #8",
        ] {
            assert!(closing_numbers(body).is_empty(), "{body}");
        }
        // A negation further back is another clause's.
        assert_eq!(closing_numbers("Nothing here, but it fixes #7"), [7]);
        // One denied keyword leaves the next one standing.
        assert_eq!(closing_numbers("Does not fix #7. Closes #8"), [8]);
    }

    #[test]
    fn a_run_of_references_behind_one_keyword_names_them_all() {
        assert_eq!(closing_numbers("Closes #7, #8, #9"), [7, 8, 9]);
        assert_eq!(
            closing_numbers("Closes #7 and #8 and #9 and #10"),
            [7, 8, 9, 10]
        );
        // `Closes #7, #8 and then #9`: the word ends the run, but a reference
        // already read is read, so the last one is not the keyword's.
        assert_eq!(closing_numbers("Closes #7, #8 and then #9"), [7, 8]);
        // `Closes #7and #8`: `#7and` is one word, so the keyword names no
        // number of its own and the run holds nothing: 7 is named by nothing.
        assert!(closing_numbers("Closes #7and #8").is_empty());
        // A reference too long for the type names no issue and ends nothing:
        // the rest of the run is still read. A reference of 19 digits or more
        // is no number's at all: the keyword that leads it names nothing, and
        // what follows it is no run of that keyword's.
        assert_eq!(
            closing_numbers("Closes #21 and #99999999999999999999"),
            [21]
        );
        assert!(
            closing_numbers("Closes #99999999999999999999 and #21").is_empty(),
            "a keyword whose own number is too long names nothing"
        );
        // Non-ASCII digits are no reference at all: u64 refuses them.
        assert!(closing_numbers("Closes #٧ and #8").is_empty());
    }

    #[test]
    fn a_lean_start_needs_the_one_spec_block_of_valid_json() {
        assert!(has_spec("```pi-spec v1\n{\"intent\":\"x\"}\n```"));
        assert!(has_spec(
            "```pi-spec v1\n---\n{\"intent\":\"x\"}\n```\n\nPart."
        ));
        // None, two, and one that is no JSON all refuse the start.
        assert!(!has_spec("A plain body."));
        assert!(!has_spec(""));
        assert!(!has_spec("```pi-spec v1\nnot json\n```"));
        assert!(!has_spec(
            "```pi-spec v1\n{\"a\":1}\n```\n\n```pi-spec v1\n{\"b\":2}\n```"
        ));
        // A block of another name is no block, and the comment a person is
        // given names the ask they apply again.
        assert!(!has_spec("```json\n{\"a\":1}\n```"));
        assert!(no_spec_said("tehas:ready").contains("`tehas:ready`"));
        assert!(no_spec_said("tehas:ready").contains(SPEC_INFO));
    }

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
