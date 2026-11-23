//! tehas's markers as the adapter reads them from a text of GitHub's: each
//! edge of the ports spec's "What is a marker", one case an edge, and the
//! rows of `host/markers.tsv` the reading goes by.

use std::num::{NonZeroU32, NonZeroU64};
use tehas_github::markers::read;
use tehas_github::table;
use tehas_ports::{
    BlueprintNo, FinishMarker, FinishResult, KeyText, LastAttemptMarker, Marker, MarkerKind,
    MarkerWord, MillwrightAction, MillwrightMarker, ParkMarker, QuestionKind, Ref, Sha256,
    TicketKey,
};

fn item() -> Option<BlueprintNo> {
    BlueprintNo::new(7)
}

fn ticket(n: u64) -> Ref<TicketKey> {
    Ref::Here(TicketKey::Number(NonZeroU64::new(n).unwrap()))
}

fn blocked(by: &[u64]) -> Marker {
    Marker::Blocked {
        by: by.iter().map(|n| ticket(*n)).collect(),
    }
}

fn unreadable(kind: MarkerKind) -> Marker {
    Marker::Unreadable { kind }
}

fn markers(text: &str) -> Vec<Marker> {
    read(text, item()).markers
}

fn sha(c: char) -> String {
    c.to_string().repeat(64)
}

#[test]
fn a_text_with_no_marker_is_the_text_as_it_came() {
    for text in [
        "",
        "Plain.\n",
        "  Two\r\n\r\nlines \n\n",
        "<!-- a note -->\n",
    ] {
        let got = read(text, item());
        assert_eq!(got.text, text);
        assert!(got.markers.is_empty(), "{text:?}");
    }
}

#[test]
fn a_wrapped_marker_is_taken_out_of_the_text_and_read() {
    let got = read("Do it.\n\n<!-- tehas:blocked by=3,12 -->\n", item());
    assert_eq!(got.text, "Do it.");
    assert_eq!(got.markers, [blocked(&[3, 12])]);
    // First in its text, as a question's comment has it, and in a line.
    let got = read("<!-- tehas:hold-cleared -->\n\nGo on.\n", item());
    assert_eq!(got.text, "Go on.");
    assert_eq!(got.markers, [Marker::HoldCleared]);
    let got = read(
        "Wait <!-- tehas:blocked by=3 --> here.\r\nMore.\r\n",
        item(),
    );
    assert_eq!(got.text, "Wait  here.\r\nMore.");
    assert_eq!(got.markers, [blocked(&[3])]);
}

#[test]
fn every_row_of_the_table_reads_as_its_kind() {
    let no = BlueprintNo::new(7).unwrap();
    let word = |w: &str| MarkerWord::new(w).unwrap();
    let cases = [
        (
            "<!-- tehas:question kind=plan issue=7 turn=2 -->".to_string(),
            Marker::Question {
                kind: QuestionKind::Plan,
                blueprint: no,
                turn: NonZeroU32::new(2).unwrap(),
            },
        ),
        ("<!-- tehas:blocked by=3 -->".to_string(), blocked(&[3])),
        ("<!-- tehas:hold-cleared -->".to_string(), Marker::HoldCleared),
        (
            format!(
                "<!-- tehas:park issue=7 step=develop kind= provider=1 sig={} -->",
                sha('a')
            ),
            Marker::Park(ParkMarker {
                blueprint: no,
                step: word("develop"),
                kind: word(""),
                provider: true,
                sig: Sha256::new(sha('a')).unwrap(),
            }),
        ),
        (
            "<!-- tehas:finish issue=7 result=opened -->".to_string(),
            Marker::Finish(FinishMarker {
                blueprint: no,
                result: FinishResult::Opened,
                by_poll: false,
            }),
        ),
        (
            "<!-- tehas:finish issue=7 result=stopped by=poll -->".to_string(),
            Marker::Finish(FinishMarker {
                blueprint: no,
                result: FinishResult::Stopped,
                by_poll: true,
            }),
        ),
        (
            format!(
                "<!-- tehas:last-attempt base={} -->\nIt ran out\nof time.\n<!-- /tehas:last-attempt -->",
                sha('b')
            ),
            Marker::LastAttempt(LastAttemptMarker {
                base: Sha256::new(sha('b')).unwrap(),
                note: "It ran out\nof time.".to_string(),
            }),
        ),
        (
            "<!-- tehas:millwright issue=7 action=retry -->".to_string(),
            Marker::Millwright(MillwrightMarker {
                blueprint: no,
                action: MillwrightAction::Retry,
            }),
        ),
        (
            "<!-- tehas:ticket-of ticket=41 -->".to_string(),
            Marker::TicketOf(ticket(41)),
        ),
        (
            "<!-- tehas:ticket-of ticket=PROJ-41 -->".to_string(),
            Marker::TicketOf(Ref::Here(TicketKey::Key(KeyText::new("PROJ-41").unwrap()))),
        ),
        (
            "<!-- tehas:link blueprint=9 -->".to_string(),
            Marker::Link {
                blueprint: BlueprintNo::new(9).unwrap(),
                remove: false,
            },
        ),
        (
            "<!-- tehas:link blueprint=9 remove=1 -->".to_string(),
            Marker::Link {
                blueprint: BlueprintNo::new(9).unwrap(),
                remove: true,
            },
        ),
    ];
    let mut kinds: Vec<MarkerKind> = Vec::new();
    for (text, want) in &cases {
        let got = read(&format!("Before.\n{text}\nAfter."), item());
        assert_eq!(got.markers, std::slice::from_ref(want), "{text}");
        assert_eq!(got.text, "Before.\nAfter.", "{text}");
        if !kinds.contains(&want.kind()) {
            kinds.push(want.kind());
        }
    }
    let rows: Vec<MarkerKind> = table::marker_rows().into_iter().map(|r| r.kind).collect();
    assert_eq!(kinds, rows, "a case for every row, in the table's order");
}

#[test]
fn the_rows_are_host_markers_tsv() {
    let file = include_str!("../../../host/markers.tsv");
    let lines: Vec<&str> = file
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let rows = table::marker_rows();
    assert_eq!(rows.len(), lines.len(), "a line was not read whole");
    assert_eq!(rows.len(), 9);
    for (row, line) in rows.iter().zip(&lines) {
        let cols: Vec<&str> = line.split('\t').collect();
        assert!(
            cols[1].starts_with(&format!("<!-- tehas:{}", row.word)),
            "{line}"
        );
        assert_eq!(row.close.as_deref().unwrap_or_default(), cols[2], "{line}");
        for f in &row.fields {
            assert!(cols[3].contains(&f.name), "{line}");
        }
    }
    assert!(rows.iter().filter(|r| r.close.is_some()).count() == 1);
}

#[test]
fn a_badly_wrapped_marker_is_unreadable_of_its_kind_and_taken_out() {
    let many: Vec<String> = (1..=21).map(|n| n.to_string()).collect();
    let cases = [
        // A field that does not parse.
        (
            "<!-- tehas:blocked by=x -->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!-- tehas:blocked by=3, 4 -->".to_string(),
            MarkerKind::Blocked,
        ),
        // Leading zeros.
        (
            "<!-- tehas:blocked by=03 -->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!-- tehas:question kind=plan issue=07 turn=1 -->".to_string(),
            MarkerKind::Question,
        ),
        // A list over its limit, and a number over its digits.
        (
            format!("<!-- tehas:blocked by={} -->", many.join(",")),
            MarkerKind::Blocked,
        ),
        (
            "<!-- tehas:blocked by=1234567890 -->".to_string(),
            MarkerKind::Blocked,
        ),
        // A field missing, repeated, out of order or not of the row.
        ("<!-- tehas:blocked -->".to_string(), MarkerKind::Blocked),
        (
            "<!-- tehas:blocked by=3 by=4 -->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!-- tehas:question issue=7 kind=plan turn=1 -->".to_string(),
            MarkerKind::Question,
        ),
        (
            "<!-- tehas:millwright issue=7 action=retry asked=1 -->".to_string(),
            MarkerKind::Millwright,
        ),
        // A value outside its set, and a turn the ports' type does not take.
        (
            "<!-- tehas:millwright issue=7 action=merge -->".to_string(),
            MarkerKind::Millwright,
        ),
        (
            "<!-- tehas:question kind=plan issue=7 turn=0 -->".to_string(),
            MarkerKind::Question,
        ),
        // Not the row's form to the letter.
        (
            "<!-- tehas:blocked  by=3 -->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!--tehas:blocked by=3 -->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!-- tehas:blocked by=3-->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!-- TEHAS:Blocked by=3 -->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!-- tehas:blocked\nby=3 -->".to_string(),
            MarkerKind::Blocked,
        ),
        (
            "<!-- tehas:hold-cleared now -->".to_string(),
            MarkerKind::HoldCleared,
        ),
        // The 1.x word of a millwright's marker.
        (
            "<!-- tehas:fixer issue=7 action=retry -->".to_string(),
            MarkerKind::Millwright,
        ),
    ];
    for (text, kind) in cases {
        let got = read(&format!("Before.\n{text}\nAfter."), item());
        assert_eq!(got.markers, [unreadable(kind)], "{text}");
        assert_eq!(got.text, "Before.\nAfter.", "{text}");
    }
}

#[test]
fn a_wrapper_of_no_kind_is_unknown_and_one_with_no_close_keeps_its_words() {
    let got = read("A.\n<!-- tehas:sprint n=3 -->\nB.", item());
    assert_eq!(got.markers, [Marker::Unknown]);
    assert_eq!(got.text, "A.\nB.");
    let text = "A.\n<!-- tehas:blocked by=3\nB.";
    let got = read(text, item());
    assert_eq!(got.markers, [unreadable(MarkerKind::Blocked)]);
    assert_eq!(got.text, text);
}

#[test]
fn a_marker_in_a_quote_or_in_code_is_no_marker() {
    for text in [
        "> <!-- tehas:blocked by=3 -->\n",
        "  > as it said: tehas:blocked by=3\n",
        "```\n<!-- tehas:blocked by=3 -->\n```\n",
        "~~~text\ntehas:blocked by=3\n~~~\nAfter.\n",
        "Write `<!-- tehas:blocked by=3 -->` to hold it.\n",
        "Or ``tehas:blocked by=3`` bare.\n",
        "```\nnever closed\n<!-- tehas:blocked by=3 -->\n",
    ] {
        let got = read(text, item());
        assert!(got.markers.is_empty(), "{text:?}: {:?}", got.markers);
        assert_eq!(got.text, text);
    }
}

#[test]
fn a_bare_marker_is_bare_and_its_words_stay() {
    let bare = |kind, read: Option<Marker>| Marker::Bare {
        kind,
        read: read.map(Box::new),
    };
    let cases = [
        (
            "tehas:blocked by=12",
            bare(MarkerKind::Blocked, Some(blocked(&[12]))),
        ),
        (
            "So: TEHAS:Blocked by=12",
            bare(MarkerKind::Blocked, Some(blocked(&[12]))),
        ),
        (
            "tehas:blocked by=12 and 13",
            bare(MarkerKind::Blocked, None),
        ),
        ("\ttehas:park issue=7", bare(MarkerKind::Park, None)),
        (
            "tehas:millwright issue=7 action=retry",
            bare(
                MarkerKind::Millwright,
                Some(Marker::Millwright(MillwrightMarker {
                    blueprint: BlueprintNo::new(7).unwrap(),
                    action: MillwrightAction::Retry,
                })),
            ),
        ),
        // Another item's reads as no payload.
        (
            "tehas:millwright issue=5 action=retry",
            bare(MarkerKind::Millwright, None),
        ),
        // A bare block's open line encloses nothing.
        (
            "tehas:last-attempt base=abc",
            bare(MarkerKind::LastAttempt, None),
        ),
    ];
    for (line, want) in cases {
        let text = format!("Before.\n{line}\nAfter.\n");
        let got = read(&text, item());
        assert_eq!(got.markers, [want], "{line}");
        assert_eq!(got.text, text, "{line}");
    }
}

#[test]
fn a_kinds_words_without_a_field_are_prose() {
    for text in [
        "tehas:blocked",
        "tehas:blocked by",
        "tehas:blocked by =12",
        "tehas:blocked  by=12",
        "the tehas:blocked label",
        "see:tehas:blocked by=12",
        "tehas:hold-cleared now=1",
        "tehas:blockedby by=12",
    ] {
        let got = read(text, item());
        assert!(got.markers.is_empty(), "{text}: {:?}", got.markers);
        assert_eq!(got.text, text);
    }
}

#[test]
fn markers_keep_the_order_they_stand_in() {
    let text = "<!-- tehas:blocked by=x -->\nA tehas:park issue=7\n<!-- tehas:blocked by=3 -->\n<!-- tehas:hold-cleared -->\n<!-- tehas:blocked by=03 -->\n";
    assert_eq!(
        markers(text),
        [
            unreadable(MarkerKind::Blocked),
            Marker::Bare {
                kind: MarkerKind::Park,
                read: None
            },
            blocked(&[3]),
            Marker::HoldCleared,
            unreadable(MarkerKind::Blocked),
        ]
    );
    assert_eq!(read(text, item()).text, "A tehas:park issue=7");
}

#[test]
fn another_items_marker_does_not_read() {
    let sig = sha('a');
    for (text, kind) in [
        (
            format!("<!-- tehas:park issue=5 step=develop kind= provider=0 sig={sig} -->"),
            MarkerKind::Park,
        ),
        (
            "<!-- tehas:millwright issue=5 action=retry -->".to_string(),
            MarkerKind::Millwright,
        ),
        (
            "<!-- tehas:finish issue=5 result=opened -->".to_string(),
            MarkerKind::Finish,
        ),
        (
            "<!-- tehas:question kind=park issue=5 turn=1 -->".to_string(),
            MarkerKind::Question,
        ),
    ] {
        assert_eq!(markers(&text), [unreadable(kind)], "{text}");
    }
    // A link names the blueprint it links, which is another by its meaning.
    assert!(matches!(
        markers("<!-- tehas:link blueprint=5 -->")[..],
        [Marker::Link { .. }]
    ));
}

#[test]
fn a_block_runs_to_its_close_line_and_one_without_it_is_unreadable() {
    let open = format!("<!-- tehas:last-attempt base={} -->", sha('c'));
    let close = "<!-- /tehas:last-attempt -->";
    let got = read(
        &format!("The task.\r\n\r\n{open}\r\nNote one.\r\n\r\n> quoted\r\n{close}\r\nThe end.\r\n"),
        item(),
    );
    assert_eq!(
        got.markers,
        [Marker::LastAttempt(LastAttemptMarker {
            base: Sha256::new(sha('c')).unwrap(),
            note: "Note one.\r\n\r\n> quoted".to_string(),
        })]
    );
    assert_eq!(got.text, "The task.\r\n\r\nThe end.");
    // An empty block holds an empty note.
    let got = read(&format!("{open}\n{close}\n"), item());
    assert!(
        matches!(&got.markers[..], [Marker::LastAttempt(m)] if m.note.is_empty()),
        "{:?}",
        got.markers
    );
    assert_eq!(got.text, "");
    // No close line: the open line alone is the marker, and the text after
    // it is the body's.
    let got = read(&format!("{open}\nStill the body.\n"), item());
    assert_eq!(got.markers, [unreadable(MarkerKind::LastAttempt)]);
    assert_eq!(got.text, "Still the body.");
    // Not alone on its line.
    let got = read(&format!("x {open}\nNote.\n{close}\n"), item());
    assert_eq!(got.markers, [unreadable(MarkerKind::LastAttempt)]);
    // Two blocks of one kind are each unreadable, and neither is the body's.
    let got = read(
        &format!("{open}\nOne.\n{close}\nBetween.\n{open}\nTwo.\n{close}\n"),
        item(),
    );
    assert_eq!(
        got.markers,
        [
            unreadable(MarkerKind::LastAttempt),
            unreadable(MarkerKind::LastAttempt)
        ]
    );
    assert_eq!(got.text, "Between.");
}
