//! `tehas-dispatch poll`: one poll of the queue, as libexec/dispatch runs it
//! once its settings are loaded. The dispatcher's lock, the checks before any
//! call, the snapshot and its decision, what the poll publishes, the actions
//! in the order of the lines between them, and the end every poll that held
//! the lock has: tidy, its cost and the analytics.
//!
//! The gates of a start (systemd's answer, the code index, the checkout), the
//! journal settled first and the setting that chooses this poll are the next
//! parts of #254: nothing runs this yet.
//!
//! The bash poll's end is an EXIT trap, which a SIGTERM runs too. This one
//! runs when the poll returns: a poll systemd stops records no cost and runs
//! no tidy, and the next poll's tidy does that one's.

use crate::apply::start::Host;
use crate::apply::Writer;
use crate::decide::{iso, iso_now};
use crate::read::build::{fill, snapshot, Sources};
use crate::read::host::hostname;
use crate::read::run::{read_gh_token, run, Calls};
use crate::{decide, Settings, Snapshot};
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a held dispatcher lock may last before it is said to hang.
const HANG: u64 = 1800;

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_default()
}

fn secs(k: &str, default: u64) -> Duration {
    Duration::from_secs(env(k).parse().unwrap_or(default))
}

/// The directory of this program, libexec: where the bash poll's `$here`
/// finds health, status, tidy and plan.
pub fn here(home: &Path) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| home.join("libexec"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A line of the poll's log, as log() prints it.
fn say(line: &str) {
    println!("{} {line}", iso_now());
}

/// What a command printed, as the bash poll lets it through.
fn pass(lines: &[String]) {
    for l in lines {
        println!("{l}");
    }
}

/// The poll's own CPU and its children's, as bash's `times` prints them.
fn times() -> String {
    let ticks: f64 = run("getconf", &["CLK_TCK".into()], Duration::from_secs(5))
        .out
        .trim()
        .parse()
        .unwrap_or(100.0);
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let rest: Vec<&str> = stat
        .rsplit_once(')')
        .map(|(_, r)| r.split_whitespace().collect())
        .unwrap_or_default();
    let t = |i: usize| -> String {
        let s = rest
            .get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
            / ticks;
        format!("{}m{:.3}s", (s / 60.0).floor(), s % 60.0)
    };
    format!("{} {}\n{} {}\n", t(11), t(12), t(13), t(14))
}

/// The records of an older tehas, which install.sh imports: one still here
/// means the store has no row for it, and no row is not "none".
fn old_record(home: &Path) -> Option<String> {
    let named = |dir: &str, ok: &dyn Fn(&str) -> bool| -> Option<String> {
        let mut names: Vec<String> = std::fs::read_dir(home.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| ok(n))
            .collect();
        names.sort();
        names.first().map(|n| format!("{dir}/{n}"))
    };
    let starts = |n: &str, p: &str| {
        n.strip_prefix(p)
            .and_then(|r| r.chars().next())
            .is_some_and(|c| ('1'..='9').contains(&c))
    };
    let cycle_ends = [
        ".approved.json",
        ".park",
        ".by",
        ".requeued",
        ".taken",
        ".rukas",
        ".quiet",
        ".image",
        ".settings",
        ".branch",
        ".history.jsonl",
    ];
    named("state/cycles", &|n| {
        starts(n, "") && cycle_ends.iter().any(|e| n.ends_with(e))
    })
    .or_else(|| {
        named("state", &|n| {
            starts(n, "park-") && (n.ends_with(".id") || n.ends_with(".json"))
        })
    })
    .or_else(|| named("state", &|n| starts(n, "plan-") && n.ends_with(".json")))
    .or_else(|| {
        named("state", &|n| {
            starts(n, "plan-")
                && [".json.brief", ".json.answer", ".json.turn", ".json.keep"]
                    .iter()
                    .any(|e| n.ends_with(e))
        })
    })
    .or_else(|| {
        named("state/cycles", &|n| {
            starts(n, "plan-") && n.ends_with(".history.jsonl")
        })
    })
    .or_else(|| {
        named("state", &|n| {
            starts(n, "plan-") && n.ends_with(".history.jsonl")
        })
    })
}

/// tehas_store_ready: the store installed and its file a plain one.
fn store_ready(home: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let bin = std::fs::metadata(home.join("libexec/store"))
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    let db = std::fs::symlink_metadata(home.join("state/tehas.db"))
        .is_ok_and(|m| m.file_type().is_file());
    bin && db
}

/// The writes of a poll, with the host's commands as the settings name them.
fn writer(home: &Path, settings: &Settings, bot: &str, poll_at: i64, before: i64) -> Writer {
    let libexec = home.join("libexec");
    let opt = |k: &str| Some(env(k)).filter(|v| !v.is_empty());
    let here = here(home);
    let or =
        |k: &str, d: &str| opt(k).unwrap_or_else(|| here.join(d).to_string_lossy().into_owned());
    let analytics = libexec.join("analytics");
    let analytics = (env("TEHAS_ANALYTICS") == "on" && analytics.is_file())
        .then(|| analytics.to_string_lossy().into_owned());
    Writer {
        repo: settings.repo.clone(),
        home: home.to_path_buf(),
        gh: "gh".into(),
        docker: "docker".into(),
        calls: Calls::idle(
            secs("tehas_store_bound", 20),
            libexec.join("store").to_string_lossy().into_owned(),
        ),
        poll_at,
        // The poll asked for its row, and said nothing of it: both are the
        // state of one poll, made here and not by its parts.
        row_not_begun: false,
        before,
        bound: Duration::from_secs(30),
        // The fixed retry and poll steps of apply, at the values the bash
        // dispatcher used: not settings of the instance.
        store_retry: Duration::from_secs(1),
        host: Host {
            name: hostname(),
            comments: env("TEHAS_WORK_COMMENTS") == "1",
            allowed: env("TEHAS_ALLOWED_LABELERS")
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            millwright: crate::settings::account(&env("TEHAS_MILLWRIGHT_LOGIN")).to_string(),
            bot: if bot.is_empty() {
                env("TEHAS_BOT_LOGIN")
            } else {
                bot.to_string()
            },
            systemctl: "systemctl".into(),
            start_cmd: opt("TEHAS_CYCLE_START_CMD"),
            stop_cmd: opt("TEHAS_CYCLE_STOP_CMD"),
            start_wait: secs("TEHAS_CYCLE_START_WAIT", 30),
            status_cmd: or("TEHAS_STATUS_CMD", "status"),
            health_cmd: or("TEHAS_HEALTH_CMD", "health"),
            tidy_cmd: or("TEHAS_TIDY_CMD", "tidy"),
            analytics,
            // tehas_analytics_wait: 1 to 999 seconds, else 5.
            analytics_wait: Duration::from_secs(
                Some(env("TEHAS_ANALYTICS_WAIT"))
                    .filter(|v| {
                        (1..=3).contains(&v.len())
                            && !v.starts_with('0')
                            && v.bytes().all(|b| b.is_ascii_digit())
                    })
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5),
            ),
            work_dir: env("TEHAS_WORK_DIR"),
            user_home: env("HOME"),
            done_wait: secs("TEHAS_CYCLE_DONE_WAIT", 330),
            image: env("PI_ENSEMBLE_IMAGE"),
            plan_cmd: or("TEHAS_PLAN_CMD", "plan"),
            plan_start_cmd: opt("TEHAS_PLAN_START_CMD"),
            pin_cmd: Some(here.join("tehas-cycle"))
                .filter(|p| opt("TEHAS_PLAN_CMD").is_none() && p.is_file())
                .map(|p| p.to_string_lossy().into_owned()),
            plan_timeout: env("TEHAS_PLAN_TIMEOUT").parse().unwrap_or(3600),
            plan_max_turns: env("TEHAS_PLAN_MAX_TURNS").parse().unwrap_or(20),
            systemd_run: "systemd-run".into(),
            lock_poll: Duration::from_millis(500),
            launch_poll: Duration::from_millis(500),
            checkout: env("TEHAS_WORK_REPO"),
            branch: settings.branch.clone(),
            git: "git".into(),
            index_cmd: or("TEHAS_INDEX_CMD", "index"),
            index_start_cmd: opt("TEHAS_INDEX_START_CMD"),
            // timeout's 0 is no bound.
            fetch_wait: match env("TEHAS_FETCH_WAIT").parse::<u64>() {
                Ok(0) => Duration::from_secs(u64::from(u32::MAX)),
                Ok(n) => Duration::from_secs(n),
                Err(_) => Duration::from_secs(120),
            },
        },
        ended: None,
        spend: None,
        status_due: false,
        changed: false,
        gh_writes: 0,
        starts: 0,
        fetched: false,
    }
}

/// What a poll's actions could change: the reason its end runs the status
/// again and takes a sample. A poll that starts, stops, writes or turns
/// nothing leaves both to the next poll that does, and idle polls are the
/// common ones.
#[derive(Debug, Default)]
struct Changed {
    /// A write to GitHub, a start, a stop, or a poll that ended inside a step
    /// it could not finish: the queue itself changed, so the status's files
    /// are out of date.
    queue: bool,
}

/// The poll's end, run however it ended once it held the lock: tidy once
/// past the bot's checks, then, unless dry, its cost and the analytics.
pub struct End<'a> {
    home: &'a Path,
    dry: bool,
    tidy: bool,
    started: f64,
    gh: u32,
    starts: u32,
    changed: Changed,
}

impl End<'_> {
    /// tidy, as this end sends it: with `--dry-run` only of a dry poll, which
    /// is how the bash's trap sends it too.
    fn runs_tidy(&self, w: &mut Writer, printed: &mut Vec<String>) {
        let args: Vec<String> = if self.dry {
            vec!["--dry-run".into()]
        } else {
            Vec::new()
        };
        w.status_of(&w.host.tidy_cmd.clone(), &args, printed);
    }

    /// An end of a poll that got past the bot's login, for a test of the host
    /// commands below: `dry` is all any of them reads, so the rest are an idle
    /// poll's, and a test of these three runs neither the cost nor the status
    /// of a real end, which need the home and the tally.
    #[cfg(test)]
    pub fn host_of(dry: bool) -> End<'static> {
        End {
            home: Path::new(""),
            dry,
            tidy: true,
            started: 0.0,
            gh: 0,
            starts: 0,
            changed: Changed::default(),
        }
    }

    /// The three host commands of a poll, each the way the poll sends it: tidy
    /// as this end sends it, the status, and health as `demand_health` sends
    /// it. Reached through `End`, so the arguments are the ones a poll gives.
    #[cfg(test)]
    pub fn runs_host_commands(&self, w: &mut Writer, printed: &mut Vec<String>) {
        self.runs_tidy(w, printed);
        w.status(printed);
        w.health(printed, &mut Vec::new());
    }

    fn run(&self, w: &mut Writer) {
        if self.tidy {
            let mut printed = Vec::new();
            self.runs_tidy(w, &mut printed);
            pass(&printed);
        }
        if self.dry {
            return;
        }
        // The status and the sample below follow work, not the clock, and keep
        // their order: the sample reads the status's files. A poll that
        // started, stopped, wrote or turned nothing leaves both to the next
        // poll that does - the poll's cost is still recorded, as health and
        // status read it whatever the last poll did. The prune keeps its
        // schedule, being the one step of the end that bounds what the others
        // read.
        if self.changed.queue {
            let mut printed = Vec::new();
            w.status(&mut printed);
            pass(&printed);
        }
        let state = self.home.join("state");
        let tally = state.join("poll-gh.tally");
        let cpu = state.join("poll.times");
        let _ = std::fs::write(&tally, "\n".repeat(self.gh as usize));
        let _ = std::fs::write(&cpu, times());
        let started = format!("{:.6}", self.started);
        let (t, c) = (
            tally.to_string_lossy().into_owned(),
            cpu.to_string_lossy().into_owned(),
        );
        let writes = state
            .join("github-writes.log")
            .to_string_lossy()
            .into_owned();
        let n = self.starts.to_string();
        if let Err(f) = w.calls.store(&[
            "poll-record",
            "--started",
            &started,
            "--times",
            &c,
            "--tally",
            &t,
            "--writes-log",
            &writes,
            "--starts",
            &n,
        ]) {
            say(&format!(
                "WARN: the cost of the poll was not recorded ({})",
                f.get("word").and_then(Value::as_str).unwrap_or("")
            ));
        }
        let mut log = Vec::new();
        // The sample of the queue as the status above wrote it; the rule and
        // the prune's exemption are stated at the status.
        if self.changed.queue {
            w.analytics(&["sample".into()], &mut log);
        }
        w.analytics(&["prune".into()], &mut log);
        for l in &log {
            say(l);
        }
    }
}

/// The poll's GitHub token, taken by the first gh call of the poll and kept in
/// memory for the rest of it. One `Arc` and one `Mutex` of its own, as the
/// poll's two sets of calls - its reads and its writes - are two `Calls` and
/// both ask this for the token before they start one.
#[derive(Default)]
struct PollToken(Arc<Mutex<Option<String>>>);

/// How long the poll waits for its one run of the token program, as its other
/// gh calls wait for their bound.
const TOKEN_BOUND: Duration = Duration::from_secs(30);

impl PollToken {
    /// The token of the host's GitHub App: the one already taken, or the first
    /// run of `$TEHAS_HOME/libexec/github-token`, which answers the token and
    /// nothing else. Only when `TEHAS_GITHUB_APP_ID` names an app. A run that
    /// could not be had leaves nothing taken, so the poll's gh calls go on as
    /// they did before and each pays for a token of its own. The token is held
    /// here and nowhere else: never logged, never written, never recorded.
    fn get(&self, home: &Path) -> Option<String> {
        {
            let held = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if held.is_some() {
                return held.clone();
            }
        }
        let token = read_gh_token(home, TOKEN_BOUND);
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // Another call of the poll asked while this one ran: its token is the
        // poll's, and no second one is kept.
        if held.is_none() {
            *held = token.clone();
        }
        held.clone()
    }
}

/// One poll. The exit status the bash poll would end with.
pub fn run_poll(home: &Path, settings: &Settings, dry: bool) -> i32 {
    let state = home.join("state");
    for d in ["state/cycles", "logs", "live"] {
        let _ = std::fs::create_dir_all(home.join(d));
    }
    let Ok(lock) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.join("dispatch.lock"))
    else {
        say("ERROR: could not open the dispatcher's lock");
        return 1;
    };
    // A cycle that ended asked for a poll, and this is it.
    if !dry {
        let _ = std::fs::remove_file(state.join("dispatch.again"));
    }
    let (taken, stuck) = (state.join("dispatch.taken"), state.join("dispatch.stuck"));
    if lock.try_lock().is_err() {
        if dry {
            say("another poll holds the dispatcher's lock");
        }
        let first = |p: &PathBuf| {
            std::fs::read(p)
                .map(|b| {
                    String::from_utf8_lossy(&b[..b.len().min(16)])
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string()
                })
                .unwrap_or_default()
        };
        let at = first(&taken);
        let digits = !at.is_empty() && at.bytes().all(|b| b.is_ascii_digit());
        if let Some(t) = at.parse::<u64>().ok().filter(|_| digits) {
            if at.len() <= 12 && now().saturating_sub(t) > HANG && (dry || first(&stuck) != at) {
                say(&format!("WARN: the dispatcher's lock has been held since {}, more than {HANG}s: a poll hangs, and no poll runs until it ends", iso(t)));
                if !dry && std::fs::write(&stuck, format!("{at}\n")).is_err() {
                    say(&format!("WARN: could not write {}", stuck.display()));
                }
            }
        }
        return 0;
    }
    if std::fs::write(&taken, format!("{}\n", now())).is_err() {
        say(&format!("WARN: could not write {}", taken.display()));
    }
    if !store_ready(home) {
        say("ERROR: the state store is not installed (run install.sh)");
        return 78;
    }
    if let Some(old) = old_record(home) {
        say(&format!(
            "ERROR: records of an older tehas at {}/{old}; run install.sh again",
            home.display()
        ));
        return 78;
    }
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let mut w = writer(home, settings, "", 0, now() as i64);
    // The poll's GitHub token, kept in memory from before its first call: the
    // first gh of the poll asks the host's token program once, and every gh
    // process after it is given the token as TEHAS_GH_TOKEN, so the shim of a
    // call runs no libexec/github-token of its own: its reads, its writes, and
    // the commands it starts at its end - tidy, the status and health, all
    // through status_of - carry it in their environment too. With no app named
    // nothing is asked and nothing is given, and a token that could not be had
    // leaves the calls as they were: each pays for one. The token is never
    // logged, never written to a file and never put in the arguments of
    // anything.
    let token = PollToken::default();
    w.calls.gh_token = token.get(home);
    let mut end = End {
        home,
        dry,
        tidy: false,
        started,
        gh: 0,
        starts: 0,
        changed: Changed::default(),
    };
    let rc = body(home, settings, dry, &token, &mut w, &mut end);
    end.gh += w.calls.gh_count + w.gh_writes;
    end.starts = w.starts;
    end.changed.queue = w.changed;
    end.run(&mut w);
    drop(lock);
    rc
}

fn body(
    home: &Path,
    settings: &Settings,
    dry: bool,
    token: &PollToken,
    w: &mut Writer,
    end: &mut End,
) -> i32 {
    // The journal first, before GitHub is read for the queue: what a poll
    // that died between two writes left is settled from fresh reads. One that
    // holds nothing open has nothing to settle, which is the case of every
    // poll that did not die: the read is the poll's first call, and an idle
    // poll pays it for no reason. A journal the store would not answer is not
    // an empty one, so its WARN is said and the settling waits for the next
    // poll.
    if !dry {
        let mut lines = Vec::new();
        let open = w.journal_open(&mut lines);
        for l in &lines {
            say(l);
        }
        if let Some(open) = open.filter(|open| !open.is_empty()) {
            let mut lines = Vec::new();
            w.settle_actions(w.before, open, &mut lines);
            for l in &lines {
                say(l);
            }
        }
    }
    if !dry {
        let mut printed = Vec::new();
        w.status(&mut printed);
        pass(&printed);
    }
    let now = w.before;
    let mut src = Sources {
        home,
        calls: Calls::idle(
            secs("tehas_store_bound", 20),
            home.join("libexec/store").to_string_lossy().into_owned(),
        ),
        systemctl: "systemctl".into(),
        now,
        models: crate::read::models::Models::read(),
        endpoint: (String::new(), String::new()),
        gate: true,
    };
    // The same token for the poll's reads as for its writes: what the shim of
    // every gh call would ask for anyway, asked once by the poll.
    src.calls.gh_token = w.calls.gh_token.clone().or_else(|| token.get(home));
    let snap = snapshot(&mut src, settings);
    end.gh = src.calls.gh_count;
    let mut snap = match snap {
        Ok(s) => s,
        Err(f) => {
            say(&f.0);
            return 1;
        }
    };
    let mut d = decide(&Snapshot::parse(&snap), settings, dry);
    while fill(&src, &mut snap, &d.needs) {
        d = decide(&Snapshot::parse(&snap), settings, dry);
    }
    end.gh = src.calls.gh_count;
    end.tidy = d.tidy;
    w.host.bot = d.bot.clone();
    demand_health(home, w, &d);
    // The decision's lines, the actions between them, and the two steps the
    // bash takes where they fall: a poll an action ends logs nothing after
    // it, as the bash exits there.
    let mut next = 0;
    let mut i = 0;
    let (mut swept, mut reported) = (false, false);
    let lines_to = |upto: usize, next: &mut usize| {
        let upto = upto.min(d.log.len()).max(*next);
        for l in &d.log[*next..upto] {
            say(l);
        }
        *next = upto;
    };
    loop {
        if let Some((_, l)) = d.swept_at.filter(|(a, _)| *a == i && !swept) {
            lines_to(l, &mut next);
            sweep_locks(w, dry);
            swept = true;
        }
        if let Some((_, l)) = d.reported_at.filter(|(a, _)| *a == i && !reported) {
            lines_to(l, &mut next);
            report(home, w, &d, &snap);
            reported = true;
        }
        if i >= d.actions.len() {
            break;
        }
        let at = d.actions[i]
            .get("_line")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        lines_to(at, &mut next);
        let run = d.actions[i..]
            .iter()
            .take_while(|a| a.get("start").is_some())
            .count()
            .max(1);
        let done = w.apply(&d.actions[i..i + run]);
        i += run;
        for l in &done.log {
            say(l);
        }
        pass(&done.printed);
        for a in &done.left {
            say(&format!("WARN: no part of the poll applies the action {a}"));
        }
        if let Some(rc) = done.exit {
            return rc;
        }
    }
    lines_to(d.log.len(), &mut next);
    d.exit
}

/// live/demand.json and health, as the poll's start writes them. A dry run
/// writes neither.
fn demand_health(home: &Path, w: &mut Writer, d: &crate::Decision) {
    if let Some(demand) = &d.demand {
        let file = home.join("live/demand.json");
        if demand.is_null() {
            let _ = std::fs::remove_file(&file);
        } else {
            let mut v = demand.clone();
            v["time"] = json!(iso_now());
            let new = home.join("live/demand.json.new");
            if std::fs::write(&new, format!("{v}\n"))
                .and_then(|()| std::fs::rename(&new, &file))
                .is_err()
            {
                let _ = std::fs::remove_file(&new);
                say(&format!("WARN: could not write {}", file.display()));
            }
        }
        // health, whose gh calls are given the poll's token as the poll's own
        // are: TEHAS_HEALTH_CMD is a test's name for it, as TEHAS_STATUS_CMD
        // is, and nothing of the token travels in the name.
        let mut printed = Vec::new();
        let mut log = Vec::new();
        let rc = w.health(&mut printed, &mut log);
        pass(&printed);
        for l in &log {
            say(l);
        }
        if rc != Some(0) {
            say("WARN: the health file could not be written");
        }
    }
}

/// live/dispatch.json through the store, with its time, and the status when
/// its reason changed. A dry run has no report.
fn report(home: &Path, w: &mut Writer, d: &crate::Decision, snap: &Value) {
    let Some(report) = &d.report else {
        return;
    };
    let mut v = report.clone();
    v["time"] = json!(iso_now());
    let file = home.join("live/dispatch.json");
    let made = w.scratch("dispatch").and_then(|p| {
        let mut f: File = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&p)
            .ok()?;
        f.write_all(format!("{v}\n").as_bytes()).ok().map(|()| p)
    });
    match made {
        None => say(&format!("WARN: could not write {}", file.display())),
        Some(tmp) => {
            let t = tmp.to_string_lossy().into_owned();
            if let Err(f) = w.store_try(3, &["publish", "--dispatch", &t]) {
                say(&format!(
                    "WARN: could not write {} ({}: {})",
                    file.display(),
                    f.get("word").and_then(Value::as_str).unwrap_or(""),
                    f.get("line").and_then(Value::as_str).unwrap_or("")
                ));
            }
            let _ = std::fs::remove_file(&tmp);
        }
    }
    let before = snap.get("last_why").and_then(Value::as_str).unwrap_or("");
    if report.get("why").and_then(Value::as_str).unwrap_or("") != before {
        let mut printed = Vec::new();
        w.status(&mut printed);
        pass(&printed);
    }
}

/// The lock files of issues nothing has used for 30 days go, three a poll.
fn sweep_locks(w: &mut Writer, dry: bool) {
    let args: &[&str] = if dry {
        &["sweep-locks", "--dry-run"]
    } else {
        &["sweep-locks"]
    };
    let did = if dry { "would remove" } else { "removed" };
    match w.calls.store(args) {
        Ok(v) => {
            for n in v
                .get("removed")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                say(&format!("{did} the lock of #{n}: unused for 30 days"));
            }
        }
        Err(f) => say(&format!(
            "WARN: the unused cycle locks were not swept ({}): {}",
            f.get("word").and_then(Value::as_str).unwrap_or(""),
            f.get("line").and_then(Value::as_str).unwrap_or("")
        )),
    }
}
