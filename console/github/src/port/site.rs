//! The handle factory, `Site`: [`GitHubSite`] makes the handles of one
//! repository, each a [`GitHub`] over a runner the wiring makes for the
//! actor, and carrying the factory's own account, so an account that is
//! the factory's answers `Ours`. The adapter never obtains a token: the
//! runner it is given is how it reaches gh.

use std::sync::{Arc, Mutex};

use super::anonymous::{Anonymous, Public};
use super::people::{People, PeopleClient};
use super::unsupported;
use crate::tickets::{Bot, NOT_STARTED};
use crate::{table, AuthorisedCurl, Curl, GitHub};
use tehas_core::forge::{Failed, GhRunner};
use tehas_ports::{
    Actor, At, BlueprintWrites, Blueprints, Board, Caller, CodeHost, CodeHostWrites, Got, Identity,
    Ledger, Mark, Marker, MeterRow, Outcome, Pool, PortName, Realm, Scope, Site, TicketWrites,
    Tickets,
};

/// What makes a runner of gh for an actor: the wiring's, so the adapter
/// never holds a token or a credential. It is asked only for an actor the
/// site hands handles to, `Actor::Ours(Scope::Full)` and `Actor::Operator`,
/// and `Actor::Ours(Scope::Read)` for a code host read handle.
pub type MakeRunner = Arc<dyn Fn(&Actor) -> Got<Box<dyn GhRunner>> + Send + Sync>;

/// GitHub as a `Site`: the code host and its own tracker, both realms.
#[derive(Clone)]
pub struct GitHubSite {
    repo: String,
    bot: Bot,
    make: MakeRunner,
    meter: Arc<Mutex<Vec<MeterRow>>>,
    /// The time its handles stop at, if a view of `Site::until`.
    deadline: Option<At>,
    now: fn() -> i64,
    /// The client a person signs in through, if the wiring gave one.
    people: Option<Arc<People>>,
    /// curl and the ledger of anonymous reads, if the wiring asked for them.
    public: Option<Public>,
}

impl GitHubSite {
    /// The site of `repo` (`owner/name`), whose runners `make` gives.
    ///
    /// The factory's account is its App's bot account: `bot_login` as the
    /// factory keeps it (`<slug>[bot]`, a trailing newline trimmed) and
    /// `bot_user_id`, GitHub's numeric user id of that bot account. That is
    /// not the GitHub App's id: given the App's id, no account is `Ours`.
    /// The factory records the bot's user id beside its login in
    /// `state/account-ids`, and `Identity::me` through a handle made for
    /// `Actor::Ours` answers it too. An account is `Ours` only when GitHub
    /// types it a bot and it carries that id; with no id given, none is.
    /// The caller reads both; the adapter reads no file and no variable.
    pub fn new(
        repo: impl Into<String>,
        bot_login: &str,
        bot_user_id: Option<u64>,
        make: MakeRunner,
    ) -> GitHubSite {
        GitHubSite {
            repo: repo.into(),
            bot: Bot::new(bot_login, bot_user_id),
            make,
            meter: Arc::new(Mutex::new(Vec::new())),
            deadline: None,
            now: tehas_core::time::now,
            people: None,
            public: None,
        }
    }

    /// The same site with the factory's client for people: its identity
    /// handles sign a person in (`Identity::sign_in_link`, `signed_in`),
    /// trading the code through the adapter's authorised curl. The wiring
    /// reads the client; the adapter reads no file and no variable. A site
    /// whose project is not `owner/name` signs no one in.
    pub fn with_people_client(self, client: PeopleClient) -> GitHubSite {
        let people = People::new(client, &self.repo, AuthorisedCurl::default()).map(Arc::new);
        GitHubSite { people, ..self }
    }

    /// The same site with the client for people trading through `trade`, a
    /// test's GitHub in place of curl.
    pub fn with_people_trading(
        self,
        client: PeopleClient,
        trade: Arc<dyn tehas_core::access::GitHub>,
    ) -> GitHubSite {
        GitHubSite {
            people: Some(Arc::new(People::trading(client, trade))),
            ..self
        }
    }

    /// The same site answering `Actor::Anonymous`'s code host reads of a
    /// public project (Decided 76) through curl, with no credential, its
    /// budget kept in `ledger` as the anonymous holder's.
    pub fn with_anonymous_reads(self, ledger: Arc<dyn Ledger>) -> GitHubSite {
        self.with_anonymous_curl(Curl::new(), ledger)
    }

    /// The same, through `curl`: a test's stub in curl's place.
    pub fn with_anonymous_curl(self, curl: Curl, ledger: Arc<dyn Ledger>) -> GitHubSite {
        GitHubSite {
            public: Some(Public { curl, ledger }),
            ..self
        }
    }

    /// The same site reading the time from `now`: its handles' deadline
    /// and a spent budget's time, so a test answers a fixed time.
    pub fn on_clock(self, now: fn() -> i64) -> GitHubSite {
        GitHubSite { now, ..self }
    }

    /// A handle of `port` acting as `by`, or the outcome that stops its
    /// making before any request. The factory's account at full scope and
    /// the operator's own gh are run; every other actor is
    /// `Refused(Unsupported)`: a narrower scope, a given token, a person's
    /// or a deploy credential each need a token this adapter does not mint
    /// or keep yet, and handing them the full runner would widen them. Two
    /// exceptions are the factory's account at read scope. Its code host
    /// read handle runs the runner the wiring makes for it: the port has no
    /// write, and its remote fetches alone, so the scope is the handle's.
    /// Its identity handle, on a site given a client for people, signs a
    /// person in and runs no gh. A write handle asked at read scope or as
    /// Anonymous is `Forbidden`, as the trait says; Anonymous's code host
    /// reads are `code_host`'s, through curl.
    fn handle(&self, by: &Actor, caller: Caller, port: PortName, write: bool) -> Got<GitHub> {
        match by {
            Actor::Ours(Scope::Full) | Actor::Operator => {}
            Actor::Ours(Scope::Read) if port == PortName::CodeHost && !write => {}
            // A sign-in, the factory's account at read scope (Decided 76):
            // the client for people alone, and no runner of gh, so the
            // handle asks GitHub nothing as the factory.
            Actor::Ours(Scope::Read) if port == PortName::Identity && !write => {
                let Some(people) = &self.people else {
                    return unsupported();
                };
                let mut gh = GitHub::new(self.repo.clone(), Box::new(NoCalls)).on_clock(self.now);
                gh.bot = Some(self.bot.clone());
                gh.people = Some(Arc::clone(people));
                return Ok(gh);
            }
            Actor::Ours(Scope::Read) | Actor::Anonymous if write => return Err(Outcome::Forbidden),
            _ => return unsupported(),
        }
        let runner = Metered {
            inner: (self.make)(by)?,
            meter: Arc::clone(&self.meter),
            caller,
            port,
            write,
            deadline: self.deadline,
            now: self.now,
        };
        let mut gh = GitHub::new(self.repo.clone(), Box::new(runner)).on_clock(self.now);
        gh.bot = Some(self.bot.clone());
        gh.as_bot = matches!(by, Actor::Ours(_));
        gh.fetch_only = *by == Actor::Ours(Scope::Read);
        gh.people = self.people.clone();
        Ok(gh)
    }
}

/// The runner of a sign-in's handle: it runs no gh, so the handle asks
/// nothing as the factory. A call is refused with this reason.
struct NoCalls;

impl GhRunner for NoCalls {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        Err("a sign-in's handle asks GitHub nothing as the factory".into())
    }
}

/// The realms GitHub serves: issues are its own tracker's tickets.
const REALMS: [Realm; 2] = [Realm::CodeHost, Realm::Tickets];

impl Site for GitHubSite {
    fn realms(&self) -> &[Realm] {
        &REALMS
    }

    fn tickets(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Tickets + Send>> {
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::Tickets,
            false,
        )?))
    }

    fn ticket_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn TicketWrites + Send>> {
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::Tickets,
            true,
        )?))
    }

    fn blueprints(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Blueprints + Send>> {
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::Blueprints,
            false,
        )?))
    }

    fn blueprint_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn BlueprintWrites + Send>> {
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::Blueprints,
            true,
        )?))
    }

    /// `Actor::Anonymous` reads through curl, on a site given it, and
    /// never through a runner of gh.
    fn code_host(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHost + Send>> {
        if *by == Actor::Anonymous {
            let Some(public) = &self.public else {
                return super::unsupported();
            };
            return Ok(Box::new(Anonymous::new(
                &self.repo,
                public,
                Arc::clone(&self.meter),
                caller,
                self.deadline,
                self.now,
            )));
        }
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::CodeHost,
            false,
        )?))
    }

    fn code_host_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHostWrites + Send>> {
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::CodeHost,
            true,
        )?))
    }

    fn identity(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Identity + Send>> {
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::Identity,
            false,
        )?))
    }

    /// GitHub answers the joined read itself, with one question a page.
    fn board(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Board + Send>> {
        Ok(Box::new(self.handle(by, caller, PortName::Board, false)?))
    }

    fn meter(&self) -> Vec<MeterRow> {
        lock(&self.meter).clone()
    }

    fn mark_name(&self, mark: &Mark) -> String {
        table::mark_name(mark)
    }

    /// The marker's text from the table, as a person types it in an issue.
    /// `""` for one that is not writable.
    fn marker_text(&self, marker: &Marker) -> String {
        table::marker_text(marker).unwrap_or_default()
    }

    /// A clone sharing the meter and the runner maker. A view of a view
    /// keeps the earlier deadline.
    fn until(&self, until: At) -> Arc<dyn Site> {
        let mut view = self.clone();
        view.deadline = Some(self.deadline.map_or(until, |d| d.min(until)));
        Arc::new(view)
    }
}

/// The meter, whole even after a handle panicked while it held the lock:
/// a count is never left half made.
fn lock(meter: &Mutex<Vec<MeterRow>>) -> std::sync::MutexGuard<'_, Vec<MeterRow>> {
    meter
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A runner that counts each call it sends in the site's meter: one call
/// and one point a request. A write handle's calls are `Writes`; a read
/// handle's GraphQL questions are `Priced` and its searches `Search`, the
/// pools the adapter reads their spent budget in, and its other REST reads
/// `Calls`.
struct Metered {
    inner: Box<dyn GhRunner>,
    meter: Arc<Mutex<Vec<MeterRow>>>,
    caller: Caller,
    port: PortName,
    write: bool,
    deadline: Option<At>,
    now: fn() -> i64,
}

impl Metered {
    /// Whether the handle's deadline has passed: a call then is not sent
    /// and not counted.
    fn past(&self) -> bool {
        self.deadline.is_some_and(|d| (self.now)() >= d.secs())
    }

    fn count(&self, args: &[&str]) {
        let pool = match (self.write, args) {
            (true, _) => Pool::Writes,
            (false, ["api", "graphql", ..]) => Pool::Priced,
            (false, args) if args.contains(&"search/issues") => Pool::Search,
            (false, _) => Pool::Calls,
        };
        count(&self.meter, self.caller, self.port, pool);
    }
}

/// One call and one point of `pool` in the meter, for `caller`'s `port`.
pub(super) fn count(meter: &Mutex<Vec<MeterRow>>, caller: Caller, port: PortName, pool: Pool) {
    let mut meter = lock(meter);
    let row = meter
        .iter_mut()
        .find(|r| r.caller == caller && r.port == port && r.pool == pool);
    match row {
        Some(row) => {
            row.calls = row.calls.saturating_add(1);
            row.points = row.points.saturating_add(1);
        }
        None => {
            let mut row = MeterRow::new(caller, port, pool);
            row.calls = 1;
            row.points = 1;
            meter.push(row);
        }
    }
}

impl GhRunner for Metered {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        if self.past() {
            return Err(NOT_STARTED.to_string());
        }
        self.count(args);
        self.inner.run(args)
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        if self.past() {
            return Err(NOT_STARTED.to_string());
        }
        self.count(args);
        self.inner.run_input(args, input)
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        if self.past() {
            return Err(Failed {
                why: NOT_STARTED.to_string(),
                answer: None,
            });
        }
        self.count(args);
        self.inner.run_answer(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts what reached gh, by any of the three ways.
    struct Sends(Arc<AtomicUsize>);

    impl GhRunner for Sends {
        fn run(&mut self, _: &[&str]) -> Result<String, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok("{}".into())
        }
        fn run_input(&mut self, _: &[&str], _: &str) -> Result<String, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok("{}".into())
        }
        fn run_answer(&mut self, _: &[&str]) -> Result<String, Failed> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok("{}".into())
        }
    }

    fn metered(write: bool, until: i64) -> (Metered, Arc<AtomicUsize>, Arc<Mutex<Vec<MeterRow>>>) {
        let sent = Arc::new(AtomicUsize::new(0));
        let meter = Arc::new(Mutex::new(Vec::new()));
        let m = Metered {
            inner: Box::new(Sends(Arc::clone(&sent))),
            meter: Arc::clone(&meter),
            caller: Caller::new("test.deadline"),
            port: PortName::Blueprints,
            write,
            deadline: Some(At::new(until)),
            now: || 1_000,
        };
        (m, sent, meter)
    }

    const ARGS: [&str; 2] = ["api", "repos/acme/widget/issues/7/comments"];

    /// A site whose wiring makes a runner that counts what reached it, and
    /// keeps each actor it was asked a runner for.
    fn recording_site() -> (GitHubSite, Arc<AtomicUsize>, Arc<Mutex<Vec<Actor>>>) {
        let sent = Arc::new(AtomicUsize::new(0));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (s, a) = (Arc::clone(&sent), Arc::clone(&asked));
        let make: MakeRunner = Arc::new(move |by: &Actor| {
            a.lock().unwrap().push(by.clone());
            Ok(Box::new(Sends(Arc::clone(&s))) as Box<dyn GhRunner>)
        });
        (GitHubSite::new("acme/widget", "", None, make), sent, asked)
    }

    const READ: Actor = Actor::Ours(Scope::Read);

    #[test]
    fn the_factory_s_read_scope_reads_the_code_host_through_its_own_runner() {
        let (site, sent, asked) = recording_site();
        let mut h = site.code_host(&READ, Caller::new("update")).unwrap();
        let _ = h.releases(tehas_ports::ALL_MAX);
        assert!(sent.load(Ordering::SeqCst) > 0);
        assert_eq!(*asked.lock().unwrap(), [READ]);
        assert!(h.remote(tehas_ports::Access::Fetch).is_ok());
        assert_eq!(
            h.remote(tehas_ports::Access::Push).unwrap_err(),
            Outcome::Forbidden
        );
        // The full scope's handle still pushes.
        let mut full = site
            .code_host(&Actor::Ours(Scope::Full), Caller::new("update"))
            .unwrap();
        assert!(full.remote(tehas_ports::Access::Push).is_ok());
    }

    #[test]
    fn the_factory_s_read_scope_writes_nothing_and_reads_no_other_port() {
        let (site, sent, asked) = recording_site();
        let c = Caller::new("update");
        assert_eq!(
            site.code_host_writes(&READ, c).err(),
            Some(Outcome::Forbidden)
        );
        assert_eq!(site.ticket_writes(&READ, c).err(), Some(Outcome::Forbidden));
        assert_eq!(
            site.blueprint_writes(&READ, c).err(),
            Some(Outcome::Forbidden)
        );
        let unsupported = Some(Outcome::Refused(tehas_ports::Refusal::Unsupported));
        assert_eq!(site.tickets(&READ, c).err(), unsupported);
        assert_eq!(site.blueprints(&READ, c).err(), unsupported);
        assert_eq!(site.board(&READ, c).err(), unsupported);
        assert_eq!(site.identity(&READ, c).err(), unsupported);
        assert_eq!(sent.load(Ordering::SeqCst), 0);
        assert!(asked.lock().unwrap().is_empty());
    }

    #[test]
    fn a_write_with_input_past_the_deadline_is_not_sent_nor_counted() {
        for until in [1_000, 999] {
            let (mut m, sent, meter) = metered(true, until);
            let why = m.run_input(&ARGS, "{\"body\":\"x\"}").unwrap_err();
            assert_eq!(why, NOT_STARTED);
            assert_eq!(sent.load(Ordering::SeqCst), 0);
            assert!(lock(&meter).is_empty());
            let mut gh = GitHub::new("acme/widget", Box::new(m));
            let e = gh.call_input("the write", &ARGS, "{}").unwrap_err();
            assert_eq!(
                gh.outcome(&e),
                Outcome::Unavailable {
                    why: tehas_ports::Passing::Timeout,
                    sent: tehas_ports::Sent::No,
                }
            );
            assert_eq!(sent.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn a_call_keeping_gh_s_answer_past_the_deadline_is_not_sent_nor_counted() {
        for until in [1_000, 999] {
            let (mut m, sent, meter) = metered(false, until);
            let failed = m.run_answer(&ARGS).unwrap_err();
            assert_eq!((failed.why.as_str(), failed.answer), (NOT_STARTED, None));
            assert_eq!(sent.load(Ordering::SeqCst), 0);
            assert!(lock(&meter).is_empty());
            let e = crate::ForgeError::new("the read", failed.why);
            let gh = GitHub::new("acme/widget", Box::new(m));
            assert_eq!(
                gh.outcome(&e),
                Outcome::Unavailable {
                    why: tehas_ports::Passing::Timeout,
                    sent: tehas_ports::Sent::No,
                }
            );
        }
    }

    #[test]
    fn before_the_deadline_each_way_sends_and_counts() {
        let (mut m, sent, meter) = metered(true, 1_001);
        assert!(m.run_input(&ARGS, "{}").is_ok());
        assert!(m.run_answer(&ARGS).is_ok());
        assert!(m.run(&ARGS).is_ok());
        assert_eq!(sent.load(Ordering::SeqCst), 3);
        assert_eq!(lock(&meter)[0].calls, 3);
    }
}
