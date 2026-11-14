//! What a snapshot reads of this host: the files under $TEHAS_HOME the poll
//! reads, and the locks its cycles and planning turns hold. A lock is asked
//! as the bash asks it, with a lock taken and let go at once.

use super::run::run;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tehas_core::file::{read_regular, ReadError, READ_BOUND};

/// Whether a lock file is held by someone, or cannot be told apart from one
/// that is: anything but no lock and a lock the probe took counts as held, as
/// `flock -n` finds. The probe is core's, the one every reader of a lock asks.
///
/// A lock of a whole file is held by descriptor and one process holds only one
/// of those per file, so the probe's own open takes a lock the same process
/// already holds: a caller that holds the lock itself is told it is free. The
/// callers ask about a lock another process took.
pub fn held(path: &Path) -> bool {
    matches!(
        tehas_core::lock::probe(path),
        tehas_core::lock::Probe::Held | tehas_core::lock::Probe::Unknown(_)
    )
}

/// The issues whose cycle holds its lock.
pub fn running(home: &Path) -> Vec<u64> {
    let mut out: Vec<u64> = fs::read_dir(home.join("state/cycles"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let n: u64 = name.strip_suffix(".lock")?.parse().ok()?;
            held(&e.path()).then_some(n)
        })
        .collect();
    out.sort_unstable();
    out
}

/// What a planning turn on record as running is: in flight (its lock held or
/// its unit active), and whether it left its exit status.
pub fn turn(home: &Path, n: u64, systemctl: &str) -> (bool, bool) {
    let st = home.join(format!("state/plan-{n}.json"));
    let rc = st.with_extension("json.rc").is_file();
    if rc {
        return (false, true);
    }
    if held(&st.with_extension("json.lock")) {
        return (true, false);
    }
    let r = run(
        systemctl,
        &[
            "--user".into(),
            "show".into(),
            "-p".into(),
            "ActiveState".into(),
            "--value".into(),
            format!("tehas-plan-{n}.service"),
        ],
        Duration::from_secs(30),
    );
    let active = r.ok()
        && matches!(
            r.out.trim(),
            "active" | "activating" | "deactivating" | "reloading"
        );
    (active, false)
}

/// tehas_pause_read: the flag, "unreadable" for one that is no flag, null for
/// none.
pub fn pause(home: &Path) -> Value {
    let p = home.join("state/pause.json");
    if !p.exists() {
        return Value::Null;
    }
    let v: Value = fs::read_to_string(&p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    if v.get("at").is_some_and(Value::is_string) && v.get("reason").is_some_and(Value::is_string) {
        v
    } else {
        json!("unreadable")
    }
}

/// How long the poll holds starts on a push fault before it tries again. Part
/// 3 replaces the wait with a re-probe of the push check; until then a fault
/// that is never cleared costs one start every so often rather than one for
/// every ready issue.
pub const PUSH_FAULT_HOLD: i64 = 600;

/// The record of a sandbox that cannot push: `state/pause.json`'s neighbour,
/// written by the cycle supervisor when its push check is refused (Part 1) and
/// taken off by the start whose check passes. Null for none, "unreadable" for
/// a file that is there and says nothing usable, and {at, login, line} for the
/// fault on record. Its `line` is what git said, masked by the writer; a file
/// that does not parse still counts as a fault, with the line left empty.
pub fn push_fault(home: &Path) -> Value {
    let p = home.join("state/push-fault.json");
    let v: Value = match read_regular(&p, READ_BOUND) {
        Ok((bytes, _)) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        Err(ReadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Value::Null,
        Err(_) => Value::Null,
    };
    if v.get("at").is_some_and(Value::is_string)
        && v.get("login").is_some_and(Value::is_string)
        && v.get("line").is_some_and(Value::is_string)
    {
        v
    } else {
        json!("unreadable")
    }
}

/// The fault's line as the poll says it: git's words when the file has them,
/// the one line of the host's own for a fault that says nothing.
pub fn push_fault_line(fault: &Value) -> String {
    let line = match fault {
        Value::Object(_) => fault.get("line").and_then(Value::as_str).unwrap_or(""),
        _ => "",
    };
    if line.is_empty() {
        "the sandbox cannot push to origin".into()
    } else {
        line.to_string()
    }
}

/// Whether a push fault still holds the poll: one that is on record and no
/// older than [`PUSH_FAULT_HOLD`], or one the poll cannot read the age of.
/// A fault with no readable time holds rather than starts: nothing about the
/// host has been proven, and the wait is short either way.
pub fn push_fault_holds(fault: &Value, now: i64) -> bool {
    match fault {
        Value::Null => false,
        Value::Object(_) => match fault.get("at").and_then(Value::as_str).and_then(epoch) {
            Some(at) => now - at < PUSH_FAULT_HOLD,
            None => true,
        },
        _ => true,
    }
}

/// Seconds since the epoch of an ISO 8601 UTC time as fromdateiso8601 reads
/// it: YYYY-MM-DDTHH:MM:SSZ. Core's strict reader, kept as the name the other
/// dispatch modules call.
pub(crate) fn epoch(s: &str) -> Option<i64> {
    tehas_core::time::from_iso(s)
}

/// The limit, as `flags limit read` reads it: the one reader in tehas_core.
pub fn limit(home: &Path, now: i64) -> Value {
    tehas_core::limit::read(&home.join("state/limit.json"), now).value()
}

/// The first bytes of a file as text, empty when it cannot be read.
pub fn head(path: &Path, bytes: usize) -> String {
    fs::read(path)
        .map(|b| String::from_utf8_lossy(&b[..b.len().min(bytes)]).into_owned())
        .unwrap_or_default()
}

/// The bound of a refresh request: one is a few hundred bytes.
pub const REQUEST_BOUND: u64 = 4096;

/// refresh_requests: the issues with a request, lowest first, with what each
/// file holds (null when it cannot be read as JSON), and a warning for each
/// file over the bound, which is not read.
pub fn requests(home: &Path) -> (Vec<(u64, Value)>, Vec<String>) {
    let dir = home.join("state/refresh");
    let mut warn = Vec::new();
    let mut out: Vec<(u64, Value)> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let n = name.strip_suffix(".json")?;
            if n.is_empty()
                || n.len() > 10
                || n.starts_with('0')
                || !n.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            let v = match read_regular(&e.path(), REQUEST_BOUND) {
                Ok((bytes, _)) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
                Err(ReadError::NotRegular) => return None,
                Err(ReadError::TooLarge) => {
                    warn.push(format!(
                        "WARN: {} is over {REQUEST_BOUND} bytes, not read",
                        e.path().display()
                    ));
                    Value::Null
                }
                Err(_) => Value::Null,
            };
            Some((n.parse().ok()?, v))
        })
        .collect();
    out.sort_by_key(|(n, _)| *n);
    warn.sort();
    (out, warn)
}

/// Where the record of a plan's start is kept. No container is given the state
/// directory, and only the dispatcher writes here.
pub fn plan_start_file(home: &Path, n: u64) -> PathBuf {
    home.join(format!("state/plan-start/{n}.json"))
}

/// The record of a plan's start of an issue, None when it has none, and Null
/// when one is there and cannot be read: not a file, or not JSON.
pub fn plan_start(home: &Path, n: u64) -> Option<Value> {
    match read_regular(&plan_start_file(home, n), 4096) {
        Ok((bytes, _)) => Some(serde_json::from_slice(&bytes).unwrap_or(Value::Null)),
        Err(ReadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => Some(Value::Null),
    }
}

/// The why of the last live/dispatch.json, empty when it cannot be read. The
/// key is the core report's, as is the word: a reader that kept its own copy
/// of either would go on reading a field the poll no longer writes. It is read
/// as the store publishes it, a regular file under the shared bound, so a link
/// or a file that grew past it counts as no report.
pub fn last_why(home: &Path) -> String {
    read_regular(&home.join("live/dispatch.json"), READ_BOUND)
        .ok()
        .and_then(|(bytes, _)| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| {
            v.get(tehas_core::hold::KEY_WHY)
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// This host's name.
pub fn hostname() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn secs(var: &str, default: u64) -> Duration {
    let n = std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| (1..=99).contains(n))
        .unwrap_or(default);
    Duration::from_secs(n)
}

/// What `console slice ok` and `slice room` read of the host, in the shape
/// the snapshot's slice takes: systemctl show, docker info, the cgroup
/// files, nproc and the running cycles' cid files.
pub fn slice(home: &Path, running: &[u64]) -> Value {
    let wait = secs("TEHAS_SLICE_WAIT", 10);
    let root = std::env::var("TEHAS_CGROUP_ROOT").unwrap_or_else(|_| "/sys/fs/cgroup".into());
    let mut v = json!({"root": root});
    let a = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let show = run(
        "systemctl",
        &a(&[
            "show",
            "tehas.slice",
            "-p",
            "FragmentPath",
            "-p",
            "ActiveState",
            "-p",
            "MemoryMax",
        ]),
        wait,
    );
    if show.ok() && !show.out.trim().is_empty() {
        let mut m = serde_json::Map::new();
        for line in show.out.lines() {
            if let Some((k, val)) = line.split_once('=') {
                if matches!(k, "FragmentPath" | "ActiveState" | "MemoryMax") {
                    m.insert(k.into(), json!(val));
                }
            }
        }
        v["show"] = Value::Object(m);
    }
    let docker = run(
        "docker",
        &a(&["info", "--format", "{{.CgroupDriver}} {{.CgroupVersion}}"]),
        wait,
    );
    if docker.ok() {
        v["docker"] = json!(docker.out.lines().next().unwrap_or(""));
    }
    let root = Path::new(&root);
    if let Ok(t) = fs::read_to_string(root.join("cgroup.controllers")) {
        v["controllers"] = json!(t.lines().next().unwrap_or(""));
    }
    let nproc = run("nproc", &[], wait);
    if nproc.ok() {
        v["cpus"] = json!(nproc.out.trim());
    }
    let dir = root.join("tehas.slice");
    if let Ok(entries) = fs::read_dir(&dir) {
        let mut scopes = serde_json::Map::new();
        for e in entries.flatten() {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if !name.ends_with(".scope") {
                continue;
            }
            let read = |f: &str| fs::read_to_string(e.path().join(f)).ok();
            scopes.insert(
                name,
                json!({"stat": read("memory.stat"), "low": read("memory.low")}),
            );
        }
        v["scopes"] = Value::Object(scopes);
        if let Ok(p) = fs::read_to_string(dir.join("memory.pressure")) {
            v["pressure"] = json!(p);
        }
    }
    v["cids"] = Value::Array(
        running
            .iter()
            .filter_map(|n| fs::read_to_string(home.join(format!("state/cycles/{n}.cid"))).ok())
            .map(|t| json!(t.trim()))
            .collect(),
    );
    v
}

/// The free kilobytes of the disk of $TEHAS_HOME and of Docker's, the smaller;
/// null when either cannot be read.
pub fn disk(home: &Path) -> Value {
    let wait = secs("TEHAS_DISK_WAIT", 10);
    let kb = tehas_core::disk::free_kb(&home.to_string_lossy(), |program, args| {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let r = run(program, &args, wait);
        r.ok().then_some(r.out)
    });
    kb.map_or(Value::Null, |kb| json!(kb))
}

/// What sweep_workspaces finds: each issue with something left on this host,
/// in the order the bash's globs find them, whether any of it (or its lock)
/// was used within <days> days or cannot be dated, and whether its lock is a
/// lock file.
pub fn leftovers(home: &Path, work_dir: &str, user_home: &Path, now: i64, days: u64) -> Value {
    let work = Path::new(work_dir);
    let (works, base) = (
        work.parent().unwrap_or(Path::new("/")),
        work.file_name().and_then(|b| b.to_str()).unwrap_or(""),
    );
    let marked = works.join(".tehas-workspaces").is_file();
    let number = |s: &str| -> Option<u64> {
        (!s.is_empty()
            && s.len() <= 10
            && !s.starts_with('0')
            && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
    };
    let names = |dir: &Path| -> Vec<String> {
        let mut n: Vec<String> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        n.sort();
        n
    };
    let mut list: Vec<u64> = Vec::new();
    if marked {
        let prefix = format!("{base}-");
        for name in names(works) {
            let Some(rest) = name.strip_prefix(&prefix) else {
                continue;
            };
            let n = [".before-retry", ".images", ".state"]
                .iter()
                .find_map(|x| rest.strip_suffix(x))
                .unwrap_or(rest);
            if let Some(n) = number(n) {
                list.push(n);
            }
        }
    }
    let runs = user_home.join(".pi/agent/ensemble-runs");
    for kind in ["issue-", "plan-"] {
        for name in names(&runs) {
            if let Some(n) = name.strip_prefix(kind).and_then(number) {
                list.push(n);
            }
        }
    }
    for name in names(&home.join("live")) {
        if let Some(n) = name
            .strip_prefix("console-")
            .and_then(|r| r.strip_suffix(".txt"))
            .and_then(number)
        {
            list.push(n);
        }
    }
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for n in list {
        if seen.contains(&n) {
            continue;
        }
        seen.push(n);
        let lock = home.join(format!("state/cycles/{n}.lock"));
        // A lock that is not there (a dangling link
        // too) is free; one that is there is a plain file and no link.
        let lock_ok =
            !lock.exists() || fs::symlink_metadata(&lock).is_ok_and(|m| m.file_type().is_file());
        let mut files = vec![lock];
        if marked {
            for x in ["", ".before-retry", ".images", ".state"] {
                files.push(works.join(format!("{base}-{n}{x}")));
            }
        }
        files.push(runs.join(format!("issue-{n}")));
        files.push(runs.join(format!("plan-{n}")));
        files.push(home.join(format!("live/console-{n}.txt")));
        // A name used within <days> days keeps the issue, and so does one
        // that cannot be dated.
        let young = files.iter().any(|f| match fs::symlink_metadata(f) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => true,
            Ok(m) => m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_none_or(|t| (t.as_secs() as i64) > now - days as i64 * 86_400),
        });
        out.push(json!({"issue": n, "young": young, "lock_ok": lock_ok}));
    }
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use tehas_core::testdir::TestDir;

    /// A home of this case's own, with its `state`.
    fn home(name: &str) -> TestDir {
        let d = TestDir::new(&format!("dispatch-host-{name}"));
        fs::create_dir_all(d.join("state")).unwrap();
        d
    }

    #[test]
    fn a_push_fault_is_read_whole_and_one_that_says_nothing_counts_as_present() {
        let h = home("push-fault");
        let file = h.join("state/push-fault.json");
        assert_eq!(push_fault(&h), Value::Null, "no file");
        fs::write(
            &file,
            r#"{"at":"2026-10-31T00:00:00Z","login":"tehas-bot","line":"remote: Permission to acme/widget denied to tehas-bot"}"#,
        )
        .unwrap();
        let f = push_fault(&h);
        assert_eq!(f["login"], json!("tehas-bot"));
        assert_eq!(
            push_fault_line(&f),
            "remote: Permission to acme/widget denied to tehas-bot"
        );
        // A fault of the poll's own shape but empty of words is still a fault.
        fs::write(
            &file,
            r#"{"at":"2026-10-31T00:00:00Z","login":"tehas-bot","line":""}"#,
        )
        .unwrap();
        assert_eq!(
            push_fault_line(&push_fault(&h)),
            "the sandbox cannot push to origin"
        );
        // Anything else the file holds is no fault the poll can name, and is
        // not taken for no fault.
        fs::write(&file, "not json").unwrap();
        assert_eq!(push_fault(&h), json!("unreadable"));
        fs::write(&file, r#"{"login":"tehas-bot"}"#).unwrap();
        assert_eq!(push_fault(&h), json!("unreadable"));
        // A link is no fault, and neither is a file that grew past the bound:
        // both are a fault the poll cannot read.
        fs::remove_file(&file).unwrap();
        let real = h.join("real.json");
        fs::write(
            &real,
            r#"{"at":"2026-10-31T00:00:00Z","login":"tehas-bot","line":"denied"}"#,
        )
        .unwrap();
        std::os::unix::fs::symlink(&real, &file).unwrap();
        assert_eq!(push_fault(&h), json!("unreadable"));
        fs::remove_file(&file).unwrap();
        fs::write(
            &file,
            format!(r#"{{"line":"x"}}{}"#, " ".repeat(READ_BOUND as usize)),
        )
        .unwrap();
        assert_eq!(push_fault(&h), json!("unreadable"));
    }

    #[test]
    fn a_push_fault_holds_until_it_is_ten_minutes_old() {
        let at = "2026-10-31T00:00:00Z";
        let fault = |line: &str| json!({"at": at, "login": "tehas-bot", "line": line});
        let now = epoch(at).unwrap();
        assert!(!push_fault_holds(&Value::Null, now), "no fault");
        assert!(
            push_fault_holds(&fault("denied"), now),
            "a fault of this instant"
        );
        assert!(
            push_fault_holds(&fault("denied"), now + PUSH_FAULT_HOLD - 1),
            "a fault still inside the wait"
        );
        assert!(
            !push_fault_holds(&fault("denied"), now + PUSH_FAULT_HOLD),
            "a fault of the wait's own age starts again"
        );
        assert!(!push_fault_holds(
            &fault("denied"),
            now + PUSH_FAULT_HOLD + 100
        ));
        assert!(
            push_fault_holds(&json!({"at": "nonsense", "login": "", "line": "x"}), now),
            "a fault whose age cannot be read holds"
        );
        assert!(
            push_fault_holds(&json!("unreadable"), now),
            "a fault that cannot be read"
        );
    }

    #[test]
    fn the_last_why_is_read_only_from_a_regular_file_under_the_bound() {
        let h = home("last-why");
        assert_eq!(last_why(&h), "");
        fs::create_dir_all(h.join("live")).unwrap();
        let file = h.join("live/dispatch.json");
        fs::write(&file, r#"{"why":"room"}"#).unwrap();
        assert_eq!(last_why(&h), "room");
        let pad = " ".repeat(READ_BOUND as usize);
        fs::write(&file, format!(r#"{{"why":"room"}}{pad}"#)).unwrap();
        assert_eq!(last_why(&h), "", "a report past the bound is none");
        let real = h.join("real.json");
        fs::write(&real, r#"{"why":"room"}"#).unwrap();
        fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&real, &file).unwrap();
        assert_eq!(last_why(&h), "", "a link is not the poll's report");
    }

    #[test]
    fn a_plans_record_is_there_or_not_and_what_cannot_be_read_counts_for_nothing() {
        let h = home("plan-start");
        assert_eq!(plan_start(&h, 7), None);
        fs::create_dir_all(h.join("state/plan-start")).unwrap();
        assert_eq!(plan_start(&h, 7), None);
        let file = plan_start_file(&h, 7);
        fs::write(&file, r#"{"by":"alice"}"#).unwrap();
        assert_eq!(plan_start(&h, 7), Some(json!({"by": "alice"})));
        assert_eq!(plan_start(&h, 8), None);
        fs::write(&file, "not json").unwrap();
        assert_eq!(plan_start(&h, 7), Some(Value::Null));
        // A link is no record, whatever it points at.
        fs::remove_file(&file).unwrap();
        let there = h.join("elsewhere.json");
        fs::write(&there, r#"{"by":"alice"}"#).unwrap();
        std::os::unix::fs::symlink(&there, &file).unwrap();
        assert_eq!(plan_start(&h, 7), Some(Value::Null));
    }

    #[test]
    fn a_refresh_request_over_the_bound_is_not_read_and_is_named() {
        let h = home("requests");
        let dir = h.join("state/refresh");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("7.json"),
            r#"{"at":"2026-11-01T00:00:00Z","pr":9}"#,
        )
        .unwrap();
        fs::write(dir.join("8.json"), "not json").unwrap();
        let big = format!(r#"{{"at":"{}"}}"#, "x".repeat(REQUEST_BOUND as usize));
        fs::write(dir.join("9.json"), big).unwrap();
        std::os::unix::fs::symlink(dir.join("7.json"), dir.join("10.json")).unwrap();
        fs::write(dir.join("07.json"), "{}").unwrap();
        let (got, warn) = requests(&h);
        assert_eq!(
            got,
            vec![
                (7, json!({"at": "2026-11-01T00:00:00Z", "pr": 9})),
                (8, Value::Null),
                (9, Value::Null),
            ]
        );
        assert_eq!(
            warn,
            vec![format!(
                "WARN: {} is over {REQUEST_BOUND} bytes, not read",
                dir.join("9.json").display()
            )]
        );
        // A request at the bound is read as before.
        let fits = format!(r#"{{"at":"{}"}}"#, "x".repeat(REQUEST_BOUND as usize - 9));
        assert_eq!(fits.len() as u64, REQUEST_BOUND);
        fs::write(dir.join("9.json"), fits).unwrap();
        let (got, warn) = requests(&h);
        assert!(got[2].1["at"].is_string());
        assert!(warn.is_empty());
    }

    // The parser is core's; the row that matters here is one this reader used
    // to take: a second 60 read as a time rather than as none.
    #[test]
    fn times_read_as_fromdateiso8601_reads_them() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2026-10-31T00:00:00Z"), Some(1_793_404_800));
        assert_eq!(epoch("2026-10-31 00:00:00"), None);
        assert_eq!(epoch("2026-10-31T00:00:60Z"), None);
    }

    #[test]
    fn no_lock_and_one_the_probe_took_are_not_held() {
        let h = home("free");
        let lock = h.join("state/plan-7.json.lock");
        assert!(!held(&lock), "no lock file");
        fs::write(&lock, "").unwrap();
        assert!(!held(&lock), "a lock no one holds");
    }

    #[test]
    fn a_directory_at_a_planning_lock_path_is_held() {
        let h = home("directory");
        let lock = h.join("state/plan-7.json.lock");
        fs::create_dir(&lock).unwrap();
        assert!(held(&lock), "a name that is no lock file");
    }

    #[test]
    fn a_link_at_a_planning_lock_path_is_held() {
        let h = home("link");
        let target = h.join("elsewhere.lock");
        fs::write(&target, "").unwrap();
        let lock = h.join("state/plan-7.json.lock");
        std::os::unix::fs::symlink(&target, &lock).unwrap();
        assert!(held(&lock), "a lock asked by a name of its own");
    }

    #[test]
    fn a_lock_held_by_another_process_is_held() {
        let h = home("held");
        let lock = h.join("state/plan-7.json.lock");
        fs::write(&lock, "").unwrap();
        // The holder is a process of its own, so the probe's open is not the one
        // that already holds the lock. It prints when it holds, waits for a line,
        // and gives the lock up when the pipe closes.
        let mut child = std::process::Command::new("flock")
            .arg("-x")
            .arg(&lock)
            .arg("-c")
            .arg("echo held; read _")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut said = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut said)
            .unwrap();
        assert_eq!(said.trim(), "held", "the holder never took the lock");
        assert!(held(&lock), "a lock another process holds");
        drop(child.stdin.take().unwrap());
        child.wait().unwrap();
        assert!(!held(&lock), "the lock was given back");
    }
}
