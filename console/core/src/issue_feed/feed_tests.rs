//! The feed: who may not be on the allowed list, which label is the pinned
//! one, and how the texts are read. Each case was a case of the Protolab's
//! shell test, which read the feed through its pass.

use super::*;
use std::collections::BTreeMap;

const LABEL: &str = crate::kinds::Kind::Improvement.label();
const STAGE: &str = "the marked stage";

#[test]
fn an_allowed_list_that_names_the_bot_is_refused() {
    let line = "ERROR: the bot account tehas-bot[bot] is in TEHAS_ALLOWED_LABELERS; the bot never authorises work";
    assert_eq!(
        bot_refusal("tehas-bot[bot]", "alice tehas-bot").as_deref(),
        Some(line)
    );
    assert_eq!(
        bot_refusal("tehas-bot[bot]", "alice tehas-bot[bot]").as_deref(),
        Some(line)
    );
    assert_eq!(bot_refusal("tehas-bot[bot]", "alice"), None);
}

/// A runner that answers every call with one answer and keeps what it was
/// asked.
struct Answers {
    answer: Result<String, String>,
    asked: Vec<Vec<String>>,
}

impl GhRunner for Answers {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked
            .push(args.iter().map(|a| (*a).to_string()).collect());
        self.answer.clone()
    }
}

fn answers(answer: Result<&str, &str>) -> Answers {
    Answers {
        answer: answer.map(str::to_string).map_err(str::to_string),
        asked: Vec::new(),
    }
}

#[test]
fn only_the_pinned_label_read_from_github_puts_an_issue_at_the_stage() {
    let home = crate::testdir::TestDir::new("feed-pins");
    std::fs::create_dir_all(home.join(crate::label_pins::FILE).parent().unwrap()).unwrap();
    let pins = BTreeMap::from([(LABEL.to_string(), "LA_proto".to_string())]);
    crate::label_pins::save(&home, &pins).unwrap();
    let pinned = format!(r#"{{"name":"{LABEL}","node_id":"LA_proto"}}"#);
    let mut gh = answers(Ok(&pinned));
    assert_eq!(
        pin_refusal(&mut gh, &home, "acme/widget", LABEL, STAGE),
        Ok(None)
    );
    assert_eq!(
        gh.asked,
        [[
            "api".to_string(),
            ["repos/acme/widget/labels/", LABEL].concat()
        ]]
    );

    let other = format!(r#"{{"name":"{LABEL}","node_id":"LA_bug"}}"#);
    assert_eq!(
        pin_refusal(&mut answers(Ok(&other)), &home, "acme/widget", LABEL, STAGE),
        Ok(Some(format!("refused: the label {LABEL} is not the one pinned in state/labels (pinned LA_proto, now LA_bug): no ticket is at {STAGE}")))
    );
    assert_eq!(
        pin_refusal(
            &mut answers(Err("no answer")),
            &home,
            "acme/widget",
            LABEL,
            STAGE
        ),
        Err(format!(
            "the label {LABEL} could not be read, nothing built or removed"
        ))
    );

    let bare = crate::testdir::TestDir::new("feed-no-pins");
    let refused = pin_refusal(
        &mut answers(Ok(&pinned)),
        &bare,
        "acme/widget",
        LABEL,
        STAGE,
    );
    assert!(
        refused.clone().unwrap().unwrap().starts_with(&format!(
            "refused: the label {LABEL} (LA_proto) has no pin in state/labels"
        )),
        "{refused:?}"
    );
}

#[test]
fn the_texts_are_the_issue_of_the_answer_or_none() {
    let answer = r#"{"data":{"repository":{"issue":{"body":"hi","author":{"login":"alice"},"editor":null,"comments":{"nodes":[]}}}}}"#;
    let mut gh = answers(Ok(answer));
    let issue = texts("acme/widget", &mut gh, 7).unwrap();
    assert_eq!(issue["body"], "hi");
    assert_eq!(issue["author"]["login"], "alice");
    let asked = &gh.asked[0];
    for word in ["owner=acme", "name=widget", "number=7"] {
        assert!(asked.iter().any(|a| a == word), "{asked:?}");
    }
    let query = asked.iter().find(|a| a.starts_with("query=")).unwrap();
    assert!(query.contains("comments(first: 100)"), "{query}");
    assert!(query.contains(WHO), "{query}");

    for bad in [
        r#"{"data":{"repository":{"issue":null}}}"#,
        r#"{"message":"Bad credentials"}"#,
        "not json",
    ] {
        assert!(
            texts("acme/widget", &mut answers(Ok(bad)), 7).is_none(),
            "{bad}"
        );
    }
    assert!(texts("acme/widget", &mut answers(Err("no answer")), 7).is_none());
    // A repository with no owner asks nothing.
    let mut gh = answers(Ok(answer));
    assert!(texts("widget", &mut gh, 7).is_none());
    assert!(gh.asked.is_empty());
}
