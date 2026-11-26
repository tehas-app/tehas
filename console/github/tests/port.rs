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
    Account, AccountId, AccountKind, Act, At, Cite, Cites, CloseReason, Colour, Kind, Login, Mark,
    MarkName, MarkSpec, Outcome, Passing, Pool, Realm, Ref, Refusal, Sent, TicketKey, TicketState,
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
    adapter_in("acme/widget", answers)
}

fn adapter_in(repo: &str, answers: Vec<Result<&str, &str>>) -> (GitHub, Asked) {
    let asked = Asked::default();
    let stub = Stub {
        answers: answers
            .into_iter()
            .map(|a| a.map(str::to_string).map_err(str::to_string))
            .collect(),
        asked: Arc::clone(&asked),
    };
    (GitHub::new(repo, Box::new(stub)), asked)
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

/// The answer of the question of the ticket #7 is a part of: #3.
const PARENT: &str = r#"{"data":{"repository":{"databaseId":7001,"n7":{"number":7,"parent":{"number":3,"repository":{"databaseId":7001,"nameWithOwner":"acme/widget"}}}}}}"#;

/// The same, of a ticket that is a part of none.
const NO_PARENT: &str =
    r#"{"data":{"repository":{"databaseId":7001,"n7":{"number":7,"parent":null}}}}"#;

#[test]
fn a_tickets_digest_is_the_issues_rest_read_and_its_parent() {
    let (mut gh, asked) = adapter(vec![Ok(ISSUE), Ok(PARENT)]);
    let got = gh.several(&[key(7)]);
    let asked = asked.lock().unwrap();
    assert_eq!(asked[0], "api repos/acme/widget/issues/7");
    assert!(
        asked[1].contains(" n7: issue(number: 7) { number parent "),
        "{}",
        asked[1]
    );
    assert!(
        asked[1].contains("parent { number repository { databaseId nameWithOwner } }"),
        "{}",
        asked[1]
    );
    assert_eq!(asked.len(), 2);
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
        Some({
            let mut a = Account::new(Realm::CodeHost, Login::new("alice"), AccountKind::Person);
            a.id = Some(AccountId::new("5"));
            a
        })
    );
    assert_eq!(d.created, at("2026-01-02T03:04:05Z"));
    assert_eq!(d.updated, at("2026-01-03T00:00:00Z"));
    assert_eq!(d.link.as_str(), "https://github.com/acme/widget/issues/7");
    assert_eq!(d.parent, Some(Ref::Here(key(3))));
}

#[test]
fn a_parent_is_named_where_it_is_and_one_that_cannot_be_read_fails_the_digest() {
    let (mut gh, _) = adapter(vec![Ok(ISSUE), Ok(NO_PARENT)]);
    assert_eq!(gh.several(&[key(7)]).remove(0).1.unwrap().parent, None);
    let elsewhere = PARENT.replace(
        r#""databaseId":7001,"nameWithOwner":"acme/widget""#,
        r#""databaseId":7100,"nameWithOwner":"other/lib""#,
    );
    let (mut gh, _) = adapter(vec![Ok(ISSUE), Ok(&elsewhere)]);
    assert_eq!(
        gh.several(&[key(7)]).remove(0).1.unwrap().parent,
        Some(Ref::Elsewhere {
            project: tehas_ports::ProjectName::new("other/lib"),
            item: tehas_ports::ItemName::new("3"),
        })
    );
    // An answer that leaves the parent out does not say there is none.
    let left_out = r#"{"data":{"repository":{"databaseId":7001,"n7":{"number":7}}}}"#;
    let (mut gh, _) = adapter(vec![Ok(ISSUE), Ok(left_out)]);
    assert_eq!(
        gh.several(&[key(7)]).remove(0).1,
        Err(Outcome::Unread(Unread::Malformed))
    );
    let (mut gh, _) = adapter(vec![Ok(ISSUE), Err("HTTP 502")]);
    assert_eq!(
        gh.several(&[key(7)]).remove(0).1,
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe
        })
    );
}

/// The REST read of issue #n, as `ISSUE` is of #7.
fn rest(n: u64) -> String {
    ISSUE
        .replace(r#""number":7"#, &format!(r#""number":{n}"#))
        .replace("issues/7", &format!("issues/{n}"))
}

/// The parents of more than a hundred tickets: a question a hundred
/// distinct numbers, none for a ticket whose read failed, a repeated one
/// asked once.
#[test]
fn the_parents_of_many_tickets_are_asked_a_hundred_at_a_time() {
    let mut keys: Vec<TicketKey> = (1..=150).map(key).collect();
    keys.push(key(5));
    let parents = |numbers: &[u64]| {
        let nodes: Vec<String> = numbers
            .iter()
            .map(|n| {
                let parent = match n {
                    150 => "null".to_string(),
                    _ => format!(
                        r#"{{"number":{},"repository":{{"databaseId":7001,"nameWithOwner":"acme/widget"}}}}"#,
                        n + 1000
                    ),
                };
                format!(r#""n{n}":{{"number":{n},"parent":{parent}}}"#)
            })
            .collect();
        format!(
            r#"{{"data":{{"repository":{{"databaseId":7001,{}}}}}}}"#,
            nodes.join(",")
        )
    };
    let asked_numbers: Vec<u64> = (1..=150).filter(|n| *n != 9).collect();
    let mut answers: Vec<Result<String, String>> = keys
        .iter()
        .map(|k| match k {
            TicketKey::Number(n) if n.get() == 9 => Err("HTTP 404".to_string()),
            TicketKey::Number(n) => Ok(rest(n.get())),
            _ => unreachable!(),
        })
        .collect();
    answers.push(Ok(parents(&asked_numbers[..100])));
    answers.push(Ok(parents(&asked_numbers[100..])));
    let (mut gh, asked) = adapter(
        answers
            .iter()
            .map(|a| a.as_deref().map_err(String::as_str))
            .collect(),
    );
    let got = gh.several(&keys);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), keys.len() + 2);
    let questions = &asked[keys.len()..];
    assert_eq!(questions[0].matches(": issue(number: ").count(), 100);
    assert_eq!(questions[1].matches(": issue(number: ").count(), 49);
    assert!(questions[1].contains(" n150: issue(number: 150) "));
    assert!(!questions.iter().any(|q| q.contains(" n9: ")));
    assert_eq!(got.len(), keys.len());
    for ((k, d), want) in got.iter().zip(&keys) {
        assert_eq!(k, want);
        let TicketKey::Number(n) = k else {
            unreachable!()
        };
        match n.get() {
            9 => assert_eq!(*d, Err(Outcome::NotFound)),
            150 => assert_eq!(d.as_ref().unwrap().parent, None),
            n => assert_eq!(
                d.as_ref().unwrap().parent,
                Some(Ref::Here(key(n + 1000))),
                "#{n}"
            ),
        }
    }
}

/// A parents' answer that leaves an issue out, or says GitHub no longer
/// knows it, fails that ticket alone; the others keep their parents.
#[test]
fn a_partial_parents_answer_fails_only_the_tickets_it_leaves_unanswered() {
    let partial = r#"{"data":{"repository":{"databaseId":7001,
"n7":{"number":7,"parent":{"number":3,"repository":{"databaseId":7001,"nameWithOwner":"acme/widget"}}},
"n8":null}},
"errors":[{"type":"NOT_FOUND","path":["repository","n8"],"message":"Could not resolve to an Issue with the number of 8."}]}"#;
    let (r7, r8, r9) = (rest(7), rest(8), rest(9));
    let (mut gh, asked) = adapter(vec![Ok(&r7), Ok(&r8), Ok(&r9), Ok(partial)]);
    let got = gh.several(&[key(7), key(8), key(9)]);
    assert_eq!(asked.lock().unwrap().len(), 4);
    assert_eq!(got[0].1.as_ref().unwrap().parent, Some(Ref::Here(key(3))));
    assert_eq!(got[1].1, Err(Outcome::NotFound));
    assert_eq!(got[2].1, Err(Outcome::Unread(Unread::Malformed)));
}

/// A rate limit on the second question of parents fails the tickets it
/// asks as `Spent`; those of the first keep their parents.
#[test]
fn a_rate_limit_on_the_second_parents_question_spends_only_its_tickets() {
    let keys: Vec<TicketKey> = (1..=101).map(key).collect();
    let nodes: Vec<String> = (1..=100)
        .map(|n| format!(r#""n{n}":{{"number":{n},"parent":null}}"#))
        .collect();
    let first = format!(
        r#"{{"data":{{"repository":{{"databaseId":7001,{}}}}}}}"#,
        nodes.join(",")
    );
    let mut answers: Vec<String> = (1..=101).map(rest).collect();
    answers.push(first);
    let mut answers: Vec<Result<&str, &str>> = answers.iter().map(|a| Ok(a.as_str())).collect();
    answers.push(Err(": rate limit"));
    let (gh, asked) = adapter(answers);
    let mut gh = gh.on_clock(|| 1_767_225_600);
    let got = gh.several(&keys);
    assert_eq!(asked.lock().unwrap().len(), 103, "no question asked again");
    assert!(got[..100]
        .iter()
        .all(|(_, d)| d.as_ref().unwrap().parent.is_none()));
    assert_eq!(
        got[100],
        (
            key(101),
            Err(Outcome::Spent {
                pool: Pool::Calls,
                until: At::new(1_767_225_600),
            })
        )
    );
}

fn aliased(answer: &str, n: u32) -> String {
    answer.replacen(r#""issue":"#, &format!(r#""n{n}":"#), 1)
}

fn live(name: &str) -> String {
    let at = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/recorded/live-parent")
        .join(name);
    std::fs::read_to_string(at).unwrap()
}

/// Read live from a public repository, 2026-11-24: GitHub's REST read of an
/// issue leaves `parent_issue_url` out where the issue is a part of none
/// (the recordings `rest-*.json` keep the fields that say so), so the
/// adapter cannot tell none from an answer cut short there, and asks
/// GraphQL, whose `parent` is null for none. The `parent-*.json` are
/// GraphQL's answers to the adapter's question of one ticket, made before
/// the question asked each ticket under an alias; `aliased` moves the
/// issue's node under its alias, the rest as GitHub answered.
#[test]
fn the_parent_is_graphqls_since_rest_leaves_out_the_field_of_none() {
    let rest = |n: u32| -> serde_json::Value {
        serde_json::from_str(&live(&format!("rest-{n}.json"))).unwrap()
    };
    assert_eq!(rest(2597)["has_parent_issue_url"], false);
    assert_eq!(rest(2824)["has_parent_issue_url"], true);
    let issue = |n: u32| {
        ISSUE
            .replace(r#""number":7"#, &format!(r#""number":{n}"#))
            .replace(
                "acme/widget/issues/7",
                &format!("tehas-app/tehas-factory/issues/{n}"),
            )
    };
    for (n, want) in [(2824, Some(Ref::Here(key(2597)))), (2597, None)] {
        let (out, answer) = (issue(n), aliased(&live(&format!("parent-{n}.json")), n));
        let (mut gh, _) = adapter_in("tehas-app/tehas-factory", vec![Ok(&out), Ok(&answer)]);
        assert_eq!(
            gh.several(&[key(n.into())]).remove(0).1.unwrap().parent,
            want,
            "#{n}"
        );
    }
}

/// The project is told by its repository's database id, never by its
/// name. The live recordings were read from a repository renamed before:
/// a handle set to a name it no longer bears reads #2824's parent as this
/// project's, as one set to its name does.
#[test]
fn a_renamed_project_is_still_here_by_its_repository_id() {
    let issue = ISSUE.replace(r#""number":7"#, r#""number":2824"#).replace(
        "acme/widget/issues/7",
        "tehas-app/tehas-factory/issues/2824",
    );
    let answer = aliased(&live("parent-2824.json"), 2824);
    for configured in [
        "tehas-app/factory-before",
        "tehas-app/tehas-factory",
        "someone/else",
    ] {
        let (mut gh, _) = adapter_in(configured, vec![Ok(&issue), Ok(&answer)]);
        assert_eq!(
            gh.several(&[key(2824)]).remove(0).1.unwrap().parent,
            Some(Ref::Here(key(2597))),
            "{configured}"
        );
    }
}

/// A repository that bears this project's configured name and another id
/// is another project, and one named with no id is unread.
#[test]
fn another_repositorys_id_is_elsewhere_whatever_its_name() {
    let impostor = PARENT.replace(
        r#""databaseId":7001,"nameWithOwner""#,
        r#""databaseId":7999,"nameWithOwner""#,
    );
    let (mut gh, _) = adapter(vec![Ok(ISSUE), Ok(&impostor)]);
    assert_eq!(
        gh.several(&[key(7)]).remove(0).1.unwrap().parent,
        Some(Ref::Elsewhere {
            project: tehas_ports::ProjectName::new("acme/widget"),
            item: tehas_ports::ItemName::new("3"),
        })
    );
    let no_id = PARENT.replace(r#""databaseId":7001,"nameWithOwner""#, r#""nameWithOwner""#);
    let (mut gh, _) = adapter(vec![Ok(ISSUE), Ok(&no_id)]);
    assert_eq!(
        gh.several(&[key(7)]).remove(0).1,
        Err(Outcome::Unread(Unread::Malformed))
    );
    // An answer that names no id of this project's repository leaves the
    // handle nothing to tell here from elsewhere by.
    let unlearned = PARENT.replace(
        r#"{"repository":{"databaseId":7001,"n7""#,
        r#"{"repository":{"n7""#,
    );
    let (mut gh, _) = adapter(vec![Ok(ISSUE), Ok(&unlearned)]);
    assert_eq!(
        gh.several(&[key(7)]).remove(0).1,
        Err(Outcome::Unread(Unread::Inconsistent))
    );
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
        let (mut gh, _) = adapter(vec![Ok(&closed), Ok(NO_PARENT)]);
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

/// The repository's labels, one page: none.
const NO_LABELS: &str = r#"{"data":{"repository":{"labels":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}}}}"#;

#[test]
fn prepare_makes_every_label_of_the_table_and_one_that_fails_stops_none() {
    let specs = table::specs();
    let mut answers = vec![Ok(NO_LABELS)];
    answers.extend(vec![Ok(""); specs.len()]);
    answers[3] = Err("HTTP 502");
    answers[6] = Err("HTTP 403");
    let (mut gh, asked) = adapter(answers);
    assert_eq!(
        gh.prepare(),
        Err({
            let mut u = Unprepared::new(Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            });
            u.mark = Some(specs[2].name.clone());
            u
        })
    );
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), specs.len() + 1);
    assert!(asked[0].starts_with("api graphql "), "{}", asked[0]);
    assert_eq!(
        asked[1],
        "label create tehas:ready --repo acme/widget --color 0e8a16 --description Tehas: eligible for dispatch (apply by hand to authorize a cycle) --force"
    );
    for (call, spec) in asked[1..].iter().zip(&specs) {
        assert!(
            call.starts_with(&format!(
                "label create {} --repo acme/widget --color ",
                spec.name.as_str()
            )),
            "{call}"
        );
        assert!(call.ends_with(" --force"), "{call}");
    }
    let mut answers = vec![Ok(NO_LABELS)];
    answers.extend(vec![Ok(""); specs.len()]);
    let (mut gh, _) = adapter(answers);
    assert_eq!(gh.prepare(), Ok(()));
}

#[test]
fn prepare_whose_read_fails_still_makes_every_label_and_answers_the_read() {
    let specs = table::specs();
    let mut answers = vec![Err("HTTP 502")];
    answers.extend(vec![Ok(""); specs.len()]);
    let (mut gh, asked) = adapter(answers);
    let read = Outcome::Unavailable {
        why: Passing::Server,
        sent: Sent::Maybe,
    };
    assert_eq!(gh.prepare(), Err(Unprepared::new(read)));
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), specs.len() + 1);
    assert!(asked[1..].iter().all(|c| c.starts_with("label create ")));
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
        gh.count(&tehas_ports::Filter::default()),
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
        gh.cite(&Cite::Mark(Mark::Act(Act::Approve))),
        format!("`{}`", table::APPROVAL_LABEL)
    );
    assert_eq!(gh.cite(&Cite::Mark(Mark::Unknown)), "");
    let finish = tehas_ports::Marker::Millwright(tehas_ports::MillwrightMarker::new(
        tehas_ports::BlueprintNo::new(12).unwrap(),
        tehas_ports::MillwrightAction::Finish,
    ));
    assert_eq!(
        gh.cite(&Cite::Marker(finish.clone())),
        table::cite_marker(&finish)
    );
    assert_eq!(
        gh.cite(&Cite::Marker(finish)),
        "`<!-- tehas:millwright issue=12 action=finish -->`"
    );
    assert_eq!(gh.cite(&Cite::Marker(tehas_ports::Marker::Unknown)), "");
    assert_eq!(
        gh.new_ticket_link("A b", "c&d").as_str(),
        "https://github.com/acme/widget/issues/new?title=A%20b&body=c%26d"
    );
    let fits = "x".repeat(3000);
    assert!(gh.new_ticket_link("A", &fits).as_str().ends_with(&fits));
    let long = "x".repeat(9000);
    assert_eq!(
        gh.new_ticket_link("A", &long).as_str(),
        "https://github.com/acme/widget/issues/new?title=A"
    );
}
