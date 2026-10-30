//! The slots `auto` asks for: how many cycles and containers fit the host
//! this poll read. lib/config.sh leaves the two settings as they were written
//! (`auto`, or a number); the readings are the snapshot's, taken on every
//! poll, so a host that grew or shrank is answered with what it has now.
//!
//! The containers are the slice's `MemoryMax` over what one container is
//! given; the cycles are the host's CPUs over what one cycle is given. Either
//! count stops at `TEHAS_CYCLE_CEILING`, the endpoint's bound, and at 16, the
//! top of the settings' own range - so `auto` never asks for more than a
//! person may write. A shrink of the host moves neither count below its
//! floor: it holds new starts, and a cycle that is already running is none of
//! this code's business. A number for either setting needs none of the
//! readings and keeps today's answer.

use crate::slice;
use serde_json::{Map, Value};

const MIB: u64 = 1_048_576;
const GIB: u64 = 1_073_741_824;
/// The most containers tehas.slice may hold at once, whatever its memory
/// says: lib/config.sh's ceiling for `TEHAS_SLICE_CONTAINERS`. The same
/// number is `TEHAS_CYCLE_CEILING`'s default, so an instance that names no
/// ceiling still bounds `auto` the way a number for the setting may.
const MAX: u64 = 16;
/// The fewest containers the settings allow: a slice of one container would
/// give it the whole slice.
const MIN_CONTAINERS: u64 = 2;
/// The fewest cycles, so that `auto` on a host whose readings say nothing
/// starts one cycle instead of nothing at all.
const MIN_CYCLES: u64 = 1;

/// What a poll read of the host, for the two counts. `memory_slots` and
/// `cpu_slots` are the readings over the sizes one container and one cycle are
/// given, before any bound; 0 for a reading that is missing or is no number.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub memory_slots: u64,
    pub cpu_slots: u64,
    /// The host's CPUs, None when nproc answered nothing.
    pub cpus: Option<u64>,
    /// What one container is given, as the setting writes it.
    pub container_memory: String,
    /// What one cycle is given, as the setting writes it.
    pub cycle_cpus: String,
    /// TEHAS_CYCLE_CEILING as the instance wrote it, empty when it did not.
    pub ceiling: String,
}

impl Probe {
    /// From the poll's readings: the slice's MemoryMax in bytes and nproc's
    /// answer, None when either was read as nothing, and the three settings
    /// that say what one container and one cycle are given.
    pub fn read(
        max: Option<u64>,
        cpus: Option<u64>,
        container_memory: &str,
        cycle_cpus: &str,
        ceiling: &str,
    ) -> Probe {
        Probe {
            memory_slots: match (max, size(container_memory)) {
                (Some(max), Some(b)) => max / b,
                _ => 0,
            },
            cpu_slots: match (cpus, count(cycle_cpus.trim()).filter(|n| *n > 0)) {
                (Some(c), Some(share)) if c > 0 => c / share,
                _ => 0,
            },
            cpus,
            container_memory: container_memory.to_string(),
            cycle_cpus: cycle_cpus.to_string(),
            ceiling: ceiling.trim().to_string(),
        }
    }

    /// The most cycles `auto` may take: what the instance wrote, or 16. The
    /// settings check keeps it a count of at most nine digits; anything else
    /// leaves the default in force.
    pub fn ceiling(&self) -> u64 {
        match self.ceiling.as_str() {
            "" => MAX,
            c => count(c).unwrap_or(MAX),
        }
    }

    /// One count's bound: the ceiling when it is the bound that stopped short
    /// of what the host alone holds, and the reading itself otherwise.
    fn bounded(&self, filled: u64, name: &'static str) -> &'static str {
        match self.ceiling().min(MAX) {
            c if filled > c => "ceiling",
            _ => name,
        }
    }
}

/// Why `auto` came to the count it did, for a person.
#[derive(Clone, Debug, PartialEq)]
pub struct Auto {
    /// cpu or memory for the reading that gave the count, ceiling when the
    /// endpoint's bound did, and host when no reading answered at all.
    pub bound: &'static str,
    /// The count the readings give before the bounds, 0 for no reading.
    pub slots: u64,
    /// The host's CPUs, None when nproc answered nothing.
    pub cpus: Option<u64>,
    /// What one container is given, as the setting writes it.
    pub container_memory: String,
    /// What one cycle is given, as the setting writes it.
    pub cycle_cpus: String,
    /// TEHAS_CYCLE_CEILING as the instance wrote it, or the default that
    /// bounds `auto` when it named none.
    pub ceiling: String,
    /// Below the floor of its own range, which is the count in force.
    pub floored: bool,
}

/// The answer for one poll: the two counts in force, and how each came about.
#[derive(Clone, Debug, PartialEq)]
pub struct Slots {
    /// The cycles the poll may start up to, `auto` or the number as written.
    pub cycles: i64,
    /// The containers tehas.slice holds at once.
    pub containers: u64,
    /// `auto`, `number` or `fixed` for a setting that asked for neither.
    pub cycle_how: &'static str,
    /// `auto`, `number` or `fixed`.
    pub container_how: &'static str,
    /// The derivation of each `auto`; None where the setting is a number.
    pub cycle: Option<Auto>,
    pub container: Option<Auto>,
}

impl Slots {
    /// The numbers and the reasons, for live/dispatch.json. A factory that
    /// sets neither setting to `auto` has nothing to explain, and gets
    /// nothing: the report keeps the shape it had before these settings.
    pub fn report(&self) -> Option<Value> {
        if self.cycle_how != "auto" && self.container_how != "auto" {
            return None;
        }
        let mut m = Map::new();
        m.insert("cycles".into(), Value::from(self.cycles));
        m.insert("containers".into(), Value::from(self.containers));
        m.insert("cyclesHow".into(), Value::from(self.cycle_how));
        m.insert("containersHow".into(), Value::from(self.container_how));
        for (key, a) in [("cycle", &self.cycle), ("container", &self.container)] {
            if let Some(a) = a {
                m.insert(
                    key.into(),
                    Value::Object(
                        [
                            ("bound".to_string(), Value::from(a.bound)),
                            ("slots".to_string(), Value::from(a.slots)),
                            ("cpus".to_string(), a.cpus.map_or(Value::Null, Value::from)),
                            (
                                "containerMemory".to_string(),
                                Value::from(a.container_memory.as_str()),
                            ),
                            ("cycleCpus".to_string(), Value::from(a.cycle_cpus.as_str())),
                            ("ceiling".to_string(), Value::from(a.ceiling.as_str())),
                            ("floored".to_string(), Value::from(a.floored)),
                        ]
                        .into_iter()
                        .collect::<Map<_, _>>(),
                    ),
                );
            }
        }
        Some(Value::Object(m))
    }
}

/// A count as the settings and the host write it: no sign, no space, no
/// leading zero, and short enough for the arithmetic above to hold.
fn count(s: &str) -> Option<u64> {
    let ok = s == "0"
        || (!s.is_empty()
            && s.len() <= 9
            && s.as_bytes()[0] != b'0'
            && s.bytes().all(|b| b.is_ascii_digit()));
    if ok {
        s.parse().ok()
    } else {
        None
    }
}

/// The bytes one container is given: a valid Docker size, never zero, never
/// below the 6m Docker takes for a memory value. The rule is lib/slice.sh's for
/// the reserve and `tehas work` reads that file's answer at every start; the
/// poll holds it here too, so a size that cannot name a container is answered
/// with no reading - the floor of 2 containers - rather than a huge count.
fn size(s: &str) -> Option<u64> {
    slice::bytes(s.trim()).filter(|b| *b >= 6 * MIB)
}

/// What one of the two settings asked for: `auto`, a number, or neither.
fn how(v: &str) -> &'static str {
    match v.trim() {
        "auto" => "auto",
        _ if count(v).is_some() => "number",
        _ => "fixed",
    }
}

/// The count one `auto` came to, and why. `filled` is what the host's own
/// readings give; the bounds are put on it here, so the reason is the bound
/// that actually stopped it.
fn auto(p: &Probe, filled: u64, name: &'static str, floor: u64) -> (u64, Auto) {
    let count = filled.min(MAX).min(p.ceiling()).max(floor);
    (
        count,
        Auto {
            bound: if filled == 0 {
                "host"
            } else {
                p.bounded(filled, name)
            },
            slots: filled,
            cpus: p.cpus,
            container_memory: p.container_memory.clone(),
            cycle_cpus: p.cycle_cpus.clone(),
            ceiling: if p.ceiling.is_empty() {
                MAX.to_string()
            } else {
                p.ceiling.clone()
            },
            floored: filled < floor,
        },
    )
}

/// The slots this poll's readings give.
pub fn derive(p: &Probe, containers_written: &str, cycles_written: &str) -> Slots {
    let (containers, container) = match how(containers_written) {
        "auto" => {
            let (n, a) = auto(p, p.memory_slots, "memory", MIN_CONTAINERS);
            (n, Some(a))
        }
        // lib/config.sh keeps a number in 2..=16; anything else is taken as
        // the floor, and slice::usable says so when it reads the setting.
        "number" => (count(containers_written).unwrap_or(0), None),
        _ => (MIN_CONTAINERS, None),
    };
    let (count, cycle) = match how(cycles_written) {
        "auto" => {
            let (n, a) = auto(p, p.cpu_slots, "cpu", MIN_CYCLES);
            (n, Some(a))
        }
        "number" => (count(cycles_written).unwrap_or(MIN_CYCLES), None),
        _ => (MIN_CYCLES, None),
    };
    Slots {
        cycles: count as i64,
        containers,
        cycle_how: how(cycles_written),
        container_how: how(containers_written),
        cycle,
        container,
    }
}

/// The count of cycles in force with the reason `auto` came to it, for the
/// poll's lines and live/dispatch.json: "auto: 6, memory-bound". Empty for a
/// factory that sets a number, whose line reads as it always did: the count
/// alone, in brackets, with `limit` in front of it.
pub fn words(s: &Slots) -> String {
    let Some(a) = s.cycle.as_ref().filter(|_| s.cycle_how == "auto") else {
        return String::new();
    };
    format!(
        "auto: {}, {}-bound{}",
        s.cycles,
        a.bound,
        if a.floored { ", no reading" } else { "" },
    )
}

/// The same in full, for a person who asks what a resize did: both counts and
/// the three numbers they are derived from. live/dispatch.json holds the same
/// fields as JSON; this is the line `tehas status` prints from them.
pub fn reason(s: &Slots) -> String {
    let Some(a) = s.cycle.as_ref().filter(|_| s.cycle_how == "auto") else {
        return String::new();
    };
    let cpus = a.cpus.map_or_else(|| "?".to_string(), |c| c.to_string());
    format!(
        "{} CPUs over TEHAS_CYCLE_CPUS={}, {} containers of tehas.slice over TEHAS_CONTAINER_MEMORY={}, ceiling {}",
        cpus,
        a.cycle_cpus,
        s.containers,
        a.container_memory,
        a.ceiling,
    )
}

/// The size of one container for a person: whole GiB where it is a multiple,
/// MiB below a GiB.
pub fn container_size(b: u64) -> String {
    if b >= GIB {
        format!("{}g", b / GIB)
    } else {
        format!("{}m", b / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: u64 = 1_073_741_824;

    /// Both settings `auto`, on a host whose slice holds `mem` GiB at
    /// TEHAS_CONTAINER_MEMORY and whose CPUs hold cycles at TEHAS_CYCLE_CPUS.
    fn both(max: u64, cpus: u64, mem: &str, share: &str, cap: &str) -> Slots {
        let p = Probe::read(Some(max), Some(cpus), mem, share, cap);
        derive(&p, "auto", "auto")
    }

    #[test]
    fn the_containers_follow_the_slice_and_the_cycles_the_cpus() {
        // 32 GiB of slice at 8g each is four containers; 16 CPUs at 4 each is
        // four cycles. Each count names what bounded it.
        let s = both(32 * G, 16, "8g", "4", "");
        assert_eq!((s.cycles, s.containers), (4, 4));
        assert_eq!((s.cycle_how, s.container_how), ("auto", "auto"));
        let a = s.cycle.clone().unwrap();
        assert_eq!((a.bound, a.slots, a.cpus), ("cpu", 4, Some(16)));
        assert_eq!(
            (
                a.container_memory.as_str(),
                a.cycle_cpus.as_str(),
                a.ceiling.as_str()
            ),
            ("8g", "4", "16")
        );
        assert_eq!(s.container.clone().unwrap().bound, "memory");
        assert!(!a.floored && !s.container.unwrap().floored);
    }

    #[test]
    fn the_cpus_bound_the_cycles_when_the_slice_is_big() {
        // 128 GiB at 8g is sixteen containers, the most the slice may hold at
        // all; 16 CPUs at 4 each is four cycles, and that is the answer. A
        // host that answers less than the bound everywhere names the reading
        // that filled it, not the bound, which never came into play.
        let s = both(128 * G, 16, "8g", "4", "");
        assert_eq!((s.cycles, s.containers), (4, 16));
        assert_eq!(s.cycle.clone().unwrap().bound, "cpu");
        assert_eq!(s.cycle.clone().unwrap().slots, 4);
        // A host with plenty of both answers the most the settings allow. The
        // container is sized above Docker's own 6m floor, which a smaller one
        // would fall to the floor of two containers for.
        let s = both(1024 * G, 256, "8g", "1", "");
        assert_eq!((s.cycles, s.containers), (16, 16));
    }

    #[test]
    fn the_ceiling_bounds_both_counts_and_names_itself() {
        // An endpoint that serves three cycles at once holds both counts
        // there, whatever the host holds. The host must answer more than
        // three of each, or the reading, not the ceiling, is the reason.
        let s = both(128 * G, 64, "8g", "4", "3");
        assert_eq!((s.cycles, s.containers), (3, 3));
        assert_eq!(s.cycle.clone().unwrap().bound, "ceiling");
        assert_eq!(s.container.clone().unwrap().bound, "ceiling");
        assert_eq!(s.cycle.clone().unwrap().ceiling, "3");
        // A ceiling the host never reaches is not the reason for anything:
        // 32 GiB at 8g is four containers, and 16 CPUs at 4 are four cycles,
        // both under the twelve below.
        let s = both(32 * G, 16, "8g", "4", "12");
        assert_eq!(s.cycle.clone().unwrap().bound, "cpu");
        assert_eq!(s.container.clone().unwrap().bound, "memory");
    }

    #[test]
    fn a_shrink_holds_new_starts_and_never_goes_below_the_floors() {
        let big = both(32 * G, 16, "8g", "4", "");
        let small = both(8 * G, 4, "8g", "4", "");
        assert_eq!((big.cycles, big.containers), (4, 4));
        assert_eq!((small.cycles, small.containers), (1, 2));
        assert!(small.cycles < big.cycles && small.containers < big.containers);
        // A host whose readings say nothing gets the smallest pair the
        // settings allow, which is what keeps a broken probe from fanning out.
        let none = derive(&Probe::read(None, None, "8g", "4", ""), "auto", "auto");
        assert_eq!((none.cycles, none.containers), (1, 2));
        assert_eq!(none.cycle.clone().unwrap().bound, "host");
        assert_eq!(none.container.clone().unwrap().bound, "host");
        assert!(none.cycle.unwrap().floored && none.container.unwrap().floored);
    }

    #[test]
    fn a_number_is_taken_as_written_and_asks_nothing() {
        // Seven cycles and three containers: no reading is consulted, and no
        // reason is shown, because nothing was derived.
        let s = derive(&Probe::read(None, None, "8g", "4", ""), "3", "7");
        assert_eq!((s.cycles, s.containers), (7, 3));
        assert_eq!((s.cycle_how, s.container_how), ("number", "number"));
        assert!(s.cycle.is_none() && s.container.is_none());
        assert!(s.report().is_none());
        assert_eq!(words(&s), "");
    }

    #[test]
    fn one_auto_setting_derives_alone() {
        let p = Probe::read(Some(32 * G), Some(16), "8g", "4", "");
        let s = derive(&p, "auto", "6");
        assert_eq!((s.cycles, s.containers), (6, 4));
        assert_eq!((s.cycle_how, s.container_how), ("number", "auto"));
        assert!(s.cycle.is_none() && s.container.is_some());
        let r = s.report().unwrap();
        assert_eq!(r["cyclesHow"], "number");
        assert_eq!(r["containersHow"], "auto");
        assert!(r.get("cycle").is_none());
        assert_eq!(r["container"]["bound"], "memory");
    }

    #[test]
    fn a_reading_that_is_no_number_holds_auto_at_its_floor() {
        // A size that is no size, a share that is no share and a slice that
        // was never probed each leave their count at the floor, with host as
        // the reason: the poll starts one cycle, not what it cannot show room
        // for.
        // 16 CPUs would give four cycles at TEHAS_CYCLE_CPUS=4; 4 gives one.
        let good = Probe::read(Some(128 * G), Some(16), "8g", "4", "");
        assert_eq!(derive(&good, "auto", "auto").containers, 16);
        for mem in ["", "0", "lots", "8x", "08g"] {
            let p = Probe::read(Some(128 * G), Some(4), mem, "4", "");
            let s = derive(&p, "auto", "auto");
            // 4 CPUs at 4 each is one cycle, the same answer a host with no
            // CPUs at all gives; the reading that failed is the containers',
            // which fall to the floor of two.
            assert_eq!((s.cycles, s.containers), (1, 2), "{mem}");
            assert_eq!(s.container.clone().unwrap().bound, "host");
            assert!(s.container.unwrap().floored);
        }
        for share in ["", "0", "four", "04", "1234567890"] {
            // No share is no divisor: the cycles fall to the floor of one
            // whatever the CPUs say, and the reason names the host.
            let p = Probe::read(Some(128 * G), Some(64), "8g", share, "");
            let s = derive(&p, "auto", "auto");
            // The containers keep the sixteen the memory holds; only the
            // cycles fall to the floor of one.
            assert_eq!((s.cycles, s.containers), (1, 16), "{share}");
            assert_eq!(s.cycle.clone().unwrap().bound, "host");
            assert!(s.cycle.unwrap().floored);
        }
        // A slice that was never probed holds the containers; a host whose
        // nproc answered nothing holds the cycles.
        let p = Probe::read(None, Some(16), "8g", "4", "");
        assert_eq!(derive(&p, "auto", "auto").containers, 2);
    }

    #[test]
    fn a_ceiling_that_is_no_count_leaves_the_default_in_force() {
        let p = Probe::read(Some(1024 * G), Some(256), "8g", "1", "many");
        assert_eq!(p.ceiling(), MAX);
        let s = derive(&p, "auto", "auto");
        assert_eq!((s.cycles, s.containers), (16, 16));
    }

    #[test]
    fn the_words_name_the_count_and_the_bound() {
        // The issue's own shape: "auto: 6, memory-bound". A factory with
        // numbers says nothing new, so its line keeps the word `limit`.
        let s = both(48 * G, 24, "8g", "4", "");
        assert_eq!(words(&s), "auto: 6, cpu-bound");
        let s = both(48 * G, 64, "8g", "4", "3");
        assert_eq!(words(&s), "auto: 3, ceiling-bound");
        let s = derive(&Probe::read(None, None, "8g", "4", ""), "auto", "auto");
        assert_eq!(words(&s), "auto: 1, host-bound, no reading");
        assert_eq!(
            words(&derive(&Probe::read(None, None, "8g", "4", ""), "4", "7")),
            ""
        );
    }

    #[test]
    fn the_long_reason_names_the_numbers_behind_the_count() {
        let s = both(48 * G, 24, "8g", "4", "");
        assert_eq!(
            reason(&s),
            "24 CPUs over TEHAS_CYCLE_CPUS=4, 6 containers of tehas.slice over TEHAS_CONTAINER_MEMORY=8g, ceiling 16"
        );
        assert_eq!(
            reason(&derive(&Probe::read(None, None, "8g", "4", ""), "4", "7")),
            ""
        );
    }

    #[test]
    fn a_size_for_a_person_is_gib_or_mib() {
        assert_eq!(container_size(8 * G), "8g");
        assert_eq!(container_size(512 * MIB), "512m");
    }
}
