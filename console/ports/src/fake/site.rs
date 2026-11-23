//! `FakeSite`: a `Site` over a `World`. Every write is checked as a site
//! would check it, applied, and recorded as a `Wrote` naming the handle's
//! account. Its handles count their calls in the site's meter, and log
//! each request they send in the world: every call that reaches it and
//! every mint. An answer made with no request logs nothing.

use std::sync::{Arc, Mutex};

use super::faulty::{IdentityOp, Op};
use super::state::{lock, CommentRec, ItemRec, State};
use super::world::{World, FACTORY, OPERATOR};
use super::MemoryTokens;
use crate::{
    Account, Actor, At, Blueprint, BlueprintNo, BlueprintWrites, Blueprints, Board, Bound, Caller,
    Cite, Cites, CodeHost, CodeHostWrites, Comment, CommentId, Covers, Depth, Got, Identity, Join,
    Login, Marker, MarkerEdit, MeterRow, Outcome, Pool, PortName, ProjectName, Realm, Ref, Refusal,
    Report, Reports, Revision, Scope, Secret, SignInCode, SignInState, Site, SiteId, Standing,
    TicketDigest, TicketKey, TicketWrites, Tickets, Token, TokenStore, Took, Unread, WebLink,
    Whole, Window, Write, ALL_MAX,
};

/// How long a token the fake mints is good for, in seconds.
pub const TOKEN_LIFE: i64 = 3600;
/// How many seconds a kept token must still run to be used: ten minutes.
pub const TOKEN_LEFT: u32 = 600;
/// How many characters of a text `Depth::Cut` keeps.
pub const CUT_TEXT: usize = 200;

/// A `Site` over a `World`. Its handles answer from the world and write to
/// it; a read handle has no write method to call. Minted tokens are kept in
/// its `TokenStore`.
#[derive(Clone)]
pub struct FakeSite {
    world: World,
    realms: Vec<Realm>,
    meter: Arc<Mutex<Vec<MeterRow>>>,
    tokens: Arc<dyn TokenStore>,
    keyless: bool,
    deadline: Option<At>,
}

impl FakeSite {
    /// A site of every realm of the world: the one site of a same-item
    /// setup.
    pub fn new(world: &World) -> Self {
        Self::of(world, vec![Realm::CodeHost, Realm::Tickets])
    }

    /// A site of one realm of the world: one of a split setup's two sites.
    pub fn realm(world: &World, realm: Realm) -> Self {
        Self::of(world, vec![realm])
    }

    fn of(world: &World, realms: Vec<Realm>) -> Self {
        Self {
            world: world.clone(),
            realms,
            meter: Arc::new(Mutex::new(Vec::new())),
            tokens: Arc::new(MemoryTokens::new(world.clock())),
            keyless: false,
            deadline: None,
        }
    }

    /// The same site, keeping its tokens in `tokens`.
    pub fn with_tokens(mut self, tokens: Arc<dyn TokenStore>) -> Self {
        self.tokens = tokens;
        self
    }

    /// The same site built without the factory's key: it mints nothing and
    /// answers the factory's tokens from its `TokenStore` alone.
    pub fn keyless(mut self) -> Self {
        self.keyless = true;
        self
    }

    /// The world it answers from.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The realm of its identity: the code host's where it serves it.
    fn own_realm(&self) -> Realm {
        if self.realms.contains(&Realm::CodeHost) {
            Realm::CodeHost
        } else {
            Realm::Tickets
        }
    }

    fn handle(&self, by: &Actor, caller: Caller, port: PortName, write: bool) -> Got<Handle> {
        let needs = match port {
            PortName::Tickets => Some(Realm::Tickets),
            PortName::Identity => None,
            _ => Some(Realm::CodeHost),
        };
        if let Some(realm) = needs {
            if !self.realms.contains(&realm) {
                return Err(Outcome::Refused(Refusal::Unsupported));
            }
        }
        let me = self.me(by)?;
        let scope = match by {
            Actor::Ours(scope) => Some(*scope),
            Actor::Given(key) => self.world.given_scope(key),
            _ => None,
        };
        may_make(by, scope, port, write)?;
        let realm = match port {
            PortName::Tickets => self.world.lock().ticket_realm(),
            PortName::Identity => self.own_realm(),
            _ => Realm::CodeHost,
        };
        Ok(Handle {
            world: self.world.clone(),
            realm,
            port,
            caller,
            actor: by.clone(),
            me,
            meter: Arc::clone(&self.meter),
            reports: Vec::new(),
            tokens: Arc::clone(&self.tokens),
            keyless: self.keyless,
            scope,
            told_signed_out: false,
            deadline: self.deadline,
        })
    }

    /// The login a handle acts as, or the outcome that stops its making.
    fn me(&self, by: &Actor) -> Got<Option<String>> {
        let world = &self.world;
        match by {
            Actor::Ours(_) => Ok(Some(FACTORY.to_string())),
            Actor::Operator => Ok(Some(OPERATOR.to_string())),
            Actor::Person(key) => {
                let s = world.lock();
                s.persons
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, login)| Some(login.clone()))
                    .ok_or(Outcome::SignedOut)
            }
            Actor::Given(key) => {
                let token = crate::Keyring::given(world, key);
                match token {
                    Some(t) if t.until > world.now() => Ok(Some(FACTORY.to_string())),
                    _ => {
                        world.given_signed_out(key);
                        Err(Outcome::SignedOut)
                    }
                }
            }
            Actor::Deploy(key) => match crate::Keyring::deploy(world, key) {
                Some(_) => Ok(None),
                None => Err(Outcome::SignedOut),
            },
            Actor::Anonymous => Ok(None),
        }
    }
}

/// What an actor may make a handle for, before any request. A given
/// token may do what the factory's of its scope may.
fn may_make(by: &Actor, scope: Option<Scope>, port: PortName, write: bool) -> Got<()> {
    let ok = match by {
        Actor::Anonymous => !write,
        Actor::Deploy(_) => !write && port != PortName::Identity,
        Actor::Ours(_) | Actor::Given(_) => scope.is_some_and(|scope| {
            matches!(
                (scope, port, write),
                (Scope::Full | Scope::Agent, _, _)
                    | (
                        Scope::Cycle | Scope::CycleMerge,
                        PortName::CodeHost | PortName::Tickets | PortName::Blueprints,
                        _
                    )
                    | (Scope::Session, PortName::CodeHost, _)
                    | (
                        Scope::Cycle | Scope::CycleMerge | Scope::Session | Scope::Read,
                        _,
                        false
                    )
                    | (Scope::Files, PortName::CodeHost | PortName::Identity, false)
            )
        }),
        Actor::Operator | Actor::Person(_) => true,
    };
    if ok {
        Ok(())
    } else {
        Err(Outcome::Forbidden)
    }
}

/// May `by`, acting with `scope`, call `op`: `Forbidden` where not. An
/// agent's token writes tickets, blueprints and a change's talk, never a
/// file, a merge, a tag or a release. A cycle's or a session's proposes,
/// opens, edits and comments on a change, and merges, tags and releases
/// nothing; a cycle's with the product's merge grant also merges its own
/// change (checked in the merge). A cycle's also files, notes and edits a
/// ticket and opens, comments on and edits a blueprint, as its relay does,
/// with text alone (`Ctx::text_alone`); a session's writes no ticket or
/// blueprint. Anonymous asks identity only about others. A person's
/// standing is checked over the world, in `Handle::write`.
pub(crate) fn allowed(by: &Actor, scope: Option<Scope>, op: Op) -> Got<()> {
    use super::faulty::{BlueprintsOp as B, CodeHostOp as C, IdentityOp as I, TicketsOp as T};
    let scoped = matches!(by, Actor::Ours(_) | Actor::Given(_));
    let ok = match op {
        Op::Identity(I::Me | I::Standing | I::Assignable | I::Token | I::TokenOver) => {
            !matches!(by, Actor::Anonymous)
        }
        Op::CodeHost(c) if scoped && op.is_write() => match scope {
            Some(Scope::Full) => true,
            Some(Scope::Agent) => matches!(
                c,
                C::OpenChange
                    | C::EditChange
                    | C::MarkChange
                    | C::UnmarkChange
                    | C::CommentChange
                    | C::CloseChange
            ),
            Some(Scope::Cycle | Scope::Session) => matches!(
                c,
                C::Propose | C::OpenChange | C::EditChange | C::CommentChange
            ),
            Some(Scope::CycleMerge) => matches!(
                c,
                C::Propose | C::OpenChange | C::EditChange | C::CommentChange | C::Merge
            ),
            _ => false,
        },
        _ if scoped && op.is_write() => match scope {
            Some(Scope::Full | Scope::Agent) => true,
            Some(Scope::Cycle | Scope::CycleMerge) => matches!(
                op,
                Op::Tickets(T::File | T::Note | T::Edit)
                    | Op::Blueprints(B::Open | B::Comment | B::Edit)
            ),
            _ => false,
        },
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(Outcome::Forbidden)
    }
}

/// May a person below `Write` standing make this write: a note, a
/// comment, a new ticket or blueprint, or an edit of their own.
fn below_write_may(op: Op) -> bool {
    use super::faulty::{BlueprintsOp as B, CodeHostOp as C, TicketsOp as T};
    matches!(
        op,
        Op::Tickets(T::Note | T::File | T::Edit)
            | Op::Blueprints(B::Open | B::Edit | B::Comment | B::EditComment)
            | Op::CodeHost(C::CommentChange)
    )
}

impl Site for FakeSite {
    fn realms(&self) -> &[Realm] {
        &self.realms
    }

    fn mark_name(&self, mark: &crate::Mark) -> String {
        mark_name(mark)
    }

    /// A clone that shares the meter, the tokens and the world.
    fn until(&self, until: At) -> Arc<dyn Site> {
        let mut site = self.clone();
        site.deadline = Some(until);
        Arc::new(site)
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

    fn code_host(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHost + Send>> {
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

    /// The default join over this site's own handles and the world's
    /// links. A site of one realm cannot join: `Refused(Unsupported)`.
    fn board(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Board + Send>> {
        if self.realms.len() < 2 {
            return Err(Outcome::Refused(Refusal::Unsupported));
        }
        Ok(Box::new(Join {
            tickets: self.tickets(by, caller)?,
            blueprints: self.blueprints(by, caller)?,
            code_host: self.code_host(by, caller)?,
            links: Arc::new(self.world.clone()),
        }))
    }

    fn meter(&self) -> Vec<MeterRow> {
        lock(&self.meter).clone()
    }
}

/// One handle of a `FakeSite`, of any port. The site boxes it as the
/// trait it asked for, so a read handle has no write method to call.
pub(crate) struct Handle {
    pub world: World,
    pub realm: Realm,
    pub port: PortName,
    pub caller: Caller,
    pub actor: Actor,
    pub me: Option<String>,
    meter: Arc<Mutex<Vec<MeterRow>>>,
    reports: Vec<Report>,
    tokens: Arc<dyn TokenStore>,
    keyless: bool,
    /// The scope it acts with: its own for `Ours`, its token's for `Given`.
    pub scope: Option<Scope>,
    told_signed_out: bool,
    /// At or past it, a call answers a timeout with no request.
    deadline: Option<At>,
}

/// What a call of a handle knows besides the world's state.
pub(crate) struct Ctx {
    pub now: At,
    pub realm: Realm,
    pub me: Option<String>,
    /// A person's handle below `Write` standing: it edits its own alone.
    pub below_write: bool,
    /// The scope the handle acts with, if the factory's.
    pub scope: Option<Scope>,
}

impl Ctx {
    /// `Forbidden` where a person below `Write` standing edits what
    /// another account wrote.
    pub fn own(&self, s: &State, author: Option<&Account>) -> Got<()> {
        if self.below_write && author.map(|a| &a.login) != Some(&self.me(s).login) {
            return Err(Outcome::Forbidden);
        }
        Ok(())
    }

    /// `Forbidden` where a cycle's ticket or blueprint write carries more
    /// than text (`bare` false): a marker, a mark, a kind, a priority, a
    /// target or a parent. Its relay passes a title, a body and an item
    /// alone.
    pub fn text_alone(&self, bare: bool) -> Got<()> {
        if !bare && matches!(self.scope, Some(Scope::Cycle | Scope::CycleMerge)) {
            return Err(Outcome::Forbidden);
        }
        Ok(())
    }

    /// The handle's account in its realm. A handle with none writes
    /// nothing, so a write asks for it after `allowed` let it be made.
    pub fn me(&self, s: &State) -> Account {
        s.account(self.me.as_deref().unwrap_or(FACTORY), self.realm)
    }

    /// Record a write the world applied.
    pub fn wrote(&self, s: &mut State, what: Write) {
        let by = self.me(s);
        s.wrote.push(super::Wrote { by, what });
    }
}

impl Handle {
    /// Count one call in the meter, run it over the world, and keep its
    /// outcome for the reports.
    pub fn run<T>(&mut self, pool: Pool, f: impl FnOnce(&mut State, &Ctx) -> Got<T>) -> Got<T> {
        if let Some(o) = self.past_deadline() {
            self.report(o.clone());
            return Err(o);
        }
        self.count(pool);
        let answer = {
            let mut s = self.world.lock();
            s.sent.push(self.actor.clone());
            let ctx = self.ctx(&s);
            match self.stopped(&s) {
                Some(o) => Err(o),
                None => f(&mut s, &ctx),
            }
        };
        self.signed_out_once(&answer);
        if let Err(o) = &answer {
            self.report(o.clone());
        }
        answer
    }

    /// A write, checked as the site checks it: by the actor's scope and
    /// standing, then by an archived project's `Refused(ReadOnly)`.
    pub fn write<T>(&mut self, op: Op, f: impl FnOnce(&mut State, &Ctx) -> Got<T>) -> Got<T> {
        let actor = self.actor.clone();
        let scope = self.scope;
        self.run(Pool::Writes, move |s, ctx| {
            allowed(&actor, scope, op)?;
            if ctx.below_write && !below_write_may(op) {
                return Err(Outcome::Forbidden);
            }
            if s.archived {
                return Err(Outcome::Refused(Refusal::ReadOnly));
            }
            f(s, ctx)
        })
    }

    fn ctx(&self, s: &State) -> Ctx {
        let below_write = matches!(self.actor, Actor::Person(_))
            && self.me.as_deref().is_some_and(|login| {
                s.accounts
                    .iter()
                    .find(|a| a.login == login)
                    .is_none_or(|a| a.standing < Standing::Write)
            });
        Ctx {
            now: self.world.now(),
            realm: self.realm,
            me: self.me.clone(),
            below_write,
            scope: self.scope,
        }
    }

    /// What the site answers every call of this handle, whatever it asks:
    /// a credential the site forgot is `SignedOut`, an anonymous read of a
    /// private project or a ticket of a tracker turned off `NotFound`.
    fn stopped(&self, s: &State) -> Option<Outcome> {
        let gone = match &self.actor {
            Actor::Person(key) => !s.persons.iter().any(|(k, _)| k == key),
            Actor::Deploy(key) => !s.deploys.iter().any(|(k, _)| k == key),
            Actor::Given(key) => !s.given.iter().any(|(k, _, _)| k == key),
            _ => false,
        };
        if gone {
            Some(Outcome::SignedOut)
        } else if (matches!(self.actor, Actor::Anonymous) && !s.public)
            || (self.port == PortName::Tickets && !s.tracker_on)
        {
            Some(Outcome::NotFound)
        } else {
            None
        }
    }

    /// `Forbidden` with no request where an anonymous handle asks identity
    /// about itself or the project: it has no account to ask about.
    fn anonymous_may(&mut self, op: IdentityOp) -> Got<()> {
        let answer = allowed(&self.actor, self.scope, Op::Identity(op));
        if let Err(o) = &answer {
            self.report(o.clone());
        }
        answer
    }

    /// A person's handle signed out tells the keyring once.
    fn signed_out_once<T>(&mut self, answer: &Got<T>) {
        if let (Err(Outcome::SignedOut), Actor::Person(key)) = (answer, &self.actor) {
            if !self.told_signed_out {
                self.told_signed_out = true;
                crate::Keyring::signed_out(&self.world, key);
            }
        }
    }

    /// A read of several things, each with its own answer.
    pub fn each<K: Clone, T>(
        &mut self,
        keys: &[K],
        mut f: impl FnMut(&mut State, &Ctx, &K) -> Got<T>,
    ) -> Vec<(K, Got<T>)> {
        if let Some(o) = self.past_deadline() {
            self.report(o.clone());
            return keys.iter().map(|k| (k.clone(), Err(o.clone()))).collect();
        }
        self.count(Pool::Calls);
        let answers: Vec<(K, Got<T>)> = {
            let mut s = self.world.lock();
            s.sent.push(self.actor.clone());
            let ctx = self.ctx(&s);
            let stopped = self.stopped(&s);
            keys.iter()
                .map(|k| match &stopped {
                    Some(o) => (k.clone(), Err(o.clone())),
                    None => (k.clone(), f(&mut s, &ctx, k)),
                })
                .collect()
        };
        for (_, a) in &answers {
            if let Err(o) = a {
                self.report(o.clone());
            }
        }
        answers
    }

    /// The timeout a call past the handle's deadline answers, with no
    /// request and nothing metered.
    fn past_deadline(&self) -> Option<Outcome> {
        self.deadline
            .is_some_and(|d| self.world.now() >= d)
            .then_some(Outcome::Unavailable {
                why: crate::Passing::Timeout,
                sent: crate::Sent::No,
            })
    }

    fn count(&self, pool: Pool) {
        let mut meter = lock(&self.meter);
        match meter
            .iter_mut()
            .find(|r| r.caller == self.caller && r.port == self.port && r.pool == pool)
        {
            Some(row) => {
                row.calls += 1;
                row.points += 1;
            }
            None => meter.push(MeterRow {
                caller: self.caller,
                port: self.port,
                pool,
                calls: 1,
                points: 1,
            }),
        }
    }

    fn report(&mut self, outcome: Outcome) {
        match self.reports.iter_mut().find(|r| r.outcome == outcome) {
            Some(r) => r.calls += 1,
            None => self.reports.push(Report {
                port: self.port,
                outcome,
                calls: 1,
            }),
        }
    }

    pub fn project(&self) -> ProjectName {
        self.world.project()
    }

    fn covers(&self) -> Covers {
        Covers::Site(SiteId::new(self.project().as_str()))
    }

    /// A kept token of the factory's, or a new one kept for the next
    /// caller. A keyless site mints none: none kept is `SignedOut`. Only a
    /// mint is a request, counted in the meter and logged in the world.
    fn token_for(&self, covers: &Covers, scope: Scope, now: At) -> Got<Token> {
        if let Some(t) = self.tokens.kept(covers, scope, TOKEN_LEFT) {
            return Ok(t);
        }
        if self.keyless {
            return Err(Outcome::SignedOut);
        }
        self.count(Pool::Calls);
        let n = {
            let mut s = self.world.lock();
            s.sent.push(self.actor.clone());
            s.fresh_id("minted")
        };
        let token = Token {
            secret: Secret::new(n),
            until: At::new(now.secs() + TOKEN_LIFE),
        };
        // A token that cannot be kept is given once, kept nowhere.
        let _kept = self.tokens.keep(covers, scope, &token);
        Ok(token)
    }

    /// May this handle's actor have a token of `scope`: its own scope, or
    /// any for `Full`.
    fn scope_ok(&self, scope: Scope) -> bool {
        match &self.actor {
            Actor::Ours(own) => covers(*own, scope),
            _ => false,
        }
    }
}

/// Whether a token of scope `own` may answer for `asked`: a full token for
/// any known scope, any other only for itself.
fn covers(own: Scope, asked: Scope) -> bool {
    match own {
        Scope::Full => asked != Scope::Unknown,
        own => own == asked,
    }
}

impl Reports for Handle {
    fn take_reports(&mut self) -> Vec<Report> {
        std::mem::take(&mut self.reports)
    }
}

impl Cites for Handle {
    fn cite(&self, what: &Cite) -> String {
        fn named<T>(word: &str, r: &Ref<T>, here: impl Fn(&T) -> String) -> String {
            match r {
                Ref::Here(t) => format!("{word} {}", here(t)),
                Ref::Elsewhere { project, item } => {
                    format!("{word} {} of {}", item.as_str(), project.as_str())
                }
            }
        }
        match what {
            Cite::Ticket(r) => named("ticket", r, key_text),
            Cite::Blueprint(r) => named("blueprint", r, |n| n.get().to_string()),
            Cite::Change(r) => named("change", r, |n| n.get().to_string()),
            Cite::Account(login) => format!("account {}", login.as_str()),
            Cite::Marker(m) => match marker_text(m) {
                text if text.is_empty() => text,
                text => format!("`{text}`"),
            },
            Cite::Mark(m) => match mark_name(m) {
                name if name.is_empty() => name,
                name => format!("`{name}`"),
            },
        }
    }
}

/// The text a person types to make a marker on the fake: the marker as
/// Rust writes it for debugging, such as `HoldCleared`; `""` for one that is
/// not writable. The fake reads no marker from text, so it is never live.
pub fn marker_text(marker: &crate::Marker) -> String {
    if marker.writable() {
        format!("{marker:?}")
    } else {
        String::new()
    }
}

/// The fake's word for a mark: its group and its name, lower case, such as
/// `act:approve` or `mark:epic`; a project's own mark by its name; `""` for a word this
/// copy does not know.
pub(crate) fn mark_name(mark: &crate::Mark) -> String {
    use crate::{Act, Choice, Mark, State};
    let word = |group: &str, w: String| format!("{group}:{}", w.to_ascii_lowercase());
    match mark {
        Mark::Act(Act::Unknown)
        | Mark::State(State::Unknown)
        | Mark::Choice(Choice::Unknown)
        | Mark::Unknown => String::new(),
        Mark::Act(a) => word("act", format!("{a:?}")),
        Mark::State(s) => word("state", format!("{s:?}")),
        Mark::Choice(c) => word("choice", format!("{c:?}")),
        Mark::Epic => word("mark", "Epic".into()),
        Mark::Project(name) => name.as_str().into(),
    }
}

/// The fake's id of a mark: its word behind `id:`, None for a mark that
/// names no word.
pub(crate) fn mark_id(mark: &crate::Mark) -> Option<crate::MarkId> {
    let name = mark_name(mark);
    (!name.is_empty()).then(|| crate::MarkId::new(format!("id:{name}")))
}

impl Identity for Handle {
    fn me(&mut self) -> Got<Account> {
        self.anonymous_may(IdentityOp::Me)?;
        self.run(Pool::Calls, |s, ctx| match &ctx.me {
            Some(login) => Ok(s.account(login, ctx.realm)),
            None => Err(Outcome::NotFound),
        })
    }

    fn standing(&mut self, who: &crate::AccountId) -> Got<Standing> {
        self.anonymous_may(IdentityOp::Standing)?;
        self.run(Pool::Calls, |s, ctx| {
            s.login_of(who, ctx.realm)
                .map(|a| a.standing)
                .ok_or(Outcome::NotFound)
        })
    }

    fn account(&mut self, who: &crate::AccountId) -> Got<Account> {
        self.run(Pool::Calls, |s, ctx| {
            let login = s
                .login_of(who, ctx.realm)
                .ok_or(Outcome::NotFound)?
                .login
                .clone();
            Ok(s.account(&login, ctx.realm))
        })
    }

    fn account_by_login(&mut self, login: &Login) -> Got<Account> {
        self.run(Pool::Calls, |s, ctx| {
            let found = s
                .accounts
                .iter()
                .find(|a| a.login.eq_ignore_ascii_case(login.as_str()) && !a.organisation)
                .map(|a| a.login.clone())
                .ok_or(Outcome::NotFound)?;
            Ok(s.account(&found, ctx.realm))
        })
    }

    fn is_organisation(&mut self, name: &Login) -> Got<bool> {
        self.run(Pool::Calls, |s, _| {
            s.accounts
                .iter()
                .find(|a| a.login.eq_ignore_ascii_case(name.as_str()))
                .map(|a| a.organisation)
                .ok_or(Outcome::NotFound)
        })
    }

    fn assignable(&mut self, who: &crate::AccountId) -> Got<bool> {
        self.anonymous_may(IdentityOp::Assignable)?;
        self.run(Pool::Calls, |s, ctx| {
            s.login_of(who, ctx.realm)
                .map(|a| a.assignable)
                .ok_or(Outcome::NotFound)
        })
    }

    /// The factory's token for this project: kept, or minted and kept. A
    /// given token is answered as it was handed in, never minted. Only a
    /// mint is a request; every other answer is made with no request.
    fn token(&mut self, scope: Scope) -> Got<Token> {
        let now = self.world.now();
        let answer = match self.actor.clone() {
            Actor::Given(key) => match crate::Keyring::given(&self.world, &key) {
                Some(t) if t.until <= now => {
                    self.world.given_signed_out(&key);
                    Err(Outcome::SignedOut)
                }
                // A given token answers for its own scope alone: a wider
                // one is `Forbidden`, as `token_over` refuses, with no
                // request.
                Some(t) => match self.world.given_scope(&key) {
                    Some(own) if covers(own, scope) => Ok(t),
                    _ => Err(Outcome::Forbidden),
                },
                None => {
                    self.world.given_signed_out(&key);
                    Err(Outcome::SignedOut)
                }
            },
            Actor::Ours(_) if self.scope_ok(scope) => self.token_for(&self.covers(), scope, now),
            _ => Err(Outcome::Forbidden),
        };
        if let Err(o) = &answer {
            self.report(o.clone());
        }
        answer
    }

    /// One token over a set of projects of the factory's grant, this
    /// project among them. A project outside the grant, or a set without
    /// this project, is `Forbidden` before any mint.
    fn token_over(&mut self, scope: Scope, projects: &[ProjectName]) -> Got<Token> {
        let now = self.world.now();
        let (grant, covered) = {
            let s = self.world.lock();
            (s.grant.clone(), s.grant_projects.clone())
        };
        let own = self.project();
        let answer = if !matches!(self.actor, Actor::Ours(_))
            || !self.scope_ok(scope)
            || !projects.contains(&own)
            || projects.iter().any(|p| !covered.contains(p))
        {
            Err(Outcome::Forbidden)
        } else {
            match Covers::projects(grant, projects.to_vec()) {
                Ok(covers) => self.token_for(&covers, scope, now),
                Err(_) => Err(Outcome::Forbidden),
            }
        };
        if let Err(o) = &answer {
            self.report(o.clone());
        }
        answer
    }

    fn sign_in_link(&self, state: &SignInState) -> WebLink {
        web(&format!("sign-in?state={}", escaped(state.as_str())))
    }

    /// A code good once: used again, or never handed out, is
    /// `Refused(Invalid)`.
    fn signed_in(&mut self, code: &SignInCode) -> Got<(Account, Secret)> {
        self.run(Pool::Calls, |s, ctx| {
            let at = s
                .sign_ins
                .iter()
                .position(|(c, _, used)| c == code && !used)
                .ok_or(Outcome::Refused(Refusal::Invalid))?;
            s.sign_ins[at].2 = true;
            let login = s.sign_ins[at].1.clone();
            let secret = Secret::new(format!("secret-{login}"));
            Ok((s.account(&login, ctx.realm), secret))
        })
    }
}

/// A link on the fake's site.
pub(crate) fn web(path: &str) -> WebLink {
    WebLink::new(format!("https://fake.test/{path}"))
        .unwrap_or_else(|_| WebLink::new("https://fake.test/").expect("a fixed link"))
}

/// A text as a link may carry it: letters and digits as they are, every
/// other byte as `%` and two hex digits.
pub(crate) fn escaped(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// A ticket's key as text: its number, or its key.
pub(crate) fn key_text(key: &TicketKey) -> String {
    match key {
        TicketKey::Number(n) => n.to_string(),
        TicketKey::Key(k) => k.as_str().to_string(),
    }
}

/// The order of a ticket list's ties: numbers before keys, each in order.
pub(crate) fn key_order(key: &TicketKey) -> (u8, u64, String) {
    match key {
        TicketKey::Number(n) => (0, n.get(), String::new()),
        TicketKey::Key(k) => {
            let n = k
                .as_str()
                .rsplit('-')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(u64::MAX);
            (1, n, k.as_str().to_string())
        }
    }
}

/// The list, whole up to its bound, or `Unread(Cut)` past it.
pub(crate) fn whole<T>(items: Vec<T>, bound: Bound) -> Got<Whole<T>> {
    if items.len() > bound.get() as usize {
        Err(Outcome::Unread(Unread::Cut))
    } else {
        Ok(Whole::new(items))
    }
}

/// As much of a list, oldest first, as a window takes.
pub(crate) fn window<T: Clone>(items: &[T], w: Window) -> Got<Took<T>> {
    match w {
        Window::Whole(bound) => Ok(Took {
            items: whole(items.to_vec(), bound)?.into_items(),
            more: false,
        }),
        Window::Last(n) => {
            let n = n.get() as usize;
            let skip = items.len().saturating_sub(n);
            Ok(Took {
                items: items[skip..].to_vec(),
                more: skip > 0,
            })
        }
    }
}

/// The first `n` of a list in its order, with `more`.
pub(crate) fn first<T>(mut items: Vec<T>, n: u32) -> Took<T> {
    let n = n as usize;
    let more = items.len() > n;
    items.truncate(n);
    Took { items, more }
}

/// A text cut to `CUT_TEXT` characters, as `Depth::Cut` reads it.
pub(crate) fn cut(text: &str) -> String {
    text.chars().take(CUT_TEXT).collect()
}

/// A revision as the ports carry it.
pub(crate) fn revision(n: u64) -> Revision {
    Revision::new(format!("r{n}"))
}

/// A comment as the ports read it, its text cut when `cut_text`.
pub(crate) fn comment(c: &CommentRec, cut_text: bool) -> Comment {
    Comment {
        id: c.id.clone(),
        author: c.author.clone(),
        editor: c.editor.clone(),
        created: c.created,
        edited: c.edited,
        body: if cut_text {
            cut(&c.body)
        } else {
            c.body.clone()
        },
        markers: c.markers.clone(),
        revision: revision(c.revision),
        link: c.link.clone(),
    }
}

/// The comments of an item, not deleted, oldest first.
pub(crate) fn comments_of(s: &State, ids: &[CommentId], cut_text: bool) -> Vec<Comment> {
    ids.iter()
        .filter_map(|id| s.comment_at(id))
        .map(|at| &s.comments[at])
        .filter(|c| !c.gone)
        .map(|c| comment(c, cut_text))
        .collect()
}

/// The comments a depth takes, oldest first.
pub(crate) fn comments_at_depth(s: &State, ids: &[CommentId], depth: Depth) -> Got<Took<Comment>> {
    match depth {
        Depth::Cut { notes } => {
            let all = comments_of(s, ids, true);
            let skip = all.len().saturating_sub(usize::from(notes));
            Ok(Took {
                more: skip > 0,
                items: all.into_iter().skip(skip).collect(),
            })
        }
        Depth::Last { notes } => window(&comments_of(s, ids, false), Window::Last(notes)),
        Depth::All => window(&comments_of(s, ids, false), Window::Whole(ALL_MAX)),
    }
}

/// A blueprint as the ports read it.
pub(crate) fn blueprint(s: &State, item: &ItemRec, depth: Depth) -> Got<Blueprint> {
    let no = item.blueprint.ok_or(Outcome::NotFound)?;
    Ok(Blueprint {
        no,
        title: item.title.clone(),
        body: item.text.clone(),
        open: item.open,
        author: item.author.clone(),
        editor: item.editor.clone(),
        created: item.created,
        edited: item.edited,
        revision: revision(item.revision),
        markers: item.markers.clone(),
        comments: comments_at_depth(s, &item.comments, depth)?,
        link: s.blueprint_link(no),
    })
}

/// A ticket's fields.
pub(crate) fn digest(s: &State, item: &ItemRec) -> Option<TicketDigest> {
    let key = item.ticket.clone()?;
    let state = match (&item.open, &item.reason) {
        (true, _) => crate::TicketState::Open,
        (false, reason) => {
            crate::TicketState::Closed(reason.clone().unwrap_or(crate::CloseReason::Done))
        }
    };
    let target = item.target.as_ref().map(|id| crate::TargetName {
        id: id.clone(),
        title: s
            .targets
            .iter()
            .find(|t| t.id == *id)
            .map(|t| t.title.clone())
            .unwrap_or_default(),
    });
    Some(TicketDigest {
        link: s.ticket_link(&key),
        key,
        title: item.title.clone(),
        state,
        kind: item.kind.clone(),
        priority: item.priority,
        marks: item.marks.clone(),
        target,
        author: item.author.clone(),
        created: item.created,
        updated: item.updated,
        revision: revision(item.revision),
    })
}

/// Tickets in list order: oldest filed first, ties broken by key.
pub(crate) fn list_order(digests: &mut [TicketDigest]) {
    digests.sort_by_key(|d| (d.created, key_order(&d.key)));
}

/// Do the words all stand in the text, ASCII case folded? No word matches
/// everything.
pub(crate) fn matches(words: &str, text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    let mut any = false;
    for w in words.split_whitespace() {
        any = true;
        if !text.contains(&w.to_ascii_lowercase()) {
            return false;
        }
    }
    any
}

/// A body as a port method takes it: the text, never a sum.
pub(crate) fn text_of(body: &crate::Body) -> Got<&str> {
    match body {
        crate::Body::Text(t) => Ok(t),
        crate::Body::Summed(_) => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

/// `Refused(Invalid)` when a marker of a write may not be written.
pub(crate) fn writable(markers: &[Marker]) -> Got<()> {
    if markers.iter().all(Marker::writable) {
        Ok(())
    } else {
        Err(Outcome::Refused(Refusal::Invalid))
    }
}

/// `Refused(Invalid)` for a mark this copy does not know.
pub(crate) fn known_mark(mark: &crate::Mark) -> Got<()> {
    if *mark == crate::Mark::Unknown {
        Err(Outcome::Refused(Refusal::Invalid))
    } else {
        Ok(())
    }
}

/// Apply a `MarkerEdit`: a marker set replaces a readable one of its kind
/// and key, or is added at the end; one named in `drop` is taken off; an
/// unreadable or bare marker is kept where it stands.
pub(crate) fn edit_markers(markers: &mut Vec<Marker>, edit: &MarkerEdit) {
    let readable = |m: &Marker| !matches!(m, Marker::Unreadable { .. } | Marker::Bare { .. });
    markers.retain(|m| {
        !(readable(m)
            && edit
                .drop
                .iter()
                .any(|(k, key)| m.kind() == *k && m.key() == *key))
    });
    for set in &edit.set {
        match markers
            .iter()
            .position(|m| readable(m) && m.kind() == set.kind() && m.key() == set.key())
        {
            Some(at) => markers[at] = set.clone(),
            None => markers.push(set.clone()),
        }
    }
}

/// The world holds no blueprint of this number.
pub(crate) fn no_blueprint(s: &State, no: BlueprintNo) -> Got<usize> {
    s.blueprint_at(no).ok_or(Outcome::NotFound)
}
