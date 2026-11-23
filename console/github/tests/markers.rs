//! tehas's markers as the adapter reads them from a text of GitHub's: each
//! edge of the ports spec's "What is a marker", one case an edge, and the
//! rows of `host/markers.tsv` the reading goes by.

use std::num::{NonZeroU32, NonZeroU64};
use tehas_github::markers::read;
use tehas_github::table;
use tehas_ports::laws::{self, markers::Text};
use tehas_ports::{
    AccountId, At, Blocker, BlueprintNo, Bound, ChangeToken, Cite, Cites, ClosedTicket, Comment,
    CommentId, Cursor, Depth, Edit, Filed, Filter, FinishMarker, FinishResult, Got, HappenedKind,
    Item, KeyText, Kind, LastAttemptMarker, Mark, Marker, MarkerKey, MarkerKind, MarkerWord,
    MillwrightAction, MillwrightMarker, ParkMarker, Part, QuestionKind, Ref, Report, Reports,
    Revision, Sha256, Target, TargetId, Ticket, TicketDelta, TicketDigest, TicketEvent, TicketKey,
    Tickets, Took, WebLink, Whole, Window, Within,
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
            Marker::Park({
                let mut p = ParkMarker::new(no, Sha256::new(sha('a')).unwrap());
                p.step = word("develop");
                p.kind = word("");
                p.provider = true;
                p
            }),
        ),
        (
            "<!-- tehas:finish issue=7 result=opened -->".to_string(),
            Marker::Finish(FinishMarker::new(no, FinishResult::Opened)),
        ),
        (
            "<!-- tehas:finish issue=7 result=stopped by=poll -->".to_string(),
            Marker::Finish({
                let mut f = FinishMarker::new(no, FinishResult::Stopped);
                f.by_poll = true;
                f
            }),
        ),
        (
            format!(
                "<!-- tehas:last-attempt base={} -->\nIt ran out\nof time.\n<!-- /tehas:last-attempt -->",
                sha('b')
            ),
            Marker::LastAttempt({
                let mut l = LastAttemptMarker::new(Sha256::new(sha('b')).unwrap());
                l.note = "It ran out\nof time.".to_string();
                l
            }),
        ),
        (
            "<!-- tehas:millwright issue=7 action=retry -->".to_string(),
            Marker::Millwright(MillwrightMarker::new(no, MillwrightAction::Retry)),
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
                Some(Marker::Millwright(MillwrightMarker::new(
                    BlueprintNo::new(7).unwrap(),
                    MillwrightAction::Retry,
                ))),
            ),
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

fn park(no: u64, sig: &str) -> String {
    format!("<!-- tehas:park issue={no} step=develop kind= provider=0 sig={sig} -->")
}

#[test]
fn another_items_marker_is_ignored_and_its_words_stay() {
    let sig = sha('a');
    for text in [
        park(5, &sig),
        "<!-- tehas:millwright issue=5 action=retry -->".to_string(),
        "<!-- tehas:fixer issue=5 action=retry -->".to_string(),
        "<!-- tehas:finish issue=5 result=opened -->".to_string(),
        "<!-- tehas:question kind=park issue=5 turn=1 -->".to_string(),
        // Its other fields need not read, and it needs no close.
        park(5, "zz"),
        "<!--tehas:millwright issue=5 action=nonsense-->".to_string(),
        "<!-- tehas:park issue=5 step=develop".to_string(),
        // Bare.
        "tehas:millwright issue=5 action=retry".to_string(),
        "tehas:park issue=5".to_string(),
    ] {
        let body = format!("Before.\n{text}\nAfter.\n");
        let got = read(&body, item());
        assert_eq!(got.markers, [], "{text}");
        assert_eq!(got.text, body, "{text}");
    }
    // Beside this item's own, which reads.
    let got = read(&format!("{}\n{}\n", park(5, &sig), park(7, &sig)), item());
    assert!(
        matches!(&got.markers[..], [Marker::Park(p)] if p.blueprint == item().unwrap()),
        "{:?}",
        got.markers
    );
    assert_eq!(got.text, park(5, &sig));
    // Read from no numbered item, nothing is another's.
    assert!(matches!(
        read(&park(5, &sig), None).markers[..],
        [Marker::Park(_)]
    ));
    // A link names the blueprint it links, which is another by its meaning.
    assert!(matches!(
        markers("<!-- tehas:link blueprint=5 -->")[..],
        [Marker::Link { .. }]
    ));
}

/// The first marker of `kind` that reads, newest text first, as the ports
/// spec's "Order within a text" has a reader take it: an unreadable or bare
/// one of the kind holds the text, and only a text with none of the kind
/// falls back to the one before it.
fn newest_of(texts: &[&str], kind: &MarkerKind) -> Option<Marker> {
    for text in texts {
        let of_kind: Vec<Marker> = markers(text)
            .into_iter()
            .filter(|m| m.kind() == *kind)
            .collect();
        match of_kind.first() {
            None => continue,
            Some(Marker::Unreadable { .. } | Marker::Bare { .. }) => return None,
            Some(m) => return Some(m.clone()),
        }
    }
    None
}

#[test]
fn another_items_marker_leaves_the_fallback_to_an_older_text() {
    let sig = sha('a');
    let own = park(7, &sig);
    let copied = format!("Copied from #5:\n{}\n", park(5, &sig));
    let found = newest_of(&[&copied, &own], &MarkerKind::Park);
    assert!(
        matches!(&found, Some(Marker::Park(p)) if p.blueprint == item().unwrap()),
        "{found:?}"
    );
    // This item's own unreadable marker holds the text instead.
    let held = format!("{}\n", park(7, "zz"));
    assert_eq!(newest_of(&[&held, &own], &MarkerKind::Park), None);
}

#[test]
fn this_items_unreadable_marker_stays_unreadable_and_is_taken_out() {
    let got = read(&format!("Task.\n{}\nEnd.\n", park(7, "zz")), item());
    assert_eq!(got.markers, [unreadable(MarkerKind::Park)]);
    assert_eq!(got.text, "Task.\nEnd.");
    let close = "<!-- /tehas:last-attempt -->";
    let got = read(
        &format!("Task.\n<!-- tehas:last-attempt base=zz -->\nnote\n{close}\nEnd.\n"),
        item(),
    );
    assert_eq!(got.markers, [unreadable(MarkerKind::LastAttempt)]);
    assert_eq!(got.text, "Task.\nEnd.");
}

#[test]
fn another_items_marker_beside_this_items_unreadable_ones() {
    let sig = sha('a');
    let close = "<!-- /tehas:last-attempt -->";
    let body = format!(
        "Task.\n{}\n{}\n<!-- tehas:last-attempt base=zz -->\nsecret note\n{close}\ntehas:finish issue=5 result=opened\nEnd.\n",
        park(5, &sig),
        park(7, "zz"),
    );
    let got = read(&body, item());
    assert_eq!(
        got.markers,
        [
            unreadable(MarkerKind::Park),
            unreadable(MarkerKind::LastAttempt)
        ]
    );
    assert_eq!(
        got.text,
        format!(
            "Task.\n{}\ntehas:finish issue=5 result=opened\nEnd.",
            park(5, &sig)
        )
    );
}

/// A ticket's notes, each read by the adapter's marker reader as its
/// ticket read reads a body: the handle the ports' marker laws read a text
/// through, while the adapter answers no notes of its own.
struct Notes(Vec<(CommentId, String)>);

impl Reports for Notes {
    fn take_reports(&mut self) -> Vec<Report> {
        Vec::new()
    }
}

impl Cites for Notes {
    fn cite(&self, _: &Cite) -> String {
        unreachable!("the laws read notes alone")
    }
}

fn link() -> WebLink {
    WebLink::new("https://github.com/acme/widget/issues/7").unwrap()
}

impl Tickets for Notes {
    fn notes(&mut self, key: &TicketKey, _: Window) -> Got<Took<Comment>> {
        let items = self
            .0
            .iter()
            .map(|(id, body)| {
                let got = read(body, key.same_item());
                let mut c = Comment::new(id.clone(), At::new(0), Revision::new("r"), link());
                c.body = got.text;
                c.markers = got.markers;
                c
            })
            .collect();
        let mut took = Took::default();
        took.items = items;
        Ok(took)
    }

    fn ticket(&mut self, _: &TicketKey, _: Depth, _: Option<Window>) -> Got<Ticket> {
        unreachable!()
    }
    fn what_is(&mut self, _: NonZeroU64) -> Got<Item> {
        unreachable!()
    }
    fn recent(&mut self, _: &Filter, _: NonZeroU32) -> Got<Took<TicketDigest>> {
        unreachable!()
    }
    fn recently_closed(&mut self, _: NonZeroU32) -> Got<Took<ClosedTicket>> {
        unreachable!()
    }
    fn count(&mut self, _: &Filter) -> Got<u64> {
        unreachable!()
    }
    fn what_are(&mut self, _: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)> {
        unreachable!()
    }
    fn list(&mut self, _: &Filter, _: Bound) -> Got<Whole<TicketDigest>> {
        unreachable!()
    }
    fn several(&mut self, _: &[TicketKey]) -> Vec<(TicketKey, Got<TicketDigest>)> {
        unreachable!()
    }
    fn several_with_closers(
        &mut self,
        _: &[TicketKey],
    ) -> Vec<(TicketKey, Got<tehas_ports::WithCloser>)> {
        unreachable!()
    }
    fn marks(&mut self, _: tehas_ports::Bound) -> Got<tehas_ports::Whole<tehas_ports::MarkHeld>> {
        unreachable!()
    }
    fn history(
        &mut self,
        _: &TicketKey,
        _: Window,
        _: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        unreachable!()
    }
    fn note_edits(&mut self, _: &CommentId, _: Bound) -> Got<Whole<Edit>> {
        unreachable!()
    }
    fn blockers(&mut self, _: &TicketKey) -> Got<Whole<Blocker>> {
        unreachable!()
    }
    fn parts(&mut self, _: &TicketKey) -> Got<Whole<Part>> {
        unreachable!()
    }
    fn closed_since(&mut self, _: At, _: Bound) -> Got<Whole<ClosedTicket>> {
        unreachable!()
    }
    fn search(
        &mut self,
        _: &str,
        _: Within,
        _: Option<bool>,
        _: NonZeroU32,
    ) -> Got<Took<TicketDigest>> {
        unreachable!()
    }
    fn filed_by(&mut self, _: &AccountId, _: At, _: Bound) -> Got<Whole<Filed>> {
        unreachable!()
    }
    fn find_marked(
        &mut self,
        _: &Mark,
        _: &MarkerKind,
        _: &MarkerKey,
        _: Option<bool>,
    ) -> Got<Whole<TicketKey>> {
        unreachable!()
    }
    fn kinds(&mut self) -> Got<Whole<Kind>> {
        unreachable!()
    }
    fn targets(&mut self, _: Bound) -> Got<Whole<Target>> {
        unreachable!()
    }
    fn members(&mut self, _: &TargetId, _: Bound) -> Got<Whole<TicketDigest>> {
        unreachable!()
    }
    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        unreachable!()
    }
    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<TicketDelta> {
        unreachable!()
    }
    fn new_ticket_link(&self, _: &str, _: &str) -> WebLink {
        unreachable!()
    }
}

#[test]
fn another_items_marker_keeps_the_ports_law() {
    let sig = sha('a');
    let cases = [
        (park(5, &sig), MarkerKind::Park),
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
        (park(5, "zz"), MarkerKind::Park),
        ("tehas:park issue=5".to_string(), MarkerKind::Park),
    ];
    let key = TicketKey::Number(NonZeroU64::new(7).unwrap());
    for (text, kind) in cases {
        let id = CommentId::new("1");
        let mut notes = Notes(vec![(id.clone(), format!("Copied.\n{text}\n"))]);
        let at = Text::Note(&mut notes, key.clone(), id);
        assert_eq!(
            laws::markers::another_items_marker_does_not_read(at, &kind),
            Ok(()),
            "{text}"
        );
    }
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
        [Marker::LastAttempt({
            let mut l = LastAttemptMarker::new(Sha256::new(sha('c')).unwrap());
            l.note = "Note one.\r\n\r\n> quoted".to_string();
            l
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

#[test]
fn a_block_that_does_not_read_is_taken_out_to_its_close_line() {
    let close = "<!-- /tehas:last-attempt -->";
    for open in [
        "<!-- tehas:last-attempt base=zz -->".to_string(),
        "<!-- tehas:last-attempt -->".to_string(),
        format!("<!-- tehas:last-attempt base={} extra=1 -->", sha('c')),
    ] {
        let got = read(
            &format!("Task.\n{open}\nsecret note\n{close}\nEnd.\n"),
            item(),
        );
        assert_eq!(got.markers, [unreadable(MarkerKind::LastAttempt)], "{open}");
        assert_eq!(got.text, "Task.\nEnd.", "{open}");
    }
}
