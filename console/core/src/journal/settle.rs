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

/// A comment made this long before the row still counts, for the clocks of
/// the host and the site.
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

/// `TicketNote` and `CommentChange`: a comment of the factory's own account,
/// made at or after `at`, the row's time, whose body (without markers) has
/// the row's sum and whose markers are the row's. `comments` are those of the
/// row's ticket or change as read now.
pub fn by_comments(row: &Write, comments: &[Comment], at: At) -> Option<Found> {
    let (body, markers) = match row {
        Write::TicketNote { body, markers, .. } | Write::CommentChange { body, markers, .. } => {
            (body, markers)
        }
        _ => return None,
    };
    let (sum, len) = sum_of(body)?;
    let from = at.secs().saturating_sub(SKEW);
    let there = comments.iter().any(|c| {
        c.author
            .as_ref()
            .is_some_and(|a| a.kind == AccountKind::Ours)
            && c.created.secs() >= from
            && c.body.len() as u64 == len
            && sha256(&c.body) == sum
            && c.markers == *markers
    });
    Some(found(there))
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
    use tehas_ports::Marker;
    use tehas_ports::{
        Account, AccountKind, ChangeNo, CommentId, Login, Mark, Realm, Revision, Sha256, State,
        Sum, TicketKey, WebLink,
    };

    const AT: i64 = 2_000_000_000;

    fn ticket() -> TicketKey {
        TicketKey::Number(NonZeroU64::new(7).unwrap())
    }

    fn change() -> ChangeNo {
        ChangeNo::new(9).unwrap()
    }

    fn running() -> Mark {
        Mark::State(State::Running)
    }

    fn account(kind: AccountKind) -> Account {
        Account {
            realm: Realm::CodeHost,
            login: Login::new("tehas-bot"),
            id: None,
            kind,
        }
    }

    fn comment(body: &str, kind: AccountKind, at: i64) -> Comment {
        Comment {
            id: CommentId::new("c1"),
            author: Some(account(kind)),
            editor: None,
            created: At::new(at),
            edited: None,
            body: body.to_string(),
            markers: vec![],
            revision: Revision::new("r1"),
            link: WebLink::new("https://site.example/c1").unwrap(),
        }
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
        assert_eq!(
            by_marks(&on, std::slice::from_ref(&refreshing)),
            Some(Found::Landed)
        );
        assert_eq!(
            by_marks(&off, std::slice::from_ref(&refreshing)),
            Some(Found::NotSent)
        );
        assert_eq!(by_marks(&off, &[running()]), Some(Found::Landed));
    }

    #[test]
    fn a_row_of_another_kind_is_not_told_by_these_reads() {
        assert_eq!(by_marks(&note("x"), &[]), None);
        let mark = Write::Mark {
            ticket: ticket(),
            mark: running(),
        };
        assert_eq!(by_comments(&mark, &[], At::new(AT)), None);
    }

    #[test]
    fn a_note_of_ours_with_the_rows_text_since_its_time_is_there() {
        let read = [
            comment("Other.", AccountKind::Ours, AT + 5),
            comment("Hello.", AccountKind::Ours, AT + 5),
        ];
        assert_eq!(
            by_comments(&note("Hello."), &read, At::new(AT)),
            Some(Found::Landed)
        );
        assert_eq!(
            by_comments(&note("Bye."), &read, At::new(AT)),
            Some(Found::NotSent)
        );
    }

    #[test]
    fn a_note_with_the_rows_text_by_another_account_is_not_ours() {
        for kind in [AccountKind::Person, AccountKind::Bot, AccountKind::Unknown] {
            let read = [comment("Hello.", kind, AT + 5)];
            assert_eq!(
                by_comments(&note("Hello."), &read, At::new(AT)),
                Some(Found::NotSent),
                "{kind:?}"
            );
        }
        let mut nobody = comment("Hello.", AccountKind::Ours, AT + 5);
        nobody.author = None;
        assert_eq!(
            by_comments(&note("Hello."), &[nobody], At::new(AT)),
            Some(Found::NotSent)
        );
    }

    #[test]
    fn a_note_older_than_the_row_is_not_it_but_a_minute_of_clock_counts() {
        let old = [comment("Hello.", AccountKind::Ours, AT - 600)];
        assert_eq!(
            by_comments(&note("Hello."), &old, At::new(AT)),
            Some(Found::NotSent)
        );
        let skewed = [comment("Hello.", AccountKind::Ours, AT - 60)];
        assert_eq!(
            by_comments(&note("Hello."), &skewed, At::new(AT)),
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
        let read = [comment("Hello.", AccountKind::Ours, AT)];
        assert_eq!(
            by_comments(&marked, &read, At::new(AT)),
            Some(Found::NotSent)
        );
        let mut with = comment("Hello.", AccountKind::Ours, AT);
        with.markers = vec![Marker::HoldCleared];
        assert_eq!(
            by_comments(&marked, &[with], At::new(AT)),
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
        let read = [comment("Hello.", AccountKind::Ours, AT)];
        assert_eq!(
            by_comments(&summed, &read, At::new(AT)),
            Some(Found::Landed)
        );
        let short = [comment("Hello", AccountKind::Ours, AT)];
        assert_eq!(
            by_comments(&summed, &short, At::new(AT)),
            Some(Found::NotSent)
        );
    }
}
