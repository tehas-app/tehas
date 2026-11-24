//! tehas's markers as the adapter writes them into a text of GitHub's:
//! `markers::write`, the inverse of `markers::read`. Every marker of every
//! kind of `host/markers.tsv`, over texts of every shape the reader knows,
//! reads back as it was written; what would not is refused; and the bytes
//! are the bytes tehas 1.x writes for the same markers.

use std::num::{NonZeroU32, NonZeroU64};
use tehas_github::markers::{self, read, write};
use tehas_github::table;
use tehas_ports::{
    BlueprintNo, FinishMarker, FinishResult, KeyText, LastAttemptMarker, MarkName, Marker,
    MarkerKind, MarkerWord, MillwrightAction, MillwrightMarker, Outcome, ParkMarker, QuestionKind,
    Ref, Refusal, Sha256, TicketKey,
};

const ITEM: u64 = 7;

fn item() -> Option<BlueprintNo> {
    BlueprintNo::new(ITEM)
}

fn no(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

fn ticket(n: u64) -> Ref<TicketKey> {
    Ref::Here(TicketKey::Number(NonZeroU64::new(n).unwrap()))
}

fn sha(c: char) -> Sha256 {
    Sha256::new(c.to_string().repeat(64)).unwrap()
}

fn question(kind: QuestionKind, turn: u32) -> Marker {
    Marker::Question {
        kind,
        blueprint: no(ITEM),
        turn: NonZeroU32::new(turn).unwrap(),
    }
}

fn park(step: &str, kind: &str, provider: bool) -> Marker {
    let mut p = ParkMarker::new(no(ITEM), sha('a'));
    p.step = MarkerWord::new(step).unwrap();
    p.kind = MarkerWord::new(kind).unwrap();
    p.provider = provider;
    Marker::Park(p)
}

fn finish(result: FinishResult, by_poll: bool) -> Marker {
    let mut f = FinishMarker::new(no(ITEM), result);
    f.by_poll = by_poll;
    Marker::Finish(f)
}

fn last_attempt(note: &str) -> Marker {
    let mut l = LastAttemptMarker::new(sha('0'));
    l.note = note.to_string();
    Marker::LastAttempt(l)
}

/// One writable marker or more of every kind the table has a row for, each
/// field's values at their edges.
fn every_marker() -> Vec<Marker> {
    let mut all = vec![
        question(QuestionKind::Plan, 1),
        question(QuestionKind::Park, 999_999_999),
        Marker::Blocked {
            by: vec![ticket(3)],
        },
        Marker::Blocked {
            by: (1..=20).map(|n| ticket(n * 49_999_999)).collect(),
        },
        Marker::HoldCleared,
        park("", "", false),
        park("develop", "child-timeout", true),
        park(&"a".repeat(40), &"9-".repeat(20), false),
        last_attempt("One line."),
        last_attempt(""),
        last_attempt("### Learned\n\n- a list\n  - `a/path`\r\nwith a CR line\n\n"),
        Marker::TicketOf(ticket(42)),
        Marker::TicketOf(Ref::Here(TicketKey::Key(
            KeyText::new("PROJ-12/sub.part:x_y").unwrap(),
        ))),
        Marker::Link {
            blueprint: no(12),
            remove: false,
        },
        Marker::Link {
            blueprint: no(999_999_999),
            remove: true,
        },
    ];
    for result in [
        FinishResult::Opened,
        FinishResult::Refused,
        FinishResult::Stopped,
    ] {
        all.push(finish(result, false));
        all.push(finish(result, true));
    }
    for action in [
        MillwrightAction::Retry,
        MillwrightAction::Change,
        MillwrightAction::Answer,
        MillwrightAction::Finish,
        MillwrightAction::Person,
        MillwrightAction::Ask,
    ] {
        all.push(Marker::Millwright(MillwrightMarker::new(no(ITEM), action)));
    }
    all
}

/// Texts of every shape the reader takes apart: none, one line, lines with
/// CRs, quotes, code spans and fences that hold markers, a list, an HTML
/// comment of someone else's, another tool's handoff marker on line one.
fn texts() -> Vec<&'static str> {
    vec![
        "",
        "Done.",
        "Two\r\nlines.",
        "A list:\n\n- one\n  - two\n\n1. three",
        "> quoted\n> twice",
        "Type `tehas:park issue=7` to see the words.",
        "```\ntehas:park issue=7\n```\nAfter the fence.",
        "~~~text\nfenced\n~~~",
        "<!-- a person's own note -->\nand words",
        "<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.",
        "the `tehas:blocked` label, tehas:blocked by =12, tehas:hold-cleared",
        "  indented first line\n\ttabbed",
        "Unicode: grüße, 日本語, emoji 🦀",
    ]
}

fn round_trip(text: &str, markers: &[Marker]) {
    let body = write(text, markers, item())
        .unwrap_or_else(|o| panic!("{o:?} for {text:?} with {markers:?}"));
    let back = read(&body, item());
    assert_eq!(back.markers, markers::laid_out(markers), "{body:?}");
    assert_eq!(back.text, text, "{body:?}");
    assert!(
        !back
            .markers
            .iter()
            .any(|m| matches!(m, Marker::Unreadable { .. } | Marker::Bare { .. })),
        "{body:?}"
    );
}

#[test]
fn every_marker_of_every_kind_round_trips_through_write_then_read() {
    let all = every_marker();
    // Every kind of the table is among them.
    for row in table::marker_rows() {
        assert!(
            all.iter().any(|m| m.kind() == row.kind),
            "no {:?} written",
            row.kind
        );
    }
    for text in texts() {
        for m in &all {
            round_trip(text, std::slice::from_ref(m));
        }
    }
}

#[test]
fn several_markers_round_trip_in_the_order_given() {
    let all = every_marker();
    let leads = |m: &Marker| matches!(m.kind(), MarkerKind::Millwright | MarkerKind::Finish);
    let one_block: Vec<Marker> = all
        .iter()
        .filter(|m| m.kind() != MarkerKind::LastAttempt && !leads(m))
        .cloned()
        .chain([last_attempt("The note.")])
        .collect();
    let mut reversed = one_block.clone();
    reversed.reverse();
    let lead = Marker::Millwright(MillwrightMarker::new(no(ITEM), MillwrightAction::Answer));
    for text in texts() {
        round_trip(text, &one_block);
        round_trip(text, &reversed);
        // With one leading marker, given anywhere: it goes first.
        let mut with_lead = one_block.clone();
        with_lead.insert(3, lead.clone());
        round_trip(text, &with_lead);
        // Each pair, in both orders, but two leading markers.
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                if leads(a) && leads(b)
                    || a.kind() == MarkerKind::LastAttempt && b.kind() == MarkerKind::LastAttempt
                {
                    continue;
                }
                round_trip(text, &[a.clone(), b.clone()]);
                round_trip(text, &[b.clone(), a.clone()]);
            }
        }
    }
}

#[test]
fn two_markers_that_would_each_be_the_first_line_are_unsupported() {
    let finish = finish(FinishResult::Opened, false);
    let answer = Marker::Millwright(MillwrightMarker::new(no(ITEM), MillwrightAction::Answer));
    for two in [[finish.clone(), answer.clone()], [answer.clone(), answer]] {
        assert_eq!(
            write("Text.", &two, item()),
            Err(Outcome::Refused(Refusal::Unsupported))
        );
    }
}

#[test]
fn a_text_with_no_marker_is_written_as_it_came() {
    for text in ["", "Plain.\n", "  Two\r\n\r\nlines \n\n"] {
        assert_eq!(write(text, &[], item()).unwrap(), text);
    }
}

#[test]
fn a_text_beside_a_marker_that_the_reader_would_trim_is_refused_never_trimmed() {
    // A settle sums the text read back against the text sent, so a text the
    // reader gives back otherwise would never be found, and be sent twice.
    let m = [Marker::HoldCleared];
    for given in [
        "Done.\n",
        "\n\n  \nDone.",
        "Done.  ",
        "Done.\r\n\r\n",
        " \n",
    ] {
        assert_eq!(
            write(given, &m, item()),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{given:?}"
        );
        // With no marker beside it, it is kept whole.
        assert_eq!(write(given, &[], item()).unwrap(), given);
    }
    assert_eq!(
        read(&write("  Indented.", &m, item()).unwrap(), item()).text,
        "  Indented."
    );
}

#[test]
fn a_text_that_holds_a_marker_is_refused_never_escaped() {
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    for text in [
        // Wrapped, of this item and another, and with no marker beside it.
        "Done.\n<!-- tehas:hold-cleared -->",
        "<!-- tehas:park issue=7 step= kind= provider=0 sig=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->",
        "<!-- tehas:millwright issue=8 action=retry -->",
        // Badly wrapped, of no kind, unclosed, in another case.
        "<!-- tehas:blocked by=03 -->",
        "<!-- tehas:nonsense -->",
        "a <!-- tehas:question kind=plan",
        "<!--TEHAS:HOLD-CLEARED-->",
        "<!-- tehas:fixer issue=7 action=retry -->",
        // Bare, with a payload and without.
        "tehas:blocked by=12",
        "Then tehas:park issue=7 step=x",
        // A block's open line alone.
        "<!-- tehas:last-attempt base=0000000000000000000000000000000000000000000000000000000000000000 -->",
    ] {
        assert_eq!(write(text, &[], item()), invalid, "{text:?}");
        assert_eq!(
            write(text, &[Marker::HoldCleared], item()),
            invalid,
            "{text:?}"
        );
    }
}

#[test]
fn a_whole_that_would_not_read_back_is_refused() {
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    let hold = [Marker::HoldCleared];
    // A fence the text leaves open would swallow the markers.
    assert_eq!(write("```\ncode", &hold, item()), invalid);
    assert_eq!(write("~~~\ncode\n```", &hold, item()), invalid);
    // Two blocks of one kind read as unreadable.
    let two = [last_attempt("a"), last_attempt("b")];
    assert_eq!(write("Text.", &two, item()), invalid);
    // A marker of another item is no marker of this one.
    let mut other = ParkMarker::new(no(8), sha('a'));
    other.step = MarkerWord::new("x").unwrap();
    assert_eq!(write("Text.", &[Marker::Park(other)], item()), invalid);
    let other = Marker::Millwright(MillwrightMarker::new(no(8), MillwrightAction::Retry));
    assert_eq!(write("Text.", &[other], item()), invalid);
    // A note that holds its close line, or a marker.
    let close = "<!-- /tehas:last-attempt -->";
    assert_eq!(
        write("Text.", &[last_attempt(&format!("a\n{close}\nb"))], item()),
        invalid
    );
    assert_eq!(
        write(
            "Text.",
            &[last_attempt("a <!-- tehas:hold-cleared --> b")],
            item()
        ),
        invalid
    );
}

#[test]
fn a_marker_that_is_not_writable_is_refused_and_a_projects_own_unsupported() {
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    for m in [
        Marker::Unknown,
        Marker::Unreadable {
            kind: MarkerKind::Park,
        },
        Marker::Bare {
            kind: MarkerKind::Blocked,
            read: None,
        },
        Marker::Blocked { by: vec![] },
        Marker::Blocked {
            by: (1..=21).map(ticket).collect(),
        },
        Marker::Blocked {
            by: vec![ticket(1_000_000_000)],
        },
        question(QuestionKind::Unknown, 1),
        Marker::TicketOf(Ref::Elsewhere {
            project: tehas_ports::ProjectName::new("acme/other"),
            item: tehas_ports::ItemName::new("3"),
        }),
        Marker::Link {
            blueprint: no(1_000_000_000),
            remove: false,
        },
    ] {
        assert_eq!(
            write("Text.", std::slice::from_ref(&m), item()),
            invalid,
            "{m:?}"
        );
    }
    let own = Marker::Project {
        name: MarkName::new("acme-hold").unwrap(),
        key: None,
        fields: Vec::new(),
    };
    assert_eq!(
        write("Text.", &[own], item()),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
}

/// tehas 1.x's words around a question, from `console/core/src/question.rs`.
const ASK_PARK: &str = "Reply in a comment with the word of one of these:";
const ASK_PLAN: &str =
    "Reply in a comment with the number of one of these, or answer in your own words:";
const ONLY_ALLOWED: &str = "Only an answer from an account that may authorise work counts.";

/// The park question as 1.x's `q_render` writes it, its markers taken out:
/// the text the cycle hands `note`.
fn park_text(handoff: bool) -> String {
    let first = if handoff {
        "<!-- pi-rukas:handoff issue=7 cap=x -->\n"
    } else {
        ""
    };
    format!(
        "{first}Tehas cycle stopped.\n\n{ASK_PARK}\n\n- retry: Retry with the issue and all its comments. A new cycle starts over in a new workspace, given what allowed accounts wrote on this issue and on its pull requests.\n- retry-text: Retry with the issue text only. A new cycle starts over in a new workspace, given the title and body and nothing else.\n- park: Leave it parked. Nothing more happens until someone answers.\n\n{ONLY_ALLOWED}"
    )
}

#[test]
fn a_park_question_and_a_handoff_are_the_bytes_1x_wrote() {
    let mut p = ParkMarker::new(no(7), sha('a'));
    p.step = MarkerWord::new("review").unwrap();
    let markers = [Marker::Park(p), question(QuestionKind::Park, 1)];
    // From question.rs's test a_park_question_lists_its_choices_in_words_and_both_markers.
    let handoff = format!(
        "<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\n{ASK_PARK}\n\n- retry: Retry with the issue and all its comments. A new cycle starts over in a new workspace, given what allowed accounts wrote on this issue and on its pull requests.\n- retry-text: Retry with the issue text only. A new cycle starts over in a new workspace, given the title and body and nothing else.\n- park: Leave it parked. Nothing more happens until someone answers.\n\n{ONLY_ALLOWED}\n\n<!-- tehas:park issue=7 step=review kind= provider=0 sig=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->\n<!-- tehas:question kind=park issue=7 turn=1 -->"
    );
    assert_eq!(write(&park_text(true), &markers, item()).unwrap(), handoff);
    // The same question with no handoff of pi-rukas: its first line goes.
    let plain = handoff.replacen("<!-- pi-rukas:handoff issue=7 cap=x -->\n", "", 1);
    assert_eq!(write(&park_text(false), &markers, item()).unwrap(), plain);
    // The handoff stays the comment's first line, as the cycle's
    // carries_handoff reads it, and is no marker of tehas's.
    let back = read(&handoff, item());
    assert!(back
        .text
        .starts_with("<!-- pi-rukas:handoff issue=7 cap=x -->\n"));
    assert_eq!(back.markers, markers);
}

#[test]
fn a_last_attempt_is_where_1x_put_it_and_a_planning_question_too() {
    // From question.rs's test a_planning_question_holds_no_text_of_the_plan.
    let text = format!("Which?\n\n{ASK_PLAN}\n\n1. Update this issue\n2. End this planning session: the session ends, and nothing more is planned.\n\n{ONLY_ALLOWED}");
    let plan = format!("{text}\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->");
    assert_eq!(
        write(&text, &[question(QuestionKind::Plan, 2)], item()).unwrap(),
        plan
    );
    // From last_attempt.rs's decide and note, with no steps recorded: the
    // body, a blank line, the open line, the note, the close line.
    let base = "0".repeat(64);
    let note = "### The last attempt ran out of time\n\nThe last cycle on this text was stopped in develop: the developer ran past its time bound. Its work was not pushed, and the next cycle starts from a clean checkout.\n\nPlan a smaller change: finish one part well, or say in this issue what to leave out. This section is the factory's; edit or delete it as you like.";
    let edited = format!(
        "Fix it.\n\n<!-- tehas:last-attempt base={base} -->\n{note}\n<!-- /tehas:last-attempt -->"
    );
    assert_eq!(
        write("Fix it.", &[last_attempt(note)], item()).unwrap(),
        edited
    );
}

#[test]
fn each_markers_line_is_the_table_s_words() {
    for m in every_marker() {
        let body = write("", std::slice::from_ref(&m), item()).unwrap();
        assert_eq!(Some(body), table::marker_text(&m), "{m:?}");
    }
}

// Each kind where 1.x put it, against the bytes 1.x wrote: core's own
// writer where this crate can call it, else the 1.x source's text, named by
// file and line.

/// `text` and `markers` through the writer, on #7.
fn written(text: &str, markers: &[Marker]) -> String {
    write(text, markers, item()).unwrap_or_else(|o| panic!("{o:?}: {text:?} {markers:?}"))
}

#[test]
fn a_question_is_where_core_s_q_render_puts_it() {
    use tehas_core::question::{q_parts, q_render, Kind};
    let out = serde_json::json!({"question": {"question": "Adopt?", "options": [{"label": "Adopt all"}]}});
    let rendered = q_render(Kind::Plan, 7, 3, &out, "");
    let (text, markers) = q_parts(&rendered, Kind::Plan, 7, 3, None).unwrap();
    assert_eq!(written(&text, &markers), rendered);
}

#[test]
fn a_park_is_where_core_s_q_render_and_the_park_s_comment_put_it() {
    use tehas_core::question::{q_parts, q_render, Kind};
    let Marker::Park(p) = park("develop", "child-timeout", true) else {
        unreachable!()
    };
    let line = table::marker_text(&Marker::Park(p.clone())).unwrap();
    let out = |park: &str| serde_json::json!({"text": "Tehas cycle stopped.", "question": null, "park": park, "retry": false});
    let rendered = q_render(Kind::Park, 7, 1, &out(&line), "");
    let without = q_render(Kind::Park, 7, 1, &out(""), "");
    let (text, markers) = q_parts(&without, Kind::Park, 7, 1, Some(p.clone())).unwrap();
    assert_eq!(written(&text, &markers), rendered);
    // console/cycle/src/report.rs line 255 (Park::comment): the words, a
    // blank line, the park's marker.
    assert_eq!(
        written("Tehas cycle stopped. To retry, apply `tehas:ready`.", &[Marker::Park(p)]),
        format!("Tehas cycle stopped. To retry, apply `tehas:ready`.\n\n<!-- tehas:park issue=7 step=develop kind=child-timeout provider=1 sig={} -->", "a".repeat(64))
    );
}

#[test]
fn a_finish_is_the_first_line_as_1x_s_replies_have_it() {
    // console/dispatch/src/finish.rs line 302 (opened).
    let text = "Tehas took `tehas:finish` off this issue and opened a draft pull request from what its stopped cycle left, as alice asked: https://github.com/acme/widget/pull/9\n\nIts gate did not pass. The checks and the review decide on it as on any other pull request.";
    assert_eq!(
        written(text, &[finish(FinishResult::Opened, false)]),
        format!("<!-- tehas:finish issue=7 result=opened -->\n{text}")
    );
    // console/dispatch/src/auto_finish.rs line 90 (opened by=poll).
    let text = "The last cycle of this issue stopped because its push failed. Tehas finished it from the kept workspace on its own and opened a draft pull request: https://github.com/acme/widget/pull/90\n\nIts gate did not pass. The checks and the review decide on it as on any other pull request.";
    assert_eq!(
        written(text, &[finish(FinishResult::Opened, true)]),
        format!("<!-- tehas:finish issue=7 result=opened by=poll -->\n{text}")
    );
    // finish.rs line 311 (refused) and auto_finish.rs line 100 (stopped)
    // cite the millwright's marker in their text: no body carries a cited
    // marker, so they are refused.
    let refused = "Tehas took `tehas:finish` off this issue and finished nothing: no reason. To ask again, write a new comment that starts with `<!-- tehas:millwright issue=7 action=finish -->` and apply `tehas:finish`.";
    assert_eq!(
        write(refused, &[finish(FinishResult::Refused, false)], item()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

#[test]
fn a_millwright_marker_is_the_first_line_its_text_on_the_next() {
    // The millwright runs outside tehas: 1.x writes none. Its readers take
    // the marker as the first line (console/dispatch/src/questions.rs line
    // 326, finish.rs line 166), and their cases write it so
    // (questions.rs line 717: "{MARK}\nAdopt all, as in #12.").
    for (action, word) in [
        (MillwrightAction::Retry, "retry"),
        (MillwrightAction::Change, "change"),
        (MillwrightAction::Answer, "answer"),
        (MillwrightAction::Finish, "finish"),
        (MillwrightAction::Person, "person"),
        (MillwrightAction::Ask, "ask"),
    ] {
        let m = Marker::Millwright(MillwrightMarker::new(no(7), action));
        assert_eq!(
            written("Adopt all, as in #12.", &[m]),
            format!("<!-- tehas:millwright issue=7 action={word} -->\nAdopt all, as in #12.")
        );
    }
}

#[test]
fn a_hold_is_where_1x_s_manual_writes_it() {
    // 1.x writes no hold: a person does, as host/docs/operating.md line 249
    // shows.
    assert_eq!(
        written(
            "Blocked by #12 and #34.",
            &[Marker::Blocked {
                by: vec![ticket(12), ticket(34)]
            }]
        ),
        "Blocked by #12 and #34.\n\n<!-- tehas:blocked by=12,34 -->"
    );
}

#[test]
fn a_hold_cleared_is_where_1x_s_poll_wrote_it() {
    // console/dispatch/src/decide.rs line 958.
    let text = "Every issue the `tehas:blocked` marker names is closed, so the hold has cleared. Tehas takes `tehas:blocked` off when the cycle starts. Leave it on until then: it is what asks for the start when `tehas:ready` is not on.";
    assert_eq!(
        written(text, &[Marker::HoldCleared]),
        format!("{text}\n\n<!-- tehas:hold-cleared -->")
    );
}

#[test]
fn a_ticket_of_and_a_link_go_after_the_text_as_their_new_rows_say() {
    // New in 2.0: no 1.x writer. The words are host/markers.tsv's rows,
    // spelled here by hand.
    assert_eq!(
        written("For people.", &[Marker::TicketOf(ticket(42))]),
        "For people.\n\n<!-- tehas:ticket-of ticket=42 -->"
    );
    assert_eq!(
        written(
            "Link these.",
            &[Marker::Link {
                blueprint: no(12),
                remove: true
            }]
        ),
        "Link these.\n\n<!-- tehas:link blueprint=12 remove=1 -->"
    );
}

#[test]
fn a_fence_closed_by_a_shorter_run_stays_open_and_is_refused() {
    // CommonMark, as GitHub renders: a closing fence is at least as long
    // as the one that opened it.
    assert_eq!(
        write("````\nx\n```", &[Marker::HoldCleared], item()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(read("````\nx\n```\n<!-- tehas:hold-cleared -->", item())
        .markers
        .is_empty());
    assert_eq!(
        read("````\nx\n````\n<!-- tehas:hold-cleared -->", item()).markers,
        [Marker::HoldCleared]
    );
}

#[test]
fn a_fence_closed_by_a_line_with_words_stays_open_and_is_refused() {
    // A closing fence has nothing after its run.
    assert_eq!(
        write("```\nx\n```rust", &[Marker::HoldCleared], item()),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert!(read("```\nx\n```rust\n<!-- tehas:hold-cleared -->", item())
        .markers
        .is_empty());
}
