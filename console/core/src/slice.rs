//! tehas.slice, the systemd slice every sandbox container runs in: whether it
//! can be used, the limits of one container and the room it has. The rule is
//! written here once. The dispatcher answers it from a snapshot, the commands
//! that start a container with [`Host`] through `tehas-cycle sandbox`, and the
//! bash through `console slice` (console/src/slice.rs). A fault is never read
//! as "no limit" or "an empty slice": the answer is then "not ok" or "no room".

use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

const MIB: u64 = 1_048_576;
const GIB: u64 = 1_073_741_824;

/// What the host said about the slice. Each field is None when it gave no
/// answer.
#[derive(Clone, Debug, Default)]
pub struct Reading {
    /// `systemctl show tehas.slice`: FragmentPath, ActiveState, MemoryMax.
    pub show: Option<Map<String, Value>>,
    /// `docker info`: "<driver> <version>".
    pub docker: Option<String>,
    /// The first line of `<root>/cgroup.controllers`.
    pub controllers: Option<String>,
    /// nproc.
    pub cpus: Option<String>,
    /// The cgroup root, as messages name it.
    pub root: String,
    /// The scopes under `<root>/tehas.slice`, None when the directory is
    /// missing. Each holds the text of memory.stat and memory.low, None for a
    /// file that cannot be read.
    pub scopes: Option<Vec<Scope>>,
    /// The text of `<root>/tehas.slice/memory.pressure`.
    pub pressure: Option<String>,
    /// The container id each running cycle's cid file holds.
    pub cids: Vec<String>,
}

/// The host calls `usable` makes. It stops at the first check that fails, so
/// `docker` and `cpus` are asked only of a slice that got that far.
pub trait Probe {
    fn show(&self) -> Option<Map<String, Value>>;
    fn docker(&self) -> Option<String>;
    fn controllers(&self) -> Option<String>;
    fn cpus(&self) -> Option<String>;
    fn root(&self) -> &str;
}

impl Probe for Reading {
    fn show(&self) -> Option<Map<String, Value>> {
        self.show.clone()
    }
    fn docker(&self) -> Option<String> {
        self.docker.clone()
    }
    fn controllers(&self) -> Option<String> {
        self.controllers.clone()
    }
    fn cpus(&self) -> Option<String> {
        self.cpus.clone()
    }
    fn root(&self) -> &str {
        &self.root
    }
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub name: String,
    pub stat: Option<String>,
    pub low: Option<String>,
}

/// This host, asked when a check needs it. Each outside call is bounded by
/// TEHAS_SLICE_WAIT, so that a silent one cannot block a command, and
/// TEHAS_CGROUP_ROOT is the tests' seam for /sys/fs/cgroup.
pub struct Host {
    root: String,
}

impl Host {
    pub fn here() -> Host {
        Host { root: root() }
    }
}

fn var_or(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into())
}

pub fn root() -> String {
    var_or("TEHAS_CGROUP_ROOT", "/sys/fs/cgroup")
}

/// What a program printed under `timeout`, None when it failed.
fn answer(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("timeout")
        .arg(var_or("TEHAS_SLICE_WAIT", "10"))
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

impl Probe for Host {
    fn show(&self) -> Option<Map<String, Value>> {
        let out = answer(
            "systemctl",
            &[
                "show",
                "tehas.slice",
                "-p",
                "FragmentPath",
                "-p",
                "ActiveState",
                "-p",
                "MemoryMax",
            ],
        )?;
        if out.trim_end_matches('\n').is_empty() {
            return None;
        }
        Some(shown(&out))
    }

    fn docker(&self) -> Option<String> {
        answer(
            "docker",
            &["info", "--format", "{{.CgroupDriver}} {{.CgroupVersion}}"],
        )
    }

    fn controllers(&self) -> Option<String> {
        let bytes = fs::read(Path::new(&self.root).join("cgroup.controllers")).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        Some(text.lines().next().unwrap_or("").trim().to_string())
    }

    fn cpus(&self) -> Option<String> {
        let out = Command::new("nproc")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .trim_end_matches('\n')
                .to_string()
        })
    }

    fn root(&self) -> &str {
        &self.root
    }
}

/// The three fields of `systemctl show`, each empty when it was not said: an
/// answer without them is a slice with no file, not an answer that was not given.
fn shown(out: &str) -> Map<String, Value> {
    let mut map = Map::new();
    for key in ["FragmentPath", "ActiveState", "MemoryMax"] {
        let prefix = format!("{key}=");
        let value = out.lines().rev().find_map(|l| l.strip_prefix(&prefix));
        map.insert(key.into(), Value::from(value.unwrap_or("")));
    }
    map
}

/// The settings a container's limits are made from.
#[derive(Clone, Debug, Default)]
pub struct Load {
    /// TEHAS_CYCLE_RESERVE as the instance wrote it.
    pub reserve: String,
    /// TEHAS_SLICE_CONTAINERS.
    pub containers: String,
    /// TEHAS_TICKET_CONCURRENCY.
    pub cycles: String,
    /// TEHAS_REFRESH_CONFLICTS.
    pub conflicts: String,
    /// TEHAS_REFRESH_SLOTS.
    pub slots: String,
}

impl Load {
    /// As lib/config.sh leaves them in the environment.
    pub fn from_env() -> Load {
        let var = |k: &str| env::var(k).unwrap_or_default();
        Load {
            reserve: var("TEHAS_CYCLE_RESERVE"),
            containers: var_or("TEHAS_SLICE_CONTAINERS", "4"),
            cycles: var("TEHAS_TICKET_CONCURRENCY"),
            conflicts: var("TEHAS_REFRESH_CONFLICTS"),
            slots: var("TEHAS_REFRESH_SLOTS"),
        }
    }
}

/// A slice that can be used.
#[derive(Clone, Debug, PartialEq)]
pub struct Usable {
    pub max: u64,
    pub active: String,
    pub ceiling_mb: u64,
    /// The host's CPUs.
    pub cpus: u64,
}

/// What `room` answers.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub room: u64,
    pub committed: u64,
    pub max: u64,
    /// room, full, pressure or unreadable; empty when there is room.
    pub why: String,
    /// After unreadable: the file and what was missing in it.
    pub note: String,
    /// full avg60, as the kernel wrote it, when it was read.
    pub pressure: Option<String>,
}

/// A whole number with no leading zero, of at most 18 digits.
pub fn num(s: &str) -> Option<u64> {
    let ok = s == "0"
        || (!s.is_empty()
            && s.len() <= 18
            && s.as_bytes()[0] != b'0'
            && s.bytes().all(|b| b.is_ascii_digit()));
    if ok {
        s.parse().ok()
    } else {
        None
    }
}

/// A Docker size (6m, 2g, a bare number of bytes) in bytes. None for anything
/// else and for zero.
pub fn bytes(s: &str) -> Option<u64> {
    let (n, unit) = match s.char_indices().last() {
        Some((i, c)) if c.is_ascii_alphabetic() => (&s[..i], c),
        _ => (s, 'b'),
    };
    if n.is_empty() || n.len() > 12 || n.starts_with('0') || !n.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let v: u64 = n.parse().ok()?;
    match unit {
        'b' | 'B' => Some(v),
        _ if n.len() > 9 => None,
        'k' | 'K' => Some(v * 1024),
        'm' | 'M' => Some(v * MIB),
        'g' | 'G' => Some(v * GIB),
        _ => None,
    }
}

/// The slice, or the line a person can act on.
pub fn usable(r: &impl Probe, reserve: &str, containers: &str) -> Result<Usable, String> {
    let Some(show) = r.show().filter(|m| !m.is_empty()) else {
        return Err("systemctl gave no answer".into());
    };
    let field = |k: &str| {
        show.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    // A slice that is only a name has no file and no limit. It is never used.
    if field("FragmentPath").is_empty() {
        return Err("no slice file on this host (sudo ./bootstrap.sh slice)".into());
    }
    let Some(max) = num(&field("MemoryMax")) else {
        return Err("tehas.slice has no memory limit (sudo ./bootstrap.sh slice)".into());
    };
    let docker = r.docker().unwrap_or_default();
    let line = docker.lines().next().unwrap_or("").trim();
    let (driver, version) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let version = version.trim_start();
    if driver.is_empty() || version.is_empty() {
        return Err("docker gave no answer".into());
    }
    if (driver, version) != ("systemd", "2") {
        return Err(format!(
            "Docker runs containers with the {driver} cgroup driver on cgroup v{version}, tehas needs systemd on v2"
        ));
    }
    let ctl = format!(" {} ", r.controllers().unwrap_or_default());
    if ["cpu", "memory", "pids"]
        .iter()
        .any(|c| !ctl.contains(&format!(" {c} ")))
    {
        return Err(format!(
            "{}/cgroup.controllers does not list cpu, memory and pids",
            r.root()
        ));
    }
    let Some(reserve_b) = bytes(reserve) else {
        return Err(format!("TEHAS_CYCLE_RESERVE={reserve} is not a size"));
    };
    let n = match num(containers) {
        Some(n) if n > 0 => n,
        _ => {
            return Err(format!(
                "TEHAS_SLICE_CONTAINERS={containers} is not a count"
            ))
        }
    };
    let ceiling = max / n / MIB;
    if reserve_b > ceiling * MIB {
        // The slice is three quarters of the host and a container 1/n of the
        // slice, so the reserve needs a host of 4n/3 times its size.
        let need = reserve_b * 4 * n / 3;
        let need = if need >= GIB {
            format!("{} GiB", (need + 536_870_912) / GIB)
        } else {
            format!("{} MiB", need / MIB + 1)
        };
        return Err(format!(
            "TEHAS_CYCLE_RESERVE={reserve} is more than a container may use here ({ceiling}m, tehas.slice's MemoryMax over TEHAS_SLICE_CONTAINERS={n}); the host needs about {need} of memory for this reserve"
        ));
    }
    let cpus = match r.cpus().as_deref().and_then(num) {
        Some(c) if c > 0 => c,
        _ => return Err("nproc gave no count of the host's CPUs".into()),
    };
    Ok(Usable {
        max,
        active: field("ActiveState"),
        ceiling_mb: ceiling,
        cpus,
    })
}

/// The CPUs a container may use: all but one, never below one, one being left
/// to the host's own services and the model backends.
fn spare(cpus: u64) -> u64 {
    if cpus > 1 {
        cpus - 1
    } else {
        1
    }
}

// One cycle's share of `spare` CPUs in hundredths, never below one CPU: what it
// gets when every cycle side by side is busy. The cycles are counted at most
// TEHAS_SLICE_CONTAINERS, with the refresh lane when it takes conflicts.
fn share(spare: u64, load: &Load) -> Option<u64> {
    let mut cycles = num(&load.cycles).filter(|n| *n > 0)?;
    if load.conflicts == "1" {
        cycles += num(&load.slots).unwrap_or(0);
    }
    let cycles = cycles.min(num(&load.containers).filter(|n| *n > 0)?);
    Some((spare * 100 / cycles).max(100))
}

/// The CPU quota and the pids limit of one container. The quota is a ceiling,
/// not a share that is held: a gate run alone gets the whole host, and the
/// weight splits it when several are busy. The pids limit follows the share,
/// 1024 for each CPU of it, from 2048 to 4096: a container's processes are
/// mostly what its gate runs side by side, and 2048 is what the agents, their
/// tools and git need around it.
pub fn cpus(host: u64, load: &Load) -> Option<(String, u64)> {
    if host == 0 {
        return None;
    }
    let spare = spare(host);
    let pids = share(spare, load)?.div_ceil(100) * 1024;
    Some((format!("{spare}.00"), pids.clamp(2048, 4096)))
}

/// Exactly the fourteen words a container's limits are for docker run. None
/// when one of them cannot be made.
pub fn limits(s: &Usable, load: &Load) -> Option<Vec<String>> {
    if s.ceiling_mb == 0 {
        return None;
    }
    bytes(&load.reserve)?;
    let (quota, pids) = cpus(s.cpus, load)?;
    let memory = format!("{}m", s.ceiling_mb);
    Some(
        [
            "--cgroup-parent",
            "tehas.slice",
            "--memory",
            &memory,
            "--memory-swap",
            &memory,
            "--memory-reservation",
            &load.reserve,
            "--cpus",
            &quota,
            "--pids-limit",
            &pids.to_string(),
            "--cpu-shares",
            "1024",
        ]
        .map(String::from)
        .to_vec(),
    )
}

/// How many runs of the product's gate one container runs at once: one for
/// each two whole CPUs of its share and each 2 GiB of its ceiling, the lower,
/// at least one. The rule pi-rukas applies to a host, applied to the container.
/// Read off the words of [`limits`]; None when they hold no quota or ceiling.
pub fn verify(words: &[String], load: &Load) -> Option<u64> {
    let word = |flag: &str| {
        words
            .windows(2)
            .rfind(|w| w[0] == flag)
            .map(|w| w[1].as_str())
    };
    let quota = word("--cpus")?;
    let quota = num(quota.split('.').next().unwrap_or(""))?;
    let memory = word("--memory")?;
    let memory = num(memory.strip_suffix('m').unwrap_or(memory))?;
    // Each run is sized for the share: the quota is the whole host less one,
    // and every cycle side by side may be running its gate.
    let cpus = share(quota, load).map_or(quota, |s| quota.min(s / 100));
    Some((cpus / 2).min(memory / 2048).max(1))
}

/// What one child of the slice is charged with, in bytes: what it holds that
/// cannot be reclaimed, or its floor, whichever is more. File cache and
/// reclaimable slab can be reclaimed, so they are left out. A floor of "max"
/// reads as one reserve. Err names the file and the key that could not be read.
///
/// A kernel older than the "kernel" line (Ubuntu 22.04's 5.15 has none) would
/// otherwise hold every poll for ever: anon and shmem are counted and slab is
/// not subtracted, so the child counts a little low and its floor in full.
fn charge(dir: &str, s: &Scope, reserve: u64) -> Result<u64, String> {
    let Some(stat) = &s.stat else {
        return Err(format!("{dir}/memory.stat cannot be read"));
    };
    let Some(low) = &s.low else {
        return Err(format!("{dir}/memory.low cannot be read"));
    };
    let (mut anon, mut shmem, mut kernel, mut slab) = (None, None, None, None);
    for line in stat.lines() {
        let line = line.trim();
        let (k, v) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let v = Some(v.trim_start().to_string()).filter(|v| !v.is_empty());
        match k {
            "anon" => anon = v,
            "shmem" => shmem = v,
            "kernel" => kernel = v,
            "slab_reclaimable" => slab = v,
            _ => {}
        }
    }
    let low = low.lines().next().unwrap_or("").trim();
    let low = if low == "max" {
        reserve.to_string()
    } else {
        low.to_string()
    };
    if kernel.is_none() {
        kernel = Some("0".into());
        slab = Some("0".into());
    }
    // A scope the kernel has not filled in yet has no slab line either: it
    // reads as 0, which counts the slab as used, so it holds.
    slab.get_or_insert_with(|| "0".into());
    let mut vals = [0u64; 4];
    for (i, (k, v)) in [
        ("anon", &anon),
        ("shmem", &shmem),
        ("kernel", &kernel),
        ("slab_reclaimable", &slab),
    ]
    .iter()
    .enumerate()
    {
        vals[i] = v
            .as_deref()
            .and_then(num)
            .ok_or_else(|| format!("{dir}/memory.stat has no number for {k}"))?;
    }
    let low = num(&low).ok_or_else(|| format!("{dir}/memory.low is not a number"))?;
    let used = (vals[0] + vals[1] + vals[2]).saturating_sub(vals[3]);
    Ok(used.max(low))
}

/// The reserves that fit in what is not promised, capped so the containers'
/// ceilings never sum to more than the max. Room by use alone lets in cycles
/// at their small first steps that may each grow to a ceiling later; then the
/// whole slice reclaims and the kernel kills in the biggest. The cap does not
/// depend on TEHAS_TICKET_CONCURRENCY, so a dev session or a planning turn
/// counts against it too.
fn fit(
    max: u64,
    reserve: u64,
    ceiling_mb: u64,
    committed: u64,
    containers: u64,
) -> (u64, &'static str) {
    let slots = max / (ceiling_mb * MIB);
    let room = if committed < max {
        (max - committed) / reserve
    } else {
        0
    };
    if room == 0 {
        (0, "room")
    } else if containers >= slots {
        (0, "full")
    } else if room > slots - containers {
        (slots - containers, "")
    } else {
        (room, "")
    }
}

pub fn unreadable(committed: u64, max: u64, note: String) -> Room {
    Room {
        room: 0,
        committed,
        max,
        why: "unreadable".into(),
        note,
        pressure: None,
    }
}

/// full avg60 from the text of memory.pressure, as the kernel wrote it (0.00).
pub fn full_avg60(text: &str) -> Option<String> {
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("full ") else {
            continue;
        };
        for (at, _) in rest.rmatch_indices("avg60=") {
            let v = rest[at + "avg60=".len()..].split(' ').next().unwrap_or("");
            let (i, f) = v.split_once('.').unwrap_or((v, ""));
            let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
            if digits(i) && (f.is_empty() && !v.contains('.') || digits(f)) {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// How many more containers of one reserve fit, when `count` cycles hold a
/// lock. Each `r.cids` is the id one of them wrote with `docker run --cidfile`.
pub fn room(r: &Reading, s: &Usable, reserve: &str, count: u64) -> Room {
    let reserve = bytes(reserve).unwrap_or(0);
    let max = s.max;
    if max == 0 || reserve == 0 || s.ceiling_mb == 0 {
        return unreadable(0, max, "the slice was not probed".into());
    }
    let dir = format!("{}/tehas.slice", r.root);
    let Some(scopes) = &r.scopes else {
        // systemd removes the directory of a slice that is not active. A cycle
        // holds its lock also while it has no container in the slice (the
        // clone, the end), so each one is promised a reserve and nothing is
        // read. An active slice with no directory is wrong.
        if s.active == "active" {
            return unreadable(
                0,
                max,
                format!("tehas.slice is active and {dir} is missing"),
            );
        }
        let committed = count * reserve;
        let (room, why) = fit(max, reserve, s.ceiling_mb, committed, count);
        return Room {
            room,
            committed,
            max,
            why: why.into(),
            note: String::new(),
            pressure: None,
        };
    };
    let mut committed = 0;
    let mut containers = 0;
    let mut names = Vec::new();
    for sc in scopes.iter().filter(|sc| sc.name.ends_with(".scope")) {
        match charge(&format!("{dir}/{}", sc.name), sc, reserve) {
            Ok(c) => {
                committed += c;
                containers += 1;
                names.push(sc.name.clone());
            }
            Err(note) => return unreadable(0, max, note),
        }
    }
    // A cycle that holds its lock and has no container yet is promised one
    // reserve. The cid files tell which cycles are among the children already.
    let scoped = r
        .cids
        .iter()
        .filter(|id| {
            id.len() == 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
        .filter(|id| names.contains(&format!("docker-{id}.scope")))
        .count() as u64;
    if count > scoped {
        committed += (count - scoped) * reserve;
        containers += count - scoped;
    }
    let pressure = match &r.pressure {
        None => {
            return unreadable(
                committed,
                max,
                format!("{dir}/memory.pressure cannot be read"),
            )
        }
        Some(t) => match full_avg60(t) {
            Some(p) => p,
            None => {
                return unreadable(
                    committed,
                    max,
                    format!("{dir}/memory.pressure has no full avg60"),
                )
            }
        },
    };
    let (mut room, mut why) = fit(max, reserve, s.ceiling_mb, committed, containers);
    // Above 10 means the whole slice stood still for more than a tenth of the
    // last minute. A slice's pressure counts every stall in it, also one
    // container's at its own ceiling, which another container does not change.
    // So it holds only once more than half the slice is promised, which no
    // ceiling is alone.
    if committed * 2 > max {
        let (i, f) = pressure.split_once('.').unwrap_or((&pressure, "0"));
        let mut f: String = f.chars().take(2).collect();
        if f.len() < 2 {
            f.push('0');
        }
        let hundredths = i.parse::<u64>().unwrap_or(0) * 100 + f.parse::<u64>().unwrap_or(0);
        if hundredths > 1000 {
            room = 0;
            why = "pressure";
        }
    }
    Room {
        room,
        committed,
        max,
        why: why.into(),
        note: String::new(),
        pressure: Some(pressure),
    }
}

/// gib: a size for a person, to a tenth of a GiB, rounded down.
pub fn gib(b: u64) -> String {
    format!("{}.{} GiB", b / GIB, b % GIB * 10 / GIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(cycles: u64) -> Load {
        Load {
            reserve: "2g".into(),
            containers: "4".into(),
            cycles: cycles.to_string(),
            ..Load::default()
        }
    }

    fn slice(cpus: u64, ceiling_mb: u64) -> Usable {
        Usable {
            max: ceiling_mb * 4 * MIB,
            active: "active".into(),
            ceiling_mb,
            cpus,
        }
    }

    /// A reading that passes every check but the ones a case names.
    fn host() -> Reading {
        let mut show = Map::new();
        show.insert(
            "FragmentPath".into(),
            Value::from("/etc/systemd/system/tehas.slice"),
        );
        show.insert("ActiveState".into(), Value::from("active"));
        show.insert("MemoryMax".into(), Value::from("34359738368"));
        Reading {
            show: Some(show),
            docker: Some("systemd 2".into()),
            controllers: Some("cpu memory pids".into()),
            cpus: Some("8".into()),
            root: "/sys/fs/cgroup".into(),
            scopes: Some(vec![]),
            ..Default::default()
        }
    }

    #[test]
    fn a_count_of_containers_that_is_no_number_is_refused_by_name() {
        // The instance asked for `auto`, which config.sh lets through for the
        // poll to answer: a command that starts a container has no count to
        // size one with, and says which of the two it is.
        let why = usable(&host(), "2g", "auto").unwrap_err();
        assert_eq!(why, "TEHAS_SLICE_CONTAINERS=auto is not a count");
        assert!(usable(&host(), "2g", "4").is_ok());
    }

    #[test]
    fn docker_is_read_from_its_first_line_and_all_of_that_line_counts() {
        let answer = |docker: &str| {
            let mut r = host();
            r.docker = Some(docker.into());
            usable(&r, "2g", "4")
        };
        assert!(answer("systemd 2\ncgroupfs 1").is_ok());
        assert_eq!(
            answer("systemd 2 extra").unwrap_err(),
            "Docker runs containers with the systemd cgroup driver on cgroup v2 extra, tehas needs systemd on v2"
        );
        assert_eq!(answer("\nsystemd 2").unwrap_err(), "docker gave no answer");
    }

    #[test]
    fn the_last_avg60_of_the_first_full_line_that_has_one_is_the_pressure() {
        let read = |t: &str| full_avg60(t);
        assert_eq!(
            read("some avg60=9.00\nfull avg10=1.00 avg60=3.41 avg300=0.00\n").as_deref(),
            Some("3.41")
        );
        assert_eq!(read("full avg60=1.00 avg60=2.00").as_deref(), Some("2.00"));
        assert_eq!(read("full avg60=1.00 avg60=x").as_deref(), Some("1.00"));
        assert_eq!(read("full avg60=7").as_deref(), Some("7"));
        assert_eq!(read("full avg60=7."), None);
        assert_eq!(read("some avg60=1.00"), None);
    }

    #[test]
    fn a_line_of_memory_stat_is_a_key_and_all_that_follows_it() {
        let charged = |stat: &str, low: &str| {
            let s = Scope {
                name: "a.scope".into(),
                stat: Some(stat.into()),
                low: Some(low.into()),
            };
            charge("d", &s, 2048)
        };
        let whole = "anon 1\nshmem 2\nkernel 8\nslab_reclaimable 4\n";
        assert_eq!(charged(whole, "0\n"), Ok(7));
        assert_eq!(charged(whole, "max\n"), Ok(2048));
        assert_eq!(
            charged("anon 1 2\nshmem 2\nkernel 8\n", "0"),
            Err("d/memory.stat has no number for anon".to_string())
        );
        // A kernel line with no value is no kernel line: slab is not subtracted.
        assert_eq!(
            charged("anon 1\nshmem 2\nkernel\nslab_reclaimable 4\n", "0"),
            Ok(3)
        );
        assert_eq!(
            charged(whole, "1 2\n"),
            Err("d/memory.low is not a number".to_string())
        );
    }

    #[test]
    fn the_limits_are_fourteen_words_with_a_quarter_of_the_max() {
        let words = limits(&slice(8, 5120), &load(3)).unwrap();
        assert_eq!(
            words.join(" "),
            "--cgroup-parent tehas.slice --memory 5120m --memory-swap 5120m \
             --memory-reservation 2g --cpus 7.00 --pids-limit 3072 --cpu-shares 1024"
        );
        assert_eq!(words.len(), 14);
    }

    #[test]
    fn the_limits_carry_the_reserve_as_the_instance_wrote_it() {
        let mut l = load(1);
        l.reserve = "512m".into();
        assert_eq!(limits(&slice(8, 5120), &l).unwrap()[7], "512m");
    }

    #[test]
    fn no_limits_without_a_ceiling_a_reserve_or_a_concurrency() {
        assert_eq!(limits(&slice(8, 0), &load(1)), None);
        let mut l = load(1);
        l.reserve = "much".into();
        assert_eq!(limits(&slice(8, 5120), &l), None);
        let mut l = load(1);
        l.cycles = String::new();
        assert_eq!(limits(&slice(8, 5120), &l), None);
        assert_eq!(cpus(0, &load(1)), None);
    }

    // From test/slice_test.sh: <host CPUs> <TEHAS_TICKET_CONCURRENCY> <quota>
    // <pids limit>.
    #[test]
    fn the_quota_is_the_hosts_cpus_less_one_whatever_the_cycles_side_by_side() {
        for (host, cycles, quota, pids) in [
            (16, 4, "15.00", 4096),
            (16, 15, "15.00", 4096),
            (16, 1, "15.00", 4096),
            (12, 4, "11.00", 3072),
            (8, 3, "7.00", 3072),
            (8, 7, "7.00", 2048),
            (4, 3, "3.00", 2048),
            (4, 20, "3.00", 2048),
            (2, 1, "1.00", 2048),
            (1, 1, "1.00", 2048),
            (64, 4, "63.00", 4096),
        ] {
            assert_eq!(
                cpus(host, &load(cycles)),
                Some((quota.to_string(), pids)),
                "{host} {cycles}"
            );
        }
    }

    // From test/slice_test.sh: the cycles side by side are counted at most
    // TEHAS_SLICE_CONTAINERS, with the refresh lane when it takes conflicts.
    #[test]
    fn the_pids_limit_follows_the_share_with_the_refresh_lane() {
        for (host, cycles, lane, on, n, quota, pids) in [
            (8, 2, 1, "1", 4, "7.00", 3072),
            (8, 2, 0, "1", 4, "7.00", 4096),
            (8, 2, 1, "0", 4, "7.00", 4096),
            (8, 4, 1, "1", 4, "7.00", 2048),
            (8, 3, 2, "1", 4, "7.00", 2048),
            (16, 5, 1, "1", 4, "15.00", 4096),
            (16, 5, 1, "1", 6, "15.00", 3072),
        ] {
            let l = Load {
                reserve: "2g".into(),
                containers: n.to_string(),
                cycles: cycles.to_string(),
                conflicts: on.into(),
                slots: lane.to_string(),
            };
            assert_eq!(
                cpus(host, &l),
                Some((quota.to_string(), pids)),
                "{host} {cycles} {lane} {on} {n}"
            );
        }
    }

    // From test/slice_test.sh: <host CPUs> <TEHAS_TICKET_CONCURRENCY> <slice
    // max in GiB> <gate runs at once>.
    #[test]
    fn a_container_runs_a_gate_for_each_two_cpus_of_its_share_and_2_gib_of_its_ceiling() {
        for (host, cycles, gib, want) in [
            (16, 4, 20, 1),
            (16, 1, 20, 2),
            (16, 1, 64, 7),
            (16, 2, 64, 3),
            (16, 4, 64, 1),
            (12, 4, 64, 1),
            (8, 1, 8, 1),
            (2, 1, 64, 1),
            (64, 1, 256, 31),
        ] {
            let words = limits(&slice(host, gib * 1024 / 4), &load(cycles)).unwrap();
            assert_eq!(
                verify(&words, &load(cycles)),
                Some(want),
                "{host} {cycles} {gib}"
            );
        }
    }

    #[test]
    fn no_limits_no_count_of_gate_runs() {
        assert_eq!(verify(&[], &load(1)), None);
        let words = ["--cpus", "x", "--memory", "2048m"].map(String::from);
        assert_eq!(verify(&words, &load(1)), None);
    }

    #[test]
    fn a_concurrency_that_is_no_number_leaves_the_quota_to_size_the_gate() {
        let words = limits(&slice(16, 8192), &load(1)).unwrap();
        let mut l = load(1);
        l.cycles = String::new();
        assert_eq!(verify(&words, &l), Some(4));
    }

    #[test]
    fn the_refresh_lane_counts_only_with_refreshes_on_and_a_number() {
        let mut l = load(2);
        assert_eq!(share(8, &l), Some(400));
        l.conflicts = "1".into();
        l.slots = "1".into();
        assert_eq!(share(8, &l), Some(266));
        l.slots = "many".into();
        assert_eq!(share(8, &l), Some(400));
    }

    #[test]
    fn no_count_of_containers_no_share() {
        for bad in ["auto", "0", ""] {
            let mut l = load(2);
            l.containers = bad.into();
            assert_eq!(share(8, &l), None, "{bad:?}");
        }
    }

    #[test]
    fn verify_takes_the_last_of_a_word_given_twice_and_needs_both() {
        let w = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let mut l = load(1);
        l.cycles = String::new();
        assert_eq!(
            verify(
                &w(&["--cpus", "1.00", "--cpus", "8.00", "--memory", "8192m"]),
                &l
            ),
            Some(4)
        );
        assert_eq!(verify(&w(&["--cpus", "8.00"]), &l), None);
        assert_eq!(verify(&w(&["--memory", "8192m", "--cpus"]), &l), None);
    }

    #[test]
    fn an_answer_of_systemctl_without_a_field_leaves_that_field_empty() {
        let m = shown("Id=tehas.slice\nActiveState=active\nActiveState=inactive\n");
        assert_eq!(m["FragmentPath"], "");
        assert_eq!(m["MemoryMax"], "");
        assert_eq!(m["ActiveState"], "inactive");
    }
}
