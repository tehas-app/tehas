//! The disk a cycle's scratch may take (#1547). With TEHAS_SCRATCH_DIR set, a
//! cycle's /tmp is a plain directory on the host's disk and nothing else bounds
//! what it writes there. The watch looks at it each tick and stops the cycle
//! when its scratch holds more than TEHAS_SCRATCH_MAX, or when the file system
//! of the scratch has less free than TEHAS_CYCLE_STOP_FREE and this cycle's
//! scratch is the largest there. Every cycle's watch makes the same choice, so
//! the cycle writing most stops and the others go on.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tehas_core::{call, disk};

/// Entries one measure reads. A tree bigger than this counts as `UNCOUNTED`.
const ENTRIES: usize = 500_000;

/// What a tree too big to read whole holds: more than any cap, and more than
/// any scratch that was read whole. A walk that stopped short cannot say what
/// the rest holds.
pub const UNCOUNTED: u64 = u64::MAX;

/// What the watch is given of the scratch.
#[derive(Clone, Debug, Default)]
pub struct Bounds {
    /// <TEHAS_SCRATCH_DIR>/<name>-<issue>, or None when /tmp is a tmpfs.
    pub dir: Option<PathBuf>,
    /// TEHAS_SCRATCH_MAX in bytes; 0 is no cap.
    pub max: u64,
    /// TEHAS_CYCLE_STOP_FREE in bytes; 0 is no floor.
    pub floor: u64,
}

impl Bounds {
    /// From the settings tehas_load gave: the sizes in MiB.
    pub fn from_env(issue: u64) -> Bounds {
        let var = |n: &str| std::env::var(n).unwrap_or_default();
        let mib = |n: &str| var(n).parse::<u64>().unwrap_or(0).saturating_mul(1 << 20);
        let root = var("TEHAS_SCRATCH_DIR");
        let work = var("TEHAS_WORK_DIR");
        let name = work.rsplit('/').next().unwrap_or("");
        let dir = (!root.is_empty()).then(|| Path::new(&root).join(format!("{name}-{issue}")));
        Bounds {
            dir,
            max: mib("TEHAS_SCRATCH_MAX_MB"),
            floor: mib("TEHAS_CYCLE_STOP_FREE_MB"),
        }
    }
}

/// Why a cycle is stopped for disk.
#[derive(Debug, PartialEq, Eq)]
pub enum Over {
    /// Its scratch holds more than the cap.
    Cap { held: u64, max: u64 },
    /// The disk is under the floor and this scratch is the largest on it.
    Floor { held: u64, free: u64, floor: u64 },
}

impl Over {
    /// The note the report reads: the word, the bytes held, the bound.
    pub fn note(&self) -> String {
        match self {
            Over::Cap { held, max } => format!("cap {held} {max}"),
            Over::Floor { held, free, floor } => format!("floor {held} {free} {floor}"),
        }
    }

    pub fn line(&self) -> String {
        match self {
            Over::Cap { held: UNCOUNTED, max } => format!(
                "tehas work: the cycle's scratch has more than {ENTRIES} entries, too many to measure against TEHAS_SCRATCH_MAX of {} MiB, stopping it",
                max >> 20
            ),
            Over::Cap { held, max } => format!(
                "tehas work: the cycle's scratch holds {} MiB, over TEHAS_SCRATCH_MAX of {} MiB, stopping it",
                held >> 20,
                max >> 20
            ),
            Over::Floor { held, free, floor } => format!(
                "tehas work: {} MiB free on the scratch's disk, under TEHAS_CYCLE_STOP_FREE of {} MiB, and this cycle's scratch is the largest there ({} MiB), stopping it",
                free >> 20,
                floor >> 20,
                held >> 20
            ),
        }
    }
}

/// The bytes a tree takes on disk: the blocks of every entry, links not
/// followed and no other file system entered. `UNCOUNTED` when it has more
/// than `ENTRIES` entries.
pub fn held(dir: &Path) -> u64 {
    held_within(dir, ENTRIES)
}

fn held_within(dir: &Path, bound: usize) -> u64 {
    let Ok(top) = fs::symlink_metadata(dir) else {
        return 0;
    };
    let dev = top.dev();
    let mut total = top.blocks() * 512;
    let mut todo = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    while let Some(d) = todo.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            seen += 1;
            if seen > bound {
                return UNCOUNTED;
            }
            let Ok(m) = e.path().symlink_metadata() else {
                continue;
            };
            if m.dev() != dev {
                continue;
            }
            total = total.saturating_add(m.blocks() * 512);
            if m.is_dir() {
                todo.push(e.path());
            }
        }
    }
    total
}

/// The free bytes of the file system of `path`, or None when df says nothing.
pub fn free(path: &Path) -> Option<u64> {
    let args = vec!["-Pk".to_string(), path.to_string_lossy().into_owned()];
    let ran = call::run("df", &args, Duration::from_secs(10));
    if !ran.ok() {
        return None;
    }
    disk::df_free(&ran.out).map(|kb| kb.saturating_mul(1024))
}

/// The largest scratch beside `own` in its directory, `own` left out.
pub fn largest_other(own: &Path) -> u64 {
    let (Some(root), Some(name)) = (own.parent(), own.file_name()) else {
        return 0;
    };
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.file_name() != name)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| held(&e.path()))
        .max()
        .unwrap_or(0)
}

/// The decision of one look. `free` and `other` are asked only when needed:
/// the free space when there is a floor, the other scratches when under it.
/// No figure of the free space stops nothing.
pub fn look(
    b: &Bounds,
    held: u64,
    free: impl FnOnce() -> Option<u64>,
    other: impl FnOnce() -> u64,
) -> Option<Over> {
    b.dir.as_ref()?;
    if b.max > 0 && held > b.max {
        return Some(Over::Cap { held, max: b.max });
    }
    if b.floor == 0 {
        return None;
    }
    let free = free()?;
    if free >= b.floor || held == 0 || held < other() {
        return None;
    }
    Some(Over::Floor {
        held,
        free,
        floor: b.floor,
    })
}

/// One look at the disk for the watch.
pub fn check(b: &Bounds) -> Option<Over> {
    let dir = b.dir.as_ref()?;
    let own = held(dir);
    look(b, own, || free(dir), || largest_other(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const G: u64 = 1 << 30;

    fn bounds(max: u64, floor: u64) -> Bounds {
        Bounds {
            dir: Some(PathBuf::from("/s/tehas-1")),
            max,
            floor,
        }
    }

    #[test]
    fn a_scratch_over_its_cap_stops() {
        let got = look(&bounds(20 * G, 0), 21 * G, || None, || 0);
        assert_eq!(
            got,
            Some(Over::Cap {
                held: 21 * G,
                max: 20 * G
            })
        );
        assert_eq!(look(&bounds(20 * G, 0), 20 * G, || None, || 0), None);
    }

    #[test]
    fn under_the_floor_the_largest_scratch_stops() {
        let b = bounds(0, 5 * G);
        let got = look(&b, 3 * G, || Some(G), || 2 * G);
        assert_eq!(
            got,
            Some(Over::Floor {
                held: 3 * G,
                free: G,
                floor: 5 * G
            })
        );
        // Another holds more: that one stops, not this one.
        assert_eq!(look(&b, 3 * G, || Some(G), || 4 * G), None);
        // An empty scratch did not fill the disk.
        assert_eq!(look(&b, 0, || Some(G), || 0), None);
        // Above the floor, or no figure of the free space: nothing stops.
        assert_eq!(look(&b, 3 * G, || Some(6 * G), || 0), None);
        assert_eq!(look(&b, 3 * G, || None, || 0), None);
    }

    #[test]
    fn no_scratch_directory_is_no_check() {
        let b = Bounds {
            dir: None,
            max: 1,
            floor: 1,
        };
        assert_eq!(look(&b, 99 * G, || Some(0), || 0), None);
    }

    #[test]
    fn the_notes_say_the_word_and_the_figures() {
        assert_eq!(Over::Cap { held: 3, max: 2 }.note(), "cap 3 2");
        let floor = Over::Floor {
            held: 3,
            free: 1,
            floor: 2,
        };
        assert_eq!(floor.note(), "floor 3 1 2");
    }

    #[test]
    fn held_counts_the_tree_and_the_largest_other_leaves_out_its_own() {
        let d = tehas_core::testdir::TestDir::new("scratch-held");
        let own = d.path().join("tehas-1");
        let other = d.path().join("tehas-2");
        fs::create_dir_all(own.join("a/b")).unwrap();
        fs::create_dir_all(&other).unwrap();
        let mut f = fs::File::create(own.join("a/b/big")).unwrap();
        f.write_all(&vec![1u8; 1 << 20]).unwrap();
        f.sync_all().unwrap();
        fs::write(other.join("small"), b"x").unwrap();
        std::os::unix::fs::symlink("/", own.join("root")).unwrap();
        let own_held = held(&own);
        assert!(own_held >= 1 << 20, "{own_held}");
        assert!(own_held < 2 << 20, "{own_held}");
        let other_held = largest_other(&own);
        assert!(other_held < own_held, "{other_held}");
        assert_eq!(largest_other(&other), own_held);
        assert_eq!(held(&d.path().join("none")), 0);
    }

    #[test]
    fn a_tree_too_big_to_read_whole_counts_as_over_every_cap() {
        let d = tehas_core::testdir::TestDir::new("scratch-uncounted");
        let own = d.path().join("tehas-1");
        fs::create_dir_all(own.join("many")).unwrap();
        for i in 0..4 {
            fs::write(own.join("many").join(i.to_string()), b"").unwrap();
        }
        fs::create_dir_all(own.join("many/sub")).unwrap();
        fs::write(own.join("many/sub/big"), vec![1u8; 1 << 20]).unwrap();
        assert_eq!(held_within(&own, 3), UNCOUNTED);
        assert!(held_within(&own, 100) < UNCOUNTED);

        let got = look(&bounds(20 * G, 0), UNCOUNTED, || None, || 0);
        assert_eq!(
            got,
            Some(Over::Cap {
                held: UNCOUNTED,
                max: 20 * G
            })
        );
        assert!(got.unwrap().line().contains("more than"));
        // Under the floor it is the largest: it stops, and the scratch beside
        // it that was read whole does not.
        let b = bounds(0, 5 * G);
        assert!(look(&b, UNCOUNTED, || Some(G), || 3 * G).is_some());
        assert_eq!(look(&b, 3 * G, || Some(G), || UNCOUNTED), None);
    }
}
