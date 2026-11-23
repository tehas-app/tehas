//! The joined reads across tickets, blueprints and the code host: `Board`,
//! what it is asked, `BoardAsk` and `ChangeAsk`, what it answers,
//! `WorkItem`, `OpenWork` and `ChangeItem`, and `Join`, the default join
//! over three owned handles and `Links`.

use std::num::NonZeroU32;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    At, Blueprint, BlueprintNo, Blueprints, Bound, ChangeDigest, ChangeEvent, ChangeFilter,
    ChangeState, ChangeStates, ChangedFile, CodeHost, Comment, Depth, Filter, Got, Links, Outcome,
    Ref, Report, Reports, TargetId, Ticket, TicketDigest, TicketKey, Tickets, Took, Unread, Whole,
    Window,
};

/// What a joined read asks for, so a cheap poll asks less.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoardAsk {
    /// How much of each ticket, its notes and its blueprints to read.
    pub depth: Depth,
    /// Each ticket's history. A poll that must not miss an approval asks
    /// `Window::Whole`; a queue read may ask `Window::Last` with
    /// `Depth::Last`, and reads a `more` as possibly cut.
    pub history: Option<Window>,
    /// The changes that close each item's blueprints.
    pub changes: bool,
    /// Which tickets.
    pub filter: Filter,
    /// The control ticket, read whole with its history beside the rest.
    pub control: Option<TicketKey>,
}

/// One ticket with its notes, its blueprints and their changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkItem {
    /// The ticket.
    pub ticket: Ticket,
    /// Notes on the ticket itself; empty where ticket and blueprint are one.
    /// As deep as `BoardAsk.depth`, with `more`.
    pub notes: Took<Comment>,
    /// The blueprints linked to it.
    pub blueprints: Vec<Blueprint>,
    /// The changes that close those blueprints, open first.
    pub changes: Vec<ChangeDigest>,
}

impl WorkItem {
    /// The comments a ticket marker is read from: the notes, or, where
    /// ticket and blueprint are one item, that blueprint's comments. A
    /// ticket named by a number is an item of a code host's own tracker, so
    /// the blueprint of the same number is the ticket itself.
    pub fn ticket_comments(&self) -> &Took<Comment> {
        if let TicketKey::Number(n) = self.ticket.digest.key {
            if let Some(same) = self.blueprints.iter().find(|b| b.no.get() == n.get()) {
                return &same.comments;
            }
        }
        &self.notes
    }
}

/// The list of open tickets is whole; each item has its own answer, so one
/// ticket past a bound, or unread, is `Unread` for that item alone and
/// fails closed alone. The others are still answered.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpenWork {
    /// Each open ticket, with its own answer.
    pub items: Whole<(TicketKey, Got<WorkItem>)>,
    /// The control ticket, if it was asked for.
    pub control: Option<Ticket>,
}

/// What a read centred on changes asks for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChangeAsk {
    /// Which changes.
    pub filter: ChangeFilter,
    /// Each change's files, whole.
    pub files: bool,
    /// The blueprints each closes, with their text and marks, and their
    /// tickets.
    pub closing: bool,
    /// The other open changes that close the same blueprints.
    pub siblings: bool,
    /// Each change's events: a stale read asks `Window::Last(50)`.
    pub activity: Option<Window>,
}

/// One change with what the ask named.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChangeItem {
    /// The change.
    pub change: ChangeDigest,
    /// Its files, if asked.
    pub files: Option<Whole<ChangedFile>>,
    /// The blueprints of this project it closes, each with its linked
    /// ticket, if asked.
    pub closing: Vec<(Blueprint, Option<TicketDigest>)>,
    /// The other open changes that close the same blueprints, if asked.
    pub siblings: Vec<ChangeDigest>,
    /// Its events, if asked.
    pub activity: Option<Took<ChangeEvent>>,
}

/// The joined reads. A site that serves all three ports answers them with
/// one question and owns its cost.
pub trait Board: Reports {
    /// The open tickets the filter takes, each with its own answer, and the
    /// control ticket.
    fn open_work(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork>;
    /// One ticket whole, as the ask names.
    fn work_item(&mut self, key: &TicketKey, ask: &BoardAsk) -> Got<WorkItem>;
    /// The tickets closed since a time. Per item, as `OpenWork.items`.
    fn closed_work(
        &mut self,
        ask: &BoardAsk,
        since: At,
        bound: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>>;
    /// The tickets of a target. Per item, as `OpenWork.items`.
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

/// The most notes of a ticket, or changes of its blueprints, the join reads
/// for one item. Past it, that item is `Unread(Cut)`.
const ITEM_MAX: NonZeroU32 = NonZeroU32::new(1000).unwrap();

/// An answer the join cannot match to what it asked.
const MISMATCH: Outcome = Outcome::Unread(Unread::Inconsistent);

/// The notes as deep as `depth`: the newest n, with `more`, or every one.
fn deep(mut notes: Vec<Comment>, depth: Depth) -> Took<Comment> {
    let n = match depth {
        Depth::Cut { notes } => usize::from(notes),
        Depth::Last { notes } => usize::try_from(notes.get()).unwrap_or(usize::MAX),
        Depth::All => usize::MAX,
    };
    if notes.len() > n {
        let newest = notes.split_off(notes.len() - n);
        Took {
            items: newest,
            more: true,
        }
    } else {
        Took {
            items: notes,
            more: false,
        }
    }
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
    /// The filter's open tickets alone, whatever its `open` says.
    fn open_work(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork> {
        let filter = Filter {
            open: Some(true),
            ..ask.filter.clone()
        };
        let listed = self.tickets.list(&filter, bound)?;
        let control = match &ask.control {
            Some(key) => Some(self.tickets.ticket(key, Depth::All)?),
            None => None,
        };
        let keys = listed.into_items().into_iter().map(|d| d.key).collect();
        Ok(OpenWork {
            items: self.items(keys, ask),
            control,
        })
    }

    fn work_item(&mut self, key: &TicketKey, ask: &BoardAsk) -> Got<WorkItem> {
        let mut ticket = self.tickets.ticket(key, ask.depth)?;
        if let Some(window) = ask.history {
            ticket.history = self.tickets.history(key, window)?;
        }
        let notes = deep(
            self.tickets.notes(key, ITEM_MAX.into())?.into_items(),
            ask.depth,
        );
        let nos = self.links.blueprints_of(key)?;
        let blueprints = self.blueprints_of(&nos, ask.depth)?;
        let mut changes = Vec::new();
        if ask.changes && !nos.is_empty() {
            let filter = ChangeFilter {
                states: ChangeStates::All,
                closing: nos,
                ..ChangeFilter::default()
            };
            changes = self
                .code_host
                .changes(&filter, ITEM_MAX.into())?
                .into_items();
            changes.sort_by_key(|c| !matches!(c.state, ChangeState::Open));
        }
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
    /// another project is left out of `closing` and `siblings`.
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
            let files = if ask.files {
                Some(self.code_host.changed_files(change.no, bound)?)
            } else {
                None
            };
            let mut closing = Vec::new();
            if ask.closing {
                for no in &here {
                    let blueprint = self.blueprints.blueprint(*no, Depth::Cut { notes: 0 })?;
                    let ticket = match self.links.ticket_of(*no)? {
                        Some(key) => Some(self.digest(&key)?),
                        None => None,
                    };
                    closing.push((blueprint, ticket));
                }
            }
            let mut siblings = Vec::new();
            if ask.siblings && !here.is_empty() {
                let filter = ChangeFilter {
                    closing: here,
                    ..ChangeFilter::default()
                };
                siblings = self.code_host.changes(&filter, bound)?.into_items();
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

    fn comments(n: usize) -> Vec<Comment> {
        (0..n)
            .map(|i| Comment {
                id: crate::CommentId::new(i.to_string()),
                author: None,
                editor: None,
                created: At::new(i as i64),
                edited: None,
                body: String::new(),
                markers: vec![],
                revision: crate::Revision::new("r"),
                link: crate::WebLink::new("https://example.test/c").unwrap(),
            })
            .collect()
    }

    #[test]
    fn notes_as_deep_as_asked_keep_the_newest() {
        let three = NonZeroU32::new(3).unwrap();
        let took = deep(comments(5), Depth::Last { notes: three });
        assert!(took.more);
        let ids: Vec<&str> = took.items.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["2", "3", "4"]);
        let took = deep(comments(2), Depth::Cut { notes: 3 });
        assert!(!took.more);
        assert_eq!(took.items.len(), 2);
        let took = deep(comments(5), Depth::Cut { notes: 0 });
        assert!(took.more);
        assert!(took.items.is_empty());
        let took = deep(comments(5), Depth::All);
        assert!(!took.more);
        assert_eq!(took.items.len(), 5);
    }
}
