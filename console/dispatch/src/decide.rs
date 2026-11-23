//! One poll's decisions, in the order the bash dispatcher took them. Every line
//! it logs and every text it writes on an issue is the bash's, word for word,
//! so the two can be compared line by line.

use crate::settings::{account, Settings};
use crate::slice::gib;
#[cfg(test)]
use crate::snapshot::check_state;
use crate::snapshot::{
    Check, Comment, CommentId, HostStart, Issues, Labels, LimitState, Listed, OrderState,
    PauseFlag, PlanStart, Snapshot, Text, Until, Work, WorkIssue,
};
use crate::text;
use serde_json::{json, Map, Value};
use tehas_core::driver::Driver;
use tehas_core::events::{
    edit_history_unread_of, edit_holds_of, first_unallowed_rename, label_actor, label_state,
    pause_holds, plan_credit, sole_application, standing_actor, standing_past_bot, EditRecord,
    Event, Events, LabelState, Stamp, NOBODY, UNKNOWN_ACTOR,
};
use tehas_core::hold::{Held, Report, Why, HELD_PARKED, HELD_TEXT_MAX};
use tehas_core::kinds::Kind;

mod action;
mod clean;
mod epic;
mod flow;
mod note;
mod plan;
mod retry;
mod starts;

pub(crate) use crate::snapshot::{base_approver, base_stands, named_blockers, record_base, shape};
pub use action::{
    Action, CommentRef, CycleStart, LabelChange, NoteKind, QuietDue, RefreshOf, RetryTick, Spend,
    StartExtra, Turn,
};
pub(crate) use clean::{iso, quiet_due, quiet_why, Due, QUIET_ASKS};
pub(crate) use flow::in_flow;
pub(crate) use note::{in_window, window_order};
pub(crate) use retry::refresh_why;

/// The time now as log() writes it.
pub fn iso_now() -> String {
    iso(tehas_core::time::now() as u64)
}

/// The numbers every blocker comment names.
pub(crate) fn named_blockers_of(nodes: &[Comment]) -> Vec<u64> {
    let mut out: Vec<u64> = nodes
        .iter()
        .filter_map(|x| x.body_text())
        .filter_map(blockers)
        .flatten()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The label that chooses the lean driver for one issue.
pub(crate) const LEAN: &str = Driver::Lean.label();

/// The label that chooses the classic driver for one issue, where
/// `TEHAS_DRIVER=lean` makes lean the default.
pub(crate) const CLASSIC: &str = Driver::Classic.label();

/// The driver labels `Poll::driver` asks the applier of. Under the classic
/// default `tehas:classic` agrees with it and chooses nothing; under
/// `TEHAS_DRIVER=lean` either label is a choice, since with neither the text
/// decides.
pub(crate) fn driver_labels(default: Driver) -> &'static [&'static str] {
    match default {
        Driver::Lean => &[CLASSIC, LEAN],
        Driver::Classic => &[LEAN],
    }
}

/// The label that holds an issue out of rule 6 until its blockers are closed.
/// Spelled once so the read that batchs its reads and the rule that waits on
/// it cannot drift apart.
pub(crate) const BLOCKED: &str = "tehas:blocked";

/// The kind of a proposal the factory files. With TEHAS_ISSUE_TYPES
/// mapping improvement, an issue of that type carries it in the snapshot.
pub(crate) const IMPROVEMENT: &str = Kind::Improvement.label();

/// What asks for the start of an issue `tehas start` approved (#1967), where a
/// label would: no label, so nothing comes off the issue when a rule holds it.
pub(crate) const HOST: &str = "host";

/// The driver a start runs, as `Poll::driver` reads it.
enum Choice {
    Run(Driver),
    /// An account outside the list made the choice, or the issue carries both
    /// driver labels: nothing starts, the default driver included.
    Stranger(Stranger),
    /// Its record could not be read this poll.
    Wait,
}

impl Choice {
    /// The action of a classic start names no driver. A lean start takes
    /// `needs-human-attention` off with its other labels: classic's
    /// `--restart` clears it in the cycle, and work-lean has no restart.
    fn bind(&self, extra: &mut StartExtra, remove: &mut Vec<String>, labels: &[String]) {
        if matches!(self, Choice::Run(Driver::Lean)) {
            extra.driver = Some(Driver::Lean);
            let attention = "needs-human-attention";
            if has(labels, attention) && !remove.iter().any(|l| l == attention) {
                remove.push(attention.into());
            }
        }
    }
}

/// What the record of a plan's start makes of a queued issue, as `plan_start`
/// reads it.
enum Planned {
    /// Nothing: the label is judged as any other.
    No,
    /// The bot's label came off, and nothing starts.
    Refused,
    /// The account and the time a start is credited with.
    Go(String, String),
}

/// Who made the choice of driver that does not count: applied `label` (one of
/// [`driver_labels`]), or with `off` took an allowed account's off. With
/// `both`, the issue carries both driver labels and nobody's choice is read.
/// `lean_default` is `TEHAS_DRIVER=lean`, which says what runs with no label.
/// With `building`, the start would run lean and the lean feature is not on
/// in this release: nobody's choice is wrong, but nothing starts and classic
/// does not run in its place.
struct Stranger {
    who: String,
    off: bool,
    label: &'static str,
    both: bool,
    building: bool,
    lean_default: bool,
}

impl Stranger {
    /// Both driver labels are on the issue.
    fn both() -> Stranger {
        Stranger {
            who: String::new(),
            off: false,
            label: LEAN,
            both: true,
            building: false,
            lean_default: false,
        }
    }

    /// The start would run lean, and the lean feature is off.
    fn building() -> Stranger {
        Stranger {
            who: String::new(),
            off: false,
            label: LEAN,
            both: false,
            building: true,
            lean_default: false,
        }
    }

    /// The driver that runs when `label` is off, and that tehas does not run
    /// in its place. Under `TEHAS_DRIVER=lean` that is whichever the issue's
    /// text gives, for either label.
    fn default_name(&self) -> &'static str {
        if self.lean_default {
            "the driver its text would choose"
        } else {
            Driver::Classic.word()
        }
    }

    /// What a start refused over it says on the issue.
    fn said(&self) -> String {
        if self.building {
            return format!("`{LEAN}` asks for the lean driver, which is still being built in this release, and tehas does not run classic in its place. An account named in `TEHAS_ALLOWED_LABELERS` takes `{LEAN}` off");
        }
        if self.both {
            return format!("it carries both `{LEAN}` and `{CLASSIC}`, and tehas does not guess which driver to run. An account named in `TEHAS_ALLOWED_LABELERS` takes one of them off");
        }
        let (label, other) = (self.label, self.default_name());
        let (what, rest) = if self.off {
            ("taken off", format!("and takes it off for {other}"))
        } else {
            ("applied", format!("or takes it off for {other}"))
        };
        format!("`{label}` was {what} by `{}`, an account that may not choose the driver in this factory, and tehas does not run {other} in its place. An account named in `TEHAS_ALLOWED_LABELERS` applies `{label}` again, {rest}", self.who)
    }

    /// What a dry run logs in place of the refusal.
    fn dry_line(&self) -> String {
        if self.building {
            format!("{LEAN} asks for lean, still being built in this release")
        } else if self.both {
            format!("it carries both {LEAN} and {CLASSIC}")
        } else {
            format!("{} is not an allowed account's", self.label)
        }
    }
}

/// What a poll does: its exit status, the lines it logs, the writes it makes,
/// in order, and the files it publishes.
#[derive(Clone, Debug, Default)]
pub struct Decision {
    pub exit: i32,
    pub log: Vec<String>,
    pub actions: Vec<Action>,
    /// live/dispatch.json, without its time. A dry run writes none. The keys
    /// and the hold's words are tehas-core's, so the readers of the file name
    /// them by the same type the writer does.
    pub report: Option<Report>,
    /// live/demand.json, without its time: Some(Unread) removes it. A dry
    /// run writes none.
    pub demand: Option<Demand>,
    /// The issues the demand left out only for the planning session open on
    /// each, and when each began to wait: a poll that ends one of those
    /// sessions counts it again (`demand_after`).
    pub demand_planning: Vec<(u64, String)>,
    /// The issues whose cycle starts, in order.
    pub picked: Vec<u64>,
    /// Facts the snapshot lacked that this decision needed. When any is
    /// named, the caller reads them and decides again.
    pub needs: Vec<&'static str>,
    /// Past the bot's checks: the poll's end runs tidy from here on.
    pub tidy: bool,
    /// A pause holds the factory: the poll's finish step waits with it.
    pub paused: bool,
    /// The bot's login, which the poll's writes name.
    pub bot: String,
    /// The poll's steps in order: its lines, its actions, and where it sweeps
    /// the unused locks (after its clean-up) and publishes its report (at
    /// rule 3), as the bash does them there.
    pub plan: Vec<PlanStep>,
    /// What the poll decided of each issue it could start, and the holds
    /// between issues it read: state/starts.json, without its time. A dry run
    /// writes none.
    pub starts: tehas_core::starts::Record,
}

/// One step of a poll's plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanStep {
    /// Log the line at this index of `log`.
    Say(usize),
    /// Apply the action at this index of `actions`.
    Act(usize),
    SweepLocks,
    Report,
}

/// live/demand.json, without its time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Demand {
    /// A read the count needs failed: the file is removed.
    Unread,
    Counted(DemandCount),
}

/// The open issues that would start with room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DemandCount {
    pub waiting: u64,
    /// When the oldest was accepted.
    pub oldest: Option<String>,
    /// Why the image holds every refresh.
    pub refresh_hold: Option<String>,
    /// The issues held only by a blocked label (`label_alone`).
    pub label_alone: Vec<u64>,
}

/// The file's keys: Null for Unread, and a key left out when it is empty.
impl From<&Demand> for Value {
    fn from(d: &Demand) -> Self {
        let Demand::Counted(c) = d else {
            return Value::Null;
        };
        let mut m = Map::new();
        m.insert("waiting".into(), json!(c.waiting));
        if let Some(o) = &c.oldest {
            m.insert("oldest".into(), json!(o));
        }
        if let Some(h) = &c.refresh_hold {
            m.insert("refreshHold".into(), json!(h));
        }
        if !c.label_alone.is_empty() {
            m.insert("labelAlone".into(), json!(c.label_alone));
        }
        Value::Object(m)
    }
}

/// The demand a poll wrote, with the issues it left out for a planning
/// session counted once the poll ended that session, as the next poll would
/// count them. None when it ended none of them: the file stays as it is.
pub fn demand_after(demand: &Demand, planning: &[(u64, String)], ended: &[u64]) -> Option<Demand> {
    let freed: Vec<&str> = planning
        .iter()
        .filter(|(n, _)| ended.contains(n))
        .map(|(_, at)| at.as_str())
        .collect();
    if freed.is_empty() {
        return None;
    }
    let Demand::Counted(c) = demand else {
        return None;
    };
    let mut c = c.clone();
    let mut oldest = c.oldest.take().unwrap_or_default();
    for at in &freed {
        if !at.is_empty() && (oldest.is_empty() || *at < oldest.as_str()) {
            oldest = at.to_string();
        }
    }
    c.waiting += freed.len() as u64;
    c.oldest = (!oldest.is_empty()).then_some(oldest);
    Some(Demand::Counted(c))
}

/// The poll ends here with this exit status.
struct End(i32);
type Step<T = ()> = Result<T, End>;

struct Start {
    starter: String,
    verb: String,
    how: String,
    given_at: String,
    given_how: String,
    remove: Vec<String>,
    /// What a start of its kind adds to the action: the comment of a retry
    /// tick, the pull request of a refresh.
    extra: StartExtra,
}

/// What an issue's epic says of the millwright's label on the issue, under
/// TEHAS_MILLWRIGHT_EPICS.
enum EpicPart {
    /// No epic a person made ready stands over it.
    None,
    /// One stands, and the issue may not run on it: why.
    Refused(String),
    /// It stands: the person's approval of the epic.
    Granted(EpicGrant),
}

#[derive(Clone)]
struct EpicGrant {
    by: String,
    at: String,
    epic: u64,
}

/// The labels that keep the millwright from starting an issue on no
/// approval of the issue itself.
const MILLWRIGHT_HELD: [&str; 3] = [
    IMPROVEMENT,
    "needs-human-attention",
    "tehas:millwright-escalated",
];

struct Poll<'a> {
    s: &'a Snapshot,
    c: &'a Settings,
    dry: bool,
    d: Decision,
    control_skipped: bool,
    work_refused: Vec<u64>,
    starts: Vec<(u64, Start)>,
    /// image_ok's answer, asked once a poll: the hold, empty for none.
    image_hold: Option<String>,
    /// The finished cycles this poll stops: their slots are free.
    stopped: Vec<u64>,
    /// The bot's login, once the poll has read it.
    bot: String,
    /// "hold refreshes" was said this poll.
    refresh_said: bool,
    /// The hold of a host whose sandbox cannot push, empty when it can: the one
    /// line said as the hold of the poll while the fault stands on record.
    push_hold: String,
    /// Parked issues with a box ticked: rule 5a leaves them.
    park_ticked: Vec<u64>,
    /// The issue whose planning turn is still running, and the sessions
    /// this poll closed.
    plan_busy: Option<u64>,
    plan_closed: Vec<u64>,
    /// Where rules 6 and 7 began, as the line and the action, once they did.
    cand_from: Option<(usize, usize)>,
    /// The issues of rule 7's order, refreshes left out, and those of them the
    /// rules came to before the room ran out.
    cand_order: Vec<u64>,
    cand_seen: Vec<u64>,
    /// The hold of a poll with no room for a new cycle.
    room_hold: String,
}

/// The decisions of one poll, a dry run when `dry`.
pub fn decide(s: &Snapshot, c: &Settings, dry: bool) -> Decision {
    let mut p = Poll {
        s,
        c,
        dry,
        d: Decision::default(),
        control_skipped: false,
        work_refused: Vec::new(),
        starts: Vec::new(),
        image_hold: None,
        stopped: Vec::new(),
        bot: String::new(),
        refresh_said: false,
        push_hold: String::new(),
        park_ticked: Vec::new(),
        plan_busy: None,
        plan_closed: Vec::new(),
        cand_from: None,
        cand_order: Vec::new(),
        cand_seen: Vec::new(),
        room_hold: String::new(),
    };
    p.d.exit = match p.run() {
        Ok(()) => 0,
        Err(End(rc)) => rc,
    };
    p.d.starts = p.starts_record();
    p.d
}

/// The label vocabulary of a cycle: a label is present when an issue's own
/// list holds it exactly.
pub(crate) fn has(labels: &[String], l: &str) -> bool {
    labels.iter().any(|x| x == l)
}

fn or_unknown(who: &str) -> &str {
    if who.is_empty() {
        UNKNOWN_ACTOR
    } else {
        who
    }
}

impl<'a> Poll<'a> {
    fn log(&mut self, line: impl Into<String>) {
        self.d.plan.push(PlanStep::Say(self.d.log.len()));
        self.d.log.push(line.into());
    }

    fn fail(&mut self, line: impl Into<String>, rc: i32) -> End {
        self.log(line);
        End(rc)
    }

    /// The end of a poll on a read the rules cannot use. The line names the
    /// read, and the line of the gh call it failed on when one failed: a read
    /// that is missing or that no gh call failed for blames no call.
    fn unreadable(&mut self, what: &str, call: Option<&str>) -> End {
        let line = match call {
            Some(l) => format!(
                "ERROR: unreadable {what}: {}",
                l.strip_prefix("ERROR: ").unwrap_or(l)
            ),
            None => format!("ERROR: unreadable {what}"),
        };
        self.fail(tehas_core::call::trim(&line), 1)
    }

    /// The end of a poll on comments of `n` that were not read.
    fn comments_unread(&mut self, n: u64) -> End {
        let call = self
            .s
            .issues
            .get(&n)
            .and_then(|i| i.comments_failed.clone());
        self.unreadable(&format!("comments of #{n}"), call.as_deref())
    }

    /// An action, in its place among the poll's lines.
    fn act(&mut self, a: Action) {
        self.d.plan.push(PlanStep::Act(self.d.actions.len()));
        self.d.actions.push(a);
    }

    /// The line the poll logs when a label change fails: the flags it
    /// gave gh, as `--remove-label X --add-label Y`.
    fn label_fail(n: u64, remove: &[&str], add: &[&str]) -> String {
        let mut flags: Vec<String> = remove
            .iter()
            .map(|l| format!("--remove-label {l}"))
            .collect();
        flags.extend(add.iter().map(|l| format!("--add-label {l}")));
        format!("WARN: label change on #{n} failed: {}", flags.join(" "))
    }

    fn need(&mut self, what: &'static str) {
        if !self.d.needs.contains(&what) {
            self.d.needs.push(what);
        }
    }

    /// The cycles that hold their lock, less those this poll stops.
    fn live(&self) -> impl Iterator<Item = &(u64, String)> {
        self.s
            .running
            .iter()
            .filter(|(n, _)| !self.stopped.contains(n))
    }

    fn running(&self, n: u64) -> bool {
        self.live().any(|(r, _)| *r == n)
    }

    /// TEHAS_READ_ONLY (#608): a start of this host is on record for the
    /// issue, and the labels that would say so on GitHub were never written.
    /// Said in a dry run only, as each poll would say it again.
    fn read_only_ran(&mut self, n: u64) -> bool {
        if !self.c.read_only || !self.s.read_only_ran.contains(&n) {
            return false;
        }
        if self.dry {
            self.log(format!(
                "skip #{n}: read-only, and this host started it before: remove state/read-only/{n}.json to start it again"
            ));
        }
        true
    }

    fn plan_open(&self, n: u64) -> bool {
        matches!(&self.s.plan_open, Ok(v) if v.contains(&n)) && !self.plan_closed.contains(&n)
    }

    /// An issue's label events, or the end of the poll.
    fn events(&mut self, n: u64) -> Step<Vec<Event>> {
        let e = self
            .s
            .issues
            .get(&n)
            .map(|i| i.events.clone())
            .unwrap_or(Events::Failed);
        match e {
            Events::Read(v) => Ok(v),
            Events::Failed => {
                let repo = self.c.repo.clone();
                Err(self.fail(
                    format!("ERROR: gh api repos/{repo}/issues/{n}/events?per_page=100 failed"),
                    1,
                ))
            }
            Events::Unreadable => {
                Err(self.fail(format!("ERROR: unreadable label events of #{n}"), 1))
            }
        }
    }

    fn list(&mut self, l: &Issues) -> Step<Vec<Listed>> {
        match &l.0 {
            Some(v) => Ok(v.clone()),
            None => Err(self.fail("ERROR: gh issue list failed", 1)),
        }
    }

    fn run(&mut self) -> Step {
        let pause = self.pause_flag();
        let s = self.s;
        let c = self.c;
        let Some(actual) = s.repo.clone() else {
            return Err(self.fail("ERROR: gh repo view failed", 1));
        };
        if actual.to_lowercase() != c.repo.to_lowercase() {
            let actual = or_unknown(&actual).to_string();
            return Err(self.fail(
                format!(
                    "ERROR: {} is now {actual} on GitHub: set TEHAS_REPO in {}/instance.env and the checkout's origin",
                    c.repo, c.instance_dir
                ),
                1,
            ));
        }
        let bot = c.bot_login.clone();
        if bot.is_empty() {
            return Err(self.fail("ERROR: the bot's own login could not be read", 1));
        }
        if c.allowed(&bot) || c.allowed(crate::settings::account(&bot)) {
            return Err(self.fail(
                format!("ERROR: the bot account {bot} is in TEHAS_ALLOWED_LABELERS; the bot never authorises work"),
                78,
            ));
        }
        if let Some(t) = &c.improvement_type {
            match &s.issue_types {
                None => {
                    return Err(self.fail(
                        format!("ERROR: the issue types of {}'s owner could not be read", c.repo),
                        1,
                    ))
                }
                Some(types) if !types.contains(t) => {
                    return Err(self.fail(
                        format!("ERROR: TEHAS_ISSUE_TYPES maps improvement to {t}, which is no issue type of {}'s owner: name one in {}/instance.env, or leave improvement out for the tehas:improvement label", c.repo, c.instance_dir),
                        78,
                    ))
                }
                Some(_) => {}
            }
        }
        self.d.tidy = true;
        self.d.bot = bot.clone();
        if let Err((word, rc)) = &s.plan_open {
            let rc = if *rc == 1 { 1 } else { 78 };
            return Err(self.fail(
                format!(
                    "ERROR: the state store could not list the open planning sessions ({word})"
                ),
                rc,
            ));
        }
        if s.running_unlisted {
            return Err(self.fail(
                "ERROR: state/cycles could not be listed, so no cycle can be told to run or not: the poll acts on none",
                1,
            ));
        }
        self.bot = bot.clone();
        if !self.dry {
            let demand = self.demand();
            self.d.demand = Some(demand);
        }
        let (holds, news) = match pause {
            Some(h) => (Some(h), false),
            None => self.pause_label()?,
        };
        self.clean_up(&bot)?;
        self.d.plan.push(PlanStep::SweepLocks);
        // 1b. The pause switches end the poll here.
        if let Some(h) = holds {
            self.d.paused = true;
            if self.dry || news {
                self.log(format!("paused: {h}"));
            }
            return Ok(());
        }
        self.epic_titles();
        let room = self.room()?;
        self.questions(&room, &bot)?;
        self.plan_new(&room)?;
        self.plan_refine(&room)?;
        self.start_work(room)?;
        self.hold_cleared();
        Ok(())
    }

    fn start_work(&mut self, room: Room) -> Step {
        // With no room for new work, only a refresh may still have room.
        if room.want == 0 {
            self.room_hold = room.hold.clone();
            if self.dry {
                self.log(room.hold.clone());
            }
            if room.want_all <= 0 || self.s.refresh_requests.is_empty() {
                return Ok(());
            }
        }
        self.outage(&room)?;
        self.candidates(room)
    }

    /// 1. The flag tehas pause writes: what holds this host, if anything.
    fn pause_flag(&mut self) -> Option<String> {
        match self.s.pause_flag.as_ref()? {
            PauseFlag::Held { at } => Some(format!(
                "the flag tehas pause wrote at {at} holds this host"
            )),
            PauseFlag::Unreadable => {
                let home = self.c.home.clone();
                self.log(format!(
                    "WARN: {home}/state/pause.json is not a pause flag: taking it as paused"
                ));
                Some("the pause flag is there and cannot be read".into())
            }
        }
    }

    /// 1. The label on the control issue, and who took it off.
    fn pause_label(&mut self) -> Step<(Option<String>, bool)> {
        let control = self.c.control;
        let Some(labels) = self.s.control_labels.clone() else {
            return Err(self.fail("ERROR: gh issue view failed", 1));
        };
        if has(&labels, "tehas:pause") {
            return Ok((Some(format!("issue #{control} carries tehas:pause")), false));
        }
        let events = match self.s.control_events.clone().unwrap_or(Events::Failed) {
            Events::Read(v) => v,
            Events::Failed => {
                let repo = self.c.repo.clone();
                return Err(self.fail(
                    format!(
                        "ERROR: gh api repos/{repo}/issues/{control}/events?per_page=100 failed"
                    ),
                    1,
                ));
            }
            Events::Unreadable => {
                return Err(self.fail(format!("ERROR: unreadable label events of #{control}"), 1))
            }
        };
        let c = self.c;
        Ok(match pause_holds(&events, &|l| c.allowed(l)) {
            Some((kind, by)) if kind == "unlabeled" => (
                Some(format!(
                    "tehas:pause was taken off #{control} by '{}', who may not authorise work here; the factory stays paused until an allowed account applies tehas:pause and takes it off again",
                    or_unknown(&by)
                )),
                true,
            ),
            Some(_) => (
                Some(format!(
                    "tehas:pause is not on #{control}, but its last tehas:pause event only applied it: nothing has taken the pause off"
                )),
                true,
            ),
            None => (None, false),
        })
    }

    /// report_demand: the open issues that would start with room, and when
    /// the oldest was accepted. A read that fails removes the file.
    fn demand(&mut self) -> Demand {
        let s = self.s;
        let Some(list) = &s.ready.0 else {
            return Demand::Unread;
        };
        let (mut waiting, mut oldest) = (0, String::new());
        let mut counted: Vec<u64> = Vec::new();
        for i in list.iter().filter(|i| !has(&i.labels, "tehas:running")) {
            let Some(n) = i.number else { continue };
            if n == self.c.control || self.running(n) {
                continue;
            }
            let planning = self.plan_open(n);
            let ev = match self.s.issues.get(&n).map(|x| &x.events) {
                Some(Events::Read(v)) => v,
                _ if planning => continue,
                _ => return Demand::Unread,
            };
            let (by, at) = standing_actor(ev, "tehas:ready");
            if !self.c.allowed(&by) && !self.c.is_millwright(&by) && !self.planned(n, &by) {
                continue;
            }
            if planning {
                self.d.demand_planning.push((n, at));
                continue;
            }
            counted.push(n);
            waiting += 1;
            if !at.is_empty() && (oldest.is_empty() || at < oldest) {
                oldest = at;
            }
        }
        // A Retry ticked on a park waits as a ready issue does: rule 4 starts
        // it once there is room, so with no room it is demand (#1776).
        for (n, at) in self.retries_ticked() {
            if counted.contains(&n) {
                continue;
            }
            waiting += 1;
            if !at.is_empty() && (oldest.is_empty() || at < oldest) {
                oldest = at;
            }
        }
        // A refresh request waits as a ready issue does, unless the image holds
        // every refresh. The hold is in the file, and the log has it once.
        let mut held = String::new();
        let requests = self.s.refresh_requests.clone();
        if !requests.is_empty() {
            held = self.refresh_held().unwrap_or_default();
            if held.is_empty() {
                for n in requests {
                    if self.running(n) {
                        continue;
                    }
                    let at = self
                        .s
                        .issues
                        .get(&n)
                        .and_then(|i| i.request.as_ref())
                        .map(|r| r.at.clone())
                        .unwrap_or_default();
                    if self.plan_open(n) {
                        self.d.demand_planning.push((n, at));
                        continue;
                    }
                    waiting += 1;
                    if !at.is_empty() && (oldest.is_empty() || at < oldest) {
                        oldest = at;
                    }
                }
            }
            if held != self.s.refresh_held {
                if !held.is_empty() {
                    self.log(format!("hold refreshes: {held}"));
                }
                self.act(Action::RefreshHeld(held.clone()));
            }
            self.refresh_said = true;
        } else if !self.s.refresh_held.is_empty() {
            self.act(Action::RefreshHeld(String::new()));
        }
        Demand::Counted(DemandCount {
            waiting,
            oldest: (!oldest.is_empty()).then_some(oldest),
            refresh_hold: (!held.is_empty()).then_some(held),
            label_alone: self.label_alone(),
        })
    }

    /// The parked issues with a question on record where one allowed account
    /// ticked a Retry, and when: what rule 4 would start with room. An issue
    /// whose question or comments this poll did not read is left out.
    fn retries_ticked(&self) -> Vec<(u64, String)> {
        let Some(Some(list)) = &self.s.asking else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for i in list {
            let Some(n) = i.number else {
                continue;
            };
            let labels = &i.labels;
            if n == self.c.control
                || self.plan_open(n)
                || self.running(n)
                || has(labels, "tehas:running")
                || has(labels, "tehas:awaiting-merge")
                || !has(labels, "needs-human-attention")
            {
                continue;
            }
            let Some(Ok(q)) = self.s.issues.get(&n).and_then(|x| x.question.as_ref()) else {
                continue;
            };
            if q.kind != "park" {
                continue;
            }
            if let Some(Ok((_, at))) = self.retry_tick(n, Some(&q.id)) {
                out.push((n, at));
            }
        }
        out
    }

    /// The issues `tehas:blocked` holds by the label alone: rule 6 would wait
    /// on them with `LABEL_ALONE`, as `order_label` and `newest_marker` read
    /// them. An issue whose events or comments this poll did not read is left
    /// out: the rule ends the poll on such a read, so it is not reported as
    /// held. At most `LABEL_ALONE_MAX`, by number.
    fn label_alone(&self) -> Vec<u64> {
        let mut seen: Vec<u64> = Vec::new();
        let lists = [&self.s.ready.0, &self.s.blocked.0];
        for i in lists.into_iter().flatten().flatten() {
            let Some(n) = i.number else { continue };
            if !has(&i.labels, BLOCKED)
                || in_flow(&i.labels)
                || n == self.c.control
                || self.plan_open(n)
                || self.running(n)
                || seen.contains(&n)
            {
                continue;
            }
            let Some(issue) = self.s.issues.get(&n) else {
                continue;
            };
            let Events::Read(ev) = &issue.events else {
                continue;
            };
            if !self.order_stands(&label_state(ev, BLOCKED)) {
                continue;
            }
            let Some(nodes) = issue.comments.as_ref().and_then(|c| c.listed()) else {
                continue;
            };
            if newest_marker(nodes, self.c).is_none() {
                seen.push(n);
            }
        }
        seen.sort_unstable();
        seen.truncate(LABEL_ALONE_MAX);
        seen
    }
}

impl<'a> Poll<'a> {
    /// Says once that an issue's `tehas:blocked` hold has cleared when it did
    /// not start this poll. Until the start takes the label off, the issue
    /// reads as held, and taking the label off by hand withdraws the start an
    /// allowed account asked for with it. A newer marker comment is a new hold,
    /// so its clearing is said again.
    fn hold_cleared(&mut self) {
        let mut cleared: Vec<u64> = Vec::new();
        let lists = [&self.s.ready.0, &self.s.blocked.0];
        for i in lists.into_iter().flatten().flatten() {
            let Some(n) = i.number else { continue };
            if !has(&i.labels, BLOCKED)
                || in_flow(&i.labels)
                || n == self.c.control
                || self.plan_open(n)
                || self.running(n)
                || self.d.picked.contains(&n)
                || cleared.contains(&n)
            {
                continue;
            }
            let Some(issue) = self.s.issues.get(&n) else {
                continue;
            };
            let Events::Read(ev) = &issue.events else {
                continue;
            };
            let state = label_state(ev, BLOCKED);
            if state.event() != "labeled" || !self.c.allowed(state.actor()) {
                continue;
            }
            let links = self.work(n).map_or(0, |w| w.blocked_by);
            let Some(nodes) = issue.comments.as_ref().and_then(|c| c.listed()) else {
                continue;
            };
            let Some(marker) = newest_marker(nodes, self.c) else {
                continue;
            };
            if links > 0 || !self.marker_hold(n, nodes).is_empty() {
                continue;
            }
            let at = |x: &Comment| x.created_text().unwrap_or_default().to_string();
            let since = at(marker);
            let said = nodes.iter().any(|x| {
                x.body_text().is_some_and(|b| b.contains(HOLD_CLEARED))
                    && tehas_core::accounts::factorys(&x.author.login, &self.c.bot_login)
                    && at(x) >= since
            });
            if !said {
                cleared.push(n);
            }
        }
        for n in cleared {
            if self.dry {
                self.log(format!(
                    "would say on #{n}: its tehas:blocked hold has cleared"
                ));
                continue;
            }
            self.act(Action::Say { issue: n, say: format!("Every issue the `tehas:blocked` marker names is closed, so the hold has cleared. Tehas takes `tehas:blocked` off when the cycle starts. Leave it on until then: it is what asks for the start when `tehas:ready` is not on.\n\n{HOLD_CLEARED}") });
        }
    }
}

/// What rules 2 and 3 leave for the candidates.
struct Room {
    want: i64,
    want_all: i64,
    /// The refresh lane's share of want_all, when there is a lane.
    want_refresh: Option<i64>,
    hold: String,
    /// slice_ok, limit_ok and limit_plan_ok: what holds a planning turn.
    slice_ok: bool,
    limit_ok: bool,
    limit_plan_ok: bool,
}

impl<'a> Poll<'a> {
    /// The ready list, read once a poll: the open issues labelled tehas:ready,
    /// and those labelled tehas:blocked in no other flow, by number.
    fn ready(&mut self) -> Step<Vec<Listed>> {
        let ready = self.list(&self.s.ready.clone())?;
        let blocked = self.list(&self.s.blocked.clone())?;
        let mut all: Vec<Listed> = ready;
        all.extend(blocked.into_iter().filter(|i| !in_flow(&i.labels)));
        match &self.s.host_starts {
            Some(hosts) => all.extend(hosts.iter().filter_map(|h| {
                Some(Listed {
                    number: Some(h.issue?),
                    labels: h.labels.clone(),
                    state: "OPEN".into(),
                })
            })),
            None => self.log(
                "WARN: the host's approvals could not be read from the state store: no issue starts on one this poll",
            ),
        }
        all.sort_by_key(|i| i.number);
        all.dedup_by_key(|i| i.number);
        Ok(all)
    }

    /// refresh_waits: a refresh request is on this host.
    fn refresh_waits(&self) -> bool {
        !self.s.refresh_requests.is_empty()
    }

    /// The approval `tehas start` recorded for `n` that waits for a start.
    fn host_start(&self, n: u64) -> Option<HostStart> {
        self.s
            .host_starts
            .as_ref()?
            .iter()
            .find(|h| h.issue == Some(n))
            .cloned()
    }

    /// host_ok: the host's approval binds the title and the text it was given
    /// on; a change since starts nothing until the host approves it again. The
    /// time of the approval, or None.
    fn host_ok(&mut self, n: u64, h: &HostStart) -> Option<String> {
        let at = h.at.map(iso)?;
        let t = self.text(n);
        let body = h.body_sha256.as_deref() == Some(&text::body_sum(t.body_text()));
        let title = match h.title_sha256.as_deref() {
            Some(sum) => text::title_sum(t.title.as_deref()).as_deref() == Some(sum),
            None => true,
        };
        if body && title {
            return Some(at);
        }
        self.log(format!(
            "skip #{n}: its title or text changed since the host approved it at {at}; run `tehas start {n}` again to approve it"
        ));
        None
    }

    /// 2 and 3: the slots, the limit, the slice and the cap on unreviewed
    /// work, with the hold that keeps cycles back when one does.
    fn room(&mut self) -> Step<Room> {
        let s = self.s;
        let c = self.c;
        let count = self.live().count() as i64;
        let mut cycles = count;
        let mut lane: Option<i64> = None;
        let refreshing = self.live().filter(|(_, how)| how == "refresh").count() as i64;
        // The lane is kept only while a request waits or a refresh runs, so
        // the cycles keep every slot otherwise.
        if c.refresh_slots > 0 && (refreshing > 0 || self.refresh_waits()) {
            cycles = count - refreshing;
            lane = Some((c.refresh_slots - refreshing).max(0));
        }
        let concurrency = c.concurrency;
        let free = concurrency - cycles;
        let limit_state = s.limit.state;
        let limit_cycles = s.limit.cycles;
        let limit_reason = s.limit.reason.clone();
        let limit_room: Option<i64> = match limit_state {
            LimitState::Ok => Some((limit_cycles - count).max(0)),
            LimitState::Expired | LimitState::Unreadable => Some(0),
            LimitState::None => None,
        };
        let limit_words = if limit_cycles == 1 {
            "1 cycle".to_string()
        } else {
            format!("{limit_cycles} cycles")
        };
        // The plan of the file, read the way the dispatcher's other reader of
        // it does: a boolean or the text of one.
        let limit_plan_off = s.limit.plan_off;
        let with_reason = |r: &str| {
            if r.is_empty() {
                String::new()
            } else {
                format!(": {r}")
            }
        };

        // The slice.
        let (mut room_mem, mut committed, mut slice_max) = (0u64, 0u64, 0u64);
        let mut slice_hold = String::new();
        let mut mem_why = String::new();
        let mut mem_note = String::new();
        let mut pressure = None;
        let mut ceiling_mb = 0;
        // The word of the slice's own reason, remembered for the arm that
        // answers with it later: the slice's text decides it, and only the
        // arms of `slice::room` name it.
        let mut slice_word = None;
        match s.slice.usable(&c.reserve, &c.slice_containers) {
            Ok(u) => {
                ceiling_mb = u.ceiling_mb;
                let r = s.slice.room(&u, &c.reserve, count as u64);
                room_mem = r.room;
                committed = r.committed;
                slice_max = r.max;
                mem_why = r.why;
                mem_note = r.note;
                pressure = r.pressure;
                match mem_why.as_str() {
                    "pressure" | "unreadable" | "full" => room_mem = 0,
                    _ => {
                        mem_why = if room_mem == 0 {
                            "room".into()
                        } else {
                            String::new()
                        }
                    }
                }
                slice_word = Why::parse(&mem_why);
            }
            Err(why) => slice_hold = format!("holding: {why}"),
        }
        let slice_ok = slice_hold.is_empty() && mem_why != "unreadable";
        let limit_ok = !matches!(limit_state, LimitState::Expired | LimitState::Unreadable);
        let limit_plan_ok = limit_ok && !limit_plan_off;
        for line in s.endpoint_log.clone() {
            self.log(line);
        }
        let room_mem_i = room_mem as i64;
        let (mut hold, mut why) = (String::new(), None);
        let (mut want, mut want_all) = (0i64, 0i64);
        let mut want_refresh = None;
        let mut cut = "";
        let mut await_room: Option<i64> = None;
        let mut awaiting = 0i64;
        let disk_low = |p: &mut Self| -> Option<Option<u64>> {
            match s.disk_kb {
                Some(kb) => Some(kb),
                None => {
                    p.need("disk");
                    None
                }
            }
        };
        if !slice_hold.is_empty() {
            why = Some(Why::Slice);
            hold = slice_hold;
        } else if mem_why == "unreadable" {
            why = Some(Why::Unreadable);
            hold = format!(
                "holding: the slice's use could not be read{}",
                if mem_note.is_empty() {
                    String::new()
                } else {
                    format!(": {mem_note}")
                }
            );
        } else if !s.endpoint_hold.is_empty() {
            why = Some(Why::Resources);
            hold = s.endpoint_hold.clone();
        } else if let Some(Some(kb)) = disk_low(self)
            .filter(|kb| matches!(kb, Some(k) if *k < c.min_free_mb.saturating_mul(1024)))
        {
            why = Some(Why::Disk);
            hold = format!(
                "holding: {} free on the disk, under TEHAS_CYCLE_MIN_FREE={}: no cycle starts",
                gib(kb * 1024),
                c.min_free
            );
        } else if matches!(s.disk_kb, Some(None)) {
            why = Some(Why::Disk);
            hold = "holding: the free disk space could not be read: no cycle starts".into();
        } else if limit_state == LimitState::Expired {
            why = Some(Why::Limit);
            hold = format!(
                "holding: the limit expired at {}: nothing starts until it is set again or cleared",
                match &s.limit.until {
                    Until::At(t) => t.as_str(),
                    Until::Unset | Until::Other => "null",
                }
            );
        } else if limit_state == LimitState::Unreadable {
            why = Some(Why::Limit);
            hold = format!(
                "holding: the limit cannot be read: nothing starts until it is set again or cleared ({}/state/limit.json)",
                c.home
            );
        } else if matches!(limit_room, Some(r) if r <= 0) {
            why = Some(Why::Limit);
            // A drain is a limit of no cycles at once which no clock ends: it
            // stays until a person clears it, so the line names the command
            // that frees the factory. Another limit under its own ceiling is
            // no drain and keeps its words.
            let drained =
                limit_cycles == 0 && count == 0 && limit_plan_off && s.limit.until == Until::Unset;
            hold = format!(
                "holding: the limit allows {limit_words} at once and {count} {} running{}{}",
                if count == 1 { "is" } else { "are" },
                with_reason(&limit_reason),
                if drained {
                    "; tehas limit clear starts the factory"
                } else {
                    ""
                }
            );
        } else if self.push_fault() {
            // A sandbox that cannot push starts no cycle. The word is `lane`,
            // the one of the thirteen a reader outside the dispatcher already
            // uses for the host that keeps what it has while nothing new
            // starts: the cycles that run, the planning turns and the
            // refreshes of a pull request that already stands on the remote
            // are untouched by it.
            why = Some(Why::Lane);
            hold = self.push_hold.clone();
            // Cycles start none; the refreshes keep what they would have been
            // given without the fault: the lane when the setting keeps one,
            // else the slots of the cycle ceiling, which is `None` at rule 6.
            want_all = free + lane.unwrap_or(0);
            if let Some(l) = limit_room {
                want_all = want_all.min(l);
            }
            want_all = want_all.min(room_mem_i);
            want_refresh = lane.map(|l| l.min(want_all));
        } else if let Some(word) = self.branch_check() {
            why = Some(word.0);
            hold = word.1;
        } else if free <= 0 {
            why = Some(Why::Count);
            hold = format!("holding: {cycles} cycles are running (limit {concurrency})");
            if let Some(lane) = lane {
                hold = format!(
                    "{hold}; the refresh lane has {lane} of {} free",
                    c.refresh_slots
                );
                want_all = lane;
                if let Some(l) = limit_room {
                    want_all = want_all.min(l);
                }
                want_all = want_all.min(room_mem_i);
                want_refresh = Some(want_all);
            }
        } else {
            want_all = free + lane.unwrap_or(0);
            if let Some(l) = limit_room {
                want_all = want_all.min(l);
            }
            want_all = want_all.min(room_mem_i);
            want_refresh = lane.map(|l| l.min(want_all));
            let Some(list) = s.awaiting.clone() else {
                self.need("awaiting");
                return Ok(Room {
                    want: 0,
                    want_all: 0,
                    want_refresh: None,
                    hold: String::new(),
                    slice_ok,
                    limit_ok,
                    limit_plan_ok,
                });
            };
            awaiting = self.list(&list)?.len() as i64;
            let room = c.max_awaiting - awaiting - count;
            await_room = Some(room);
            if room <= 0 {
                why = Some(Why::Awaiting);
                hold = if count == 0 {
                    format!(
                        "holding: {awaiting} issues await merge (limit {})",
                        c.max_awaiting
                    )
                } else {
                    format!(
                        "holding: {awaiting} issues await merge and {count} cycles are running (limit {})",
                        c.max_awaiting
                    )
                };
            } else if !mem_why.is_empty() {
                why = slice_word;
                hold = match why.map(|w| w.as_str()).unwrap_or("") {
                    "pressure" => format!(
                        "holding: tehas.slice is under memory pressure (full avg60 {})",
                        pressure.clone().unwrap_or_else(|| "unknown".into())
                    ),
                    "room" => format!(
                        "holding: tehas.slice has no room for another cycle ({} of {} promised, each cycle is given {})",
                        gib(committed),
                        gib(slice_max),
                        c.reserve
                    ),
                    _ => format!(
                        "holding: tehas.slice holds as many containers as their ceilings fit ({} of {ceiling_mb}m)",
                        slice_max / (ceiling_mb * 1_048_576)
                    ),
                };
            } else {
                want = free.min(room);
                cut = "awaiting";
                if let Some(l) = limit_room.filter(|l| *l < want) {
                    want = l;
                    cut = "limit";
                }
                if room_mem_i < want {
                    want = room_mem_i;
                    cut = "room";
                }
                if let Some(lane) = lane.filter(|lane| room_mem_i - lane < want) {
                    want = (room_mem_i - lane).max(0);
                    cut = "lane";
                }
                if want == 0 && cut == "lane" {
                    why = Some(Why::Lane);
                    hold = format!(
                        "holding: tehas.slice has room for {room_mem} more and the refresh lane keeps {}",
                        lane.unwrap_or(0)
                    );
                }
            }
        }
        // Fewer start than there are free slots and issues waiting.
        let mut short = false;
        if free > 0 && want < free {
            let waiting = self.count_waiting()?;
            short = waiting > want;
        }
        if short && want > 0 {
            why = Why::parse(cut);
            hold = match cut {
                "awaiting" => format!(
                    "holding: {want} of {free} free slots start: {awaiting} issues await merge and {count} cycles are running (limit {})",
                    c.max_awaiting
                ),
                "limit" => format!(
                    "holding: {want} of {free} free slots start: the limit allows {limit_words} at once{}",
                    with_reason(&limit_reason)
                ),
                "lane" => format!(
                    "holding: {want} of {free} free slots start: tehas.slice has room for {room_mem} more and the refresh lane keeps {}",
                    lane.unwrap_or(0)
                ),
                _ => format!(
                    "holding: {want} of {free} free slots start: tehas.slice has room for {want} more ({} of {} promised, each cycle is given {})",
                    gib(committed),
                    gib(slice_max),
                    c.reserve
                ),
            };
            if self.dry {
                self.log(hold.clone());
            }
        }
        if !self.dry {
            if why.is_some() {
                if why.map_or("", |w| w.as_str()) != s.last_why || short {
                    self.log(hold.clone());
                }
            } else if !s.last_why.is_empty() {
                self.log("released: cycles may start again");
            }
            self.d.report = Some(Report {
                why,
                hold: hold.clone(),
                count,
                room: room_mem,
                committed,
                max: slice_max,
                reserve: c.reserve.clone(),
                free,
                await_room,
                limit_room,
                refresh_room: lane,
                endpoint: (!s.endpoint.is_empty()).then(|| s.endpoint.clone()),
                held: Vec::new(),
            });
            self.d.plan.push(PlanStep::Report);
        }
        Ok(Room {
            want,
            want_all,
            want_refresh,
            hold,
            slice_ok,
            limit_ok,
            limit_plan_ok,
        })
    }

    /// count_waiting: the ready issues that could start now, as far as this
    /// host knows without asking GitHub more.
    fn count_waiting(&mut self) -> Step<i64> {
        let ready = self.ready()?;
        Ok(ready
            .iter()
            .filter(|i| has(&i.labels, "tehas:ready"))
            .filter_map(|i| i.number)
            .filter(|n| !self.plan_open(*n) && !self.running(*n))
            .count() as i64)
    }

    /// The hold of the host's own push fault, and whether it holds the poll:
    /// while a sandbox that cannot push is on record the poll starts no cycle,
    /// so every ready issue does not fail the same check in turn. The one line
    /// of the hold is left on `push_hold`, empty for no fault.
    ///
    /// A fault of any age holds the starts: past `host::PUSH_FAULT_HOLD` the
    /// poll asked the sandbox before the rules were read (`crate::poll`), and
    /// left the record that stands, or none.
    ///
    /// The report's word for the hold is `lane`, not a word of its own: the
    /// readers outside the dispatcher (`console/ops/src/health.rs`,
    /// `console/mcp/`) know the thirteen words of `tehas_core::hold::Why`, and
    /// `lane` is the one of them already used for the host that keeps running
    /// while nothing new starts.
    fn push_fault(&mut self) -> bool {
        let Some(fault) = self.s.push_fault.as_ref() else {
            self.push_hold.clear();
            return false;
        };
        let who = if fault.login.is_empty() {
            String::new()
        } else {
            format!(" ({})", fault.login)
        };
        self.push_hold = format!("holding: the sandbox cannot push: {}{who}", fault.line);
        true
    }

    /// The hold of the branch's check, with the one word for its reason:
    /// `red` when TEHAS_BRANCH_CHECK names a check that failed on the
    /// branch's head commit, and `unknown-gate` when its runs finished and
    /// none of them says it passed. Anything else holds nothing, and says why
    /// it could not tell once.
    fn branch_check(&mut self) -> Option<(Why, String)> {
        let c = self.c;
        if c.branch_check.is_empty() {
            return None;
        }
        let state = match &self.s.checks {
            Some(check) => check.clone(),
            None => {
                self.need("checks");
                return None;
            }
        };
        let (name, branch) = (&c.branch_check, &c.branch);
        // Exhaustive: a new state is a compile error here, never a fall
        // through to the arm that starts cycles.
        let (red, note) = match state {
            Check::Red(sha) => (
                Some((Why::Red, format!("holding: {branch} is red at {sha}"))),
                String::new(),
            ),
            Check::Done(_) => (None, String::new()),
            Check::Pending(sha) => (
                None,
                format!("the check {name} on {branch} at {sha} has not finished: cycles start"),
            ),
            Check::None => (
                None,
                format!("{branch} has no check named {name} on its head commit: cycles start"),
            ),
            // The runs finished and none says it passed: the poll does not
            // know what ended the gate, so nothing starts on it unsaid. The
            // note of the same words goes to state/branch-check, where the
            // manual sends a person for the branch's condition. The name is
            // quoted: an instance may set one with spaces in it.
            Check::Unknown(sha) => {
                let said = format!(
                    "the check {name:?} on {branch} at {sha} finished without saying it passed: cycles start"
                );
                // The hold's own word, not `red`: a person fixes a red gate on
                // the branch, and an outcome the poll cannot read by naming
                // the check that answers it. The hold itself is the same hold.
                (Some((Why::UnknownGate, format!("holding: {said}"))), said)
            }
            Check::Unreadable => (
                None,
                format!("the check {name} on {branch} could not be read: cycles start"),
            ),
        };
        if self.dry {
            if !note.is_empty() {
                self.log(note);
            }
        } else if self.s.branch_note != note {
            if !note.is_empty() {
                self.log(note.clone());
            }
            self.act(Action::BranchNote(note));
        }
        red
    }
}

impl<'a> Poll<'a> {
    /// deny_label: the label came from an account outside the list.
    fn deny(&mut self, verb: &str, n: u64, label: &str, who: &str) {
        self.log(format!(
            "{verb} #{n}: {label} was applied by '{}', not an allowed account",
            or_unknown(who)
        ));
    }

    /// order_label: tehas:blocked stands when an allowed account
    /// or the millwright applied it last and nobody took it off since.
    fn order_label(&mut self, n: u64, label: &str) -> Step<bool> {
        let ev = self.events(n)?;
        let state = label_state(&ev, label);
        if state.event() == "unlabeled" {
            self.log(format!(
                "ignore #{n}: {label} was taken off by '{}' since it was applied",
                or_unknown(state.actor())
            ));
            return Ok(false);
        }
        if self.order_stands(&state) {
            return Ok(true);
        }
        self.deny("ignore", n, label, state.actor());
        Ok(false)
    }

    /// Whether an order label's last event, as `order_label` reads it, lets
    /// the label stand: an application by an allowed account or the millwright,
    /// or one of nobody. The pins leave a refused application with nobody's
    /// account (`crate::pins`): it starts nothing, but a hold that may not be
    /// a person's still holds, so a refused pin never lets an issue start
    /// sooner.
    fn order_stands(&self, state: &LabelState) -> bool {
        let by = state.actor();
        state.event() == "labeled"
            && (by == NOBODY || self.c.allowed(by) || self.c.is_millwright(by))
    }

    /// driver: the driver of a start, read of who applied a driver label
    /// ([`driver_labels`]) and never of the label alone. With no label a
    /// start runs the default, whatever the last one ran, unless an account
    /// outside the list took an allowed account's choice off. The default is
    /// classic, or under `TEHAS_DRIVER=lean` what the issue's text gives
    /// (`default_driver`). With a label on, a start that rests on an earlier
    /// approval (`retry`) keeps the driver on that record, so a label added
    /// since changes nothing until a person approves a new start. With both on
    /// nothing starts: tehas does not guess.
    ///
    /// A start that would run lean while the lean feature is off starts
    /// nothing, as a stranger's choice does, and says why. A lean default
    /// with the feature off stops the poll before it gets here
    /// (`Settings::refused`).
    fn driver(&mut self, n: u64, labels: &[String], retry: bool) -> Step<Choice> {
        Ok(match self.chosen_driver(n, labels, retry)? {
            Choice::Run(Driver::Lean) if !self.c.lean => Choice::Stranger(Stranger::building()),
            c => c,
        })
    }

    fn chosen_driver(&mut self, n: u64, labels: &[String], retry: bool) -> Step<Choice> {
        if has(labels, LEAN) && has(labels, CLASSIC) {
            return Ok(Choice::Stranger(Stranger::both()));
        }
        let read = driver_labels(self.c.driver_default);
        let lean_default = self.c.driver_default == Driver::Lean;
        let stranger = |who: String, off: bool, label: &'static str| {
            Choice::Stranger(Stranger {
                who,
                off,
                label,
                both: false,
                building: false,
                lean_default,
            })
        };
        // A label the events say was taken off is off, though the issue's
        // labels still list it.
        let ev = if read.iter().any(|l| has(labels, l)) {
            self.events(n)?
        } else {
            Vec::new()
        };
        let on = |l: &str| has(labels, l) && label_state(&ev, l).event() != "unlabeled";
        let Some(label) = read.iter().copied().find(|l| on(l)) else {
            for l in read {
                if let Some(who) = self.taken_off(n, l) {
                    return Ok(stranger(who, true, l));
                }
            }
            return Ok(self.default_driver(n));
        };
        let chosen = Choice::Run(if label == CLASSIC {
            Driver::Classic
        } else {
            Driver::Lean
        });
        let (who, _) = standing_actor(&ev, label);
        if !self.c.allowed(&who) {
            return Ok(stranger(or_unknown(&who).to_string(), false, label));
        }
        if !retry {
            return Ok(chosen);
        }
        match self.s.issues.get(&n).and_then(|i| i.approval.clone()) {
            None => {
                self.need("approval");
                Ok(Choice::Wait)
            }
            Some(Err(())) => {
                self.log(format!("wait #{n}: its approval could not be read"));
                Ok(Choice::Wait)
            }
            // No start on record to follow: this one is the first.
            Some(Ok(None)) => Ok(chosen),
            // Only a record that names lean runs lean; any other runs classic.
            Some(Ok(Some(r))) if r.driver == Some(Driver::Lean) => Ok(Choice::Run(Driver::Lean)),
            Some(Ok(Some(_))) => Ok(Choice::Run(Driver::Classic)),
        }
    }

    /// default_driver: the driver of a start no label chose. Classic, or under
    /// `TEHAS_DRIVER=lean` lean when the issue's text holds a `pi-spec v1`
    /// block and classic when it does not: lean declines an issue without
    /// one. A body that was not read chooses nothing: the start waits rather
    /// than run a driver nobody chose.
    fn default_driver(&mut self, n: u64) -> Choice {
        if self.c.driver_default != Driver::Lean {
            return Choice::Run(Driver::Classic);
        }
        let Some(body) = self.body(n) else {
            self.log(format!(
                "wait #{n}: its text could not be read, and TEHAS_DRIVER=lean chooses the driver by its {} block",
                text::SPEC_INFO
            ));
            return Choice::Wait;
        };
        if text::has_spec(&body) {
            self.log(format!(
                "start #{n} on lean: TEHAS_DRIVER=lean and its text holds a {} block",
                text::SPEC_INFO
            ));
            Choice::Run(Driver::Lean)
        } else {
            self.log(format!(
                "start #{n} on classic: TEHAS_DRIVER=lean and its text holds no {} block lean can read",
                text::SPEC_INFO
            ));
            Choice::Run(Driver::Classic)
        }
    }

    /// The body a start reads its text from: the issue's own, as the page of
    /// the queue gave it. `edit` is the record an approval binds and `work`
    /// the REST issue; the read of the queue puts the same text in both, and a
    /// start that is not a retry has no record to follow, so the issue's own
    /// answer is asked of. Nothing when the body was not read at all, or was
    /// read as no string: a rule of the poll may not read either as a body of
    /// no text, which is how `size_ok` and `image_ok` hold the same field.
    fn body(&self, n: u64) -> Option<String> {
        let i = self.s.issues.get(&n)?;
        // A body that is there and no string answers `Some(None)`: the poll
        // ends rather than reading it as a body of no text.
        let read = |t: &Text| match t {
            Text::None => None,
            Text::Some(b) => Some(Some(b.clone())),
            Text::Unreadable => Some(None),
        };
        i.edit
            .as_ref()
            .and_then(|e| read(&e.body))
            .or_else(|| match i.work.as_ref()? {
                Work::Issue(w) => read(&w.body),
                Work::Unreadable => None,
            })?
    }

    /// The labels that ask for a start come off, and the issue says why once:
    /// `ask` with `tehas:blocked` when that is on too, which is what keeps the
    /// reason said once rather than twice on the next tick. `tehas:lean` is
    /// never in the list a caller gives: with it off the next `tehas:ready`
    /// would run classic in lean's place. `why` is the reason a person reads
    /// and `dry_line` what the log says in their place, so far.
    fn refuse_start(&mut self, n: u64, ask: &str, labels: &[String], why: &str, dry_line: &str) {
        // A start the host asked for has no label to take off: the line its
        // caller logged is the answer, and the approval waits.
        if ask == HOST {
            return;
        }
        let mut off = vec![ask];
        if ask != "tehas:blocked" && has(labels, "tehas:blocked") {
            off.push("tehas:blocked");
        }
        if self.dry {
            self.log(format!(
                "would take {} off #{n}: {dry_line}",
                off.join(" and ")
            ));
            return;
        }
        let names = format!("`{}`", off.join("` and `"));
        self.act(Action::Label(
            LabelChange::new(n, &off, Self::label_fail(n, &off, &[])).say(format!(
                "Tehas took {names} off this issue and started nothing: {why}"
            )),
        ));
    }

    /// spec_ok: the last hold of a lean start. The driver reads only the
    /// issue's `pi-spec v1` block, so an issue without one is asked for it
    /// here rather than started and declined by lean: nothing starts, the
    /// labels that asked for the cycle come off, and the reason is said once.
    /// `tehas:lean` stays on, or the next `tehas:ready` would run classic in
    /// its place. Classic is asked nothing of the block, and a start that
    /// follows a record on purpose runs the driver of that record: a tick, an
    /// outage retry and a refresh ask nothing new of the body. Only a start
    /// that asks for a cycle is asked this question, so `ask` is the label
    /// that asked it and always sits on the issue here.
    fn spec_ok(&mut self, n: u64, ask: &str, labels: &[String]) -> Step<bool> {
        let Some(body) = self.body(n) else {
            return Err(self.fail(format!("ERROR: the text of #{n} could not be read"), 1));
        };
        if text::has_spec(&body) {
            return Ok(true);
        }
        let why = text::no_spec_said(ask);
        self.log(format!("skip #{n}: {why}"));
        self.refuse_start(
            n,
            ask,
            labels,
            &why,
            &format!("its text holds no {} block", text::SPEC_INFO),
        );
        Ok(false)
    }

    /// taken_off: who took the driver label `label` off, when an allowed
    /// account's choice went and that account may not take it. An issue whose
    /// events were not read names nobody: each start reads them itself.
    fn taken_off(&self, n: u64, label: &str) -> Option<String> {
        let i = self.s.issues.get(&n)?;
        let Events::Read(ev) = &i.events else {
            return None;
        };
        let state = label_state(ev, label);
        // The application before the removal, which `standing_actor` no
        // longer names.
        (state.event() == "unlabeled"
            && !self.c.allowed(state.actor())
            && self.c.allowed(&label_actor(ev, label).0))
        .then(|| or_unknown(state.actor()).to_string())
    }

    /// lean_deny: the log line of a choice of driver that does not count.
    fn lean_deny(&mut self, n: u64, s: &Stranger) {
        if s.building {
            self.log(format!("skip #{n}: {LEAN} asks for the lean driver, still being built in this release; nothing starts until it comes off"));
        } else if s.both {
            self.log(format!("skip #{n}: it carries both {LEAN} and {CLASSIC}; tehas does not guess the driver, so nothing starts until one comes off"));
        } else if s.off {
            let (who, label) = (&s.who, s.label);
            self.log(format!(
                "skip #{n}: {label} was taken off by '{who}', not an allowed account"
            ));
        } else {
            self.deny("skip", n, s.label, &s.who);
        }
    }

    /// lean_refused: the labels that ask for the start come off, so this is
    /// said once. The driver label stays, or the next tehas:ready would run
    /// the other driver.
    fn lean_refused(&mut self, n: u64, ask: &str, labels: &[String], s: &Stranger) {
        self.lean_deny(n, s);
        self.refuse_start(
            n,
            ask,
            labels,
            &format!("{}, then applies `{ask}`.", s.said()),
            &s.dry_line(),
        );
    }

    /// unchanged_since: the title and body are what the approval at `since`
    /// covered, changed since by nobody outside the list. The two reads of a
    /// change rank a missing time in opposite directions on purpose: an
    /// unstamped rename counts as after the approval and holds the start
    /// (`first_unallowed_rename`), and an unstamped edit is no edit at all
    /// (`edit_holds`). A change in the second of `since` is after it unless
    /// `approver` made it (`changed_after`).
    fn unchanged_since(
        &mut self,
        n: u64,
        since: &str,
        approver: &str,
        what: &str,
        again: &str,
    ) -> Step<bool> {
        let ev = self.events(n)?;
        let c = self.c;
        // The rename is asked first and answers first: it is the one change of
        // the two GitHub stamps least often.
        let stale = match first_unallowed_rename(&ev, since, approver, &|w| c.allowed(w)) {
            Some(who) => Some(who),
            None => {
                let Some(edit) = self.s.issues.get(&n).and_then(|i| i.edit.clone()) else {
                    return Err(self.unreadable(&format!("edit record of #{n}"), None));
                };
                let held = edit_holds_of(&edit, since, approver, &|w| c.allowed(w));
                // A history the poll asked for and GitHub did not give ends
                // the poll, as an unread comment does.
                if held.is_some() && edit_history_unread_of(&edit, &|w| c.allowed(w)) {
                    let line = edit.history.line();
                    return Err(self.unreadable(&format!("edit history of #{n}"), line));
                }
                held
            }
        };
        if let Some(stale) = stale {
            self.log(format!(
                "skip #{n}: its text was changed by '{stale}' after {what}; {again} to approve it"
            ));
            return Ok(false);
        }
        Ok(true)
    }

    /// approval: who applied the label, and whether it approves the issue.
    /// Returns the account and time, or the login refused for not being
    /// allowed (empty for no account; `None` for every other refusal). A
    /// label the events say was taken off since approves nothing, though the
    /// issue's labels still list it: they may be older than the events.
    fn approval(&mut self, n: u64, label: &str) -> Step<Result<(String, String), Option<String>>> {
        let ev = self.events(n)?;
        if let LabelState::Unlabeled { actor, .. } = label_state(&ev, label) {
            self.log(format!(
                "skip #{n}: {label} was taken off by '{}' since it was applied",
                or_unknown(&actor)
            ));
            return Ok(Err(None));
        }
        let (who, at) = standing_actor(&ev, label);
        if !self.c.allowed(&who) {
            // The millwright's labels are answered on the epic path alone.
            let granted = matches!(label, "tehas:ready" | "tehas:plan" | "tehas:refine");
            if !granted || !self.c.is_millwright(&who) {
                self.deny("skip", n, label, &who);
            }
            return Ok(Err(Some(who)));
        }
        if self.unchanged_since(
            n,
            &at,
            &who,
            &format!("{who} applied {label}"),
            "apply the label again",
        )? {
            Ok(Ok((who, at)))
        } else {
            Ok(Err(None))
        }
    }

    /// The bot applied `tehas:ready` here, and the issue has the record of a
    /// plan's start. Nothing more: `plan_start` judges what that is worth.
    fn planned(&self, n: u64, who: &str) -> bool {
        !who.is_empty()
            && account(who) == account(&self.d.bot)
            && self
                .s
                .issues
                .get(&n)
                .is_some_and(|i| i.plan_start.is_some())
    }

    /// plan_start: a tehas:ready from the bot never approves anything, except
    /// on the record the dispatcher left for this one issue when it wrote a
    /// plan with no open points. The start is the record's account's, so the
    /// account is held to everything its own label would be: the events still
    /// name it, the list still does and the text is what was written. That
    /// label must be the bot's one application since. Any other answer takes
    /// the label off and the record with it, so a person decides.
    fn plan_start(&mut self, n: u64, ask: &str) -> Step<Planned> {
        if ask != "tehas:ready" {
            return Ok(Planned::No);
        }
        let ev = self.events(n)?;
        let (who, at) = standing_actor(&ev, ask);
        if !self.planned(n, &who) {
            return Ok(Planned::No);
        }
        let (by, sum) = match self.s.issues[&n].plan_start.clone() {
            Some(PlanStart::Record { by, sum }) => (by, sum),
            _ => (String::new(), None),
        };
        let by = by.as_str();
        let c = self.c;
        let credit = plan_credit(&ev, by, &|l| c.allowed(l), &|l| c.is_millwright(l));
        let Some(since) = credit else {
            return Ok(self.plan_refused(n, "no allowed person's `tehas:plan` is on record for it"));
        };
        let bot = account(&self.d.bot).to_string();
        if !sole_application(&ev, ask, &since, &|l| account(l) == bot) {
            return Ok(self.plan_refused(
                n,
                "`tehas:ready` was taken off or applied again since the plan was written",
            ));
        }
        // The bot's own write of the plan may share the label's second.
        if !self.unchanged_since(n, &at, &who, "the plan was written", "apply `tehas:ready`")? {
            return Ok(self.plan_taken_off(n, "its text was changed since the plan was written"));
        }
        // The time cannot see a change made before the label went on.
        let t = self.text(n);
        let plan = text::plan_sum_of(t.title.as_deref().unwrap_or(""), t.body_text());
        if sum.as_deref() != Some(plan.as_str()) {
            return Ok(self.plan_refused(n, "its title or text is not the plan tehas wrote"));
        }
        Ok(Planned::Go(by.to_string(), at))
    }

    fn plan_refused(&mut self, n: u64, why: &str) -> Planned {
        self.log(format!(
            "skip #{n}: tehas:ready from the bot starts nothing: {why}"
        ));
        self.plan_taken_off(n, why)
    }

    /// The bot's label and the record it stood on go, and the issue says why.
    fn plan_taken_off(&mut self, n: u64, why: &str) -> Planned {
        if self.dry {
            self.log(format!("would take tehas:ready off #{n}"));
        } else {
            self.act(Action::Label(
                LabelChange::new(n, &["tehas:ready"], Self::label_fail(n, &["tehas:ready"], &[]))
                    .say(format!("Tehas took `tehas:ready` off this issue and started nothing: {why}. Read it, and an account named in `TEHAS_ALLOWED_LABELERS` applies `tehas:ready` to build it.")),
            ));
            self.act(Action::PlanStartDrop(n));
        }
        Planned::Refused
    }

    /// clear_unauthorised: a label from an account outside the list comes off,
    /// and the issue says why once the label is off.
    fn clear_unauthorised(&mut self, n: u64, label: &str, by: &str) {
        if self.dry {
            self.log(format!(
                "would take {label} off #{n}: it was applied by '{by}'"
            ));
            return;
        }
        self.act(Action::Label(
            LabelChange::new(n, &[label], Self::label_fail(n, &[label], &[]))
                .log(format!("clear #{n}: {label} off, applied by '{by}' who may not authorise work here"))
                .say(format!("Tehas took `{label}` off this issue. It was applied by `{by}`, an account that may not authorise work in this factory, so nothing was started and the issue is not queued. To ask again, an account named in `TEHAS_ALLOWED_LABELERS` applies `{label}`.")),
        ));
    }

    /// links_ok: whether the issue's native blocked-by links let it start.
    /// An open blocker holds it, whatever label asks. A link only holds: once
    /// every blocker is closed the issue starts on its approval alone, as it
    /// would with no link, and a link never stands in for one. A forge answer
    /// without the field holds nothing, so the marked comment of `blocked_ok`
    /// is all that names blockers there.
    fn links_ok(&mut self, n: u64) -> bool {
        let open = self.work(n).map_or(0, |w| w.blocked_by);
        if open == 0 {
            return true;
        }
        let count = if open == 1 {
            "1 blocker is open".to_string()
        } else {
            format!("{open} blockers are open")
        };
        self.log(format!("wait #{n}: a blocked-by link holds it: {count}"));
        false
    }

    /// blocked_ok: whether tehas:blocked lets the issue start.
    fn blocked_ok(&mut self, n: u64) -> Step<bool> {
        if !self.order_label(n, "tehas:blocked")? {
            return Ok(true);
        }
        let Some(answer) = self.s.issues.get(&n).and_then(|i| i.comments.clone()) else {
            return Err(self.comments_unread(n));
        };
        let Some(nodes) = answer.listed() else {
            return Err(self.fail(format!("ERROR: unreadable comments of #{n}"), 1));
        };
        let why = self.marker_hold(n, nodes);
        if why.is_empty() {
            return Ok(true);
        }
        self.log(format!("wait #{n}: tehas:blocked holds it: {why}"));
        Ok(false)
    }

    /// What the newest marker comment of `n` still holds it on, empty when
    /// every blocker it names is closed.
    fn marker_hold(&self, n: u64, nodes: &[Comment]) -> String {
        match newest_marker(nodes, self.c) {
            None => LABEL_ALONE.to_string(),
            Some(x) => {
                let editor = x.editor_login();
                if x.edited && editor != x.author.login {
                    format!("the newest comment that names its blockers was edited by '{editor}'")
                } else {
                    match blockers(x.body_text().unwrap_or("")) {
                        None => "the newest comment that names its blockers cannot be read".into(),
                        Some(list) => {
                            let states = &self.s.issues[&n].blockers;
                            let mut why = String::new();
                            for b in list {
                                match states.get(&b).map(String::as_str) {
                                    Some("CLOSED" | "MERGED") => {}
                                    Some("OPEN") => {
                                        why = format!("#{b} is open");
                                        break;
                                    }
                                    _ => {
                                        why = format!("the state of #{b} could not be read");
                                        break;
                                    }
                                }
                            }
                            why
                        }
                    }
                }
            }
        }
    }

    /// work_ok: the number is still an open issue.
    fn work_ok(&mut self, n: u64) -> Step<bool> {
        if self.work_refused.contains(&n) {
            return Ok(false);
        }
        let Some(w) = self.s.issues.get(&n).and_then(|i| i.work.clone()) else {
            let repo = self.c.repo.clone();
            return Err(self.fail(format!("ERROR: gh api repos/{repo}/issues/{n} failed"), 1));
        };
        let kind = match w {
            Work::Unreadable => "unreadable",
            Work::Issue(w) if w.is_pull => "pr",
            Work::Issue(w) => match w.state.as_deref().unwrap_or("").to_lowercase().as_str() {
                "open" => "open",
                "closed" => "closed",
                _ => "unreadable",
            },
        };
        match kind {
            "open" => Ok(true),
            "pr" => {
                self.work_refused.push(n);
                self.log(format!(
                    "skip #{n}: it is a pull request, not an issue the factory works on"
                ));
                Ok(false)
            }
            "closed" => {
                self.work_refused.push(n);
                self.log(format!(
                    "skip #{n}: it is closed, and a closed issue is never taken"
                ));
                Ok(false)
            }
            _ => Err(self.fail(
                format!("ERROR: GitHub's answer for #{n} could not be read"),
                1,
            )),
        }
    }

    /// open_pr: the url of an open pull request that closes the issue. The
    /// first open one decides; one with no url is none.
    pub(super) fn open_pr(&mut self, n: u64) -> Step<Option<String>> {
        let Some(prs) = self.s.issues.get(&n).map(|i| i.prs.clone()) else {
            return Ok(None);
        };
        let Some(prs) = prs else {
            return Err(self.unreadable(&format!("pull requests of #{n}"), None));
        };
        Ok(prs
            .iter()
            .find(|p| p.open)
            .map(|p| p.url.clone())
            .filter(|u| !u.is_empty()))
    }

    /// The address of an open pull request that names the issue with a closing
    /// keyword in its body, whatever branch it targets. GitHub links a closing
    /// keyword only for a pull request into the default branch, so a stacked
    /// one reaches `open_pr` with nothing; the page's own read of the bodies
    /// is what holds its issue. The lowest number decides, so the line is the
    /// same on every poll while two of them name the issue.
    pub(super) fn named_pr(&self, n: u64) -> Option<String> {
        // The address comes from the repository the page answered, the way
        // `open_pr` reads GitHub's own url: no repository, no address, no hold.
        let repo = self.s.repo.as_deref().filter(|r| !r.is_empty())?;
        self.s
            .pulls_name
            .get(&n)
            .and_then(|hits| hits.iter().min())
            .map(|pr| format!("https://github.com/{repo}/pull/{pr}"))
    }

    /// The hold of an open pull request: `tehas:ready` (or whatever asked)
    /// comes off and the issue is told which pull request holds it. One line,
    /// whether GitHub linked the two or the pull request's body says so. The
    /// retry tick of a park has its own hold in `holds_on_pr_tick`, which
    /// takes the box off instead of a label and keeps the wording the same.
    fn holds_on_pr(&mut self, n: u64, ask: &str, url: &str) {
        self.log(format!(
            "skip #{n}: pull request {url} is open and closes it"
        ));
        if !self.dry && ask != HOST {
            self.act(Action::Label(
                LabelChange::new(n, &[ask], Self::label_fail(n, &[ask], &[]))
                    .say(format!("Tehas took `{ask}` off this issue and started nothing: {url} is open and closes it. Merge or close that pull request, then apply `tehas:ready` if work is still needed.")),
            ));
        }
    }

    /// The answer work_ok read for the issue, when it is an object.
    fn work(&self, n: u64) -> Option<&WorkIssue> {
        match self.s.issues.get(&n)?.work.as_ref()? {
            Work::Issue(w) => Some(w),
            Work::Unreadable => None,
        }
    }

    /// The edit record unchanged_since read, the text an approval binds.
    fn text(&self, n: u64) -> EditRecord {
        self.s
            .issues
            .get(&n)
            .and_then(|i| i.edit.clone())
            .unwrap_or_default()
    }

    /// epic_ok: an issue with open sub-issues is a plan, never started. Only
    /// the label that asked for the cycle comes off.
    fn epic_ok(&mut self, n: u64, ask: &str, labels: &[String]) -> bool {
        let sum = self.work(n).and_then(|w| w.sub_issues.as_ref());
        let (Some(total), Some(done)) =
            (sum.and_then(|s| s.total.f), sum.and_then(|s| s.completed.f))
        else {
            return true;
        };
        let open = total - done;
        if open <= 0.0 || open.fract() != 0.0 {
            return true;
        }
        let word = if open == 1.0 {
            "sub-issue"
        } else {
            "sub-issues"
        };
        self.log(format!(
            "skip #{n}: it is an epic with {open} {word} open, and an epic is never started"
        ));
        // A host start has no label of its own: a label that is only named
        // `host` stays on and is not talked about.
        if ask == HOST || !has(labels, ask) {
            return false;
        }
        if self.dry {
            self.log(format!("would take {ask} off #{n}: it is an epic"));
        } else {
            let say = format!("Tehas took `{ask}` off this issue and started nothing. It has {open} {word} open, so it is an epic: a plan, not a change. An epic is never started. {}", self.epic_next(n, ask));
            self.act(Action::Label(
                LabelChange::new(n, &[ask], Self::label_fail(n, &[ask], &[])).say(say),
            ));
        }
        false
    }

    /// What the comment on an epic's taken label says to do next: with
    /// TEHAS_MILLWRIGHT_EPICS and a person's tehas:ready, the millwright
    /// takes the sub-issues.
    fn epic_next(&self, n: u64, ask: &str) -> &'static str {
        let by = match self.s.issues.get(&n).map(|i| &i.events) {
            Some(Events::Read(ev)) => standing_actor(ev, ask).0,
            _ => String::new(),
        };
        if ask == "tehas:ready"
            && self.c.millwright_epics
            && !self.c.millwright.is_empty()
            && self.c.allowed(&by)
        {
            "Its open sub-issues are approved for the millwright, which plans each one that needs it and applies `tehas:ready` to it. To take that back, close this issue or put `needs-human-attention` on it."
        } else {
            "Apply `tehas:ready` to one of its sub-issues instead."
        }
    }

    /// epic_stands: the person's tehas:ready on epic `p`, open with open
    /// sub-issues and without needs-human-attention. The poll's page has
    /// every open issue, so a closed epic is not in it.
    fn epic_stands(&self, p: u64) -> Option<(String, String)> {
        let i = self.s.issues.get(&p)?;
        let w = self.work(p)?;
        if w.state.as_deref() != Some("open") {
            return None;
        }
        let sum = w.sub_issues.as_ref()?;
        if sum.total.u? <= sum.completed.u? {
            return None;
        }
        let Labels::Names(names) = &w.labels else {
            return None;
        };
        let held = has(names, "needs-human-attention");
        if held {
            return None;
        }
        let Events::Read(ev) = &i.events else {
            return None;
        };
        let (by, at) = standing_past_bot(ev, "tehas:ready", &self.c.bot_login);
        (self.c.allowed(&by) && !at.is_empty()).then_some((by, at))
    }

    /// epic_part: whether the millwright's label on `n` stands on a person's
    /// tehas:ready on its epic. The issue's text must be written and last
    /// changed by an allowed account or by tehas itself, whose planner writes
    /// a sub-issue's plan.
    fn epic_part(&mut self, n: u64) -> Step<EpicPart> {
        if !self.c.millwright_epics || self.c.millwright.is_empty() {
            return Ok(EpicPart::None);
        }
        let Some(p) = self.work(n).and_then(|w| w.parent) else {
            return Ok(EpicPart::None);
        };
        let Some((by, at)) = self.epic_stands(p) else {
            return Ok(EpicPart::None);
        };
        let labels: Vec<String> = self
            .work(n)
            .map(|w| w.labels.names().to_vec())
            .unwrap_or_default();
        let refuse = |why: String| {
            Ok(EpicPart::Refused(format!(
                "its epic #{p} is approved, but {why}"
            )))
        };
        if let Some(l) = MILLWRIGHT_HELD.into_iter().find(|l| has(&labels, l)) {
            return refuse(format!("it carries `{l}`"));
        }
        let ev = self.events(n)?;
        let c = self.c;
        let bot = account(&self.d.bot).to_string();
        let ours = |w: &str| c.allowed(w) || (!bot.is_empty() && account(w) == bot);
        let t = self.text(n);
        // GitHub answers null for a text never edited. A record without the
        // field, or with something else in it, was not read and grants nothing.
        let edited = match t.last_edited {
            Stamp::Never => false,
            Stamp::At(_) => true,
            Stamp::Unread => {
                return refuse(
                    "whether its text was changed since it was written could not be read".into(),
                )
            }
        };
        let author = t.author.clone();
        if !ours(&author) {
            return refuse(format!(
                "its author {} is neither tehas nor named in `TEHAS_ALLOWED_LABELERS`",
                or_unknown(&author)
            ));
        }
        let editor = t.editor.clone();
        if edited && !ours(&editor) {
            return refuse(format!(
                "its text was last changed by {}",
                or_unknown(&editor)
            ));
        }
        // Every edit counts, not only the last.
        if let Some(who) = edited.then(|| edit_holds_of(&t, "", "", &ours)).flatten() {
            if edit_history_unread_of(&t, &ours) {
                let line = t.history.line();
                return Err(self.unreadable(&format!("edit history of #{n}"), line));
            }
            return refuse(format!("its text was changed by {who}"));
        }
        if let Some(who) = first_unallowed_rename(&ev, "", "", &ours) {
            return refuse(format!("its title was changed by {who}"));
        }
        Ok(EpicPart::Granted(EpicGrant { by, at, epic: p }))
    }

    /// epic_ready: the millwright's tehas:ready, applied at `at`, on the
    /// first cycle of an approved epic's sub-issue. None when no epic stands
    /// over it or a cycle of it was approved before; Some(None) with the label
    /// off and the reason said, or kept while its approvals are not read.
    fn epic_ready(&mut self, n: u64, who: &str, at: &str) -> Step<Option<Option<EpicGrant>>> {
        match self.s.issues.get(&n).map(|i| &i.approval) {
            Some(Some(Ok(None))) => {}
            Some(Some(Err(()))) => {
                self.log(format!(
                    "skip #{n}: tehas:ready from the millwright waits: its approvals on the state store could not be read"
                ));
                return Ok(Some(None));
            }
            None | Some(None) => {
                self.need("approval");
                return Ok(Some(None));
            }
            Some(Some(Ok(Some(_)))) => return Ok(None),
        }
        match self.epic_part(n)? {
            EpicPart::None => Ok(None),
            EpicPart::Refused(why) => {
                self.millwright_refused(n, &why);
                Ok(Some(None))
            }
            EpicPart::Granted(g) => {
                if !self.unchanged_since(
                    n,
                    at,
                    who,
                    "the millwright applied tehas:ready",
                    "the millwright or a person applies it again",
                )? {
                    return Ok(Some(None));
                }
                Ok(Some(Some(g)))
            }
        }
    }

    /// size_ok: with TEHAS_SIZE_MAX_FILES, a body that names more files is
    /// held, its label off and needs-human-attention on.
    fn size_ok(&mut self, n: u64, ask: &str, labels: &[String]) -> Step<bool> {
        let Some(max) = self.c.size_max_files else {
            return Ok(true);
        };
        let body = match self.work(n).map(|w| &w.body) {
            None | Some(Text::None) => "",
            Some(Text::Some(b)) => b,
            Some(Text::Unreadable) => {
                return Err(self.fail(format!("ERROR: the size of #{n} could not be read"), 1))
            }
        };
        let files = text::size_files(body, self.c.size_max_bytes, self.c.size_max_lines) as u64;
        if files <= max {
            return Ok(true);
        }
        self.log(format!("skip #{n}: its body names {files} files, more than TEHAS_SIZE_MAX_FILES ({max}), and one cycle does not make that many changes"));
        if ask == HOST || !has(labels, ask) {
            return Ok(false);
        }
        if self.dry {
            self.log(format!("would take {ask} off #{n} and add needs-human-attention: its body names {files} files, more than TEHAS_SIZE_MAX_FILES ({max})"));
        } else {
            self.act(Action::Label(
                LabelChange::new(n, &[ask], Self::label_fail(n, &[ask], &["needs-human-attention"]))
                    .put_on(&["needs-human-attention"])
                    .say(format!("Tehas took `{ask}` off this issue, put `needs-human-attention` on it and started nothing. Its body names {files} files, more than the {max} of `TEHAS_SIZE_MAX_FILES`, so one cycle would be asked for too many changes at once. Narrow this issue to the part of it one cycle can make, or split it into issues of that size, and apply `tehas:ready` again to start one of them.")),
            ));
        }
        Ok(false)
    }

    /// image_ok: with TEHAS_WORK_IMAGES=1, an issue that links an attachment
    /// waits while the sandbox's pi-rukas or the model cannot read one. The
    /// image and the model are asked once a poll.
    fn image_ok(&mut self, n: u64) -> Step<bool> {
        if !self.c.work_images {
            return Ok(true);
        }
        let linked = match self.work(n).map(|w| &w.body) {
            None | Some(Text::None) => false,
            Some(Text::Some(b)) => text::links(b) > 0,
            Some(Text::Unreadable) => {
                return Err(self.fail(
                    format!("ERROR: the text of #{n} could not be read for its links"),
                    1,
                ))
            }
        };
        if !linked {
            return Ok(true);
        }
        if self.image_hold.is_none() {
            let Some(i) = self.s.images.clone() else {
                self.need("images");
                return Ok(false);
            };
            let model = &self.c.model;
            self.image_hold = Some(if i.rukas == "0" {
                let commit = if i.commit.is_empty() {
                    String::new()
                } else {
                    format!(" {}", &i.commit[..i.commit.len().min(12)])
                };
                format!("the pi-rukas in the sandbox image{commit} does not read them")
            } else if self.s.endpoint.is_empty() {
                String::new()
            } else if !i.declares {
                format!("{model} does not declare image input in models.json")
            } else if !i.answers {
                format!("{model} does not answer an image")
            } else {
                String::new()
            });
        }
        let hold = self.image_hold.clone().unwrap_or_default();
        if hold.is_empty() {
            return Ok(true);
        }
        self.log(format!(
            "hold #{n}: its text links an attachment and TEHAS_WORK_IMAGES is 1, but {hold}"
        ));
        Ok(false)
    }

    /// image_missing_ok: a text that promises a file and links none waits for
    /// its owner, with nothing on the issue changed.
    fn image_missing_ok(&mut self, n: u64) -> bool {
        let Some(word) = self.work(n).and_then(|w| text::promise_of(&w.strings)) else {
            return true;
        };
        self.log(format!("hold #{n}: its text says {word} and links no attachment, so the cycle would be given none; attach the file or write out what it shows, before applying `tehas:ready`"));
        false
    }

    /// park_holds: a fresh label on the very body the last cycle parked on
    /// starts nothing. Said on the issue once a park.
    fn park_holds(&mut self, n: u64) -> Step<bool> {
        let Some(park) = self.s.issues.get(&n).and_then(|i| i.park.clone()) else {
            self.need("park");
            return Ok(true);
        };
        let park = match park {
            Ok(p) => p,
            Err(e) if e.rc == 1 => {
                self.log(format!(
                    "skip #{n}: its park on record could not be read ({})",
                    e.word
                ));
                return Ok(true);
            }
            Err(e) => {
                return Err(self.fail(
                    format!("ERROR: the state store cannot be used: {}", e.line),
                    78,
                ))
            }
        };
        let Some(p) = park else {
            return Ok(false);
        };
        let sum = p.body_sha256.as_str();
        let hex = sum.len() == 64 && sum.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if p.reason == "provider-outage" || !p.fresh || !hex {
            return Ok(false);
        }
        if text::body_sum(self.text(n).body_text()) != sum {
            return Ok(false);
        }
        // As `read -r` leaves it: only its first line counts, trimmed of
        // spaces and tabs.
        let cap = p
            .reason
            .split('\n')
            .next()
            .unwrap_or("")
            .trim_matches([' ', '\t']);
        let cap = if cap.is_empty() { "nothing" } else { cap }.to_string();
        // A Retry ticked in the park's own question is the new approval the
        // hold waits for, also once the question label is off (#1829). A read
        // that failed proves no tick: the hold stays, untold.
        match self
            .retry_tick(n, p.question.as_deref())
            .map(|r| r.map(|(by, _)| by))
        {
            Some(Ok(by)) => {
                self.log(format!(
                    "lift #{n}: its last cycle parked on {cap} and the body has not changed since, lifted on the retry {by} answered"
                ));
                return Ok(false);
            }
            Some(Err(())) => {
                self.log(format!(
                    "hold #{n}: its last cycle parked on {cap} and the body has not changed since, and its park question could not be read"
                ));
                self.held_parked(n, &cap);
                return Ok(true);
            }
            None => {}
        }
        // A develop timeout on the same text twice offers no retry and asks
        // nothing: only a new text releases it.
        if p.question.is_none() && cap == "child-timeout:develop" {
            if !p.told && !self.dry {
                self.act(Action::ParkTold {
                    issue: n,
                    say: format!("Not started: the last cycle ran out of time in develop twice on this text, so it offers no retry: {}.", tehas_core::hold::HELD_PARKED_EDIT),
                });
            }
            self.log(format!(
                "hold #{n}: its last cycle parked on {cap} twice on the same text, and the body has not changed since"
            ));
            self.held_parked_as(n, &cap, tehas_core::hold::HELD_PARKED_EDIT);
            return Ok(true);
        }
        // With no question on record no reply can start the park, so the
        // label is not held.
        if p.question.is_none() {
            self.log(format!(
                "lift #{n}: its last cycle parked on {cap} and the body has not changed since, lifted as its park has no question on record to answer"
            ));
            return Ok(false);
        }
        if !p.told && !self.dry {
            self.act(Action::ParkTold {
                issue: n,
                say: format!("Not started: the last cycle parked on `{cap}` and the issue text has not changed since. Edit the body, or reply to the park comment with the word of one of its retries."),
            });
        }
        self.log(format!(
            "hold #{n}: its last cycle parked on {cap} and the body has not changed since"
        ));
        self.held_parked(n, &cap);
        Ok(true)
    }

    /// retry_tick: who ticked a Retry box in the question `qid` on record
    /// of a park and when, as questions::answer reads it, when one tick of an
    /// allowed account did; an answer `tehas answer` gave counts as the box
    /// it stands for. None when the park has no question or nothing in it
    /// counts; Some(Err) when its comments could not be read.
    fn retry_tick(&self, n: u64, qid: Option<&str>) -> Option<Result<(String, String), ()>> {
        let qid = qid.filter(|q| !q.is_empty())?;
        let Some(comments) = self.s.issues.get(&n).and_then(|i| i.comments.as_ref()) else {
            return Some(Err(()));
        };
        let Some(ans) = comments.or_none().and_then(|c| {
            crate::questions::answer_of(
                c,
                &self.bot,
                self.s.bot_id,
                &self.c.allowed,
                qid,
                &self.c.millwright,
            )
        }) else {
            return Some(Err(()));
        };
        let crate::questions::Answer::Asked(mut ans) = ans else {
            return None;
        };
        let given = self.s.issues.get(&n).and_then(|i| i.answer.as_ref());
        crate::answer::overlay(&mut ans, given, |by| self.c.allowed(by));
        if ans.issue_number() != Some(n)
            || ans.kind != "park"
            || ans.tick.status != crate::questions::Status::One
        {
            return None;
        }
        ans.ticked_label()
            .and_then(tehas_core::question::retry_of)?;
        Some(Ok((ans.tick.by.clone(), ans.tick.at.clone())))
    }

    /// The park's hold of `n` goes into the poll's report, so status names
    /// the issue the queue would otherwise show as next.
    fn held_parked(&mut self, n: u64, cap: &str) {
        self.held_parked_as(n, cap, tehas_core::hold::HELD_PARKED_CLEAR);
    }

    /// held_parked with what releases it said as `clear`.
    fn held_parked_as(&mut self, n: u64, cap: &str, clear: &str) {
        let Some(r) = self.d.report.as_mut() else {
            return;
        };
        if r.held.iter().any(|h| h.issue == n) {
            return;
        }
        let text = format!("its last cycle parked on `{cap}` and the body has not changed since");
        r.held.push(Held {
            issue: n,
            why: HELD_PARKED,
            text: text.chars().take(HELD_TEXT_MAX).collect(),
            clear: clear.into(),
        });
    }

    /// The millwright's tehas:ready comes off, and the issue says why.
    fn millwright_refused(&mut self, n: u64, why: &str) {
        self.log(format!(
            "skip #{n}: tehas:ready from the millwright starts nothing: {why}"
        ));
        if self.dry {
            self.log(format!("would take tehas:ready off #{n}"));
        } else {
            self.act(Action::Label(
                LabelChange::new(n, &["tehas:ready"], Self::label_fail(n, &["tehas:ready"], &[]))
                    .say(format!("Tehas took `tehas:ready` off this issue and started nothing. The millwright applied it, and the millwright may not start this issue: {why}. A person decides now: an account named in `TEHAS_ALLOWED_LABELERS` applies `tehas:ready` to start a cycle.")),
            ));
        }
    }

    /// The start order `tehas order` set, when one is in force: its issues
    /// that are ready, in order, and whether it holds every other ready issue.
    fn ranked(&mut self, ready: &[Listed]) -> (Vec<u64>, bool) {
        let o = &self.s.order;
        match o.state {
            OrderState::Ok => {}
            OrderState::Expired => {
                let until = o.until.as_deref().unwrap_or("?");
                self.log(format!(
                    "order expired at {until}: ready issues start lowest number first"
                ));
                return (Vec::new(), false);
            }
            OrderState::Unreadable => {
                self.log(
                    "WARN: the start order could not be read from the state store: ready issues start lowest number first",
                );
                return (Vec::new(), false);
            }
            OrderState::Other => return (Vec::new(), false),
        }
        let only = o.only;
        let listed: Vec<u64> = ready.iter().filter_map(|i| i.number).collect();
        let ranked = o
            .issues
            .iter()
            .copied()
            .filter(|n| listed.contains(n))
            .collect();
        (ranked, only)
    }

    /// 6 and 7: the candidates in their order, cut to the room.
    fn candidates(&mut self, room: Room) -> Step {
        let Room {
            want,
            want_all,
            want_refresh,
            ..
        } = room;
        self.cand_from = Some((self.d.log.len(), self.d.actions.len()));
        let ready = self.ready()?;
        let (ranked, only) = self.ranked(&ready);
        let mut order: Vec<(char, u64)> = Vec::new();
        let numbers: Vec<u64> = ready.iter().filter_map(|i| i.number).collect();
        order.extend(ranked.iter().map(|n| ('o', *n)));
        order.extend(self.s.refresh_requests.iter().map(|n| ('r', *n)));
        order.extend(
            numbers
                .iter()
                .filter(|n| !ranked.contains(n))
                .map(|n| ('x', *n)),
        );
        let mut ready_picked = 0;
        let mut refresh_picked = 0;
        self.cand_order = order
            .iter()
            .filter(|(k, _)| *k != 'r')
            .map(|(_, n)| *n)
            .collect();
        for (kind, n) in order {
            if self.d.picked.len() as i64 >= want_all {
                break;
            }
            if self.d.picked.contains(&n) {
                continue;
            }
            if kind == 'r' {
                if want_refresh.is_some_and(|w| refresh_picked >= w) {
                    let refreshing = self.live().filter(|(_, h)| h == "refresh").count() as i64;
                    let lane = (self.c.refresh_slots - refreshing).max(0);
                    self.log(format!(
                        "wait #{n}: the refresh lane is full ({} of {})",
                        self.c.refresh_slots - lane + refresh_picked,
                        self.c.refresh_slots
                    ));
                    continue;
                }
                let bot = self.bot.clone();
                if let Some(st) = self.refresh_ok(n, &bot)? {
                    self.d.picked.push(n);
                    refresh_picked += 1;
                    self.starts.push((n, st));
                }
                continue;
            }
            if kind == 'x' && only {
                self.cand_seen.push(n);
                if !self.running(n) && n != self.c.control {
                    self.log(format!("wait #{n}: not in the start order (--only)"));
                }
                continue;
            }
            if ready_picked >= want {
                continue;
            }
            self.cand_seen.push(n);
            let labels = ready
                .iter()
                .find(|i| i.number == Some(n))
                .map(|i| i.labels.clone())
                .unwrap_or_default();
            if self.plan_open(n) || has(&labels, "tehas:planning") {
                self.log(format!(
                    "skip #{n}: a planning session is open on it; end it first"
                ));
                continue;
            }
            if n == self.c.control {
                if !self.control_skipped {
                    self.log(format!(
                        "skip #{n}: the control issue is never worked on (`tehas:ready`)"
                    ));
                    self.control_skipped = true;
                }
                continue;
            }
            if self.running(n) {
                self.log(format!("wait #{n}: its cycle is still running"));
                continue;
            }
            if self.read_only_ran(n) {
                continue;
            }
            let host = self.host_start(n);
            let ask = if host.is_some() {
                HOST
            } else if has(&labels, "tehas:ready") {
                "tehas:ready"
            } else {
                "tehas:blocked"
            };
            let mut epic: Option<EpicGrant> = None;
            let mut planned = false;
            let (who, at) = if let Some(h) = &host {
                match self.host_ok(n, h) {
                    Some(at) => (HOST.to_string(), at),
                    None => continue,
                }
            } else {
                match self.plan_start(n, ask)? {
                    Planned::Refused => continue,
                    Planned::Go(who, at) => {
                        planned = true;
                        (who, at)
                    }
                    Planned::No => match self.approval(n, ask)? {
                        Ok(a) => a,
                        Err(Some(refused))
                            if ask == "tehas:ready"
                                && !refused.is_empty()
                                && self.c.is_millwright(&refused) =>
                        {
                            let ev = self.events(n)?;
                            let (_, at) = standing_actor(&ev, "tehas:ready");
                            match self.epic_ready(n, &refused, &at)? {
                                Some(Some(g)) => {
                                    let by = g.by.clone();
                                    epic = Some(g);
                                    (by, at)
                                }
                                Some(None) => continue,
                                None => {
                                    self.deny("skip", n, "tehas:ready", &refused);
                                    self.clear_unauthorised(n, "tehas:ready", &refused);
                                    continue;
                                }
                            }
                        }
                        Err(Some(refused)) => {
                            if ask == "tehas:ready" {
                                self.clear_unauthorised(n, "tehas:ready", or_unknown(&refused));
                            }
                            continue;
                        }
                        Err(None) => continue,
                    },
                }
            };
            if !self.links_ok(n) {
                continue;
            }
            if has(&labels, "tehas:blocked") && !self.blocked_ok(n)? {
                continue;
            }
            if !self.work_ok(n)? {
                continue;
            }
            if !self.epic_ok(n, ask, &labels) {
                continue;
            }
            if !self.size_ok(n, ask, &labels)? {
                continue;
            }
            if let Some(url) = self.open_pr(n)? {
                self.holds_on_pr(n, ask, &url);
                continue;
            }
            // A pull request stacked on another branch names its issue in its
            // body alone, and GitHub records no closing reference for it: the
            // page's read of the bodies is what holds this issue.
            if let Some(url) = self.named_pr(n) {
                self.holds_on_pr(n, ask, &url);
                continue;
            }
            if !self.image_ok(n)? || !self.image_missing_ok(n) || self.park_holds(n)? {
                continue;
            }
            let driver = match self.driver(n, &labels, false)? {
                Choice::Wait => continue,
                Choice::Stranger(s) => {
                    self.lean_refused(n, ask, &labels, &s);
                    continue;
                }
                Choice::Run(Driver::Lean) if !self.spec_ok(n, ask, &labels)? => {
                    continue;
                }
                d => d,
            };
            self.d.picked.push(n);
            ready_picked += 1;
            // A host start takes off a `tehas:ready` that is on the issue too,
            // so the label does not start the issue again after this cycle.
            let mut remove = match ask {
                HOST if has(&labels, "tehas:ready") => vec!["tehas:ready".to_string()],
                HOST => Vec::new(),
                _ => vec![ask.to_string()],
            };
            if ask != "tehas:blocked" && has(&labels, "tehas:blocked") {
                remove.push("tehas:blocked".into());
            }
            for l in [
                "tehas:awaiting-merge",
                "tehas:question",
                "tehas:millwright-escalated",
            ] {
                if has(&labels, l) {
                    remove.push(l.into());
                }
            }
            let mut st = Start {
                starter: who,
                verb: "labelled".into(),
                how: format!("`{ask}` label"),
                given_at: at,
                given_how: "approved".into(),
                remove,
                extra: StartExtra::default(),
            };
            if ask == HOST {
                st.verb = "started from the host".into();
                st.how = "`tehas start`".into();
                st.given_how = HOST.into();
            }
            // The start is recorded as the person's label: their tehas:ready
            // on the epic approved this issue, and a retry of the millwright's
            // rests on it as on any person's approval.
            if let Some(g) = &epic {
                st.verb = format!("approved on epic #{}, labelled by the millwright,", g.epic);
                st.how = format!(
                    "`tehas:ready` label on epic #{} at {}, which the millwright carried to this sub-issue",
                    g.epic, g.at
                );
            }
            if planned {
                st.verb = "planned".into();
                st.how = "`tehas:plan` label, on a plan with no open points".into();
                st.extra.approved_label = Some("tehas:plan".into());
            }
            driver.bind(&mut st.extra, &mut st.remove, &labels);
            self.starts.push((n, st));
        }
        let count = self.live().count();
        if self.dry {
            self.log(format!("{count} cycles running, room for {want} more"));
        }
        if self.d.picked.is_empty() {
            if self.dry {
                self.log("nothing to do");
            }
            return Ok(());
        }
        let starts = std::mem::take(&mut self.starts);
        if !self.dry {
            self.act_gates(self.d.picked[0]);
        }
        for (n, st) in starts {
            if self.dry {
                self.log(format!(
                    "would run a cycle for #{n} ({} by {})",
                    st.verb, st.starter
                ));
            } else {
                self.act_start(n, st);
            }
        }
        Ok(())
    }

    /// What every start passes first, before any label changes: systemd's
    /// answer for the first issue's unit (units_ok) and the code index
    /// (index_gate), which names the cycles still running after this poll's
    /// stops.
    pub(super) fn act_gates(&mut self, first: u64) {
        let running: Vec<u64> = self.live().map(|(n, _)| *n).collect();
        self.act(Action::Units { first });
        self.act(Action::IndexGate { running });
    }

    /// The start of a cycle as an action: what start_cycles records and
    /// changes.
    fn act_start(&mut self, n: u64, st: Start) {
        // What the approval binds: the sums and links of the text this poll
        // read, which the record is written from.
        let t = self.text(n);
        let body = t.body_text();
        let log = if st.given_how == HOST {
            format!("start #{n} ({})", st.verb)
        } else {
            format!("start #{n} ({} by {})", st.verb, st.starter)
        };
        self.act(Action::Start(Box::new(CycleStart {
            issue: n,
            by: st.starter,
            how: st.how,
            at: st.given_at,
            given: st.given_how,
            remove: st.remove,
            verb: st.verb,
            log,
            text: !t.empty,
            body_sha256: text::body_sum(body),
            title_sha256: text::title_sum(t.title.as_deref()),
            links: text::link_list(body),
            extra: st.extra,
        })));
    }
}

/// The most numbers `labelAlone` lists, so the line status prints is bounded.
pub(crate) const LABEL_ALONE_MAX: usize = 50;

/// The marker of the comment `hold_cleared` writes. It does not start
/// `<!-- tehas:blocked`, so `newest_marker` never reads it as one. Only the
/// bot's own comment counts as said.
const HOLD_CLEARED: &str = "<!-- tehas:hold-cleared -->";

/// The wait line of a `tehas:blocked` that counts while no comment names its
/// blockers. The dispatcher reads blockers only from comments, so it says
/// what a person must do; a marker in the issue's body is not read.
pub(crate) const LABEL_ALONE: &str = "no comment of an allowed account or the millwright names its blockers: it is held by the label alone, names no blockers and never starts until an allowed account or the millwright comments a `<!-- tehas:blocked by=N -->` marker or a person takes tehas:blocked off. A marker in the issue's body is not read";

/// The newest comment that holds a `tehas:blocked` marker and was written by
/// an allowed account or the millwright: the one rule 6 reads its blockers from.
fn newest_marker<'v>(nodes: &'v [Comment], c: &Settings) -> Option<&'v Comment> {
    let mut marked: Vec<&Comment> = nodes
        .iter()
        .filter(|x| {
            x.body_text()
                .is_some_and(|b| b.contains("<!-- tehas:blocked"))
                && {
                    let a = x.author.login.as_str();
                    !a.is_empty() && (c.allowed(a) || c.is_millwright(a))
                }
        })
        .collect();
    marked.sort_by_key(|x| {
        (
            x.created_text().unwrap_or_default().to_string(),
            CommentId::sort_key(x.id.as_ref()),
        )
    });
    marked.last().copied()
}

/// The numbers a tehas:blocked marker names, sorted and unique, or None when
/// no marker reads as one: the first `<!-- tehas:blocked by=12,34 -->` in the
/// body, each number 1 to 9 digits with no leading zero, at most 20.
fn blockers(body: &str) -> Option<Vec<u64>> {
    const OPEN: &str = "<!-- tehas:blocked by=";
    let mut from = 0;
    while let Some(i) = body[from..].find(OPEN) {
        let rest = &body[from + i + OPEN.len()..];
        from += i + OPEN.len();
        let len = rest
            .bytes()
            .take_while(|b| b.is_ascii_digit() || *b == b',')
            .count();
        if !rest[len..].starts_with(" -->") {
            continue;
        }
        let parts: Vec<&str> = rest[..len].split(',').collect();
        let ok = parts.len() <= 20
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 9 && !p.starts_with('0'));
        if !ok {
            continue;
        }
        let mut out: Vec<u64> = parts.iter().filter_map(|p| p.parse().ok()).collect();
        out.sort_unstable();
        out.dedup();
        return Some(out);
    }
    None
}

#[cfg(test)]
mod label_tests {
    use super::IMPROVEMENT;

    #[test]
    fn the_improvement_constant_keeps_its_spelling() {
        assert_eq!(IMPROVEMENT, "tehas:improvement");
    }
}

#[cfg(test)]
mod millwright_tests {
    use super::{base_approver, base_stands, record_base};
    use serde_json::{json, Value};

    const SUM: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn person(by: &str) -> Value {
        json!({"at": "2026-10-01T00:00:00Z", "by": by, "how": "approved", "body_sha256": SUM, "links": []})
    }

    fn by_millwright(rests_on: Value) -> Value {
        json!({"at": "2026-10-02T00:00:00Z", "by": "tehas-millwright", "how": "millwright", "body_sha256": SUM,
            "links": [], "rests_on": rests_on})
    }

    fn by_refresh() -> Value {
        json!({"at": "2026-10-01T00:00:00Z", "by": "alice", "how": "refresh", "body_sha256": SUM,
            "links": [], "refreshes": 1, "rests_on": person("alice")})
    }

    #[test]
    fn a_base_is_the_persons_approval_a_record_rests_on() {
        let want = json!({"at": "2026-10-01T00:00:00Z", "by": "alice", "how": "approved", "body_sha256": SUM});
        assert_eq!(record_base(&person("alice")), Some(want.clone()));
        assert_eq!(
            record_base(&by_millwright(person("alice"))),
            Some(want.clone())
        );
        assert_eq!(record_base(&by_refresh()), Some(want));
    }

    #[test]
    fn a_millwrights_record_that_rests_on_no_person_is_no_base() {
        assert_eq!(record_base(&by_millwright(Value::Null)), None);
        let twice = by_millwright(by_millwright(person("alice")));
        assert_eq!(record_base(&twice), None);
        let mut own = person("alice");
        own["how"] = json!("millwright");
        assert_eq!(record_base(&own), None);
    }

    /// #1967: the host's approval is a base the millwright and a finish may rest
    /// on, whatever the list of accounts names; a person's needs the list.
    #[test]
    fn the_hosts_approval_stands_as_a_base_with_no_account_on_the_list() {
        let c = crate::Settings::parse(&json!({"TEHAS_ALLOWED_LABELERS": "alice"}));
        let mut host = person("host");
        host["how"] = json!("host");
        let base = record_base(&by_millwright(host)).unwrap();
        assert_eq!(base["how"], "host");
        assert!(base_stands(&c, &base));
        assert!(base_stands(&c, &record_base(&person("alice")).unwrap()));
        assert!(!base_stands(&c, &record_base(&person("host")).unwrap()));
    }

    /// #2571: the host's approval names no account, so a change in its second
    /// by an account called `host` is after it. A person's own is not.
    #[test]
    fn the_hosts_approval_has_no_approver_to_excuse_a_change_in_its_second() {
        let mut host = person("host");
        host["how"] = json!("host");
        let base = record_base(&host).unwrap();
        assert_eq!(base_approver(&base), "");
        let at = base["at"].as_str().unwrap();
        assert!(tehas_core::events::changed_after(
            at,
            "host",
            at,
            base_approver(&base)
        ));
        let own = record_base(&person("host")).unwrap();
        assert_eq!(base_approver(&own), "host");
        assert!(!tehas_core::events::changed_after(
            at,
            "host",
            at,
            base_approver(&own)
        ));
    }

    #[test]
    fn an_approval_whose_account_has_a_newline_on_the_end_is_no_base() {
        assert_eq!(record_base(&person("alice\n")), None);
    }
}

#[cfg(test)]
mod check_state_tests {
    use super::{check_state, Check};
    use serde_json::{json, Value};

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const HEAD: &str = "0123456789ab";

    fn answer(runs: Vec<Value>) -> Value {
        json!({"check_runs": runs})
    }

    fn run(status: &str, conclusion: Option<&str>) -> Value {
        json!({"head_sha": SHA, "status": status, "conclusion": conclusion})
    }

    /// Every conclusion GitHub answers is classified: the three that pass, the
    /// three that are red, and the rest that leave the gate's outcome unknown
    /// and hold the start.
    #[test]
    fn every_finished_conclusion_is_classified() {
        for c in ["success", "neutral", "skipped"] {
            let a = answer(vec![run("completed", Some(c))]);
            assert_eq!(check_state(&a), Some(Check::Done(HEAD.into())), "{c}");
        }
        for c in ["failure", "timed_out", "startup_failure"] {
            let a = answer(vec![run("completed", Some(c))]);
            assert_eq!(check_state(&a), Some(Check::Red(HEAD.into())), "{c}");
        }
        for c in ["cancelled", "action_required", "stale", "wontfix"] {
            let a = answer(vec![run("completed", Some(c))]);
            assert_eq!(
                check_state(&a),
                Some(Check::Unknown(HEAD.into())),
                "{c} must hold"
            );
        }
        // A finished run that names no conclusion at all is unknown too.
        let a = answer(vec![run("completed", None)]);
        assert_eq!(check_state(&a), Some(Check::Unknown(HEAD.into())));
        let a = json!({"check_runs": [{"head_sha": SHA, "status": "completed"}]});
        assert_eq!(check_state(&a), Some(Check::Unknown(HEAD.into())));
    }

    #[test]
    fn a_run_that_has_not_finished_is_pending() {
        for st in ["queued", "in_progress"] {
            let a = answer(vec![run(st, None)]);
            assert_eq!(check_state(&a), Some(Check::Pending(HEAD.into())), "{st}");
        }
        // One run of two that has not finished is enough: nothing is known of
        // the gate until all of them answer.
        let a = answer(vec![
            run("completed", Some("cancelled")),
            run("in_progress", None),
        ]);
        assert_eq!(check_state(&a), Some(Check::Pending(HEAD.into())));
    }

    #[test]
    fn a_red_run_outweighs_every_other_reading() {
        let a = answer(vec![
            run("completed", Some("success")),
            run("completed", Some("failure")),
        ]);
        assert_eq!(check_state(&a), Some(Check::Red(HEAD.into())));
    }

    #[test]
    fn no_runs_and_an_answer_that_is_not_runs() {
        assert_eq!(check_state(&answer(vec![])), Some(Check::None));
        assert_eq!(check_state(&json!({})), None);
        assert_eq!(check_state(&json!({"check_runs": {}})), None);
        // A head sha the poll cannot name is an answer it cannot read.
        let a = json!({"check_runs": [{"head_sha": "<b>", "status": "completed",
                                       "conclusion": "success"}]});
        assert_eq!(check_state(&a), None);
    }
}

#[cfg(test)]
mod demand_tests {
    use super::{demand_after, Demand, DemandCount};

    fn planning() -> Vec<(u64, String)> {
        vec![
            (7, "2026-10-17T08:00:00Z".to_string()),
            (8, "2026-10-17T11:00:00Z".to_string()),
        ]
    }

    fn count(waiting: u64, oldest: Option<&str>, label_alone: Vec<u64>) -> Demand {
        Demand::Counted(DemandCount {
            waiting,
            oldest: oldest.map(str::to_string),
            refresh_hold: None,
            label_alone,
        })
    }

    #[test]
    fn a_session_this_poll_ended_counts_its_issue_and_its_wait() {
        let d = count(1, Some("2026-10-17T10:00:00Z"), vec![9]);
        assert_eq!(
            demand_after(&d, &planning(), &[7]),
            Some(count(2, Some("2026-10-17T08:00:00Z"), vec![9]))
        );
        assert_eq!(
            demand_after(&count(0, None, vec![]), &planning(), &[8, 7]),
            Some(count(2, Some("2026-10-17T08:00:00Z"), vec![]))
        );
    }

    #[test]
    fn no_session_of_a_waiting_issue_ended_leaves_the_demand_as_written() {
        let d = count(0, None, vec![]);
        assert_eq!(demand_after(&d, &planning(), &[]), None);
        assert_eq!(demand_after(&d, &planning(), &[12]), None);
        assert_eq!(demand_after(&d, &[], &[7]), None);
        // A demand the poll could not count stays uncounted.
        assert_eq!(demand_after(&Demand::Unread, &planning(), &[7]), None);
    }
}
