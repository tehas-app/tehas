//! The settle rules of a row left unknown (the ports spec, "How a row left
//! unknown is settled", Decided 28): whether a write the site may have taken
//! is there, from the read its kind names. Pure: the caller reads the row's
//! item and a rule decides, so the poll's settle and the cycle's give the
//! same answer. None: the rule cannot tell from this read, because the row is
//! of another kind or its body's sum cannot be had. A read that failed is
//! never given to a rule: the row stays unknown.
//!
//! The kinds a cycle's report sends are here. The poll's other kinds join
//! them at T4.7.

use super::{sha256, Found};
use tehas_ports::{AccountKind, At, Body, Comment, Write};

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
}
