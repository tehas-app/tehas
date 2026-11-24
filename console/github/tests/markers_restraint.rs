//! A restraining marker the writer puts in a text reads live in the whole
//! text it posts, or the write is refused: whatever the text before it
//! leaves open (a fence, an HTML block, a list item, a quote, a table, a
//! code span's backtick), a restraint is never posted where the reader,
//! and GitHub, would not see it. `write` and `edit` read the whole
//! composed text back and compare its markers with those asked.

use std::num::{NonZeroU32, NonZeroU64};
use tehas_github::markers::{edit, read, write};
use tehas_ports::{
    BlueprintNo, CommentId, FinishMarker, FinishResult, LastAttemptMarker, Marker, MarkerEdit,
    MillwrightAction, MillwrightMarker, ParkMarker, QuestionKind, Ref, Sha256, TicketKey,
};

fn item() -> Option<BlueprintNo> {
    BlueprintNo::new(7)
}

/// One marker of each kind that holds the factory back.
fn restraints() -> Vec<Marker> {
    let no = BlueprintNo::new(7).unwrap();
    let sig = Sha256::new("a".repeat(64)).unwrap();
    let mut last = LastAttemptMarker::new(sig.clone());
    last.note = "the last try".to_string();
    vec![
        Marker::Blocked {
            by: vec![Ref::Here(TicketKey::Number(NonZeroU64::new(3).unwrap()))],
        },
        Marker::Declined {
            question: CommentId::new("12"),
        },
        Marker::Park(ParkMarker::new(no, sig)),
        Marker::Finish(FinishMarker::new(no, FinishResult::Opened)),
        Marker::LastAttempt(last),
        Marker::Millwright(MillwrightMarker::new(no, MillwrightAction::Person)),
        Marker::Millwright(MillwrightMarker::new(no, MillwrightAction::Ask)),
        Marker::Question {
            kind: QuestionKind::Plan,
            blueprint: no,
            turn: NonZeroU32::new(1).unwrap(),
        },
    ]
}

/// Texts that end with something open, which may swallow what follows.
const OPEN_ENDINGS: &[&str] = &[
    "Said:\n```\nperson text",
    "~~~\nx",
    "<div>\nx",
    "<pre>\nx",
    "<script>\nx",
    "<!-- open",
    "- item",
    "1. item\n   more",
    "> quoted",
    "> quoted\nlazy",
    "| a | b |\n|---|---|\n| 1 | 2 |",
    "p\na | b\n-|-",
    "a ` b",
    "    indented",
];

/// The outcome of each write and edit of `m` after each text: posted with
/// the marker reading, or refused. A body posted with the marker dead
/// panics.
fn outcomes(m: &Marker) -> Vec<(&'static str, bool, bool)> {
    let mut out = Vec::new();
    for text in OPEN_ENDINGS {
        let written = write(text, std::slice::from_ref(m), item());
        if let Ok(body) = &written {
            assert!(
                read(body, item()).markers.contains(m),
                "{m:?} written dead after {text:?}: {body:?}"
            );
        }
        let asked = MarkerEdit {
            set: vec![m.clone()],
            ..Default::default()
        };
        let edited = edit(text, None, &asked, item());
        if let Ok(Some(body)) = &edited {
            assert!(
                read(body, item()).markers.contains(m),
                "{m:?} edited in dead after {text:?}: {body:?}"
            );
        }
        out.push((*text, written.is_ok(), matches!(edited, Ok(Some(_)))));
    }
    out
}

#[test]
fn a_restraint_is_posted_live_or_refused_whatever_the_text_leaves_open() {
    for m in restraints() {
        for (text, written, edited) in outcomes(&m) {
            eprintln!("{m:?}\t{text:?}\twrite {written}\tedit {edited}");
        }
    }
}

#[test]
fn a_restraint_after_an_unclosed_fence_is_refused() {
    // A marker after the text would be code to the text's end; a leading
    // marker stands before the text and reads. (In an open <pre> block a
    // marker is hidden HTML, and reads.)
    for m in restraints() {
        let leads = matches!(m, Marker::Finish(_) | Marker::Millwright(_));
        for (text, written, edited) in outcomes(&m) {
            if text.starts_with("Said:\n```") || text.starts_with("~~~") {
                assert_eq!(written, leads, "{m:?} after {text:?}");
                assert_eq!(edited, leads, "{m:?} after {text:?}");
            }
        }
    }
}

/// An edit's text cut never overlaps a marker: where a marker splits the
/// text, the text found once in the body may stand inside the marker.
#[test]
fn an_edit_whose_text_cut_overlaps_a_marker_is_refused() {
    let refused = Err(tehas_ports::Outcome::Refused(
        tehas_ports::Refusal::Unsupported,
    ));
    let none = MarkerEdit::default();

    let unknown = "a\n<!-- tehas:zzz a\nb -->\nb";
    assert_eq!(read(unknown, item()).text, "a\nb");
    assert_eq!(read(unknown, item()).markers, vec![Marker::Unknown]);
    assert_eq!(edit(unknown, Some("a\n b"), &none, item()), refused);

    let apart = "a\nb\n\n<!-- tehas:zzz c\nd -->";
    assert_eq!(read(apart, item()).markers, vec![Marker::Unknown]);
    assert_eq!(
        edit(apart, Some("a\n b"), &none, item()),
        Ok(Some("a\n b\n\n<!-- tehas:zzz c\nd -->".to_string()))
    );

    let sig = "a".repeat(64);
    let block = format!(
        "a\n<!-- tehas:last-attempt base={sig} -->\nxa\nby\n<!-- /tehas:last-attempt -->\nb"
    );
    let mut last = LastAttemptMarker::new(Sha256::new(sig).unwrap());
    last.note = "xa\nby".to_string();
    assert_eq!(read(&block, item()).text, "a\nb");
    assert_eq!(
        read(&block, item()).markers,
        vec![Marker::LastAttempt(last)]
    );
    assert_eq!(edit(&block, Some("a\n b"), &none, item()), refused);
}
