//! tehas.slice for the bash poll and `tehas improve`, called through
//! lib/slice.sh. The rules are `tehas_core::slice`, the same the commands that
//! start a container ask through `tehas-cycle sandbox`. Every verb only reads,
//! and both outside calls run under `timeout` so a silent one cannot block a
//! poll. Never called from lib/config.sh or a login shell.
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
//! The other verbs read what they need of those from the environment. No outside
//! call.
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
//! TEHAS_SLICE_WAIT (the bound of a call, 10 s) and TEHAS_CGROUP_ROOT (the
//! tests' seam for /sys/fs/cgroup).
//!
//! Exit: 0 or 1 as above, 2 usage.

use std::env;
use std::fs;
use tehas_core::slice::{self, num, root, Host, Load, Reading, Room, Usable};

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
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

fn room(count: &str, cids: &[String]) -> String {
    let reserve = var("TEHAS_CYCLE_RESERVE");
    let reserve_b = slice::bytes(&reserve).unwrap_or(0);
    let s = Usable {
        max: num(&var("TEHAS_SLICE_MAX")).unwrap_or(0),
        active: var("TEHAS_SLICE_ACTIVE"),
        ceiling_mb: num(&var("TEHAS_SLICE_CEILING_MB")).unwrap_or(0),
        cpus: 0,
    };
    let probed = s.max != 0 && reserve_b != 0 && s.ceiling_mb != 0;
    let r: Room = match num(count) {
        None if probed => {
            slice::unreadable(0, s.max, "the count of cycles is not a number".to_string())
        }
        count => {
            let reading = Reading::of_room(root(), cids);
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
    let t = fs::read(&file).map_err(|_| format!("{file} cannot be read"))?;
    slice::full_avg60(&String::from_utf8_lossy(&t))
        .ok_or_else(|| format!("{file} has no full avg60"))
}

pub fn main(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("ok") => {
            let load = Load::from_env();
            match slice::usable(&Host::here(), &load.reserve, &load.containers) {
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
            eprintln!("usage: console slice ok|room|pressure ...");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_for_eval_stays_one_word_whatever_it_holds() {
        assert_eq!(quoted("a b"), "'a b'");
        assert_eq!(quoted("it's $(x)"), r"'it'\''s $(x)'");
        assert_eq!(
            assignments(None, "tehas.slice's limit"),
            "TEHAS_SLICE_WHY='tehas.slice'\\''s limit'\nTEHAS_SLICE_MAX=''\nTEHAS_SLICE_ACTIVE=''\nTEHAS_SLICE_CEILING_MB=''\nTEHAS_SLICE_CPUS=''\n"
        );
    }
}
