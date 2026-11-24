//! Where a marker is text, as GitHub renders it: a quote's lazy
//! continuation and an indented code block are text, so a marker there does
//! not read. A line indented under a paragraph continues it, and a list
//! item's indented line is the item's text: a marker there reads. A code
//! span across line ends holds its marker as code.

use std::num::NonZeroU32;
use tehas_github::markers::{read, write};
use tehas_ports::{BlueprintNo, FinishMarker, FinishResult, Marker, QuestionKind};

const M: &str = "<!-- tehas:finish issue=7 result=opened -->";

fn item() -> Option<BlueprintNo> {
    BlueprintNo::new(7)
}

fn finish() -> Marker {
    Marker::Finish(FinishMarker::new(
        BlueprintNo::new(7).unwrap(),
        FinishResult::Opened,
    ))
}

fn markers(text: &str) -> Vec<Marker> {
    read(text, item()).markers
}

#[test]
fn a_marker_on_a_line_that_lazily_continues_a_quote_is_text() {
    assert!(markers(&format!("> quoted\nlazy {M}")).is_empty());
    // The lazy paragraph runs on to its next line too.
    assert!(markers(&format!("> quoted\nlazy\nmore {M}")).is_empty());
}

#[test]
fn a_blank_line_ends_the_quote_and_the_marker_after_it_reads() {
    assert_eq!(markers(&format!("> quoted\n\n{M}")), [finish()]);
}

#[test]
fn a_marker_line_right_after_a_quote_starts_a_block_of_its_own_and_reads() {
    // A `<!--` at a line's start opens an HTML block, which ends the
    // quote's paragraph rather than continuing it.
    assert_eq!(markers(&format!("> quoted\n{M}")), [finish()]);
}

#[test]
fn a_marker_in_an_indented_code_block_after_a_blank_line_is_text() {
    assert!(markers(&format!("para\n\n    {M}")).is_empty());
    assert!(markers(&format!("para\n\n\t{M}")).is_empty());
    // At the text's start, and on the block's later lines.
    assert!(markers(&format!("    {M}")).is_empty());
    assert!(markers(&format!("para\n\n    code\n\n    {M}")).is_empty());
}

#[test]
fn a_line_indented_under_a_paragraph_continues_it_and_its_marker_reads() {
    // CommonMark: an indented code block cannot interrupt a paragraph.
    assert_eq!(markers(&format!("para\n    {M}")), [finish()]);
}

#[test]
fn an_indented_line_in_a_list_item_is_the_item_s_text_as_before() {
    assert_eq!(markers(&format!("- item\n\n    {M}")), [finish()]);
    // A paragraph that ends the list makes the next indented line code.
    assert!(markers(&format!("- item\n\npara\n\n    {M}")).is_empty());
}

#[test]
fn a_code_span_across_line_ends_holds_its_marker_as_code() {
    // CommonMark lets a code span cross the line ends of its paragraph, and
    // GitHub shows the marker in it as code.
    assert!(markers(&format!("a `x\nb {M} `")).is_empty());
    // A blank line ends the paragraph, and the backtick opens nothing.
    assert_eq!(markers(&format!("a `x\n\nb {M} `")), [finish()]);
}

#[test]
fn a_text_ending_in_a_quote_reads_back_with_its_markers_after_it() {
    let q = Marker::Question {
        kind: QuestionKind::Park,
        blueprint: BlueprintNo::new(7).unwrap(),
        turn: NonZeroU32::new(1).unwrap(),
    };
    let body = write("Said:\n> quoted", &[Marker::HoldCleared, q.clone()], item()).unwrap();
    assert_eq!(
        body,
        "Said:\n> quoted\n\n<!-- tehas:hold-cleared -->\n<!-- tehas:question kind=park issue=7 turn=1 -->"
    );
    assert_eq!(markers(&body), [Marker::HoldCleared, q]);
}

#[test]
fn a_first_line_marker_before_an_indented_text_reads_back() {
    let body = write("    indented first line\nmore", &[finish()], item()).unwrap();
    assert_eq!(body, format!("{M}\n    indented first line\nmore"));
    let back = read(&body, item());
    assert_eq!(back.markers, [finish()]);
    assert_eq!(back.text, "    indented first line\nmore");
}
