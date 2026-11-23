//! Every law against the fake, as the contract test any adapter runs; the
//! `Writer` sending each kind of write to the fake; `Faulty` through the
//! resilience cases; and the health laws over `FakeSite`, a `MemoryLedger`
//! and the `NoticeLog` sink, on the fake's clock.

use std::io::Read;
use std::num::NonZeroU32;
use std::sync::Arc;

use tehas_ports::fake::{
    self, Asked, BlueprintsOp, BoardOp, CodeHostOp, FakeSite, Fault, Faulty, IdentityOp,
    MemoryLedger, MemoryTokens, Op, TicketsOp, Watch, World, Wrote, FACTORY, OPERATOR,
};
use tehas_ports::laws::{self, code_host::Left, code_host::Leftover, health::Wiring};
use tehas_ports::*;

const C: Caller = Caller::new("test.laws");

fn full() -> Actor {
    Actor::Ours(Scope::Full)
}

fn b(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn n32(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

fn bp(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

fn cn(n: u64) -> ChangeNo {
    ChangeNo::new(n).unwrap()
}

fn branch(s: &str) -> Branch {
    Branch::new(s).unwrap()
}

fn sha(c: char) -> Sha256 {
    Sha256::new(c.to_string().repeat(64)).unwrap()
}

/// Files a caller holds, by sum, with fixed test digests: the ports never
/// hash.
struct Files(Vec<(Sum, Vec<u8>)>);

impl Blobs for Files {
    fn open(&self, sum: &Sum) -> Option<Box<dyn Read + '_>> {
        self.0
            .iter()
            .find(|(s, _)| s == sum)
            .map(|(_, b)| Box::new(&b[..]) as Box<dyn Read>)
    }
}

fn version_file() -> (Files, Vec<(RepoPath, Sum)>) {
    let bytes = b"2.0.0\n".to_vec();
    let sum = Sum {
        sha256: sha('a'),
        len: bytes.len() as u64,
    };
    (
        Files(vec![(sum.clone(), bytes)]),
        vec![(RepoPath::new("VERSION").unwrap(), sum)],
    )
}

/// Each law that reads an item runs in both setups.
fn both() -> [World; 2] {
    [World::same_item(), World::split()]
}

/// The number of the first blueprint and change of each setup.
fn first_blueprint(w: &World) -> u64 {
    match w.key(1) {
        TicketKey::Number(_) => 1,
        TicketKey::Key(_) => 101,
    }
}

fn first_change(w: &World) -> u64 {
    match w.key(1) {
        TicketKey::Number(_) => 50,
        TicketKey::Key(_) => 501,
    }
}

fn ok(law: laws::Law) {
    if let Err(e) = law {
        panic!("{}: {}", e.law, e.what);
    }
}

fn proposal(w: &World, name: &str, files: &[(RepoPath, Sum)]) -> Proposal {
    Proposal {
        base: w.head("main").unwrap(),
        branch: branch(name),
        files: files.to_vec(),
        message: format!("Propose {name}"),
        change: NewChange {
            head: branch(name),
            base: branch("main"),
            title: format!("Propose {name}"),
            body: Body::Text("A proposal.".into()),
            draft: false,
            closes: vec![],
        },
        takeover: Takeover::Never,
    }
}

// The laws, against the fake.

#[test]
fn tickets_removing_an_absent_mark_is_done() {
    for w in both() {
        let w = w.ticket(1, |t| t.title("a ticket").mark(Mark::Epic));
        let site = FakeSite::new(&w);
        let mut h = site.ticket_writes(&full(), C).unwrap();
        ok(laws::tickets::removing_an_absent_mark_is_done(
            h.as_mut(),
            &w.key(1),
            &Mark::State(State::Running),
        ));
    }
}

#[test]
fn code_host_merge_at_a_moved_head_is_a_conflict() {
    for w in both() {
        let no = first_change(&w);
        let w = w.change(no, "work", |c| c.title("work"));
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&full(), C).unwrap();
        let pusher = w.clone();
        ok(laws::code_host::merge_at_a_moved_head_is_a_conflict(
            h.as_mut(),
            cn(no),
            &mut |b: &Branch| {
                pusher.push(b.as_str(), "more work", OPERATOR);
            },
        ));
    }
}

#[test]
fn code_host_merge_at_the_proposed_head() {
    for w in both() {
        let (blobs, files) = version_file();
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&full(), C).unwrap();
        let first = proposal(&w, "bump-1", &files);
        let second = proposal(&w, "bump-2", &files);
        let pusher = w.clone();
        ok(laws::code_host::merge_at_the_proposed_head(
            h.as_mut(),
            &first,
            &second,
            &blobs,
            &mut |b: &Branch| {
                pusher.push(b.as_str(), "an outside push", OPERATOR);
            },
        ));
    }
}

#[test]
fn code_host_a_closed_proposal_opens_anew() {
    for w in both() {
        let (blobs, files) = version_file();
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&full(), C).unwrap();
        ok(laws::code_host::a_closed_proposal_opens_anew(
            h.as_mut(),
            &proposal(&w, "bump-3", &files),
            &blobs,
        ));
    }
}

/// Leaves each case's branch as another account would.
fn leaver(w: &World) -> impl FnMut(&Left) -> Leftover + '_ {
    let mut n = 0u64;
    move |left: &Left| {
        n += 1;
        let name = format!("left-{n}");
        let someone = "someone";
        match left {
            Left::OneCommit { message } => {
                w.push(&name, message, someone);
            }
            Left::OneCommitBaseMoved { message } => {
                w.push(&name, message, someone);
                w.push("main", "main moved on", OPERATOR);
            }
            Left::TwoCommits { message } => {
                w.push(&name, "a first commit", someone);
                w.push(&name, message, someone);
            }
            Left::TwoParents { message } => {
                let side = w.push(&format!("side-{n}"), "a side commit", someone);
                w.push_merge(&name, &side, message, someone);
            }
            Left::OpenChange { message } => {
                w.push(&name, message, someone);
                let site = FakeSite::new(w);
                let mut h = site.code_host_writes(&full(), C).unwrap();
                h.open_change(&NewChange {
                    head: branch(&name),
                    base: branch("main"),
                    title: "an open change".into(),
                    body: Body::Text(String::new()),
                    draft: false,
                    closes: vec![],
                })
                .unwrap();
            }
        }
        Leftover {
            branch: branch(&name),
            base: w.head("main").unwrap(),
            into: branch("main"),
        }
    }
}

#[test]
fn code_host_takeover_by_shape() {
    for w in both() {
        let (blobs, files) = version_file();
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&full(), C).unwrap();
        ok(laws::code_host::takeover_by_shape(
            h.as_mut(),
            &files,
            &blobs,
            &mut leaver(&w),
        ));
    }
}

#[test]
fn lists_past_the_bound_is_unread() {
    for w in both() {
        let no = first_blueprint(&w);
        let w = w
            .ticket(1, |t| t.title("one"))
            .ticket(2, |t| t.title("two"))
            .ticket(3, |t| t.title("three"))
            .blueprint(no, |b| b.comment("alice", "a").comment("bob", "b"));
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&full(), C).unwrap();
        ok(laws::lists::past_the_bound_is_unread(
            |bound| t.list(&Filter::default(), bound),
            3,
        ));
        let mut bl = site.blueprints(&full(), C).unwrap();
        ok(laws::lists::past_the_bound_is_unread(
            |bound| bl.comments(bp(no), bound),
            2,
        ));
    }
}

#[test]
fn lists_last_n_is_never_cut_and_more_is_exact() {
    for w in both() {
        let no = first_change(&w);
        let at = w.now();
        let w = w.ticket(1, |t| t.events(5, at)).change(no, "work", |c| {
            c.event("alice", ChangeHappened::Pushed)
                .event("bob", ChangeHappened::Commented)
                .event("alice", ChangeHappened::Reviewed)
        });
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&full(), C).unwrap();
        let key = w.key(1);
        ok(laws::lists::last_n_is_never_cut(
            |win| t.history(&key, win, None),
            5,
        ));
        ok(laws::lists::more_is_exact(
            |win| t.history(&key, win, None),
            5,
        ));
        let mut c = site.code_host(&full(), C).unwrap();
        ok(laws::lists::last_n_is_never_cut(
            |win| c.activity(cn(no), win),
            3,
        ));
        ok(laws::lists::more_is_exact(|win| c.activity(cn(no), win), 3));
    }
}

#[test]
fn every_mark_names_a_word_and_reads_back_cited() {
    for w in both() {
        let no = first_blueprint(&w);
        let w = w.blueprint(no, |b| b.title("a plan"));
        let site = FakeSite::new(&w);
        let ours = [Mark::Project(MarkName::new("bug").unwrap())];
        ok(laws::marks::every_mark_names_a_word(&site, &ours));
        let mut h = site.blueprint_writes(&full(), C).unwrap();
        for m in laws::marks::factory_marks().iter().chain(&ours) {
            ok(laws::marks::a_cited_mark_reads_back_as_its_name(
                &site,
                h.as_mut(),
                bp(no),
                m,
            ));
        }
        let setup = Setup::One(Arc::new(site.clone()));
        assert_eq!(
            setup.mark_name(Realm::Tickets, &Mark::Epic),
            site.mark_name(&Mark::Epic)
        );
    }
}

#[test]
fn deltas_page_oldest_first_past_the_bound() {
    for w in both() {
        let no = first_blueprint(&w);
        let ch = first_change(&w);
        let w = w
            .ticket(1, |t| t.title("one").comment("alice", "a note"))
            .ticket(2, |t| t.title("two"))
            .ticket(3, |t| t.title("three"))
            .blueprint(no, |b| b.comment("alice", "a").comment("bob", "b"))
            .change(ch, "work-1", |c| c.comment("alice", "c"))
            .change(ch + 1, "work-2", |c| c)
            .change(ch + 2, "work-3", |c| c.comment("bob", "d"));
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&full(), C).unwrap();
        let mut bl = site.blueprints(&full(), C).unwrap();
        let mut c = site.code_host(&full(), C).unwrap();
        for page in [1, 2] {
            ok(laws::delta::tickets_page_on(t.as_mut(), None, b(page)));
            ok(laws::delta::blueprints_page_on(bl.as_mut(), None, b(1)));
            ok(laws::delta::changes_page_on(c.as_mut(), None, b(page)));
        }
        let first = t.delta(None, b(1)).unwrap();
        assert!(first.more);
        ok(laws::delta::tickets_page_on(
            t.as_mut(),
            Some(&first.next),
            b(1),
        ));
    }
}

#[test]
fn a_delta_past_the_bound_is_its_oldest_changes() {
    let w = World::split()
        .ticket(1, |t| t.title("one"))
        .ticket(2, |t| t.title("two"))
        .ticket(3, |t| t.title("three"));
    let site = FakeSite::new(&w);
    let mut h = site.ticket_writes(&full(), C).unwrap();
    h.mark(&w.key(1), &Mark::Epic).unwrap();
    let page = h.delta(None, b(2)).unwrap();
    let titles: Vec<&str> = page
        .tickets
        .iter()
        .map(|t| t.digest.title.as_str())
        .collect();
    assert_eq!(titles, ["two", "three"]);
    assert!(page.more);
    let rest = h.delta(Some(&page.next), b(2)).unwrap();
    let titles: Vec<&str> = rest
        .tickets
        .iter()
        .map(|t| t.digest.title.as_str())
        .collect();
    assert_eq!(titles, ["one"]);
    assert!(!rest.more);
}

#[test]
fn lists_hold_for_a_split_tickets_notes() {
    let w = World::split().ticket(1, |t| {
        t.comment("alice", "one")
            .comment("bob", "two")
            .comment("alice", "three")
            .comment("carol", "four")
    });
    let site = FakeSite::new(&w);
    let mut t = site.tickets(&full(), C).unwrap();
    let key = w.key(1);
    ok(laws::lists::last_n_is_never_cut(
        |win| t.notes(&key, win),
        4,
    ));
    ok(laws::lists::more_is_exact(|win| t.notes(&key, win), 4));
}

#[test]
fn board_one_item_fails_alone() {
    for w in both() {
        let at = w.now();
        let w = w
            .ticket(1, |t| t.title("small"))
            .ticket(2, |t| t.title("big").events(5, at))
            .ticket(3, |t| t.title("small too").events(2, at));
        let site = FakeSite::new(&w);
        let mut board = site.board(&full(), C).unwrap();
        let ask = BoardAsk {
            depth: Depth::Cut { notes: 0 },
            history: Some(Window::Whole(b(3))),
            changes: None,
            filter: Filter::default(),
            control: None,
        };
        ok(laws::board::one_item_fails_alone(
            board.as_mut(),
            &ask,
            b(50),
            &w.key(2),
        ));
    }
}

#[test]
fn history_oldest_first() {
    for w in both() {
        let at = w.now();
        let w = w.ticket(4, |t| t.events(6, at).act(Act::Approve, "alice"));
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&full(), C).unwrap();
        ok(laws::history::oldest_first(t.as_mut(), &w.key(4)));
    }
}

#[test]
fn blueprints_an_edit_moves_the_revision() {
    for w in both() {
        let no = first_blueprint(&w);
        let w = w.blueprint(no, |b| b.title("a plan").body("Do it."));
        let site = FakeSite::new(&w);
        let mut h = site.blueprint_writes(&full(), C).unwrap();
        ok(laws::blueprints::an_edit_moves_the_revision(
            h.as_mut(),
            bp(no),
        ));
    }
}

#[test]
fn blueprints_an_attachment_has_a_link_and_a_pasted_one_no_name() {
    for w in both() {
        let no = first_blueprint(&w);
        let w = w.blueprint(no, |b| {
            b.attachment("plan.pdf", b"%PDF")
                .pasted(b"png")
                .attachment("notes.txt", b"n")
        });
        let site = FakeSite::new(&w);
        let mut h = site.blueprints(&full(), C).unwrap();
        ok(
            laws::blueprints::an_attachment_has_a_link_and_a_pasted_one_no_name(
                h.as_mut(),
                bp(no),
                &["plan.pdf", "notes.txt"],
                1,
            ),
        );
    }
}

#[test]
fn search_laws_over_both_ports() {
    for w in both() {
        let w = w
            .ticket(1, |t| t.title("Alpha one"))
            .ticket(2, |t| t.title("beta").text("alpha in the text alone"))
            .ticket(3, |t| t.title("alpha two"))
            .ticket(4, |t| t.title("ALPHA three"))
            .ticket(5, |t| {
                t.title("about kind:bug and alpha project:other mark:epic is:open")
            })
            .ticket(6, |t| {
                t.title("gamma")
                    .text("kind:bug project:other mark:epic is:open in the text")
            });
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&full(), C).unwrap();
        let reader = std::cell::RefCell::new(site.tickets(&full(), C).unwrap());
        ok(laws::search::first_n_by_relevance(
            |n| t.search("alpha", Within::Titles, None, n),
            4,
        ));
        ok(laws::search::titles_only_matches_titles(
            |within| t.search("alpha", within, None, n32(50)),
            |d: &TicketDigest| d.title.as_str(),
            "alpha",
        ));
        ok(laws::search::words_never_widen(
            |words, within| t.search(words, within, None, n32(50)),
            |d: &TicketDigest| d.title.as_str(),
            |d: &TicketDigest| {
                let whole = reader
                    .borrow_mut()
                    .ticket(&d.key, Depth::All, None)
                    .unwrap();
                format!("{} {}", whole.digest.title, whole.text)
            },
            &["kind:bug", "project:other", "mark:epic", "is:open"],
        ));
    }
    let w = World::split()
        .blueprint(101, |b| b.title("gamma plan"))
        .blueprint(102, |b| b.title("delta").body("gamma in the body"))
        .blueprint(103, |b| b.title("Gamma again"));
    let site = FakeSite::new(&w);
    let mut bl = site.blueprints(&full(), C).unwrap();
    ok(laws::search::first_n_by_relevance(
        |n| bl.search("gamma", Within::Titles, None, n),
        2,
    ));
    ok(laws::search::titles_only_matches_titles(
        |within| bl.search("gamma", within, None, n32(50)),
        |h: &BlueprintHit| h.title.as_str(),
        "gamma",
    ));
}

fn park(no: u64) -> Marker {
    Marker::Park(ParkMarker {
        blueprint: bp(no),
        step: MarkerWord::new("build").unwrap(),
        kind: MarkerWord::new("tests").unwrap(),
        provider: false,
        sig: sha('b'),
    })
}

fn blocked(n: u64) -> Marker {
    Marker::Blocked {
        by: vec![Ref::Here(TicketKey::Number(
            std::num::NonZeroU64::new(n).unwrap(),
        ))],
    }
}

/// The same text three times in a split world: blueprint `n`'s body, a
/// comment on blueprint `n + 50`, and a note on ticket `n`.
fn texts(w: World, n: u64, body: &str, markers: &[Marker]) -> World {
    let m = markers.to_vec();
    w.blueprint(n, |b| {
        markers
            .iter()
            .fold(b.body(body), |b, mk| b.marker(mk.clone()))
    })
    .blueprint(n + 50, |b| b.comment_marked("alice", body, m.clone()))
    .ticket(n, |t| t.comment_marked("alice", body, m))
}

/// Each marker law, over a body, a comment and a note.
#[test]
fn marker_laws() {
    let unreadable = |kind: MarkerKind| Marker::Unreadable { kind };
    let order = vec![
        unreadable(MarkerKind::Blocked),
        blocked(1),
        unreadable(MarkerKind::Blocked),
        Marker::HoldCleared,
    ];
    let bare = |read: Option<Marker>| Marker::Bare {
        kind: MarkerKind::Park,
        read: read.map(Box::new),
    };
    let w = World::split().with_account("alice", AccountKind::Person, Standing::Write);
    let w = texts(w, 101, "> a quoted marker", &[]);
    let w = texts(w, 102, "", &[unreadable(MarkerKind::Park)]);
    let w = texts(w, 103, "", &[unreadable(MarkerKind::Question)]);
    let w = texts(w, 104, "", &[unreadable(MarkerKind::Blocked)]);
    let w = texts(w, 105, "", &[bare(Some(park(105)))]);
    let w = texts(w, 106, "", &[bare(None)]);
    let w = texts(w, 107, "the blocked words, no field", &[]);
    let w = texts(w, 108, "", &order);
    let w = texts(w, 109, "a park of blueprint 101", &[]);
    let w = texts(w, 110, "", &[unreadable(MarkerKind::LastAttempt)]);
    let w = w.blueprint(111, |b| {
        b.marker(Marker::HoldCleared)
            .marker(unreadable(MarkerKind::LastAttempt))
    });
    let site = FakeSite::new(&w);
    let mut bl = site.blueprints(&full(), C).unwrap();
    let mut t = site.tickets(&full(), C).unwrap();
    let comment = |bl: &mut Box<dyn Blueprints + Send>, n: u64| {
        bl.comments(bp(n + 50), b(10)).unwrap().items()[0]
            .id
            .clone()
    };
    let note = |t: &mut Box<dyn Tickets + Send>, n: u64| {
        t.notes(&w.key(n), Window::Last(n32(1))).unwrap().items[0]
            .id
            .clone()
    };
    use laws::markers::{self as m, Text};
    let mut each = |n: u64, law: &dyn Fn(Text<'_>) -> laws::Law| {
        let c = comment(&mut bl, n);
        let k = note(&mut t, n);
        ok(law(Text::Body(bl.as_mut(), bp(n))));
        ok(law(Text::Comment(bl.as_mut(), c)));
        ok(law(Text::Note(t.as_mut(), w.key(n), k)));
    };
    each(101, &m::a_quoted_marker_is_no_marker);
    each(102, &|at| {
        m::a_bad_field_is_unreadable(at, &MarkerKind::Park)
    });
    each(103, &|at| {
        m::leading_zeros_are_unreadable(at, &MarkerKind::Question)
    });
    each(104, &m::a_list_over_its_limit_is_unreadable);
    each(105, &|at| {
        m::a_bare_marker_is_bare(at, &MarkerKind::Park, Some(&park(105)))
    });
    each(106, &|at| {
        m::a_bare_marker_is_bare(at, &MarkerKind::Park, None)
    });
    each(107, &m::words_without_a_field_are_prose);
    each(108, &|at| m::markers_keep_their_order(at, &order));
    each(109, &|at| {
        m::another_items_marker_does_not_read(at, &MarkerKind::Park)
    });
    each(110, &|at| {
        m::a_block_without_its_close_is_unreadable(at, &MarkerKind::LastAttempt)
    });
    let mut h = site.blueprint_writes(&full(), C).unwrap();
    ok(m::an_unreadable_marker_is_kept(h.as_mut(), bp(111)));
    for marker in [
        park(101),
        Marker::HoldCleared,
        Marker::Millwright(MillwrightMarker {
            blueprint: bp(101),
            action: MillwrightAction::Finish,
        }),
    ] {
        ok(m::a_cited_marker_is_no_marker(h.as_mut(), bp(101), &marker));
    }
}

#[test]
fn a_marker_law_finds_a_marker_that_reads() {
    let w = World::split().blueprint(101, |b| b.marker(park(101)));
    let site = FakeSite::new(&w);
    let mut h = site.blueprints(&full(), C).unwrap();
    let e = laws::markers::a_bad_field_is_unreadable(
        laws::markers::Text::Body(h.as_mut(), bp(101)),
        &MarkerKind::Park,
    )
    .unwrap_err();
    assert_eq!(e.law, "markers::a_bad_field_is_unreadable");
}

#[test]
fn the_weak_fixtures_of_the_marker_laws_are_refused() {
    use laws::markers::{self as m, Text};
    let unreadable = Marker::Unreadable {
        kind: MarkerKind::Blocked,
    };
    // An order with no unreadable marker after the good one.
    let order = vec![unreadable.clone(), blocked(1)];
    let w = World::split()
        .blueprint(101, |b| order.iter().fold(b, |b, mk| b.marker(mk.clone())))
        .blueprint(102, |b| b.marker(unreadable.clone()));
    let site = FakeSite::new(&w);
    let mut h = site.blueprint_writes(&full(), C).unwrap();
    let e = m::markers_keep_their_order(Text::Body(h.as_mut(), bp(101)), &order).unwrap_err();
    assert!(e.what.contains("no good marker"), "{}", e.what);
    // A body with no marker that reads.
    let e = m::an_unreadable_marker_is_kept(h.as_mut(), bp(102)).unwrap_err();
    assert!(e.what.contains("no marker that reads"), "{}", e.what);
}

#[test]
fn identity_a_token_covers_its_set_alone() {
    let w = World::new().grant(&["fake/project", "fake/other"]);
    let tokens = Arc::new(MemoryTokens::new(w.clock()));
    let site = FakeSite::new(&w).with_tokens(tokens.clone());
    let mut h = site.identity(&full(), C).unwrap();
    let set = [
        ProjectName::new("fake/project"),
        ProjectName::new("fake/other"),
    ];
    ok(laws::identity::a_token_covers_its_set_alone(
        h.as_mut(),
        tokens.as_ref(),
        Scope::Agent,
        &set,
        &ProjectName::new("fake/elsewhere"),
    ));
    assert!(matches!(
        tokens.asked().as_slice(),
        [Asked::Keep(Covers::Projects { .. }, Scope::Agent)]
    ));
}

#[test]
fn identity_a_given_token_is_never_minted() {
    let w = World::new();
    let now = w.now();
    let given = Token {
        secret: Secret::new("handed-in"),
        until: At::new(now.secs() + 1200),
    };
    let w = w.given("fresh", Scope::Cycle, given.clone()).given(
        "old",
        Scope::Cycle,
        Token {
            secret: Secret::new("stale"),
            until: At::new(now.secs() - 1),
        },
    );
    let tokens = Arc::new(MemoryTokens::new(w.clock()));
    let site = FakeSite::new(&w).with_tokens(tokens.clone());
    let mut h = site
        .identity(&Actor::Given(PersonKey::new("fresh")), C)
        .unwrap();
    ok(laws::identity::a_given_token_is_never_minted(
        h.as_mut(),
        tokens.as_ref(),
        Scope::Cycle,
        &given,
        || site.identity(&Actor::Given(PersonKey::new("old")), C),
        || {
            w.signed_out_keys()
                .iter()
                .filter(|k| **k == PersonKey::new("old"))
                .count()
        },
        || calls(&site),
    ));
    assert_eq!(w.signed_out_keys(), vec![PersonKey::new("old")]);
}

/// An identity handle that answers its given token for any scope asked,
/// the bug the given-token law must catch.
struct AnyScope {
    inner: Box<dyn Identity + Send>,
}

impl Reports for AnyScope {
    fn take_reports(&mut self) -> Vec<Report> {
        self.inner.take_reports()
    }
}

impl Identity for AnyScope {
    fn me(&mut self) -> Got<Account> {
        self.inner.me()
    }
    fn standing(&mut self, who: &AccountId) -> Got<Standing> {
        self.inner.standing(who)
    }
    fn account(&mut self, who: &AccountId) -> Got<Account> {
        self.inner.account(who)
    }
    fn account_by_login(&mut self, login: &Login) -> Got<Account> {
        self.inner.account_by_login(login)
    }
    fn is_organisation(&mut self, name: &Login) -> Got<bool> {
        self.inner.is_organisation(name)
    }
    fn assignable(&mut self, who: &AccountId) -> Got<bool> {
        self.inner.assignable(who)
    }
    fn token(&mut self, _scope: Scope) -> Got<Token> {
        self.inner.token(Scope::Cycle)
    }
    fn token_over(&mut self, scope: Scope, projects: &[ProjectName]) -> Got<Token> {
        self.inner.token_over(scope, projects)
    }
    fn sign_in_link(&self, state: &SignInState) -> WebLink {
        self.inner.sign_in_link(state)
    }
    fn signed_in(&mut self, code: &SignInCode) -> Got<(Account, Secret)> {
        self.inner.signed_in(code)
    }
}

#[test]
fn the_given_token_law_breaks_where_a_wider_scope_is_answered() {
    let w = World::new();
    let now = w.now();
    let given = Token {
        secret: Secret::new("handed-in"),
        until: At::new(now.secs() + 1200),
    };
    let w = w.given("fresh", Scope::Cycle, given.clone()).given(
        "old",
        Scope::Cycle,
        Token {
            secret: Secret::new("stale"),
            until: At::new(now.secs() - 1),
        },
    );
    let tokens = Arc::new(MemoryTokens::new(w.clock()));
    let site = FakeSite::new(&w).with_tokens(tokens.clone());
    let mut h = AnyScope {
        inner: site
            .identity(&Actor::Given(PersonKey::new("fresh")), C)
            .unwrap(),
    };
    let broke = laws::identity::a_given_token_is_never_minted(
        &mut h,
        tokens.as_ref(),
        Scope::Cycle,
        &given,
        || site.identity(&Actor::Given(PersonKey::new("old")), C),
        || {
            w.signed_out_keys()
                .iter()
                .filter(|k| **k == PersonKey::new("old"))
                .count()
        },
        || calls(&site),
    );
    assert!(broke.is_err(), "the law passed a token answered for Full");
}

#[test]
fn the_given_token_law_breaks_where_a_sign_out_is_told_every_time() {
    let w = World::new();
    let now = w.now();
    let given = Token {
        secret: Secret::new("handed-in"),
        until: At::new(now.secs() + 1200),
    };
    let w = w.given("fresh", Scope::Cycle, given.clone()).given(
        "old",
        Scope::Cycle,
        Token {
            secret: Secret::new("stale"),
            until: At::new(now.secs() - 1),
        },
    );
    let tokens = Arc::new(MemoryTokens::new(w.clock()));
    let site = FakeSite::new(&w).with_tokens(tokens.clone());
    let mut h = site
        .identity(&Actor::Given(PersonKey::new("fresh")), C)
        .unwrap();
    // A site that tells the keyring each time the expired token is asked.
    let tells_every_time = || {
        let made = site.identity(&Actor::Given(PersonKey::new("old")), C);
        if made.is_err() {
            Keyring::signed_out(&w, &PersonKey::new("old"));
        }
        made
    };
    let broke = laws::identity::a_given_token_is_never_minted(
        h.as_mut(),
        tokens.as_ref(),
        Scope::Cycle,
        &given,
        tells_every_time,
        || {
            w.signed_out_keys()
                .iter()
                .filter(|k| **k == PersonKey::new("old"))
                .count()
        },
        || calls(&site),
    );
    assert!(broke.is_err(), "the law passed a keyring told every time");
}

#[test]
fn identity_a_keyless_site_never_mints() {
    let w = World::new();
    let tokens = Arc::new(MemoryTokens::new(w.clock()));
    let site = FakeSite::new(&w).with_tokens(tokens.clone()).keyless();
    let mut h = site.identity(&full(), C).unwrap();
    ok(laws::identity::a_keyless_site_never_mints(
        h.as_mut(),
        tokens.as_ref(),
        Scope::Cycle,
        || calls(&site),
    ));
    // A token kept elsewhere is answered from the store.
    let covers = Covers::Site(SiteId::new("fake/project"));
    let kept = Token {
        secret: Secret::new("kept"),
        until: At::new(w.now().secs() + 3600),
    };
    tokens.seed(covers, Scope::Cycle, kept);
    assert_eq!(h.token(Scope::Cycle).unwrap().secret.expose(), "kept");
}

#[test]
fn scopes_an_agent_makes_no_file_write_and_no_push() {
    for w in both() {
        let (blobs, files) = version_file();
        let site = FakeSite::new(&w);
        let mut h = site
            .code_host_writes(&Actor::Ours(Scope::Agent), C)
            .unwrap();
        ok(laws::scopes::an_agent_makes_no_file_write_and_no_push(
            h.as_mut(),
            &proposal(&w, "agent-bump", &files),
            &blobs,
        ));
        // A fake that allows too much breaks it.
        let mut wide = site.code_host_writes(&full(), C).unwrap();
        let e = laws::scopes::an_agent_makes_no_file_write_and_no_push(
            wide.as_mut(),
            &proposal(&w, "wide-bump", &files),
            &blobs,
        )
        .unwrap_err();
        assert_eq!(e.law, "scopes::an_agent_makes_no_file_write_and_no_push");
    }
}

/// A world with change 50 the factory opened, carrying `Refreshing`, and
/// change 51 alice opened; tag `v1.0.0` on main without a release, and that
/// release to make; tag `v0.9.0` with a release that holds no file.
fn scope_world(w: World) -> (World, NewRelease) {
    let w = w
        .change(50, "work", |c| {
            c.title("work").mark(Mark::State(State::Refreshing))
        })
        .change(51, "theirs", |c| c.title("theirs").author("alice"));
    let main = w.head("main").unwrap();
    let w = w
        .tag("v1.0.0", &main)
        .tag("v0.9.0", &main)
        .release("v0.9.0", &[]);
    let release = NewRelease {
        tag: TagName::new("v1.0.0").unwrap(),
        title: "1.0.0".into(),
        notes: Body::Text("notes".into()),
        draft: true,
        prerelease: false,
    };
    (w, release)
}

fn merge_world() -> (World, NewRelease) {
    scope_world(World::new())
}

/// What the refused writes on a change and a release ask with, owned.
struct Asks {
    on: Mark,
    off: Mark,
    status: Status,
    tag: TagName,
    name: FileName,
    sum: Sum,
}

fn asks() -> Asks {
    Asks {
        on: Mark::State(State::Refreshing),
        off: Mark::State(State::AwaitingMerge),
        status: Status {
            name: "tehas/verify".into(),
            state: StatusState::Passed,
            about: "the gate passed".into(),
            link: None,
        },
        tag: TagName::new("v0.9.0").unwrap(),
        name: FileName::new("tehas.tar.gz").unwrap(),
        sum: Sum {
            sha256: sha('a'),
            len: 3,
        },
    }
}

impl Asks {
    fn of<'a>(&'a self, at: &'a CommitId) -> laws::scopes::Refusals<'a> {
        laws::scopes::Refusals {
            no: cn(50),
            on: &self.on,
            off: &self.off,
            at,
            status: &self.status,
            upload: (&self.tag, &self.name, &self.sum),
        }
    }
}

#[test]
fn scopes_a_cycle_merges_tags_and_releases_nothing() {
    let tag = TagName::new("v2.0.0").unwrap();
    for scope in [Scope::Cycle, Scope::Session] {
        let (w, release) = merge_world();
        let head = w.head("work").unwrap();
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&Actor::Ours(scope), C).unwrap();
        ok(laws::scopes::a_cycle_merges_tags_and_releases_nothing(
            h.as_mut(),
            cn(50),
            &head,
            &tag,
            &release,
        ));
    }
    // A fake that allows too much breaks it: the full scope, and a cycle's
    // with the merge grant.
    for actor in [full(), Actor::Ours(Scope::CycleMerge)] {
        let (w, release) = merge_world();
        let head = w.head("work").unwrap();
        let site = FakeSite::new(&w);
        let mut wide = site.code_host_writes(&actor, C).unwrap();
        let e = laws::scopes::a_cycle_merges_tags_and_releases_nothing(
            wide.as_mut(),
            cn(50),
            &head,
            &tag,
            &release,
        )
        .unwrap_err();
        assert_eq!(e.law, "scopes::a_cycle_merges_tags_and_releases_nothing");
    }
}

#[test]
fn scopes_an_agent_merges_tags_and_releases_nothing() {
    const LAW: &str = "scopes::an_agent_merges_tags_and_releases_nothing";
    let tag = TagName::new("v2.0.0").unwrap();
    let run = |w: World, actor: Actor| {
        let (w, release) = scope_world(w);
        let head = w.head("work").unwrap();
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&actor, C).unwrap();
        laws::scopes::an_agent_merges_tags_and_releases_nothing(
            h.as_mut(),
            cn(50),
            &head,
            &tag,
            &release,
        )
    };
    for w in both() {
        ok(run(w, Actor::Ours(Scope::Agent)));
    }
    // Too much: a cycle's with the merge grant merges its own change, and
    // the full scope merges and tags.
    for actor in [Actor::Ours(Scope::CycleMerge), full()] {
        assert_eq!(run(World::new(), actor).unwrap_err().law, LAW);
    }
}

#[test]
fn scopes_a_cycle_marks_closes_and_uploads_nothing() {
    const LAW: &str = "scopes::a_cycle_marks_closes_and_uploads_nothing";
    let run = |w: World, actor: Actor| {
        let (w, _) = scope_world(w);
        let head = w.head("work").unwrap();
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&actor, C).unwrap();
        let a = asks();
        laws::scopes::a_cycle_marks_closes_and_uploads_nothing(h.as_mut(), &a.of(&head), &NoBlobs)
    };
    for scope in [Scope::Cycle, Scope::CycleMerge, Scope::Session] {
        for w in both() {
            ok(run(w, Actor::Ours(scope)));
        }
    }
    // Too much: an agent's token marks and closes a change, and the full
    // scope does everything.
    for actor in [Actor::Ours(Scope::Agent), full()] {
        assert_eq!(run(World::new(), actor).unwrap_err().law, LAW);
    }
}

#[test]
fn scopes_a_cycle_with_the_merge_grant_merges_its_own_change() {
    const LAW: &str = "scopes::a_cycle_with_the_merge_grant_merges_its_own_change";
    let tag = TagName::new("v2.0.0").unwrap();
    let run = |actor: Actor| {
        let (w, release) = merge_world();
        let own = w.head("work").unwrap();
        let other = w.head("theirs").unwrap();
        let site = FakeSite::new(&w);
        let mut h = site.code_host_writes(&actor, C).unwrap();
        laws::scopes::a_cycle_with_the_merge_grant_merges_its_own_change(
            h.as_mut(),
            (cn(50), &own),
            (cn(51), &other),
            &tag,
            &release,
        )
    };
    ok(run(Actor::Ours(Scope::CycleMerge)));
    // Too much: the full scope merges another's change, and tags. Too
    // little: a cycle's without the grant merges nothing.
    for actor in [full(), Actor::Ours(Scope::Cycle)] {
        assert_eq!(run(actor).unwrap_err().law, LAW);
    }
}

#[test]
fn scopes_a_cycle_marks_and_closes_no_ticket() {
    for scope in [Scope::Cycle, Scope::CycleMerge] {
        for w in both() {
            let on = Mark::State(State::Running);
            let off = Mark::State(State::Question);
            let w = w.ticket(1, |t| t.title("work").mark(on.clone()));
            let marked = NewTicket {
                marks: vec![off.clone()],
                ..new_ticket()
            };
            let site = FakeSite::new(&w);
            let mut h = site.ticket_writes(&Actor::Ours(scope), C).unwrap();
            ok(laws::scopes::a_cycle_marks_and_closes_no_ticket(
                h.as_mut(),
                &w.key(1),
                &on,
                &off,
                &marked,
            ));
            // A fake that allows too much breaks it.
            let mut wide = site.ticket_writes(&full(), C).unwrap();
            let e = laws::scopes::a_cycle_marks_and_closes_no_ticket(
                wide.as_mut(),
                &w.key(1),
                &on,
                &off,
                &marked,
            )
            .unwrap_err();
            assert_eq!(e.law, "scopes::a_cycle_marks_and_closes_no_ticket");
        }
    }
}

#[test]
fn each_scope_writes_on_the_code_host_what_it_may() {
    let w = World::new().change(50, "work", |c| c.title("work"));
    let site = FakeSite::new(&w);
    let mark = Mark::State(State::Running);
    let mut agent = site
        .code_host_writes(&Actor::Ours(Scope::Agent), C)
        .unwrap();
    agent.mark_change(cn(50), &mark).unwrap();
    agent.comment_change(cn(50), "a note", &[]).unwrap();
    assert!(agent.remote(Access::Fetch).is_ok());
    for scope in [Scope::Cycle, Scope::CycleMerge, Scope::Session] {
        let mut h = site.code_host_writes(&Actor::Ours(scope), C).unwrap();
        h.comment_change(cn(50), "a note", &[]).unwrap();
        h.edit_change(cn(50), Some("work, retitled"), None, &MarkerEdit::default())
            .unwrap();
        assert!(h.remote(Access::Push).is_ok());
        // The marks on a change are the factory's own writes.
        for (what, answer) in [
            ("mark", h.mark_change(cn(50), &mark)),
            ("unmark", h.unmark_change(cn(50), &mark)),
            ("close", h.close_change(cn(50))),
        ] {
            assert_eq!(answer, Err(Outcome::Forbidden), "{scope:?} {what}");
        }
    }
}

#[test]
fn each_scope_writes_on_tickets_what_it_may() {
    for w in both() {
        let w = w.ticket(1, |t| t.title("work"));
        let site = FakeSite::new(&w);
        let key = w.key(1);
        let rev = |site: &FakeSite| {
            let mut r = site.tickets(&full(), C).unwrap();
            r.ticket(&key, Depth::Cut { notes: 0 }, None)
                .unwrap()
                .digest
                .revision
        };
        // A cycle's relay files, notes and edits, with text alone.
        let mut cycle = site.ticket_writes(&Actor::Ours(Scope::Cycle), C).unwrap();
        cycle.file(&new_ticket()).unwrap();
        cycle.note(&key, "a note", &[]).unwrap();
        let since = rev(&site);
        let none = MarkerEdit::default();
        cycle
            .edit(&key, Some("work, retitled"), None, &none, &since)
            .unwrap();
        let urgent = NewTicket {
            priority: Priority::Urgent,
            ..new_ticket()
        };
        assert_eq!(cycle.file(&urgent).map(|_| ()), Err(Outcome::Forbidden));
        let marker = Marker::HoldCleared;
        assert_eq!(
            cycle.note(&key, "a note", &[marker]).map(|_| ()),
            Err(Outcome::Forbidden),
            "a cycle's note carries no marker"
        );
        // A session, Read and Files write no ticket.
        for scope in [Scope::Session, Scope::Read, Scope::Files] {
            assert_eq!(
                site.ticket_writes(&Actor::Ours(scope), C).map(drop),
                Err(Outcome::Forbidden),
                "{scope:?}"
            );
        }
    }
}

#[test]
fn each_scope_writes_on_blueprints_what_it_may() {
    for w in both() {
        let n = first_blueprint(&w);
        let w = w.blueprint(n, |b| b.title("plan"));
        let site = FakeSite::new(&w);
        let plan = |markers: Vec<Marker>| NewBlueprint {
            title: "a plan".into(),
            body: Body::Text("steps".into()),
            markers,
        };
        let mut cycle = site
            .blueprint_writes(&Actor::Ours(Scope::Cycle), C)
            .unwrap();
        cycle.open(&plan(vec![])).unwrap();
        cycle.comment(bp(n), "a note", &[]).unwrap();
        assert_eq!(
            cycle.open(&plan(vec![Marker::HoldCleared])).map(|_| ()),
            Err(Outcome::Forbidden)
        );
        assert_eq!(cycle.close(bp(n), true), Err(Outcome::Forbidden));
        // A session writes no blueprint.
        assert_eq!(
            site.blueprint_writes(&Actor::Ours(Scope::Session), C)
                .map(drop),
            Err(Outcome::Forbidden)
        );
    }
}

// Health, over a `Faulty` `FakeSite`, a `MemoryLedger` and the sink.

struct Wired {
    world: World,
    ledger: Arc<MemoryLedger>,
    sink: Arc<fake::NoticeLog>,
    host: HostId,
}

impl Wired {
    fn new(world: World) -> Self {
        Self {
            world,
            ledger: Arc::new(MemoryLedger::new()),
            sink: Arc::new(fake::NoticeLog::new()),
            host: HostId::new("code-host"),
        }
    }

    fn watch(&self, deadline: Option<At>) -> Watch {
        self.watch_of("fake/project", deadline)
    }

    fn watch_of(&self, project: &str, deadline: Option<At>) -> Watch {
        Watch {
            host: self.host.clone(),
            realm: Realm::CodeHost,
            site: SiteId::new(project),
            ours: HolderId::new("grant-1"),
            ledger: self.ledger.clone(),
            notices: self.sink.clone(),
            clock: self.world.clock(),
            deadline,
            world: self.world.clone(),
        }
    }

    fn faulty(&self) -> Faulty<FakeSite> {
        Faulty::new(FakeSite::new(&self.world)).watch(self.watch(None))
    }

    fn wiring<'a>(&'a self, requests: &'a dyn Fn() -> u32) -> Wiring<'a> {
        Wiring {
            ledger: self.ledger.as_ref(),
            sink: self.sink.as_ref(),
            host: &self.host,
            requests,
        }
    }

    fn told(&self) -> Vec<Health> {
        self.sink.all().into_iter().map(|n| n.what).collect()
    }
}

#[test]
fn health_held_calls_raise_no_recovered() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("a")));
    let faulty = wired.faulty();
    let actor = Actor::Ours(Scope::Read);
    let mut h = faulty.tickets(&actor, C).unwrap();
    let holder = wired.watch(None).holder(&actor);
    ok(laws::health::held_calls_raise_no_recovered(
        h.as_mut(),
        &wired.world.key(1),
        wired.wiring(&|| faulty.requests()),
        &holder,
        Op::Tickets(TicketsOp::Ticket).pool(),
        wired.world.now(),
    ));
    assert_eq!(faulty.requests(), 0, "a held call sends nothing");
}

#[test]
fn health_past_the_deadline_raises_no_unavailable() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("a")));
    let now = wired.world.now();
    let faulty = Faulty::new(FakeSite::new(&wired.world)).watch(wired.watch(Some(now)));
    let mut h = faulty.tickets(&full(), C).unwrap();
    ok(laws::health::past_the_deadline_raises_no_unavailable(
        h.as_mut(),
        &wired.world.key(1),
        wired.wiring(&|| faulty.requests()),
    ));
    assert_eq!(
        faulty.requests(),
        0,
        "a call past its deadline sends nothing"
    );
}

/// Every call the site's meter counted: its requests.
fn calls(site: &FakeSite) -> u32 {
    site.meter().iter().map(|r| r.calls).sum()
}

/// A token good for an hour, as the store keeps it.
fn good_token(w: &World, secret: &str) -> Token {
    Token {
        secret: Secret::new(secret),
        until: At::new(w.now().secs() + 3600),
    }
}

/// A `Faulty` `FakeSite` whose tokens are `tokens`, watched by `wired`.
fn token_site(wired: &Wired, tokens: &Arc<MemoryTokens>) -> Faulty<FakeSite> {
    Faulty::new(FakeSite::new(&wired.world).with_tokens(tokens.clone())).watch(wired.watch(None))
}

#[test]
fn the_world_logs_each_request_and_no_local_answer() {
    let wired = Wired::new(World::new());
    let tokens = Arc::new(MemoryTokens::new(wired.world.clock()));
    let faulty = token_site(&wired, &tokens);
    let mut h = faulty.identity(&full(), C).unwrap();
    h.token(Scope::Cycle).unwrap();
    assert_eq!((wired.world.sent(), faulty.requests()), (1, 1), "a mint");
    h.token(Scope::Cycle).unwrap();
    assert_eq!((wired.world.sent(), faulty.requests()), (1, 1), "kept");
    assert!(matches!(
        h.token_over(Scope::Cycle, &[]),
        Err(Outcome::Forbidden)
    ));
    assert_eq!((wired.world.sent(), faulty.requests()), (1, 1), "refused");
    h.me().unwrap();
    assert_eq!((wired.world.sent(), faulty.requests()), (2, 2), "a call");
    assert_eq!(wired.world.sent_by(&full()), 2);
    assert_eq!(wired.world.sent_by(&Actor::Anonymous), 0);
}

#[test]
fn health_a_kept_token_raises_no_recovered() {
    let wired = Wired::new(World::new());
    let tokens = Arc::new(MemoryTokens::new(wired.world.clock()));
    let covers = Covers::Site(SiteId::new("fake/project"));
    tokens.seed(covers, Scope::Cycle, good_token(&wired.world, "kept"));
    let faulty = token_site(&wired, &tokens);
    let mut h = faulty.identity(&full(), C).unwrap();
    ok(laws::health::a_kept_token_raises_no_recovered(
        h.as_mut(),
        wired.wiring(&|| faulty.requests()),
        Scope::Cycle,
        wired.world.now(),
    ));
    assert_eq!(faulty.requests(), 0, "a kept token sends nothing");
}

#[test]
fn health_an_expired_token_raises_no_recovered() {
    // A kept token past its time, on a site that cannot mint.
    let wired = Wired::new(World::new());
    let tokens = Arc::new(MemoryTokens::new(wired.world.clock()));
    let covers = Covers::Site(SiteId::new("fake/project"));
    let old = Token {
        secret: Secret::new("old"),
        until: At::new(wired.world.now().secs() - 1),
    };
    tokens.seed(covers, Scope::Cycle, old);
    let site = FakeSite::new(&wired.world)
        .with_tokens(tokens.clone())
        .keyless();
    let faulty = Faulty::new(site).watch(wired.watch(None));
    let mut h = faulty.identity(&full(), C).unwrap();
    ok(laws::health::an_expired_token_raises_no_recovered(
        h.as_mut(),
        wired.wiring(&|| faulty.requests()),
        Scope::Cycle,
        wired.world.now(),
    ));
    assert_eq!(faulty.requests(), 0, "an expired kept token sends nothing");
    // A given token that ran out after its handle was made.
    let w = World::new();
    let soon = good_token(&w, "handed-in");
    let w = w.given("soon", Scope::Cycle, soon);
    let wired = Wired::new(w);
    let faulty = Faulty::new(FakeSite::new(&wired.world)).watch(wired.watch(None));
    let mut h = faulty
        .identity(&Actor::Given(PersonKey::new("soon")), C)
        .unwrap();
    wired.world.advance(3601);
    ok(laws::health::an_expired_token_raises_no_recovered(
        h.as_mut(),
        wired.wiring(&|| faulty.requests()),
        Scope::Cycle,
        wired.world.now(),
    ));
    assert_eq!(faulty.requests(), 0, "an expired given token sends nothing");
}

#[test]
fn health_an_out_of_set_token_raises_no_recovered() {
    let wired = Wired::new(World::new().grant(&["fake/project", "fake/other"]));
    let tokens = Arc::new(MemoryTokens::new(wired.world.clock()));
    let faulty = token_site(&wired, &tokens);
    let mut h = faulty.identity(&full(), C).unwrap();
    let wider = [
        ProjectName::new("fake/project"),
        ProjectName::new("fake/elsewhere"),
    ];
    ok(laws::health::an_out_of_set_token_raises_no_recovered(
        h.as_mut(),
        wired.wiring(&|| faulty.requests()),
        Scope::Agent,
        &wider,
        wired.world.now(),
    ));
    assert_eq!(faulty.requests(), 0, "a refused set sends nothing");
}

/// A wiring that gives every token answer to `health::next`, those made
/// with no request among them: what the token health laws catch.
struct SeesAll<'a> {
    inner: Box<dyn Identity + Send>,
    wired: &'a Wired,
}

impl SeesAll<'_> {
    fn seen<T>(&mut self, answer: Got<T>) -> Got<T> {
        let seen = answer.as_ref().map(|_| ()).map_err(Clone::clone);
        let w = self.wired;
        let was = w.ledger.health(&w.host, &Whose::Host).unwrap();
        let now = w.world.now();
        let (new, notice) = next(&w.host, &Whose::Host, Realm::CodeHost, now, &was, &seen);
        if w.ledger.swap_health(&w.host, &Whose::Host, &was, &new) == Ok(true) {
            notice.into_iter().for_each(|n| w.sink.notice(n));
        }
        answer
    }
}

impl Reports for SeesAll<'_> {
    fn take_reports(&mut self) -> Vec<Report> {
        self.inner.take_reports()
    }
}

impl Identity for SeesAll<'_> {
    fn me(&mut self) -> Got<Account> {
        self.inner.me()
    }
    fn standing(&mut self, who: &AccountId) -> Got<Standing> {
        self.inner.standing(who)
    }
    fn account(&mut self, who: &AccountId) -> Got<Account> {
        self.inner.account(who)
    }
    fn account_by_login(&mut self, login: &Login) -> Got<Account> {
        self.inner.account_by_login(login)
    }
    fn is_organisation(&mut self, name: &Login) -> Got<bool> {
        self.inner.is_organisation(name)
    }
    fn assignable(&mut self, who: &AccountId) -> Got<bool> {
        self.inner.assignable(who)
    }
    fn token(&mut self, scope: Scope) -> Got<Token> {
        let answer = self.inner.token(scope);
        self.seen(answer)
    }
    fn token_over(&mut self, scope: Scope, projects: &[ProjectName]) -> Got<Token> {
        let answer = self.inner.token_over(scope, projects);
        self.seen(answer)
    }
    fn sign_in_link(&self, state: &SignInState) -> WebLink {
        self.inner.sign_in_link(state)
    }
    fn signed_in(&mut self, code: &SignInCode) -> Got<(Account, Secret)> {
        self.inner.signed_in(code)
    }
}

#[test]
fn the_token_health_laws_break_where_local_answers_are_seen() {
    let w = World::new().grant(&["fake/project", "fake/other"]);
    let soon = good_token(&w, "handed-in");
    let w = w.given("soon", Scope::Cycle, soon);
    let wired = Wired::new(w);
    let tokens = Arc::new(MemoryTokens::new(wired.world.clock()));
    let covers = Covers::Site(SiteId::new("fake/project"));
    tokens.seed(covers, Scope::Cycle, good_token(&wired.world, "kept"));
    let site = FakeSite::new(&wired.world).with_tokens(tokens.clone());
    let sees = |actor: &Actor| SeesAll {
        inner: site.identity(actor, C).unwrap(),
        wired: &wired,
    };
    let now = wired.world.now();
    let e = laws::health::a_kept_token_raises_no_recovered(
        &mut sees(&full()),
        wired.wiring(&|| calls(&site)),
        Scope::Cycle,
        now,
    )
    .unwrap_err();
    assert_eq!(e.law, "health::a_kept_token_raises_no_recovered");
    let wider = [
        ProjectName::new("fake/project"),
        ProjectName::new("fake/elsewhere"),
    ];
    let e = laws::health::an_out_of_set_token_raises_no_recovered(
        &mut sees(&full()),
        wired.wiring(&|| calls(&site)),
        Scope::Agent,
        &wider,
        now,
    )
    .unwrap_err();
    assert_eq!(e.law, "health::an_out_of_set_token_raises_no_recovered");
    let mut given = sees(&Actor::Given(PersonKey::new("soon")));
    wired.world.advance(3601);
    let e = laws::health::an_expired_token_raises_no_recovered(
        &mut given,
        wired.wiring(&|| calls(&site)),
        Scope::Cycle,
        wired.world.now(),
    )
    .unwrap_err();
    assert_eq!(e.law, "health::an_expired_token_raises_no_recovered");
}

#[test]
fn a_held_call_given_to_next_would_raise_a_recovery() {
    // What the law guards: the held `Spent`, were it seen, reads as the
    // host up.
    let host = HostId::new("code-host");
    let failing = HealthState::Failing {
        since: At::new(0),
        why: Passing::Timeout,
    };
    let held = Err(Outcome::Spent {
        pool: Pool::Calls,
        until: At::new(10),
    });
    let (_, notice) = tehas_ports::next(
        &host,
        &Whose::Host,
        Realm::CodeHost,
        At::new(1),
        &failing,
        &held,
    );
    assert_eq!(notice.map(|n| n.what), Some(Health::Recovered));
}

// The `Writer`, sending each kind of write to the fake.

#[test]
fn the_writer_sends_each_write_to_its_port_and_answers_its_receipt() {
    let w = World::new()
        .ticket(1, |t| t.title("one").mark(Mark::Epic))
        .ticket(2, |t| t.title("two"))
        .ticket(3, |t| t.title("three").closed(CloseReason::Done))
        .target("target-a", "1.0", None)
        .with_account("alice", AccountKind::Person, Standing::Write)
        .change(50, "work", |c| c.title("work").closes(2));
    let head = w.head("work").unwrap();
    let w = w
        .tag("v1.0.0", &head)
        .blueprint(1, |b| b.comment("alice", "hello"));
    let site = FakeSite::new(&w);
    let mut writer = Writer {
        tickets: site.ticket_writes(&full(), C).unwrap(),
        blueprints: site.blueprint_writes(&full(), C).unwrap(),
        code_host: site.code_host_writes(&full(), C).unwrap(),
    };
    assert!(!writer.read_only());
    let k = |n: u64| w.key(n);
    let text = |s: &str| Body::Text(s.into());
    let mut reader = site.tickets(&full(), C).unwrap();
    let rev = |r: &mut Box<dyn Tickets + Send>, n: u64| {
        r.ticket(&k(n), Depth::Cut { notes: 0 }, None)
            .unwrap()
            .digest
            .revision
    };
    let comment_id = site
        .blueprints(&full(), C)
        .unwrap()
        .comments(bp(1), b(10))
        .unwrap()
        .items()[0]
        .id
        .clone();
    let (blobs, files) = version_file();
    let alice = w.account("alice", Realm::CodeHost).id.unwrap();

    type Want = fn(&Receipt) -> bool;
    let done: Want = |r| *r == Receipt::Done;
    let comment: Want = |r| matches!(r, Receipt::Comment(_));
    let edited: Want = |r| matches!(r, Receipt::Edited(_));
    let mut sent: Vec<WriteKind> = Vec::new();
    let mut send = |writer: &mut Writer, write: Write, want: Want| {
        let receipt = writer
            .send(&write, &blobs)
            .unwrap_or_else(|o| panic!("{:?}: {}", write.kind(), o.line()));
        assert!(want(&receipt), "{:?} answered {receipt:?}", write.kind());
        let last = w.wrote().pop().unwrap();
        assert_eq!(last.what, write, "{:?} recorded as written", write.kind());
        assert_eq!(last.by.login.as_str(), FACTORY);
        sent.push(write.kind());
    };

    send(
        &mut writer,
        Write::Mark {
            ticket: k(1),
            mark: Mark::State(State::Running),
        },
        done,
    );
    send(
        &mut writer,
        Write::Unmark {
            ticket: k(1),
            mark: Mark::Epic,
        },
        done,
    );
    send(
        &mut writer,
        Write::Close {
            ticket: k(2),
            reason: CloseReason::NotPlanned,
        },
        done,
    );
    send(&mut writer, Write::Reopen { ticket: k(2) }, done);
    send(
        &mut writer,
        Write::TicketNote {
            ticket: k(1),
            body: text("a note"),
            markers: vec![Marker::HoldCleared],
        },
        comment,
    );
    send(
        &mut writer,
        Write::FileTicket(NewTicket {
            title: "filed".into(),
            text: text("first line\nmore"),
            markers: vec![],
            marks: vec![Mark::Act(Act::Plan)],
            kind: Some(Kind::Bug),
            priority: Priority::Urgent,
            target: None,
            parent: Some(k(1)),
        }),
        |r| matches!(r, Receipt::Ticket(_)),
    );
    let r1 = rev(&mut reader, 1);
    send(
        &mut writer,
        Write::EditTicket {
            ticket: k(1),
            title: Some("one, retitled".into()),
            text: Some(text("new text")),
            markers: MarkerEdit::default(),
            since: r1,
        },
        edited,
    );
    send(
        &mut writer,
        Write::SetKind {
            ticket: k(1),
            kind: Some(Kind::Feature),
        },
        done,
    );
    send(
        &mut writer,
        Write::SetPriority {
            ticket: k(1),
            priority: Priority::Urgent,
        },
        done,
    );
    send(
        &mut writer,
        Write::WaitOn {
            ticket: k(1),
            on: Item::Ticket(k(2)),
        },
        done,
    );
    send(
        &mut writer,
        Write::StopWaiting {
            ticket: k(1),
            on: Item::Ticket(k(2)),
        },
        done,
    );
    send(
        &mut writer,
        Write::Assign {
            ticket: k(1),
            who: alice,
        },
        done,
    );
    send(
        &mut writer,
        Write::AddPart {
            parent: k(1),
            child: k(2),
        },
        done,
    );
    send(
        &mut writer,
        Write::RemovePart {
            parent: k(1),
            child: k(2),
        },
        done,
    );
    send(
        &mut writer,
        Write::NewTarget {
            title: "2.0".into(),
            due: Some(At::new(5)),
        },
        |r| matches!(r, Receipt::Target(_)),
    );
    send(
        &mut writer,
        Write::EditTarget {
            id: TargetId::new("target-a"),
            edit: TargetEdit::Due(None),
        },
        done,
    );
    send(
        &mut writer,
        Write::SetTarget {
            ticket: k(1),
            target: Some(TargetId::new("target-a")),
        },
        done,
    );
    send(&mut writer, Write::Prepare, done);
    send(
        &mut writer,
        Write::EnsureMark(MarkSpec {
            name: MarkName::new("area: docs").unwrap(),
            colour: Colour::new("0e8a16").ok(),
            about: None,
        }),
        done,
    );
    send(
        &mut writer,
        Write::OpenBlueprint(NewBlueprint {
            title: "a plan".into(),
            body: text("steps"),
            markers: vec![],
        }),
        |r| matches!(r, Receipt::Blueprint(_)),
    );
    let bp_rev = site
        .blueprints(&full(), C)
        .unwrap()
        .blueprint(bp(1), Depth::Cut { notes: 0 })
        .unwrap()
        .revision;
    send(
        &mut writer,
        Write::EditBlueprint {
            no: bp(1),
            title: None,
            body: Some(text("a new body")),
            markers: MarkerEdit::default(),
            since: bp_rev,
        },
        edited,
    );
    send(
        &mut writer,
        Write::Comment {
            on: bp(1),
            body: text("a comment"),
            markers: vec![],
        },
        comment,
    );
    send(
        &mut writer,
        Write::EditComment {
            id: comment_id,
            body: text("hello, edited"),
            markers: MarkerEdit::default(),
            since: Revision::new("r1"),
        },
        edited,
    );
    send(
        &mut writer,
        Write::CloseBlueprint {
            no: bp(1),
            done: false,
        },
        done,
    );
    w.push("other", "other work", FACTORY);
    send(
        &mut writer,
        Write::OpenChange(NewChange {
            head: branch("other"),
            base: branch("main"),
            title: "other".into(),
            body: text("a change"),
            draft: false,
            closes: vec![],
        }),
        |r| matches!(r, Receipt::Change(_)),
    );
    send(
        &mut writer,
        Write::EditChange {
            no: cn(50),
            title: None,
            body: Some(text("stamped")),
            markers: MarkerEdit::default(),
        },
        edited,
    );
    send(
        &mut writer,
        Write::MarkChange {
            no: cn(50),
            mark: Mark::State(State::Refreshing),
        },
        done,
    );
    send(
        &mut writer,
        Write::UnmarkChange {
            no: cn(50),
            mark: Mark::State(State::Refreshing),
        },
        done,
    );
    send(
        &mut writer,
        Write::CommentChange {
            no: cn(50),
            body: text("a word"),
            markers: vec![],
        },
        comment,
    );
    send(
        &mut writer,
        Write::Merge {
            no: cn(50),
            head: head.clone(),
            how: MergeHow::Squash,
        },
        done,
    );
    let opened = site
        .code_host(&full(), C)
        .unwrap()
        .changes(
            &ChangeFilter {
                head: Some(branch("other")),
                ..ChangeFilter::default()
            },
            b(5),
        )
        .unwrap()
        .items()[0]
        .no;
    send(&mut writer, Write::CloseChange { no: opened }, done);
    send(
        &mut writer,
        Write::Propose(proposal(&w, "bump-9", &files)),
        |r| matches!(r, Receipt::Proposed(_)),
    );
    send(
        &mut writer,
        Write::Tag {
            name: TagName::new("v1.0.0").unwrap(),
            at: head.clone(),
        },
        done,
    );
    send(
        &mut writer,
        Write::SetStatus {
            at: head.clone(),
            status: Status {
                name: "plugin".into(),
                state: StatusState::Passed,
                about: "ok".into(),
                link: None,
            },
        },
        done,
    );
    send(
        &mut writer,
        Write::NewRelease(NewRelease {
            tag: TagName::new("v1.0.0").unwrap(),
            title: "1.0.0".into(),
            notes: text("notes"),
            draft: true,
            prerelease: false,
        }),
        |r| matches!(r, Receipt::Release(_)),
    );
    let release = site
        .code_host(&full(), C)
        .unwrap()
        .release(&TagName::new("v1.0.0").unwrap())
        .unwrap()
        .id;
    send(
        &mut writer,
        Write::Upload {
            release: release.clone(),
            name: FileName::new("VERSION").unwrap(),
            file: files[0].1.clone(),
        },
        done,
    );
    send(&mut writer, Write::PublishRelease { release }, done);

    // Every kind but `Unknown` was sent once.
    let mut kinds = sent.clone();
    kinds.sort_by_key(|k| format!("{k:?}"));
    kinds.dedup();
    assert_eq!(kinds.len(), sent.len());
    assert_eq!(sent.len(), 37);
    assert!(!sent.contains(&WriteKind::Unknown));
}

#[test]
fn writes_a_write_is_idempotent_by_its_inputs() {
    let w = World::new();
    let head = w.head("main").unwrap();
    let w = w.tag("v1.0.0", &head);
    let site = FakeSite::new(&w);
    let mut writer = Writer {
        tickets: site.ticket_writes(&full(), C).unwrap(),
        blueprints: site.blueprint_writes(&full(), C).unwrap(),
        code_host: site.code_host_writes(&full(), C).unwrap(),
    };
    let (blobs, files) = version_file();
    let draft = Write::NewRelease(NewRelease {
        tag: TagName::new("v1.0.0").unwrap(),
        title: "1.0.0".into(),
        notes: Body::Text("notes".into()),
        draft: true,
        prerelease: false,
    });
    let Ok(Receipt::Release(release)) = writer.send(&draft, &blobs) else {
        panic!("no draft release");
    };
    let release = release.id;
    let writes = [
        Write::Prepare,
        Write::EnsureMark(MarkSpec {
            name: MarkName::new("area: docs").unwrap(),
            colour: Colour::new("0e8a16").ok(),
            about: None,
        }),
        Write::Propose(proposal(&w, "bump-again", &files)),
        Write::Tag {
            name: TagName::new("v1.1.0").unwrap(),
            at: head.clone(),
        },
        Write::SetStatus {
            at: head.clone(),
            status: Status {
                name: "plugin".into(),
                state: StatusState::Passed,
                about: "ok".into(),
                link: None,
            },
        },
        Write::Upload {
            release: release.clone(),
            name: FileName::new("VERSION").unwrap(),
            file: files[0].1.clone(),
        },
        Write::PublishRelease { release },
    ];
    let mut kinds: Vec<WriteKind> = writes.iter().map(Write::kind).collect();
    kinds.sort_by_key(|k| format!("{k:?}"));
    let mut idempotent = laws::writes::IDEMPOTENT.to_vec();
    idempotent.sort_by_key(|k| format!("{k:?}"));
    assert_eq!(kinds, idempotent, "every write sent again is tried");
    for write in &writes {
        ok(laws::writes::a_write_is_idempotent_by_its_inputs(
            &mut writer,
            write,
            &blobs,
            || w.held(),
        ));
    }
    // A write sent once only is no write the law takes.
    let note = Write::TicketNote {
        ticket: w.key(1),
        body: Body::Text("hello".into()),
        markers: vec![],
    };
    let e =
        laws::writes::a_write_is_idempotent_by_its_inputs(&mut writer, &note, &blobs, || w.held())
            .unwrap_err();
    assert_eq!(e.law, "writes::a_write_is_idempotent_by_its_inputs");
}

/// A writer that files a new ticket on every `Prepare`: what the
/// idempotence law catches.
struct Twice(Writer);

impl Writes for Twice {
    fn send(&mut self, w: &Write, files: &dyn Blobs) -> Got<Receipt> {
        if *w == Write::Prepare {
            self.0.send(&Write::FileTicket(new_ticket()), files)?;
        }
        self.0.send(w, files)
    }
    fn read_only(&self) -> bool {
        false
    }
}

#[test]
fn the_idempotence_law_breaks_where_a_repeat_writes_again() {
    let w = World::new();
    let site = FakeSite::new(&w);
    let mut writer = Twice(Writer {
        tickets: site.ticket_writes(&full(), C).unwrap(),
        blueprints: site.blueprint_writes(&full(), C).unwrap(),
        code_host: site.code_host_writes(&full(), C).unwrap(),
    });
    let e = laws::writes::a_write_is_idempotent_by_its_inputs(
        &mut writer,
        &Write::Prepare,
        &NoBlobs,
        || w.held(),
    )
    .unwrap_err();
    assert_eq!(e.law, "writes::a_write_is_idempotent_by_its_inputs");
    assert!(e.what.contains("changed the site"), "{}", e.what);
}

#[test]
fn the_writer_refuses_an_unknown_write_and_a_summed_body_before_any_request() {
    let w = World::new().ticket(1, |t| t.title("one"));
    let site = FakeSite::new(&w);
    let mut writer = Writer {
        tickets: site.ticket_writes(&full(), C).unwrap(),
        blueprints: site.blueprint_writes(&full(), C).unwrap(),
        code_host: site.code_host_writes(&full(), C).unwrap(),
    };
    let invalid = Err(Outcome::Refused(Refusal::Invalid));
    assert_eq!(writer.send(&Write::Unknown, &NoBlobs), invalid);
    let summed = Body::Summed(Sum {
        sha256: sha('c'),
        len: 3,
    });
    let note = Write::TicketNote {
        ticket: w.key(1),
        body: summed.clone(),
        markers: vec![],
    };
    assert_eq!(writer.send(&note, &NoBlobs), invalid);
    let comment = Write::Comment {
        on: bp(1),
        body: summed,
        markers: vec![],
    };
    assert_eq!(writer.send(&comment, &NoBlobs), invalid);
    assert!(w.wrote().is_empty());
    assert!(site.meter().is_empty(), "no request went out");
}

#[test]
fn the_writer_answers_an_upload_whose_bytes_are_gone_refused() {
    let w = World::new();
    let head = w.head("main").unwrap();
    let w = w.tag("v1", &head).release("v1", &[]);
    let site = FakeSite::new(&w);
    let release = site
        .code_host(&full(), C)
        .unwrap()
        .release(&TagName::new("v1").unwrap())
        .unwrap()
        .id;
    let mut writer = Writer {
        tickets: site.ticket_writes(&full(), C).unwrap(),
        blueprints: site.blueprint_writes(&full(), C).unwrap(),
        code_host: site.code_host_writes(&full(), C).unwrap(),
    };
    let (_, files) = version_file();
    let upload = Write::Upload {
        release,
        name: FileName::new("VERSION").unwrap(),
        file: files[0].1.clone(),
    };
    assert_eq!(
        writer.send(&upload, &NoBlobs),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

// The fake checks writes as a site would.

#[test]
fn the_fake_refuses_as_a_site_would() {
    let w = World::new()
        .ticket(1, |t| t.title("one"))
        .ticket(2, |t| t.title("two"))
        .ticket(3, |t| t.title("closed").closed(CloseReason::Done))
        .with_account("ghost", AccountKind::Person, Standing::Read)
        .assignable("ghost", false)
        .sign_in("code-1", "alice")
        .sign_in("code-2", "alice");
    let site = FakeSite::new(&w);
    let mut t = site.ticket_writes(&full(), C).unwrap();
    t.wait_on(&w.key(1), &Item::Ticket(w.key(2))).unwrap();
    assert_eq!(
        t.wait_on(&w.key(2), &Item::Ticket(w.key(1))),
        Err(Outcome::Refused(Refusal::Cycle))
    );
    assert_eq!(
        t.mark(&w.key(3), &Mark::Epic),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    assert_eq!(
        t.mark(&w.key(1), &Mark::Unknown),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    let ghost = w.account("ghost", Realm::CodeHost).id.unwrap();
    assert_eq!(
        t.assign(&w.key(1), &ghost),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    let unwritable = [Marker::Unreadable {
        kind: MarkerKind::Park,
    }];
    assert_eq!(
        t.note(&w.key(1), "x", &unwritable),
        Err(Outcome::Refused(Refusal::Invalid))
    );
    // Anonymous asks identity about others alone.
    let mut anon = site.identity(&Actor::Anonymous, C).unwrap();
    let ghost_login = Login::new("ghost");
    assert!(anon.is_organisation(&ghost_login).is_ok());
    assert!(anon.account_by_login(&ghost_login).is_ok());
    assert!(anon.account(&ghost).is_ok());
    assert!(anon.signed_in(&SignInCode::new("code-2")).is_ok());
    let forbidden = Err(Outcome::Forbidden);
    assert_eq!(anon.me().map(|_| ()), forbidden, "anonymous has no self");
    assert_eq!(anon.standing(&ghost).map(|_| ()), forbidden);
    assert_eq!(anon.token(Scope::Read).map(|_| ()), forbidden);
    let own = [ProjectName::new("fake/project")];
    assert_eq!(anon.token_over(Scope::Read, &own).map(|_| ()), forbidden);
    let mut id = site.ticket_writes(&Actor::Anonymous, C).map(|_| ());
    assert_eq!(id, Err(Outcome::Forbidden));
    id = site
        .tickets(&Actor::Person(PersonKey::new("nobody")), C)
        .map(|_| ());
    assert_eq!(id, Err(Outcome::SignedOut));
    let mut ident = site.identity(&full(), C).unwrap();
    assert!(ident.signed_in(&SignInCode::new("code-1")).is_ok());
    assert_eq!(
        ident.signed_in(&SignInCode::new("code-1")).map(|_| ()),
        Err(Outcome::Refused(Refusal::Invalid)),
        "a used code"
    );
    let split = World::split();
    let tickets_only = FakeSite::realm(&split, Realm::Tickets);
    assert_eq!(
        tickets_only.board(&full(), C).map(|_| ()),
        Err(Outcome::Refused(Refusal::Unsupported)),
        "a site that cannot join"
    );
    assert_eq!(
        tickets_only.code_host(&full(), C).map(|_| ()),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    // Only the writes that were applied are recorded, as the account.
    let wrote: Vec<Wrote> = w.wrote();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].by.login.as_str(), FACTORY);
}

#[test]
fn a_split_world_keeps_its_three_number_spaces_apart() {
    let w = World::split();
    let site = FakeSite::new(&w);
    let mut t = site.ticket_writes(&full(), C).unwrap();
    let mut bl = site.blueprint_writes(&full(), C).unwrap();
    let filed = t
        .file(&NewTicket {
            title: "t".into(),
            text: Body::Text(String::new()),
            markers: vec![],
            marks: vec![],
            kind: None,
            priority: Priority::Normal,
            target: None,
            parent: None,
        })
        .unwrap();
    assert_eq!(filed.id, w.key(1));
    let opened = bl
        .open(&NewBlueprint {
            title: "b".into(),
            body: Body::Text(String::new()),
            markers: vec![],
        })
        .unwrap();
    assert_eq!(opened.id, bp(101));
    assert_eq!(
        t.what_is(std::num::NonZeroU64::new(1).unwrap()),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
    assert_eq!(
        bl.blueprint(bp(1), Depth::All).map(|_| ()),
        Err(Outcome::NotFound)
    );
    // The setup the world gives a split's two sites joins through its links.
    let w = w.link(1, 101);
    let setup = Setup::Split(Split {
        code_host: Arc::new(FakeSite::realm(&w, Realm::CodeHost)),
        tickets: Arc::new(FakeSite::realm(&w, Realm::Tickets)),
        links: Arc::new(w.clone()),
    });
    let item = setup
        .board(&full(), C)
        .unwrap()
        .work_item(
            &w.key(1),
            &BoardAsk {
                depth: Depth::All,
                history: None,
                changes: None,
                filter: Filter::default(),
                control: None,
            },
        )
        .unwrap();
    assert_eq!(item.blueprints.len(), 1);
    assert_eq!(item.blueprints[0].no, bp(101));
}

// `Faulty`, through the resilience cases.

type Call = fn(&dyn Site, &World) -> Result<(), Outcome>;

fn k1(w: &World) -> TicketKey {
    w.key(1)
}

/// Each cell of the spec's table of faults by port.
fn cells() -> Vec<(Op, Fault, Call)> {
    use BlueprintsOp as B;
    use CodeHostOp as Ch;
    use TicketsOp as T;
    let spent = Fault::Spent {
        pool: Pool::Calls,
        until: At::new(2_000_000),
    };
    let rows: Vec<(Op, Fault, Call)> = vec![
        // Time out.
        (Op::Tickets(T::Ticket), Fault::Timeout, |s, w| {
            s.tickets(&full(), C)?
                .ticket(&k1(w), Depth::All, None)
                .map(drop)
        }),
        (
            Op::Tickets(T::Mark),
            Fault::TimeoutMaybe { applied: false },
            |s, w| s.ticket_writes(&full(), C)?.mark(&k1(w), &Mark::Epic),
        ),
        (Op::Blueprints(B::Comments), Fault::Timeout, |s, _| {
            s.blueprints(&full(), C)?.comments(bp(1), b(5)).map(drop)
        }),
        (
            Op::Blueprints(B::Comment),
            Fault::TimeoutMaybe { applied: false },
            |s, _| {
                s.blueprint_writes(&full(), C)?
                    .comment(bp(1), "x", &[])
                    .map(drop)
            },
        ),
        (Op::CodeHost(Ch::Checks), Fault::Timeout, |s, _| {
            s.code_host(&full(), C)?
                .checks(&CommitId::new("abc1234").unwrap(), None)
                .map(drop)
        }),
        (Op::CodeHost(Ch::Changes), Fault::Timeout, |s, _| {
            s.code_host(&full(), C)?
                .changes(&ChangeFilter::default(), b(5))
                .map(drop)
        }),
        (Op::Identity(IdentityOp::Token), Fault::Timeout, |s, _| {
            s.identity(&full(), C)?.token(Scope::Cycle).map(drop)
        }),
        (Op::Board(BoardOp::OpenWork), Fault::Timeout, |s, _| {
            s.board(&full(), C)?.open_work(&board_ask(), b(5)).map(drop)
        }),
        (Op::Board(BoardOp::OpenChanges), Fault::Timeout, |s, _| {
            s.board(&full(), C)?
                .open_changes(&change_ask(), b(5))
                .map(drop)
        }),
        // Spent.
        (Op::Tickets(T::List), spent, |s, _| {
            s.tickets(&full(), C)?
                .list(&Filter::default(), b(5))
                .map(drop)
        }),
        (Op::Blueprints(B::CommentsById), spent, |s, _| {
            first_err(
                s.blueprints(&full(), C)?
                    .comments_by_id(&[CommentId::new("c")]),
            )
        }),
        (Op::CodeHost(Ch::Changes), spent, |s, _| {
            s.code_host(&full(), C)?
                .changes(&ChangeFilter::default(), b(5))
                .map(drop)
        }),
        (Op::Identity(IdentityOp::Standing), spent, |s, _| {
            s.identity(&full(), C)?
                .standing(&AccountId::new("id-factory"))
                .map(drop)
        }),
        (
            Op::Board(BoardOp::OpenWork),
            Fault::Spent {
                pool: Pool::Priced,
                until: At::new(2_000_000),
            },
            |s, _| s.board(&full(), C)?.open_work(&board_ask(), b(5)).map(drop),
        ),
        // Refused.
        (
            Op::Tickets(T::WaitOn),
            Fault::Refused(Refusal::Cycle),
            |s, w| {
                s.ticket_writes(&full(), C)?
                    .wait_on(&k1(w), &Item::Ticket(k1(w)))
            },
        ),
        (
            Op::Tickets(T::Mark),
            Fault::Refused(Refusal::Invalid),
            |s, w| s.ticket_writes(&full(), C)?.mark(&k1(w), &Mark::Epic),
        ),
        (
            Op::Tickets(T::Assign),
            Fault::Refused(Refusal::Invalid),
            |s, w| {
                s.ticket_writes(&full(), C)?
                    .assign(&k1(w), &AccountId::new("x"))
            },
        ),
        (
            Op::Blueprints(B::EditComment),
            Fault::Refused(Refusal::Invalid),
            |s, _| {
                s.blueprint_writes(&full(), C)?
                    .edit_comment(
                        &CommentId::new("c"),
                        "x",
                        &MarkerEdit::default(),
                        &Revision::new("r1"),
                    )
                    .map(drop)
            },
        ),
        (
            Op::Blueprints(B::Open),
            Fault::Refused(Refusal::Invalid),
            |s, _| {
                s.blueprint_writes(&full(), C)?
                    .open(&NewBlueprint {
                        title: "t".into(),
                        body: Body::Text(String::new()),
                        markers: vec![],
                    })
                    .map(drop)
            },
        ),
        (
            Op::CodeHost(Ch::MarkChange),
            Fault::Refused(Refusal::Invalid),
            |s, _| {
                s.code_host_writes(&full(), C)?
                    .mark_change(cn(1), &Mark::Epic)
            },
        ),
        (
            Op::CodeHost(Ch::Merge),
            Fault::Refused(Refusal::NotMergeable),
            |s, _| {
                s.code_host_writes(&full(), C)?.merge(
                    cn(1),
                    &CommitId::new("abc1234").unwrap(),
                    MergeHow::Merge,
                )
            },
        ),
        (
            Op::CodeHost(Ch::Propose),
            Fault::Refused(Refusal::Exists),
            |s, w| {
                let p = proposal(w, "bump-f", &[]);
                match s.code_host_writes(&full(), C)?.propose(&p, &NoBlobs) {
                    Ok(_) => Ok(()),
                    Err(a) => Err(a.stopped.unwrap().1),
                }
            },
        ),
        (
            Op::Identity(IdentityOp::SignedIn),
            Fault::Refused(Refusal::Invalid),
            |s, _| {
                s.identity(&full(), C)?
                    .signed_in(&SignInCode::new("used"))
                    .map(drop)
            },
        ),
        (
            Op::Board(BoardOp::OpenChanges),
            Fault::Refused(Refusal::Unsupported),
            |s, _| {
                s.board(&full(), C)?
                    .open_changes(&change_ask(), b(5))
                    .map(drop)
            },
        ),
        // Forbidden.
        (Op::Tickets(T::Note), Fault::Forbidden, |s, w| {
            s.ticket_writes(&full(), C)?
                .note(&k1(w), "x", &[])
                .map(drop)
        }),
        (Op::Tickets(T::File), Fault::Forbidden, |s, _| {
            s.ticket_writes(&full(), C)?.file(&new_ticket()).map(drop)
        }),
        (Op::Blueprints(B::Comment), Fault::Forbidden, |s, _| {
            s.blueprint_writes(&full(), C)?
                .comment(bp(1), "x", &[])
                .map(drop)
        }),
        (Op::Blueprints(B::Edit), Fault::Forbidden, |s, _| {
            s.blueprint_writes(&full(), C)?
                .edit(
                    bp(1),
                    None,
                    Some("x"),
                    &MarkerEdit::default(),
                    &Revision::new("r1"),
                )
                .map(drop)
        }),
        (Op::CodeHost(Ch::MarkChange), Fault::Forbidden, |s, _| {
            s.code_host_writes(&full(), C)?
                .mark_change(cn(1), &Mark::Epic)
        }),
        (Op::CodeHost(Ch::Remote), Fault::Forbidden, |s, _| {
            s.code_host(&full(), C)?.remote(Access::Push).map(drop)
        }),
        (Op::Identity(IdentityOp::Token), Fault::Forbidden, |s, _| {
            s.identity(&full(), C)?.token(Scope::Cycle).map(drop)
        }),
        (Op::Board(BoardOp::OpenWork), Fault::Forbidden, |s, _| {
            s.board(&full(), C)?.open_work(&board_ask(), b(5)).map(drop)
        }),
        // Signed out.
        (Op::Tickets(T::Ticket), Fault::SignedOut, |s, w| {
            s.tickets(&full(), C)?
                .ticket(&k1(w), Depth::All, None)
                .map(drop)
        }),
        (Op::Blueprints(B::Blueprint), Fault::SignedOut, |s, _| {
            s.blueprints(&full(), C)?
                .blueprint(bp(1), Depth::All)
                .map(drop)
        }),
        (Op::CodeHost(Ch::Change), Fault::SignedOut, |s, _| {
            s.code_host(&full(), C)?.change(cn(1), b(5)).map(drop)
        }),
        (Op::Identity(IdentityOp::Me), Fault::SignedOut, |s, _| {
            s.identity(&full(), C)?.me().map(drop)
        }),
        (Op::Board(BoardOp::OpenWork), Fault::SignedOut, |s, _| {
            s.board(&full(), C)?.open_work(&board_ask(), b(5)).map(drop)
        }),
        // Odd: cut.
        (Op::Tickets(T::List), Fault::Cut, |s, _| {
            s.tickets(&full(), C)?
                .list(&Filter::default(), b(5))
                .map(drop)
        }),
        (Op::Blueprints(B::Comments), Fault::Cut, |s, _| {
            s.blueprints(&full(), C)?.comments(bp(1), b(5)).map(drop)
        }),
        (Op::CodeHost(Ch::ChangedFiles), Fault::Cut, |s, _| {
            s.code_host(&full(), C)?
                .changed_files(cn(1), b(5))
                .map(drop)
        }),
        (Op::Board(BoardOp::OpenWork), Fault::Cut, |s, _| {
            s.board(&full(), C)?.open_work(&board_ask(), b(5)).map(drop)
        }),
        // Odd: inconsistent.
        (Op::Tickets(T::History), Fault::OutOfOrder, |s, w| {
            s.tickets(&full(), C)?
                .history(&k1(w), Window::Whole(b(5)), None)
                .map(drop)
        }),
        (Op::Blueprints(B::Comments), Fault::Repeated, |s, _| {
            s.blueprints(&full(), C)?.comments(bp(1), b(5)).map(drop)
        }),
        (Op::CodeHost(Ch::Changes), Fault::Repeated, |s, _| {
            s.code_host(&full(), C)?
                .changes(&ChangeFilter::default(), b(5))
                .map(drop)
        }),
        (
            Op::Identity(IdentityOp::AccountByLogin),
            Fault::Inconsistent,
            |s, _| {
                s.identity(&full(), C)?
                    .account_by_login(&Login::new("alice"))
                    .map(drop)
            },
        ),
        (Op::Board(BoardOp::OpenWork), Fault::Inconsistent, |s, _| {
            s.board(&full(), C)?.open_work(&board_ask(), b(5)).map(drop)
        }),
        // Odd: malformed.
        (Op::Tickets(T::Ticket), Fault::Malformed, |s, w| {
            s.tickets(&full(), C)?
                .ticket(&k1(w), Depth::All, None)
                .map(drop)
        }),
        (Op::Blueprints(B::Blueprint), Fault::Malformed, |s, _| {
            s.blueprints(&full(), C)?
                .blueprint(bp(1), Depth::All)
                .map(drop)
        }),
        (Op::CodeHost(Ch::Checks), Fault::Malformed, |s, _| {
            s.code_host(&full(), C)?
                .checks(&CommitId::new("abc1234").unwrap(), None)
                .map(drop)
        }),
        (
            Op::Identity(IdentityOp::Standing),
            Fault::Malformed,
            |s, _| {
                s.identity(&full(), C)?
                    .standing(&AccountId::new("x"))
                    .map(drop)
            },
        ),
        (Op::Board(BoardOp::OpenChanges), Fault::Malformed, |s, _| {
            s.board(&full(), C)?
                .open_changes(&change_ask(), b(5))
                .map(drop)
        }),
        // Odd: too large.
        (Op::Tickets(T::Ticket), Fault::TooLarge, |s, w| {
            s.tickets(&full(), C)?
                .ticket(&k1(w), Depth::All, None)
                .map(drop)
        }),
        (Op::Blueprints(B::Fetch), Fault::TooLarge, |s, _| {
            s.blueprints(&full(), C)?
                .fetch(&AttachmentId::new("a"), 10, &mut Vec::new())
        }),
        (Op::CodeHost(Ch::File), Fault::TooLarge, |s, _| {
            s.code_host(&full(), C)?
                .file(
                    &RepoPath::new("a").unwrap(),
                    &Rev::Branch(branch("main")),
                    10,
                )
                .map(drop)
        }),
        (Op::CodeHost(Ch::Download), Fault::TooLarge, |s, _| {
            s.code_host(&full(), C)?.download(
                &TagName::new("v1").unwrap(),
                &FileName::new("f").unwrap(),
                10,
                &mut Vec::new(),
            )
        }),
        (Op::Board(BoardOp::WorkItem), Fault::TooLarge, |s, w| {
            s.board(&full(), C)?
                .work_item(&k1(w), &board_ask())
                .map(drop)
        }),
    ];
    rows
}

fn board_ask() -> BoardAsk {
    BoardAsk {
        depth: Depth::Cut { notes: 0 },
        history: None,
        changes: None,
        filter: Filter::default(),
        control: None,
    }
}

fn change_ask() -> ChangeAsk {
    ChangeAsk {
        filter: ChangeFilter::default(),
        files: None,
        closing: false,
        siblings: None,
        activity: None,
    }
}

fn new_ticket() -> NewTicket {
    NewTicket {
        title: "t".into(),
        text: Body::Text(String::new()),
        markers: vec![],
        marks: vec![],
        kind: None,
        priority: Priority::Normal,
        target: None,
        parent: None,
    }
}

fn first_err<K, T>(answers: Vec<(K, Got<T>)>) -> Result<(), Outcome> {
    match answers.into_iter().find_map(|(_, a)| a.err()) {
        Some(o) => Err(o),
        None => Ok(()),
    }
}

#[test]
fn every_cell_of_the_resilience_table_answers_its_fault() {
    for w in both() {
        for (op, fault, call) in cells() {
            let faulty = Faulty::new(FakeSite::new(&w)).on(op, fault);
            let answer = call(&faulty, &w);
            assert_eq!(answer, Err(fault.outcome()), "{op:?} {fault:?}");
            assert_eq!(faulty.requests(), 1, "{op:?}: one request");
        }
    }
}

#[test]
fn a_fault_answers_its_calls_then_lets_them_through() {
    let w = World::new().ticket(1, |t| t.title("one"));
    let faulty = Faulty::new(FakeSite::new(&w))
        .on(Op::Board(BoardOp::OpenWork), Fault::Timeout)
        .times(2);
    let mut board = faulty.board(&full(), C).unwrap();
    let timeout = Outcome::Unavailable {
        why: Passing::Timeout,
        sent: Sent::No,
    };
    assert_eq!(
        board.open_work(&board_ask(), b(5)).map(drop),
        Err(timeout.clone())
    );
    assert_eq!(
        board.open_work(&board_ask(), b(5)).map(drop),
        Err(timeout.clone())
    );
    let work = board.open_work(&board_ask(), b(5)).unwrap();
    assert_eq!(work.items.items().len(), 1);
    // `after` lets the first calls through.
    let faulty = Faulty::new(FakeSite::new(&w))
        .on(Op::Tickets(TicketsOp::Ticket), Fault::Malformed)
        .after(1)
        .times(1);
    let mut t = faulty.tickets(&full(), C).unwrap();
    assert!(t.ticket(&w.key(1), Depth::All, None).is_ok());
    assert_eq!(
        t.ticket(&w.key(1), Depth::All, None).map(drop),
        Err(Outcome::Unread(Unread::Malformed))
    );
    assert!(t.ticket(&w.key(1), Depth::All, None).is_ok());
    let reports = t.take_reports();
    assert_eq!(
        reports,
        vec![Report {
            port: PortName::Tickets,
            outcome: Outcome::Unread(Unread::Malformed),
            calls: 1
        }]
    );
}

#[test]
fn a_write_that_may_have_landed_is_applied_or_not_as_the_test_says() {
    for applied in [true, false] {
        let w = World::new().ticket(1, |t| t.title("one"));
        let faulty = Faulty::new(FakeSite::new(&w)).on(
            Op::Tickets(TicketsOp::Note),
            Fault::TimeoutMaybe { applied },
        );
        let mut t = faulty.ticket_writes(&full(), C).unwrap();
        assert_eq!(
            t.note(&w.key(1), "a note", &[]).map(drop),
            Err(Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe
            })
        );
        assert_eq!(w.wrote().len(), usize::from(applied));
    }
}

#[test]
fn an_outage_is_reported_once_and_its_end_once_across_processes_and_projects() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    let other = World::new().ticket(1, |t| t.title("other"));
    // Two passes of one project and one of another on the same host time
    // out; then two passes answer.
    for (world, project) in [
        (&wired.world, "fake/project"),
        (&wired.world, "fake/project"),
        (&other, "fake/other"),
    ] {
        let faulty = Faulty::new(FakeSite::new(world))
            .on(Op::Board(BoardOp::OpenWork), Fault::Timeout)
            .watch(wired.watch_of(project, None));
        let mut board = faulty.board(&full(), C).unwrap();
        assert!(board.open_work(&board_ask(), b(5)).is_err());
        let reports = board.take_reports();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].calls, 1);
        wired.world.advance(1);
    }
    for _ in 0..2 {
        let faulty = wired.faulty();
        let mut board = faulty.board(&full(), C).unwrap();
        assert!(board.open_work(&board_ask(), b(5)).is_ok());
    }
    assert_eq!(
        wired.told(),
        vec![Health::Unavailable(Passing::Timeout), Health::Recovered]
    );
    // The health check reads the three failures, with our fixed line.
    let failures = wired.ledger.failures(&wired.host, At::new(0)).unwrap();
    assert_eq!(failures.len(), 3);
    assert_eq!(failures[2].site, SiteId::new("fake/other"));
    assert_eq!(failures[0].outcome.line(), "timed out before sending");
}

#[test]
fn a_spent_pool_holds_its_holder_on_every_project_and_recovers_once() {
    let wired = Wired::new(
        World::new()
            .ticket(1, |t| t.title("one"))
            .person("alice-key", "alice"),
    );
    let until = At::new(wired.world.now().secs() + 600);
    let spent = Fault::Spent {
        pool: Pool::Priced,
        until,
    };
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .on(Op::Board(BoardOp::OpenWork), spent)
        .times(1)
        .watch(wired.watch(None));
    let mut board = faulty.board(&full(), C).unwrap();
    let out = Err(Outcome::Spent {
        pool: Pool::Priced,
        until,
    });
    assert_eq!(board.open_work(&board_ask(), b(5)).map(drop), out);
    assert_eq!(faulty.requests(), 1);
    // The factory's next call in the pool is held: no request.
    assert_eq!(board.open_work(&board_ask(), b(5)).map(drop), out);
    assert_eq!(faulty.requests(), 1);
    // Its calls in another pool go on.
    let mut t = faulty.tickets(&full(), C).unwrap();
    assert!(t.ticket(&wired.world.key(1), Depth::All, None).is_ok());
    // A person, another holder, still reads in that pool.
    let person = Actor::Person(PersonKey::new("alice-key"));
    let mut theirs = faulty.board(&person, C).unwrap();
    assert!(theirs.open_work(&board_ask(), b(5)).is_ok());
    // A second project on the same host, read as the factory, is held too.
    let other = World::new();
    let second = Faulty::new(FakeSite::new(&other)).watch(wired.watch_of("fake/other", None));
    let mut b2 = second.board(&full(), C).unwrap();
    assert_eq!(b2.open_work(&board_ask(), b(5)).map(drop), out);
    assert_eq!(second.requests(), 0);
    // Past `until`, a call goes out and the pool recovers, once.
    wired.world.set_now(until);
    assert!(b2.open_work(&board_ask(), b(5)).is_ok());
    assert!(board.open_work(&board_ask(), b(5)).is_ok());
    assert_eq!(
        wired.told(),
        vec![
            Health::Spent {
                pool: Pool::Priced,
                until
            },
            Health::Recovered
        ]
    );
}

#[test]
fn a_spent_named_for_another_pool_holds_that_pool() {
    // A read in the Calls pool answered with a spent Priced pool: the spend
    // goes to the pool the answer names (`health::keys`), so the next
    // Priced read is held with no request.
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    let until = At::new(wired.world.now().secs() + 600);
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .on(
            Op::Tickets(TicketsOp::Ticket),
            Fault::Spent {
                pool: Pool::Priced,
                until,
            },
        )
        .times(1)
        .watch(wired.watch(None));
    let mut t = faulty.tickets(&full(), C).unwrap();
    assert!(t.ticket(&wired.world.key(1), Depth::All, None).is_err());
    assert_eq!(faulty.requests(), 1);
    let mut board = faulty.board(&full(), C).unwrap();
    assert_eq!(
        board.open_work(&board_ask(), b(5)).map(drop),
        Err(Outcome::Spent {
            pool: Pool::Priced,
            until
        })
    );
    assert_eq!(faulty.requests(), 1);
    assert_eq!(
        wired.told(),
        vec![Health::Spent {
            pool: Pool::Priced,
            until
        }]
    );
}

#[test]
fn a_sign_out_is_reported_once_and_cleared_by_a_good_call() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    for _ in 0..2 {
        let faulty = Faulty::new(FakeSite::new(&wired.world))
            .on(Op::Tickets(TicketsOp::Ticket), Fault::SignedOut)
            .times(1)
            .watch(wired.watch(None));
        let mut t = faulty.tickets(&full(), C).unwrap();
        assert_eq!(
            t.ticket(&wired.world.key(1), Depth::All, None).map(drop),
            Err(Outcome::SignedOut)
        );
    }
    let faulty = wired.faulty();
    let mut t = faulty.tickets(&full(), C).unwrap();
    assert!(t.ticket(&wired.world.key(1), Depth::All, None).is_ok());
    assert!(t.ticket(&wired.world.key(1), Depth::All, None).is_ok());
    let whose: Vec<(Whose, Health)> = wired
        .sink
        .all()
        .into_iter()
        .map(|n| (n.whose, n.what))
        .collect();
    let ours = Whose::Holder(Holder::Ours(HolderId::new("grant-1")));
    assert_eq!(
        whose,
        vec![(ours.clone(), Health::SignedOut), (ours, Health::Recovered)]
    );
}

#[test]
fn odd_answers_and_refusals_raise_no_notice() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .on(Op::Tickets(TicketsOp::List), Fault::Cut)
        .on(Op::Tickets(TicketsOp::History), Fault::OutOfOrder)
        .on(
            Op::Tickets(TicketsOp::Mark),
            Fault::Refused(Refusal::Invalid),
        )
        .watch(wired.watch(None));
    let mut t = faulty.ticket_writes(&full(), C).unwrap();
    assert!(t.list(&Filter::default(), b(5)).is_err());
    assert!(t
        .history(&wired.world.key(1), Window::Whole(b(5)), None)
        .is_err());
    assert!(t.mark(&wired.world.key(1), &Mark::Epic).is_err());
    assert!(wired.sink.all().is_empty());
    let mut reports = t.take_reports();
    reports.sort_by_key(|r| format!("{:?}", r.outcome));
    assert_eq!(reports.len(), 3);
}

#[test]
fn a_ledger_that_fails_fails_toward_reporting() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    wired.ledger.set_down(true);
    for _ in 0..2 {
        let faulty = Faulty::new(FakeSite::new(&wired.world))
            .on(Op::Board(BoardOp::OpenWork), Fault::Timeout)
            .watch(wired.watch(None));
        let mut board = faulty.board(&full(), C).unwrap();
        assert!(board.open_work(&board_ask(), b(5)).is_err());
        assert!(board.open_work(&board_ask(), b(5)).is_err());
    }
    let told = wired.told();
    assert!(told.contains(&Health::Unavailable(Passing::Timeout)));
    // Once per handle: two handles, two notices of the ledger.
    assert_eq!(wired.sink.count(&Health::LedgerDown), 2);
    assert!(
        wired.ledger.failures(&wired.host, At::new(0)).is_err(),
        "the check reads failing"
    );
}

#[test]
fn the_memory_ledger_swaps_only_from_the_state_expected() {
    let ledger = MemoryLedger::new();
    let host = HostId::new("h");
    let failing = HealthState::Failing {
        since: At::new(1),
        why: Passing::Server,
    };
    assert_eq!(
        ledger.swap_health(&host, &Whose::Host, &failing, &HealthState::Healthy),
        Ok(false)
    );
    assert_eq!(
        ledger.swap_health(&host, &Whose::Host, &HealthState::Healthy, &failing),
        Ok(true)
    );
    assert_eq!(ledger.health(&host, &Whose::Host), Ok(failing));
    assert_eq!(
        ledger.swap_health(&host, &Whose::Host, &failing, &HealthState::Healthy),
        Ok(true)
    );
    assert!(ledger.states().is_empty(), "a healthy state is not kept");
    let holder = Holder::Ours(HolderId::new("g"));
    let b1 = Budget {
        left: 5,
        resets: At::new(9),
    };
    ledger.put_budget(&host, &holder, Pool::Search, b1).unwrap();
    assert_eq!(ledger.budget(&host, &holder, Pool::Search), Ok(Some(b1)));
    assert_eq!(ledger.budget(&host, &holder, Pool::Calls), Ok(None));
    let f = |at: i64| Failure {
        at: At::new(at),
        site: SiteId::new("p"),
        outcome: Outcome::SignedOut,
    };
    ledger.failed(&host, f(10), 600).unwrap();
    ledger.failed(&host, f(700), 600).unwrap();
    assert_eq!(
        ledger.failures(&host, At::new(0)).unwrap(),
        vec![f(700)],
        "older than keep is dropped"
    );
}

#[test]
fn memory_tokens_keep_and_forget_by_what_they_cover() {
    let clock = fake::Clock::new(At::new(100));
    let tokens = MemoryTokens::new(clock.clone());
    let site = Covers::Site(SiteId::new("p"));
    let t = Token {
        secret: Secret::new("s"),
        until: At::new(800),
    };
    assert!(tokens.keep(&site, Scope::Cycle, &t));
    assert!(tokens.kept(&site, Scope::Cycle, 600).is_some());
    assert!(tokens.kept(&site, Scope::Session, 600).is_none());
    clock.advance(200);
    assert!(
        tokens.kept(&site, Scope::Cycle, 600).is_none(),
        "less than ten minutes left"
    );
    assert!(tokens.keep_fact(&SiteId::new("p"), Kept::Me, "factory"));
    assert!(tokens.forget(&site, None));
    assert!(tokens.fact(&SiteId::new("p"), Kept::Me).is_none());
    tokens.set_failing(true);
    assert!(!tokens.keep(&site, Scope::Cycle, &t));
    assert!(tokens.lock(&site, Scope::Cycle, 1).is_none());
    assert_eq!(
        tokens.asked(),
        vec![
            Asked::Keep(site.clone(), Scope::Cycle),
            Asked::Forget(site.clone(), None),
            Asked::Keep(site, Scope::Cycle),
        ]
    );
}

#[test]
fn the_fakes_meter_counts_calls_by_caller_port_and_pool() {
    let w = World::new().ticket(1, |t| t.title("alpha"));
    let site = FakeSite::new(&w);
    let mut t = site.ticket_writes(&full(), C).unwrap();
    t.ticket(&w.key(1), Depth::All, None).unwrap();
    t.search("alpha", Within::Titles, None, n32(5)).unwrap();
    t.mark(&w.key(1), &Mark::Epic).unwrap();
    t.mark(&w.key(1), &Mark::Epic).unwrap();
    let mut meter = site.meter();
    meter.sort_by_key(|r| format!("{:?}", r.pool));
    let rows: Vec<(Pool, u32)> = meter.iter().map(|r| (r.pool, r.calls)).collect();
    assert_eq!(
        rows,
        vec![(Pool::Calls, 1), (Pool::Search, 1), (Pool::Writes, 2)]
    );
    assert!(meter
        .iter()
        .all(|r| r.caller == C && r.port == PortName::Tickets));
}

#[test]
fn an_operator_a_deploy_key_and_a_given_token_are_holders_apart_from_the_factory() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    let watch = wired.watch(None);
    let factory = watch.holder(&full());
    let operator = watch.holder(&Actor::Operator);
    let deploy = watch.holder(&Actor::Deploy(PersonKey::new("ci")));
    let given = watch.holder(&Actor::Given(PersonKey::new("reader")));
    let anyone = watch.holder(&Actor::Anonymous);
    let all = [&factory, &operator, &deploy, &given, &anyone];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b, "two actors share a holder");
        }
    }
    assert!(matches!(operator, Holder::Person(_)));
    // An operator's refused sign-in is the operator's: the factory's own
    // health stays as it was.
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .on(Op::Tickets(TicketsOp::Ticket), Fault::SignedOut)
        .by(Actor::Operator)
        .watch(watch);
    let mut t = faulty.tickets(&Actor::Operator, C).unwrap();
    assert_eq!(
        t.ticket(&wired.world.key(1), Depth::All, None).map(drop),
        Err(Outcome::SignedOut)
    );
    assert_eq!(
        wired.ledger.health(&wired.host, &Whose::Holder(factory)),
        Ok(HealthState::Healthy)
    );
    assert_eq!(
        wired
            .ledger
            .health(&wired.host, &Whose::Holder(operator))
            .map(|s| matches!(s, HealthState::SignedOut { .. })),
        Ok(true)
    );
}

#[test]
fn two_sessions_of_one_person_share_one_holder() {
    let wired = Wired::new(
        World::new()
            .person("phone", "alice")
            .person("laptop", "alice"),
    );
    let phone = Actor::Person(PersonKey::new("phone"));
    let laptop = Actor::Person(PersonKey::new("laptop"));
    let watch = wired.watch(None);
    assert_eq!(watch.holder(&phone), watch.holder(&laptop));
    let until = At::new(wired.world.now().secs() + 600);
    let spent = Fault::Spent {
        pool: Pool::Priced,
        until,
    };
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .on(Op::Board(BoardOp::OpenWork), spent)
        .by(phone.clone())
        .watch(watch);
    let out = Err(Outcome::Spent {
        pool: Pool::Priced,
        until,
    });
    let mut on_phone = faulty.board(&phone, C).unwrap();
    assert_eq!(on_phone.open_work(&board_ask(), b(5)).map(drop), out);
    // The other session is the same account: held, with no request.
    let mut on_laptop = faulty.board(&laptop, C).unwrap();
    assert_eq!(on_laptop.open_work(&board_ask(), b(5)).map(drop), out);
    assert_eq!(faulty.requests_by(&phone), 1);
    assert_eq!(faulty.requests_by(&laptop), 0);
    // The factory is another holder, and the rule is not its.
    let mut ours = faulty.board(&full(), C).unwrap();
    assert!(ours.open_work(&board_ask(), b(5)).is_ok());
    assert_eq!(faulty.requests_by(&full()), 1);
    assert_eq!(faulty.requests(), 2);
}

#[test]
fn a_signed_out_credential_answers_signed_out_and_tells_the_keyring_once() {
    let w = World::new()
        .ticket(1, |t| t.title("one"))
        .person("alice-key", "alice");
    let site = FakeSite::new(&w);
    let key = PersonKey::new("alice-key");
    let mut t = site.tickets(&Actor::Person(key.clone()), C).unwrap();
    assert!(t.ticket(&w.key(1), Depth::All, None).is_ok());
    w.sign_out(&key);
    for _ in 0..2 {
        assert_eq!(
            t.ticket(&w.key(1), Depth::All, None).map(drop),
            Err(Outcome::SignedOut)
        );
    }
    assert_eq!(w.signed_out_keys(), vec![key.clone()]);
    assert_eq!(
        site.tickets(&Actor::Person(key), C).map(drop),
        Err(Outcome::SignedOut)
    );
}

#[test]
fn an_op_draws_on_the_pools_the_test_sets() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    let until = At::new(wired.world.now().secs() + 600);
    let ticket = Op::Tickets(TicketsOp::Ticket);
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .draws(ticket, &[Pool::Calls, Pool::Priced])
        .watch(wired.watch(None));
    let holder = wired.watch(None).holder(&full());
    let whose = Whose::Pool(holder, Pool::Priced);
    let spent = HealthState::Spent {
        pool: Pool::Priced,
        until,
    };
    assert_eq!(
        wired
            .ledger
            .swap_health(&wired.host, &whose, &HealthState::Healthy, &spent),
        Ok(true)
    );
    let mut t = faulty.tickets(&full(), C).unwrap();
    assert_eq!(
        t.ticket(&wired.world.key(1), Depth::All, None).map(drop),
        Err(Outcome::Spent {
            pool: Pool::Priced,
            until
        }),
        "a call drawing on a held pool is held"
    );
    assert_eq!(faulty.requests(), 0);
}

#[test]
fn a_server_error_may_have_written_and_a_broken_check_is_the_host_up() {
    let wired = Wired::new(World::new().ticket(1, |t| t.title("one")));
    let mark = Op::Tickets(TicketsOp::Mark);
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .on(mark, Fault::Server { applied: true })
        .times(1)
        .on(mark, Fault::Server { applied: false })
        .after(1)
        .times(1)
        .watch(wired.watch(None));
    let mut t = faulty.ticket_writes(&full(), C).unwrap();
    let server = Err(Outcome::Unavailable {
        why: Passing::Server,
        sent: Sent::Maybe,
    });
    assert_eq!(t.mark(&wired.world.key(1), &Mark::Epic), server);
    assert_eq!(wired.world.wrote().len(), 1, "applied, then the error");
    assert_eq!(t.unmark(&wired.world.key(1), &Mark::Epic), Ok(()));
    assert_eq!(t.mark(&wired.world.key(1), &Mark::Epic), server);
    assert_eq!(wired.world.wrote().len(), 2, "the second not applied");
    // `Broken` ends an outage: the site answered.
    let failing = HealthState::Failing {
        since: wired.world.now(),
        why: Passing::Timeout,
    };
    let host = Whose::Host;
    let was = wired.ledger.health(&wired.host, &host).unwrap();
    assert_eq!(
        wired.ledger.swap_health(&wired.host, &host, &was, &failing),
        Ok(true)
    );
    let checks = Op::CodeHost(CodeHostOp::Checks);
    let broken = Faulty::new(FakeSite::new(&wired.world))
        .on(checks, Fault::Broken)
        .watch(wired.watch(None));
    let head = wired.world.head("main").unwrap();
    let mut c = broken.code_host(&full(), C).unwrap();
    assert!(c.checks(&head, None).is_err());
    assert_eq!(
        wired.ledger.health(&wired.host, &host),
        Ok(HealthState::Healthy)
    );
    assert!(wired.told().contains(&Health::Recovered));
    let since = At::new(0);
    let failures = wired.ledger.failures(&wired.host, since).unwrap();
    let whys: Vec<Passing> = failures
        .iter()
        .filter_map(|f| match f.outcome {
            Outcome::Unavailable { why, .. } => Some(why),
            _ => None,
        })
        .collect();
    assert_eq!(
        whys,
        vec![Passing::Server, Passing::Server],
        "a failed check run is no failure of the site"
    );
}

#[test]
fn a_batch_is_answered_when_the_site_answered() {
    let wired = Wired::new(
        World::new()
            .ticket(1, |t| t.title("one"))
            .person("alice-key", "alice"),
    );
    // Ticket 9 is not there: that item's, not the call's.
    let faulty = Faulty::new(FakeSite::new(&wired.world))
        .on(Op::Tickets(TicketsOp::List), Fault::Cut)
        .on(Op::Tickets(TicketsOp::Ticket), Fault::TooLarge)
        .watch(wired.watch(None));
    let failing = HealthState::Failing {
        since: wired.world.now(),
        why: Passing::Timeout,
    };
    let was = wired.ledger.health(&wired.host, &Whose::Host).unwrap();
    wired
        .ledger
        .swap_health(&wired.host, &Whose::Host, &was, &failing)
        .unwrap();
    let mut t = faulty.tickets(&full(), C).unwrap();
    let answers = t.several(&[wired.world.key(1), wired.world.key(9)]);
    assert!(answers[0].1.is_ok() && answers[1].1.is_err());
    assert_eq!(
        wired.ledger.health(&wired.host, &Whose::Host),
        Ok(HealthState::Healthy),
        "the site answered the batch"
    );
    // Odd answers and a person's sign-out are no failures of the site.
    assert!(t.list(&Filter::default(), b(5)).is_err());
    assert!(t.ticket(&wired.world.key(1), Depth::All, None).is_err());
    let person = Actor::Person(PersonKey::new("alice-key"));
    let theirs = Faulty::new(FakeSite::new(&wired.world))
        .on(Op::Tickets(TicketsOp::Ticket), Fault::SignedOut)
        .watch(wired.watch(None));
    let mut p = theirs.tickets(&person, C).unwrap();
    assert!(p.ticket(&wired.world.key(1), Depth::All, None).is_err());
    let failures = wired.ledger.failures(&wired.host, At::new(0)).unwrap();
    assert!(failures.is_empty(), "kept {failures:?}");
    // The factory's own sign-out is.
    let ours = Faulty::new(FakeSite::new(&wired.world))
        .on(Op::Tickets(TicketsOp::Ticket), Fault::SignedOut)
        .watch(wired.watch(None));
    let mut o = ours.tickets(&full(), C).unwrap();
    assert!(o.ticket(&wired.world.key(1), Depth::All, None).is_err());
    assert_eq!(
        wired
            .ledger
            .failures(&wired.host, At::new(0))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn triage_is_below_write_and_maintain_is_not() {
    let w = World::split()
        .person("tia-key", "tia")
        .person("max-key", "max")
        .with_account("tia", AccountKind::Person, Standing::Triage)
        .with_account("max", AccountKind::Person, Standing::Maintain)
        .blueprint(101, |b| b.title("theirs").author("alice"))
        .blueprint(102, |b| b.title("also theirs").author("alice"));
    let site = FakeSite::new(&w);
    let tia = Actor::Person(PersonKey::new("tia-key"));
    let max = Actor::Person(PersonKey::new("max-key"));
    let mut bl = site.blueprint_writes(&tia, C).unwrap();
    assert_eq!(bl.close(bp(101), true), Err(Outcome::Forbidden));
    let mut bl = site.blueprint_writes(&max, C).unwrap();
    assert_eq!(bl.close(bp(102), true), Ok(()));
    let mut id = site.identity(&full(), C).unwrap();
    let tia = id.account_by_login(&Login::new("tia")).unwrap();
    let standing = id.standing(tia.id.as_ref().unwrap()).unwrap();
    assert_eq!(standing, Standing::Triage);
}

#[test]
fn a_person_below_write_comments_files_and_edits_their_own() {
    let w = World::split()
        .person("bob-key", "bob")
        .blueprint(101, |b| b.title("theirs").author("alice"))
        .blueprint(102, |b| b.title("bob's").author("bob"))
        .change(501, "work", |c| c.title("work"));
    let head = w.head("work").unwrap();
    let site = FakeSite::new(&w);
    let bob = Actor::Person(PersonKey::new("bob-key"));
    let mut bl = site.blueprint_writes(&bob, C).unwrap();
    let posted = bl.comment(bp(101), "a comment", &[]).unwrap();
    let mine = bl.blueprint(bp(102), Depth::All).unwrap();
    assert!(bl
        .edit(
            bp(102),
            Some("bob's, edited"),
            None,
            &MarkerEdit::default(),
            &mine.revision
        )
        .is_ok());
    let theirs = bl.blueprint(bp(101), Depth::All).unwrap();
    assert_eq!(
        bl.edit(
            bp(101),
            Some("taken"),
            None,
            &MarkerEdit::default(),
            &theirs.revision
        )
        .map(drop),
        Err(Outcome::Forbidden),
        "another's blueprint"
    );
    let comment = bl.comments_by_id(std::slice::from_ref(&posted.id));
    let rev = comment[0].1.as_ref().unwrap().revision.clone();
    assert!(bl
        .edit_comment(&posted.id, "edited", &MarkerEdit::default(), &rev)
        .is_ok());
    assert_eq!(bl.close(bp(102), true), Err(Outcome::Forbidden));
    let mut t = site.ticket_writes(&bob, C).unwrap();
    assert!(t.file(&new_ticket()).is_ok());
    let mut c = site.code_host_writes(&bob, C).unwrap();
    assert!(c.comment_change(cn(501), "a note", &[]).is_ok());
    assert_eq!(
        c.merge(cn(501), &head, MergeHow::Squash),
        Err(Outcome::Forbidden),
        "no merge below Write"
    );
    assert_eq!(c.remote(Access::Push).map(drop), Err(Outcome::Forbidden));
}

#[test]
fn a_given_token_does_what_its_scope_may() {
    let w = World::new().change(50, "work", |c| c.title("work"));
    let head = w.head("work").unwrap();
    let token = good_token(&w, "handed-in");
    let w = w.given("cycle", Scope::Cycle, token);
    let site = FakeSite::new(&w);
    let given = Actor::Given(PersonKey::new("cycle"));
    let mut t = site.ticket_writes(&given, C).unwrap();
    assert_eq!(
        t.close(&w.key(1), &CloseReason::Done),
        Err(Outcome::Forbidden)
    );
    let mut c = site.code_host_writes(&given, C).unwrap();
    assert!(c.comment_change(cn(50), "a note", &[]).is_ok());
    assert_eq!(
        c.merge(cn(50), &head, MergeHow::Squash),
        Err(Outcome::Forbidden)
    );
}

#[test]
fn the_world_builds_a_named_private_archived_project() {
    let clock = fake::Clock::new(At::new(5));
    let w = World::new()
        .named("fake/elsewhere")
        .on_clock(clock.clone())
        .private()
        .tracker_off()
        .archived()
        .default_branch("trunk")
        .ticket(1, |t| t.title("one"));
    assert_eq!(w.now(), At::new(5));
    clock.advance(10);
    assert_eq!(
        w.now(),
        At::new(15),
        "the world reads the clock it was given"
    );
    let site = FakeSite::new(&w);
    let facts = site.code_host(&full(), C).unwrap().facts().unwrap();
    assert_eq!(facts.name, ProjectName::new("fake/elsewhere"));
    assert!(!facts.public && !facts.tracker_on && facts.archived);
    assert_eq!(facts.default_branch, branch("trunk"));
    assert_eq!(w.head("trunk"), w.head("main"));
    let mut anon = site.code_host(&Actor::Anonymous, C).unwrap();
    assert_eq!(anon.facts().map(drop), Err(Outcome::NotFound));
    let mut t = site.ticket_writes(&full(), C).unwrap();
    assert_eq!(
        t.ticket(&w.key(1), Depth::All, None).map(drop),
        Err(Outcome::NotFound),
        "the tracker is off"
    );
    let mut c = site.code_host_writes(&full(), C).unwrap();
    let head = w.head("trunk").unwrap();
    assert_eq!(
        c.tag(&TagName::new("v1").unwrap(), &head),
        Err(Outcome::Refused(Refusal::ReadOnly))
    );
}

#[test]
fn a_law_breaks_where_its_rule_does() {
    // Without a watch nothing is held: the call goes out and answers.
    let wired = Wired::new(World::new().ticket(1, |t| t.title("a")));
    let bare = Faulty::new(FakeSite::new(&wired.world));
    let actor = Actor::Ours(Scope::Read);
    let mut h = bare.tickets(&actor, C).unwrap();
    let e = laws::health::held_calls_raise_no_recovered(
        h.as_mut(),
        &wired.world.key(1),
        wired.wiring(&|| bare.requests()),
        &wired.watch(None).holder(&actor),
        Pool::Calls,
        wired.world.now(),
    )
    .unwrap_err();
    assert_eq!(e.law, "health::held_calls_raise_no_recovered");
    // A list that says more where it holds no more.
    let wrong = |w: Window| -> Got<Took<u32>> {
        Ok(Took {
            items: vec![1, 2],
            more: matches!(w, Window::Last(_)),
        })
    };
    assert!(laws::lists::more_is_exact(wrong, 2).is_err());
}
