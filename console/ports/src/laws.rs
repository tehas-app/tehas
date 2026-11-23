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
//!
//! An adapter runs the laws over recorded replays of the fixture project,
//! so every answer is fixed. Run live, a hook that changes the site (a
//! push, a write elsewhere) waits until the site shows the change before
//! it returns: a search and a change's head lag behind a push.

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

/// The notices a sink was told, oldest first, as a law reads them: the
/// fake's `NoticeLog`, or a consumer's own sink.
pub trait Told {
    /// Every notice told so far, oldest first.
    fn told(&self) -> Vec<crate::Notice>;
}

/// What a token store was asked to keep, oldest first, as a law reads it:
/// the fake's `MemoryTokens`, or a consumer's own store.
pub trait TokenLog {
    /// Each `Covers` and scope a token was kept under, oldest first.
    fn kept(&self) -> Vec<(crate::Covers, crate::Scope)>;
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

/// Laws of the writes, through `Writes`.
pub mod writes {
    use std::fmt::Debug;

    use super::{got, hold, Law};
    use crate::{Blobs, Write, WriteKind, Writes};

    /// The writes the journal sends again when it cannot tell whether they
    /// landed: each idempotent by its inputs.
    pub const IDEMPOTENT: [WriteKind; 7] = [
        WriteKind::Prepare,
        WriteKind::EnsureMark,
        WriteKind::Propose,
        WriteKind::Tag,
        WriteKind::SetStatus,
        WriteKind::Upload,
        WriteKind::PublishRelease,
    ];

    /// A write is idempotent by its inputs: sent twice, it answers the
    /// same receipt and leaves the site as one send did, so the journal
    /// can settle a row by sending it again. `w` is of a kind in
    /// `IDEMPOTENT`, and the fixture lets it succeed; `look` reads what
    /// the site holds.
    pub fn a_write_is_idempotent_by_its_inputs<S: PartialEq + Debug>(
        h: &mut dyn Writes,
        w: &Write,
        files: &dyn Blobs,
        mut look: impl FnMut() -> S,
    ) -> Law {
        const LAW: &str = "writes::a_write_is_idempotent_by_its_inputs";
        let kind = w.kind();
        hold(IDEMPOTENT.contains(&kind), LAW, || {
            format!("{kind:?} is not a write sent again")
        })?;
        let once = got(LAW, "the first send", h.send(w, files))?;
        let after_once = look();
        let twice = got(LAW, "the second send", h.send(w, files))?;
        hold(twice == once, LAW, || {
            format!("{kind:?} sent again answered {twice:?}, not {once:?}")
        })?;
        let after_twice = look();
        hold(after_twice == after_once, LAW, || {
            format!("{kind:?} sent again changed the site: {after_once:?} became {after_twice:?}")
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

    /// The change is merged, as a merge that succeeded leaves it.
    fn merged(h: &mut dyn CodeHostWrites, law: &str, no: ChangeNo) -> Law {
        let now = state(h, law, no)?;
        hold(matches!(now, ChangeState::Merged { .. }), law, || {
            format!("the change is {now:?} after its merge")
        })
    }

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
        merged(h, LAW, one.change.id)?;
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
        merged(h, LAW, anew.change.id)?;
        let again = proposed(LAW, "the proposal after the merge", h.propose(p, files))?;
        hold(again.change.id == anew.change.id, LAW, || {
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

    /// A ticket's history is oldest first, and its newest n are its last
    /// n. `ticket(Depth::All).history` and `history(Whole)` agree item for
    /// item: the same events, in the same order, each with the same
    /// fields. The fixture's ticket has a history of events with times.
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
    use crate::{BlueprintNo, BlueprintWrites, Blueprints, Depth, Lost, MarkerEdit, Outcome};

    /// Each attachment of the blueprint `no` has its own link, and a name
    /// unless it was pasted. `named` are the fixture's named files, in the
    /// order its text links them; it also holds `pasted` pasted images.
    pub fn an_attachment_has_a_link_and_a_pasted_one_no_name(
        h: &mut dyn Blueprints,
        no: BlueprintNo,
        named: &[&str],
        pasted: usize,
    ) -> Law {
        const LAW: &str = "blueprints::an_attachment_has_a_link_and_a_pasted_one_no_name";
        let all = got(LAW, "the attachments", h.attachments(no))?.into_items();
        let names: Vec<&str> = all.iter().filter_map(|a| a.name.as_deref()).collect();
        hold(names == named, LAW, || {
            format!("named {names:?}, not {named:?}")
        })?;
        let unnamed = all.iter().filter(|a| a.name.is_none()).count();
        hold(unnamed == pasted, LAW, || {
            format!("{unnamed} with no name, not {pasted}")
        })?;
        let mut links: Vec<&str> = all.iter().map(|a| a.link.as_str()).collect();
        links.sort_unstable();
        links.dedup();
        hold(links.len() == all.len(), LAW, || {
            format!("{} links for {} attachments", links.len(), all.len())
        })
    }

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

/// Laws of naming a mark: on a terminal, in a log and in a body.
pub mod marks {
    use super::{got, hold, Law};
    use crate::{Act, BlueprintNo, BlueprintWrites, Choice, Cite, Mark, Site, State};

    /// The variants listed, every one of the enum but `Unknown`. The match
    /// has no arm for the rest, so a variant added to the enum stops the
    /// build here until it is listed.
    macro_rules! every {
        ($ty:ident: $($v:ident),+ $(,)?) => {{
            let _ = |w: $ty| match w {
                $($ty::$v)|+ | $ty::Unknown => (),
            };
            [$($ty::$v),+]
        }};
    }

    /// Every word of the factory this copy knows, as a mark: each act,
    /// state and choice, and the epic mark.
    pub fn factory_marks() -> Vec<Mark> {
        let acts = every!(Act: Approve, Plan, Refine, Finish, Block, Pause);
        let states = every!(
            State: Running,
            Planning,
            Refreshing,
            AwaitingMerge,
            Shipped,
            Question,
            NeedsPerson,
            Replaced,
            Refined,
            Escalated,
            Proposal
        );
        let choices = every!(Choice: Lean, Classic);
        let mut all: Vec<Mark> = acts.into_iter().map(Mark::Act).collect();
        all.extend(states.into_iter().map(Mark::State));
        all.extend(choices.into_iter().map(Mark::Choice));
        all.push(Mark::Epic);
        all
    }

    /// Every mark names a word: each of `factory_marks` and each of the
    /// fixture's `project` marks names a word of its own, not empty and
    /// unlike any other. `Mark::Unknown` names `""`.
    pub fn every_mark_names_a_word(site: &dyn Site, project: &[Mark]) -> Law {
        const LAW: &str = "marks::every_mark_names_a_word";
        let mut seen: Vec<(String, Mark)> = Vec::new();
        for m in factory_marks().iter().chain(project) {
            let name = site.mark_name(m);
            hold(!name.trim().is_empty(), LAW, || {
                format!("{m:?} names nothing")
            })?;
            if let Some((_, other)) = seen.iter().find(|(n, _)| *n == name) {
                return super::broken(LAW, format!("{m:?} and {other:?} both name {name:?}"));
            }
            seen.push((name, m.clone()));
        }
        let unknown = site.mark_name(&Mark::Unknown);
        hold(unknown.is_empty(), LAW, || {
            format!("an unknown mark names {unknown:?}")
        })
    }

    /// `Cite::Mark` in a body reads back as that mark's name: written in a
    /// comment on the blueprint `no`, the text reads back holding the word
    /// `Site::mark_name` gives, and no marker. `Mark::Unknown` cites as
    /// `""`. `h` is a handle of `site`.
    pub fn a_cited_mark_reads_back_as_its_name(
        site: &dyn Site,
        h: &mut dyn BlueprintWrites,
        no: BlueprintNo,
        mark: &Mark,
    ) -> Law {
        const LAW: &str = "marks::a_cited_mark_reads_back_as_its_name";
        let unknown = h.cite(&Cite::Mark(Mark::Unknown));
        hold(unknown.is_empty(), LAW, || {
            format!("an unknown mark cites as {unknown:?}")
        })?;
        let name = site.mark_name(mark);
        hold(!name.is_empty(), LAW, || format!("{mark:?} names nothing"))?;
        let text = h.cite(&Cite::Mark(mark.clone()));
        let posted = got(LAW, "the comment", h.comment(no, &text, &[]))?;
        let mut answers = h.comments_by_id(std::slice::from_ref(&posted.id));
        let c = match answers.pop() {
            Some((_, answer)) => got(LAW, "the comment read back", answer)?,
            None => return super::broken(LAW, "the comment was not answered"),
        };
        hold(c.body.contains(&name), LAW, || {
            format!("{text:?} read back as {:?}, without {name:?}", c.body)
        })?;
        hold(c.markers.is_empty(), LAW, || {
            format!("{text:?} read as {:?}", c.markers)
        })
    }
}

/// Laws of the deltas: a long history read over several passes.
pub mod delta {
    use std::fmt::Debug;

    use super::{got, hold, Law};
    use crate::{Blueprints, Bound, CodeHost, Cursor, Got, Tickets, ALL_MAX};

    /// A page as the law reads it: the two lists, where the next starts,
    /// and whether more wait.
    type Page<A, B> = (Vec<A>, Vec<B>, Cursor, bool);

    /// Read from `from` a page of `page` at a time: every page but the last
    /// holds `page` changes with `more`, the last holds the rest without,
    /// and the pages in order are the whole delta, each list in its order.
    /// Read on from the last `next`, nothing has changed. Past the bound is
    /// never `Unread(Cut)`. The fixture holds more changes than a page and
    /// no more than `ALL_MAX`, and nothing writes while the law reads.
    fn pages_resume<A: PartialEq + Debug, B: PartialEq + Debug>(
        law: &str,
        mut read: impl FnMut(Option<&Cursor>, Bound) -> Got<Page<A, B>>,
        from: Option<&Cursor>,
        page: Bound,
    ) -> Law {
        let (whole_a, whole_b, _, more) = got(law, "the whole delta", read(from, ALL_MAX))?;
        hold(!more, law, || "the whole delta says more".into())?;
        let total = whole_a.len() + whole_b.len();
        let size = page.get() as usize;
        hold(total > size, law, || {
            format!("the fixture holds {total} changes, no more than a page")
        })?;
        let (mut a, mut b) = (Vec::new(), Vec::new());
        let mut at = from.cloned();
        let mut done = false;
        for _ in 0..=total {
            let (pa, pb, next, more) = got(law, "a page", read(at.as_ref(), page))?;
            let n = pa.len() + pb.len();
            hold(if more { n == size } else { n <= size }, law, || {
                format!("a page of {n} changes with more {more}, the bound {size}")
            })?;
            a.extend(pa);
            b.extend(pb);
            at = Some(next);
            if !more {
                done = true;
                break;
            }
        }
        hold(done, law, || "the pages never ended".into())?;
        hold(a == whole_a && b == whole_b, law, || {
            format!("the pages read {a:?} and {b:?}, not {whole_a:?} and {whole_b:?}")
        })?;
        let (ra, rb, _, more) = got(law, "the delta after", read(at.as_ref(), page))?;
        hold(ra.is_empty() && rb.is_empty() && !more, law, || {
            format!("after the last page read {ra:?} and {rb:?}")
        })
    }

    /// `Tickets::delta` pages from `from`, `page` changes at a time.
    pub fn tickets_page_on(h: &mut dyn Tickets, from: Option<&Cursor>, page: Bound) -> Law {
        pages_resume(
            "delta::tickets_page_on",
            |f, b| h.delta(f, b).map(|d| (d.tickets, d.notes, d.next, d.more)),
            from,
            page,
        )
    }

    /// `Blueprints::delta` pages from `from`, `page` changes at a time.
    pub fn blueprints_page_on(h: &mut dyn Blueprints, from: Option<&Cursor>, page: Bound) -> Law {
        pages_resume(
            "delta::blueprints_page_on",
            |f, b| {
                h.delta(f, b)
                    .map(|d| (d.blueprints, d.comments, d.next, d.more))
            },
            from,
            page,
        )
    }

    /// `CodeHost::delta` pages from `from`, `page` changes at a time.
    pub fn changes_page_on(h: &mut dyn CodeHost, from: Option<&Cursor>, page: Bound) -> Law {
        pages_resume(
            "delta::changes_page_on",
            |f, b| {
                h.delta(f, b)
                    .map(|d| (d.changes, d.comments, d.next, d.more))
            },
            from,
            page,
        )
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
    /// of item, a mark) are words: a search of titles finds only hits whose
    /// title holds them as text, and a search of everything only hits whose
    /// title or text holds them, and each finds at least one. `search`
    /// searches for the words it is given; `text` answers a hit's title and
    /// text together, as the caller reads them. The fixture's words are
    /// plain ASCII, each in a title and in a text alone. An adapter may
    /// filter the site's hits to keep this law: the words, never the
    /// site's qualifiers, decide a hit.
    pub fn words_never_widen<T: Debug>(
        mut search: impl FnMut(&str, Within) -> Got<Took<T>>,
        title: impl Fn(&T) -> &str,
        text: impl Fn(&T) -> String,
        words: &[&str],
    ) -> Law {
        const LAW: &str = "search::words_never_widen";
        for w in words {
            let word = w.to_ascii_lowercase();
            for within in [Within::Titles, Within::Everything] {
                let hits = got(LAW, "a search", search(w, within))?;
                hold(!hits.items.is_empty(), LAW, || {
                    format!("{w:?} in {within:?} found nothing")
                })?;
                for hit in &hits.items {
                    let holds = match within {
                        Within::Titles => title(hit).to_ascii_lowercase().contains(&word),
                        Within::Everything => text(hit).to_ascii_lowercase().contains(&word),
                    };
                    hold(holds, LAW, || {
                        format!("{w:?} in {within:?} found {hit:?}, which does not hold it")
                    })?;
                }
            }
        }
        Ok(())
    }
}

/// Laws of the markers, read from a blueprint's body, a comment or a
/// ticket's note.
pub mod markers {
    use super::{got, hold, Broken, Law};
    use crate::{
        BlueprintNo, BlueprintWrites, Blueprints, Cite, CommentId, Depth, Marker, MarkerEdit,
        MarkerKind, TicketKey, Tickets, Window, ALL_MAX,
    };

    /// The text a marker law reads, through the handle that reads it.
    pub enum Text<'a> {
        /// A blueprint's body.
        Body(&'a mut dyn Blueprints, BlueprintNo),
        /// A comment on a blueprint, by its id.
        Comment(&'a mut dyn Blueprints, CommentId),
        /// A note on a ticket, by its id.
        Note(&'a mut dyn Tickets, TicketKey, CommentId),
    }

    /// The markers of the text, in the order the port answers them.
    fn read(at: &mut Text<'_>, law: &str) -> Result<Vec<Marker>, Broken> {
        match at {
            Text::Body(h, no) => Ok(got(
                law,
                "the blueprint",
                h.blueprint(*no, Depth::Cut { notes: 0 }),
            )?
            .markers),
            Text::Comment(h, id) => {
                let mut answers = h.comments_by_id(std::slice::from_ref(id));
                match answers.pop() {
                    Some((_, answer)) => Ok(got(law, "the comment", answer)?.markers),
                    None => Err(Broken {
                        law: law.to_string(),
                        what: "the comment was not answered".into(),
                    }),
                }
            }
            Text::Note(h, key, id) => {
                let notes = got(law, "the notes", h.notes(key, Window::Whole(ALL_MAX)))?;
                match notes.items.into_iter().find(|c| c.id == *id) {
                    Some(c) => Ok(c.markers),
                    None => Err(Broken {
                        law: law.to_string(),
                        what: "the note is not among the ticket's notes".into(),
                    }),
                }
            }
        }
    }

    fn is_readable(m: &Marker) -> bool {
        !matches!(m, Marker::Unreadable { .. } | Marker::Bare { .. })
    }

    fn readable_of(markers: &[Marker], kind: &MarkerKind) -> bool {
        markers.iter().any(|m| m.kind() == *kind && is_readable(m))
    }

    fn unreadable(mut at: Text<'_>, law: &str, kind: &MarkerKind) -> Law {
        let markers = read(&mut at, law)?;
        hold(
            markers.contains(&Marker::Unreadable { kind: kind.clone() }),
            law,
            || format!("no unreadable {kind:?} in {markers:?}"),
        )?;
        hold(!readable_of(&markers, kind), law, || {
            format!("a {kind:?} read in {markers:?}")
        })
    }

    /// A marker in a quote or in code is no marker. The fixture's text
    /// quotes one, and holds no other.
    pub fn a_quoted_marker_is_no_marker(mut at: Text<'_>) -> Law {
        const LAW: &str = "markers::a_quoted_marker_is_no_marker";
        let markers = read(&mut at, LAW)?;
        hold(markers.is_empty(), LAW, || format!("read {markers:?}"))
    }

    /// A cited marker is no marker to the port's reader, which skips
    /// quotes and code: `Cite::Marker` of a writable marker holds `typed`,
    /// the text a person types to make it as the site's adapter writes it,
    /// and, written in a comment on the blueprint `no`, reads back holding
    /// that text and no marker. A marker that is not writable (unknown,
    /// unreadable or bare of its kind) cites as `""`.
    pub fn a_cited_marker_is_no_marker(
        h: &mut dyn BlueprintWrites,
        no: BlueprintNo,
        marker: &Marker,
        typed: &str,
    ) -> Law {
        const LAW: &str = "markers::a_cited_marker_is_no_marker";
        hold(marker.writable(), LAW, || {
            format!("the fixture's {marker:?} is not writable")
        })?;
        let kind = marker.kind();
        for not in [
            Marker::Unknown,
            Marker::Unreadable { kind: kind.clone() },
            Marker::Bare { kind, read: None },
        ] {
            let text = h.cite(&Cite::Marker(not.clone()));
            hold(text.is_empty(), LAW, || {
                format!("{not:?} cites as {text:?}")
            })?;
        }
        let text = h.cite(&Cite::Marker(marker.clone()));
        hold(!typed.trim().is_empty(), LAW, || {
            format!("the fixture types {marker:?} as nothing")
        })?;
        hold(text.contains(typed), LAW, || {
            format!("{marker:?} cites as {text:?}, without {typed:?}")
        })?;
        let posted = got(LAW, "the comment", h.comment(no, &text, &[]))?;
        let mut answers = h.comments_by_id(std::slice::from_ref(&posted.id));
        let c = match answers.pop() {
            Some((_, answer)) => got(LAW, "the comment read back", answer)?,
            None => return super::broken(LAW, "the comment was not answered"),
        };
        hold(c.markers.is_empty(), LAW, || {
            format!("{text:?} read as {:?}", c.markers)
        })?;
        hold(c.body.contains(typed), LAW, || {
            format!("{text:?} read back as {:?}", c.body)
        })
    }

    /// A wrapped marker with a field that does not parse is unreadable of
    /// its kind.
    pub fn a_bad_field_is_unreadable(at: Text<'_>, kind: &MarkerKind) -> Law {
        unreadable(at, "markers::a_bad_field_is_unreadable", kind)
    }

    /// A number with leading zeros makes its marker unreadable.
    pub fn leading_zeros_are_unreadable(at: Text<'_>, kind: &MarkerKind) -> Law {
        unreadable(at, "markers::leading_zeros_are_unreadable", kind)
    }

    /// A blocked list over its limit is unreadable.
    pub fn a_list_over_its_limit_is_unreadable(at: Text<'_>) -> Law {
        unreadable(
            at,
            "markers::a_list_over_its_limit_is_unreadable",
            &MarkerKind::Blocked,
        )
    }

    /// A bare marker is `Bare` of its kind, with its payload when that
    /// reads (`read`), and is no marker that reads.
    pub fn a_bare_marker_is_bare(
        mut at: Text<'_>,
        kind: &MarkerKind,
        payload: Option<&Marker>,
    ) -> Law {
        const LAW: &str = "markers::a_bare_marker_is_bare";
        let markers = read(&mut at, LAW)?;
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
    /// text holds such words, and no marker.
    pub fn words_without_a_field_are_prose(mut at: Text<'_>) -> Law {
        const LAW: &str = "markers::words_without_a_field_are_prose";
        let markers = read(&mut at, LAW)?;
        hold(markers.is_empty(), LAW, || format!("read {markers:?}"))
    }

    /// The markers of a text keep the order they stand in, unreadable ones
    /// included, so an unreadable marker before or after a good one of its
    /// kind is there to hold the text. `order` is the fixture's, and holds
    /// an unreadable marker both before and after a good one of its kind.
    pub fn markers_keep_their_order(mut at: Text<'_>, order: &[Marker]) -> Law {
        const LAW: &str = "markers::markers_keep_their_order";
        let around = order.iter().enumerate().any(|(i, m)| {
            let bad = Marker::Unreadable { kind: m.kind() };
            is_readable(m) && order[..i].contains(&bad) && order[i + 1..].contains(&bad)
        });
        hold(around, LAW, || {
            format!("{order:?} holds no good marker with an unreadable one on each side")
        })?;
        let markers = read(&mut at, LAW)?;
        hold(markers == order, LAW, || {
            format!("read {markers:?}, not {order:?}")
        })
    }

    /// A marker of another blueprint than the item it is read from does
    /// not read.
    pub fn another_items_marker_does_not_read(mut at: Text<'_>, kind: &MarkerKind) -> Law {
        const LAW: &str = "markers::another_items_marker_does_not_read";
        let markers = read(&mut at, LAW)?;
        hold(!readable_of(&markers, kind), LAW, || {
            format!("a {kind:?} read in {markers:?}")
        })
    }

    /// A block's open line with no close line is unreadable of its kind.
    pub fn a_block_without_its_close_is_unreadable(at: Text<'_>, kind: &MarkerKind) -> Law {
        unreadable(at, "markers::a_block_without_its_close_is_unreadable", kind)
    }

    /// An unreadable marker is kept, where it stands, across an edit
    /// through `MarkerEdit::default()`, and so is a marker that reads. The
    /// fixture's body holds one of each.
    pub fn an_unreadable_marker_is_kept(h: &mut dyn BlueprintWrites, no: BlueprintNo) -> Law {
        const LAW: &str = "markers::an_unreadable_marker_is_kept";
        let before = got(
            LAW,
            "the blueprint",
            h.blueprint(no, Depth::Cut { notes: 0 }),
        )?;
        let bad: Vec<&Marker> = before.markers.iter().filter(|m| !is_readable(m)).collect();
        let good: Vec<&Marker> = before.markers.iter().filter(|m| is_readable(m)).collect();
        hold(
            bad.iter().any(|m| matches!(m, Marker::Unreadable { .. })),
            LAW,
            || "the fixture's body holds no unreadable marker".into(),
        )?;
        hold(!good.is_empty(), LAW, || {
            "the fixture's body holds no marker that reads".into()
        })?;
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
        let after = read(&mut Text::Body(h, no), LAW)?;
        for m in bad.iter().chain(good.iter()) {
            hold(after.contains(m), LAW, || {
                format!("{m:?} was lost in the edit: read {after:?}")
            })?;
        }
        hold(after == before.markers, LAW, || {
            format!("read {after:?} after the edit, not {:?}", before.markers)
        })
    }
}

/// Laws of what a scope may write on the code host. A write its scope
/// does not allow is `Forbidden` and changes nothing.
pub mod scopes {
    use super::{got, hold, Law};
    use crate::{
        Access, Blobs, ChangeNo, ChangeState, CloseReason, CodeHostWrites, CommitId, Depth,
        FileName, Filter, Mark, MergeHow, NewRelease, NewTicket, Outcome, Proposal, Status, Sum,
        TagName, TicketKey, TicketWrites, ALL_MAX,
    };

    /// What a scope's refused writes on a change and a release are asked
    /// with: an open change `no` that carries `on` and not `off`, a status
    /// to set at `at`, and a file to upload to the release of
    /// `upload.0`, which is there and does not hold it.
    pub struct Refusals<'a> {
        /// An open change.
        pub no: ChangeNo,
        /// A mark the change carries.
        pub on: &'a Mark,
        /// A mark it does not carry.
        pub off: &'a Mark,
        /// The commit a status is set at.
        pub at: &'a CommitId,
        /// The status to set.
        pub status: &'a Status,
        /// The release's tag, the file's name and its sum.
        pub upload: (&'a TagName, &'a FileName, &'a Sum),
    }

    /// An agent's token makes no file write and no push: a proposal is
    /// `Forbidden` and leaves no branch, and a remote to push is
    /// `Forbidden`. `h` acts as `Ours(Agent)`; `p` proposes a fresh
    /// branch.
    pub fn an_agent_makes_no_file_write_and_no_push(
        h: &mut dyn CodeHostWrites,
        p: &Proposal,
        files: &dyn Blobs,
    ) -> Law {
        const LAW: &str = "scopes::an_agent_makes_no_file_write_and_no_push";
        let proposed = h.propose(p, files);
        let refused = matches!(
            &proposed,
            Err(a) if a.done.is_empty()
                && matches!(&a.stopped, Some((_, Outcome::Forbidden)))
        );
        hold(refused, LAW, || {
            format!("an agent's proposal answered {proposed:?}")
        })?;
        let branch = h.branch(&p.branch);
        hold(branch == Ok(None), LAW, || {
            format!("the refused proposal left the branch: {branch:?}")
        })?;
        let push = h.remote(Access::Push).map(|_| ());
        hold(push == Err(Outcome::Forbidden), LAW, || {
            format!("an agent's push remote answered {push:?}")
        })
    }

    /// An agent's token merges, tags and releases nothing: each is
    /// `Forbidden` and leaves the change open, the tag absent and no
    /// release. `h` acts as `Ours(Agent)`; the rest as for
    /// `a_cycle_merges_tags_and_releases_nothing`.
    pub fn an_agent_merges_tags_and_releases_nothing(
        h: &mut dyn CodeHostWrites,
        no: ChangeNo,
        head: &CommitId,
        tag: &TagName,
        release: &NewRelease,
    ) -> Law {
        const LAW: &str = "scopes::an_agent_merges_tags_and_releases_nothing";
        refused_merge(h, LAW, no, head)?;
        tags_and_releases_nothing(h, LAW, head, tag, release)
    }

    /// A cycle's or a session's token merges, tags and releases nothing:
    /// each is `Forbidden` and leaves the change open, the tag absent and
    /// no release. A merge needs the product's grant (`Scope::CycleMerge`).
    /// `h` acts as `Ours(Cycle)` or `Ours(Session)`; `no` is an open change
    /// that can merge at `head`; `tag` is absent; `release` is of a tag
    /// that is there and has no release.
    pub fn a_cycle_merges_tags_and_releases_nothing(
        h: &mut dyn CodeHostWrites,
        no: ChangeNo,
        head: &CommitId,
        tag: &TagName,
        release: &NewRelease,
    ) -> Law {
        const LAW: &str = "scopes::a_cycle_merges_tags_and_releases_nothing";
        refused_merge(h, LAW, no, head)?;
        tags_and_releases_nothing(h, LAW, head, tag, release)
    }

    /// A cycle's or a session's token marks, unmarks and closes no change,
    /// sets no status and uploads nothing: each is `Forbidden` and leaves
    /// the change as it was. Those are the factory's own writes. `h` acts
    /// as `Ours(Cycle)`, `Ours(CycleMerge)` or `Ours(Session)`; `refused`
    /// as `Refusals` says.
    pub fn a_cycle_marks_closes_and_uploads_nothing(
        h: &mut dyn CodeHostWrites,
        refused: &Refusals,
        files: &dyn Blobs,
    ) -> Law {
        const LAW: &str = "scopes::a_cycle_marks_closes_and_uploads_nothing";
        marks_closes_and_uploads_nothing(h, LAW, refused, files)
    }

    /// A cycle's token with the product's merge grant merges its own change
    /// and no other, and still tags and releases nothing. `h` acts as
    /// `Ours(CycleMerge)`; `own` is an open change its account opened and
    /// `other` one another account opened, each able to merge at its head;
    /// `tag` and `release` as for `a_cycle_merges_tags_and_releases_nothing`.
    pub fn a_cycle_with_the_merge_grant_merges_its_own_change(
        h: &mut dyn CodeHostWrites,
        own: (ChangeNo, &CommitId),
        other: (ChangeNo, &CommitId),
        tag: &TagName,
        release: &NewRelease,
    ) -> Law {
        const LAW: &str = "scopes::a_cycle_with_the_merge_grant_merges_its_own_change";
        refused_merge(h, LAW, other.0, other.1)?;
        let merged = h.merge(own.0, own.1, MergeHow::Squash);
        hold(merged.is_ok(), LAW, || {
            format!("a merge of its own change answered {merged:?}")
        })?;
        let state = got(LAW, "its own change", h.change(own.0, ALL_MAX))?
            .digest
            .state;
        hold(matches!(state, ChangeState::Merged { .. }), LAW, || {
            format!("its own change is {state:?} after the merge")
        })?;
        tags_and_releases_nothing(h, LAW, own.1, tag, release)
    }

    /// A merge of `no` is `Forbidden` and leaves it open.
    fn refused_merge(h: &mut dyn CodeHostWrites, law: &str, no: ChangeNo, head: &CommitId) -> Law {
        let merged = h.merge(no, head, MergeHow::Squash);
        hold(merged == Err(Outcome::Forbidden), law, || {
            format!("a merge of {no:?} answered {merged:?}")
        })?;
        let state = h.change(no, ALL_MAX).map(|c| c.digest.state);
        hold(state == Ok(ChangeState::Open), law, || {
            format!("{no:?} is {state:?} after the refused merge")
        })
    }

    /// A mark, an unmark and a close of a change, a status and an upload are
    /// each `Forbidden`, and leave the change as it was.
    fn marks_closes_and_uploads_nothing(
        h: &mut dyn CodeHostWrites,
        law: &str,
        r: &Refusals,
        files: &dyn Blobs,
    ) -> Law {
        let forbidden = Err(Outcome::Forbidden);
        let before = got(law, "the change", h.change(r.no, ALL_MAX))?.digest;
        let marked = h.mark_change(r.no, r.off);
        hold(marked == forbidden, law, || {
            format!("a mark of a change answered {marked:?}")
        })?;
        let unmarked = h.unmark_change(r.no, r.on);
        hold(unmarked == forbidden, law, || {
            format!("an unmark of a change answered {unmarked:?}")
        })?;
        let closed = h.close_change(r.no);
        hold(closed == forbidden, law, || {
            format!("a close of a change answered {closed:?}")
        })?;
        let after = got(law, "the change again", h.change(r.no, ALL_MAX))?.digest;
        hold(after == before, law, || {
            format!("the refused writes left {after:?}, not {before:?}")
        })?;
        let status = h.set_status(r.at, r.status);
        hold(status == forbidden, law, || {
            format!("a status answered {status:?}")
        })?;
        let (tag, name, sum) = r.upload;
        let release = got(law, "the release", h.release(tag))?;
        let uploaded = h.upload(&release.id, name, sum, files);
        hold(uploaded == forbidden, law, || {
            format!("an upload answered {uploaded:?}")
        })
    }

    /// A tag and a release are each `Forbidden`, and leave the tag absent
    /// and no release.
    fn tags_and_releases_nothing(
        h: &mut dyn CodeHostWrites,
        law: &str,
        head: &CommitId,
        tag: &TagName,
        release: &NewRelease,
    ) -> Law {
        let forbidden = Err(Outcome::Forbidden);
        let tagged = h.tag(tag, head);
        hold(tagged == forbidden, law, || {
            format!("a tag answered {tagged:?}")
        })?;
        let there = h.resolve_tag(tag);
        hold(there == Err(Outcome::NotFound), law, || {
            format!("the refused tag reads {there:?}")
        })?;
        let made = h.new_release(release).map(|_| ());
        hold(made == forbidden, law, || {
            format!("a release answered {made:?}")
        })?;
        let read = h.release(&release.tag).map(|_| ());
        hold(read == Err(Outcome::NotFound), law, || {
            format!("the refused release reads {read:?}")
        })
    }

    /// A cycle's token marks, unmarks and closes no ticket, and files none
    /// that carries more than text: each is `Forbidden`, the ticket reads
    /// as it did and no ticket is filed. Marks on a ticket are the
    /// factory's own writes. `h` acts as `Ours(Cycle)` or
    /// `Ours(CycleMerge)`; `key` is an open ticket that carries `on` and
    /// not `off`; `marked` is a ticket to file that carries a mark.
    pub fn a_cycle_marks_and_closes_no_ticket(
        h: &mut dyn TicketWrites,
        key: &TicketKey,
        on: &Mark,
        off: &Mark,
        marked: &NewTicket,
    ) -> Law {
        const LAW: &str = "scopes::a_cycle_marks_and_closes_no_ticket";
        let forbidden = Err(Outcome::Forbidden);
        let depth = Depth::Cut { notes: 0 };
        let before = got(LAW, "the ticket", h.ticket(key, depth, None))?.digest;
        let count =
            |h: &mut dyn TicketWrites| h.list(&Filter::default(), ALL_MAX).map(|w| w.items().len());
        let tickets = got(LAW, "the tickets", count(h))?;
        let marked_on = h.mark(key, off);
        hold(marked_on == forbidden, LAW, || {
            format!("a mark answered {marked_on:?}")
        })?;
        let unmarked = h.unmark(key, on);
        hold(unmarked == forbidden, LAW, || {
            format!("an unmark answered {unmarked:?}")
        })?;
        let closed = h.close(key, &CloseReason::Done);
        hold(closed == forbidden, LAW, || {
            format!("a close answered {closed:?}")
        })?;
        let after = got(LAW, "the ticket again", h.ticket(key, depth, None))?.digest;
        hold(after == before, LAW, || {
            format!("the refused writes left {after:?}, not {before:?}")
        })?;
        let filed = h.file(marked).map(|_| ());
        hold(filed == forbidden, LAW, || {
            format!("filing a marked ticket answered {filed:?}")
        })?;
        let now = got(LAW, "the tickets again", count(h))?;
        hold(now == tickets, LAW, || {
            format!("the refused filing left {now} tickets, not {tickets}")
        })
    }
}

/// Laws of identity and its tokens.
pub mod identity {
    use super::{got, hold, Law, TokenLog};
    use crate::{Covers, Got, Identity, Outcome, ProjectName, Scope, Token};

    fn kept_since(tokens: &dyn TokenLog, from: usize) -> Vec<(Covers, Scope)> {
        tokens.kept().into_iter().skip(from).collect()
    }

    /// One token over a set of projects covers that set alone: asked with a
    /// project the grant does not cover, it is `Forbidden` and nothing is
    /// kept; asked with the set, it is kept under that set and no other.
    /// `set` is the fixture's grant's, the handle's project among them.
    pub fn a_token_covers_its_set_alone(
        h: &mut dyn Identity,
        tokens: &dyn TokenLog,
        scope: Scope,
        set: &[ProjectName],
        outside: &ProjectName,
    ) -> Law {
        const LAW: &str = "identity::a_token_covers_its_set_alone";
        let from = tokens.kept().len();
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
        let kept = kept_since(tokens, from);
        hold(!kept.is_empty(), LAW, || {
            "a token over the set was not kept".into()
        })?;
        for asked in kept {
            let ok = matches!(
                &asked,
                (Covers::Projects { projects, .. }, s)
                    if projects.as_slice() == sorted.as_slice() && *s == scope
            );
            hold(ok, LAW, || format!("kept as {asked:?}"))?;
        }
        Ok(())
    }

    /// A given token is answered as it was handed in, never minted, renewed
    /// or kept. Past its time, its handle is `SignedOut`, made or asked,
    /// and `Keyring::signed_out` is called once for its key. `h` acts as
    /// `Given` with `given`; `expired` is the making of a handle whose
    /// given token is past its time; `signed_out` counts the keyring's
    /// `signed_out` calls for that handle's key; `requests` counts the
    /// requests the site was sent, so a mint shows.
    pub fn a_given_token_is_never_minted(
        h: &mut dyn Identity,
        tokens: &dyn TokenLog,
        scope: Scope,
        given: &Token,
        mut expired: impl FnMut() -> Got<Box<dyn Identity + Send>>,
        signed_out: impl Fn() -> usize,
        requests: impl Fn() -> u32,
    ) -> Law {
        const LAW: &str = "identity::a_given_token_is_never_minted";
        let from = tokens.kept().len();
        let sent = requests();
        let t = got(LAW, "the token", h.token(scope))?;
        let now = requests();
        hold(now == sent, LAW, || {
            format!("a given token sent {} requests", now - sent)
        })?;
        hold(
            t.secret.expose() == given.secret.expose() && t.until == given.until,
            LAW,
            || "the token is not the one handed in".into(),
        )?;
        hold(kept_since(tokens, from).is_empty(), LAW, || {
            "a given token was kept".into()
        })?;
        // A given token answers for its own scope alone: a wider one is
        // refused, as `token_over` refuses, with no request.
        if scope != Scope::Full {
            let sent = requests();
            let wide = h.token(Scope::Full).map(|_| ());
            hold(wide == Err(Outcome::Forbidden), LAW, || {
                format!("a given token of {scope:?} answered {wide:?} for Full")
            })?;
            let now = requests();
            hold(now == sent, LAW, || {
                format!("a refused wider scope sent {} requests", now - sent)
            })?;
        }
        // Asked twice, by two handles, each asked twice: the keyring is told
        // once, however often the token is asked for.
        for _ in 0..2 {
            let late = match expired() {
                Err(o) => Err(o),
                Ok(mut h) => {
                    let first = h.token(scope).map(|_| ());
                    let again = h.token(scope).map(|_| ());
                    hold(again == first, LAW, || {
                        format!("a given token past its time answered {first:?}, then {again:?}")
                    })?;
                    first
                }
            };
            hold(late == Err(Outcome::SignedOut), LAW, || {
                format!("a given token past its time answered {late:?}")
            })?;
        }
        let n = signed_out();
        hold(n == 1, LAW, || {
            format!("the keyring was told of the sign-out {n} times, not once")
        })
    }

    /// A site built without the factory's key answers its tokens from the
    /// store alone: none kept is `SignedOut`, and nothing is minted or
    /// kept. The store holds no token of `scope` for the handle's project;
    /// `requests` counts the requests the site was sent.
    pub fn a_keyless_site_never_mints(
        h: &mut dyn Identity,
        tokens: &dyn TokenLog,
        scope: Scope,
        requests: impl Fn() -> u32,
    ) -> Law {
        const LAW: &str = "identity::a_keyless_site_never_mints";
        let from = tokens.kept().len();
        let sent = requests();
        let t = h.token(scope).map(|_| ());
        hold(t == Err(Outcome::SignedOut), LAW, || {
            format!("with none kept it answered {t:?}")
        })?;
        let now = requests();
        hold(now == sent, LAW, || {
            format!("a keyless site sent {} requests", now - sent)
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
    use super::{hold, Law, Told};
    use crate::{
        At, Depth, Health, HealthState, Holder, HostId, Identity, Ledger, Outcome, Passing, Pool,
        ProjectName, Scope, Sent, TicketKey, Tickets, Whose,
    };

    /// What the handle's site was wired with: the ledger, the sink and the
    /// host its notices name, and a count of the requests it sent.
    #[derive(Clone, Copy)]
    pub struct Wiring<'a> {
        /// The ledger.
        pub ledger: &'a dyn Ledger,
        /// The sink, as the law reads it.
        pub sink: &'a dyn Told,
        /// The host.
        pub host: &'a HostId,
        /// How many requests the handle's site has sent so far.
        pub requests: &'a dyn Fn() -> u32,
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
        let until = At::new(now.secs() + 3600);
        let spent = HealthState::Spent { pool, until };
        let failing = outage(wired, LAW, now)?;
        set(wired, LAW, &Whose::Pool(holder.clone(), pool), &spent)?;
        let told = (wired.sink.told().len(), (wired.requests)());
        let answer = h.ticket(key, Depth::Cut { notes: 0 }, None).map(|_| ());
        hold(answer == Err(Outcome::Spent { pool, until }), LAW, || {
            format!("a held call answered {answer:?}")
        })?;
        still_failing(wired, LAW, told, &failing, "a held call")
    }

    /// Put `state` under `whose` in the wired ledger.
    fn set(wired: Wiring<'_>, law: &str, whose: &Whose, state: &HealthState) -> Law {
        let Wiring { ledger, host, .. } = wired;
        let was = ledger.health(host, whose).map_err(|_| super::Broken {
            law: law.into(),
            what: "the ledger is down".into(),
        })?;
        let won = ledger.swap_health(host, whose, &was, state);
        hold(won == Ok(true), law, || {
            format!("the ledger kept no {state:?}")
        })
    }

    /// Put an outage of the host in the wired ledger, and answer it.
    fn outage(wired: Wiring<'_>, law: &str, now: At) -> Result<HealthState, super::Broken> {
        let failing = HealthState::Failing {
            since: now,
            why: Passing::Timeout,
        };
        set(wired, law, &Whose::Host, &failing)?;
        Ok(failing)
    }

    /// No request went out since `sent` were counted.
    fn no_request(law: &str, sent: u32, now: u32, what: &str) -> Law {
        hold(now == sent, law, || {
            format!("{what} sent {} requests", now.saturating_sub(sent))
        })
    }

    /// No request went out since `sent`, no notice since the `told`th is
    /// `Recovered`, and the host is still `failing`.
    fn still_failing(
        wired: Wiring<'_>,
        law: &str,
        (told, sent): (usize, u32),
        failing: &HealthState,
        what: &str,
    ) -> Law {
        let Wiring {
            ledger,
            sink,
            host,
            requests,
        } = wired;
        no_request(law, sent, requests(), what)?;
        let new: Vec<_> = sink.told().into_iter().skip(told).collect();
        hold(
            !new.iter().any(|n| n.what == Health::Recovered),
            law,
            || format!("{what} raised {new:?}"),
        )?;
        let host_now = ledger.health(host, &Whose::Host);
        hold(host_now.as_ref() == Ok(failing), law, || {
            format!("the host is {host_now:?} after {what}")
        })
    }

    /// During an outage, a token answered from the store makes no request,
    /// and raises no `Recovered`: the host stays failing. The store the
    /// handle's site reads holds a good token of `scope` for its project.
    pub fn a_kept_token_raises_no_recovered(
        h: &mut dyn Identity,
        wired: Wiring<'_>,
        scope: Scope,
        now: At,
    ) -> Law {
        const LAW: &str = "health::a_kept_token_raises_no_recovered";
        let failing = outage(wired, LAW, now)?;
        let told = (wired.sink.told().len(), (wired.requests)());
        let answer = h.token(scope).map(|_| ());
        hold(answer.is_ok(), LAW, || {
            format!("a kept token answered {answer:?}")
        })?;
        still_failing(wired, LAW, told, &failing, "a kept token")
    }

    /// During an outage, an expired token, kept or given, answers
    /// `SignedOut` with no request, and raises no `Recovered`: the host
    /// stays failing. The handle's token of `scope` is past its time, and
    /// no other is kept or can be minted.
    pub fn an_expired_token_raises_no_recovered(
        h: &mut dyn Identity,
        wired: Wiring<'_>,
        scope: Scope,
        now: At,
    ) -> Law {
        const LAW: &str = "health::an_expired_token_raises_no_recovered";
        let failing = outage(wired, LAW, now)?;
        let told = (wired.sink.told().len(), (wired.requests)());
        let answer = h.token(scope).map(|_| ());
        hold(answer == Err(Outcome::SignedOut), LAW, || {
            format!("an expired token answered {answer:?}")
        })?;
        still_failing(wired, LAW, told, &failing, "an expired token")
    }

    /// During an outage, a token over a set with a project outside the
    /// grant answers `Forbidden` with no request, and raises no
    /// `Recovered`: the host stays failing. `wider` holds such a project.
    pub fn an_out_of_set_token_raises_no_recovered(
        h: &mut dyn Identity,
        wired: Wiring<'_>,
        scope: Scope,
        wider: &[ProjectName],
        now: At,
    ) -> Law {
        const LAW: &str = "health::an_out_of_set_token_raises_no_recovered";
        let failing = outage(wired, LAW, now)?;
        let told = (wired.sink.told().len(), (wired.requests)());
        let answer = h.token_over(scope, wider).map(|_| ());
        hold(answer == Err(Outcome::Forbidden), LAW, || {
            format!("a set outside the grant answered {answer:?}")
        })?;
        still_failing(wired, LAW, told, &failing, "an out-of-set token")
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
        let Wiring {
            ledger,
            sink,
            host,
            requests,
        } = wired;
        let before = ledger.health(host, &Whose::Host);
        hold(before == Ok(HealthState::Healthy), LAW, || {
            format!("the fixture's host is {before:?}")
        })?;
        let told = sink.told().len();
        let sent = requests();
        let answer = h.ticket(key, Depth::Cut { notes: 0 }, None).map(|_| ());
        no_request(LAW, sent, requests(), "a call past the deadline")?;
        let timeout = Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No,
        };
        hold(answer == Err(timeout), LAW, || {
            format!("a call past the deadline answered {answer:?}")
        })?;
        let new: Vec<_> = sink.told().into_iter().skip(told).collect();
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

/// Laws of a `Ledger` itself, as its trait's docs give them. Each takes a
/// ledger and a host it keeps nothing for yet, and a time of the caller's.
pub mod ledger {
    use super::{hold, Broken, Law};
    use crate::{
        At, Budget, Failure, HealthState, Holder, HolderId, HostId, Ledger, Outcome, Passing, Pool,
        Sent, SiteId, Unread, Whose,
    };

    fn up<T>(law: &str, what: &str, answer: Result<T, crate::LedgerDown>) -> Result<T, Broken> {
        answer.map_err(|_| Broken {
            law: law.to_string(),
            what: format!("{what}: the ledger is down"),
        })
    }

    fn ours() -> Holder {
        Holder::Ours(HolderId::new("law-ours"))
    }

    fn other() -> Holder {
        Holder::Anonymous(HolderId::new("law-other"))
    }

    /// What is put for a holder's pool is what is read back, and each of
    /// host, holder and pool is a key of its own. Nothing put reads None.
    pub fn a_budget_is_read_back(l: &dyn Ledger, host: &HostId, now: At) -> Law {
        const LAW: &str = "ledger::a_budget_is_read_back";
        let first = up(LAW, "a read", l.budget(host, &ours(), Pool::Calls))?;
        hold(first.is_none(), LAW, || {
            format!("nothing put reads {first:?}")
        })?;
        let b = Budget {
            left: 10,
            resets: At::new(now.secs() + 60),
        };
        up(LAW, "a put", l.put_budget(host, &ours(), Pool::Calls, b))?;
        let later = Budget { left: 9, ..b };
        up(
            LAW,
            "a put",
            l.put_budget(host, &ours(), Pool::Calls, later),
        )?;
        let read = up(LAW, "a read", l.budget(host, &ours(), Pool::Calls))?;
        hold(read == Some(later), LAW, || {
            format!("{later:?} was put last and {read:?} read")
        })?;
        for (holder, pool) in [(ours(), Pool::Search), (other(), Pool::Calls)] {
            let read = up(LAW, "a read", l.budget(host, &holder, pool))?;
            hold(read.is_none(), LAW, || {
                format!("{holder:?}'s {pool:?}, never put, reads {read:?}")
            })?;
        }
        let elsewhere = HostId::new(format!("{}-elsewhere", host.as_str()));
        let read = up(LAW, "a read", l.budget(&elsewhere, &ours(), Pool::Calls))?;
        hold(read.is_none(), LAW, || {
            format!("another host reads {read:?}")
        })
    }

    /// A state never written is `Healthy`, for the host, a holder and a
    /// pool.
    pub fn an_unkept_state_is_healthy(l: &dyn Ledger, host: &HostId) -> Law {
        const LAW: &str = "ledger::an_unkept_state_is_healthy";
        for whose in [
            Whose::Host,
            Whose::Holder(ours()),
            Whose::Pool(ours(), Pool::Writes),
        ] {
            let s = up(LAW, "a read", l.health(host, &whose))?;
            hold(s == HealthState::Healthy, LAW, || {
                format!("{whose:?}, never written, is {s:?}")
            })?;
        }
        Ok(())
    }

    /// `swap_health` writes only when the state is still what the caller
    /// expects, answers whether it wrote, and a `Healthy` written reads
    /// `Healthy` again. Whose states are apart.
    pub fn a_swap_writes_only_on_a_match(l: &dyn Ledger, host: &HostId, now: At) -> Law {
        const LAW: &str = "ledger::a_swap_writes_only_on_a_match";
        let healthy = HealthState::Healthy;
        let failing = HealthState::Failing {
            since: now,
            why: Passing::Timeout,
        };
        let spent = HealthState::Spent {
            pool: Pool::Writes,
            until: At::new(now.secs() + 60),
        };
        let pool = Whose::Pool(ours(), Pool::Writes);
        let won = up(
            LAW,
            "a swap",
            l.swap_health(host, &Whose::Host, &failing, &healthy),
        )?;
        hold(!won, LAW, || "a swap from a state not kept wrote".into())?;
        let won = up(
            LAW,
            "a swap",
            l.swap_health(host, &Whose::Host, &healthy, &failing),
        )?;
        hold(won, LAW, || {
            "a swap from the kept state did not write".into()
        })?;
        let won = up(LAW, "a swap", l.swap_health(host, &pool, &healthy, &spent))?;
        hold(won, LAW, || "a swap of a pool did not write".into())?;
        let s = up(LAW, "a read", l.health(host, &Whose::Host))?;
        hold(s == failing, LAW, || {
            format!("the host is {s:?}, not {failing:?}")
        })?;
        let lost = up(
            LAW,
            "a swap",
            l.swap_health(host, &Whose::Host, &healthy, &spent),
        )?;
        hold(!lost, LAW, || "a swap from a stale state wrote".into())?;
        let s = up(LAW, "a read", l.health(host, &Whose::Host))?;
        hold(s == failing, LAW, || format!("a lost swap left {s:?}"))?;
        let won = up(
            LAW,
            "a swap",
            l.swap_health(host, &Whose::Host, &failing, &healthy),
        )?;
        hold(won, LAW, || "a swap back to Healthy did not write".into())?;
        let s = up(LAW, "a read", l.health(host, &Whose::Host))?;
        hold(s == healthy, LAW, || {
            format!("the host is {s:?} after its recovery")
        })?;
        let s = up(LAW, "a read", l.health(host, &pool))?;
        hold(s == spent, LAW, || {
            format!("the pool is {s:?}, not {spent:?}")
        })
    }

    /// `failed` keeps what it is given, an odd answer too: the adapter
    /// chooses which failures count. Each `failed` drops the host's failures
    /// older than its own failure's time less `keep`, and keeps one at that
    /// time exactly. `failures` answers those at or after `since`, oldest
    /// first, of that host alone.
    pub fn failures_are_kept_within_keep_oldest_first(
        l: &dyn Ledger,
        host: &HostId,
        now: At,
    ) -> Law {
        const LAW: &str = "ledger::failures_are_kept_within_keep_oldest_first";
        let failure = |secs: i64, outcome: Outcome| Failure {
            at: At::new(now.secs() + secs),
            site: SiteId::new("law-site"),
            outcome,
        };
        let down = || Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::No,
        };
        let odd = Outcome::Unread(Unread::Inconsistent);
        let elsewhere = HostId::new(format!("{}-elsewhere", host.as_str()));
        up(
            LAW,
            "a failure kept",
            l.failed(&elsewhere, failure(-500, down()), 60),
        )?;
        // An older failure given after a newer one is kept: it is within 60 s
        // of its own time, though not of the newest.
        up(
            LAW,
            "a failure kept",
            l.failed(host, failure(0, down()), 60),
        )?;
        up(
            LAW,
            "a failure kept",
            l.failed(host, failure(-100, down()), 60),
        )?;
        let both = up(LAW, "a read", l.failures(host, At::new(0)))?;
        hold(
            both == vec![failure(-100, down()), failure(0, down())],
            LAW,
            || format!("after 0 then -100 it kept {both:?}, not both"),
        )?;
        // -100 is exactly 60 s before -40: kept.
        up(
            LAW,
            "a failure kept",
            l.failed(host, failure(-40, odd.clone()), 60),
        )?;
        let at_edge = up(LAW, "a read", l.failures(host, At::new(0)))?;
        hold(at_edge.len() == 3, LAW, || {
            format!("a failure exactly `keep` older was dropped: {at_edge:?}")
        })?;
        // -40 is exactly 60 s before 20: kept; -100 goes.
        up(
            LAW,
            "a failure kept",
            l.failed(host, failure(20, down()), 60),
        )?;
        let all = up(LAW, "a read", l.failures(host, At::new(0)))?;
        let want = vec![failure(-40, odd), failure(0, down()), failure(20, down())];
        hold(all == want, LAW, || format!("kept {all:?}, not {want:?}"))?;
        let since = up(LAW, "a read", l.failures(host, At::new(now.secs() - 40)))?;
        hold(since == want, LAW, || {
            format!("since the oldest kept answered {since:?}")
        })?;
        let since = up(LAW, "a read", l.failures(host, At::new(now.secs() - 39)))?;
        hold(since == want[1..], LAW, || {
            format!("since a second after the oldest answered {since:?}")
        })?;
        let theirs = up(LAW, "a read", l.failures(&elsewhere, At::new(0)))?;
        hold(theirs == vec![failure(-500, down())], LAW, || {
            format!("another host's failures are {theirs:?}")
        })
    }

    /// Every law of this module, over one ledger, each on a host of its own
    /// named after `host`.
    pub fn all(l: &dyn Ledger, host: &HostId, now: At) -> Law {
        let on = |law: &str| HostId::new(format!("{}-{law}", host.as_str()));
        a_budget_is_read_back(l, &on("budget"), now)?;
        an_unkept_state_is_healthy(l, &on("unkept"))?;
        a_swap_writes_only_on_a_match(l, &on("swap"), now)?;
        failures_are_kept_within_keep_oldest_first(l, &on("failures"), now)
    }
}
