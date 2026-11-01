//! One text check for what tehas writes to an issue.
//!
//! A planned title has been published as `EPIC: EPIC` and a body has been
//! published as the planner's brief echoed back: `Descriptor:` with a
//! `Prior context inventory` heading and nothing of the issue in it. Neither
//! says what the issue is. This module says no, in words, before the text
//! reaches GitHub: [`check_title`] for the title and [`check_body`] for the
//! body, each returning the reason as text.
//!
//! The rules read the text the way a person does: split on whitespace and
//! compare without case or the punctuation around a word. No regex, and
//! nothing here is behind a feature.
//!
//! The check has no caller yet. It is the shared rule, landed ahead of the
//! writers, so each writer that takes it on in a later part of #1015 calls
//! these same two functions.

/// The words a title may open with and still be a title: the classifications
/// `host/docs/operating.md` says a plan's title carries, the `research:` the
/// planner adds, and `epic:` for an epic. A title of nothing but one of these
/// is a label, not a summary. Case does not matter, and the colon may be
/// there or not: `epic` is the word `epic:`.
const LABELS: [&str; 16] = [
    "epic",
    "epic:",
    "feat",
    "feat:",
    "fix",
    "fix:",
    "chore",
    "chore:",
    "docs",
    "docs:",
    "test",
    "test:",
    "refactor",
    "refactor:",
    "research",
    "research:",
];

/// The whole of a body that is the planner's brief and nothing else. Both
/// words must appear: `Descriptor:` on its own is a field a body may well
/// hold, and so is a section that invents prior context. Together they are
/// the brief, which is what #898 published as its issue.
const DESCRIPTOR: &str = "descriptor:";
const INVENTORY: &str = "prior context inventory";

/// The marker a brief leaves where its answer should be, on a line of its
/// own. A body of nothing but these and blanks is a form nobody filled in.
const PARAM: &str = "[context param]";

/// The title's words as a person reads them: lower-cased and with the
/// punctuation at either end of a word taken off, so `EPIC:` and `epic` are
/// one word, and `Fast` and `fast?` are one word.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.to_lowercase()
                .trim_matches(|c: char| !(c.is_alphanumeric() || c == '_'))
                .to_owned()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// A title that says what the issue is, or the reason it does not.
///
/// `body` is the body the title is published with. No rule here reads it: the
/// title repeating the body, which the epic's word-overlap rule would catch,
/// is a later part of #1015. The parameter stays so that rule is added inside
/// this signature rather than by changing every call.
pub fn check_title(title: &str, _body: &str) -> Result<(), String> {
    let words = words(title);

    if words.is_empty() {
        return Err("the title is empty".into());
    }

    if words.iter().all(|w| LABELS.contains(&w.as_str())) {
        return Err(format!(
            "the title is nothing but its prefix: {}",
            title.trim()
        ));
    }

    // A word of its own written twice. `EPIC: EPIC` is the prefix above, which
    // is the truer reason and is asked first.
    if words.len() > 1 && words.iter().all(|w| w == &words[0]) {
        return Err(format!("the title is one word repeated: {}", title.trim()));
    }

    Ok(())
}

/// A body that holds the issue, or the reason it does not.
pub fn check_body(body: &str) -> Result<(), String> {
    let low = body.to_lowercase();
    if low.contains(DESCRIPTOR) && low.contains(INVENTORY) {
        return Err(
            "the body is the planner's brief: it holds both `Descriptor:` and a `Prior context inventory`"
                .into(),
        );
    }

    let left: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != PARAM)
        .collect();
    if left.is_empty() {
        return Err("the body is nothing but `[context param]` markers".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(title: &str) -> String {
        check_title(title, "").expect_err("expected a refusal")
    }

    fn refused_body(body: &str) -> String {
        check_body(body).expect_err("expected a refusal")
    }

    #[test]
    fn a_title_of_only_its_prefix_is_refused() {
        assert_eq!(
            refused("EPIC: EPIC"),
            "the title is nothing but its prefix: EPIC: EPIC"
        );
        for only in [
            "epic:",
            "EPIC:",
            "Epic",
            "feat:",
            "Fix:",
            "chore: chore:",
            "docs: docs: docs:",
        ] {
            let why = refused(only);
            assert!(
                why.starts_with("the title is nothing but its prefix"),
                "{only:?} -> {why}"
            );
        }
        // A summary after the prefix makes it a title, and the list is the
        // list and no more: `milestone:` is no label of tehas's, so a title
        // of nothing but it is caught as one word repeated, not as a prefix.
        assert!(check_title("EPIC: Issue titles must summarise the issue", "").is_ok());
        assert!(check_title("feat(work): the console reads the new key", "").is_ok());
        assert!(refused("milestone: milestone").starts_with("the title is one word repeated"));
        assert!(check_title("milestone: the console reads it", "").is_ok());
    }

    #[test]
    fn a_title_of_one_word_repeated_is_refused() {
        assert_eq!(
            refused("Fast fast"),
            "the title is one word repeated: Fast fast"
        );
        for again in ["work work", "Work  WORK", "ready? ready?", "ab ab ab"] {
            let why = refused(again);
            assert!(
                why.starts_with("the title is one word repeated"),
                "{again:?} -> {why}"
            );
        }
        assert!(check_title("Fast work", "").is_ok());
    }

    #[test]
    fn a_plain_short_title_passes() {
        assert!(check_title("Fast", "").is_ok());
        assert!(check_title("The console reads the new key", "").is_ok());
        // An empty title says nothing, as a prefix alone does.
        assert!(check_title("", "").is_err());
        assert!(check_title("   ", "").is_err());
    }

    #[test]
    fn the_echoed_brief_is_refused() {
        let brief = "Descriptor: what the issue is\n\n## Prior context inventory\n\n- the file";
        assert!(refused_body(brief).contains("the planner's brief"));
        // Neither mark alone is the brief: a body may hold either in good
        // faith, and order and case do not matter.
        assert!(refused_body("Prior Context Inventory\nDescriptor: x").contains("planner's brief"));
        assert!(check_body("Descriptor: what we ship\n\nThe console reads it.").is_ok());
        assert!(check_body("## Prior context inventory\n\nThe gate is green.").is_ok());
    }

    #[test]
    fn a_body_of_only_param_markers_is_refused() {
        assert_eq!(
            refused_body("[context param]\n\n  [context param]  \n"),
            "the body is nothing but `[context param]` markers"
        );
        assert!(refused_body("").contains("nothing but"));
        // One line of the issue's own text makes it a body.
        assert!(check_body("[context param]\n\nThe console reads the new key.").is_ok());
    }
}
