//! tehas.slice for the commands that start a container, called through
//! lib/slice.sh. The rules of usable and room are `tehas_core::slice`; here are
//! the host's answers and what a container is given. Every verb only reads, and
//! both outside calls run under `timeout` so a silent one cannot block a poll.
//! Never called from lib/config.sh or a login shell.
//!
//!   console slice ok
//!
//! Is the slice on this host usable? One systemctl call, one docker call, one
//! file read. Prints shell assignments for `eval` and exits 0 or 1:
//!   TEHAS_SLICE_WHY         one line a person can act on, empty when usable
//!   TEHAS_SLICE_MAX         the slice's MemoryMax, bytes
//!   TEHAS_SLICE_ACTIVE      "active", or another state
//!   TEHAS_SLICE_CEILING_MB  what one container may use: max over
//!                           TEHAS_SLICE_CONTAINERS, MiB
//!   TEHAS_SLICE_CPUS        the host's CPUs, from nproc
//!
//! The other verbs read the last four from the environment. No outside call.
//!
//!   console slice limits
//!
//! The fourteen words for docker run, on one line, or exit 1:
//!   --cgroup-parent tehas.slice --memory <n>m --memory-swap <n>m
//!   --memory-reservation <reserve> --cpus <c> --pids-limit <p> --cpu-shares 1024
//!
//!   console slice cpus
//!
//! The quota and the pids limit of one container, "<c> <p>", or exit 1 without
//! a probe or with a concurrency that is not a number. <c> is the host's CPUs
//! less one, never below one CPU, as docker writes it (15.00). <p> is 1024 for
//! each CPU of the container's share, rounded up, from 2048 to 4096.
//!
//!   console slice verify <the words of limits>
//!
//! How many runs of the product's gate one container runs at once: one for each
//! two whole CPUs of its share and each 2 GiB of its memory ceiling, the lower,
//! at least one. The rule pi-rukas applies to a host, applied to the container.
//! Exit 1 when the words have no quota or ceiling.
//!
//!   console slice room <count> [<cid file>...]
//!
//! How many more containers of one reserve fit. <count> is the cycles that hold
//! a lock; each cid file is what `docker run --cidfile` wrote for one of them.
//! Always exits 0 and prints one line:
//!   <room> <committed> <max> <reserve> [<why> [<note>]]
//! The three in the middle are bytes. <why> is absent when there is room, else
//! room (promised to the full), full (as many containers as their ceilings
//! fit), pressure (full avg60 above 10 and more than half the max promised) or
//! unreadable (room is 0, and <note> names the file and what was missing).
//!
//!   console slice pressure
//!
//! "full avg60" of the slice as the kernel writes it (0.00), or, with exit 1,
//! the file and what was missing.
//!
//! Settings read: TEHAS_CYCLE_RESERVE, TEHAS_SLICE_CONTAINERS (4 when unset),
//! TEHAS_TICKET_CONCURRENCY, TEHAS_REFRESH_CONFLICTS and TEHAS_REFRESH_SLOTS,
//! TEHAS_SLICE_WAIT (the bound of a call, 10 s) and TEHAS_CGROUP_ROOT (the
//! tests' seam for /sys/fs/cgroup).
//!
//! Exit: 0 or 1 as above, 2 usage.

use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::slice::{self, num, Probe, Reading, Room, Scope, Usable};

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

fn var_or(name: &str, default: &str) -> String {
    Some(var(name))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into())
}

fn root() -> String {
    var_or("TEHAS_CGROUP_ROOT", "/sys/fs/cgroup")
}

fn text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
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

/// This host, asked when a check needs it.
struct Host {
    root: String,
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
        let t = text(&Path::new(&self.root).join("cgroup.controllers"))?;
        Some(t.lines().next().unwrap_or("").trim().to_string())
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

fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn assignments(u: Option<&Usable>, why: &str) -> String {
    let fields = [
        ("WHY", why.to_string()),
        ("MAX", u.map(|u| u.max.to_string()).unwrap_or_default()),
        ("ACTIVE", u.map(|u| u.active.clone()).unwrap_or_default()),
        (
            "CEILING_MB",
            u.map(|u| u.ceiling_mb.to_string()).unwrap_or_default(),
        ),
        ("CPUS", u.map(|u| u.cpus.to_string()).unwrap_or_default()),
    ];
    fields
        .iter()
        .map(|(k, v)| format!("TEHAS_SLICE_{k}={}\n", quoted(v)))
        .collect()
}

/// The settings a container's size comes from, as written.
struct Side {
    cpus: String,
    cycles: String,
    conflicts: String,
    slots: String,
    containers: String,
}

impl Side {
    fn from_env() -> Side {
        Side {
            cpus: var("TEHAS_SLICE_CPUS"),
            cycles: var("TEHAS_TICKET_CONCURRENCY"),
            conflicts: var("TEHAS_REFRESH_CONFLICTS"),
            slots: var("TEHAS_REFRESH_SLOTS"),
            containers: var_or("TEHAS_SLICE_CONTAINERS", "4"),
        }
    }

    /// One cycle's share of the host's CPUs less one, in hundredths, never
    /// below one CPU: the CPUs it gets when every cycle side by side is busy.
    fn share(&self) -> Option<u64> {
        let cpus = num(&self.cpus).filter(|c| *c > 0)?;
        let mut cycles = num(&self.cycles).filter(|c| *c > 0)?;
        if self.conflicts == "1" {
            cycles += num(&self.slots).unwrap_or(0);
        }
        let cycles = cycles.min(num(&self.containers).filter(|n| *n > 0)?);
        let spare = cpus.saturating_sub(1).max(1);
        Some((spare * 100 / cycles).max(100))
    }

    /// The quota is a ceiling, not a share that is held: a container may use
    /// every CPU but one, and the weight splits them when several are busy. A
    /// gate run alone gets the whole host, not a fixed part of it. One CPU is
    /// left to the host for its own services and the model backends.
    ///
    /// The pids limit follows the share, not the quota: a container's processes
    /// are mostly what its gate runs side by side, and a gate sized by its
    /// quota on a busy host gets its share of CPU, not more processes' worth of
    /// it. 2048 at least, for the agents, their tools and git around the gate.
    fn cpus(&self) -> Option<(String, u64)> {
        let share = self.share()?;
        let quota = num(&self.cpus)?.saturating_sub(1).max(1);
        let pids = share.div_ceil(100) * 1024;
        Some((format!("{quota}.00"), pids.clamp(2048, 4096)))
    }

    fn limits(&self, ceiling_mb: &str, reserve: &str) -> Option<Vec<String>> {
        num(ceiling_mb).filter(|mb| *mb > 0)?;
        slice::bytes(reserve)?;
        let (quota, pids) = self.cpus()?;
        let memory = format!("{ceiling_mb}m");
        let words = [
            "--cgroup-parent",
            "tehas.slice",
            "--memory",
            &memory,
            "--memory-swap",
            &memory,
            "--memory-reservation",
            reserve,
            "--cpus",
            &quota,
            "--pids-limit",
            &pids.to_string(),
            "--cpu-shares",
            "1024",
        ];
        Some(words.map(str::to_string).to_vec())
    }

    fn verify(&self, words: &[String]) -> Option<u64> {
        let (mut cpus, mut mb) = ("", "");
        for pair in words.windows(2) {
            match pair[0].as_str() {
                "--cpus" => cpus = pair[1].split('.').next().unwrap_or(""),
                "--memory" => mb = pair[1].strip_suffix('m').unwrap_or(&pair[1]),
                _ => {}
            }
        }
        let (mut cpus, mb) = (num(cpus)?, num(mb)?);
        // Each run is sized for the share: the quota is the whole host less
        // one, and every cycle side by side may be running its gate.
        if let Some(share) = self.share() {
            cpus = cpus.min(share / 100);
        }
        Some((cpus / 2).min(mb / 2048).max(1))
    }
}

/// The children of the slice in the order a shell's glob gives them. A
/// container that ended while it was read is gone, not unreadable. None when
/// the directory is missing.
fn scopes(dir: &Path) -> Option<Vec<Scope>> {
    if !dir.is_dir() {
        return None;
    }
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".scope") && !n.starts_with('.'))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let path = dir.join(&name);
        if !path.is_dir() {
            continue;
        }
        let (stat, low) = (
            text(&path.join("memory.stat")),
            text(&path.join("memory.low")),
        );
        if (stat.is_none() || low.is_none()) && !path.is_dir() {
            continue;
        }
        out.push(Scope { name, stat, low });
    }
    Some(out)
}

/// The container id in a cid file, as `read` gives its first line.
fn cid(file: &str) -> Option<String> {
    let t = text(Path::new(file))?;
    Some(t.lines().next().unwrap_or("").trim().to_string())
}

fn room(count: &str, cids: &[String]) -> String {
    let reserve = var("TEHAS_CYCLE_RESERVE");
    let reserve_b = slice::bytes(&reserve).unwrap_or(0);
    let s = Usable {
        max: num(&var("TEHAS_SLICE_MAX")).unwrap_or(0),
        active: var("TEHAS_SLICE_ACTIVE"),
        ceiling_mb: num(&var("TEHAS_SLICE_CEILING_MB")).unwrap_or(0),
        cpus: 0,
    };
    let root = root();
    let dir = Path::new(&root).join("tehas.slice");
    let probed = s.max != 0 && reserve_b != 0 && s.ceiling_mb != 0;
    let r: Room = match num(count) {
        None if probed => {
            slice::unreadable(0, s.max, "the count of cycles is not a number".to_string())
        }
        count => {
            let reading = Reading {
                scopes: scopes(&dir),
                pressure: text(&dir.join("memory.pressure")),
                cids: cids.iter().filter_map(|f| cid(f)).collect(),
                root,
                ..Default::default()
            };
            slice::room(&reading, &s, &reserve, count.unwrap_or(0))
        }
    };
    let mut line = format!("{} {} {} {reserve_b}", r.room, r.committed, r.max);
    for word in [&r.why, &r.note] {
        if !word.is_empty() {
            line.push(' ');
            line.push_str(word);
        }
    }
    line
}

fn pressure() -> Result<String, String> {
    let file = format!("{}/tehas.slice/memory.pressure", root());
    let t = text(Path::new(&file)).ok_or_else(|| format!("{file} cannot be read"))?;
    slice::full_avg60(&t).ok_or_else(|| format!("{file} has no full avg60"))
}

fn print_or_fail(line: Option<String>) -> u8 {
    match line {
        Some(line) => {
            println!("{line}");
            0
        }
        None => 1,
    }
}

pub fn main(given: &[String]) -> u8 {
    let side = Side::from_env;
    match given.first().map(String::as_str) {
        Some("ok") => {
            let host = Host { root: root() };
            let reserve = var("TEHAS_CYCLE_RESERVE");
            let containers = var_or("TEHAS_SLICE_CONTAINERS", "4");
            match slice::usable(&host, &reserve, &containers) {
                Ok(u) => {
                    print!("{}", assignments(Some(&u), ""));
                    0
                }
                Err(why) => {
                    print!("{}", assignments(None, &why));
                    1
                }
            }
        }
        Some("limits") => print_or_fail(
            side()
                .limits(&var("TEHAS_SLICE_CEILING_MB"), &var("TEHAS_CYCLE_RESERVE"))
                .map(|w| w.join(" ")),
        ),
        Some("cpus") => print_or_fail(side().cpus().map(|(c, p)| format!("{c} {p}"))),
        Some("verify") => print_or_fail(side().verify(&given[1..]).map(|n| n.to_string())),
        Some("room") => {
            let count = given.get(1).map(String::as_str).unwrap_or("");
            println!("{}", room(count, given.get(2..).unwrap_or(&[])));
            0
        }
        Some("pressure") => match pressure() {
            Ok(p) => {
                println!("{p}");
                0
            }
            Err(why) => {
                println!("{why}");
                1
            }
        },
        _ => {
            eprintln!("usage: console slice ok|limits|cpus|verify|room|pressure ...");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(cpus: &str, cycles: &str) -> Side {
        Side {
            cpus: cpus.into(),
            cycles: cycles.into(),
            conflicts: "0".into(),
            slots: "1".into(),
            containers: "4".into(),
        }
    }

    #[test]
    fn a_value_for_eval_stays_one_word_whatever_it_holds() {
        assert_eq!(quoted("a b"), "'a b'");
        assert_eq!(quoted("it's $(x)"), r"'it'\''s $(x)'");
        assert_eq!(
            assignments(None, "tehas.slice's limit"),
            "TEHAS_SLICE_WHY='tehas.slice'\\''s limit'\nTEHAS_SLICE_MAX=''\nTEHAS_SLICE_ACTIVE=''\nTEHAS_SLICE_CEILING_MB=''\nTEHAS_SLICE_CPUS=''\n"
        );
    }

    #[test]
    fn an_answer_of_systemctl_without_a_field_leaves_that_field_empty() {
        let m = shown("Id=tehas.slice\nActiveState=active\nActiveState=inactive\n");
        assert_eq!(m["FragmentPath"], "");
        assert_eq!(m["MemoryMax"], "");
        assert_eq!(m["ActiveState"], "inactive");
    }

    #[test]
    fn the_refresh_lane_counts_only_with_refreshes_on_and_a_number() {
        let mut s = side("9", "2");
        assert_eq!(s.share(), Some(400));
        s.conflicts = "1".into();
        assert_eq!(s.share(), Some(266));
        s.slots = "many".into();
        assert_eq!(s.share(), Some(400));
    }

    #[test]
    fn no_count_of_containers_no_share() {
        for bad in ["auto", "0", ""] {
            let mut s = side("9", "2");
            s.containers = bad.into();
            assert_eq!(s.share(), None, "{bad:?}");
        }
    }

    #[test]
    fn one_cpu_is_a_quota_of_one() {
        assert_eq!(side("1", "1").cpus(), Some(("1.00".into(), 2048)));
    }

    #[test]
    fn verify_takes_the_last_of_a_word_given_twice_and_needs_both() {
        let w = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let s = side("", "");
        assert_eq!(
            s.verify(&w(&[
                "--cpus", "1.00", "--cpus", "8.00", "--memory", "8192m"
            ])),
            Some(4)
        );
        assert_eq!(s.verify(&w(&["--cpus", "8.00"])), None);
        assert_eq!(s.verify(&w(&["--memory", "8192m", "--cpus"])), None);
    }

    #[test]
    fn a_scope_gone_while_it_was_read_is_not_in_the_list() {
        let dir = std::env::temp_dir().join(format!("tehas-slice-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("docker-b.scope")).unwrap();
        fs::create_dir_all(dir.join("docker-a.scope")).unwrap();
        fs::create_dir_all(dir.join("not-a-child")).unwrap();
        fs::write(dir.join("file.scope"), "").unwrap();
        fs::write(dir.join("docker-a.scope/memory.stat"), "anon 1\n").unwrap();
        let listed = scopes(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let names: Vec<_> = listed.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["docker-a.scope", "docker-b.scope"]);
        assert!(listed[0].stat.is_some() && listed[0].low.is_none());
        assert_eq!(scopes(&dir).map(|s| s.len()), None);
    }
}
