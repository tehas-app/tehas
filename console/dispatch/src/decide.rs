//! One poll's decisions, in the order libexec/dispatch takes them. Every line
//! it logs and every text it writes on an issue is the bash's, word for word,
//! so the two can be compared line by line.

use crate::events::{
    first_unallowed_rename, label_actor, label_state, pause_holds, plan_credit, sole_application,
    Event, Events, UNKNOWN_ACTOR,
};
use crate::settings::{account, Settings};
use crate::slice::{self, gib};
use crate::slots::{self, Probe, Slots};
use crate::snapshot::{Issues, Listed, Snapshot};
use crate::text;
use serde_json::{json, Map, Value};
use tehas_core::hold::{Report, Why};

mod clean;
mod plan;
mod retry;

pub(crate) use clean::{iso, quiet_due, QUIET_ASKS, QUIET_BOUND};
pub(crate) use plan::shape;
pub(crate) use retry::{hex40, refresh_why};

/// The time now as log() writes it.
pub fn iso_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso(secs)
}

/// The numbers every blocker comment of an issue names, for the snapshot to
/// ask their states: blocked_ok reads the newest one an allowed account wrote.
pub(crate) fn named_blockers(comments: &Value) -> Vec<u64> {
    let mut out: Vec<u64> = comments
        .pointer("/data/repository/issue/comments/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.get("body").and_then(Value::as_str))
        .filter_map(blockers)
        .flatten()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The label that chooses the lean driver for one issue.
pub(crate) const LEAN: &str = "tehas:lean";

/// The label that holds an issue out of rule 6 until its blockers are closed.
/// Spelled once so the read that batchs its reads and the rule that waits on
/// it cannot drift apart.
pub(crate) const BLOCKED: &str = "tehas:blocked";

/// The driver a start runs, as `Poll::driver` reads it.
enum Driver {
    Classic,
    Lean,
    /// An account outside the list made the choice: nothing starts, classic
    /// included.
    Stranger(Stranger),
    /// Its record could not be read this poll.
    Wait,
}

impl Driver {
    /// The action of a classic start names no driver.
    fn bind(&self, extra: &mut Map<String, Value>) {
        if matches!(self, Driver::Lean) {
            extra.insert("driver".into(), json!("lean"));
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

/// Who made the choice of driver that does not count: applied tehas:lean, or
/// with `off` took an allowed account's off.
struct Stranger {
    who: String,
    off: bool,
}

impl Stranger {
    /// What a start refused over it says on the issue.
    fn said(&self) -> String {
        let (what, rest) = if self.off {
            ("taken off", "and takes it off for classic")
        } else {
            ("applied", "or takes it off for classic")
        };
        format!("`{LEAN}` was {what} by `{}`, an account that may not choose the driver in this factory, and tehas does not run classic in its place. An account named in `TEHAS_ALLOWED_LABELERS` applies `{LEAN}` again, {rest}", self.who)
    }
}

/// What a poll does: its exit status, the lines it logs, the writes it makes,
/// in order, and the files it publishes.
#[derive(Clone, Debug, Default)]
pub struct Decision {
    pub exit: i32,
    pub log: Vec<String>,
    pub actions: Vec<Value>,
    /// live/dispatch.json, without its time. A dry run writes none. The keys
    /// and the hold's words are tehas-core's, so the readers of the file name
    /// them by the same type the writer does.
    pub report: Option<Report>,
    /// live/demand.json, without its time: Some(Null) removes it. A dry run
    /// writes none.
    pub demand: Option<Value>,
    /// The issues whose cycle starts, in order.
    pub picked: Vec<u64>,
    /// Facts the snapshot lacked that this decision needed. When any is
    /// named, the caller reads them and decides again.
    pub needs: Vec<&'static str>,
    /// Past the bot's checks: the poll's end runs tidy from here on.
    pub tidy: bool,
    /// The bot's login, which the poll's writes name.
    pub bot: String,
    /// Where the poll sweeps the unused locks (after its clean-up) and
    /// publishes its report (at rule 3): the action and the line each comes
    /// before, as the bash does them there.
    pub swept_at: Option<(usize, usize)>,
    pub reported_at: Option<(usize, usize)>,
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
    /// A start of the fixer's: the person's approval it rests on, and its
    /// starts with this one.
    fixer: Option<(Value, u64)>,
    /// The millwright's guidance comment that lifted the park's hold off this
    /// start's label: the one entry the record names `park_lifted`, so the
    /// reason the start was allowed is on it (#1130). A person's label lifts
    /// nothing.
    park_lifted: Option<Value>,
    /// What a start of its kind adds to the action: the comment of a retry
    /// tick, the pull request of a refresh.
    extra: Map<String, Value>,
}

struct Poll<'a> {
    s: &'a Snapshot,
    c: &'a Settings,
    dry: bool,
    d: Decision,
    control_skipped: bool,
    /// Fed starts this poll picked, beside the ones running.
    fed_picked: u64,
    work_refused: Vec<u64>,
    gate_asked: bool,
    gate_answer: i32,
    starts: Vec<(u64, Start)>,
    /// image_ok's answer, asked once a poll: the hold, empty for none.
    image_hold: Option<String>,
    /// The finished cycles this poll stops: their slots are free.
    stopped: Vec<u64>,
    /// The bot's login, once the poll has read it.
    bot: String,
    /// The cycles and the containers this poll's readings give, `auto` or as
    /// written: settled once, and asked of no reading a number does not need.
    slots: Slots,
    /// "hold refreshes" was said this poll.
    refresh_said: bool,
    /// Parked issues with a box ticked: rule 5a leaves them.
    park_ticked: Vec<u64>,
    /// The issue whose planning turn is still running, and the sessions
    /// this poll closed.
    plan_busy: Option<u64>,
    plan_closed: Vec<u64>,
}

/// The decisions of one poll, a dry run when `dry`.
pub fn decide(s: &Snapshot, c: &Settings, dry: bool) -> Decision {
    let mut p = Poll {
        s,
        c,
        dry,
        d: Decision::default(),
        control_skipped: false,
        fed_picked: 0,
        work_refused: Vec::new(),
        gate_asked: false,
        gate_answer: 0,
        starts: Vec::new(),
        image_hold: None,
        stopped: Vec::new(),
        bot: String::new(),
        slots: Slots {
            cycles: 0,
            containers: 0,
            cycle_how: "fixed",
            container_how: "fixed",
            cycle: None,
            container: None,
        },
        refresh_said: false,
        park_ticked: Vec::new(),
        plan_busy: None,
        plan_closed: Vec::new(),
    };
    p.d.exit = match p.run() {
        Ok(()) => 0,
        Err(End(rc)) => rc,
    };
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
        self.d.log.push(line.into());
    }

    fn fail(&mut self, line: impl Into<String>, rc: i32) -> End {
        self.log(line);
        End(rc)
    }

    /// An action, with its place among the poll's lines: the lines before it
    /// are logged before it is applied.
    fn act(&mut self, mut a: Value) {
        if let Some(o) = a.as_object_mut() {
            o.insert("_line".into(), json!(self.d.log.len()));
        }
        self.d.actions.push(a);
    }

    /// The line the bash poll logs when a label change fails: the flags it
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
        let bot = if !c.bot_login.is_empty() {
            c.bot_login.clone()
        } else {
            let Some(user) = &s.user else {
                return Err(self.fail("ERROR: gh api user failed", 1));
            };
            user.get("login")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        if bot.is_empty() {
            return Err(self.fail("ERROR: the bot's own login could not be read", 1));
        }
        if c.allowed(&bot) {
            return Err(self.fail(
                format!("ERROR: the bot account {bot} is in TEHAS_ALLOWED_LABELERS; the bot never authorises work"),
                78,
            ));
        }
        if c.is_millwright(&bot) {
            return Err(self.fail(
                format!("ERROR: the bot account {bot} is TEHAS_MILLWRIGHT_LOGIN; every agent posts as the bot, so the millwright must be an account of its own"),
                78,
            ));
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
        if !self.dry {
            let demand = self.demand();
            self.d.demand = Some(demand);
        }
        let (holds, news) = match pause {
            Some(h) => (Some(h), false),
            None => self.pause_label()?,
        };
        self.bot = bot.clone();
        self.clean_up(&bot)?;
        self.d.swept_at = Some((self.d.actions.len(), self.d.log.len()));
        // 1b. The pause switches end the poll here.
        if let Some(h) = holds {
            if self.dry || news {
                self.log(format!("paused: {h}"));
            }
            return Ok(());
        }
        let room = self.room()?;
        self.questions(&room, &bot)?;
        self.plan_new(&room)?;
        self.plan_refine(&room)?;
        // With no room for new work, only a refresh may still have room.
        if room.want == 0 {
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
        let f = self.s.pause_flag.as_ref()?;
        let ok = f.as_object().filter(|o| {
            o.get("at").map(Value::is_string) == Some(true)
                && o.get("reason").map(Value::is_string) == Some(true)
        });
        match ok {
            Some(o) => Some(format!(
                "the flag tehas pause wrote at {} holds this host",
                o["at"].as_str().unwrap_or("")
            )),
            None => {
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
    fn demand(&mut self) -> Value {
        let Some(list) = &self.s.ready.0 else {
            return Value::Null;
        };
        let (mut waiting, mut oldest) = (0, String::new());
        for i in list.iter().filter(|i| !has(&i.labels, "tehas:running")) {
            let Some(n) = i.number else { continue };
            if n == self.c.control || self.plan_open(n) || self.running(n) {
                continue;
            }
            let ev = match self.s.issues.get(&n).map(|x| &x.events) {
                Some(Events::Read(v)) => v,
                _ => return Value::Null,
            };
            let (by, at) = label_actor(ev, "tehas:ready");
            if !self.c.allowed(&by) && !self.c.is_millwright(&by) && !self.planned(n, &by) {
                continue;
            }
            waiting += 1;
            if !at.is_empty() && (oldest.is_empty() || at < oldest) {
                oldest = at;
            }
        }
        // A refresh request waits as a ready issue does, unless refreshes are
        // held. The hold is in the file, and the log has it once.
        let mut held = String::new();
        let requests = self.s.refresh_requests.clone();
        if !requests.is_empty() {
            held = self.refresh_held().unwrap_or_default();
            if held != self.s.refresh_held {
                if !held.is_empty() {
                    self.log(format!("hold refreshes: {held}"));
                }
                self.act(json!({"refresh_held": held}));
            }
            self.refresh_said = true;
        } else if !self.s.refresh_held.is_empty() {
            self.act(json!({"refresh_held": ""}));
        }
        if held.is_empty() {
            for n in requests {
                if self.plan_open(n) || self.running(n) {
                    continue;
                }
                let at = self
                    .s
                    .issues
                    .get(&n)
                    .and_then(|i| i.request.as_ref())
                    .and_then(|r| r.get("at"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                waiting += 1;
                if !at.is_empty() && (oldest.is_empty() || at < oldest) {
                    oldest = at;
                }
            }
        }
        let mut m = Map::new();
        m.insert("waiting".into(), json!(waiting));
        if !oldest.is_empty() {
            m.insert("oldest".into(), json!(oldest));
        }
        if !held.is_empty() {
            m.insert("refreshHold".into(), json!(held));
        }
        Value::Object(m)
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
        let flow = |l: &str| {
            matches!(
                l,
                "tehas:running"
                    | "tehas:awaiting-merge"
                    | "tehas:question"
                    | "tehas:planning"
                    | "tehas:refreshing"
                    | "tehas:plan"
                    | "tehas:refine"
            )
        };
        let mut all: Vec<Listed> = ready;
        all.extend(
            blocked
                .into_iter()
                .filter(|i| !i.labels.iter().any(|l| flow(l))),
        );
        all.sort_by_key(|i| i.number);
        all.dedup_by_key(|i| i.number);
        Ok(all)
    }

    /// 2 and 3: the slots, the limit, the slice and the cap on unreviewed
    /// work, with the hold that keeps cycles back when one does.
    fn room(&mut self) -> Step<Room> {
        let s = self.s;
        let c = self.c;
        let count = self.live().count() as i64;
        let mut cycles = count;
        let mut lane: Option<i64> = None;
        if c.refresh_conflicts && c.refresh_slots > 0 {
            let refreshing = self.live().filter(|(_, how)| how == "refresh").count() as i64;
            cycles = count - refreshing;
            lane = Some((c.refresh_slots - refreshing).max(0));
        }
        // The ceiling the poll decides by. `auto` is derived from the two
        // readings the snapshot took this poll, so a host that grew or
        // shrank is answered at the next poll and a running cycle is never
        // stopped: only the room to start another moves.
        self.slots = slots::derive(
            &Probe::read(
                s.slice
                    .show
                    .as_ref()
                    .and_then(|m| m.get("MemoryMax"))
                    .and_then(Value::as_str)
                    .and_then(slice::bytes),
                s.slice.cpus.as_deref().and_then(|c| c.parse().ok()),
                &c.container_memory,
                &c.cycle_cpus,
                &c.cycle_ceiling,
            ),
            &c.slice_containers,
            &c.concurrency_written,
        );
        let concurrency = self.slots.cycles;
        let free = concurrency - cycles;
        let limit_state = s
            .limit
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unreadable")
            .to_string();
        let limit_cycles = s.limit.get("cycles").and_then(Value::as_i64).unwrap_or(0);
        let limit_reason = s
            .limit
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let limit_room: Option<i64> = match limit_state.as_str() {
            "ok" => Some((limit_cycles - count).max(0)),
            "expired" | "unreadable" => Some(0),
            _ => None,
        };
        let limit_words = if limit_cycles == 1 {
            "1 cycle".to_string()
        } else {
            format!("{limit_cycles} cycles")
        };
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
        // The containers in force, so `auto` divides the slice by the number
        // the poll derived rather than by the word in the setting.
        let containers = self.slots.containers.to_string();
        match slice::usable(&s.slice, &c.reserve, &containers) {
            Ok(u) => {
                ceiling_mb = u.ceiling_mb;
                let r = slice::room(&s.slice, &u, &c.reserve, count as u64);
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
        let limit_ok = limit_state != "expired" && limit_state != "unreadable";
        let limit_plan_ok = limit_ok
            && !matches!(s.limit.get("plan"), Some(Value::Bool(false)))
            && s.limit.get("plan").and_then(Value::as_str) != Some("false");
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
        } else if let Some(Some(kb)) =
            disk_low(self).filter(|kb| matches!(kb, Some(k) if *k < c.min_free_mb * 1024))
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
        } else if limit_state == "expired" {
            why = Some(Why::Limit);
            hold =
                format!(
                "holding: the limit expired at {}: nothing starts until it is set again or cleared",
                s.limit.get("until").and_then(Value::as_str).unwrap_or("null")
            );
        } else if limit_state == "unreadable" {
            why = Some(Why::Limit);
            hold = format!(
                "holding: the limit cannot be read: nothing starts until it is set again or cleared ({}/state/limit.json)",
                c.home
            );
        } else if matches!(limit_room, Some(r) if r <= 0) {
            why = Some(Why::Limit);
            hold = format!(
                "holding: the limit allows {limit_words} at once and {count} {} running{}",
                if count == 1 { "is" } else { "are" },
                with_reason(&limit_reason)
            );
        } else if let Some(word) = self.branch_check() {
            why = Some(word.0);
            hold = word.1;
        } else if free <= 0 {
            why = Some(Why::Count);
            hold = match slots::words(&self.slots).as_str() {
                // A number as always: the count alone.
                "" => format!("holding: {cycles} cycles are running (limit {concurrency})"),
                // `auto` names its count and the reading that gave it, so the
                // line says what a resize of the host did.
                why => format!("holding: {cycles} cycles are running ({why})"),
            };
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
                slots: self.slots.report(),
            });
            self.d.reported_at = Some((self.d.actions.len(), self.d.log.len()));
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
        let answer = match &self.s.checks {
            Some(a) => a.clone(),
            None => {
                self.need("checks");
                return None;
            }
        };
        let state = answer.as_ref().and_then(check_state);
        // None stands for the answer the poll could not read: the arm below
        // that holds nothing, so it is named here rather than left to a
        // catch-all.
        let state = state.or(Some(Check::Unreadable));
        let (name, branch) = (&c.branch_check, &c.branch);
        // Exhaustive: a new state is a compile error here, never a fall
        // through to the arm that starts cycles.
        let (red, note) = match state {
            Some(Check::Red(sha)) => (
                Some((Why::Red, format!("holding: {branch} is red at {sha}"))),
                String::new(),
            ),
            Some(Check::Done(_)) => (None, String::new()),
            Some(Check::Pending(sha)) => (
                None,
                format!("the check {name} on {branch} at {sha} has not finished: cycles start"),
            ),
            Some(Check::None) => (
                None,
                format!("{branch} has no check named {name} on its head commit: cycles start"),
            ),
            // The runs finished and none says it passed: the poll does not
            // know what ended the gate, so nothing starts on it unsaid. The
            // note of the same words goes to state/branch-check, where the
            // manual sends a person for the branch's condition. The name is
            // quoted: an instance may set one with spaces in it.
            Some(Check::Unknown(sha)) => {
                let said = format!(
                    "the check {name:?} on {branch} at {sha} finished without saying it passed: cycles start"
                );
                // The hold's own word, not `red`: a person fixes a red gate on
                // the branch, and an outcome the poll cannot read by naming
                // the check that answers it. The hold itself is the same hold.
                (Some((Why::UnknownGate, format!("holding: {said}"))), said)
            }
            Some(Check::Unreadable) | None => (
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
            self.act(json!({"branch_note": note}));
        }
        red
    }
}

/// What the poll reads of a check's runs on the branch's head. The words are
/// the set the store pins in console/store/src/schema.sql
/// (`red`|`done`|`pending`|`none`|`unreadable`), so this enum is the contract
/// in both places; `libexec/dispatch`'s jq of the same answer keeps its own
/// copies until its own issue ports them.
#[derive(Debug, PartialEq, Eq)]
enum Check {
    /// A run says it failed, timed out or could not start.
    Red(String),
    /// Every run says it passed.
    Done(String),
    /// A run has not finished.
    Pending(String),
    /// The branch's head has no run of that check at all.
    None,
    /// Every run finished and none says it passed: the poll cannot tell what
    /// ended the gate, so nothing starts on it unsaid. Its sha is the head the
    /// hold names.
    Unknown(String),
    /// The answer is no answer: no list of runs, or a head sha that is not one.
    Unreadable,
}

/// The state of a branch's check runs, as libexec/dispatch reads them. Of the
/// conclusions GitHub answers, these three pass: `success`, `neutral` and
/// `skipped`; these three are red: `failure`, `timed_out` and
/// `startup_failure`. Every other conclusion, and a finished run that names
/// none, is unknown: the gate holds. A conclusion GitHub adds later is a
/// deliberate edit to one of the three lists beside this function, never a
/// run that reads as green.
fn check_state(a: &Value) -> Option<Check> {
    let runs = a.get("check_runs")?.as_array()?;
    if runs.is_empty() {
        return Some(Check::None);
    }
    let sha_ok = |r: &Value| {
        r.get("head_sha").and_then(Value::as_str).is_some_and(|s| {
            s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
    };
    if !runs.iter().all(sha_ok) {
        return None;
    }
    let sha = &runs[0]["head_sha"].as_str()?[..12];
    // One walk of the runs: which of the four readings hold.
    let mut any_red = false;
    let mut all_done = true;
    let mut all_passed = true;
    for r in runs {
        let finished = r.get("status").and_then(Value::as_str) == Some("completed");
        let conclusion = r.get("conclusion").and_then(Value::as_str);
        all_done &= finished;
        if finished
            && matches!(
                conclusion,
                Some("failure" | "timed_out" | "startup_failure")
            )
        {
            any_red = true;
        }
        all_passed &= finished && matches!(conclusion, Some("success" | "neutral" | "skipped"));
    }
    if any_red {
        Some(Check::Red(sha.into()))
    } else if all_passed {
        Some(Check::Done(sha.into()))
    } else if all_done {
        Some(Check::Unknown(sha.into()))
    } else {
        Some(Check::Pending(sha.into()))
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

    /// order_label: tehas:next or tehas:blocked stands when an allowed account
    /// or the fixer applied it last and nobody took it off since.
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
        if self.c.allowed(state.actor()) || self.c.is_millwright(state.actor()) {
            return Ok(true);
        }
        self.deny("ignore", n, label, state.actor());
        Ok(false)
    }

    /// driver: the driver of a start, read of who applied tehas:lean and never
    /// of the label alone. With the label off a start is classic, whatever the
    /// last one ran, unless an account outside the list took an allowed
    /// account's choice off. With it on, a start that rests on an earlier
    /// approval (`retry`) keeps the driver on that record, so a label added
    /// since changes nothing until a person approves a new start.
    fn driver(&mut self, n: u64, labels: &[String], retry: bool) -> Step<Driver> {
        if !has(labels, LEAN) {
            return Ok(match self.lean_taken_off(n) {
                Some(who) => Driver::Stranger(Stranger { who, off: true }),
                None => Driver::Classic,
            });
        }
        let ev = self.events(n)?;
        let (who, _) = label_actor(&ev, LEAN);
        if !self.c.allowed(&who) {
            let who = or_unknown(&who).to_string();
            return Ok(Driver::Stranger(Stranger { who, off: false }));
        }
        if !retry {
            return Ok(Driver::Lean);
        }
        match self.s.issues.get(&n).and_then(|i| i.approval.clone()) {
            None => {
                self.need("approval");
                Ok(Driver::Wait)
            }
            Some(Err(())) => {
                self.log(format!("wait #{n}: its approval could not be read"));
                Ok(Driver::Wait)
            }
            // No start on record to follow: this one is the first.
            Some(Ok(None)) => Ok(Driver::Lean),
            Some(Ok(Some(r))) if r.get("driver") == Some(&json!("lean")) => Ok(Driver::Lean),
            Some(Ok(Some(_))) => Ok(Driver::Classic),
        }
    }

    /// lean_taken_off: who took tehas:lean off, when an allowed account's
    /// choice went and that account may not take it. An issue whose events
    /// were not read names nobody: each start reads them itself.
    fn lean_taken_off(&self, n: u64) -> Option<String> {
        let i = self.s.issues.get(&n)?;
        let Events::Read(ev) = &i.events else {
            return None;
        };
        let state = label_state(ev, LEAN);
        (state.event() == "unlabeled"
            && !self.c.allowed(state.actor())
            && self.c.allowed(&label_actor(ev, LEAN).0))
        .then(|| or_unknown(state.actor()).to_string())
    }

    /// lean_deny: the log line of a choice of driver that does not count.
    fn lean_deny(&mut self, n: u64, s: &Stranger) {
        if s.off {
            let who = &s.who;
            self.log(format!(
                "skip #{n}: {LEAN} was taken off by '{who}', not an allowed account"
            ));
        } else {
            self.deny("skip", n, LEAN, &s.who);
        }
    }

    /// lean_refused: the labels that ask for the start come off, so this is
    /// said once. tehas:lean stays, or the next tehas:ready would run classic.
    fn lean_refused(&mut self, n: u64, ask: &str, labels: &[String], s: &Stranger) {
        self.lean_deny(n, s);
        let mut off = vec![ask];
        if ask != "tehas:blocked" && has(labels, "tehas:blocked") {
            off.push("tehas:blocked");
        }
        let names = format!("`{}`", off.join("` and `"));
        if self.dry {
            self.log(format!(
                "would take {} off #{n}: {LEAN} is not an allowed account's",
                off.join(" and ")
            ));
        } else {
            self.act(json!({
                "issue": n, "remove": off,
                "fail": Self::label_fail(n, &off, &[]),
                "say": format!("Tehas took {names} off this issue and started nothing: {}, then applies `{ask}`.", s.said()),
            }));
        }
    }

    /// unchanged_since: the title and body are what the approval at `since`
    /// covered, changed since by nobody outside the list. The two reads of a
    /// change rank a missing time in opposite directions on purpose: an
    /// unstamped rename counts as after the approval and holds the start (see
    /// `first_unallowed_rename`), and an unstamped edit is no edit at all, as
    /// the bash poll's `[ -n "$edited_at" ]` reads it.
    fn unchanged_since(&mut self, n: u64, since: &str, what: &str, again: &str) -> Step<bool> {
        let ev = self.events(n)?;
        let c = self.c;
        // The rename is asked first and answers first: it is the one change of
        // the two GitHub stamps least often.
        let stale = match first_unallowed_rename(&ev, since, &|w| c.allowed(w)) {
            Some(who) => Some(who),
            None => {
                let Some(edit) = self.s.issues.get(&n).and_then(|i| i.edit.clone()) else {
                    return Err(self.fail("ERROR: gh api graphql failed", 1));
                };
                let at = edit
                    .get("lastEditedAt")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let by = edit
                    .get("editor")
                    .and_then(|e| e.get("login"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !at.is_empty() && (since.is_empty() || at > since) {
                    let who = or_unknown(by).to_string();
                    (!self.c.allowed(&who)).then_some(who)
                } else {
                    None
                }
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
    /// Returns the account and time, or the account refused for not being
    /// allowed (empty for every other refusal).
    fn approval(&mut self, n: u64, label: &str) -> Step<Result<(String, String), String>> {
        let ev = self.events(n)?;
        let (who, at) = label_actor(&ev, label);
        if !self.c.allowed(&who) {
            if label != "tehas:ready" || !self.c.is_millwright(&who) {
                self.deny("skip", n, label, &who);
            }
            return Ok(Err(or_unknown(&who).to_string()));
        }
        if self.unchanged_since(
            n,
            &at,
            &format!("{who} applied {label}"),
            "apply the label again",
        )? {
            Ok(Ok((who, at)))
        } else {
            Ok(Err(String::new()))
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
        let (who, at) = label_actor(&ev, ask);
        if !self.planned(n, &who) {
            return Ok(Planned::No);
        }
        let record = self.s.issues[&n].plan_start.clone().unwrap_or(Value::Null);
        let by = record.get("by").and_then(Value::as_str).unwrap_or("");
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
        if !self.unchanged_since(n, &at, "the plan was written", "apply `tehas:ready`")? {
            return Ok(self.plan_taken_off(n, "its text was changed since the plan was written"));
        }
        // The time cannot see a change made before the label went on.
        if record.get("sum").and_then(Value::as_str) != Some(&text::plan_sum(&self.text(n))) {
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
            self.act(json!({
                "issue": n, "remove": ["tehas:ready"],
                "fail": Self::label_fail(n, &["tehas:ready"], &[]),
                "say": format!("Tehas took `tehas:ready` off this issue and started nothing: {why}. Read it, and an account named in `TEHAS_ALLOWED_LABELERS` applies `tehas:ready` to build it."),
            }));
            self.act(json!({"plan_start_drop": n}));
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
        self.act(json!({
            "issue": n, "remove": [label],
            "fail": Self::label_fail(n, &[label], &[]),
            "log": format!("clear #{n}: {label} off, applied by '{by}' who may not authorise work here"),
            "say": format!("Tehas took `{label}` off this issue. It was applied by `{by}`, an account that may not authorise work in this factory, so nothing was started and the issue is not queued. To ask again, an account named in `TEHAS_ALLOWED_LABELERS` applies `{label}`."),
        }));
    }

    /// blocked_ok: whether tehas:blocked lets the issue start.
    fn blocked_ok(&mut self, n: u64) -> Step<bool> {
        if !self.order_label(n, "tehas:blocked")? {
            return Ok(true);
        }
        let Some(answer) = self.s.issues.get(&n).and_then(|i| i.comments.clone()) else {
            return Err(self.fail("ERROR: gh api graphql failed", 1));
        };
        let Some(nodes) = answer
            .pointer("/data/repository/issue/comments/nodes")
            .and_then(Value::as_array)
        else {
            return Err(self.fail(format!("ERROR: unreadable comments of #{n}"), 1));
        };
        let c = self.c;
        let mut marked: Vec<&Value> = nodes
            .iter()
            .filter(|x| {
                x.get("body")
                    .and_then(Value::as_str)
                    .is_some_and(|b| b.contains("<!-- tehas:blocked"))
                    && {
                        let a = x
                            .pointer("/author/login")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        !a.is_empty() && (c.allowed(a) || c.is_millwright(a))
                    }
            })
            .collect();
        marked.sort_by_key(|x| {
            (
                x.get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                x.get("databaseId").and_then(Value::as_i64).unwrap_or(0),
            )
        });
        let why = match marked.last() {
            None => "no comment of an allowed account or the fixer names its blockers".to_string(),
            Some(x) => {
                let author = x
                    .pointer("/author/login")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let editor = x
                    .pointer("/editor/login")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !x.get("lastEditedAt").unwrap_or(&Value::Null).is_null() && editor != author {
                    format!("the newest comment that names its blockers was edited by '{editor}'")
                } else {
                    match blockers(x.get("body").and_then(Value::as_str).unwrap_or("")) {
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
        };
        if why.is_empty() {
            return Ok(true);
        }
        self.log(format!("wait #{n}: tehas:blocked holds it: {why}"));
        Ok(false)
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
        let kind = if !w.is_object() {
            "unreadable"
        } else if !w.get("pull_request").unwrap_or(&Value::Null).is_null() {
            "pr"
        } else {
            match w
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase()
                .as_str()
            {
                "open" => "open",
                "closed" => "closed",
                _ => "unreadable",
            }
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
    fn open_pr(&mut self, n: u64) -> Step<Option<String>> {
        let Some(prs) = self.s.issues.get(&n).map(|i| i.prs.clone()) else {
            return Ok(None);
        };
        let Some(prs) = prs else {
            if self.s.issues[&n].prs_unreadable {
                return Err(self.fail(format!("ERROR: unreadable pull requests of #{n}"), 1));
            }
            return Err(self.fail("ERROR: gh api graphql failed", 1));
        };
        Ok(prs
            .iter()
            .find(|p| p.get("state").and_then(Value::as_str) == Some("OPEN"))
            .and_then(|p| p.get("url").and_then(Value::as_str))
            .filter(|u| !u.is_empty())
            .map(str::to_string))
    }

    /// The answer work_ok read for the issue.
    fn work(&self, n: u64) -> Value {
        self.s
            .issues
            .get(&n)
            .and_then(|i| i.work.clone())
            .unwrap_or(Value::Null)
    }

    /// The edit record unchanged_since read, the text an approval binds.
    fn text(&self, n: u64) -> Value {
        self.s
            .issues
            .get(&n)
            .and_then(|i| i.edit.clone())
            .map(Value::Object)
            .unwrap_or_else(|| json!({}))
    }

    /// epic_ok: an issue with open sub-issues is a plan, never started. Only
    /// the label that asked for the cycle comes off.
    fn epic_ok(&mut self, n: u64, ask: &str, labels: &[String]) -> bool {
        let w = self.work(n);
        let sum = w.get("sub_issues_summary");
        let count = |k: &str| sum.and_then(|s| s.get(k)).and_then(Value::as_f64);
        let (Some(total), Some(done)) = (count("total"), count("completed")) else {
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
        if !has(labels, ask) {
            return false;
        }
        if self.dry {
            self.log(format!("would take {ask} off #{n}: it is an epic"));
        } else {
            self.act(json!({
                "issue": n, "remove": [ask],
                "fail": Self::label_fail(n, &[ask], &[]),
                "say": format!("Tehas took `{ask}` off this issue and started nothing. It has {open} {word} open, so it is an epic: a plan, not a change. An epic is never started. Apply `tehas:ready` to one of its sub-issues instead."),
            }));
        }
        false
    }

    /// size_ok: with TEHAS_SIZE_MAX_FILES, a body that names more files is
    /// held, its label off and needs-human-attention on.
    fn size_ok(&mut self, n: u64, ask: &str, labels: &[String]) -> Step<bool> {
        let Some(max) = self.c.size_max_files else {
            return Ok(true);
        };
        let w = self.work(n);
        let body = match w.get("body") {
            None | Some(Value::Null) => "",
            Some(Value::String(b)) => b,
            Some(_) => {
                return Err(self.fail(format!("ERROR: the size of #{n} could not be read"), 1))
            }
        };
        let files = text::size_files(body, self.c.size_max_bytes, self.c.size_max_lines) as u64;
        if files <= max {
            return Ok(true);
        }
        self.log(format!("skip #{n}: its body names {files} files, more than TEHAS_SIZE_MAX_FILES ({max}), and one cycle does not make that many changes"));
        if !has(labels, ask) {
            return Ok(false);
        }
        if self.dry {
            self.log(format!("would take {ask} off #{n} and add needs-human-attention: its body names {files} files, more than TEHAS_SIZE_MAX_FILES ({max})"));
        } else {
            self.act(json!({
                "issue": n, "remove": [ask], "add": ["needs-human-attention"],
                "fail": Self::label_fail(n, &[ask], &["needs-human-attention"]),
                "say": format!("Tehas took `{ask}` off this issue, put `needs-human-attention` on it and started nothing. Its body names {files} files, more than the {max} of `TEHAS_SIZE_MAX_FILES`, so one cycle would be asked for too many changes at once. Narrow this issue to the part of it one cycle can make, or split it into issues of that size, and apply `tehas:ready` again to start one of them."),
            }));
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
        let w = self.work(n);
        let linked = match w.get("body") {
            None | Some(Value::Null) => false,
            Some(Value::String(b)) => text::links(b) > 0,
            Some(_) => {
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
        let w = self.work(n);
        if w.is_null() {
            return true;
        }
        let Some(word) = text::promise(&w) else {
            return true;
        };
        self.log(format!("hold #{n}: its text says {word} and links no attachment, so the cycle would be given none; attach the file or write out what it shows, before applying `tehas:ready`"));
        false
    }

    /// park_holds: a fresh label on the very body the last cycle parked on
    /// starts nothing. Said on the issue once a park. The millwright's own
    /// label, applied at `fixer_at` within its grant, is lifted by a fresh
    /// `action=change` comment when the instance gives a cycle the note
    /// (#1130): `lift` then carries that comment for the start.
    fn park_holds(
        &mut self,
        n: u64,
        fixer_at: Option<&str>,
        lift: &mut Option<Value>,
    ) -> Step<bool> {
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
        let Some(p) = park.filter(|p| p.is_object()) else {
            return Ok(false);
        };
        let sum = p.get("body_sha256").and_then(Value::as_str).unwrap_or("");
        let hex = sum.len() == 64 && sum.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if p.get("reason").and_then(Value::as_str) == Some("provider-outage")
            || p.get("retries").and_then(Value::as_f64) != Some(0.0)
            || !hex
        {
            return Ok(false);
        }
        if text::body_sha256(&self.text(n)) != sum {
            return Ok(false);
        }
        // As `tostring` and `read -r` leave it: no reason is "null", and
        // only its first line counts, trimmed of spaces and tabs.
        let reason = match p.get("reason") {
            None => "null".to_string(),
            Some(Value::String(r)) => r.clone(),
            Some(r) => r.to_string(),
        };
        let cap = reason
            .split('\n')
            .next()
            .unwrap_or("")
            .trim_matches([' ', '\t']);
        let cap = if cap.is_empty() { "nothing" } else { cap }.to_string();
        // The way round a park open to the millwright alone: its own guidance
        // comment says the next run is not the run that stopped. The grant was
        // reached before this, and the note reaches a cycle only where the
        // instance gives it images or files. A read that failed proves no note
        // either way: the hold stays, untold, and the next poll asks again.
        if let Some(at) = fixer_at.filter(|_| self.c.work_images || self.c.work_files) {
            let read = self
                .s
                .issues
                .get(&n)
                .and_then(|i| i.comments.as_ref())
                .and_then(|list| crate::apply::start::change_note(list, n, &self.c.millwright, at));
            let Some(found) = read else {
                self.log(format!(
                    "hold #{n}: its last cycle parked on {cap} and the body has not changed since, and the fixer's comments could not be read"
                ));
                return Ok(true);
            };
            if let Some(note) = found {
                *lift = Some(note);
                self.log(format!(
                    "lift #{n}: its last cycle parked on {cap} and the body has not changed since, lifted on the fixer's guidance comment"
                ));
                return Ok(false);
            }
        }
        let told = !p.get("told_at").unwrap_or(&Value::Null).is_null();
        if !told && !self.dry {
            self.act(json!({
                "issue": n, "park_told": true,
                "say": format!("Not started: the last cycle parked on `{cap}` and the issue text has not changed since. Edit the body, or tick Retry in the park comment."),
            }));
        }
        self.log(format!(
            "hold #{n}: its last cycle parked on {cap} and the body has not changed since"
        ));
        Ok(true)
    }

    /// fixer_ok: the fixer's tehas:ready, applied at `at`. The person's
    /// approval it rests on and the count of fixer starts with this one, or
    /// None with the label off and the reason said. With the feed on, an
    /// issue never approved may rest on its author instead (feed_ok).
    fn fixer_ok(&mut self, n: u64, at: &str, labels: &[String]) -> Step<Option<(Value, u64)>> {
        if !self.unchanged_since(
            n,
            at,
            "the fixer applied tehas:ready",
            "the fixer or a person applies it again",
        )? {
            return Ok(None);
        }
        if n != self.c.control && !matches!(self.s.issues.get(&n), Some(i) if i.approval.is_some())
        {
            self.need("approval");
            return Ok(None);
        }
        let sum = text::body_sha256(&self.text(n));
        let never =
            matches!(self.s.issues.get(&n), Some(i) if matches!(i.approval, Some(Ok(None))));
        if self.c.millwright_feed && never && n != self.c.control {
            return self.feed_ok(n, &sum, labels);
        }
        let why = match self.fixer_grant(n, &sum) {
            Ok((base, count)) => {
                let by = base["by"].as_str().unwrap_or("").to_string();
                let since = base["at"].as_str().unwrap_or("").to_string();
                if self.unchanged_since(
                    n,
                    &since,
                    &format!("{by} approved it"),
                    "a person applies tehas:ready",
                )? {
                    return Ok(Some((base, count)));
                }
                format!("its title or text was changed since {by} approved it")
            }
            Err((why, true)) => {
                self.log(format!(
                    "skip #{n}: tehas:ready from the fixer waits: {why}"
                ));
                return Ok(None);
            }
            Err((why, false)) => why,
        };
        self.fixer_refused(n, &why);
        Ok(None)
    }

    /// The fixer's tehas:ready comes off, and the issue says why.
    fn fixer_refused(&mut self, n: u64, why: &str) {
        self.log(format!(
            "skip #{n}: tehas:ready from the fixer starts nothing: {why}"
        ));
        if self.dry {
            self.log(format!("would take tehas:ready off #{n}"));
        } else {
            self.act(json!({
                "issue": n, "remove": ["tehas:ready"],
                "fail": Self::label_fail(n, &["tehas:ready"], &[]),
                "say": format!("Tehas took `tehas:ready` off this issue and started nothing. The fixer applied it, and the fixer may not start this issue: {why}. A person decides now: an account named in `TEHAS_ALLOWED_LABELERS` applies `tehas:ready` to start a cycle."),
            }));
        }
    }

    /// feed_ok: the millwright's tehas:ready on an issue no person approved a
    /// cycle of, under TEHAS_MILLWRIGHT_FEED. A person wrote it, and the text
    /// is still theirs: the start rests on that. Some(base, 1), or None with
    /// the label off and the reason said, or kept when the cap only defers it.
    fn feed_ok(&mut self, n: u64, sum: &str, labels: &[String]) -> Step<Option<(Value, u64)>> {
        let Some(running) = self.s.fed else {
            self.log(format!(
                "skip #{n}: tehas:ready from the fixer waits: the fed cycles running could not be counted"
            ));
            return Ok(None);
        };
        let max = self.c.millwright_max_feeds.max(0) as u64;
        if running + self.fed_picked >= max {
            self.log(format!(
                "wait #{n}: {} fed cycles run, TEHAS_MILLWRIGHT_MAX_FEEDS is {max}",
                running + self.fed_picked
            ));
            return Ok(None);
        }
        let t = self.text(n);
        let author = t
            .pointer("/author/login")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let written = t
            .get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let held = [
            "tehas:improvement",
            "cross-project",
            "needs-human-attention",
            "tehas:millwright-escalated",
        ]
        .into_iter()
        .find(|l| has(labels, l));
        let ev = self.events(n)?;
        let c = self.c;
        let undone = ev
            .iter()
            .find(|e| e.event == "unlabeled" && e.label == "tehas:ready" && c.allowed(&e.actor))
            .map(|e| e.actor.clone());
        let why = if !self.c.allowed(&author) {
            Some(format!(
                "no person approved a cycle of it, and its author {} is not named in `TEHAS_ALLOWED_LABELERS`",
                or_unknown(&author)
            ))
        } else if written.is_empty() {
            Some(
                "no person approved a cycle of it, and when it was written could not be read"
                    .into(),
            )
        } else if let Some(l) = held {
            Some(format!(
                "no person approved a cycle of it, and it carries `{l}`"
            ))
        } else {
            undone.map(|who| format!("{who} took `tehas:ready` off it before"))
        };
        if let Some(why) = why {
            self.fixer_refused(n, &why);
            return Ok(None);
        }
        if !self.unchanged_since(
            n,
            &written,
            &format!("{author} wrote it"),
            "a person applies tehas:ready",
        )? {
            return Ok(None);
        }
        Ok(Some((
            json!({"at": written, "by": author, "how": "feed", "body_sha256": sum}),
            1,
        )))
    }

    /// tehas_fixer_grant: the approval and count, or why not and whether
    /// that is only for now (the store could not answer).
    fn fixer_grant(&self, n: u64, sum: &str) -> Result<(Value, u64), (String, bool)> {
        if n == self.c.control {
            return Err(("it is the control issue".into(), false));
        }
        let record = match self.s.issues.get(&n).and_then(|i| i.approval.clone()) {
            Some(Err(())) => {
                return Err((
                    "its approvals on the state store could not be read".into(),
                    true,
                ))
            }
            Some(Ok(r)) => r,
            None => None,
        };
        let none = || {
            (
                "no person approved an earlier cycle of it, so the fixer may not start one"
                    .to_string(),
                false,
            )
        };
        let record = record.ok_or_else(none)?;
        let base = fixer_base(&record)
            .filter(|b| self.c.allowed(b["by"].as_str().unwrap_or("")))
            .ok_or_else(none)?;
        let by = base["by"].as_str().unwrap_or("").to_string();
        if sum != base["body_sha256"].as_str().unwrap_or("") {
            return Err((format!("its text is not the text {by} approved"), false));
        }
        let Some(had) = fixer_starts(&record) else {
            return Err((
                "its approval record does not say how many cycles the fixer started".into(),
                false,
            ));
        };
        if had as i64 >= self.c.fixer_max_retries {
            return Err((
                format!(
                    "the fixer has started {had} cycles on it since {by} approved it, the limit"
                ),
                false,
            ));
        }
        Ok((base, had + 1))
    }

    /// low_gate: whether an issue of the lowest kind may start now.
    fn low_gate(&mut self, n: u64) -> bool {
        if self.c.low_gate.is_empty() {
            return true;
        }
        if !self.gate_asked {
            self.gate_asked = true;
            let Some(g) = self.s.gate.clone() else {
                self.need("gate");
                return false;
            };
            let mut marker = true;
            let mut held = false;
            if g.held == "other" {
                let home = self.c.home.clone();
                self.log(format!(
                    "WARN: {home}/state/low-gate.held is not a regular file: no hold is read from it or written to it"
                ));
                marker = false;
            } else if g.held == "file" {
                held = true;
            }
            self.gate_answer = g.answer;
            if g.answer != 0 {
                if !held || self.dry {
                    self.log(format!(
                        "low issues wait: TEHAS_LOW_GATE answered {}",
                        g.answer
                    ));
                }
                if !self.dry && marker && !held {
                    self.act(json!({"gate_held": true}));
                }
            } else if held {
                self.log("low issues may start: TEHAS_LOW_GATE answered 0");
                if !self.dry {
                    self.act(json!({"gate_held": false}));
                }
            }
        }
        if self.gate_answer != 0 {
            let a = self.gate_answer;
            self.log(format!(
                "wait #{n}: the low-priority gate is closed (exit {a})"
            ));
            return false;
        }
        true
    }

    /// 6 and 7: the candidates in their order, cut to the room.
    fn candidates(&mut self, room: Room) -> Step {
        let Room {
            want,
            want_all,
            want_refresh,
            ..
        } = room;
        let ready = self.ready()?;
        let mut urgent = Vec::new();
        for i in ready.iter().filter(|i| has(&i.labels, "tehas:urgent")) {
            let Some(n) = i.number else { continue };
            if want <= 0 {
                break;
            }
            if self.plan_open(n) || self.running(n) {
                continue;
            }
            let ev = self.events(n)?;
            let (who, _) = label_actor(&ev, "tehas:urgent");
            if self.c.allowed(&who) {
                urgent.push(n);
            } else {
                self.deny("ignore", n, "tehas:urgent", &who);
            }
        }
        let mut next = Vec::new();
        for i in ready.iter().filter(|i| has(&i.labels, "tehas:next")) {
            let Some(n) = i.number else { continue };
            if want <= 0 {
                break;
            }
            if self.plan_open(n) || self.running(n) {
                continue;
            }
            if self.order_label(n, "tehas:next")? {
                next.push(n);
            }
        }
        let mut order: Vec<(char, u64)> = Vec::new();
        let numbers: Vec<u64> = ready.iter().filter_map(|i| i.number).collect();
        order.extend(
            numbers
                .iter()
                .filter(|n| urgent.contains(n))
                .map(|n| ('u', *n)),
        );
        order.extend(self.s.refresh_requests.iter().map(|n| ('r', *n)));
        order.extend(
            numbers
                .iter()
                .filter(|n| !urgent.contains(n) && next.contains(n))
                .map(|n| ('n', *n)),
        );
        order.extend(
            numbers
                .iter()
                .filter(|n| !urgent.contains(n) && !next.contains(n))
                .map(|n| ('x', *n)),
        );
        let mut ready_picked = 0;
        let mut refresh_picked = 0;
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
            if kind == 'x' && self.gate_asked && self.gate_answer != 0 {
                let a = self.gate_answer;
                self.log(format!(
                    "wait #{n}: the low-priority gate is closed (exit {a})"
                ));
                continue;
            }
            if ready_picked >= want {
                continue;
            }
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
            let ask = if has(&labels, "tehas:ready") {
                "tehas:ready"
            } else {
                "tehas:blocked"
            };
            let mut fixer = None;
            let mut planned = false;
            let (who, at) = match self.plan_start(n, ask)? {
                Planned::Refused => continue,
                Planned::Go(who, at) => {
                    planned = true;
                    (who, at)
                }
                Planned::No => match self.approval(n, ask)? {
                    Ok(a) => a,
                    Err(refused) => {
                        if ask == "tehas:ready"
                            && !refused.is_empty()
                            && self.c.is_millwright(&refused)
                        {
                            let ev = self.events(n)?;
                            let (_, at) = label_actor(&ev, "tehas:ready");
                            match self.fixer_ok(n, &at, &labels)? {
                                Some(grant) => fixer = Some(grant),
                                None => continue,
                            }
                            (refused, at)
                        } else {
                            if ask == "tehas:ready" && !refused.is_empty() {
                                self.clear_unauthorised(n, "tehas:ready", &refused);
                            }
                            continue;
                        }
                    }
                },
            };
            if has(&labels, "tehas:blocked") && !self.blocked_ok(n)? {
                continue;
            }
            if !self.work_ok(n)? {
                continue;
            }
            if !self.epic_ok(n, ask, &labels) {
                continue;
            }
            if has(&labels, "cross-project") && has(&labels, "tehas:improvement") {
                self.log(format!("skip #{n}: it carries cross-project and tehas:improvement, so its fix needs a change in another repository too, and one cycle here cannot make it"));
                continue;
            }
            if !self.size_ok(n, ask, &labels)? {
                continue;
            }
            if let Some(url) = self.open_pr(n)? {
                self.log(format!(
                    "skip #{n}: pull request {url} is open and closes it"
                ));
                if !self.dry {
                    self.act(json!({
                        "issue": n, "remove": [ask],
                        "fail": Self::label_fail(n, &[ask], &[]),
                        "say": format!("Tehas took `{ask}` off this issue and started nothing: {url} is open and closes it. Merge or close that pull request, then apply `tehas:ready` if work is still needed."),
                    }));
                }
                continue;
            }
            // The start a lift lets through is held back here and built with
            // the note below, so the record says why the hold let it go.
            let mut lift = None;
            if !self.image_ok(n)?
                || !self.image_missing_ok(n)
                || self.park_holds(n, fixer.as_ref().map(|_| at.as_str()), &mut lift)?
            {
                continue;
            }
            if kind == 'x' && !self.low_gate(n) {
                continue;
            }
            let driver = match self.driver(n, &labels, fixer.is_some())? {
                Driver::Wait => continue,
                Driver::Stranger(s) => {
                    self.lean_refused(n, ask, &labels, &s);
                    continue;
                }
                d => d,
            };
            self.d.picked.push(n);
            ready_picked += 1;
            let mut remove = vec![ask.to_string()];
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
                given_how: "label".into(),
                remove,
                fixer: None,
                park_lifted: None,
                extra: Map::new(),
            };
            if let Some((base, count)) = fixer {
                let s = |k: &str| {
                    base.get(k)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                };
                st.verb = "labelled by the fixer".into();
                st.how = if s("how") == "feed" {
                    self.fed_picked += 1;
                    format!(
                        "`tehas:ready` label as the fixer, on the issue {} wrote at {}",
                        s("by"),
                        s("at")
                    )
                } else {
                    format!(
                        "`tehas:ready` label as the fixer, on {}'s approval of {}",
                        s("by"),
                        s("at")
                    )
                };
                st.given_how = "millwright".into();
                if let Some(note) = lift.take() {
                    st.park_lifted = Some(note);
                }
                st.fixer = Some((base, count));
            }
            if planned {
                st.verb = "planned".into();
                st.how = "`tehas:plan` label, on a plan with no open points".into();
                st.extra
                    .insert("approved_label".into(), json!("tehas:plan"));
            }
            driver.bind(&mut st.extra);
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
        self.act(json!({"units": first}));
        self.act(json!({"index_gate": running}));
    }

    /// The start of a cycle as an action: what start_cycles records and
    /// changes.
    fn act_start(&mut self, n: u64, st: Start) {
        // What the approval binds: the sums and links of the text this poll
        // read, which the record is written from.
        let t = self.text(n);
        let body = t.get("body").and_then(Value::as_str).unwrap_or("");
        let mut a = json!({
            "start": n, "by": st.starter, "how": st.how, "at": st.given_at,
            "given": st.given_how, "remove": st.remove, "verb": st.verb,
            // The line apply logs as the start begins, which the shadow reads.
            "log": format!("start #{n} ({} by {})", st.verb, st.starter),
            "text": !t.as_object().is_none_or(|o| o.is_empty()),
            "body_sha256": text::body_sha256(&t), "title_sha256": text::title_sha256(&t),
            "links": text::link_list(body),
        });
        if let Some((base, count)) = st.fixer {
            a["rests_on"] = base;
            a["fixer_starts"] = json!(count);
        }
        if let Some(note) = st.park_lifted {
            a["park_lifted"] = note;
        }
        for (k, v) in st.extra {
            a[k] = v;
        }
        self.act(a);
    }
}

/// tehas_fixer_base: the person's approval a record rests on, {at, by, how,
/// body_sha256}, when it reads as one.
fn fixer_base(record: &Value) -> Option<Value> {
    let how = record.get("how").and_then(Value::as_str);
    let base = if matches!(how, Some("fixer" | "refresh")) {
        record.get("rests_on")?
    } else {
        record
    };
    let s = |k: &str| base.get(k).and_then(Value::as_str);
    let at = s("at")?;
    let by = s("by")?;
    let how = s("how")?;
    let sum = s("body_sha256")?;
    let ok = !at.is_empty()
        && at
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'T' | b':' | b'.' | b'Z' | b'-'))
        && !by.is_empty()
        && by.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && matches!(how, "label" | "retry-tick" | "outage-retry")
        && sum.len() == 64
        && sum.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    ok.then(|| json!({"at": at, "by": by, "how": how, "body_sha256": sum}))
}

/// tehas_fixer_starts: the fixer starts a record counts, at most six digits.
fn fixer_starts(record: &Value) -> Option<u64> {
    let whole = |v: &Value, min: f64| v.as_f64().filter(|f| *f >= min && f.fract() == 0.0);
    let n = match record.get("how").and_then(Value::as_str) {
        Some("fixer") => whole(record.get("fixer_starts")?, 1.0)?,
        Some("refresh") => match record.get("fixer_starts") {
            None | Some(Value::Null) => 0.0,
            Some(v) => whole(v, 0.0)?,
        },
        _ => 0.0,
    };
    (n < 1_000_000.0).then_some(n as u64)
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
