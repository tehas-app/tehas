//! The adapter's table of labels against `host/labels.tsv`, the table's one
//! source: the same labels, each with the same word, in the same order.

use tehas_github::table;
use tehas_ports::{Mark, Marker};

const THE_TABLE: &str = include_str!("../../../host/labels.tsv");

/// The mark a word of the file's last column names: `Act::Approve`,
/// `State::Running`, `Choice::Lean`, `Mark::Epic`.
fn word(w: &str) -> Mark {
    let json = match w.split_once("::") {
        Some(("Mark", v)) => format!(r#"{{"type":"{v}"}}"#),
        Some((t, v)) => format!(r#"{{"type":"{t}","value":"{v}"}}"#),
        None => panic!("{w} is no word"),
    };
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{w}: {e}"))
}

#[test]
fn the_table_is_host_labels_tsv() {
    let file: Vec<(String, Mark)> = THE_TABLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            (cols[0].to_string(), word(cols[cols.len() - 1]))
        })
        .collect();
    let ours: Vec<(String, Mark)> = table::words()
        .into_iter()
        .map(|(l, m)| (l.to_string(), m))
        .collect();
    assert_eq!(ours, file);
    for (label, mark) in &file {
        assert!(!matches!(mark, Mark::Unknown), "{label}");
        assert_eq!(table::word(label).as_ref(), Some(mark), "{label}");
        assert_eq!(table::label(mark).as_deref(), Some(label.as_str()));
    }
}

#[test]
fn a_label_of_no_word_is_none_and_a_projects_mark_is_its_name() {
    assert_eq!(table::word("bug"), None);
    let bug = Mark::Project(tehas_ports::MarkName::new("bug").unwrap());
    assert_eq!(table::label(&bug).as_deref(), Some("bug"));
    assert_eq!(table::label(&Mark::Unknown), None);
}

#[test]
fn the_approval_is_the_word_approve() {
    assert_eq!(
        table::word(table::APPROVAL_LABEL),
        Some(Mark::Act(tehas_ports::Act::Approve))
    );
}

const THE_MARKERS: &str = include_str!("../../../host/markers.tsv");

fn bp(n: u64) -> tehas_ports::BlueprintNo {
    tehas_ports::BlueprintNo::new(n).unwrap()
}

fn sha(c: char) -> tehas_ports::Sha256 {
    tehas_ports::Sha256::new(c.to_string().repeat(64)).unwrap()
}

/// One writable marker of each kind of the table, and the text it writes.
fn written() -> Vec<(Marker, &'static str)> {
    use std::num::{NonZeroU32, NonZeroU64};
    use tehas_ports::*;
    let n = |n: u64| Ref::Here(TicketKey::Number(NonZeroU64::new(n).unwrap()));
    vec![
        (
            Marker::Question {
                kind: QuestionKind::Plan,
                blueprint: bp(12),
                turn: NonZeroU32::new(2).unwrap(),
            },
            "<!-- tehas:question kind=plan issue=12 turn=2 -->",
        ),
        (
            Marker::Blocked { by: vec![n(3), n(40)] },
            "<!-- tehas:blocked by=3,40 -->",
        ),
        (Marker::HoldCleared, "<!-- tehas:hold-cleared -->"),
        (
            Marker::Park({
let mut park_marker = ParkMarker::new(bp(12), sha('a'));
park_marker.step = MarkerWord::new("gate").unwrap();
park_marker.kind = MarkerWord::new("").unwrap();
park_marker.provider = true;
park_marker
}),
            "<!-- tehas:park issue=12 step=gate kind= provider=1 sig=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->",
        ),
        (
            Marker::Finish(FinishMarker::new(bp(12), FinishResult::Refused)),
            "<!-- tehas:finish issue=12 result=refused -->",
        ),
        (
            Marker::Finish({
let mut finish_marker = FinishMarker::new(bp(12), FinishResult::Stopped);
finish_marker.by_poll = true;
finish_marker
}),
            "<!-- tehas:finish issue=12 result=stopped by=poll -->",
        ),
        (
            Marker::LastAttempt({
let mut last_attempt_marker = LastAttemptMarker::new(sha('b'));
last_attempt_marker.note = "it ran out of time".into();
last_attempt_marker
}),
            "<!-- tehas:last-attempt base=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb -->\nit ran out of time\n<!-- /tehas:last-attempt -->",
        ),
        (
            Marker::Millwright(MillwrightMarker::new(bp(12), MillwrightAction::Finish)),
            "<!-- tehas:millwright issue=12 action=finish -->",
        ),
        (Marker::TicketOf(n(7)), "<!-- tehas:ticket-of ticket=7 -->"),
        (
            Marker::Link {
                blueprint: bp(12),
                remove: false,
            },
            "<!-- tehas:link blueprint=12 -->",
        ),
        (
            Marker::Link {
                blueprint: bp(12),
                remove: true,
            },
            "<!-- tehas:link blueprint=12 remove=1 -->",
        ),
        (
            Marker::Declined {
                question: CommentId::new("2501234567"),
            },
            "<!-- tehas:declined question=2501234567 -->",
        ),
    ]
}

#[test]
fn each_marker_writes_its_row_of_host_markers_tsv() {
    let kinds: Vec<&str> = THE_MARKERS
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    let all = written();
    for kind in &kinds {
        assert!(
            all.iter().any(|(m, _)| format!("{:?}", m.kind()) == *kind),
            "{kind} has no case"
        );
    }
    for (marker, text) in all {
        assert_eq!(table::marker_text(&marker).as_deref(), Some(text));
    }
}

#[test]
fn a_cited_marker_is_quoted_as_code() {
    let finish = Marker::Millwright(tehas_ports::MillwrightMarker::new(
        bp(12),
        tehas_ports::MillwrightAction::Finish,
    ));
    assert_eq!(
        table::cite_marker(&finish),
        "`<!-- tehas:millwright issue=12 action=finish -->`"
    );
    let block = Marker::LastAttempt({
        let mut last_attempt_marker = tehas_ports::LastAttemptMarker::new(sha('b'));
        last_attempt_marker.note = "a ```fenced``` note".into();
        last_attempt_marker
    });
    let cited = table::cite_marker(&block);
    assert!(
        cited.starts_with("````\n<!-- tehas:last-attempt "),
        "{cited}"
    );
    assert!(
        cited.ends_with("\n<!-- /tehas:last-attempt -->\n````"),
        "{cited}"
    );
}

#[test]
fn a_marker_that_is_not_writable_cites_as_nothing() {
    use tehas_ports::{MarkerKind, Ref};
    let elsewhere = Marker::TicketOf(Ref::Elsewhere {
        project: tehas_ports::ProjectName::new("acme/other"),
        item: tehas_ports::ItemName::new("7"),
    });
    let project = Marker::Project {
        name: tehas_ports::MarkName::new("ours").unwrap(),
        key: None,
        fields: vec![],
    };
    for m in [
        Marker::Unknown,
        Marker::Unreadable {
            kind: MarkerKind::Finish,
        },
        Marker::Bare {
            kind: MarkerKind::Park,
            read: None,
        },
        Marker::Blocked { by: vec![] },
        elsewhere,
        project,
    ] {
        assert_eq!(table::cite_marker(&m), "", "{m:?}");
        assert_eq!(table::marker_text(&m), None, "{m:?}");
    }
}

#[test]
fn every_word_names_its_label_and_cites_it_as_code() {
    for (label, mark) in table::words() {
        assert_eq!(table::mark_name(&mark), label);
        assert_eq!(table::cite_mark(&mark), format!("`{label}`"));
    }
    let bug = Mark::Project(tehas_ports::MarkName::new("bug").unwrap());
    assert_eq!(table::mark_name(&bug), "bug");
    assert_eq!(table::cite_mark(&bug), "`bug`");
    for unknown in [
        Mark::Unknown,
        Mark::Act(tehas_ports::Act::Unknown),
        Mark::State(tehas_ports::State::Unknown),
    ] {
        assert_eq!(table::mark_name(&unknown), "", "{unknown:?}");
        assert_eq!(table::cite_mark(&unknown), "", "{unknown:?}");
    }
}

#[test]
fn every_factory_mark_has_a_label() {
    let names: Vec<String> = tehas_ports::laws::marks::factory_marks()
        .iter()
        .map(table::mark_name)
        .collect();
    assert!(names.iter().all(|n| !n.is_empty()), "{names:?}");
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len());
}

#[test]
fn every_row_of_the_file_gives_its_name_colour_and_text() {
    let rows: Vec<&str> = THE_TABLE.lines().collect();
    let specs = table::specs();
    assert_eq!(specs.len(), rows.len());
    for (spec, row) in specs.iter().zip(&rows) {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(cols.len(), 4, "{row}");
        assert_eq!(spec.name.as_str(), cols[0]);
        assert!(table::is_label(cols[0]), "{row}");
        assert_eq!(spec.colour.as_ref().map(|c| c.as_str()), Some(cols[1]));
        assert_eq!(spec.about.as_deref(), Some(cols[2]));
    }
}
