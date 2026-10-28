//! The live view: cycle-heartbeat, which a running cycle calls every ten
//! seconds, and publish, the one writer of the files under live/ that describe
//! the cycles and the factory's health.
//!
//! publish holds state/publish.lock while it reads the rows and writes the
//! files, so two publishers never interleave. Each file is written to a hidden
//! temp name of its own (live/.<name>.<pid>.<n>.new, which the docs server
//! does not serve) and renamed into place; a failed write or rename fails the
//! call (B1). It writes, in this order:
//!
//!   dispatch.json      the file given with --dispatch, as given
//!   cycles/<n>.json    each running cycle and the last that finished,
//!                      pretty-printed as tehas work's jq -n wrote it
//!   endings.json       the ten last ends, oldest first, one line
//!   running.json       {"cycles": [...], "last": ...}, one line
//!   health.json        the file given with --health, as given; health.down
//!                      is made before a "fail" report is renamed into place,
//!                      and removed after any other is
//!
//! and then removes each live/cycles/<number>.json that it did not write.
//! It reads the rows and writes files; it writes no row.

use crate::lock;
use crate::verbs::{self, j, line, list, obj, refuse, Args, Fail, Kind, Q};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Heartbeat(Beat),
    Publish {
        dispatch: Option<PathBuf>,
        health: Option<PathBuf>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Beat {
    pub issue: i64,
    pub attempt: Option<i64>,
    pub running: bool,
    pub status: String,
    pub step: Option<String>,
    pub text: Option<String>,
}

/// A running heartbeat older than this, whose lock is free, is a lost cycle.
pub const LOST_AFTER: i64 = 120;
/// The heartbeats kept per issue.
pub const KEEP: i64 = 100;
/// How long publish waits for state/publish.lock. `db::lock_wait` is where the
/// wait is read, and it is the only place the tests' short pair is applied here.
/// The dependency between the two modules runs this way alone: `live` calls into
/// `db`, and `db` names the three installed waits itself rather than reading back
/// here. `tests::the_installed_bounds_are_the_ones_the_spec_states` pins this as
/// a constant, and
/// `tests::publish_waits_the_lock_bound_the_spec_states` is the test that shows
/// `db::lock_wait` answers with it on a thread that never asked for the short
/// pair -- a pin of the constant alone would not notice the wait being given
/// another value.
pub const LOCK_WAIT: Duration = Duration::from_secs(5);
/// The largest --dispatch or --health file read.
const REPORT_BOUND: u64 = 1 << 20;
const TEXT_MAX: usize = 300;
/// How many ends endings.json holds.
const ENDINGS: usize = 10;

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "cycle-heartbeat" => heartbeat_args(rest),
        "publish" => Args::parse(
            "publish",
            rest,
            &[("--dispatch", Kind::Value), ("--health", Kind::Value)],
            false,
        )
        .and_then(|a| {
            Ok(Call::Publish {
                dispatch: a.path("--dispatch")?,
                health: a.path("--health")?,
            })
        }),
        _ => return None,
    })
}

fn heartbeat_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "cycle-heartbeat",
        rest,
        &[
            ("--attempt", Kind::Value),
            ("--running", Kind::Switch),
            ("--ended", Kind::Switch),
            ("--status", Kind::Value),
            ("--step", Kind::Value),
            ("--text", Kind::Value),
        ],
        true,
    )?;
    let running = match (a.has("--running"), a.has("--ended")) {
        (true, false) => true,
        (false, true) => false,
        _ => return Err(a.usage("--running", "or --ended, one of them")),
    };
    let mut c = verbs::Cleaned::default();
    Ok(Call::Heartbeat(Beat {
        issue: a.issue()?,
        attempt: a.strict("--attempt", "a number", verbs::number)?,
        running,
        status: c
            .word("--status", Some(a.need("--status")?), verbs::word_char, 40)
            .unwrap_or_else(|| "unknown".to_string()),
        step: c.word("--step", a.value("--step"), verbs::word_char, 40),
        text: a.value("--text").and_then(text),
    }))
}

/// The notice as the page may show it: no control characters, at most 300
/// characters. Empty is None.
pub fn text(given: &str) -> Option<String> {
    let v: String = given
        .chars()
        .filter(|c| !c.is_control())
        .take(TEXT_MAX)
        .collect();
    (!v.is_empty()).then_some(v)
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Heartbeat(b) => heartbeat(home, b, now),
        Call::Publish { dispatch, health } => {
            publish(home, dispatch.as_deref(), health.as_deref(), now)
        }
    }
}

fn heartbeat(home: &Path, b: &Beat, now: i64) -> Result<String, Fail> {
    let issue = b.issue;
    verbs::write(home, |c| {
        let row = crate::cycle::row(c, issue)?
            .ok_or_else(|| refuse("nocycle", issue, "no cycle on record"))?;
        if let Some(a) = b.attempt {
            if a != row.attempt {
                return Err(refuse(
                    "attempt",
                    issue,
                    &format!("the cycle is attempt {}", row.attempt),
                ));
            }
        }
        if row.ended_at.is_some() {
            return Err(refuse("ended", issue, "the cycle has ended"));
        }
        c.execute(
            "INSERT OR REPLACE INTO heartbeats (issue, at, running, step, status, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![issue, now, b.running, b.step, b.status, b.text],
        )
        .q()?;
        c.execute(
            "DELETE FROM heartbeats WHERE issue = ?1 AND at NOT IN
               (SELECT at FROM heartbeats WHERE issue = ?1 ORDER BY at DESC LIMIT ?2)",
            params![issue, KEEP],
        )
        .q()?;
        Ok(line("cycle-heartbeat", &[("at", j(now))]))
    })
}

/// A cycle as the page shows it.
#[derive(Debug, Clone)]
struct View {
    issue: i64,
    running: bool,
    status: String,
    step: String,
    text: String,
    started_at: i64,
    updated_at: i64,
}

impl View {
    fn pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("issue", j(self.issue)),
            ("running", j(self.running)),
            ("status", j(self.status.as_str())),
            ("step", j(self.step.as_str())),
            ("statusText", j(self.text.as_str())),
            ("startedAt", j(self.started_at)),
            ("updatedAt", j(self.updated_at)),
            ("console", j(format!("console-{}.txt", self.issue))),
        ]
    }

    fn ending(&self) -> Ending {
        Ending {
            issue: self.issue,
            status: self.status.clone(),
            step: Some(self.step.clone()),
            started_at: self.started_at,
            ended_at: self.updated_at,
        }
    }
}

struct Ending {
    issue: i64,
    status: String,
    step: Option<String>,
    started_at: i64,
    ended_at: i64,
}

impl Ending {
    fn json(&self) -> String {
        obj(&[
            ("issue", j(self.issue)),
            ("status", j(self.status.as_str())),
            ("step", j(self.step.as_deref())),
            ("startedAt", j(self.started_at)),
            ("updatedAt", j(self.ended_at)),
        ])
    }
}

/// The object as `jq -n` prints it: one key to a line, two spaces in.
fn pretty(pairs: &[(&str, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("  {}: {v}", j(*k)))
        .collect();
    format!("{{\n{}\n}}\n", body.join(",\n"))
}

/// What publish read: the cycles to show and the ends.
struct Read {
    running: Vec<View>,
    last: Option<View>,
    endings: Vec<Ending>,
}

fn read(home: &Path, c: &Connection, now: i64) -> Result<Read, Fail> {
    let mut q = c
        .prepare(
            "SELECT h.issue, h.running, h.status, h.step, h.text, c.started_at, h.at
             FROM heartbeats h JOIN cycles c ON c.issue = h.issue
             WHERE h.at = (SELECT max(at) FROM heartbeats WHERE issue = h.issue)
               AND h.at >= c.started_at",
        )
        .q()?;
    let rows: Vec<View> = q
        .query_map([], |r| {
            Ok(View {
                issue: r.get(0)?,
                running: r.get(1)?,
                status: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                step: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                text: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                started_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })
        .q()?
        .collect::<Result<_, _>>()
        .q()?;
    let mut running = Vec::new();
    let mut ended = Vec::new();
    let mut lost = Vec::new();
    for mut v in rows {
        if !v.running {
            ended.push(v);
            continue;
        }
        // Quiet for two minutes and no one holds its lock: the cycle died
        // without saying so. Its time is when it became lost, so the view
        // does not move on every publish.
        if v.updated_at < now - LOST_AFTER && lock::probe(home, v.issue, false).rc == 0 {
            v.running = false;
            v.status = "lost".to_string();
            v.updated_at += LOST_AFTER;
            lost.push(v.clone());
            ended.push(v);
            continue;
        }
        running.push(v);
    }
    running.sort_by_key(|v| (v.started_at, v.issue));
    let last = ended.into_iter().max_by_key(|v| (v.updated_at, v.issue));
    let mut q = c
        .prepare(
            "SELECT issue, status, step, started_at, ended_at FROM endings
             ORDER BY ended_at DESC, rowid DESC LIMIT ?1",
        )
        .q()?;
    let mut endings: Vec<Ending> = q
        .query_map([ENDINGS as i64], |r| {
            Ok(Ending {
                issue: r.get(0)?,
                status: r.get(1)?,
                step: r.get(2)?,
                started_at: r.get(3)?,
                ended_at: r.get(4)?,
            })
        })
        .q()?
        .collect::<Result<_, _>>()
        .q()?;
    let known: BTreeSet<(i64, i64)> = endings.iter().map(|e| (e.issue, e.started_at)).collect();
    endings.extend(
        lost.iter()
            .filter(|v| !known.contains(&(v.issue, v.started_at)))
            .map(View::ending),
    );
    // Read newest first; a stable sort then keeps ends of the same second
    // oldest first, as they were recorded.
    endings.reverse();
    endings.sort_by_key(|e| e.ended_at);
    let cut = endings.len().saturating_sub(ENDINGS);
    endings.drain(..cut);
    Ok(Read {
        running,
        last,
        endings,
    })
}

/// A report given with --dispatch or --health: a JSON object, as its bytes.
fn report(path: &Path, flag: &str) -> Result<(Vec<u8>, Value), Fail> {
    let unreadable = |why: String| (1, format!("unreadable {flag}: {why}"));
    let (bytes, _) =
        verbs::read_file(path, REPORT_BOUND).map_err(|e| unreadable(format!("{e:?}")))?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| unreadable(e.to_string()))?;
    if !v.is_object() {
        return Err(unreadable("not a JSON object".to_string()));
    }
    Ok((bytes, v))
}

fn write_failed(name: &str, e: std::io::Error) -> Fail {
    (1, format!("write live/{name}: {e}"))
}

/// Writes `bytes` to a hidden temp file in `dir` and renames it to `name`.
fn put(dir: &Path, name: &str, bytes: &[u8], n: &mut u32) -> Result<(), Fail> {
    let base = Path::new(name)
        .file_name()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let to = dir.join(name);
    let parent = to.parent().unwrap_or(dir);
    *n += 1;
    let tmp = parent.join(format!(".{base}.{}.{n}.new", std::process::id()));
    let done = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&tmp)?;
        f.set_permissions(fs::Permissions::from_mode(0o644))?;
        f.write_all(bytes)?;
        drop(f);
        fs::rename(&tmp, &to)
    })();
    done.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        write_failed(name, e)
    })
}

/// state/publish.lock, held until the returned file is dropped. None when
/// there is no state/: the store is missing too, and the read says so.
fn take_lock(home: &Path) -> Result<Option<File>, Fail> {
    let path = home.join("state").join("publish.lock");
    let f = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err((1, format!("lock state/publish.lock: {e}"))),
    };
    let start = Instant::now();
    // Read on every pass of the retry loop: the bound publish waits is the one
    // `db::lock_wait` answers with, which is where the tests' short pair is
    // applied.
    let bound = crate::db::lock_wait();
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(Some(f)),
            Err(TryLockError::WouldBlock) if start.elapsed() < bound => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TryLockError::WouldBlock) => {
                return Err((1, "busy: state/publish.lock is held".to_string()))
            }
            Err(TryLockError::Error(e)) => {
                return Err((1, format!("lock state/publish.lock: {e}")))
            }
        }
    }
}

fn mkdir(dir: &Path) -> Result<(), Fail> {
    match fs::create_dir(dir) {
        Ok(()) => fs::set_permissions(dir, fs::Permissions::from_mode(0o755))
            .map_err(|e| (1, format!("mkdir {}: {e}", dir.display()))),
        Err(e) if e.kind() == ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
        Err(e) => Err((1, format!("mkdir {}: {e}", dir.display()))),
    }
}

/// `<number>.json`, the name of a cycle's view.
fn cycle_file(name: &str) -> Option<i64> {
    let n = name.strip_suffix(".json")?;
    if n.is_empty() || n.len() > 10 || n.starts_with('0') || !n.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    n.parse().ok()
}

fn publish(
    home: &Path,
    dispatch: Option<&Path>,
    health: Option<&Path>,
    now: i64,
) -> Result<String, Fail> {
    let dispatch = dispatch.map(|p| report(p, "--dispatch")).transpose()?;
    let health = health
        .map(|p| {
            let (bytes, v) = report(p, "--health")?;
            match v.get("status").and_then(Value::as_str) {
                Some(s) => Ok((bytes, s == "fail")),
                None => Err((1, "unreadable --health: no status".to_string())),
            }
        })
        .transpose()?;
    let _lock = take_lock(home)?;
    let got = verbs::look(home, |c| read(home, c, now))?;
    let live = home.join("live");
    mkdir(&live)?;
    mkdir(&live.join("cycles"))?;
    let mut n = 0;
    let mut written = Vec::new();
    if let Some((bytes, _)) = &dispatch {
        put(&live, "dispatch.json", bytes, &mut n)?;
        written.push(j("dispatch.json"));
    }
    let mut keep = BTreeSet::new();
    for v in got.running.iter().chain(got.last.iter()) {
        let name = format!("cycles/{}.json", v.issue);
        put(&live, &name, pretty(&v.pairs()).as_bytes(), &mut n)?;
        keep.insert(v.issue);
        written.push(j(name));
    }
    let endings: Vec<String> = got.endings.iter().map(Ending::json).collect();
    put(
        &live,
        "endings.json",
        format!("{}\n", list(&endings)).as_bytes(),
        &mut n,
    )?;
    written.push(j("endings.json"));
    let cycles: Vec<String> = got.running.iter().map(|v| obj(&v.pairs())).collect();
    let last = got
        .last
        .as_ref()
        .map_or("null".to_string(), |v| obj(&v.pairs()));
    let view = obj(&[("cycles", list(&cycles)), ("last", last)]);
    put(
        &live,
        "running.json",
        format!("{view}\n").as_bytes(),
        &mut n,
    )?;
    written.push(j("running.json"));
    let mut removed = Vec::new();
    let entries = fs::read_dir(live.join("cycles")).map_err(|e| write_failed("cycles", e))?;
    for e in entries {
        let e = e.map_err(|e| write_failed("cycles", e))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(issue) = cycle_file(&name) else {
            continue;
        };
        if keep.contains(&issue) {
            continue;
        }
        match fs::remove_file(e.path()) {
            Ok(()) => removed.push(j(issue)),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(write_failed(&format!("cycles/{name}"), e)),
        }
    }
    let mut down = Value::Null;
    if let Some((bytes, fail)) = &health {
        let marker = live.join("health.down");
        if *fail {
            File::create(&marker).map_err(|e| write_failed("health.down", e))?;
        }
        put(&live, "health.json", bytes, &mut n)?;
        written.push(j("health.json"));
        if !*fail {
            match fs::remove_file(&marker) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(write_failed("health.down", e)),
            }
        }
        down = Value::Bool(*fail);
    }
    Ok(line(
        "publish",
        &[
            ("written", list(&written)),
            ("removed", list(&removed)),
            ("down", down.to_string()),
        ],
    ))
}
