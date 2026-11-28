//! `Faulty`: wraps any site and answers chosen calls with an outcome, and
//! runs the same health machine as an adapter, so it raises the same
//! notices.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::site::FakeSite;
use super::state::lock;
use super::world::{Clock, World};
use crate::{
    health, Access, AccountId, Actor, Applied, At, Attachment, AttachmentId, Blobs, Blocker,
    Blueprint, BlueprintDelta, BlueprintHit, BlueprintNo, BlueprintWrites, Blueprints, Board,
    BoardAsk, Bound, Branch, Caller, Change, ChangeAsk, ChangeDelta, ChangeDigest, ChangeEvent,
    ChangeFilter, ChangeItem, ChangeNo, ChangeReview, ChangeToken, ChangedFile, Checks, Cite,
    Cites, CloseReason, Closed, ClosedTicket, CodeHost, CodeHostWrites, Comment, CommentId,
    CommitId, Compared, Cursor, Depth, Did, Edit, Facts, FileName, Filed, Filter, Got,
    HappenedKind, HealthState, Holder, HolderId, HostId, Identity, Item, Kind, Ledger, Login, Mark,
    MarkHeld, MarkSpec, Marker, MarkerEdit, MarkerKey, MarkerKind, MergeHow, MeterRow,
    NewBlueprint, NewChange, NewRelease, NewTicket, OpenWork, Outcome, Part, Passing, Pool,
    PortName, Posted, Priority, ProjectName, Proposal, ProposalPart, Proposed, Realm, Refusal,
    Release, ReleaseId, Remote, RepoPath, Report, Reports, Rev, ReviewComment, Revision, Scope,
    Secret, Sent, SignInCode, SignInState, Site, SiteId, Standing, Status, Step, StepPart, Sum,
    Tag, TagName, Target, TargetEdit, TargetId, Ticket, TicketDelta, TicketDigest, TicketEvent,
    TicketKey, TicketWrites, Tickets, Token, Took, Unprepared, Unread, WebLink, Whole, Whose,
    Window, WithCloser, Within, WorkItem,
};

/// How many seconds of failures a `Faulty` asks its ledger to keep: the
/// health check's window. Owned by `health`, with the rule that asks.
pub use crate::health::FAILURES_KEPT;

/// A method of the tickets port, reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TicketsOp {
    /// `Tickets::ticket`.
    Ticket,
    /// `Tickets::what_is`.
    WhatIs,
    /// `Tickets::what_are`.
    WhatAre,
    /// `Tickets::list`.
    List,
    /// `Tickets::several`.
    Several,
    /// `Tickets::several_with_closers`.
    SeveralWithClosers,
    /// `Tickets::history`.
    History,
    /// `Tickets::notes`.
    Notes,
    /// `Tickets::note_edits`.
    NoteEdits,
    /// `Tickets::blockers`.
    Blockers,
    /// `Tickets::parts`.
    Parts,
    /// `Tickets::closed_since`.
    ClosedSince,
    /// `Tickets::recent`.
    Recent,
    /// `Tickets::recently_closed`.
    RecentlyClosed,
    /// `Tickets::count`.
    Count,
    /// `Tickets::search`.
    Search,
    /// `Tickets::filed_by`.
    FiledBy,
    /// `Tickets::find_marked`.
    FindMarked,
    /// `Tickets::kinds`.
    Kinds,
    /// `Tickets::marks`.
    Marks,
    /// `Tickets::targets`.
    Targets,
    /// `Tickets::members`.
    Members,
    /// `Tickets::changed`.
    Changed,
    /// `Tickets::delta`.
    Delta,
    /// `TicketWrites::apply`.
    Apply,
    /// `TicketWrites::prepare`.
    Prepare,
    /// `TicketWrites::prepare_one`.
    PrepareOne,
    /// `TicketWrites::ensure_mark`.
    EnsureMark,
    /// `TicketWrites::mark`.
    Mark,
    /// `TicketWrites::unmark`.
    Unmark,
    /// `TicketWrites::close`.
    Close,
    /// `TicketWrites::note`.
    Note,
    /// `TicketWrites::file`.
    File,
    /// `TicketWrites::edit`.
    Edit,
    /// `TicketWrites::reopen`.
    Reopen,
    /// `TicketWrites::set_kind`.
    SetKind,
    /// `TicketWrites::set_priority`.
    SetPriority,
    /// `TicketWrites::wait_on`.
    WaitOn,
    /// `TicketWrites::stop_waiting`.
    StopWaiting,
    /// `TicketWrites::assign`.
    Assign,
    /// `TicketWrites::add_part`.
    AddPart,
    /// `TicketWrites::remove_part`.
    RemovePart,
    /// `TicketWrites::new_target`.
    NewTarget,
    /// `TicketWrites::edit_target`.
    EditTarget,
    /// `TicketWrites::set_target`.
    SetTarget,
}

/// A method of the blueprints port, reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlueprintsOp {
    /// `Blueprints::blueprint`.
    Blueprint,
    /// `Blueprints::several`.
    Several,
    /// `Blueprints::comments`.
    Comments,
    /// `Blueprints::comments_by_id`.
    CommentsById,
    /// `Blueprints::body_edits`.
    BodyEdits,
    /// `Blueprints::comment_edits`.
    CommentEdits,
    /// `Blueprints::body_edits_of`.
    BodyEditsOf,
    /// `Blueprints::comment_edits_of`.
    CommentEditsOf,
    /// `Blueprints::search`.
    Search,
    /// `Blueprints::attachments`.
    Attachments,
    /// `Blueprints::fetch`.
    Fetch,
    /// `Blueprints::changed`.
    Changed,
    /// `Blueprints::delta`.
    Delta,
    /// `BlueprintWrites::open`.
    Open,
    /// `BlueprintWrites::edit`.
    Edit,
    /// `BlueprintWrites::comment`.
    Comment,
    /// `BlueprintWrites::edit_comment`.
    EditComment,
    /// `BlueprintWrites::close`.
    Close,
}

/// A method of the code host port, reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeHostOp {
    /// `CodeHost::facts`.
    Facts,
    /// `CodeHost::branch`.
    Branch,
    /// `CodeHost::compare`.
    Compare,
    /// `CodeHost::file`.
    File,
    /// `CodeHost::changes`.
    Changes,
    /// `CodeHost::change`.
    Change,
    /// `CodeHost::changed_files`.
    ChangedFiles,
    /// `CodeHost::activity`.
    Activity,
    /// `CodeHost::reviews`.
    Reviews,
    /// `CodeHost::review_comments`.
    ReviewComments,
    /// `CodeHost::may_write`.
    MayWrite,
    /// `CodeHost::checks`.
    Checks,
    /// `CodeHost::tags`.
    Tags,
    /// `CodeHost::recent_tags`.
    RecentTags,
    /// `CodeHost::recent_releases`.
    RecentReleases,
    /// `CodeHost::resolve_tag`.
    ResolveTag,
    /// `CodeHost::releases`.
    Releases,
    /// `CodeHost::latest_release`.
    LatestRelease,
    /// `CodeHost::release`.
    Release,
    /// `CodeHost::download`.
    Download,
    /// `CodeHost::archive`.
    Archive,
    /// `CodeHost::changed`.
    Changed,
    /// `CodeHost::delta`.
    Delta,
    /// `CodeHost::remote`.
    Remote,
    /// `CodeHostWrites::open_change`.
    OpenChange,
    /// `CodeHostWrites::edit_change`.
    EditChange,
    /// `CodeHostWrites::mark_change`.
    MarkChange,
    /// `CodeHostWrites::unmark_change`.
    UnmarkChange,
    /// `CodeHostWrites::comment_change`.
    CommentChange,
    /// `CodeHostWrites::close_change`.
    CloseChange,
    /// `CodeHostWrites::merge`.
    Merge,
    /// `CodeHostWrites::propose`.
    Propose,
    /// `CodeHostWrites::tag`.
    Tag,
    /// `CodeHostWrites::set_status`.
    SetStatus,
    /// `CodeHostWrites::new_release`.
    NewRelease,
    /// `CodeHostWrites::upload`.
    Upload,
    /// `CodeHostWrites::publish_release`.
    PublishRelease,
}

/// A method of identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IdentityOp {
    /// `Identity::me`.
    Me,
    /// `Identity::standing`.
    Standing,
    /// `Identity::account`.
    Account,
    /// `Identity::account_by_login`.
    AccountByLogin,
    /// `Identity::is_organisation`.
    IsOrganisation,
    /// `Identity::assignable`.
    Assignable,
    /// `Identity::token`.
    Token,
    /// `Identity::token_over`.
    TokenOver,
    /// `Identity::signed_in`.
    SignedIn,
}

/// A method of the joined reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BoardOp {
    /// `Board::open_work`.
    OpenWork,
    /// `Board::work_item`.
    WorkItem,
    /// `Board::closed_work`.
    ClosedWork,
    /// `Board::target_work`.
    TargetWork,
    /// `Board::open_changes`.
    OpenChanges,
    /// `Board::as_blockers`.
    AsBlockers,
}

/// Every method of every port that answers an outcome. A method that
/// makes no request (`cite`, `link`, `attachments_in`, `take_reports`, `new_ticket_link`,
/// `sign_in_link`) has no outcome to inject and no `Op`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Op {
    /// A method of the tickets port.
    Tickets(TicketsOp),
    /// A method of the blueprints port.
    Blueprints(BlueprintsOp),
    /// A method of the code host port.
    CodeHost(CodeHostOp),
    /// A method of identity.
    Identity(IdentityOp),
    /// A method of the joined reads.
    Board(BoardOp),
}

impl Op {
    /// The port the method is of.
    pub fn port(&self) -> PortName {
        match self {
            Op::Tickets(_) => PortName::Tickets,
            Op::Blueprints(_) => PortName::Blueprints,
            Op::CodeHost(_) => PortName::CodeHost,
            Op::Identity(_) => PortName::Identity,
            Op::Board(_) => PortName::Board,
        }
    }

    /// True for a method that writes.
    pub fn is_write(&self) -> bool {
        matches!(
            *self,
            Op::Tickets(
                T::Apply
                    | T::Prepare
                    | T::PrepareOne
                    | T::EnsureMark
                    | T::Mark
                    | T::Unmark
                    | T::Close
                    | T::Note
                    | T::File
                    | T::Edit
                    | T::Reopen
                    | T::SetKind
                    | T::SetPriority
                    | T::WaitOn
                    | T::StopWaiting
                    | T::Assign
                    | T::AddPart
                    | T::RemovePart
                    | T::NewTarget
                    | T::EditTarget
                    | T::SetTarget,
            ) | Op::Blueprints(B::Open | B::Edit | B::Comment | B::EditComment | B::Close)
                | Op::CodeHost(
                    C::OpenChange
                        | C::EditChange
                        | C::MarkChange
                        | C::UnmarkChange
                        | C::CommentChange
                        | C::CloseChange
                        | C::Merge
                        | C::Propose
                        | C::Tag
                        | C::SetStatus
                        | C::NewRelease
                        | C::Upload
                        | C::PublishRelease,
                )
        )
    }

    /// The budget the fakes draw a call on by default: a search the search
    /// pool, a write the writes pool, a joined read the priced pool, any
    /// other read the calls pool. This map is tehas's guess at a site's
    /// budget, not a site's rule: a consumer must not rely on it, and
    /// `Faulty::draws` sets another.
    pub fn pool(&self) -> Pool {
        match *self {
            Op::Tickets(T::Search) | Op::Blueprints(B::Search) => Pool::Search,
            Op::Board(_) => Pool::Priced,
            op if op.is_write() => Pool::Writes,
            _ => Pool::Calls,
        }
    }
}

/// An outcome a `Faulty` answers in place of its site's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Fault {
    /// No answer in time, the request not sent, the write not applied.
    Timeout,
    /// No answer in time after the request went out; the write applied or
    /// not, as the test says.
    TimeoutMaybe {
        /// The site applied the write before the time ran out.
        applied: bool,
    },
    /// The site could not be reached.
    Unreachable,
    /// The site failed with an error of its own after the request went
    /// out; the write made first or not, as the test says.
    Server {
        /// The site made the write before it failed.
        applied: bool,
    },
    /// The site answered that a check run or job errored: the one
    /// `Unavailable` that health reads as the host up.
    Broken,
    /// The holder's pool is spent until a time.
    Spent {
        /// The pool.
        pool: Pool,
        /// When it has budget again.
        until: At,
    },
    /// The credential is no longer good.
    SignedOut,
    /// This actor may not do this.
    Forbidden,
    /// The site will not take this write.
    Refused(Refusal),
    /// A list longer than its bound.
    Cut,
    /// An answer whose parts disagree.
    Inconsistent,
    /// An item too large to give whole.
    TooLarge,
    /// An answer that does not parse.
    Malformed,
    /// A list with one item twice.
    Repeated,
    /// A history out of order.
    OutOfOrder,
}

impl Fault {
    /// The outcome an adapter answers for this fault: an odd answer reads
    /// as `Unread`, never as a partial list.
    pub fn outcome(&self) -> Outcome {
        match *self {
            Fault::Timeout => Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::No,
            },
            Fault::TimeoutMaybe { .. } => Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
            Fault::Unreachable => Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            },
            Fault::Server { .. } => Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            },
            Fault::Broken => Outcome::Unavailable {
                why: Passing::Broken,
                sent: Sent::Maybe,
            },
            Fault::Spent { pool, until } => Outcome::Spent { pool, until },
            Fault::SignedOut => Outcome::SignedOut,
            Fault::Forbidden => Outcome::Forbidden,
            Fault::Refused(r) => Outcome::Refused(r),
            Fault::Cut => Outcome::Unread(Unread::Cut),
            Fault::Inconsistent | Fault::Repeated | Fault::OutOfOrder => {
                Outcome::Unread(Unread::Inconsistent)
            }
            Fault::TooLarge => Outcome::Unread(Unread::TooLarge),
            Fault::Malformed => Outcome::Unread(Unread::Malformed),
        }
    }
}

/// What a `Faulty` reports its health through, as an adapter's wiring
/// gives it: the host and realm its notices name, the project its failures
/// are kept for, the factory's holder, the ledger and sink shared with
/// other processes, the clock, and the deadline of the pass.
#[derive(Clone)]
pub struct Watch {
    /// The host its notices name.
    pub host: HostId,
    /// The realm its notices name.
    pub realm: Realm,
    /// The project its failures are kept for.
    pub site: SiteId,
    /// The factory's holder on the host.
    pub ours: HolderId,
    /// The ledger, shared.
    pub ledger: Arc<dyn Ledger>,
    /// The sink, shared.
    pub notices: Arc<dyn crate::Notices>,
    /// The clock.
    pub clock: Clock,
    /// Past this time, a call answers `Unavailable { why: Timeout, sent:
    /// No }` with no request, and is not given to `health::next`.
    pub deadline: Option<At>,
    /// The world a person's key is read in, for the account their budget
    /// is kept under.
    pub world: World,
}

impl Watch {
    /// The holder whose budget an actor's calls spend. The factory's own
    /// tokens are the factory's. A person is held by their account's id, so
    /// every session of one person shares one budget (a key the world does
    /// not know stands for its own account), and the operator by their own
    /// account's. A given token and a deploy key are each a holder of their
    /// own, and every anonymous caller one together. So an operator's, a
    /// deploy key's or a given token's failure never counts against the
    /// factory.
    pub fn holder(&self, by: &Actor) -> Holder {
        match by {
            Actor::Ours(_) => Holder::Ours(self.ours.clone()),
            Actor::Person(key) => Holder::Person(
                self.world
                    .person_account(key, self.realm)
                    .unwrap_or_else(|| AccountId::new(key.as_str())),
            ),
            // The operator acts as their own account.
            Actor::Operator => Holder::Person(crate::fake::state::account_id(
                crate::fake::OPERATOR,
                self.realm,
            )),
            // A given token is budgeted apart, so its sign-out is its own.
            Actor::Given(key) => Holder::Ours(HolderId::new(format!("given:{}", key.as_str()))),
            // A deploy key is no account: a holder of its own.
            Actor::Deploy(key) => {
                Holder::Anonymous(HolderId::new(format!("deploy:{}", key.as_str())))
            }
            Actor::Anonymous => Holder::Anonymous(HolderId::new("anonymous")),
        }
    }
}

#[derive(Debug, Clone)]
struct Rule {
    op: Op,
    fault: Fault,
    by: Option<Actor>,
    skip: u32,
    times: Option<u32>,
}

/// The calls of one method, by every actor (`None`) or by one.
type Calls = Vec<(Op, Option<Actor>, u32)>;

struct Shared {
    rules: Vec<Rule>,
    draws: Vec<(Op, Vec<Pool>)>,
    /// Shared with the `until` views made of it.
    calls: Arc<Mutex<Calls>>,
    /// Shared with the `until` views made of it.
    requests: Arc<Mutex<Vec<(Actor, u32)>>>,
    watch: Option<Watch>,
}

/// What `Faulty` reads of the site it wraps to tell that a call sent a
/// request: the `World` that logs each request the site sends. A call
/// sent a request when the log grew while the site answered it, so a call
/// run at the same time on another thread over the same world can be
/// taken for its request.
pub trait RequestLog {
    /// The world that logs this site's requests. A site that names none
    /// is taken to send a request on every call it is handed.
    fn log(&self) -> Option<&World> {
        None
    }
}

impl RequestLog for FakeSite {
    fn log(&self) -> Option<&World> {
        Some(self.world())
    }
}

/// Wraps any site and answers chosen calls with a fault:
/// `Faulty::new(site).on(Op::Board(BoardOp::OpenWork), Fault::Timeout).times(2)`.
/// A rule answers every matching call, from the first, unless `after` or
/// `times` narrow it; the first rule that matches a call wins. Calls of one
/// method are counted across every handle the wrapper made. Given a
/// `Watch`, it holds a spent pool's calls with no request, answers past the
/// deadline with no request, and gives every other answer to
/// `health::next`, raising a notice only when its swap wins. An answer its
/// site made with no request (`FakeSite`'s kept or given token, or a
/// token refused before any mint) is not given to `health::next`: the
/// site's `RequestLog` tells which. Over a site whose log names no world,
/// every call it hands the site counts as a request.
pub struct Faulty<S: Site> {
    site: S,
    shared: Arc<Shared>,
    log: Option<World>,
}

impl<S: Site> Faulty<S> {
    /// The site, with no fault and no watch.
    pub fn new(site: S) -> Self
    where
        S: RequestLog,
    {
        Self {
            log: site.log().cloned(),
            site,
            shared: Arc::new(Shared {
                rules: Vec::new(),
                draws: Vec::new(),
                calls: Arc::new(Mutex::new(Vec::new())),
                requests: Arc::new(Mutex::new(Vec::new())),
                watch: None,
            }),
        }
    }

    /// The shared state, to change while building. Handles made before
    /// keep what they were made with.
    fn shared(&mut self) -> &mut Shared {
        if Arc::get_mut(&mut self.shared).is_none() {
            let s = &*self.shared;
            let copy = Shared {
                rules: s.rules.clone(),
                draws: s.draws.clone(),
                calls: Arc::new(Mutex::new(lock(&s.calls).clone())),
                requests: Arc::new(Mutex::new(lock(&s.requests).clone())),
                watch: s.watch.clone(),
            };
            self.shared = Arc::new(copy);
        }
        Arc::get_mut(&mut self.shared).expect("one owner")
    }

    /// Answer calls of `op` with `fault`.
    pub fn on(mut self, op: Op, fault: Fault) -> Self {
        self.shared().rules.push(Rule {
            op,
            fault,
            by: None,
            skip: 0,
            times: None,
        });
        self
    }

    /// The last rule answers only the calls of `actor`, counted apart.
    pub fn by(mut self, actor: Actor) -> Self {
        if let Some(r) = self.shared().rules.last_mut() {
            r.by = Some(actor);
        }
        self
    }

    /// Calls of `op` draw on `pools`, in place of `Op::pool`'s guess.
    pub fn draws(mut self, op: Op, pools: &[Pool]) -> Self {
        let shared = self.shared();
        shared.draws.retain(|(o, _)| *o != op);
        shared.draws.push((op, pools.to_vec()));
        self
    }

    /// The last rule answers this many calls, then lets them through.
    pub fn times(mut self, n: u32) -> Self {
        if let Some(r) = self.shared().rules.last_mut() {
            r.times = Some(n);
        }
        self
    }

    /// The last rule lets the first `n` calls through.
    pub fn after(mut self, n: u32) -> Self {
        if let Some(r) = self.shared().rules.last_mut() {
            r.skip = n;
        }
        self
    }

    /// Report health through `watch`.
    pub fn watch(mut self, watch: Watch) -> Self {
        self.shared().watch = Some(watch);
        self
    }

    /// How many calls went out: those the site answered and those a fault
    /// answered in its place. A held call, one past the deadline, and one
    /// the site answered with no request are not counted.
    pub fn requests(&self) -> u32 {
        lock(&self.shared.requests).iter().map(|(_, n)| n).sum()
    }

    /// How many calls of `actor`'s handles went out, counted as `requests`.
    pub fn requests_by(&self, actor: &Actor) -> u32 {
        lock(&self.shared.requests)
            .iter()
            .find(|(a, _)| a == actor)
            .map_or(0, |(_, n)| *n)
    }

    /// The site it wraps.
    pub fn inner(&self) -> &S {
        &self.site
    }

    fn wrap<H: ?Sized>(&self, inner: Box<H>, by: &Actor, port: PortName) -> Wrap<H> {
        let holder = match &self.shared.watch {
            Some(w) => w.holder(by),
            None => Holder::Ours(HolderId::new("none")),
        };
        Wrap {
            inner,
            shared: Arc::clone(&self.shared),
            actor: by.clone(),
            holder,
            port,
            reports: Vec::new(),
            told_down: AtomicBool::new(false),
            log: self.log.clone(),
        }
    }
}

impl<S: Site> Site for Faulty<S> {
    fn realms(&self) -> &[Realm] {
        self.site.realms()
    }

    /// The view of its site's `until`, under the same rules, counting its
    /// calls and requests with this one's. With a watch, the watch's
    /// `deadline` is `until`, or the earlier one it had.
    fn until(&self, until: At) -> Arc<dyn Site> {
        let s = &*self.shared;
        let watch = s.watch.clone().map(|mut w| {
            w.deadline = Some(w.deadline.map_or(until, |d| d.min(until)));
            w
        });
        Arc::new(Faulty {
            site: self.site.until(until),
            shared: Arc::new(Shared {
                rules: s.rules.clone(),
                draws: s.draws.clone(),
                calls: Arc::clone(&s.calls),
                requests: Arc::clone(&s.requests),
                watch,
            }),
            log: self.log.clone(),
        })
    }

    fn mark_name(&self, mark: &crate::Mark) -> String {
        self.site.mark_name(mark)
    }

    fn marker_text(&self, marker: &crate::Marker) -> String {
        self.site.marker_text(marker)
    }

    fn tickets(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Tickets + Send>> {
        let h = self.site.tickets(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::Tickets)))
    }

    fn ticket_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn TicketWrites + Send>> {
        let h = self.site.ticket_writes(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::Tickets)))
    }

    fn blueprints(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Blueprints + Send>> {
        let h = self.site.blueprints(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::Blueprints)))
    }

    fn blueprint_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn BlueprintWrites + Send>> {
        let h = self.site.blueprint_writes(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::Blueprints)))
    }

    fn code_host(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHost + Send>> {
        let h = self.site.code_host(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::CodeHost)))
    }

    fn code_host_writes(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHostWrites + Send>> {
        let h = self.site.code_host_writes(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::CodeHost)))
    }

    fn identity(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Identity + Send>> {
        let h = self.site.identity(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::Identity)))
    }

    /// The site's own board, wrapped: faults on `Op::Board` apply to it,
    /// not those on the ports it joins.
    fn board(&self, by: &Actor, caller: Caller) -> Got<Box<dyn Board + Send>> {
        let h = self.site.board(by, caller)?;
        Ok(Box::new(self.wrap(h, by, PortName::Board)))
    }

    fn meter(&self) -> Vec<MeterRow> {
        self.site.meter()
    }
}

/// An answer of any shape, read as `health::next` reads it.
trait Answer {
    fn seen(&self) -> Result<(), Outcome>;
    /// Every outcome the answer holds, for the reports.
    fn met(&self) -> Vec<Outcome> {
        self.seen().err().into_iter().collect()
    }
}

impl<T> Answer for Result<T, Outcome> {
    fn seen(&self) -> Result<(), Outcome> {
        match self {
            Ok(_) => Ok(()),
            Err(o) => Err(o.clone()),
        }
    }
}

impl Answer for Result<(), Unprepared> {
    fn seen(&self) -> Result<(), Outcome> {
        self.clone().map_err(|u| u.why)
    }
}

/// A batch is answered when the site answered: one item cut, too large or
/// not found is that item's, not the call's. Only an outcome every item
/// shares that no answer of the site gives (an outage, a spent pool, a
/// sign-out) is the call's.
impl<K, T> Answer for Vec<(K, Got<T>)> {
    fn seen(&self) -> Result<(), Outcome> {
        let first = match self.first() {
            Some((_, Err(o))) => o,
            _ => return Ok(()),
        };
        let call_level = matches!(
            first,
            Outcome::Unavailable { .. } | Outcome::Spent { .. } | Outcome::SignedOut
        );
        if call_level && self.iter().all(|(_, a)| a.as_ref().err() == Some(first)) {
            Err(first.clone())
        } else {
            Ok(())
        }
    }

    /// Each outcome once, as one call met it.
    fn met(&self) -> Vec<Outcome> {
        let mut met: Vec<Outcome> = Vec::new();
        for o in self.iter().filter_map(|(_, a)| a.as_ref().err()) {
            if !met.contains(o) {
                met.push(o.clone());
            }
        }
        met
    }
}

impl Answer for (Applied<StepPart>, Option<Posted<CommentId>>) {
    fn seen(&self) -> Result<(), Outcome> {
        match &self.0.stopped {
            Some((_, o)) => Err(o.clone()),
            None => Ok(()),
        }
    }
}

impl Answer for Result<Proposed, Applied<ProposalPart>> {
    fn seen(&self) -> Result<(), Outcome> {
        match self {
            Err(Applied {
                stopped: Some((_, o)),
                ..
            }) => Err(o.clone()),
            _ => Ok(()),
        }
    }
}

/// One handle of a `Faulty`, over its site's handle.
struct Wrap<H: ?Sized> {
    inner: Box<H>,
    shared: Arc<Shared>,
    actor: Actor,
    holder: Holder,
    port: PortName,
    reports: Vec<Report>,
    told_down: AtomicBool,
    /// The world that logs the site's requests, if it names one.
    log: Option<World>,
}

impl<H: ?Sized> Wrap<H> {
    fn call<R: Answer>(
        &mut self,
        op: Op,
        fail: impl FnOnce(Outcome) -> R,
        f: impl FnOnce(&mut H) -> R,
    ) -> R {
        let pools = self.pools(op);
        if let Some(o) = self.made_without_request(&pools) {
            self.report(&o);
            return fail(o);
        }
        let fault = self.fault(op);
        let before = self.sent();
        let answer = match fault {
            None => f(&mut self.inner),
            Some(fault) => {
                if matches!(
                    fault,
                    Fault::TimeoutMaybe { applied: true } | Fault::Server { applied: true }
                ) {
                    let _applied = f(&mut self.inner);
                }
                fail(fault.outcome())
            }
        };
        let seen = answer.seen();
        // A fault stands for an answer of the site; an answer the site made
        // with no request is neither counted nor seen.
        if fault.is_some() || before.is_none() || self.sent() != before {
            self.count_request();
            self.observe(&pools, &seen);
        }
        for o in answer.met() {
            self.report(&o);
        }
        answer
    }

    /// How many requests the site's world has logged, if it names one.
    fn sent(&self) -> Option<u64> {
        self.log.as_ref().map(World::sent)
    }

    /// The pools a call of `op` draws on: as `draws` set, else `Op::pool`.
    fn pools(&self, op: Op) -> Vec<Pool> {
        match self.shared.draws.iter().find(|(o, _)| *o == op) {
            Some((_, pools)) => pools.clone(),
            None => vec![op.pool()],
        }
    }

    fn count_request(&self) {
        let mut requests = lock(&self.shared.requests);
        match requests.iter_mut().find(|(a, _)| *a == self.actor) {
            Some((_, n)) => *n += 1,
            None => requests.push((self.actor.clone(), 1)),
        }
    }

    /// The answer an adapter makes with no request: past the deadline, or
    /// a pool the call draws on that the ledger holds spent.
    fn made_without_request(&mut self, pools: &[Pool]) -> Option<Outcome> {
        let w = self.shared.watch.clone()?;
        let now = w.clock.now();
        if w.deadline.is_some_and(|d| now >= d) {
            return Some(Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::No,
            });
        }
        for &pool in pools {
            let whose = Whose::Pool(self.holder.clone(), pool);
            match w.ledger.health(&w.host, &whose) {
                Ok(HealthState::Spent { pool: p, until }) if p == pool && now < until => {
                    return Some(Outcome::Spent { pool, until });
                }
                Ok(_) => {}
                Err(_) => health::ledger_down(&*w.notices, &self.told_down, &w.host, w.realm, now),
            }
        }
        None
    }

    /// The fault for this call: the first rule of `op` whose count of
    /// calls (all actors', or its own actor's) it is within.
    fn fault(&self, op: Op) -> Option<Fault> {
        let mut calls = lock(&self.shared.calls);
        let mut bump = |by: Option<Actor>| -> u32 {
            match calls.iter_mut().find(|(o, a, _)| *o == op && *a == by) {
                Some((_, _, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    calls.push((op, by, 1));
                    1
                }
            }
        };
        let all = bump(None);
        let mine = bump(Some(self.actor.clone()));
        self.shared
            .rules
            .iter()
            .find(|r| {
                let n = match &r.by {
                    None => all,
                    Some(a) if *a == self.actor => mine,
                    Some(_) => return false,
                };
                r.op == op && n > r.skip && r.times.is_none_or(|t| n <= r.skip + t)
            })
            .map(|r| r.fault)
    }

    /// Give the answer of an attempt to reach the site to `health::observe`,
    /// the one rule of the ledger and the sink, shared with the adapter's
    /// anonymous handle. A ledger that fails fails toward reporting.
    fn observe(&self, pools: &[Pool], seen: &Result<(), Outcome>) {
        let Some(w) = self.shared.watch.clone() else {
            return;
        };
        let now = w.clock.now();
        let o = health::Observer {
            ledger: &*w.ledger,
            sink: &*w.notices,
            told_down: &self.told_down,
            host: &w.host,
            holder: &self.holder,
            realm: w.realm,
            site: &w.site,
            at: now,
        };
        health::observe(&o, seen, pools);
    }

    fn report(&mut self, outcome: &Outcome) {
        match self.reports.iter_mut().find(|r| r.outcome == *outcome) {
            Some(r) => r.calls += 1,
            None => self.reports.push(Report {
                port: self.port,
                outcome: outcome.clone(),
                calls: 1,
            }),
        }
    }
}

impl<H: Reports + ?Sized> Reports for Wrap<H> {
    /// Each outcome this handle met, once with its count: a call the site
    /// answered is counted here too, so the site handle's own reports are
    /// taken and dropped.
    fn take_reports(&mut self) -> Vec<Report> {
        let _counted = self.inner.take_reports();
        std::mem::take(&mut self.reports)
    }
}

impl<H: Cites + ?Sized> Cites for Wrap<H> {
    fn cite(&self, what: &Cite) -> String {
        self.inner.cite(what)
    }
    fn link(&self, what: &Cite) -> Option<WebLink> {
        self.inner.link(what)
    }
}

use BlueprintsOp as B;
use CodeHostOp as C;
use TicketsOp as T;

impl<H: Tickets + ?Sized> Tickets for Wrap<H> {
    fn ticket(&mut self, key: &TicketKey, depth: Depth, history: Option<Window>) -> Got<Ticket> {
        self.call(Op::Tickets(T::Ticket), Err, |h| {
            h.ticket(key, depth, history)
        })
    }
    fn what_is(&mut self, number: NonZeroU64) -> Got<Item> {
        self.call(Op::Tickets(T::WhatIs), Err, |h| h.what_is(number))
    }
    fn what_are(&mut self, numbers: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)> {
        let fail = |o: Outcome| numbers.iter().map(|n| (*n, Err(o.clone()))).collect();
        self.call(Op::Tickets(T::WhatAre), fail, |h| h.what_are(numbers))
    }
    fn list(&mut self, filter: &Filter, bound: Bound) -> Got<Whole<TicketDigest>> {
        self.call(Op::Tickets(T::List), Err, |h| h.list(filter, bound))
    }
    fn recent(&mut self, filter: &Filter, first: NonZeroU32) -> Got<Took<TicketDigest>> {
        self.call(Op::Tickets(T::Recent), Err, |h| h.recent(filter, first))
    }
    fn recently_closed(&mut self, first: NonZeroU32) -> Got<Took<ClosedTicket>> {
        self.call(Op::Tickets(T::RecentlyClosed), Err, |h| {
            h.recently_closed(first)
        })
    }
    fn count(&mut self, filter: &Filter) -> Got<u64> {
        self.call(Op::Tickets(T::Count), Err, |h| h.count(filter))
    }
    fn several(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<TicketDigest>)> {
        let fail = |o: Outcome| keys.iter().map(|k| (k.clone(), Err(o.clone()))).collect();
        self.call(Op::Tickets(T::Several), fail, |h| h.several(keys))
    }
    fn several_with_closers(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<WithCloser>)> {
        let fail = |o: Outcome| keys.iter().map(|k| (k.clone(), Err(o.clone()))).collect();
        self.call(Op::Tickets(T::SeveralWithClosers), fail, |h| {
            h.several_with_closers(keys)
        })
    }
    fn history(
        &mut self,
        key: &TicketKey,
        window: Window,
        only: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        self.call(Op::Tickets(T::History), Err, |h| {
            h.history(key, window, only)
        })
    }
    fn notes(&mut self, key: &TicketKey, window: Window) -> Got<Took<Comment>> {
        self.call(Op::Tickets(T::Notes), Err, |h| h.notes(key, window))
    }
    fn note_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>> {
        self.call(Op::Tickets(T::NoteEdits), Err, |h| h.note_edits(id, bound))
    }
    fn blockers(&mut self, key: &TicketKey) -> Got<Whole<Blocker>> {
        self.call(Op::Tickets(T::Blockers), Err, |h| h.blockers(key))
    }
    fn parts(&mut self, key: &TicketKey) -> Got<Whole<Part>> {
        self.call(Op::Tickets(T::Parts), Err, |h| h.parts(key))
    }
    fn closed_since(&mut self, since: At, bound: Bound) -> Got<Whole<ClosedTicket>> {
        self.call(Op::Tickets(T::ClosedSince), Err, |h| {
            h.closed_since(since, bound)
        })
    }
    fn search(
        &mut self,
        words: &str,
        within: Within,
        open: Option<bool>,
        first: NonZeroU32,
    ) -> Got<Took<TicketDigest>> {
        self.call(Op::Tickets(T::Search), Err, |h| {
            h.search(words, within, open, first)
        })
    }
    fn filed_by(&mut self, by: &AccountId, since: At, bound: Bound) -> Got<Whole<Filed>> {
        self.call(Op::Tickets(T::FiledBy), Err, |h| {
            h.filed_by(by, since, bound)
        })
    }
    fn find_marked(
        &mut self,
        mark: &Mark,
        kind: &MarkerKind,
        key: &MarkerKey,
        open: Option<bool>,
    ) -> Got<Whole<TicketKey>> {
        self.call(Op::Tickets(T::FindMarked), Err, |h| {
            h.find_marked(mark, kind, key, open)
        })
    }
    fn kinds(&mut self) -> Got<Whole<Kind>> {
        self.call(Op::Tickets(T::Kinds), Err, |h| h.kinds())
    }
    fn marks(&mut self, bound: Bound) -> Got<Whole<MarkHeld>> {
        self.call(Op::Tickets(T::Marks), Err, |h| h.marks(bound))
    }
    fn targets(&mut self, bound: Bound) -> Got<Whole<Target>> {
        self.call(Op::Tickets(T::Targets), Err, |h| h.targets(bound))
    }
    fn members(&mut self, target: &TargetId, bound: Bound) -> Got<Whole<TicketDigest>> {
        self.call(Op::Tickets(T::Members), Err, |h| h.members(target, bound))
    }
    fn changed(&mut self, since: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        self.call(Op::Tickets(T::Changed), Err, |h| h.changed(since))
    }
    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<TicketDelta> {
        self.call(Op::Tickets(T::Delta), Err, |h| h.delta(from, bound))
    }
    fn new_ticket_link(&self, title: &str, text: &str) -> WebLink {
        self.inner.new_ticket_link(title, text)
    }
}

impl<H: TicketWrites + ?Sized> TicketWrites for Wrap<H> {
    /// A fault on `Apply` stops the step at its first part.
    fn apply(
        &mut self,
        key: &TicketKey,
        step: &Step,
    ) -> (Applied<StepPart>, Option<Posted<CommentId>>) {
        let first = if step.note.is_some() || !step.markers.is_empty() {
            StepPart::Note
        } else if let Some(m) = step.unmark.first() {
            StepPart::Unmark(m.clone())
        } else if let Some(m) = step.mark.first() {
            StepPart::Mark(m.clone())
        } else {
            StepPart::Close
        };
        let fail = |o: Outcome| {
            (
                Applied {
                    done: Vec::new(),
                    stopped: Some((first, o)),
                },
                None,
            )
        };
        self.call(Op::Tickets(T::Apply), fail, |h| h.apply(key, step))
    }
    fn prepare(&mut self) -> Result<(), Unprepared> {
        self.call(Op::Tickets(T::Prepare), |o| Err(o.into()), |h| h.prepare())
    }
    fn prepare_one(&mut self, mark: &Mark) -> Did {
        self.call(Op::Tickets(T::PrepareOne), Err, |h| h.prepare_one(mark))
    }
    fn ensure_mark(&mut self, spec: &MarkSpec) -> Did {
        self.call(Op::Tickets(T::EnsureMark), Err, |h| h.ensure_mark(spec))
    }
    fn mark(&mut self, key: &TicketKey, mark: &Mark) -> Did {
        self.call(Op::Tickets(T::Mark), Err, |h| h.mark(key, mark))
    }
    fn unmark(&mut self, key: &TicketKey, mark: &Mark) -> Did {
        self.call(Op::Tickets(T::Unmark), Err, |h| h.unmark(key, mark))
    }
    fn close(&mut self, key: &TicketKey, reason: &CloseReason) -> Did {
        self.call(Op::Tickets(T::Close), Err, |h| h.close(key, reason))
    }
    fn close_at(&mut self, key: &TicketKey, reason: &CloseReason) -> Closed {
        self.call(Op::Tickets(T::Close), Err, |h| h.close_at(key, reason))
    }
    fn note(&mut self, key: &TicketKey, body: &str, markers: &[Marker]) -> Got<Posted<CommentId>> {
        self.call(Op::Tickets(T::Note), Err, |h| h.note(key, body, markers))
    }
    fn file(&mut self, new: &NewTicket) -> Got<Posted<TicketKey>> {
        self.call(Op::Tickets(T::File), Err, |h| h.file(new))
    }
    fn edit(
        &mut self,
        key: &TicketKey,
        title: Option<&str>,
        text: Option<&str>,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink> {
        self.call(Op::Tickets(T::Edit), Err, |h| {
            h.edit(key, title, text, markers, since)
        })
    }
    fn reopen(&mut self, key: &TicketKey) -> Did {
        self.call(Op::Tickets(T::Reopen), Err, |h| h.reopen(key))
    }
    fn set_kind(&mut self, key: &TicketKey, kind: Option<&Kind>) -> Did {
        self.call(Op::Tickets(T::SetKind), Err, |h| h.set_kind(key, kind))
    }
    fn set_priority(&mut self, key: &TicketKey, p: Priority) -> Did {
        self.call(Op::Tickets(T::SetPriority), Err, |h| h.set_priority(key, p))
    }
    fn wait_on(&mut self, key: &TicketKey, on: &Item) -> Did {
        self.call(Op::Tickets(T::WaitOn), Err, |h| h.wait_on(key, on))
    }
    fn stop_waiting(&mut self, key: &TicketKey, on: &Item) -> Did {
        self.call(Op::Tickets(T::StopWaiting), Err, |h| {
            h.stop_waiting(key, on)
        })
    }
    fn assign(&mut self, key: &TicketKey, who: &AccountId) -> Did {
        self.call(Op::Tickets(T::Assign), Err, |h| h.assign(key, who))
    }
    fn add_part(&mut self, parent: &TicketKey, child: &TicketKey) -> Did {
        self.call(Op::Tickets(T::AddPart), Err, |h| h.add_part(parent, child))
    }
    fn remove_part(&mut self, parent: &TicketKey, child: &TicketKey) -> Did {
        self.call(Op::Tickets(T::RemovePart), Err, |h| {
            h.remove_part(parent, child)
        })
    }
    fn new_target(&mut self, title: &str, due: Option<At>) -> Got<TargetId> {
        self.call(Op::Tickets(T::NewTarget), Err, |h| h.new_target(title, due))
    }
    fn edit_target(&mut self, id: &TargetId, edit: &TargetEdit) -> Did {
        self.call(Op::Tickets(T::EditTarget), Err, |h| h.edit_target(id, edit))
    }
    fn set_target(&mut self, key: &TicketKey, target: Option<&TargetId>) -> Did {
        self.call(Op::Tickets(T::SetTarget), Err, |h| {
            h.set_target(key, target)
        })
    }
}

impl<H: Blueprints + ?Sized> Blueprints for Wrap<H> {
    fn blueprint(&mut self, no: BlueprintNo, depth: Depth) -> Got<Blueprint> {
        self.call(Op::Blueprints(B::Blueprint), Err, |h| {
            h.blueprint(no, depth)
        })
    }
    fn several(&mut self, nos: &[BlueprintNo], depth: Depth) -> Vec<(BlueprintNo, Got<Blueprint>)> {
        let fail = |o: Outcome| nos.iter().map(|n| (*n, Err(o.clone()))).collect();
        self.call(Op::Blueprints(B::Several), fail, |h| h.several(nos, depth))
    }
    fn comments(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Comment>> {
        self.call(Op::Blueprints(B::Comments), Err, |h| h.comments(no, bound))
    }
    fn comments_by_id(&mut self, ids: &[CommentId]) -> Vec<(CommentId, Got<Comment>)> {
        let fail = |o: Outcome| ids.iter().map(|i| (i.clone(), Err(o.clone()))).collect();
        self.call(Op::Blueprints(B::CommentsById), fail, |h| {
            h.comments_by_id(ids)
        })
    }
    fn body_edits(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Edit>> {
        self.call(Op::Blueprints(B::BodyEdits), Err, |h| {
            h.body_edits(no, bound)
        })
    }
    fn comment_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>> {
        self.call(Op::Blueprints(B::CommentEdits), Err, |h| {
            h.comment_edits(id, bound)
        })
    }
    /// The whole call goes to the inner handle, so an adapter's batch is
    /// kept; a fault fails each item.
    fn body_edits_of(
        &mut self,
        nos: &[BlueprintNo],
        bound: Bound,
    ) -> Vec<(BlueprintNo, Got<Whole<Edit>>)> {
        let fail = |o: Outcome| nos.iter().map(|&n| (n, Err(o.clone()))).collect();
        self.call(Op::Blueprints(B::BodyEditsOf), fail, |h| {
            h.body_edits_of(nos, bound)
        })
    }
    /// As `body_edits_of`, under its `Op`; a fault fails the read.
    fn body_edits_together(
        &mut self,
        nos: &[BlueprintNo],
        bound: Bound,
    ) -> Got<Vec<(BlueprintNo, Got<Whole<Edit>>)>> {
        self.call(Op::Blueprints(B::BodyEditsOf), Err, |h| {
            h.body_edits_together(nos, bound)
        })
    }
    fn comment_edits_of(
        &mut self,
        ids: &[CommentId],
        bound: Bound,
    ) -> Vec<(CommentId, Got<Whole<Edit>>)> {
        let fail = |o: Outcome| ids.iter().map(|i| (i.clone(), Err(o.clone()))).collect();
        self.call(Op::Blueprints(B::CommentEditsOf), fail, |h| {
            h.comment_edits_of(ids, bound)
        })
    }
    fn search(
        &mut self,
        words: &str,
        within: Within,
        open: Option<bool>,
        first: NonZeroU32,
    ) -> Got<Took<BlueprintHit>> {
        self.call(Op::Blueprints(B::Search), Err, |h| {
            h.search(words, within, open, first)
        })
    }
    fn attachments(&mut self, no: BlueprintNo) -> Got<Whole<Attachment>> {
        self.call(Op::Blueprints(B::Attachments), Err, |h| h.attachments(no))
    }
    fn attachments_in(&self, text: &str) -> Vec<Attachment> {
        self.inner.attachments_in(text)
    }
    fn fetch(&mut self, a: &AttachmentId, limit: u64, into: &mut dyn std::io::Write) -> Did {
        self.call(Op::Blueprints(B::Fetch), Err, |h| h.fetch(a, limit, into))
    }
    fn changed(&mut self, since: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        self.call(Op::Blueprints(B::Changed), Err, |h| h.changed(since))
    }
    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<BlueprintDelta> {
        self.call(Op::Blueprints(B::Delta), Err, |h| h.delta(from, bound))
    }
}

impl<H: BlueprintWrites + ?Sized> BlueprintWrites for Wrap<H> {
    fn open(&mut self, new: &NewBlueprint) -> Got<Posted<BlueprintNo>> {
        self.call(Op::Blueprints(B::Open), Err, |h| h.open(new))
    }
    fn edit(
        &mut self,
        no: BlueprintNo,
        title: Option<&str>,
        body: Option<&str>,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink> {
        self.call(Op::Blueprints(B::Edit), Err, |h| {
            h.edit(no, title, body, markers, since)
        })
    }
    fn comment(
        &mut self,
        no: BlueprintNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>> {
        self.call(Op::Blueprints(B::Comment), Err, |h| {
            h.comment(no, body, markers)
        })
    }
    fn edit_comment(
        &mut self,
        id: &CommentId,
        body: &str,
        markers: &MarkerEdit,
        since: &Revision,
    ) -> Got<WebLink> {
        self.call(Op::Blueprints(B::EditComment), Err, |h| {
            h.edit_comment(id, body, markers, since)
        })
    }
    fn close(&mut self, no: BlueprintNo, done: bool) -> Did {
        self.call(Op::Blueprints(B::Close), Err, |h| h.close(no, done))
    }
}

impl<H: CodeHost + ?Sized> CodeHost for Wrap<H> {
    fn facts(&mut self) -> Got<Facts> {
        self.call(Op::CodeHost(C::Facts), Err, |h| h.facts())
    }
    fn branch(&mut self, name: &Branch) -> Got<Option<CommitId>> {
        self.call(Op::CodeHost(C::Branch), Err, |h| h.branch(name))
    }
    fn compare(&mut self, base: &CommitId, head: &CommitId) -> Got<Compared> {
        self.call(Op::CodeHost(C::Compare), Err, |h| h.compare(base, head))
    }
    fn file(&mut self, path: &RepoPath, at: &Rev, limit: u64) -> Got<Vec<u8>> {
        self.call(Op::CodeHost(C::File), Err, |h| h.file(path, at, limit))
    }
    fn changes(&mut self, filter: &ChangeFilter, bound: Bound) -> Got<Whole<ChangeDigest>> {
        self.call(Op::CodeHost(C::Changes), Err, |h| h.changes(filter, bound))
    }
    fn change(&mut self, no: ChangeNo, bound: Bound) -> Got<Change> {
        self.call(Op::CodeHost(C::Change), Err, |h| h.change(no, bound))
    }
    fn changed_files(&mut self, no: ChangeNo, bound: Bound) -> Got<Whole<ChangedFile>> {
        self.call(Op::CodeHost(C::ChangedFiles), Err, |h| {
            h.changed_files(no, bound)
        })
    }
    fn activity(&mut self, no: ChangeNo, window: Window) -> Got<Took<ChangeEvent>> {
        self.call(Op::CodeHost(C::Activity), Err, |h| h.activity(no, window))
    }
    fn reviews(&mut self, no: ChangeNo, bound: Bound) -> Got<Whole<ChangeReview>> {
        self.call(Op::CodeHost(C::Reviews), Err, |h| h.reviews(no, bound))
    }
    fn review_comments(&mut self, no: ChangeNo, bound: Bound) -> Got<Whole<ReviewComment>> {
        self.call(Op::CodeHost(C::ReviewComments), Err, |h| {
            h.review_comments(no, bound)
        })
    }
    fn may_write(&mut self, who: &Login) -> Got<bool> {
        self.call(Op::CodeHost(C::MayWrite), Err, |h| h.may_write(who))
    }
    fn checks(&mut self, at: &CommitId, name: Option<&str>) -> Got<Checks> {
        self.call(Op::CodeHost(C::Checks), Err, |h| h.checks(at, name))
    }
    fn tags(&mut self, bound: Bound) -> Got<Whole<Tag>> {
        self.call(Op::CodeHost(C::Tags), Err, |h| h.tags(bound))
    }
    fn resolve_tag(&mut self, name: &TagName) -> Got<CommitId> {
        self.call(Op::CodeHost(C::ResolveTag), Err, |h| h.resolve_tag(name))
    }
    fn recent_tags(&mut self, first: NonZeroU32) -> Got<Took<Tag>> {
        self.call(Op::CodeHost(C::RecentTags), Err, |h| h.recent_tags(first))
    }
    fn recent_releases(&mut self, first: NonZeroU32) -> Got<Took<Release>> {
        self.call(Op::CodeHost(C::RecentReleases), Err, |h| {
            h.recent_releases(first)
        })
    }
    fn releases(&mut self, bound: Bound) -> Got<Whole<Release>> {
        self.call(Op::CodeHost(C::Releases), Err, |h| h.releases(bound))
    }
    fn latest_release(&mut self) -> Got<Option<Release>> {
        self.call(Op::CodeHost(C::LatestRelease), Err, |h| h.latest_release())
    }
    fn release(&mut self, tag: &TagName) -> Got<Release> {
        self.call(Op::CodeHost(C::Release), Err, |h| h.release(tag))
    }
    fn download(
        &mut self,
        tag: &TagName,
        file: &FileName,
        limit: u64,
        into: &mut dyn std::io::Write,
    ) -> Did {
        self.call(Op::CodeHost(C::Download), Err, |h| {
            h.download(tag, file, limit, into)
        })
    }
    fn archive(&mut self, at: &CommitId, limit: u64, into: &mut dyn std::io::Write) -> Did {
        self.call(Op::CodeHost(C::Archive), Err, |h| {
            h.archive(at, limit, into)
        })
    }
    fn changed(&mut self, since: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        self.call(Op::CodeHost(C::Changed), Err, |h| h.changed(since))
    }
    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<ChangeDelta> {
        self.call(Op::CodeHost(C::Delta), Err, |h| h.delta(from, bound))
    }
    fn remote(&mut self, access: Access) -> Got<Remote> {
        self.call(Op::CodeHost(C::Remote), Err, |h| h.remote(access))
    }
}

impl<H: CodeHostWrites + ?Sized> CodeHostWrites for Wrap<H> {
    fn open_change(&mut self, new: &NewChange) -> Got<Posted<ChangeNo>> {
        self.call(Op::CodeHost(C::OpenChange), Err, |h| h.open_change(new))
    }
    fn edit_change(
        &mut self,
        no: ChangeNo,
        title: Option<&str>,
        body: Option<&str>,
        markers: &MarkerEdit,
    ) -> Got<WebLink> {
        self.call(Op::CodeHost(C::EditChange), Err, |h| {
            h.edit_change(no, title, body, markers)
        })
    }
    fn mark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did {
        self.call(Op::CodeHost(C::MarkChange), Err, |h| {
            h.mark_change(no, mark)
        })
    }
    fn unmark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did {
        self.call(Op::CodeHost(C::UnmarkChange), Err, |h| {
            h.unmark_change(no, mark)
        })
    }
    fn comment_change(
        &mut self,
        no: ChangeNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>> {
        self.call(Op::CodeHost(C::CommentChange), Err, |h| {
            h.comment_change(no, body, markers)
        })
    }
    fn close_change(&mut self, no: ChangeNo) -> Did {
        self.call(Op::CodeHost(C::CloseChange), Err, |h| h.close_change(no))
    }
    fn merge(&mut self, no: ChangeNo, head: &CommitId, how: MergeHow) -> Did {
        self.call(Op::CodeHost(C::Merge), Err, |h| h.merge(no, head, how))
    }
    /// A fault stops the proposal before its branch.
    fn propose(
        &mut self,
        p: &Proposal,
        files: &dyn Blobs,
    ) -> Result<Proposed, Applied<ProposalPart>> {
        let fail = |o: Outcome| {
            Err(Applied {
                done: Vec::new(),
                stopped: Some((ProposalPart::Branch, o)),
            })
        };
        self.call(Op::CodeHost(C::Propose), fail, |h| h.propose(p, files))
    }
    fn tag(&mut self, name: &TagName, at: &CommitId) -> Did {
        self.call(Op::CodeHost(C::Tag), Err, |h| h.tag(name, at))
    }
    fn set_status(&mut self, at: &CommitId, status: &Status) -> Did {
        self.call(Op::CodeHost(C::SetStatus), Err, |h| {
            h.set_status(at, status)
        })
    }
    fn new_release(&mut self, new: &NewRelease) -> Got<Posted<ReleaseId>> {
        self.call(Op::CodeHost(C::NewRelease), Err, |h| h.new_release(new))
    }
    fn upload(
        &mut self,
        release: &ReleaseId,
        name: &FileName,
        file: &Sum,
        files: &dyn Blobs,
    ) -> Did {
        self.call(Op::CodeHost(C::Upload), Err, |h| {
            h.upload(release, name, file, files)
        })
    }
    fn publish_release(&mut self, release: &ReleaseId) -> Did {
        self.call(Op::CodeHost(C::PublishRelease), Err, |h| {
            h.publish_release(release)
        })
    }
}

impl<H: Identity + ?Sized> Identity for Wrap<H> {
    fn me(&mut self) -> Got<crate::Account> {
        self.call(Op::Identity(IdentityOp::Me), Err, |h| h.me())
    }
    fn standing(&mut self, who: &AccountId) -> Got<Standing> {
        self.call(Op::Identity(IdentityOp::Standing), Err, |h| h.standing(who))
    }
    fn account(&mut self, who: &AccountId) -> Got<crate::Account> {
        self.call(Op::Identity(IdentityOp::Account), Err, |h| h.account(who))
    }
    fn account_by_login(&mut self, login: &Login) -> Got<crate::Account> {
        self.call(Op::Identity(IdentityOp::AccountByLogin), Err, |h| {
            h.account_by_login(login)
        })
    }
    fn is_organisation(&mut self, name: &Login) -> Got<bool> {
        self.call(Op::Identity(IdentityOp::IsOrganisation), Err, |h| {
            h.is_organisation(name)
        })
    }
    fn assignable(&mut self, who: &AccountId) -> Got<bool> {
        self.call(Op::Identity(IdentityOp::Assignable), Err, |h| {
            h.assignable(who)
        })
    }
    fn token(&mut self, scope: Scope) -> Got<Token> {
        self.call(Op::Identity(IdentityOp::Token), Err, |h| h.token(scope))
    }
    fn token_over(&mut self, scope: Scope, projects: &[ProjectName]) -> Got<Token> {
        self.call(Op::Identity(IdentityOp::TokenOver), Err, |h| {
            h.token_over(scope, projects)
        })
    }
    fn sign_in_link(&self, state: &SignInState) -> Got<WebLink> {
        self.inner.sign_in_link(state)
    }
    fn signed_in(&mut self, code: &SignInCode) -> Got<(crate::Account, Secret)> {
        self.call(Op::Identity(IdentityOp::SignedIn), Err, |h| {
            h.signed_in(code)
        })
    }
}

impl<H: Board + ?Sized> Board for Wrap<H> {
    fn open_work(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork> {
        self.call(Op::Board(BoardOp::OpenWork), Err, |h| {
            h.open_work(ask, bound)
        })
    }
    fn work_item(&mut self, key: &TicketKey, ask: &BoardAsk) -> Got<WorkItem> {
        self.call(Op::Board(BoardOp::WorkItem), Err, |h| h.work_item(key, ask))
    }
    fn closed_work(
        &mut self,
        ask: &BoardAsk,
        since: At,
        bound: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>> {
        self.call(Op::Board(BoardOp::ClosedWork), Err, |h| {
            h.closed_work(ask, since, bound)
        })
    }
    fn target_work(
        &mut self,
        ask: &BoardAsk,
        target: &TargetId,
        bound: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>> {
        self.call(Op::Board(BoardOp::TargetWork), Err, |h| {
            h.target_work(ask, target, bound)
        })
    }
    fn open_changes(&mut self, ask: &ChangeAsk, bound: Bound) -> Got<Whole<ChangeItem>> {
        self.call(Op::Board(BoardOp::OpenChanges), Err, |h| {
            h.open_changes(ask, bound)
        })
    }
    fn as_blockers(&mut self, numbers: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Blocker>)> {
        let fail = |o: Outcome| numbers.iter().map(|&n| (n, Err(o.clone()))).collect();
        self.call(Op::Board(BoardOp::AsBlockers), fail, |h| {
            h.as_blockers(numbers)
        })
    }
}
