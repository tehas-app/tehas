//! One clock for the workspace: no program reads `SystemTime` for the time,
//! it asks `tehas_core::time`. The guard test reads the code a host runs in
//! every crate and allows only the reads named below.

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

/// The crates the guard reads. The clock itself, `core/src/time.rs`, is
/// left out: it is the one place that may read the host's clock.
const DIRS: [&str; 12] = [
    "src",
    "analytics/src",
    "attach/src",
    "contractor/src",
    "core/src",
    "cycle/src",
    "dispatch/src",
    "mcp/src",
    "ops/src",
    "proxy/src",
    "store/src",
    "xtask/src",
];

/// The reads that stay. Each is one `SystemTime::now` of one file, named by
/// what it reads the clock for rather than by a line, so a read that moves in
/// the file is not a new one. A seed reads nanoseconds of the host's clock to
/// tell two names apart, and an mtime age compares a file's time with the
/// host's clock: no second of the workspace's clock answers either.
const EXEMPT: [(&str, &str); 10] = [
    ("core/src/file.rs", "seed of a temporary's name"),
    ("cycle/src/after.rs", "seed of a temporary's name"),
    ("cycle/src/start.rs", "seed of a temporary's name"),
    (
        "dispatch/src/apply/start.rs",
        "seed of a scratch file's name",
    ),
    ("ops/src/clean.rs", "mtime age"),
    ("ops/src/sandbox.rs", "seed of a temporary's name"),
    (
        "ops/src/update/mod.rs",
        "seed of a temporary directory's name",
    ),
    ("store/src/upkeep.rs", "mtime age"),
    ("xtask/src/pack.rs", "seed of a scratch name"),
    ("xtask/src/run.rs", "seed of a scratch name"),
];

/// The code a host runs: the file up to its inline test module. A test may
/// hold a clock of its own.
fn host_code(text: &str) -> &str {
    text.find("#[cfg(test)]\nmod tests {")
        .map_or(text, |at| &text[..at])
}

#[test]
fn the_workspace_reads_the_time_only_through_core() {
    let mut found = Vec::new();
    let mut exempt = Vec::new();
    for (rel, path) in DIRS.iter().flat_map(|d| sources(d)) {
        if rel == "core/src/time.rs" {
            continue;
        }
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        // A test's helper may hold a clock of its own; the code a host runs
        // may not.
        if name.ends_with("_tests.rs") || name == "tests.rs" || rel.contains("/tests/") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let text = host_code(&text);
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
    // Each crate exists, so a path moved out of one would fail above for the
    // wrong reason: it would be no source at all.
    for dir in DIRS {
        assert!(!sources(dir).is_empty(), "console/{dir} holds no source");
    }
    for (rel, _) in EXEMPT {
        assert!(source(rel).is_file(), "console/{rel} is not there");
    }
}

#[test]
fn the_host_code_stops_at_the_inline_test_module() {
    let text = "fn a() {}\n#[cfg(test)]\nmod tests {\n    SystemTime::now();\n}\n";
    assert_eq!(host_code(text), "fn a() {}\n");
    // A test module in a file of its own leaves the file whole.
    let text = "#[cfg(test)]\nmod tests;\nfn b() { SystemTime::now(); }\n";
    assert_eq!(host_code(text), text);
}

#[test]
fn the_civil_calendar_is_written_once() {
    // 719 468 is the day count from 0000-03-01 to the epoch in Howard
    // Hinnant's algorithms: a file that spells it holds a calendar of its own.
    let mut found = Vec::new();
    for (rel, path) in DIRS.iter().flat_map(|d| sources(d)) {
        if rel == "core/src/time.rs" {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
        if text.contains("719_468") || text.contains("719468") {
            found.push(rel);
        }
    }
    assert!(found.is_empty(), "a calendar of its own: {found:?}");
}
