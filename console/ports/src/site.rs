//! The adapter's front: `Site`, which makes one realm's handles, its
//! meter's line, `MeterRow`, and `Setup` and `Split`, one site or two sites
//! as one setup.

use std::sync::Arc;

use serde::Serialize;

use crate::{
    Actor, BlueprintWrites, Blueprints, Board, Caller, CodeHost, CodeHostWrites, Got, Identity,
    Join, Links, Outcome, Pool, PortName, Realm, Refusal, TicketWrites, Tickets,
};

/// One realm's supplier for one project. A port it does not serve answers
/// `Refused(Unsupported)` when its handle is asked for. Making a handle
/// can fail before any request: a person whose credential the keyring no
/// longer has, or a given token past its time, is `SignedOut`, and a write
/// handle asked as `Anonymous` is `Forbidden`.
pub trait Site: Send + Sync {
    /// The realms this site serves: a code host serves both where tickets
    /// come from its own tracker.
    fn realms(&self) -> &[Realm];
    /// A tickets read handle.
    fn tickets(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Tickets + Send>>;
    /// A tickets write handle.
    fn ticket_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn TicketWrites + Send>>;
    /// A blueprints read handle.
    fn blueprints(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Blueprints + Send>>;
    /// A blueprints write handle.
    fn blueprint_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn BlueprintWrites + Send>>;
    /// A code host read handle.
    fn code_host(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHost + Send>>;
    /// A code host write handle.
    fn code_host_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHostWrites + Send>>;
    /// The identity of this site's realm.
    fn identity(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Identity + Send>>;
    /// The joined reads. A site that serves all three ports answers it with
    /// one question and owns its cost; a site that does not answers
    /// `Refused(Unsupported)`.
    fn board(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Board + Send>>;
    /// Calls and points by caller, port and pool, since the site was made.
    fn meter(&self) -> Vec<MeterRow>;
}

/// One line of the meter. `Serialize` only: `Caller` is a `&'static str`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct MeterRow {
    /// Who made the calls.
    pub caller: Caller,
    /// The port they were to.
    pub port: PortName,
    /// The budget they drew on.
    pub pool: Pool,
    /// How many calls.
    pub calls: u32,
    /// How many points the site counted for them.
    pub points: u32,
}

/// A setup: one site, or a code host's site and a tickets site. The one
/// thing a program's wiring hands its callers.
pub enum Setup {
    /// One site serves every port.
    One(Arc<dyn Site>),
    /// A code host's site and a tickets site.
    Split(Split),
}

/// Two sites as one setup, and the links between their items.
pub struct Split {
    /// The site of the code host realm: blueprints and the code host.
    pub code_host: Arc<dyn Site>,
    /// The site of the ticket realm.
    pub tickets: Arc<dyn Site>,
    /// Which blueprints each ticket has.
    pub links: Arc<dyn Links>,
}

impl Setup {
    fn site_of(&self, realm: Realm) -> &dyn Site {
        match (self, realm) {
            (Setup::One(site), _) => site.as_ref(),
            (Setup::Split(split), Realm::Tickets) => split.tickets.as_ref(),
            (Setup::Split(split), Realm::CodeHost) => split.code_host.as_ref(),
        }
    }

    /// A tickets read handle, from the site of the ticket realm.
    pub fn tickets(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Tickets + Send>> {
        self.site_of(Realm::Tickets).tickets(by, caller)
    }

    /// A tickets write handle, from the site of the ticket realm.
    pub fn ticket_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn TicketWrites + Send>> {
        self.site_of(Realm::Tickets).ticket_writes(by, caller)
    }

    /// A blueprints read handle, from the code host's site.
    pub fn blueprints(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Blueprints + Send>> {
        self.site_of(Realm::CodeHost).blueprints(by, caller)
    }

    /// A blueprints write handle, from the code host's site.
    pub fn blueprint_writes(
        &self,
        by: &Actor,
        caller: Caller,
    ) -> Got<Box<dyn BlueprintWrites + Send>> {
        self.site_of(Realm::CodeHost).blueprint_writes(by, caller)
    }

    /// A code host read handle, from the code host's site.
    pub fn code_host(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHost + Send>> {
        self.site_of(Realm::CodeHost).code_host(by, caller)
    }

    /// A code host write handle, from the code host's site.
    pub fn code_host_writes(
        &self,
        by: &Actor,
        caller: Caller,
    ) -> Got<Box<dyn CodeHostWrites + Send>> {
        self.site_of(Realm::CodeHost).code_host_writes(by, caller)
    }

    /// The identity of a realm, from that realm's site. A realm the site
    /// does not serve is `Refused(Unsupported)`.
    pub fn identity(
        &self,
        realm: Realm,
        by: &Actor,
        caller: Caller,
    ) -> Got<Box<dyn Identity + Send>> {
        let site = self.site_of(realm);
        if !site.realms().contains(&realm) {
            return Err(Outcome::Refused(Refusal::Unsupported));
        }
        site.identity(by, caller)
    }

    /// `One`: the site's own board. `Split`: a `Join` over handles of both.
    pub fn board(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Board + Send>> {
        match self {
            Setup::One(site) => site.board(by, caller),
            Setup::Split(split) => Ok(Box::new(Join {
                tickets: split.tickets.tickets(by, caller)?,
                blueprints: split.code_host.blueprints(by, caller)?,
                code_host: split.code_host.code_host(by, caller)?,
                links: Arc::clone(&split.links),
            })),
        }
    }
}
