//! What a snapshot reads of this host: the files under $TEHAS_HOME the poll
//! reads, and the locks its cycles and planning turns hold. A lock is asked
//! as the bash asks it, with a lock taken and let go at once.

use super::run::run;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::Path;
use std::time::Duration;

/// Whether a lock file is held by someone: it is there, and cannot be taken.
pub fn held(path: &Path) -> bool {
    // A lock file that is there and cannot be opened cannot be taken either,
    // as flock -n finds.
    let Ok(f) = File::open(path) else {
        return path.exists();
    };
    match f.try_lock() {
        Ok(()) => {
            let _ = f.unlock();
            false
        }
        Err(_) => true,
    }
}

/// tehas_cycles_running: the issues whose cycle holds its lock.
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

/// Seconds since the epoch of an ISO 8601 UTC time as fromdateiso8601 reads
/// it: YYYY-MM-DDTHH:MM:SSZ.
pub(crate) fn epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (
        n(0..4)?,
        n(5..7)?,
        n(8..10)?,
        n(11..13)?,
        n(14..16)?,
        n(17..19)?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// tehas_limit_read.
pub fn limit(home: &Path, now: i64) -> Value {
    let p = home.join("state/limit.json");
    if !p.exists() && p.symlink_metadata().is_err() {
        return json!({"state": "none"});
    }
    let v: Value = fs::read_to_string(&p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let cycles = v.get("cycles").and_then(Value::as_f64);
    let ok = v.get("v") == Some(&json!(1))
        && cycles.is_some_and(|c| c >= 0.0 && c.fract() == 0.0)
        && v.get("plan").is_some_and(Value::is_boolean)
        && v.get("until").is_none_or(|u| u.is_null() || u.is_string())
        && v.get("reason").is_none_or(|r| r.is_null() || r.is_string());
    if !ok {
        return json!({"state": "unreadable"});
    }
    let until = v.get("until").and_then(Value::as_str);
    let u = match until {
        Some(t) => match epoch(t) {
            Some(e) => Some(e),
            None => return json!({"state": "unreadable"}),
        },
        None => None,
    };
    let mut out = json!({"state": if u.is_some_and(|u| u <= now) { "expired" } else { "ok" },
        "cycles": v["cycles"], "plan": v["plan"]});
    if let Some(t) = until {
        out["until"] = json!(t);
    }
    let reason: String = v
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(200)
        .collect();
    if !reason.is_empty() {
        out["reason"] = json!(reason);
    }
    out
}

/// The first bytes of a file as text, empty when it cannot be read.
pub fn head(path: &Path, bytes: usize) -> String {
    fs::read(path)
        .map(|b| String::from_utf8_lossy(&b[..b.len().min(bytes)]).into_owned())
        .unwrap_or_default()
}

/// refresh_requests: the issues with a request, lowest first, with what each
/// file holds (null when it cannot be read as JSON).
pub fn requests(home: &Path) -> Vec<(u64, Value)> {
    let dir = home.join("state/refresh");
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
            let meta = fs::symlink_metadata(e.path()).ok()?;
            if !meta.is_file() {
                return None;
            }
            let v = fs::read_to_string(e.path())
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(Value::Null);
            Some((n.parse().ok()?, v))
        })
        .collect();
    out.sort_by_key(|(n, _)| *n);
    out
}

/// The why of the last live/dispatch.json, empty when it cannot be read.
pub fn last_why(home: &Path) -> String {
    fs::read_to_string(home.join("live/dispatch.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("why").and_then(Value::as_str).map(str::to_string))
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

/// What tehas_slice_ok and tehas_slice_room read of the host, in the shape
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

/// tehas_free_kb: the free kilobytes of the disk of $TEHAS_HOME and of
/// Docker's, the smaller; null when either cannot be read.
pub fn disk(home: &Path) -> Value {
    let wait = secs("TEHAS_DISK_WAIT", 10);
    let kb = |p: &str| -> Option<u64> {
        let r = run("df", &["-Pk".into(), p.into()], wait);
        if !r.ok() {
            return None;
        }
        let f = r.out.lines().nth(1)?.split_whitespace().nth(3)?;
        (f == "0"
            || (!f.starts_with('0') && f.len() <= 18 && f.bytes().all(|b| b.is_ascii_digit())))
        .then(|| f.parse().ok())
        .flatten()
    };
    let docker = run(
        "docker",
        &[
            "info".into(),
            "--format".into(),
            "{{.DockerRootDir}}".into(),
        ],
        wait,
    );
    let dir = if docker.ok() && !docker.out.trim().is_empty() {
        docker.out.trim().to_string()
    } else {
        "/var/lib/docker".into()
    };
    match (kb(&home.to_string_lossy()), kb(&dir)) {
        (Some(a), Some(b)) => json!(a.min(b)),
        _ => Value::Null,
    }
}

/// What sweep_workspaces finds: each issue with something left on this host,
/// in the order the bash's globs find them, whether any of it (or its lock)
/// was used within 7 days or cannot be dated, and whether its lock is a lock
/// file.
pub fn leftovers(home: &Path, work_dir: &str, user_home: &Path, now: i64) -> Value {
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
        // tehas_cycle_lock_free: a lock that is not there (a dangling link
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
        // The age rule of tehasbot clean: a name used within 7 days keeps
        // the issue, and so does one that cannot be dated.
        let young = files.iter().any(|f| match fs::symlink_metadata(f) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => true,
            Ok(m) => m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_none_or(|t| (t.as_secs() as i64) > now - 7 * 86_400),
        });
        out.push(json!({"issue": n, "young": young, "lock_ok": lock_ok}));
    }
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_fromdateiso8601_reads_them() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2026-10-31T00:00:00Z"), Some(1_793_404_800));
        assert_eq!(epoch("2026-10-31 00:00:00"), None);
    }
}
