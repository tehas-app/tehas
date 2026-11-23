//! The handle factory, `Site`: [`GitHubSite`] makes the handles of one
//! repository, each a [`GitHub`] over a runner the wiring makes for the
//! actor, and carrying the factory's own account, so an account that is
//! the factory's answers `Ours`. The adapter never obtains a token: the
//! runner it is given is how it reaches gh.

use std::sync::{Arc, Mutex};

use super::unsupported;
use crate::tickets::Bot;
use crate::{table, GitHub};
use tehas_core::forge::{Failed, GhRunner};
use tehas_ports::{
    Actor, BlueprintWrites, Blueprints, Board, Caller, CodeHost, CodeHostWrites, Got, Identity,
    Mark, MeterRow, Outcome, Pool, PortName, Realm, Scope, Site, TicketWrites, Tickets,
};

/// What makes a runner of gh for an actor: the wiring's, so the adapter
/// never holds a token or a credential. It is asked only for an actor the
/// site hands handles to, `Actor::Ours(Scope::Full)` and `Actor::Operator`.
pub type MakeRunner = Arc<dyn Fn(&Actor) -> Got<Box<dyn GhRunner>> + Send + Sync>;

/// GitHub as a `Site`: the code host and its own tracker, both realms.
pub struct GitHubSite {
    repo: String,
    bot: Bot,
    make: MakeRunner,
    meter: Arc<Mutex<Vec<MeterRow>>>,
}

impl GitHubSite {
    /// The site of `repo` (`owner/name`), whose factory account is the bot
    /// of `bot_login`, in any of the App's spellings, and whose runners
    /// `make` gives. The caller reads the login; the adapter reads no file
    /// and no variable.
    pub fn new(repo: impl Into<String>, bot_login: &str, make: MakeRunner) -> GitHubSite {
        GitHubSite {
            repo: repo.into(),
            bot: Bot::new(bot_login),
            make,
            meter: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The same site knowing the bot's user id: an account then answers
    /// `Ours` only where it carries that id as well as the bot's login.
    pub fn with_bot_id(mut self, id: u64) -> GitHubSite {
        self.bot.id = Some(id);
        self
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
            pool: if write { Pool::Writes } else { Pool::Calls },
        };
        let mut gh = GitHub::new(self.repo.clone(), Box::new(runner));
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

    /// Not built in this adapter yet.
    fn blueprints(&self, _: &Actor, _: Caller) -> Got<Box<dyn Blueprints + Send>> {
        unsupported()
    }

    /// Not built in this adapter yet.
    fn blueprint_writes(&self, _: &Actor, _: Caller) -> Got<Box<dyn BlueprintWrites + Send>> {
        unsupported()
    }

    /// Not built in this adapter yet.
    fn code_host(&self, _: &Actor, _: Caller) -> Got<Box<dyn CodeHost + Send>> {
        unsupported()
    }

    /// Not built in this adapter yet.
    fn code_host_writes(&self, _: &Actor, _: Caller) -> Got<Box<dyn CodeHostWrites + Send>> {
        unsupported()
    }

    /// Not built in this adapter yet.
    fn identity(&self, _: &Actor, _: Caller) -> Got<Box<dyn Identity + Send>> {
        unsupported()
    }

    /// Not built in this adapter yet: the joined read waits on blueprints
    /// and the code host.
    fn board(&self, _: &Actor, _: Caller) -> Got<Box<dyn Board + Send>> {
        unsupported()
    }

    fn meter(&self) -> Vec<MeterRow> {
        self.meter.lock().map(|m| m.clone()).unwrap_or_default()
    }

    fn mark_name(&self, mark: &Mark) -> String {
        table::mark_name(mark)
    }
}

/// A runner that counts each call it sends in the site's meter: one call
/// and one point, as GitHub's REST budget counts a request.
struct Metered {
    inner: Box<dyn GhRunner>,
    meter: Arc<Mutex<Vec<MeterRow>>>,
    caller: Caller,
    port: PortName,
    pool: Pool,
}

impl Metered {
    fn count(&self) {
        let Ok(mut meter) = self.meter.lock() else {
            return;
        };
        let row = meter
            .iter_mut()
            .find(|r| r.caller == self.caller && r.port == self.port && r.pool == self.pool);
        match row {
            Some(row) => {
                row.calls += 1;
                row.points += 1;
            }
            None => {
                let mut row = MeterRow::new(self.caller, self.port, self.pool);
                row.calls = 1;
                row.points = 1;
                meter.push(row);
            }
        }
    }
}

impl GhRunner for Metered {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.count();
        self.inner.run(args)
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        self.count();
        self.inner.run_input(args, input)
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        self.count();
        self.inner.run_answer(args)
    }
}
