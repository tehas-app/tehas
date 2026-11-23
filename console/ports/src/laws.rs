//! The laws every adapter and fake passes, as functions over the traits.
//! Each takes the handles to test and the fixture it reads (items the
//! fixture project holds, a hook that does what no port does, such as a
//! push), and answers `Ok` or what it found broken. A law makes no effect
//! of its own and reads no clock: the time it is given is the caller's.
//!
//! A law that reads a marker's text form (a quoted marker, a bad field,
//! leading zeros, a bare marker) checks what the port answers for a
//! fixture item whose text the adapter's recorded sample holds; the fake
//! holds markers beside its text, so over the fake those laws check that
//! what was put in is read back.

use serde::{Deserialize, Serialize};

/// What a law answers: `Ok`, or what it found broken.
pub type Law = Result<(), Broken>;

/// A law found broken: which, and what it saw.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Broken {
    /// The law's name, its path under `laws`.
    pub law: String,
    /// What it saw, in our words.
    pub what: String,
}

fn broken(law: &str, what: impl Into<String>) -> Law {
    Err(Broken {
        law: law.to_string(),
        what: what.into(),
    })
}

fn hold(cond: bool, law: &str, what: impl FnOnce() -> String) -> Law {
    if cond {
        Ok(())
    } else {
        broken(law, what())
    }
}

/// The value of an answer, or the law broken by its outcome.
fn got<T>(law: &str, what: &str, answer: crate::Got<T>) -> Result<T, Broken> {
    answer.map_err(|o| Broken {
        law: law.to_string(),
        what: format!("{what}: {}", o.line()),
    })
}

/// Laws of the tickets port's writes.
pub mod tickets {
    use super::{got, hold, Law};
    use crate::{Depth, Mark, TicketKey, TicketWrites};

    /// Taking off a mark a ticket does not carry is done, and leaves it
    /// without the mark. The fixture's ticket must not carry `mark`.
    pub fn removing_an_absent_mark_is_done(
        h: &mut dyn TicketWrites,
        key: &TicketKey,
        mark: &Mark,
    ) -> Law {
        const LAW: &str = "tickets::removing_an_absent_mark_is_done";
        let before = got(
            LAW,
            "the ticket",
            h.ticket(key, Depth::Cut { notes: 0 }, None),
        )?;
        hold(!before.digest.marks.contains(mark), LAW, || {
            "the fixture's ticket carries the mark".into()
        })?;
        got(LAW, "unmark", h.unmark(key, mark))?;
        let after = got(
            LAW,
            "the ticket again",
            h.ticket(key, Depth::Cut { notes: 0 }, None),
        )?;
        hold(!after.digest.marks.contains(mark), LAW, || {
            "the mark is on after it was taken off".into()
        })
    }
}

/// Laws of the code host port.
pub mod code_host {
    use super::{broken, got, hold, Law};
    use crate::{
        Applied, Blobs, Branch, ChangeNo, ChangeState, CodeHostWrites, CommitId, Lost, MergeHow,
        NewChange, Outcome, Proposal, ProposalPart, Proposed, Refusal, RepoPath, Sum, Takeover,
        ALL_MAX,
    };

    fn state(h: &mut dyn CodeHostWrites, law: &str, no: ChangeNo) -> Result<ChangeState, Broken> {
        Ok(got(law, "the change", h.change(no, ALL_MAX))?.digest.state)
    }

    use super::Broken;

    fn proposed(
        law: &str,
        what: &str,
        answer: Result<Proposed, Applied<ProposalPart>>,
    ) -> Result<Proposed, Broken> {
        answer.map_err(|a| Broken {
            law: law.to_string(),
            what: match a.stopped {
                Some((part, o)) => format!("{what}: stopped at {part:?}: {}", o.line()),
                None => format!("{what}: stopped with nothing applied"),
            },
        })
    }

    /// A merge at a head the branch has moved from is
    /// `Conflict(HeadMoved)`, and leaves the change open. `push` moves the
    /// change's branch to a new commit, as git does.
    pub fn merge_at_a_moved_head_is_a_conflict(
        h: &mut dyn CodeHostWrites,
        no: ChangeNo,
        push: &mut dyn FnMut(&Branch),
    ) -> Law {
        const LAW: &str = "code_host::merge_at_a_moved_head_is_a_conflict";
        let before = got(LAW, "the change", h.change(no, ALL_MAX))?.digest;
        push(&before.head);
        let after = got(LAW, "the change again", h.change(no, ALL_MAX))?.digest;
        hold(after.head_commit != before.head_commit, LAW, || {
            "the push did not move the head".into()
        })?;
        let merged = h.merge(no, &before.head_commit, MergeHow::Merge);
        hold(
            merged == Err(Outcome::Conflict(Lost::HeadMoved)),
            LAW,
            || format!("a merge at the old head answered {merged:?}"),
        )?;
        let now = state(h, LAW, no)?;
        hold(now == ChangeState::Open, LAW, || {
            format!("the change is {now:?} after the refused merge")
        })
    }

    /// A merge at `Proposed.head` succeeds while nothing was pushed, and is
    /// `Conflict(HeadMoved)` after a push. `first` and `second` are two
    /// proposals of fresh branches; `push` moves a branch, as git does.
    pub fn merge_at_the_proposed_head(
        h: &mut dyn CodeHostWrites,
        first: &Proposal,
        second: &Proposal,
        files: &dyn Blobs,
        push: &mut dyn FnMut(&Branch),
    ) -> Law {
        const LAW: &str = "code_host::merge_at_the_proposed_head";
        let one = proposed(LAW, "the first proposal", h.propose(first, files))?;
        got(
            LAW,
            "a merge at the proposed head",
            h.merge(one.change.id, &one.head, MergeHow::Squash),
        )?;
        let two = proposed(LAW, "the second proposal", h.propose(second, files))?;
        push(&second.branch);
        let merged = h.merge(two.change.id, &two.head, MergeHow::Squash);
        hold(
            merged == Err(Outcome::Conflict(Lost::HeadMoved)),
            LAW,
            || format!("a merge at the proposed head after a push answered {merged:?}"),
        )
    }

    /// The same inputs answer the same change while it is open or merged;
    /// after a close unmerged they open a new change. `p` proposes a fresh
    /// branch that can merge.
    pub fn a_closed_proposal_opens_anew(
        h: &mut dyn CodeHostWrites,
        p: &Proposal,
        files: &dyn Blobs,
    ) -> Law {
        const LAW: &str = "code_host::a_closed_proposal_opens_anew";
        let first = proposed(LAW, "the proposal", h.propose(p, files))?;
        let again = proposed(LAW, "the proposal again", h.propose(p, files))?;
        hold(again == first, LAW, || {
            format!("asked again while open it answered {again:?}, not {first:?}")
        })?;
        got(LAW, "the close", h.close_change(first.change.id))?;
        let anew = proposed(LAW, "the proposal after the close", h.propose(p, files))?;
        hold(anew.change.id != first.change.id, LAW, || {
            "after a close unmerged it answered the closed change".into()
        })?;
        let open = state(h, LAW, anew.change.id)?;
        hold(open == ChangeState::Open, LAW, || {
            format!("the new change is {open:?}")
        })?;
        got(
            LAW,
            "a merge of the new change",
            h.merge(anew.change.id, &anew.head, MergeHow::Squash),
        )?;
        let merged = proposed(LAW, "the proposal after the merge", h.propose(p, files))?;
        hold(merged.change.id == anew.change.id, LAW, || {
            "after a merge it did not answer the merged change".into()
        })
    }

    /// What a test leaves on a branch for `takeover_by_shape`, as another
    /// account, before the law proposes on it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Left {
        /// One commit with this message over the base.
        OneCommit {
            /// Its message.
            message: String,
        },
        /// One commit with this message over an older commit of the base,
        /// the base having moved on since.
        OneCommitBaseMoved {
            /// Its message.
            message: String,
        },
        /// Two commits past the base's history, the newest with this
        /// message.
        TwoCommits {
            /// The newest one's message.
            message: String,
        },
        /// One commit of two parents with this message.
        TwoParents {
            /// Its message.
            message: String,
        },
        /// One commit with this message, and an open change on the branch.
        OpenChange {
            /// Its message.
            message: String,
        },
    }

    /// Where a test left its branch: the branch, the base commit to
    /// propose from, and the branch the change goes into.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Leftover {
        /// The branch left.
        pub branch: Branch,
        /// The commit to propose from: the base's head now.
        pub base: CommitId,
        /// The branch the change goes into.
        pub into: Branch,
    }

    /// The message `takeover_by_shape` proposes with.
    pub const MESSAGE: &str = "Bump the version";

    fn proposal(left: &Leftover, files: &[(RepoPath, Sum)], takeover: Takeover) -> Proposal {
        Proposal {
            base: left.base.clone(),
            branch: left.branch.clone(),
            files: files.to_vec(),
            message: MESSAGE.to_string(),
            change: NewChange {
                head: left.branch.clone(),
                base: left.into.clone(),
                title: MESSAGE.to_string(),
                body: crate::Body::Text("A bump.".into()),
                draft: false,
                closes: Vec::new(),
            },
            takeover,
        }
    }

    /// `Takeover::SameShape` moves a branch of one commit with the message,
    /// whoever wrote it, and one whose base has moved on since; the message
    /// with a line end or spaces after it moves too, and `propose`'s own
    /// left commit moves when its `Proposal.message` is given. Two commits
    /// past the base's history, a commit of two parents, a message that
    /// differs in any other byte, or an open change on the branch is
    /// `Refused(Exists)`, and the branch stays. `leave` makes a fresh
    /// branch as each case says.
    pub fn takeover_by_shape(
        h: &mut dyn CodeHostWrites,
        files: &[(RepoPath, Sum)],
        blobs: &dyn Blobs,
        leave: &mut dyn FnMut(&Left) -> Leftover,
    ) -> Law {
        const LAW: &str = "code_host::takeover_by_shape";
        let same = |m: &str| Left::OneCommit {
            message: m.to_string(),
        };
        let cases: Vec<(Left, bool)> = vec![
            (same(MESSAGE), true),
            (same(&format!("{MESSAGE}\n")), true),
            (same(&format!("{MESSAGE}  \t\r\n")), true),
            (
                Left::OneCommitBaseMoved {
                    message: MESSAGE.into(),
                },
                true,
            ),
            (
                Left::TwoCommits {
                    message: MESSAGE.into(),
                },
                false,
            ),
            (
                Left::TwoParents {
                    message: MESSAGE.into(),
                },
                false,
            ),
            (same(&format!("{MESSAGE}.")), false),
            (same(&MESSAGE.to_ascii_lowercase()), false),
            (same("Bump  the version"), false),
            (same(&format!("{MESSAGE}\n\nA trailer: x")), false),
            (
                Left::OpenChange {
                    message: MESSAGE.into(),
                },
                false,
            ),
        ];
        let shape = Takeover::SameShape {
            message: MESSAGE.to_string(),
        };
        for (left, moves) in cases {
            let at = leave(&left);
            let before = got(LAW, "the branch", h.branch(&at.branch))?;
            let answer = h.propose(&proposal(&at, files, shape.clone()), blobs);
            let after = got(LAW, "the branch again", h.branch(&at.branch))?;
            match (answer, moves) {
                (Ok(p), true) => hold(after.as_ref() == Some(&p.head), LAW, || {
                    format!("{left:?}: the branch is not at the proposed head")
                })?,
                (
                    Err(Applied {
                        stopped: Some((_, Outcome::Refused(Refusal::Exists))),
                        ..
                    }),
                    false,
                ) => hold(after == before, LAW, || {
                    format!("{left:?}: refused, yet the branch moved")
                })?,
                (answer, _) => {
                    return broken(LAW, format!("{left:?}: answered {answer:?}"));
                }
            }
        }
        // The commit `propose` left, its change closed, moves for a
        // proposal of other inputs that gives its message.
        let at = leave(&same(MESSAGE));
        let mine = proposal(&at, files, shape);
        let first = proposed(LAW, "a proposal of its own", h.propose(&mine, blobs))?;
        got(LAW, "the close", h.close_change(first.change.id))?;
        let other = Proposal {
            change: NewChange {
                title: format!("{MESSAGE}, again"),
                ..mine.change.clone()
            },
            ..mine
        };
        let again = proposed(
            LAW,
            "a proposal over its own left commit",
            h.propose(&other, blobs),
        )?;
        hold(again.change.id != first.change.id, LAW, || {
            "its own left commit did not move".into()
        })
    }
}

/// Laws of every list read: a whole list or the newest n.
pub mod lists {
    use std::fmt::Debug;
    use std::num::NonZeroU32;

    use super::{got, hold, Law};
    use crate::{Bound, Got, Outcome, Took, Unread, Whole, Window};

    /// A list read past its bound is `Unread(Cut)`, and read at its bound
    /// is whole. For lists, not searches. `read` reads the fixture's list
    /// of `held` items, `held` at least 2.
    pub fn past_the_bound_is_unread<T>(
        mut read: impl FnMut(Bound) -> Got<Whole<T>>,
        held: u32,
    ) -> Law {
        const LAW: &str = "lists::past_the_bound_is_unread";
        let (Some(at), Some(under)) = (Bound::new(held), Bound::new(held.saturating_sub(1))) else {
            return super::broken(LAW, "the fixture holds fewer than 2 items");
        };
        let whole = got(LAW, "the list at its bound", read(at))?;
        hold(whole.items().len() == held as usize, LAW, || {
            format!("at its bound it read {} of {held}", whole.items().len())
        })?;
        let cut = read(under).map(|w| w.items().len());
        hold(cut == Err(Outcome::Unread(Unread::Cut)), LAW, || {
            format!("one under its bound it answered {cut:?}")
        })
    }

    fn last<T>(
        read: &mut impl FnMut(Window) -> Got<Took<T>>,
        n: u32,
        law: &str,
    ) -> Result<Took<T>, super::Broken> {
        let n = NonZeroU32::new(n).unwrap_or(NonZeroU32::MIN);
        got(law, "the newest n", read(Window::Last(n)))
    }

    /// The newest n of a list are its last n, each whole, never
    /// `Unread(Cut)`, for every n. `read` reads the fixture's list of
    /// `held` items.
    pub fn last_n_is_never_cut<T: PartialEq + Debug>(
        mut read: impl FnMut(Window) -> Got<Took<T>>,
        held: u32,
    ) -> Law {
        const LAW: &str = "lists::last_n_is_never_cut";
        let bound = Bound::new(held.max(1)).unwrap_or(crate::ALL_MAX);
        let whole = got(LAW, "the whole list", read(Window::Whole(bound)))?;
        for n in 1..=held + 1 {
            let took = last(&mut read, n, LAW)?;
            let skip = whole.items.len().saturating_sub(n as usize);
            hold(took.items[..] == whole.items[skip..], LAW, || {
                format!("the newest {n} are not the list's last {n}")
            })?;
        }
        Ok(())
    }

    /// `more` is true exactly when the site holds more than n, for every
    /// n, and false for a whole read. `read` reads the fixture's list of
    /// `held` items.
    pub fn more_is_exact<T>(mut read: impl FnMut(Window) -> Got<Took<T>>, held: u32) -> Law {
        const LAW: &str = "lists::more_is_exact";
        for n in 1..=held + 1 {
            let took = last(&mut read, n, LAW)?;
            hold(took.more == (held > n), LAW, || {
                format!("the newest {n} of {held} said more is {}", took.more)
            })?;
        }
        let bound = Bound::new(held.max(1)).unwrap_or(crate::ALL_MAX);
        let whole = got(LAW, "the whole list", read(Window::Whole(bound)))?;
        hold(!whole.more, LAW, || "a whole read said more".into())
    }
}

/// Laws of the joined reads.
pub mod board {
    use super::{got, hold, Law};
    use crate::{Board, BoardAsk, Bound, Outcome, TicketKey};

    /// One ticket past a bound is `Unread` for that item, and the rest are
    /// answered. The fixture's open tickets include `big`, past the ask's
    /// bound, and at least one other within it.
    pub fn one_item_fails_alone(
        b: &mut dyn Board,
        ask: &BoardAsk,
        bound: Bound,
        big: &TicketKey,
    ) -> Law {
        const LAW: &str = "board::one_item_fails_alone";
        let work = got(LAW, "the open work", b.open_work(ask, bound))?;
        let items = work.items.items();
        let mine = items.iter().find(|(k, _)| k == big);
        hold(
            matches!(mine, Some((_, Err(Outcome::Unread(_))))),
            LAW,
            || {
                format!(
                    "the item past its bound answered {:?}",
                    mine.map(|(_, a)| a)
                )
            },
        )?;
        let rest: Vec<_> = items.iter().filter(|(k, _)| k != big).collect();
        hold(!rest.is_empty(), LAW, || {
            "the fixture has no other item".into()
        })?;
        hold(rest.iter().all(|(_, a)| a.is_ok()), LAW, || {
            "another item failed with it".into()
        })
    }
}

/// Laws of a ticket's history.
pub mod history {
    use super::{got, hold, Law};
    use crate::{Depth, TicketKey, Tickets, Window, ALL_MAX};

    /// A ticket's history is oldest first, as the history read and the
    /// ticket's own read give it alike, and its newest n are its last n.
    /// The fixture's ticket has a history of events with times.
    pub fn oldest_first(h: &mut dyn Tickets, key: &TicketKey) -> Law {
        const LAW: &str = "history::oldest_first";
        let whole = got(
            LAW,
            "the history",
            h.history(key, Window::Whole(ALL_MAX), None),
        )?;
        let times: Vec<_> = whole.items.iter().filter_map(|e| e.at).collect();
        hold(times.windows(2).all(|w| w[0] <= w[1]), LAW, || {
            "the history is not oldest first".into()
        })?;
        let ticket = got(LAW, "the ticket", h.ticket(key, Depth::All, None))?;
        hold(ticket.history.items == whole.items, LAW, || {
            "the ticket's history differs from the history read".into()
        })?;
        for n in 1..=whole.items.len() {
            let last = got(
                LAW,
                "the newest n",
                h.history(
                    key,
                    Window::Last(
                        std::num::NonZeroU32::new(n as u32).unwrap_or(std::num::NonZeroU32::MIN),
                    ),
                    None,
                ),
            )?;
            hold(
                last.items[..] == whole.items[whole.items.len() - n..],
                LAW,
                || format!("the newest {n} are not the last {n}"),
            )?;
        }
        Ok(())
    }
}

/// Laws of the blueprints port.
pub mod blueprints {
    use super::{got, hold, Law};
    use crate::{BlueprintNo, BlueprintWrites, Depth, Lost, MarkerEdit, Outcome};

    /// An edit moves the revision, and an edit at the old revision is then
    /// `Conflict(Changed)`.
    pub fn an_edit_moves_the_revision(h: &mut dyn BlueprintWrites, no: BlueprintNo) -> Law {
        const LAW: &str = "blueprints::an_edit_moves_the_revision";
        let depth = Depth::Cut { notes: 0 };
        let before = got(LAW, "the blueprint", h.blueprint(no, depth))?;
        let body = format!("{}\n\nEdited.", before.body);
        got(
            LAW,
            "the edit",
            h.edit(
                no,
                None,
                Some(&body),
                &MarkerEdit::default(),
                &before.revision,
            ),
        )?;
        let after = got(LAW, "the blueprint again", h.blueprint(no, depth))?;
        hold(after.revision != before.revision, LAW, || {
            "the edit did not move the revision".into()
        })?;
        let again = h.edit(
            no,
            None,
            Some("Stale."),
            &MarkerEdit::default(),
            &before.revision,
        );
        hold(again == Err(Outcome::Conflict(Lost::Changed)), LAW, || {
            format!("an edit at the old revision answered {again:?}")
        })
    }
}

/// Laws of the searches.
pub mod search {
    use std::fmt::Debug;
    use std::num::NonZeroU32;

    use super::{got, hold, Law};
    use crate::{Got, Took, Within};

    fn n(k: u32) -> NonZeroU32 {
        NonZeroU32::new(k).unwrap_or(NonZeroU32::MIN)
    }

    /// The first n hits in the site's order, with `more` exact: each first
    /// n is the start of a longer ask. `search` asks the fixture's words,
    /// which `held` items match.
    pub fn first_n_by_relevance<T: PartialEq + Debug>(
        mut search: impl FnMut(NonZeroU32) -> Got<Took<T>>,
        held: u32,
    ) -> Law {
        const LAW: &str = "search::first_n_by_relevance";
        let all = got(LAW, "every hit", search(n(held + 1)))?;
        hold(all.items.len() == held as usize && !all.more, LAW, || {
            format!(
                "asked {} it found {}, more {}",
                held + 1,
                all.items.len(),
                all.more
            )
        })?;
        for k in 1..=held {
            let first = got(LAW, "the first n", search(n(k)))?;
            hold(first.items[..] == all.items[..k as usize], LAW, || {
                format!("the first {k} are not the start of every hit")
            })?;
            hold(first.more == (held > k), LAW, || {
                format!("the first {k} of {held} said more is {}", first.more)
            })?;
        }
        Ok(())
    }

    /// A search of titles finds only items whose title holds every word,
    /// ASCII case folded, and finds fewer than a search of everything. The
    /// fixture holds an item with the words in its title and one with them
    /// in its text alone.
    pub fn titles_only_matches_titles<T: PartialEq + Debug>(
        mut search: impl FnMut(Within) -> Got<Took<T>>,
        title: impl Fn(&T) -> &str,
        words: &str,
    ) -> Law {
        const LAW: &str = "search::titles_only_matches_titles";
        let titles = got(LAW, "a search of titles", search(Within::Titles))?;
        let all = got(LAW, "a search of everything", search(Within::Everything))?;
        hold(!titles.items.is_empty(), LAW, || "no title matched".into())?;
        for hit in &titles.items {
            let t = title(hit).to_ascii_lowercase();
            hold(
                words
                    .split_whitespace()
                    .all(|w| t.contains(&w.to_ascii_lowercase())),
                LAW,
                || format!("{hit:?} matched by its text"),
            )?;
            hold(all.items.contains(hit), LAW, || {
                format!("{hit:?} is a title hit and no hit of everything")
            })?;
        }
        hold(all.items.len() > titles.items.len(), LAW, || {
            "a search of everything found no more than one of titles".into()
        })
    }

    /// Words that spell a site's qualifiers (another project, another kind
    /// of item, a mark) are words: every hit's title holds them as text.
    /// `search` searches titles for the words it is given.
    pub fn words_never_widen<T: Debug>(
        mut search: impl FnMut(&str) -> Got<Took<T>>,
        title: impl Fn(&T) -> &str,
        words: &[&str],
    ) -> Law {
        const LAW: &str = "search::words_never_widen";
        for w in words {
            let hits = got(LAW, "a search", search(w))?;
            for hit in &hits.items {
                hold(
                    title(hit)
                        .to_ascii_lowercase()
                        .contains(&w.to_ascii_lowercase()),
                    LAW,
                    || format!("{w:?} found {hit:?}, whose title does not hold it"),
                )?;
            }
        }
        Ok(())
    }
}

/// Laws of the markers, read from a blueprint's body.
pub mod markers {
    use super::{got, hold, Law};
    use crate::{BlueprintNo, BlueprintWrites, Blueprints, Depth, Marker, MarkerEdit, MarkerKind};

    fn read(
        h: &mut dyn Blueprints,
        law: &str,
        no: BlueprintNo,
    ) -> Result<Vec<Marker>, super::Broken> {
        Ok(got(
            law,
            "the blueprint",
            h.blueprint(no, Depth::Cut { notes: 0 }),
        )?
        .markers)
    }

    fn readable_of(markers: &[Marker], kind: &MarkerKind) -> bool {
        markers.iter().any(|m| {
            m.kind() == *kind && !matches!(m, Marker::Unreadable { .. } | Marker::Bare { .. })
        })
    }

    fn unreadable(h: &mut dyn Blueprints, law: &str, no: BlueprintNo, kind: &MarkerKind) -> Law {
        let markers = read(h, law, no)?;
        hold(
            markers.contains(&Marker::Unreadable { kind: kind.clone() }),
            law,
            || format!("no unreadable {kind:?} in {markers:?}"),
        )?;
        hold(!readable_of(&markers, kind), law, || {
            format!("a {kind:?} read in {markers:?}")
        })
    }

    /// A marker in a quote or in code is no marker. The fixture's body
    /// quotes one, and holds no other.
    pub fn a_quoted_marker_is_no_marker(h: &mut dyn Blueprints, no: BlueprintNo) -> Law {
        const LAW: &str = "markers::a_quoted_marker_is_no_marker";
        let markers = read(h, LAW, no)?;
        hold(markers.is_empty(), LAW, || format!("read {markers:?}"))
    }

    /// A wrapped marker with a field that does not parse is unreadable of
    /// its kind.
    pub fn a_bad_field_is_unreadable(
        h: &mut dyn Blueprints,
        no: BlueprintNo,
        kind: &MarkerKind,
    ) -> Law {
        unreadable(h, "markers::a_bad_field_is_unreadable", no, kind)
    }

    /// A number with leading zeros makes its marker unreadable.
    pub fn leading_zeros_are_unreadable(
        h: &mut dyn Blueprints,
        no: BlueprintNo,
        kind: &MarkerKind,
    ) -> Law {
        unreadable(h, "markers::leading_zeros_are_unreadable", no, kind)
    }

    /// A blocked list over its limit is unreadable.
    pub fn a_list_over_its_limit_is_unreadable(h: &mut dyn Blueprints, no: BlueprintNo) -> Law {
        unreadable(
            h,
            "markers::a_list_over_its_limit_is_unreadable",
            no,
            &MarkerKind::Blocked,
        )
    }

    /// A bare marker is `Bare` of its kind, with its payload when that
    /// reads (`read`), and is no marker that reads.
    pub fn a_bare_marker_is_bare(
        h: &mut dyn Blueprints,
        no: BlueprintNo,
        kind: &MarkerKind,
        payload: Option<&Marker>,
    ) -> Law {
        const LAW: &str = "markers::a_bare_marker_is_bare";
        let markers = read(h, LAW, no)?;
        let want = Marker::Bare {
            kind: kind.clone(),
            read: payload.map(|m| Box::new(m.clone())),
        };
        hold(markers.contains(&want), LAW, || {
            format!("no {want:?} in {markers:?}")
        })?;
        hold(!readable_of(&markers, kind), LAW, || {
            format!("a {kind:?} read in {markers:?}")
        })
    }

    /// A kind's words with no field after them are prose. The fixture's
    /// body holds such words, and no marker.
    pub fn words_without_a_field_are_prose(h: &mut dyn Blueprints, no: BlueprintNo) -> Law {
        const LAW: &str = "markers::words_without_a_field_are_prose";
        let markers = read(h, LAW, no)?;
        hold(markers.is_empty(), LAW, || format!("read {markers:?}"))
    }

    /// The markers of a text keep the order they stand in, unreadable ones
    /// included, so an unreadable marker before or after a good one of its
    /// kind is there to hold the text. `order` is the fixture's.
    pub fn markers_keep_their_order(
        h: &mut dyn Blueprints,
        no: BlueprintNo,
        order: &[Marker],
    ) -> Law {
        const LAW: &str = "markers::markers_keep_their_order";
        let markers = read(h, LAW, no)?;
        hold(markers == order, LAW, || {
            format!("read {markers:?}, not {order:?}")
        })
    }

    /// A marker of another blueprint than the item it is read from does
    /// not read.
    pub fn another_items_marker_does_not_read(
        h: &mut dyn Blueprints,
        no: BlueprintNo,
        kind: &MarkerKind,
    ) -> Law {
        const LAW: &str = "markers::another_items_marker_does_not_read";
        let markers = read(h, LAW, no)?;
        hold(!readable_of(&markers, kind), LAW, || {
            format!("a {kind:?} read in {markers:?}")
        })
    }

    /// A block's open line with no close line is unreadable of its kind.
    pub fn a_block_without_its_close_is_unreadable(
        h: &mut dyn Blueprints,
        no: BlueprintNo,
        kind: &MarkerKind,
    ) -> Law {
        unreadable(
            h,
            "markers::a_block_without_its_close_is_unreadable",
            no,
            kind,
        )
    }

    /// An unreadable marker is kept, where it stands, across an edit
    /// through `MarkerEdit::default()`. The fixture's body holds one.
    pub fn an_unreadable_marker_is_kept(h: &mut dyn BlueprintWrites, no: BlueprintNo) -> Law {
        const LAW: &str = "markers::an_unreadable_marker_is_kept";
        let before = got(
            LAW,
            "the blueprint",
            h.blueprint(no, Depth::Cut { notes: 0 }),
        )?;
        hold(
            before
                .markers
                .iter()
                .any(|m| matches!(m, Marker::Unreadable { .. })),
            LAW,
            || "the fixture's body holds no unreadable marker".into(),
        )?;
        let body = format!("{}\n\nEdited.", before.body);
        got(
            LAW,
            "the edit",
            h.edit(
                no,
                None,
                Some(&body),
                &MarkerEdit::default(),
                &before.revision,
            ),
        )?;
        let after = read(h, LAW, no)?;
        hold(after == before.markers, LAW, || {
            format!("read {after:?} after the edit, not {:?}", before.markers)
        })
    }
}

/// Laws of identity and its tokens.
pub mod identity {
    use super::{got, hold, Law};
    use crate::fake::{Asked, MemoryTokens};
    use crate::{Covers, Got, Identity, Outcome, ProjectName, Scope, Token};

    fn kept_since(tokens: &MemoryTokens, from: usize) -> Vec<Asked> {
        tokens
            .asked()
            .into_iter()
            .skip(from)
            .filter(|a| matches!(a, Asked::Keep(..)))
            .collect()
    }

    /// One token over a set of projects covers that set alone: asked with a
    /// project the grant does not cover, it is `Forbidden` and nothing is
    /// kept; asked with the set, it is kept under that set and no other.
    /// `set` is the fixture's grant's, the handle's project among them.
    pub fn a_token_covers_its_set_alone(
        h: &mut dyn Identity,
        tokens: &MemoryTokens,
        scope: Scope,
        set: &[ProjectName],
        outside: &ProjectName,
    ) -> Law {
        const LAW: &str = "identity::a_token_covers_its_set_alone";
        let from = tokens.asked().len();
        let mut wider = set.to_vec();
        wider.push(outside.clone());
        let refused = h.token_over(scope, &wider).map(|_| ());
        hold(refused == Err(Outcome::Forbidden), LAW, || {
            format!("a set with a project outside the grant answered {refused:?}")
        })?;
        hold(kept_since(tokens, from).is_empty(), LAW, || {
            "a refused set was kept".into()
        })?;
        got(LAW, "a token over the set", h.token_over(scope, set))?;
        let mut sorted = set.to_vec();
        sorted.sort();
        sorted.dedup();
        for asked in kept_since(tokens, from) {
            let ok = matches!(
                &asked,
                Asked::Keep(Covers::Projects { projects, .. }, s)
                    if projects.as_slice() == sorted.as_slice() && *s == scope
            );
            hold(ok, LAW, || format!("kept as {asked:?}"))?;
        }
        Ok(())
    }

    /// A given token is answered as it was handed in, never minted, renewed
    /// or kept. Past its time, its handle is `SignedOut`, made or asked.
    /// `h` acts as `Given` with `given`; `expired` is the making of a
    /// handle whose given token is past its time.
    pub fn a_given_token_is_never_minted(
        h: &mut dyn Identity,
        tokens: &MemoryTokens,
        scope: Scope,
        given: &Token,
        expired: Got<Box<dyn Identity + Send>>,
    ) -> Law {
        const LAW: &str = "identity::a_given_token_is_never_minted";
        let from = tokens.asked().len();
        let t = got(LAW, "the token", h.token(scope))?;
        hold(
            t.secret.expose() == given.secret.expose() && t.until == given.until,
            LAW,
            || "the token is not the one handed in".into(),
        )?;
        hold(kept_since(tokens, from).is_empty(), LAW, || {
            "a given token was kept".into()
        })?;
        let late = match expired {
            Err(o) => Err(o),
            Ok(mut h) => h.token(scope).map(|_| ()),
        };
        hold(late == Err(Outcome::SignedOut), LAW, || {
            format!("a given token past its time answered {late:?}")
        })
    }

    /// A site built without the factory's key answers its tokens from the
    /// store alone: none kept is `SignedOut`, and nothing is minted or
    /// kept. The store holds no token of `scope` for the handle's project.
    pub fn a_keyless_site_never_mints(
        h: &mut dyn Identity,
        tokens: &MemoryTokens,
        scope: Scope,
    ) -> Law {
        const LAW: &str = "identity::a_keyless_site_never_mints";
        let from = tokens.asked().len();
        let t = h.token(scope).map(|_| ());
        hold(t == Err(Outcome::SignedOut), LAW, || {
            format!("with none kept it answered {t:?}")
        })?;
        hold(kept_since(tokens, from).is_empty(), LAW, || {
            "a keyless site kept a token".into()
        })
    }
}

/// Laws of health: `health::next` takes every attempt the adapter made to
/// reach the site, one that could not connect included, and never an answer
/// it made without trying, so such an answer raises nothing.
pub mod health {
    use super::{hold, Law};
    use crate::fake::Notices;
    use crate::{
        At, Depth, Health, HealthState, Holder, HostId, Ledger, Outcome, Passing, Pool, Sent,
        TicketKey, Tickets, Whose,
    };

    /// What the handle's site was wired with: the ledger, the sink and the
    /// host its notices name.
    #[derive(Clone, Copy)]
    pub struct Wiring<'a> {
        /// The ledger.
        pub ledger: &'a dyn Ledger,
        /// The sink.
        pub sink: &'a Notices,
        /// The host.
        pub host: &'a HostId,
    }

    /// During an outage, a call of a holder whose pool is held spent
    /// answers `Spent` with no request, and raises no `Recovered`: the host
    /// stays failing. The law puts the outage and the spent pool in the
    /// ledger the handle's site was wired with; `pool` is the one a ticket
    /// read draws on, `holder` the handle's.
    pub fn held_calls_raise_no_recovered(
        h: &mut dyn Tickets,
        key: &TicketKey,
        wired: Wiring<'_>,
        holder: &Holder,
        pool: Pool,
        now: At,
    ) -> Law {
        const LAW: &str = "health::held_calls_raise_no_recovered";
        let Wiring { ledger, sink, host } = wired;
        let failing = HealthState::Failing {
            since: now,
            why: Passing::Timeout,
        };
        let until = At::new(now.secs() + 3600);
        let spent = HealthState::Spent { pool, until };
        let whose = Whose::Pool(holder.clone(), pool);
        let set = |w: &Whose, s: &HealthState| -> Law {
            let was = ledger.health(host, w).map_err(|_| super::Broken {
                law: LAW.into(),
                what: "the ledger is down".into(),
            })?;
            let won = ledger.swap_health(host, w, &was, s);
            hold(won == Ok(true), LAW, || format!("the ledger kept no {s:?}"))
        };
        set(&Whose::Host, &failing)?;
        set(&whose, &spent)?;
        let told = sink.all().len();
        let answer = h.ticket(key, Depth::Cut { notes: 0 }, None).map(|_| ());
        hold(answer == Err(Outcome::Spent { pool, until }), LAW, || {
            format!("a held call answered {answer:?}")
        })?;
        let new: Vec<_> = sink.all().into_iter().skip(told).collect();
        hold(
            !new.iter().any(|n| n.what == Health::Recovered),
            LAW,
            || format!("a held call raised {new:?}"),
        )?;
        let host_now = ledger.health(host, &Whose::Host);
        hold(host_now == Ok(failing), LAW, || {
            format!("the host is {host_now:?} after a held call")
        })
    }

    /// A call past its pass's deadline answers `Unavailable { why: Timeout,
    /// sent: No }` with no request, and raises no `Unavailable`: the host
    /// stays healthy. The handle was made with its deadline passed; the
    /// host is healthy in the ledger.
    pub fn past_the_deadline_raises_no_unavailable(
        h: &mut dyn Tickets,
        key: &TicketKey,
        wired: Wiring<'_>,
    ) -> Law {
        const LAW: &str = "health::past_the_deadline_raises_no_unavailable";
        let Wiring { ledger, sink, host } = wired;
        let before = ledger.health(host, &Whose::Host);
        hold(before == Ok(HealthState::Healthy), LAW, || {
            format!("the fixture's host is {before:?}")
        })?;
        let told = sink.all().len();
        let answer = h.ticket(key, Depth::Cut { notes: 0 }, None).map(|_| ());
        let timeout = Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No,
        };
        hold(answer == Err(timeout), LAW, || {
            format!("a call past the deadline answered {answer:?}")
        })?;
        let new: Vec<_> = sink.all().into_iter().skip(told).collect();
        hold(
            !new.iter().any(|n| matches!(n.what, Health::Unavailable(_))),
            LAW,
            || format!("a call past the deadline raised {new:?}"),
        )?;
        let after = ledger.health(host, &Whose::Host);
        hold(after == Ok(HealthState::Healthy), LAW, || {
            format!("the host is {after:?} after a call past the deadline")
        })
    }
}
