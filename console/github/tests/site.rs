//! The adapter's `Site`: which actors get handles and which are refused
//! before any request, the ports it does not serve yet, a mark named with
//! no handle, the meter, and the factory's own account answered `Ours` on
//! a ticket's author and a history event's actor.

use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};

use tehas_core::forge::GhRunner;
use tehas_github::{table, GitHub, GitHubSite, MakeRunner};
use tehas_ports::laws::marks::{every_mark_names_a_word, factory_marks};
use tehas_ports::{
    Account, AccountKind, Actor, Caller, HappenedKind, Mark, MarkName, Outcome, PersonKey, Pool,
    PortName, Realm, Refusal, Scope, Setup, Site, TicketKey, Tickets, Window, ALL_MAX,
};

const CALLER: Caller = Caller::new("test.site");
const BOT: &str = "tehas-bot[bot]";

/// Answers each call from the list in turn; a call past it fails.
struct Stub {
    answers: Vec<String>,
    next: usize,
}

impl GhRunner for Stub {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        self.next += 1;
        self.answers
            .get(self.next - 1)
            .cloned()
            .ok_or_else(|| "no fixture".to_string())
    }
}

/// A runner maker answering `answers` to each runner it makes, and the
/// actors it was asked for.
fn maker(answers: &[&str]) -> (MakeRunner, Arc<Mutex<Vec<Actor>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let answers: Vec<String> = answers.iter().map(|a| a.to_string()).collect();
    let make: MakeRunner = Arc::new(move |by: &Actor| {
        seen.lock().unwrap().push(by.clone());
        Ok(Box::new(Stub {
            answers: answers.clone(),
            next: 0,
        }) as Box<dyn GhRunner>)
    });
    (make, asked)
}

fn site(answers: &[&str]) -> (GitHubSite, Arc<Mutex<Vec<Actor>>>) {
    let (make, asked) = maker(answers);
    (GitHubSite::new("acme/widget", BOT, make), asked)
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

fn refused<T>(got: tehas_ports::Got<T>) -> Option<Outcome> {
    got.err()
}

const UNSUPPORTED: Outcome = Outcome::Refused(Refusal::Unsupported);

#[test]
fn the_factorys_account_and_the_operator_get_handles_of_both_ticket_ports() {
    let (site, asked) = site(&[]);
    for by in [Actor::Ours(Scope::Full), Actor::Operator] {
        assert!(site.tickets(&by, CALLER).is_ok(), "{by:?}");
        assert!(site.ticket_writes(&by, CALLER).is_ok(), "{by:?}");
    }
    assert_eq!(
        *asked.lock().unwrap(),
        [
            Actor::Ours(Scope::Full),
            Actor::Ours(Scope::Full),
            Actor::Operator,
            Actor::Operator
        ]
    );
    assert_eq!(site.realms(), [Realm::CodeHost, Realm::Tickets]);
}

#[test]
fn an_actor_the_factory_cannot_run_is_unsupported_before_a_runner_is_made() {
    let (site, asked) = site(&[]);
    let key = PersonKey::new("k");
    let actors = [
        Actor::Ours(Scope::Cycle),
        Actor::Ours(Scope::CycleMerge),
        Actor::Ours(Scope::Session),
        Actor::Ours(Scope::Agent),
        Actor::Ours(Scope::Read),
        Actor::Ours(Scope::Files),
        Actor::Person(key.clone()),
        Actor::Deploy(key.clone()),
        Actor::Given(key),
        Actor::Anonymous,
    ];
    for by in &actors {
        assert_eq!(
            refused(site.tickets(by, CALLER)),
            Some(UNSUPPORTED),
            "{by:?}"
        );
        let want = if *by == Actor::Anonymous {
            Outcome::Forbidden
        } else {
            UNSUPPORTED
        };
        assert_eq!(
            refused(site.ticket_writes(by, CALLER)),
            Some(want),
            "{by:?}"
        );
    }
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_port_not_built_here_is_unsupported_and_makes_no_runner() {
    let (site, asked) = site(&[]);
    let by = Actor::Ours(Scope::Full);
    assert_eq!(refused(site.blueprints(&by, CALLER)), Some(UNSUPPORTED));
    assert_eq!(
        refused(site.blueprint_writes(&by, CALLER)),
        Some(UNSUPPORTED)
    );
    assert_eq!(refused(site.code_host(&by, CALLER)), Some(UNSUPPORTED));
    assert_eq!(
        refused(site.code_host_writes(&by, CALLER)),
        Some(UNSUPPORTED)
    );
    assert_eq!(refused(site.board(&by, CALLER)), Some(UNSUPPORTED));
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_runner_the_wiring_cannot_make_stops_the_handle_with_its_outcome() {
    let make: MakeRunner = Arc::new(|_: &Actor| Err(Outcome::SignedOut));
    let site = GitHubSite::new("acme/widget", BOT, make);
    let got = site.tickets(&Actor::Ours(Scope::Full), CALLER);
    assert_eq!(refused(got), Some(Outcome::SignedOut));
}

#[test]
fn every_mark_is_named_as_the_table_names_it_with_no_handle() {
    let (site, asked) = site(&[]);
    let project = [Mark::Project(MarkName::new("bug").unwrap())];
    assert_eq!(every_mark_names_a_word(&site, &project), Ok(()));
    for m in factory_marks().iter().chain(&project) {
        assert_eq!(site.mark_name(m), table::mark_name(m), "{m:?}");
        assert_eq!(Some(site.mark_name(m)), table::label(m), "{m:?}");
    }
    assert_eq!(site.mark_name(&Mark::Unknown), "");
    let setup = Setup::One(Arc::new(site));
    for m in factory_marks() {
        for realm in [Realm::Tickets, Realm::CodeHost] {
            assert_eq!(setup.mark_name(realm, &m), table::mark_name(&m), "{m:?}");
        }
    }
    assert_eq!(setup.mark_name(Realm::Tickets, &project[0]), "bug");
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_setup_of_the_site_hands_out_its_handles_by_realm() {
    let (site, asked) = site(&[]);
    let setup = Setup::One(Arc::new(site));
    let by = Actor::Ours(Scope::Full);
    assert!(setup.tickets(&by, CALLER).is_ok());
    assert!(setup.ticket_writes(&by, CALLER).is_ok());
    assert!(setup.identity(Realm::CodeHost, &by, CALLER).is_ok());
    assert!(setup.identity(Realm::Tickets, &by, CALLER).is_ok());
    assert_eq!(refused(setup.board(&by, CALLER)), Some(UNSUPPORTED));
    assert_eq!(asked.lock().unwrap().len(), 4);
}

#[test]
fn the_meter_counts_each_call_by_caller_port_and_pool() {
    let (site, _) = site(&[]);
    let by = Actor::Ours(Scope::Full);
    let mut read = site.tickets(&by, CALLER).unwrap();
    let _ = read.several(&[key(1), key(2)]);
    let mut write = site.ticket_writes(&by, Caller::new("test.writes")).unwrap();
    let _ = write.mark(&key(1), &Mark::Epic);
    let mut meter = site.meter();
    meter.sort_by_key(|r| r.caller.as_str());
    assert_eq!(meter.len(), 2);
    assert_eq!(
        (meter[0].caller, meter[0].port, meter[0].pool),
        (CALLER, PortName::Tickets, Pool::Calls)
    );
    assert_eq!((meter[0].calls, meter[0].points), (2, 2));
    assert_eq!(
        (meter[1].caller.as_str(), meter[1].pool, meter[1].calls),
        ("test.writes", Pool::Writes, 1)
    );
}

/// An issue whose author is `user`, a JSON object.
fn issue(user: &str) -> String {
    format!(
        r#"{{"number":7,"title":"Make it so","state":"open","state_reason":null,
"labels":[],"user":{user},
"created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-03T00:00:00Z",
"html_url":"https://github.com/acme/widget/issues/7"}}"#
    )
}

/// One page of label events, one event by `actor`, a JSON object.
fn events(actor: &str) -> String {
    format!(
        r#"[{{"event":"labeled","label":{{"name":"tehas:pause"}},"actor":{actor},
"created_at":"2026-01-02T03:04:05Z"}}]"#
    )
}

fn author(h: &mut dyn Tickets) -> Account {
    let (_, d) = h.several(&[key(7)]).pop().unwrap();
    d.unwrap().author.unwrap()
}

fn actor(h: &mut dyn Tickets) -> Account {
    let took = h
        .history(
            &key(7),
            Window::Whole(ALL_MAX),
            Some(&[HappenedKind::Marked]),
        )
        .unwrap();
    took.items[0].by.clone().unwrap()
}

/// The kind of the author and of the event's actor that `user` names, read
/// through a handle of `site`.
fn kinds(site: impl Fn(&[&str]) -> GitHubSite, user: &str) -> (AccountKind, AccountKind) {
    let (issue, events) = (issue(user), events(user));
    let by = Actor::Ours(Scope::Full);
    let s = site(&[&issue]);
    let a = author(s.tickets(&by, CALLER).unwrap().as_mut());
    let s = site(&[&events]);
    let e = actor(s.tickets(&by, CALLER).unwrap().as_mut());
    (a.kind, e.kind)
}

fn plain(answers: &[&str]) -> GitHubSite {
    site(answers).0
}

fn with_id(answers: &[&str]) -> GitHubSite {
    site(answers).0.with_bot_id(42)
}

#[test]
fn the_factorys_account_is_ours_in_every_spelling_and_case() {
    let ours = (AccountKind::Ours, AccountKind::Ours);
    for login in [
        "tehas-bot",
        "tehas-bot[bot]",
        "app/tehas-bot",
        "Tehas-Bot[BOT]",
    ] {
        let bot = format!(r#"{{"login":"{login}","id":42,"type":"Bot"}}"#);
        assert_eq!(kinds(plain, &bot), ours, "{login}");
        assert_eq!(kinds(with_id, &bot), ours, "{login} with the id");
        let untyped = format!(r#"{{"login":"{login}","id":42}}"#);
        assert_eq!(kinds(plain, &untyped), ours, "{login} untyped");
    }
    // The bot's login given in another spelling names the same account.
    let given = |a: &[&str]| {
        let (make, _) = maker(a);
        GitHubSite::new("acme/widget", "App/Tehas-Bot", make)
    };
    let bot = r#"{"login":"tehas-bot[bot]","id":42,"type":"Bot"}"#;
    assert_eq!(kinds(given, bot), ours);
}

#[test]
fn an_account_that_only_looks_like_the_factorys_is_not_ours() {
    let bot = (AccountKind::Bot, AccountKind::Bot);
    // The same login on another id, once the bot's id is known.
    let other = r#"{"login":"tehas-bot[bot]","id":7,"type":"Bot"}"#;
    assert_eq!(kinds(with_id, other), bot);
    // The bot's login with no id, once the bot's id is known.
    let no_id = r#"{"login":"tehas-bot[bot]","type":"Bot"}"#;
    assert_eq!(kinds(with_id, no_id), bot);
    // A person holding the slug the App's login folds to.
    let person = r#"{"login":"tehas-bot","id":42,"type":"User"}"#;
    let people = (AccountKind::Person, AccountKind::Person);
    assert_eq!(kinds(plain, person), people);
    assert_eq!(kinds(with_id, person), people);
    // Another login altogether, and one that only begins with the bot's.
    for login in ["alice", "tehas-bot2[bot]", "tehas-bo"] {
        let user = format!(r#"{{"login":"{login}","id":42,"type":"Bot"}}"#);
        assert_eq!(kinds(plain, &user), bot, "{login}");
    }
}

#[test]
fn a_site_with_an_empty_bot_login_answers_no_account_ours() {
    let empty = |a: &[&str]| {
        let (make, _) = maker(a);
        GitHubSite::new("acme/widget", "", make)
    };
    let bot = r#"{"login":"tehas-bot[bot]","id":42,"type":"Bot"}"#;
    assert_eq!(kinds(empty, bot), (AccountKind::Bot, AccountKind::Bot));
    let marks = r#"{"login":"[bot]","id":42,"type":"Bot"}"#;
    assert_eq!(kinds(empty, marks), (AccountKind::Bot, AccountKind::Bot));
    let only_marks = |a: &[&str]| {
        let (make, _) = maker(a);
        GitHubSite::new("acme/widget", "[bot]", make)
    };
    assert_eq!(
        kinds(only_marks, marks),
        (AccountKind::Bot, AccountKind::Bot)
    );
}

#[test]
fn a_handle_made_without_a_site_never_answers_ours() {
    let user = r#"{"login":"tehas-bot[bot]","id":42,"type":"Bot"}"#;
    let mut gh = GitHub::new(
        "acme/widget",
        Box::new(Stub {
            answers: vec![issue(user), events(user)],
            next: 0,
        }),
    );
    assert_eq!(author(&mut gh).kind, AccountKind::Bot);
    assert_eq!(actor(&mut gh).kind, AccountKind::Bot);
}

/// The issue's timeline, every kind, with one label event and one close by
/// `actor`, a GraphQL account object.
fn timeline(actor: &str) -> String {
    format!(
        r#"{{"data":{{"repository":{{"issue":{{"number":7,"milestone":null,"events":{{"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[
{{"__typename":"LabeledEvent","createdAt":"2026-01-01T00:00:01Z","actor":{actor},"label":{{"name":"bug"}}}},
{{"__typename":"ClosedEvent","createdAt":"2026-01-01T00:00:02Z","actor":{actor},"stateReason":"COMPLETED","closer":null}}]}}}}}}}}}}"#
    )
}

#[test]
fn the_timeline_answers_the_factorys_account_ours_and_a_person_of_its_slug_not() {
    let read = |actor: &str| {
        let answer = timeline(actor);
        let s = with_id(&[&answer]);
        let mut h = s.tickets(&Actor::Ours(Scope::Full), CALLER).unwrap();
        let took = h.history(&key(7), Window::Whole(ALL_MAX), None).unwrap();
        let by: Vec<AccountKind> = took
            .items
            .iter()
            .map(|e| e.by.clone().unwrap().kind)
            .collect();
        let closer = match &took.items[1].what {
            tehas_ports::Happened::Closed {
                closer: Some(tehas_ports::Closer::Account(a)),
                ..
            } => a.kind,
            other => panic!("{other:?}"),
        };
        (by, closer)
    };
    let bot = r#"{"__typename":"Bot","login":"tehas-bot","databaseId":42}"#;
    assert_eq!(
        read(bot),
        (
            vec![AccountKind::Ours, AccountKind::Ours],
            AccountKind::Ours
        )
    );
    let person = r#"{"__typename":"User","login":"tehas-bot","databaseId":42}"#;
    assert_eq!(
        read(person),
        (
            vec![AccountKind::Person, AccountKind::Person],
            AccountKind::Person
        )
    );
    let other = r#"{"__typename":"Bot","login":"tehas-bot","databaseId":7}"#;
    assert_eq!(
        read(other),
        (vec![AccountKind::Bot, AccountKind::Bot], AccountKind::Bot)
    );
}

/// What `me` answers through an identity handle of `site`, the viewer and
/// the account read answering as recorded.
fn me_of(site: impl Fn(&[&str]) -> GitHubSite, viewer: &str, user: &str) -> Account {
    let s = site(&[viewer, user]);
    let mut h = s.identity(&Actor::Ours(Scope::Full), CALLER).unwrap();
    h.me().unwrap()
}

const USER_900: &str = r#"{"login":"tehas-bot[bot]","id":900,"type":"Bot"}"#;

#[test]
fn me_answers_the_factorys_own_account_ours() {
    let id_900 = |a: &[&str]| site(a).0.with_bot_id(900);
    for viewer in [
        r#"{"data":{"viewer":{"login":"tehas-bot[bot]","databaseId":900}}}"#,
        r#"{"data":{"viewer":{"login":"Tehas-Bot","databaseId":900}}}"#,
    ] {
        assert_eq!(me_of(plain, viewer, USER_900).kind, AccountKind::Ours);
        assert_eq!(me_of(id_900, viewer, USER_900).kind, AccountKind::Ours);
    }
    // The same login on another id is another App's.
    let viewer = r#"{"data":{"viewer":{"login":"tehas-bot","databaseId":900}}}"#;
    assert_eq!(me_of(with_id, viewer, USER_900).kind, AccountKind::Bot);
    // The operator's own account is a person.
    let viewer = r#"{"data":{"viewer":{"login":"Alice","databaseId":101}}}"#;
    let user = r#"{"login":"Alice","id":101,"type":"User"}"#;
    assert_eq!(me_of(plain, viewer, user).kind, AccountKind::Person);
}

#[test]
fn me_through_a_handle_made_without_a_site_is_never_ours() {
    let viewer = r#"{"data":{"viewer":{"login":"tehas-bot[bot]","databaseId":900}}}"#;
    let mut gh = GitHub::new(
        "acme/widget",
        Box::new(Stub {
            answers: vec![viewer.to_string(), USER_900.to_string()],
            next: 0,
        }),
    );
    assert_eq!(
        tehas_ports::Identity::me(&mut gh).unwrap().kind,
        AccountKind::Bot
    );
}
