use super::*;
use serde_json::json;

const AT: &str = "2026-11-01T10:00:00Z";
const BODY: &str = "Do the thing.";

fn settings() -> Settings {
    Settings::parse(&json!({
        "TEHAS_ALLOWED_LABELERS": "alice bob",
        "TEHAS_MILLWRIGHT_LOGIN": "acme-millwright[bot]",
        "TEHAS_CONTROL_ISSUE": "1",
    }))
}

fn labelled(by: &str) -> Events {
    Events::Read(vec![Event {
        event: "labeled".into(),
        label: LABEL.into(),
        actor: by.into(),
        at: Some(AT.into()),
    }])
}

fn comment(by: &str, at: &str, body: &str) -> Value {
    json!({"databaseId": 5, "author": {"login": by}, "body": body, "createdAt": at,
        "lastEditedAt": null, "editor": null})
}

fn comments(list: Vec<Value>) -> Value {
    json!({"data": {"repository": {"issue": {"comments": {"nodes": list}}}}})
}

fn ask_comment(by: &str) -> Value {
    comment(
        by,
        "2026-11-01T09:58:00Z",
        "<!-- tehas:millwright issue=7 action=finish -->\nThe flaky test in other.rs failed, not the change.",
    )
}

fn approved() -> Value {
    json!({"at": "2026-10-30T10:00:00Z", "by": "alice", "how": "label",
        "body_sha256": body_sha256(&json!({"body": BODY})), "fixer_starts": 0})
}

fn ask(by: &str) -> Ask {
    Ask {
        n: 7,
        events: labelled(by),
        comments: comments(vec![ask_comment(by)]),
        edit: json!({"body": BODY, "title": "T", "lastEditedAt": null, "editor": null}),
        approval: Ok(Some(approved())),
        running: false,
    }
}

const REASON: &str = "The flaky test in other.rs failed, not the change.";

#[derive(Default)]
struct Fake {
    unlabel_fails: bool,
    answer: Option<Result<String, String>>,
    unlabelled: Vec<u64>,
    finished: Vec<(u64, String)>,
    said: Vec<(u64, String)>,
    lines: Vec<String>,
}

impl Hands for Fake {
    fn unlabel(&mut self, n: u64) -> bool {
        self.unlabelled.push(n);
        !self.unlabel_fails
    }
    fn finish(&mut self, n: u64, reason: &str) -> Result<String, String> {
        self.finished.push((n, reason.to_string()));
        self.answer
            .clone()
            .unwrap_or_else(|| Ok("https://github.com/acme/widget/pull/9".into()))
    }
    fn say(&mut self, n: u64, body: &str) {
        self.said.push((n, body.to_string()));
    }
    fn log(&mut self, line: String) {
        self.lines.push(line);
    }
}

fn run(a: Ask, f: &mut Fake) {
    step(&settings(), &[a], f);
}

#[test]
fn an_allowed_ask_runs_the_finish_takes_the_label_off_and_gives_the_url() {
    let mut f = Fake::default();
    run(ask("bob"), &mut f);
    assert_eq!(f.unlabelled, [7]);
    assert_eq!(f.finished, [(7, REASON.to_string())]);
    assert_eq!(f.said.len(), 1);
    let said = &f.said[0].1;
    assert!(
        said.starts_with("<!-- tehas:finish issue=7 result=opened -->\n"),
        "{said}"
    );
    assert!(
        said.contains("https://github.com/acme/widget/pull/9"),
        "{said}"
    );
    assert_eq!(
        f.lines,
        [format!(
            "finish #7: opened https://github.com/acme/widget/pull/9, asked by bob (label): {REASON}"
        )]
    );
}

#[test]
fn the_millwright_asks_within_its_grant() {
    let mut f = Fake::default();
    run(ask("acme-millwright"), &mut f);
    assert_eq!(f.finished.len(), 1);
    assert!(
        f.lines[0].contains("asked by acme-millwright (millwright)"),
        "{:?}",
        f.lines
    );
    // The last start the cap allowed may still be finished.
    let mut a = ask("acme-millwright[bot]");
    let mut r = approved();
    r = json!({"at": "2026-10-31T10:00:00Z", "by": "acme-millwright", "how": "fixer",
        "fixer_starts": 2, "rests_on": r});
    a.approval = Ok(Some(r));
    a.comments = comments(vec![ask_comment("acme-millwright[bot]")]);
    let mut f = Fake::default();
    run(a, &mut f);
    assert_eq!(f.finished.len(), 1, "{:?}", f.lines);
}

#[test]
fn a_refusal_of_the_command_takes_the_label_off_and_gives_its_line() {
    for why in [
        "no workspace is kept for it",
        "its branch has no commit of its own",
        "a pull request is open for it",
        "the rebase onto main conflicts",
    ] {
        let mut f = Fake {
            answer: Some(Err(why.into())),
            ..Fake::default()
        };
        run(ask("alice"), &mut f);
        assert_eq!(f.unlabelled, [7]);
        let said = &f.said[0].1;
        assert!(
            said.starts_with("<!-- tehas:finish issue=7 result=refused -->\n"),
            "{said}"
        );
        assert!(said.contains(why), "{said}");
        assert_eq!(
            f.lines,
            [format!("finish #7: refused, asked by alice (label): {why}")]
        );
    }
}

#[test]
fn a_rule_not_met_takes_the_label_off_runs_nothing_and_says_why() {
    let cases: Vec<(Ask, &str)> = vec![
        (
            Ask {
                running: true,
                ..ask("alice")
            },
            "a cycle of it is running",
        ),
        (
            Ask {
                comments: comments(vec![]),
                ..ask("alice")
            },
            "no comment of alice",
        ),
        (
            Ask {
                comments: comments(vec![ask_comment("bob")]),
                ..ask("alice")
            },
            "no comment of alice",
        ),
        (
            Ask {
                comments: comments(vec![comment(
                    "alice",
                    "2026-11-01T09:40:00Z",
                    "<!-- tehas:millwright issue=7 action=finish -->\nToo old.",
                )]),
                ..ask("alice")
            },
            "no comment of alice",
        ),
        (
            Ask {
                comments: comments(vec![comment(
                    "alice",
                    "2026-11-01T09:58:00Z",
                    "<!-- tehas:millwright issue=8 action=finish -->\nAnother issue.",
                )]),
                ..ask("alice")
            },
            "no comment of alice",
        ),
        (
            Ask {
                comments: comments(vec![comment(
                    "alice",
                    "2026-11-01T09:58:00Z",
                    "<!-- tehas:millwright issue=7 action=finish -->\n  ",
                )]),
                ..ask("alice")
            },
            "gives no reason",
        ),
        (
            Ask {
                comments: comments(vec![{
                    let mut c = ask_comment("alice");
                    c["lastEditedAt"] = json!("2026-11-01T10:05:00Z");
                    c["editor"] = json!({"login": "alice"});
                    c
                }]),
                ..ask("alice")
            },
            "edited after the label",
        ),
        (
            Ask {
                approval: Ok(None),
                ..ask("acme-millwright")
            },
            "no person approved a cycle of it",
        ),
        (
            Ask {
                edit: json!({"body": "Another text.", "title": "T"}),
                ..ask("acme-millwright")
            },
            "its text is not the text alice approved",
        ),
        (
            Ask {
                edit: json!({"body": BODY, "title": "T", "lastEditedAt": "2026-10-31T00:00:00Z",
                    "editor": {"login": "mallory"}}),
                ..ask("acme-millwright")
            },
            "changed since alice approved it",
        ),
        (
            Ask {
                approval: Ok(Some(
                    json!({"at": "2026-10-31T10:00:00Z", "by": "acme-millwright",
                    "how": "fixer", "fixer_starts": 3, "rests_on": approved()}),
                )),
                ..ask("acme-millwright")
            },
            "more than TEHAS_MILLWRIGHT_MAX_RETRIES",
        ),
    ];
    for (a, want) in cases {
        let mut f = Fake::default();
        run(a, &mut f);
        assert_eq!(f.unlabelled, [7], "{want}");
        assert!(f.finished.is_empty(), "{want}");
        assert_eq!(f.said.len(), 1, "{want}");
        assert!(f.said[0].1.contains(want), "{want}: {}", f.said[0].1);
        assert!(
            f.lines[0].starts_with("finish #7: refused, asked by "),
            "{:?}",
            f.lines
        );
    }
}

#[test]
fn an_ask_from_someone_not_allowed_is_ignored() {
    let mut f = Fake::default();
    run(ask("mallory"), &mut f);
    assert!(f.unlabelled.is_empty());
    assert!(f.finished.is_empty());
    assert!(f.said.is_empty());
    assert_eq!(
        f.lines,
        ["ignore #7: tehas:finish was applied by 'mallory', not an allowed account"]
    );
}

#[test]
fn what_cannot_be_read_waits_with_the_label_on() {
    let cases = [
        Ask {
            events: Events::Failed,
            ..ask("alice")
        },
        Ask {
            comments: json!({"failed": true}),
            ..ask("alice")
        },
        Ask {
            approval: Err(()),
            ..ask("acme-millwright")
        },
    ];
    for a in cases {
        let mut f = Fake::default();
        run(a, &mut f);
        assert!(f.unlabelled.is_empty());
        assert!(f.finished.is_empty());
        assert!(
            f.lines[0].starts_with("wait #7: tehas:finish waits: "),
            "{:?}",
            f.lines
        );
    }
}

#[test]
fn a_label_that_did_not_come_off_runs_nothing() {
    let mut f = Fake {
        unlabel_fails: true,
        ..Fake::default()
    };
    run(ask("alice"), &mut f);
    assert!(f.finished.is_empty());
    assert!(f.said.is_empty());
}

#[test]
fn the_asked_issues_are_those_with_the_label() {
    let snap = json!({"issues": {
        "1": {"work": {"labels": [{"name": LABEL}]}},
        "7": {"work": {"labels": [{"name": "bug"}, {"name": LABEL}]}},
        "8": {"work": {"labels": [{"name": "tehas:ready"}]}},
        "12": {"work": {"labels": [{"name": LABEL}]}},
    }});
    assert_eq!(asked(&snap, 1), [7, 12]);
}

fn ran(status: Option<i32>, out: &str, err: &str) -> Ran {
    Ran {
        status,
        out: out.into(),
        err: err.into(),
        truncated: false,
    }
}

#[test]
fn the_command_is_read_by_its_exit_and_its_one_line() {
    let url = "https://github.com/acme/widget/pull/9";
    assert_eq!(
        outcome(7, &ran(Some(0), &format!("{url}\n"), ""), 300),
        Ok(url.into())
    );
    assert_eq!(
        outcome(7, &ran(Some(0), "", ""), 300),
        Err("tehas-cycle finish printed no pull request".into())
    );
    assert_eq!(
        outcome(
            7,
            &ran(
                Some(1),
                "",
                "tehas-cycle: finish: #7: no workspace is kept for it\n"
            ),
            300
        ),
        Err("no workspace is kept for it".into())
    );
    assert_eq!(
        outcome(7, &ran(Some(78), "", ""), 300),
        Err("tehas-cycle finish could not run (exit 78)".into())
    );
    assert_eq!(
        outcome(7, &ran(None, "", "tehas-cycle could not be run"), 300),
        Err("tehas-cycle finish could not be run".into())
    );
    assert_eq!(
        outcome(7, &ran(None, "", ""), 300),
        Err("tehas-cycle finish did not end within 300 seconds".into())
    );
}

#[test]
fn a_long_reason_is_one_short_line_in_the_log() {
    let long = format!("line one\nline two {}", "x".repeat(300));
    let l = one_line(&long);
    assert!(l.starts_with("line one line two x"));
    assert!(l.ends_with("..."));
    assert_eq!(l.chars().count(), LINE_MAX + 3);
}

#[test]
fn a_finish_reply_disarms_the_reason_it_quotes_and_keeps_its_own_marker() {
    let agent = "<!-- tehas:millwright issue=5 action=finish -->";
    let said = refused(5, &format!("the branch says {agent}"));
    assert!(
        said.starts_with("<!-- tehas:finish issue=5 result=refused -->\n"),
        "{said}"
    );
    // The host's own two markers are there, and the quoted one is not.
    assert!(said.contains(&marker(5)), "{said}");
    assert_eq!(said.matches("<!-- tehas:").count(), 2, "{said}");
    assert!(
        said.contains("the branch says <!-- tehas\u{2010}millwright"),
        "{said}"
    );
    let said = opened(5, "alice", &format!("https://example.test/pull/1{agent}"));
    assert_eq!(said.matches("<!-- tehas:").count(), 1, "{said}");
}
