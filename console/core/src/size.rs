//! The measure of one issue body: four counts, read off its own text. `tehas
//! work` measures the body of an approved start and keeps the answer in
//! state/sizes.jsonl (console/src/size.rs). The poll's hold on a wide issue
//! reads the files count.
//!
//! The four counts:
//!
//!   files         distinct repository paths written between two ticks that
//!                 hold a "/" or end in .sh, .rs, .md, .json, .toml or .yml. A
//!                 bare `tehas work` and `notes.txt` are not paths. A path is a
//!                 path wherever it is written, inside a fenced block or not.
//!                 A path that appears only on a line starting with "Not ",
//!                 "Do not " or "Leave " is not counted; the same path
//!                 elsewhere still is. A pair of ticks that straddles a line
//!                 break counts for nothing.
//!   tests         the paths of files that start with test/
//!   deliverables  the lines of an ordered list, "1. ", "2. " and so on, under
//!                 a heading whose text is "The change", "Deliverable(s)" or
//!                 "What to change", up to the next heading of any level
//!   done_when     the items of an unordered list, "- " or "* ", under a
//!                 heading whose text is "Done when", up to the next heading
//!                 of any level
//!
//! A heading is a line that begins with 1 to 6 # and is read as one: not a
//! line at or after a body's first fence, and not one indented four spaces or
//! more, which markdown reads as code. The first fence ends the reading for
//! good: a fence that closes the block does not start it again. After it a
//! path is still a path, and a numbered line is prose.
//!
//! The headings are matched whole, so "Changes to make" and "Files" are
//! neither. A heading governs its own line and every line after it, up to the
//! next heading of any level and the end of the body. What stands before the
//! first heading belongs to no section.
//!
//! The measure is bounded by two settings, an instance's to set:
//! TEHAS_SIZE_MAX_BYTES and TEHAS_SIZE_MAX_LINES. Neither bound alone holds the
//! other's worst case, a body of one huge line and a body of many short ones.
//! The counts are coarse, so a body past either is measured from its front:
//! the bytes first, then the lines. The byte cut can split one character, and
//! what cannot be decoded then is replaced; no path, list item or heading
//! spans one.

use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

pub const MAX_BYTES: usize = 131_072;
pub const MAX_LINES: usize = 1000;

static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new("^#{1,6}[ \t]*[^ \t]").unwrap());
static CHANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^#{1,6}[ \t]*(?i)(the )?(change|deliverables?|what to change)[ \t]*$").unwrap()
});
static DONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^#{1,6}[ \t]*(?i)done when[ \t]*$").unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[ \t]{0,3}(```|~~~)").unwrap());
static REFUSED: LazyLock<Regex> = LazyLock::new(|| Regex::new("^(Not|Do not|Leave) ").unwrap());
static TICKS: LazyLock<Regex> = LazyLock::new(|| Regex::new("`([^`]+)`").unwrap());
static PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/|[.](sh|rs|md|json|toml|yml)$").unwrap());
static NUMBERED: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[0-9]+[.] ").unwrap());
static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[-*] ").unwrap());

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub files: usize,
    pub tests: usize,
    pub deliverables: usize,
    pub done_when: usize,
}

impl Size {
    /// The keys without their braces, always in this order: state/sizes.jsonl
    /// and `console size measure` both pin it.
    pub fn fields(&self) -> String {
        format!(
            r#""files":{},"tests":{},"deliverables":{},"done_when":{}"#,
            self.files, self.tests, self.deliverables, self.done_when
        )
    }
}

pub fn measure(body: &[u8], max_bytes: usize, max_lines: usize) -> Size {
    let cut = String::from_utf8_lossy(&body[..body.len().min(max_bytes)]);
    let mut size = Size::default();
    let mut paths = BTreeSet::new();
    let (mut fenced, mut change, mut done) = (false, false, false);
    for line in cut.split('\n').take(max_lines) {
        let fence = FENCE.is_match(line);
        let run = !fenced && !fence && !line.starts_with("    ");
        fenced |= fence;
        if run && HEADING.is_match(line) {
            change = CHANGE.is_match(line);
            done = DONE.is_match(line);
        }
        if run && change && NUMBERED.is_match(line) {
            size.deliverables += 1;
        }
        if run && done && BULLET.is_match(line) {
            size.done_when += 1;
        }
        if !REFUSED.is_match(line) {
            paths.extend(
                TICKS
                    .captures_iter(line)
                    .map(|c| c.get(1).unwrap().as_str())
                    .filter(|p| PATH.is_match(p))
                    .map(str::to_string),
            );
        }
    }
    size.files = paths.len();
    size.tests = paths.iter().filter(|p| p.starts_with("test/")).count();
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(body: &str) -> Size {
        measure(body.as_bytes(), MAX_BYTES, MAX_LINES)
    }

    fn size(files: usize, tests: usize, deliverables: usize, done_when: usize) -> Size {
        Size {
            files,
            tests,
            deliverables,
            done_when,
        }
    }

    #[test]
    fn the_keys_stand_in_the_order_the_ledger_pins() {
        assert_eq!(
            size(1, 2, 3, 4).fields(),
            r#""files":1,"tests":2,"deliverables":3,"done_when":4"#
        );
    }

    #[test]
    fn a_heading_begins_with_one_to_six_hashes_and_holds_text() {
        assert_eq!(of("# Done when\n- a\n"), size(0, 0, 0, 1));
        assert_eq!(of("#Done when\n- a\n"), size(0, 0, 0, 1));
        assert_eq!(of("###### Done when\n- a\n"), size(0, 0, 0, 1));
        assert_eq!(of("####### Done when\n- a\n"), size(0, 0, 0, 0));
        // Seven hashes are no section of ours, but they end the one before.
        assert_eq!(of("# Done when\n####### x\n- a\n"), size(0, 0, 0, 0));
        // A line of nothing but a hash and spaces ends no section.
        assert_eq!(of("# Done when\n# \n- a\n"), size(0, 0, 0, 1));
    }

    #[test]
    fn a_line_end_of_a_carriage_return_is_no_white_space_of_a_heading() {
        assert_eq!(of("## Done when\r\n- a\r\n"), size(0, 0, 0, 0));
        assert_eq!(of("## Done when \t\n- a\n"), size(0, 0, 0, 1));
    }

    #[test]
    fn a_heading_indented_four_spaces_is_code_and_three_are_not_a_heading_either() {
        assert_eq!(of("    ## Done when\n- a\n"), size(0, 0, 0, 0));
        assert_eq!(of("   ## Done when\n- a\n"), size(0, 0, 0, 0));
        // A tab is no four spaces: the line runs, and a list item is not one.
        assert_eq!(of("## Done when\n\t- a\n    - b\n"), size(0, 0, 0, 0));
    }

    #[test]
    fn the_first_fence_ends_the_reading_of_headings_and_lists_for_good() {
        assert_eq!(
            of("## Done when\n- a\n```\n- b\n```\n- c\n## The change\n1. d\n"),
            size(0, 0, 0, 1)
        );
        assert_eq!(of("~~~\n~~~\n## Done when\n- a\n"), size(0, 0, 0, 0));
        assert_eq!(of("   ```\n## Done when\n- a\n"), size(0, 0, 0, 0));
        assert_eq!(of("\t\t\t```\n## Done when\n- a\n"), size(0, 0, 0, 0));
    }

    #[test]
    fn a_fence_indented_four_is_no_fence() {
        assert_eq!(of("    ```\n## Done when\n- a\n"), size(0, 0, 0, 1));
    }

    #[test]
    fn a_path_in_a_fenced_block_is_still_a_path() {
        assert_eq!(of("```\n`test/a.sh`\n```\n`b/c`\n"), size(2, 1, 0, 0));
    }

    #[test]
    fn ticks_that_straddle_a_line_count_for_nothing() {
        assert_eq!(of("`a/b\nc/d`\n"), size(0, 0, 0, 0));
    }

    #[test]
    fn a_pair_of_ticks_is_read_from_the_left() {
        assert_eq!(of("`a` b/c `d/e`\n"), size(1, 0, 0, 0));
    }

    #[test]
    fn a_body_is_cut_on_bytes_before_lines() {
        assert_eq!(measure(b"`a/b`\n`c/d`\n`e/f`", 131072, 2), size(2, 0, 0, 0));
        assert_eq!(measure(b"`a/b` `c/d`", 6, 1000), size(1, 0, 0, 0));
        assert_eq!(measure(b"`a/b`\n`c/d`\n", 7, 1), size(1, 0, 0, 0));
    }

    #[test]
    fn bytes_that_are_no_text_are_measured_all_the_same() {
        assert_eq!(measure(b"`a/\xff` `b/c`\n", 131072, 1000).files, 2);
    }

    #[test]
    fn a_body_of_nothing_is_four_zeros() {
        assert_eq!(of(""), Size::default());
    }
}
