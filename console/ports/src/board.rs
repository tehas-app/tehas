//! The joined reads across tickets, blueprints and the code host: `Board`,
//! what it is asked, `BoardAsk` and `ChangeAsk`, what it answers,
//! `WorkItem`, `OpenWork` and `ChangeItem`, and `Join`, the default join
//! over three owned handles and `Links`.

use std::num::NonZeroU32;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    At, Blueprint, BlueprintNo, Blueprints, Bound, ChangeDigest, ChangeEvent, ChangeFilter,
    ChangeState, ChangeStates, ChangedFile, CodeHost, Comment, Depth, Filter, Got, HappenedKind,
    Links, Outcome, Ref, Report, Reports, TargetId, Ticket, TicketDigest, TicketEvent, TicketKey,
    Tickets, Took, Unread, Whole, Window,
};

/// What a joined read asks for, so a cheap poll asks less.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoardAsk {
    /// How much of each ticket, its notes and its blueprints to read. It
    /// gives each ticket's history too (`Depth::history`).
    pub depth: Depth,
    /// Each ticket's history in place of the depth's, read with the ticket
    /// in one read. A poll that must not miss an approval asks
    /// `Window::Whole`; a queue read may ask `Window::Last` with
    /// `Depth::Last`, and reads a `more` as possibly cut.
    pub history: Option<Window>,
    /// A window of each ticket's history for each kind named, so a busy
    /// kind cannot cut another out: a poll asks its marks whole beside the
    /// newest few of every other kind. Not empty, the history is each
    /// kind's own window, merged oldest first; a kind not named is not
    /// read, and `history` and the depth's window give no history. `more`
    /// is set when any window held more. Empty, the history is as
    /// `history` says. Missing, it reads as empty.
    #[serde(default)]
    pub history_by_kind: Vec<(HappenedKind, Window)>,
    /// The changes that close each item's blueprints: the states taken and
    /// how many, in the order `Board` gives. None reads none.
    pub changes: Option<(ChangeStates, Window)>,
    /// Which tickets.
    pub filter: Filter,
    /// The control ticket, read whole with its history beside the rest.
    pub control: Option<TicketKey>,
}

new!(
    BoardAsk { depth: Depth },
    history,
    history_by_kind,
    changes,
    filter,
    control
);

/// One ticket with its notes, its blueprints and their changes. A list the
/// ask did not read is empty, with `more`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorkItem {
    /// The ticket.
    pub ticket: Ticket,
    /// Notes on the ticket itself; empty where ticket and blueprint are one.
    /// As deep as `BoardAsk.depth`, with `more`.
    pub notes: Took<Comment>,
    /// The blueprints linked to it.
    pub blueprints: Vec<Blueprint>,
    /// The changes that close those blueprints, in the order `Board` gives,
    /// with `more`.
    pub changes: Took<ChangeDigest>,
}

new!(WorkItem { ticket: Ticket }, notes, blueprints, changes);

impl WorkItem {
    /// The comments a ticket marker is read from: the notes, or, where
    /// ticket and blueprint are one item (`TicketKey::same_item`), that
    /// blueprint's comments.
    pub fn ticket_comments(&self) -> &Took<Comment> {
        if let Some(no) = self.ticket.digest.key.same_item() {
            if let Some(same) = self.blueprints.iter().find(|b| b.no == no) {
                return &same.comments;
            }
        }
        &self.notes
    }
}

/// The list of open tickets is whole; each item has its own answer, so one
/// ticket past a bound, or unread, is `Unread` for that item alone and
/// fails closed alone. The others are still answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OpenWork {
    /// Each open ticket, with its own answer, in list order.
    pub items: Whole<(TicketKey, Got<WorkItem>)>,
    /// The control ticket with its own answer, if it was asked for. Its
    /// failure fails it alone, not the list.
    pub control: Option<Got<Ticket>>,
}

/// What a read centred on changes asks for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChangeAsk {
    /// Which changes.
    pub filter: ChangeFilter,
    /// Each change's files, whole up to this bound per change. One change
    /// past it fails the read.
    pub files: Option<Bound>,
    /// The blueprints each closes, with their text and marks, and their
    /// tickets.
    pub closing: bool,
    /// The open changes that close the same blueprints, whole up to this
    /// bound per change, the change itself counted where it is open.
    pub siblings: Option<Bound>,
    /// Each change's events: a stale read asks `Window::Last(50)`.
    pub activity: Option<Window>,
}

/// One change with what the ask named.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChangeItem {
    /// The change.
    pub change: ChangeDigest,
    /// Its files, if asked.
    pub files: Option<Whole<ChangedFile>>,
    /// The blueprints of this project it closes, each with its linked
    /// ticket, if asked. A blueprint of another project, or one not found,
    /// is left out.
    pub closing: Vec<(Blueprint, Option<TicketDigest>)>,
    /// The other open changes that close the same blueprints, if asked.
    pub siblings: Vec<ChangeDigest>,
    /// Its events, if asked.
    pub activity: Option<Took<ChangeEvent>>,
}

new!(
    ChangeItem {
        change: ChangeDigest
    },
    files,
    closing,
    siblings,
    activity
);

/// The joined reads. A site that serves all three ports answers them with
/// one question and owns its cost.
///
/// The items of a per-item read come in the tickets port's list order:
/// oldest filed first, ties broken by key. An item's changes come open
/// first, in the code host's order, then the rest newest first by the time
/// each was merged or closed, ties broken by the higher number. Asked with
/// `Window::Last(n)`, the open changes past n make the item `Unread(Cut)`;
/// the merged and closed ones are the newest that fit in n, with `more`.
pub trait Board: Reports {
    /// The open tickets the filter takes, each with its own answer, and the
    /// control ticket with its own. Open tickets alone, whatever the
    /// filter's `open` says. Each ticket's parts and blockers are whole at
    /// every `Depth`, as `Ticket.parts` says: past a bound that item is
    /// `Unread(Cut)`, never a short list.
    fn open_work(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork>;
    /// One ticket whole, as the ask names: its parts and blockers whole at
    /// every `Depth`, or the item `Unread(Cut)`.
    fn work_item(&mut self, key: &TicketKey, ask: &BoardAsk) -> Got<WorkItem>;
    /// The tickets closed since a time. Per item, as `OpenWork.items`. The
    /// ask's filter is not applied: every ticket closed since then.
    fn closed_work(
        &mut self,
        ask: &BoardAsk,
        since: At,
        bound: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>>;
    /// The tickets of a target. Per item, as `OpenWork.items`. The ask's
    /// filter is not applied: every ticket of the target.
    fn target_work(
        &mut self,
        ask: &BoardAsk,
        target: &TargetId,
        bound: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>>;
    /// Changes with what they close, in one question where the site allows:
    /// the refresh's read, a review read, the stale and overlap reads, and
    /// the loop that counts open work every two minutes.
    fn open_changes(&mut self, ask: &ChangeAsk, bound: Bound) -> Got<Whole<ChangeItem>>;
}

/// The join over three read handles and the links, for a split setup.
/// Pure code over the traits; each supplier keeps its own budget. It owns
/// its handles, which may come from two sites, and borrows nothing.
pub struct Join {
    /// The tickets handle.
    pub tickets: Box<dyn Tickets + Send>,
    /// The blueprints handle.
    pub blueprints: Box<dyn Blueprints + Send>,
    /// The code host handle.
    pub code_host: Box<dyn CodeHost + Send>,
    /// Which blueprints each ticket has.
    pub links: Arc<dyn Links>,
}

/// The most changes of an item's blueprints the join reads to order them
/// for a `Window::Last`. Past it, that item is `Unread(Cut)`.
const ITEM_MAX: NonZeroU32 = NonZeroU32::new(1000).unwrap();

/// An answer the join cannot match to what it asked.
const MISMATCH: Outcome = Outcome::Unread(Unread::Inconsistent);

/// A list the ask did not read: empty, possibly cut.
fn not_read<T>() -> Took<T> {
    Took {
        items: Vec::new(),
        more: true,
    }
}

/// When a change was merged or closed; None while it is open.
fn ended(change: &ChangeDigest) -> Option<At> {
    match change.state {
        ChangeState::Open => None,
        ChangeState::Merged { at, .. } | ChangeState::Closed { at } => Some(at),
    }
}

/// Histories read apart, as one, oldest first: of two events in one
/// second an addition sorts before a removal, and events of one list keep
/// their order; one with no time sorts first. `more` when any list held
/// more.
fn merged(each: Vec<Took<TicketEvent>>) -> Took<TicketEvent> {
    let more = each.iter().any(|t| t.more);
    let mut items: Vec<TicketEvent> = each.into_iter().flat_map(|t| t.items).collect();
    let removal = |e: &TicketEvent| {
        matches!(
            e.what.kind(),
            HappenedKind::Unmarked | HappenedKind::BlockerRemoved | HappenedKind::Unassigned
        )
    };
    items.sort_by_key(|e| (e.at, removal(e)));
    Took { items, more }
}

/// The changes in the board's order, as much as `window` takes.
fn ordered(changes: Vec<ChangeDigest>, window: Window) -> Got<Took<ChangeDigest>> {
    let (mut open, mut rest): (Vec<_>, Vec<_>) =
        changes.into_iter().partition(|c| ended(c).is_none());
    rest.sort_by_key(|c| std::cmp::Reverse((ended(c), c.no)));
    let n = match window {
        Window::Whole(_) => usize::MAX,
        Window::Last(n) => usize::try_from(n.get()).unwrap_or(usize::MAX),
    };
    if open.len() > n {
        return Err(Outcome::Unread(Unread::Cut));
    }
    let room = n - open.len();
    let more = rest.len() > room;
    rest.truncate(room);
    open.append(&mut rest);
    Ok(Took { items: open, more })
}

impl Join {
    fn items(&mut self, keys: Vec<TicketKey>, ask: &BoardAsk) -> Whole<(TicketKey, Got<WorkItem>)> {
        Whole::new(
            keys.into_iter()
                .map(|key| {
                    let item = self.work_item(&key, ask);
                    (key, item)
                })
                .collect(),
        )
    }

    /// The blueprints asked, in the order asked. One left out of the answer
    /// fails them all.
    fn blueprints_of(&mut self, nos: &[BlueprintNo], depth: Depth) -> Got<Vec<Blueprint>> {
        if nos.is_empty() {
            return Ok(Vec::new());
        }
        let mut answers = self.blueprints.several(nos, depth);
        nos.iter()
            .map(|no| {
                let at = answers.iter().position(|(n, _)| n == no).ok_or(MISMATCH)?;
                answers.swap_remove(at).1
            })
            .collect()
    }

    /// The changes that close these blueprints, in the board's order.
    fn changes_closing(
        &mut self,
        nos: Vec<BlueprintNo>,
        states: ChangeStates,
        window: Window,
    ) -> Got<Took<ChangeDigest>> {
        if nos.is_empty() {
            return Ok(Took {
                items: Vec::new(),
                more: false,
            });
        }
        let bound = match window {
            Window::Whole(bound) => bound,
            Window::Last(_) => ITEM_MAX.into(),
        };
        let filter = ChangeFilter {
            states,
            closing: nos,
            ..ChangeFilter::default()
        };
        ordered(self.code_host.changes(&filter, bound)?.into_items(), window)
    }

    fn digest(&mut self, key: &TicketKey) -> Got<TicketDigest> {
        self.tickets
            .several(std::slice::from_ref(key))
            .into_iter()
            .find(|(k, _)| k == key)
            .ok_or(MISMATCH)?
            .1
    }
}

impl Reports for Join {
    fn take_reports(&mut self) -> Vec<Report> {
        let mut all: Vec<Report> = Vec::new();
        let taken = [
            self.tickets.take_reports(),
            self.blueprints.take_reports(),
            self.code_host.take_reports(),
        ];
        for report in taken.into_iter().flatten() {
            match all
                .iter_mut()
                .find(|r| r.port == report.port && r.outcome == report.outcome)
            {
                Some(same) => same.calls = same.calls.saturating_add(report.calls),
                None => all.push(report),
            }
        }
        all
    }
}

impl Board for Join {
    /// The filter's open tickets alone, whatever its `open` says, in list
    /// order. The control ticket is read with `Depth::All`.
    fn open_work(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork> {
        let filter = Filter {
            open: Some(true),
            ..ask.filter.clone()
        };
        let listed = self.tickets.list(&filter, bound)?;
        let control = ask
            .control
            .as_ref()
            .map(|key| self.tickets.ticket(key, Depth::All, None));
        let keys = listed.into_items().into_iter().map(|d| d.key).collect();
        Ok(OpenWork {
            items: self.items(keys, ask),
            control,
        })
    }

    /// The ticket and its history in one read. The notes are read as
    /// `Depth::notes` gives, none at `Depth::Cut { notes: 0 }`; `Depth::All`
    /// takes at most `ALL_MAX` (1000), past it `Unread(Cut)`.
    /// `Depth::Cut`'s cut of each note's text is the site's own read's;
    /// this join does not apply it.
    fn work_item(&mut self, key: &TicketKey, ask: &BoardAsk) -> Got<WorkItem> {
        // Each kind's own window replaces the history, so the ticket's read
        // takes the least: a whole one could fail on a busy kind's volume.
        let history = match ask.history_by_kind.is_empty() {
            true => ask.history,
            false => Some(Window::Last(NonZeroU32::MIN)),
        };
        let mut ticket = self.tickets.ticket(key, ask.depth, history)?;
        if !ask.history_by_kind.is_empty() {
            let mut each = Vec::with_capacity(ask.history_by_kind.len());
            for (kind, window) in &ask.history_by_kind {
                each.push(
                    self.tickets
                        .history(key, *window, Some(std::slice::from_ref(kind)))?,
                );
            }
            ticket.history = merged(each);
        }
        let notes = match ask.depth.notes() {
            Some(window) => self.tickets.notes(key, window)?,
            None => not_read(),
        };
        let nos = self.links.blueprints_of(key)?;
        let blueprints = self.blueprints_of(&nos, ask.depth)?;
        let changes = match ask.changes {
            Some((states, window)) => self.changes_closing(nos, states, window)?,
            None => not_read(),
        };
        Ok(WorkItem {
            ticket,
            notes,
            blueprints,
            changes,
        })
    }

    /// The ask's filter is not applied: every ticket closed since `since`.
    fn closed_work(
        &mut self,
        ask: &BoardAsk,
        since: At,
        bound: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>> {
        let closed = self.tickets.closed_since(since, bound)?;
        let keys = closed
            .into_items()
            .into_iter()
            .map(|c| c.digest.key)
            .collect();
        Ok(self.items(keys, ask))
    }

    /// The ask's filter is not applied: every ticket of the target.
    fn target_work(
        &mut self,
        ask: &BoardAsk,
        target: &TargetId,
        bound: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>> {
        let members = self.tickets.members(target, bound)?;
        let keys = members.into_items().into_iter().map(|d| d.key).collect();
        Ok(self.items(keys, ask))
    }

    /// A blueprint a change closes is read without its comments; one of
    /// another project, or one not found, is left out of `closing`, and one
    /// of another project out of `siblings`.
    fn open_changes(&mut self, ask: &ChangeAsk, bound: Bound) -> Got<Whole<ChangeItem>> {
        let changes = self.code_host.changes(&ask.filter, bound)?.into_items();
        let mut items = Vec::with_capacity(changes.len());
        for change in changes {
            let here: Vec<BlueprintNo> = change
                .closes
                .iter()
                .filter_map(|r| match r {
                    Ref::Here(no) => Some(*no),
                    Ref::Elsewhere { .. } => None,
                })
                .collect();
            let files = match ask.files {
                Some(each) => Some(self.code_host.changed_files(change.no, each)?),
                None => None,
            };
            let mut closing = Vec::new();
            if ask.closing {
                for no in &here {
                    let blueprint = match self.blueprints.blueprint(*no, Depth::Cut { notes: 0 }) {
                        Ok(blueprint) => blueprint,
                        Err(Outcome::NotFound) => continue,
                        Err(other) => return Err(other),
                    };
                    let ticket = match self.links.ticket_of(*no)? {
                        Some(key) => Some(self.digest(&key)?),
                        None => None,
                    };
                    closing.push((blueprint, ticket));
                }
            }
            let mut siblings = Vec::new();
            if let (Some(each), false) = (ask.siblings, here.is_empty()) {
                let filter = ChangeFilter {
                    closing: here,
                    ..ChangeFilter::default()
                };
                siblings = self.code_host.changes(&filter, each)?.into_items();
                siblings.retain(|c| c.no != change.no);
            }
            let activity = match ask.activity {
                Some(window) => Some(self.code_host.activity(change.no, window)?),
                None => None,
            };
            items.push(ChangeItem {
                change,
                files,
                closing,
                siblings,
                activity,
            });
        }
        Ok(Whole::new(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(no: u64, state: ChangeState) -> ChangeDigest {
        ChangeDigest {
            no: crate::ChangeNo::new(no).unwrap(),
            title: String::new(),
            state,
            head: crate::Branch::new("work").unwrap(),
            head_commit: crate::CommitId::new("abc1234").unwrap(),
            base: crate::Branch::new("main").unwrap(),
            base_commit: crate::CommitId::new("abc1230").unwrap(),
            author: None,
            author_relation: None,
            from_fork: false,
            draft: false,
            merge_state: crate::Mergeable::Unknown,
            marks: vec![],
            closes: vec![],
            says_closes: vec![],
            size: None,
            body: None,
            created: At::new(0),
            updated: At::new(0),
            edited: None,
            editor: None,
            link: crate::WebLink::new("https://example.test/c").unwrap(),
        }
    }

    fn closed(no: u64, at: i64) -> ChangeDigest {
        change(no, ChangeState::Closed { at: At::new(at) })
    }

    fn nos(took: &Took<ChangeDigest>) -> Vec<u64> {
        took.items.iter().map(|c| c.no.get()).collect()
    }

    #[test]
    fn changes_come_open_first_then_newest_ended_first() {
        let all = vec![
            closed(1, 10),
            change(2, ChangeState::Open),
            closed(3, 30),
            closed(4, 30),
            closed(5, 20),
        ];
        let whole = ordered(all.clone(), Window::Whole(crate::ALL_MAX)).unwrap();
        assert_eq!(nos(&whole), [2, 4, 3, 5, 1]);
        assert!(!whole.more);
        let three = ordered(all, Window::Last(NonZeroU32::new(3).unwrap())).unwrap();
        assert_eq!(nos(&three), [2, 4, 3]);
        assert!(three.more);
    }
}
