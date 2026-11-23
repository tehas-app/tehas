//! The tickets port as the adapter answers it: the digest of a ticket, the
//! factory's labels made, a project's own label, a mark put on and taken
//! off, and what it does not answer yet. Each case runs the adapter against
//! a runner that answers from fixtures and writes down every call.

use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::Mutex;
use tehas_core::forge::GhRunner;
use tehas_github::{table, GitHub};
use tehas_ports::{
    Account, AccountId, AccountKind, Act, At, Cite, Cites, CloseReason, Colour, Depth, Kind, Login,
    Mark, MarkName, MarkSpec, Outcome, Passing, Realm, Ref, Refusal, Sent, TicketKey, TicketState,
    TicketWrites, Tickets, Unprepared, Unread,
};

type Asked = Arc<Mutex<Vec<String>>>;

/// Answers each call from the list in turn; a call past it fails.
struct Stub {
    answers: Vec<Result<String, String>>,
    asked: Asked,
}

impl GhRunner for Stub {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let mut asked = self.asked.lock().unwrap();
        asked.push(args.join(" "));
        self.answers
            .get(asked.len() - 1)
            .cloned()
            .unwrap_or_else(|| Err("no fixture".to_string()))
    }
}

fn adapter(answers: Vec<Result<&str, &str>>) -> (GitHub, Asked) {
    let asked = Asked::default();
    let stub = Stub {
        answers: answers
            .into_iter()
            .map(|a| a.map(str::to_string).map_err(str::to_string))
            .collect(),
        asked: Arc::clone(&asked),
    };
    (GitHub::new("acme/widget", Box::new(stub)), asked)
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

fn at(iso: &str) -> At {
    At::new(tehas_core::time::from_iso(iso).unwrap())
}

const ISSUE: &str = r#"{"number":7,"title":"Make it so","state":"open","state_reason":null,
"labels":[{"name":"bug"},{"name":"tehas:pause"}],"type":{"name":"Bug"},
"milestone":{"number":3,"title":"v2"},"user":{"login":"Alice","id":5,"type":"User"},
"created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-03T00:00:00Z",
"html_url":"https://github.com/acme/widget/issues/7"}"#;

#[test]
fn a_tickets_digest_is_the_issues_rest_read() {
    let (mut gh, asked) = adapter(vec![Ok(ISSUE)]);
    let got = gh.several(&[key(7)]);
    assert_eq!(*asked.lock().unwrap(), ["api repos/acme/widget/issues/7"]);
    let (k, d) = &got[0];
    assert_eq!(*k, key(7));
    let d = d.as_ref().unwrap();
    assert_eq!(d.title, "Make it so");
    assert_eq!(d.state, TicketState::Open);
    assert_eq!(d.kind, Some(Kind::Bug));
    assert_eq!(
        d.marks,
        [
            Mark::Project(MarkName::new("bug").unwrap()),
            Mark::Act(Act::Pause)
        ]
    );
    assert_eq!(d.target.as_ref().unwrap().title, "v2");
    assert_eq!(
        d.author,
        Some(Account {
            realm: Realm::CodeHost,
            login: Login::new("alice"),
            id: Some(AccountId::new("5")),
            kind: AccountKind::Person,
        })
    );
    assert_eq!(d.created, at("2026-01-02T03:04:05Z"));
    assert_eq!(d.updated, at("2026-01-03T00:00:00Z"));
    assert_eq!(d.link.as_str(), "https://github.com/acme/widget/issues/7");
}

#[test]
fn a_closed_issue_says_why() {
    for (reason, want) in [
        ("null", CloseReason::Done),
        (r#""completed""#, CloseReason::Done),
        (r#""not_planned""#, CloseReason::NotPlanned),
    ] {
        let closed = ISSUE
            .replace(r#""state":"open""#, r#""state":"closed""#)
            .replace(
                r#""state_reason":null"#,
                &format!(r#""state_reason":{reason}"#),
            );
        let (mut gh, _) = adapter(vec![Ok(&closed)]);
        let d = gh.several(&[key(7)]).remove(0).1.unwrap();
        assert_eq!(d.state, TicketState::Closed(want), "{reason}");
    }
}

#[test]
fn an_answer_that_is_no_whole_issue_is_unread_and_a_failure_is_its_outcome() {
    for out in [r#"{"labels":[]}"#, "{}", "not json"] {
        let (mut gh, _) = adapter(vec![Ok(out)]);
        assert_eq!(
            gh.several(&[key(7)]).remove(0).1,
            Err(Outcome::Unread(Unread::Malformed)),
            "{out}"
        );
    }
    let (mut gh, _) = adapter(vec![Err("HTTP 502")]);
    assert_eq!(
        gh.several(&[key(7)]).remove(0).1,
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe
        })
    );
    let (mut gh, _) = adapter(vec![Err("HTTP 404")]);
    assert_eq!(gh.several(&[key(7)]).remove(0).1, Err(Outcome::NotFound));
}

#[test]
fn prepare_makes_every_label_of_the_table_and_one_that_fails_stops_none() {
    let specs = table::specs();
    let mut answers = vec![Ok(""); specs.len()];
    answers[2] = Err("HTTP 502");
    answers[5] = Err("HTTP 403");
    let (mut gh, asked) = adapter(answers);
    assert_eq!(
        gh.prepare(),
        Err(Unprepared {
            mark: Some(specs[2].name.clone()),
            why: Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe
            }
        })
    );
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), specs.len());
    assert_eq!(
        asked[0],
        "label create tehas:ready --repo acme/widget --color 0e8a16 --description Tehas: eligible for dispatch (apply by hand to authorize a cycle) --force"
    );
    for (call, spec) in asked.iter().zip(&specs) {
        assert!(
            call.starts_with(&format!(
                "label create {} --repo acme/widget --color ",
                spec.name.as_str()
            )),
            "{call}"
        );
        assert!(call.ends_with(" --force"), "{call}");
    }
    let (mut gh, _) = adapter(vec![Ok(""); specs.len()]);
    assert_eq!(gh.prepare(), Ok(()));
}

#[test]
fn ensure_mark_makes_a_projects_label_with_what_its_spec_gives() {
    let (mut gh, asked) = adapter(vec![Ok(""), Ok("")]);
    let bare = MarkSpec {
        name: MarkName::new("area-ui").unwrap(),
        colour: None,
        about: None,
    };
    assert_eq!(gh.ensure_mark(&bare), Ok(()));
    let full = MarkSpec {
        colour: Some(Colour::new("00ff00").unwrap()),
        about: Some("The UI".to_string()),
        ..bare
    };
    assert_eq!(gh.ensure_mark(&full), Ok(()));
    assert_eq!(
        *asked.lock().unwrap(),
        [
            "label create area-ui --repo acme/widget --force",
            "label create area-ui --repo acme/widget --color 00ff00 --description The UI --force"
        ]
    );
    // A name no label of tehas can have is refused with no call.
    let (mut gh, asked) = adapter(vec![]);
    let spaced = MarkSpec {
        name: MarkName::new("two words").unwrap(),
        colour: None,
        about: None,
    };
    assert_eq!(
        gh.ensure_mark(&spaced),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_mark_goes_on_and_comes_off_as_its_label() {
    let (mut gh, asked) = adapter(vec![Ok(""), Err("HTTP 404"), Err("HTTP 502")]);
    let pause = Mark::Act(Act::Pause);
    assert_eq!(gh.mark(&key(7), &pause), Ok(()));
    // Not there is the state wanted.
    assert_eq!(gh.unmark(&key(7), &pause), Ok(()));
    assert!(gh.unmark(&key(7), &pause).is_err());
    assert_eq!(
        *asked.lock().unwrap(),
        [
            "api -X POST repos/acme/widget/issues/7/labels -f labels[]=tehas:pause",
            "api -X DELETE repos/acme/widget/issues/7/labels/tehas%3Apause",
            "api -X DELETE repos/acme/widget/issues/7/labels/tehas%3Apause",
        ]
    );
    let (mut gh, asked) = adapter(vec![]);
    assert_eq!(
        gh.mark(&key(7), &Mark::Unknown),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn what_no_caller_asks_yet_is_unsupported_before_any_request() {
    let (mut gh, asked) = adapter(vec![]);
    assert_eq!(
        gh.ticket(&key(7), Depth::Cut { notes: 0 }, None),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    assert_eq!(
        gh.close(&key(7), &CloseReason::Done),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_cite_is_githubs_reference_and_a_new_ticket_link_its_page() {
    let (gh, _) = adapter(vec![]);
    assert_eq!(gh.cite(&Cite::Ticket(Ref::Here(key(7)))), "#7");
    assert_eq!(gh.cite(&Cite::Account(Login::new("alice"))), "@alice");
    assert_eq!(
        gh.new_ticket_link("A b", "c&d").as_str(),
        "https://github.com/acme/widget/issues/new?title=A%20b&body=c%26d"
    );
    let long = "x".repeat(3000);
    assert_eq!(
        gh.new_ticket_link("A", &long).as_str(),
        "https://github.com/acme/widget/issues/new?title=A"
    );
}
