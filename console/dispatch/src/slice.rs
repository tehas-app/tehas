//! tehas.slice, as lib/slice.sh reads it: whether it can be used
//! (tehas_slice_ok) and the room it has (tehas_slice_room). The readings are
//! the snapshot's; the arithmetic is unchanged.

use serde_json::{Map, Value};

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

#[derive(Clone, Debug)]
pub struct Scope {
    pub name: String,
    pub stat: Option<String>,
    pub low: Option<String>,
}

/// A slice that can be used.
#[derive(Clone, Debug, PartialEq)]
pub struct Usable {
    pub max: u64,
    pub active: String,
    pub ceiling_mb: u64,
}

/// tehas_slice_room's answer.
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

fn num(s: &str) -> Option<u64> {
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

/// A Docker size (6m, 2g, bytes) in bytes: _tehas_slice_bytes.
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

/// tehas_slice_ok: the slice, or the line a person can act on.
pub fn usable(r: &Reading, reserve: &str) -> Result<Usable, String> {
    let Some(show) = r.show.as_ref().filter(|m| !m.is_empty()) else {
        return Err("systemctl gave no answer".into());
    };
    let field = |k: &str| {
        show.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    if field("FragmentPath").is_empty() {
        return Err("no slice file on this host (sudo ./bootstrap.sh slice)".into());
    }
    let Some(max) = num(&field("MemoryMax")) else {
        return Err("tehas.slice has no memory limit (sudo ./bootstrap.sh slice)".into());
    };
    let docker = r.docker.clone().unwrap_or_default();
    let mut w = docker.split_whitespace();
    let (driver, version) = (w.next().unwrap_or(""), w.next().unwrap_or(""));
    if driver.is_empty() || version.is_empty() {
        return Err("docker gave no answer".into());
    }
    if (driver, version) != ("systemd", "2") {
        return Err(format!(
            "Docker runs containers with the {driver} cgroup driver on cgroup v{version}, tehas needs systemd on v2"
        ));
    }
    let ctl = format!(" {} ", r.controllers.clone().unwrap_or_default());
    if ["cpu", "memory", "pids"]
        .iter()
        .any(|c| !ctl.contains(&format!(" {c} ")))
    {
        return Err(format!(
            "{}/cgroup.controllers does not list cpu, memory and pids",
            r.root
        ));
    }
    let Some(reserve_b) = bytes(reserve) else {
        return Err(format!("TEHAS_CYCLE_RESERVE={reserve} is not a size"));
    };
    let ceiling = max / 4 / MIB;
    if reserve_b > ceiling * MIB {
        let need = reserve_b * 16 / 3;
        let need = if need >= GIB {
            format!("{} GiB", (need + 536_870_912) / GIB)
        } else {
            format!("{} MiB", need / MIB + 1)
        };
        return Err(format!(
            "TEHAS_CYCLE_RESERVE={reserve} is more than a container may use here ({ceiling}m, a quarter of tehas.slice's MemoryMax); the host needs about {need} of memory for this reserve"
        ));
    }
    match r.cpus.as_deref().and_then(num) {
        Some(c) if c > 0 => {}
        _ => return Err("nproc gave no count of the host's CPUs".into()),
    }
    Ok(Usable {
        max,
        active: field("ActiveState"),
        ceiling_mb: ceiling,
    })
}

/// _tehas_slice_charge: what one scope is charged with, or the file and key
/// that could not be read.
fn charge(dir: &str, s: &Scope, reserve: u64) -> Result<u64, String> {
    let Some(stat) = &s.stat else {
        return Err(format!("{dir}/memory.stat cannot be read"));
    };
    let Some(low) = &s.low else {
        return Err(format!("{dir}/memory.low cannot be read"));
    };
    let (mut anon, mut shmem, mut kernel, mut slab) = (None, None, None, None);
    for line in stat.lines() {
        let mut w = line.split_whitespace();
        let (k, v) = (w.next().unwrap_or(""), w.next().unwrap_or("").to_string());
        match k {
            "anon" => anon = Some(v),
            "shmem" => shmem = Some(v),
            "kernel" => kernel = Some(v),
            "slab_reclaimable" => slab = Some(v),
            _ => {}
        }
    }
    let low = low.split_whitespace().next().unwrap_or("");
    let low = if low == "max" {
        reserve.to_string()
    } else {
        low.to_string()
    };
    if kernel.is_none() {
        kernel = Some("0".into());
        slab = Some("0".into());
    }
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

/// _tehas_slice_fit: the reserves that fit, capped by the containers the
/// slice's max holds at their ceilings.
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

fn unreadable(committed: u64, max: u64, note: String) -> Room {
    Room {
        room: 0,
        committed,
        max,
        why: "unreadable".into(),
        note,
        pressure: None,
    }
}

/// full avg60 from the text of memory.pressure.
fn full_avg60(text: &str) -> Option<String> {
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("full ") else {
            continue;
        };
        for w in rest.split(' ') {
            if let Some(v) = w.strip_prefix("avg60=") {
                let (i, f) = v.split_once('.').unwrap_or((v, ""));
                let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
                if digits(i) && (f.is_empty() && !v.contains('.') || digits(f)) {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// tehas_slice_room: `count` cycles hold a lock.
pub fn room(r: &Reading, s: &Usable, reserve: &str, count: u64) -> Room {
    let reserve = bytes(reserve).unwrap_or(0);
    let max = s.max;
    if max == 0 || reserve == 0 || s.ceiling_mb == 0 {
        return unreadable(0, max, "the slice was not probed".into());
    }
    let dir = format!("{}/tehas.slice", r.root);
    let Some(scopes) = &r.scopes else {
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
