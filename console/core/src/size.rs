//! The measure of one issue body: the files it names, read off its own
//! text. The poll's hold on a wide issue reads it.
//!
//! A file is a distinct repository path written between two ticks that holds
//! a "/" or ends in .sh, .rs, .md, .json, .toml or .yml. A bare `tehas work`
//! and `notes.txt` are not paths. A path is a path wherever it is written,
//! inside a fenced block or not. A path that appears only on a line starting
//! with "Not ", "Do not " or "Leave " is not counted; the same path elsewhere
//! still is. A pair of ticks that straddles a line break counts for nothing.
//!
//! The measure is bounded by two settings, an instance's to set:
//! TEHAS_SIZE_MAX_BYTES and TEHAS_SIZE_MAX_LINES. Neither bound alone holds the
//! other's worst case, a body of one huge line and a body of many short ones.
//! The count is coarse, so a body past either is measured from its front:
//! the bytes first, then the lines. The byte cut can split one character, and
//! what cannot be decoded then is replaced; no path spans one.

use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

pub const MAX_BYTES: usize = 131_072;
pub const MAX_LINES: usize = 1000;

static REFUSED: LazyLock<Regex> = LazyLock::new(|| Regex::new("^(Not|Do not|Leave) ").unwrap());
static TICKS: LazyLock<Regex> = LazyLock::new(|| Regex::new("`([^`]+)`").unwrap());
static PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/|[.](sh|rs|md|json|toml|yml)$").unwrap());

pub fn files(body: &[u8], max_bytes: usize, max_lines: usize) -> usize {
    let cut = String::from_utf8_lossy(&body[..body.len().min(max_bytes)]);
    let mut paths = BTreeSet::new();
    for line in cut.split('\n').take(max_lines) {
        if !REFUSED.is_match(line) {
            paths.extend(
                TICKS
                    .captures_iter(line)
                    .map(|c| c.get(1).unwrap().as_str())
                    .filter(|p| PATH.is_match(p)),
            );
        }
    }
    paths.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(body: &str) -> usize {
        files(body.as_bytes(), MAX_BYTES, MAX_LINES)
    }

    #[test]
    fn a_path_in_a_fenced_block_is_still_a_path() {
        assert_eq!(of("```\n`test/a.sh`\n```\n`b/c`\n"), 2);
    }

    #[test]
    fn ticks_that_straddle_a_line_count_for_nothing() {
        assert_eq!(of("`a/b\nc/d`\n"), 0);
    }

    #[test]
    fn a_pair_of_ticks_is_read_from_the_left() {
        assert_eq!(of("`a` b/c `d/e`\n"), 1);
    }

    #[test]
    fn a_body_is_cut_on_bytes_before_lines() {
        assert_eq!(files(b"`a/b`\n`c/d`\n`e/f`", 131072, 2), 2);
        assert_eq!(files(b"`a/b` `c/d`", 6, 1000), 1);
        assert_eq!(files(b"`a/b`\n`c/d`\n", 7, 1), 1);
    }

    /// A person who pastes a dump into an issue body must not slow the poll
    /// that measures it. Moved from console/tests/size.rs.
    #[test]
    fn a_body_of_forty_thousand_lines_is_measured_from_its_front_in_good_time() {
        let body: String = (1..=40_000).map(|i| format!("`test/f{i}.sh`\n")).collect();
        let begun = std::time::Instant::now();
        assert_eq!(files(body.as_bytes(), MAX_BYTES, MAX_LINES), 1000);
        assert!(
            begun.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            begun.elapsed()
        );
    }

    #[test]
    fn bytes_that_are_no_text_are_measured_all_the_same() {
        assert_eq!(files(b"`a/\xff` `b/c`\n", 131072, 1000), 2);
    }

    #[test]
    fn a_body_of_nothing_names_no_file() {
        assert_eq!(of(""), 0);
    }

    // Moved from test/sizing_test.sh.

    #[test]
    fn a_narrow_issue_names_two_files() {
        let body = "Give `lib/sizing.sh` one function.

## The change

1. `lib/sizing.sh` holds tehas_issue_size.
2. the gate lints it.

## Done when

- `test/sizing_test.sh` has a case for it.
";
        assert_eq!(of(body), 2);
    }

    #[test]
    fn a_wide_issue_names_six_files() {
        let body = "Size an issue from its body.

## What to change

1. `lib/sizing.sh` with tehas_issue_size.
2. `test/sizing_test.sh` with the cases.
3. `docs/specs/sizing.md` says the rules, and the manual says so too.
4. `console/ops/src/size.rs` is for a later issue.

Not `test/work_test.sh`.

## Done when

- `test/sizing_test.sh` runs.
- `test/limit_test.sh` still passes.
- `scripts/verify.sh` passes, and the gate lints it.
";
        assert_eq!(of(body), 6);
    }

    #[test]
    fn a_path_named_only_on_a_not_do_not_or_leave_line_is_not_counted() {
        let body = "Touch `lib/a.sh`.

Not `test/work_test.sh`
Do not touch `lib/b.sh`
Leave `instances/example/instance.env` alone
";
        assert_eq!(of(body), 1);
    }

    #[test]
    fn a_path_refused_on_one_line_and_asked_for_on_another_counts_once() {
        let body = "Not `test/work_test.sh`

1. `test/work_test.sh` stays as it is and `lib/a.sh` changes.
2. `lib/a.sh` is the only file written.
";
        assert_eq!(of(body), 2);
    }

    #[test]
    fn duplicates_count_once_whatever_the_line() {
        let body = "`lib/a.sh`, `lib/a.sh` and again `lib/a.sh`.

`bin/tehas` too, and `bin/tehas`.
";
        assert_eq!(of(body), 2);
    }

    #[test]
    fn no_slash_and_no_known_extension_or_no_ticks_is_no_path() {
        let body = "Run tehas work on notes.txt, and read README without the extension.

`tehas work` `notes.txt` `a.SH` `libexec/work`
";
        assert_eq!(of(body), 1);
    }

    #[test]
    fn every_extension_the_measure_knows_counts_without_a_slash() {
        let body = "`a.sh` `b.rs` `c.md` `d.json` `e.toml` `f.yml` `g.txt` `h.py`\n";
        assert_eq!(of(body), 6);
    }
}
