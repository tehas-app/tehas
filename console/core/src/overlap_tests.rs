//! The matcher of the paths an issue's body names (crate::overlap).

use crate::overlap::{matches, named};

const BODY: &str = "## Deliverables\n\n\
1. A matcher. [paths: `console/core/src/overlap.rs` (new), `console/core/src/lib.rs`]\n\
2. The glue. [paths: `libexec/dispatch`, `lib/`]\n";

#[test]
fn a_named_file_that_a_branch_changes_matches() {
    let changed = ["README.md", "libexec/dispatch", "test/x_test.sh"];
    assert_eq!(matches(BODY, &changed), vec!["libexec/dispatch"]);
}

#[test]
fn a_named_file_matches_only_by_equality() {
    // A file whose name holds the named one, or the other way round, is
    // another file.
    let changed = [
        "libexec/dispatch.bak",
        "libexec/dis",
        "console/core/src/lib.rsx",
    ];
    assert!(matches(BODY, &changed).is_empty());
}

#[test]
fn a_named_directory_covers_a_file_under_it() {
    let changed = ["lib/config.sh"];
    assert_eq!(matches(BODY, &changed), vec!["lib/"]);
    // A file beside the directory, with the same start of a name, is not under it.
    assert!(matches(BODY, &["library/x.sh", "lib"]).is_empty());
}

#[test]
fn the_new_marker_is_ignored() {
    let changed = ["console/core/src/overlap.rs"];
    assert_eq!(matches(BODY, &changed), vec!["console/core/src/overlap.rs"]);
    // Inside the backquotes too, and without backquotes at all.
    let body = "[paths: `a/b.rs (new)`, c/d.rs (new) ,  e.rs ]";
    assert_eq!(named(body), vec!["a/b.rs", "c/d.rs", "e.rs"]);
}

#[test]
fn a_body_with_no_paths_list_matches_nothing() {
    let body = "Change `libexec/dispatch` and `lib/config.sh`.";
    assert!(named(body).is_empty());
    assert!(matches(body, &["libexec/dispatch", "lib/config.sh"]).is_empty());
    assert!(matches("", &["libexec/dispatch"]).is_empty());
}

#[test]
fn the_matches_come_once_in_the_order_the_body_names_them() {
    let body = "[paths: `b`, `a`] and again [paths: `a`, `lib/`]";
    let changed = ["a", "lib/x", "lib/y", "b"];
    assert_eq!(matches(body, &changed), vec!["b", "a", "lib/"]);
}

#[test]
fn changed_lines_are_trimmed_and_blank_ones_are_no_file() {
    let changed = ["  libexec/dispatch \r", "", "   "];
    assert_eq!(matches(BODY, &changed), vec!["libexec/dispatch"]);
    // A blank changed line never makes a named directory match.
    assert!(matches("[paths: `/`]", &[""]).is_empty());
}

#[test]
fn a_comma_or_bracket_between_backquotes_belongs_to_the_path() {
    let body = "[paths: `odd,name]`, `x`] after";
    assert_eq!(named(body), vec!["odd,name]", "x"]);
}

#[test]
fn a_list_that_never_closes_reads_to_the_end() {
    assert_eq!(named("[paths: `a`, `b`"), vec!["a", "b"]);
    assert!(named("[paths:").is_empty());
}
