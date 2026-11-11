//! One clock for the dispatcher and the store: neither program reads
//! `SystemTime` for the time, it asks `tehas_core::time`. The guard test reads
//! the two crates' sources and allows only the reads named below.

use std::fs;
use std::path::{Path, PathBuf};

/// A source file of the workspace: console/, then the path under it.
fn source(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Every `.rs` under `dir`, as `(path relative to console/, file name)`.
fn sources(dir: &str) -> Vec<(String, PathBuf)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(root, &path, out);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push((rel.to_string_lossy().into_owned(), path));
        }
    }
    let root = source("");
    let mut out = Vec::new();
    walk(&root, &root.join(dir), &mut out);
    out.sort();
    out
}

/// The reads that stay. Each is one `SystemTime::now` of one file, named by
/// what it reads the clock for rather than by a line, so a read that moves in
/// the file is not a new one.
const EXEMPT: [(&str, &str); 2] = [
    // The tag of a scratch file's name: nanoseconds of the host's clock, no
    // second of it, so no clock of the workspace answers the question.
    ("dispatch/src/apply/start.rs", "subsec_nanos"),
    // Whether a file was touched in the last minute: an mtime read against
    // the host's clock, which is what the check asks.
    ("store/src/upkeep.rs", "mtime-age"),
];

#[test]
fn the_dispatch_and_the_store_read_the_time_only_through_core() {
    let mut found = Vec::new();
    let mut exempt = Vec::new();
    for (rel, path) in sources("dispatch/src")
        .into_iter()
        .chain(sources("store/src"))
    {
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        // A test's helper may hold a clock of its own; the code a host runs
        // may not.
        if name.ends_with("_tests.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
        // `std::time::now` is not spelled out in the sources, so it is the
        // shape a re-introduced private clock of a file would take: `use
        // std::time::now as now` answers the same question `SystemTime::now`
        // does and would leave a scan for that name alone.
        let hits = text.matches("SystemTime::now").count() + text.matches("std::time::now").count();
        let wanted = EXEMPT.iter().filter(|(file, _)| *file == rel).count();
        if hits == wanted {
            exempt.extend(
                EXEMPT
                    .iter()
                    .filter(|(file, _)| *file == rel)
                    .map(|(f, w)| (*f, *w)),
            );
            continue;
        }
        found.push(format!(
            "{rel} reads SystemTime::now {hits} times, expected {wanted}"
        ));
    }
    // Every exemption is still there: an exemption nobody needs is a line this
    // test would let through for nothing.
    assert_eq!(
        exempt.len(),
        EXEMPT.len(),
        "an exempt read is no longer in the sources it names: {exempt:?}"
    );
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_guard_reads_the_sources_it_claims_to() {
    // The two crates exist, so a path moved out of either would fail above for
    // the wrong reason: it would be no source at all.
    for dir in ["dispatch/src", "store/src"] {
        assert!(!sources(dir).is_empty(), "console/{dir} holds no source");
    }
    for (rel, _) in EXEMPT {
        assert!(source(rel).is_file(), "console/{rel} is not there");
    }
}
