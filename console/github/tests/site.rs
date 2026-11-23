//! The adapter's `Site`: which actors get handles and which are refused
//! before any request, the ports it does not serve yet, a mark named with
//! no handle, and the meter. Which accounts are `Ours` is `site_ours.rs`.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::{Arc, Mutex};

use tehas_core::forge::GhRunner;
use tehas_github::{table, GitHubSite, MakeRunner};
use tehas_ports::laws::marks::{every_mark_names_a_word, factory_marks};
use tehas_ports::{
    Actor, Caller, Mark, MarkName, Outcome, PersonKey, Pool, PortName, Realm, Refusal, Scope,
    Setup, Site, TicketKey, Window, ALL_MAX,
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
    (GitHubSite::new("acme/widget", BOT, Some(900), make), asked)
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

fn refused<T>(got: tehas_ports::Got<T>) -> Option<Outcome> {
    got.err()
}

const UNSUPPORTED: Outcome = Outcome::Refused(Refusal::Unsupported);

#[test]
fn the_factorys_account_and_the_operator_get_handles_of_each_port_built() {
    let (site, asked) = site(&[]);
    for by in [Actor::Ours(Scope::Full), Actor::Operator] {
        assert!(site.tickets(&by, CALLER).is_ok(), "{by:?}");
        assert!(site.ticket_writes(&by, CALLER).is_ok(), "{by:?}");
        assert!(site.blueprints(&by, CALLER).is_ok(), "{by:?}");
        assert!(site.identity(&by, CALLER).is_ok(), "{by:?}");
    }
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 8);
    assert!(asked[..4].iter().all(|a| *a == Actor::Ours(Scope::Full)));
    assert!(asked[4..].iter().all(|a| *a == Actor::Operator));
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
        assert_eq!(
            refused(site.blueprints(by, CALLER)),
            Some(UNSUPPORTED),
            "{by:?}"
        );
        assert_eq!(
            refused(site.identity(by, CALLER)),
            Some(UNSUPPORTED),
            "{by:?}"
        );
    }
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_port_not_built_here_is_unsupported_and_makes_no_runner() {
    let (site, asked) = site(&[]);
    let by = Actor::Ours(Scope::Full);
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
    let site = GitHubSite::new("acme/widget", BOT, Some(900), make);
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
    let _ = read.history(&key(1), Window::Whole(ALL_MAX), None);
    let mut search = site.blueprints(&by, CALLER).unwrap();
    let _ = search.search(
        "importer",
        tehas_ports::Within::Everything,
        None,
        NonZeroU32::new(5).unwrap(),
    );
    let mut write = site.ticket_writes(&by, Caller::new("test.writes")).unwrap();
    let _ = write.mark(&key(1), &Mark::Epic);
    let meter: Vec<_> = site
        .meter()
        .into_iter()
        .map(|r| (r.caller.as_str(), r.port, r.pool, r.calls, r.points))
        .collect();
    let want = [
        ("test.site", PortName::Tickets, Pool::Calls, 2, 2),
        ("test.site", PortName::Tickets, Pool::Priced, 1, 1),
        ("test.site", PortName::Blueprints, Pool::Search, 1, 1),
        ("test.writes", PortName::Tickets, Pool::Writes, 1, 1),
    ];
    assert_eq!(meter.len(), want.len(), "{meter:?}");
    for row in want {
        assert!(meter.contains(&row), "{row:?} in {meter:?}");
    }
}

/// The site's clock in the deadline tests.
const NOW: i64 = 1_767_225_600;

fn deadline_site(answers: &[&str]) -> GitHubSite {
    site(answers).0.on_clock(|| NOW)
}

const TIMEOUT: Outcome = Outcome::Unavailable {
    why: tehas_ports::Passing::Timeout,
    sent: tehas_ports::Sent::No,
};

#[test]
fn a_call_past_the_views_deadline_is_a_timeout_never_sent_nor_counted() {
    let site = deadline_site(&[]);
    let by = Actor::Ours(Scope::Full);
    let mut before = site.tickets(&by, CALLER).unwrap();
    for until in [NOW, NOW - 1] {
        let view = site.until(tehas_ports::At::new(until));
        let mut h = view.tickets(&by, CALLER).unwrap();
        let (_, got) = h.several(&[key(7)]).pop().unwrap();
        assert_eq!(got.err(), Some(TIMEOUT), "until {until}");
        let mut w = view.ticket_writes(&by, CALLER).unwrap();
        assert_eq!(w.mark(&key(7), &Mark::Epic), Err(TIMEOUT));
    }
    assert!(site.meter().is_empty(), "{:?}", site.meter());
    // A handle made before the view sends, and the view shares its meter.
    let _ = before.several(&[key(7)]);
    assert_eq!(site.meter().len(), 1);
}

#[test]
fn a_view_before_its_deadline_sends_and_a_view_of_a_view_keeps_the_earlier() {
    let site = deadline_site(&[]);
    let by = Actor::Ours(Scope::Full);
    let later = site.until(tehas_ports::At::new(NOW + 60));
    let mut h = later.tickets(&by, CALLER).unwrap();
    let (_, got) = h.several(&[key(7)]).pop().unwrap();
    assert_ne!(got.err(), Some(TIMEOUT));
    assert_eq!(later.meter().len(), 1);
    assert_eq!(site.meter(), later.meter());
    let earlier = later.until(tehas_ports::At::new(NOW));
    let again = earlier.until(tehas_ports::At::new(NOW + 600));
    let mut h = again.tickets(&by, CALLER).unwrap();
    let (_, got) = h.several(&[key(7)]).pop().unwrap();
    assert_eq!(got.err(), Some(TIMEOUT));
}
