//! The handle factory, `Site`: [`GitHubSite`] makes the handles of one
//! repository, each a [`GitHub`] over a runner the wiring makes for the
//! actor, and carrying the factory's own account, so an account that is
//! the factory's answers `Ours`. The adapter never obtains a token: the
//! runner it is given is how it reaches gh.

use std::sync::{Arc, Mutex};

use super::unsupported;
use crate::tickets::{Bot, NOT_STARTED};
use crate::{table, GitHub};
use tehas_core::forge::{Failed, GhRunner};
use tehas_ports::{
    Actor, At, BlueprintWrites, Blueprints, Board, Caller, CodeHost, CodeHostWrites, Got, Identity,
    Mark, MeterRow, Outcome, Pool, PortName, Realm, Scope, Site, TicketWrites, Tickets,
};

/// What makes a runner of gh for an actor: the wiring's, so the adapter
/// never holds a token or a credential. It is asked only for an actor the
/// site hands handles to, `Actor::Ours(Scope::Full)` and `Actor::Operator`.
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
    /// or keep yet, and handing them the full runner would widen them.
    /// Anonymous asking a write handle is `Forbidden`, as the trait says.
    fn handle(&self, by: &Actor, caller: Caller, port: PortName, write: bool) -> Got<GitHub> {
        match by {
            Actor::Ours(Scope::Full) | Actor::Operator => {}
            Actor::Anonymous if write => return Err(Outcome::Forbidden),
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
        Ok(gh)
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

    /// Not built in this adapter yet.
    fn code_host(&self, _: &Actor, _: Caller) -> Got<Box<dyn CodeHost + Send>> {
        unsupported()
    }

    /// Not built in this adapter yet.
    fn code_host_writes(&self, _: &Actor, _: Caller) -> Got<Box<dyn CodeHostWrites + Send>> {
        unsupported()
    }

    fn identity(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Identity + Send>> {
        Ok(Box::new(self.handle(
            by,
            caller,
            PortName::Identity,
            false,
        )?))
    }

    /// Not built in this adapter yet: the joined read waits on blueprints
    /// and the code host.
    fn board(&self, _: &Actor, _: Caller) -> Got<Box<dyn Board + Send>> {
        unsupported()
    }

    fn meter(&self) -> Vec<MeterRow> {
        lock(&self.meter).clone()
    }

    fn mark_name(&self, mark: &Mark) -> String {
        table::mark_name(mark)
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
        let mut meter = lock(&self.meter);
        let row = meter
            .iter_mut()
            .find(|r| r.caller == self.caller && r.port == self.port && r.pool == pool);
        match row {
            Some(row) => {
                row.calls = row.calls.saturating_add(1);
                row.points = row.points.saturating_add(1);
            }
            None => {
                let mut row = MeterRow::new(self.caller, self.port, pool);
                row.calls = 1;
                row.points = 1;
                meter.push(row);
            }
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
