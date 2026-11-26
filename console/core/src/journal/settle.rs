//! The settle rules of a row left unknown (the ports spec, "How a row left
//! unknown is settled", Decided 28): whether a write the site may have taken
//! is there, from the read its kind names. [`read`] makes that read through
//! the ports and hands it to the kind's rule; the rules themselves are pure,
//! so the poll's settle and the cycle's give the same answer. None: the rule
//! cannot tell from this read, because the row is of another kind or its
//! body's sum cannot be had. A read that failed is never given to a rule:
//! the row stays unknown.
//!
//! The kinds a cycle's report sends are here. The poll's other kinds join
//! them at T4.7.

use super::{sha256, Found, Handles};
use tehas_ports::{AccountKind, At, Body, Comment, Depth, TicketState, Write};
use tehas_ports::{Bound, Window};

/// A comment made this long before the row still counts: the host's clock
/// and the site's may be this far apart, and a write the site took is not
/// sent again for it.
const SKEW: i64 = 60;

fn found(there: bool) -> Found {
    if there {
        Found::Landed
    } else {
        Found::NotSent
    }
}

/// `Mark`, `Unmark`, `MarkChange` and `UnmarkChange`: the mark is on, or off,
/// among `marks`, the marks of the row's ticket or change as read now.
pub fn by_marks(row: &Write, marks: &[tehas_ports::Mark]) -> Option<Found> {
    let (mark, on) = match row {
        Write::Mark { mark, .. } | Write::MarkChange { mark, .. } => (mark, true),
        Write::Unmark { mark, .. } | Write::UnmarkChange { mark, .. } => (mark, false),
        _ => return None,
    };
    Some(found(marks.contains(mark) == on))
}

/// `TicketNote` and `CommentChange`: a comment made at or after `at`, the
/// row's time, whose body (without markers) has the row's sum and whose
/// markers are the row's. `comments` are those of the row's ticket or change
/// as read now. One such comment of the factory's own account (`Ours`) is
/// the row's: landed. One by an account that could be ours but is not
/// vouched for as ours (a bot, an account the site does not type, or none),
/// as a site answers while it is not told the factory's id, cannot tell:
/// None, so the write stays maybe and is never sent again. NotSent only when
/// no comment matches, or only a person's does: a person's copy is not our
/// post.
pub fn by_comments(row: &Write, comments: &[Comment], at: At) -> Option<Found> {
    let (body, markers) = match row {
        Write::TicketNote { body, markers, .. } | Write::CommentChange { body, markers, .. } => {
            (body, markers)
        }
        _ => return None,
    };
    let (sum, len) = sum_of(body)?;
    let from = at.secs().saturating_sub(SKEW);
    let matching: Vec<&Comment> = comments
        .iter()
        .filter(|c| {
            c.created.secs() >= from
                && c.body.len() as u64 == len
                && sha256(&c.body) == sum
                && c.markers == *markers
        })
        .collect();
    let kind = |c: &&Comment| c.author.as_ref().map(|a| a.kind);
    if matching.iter().any(|c| kind(c) == Some(AccountKind::Ours)) {
        return Some(Found::Landed);
    }
    if matching
        .iter()
        .any(|c| kind(c) != Some(AccountKind::Person))
    {
        return None;
    }
    Some(Found::NotSent)
}

/// Whether `write` is there, read through `h` by the rule of its kind,
/// counting what was made at or after `since` (less the rule's slack).
/// `bound`: how many comments a read of a blueprint or change takes.
pub fn read(write: &Write, h: &mut Handles, since: At, bound: Bound) -> Option<Found> {
    match write {
        Write::Mark { ticket, .. } | Write::Unmark { ticket, .. } => {
            let (_, digest) = h.tickets.several(std::slice::from_ref(ticket)).pop()?;
            by_marks(write, &digest.ok()?.marks)
        }
        Write::Close { ticket, .. } => {
            // Closed is the state this write asks for; whose hand moved it
            // is not asked, as the mark rules ask it neither. Open now: not
            // sent, or sent and opened again since, and the row is no more
            // ours to call landed than a mark put back by hand. One read
            // carries the state, the site's time of the close and its
            // reason: the raiser of `ticket.closed` keys on all three.
            let t = h
                .tickets
                .ticket(ticket, Depth::Cut { notes: 0 }, None)
                .ok()?;
            Some(match t.digest.state {
                TicketState::Closed(reason) => Found::Closed {
                    at: t.closed,
                    reason,
                },
                TicketState::Open => Found::NotSent,
            })
        }
        Write::TicketNote { ticket, .. } => {
            let notes = match ticket.same_item() {
                Some(no) => h.blueprints.comments(no, bound).ok()?.items().to_vec(),
                None => {
                    let last = std::num::NonZeroU32::new(100).expect("100 is no zero");
                    h.tickets.notes(ticket, Window::Last(last)).ok()?.items
                }
            };
            by_comments(write, &notes, since)
        }
        Write::MarkChange { no, .. } | Write::UnmarkChange { no, .. } => {
            // Its digest alone: the whole change answers Cut past its
            // bound of files or comments, and its marks would never
            // settle.
            let filter = tehas_ports::ChangeFilter {
                states: tehas_ports::ChangeStates::All,
                numbers: vec![*no],
                ..Default::default()
            };
            let changes = h.code_host.changes(&filter, bound).ok()?;
            let digest = changes.items().iter().find(|d| d.no == *no)?;
            by_marks(write, &digest.marks)
        }
        Write::CommentChange { no, .. } => {
            let change = h.code_host.change(*no, bound).ok()?;
            by_comments(write, &change.comments, since)
        }
        _ => None,
    }
}

/// The sum of a row's body, as sent or as the store keeps it.
fn sum_of(body: &Body) -> Option<(String, u64)> {
    match body {
        Body::Text(t) => Some((sha256(t), t.len() as u64)),
        Body::Summed(s) => Some((s.sha256.as_str().to_string(), s.len)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tehas_ports::fake::{FakeSite, World, FACTORY};
    use tehas_ports::{
        AccountKind, Actor, BlueprintNo, Bound, Caller, ChangeNo, Mark, Marker, Sha256, Site,
        Standing, State, Sum, TicketKey,
    };

    fn ticket() -> TicketKey {
        TicketKey::Number(NonZeroU64::new(7).unwrap())
    }

    fn change() -> ChangeNo {
        ChangeNo::new(9).unwrap()
    }

    fn running() -> Mark {
        Mark::State(State::Running)
    }

    /// The notes of ticket 7 as the fake site gives them: on a site where a
    /// ticket is its own blueprint, its blueprint's comments.
    fn notes(world: &World) -> Vec<Comment> {
        let site = FakeSite::new(world);
        let mut b = site
            .blueprints(&Actor::Operator, Caller::new("test"))
            .unwrap();
        b.comments(BlueprintNo::new(7).unwrap(), Bound::new(50).unwrap())
            .unwrap()
            .items()
            .to_vec()
    }

    /// Ticket 7 with a note `body` by `login`, the row sent `ago` seconds
    /// after it: the row's time.
    fn noted(login: &str, body: &str, ago: i64) -> (Vec<Comment>, At) {
        let world = World::new()
            .with_account("other-bot", AccountKind::Bot, Standing::Read)
            .ticket(7, |t| t.comment(login, body));
        world.advance(ago);
        (notes(&world), world.now())
    }

    fn note(body: &str) -> Write {
        Write::TicketNote {
            ticket: ticket(),
            body: Body::Text(body.to_string()),
            markers: vec![],
        }
    }

    #[test]
    fn a_mark_put_on_is_there_when_the_ticket_carries_it() {
        let on = Write::Mark {
            ticket: ticket(),
            mark: running(),
        };
        assert_eq!(by_marks(&on, &[running()]), Some(Found::Landed));
        assert_eq!(by_marks(&on, &[]), Some(Found::NotSent));
    }

    #[test]
    fn a_mark_taken_off_is_there_when_the_ticket_no_longer_carries_it() {
        let off = Write::Unmark {
            ticket: ticket(),
            mark: running(),
        };
        assert_eq!(by_marks(&off, &[]), Some(Found::Landed));
        assert_eq!(by_marks(&off, &[running()]), Some(Found::NotSent));
    }

    #[test]
    fn a_changes_mark_is_read_as_a_tickets_is() {
        let refreshing = Mark::State(State::Refreshing);
        let on = Write::MarkChange {
            no: change(),
            mark: refreshing.clone(),
        };
        let off = Write::UnmarkChange {
            no: change(),
            mark: refreshing.clone(),
        };
        let carried = std::slice::from_ref(&refreshing);
        assert_eq!(by_marks(&on, carried), Some(Found::Landed));
        assert_eq!(by_marks(&off, carried), Some(Found::NotSent));
        assert_eq!(by_marks(&off, &[running()]), Some(Found::Landed));
    }

    #[test]
    fn a_row_of_another_kind_is_not_told_by_these_reads() {
        assert_eq!(by_marks(&note("x"), &[]), None);
        let mark = Write::Mark {
            ticket: ticket(),
            mark: running(),
        };
        assert_eq!(by_comments(&mark, &[], At::new(0)), None);
    }

    #[test]
    fn a_note_of_ours_with_the_rows_text_since_its_time_is_there() {
        let (read, at) = noted(FACTORY, "Hello.", 0);
        assert_eq!(by_comments(&note("Hello."), &read, at), Some(Found::Landed));
        assert_eq!(by_comments(&note("Bye."), &read, at), Some(Found::NotSent));
    }

    #[test]
    fn a_persons_note_with_the_rows_text_is_not_ours() {
        let (read, at) = noted("alice", "Hello.", 0);
        assert_eq!(
            by_comments(&note("Hello."), &read, at),
            Some(Found::NotSent)
        );
    }

    /// While the site is not told the factory's id it answers our own
    /// comments as a bot's: a match it cannot vouch for leaves the row
    /// unknown, never sent again.
    #[test]
    fn a_note_with_the_rows_text_that_could_be_ours_cannot_tell() {
        let (read, at) = noted("other-bot", "Hello.", 0);
        assert_eq!(by_comments(&note("Hello."), &read, at), None);
        // No match at all is still not sent, whoever wrote other notes.
        assert_eq!(by_comments(&note("Bye."), &read, at), Some(Found::NotSent));
        // An account the site does not type, or none at all.
        let built = |author: Option<tehas_ports::Account>| {
            let mut c = Comment::new(
                tehas_ports::CommentId::new("1"),
                at,
                tehas_ports::Revision::new("r"),
                tehas_ports::WebLink::new("https://site.example/c/1").unwrap(),
            );
            c.author = author;
            c.body = "Hello.".to_string();
            c
        };
        let unknown = tehas_ports::Account::new(
            tehas_ports::Realm::CodeHost,
            tehas_ports::Login::new("someone"),
            AccountKind::Unknown,
        );
        for c in [built(Some(unknown)), built(None)] {
            assert_eq!(
                by_comments(&note("Hello."), std::slice::from_ref(&c), at),
                None,
                "{c:?}"
            );
        }
    }

    #[test]
    fn a_note_older_than_the_row_is_not_it_but_a_minute_of_clock_counts() {
        let (old, at) = noted(FACTORY, "Hello.", 600);
        assert_eq!(by_comments(&note("Hello."), &old, at), Some(Found::NotSent));
        let (skewed, at) = noted(FACTORY, "Hello.", SKEW);
        assert_eq!(
            by_comments(&note("Hello."), &skewed, at),
            Some(Found::Landed)
        );
    }

    #[test]
    fn a_note_whose_markers_differ_is_not_it() {
        let marked = Write::TicketNote {
            ticket: ticket(),
            body: Body::Text("Hello.".into()),
            markers: vec![Marker::HoldCleared],
        };
        let (plain, at) = noted(FACTORY, "Hello.", 0);
        assert_eq!(by_comments(&marked, &plain, at), Some(Found::NotSent));
        let world = World::new().ticket(7, |t| {
            t.comment_marked(FACTORY, "Hello.", vec![Marker::HoldCleared])
        });
        assert_eq!(
            by_comments(&marked, &notes(&world), world.now()),
            Some(Found::Landed)
        );
    }

    #[test]
    fn a_row_kept_by_its_sum_is_read_as_one_kept_with_its_text() {
        let summed = Write::CommentChange {
            no: change(),
            body: Body::Summed(Sum {
                sha256: Sha256::new(sha256("Hello.")).unwrap(),
                len: 6,
            }),
            markers: vec![],
        };
        let (read, at) = noted(FACTORY, "Hello.", 0);
        assert_eq!(by_comments(&summed, &read, at), Some(Found::Landed));
        let (short, at) = noted(FACTORY, "Hello", 0);
        assert_eq!(by_comments(&summed, &short, at), Some(Found::NotSent));
    }

    /// The factory's handles of a fake site holding `world`.
    fn handles(world: &World) -> Handles {
        use tehas_ports::{Scope, Writer};
        let site = FakeSite::new(world);
        let (by, c) = (Actor::Ours(Scope::Full), Caller::new("test"));
        Handles {
            writes: Box::new(Writer {
                tickets: site.ticket_writes(&by, c).unwrap(),
                blueprints: site.blueprint_writes(&by, c).unwrap(),
                code_host: site.code_host_writes(&by, c).unwrap(),
            }),
            tickets: site.tickets(&by, c).unwrap(),
            blueprints: site.blueprints(&by, c).unwrap(),
            code_host: site.code_host(&by, c).unwrap(),
        }
    }

    /// `read` asks the read a row's kind names and hands it to the kind's
    /// rule: a mark by the ticket's marks, a note by its notes, the row as
    /// the journal keeps it with its body summed.
    #[test]
    fn a_row_is_read_by_the_read_its_kind_names() {
        use tehas_ports::{Body, CloseReason, NoBlobs};
        let world = World::new().ticket(7, |t| t.mark(running()));
        let mut h = handles(&world);
        let since = world.now();
        let bound = Bound::new(50).unwrap();
        let mark = |m: Mark| Write::Mark {
            ticket: ticket(),
            mark: m,
        };
        let unmark = Write::Unmark {
            ticket: ticket(),
            mark: running(),
        };
        let attention = Mark::State(State::NeedsPerson);
        assert_eq!(
            read(&mark(running()), &mut h, since, bound),
            Some(Found::Landed)
        );
        assert_eq!(read(&unmark, &mut h, since, bound), Some(Found::NotSent));
        assert_eq!(
            read(&mark(attention), &mut h, since, bound),
            Some(Found::NotSent)
        );

        let note = |text: &str| Write::TicketNote {
            ticket: ticket(),
            body: Body::Text(text.into()),
            markers: Vec::new(),
        };
        let sent = note("Started.");
        assert_eq!(read(&sent, &mut h, since, bound), Some(Found::NotSent));
        h.writes.send(&sent, &NoBlobs).unwrap();
        let kept = sent.summed(|t| Sha256::new(sha256(t)).ok()).unwrap();
        assert_eq!(read(&kept, &mut h, since, bound), Some(Found::Landed));
        assert_eq!(
            read(&note("Stopped."), &mut h, since, bound),
            Some(Found::NotSent)
        );

        // A close is read by the ticket's state: closed is landed, whoever
        // closed it, with the site's time and reason of it in the answer,
        // and open is not sent.
        let close = Write::Close {
            ticket: ticket(),
            reason: CloseReason::Done,
        };
        assert_eq!(read(&close, &mut h, since, bound), Some(Found::NotSent));
        h.writes.send(&close, &NoBlobs).unwrap();
        assert!(
            matches!(
                read(&close, &mut h, since, bound),
                Some(Found::Closed {
                    reason: CloseReason::Done,
                    ..
                })
            ),
            "the close reads landed with the site's reason"
        );

        // A kind no rule reads yet stays unknown.
        let reopen = Write::Reopen { ticket: ticket() };
        assert_eq!(read(&reopen, &mut h, since, bound), None);
    }
}
