//! The drafts that did reach an issue, or were offered to, read by the text
//! check of #1015: each says nothing a person's title and body do not say
//! better, so each is refused, and the person's own text beside it stands.
//!
//! The files under `tests/drafts/` are the drafts as the planner posted them,
//! cut after their context inventory: the title on the first line, a blank
//! line, then the head of the body. trail-openers/nessie#1184 is the one the
//! factory wrote over a person's title (`EPIC: EPIC`); tehas #1015, #980 and
//! #608 are the drafts offered on this repository's issues. The nessie draft
//! keeps only the lines #1015 already quotes and tehas's own brief, as the rest
//! of it is that repository's.

use serde_json::json;
use tehas_core::plan::checked;
use tehas_core::textcheck::{check_body, check_title, Reason};

/// The title and the body of a fixture.
fn read(text: &str) -> (&str, &str) {
    let (title, body) = text
        .split_once("\n\n")
        .expect("a title, a blank line, a body");
    (title, body)
}

const DRAFTED: [(&str, &str, Reason); 4] = [
    (
        "nessie#1184",
        include_str!("drafts/nessie-1184.drafted.md"),
        Reason::OnlyPrefix,
    ),
    (
        "tehas#1015",
        include_str!("drafts/tehas-1015.drafted.md"),
        Reason::OnlyPrefix,
    ),
    (
        "tehas#980",
        include_str!("drafts/tehas-980.drafted.md"),
        Reason::OnlyPrefix,
    ),
    (
        "tehas#608",
        include_str!("drafts/tehas-608.drafted.md"),
        Reason::NamesOnlyTheIssue,
    ),
];

const PERSON: [(&str, &str); 3] = [
    ("tehas#1015", include_str!("drafts/tehas-1015.person.md")),
    ("tehas#980", include_str!("drafts/tehas-980.person.md")),
    ("tehas#608", include_str!("drafts/tehas-608.person.md")),
];

#[test]
fn every_real_draft_title_is_refused_for_its_own_rule() {
    let titles: Vec<&str> = DRAFTED.iter().map(|(_, text, _)| read(text).0).collect();
    assert_eq!(
        titles,
        [
            "EPIC: EPIC",
            "EPIC: EPIC",
            "EPIC: Epic",
            "feat: Plan issue #608"
        ]
    );
    for (case, text, rule) in DRAFTED {
        let (title, body) = read(text);
        let refused = check_title(title, body).expect_err(case);
        assert_eq!(refused.reason, rule, "{case}");
    }
}

#[test]
fn every_real_draft_body_is_the_brief_echoed_back() {
    for (case, text, _) in DRAFTED {
        let refused = check_body(read(text).1).expect_err(case);
        assert_eq!(refused.reason, Reason::Briefed, "{case}");
    }
}

#[test]
fn every_real_draft_is_refused_by_the_gate_of_update_this_issue() {
    // `checked` is what both writers of a plan ask before an `update` is
    // made: a draft it refuses leaves the issue's title and body as they are.
    for (case, text, rule) in DRAFTED {
        let (title, spec) = read(text);
        let refused = checked(&json!({"title": title, "spec": spec})).expect_err(case);
        assert_eq!(refused.reason, rule, "{case}");
        // A draft with the person's title kept and the brief as its body is
        // refused all the same.
        let refused = checked(&json!({"title": "", "spec": spec})).expect_err(case);
        assert_eq!(refused.reason, Reason::Briefed, "{case}");
    }
}

#[test]
fn the_person_text_of_each_issue_stands() {
    for (case, text) in PERSON {
        let (title, body) = read(text);
        assert_eq!(check_title(title, body), Ok(()), "{case}");
        assert_eq!(check_body(body), Ok(()), "{case}");
        assert!(
            checked(&json!({"title": title, "spec": body})).is_ok(),
            "{case}"
        );
    }
    // The title a person gave nessie#1184 back by hand, which #1015 quotes.
    let restored =
        "EPIC: Slack channel turns explain their read-only tools instead of reporting an outage";
    assert_eq!(check_title(restored, ""), Ok(()));
}
