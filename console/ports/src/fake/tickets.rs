//! The fake's tickets port, over the world.

use std::num::{NonZeroU32, NonZeroU64};

use super::faulty::{Op, TicketsOp};
use super::site::{
    comments_of, digest, edit_markers, first, known_mark, list_order, matches, revision, text_of,
    whole, whole_kept, window, writable, Ctx, Handle,
};
use super::state::{ItemRec, Layout, State};
use crate::{
    AccountId, Applied, At, Blocker, BlockerState, Bound, ChangeDigest, ChangeState, ChangeToken, CloseReason,
    Closed, ClosedTicket, Closer, Comment, CommentId, Cursor, Depth, Did, Edit, Filed, Filter, Got,
    Happened, HappenedKind, Item, Kind, Mark, MarkHeld, MarkSpec, Marker, MarkerEdit, MarkerKey,
    MarkerKind, NewTicket, Outcome, Part, Pool, Posted, Priority, Ref, Refusal, Step, StepPart,
    Target, TargetEdit, TargetId, Ticket, TicketChange, TicketDelta, TicketDigest, TicketEvent,
    TicketKey, TicketWrites, Tickets, Took, Unprepared, WebLink, Whole, Window, WithCloser, Within,
    Write,
};

fn at_ticket(s: &State, key: &TicketKey) -> Got<usize> {
    s.ticket_at(key).ok_or(Outcome::NotFound)
}

fn open_of(s: &State, key: &TicketKey) -> bool {
    s.ticket_at(key).is_some_and(|at| s.items[at].open)
}

fn ticket(s: &State, at: usize, depth: Depth, history: Option<Window>) -> Got<Ticket> {
    let item = &s.items[at];
    let digest = digest(s, item).ok_or(Outcome::NotFound)?;
    let history = match history.or(depth.history()) {
        Some(w) => window(&item.history, w)?,
        None => Took {
            items: Vec::new(),
            more: true,
        },
    };
    let text = match depth {
        Depth::Cut { .. } => super::site::cut(&item.text),
        _ => item.text.clone(),
    };
    let parts = item
        .parts
        .iter()
        .map(|k| Part {
            key: Ref::Here(k.clone()),
            open: open_of(s, k),
        })
        .collect();
    let blockers = item.blockers.iter().map(|on| blocker(s, on)).collect();
    Ok(Ticket {
        digest,
        text,
        closed: item.closed,
        closer: item.closer.clone(),
        parts: whole_kept(parts)?.into_items(),
        blockers: whole_kept(blockers)?.into_items(),
        assignees: item.assignees.clone(),
        markers: item.markers.clone(),
        mentioned_by: item.mentioned_by.iter().map(|n| Ref::Here(*n)).collect(),
        history,
    })
}

fn blocker(s: &State, on: &Item) -> Blocker {
    let state = match on {
        Item::Ticket(k) if open_of(s, k) => BlockerState::Open,
        Item::Ticket(_) => BlockerState::Closed,
        Item::Change(no) => match s.change_at(*no).map(|at| &s.changes[at].state) {
            Some(ChangeState::Open) => BlockerState::Open,
            Some(ChangeState::Merged { .. }) => BlockerState::Merged,
            _ => BlockerState::Closed,
        },
    };
    Blocker {
        on: Ref::Here(on.clone()),
        state,
    }
}

/// Every ticket's digest, in list order.
fn listed(s: &State, keep: impl Fn(&super::state::ItemRec) -> bool) -> Vec<TicketDigest> {
    let mut all: Vec<TicketDigest> = s
        .items
        .iter()
        .filter(|i| keep(i))
        .filter_map(|i| digest(s, i))
        .collect();
    list_order(&mut all);
    all
}

/// Does the filter take the item?
fn takes(filter: &Filter, i: &super::state::ItemRec) -> bool {
    filter.open.is_none_or(|o| i.open == o)
        && filter.carrying.iter().all(|m| i.marks.contains(m))
        && (filter.carrying_any.is_empty()
            || filter.carrying_any.iter().any(|m| i.marks.contains(m)))
        && filter
            .kind
            .as_ref()
            .is_none_or(|k| i.kind.as_ref() == Some(k))
        && filter
            .target
            .as_ref()
            .is_none_or(|t| i.target.as_ref() == Some(t))
}

/// Newest `updated` first; ties in reverse list order, so the order is the
/// same on every read.
fn newest_first<T>(all: &mut [T], digest: impl Fn(&T) -> &TicketDigest) {
    all.reverse();
    all.sort_by_key(|x| std::cmp::Reverse(digest(x).updated));
}

/// The closed tickets whose close time `when` takes, in list order. A
/// ticket's merged changes are those that close it where ticket and
/// blueprint are one item; none in a split world.
fn closed_tickets(s: &State, when: impl Fn(At) -> bool) -> Vec<ClosedTicket> {
    listed(s, |i| !i.open && i.closed.is_some_and(&when))
        .into_iter()
        .filter_map(|d| {
            let at = s.ticket_at(&d.key)?;
            let item = &s.items[at];
            Some(ClosedTicket {
                closed: item.closed?,
                closer: item.closer.clone(),
                part_count: item.parts.len() as u32,
                merged: merged_closing(s, item),
                digest: d,
            })
        })
        .collect()
}

/// The merged changes that close a ticket, where ticket and blueprint are
/// one item: elsewhere the tickets are not on the code host.
fn merged_closing(s: &State, item: &ItemRec) -> Vec<ChangeDigest> {
    match (s.layout, item.blueprint) {
        (Layout::SameItem, Some(no)) => s
            .changes
            .iter()
            .filter(|c| c.closes.contains(&no) && matches!(c.state, ChangeState::Merged { .. }))
            .map(|c| super::code_host::change_digest(s, c, false))
            .collect(),
        _ => Vec::new(),
    }
}

/// The notes of a ticket: its own where ticket and blueprint are apart,
/// none where they are one item.
fn notes_of(s: &State, at: usize) -> Vec<Comment> {
    match s.layout {
        Layout::SameItem => Vec::new(),
        Layout::Split => comments_of(s, &s.items[at].comments, false),
    }
}

/// Does `from` wait on `to`, through any chain of blockers?
fn waits_on(s: &State, from: &TicketKey, to: &TicketKey) -> bool {
    let mut todo = vec![from.clone()];
    let mut seen: Vec<TicketKey> = Vec::new();
    while let Some(k) = todo.pop() {
        if k == *to {
            return true;
        }
        if seen.contains(&k) {
            continue;
        }
        if let Some(at) = s.ticket_at(&k) {
            for b in &s.items[at].blockers {
                if let Item::Ticket(next) = b {
                    todo.push(next.clone());
                }
            }
        }
        seen.push(k);
    }
    false
}

/// Is `of` a part of `parent`, at any depth?
fn part_of(s: &State, parent: &TicketKey, of: &TicketKey) -> bool {
    let mut todo = vec![parent.clone()];
    let mut seen: Vec<TicketKey> = Vec::new();
    while let Some(k) = todo.pop() {
        if k == *of {
            return true;
        }
        if seen.contains(&k) {
            continue;
        }
        if let Some(at) = s.ticket_at(&k) {
            todo.extend(s.items[at].parts.iter().cloned());
        }
        seen.push(k);
    }
    false
}

fn event(s: &mut State, at: usize, ctx: &Ctx, what: Happened) {
    let by = Some(ctx.me(s));
    let mark_id = match &what {
        Happened::Marked(m) | Happened::Unmarked(m) => s.mark_id(m),
        _ => None,
    };
    s.items[at].history.push(TicketEvent {
        what,
        by,
        at: Some(ctx.now),
        mark_id,
    });
    s.touch(at, ctx.now);
}

fn target_counts(s: &State, t: &Target) -> Target {
    let mut t = t.clone();
    let of = s
        .items
        .iter()
        .filter(|i| i.ticket.is_some() && i.target.as_ref() == Some(&t.id));
    t.open_count = 0;
    t.closed_count = 0;
    for i in of {
        if i.open {
            t.open_count += 1;
        } else {
            t.closed_count += 1;
        }
    }
    t
}

fn seq_of(c: Option<&Cursor>) -> u64 {
    c.and_then(|c| c.as_str().strip_prefix('s'))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

pub(crate) fn token(s: &State) -> ChangeToken {
    ChangeToken::new(format!("v{}", s.seq))
}

pub(crate) fn since(c: Option<&Cursor>) -> u64 {
    seq_of(c)
}

/// A delta's page: the items and the notes, each with the sequence of its
/// change, oldest change first, the first `bound` of them, the cursor after
/// the last one taken, and whether more wait past it.
pub(crate) fn page<A, B>(
    s: &State,
    mut items: Vec<(u64, A)>,
    mut notes: Vec<(u64, B)>,
    bound: Bound,
) -> (Vec<A>, Vec<B>, Cursor, bool) {
    items.sort_by_key(|(seq, _)| *seq);
    notes.sort_by_key(|(seq, _)| *seq);
    let mut seqs: Vec<u64> = items.iter().map(|(q, _)| *q).collect();
    seqs.extend(notes.iter().map(|(q, _)| *q));
    seqs.sort_unstable();
    let n = bound.get() as usize;
    let (last, more) = if seqs.len() > n {
        (seqs[n - 1], true)
    } else {
        (s.seq, false)
    };
    let items = items
        .into_iter()
        .filter(|(q, _)| *q <= last)
        .map(|(_, x)| x)
        .collect();
    let notes = notes
        .into_iter()
        .filter(|(q, _)| *q <= last)
        .map(|(_, x)| x)
        .collect();
    (items, notes, Cursor::new(format!("s{last}")), more)
}

fn has_marker(markers: &[Marker], kind: &MarkerKind, key: &MarkerKey) -> bool {
    markers
        .iter()
        .any(|m| m.kind() == *kind && m.key().as_ref() == Some(key))
}

impl Tickets for Handle {
    fn ticket(&mut self, key: &TicketKey, depth: Depth, history: Option<Window>) -> Got<Ticket> {
        self.run(Pool::Calls, |s, _| {
            ticket(s, at_ticket(s, key)?, depth, history)
        })
    }

    /// `Refused(Unsupported)` in a split world: its tickets and changes
    /// share no number space.
    fn what_is(&mut self, number: NonZeroU64) -> Got<Item> {
        self.run(Pool::Calls, |s, _| what_is(s, number))
    }

    fn what_are(&mut self, numbers: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)> {
        self.each(numbers, |s, _, n| what_is(s, *n))
    }

    fn list(&mut self, filter: &Filter, bound: Bound) -> Got<Whole<TicketDigest>> {
        self.run(Pool::Calls, |s, _| {
            whole(listed(s, |i| takes(filter, i)), bound)
        })
    }

    fn recent(&mut self, filter: &Filter, n: NonZeroU32) -> Got<Took<TicketDigest>> {
        self.run(Pool::Calls, |s, _| {
            let mut all = listed(s, |i| takes(filter, i));
            newest_first(&mut all, |d| d);
            Ok(first(all, n.get()))
        })
    }

    fn recently_closed(&mut self, n: NonZeroU32) -> Got<Took<ClosedTicket>> {
        self.run(Pool::Calls, |s, _| {
            let mut all = closed_tickets(s, |_| true);
            newest_first(&mut all, |c| &c.digest);
            Ok(first(all, n.get()))
        })
    }

    fn count(&mut self, filter: &Filter) -> Got<u64> {
        self.run(Pool::Calls, |s, _| {
            if !s.count_any_on && !filter.carrying_any.is_empty() {
                return Err(Outcome::Refused(Refusal::Unsupported));
            }
            Ok(listed(s, |i| takes(filter, i)).len() as u64)
        })
    }

    fn several(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<TicketDigest>)> {
        self.each(keys, |s, _, k| {
            let at = at_ticket(s, k)?;
            digest(s, &s.items[at]).ok_or(Outcome::NotFound)
        })
    }

    fn several_with_closers(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<WithCloser>)> {
        self.each(keys, |s, _, k| {
            let at = at_ticket(s, k)?;
            let item = &s.items[at];
            let digest = digest(s, item).ok_or(Outcome::NotFound)?;
            let mut with = WithCloser::new(digest);
            if !item.open {
                with.closer = item.closer.clone();
                with.merged = merged_closing(s, item);
            }
            Ok(with)
        })
    }

    fn history(
        &mut self,
        key: &TicketKey,
        window_of: Window,
        only: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        self.run(Pool::Calls, |s, _| {
            let at = at_ticket(s, key)?;
            let events: Vec<TicketEvent> = s.items[at]
                .history
                .iter()
                .filter(|e| only.is_none_or(|kinds| kinds.contains(&e.what.kind())))
                .cloned()
                .collect();
            window(&events, window_of)
        })
    }

    fn notes(&mut self, key: &TicketKey, w: Window) -> Got<Took<Comment>> {
        self.run(Pool::Calls, |s, _| {
            let at = at_ticket(s, key)?;
            window(&notes_of(s, at), w)
        })
    }

    fn note_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>> {
        self.run(Pool::Calls, |s, _| {
            let at = s.comment_at(id).ok_or(Outcome::NotFound)?;
            whole(s.comments[at].edits.clone(), bound)
        })
    }

    fn blockers(&mut self, key: &TicketKey) -> Got<Whole<Blocker>> {
        self.run(Pool::Calls, |s, _| {
            let at = at_ticket(s, key)?;
            whole_kept(s.items[at].blockers.iter().map(|b| blocker(s, b)).collect())
        })
    }

    fn parts(&mut self, key: &TicketKey) -> Got<Whole<Part>> {
        self.run(Pool::Calls, |s, _| {
            let at = at_ticket(s, key)?;
            whole_kept(
                s.items[at]
                    .parts
                    .iter()
                    .map(|k| Part {
                        key: Ref::Here(k.clone()),
                        open: open_of(s, k),
                    })
                    .collect(),
            )
        })
    }

    fn closed_since(&mut self, since_at: At, bound: Bound) -> Got<Whole<ClosedTicket>> {
        self.run(Pool::Calls, |s, _| {
            whole(closed_tickets(s, |c| c >= since_at), bound)
        })
    }

    /// The fake's order of relevance is list order. Words match ASCII case
    /// folded; no word is a qualifier.
    fn search(
        &mut self,
        words: &str,
        within: Within,
        open: Option<bool>,
        n: NonZeroU32,
    ) -> Got<Took<TicketDigest>> {
        self.run(Pool::Search, |s, _| {
            let hits = listed(s, |i| {
                open.is_none_or(|o| i.open == o)
                    && match within {
                        Within::Titles => matches(words, &i.title),
                        Within::Everything => matches(words, &format!("{}\n{}", i.title, i.text)),
                    }
            });
            Ok(first(hits, n.get()))
        })
    }

    fn filed_by(&mut self, by: &AccountId, since_at: At, bound: Bound) -> Got<Whole<Filed>> {
        self.run(Pool::Calls, |s, _| {
            let mine = listed(s, |i| {
                i.created >= since_at && i.author.as_ref().and_then(|a| a.id.as_ref()) == Some(by)
            });
            let all = mine
                .into_iter()
                .filter_map(|d| {
                    let at = s.ticket_at(&d.key)?;
                    Some(Filed {
                        first_line: s.items[at].text.lines().next().unwrap_or("").to_string(),
                        title: d.title,
                        link: d.link,
                        key: d.key,
                    })
                })
                .collect();
            whole(all, bound)
        })
    }

    fn find_marked(
        &mut self,
        mark: &Mark,
        kind: &MarkerKind,
        key: &MarkerKey,
        open: Option<bool>,
    ) -> Got<Whole<TicketKey>> {
        self.run(Pool::Calls, |s, _| {
            let found = listed(s, |i| {
                open.is_none_or(|o| i.open == o)
                    && i.marks.contains(mark)
                    && (has_marker(&i.markers, kind, key)
                        || comments_of(s, &i.comments, false)
                            .iter()
                            .any(|c| has_marker(&c.markers, kind, key)))
            });
            whole_kept(found.into_iter().map(|d| d.key).collect())
        })
    }

    /// The marks any item carries, the factory's once prepared, and each
    /// project mark ensured, by word.
    fn marks(&mut self, bound: Bound) -> Got<Whole<MarkHeld>> {
        self.run(Pool::Calls, |s, _| {
            let mut all: Vec<Mark> = s.items.iter().flat_map(|i| i.marks.clone()).collect();
            all.extend(s.changes.iter().flat_map(|c| c.marks.clone()));
            if s.prepared {
                all.extend(crate::laws::marks::factory_marks());
            }
            all.extend(s.prepared_marks.iter().cloned());
            all.extend(
                s.project_marks
                    .iter()
                    .map(|m| Mark::Project(m.name.clone())),
            );
            let mut named: Vec<MarkHeld> = Vec::new();
            for m in all {
                if let Some(id) = s.mark_id(&m) {
                    if !named.iter().any(|n| n.mark == m) {
                        named.push(MarkHeld::new(m, id));
                    }
                }
            }
            named.sort_by(|a, b| a.id.cmp(&b.id));
            whole(named, bound)
        })
    }

    fn kinds(&mut self) -> Got<Whole<Kind>> {
        self.run(Pool::Calls, |s, _| {
            whole_kept(
                s.kinds
                    .iter()
                    .filter(|k| **k != Kind::Unknown)
                    .cloned()
                    .collect(),
            )
        })
    }

    fn targets(&mut self, bound: Bound) -> Got<Whole<Target>> {
        self.run(Pool::Calls, |s, _| {
            whole(
                s.targets.iter().map(|t| target_counts(s, t)).collect(),
                bound,
            )
        })
    }

    fn members(&mut self, target: &TargetId, bound: Bound) -> Got<Whole<TicketDigest>> {
        self.run(Pool::Calls, |s, _| {
            if !s.targets.iter().any(|t| t.id == *target) {
                return Err(Outcome::NotFound);
            }
            whole(listed(s, |i| i.target.as_ref() == Some(target)), bound)
        })
    }

    fn changed(&mut self, since_token: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        self.run(Pool::Calls, |s, _| {
            let now = token(s);
            Ok((since_token != Some(&now), now))
        })
    }

    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<TicketDelta> {
        self.run(Pool::Calls, |s, _| {
            let after = since(from);
            let mut tickets = Vec::new();
            let mut notes = Vec::new();
            for (at, item) in s.items.iter().enumerate() {
                let Some(d) = digest(s, item) else { continue };
                if item.seq > after {
                    let mut change = TicketChange::new(d.clone());
                    change.text = item.text.clone();
                    change.closed = item.closed;
                    tickets.push((item.seq, change));
                }
                for c in notes_of(s, at) {
                    let seq = s.comment_at(&c.id).map_or(0, |i| s.comments[i].seq);
                    if seq > after {
                        notes.push((seq, (d.key.clone(), c)));
                    }
                }
            }
            let (tickets, notes, next, more) = page(s, tickets, notes, bound);
            let mut delta = TicketDelta::new(next);
            delta.tickets = tickets;
            delta.notes = notes;
            delta.more = more;
            Ok(delta)
        })
    }

    fn new_ticket_link(&self, title: &str, text: &str) -> WebLink {
        super::site::web(&format!(
            "{}/t/new?title={}&text={}",
            self.project().as_str(),
            super::site::escaped(title),
            super::site::escaped(text)
        ))
    }
}

fn what_is(s: &State, number: NonZeroU64) -> Got<Item> {
    if s.layout == Layout::Split {
        return Err(Outcome::Refused(Refusal::Unsupported));
    }
    let key = TicketKey::Number(number);
    if s.ticket_at(&key).is_some() {
        return Ok(Item::Ticket(key));
    }
    let no = crate::ChangeNo::from(number);
    if s.change_at(no).is_some() {
        return Ok(Item::Change(no));
    }
    Err(Outcome::NotFound)
}

/// Mark a ticket: `Refused(Invalid)` on a closed ticket or an unknown mark;
/// a mark already on is done with no event.
fn mark(s: &mut State, ctx: &Ctx, key: &TicketKey, m: &Mark) -> Did {
    known_mark(m)?;
    let at = at_ticket(s, key)?;
    if !s.items[at].open {
        return Err(Outcome::Refused(Refusal::Invalid));
    }
    if s.items[at].put_mark(m) {
        event(s, at, ctx, Happened::Marked(m.clone()));
    }
    ctx.wrote(
        s,
        Write::Mark {
            ticket: key.clone(),
            mark: m.clone(),
        },
    );
    Ok(())
}

/// Take a mark off: one not on is done.
fn unmark(s: &mut State, ctx: &Ctx, key: &TicketKey, m: &Mark) -> Did {
    known_mark(m)?;
    let at = at_ticket(s, key)?;
    if s.items[at].take_mark(m) {
        event(s, at, ctx, Happened::Unmarked(m.clone()));
    }
    ctx.wrote(
        s,
        Write::Unmark {
            ticket: key.clone(),
            mark: m.clone(),
        },
    );
    Ok(())
}

/// Close a ticket by hand: already closed is done.
pub(crate) fn close_item(s: &mut State, ctx: &Ctx, at: usize, reason: &CloseReason) {
    if s.items[at].open {
        let closer = Some(Closer::Account(ctx.me(s)));
        let item = &mut s.items[at];
        item.open = false;
        item.reason = Some(reason.clone());
        item.closed = Some(ctx.now);
        item.closer = closer.clone();
        event(
            s,
            at,
            ctx,
            Happened::Closed {
                reason: reason.clone(),
                closer,
            },
        );
    }
}

/// A note on a ticket: on the item, which is the blueprint's where they
/// are one.
fn note(
    s: &mut State,
    ctx: &Ctx,
    key: &TicketKey,
    body: &str,
    markers: &[Marker],
) -> Got<Posted<CommentId>> {
    writable(markers)?;
    let at = at_ticket(s, key)?;
    let author = Some(ctx.me(s));
    let id = s.new_comment(author, body, markers.to_vec(), ctx.now);
    s.items[at].comments.push(id.clone());
    s.touch(at, ctx.now);
    ctx.wrote(
        s,
        Write::TicketNote {
            ticket: key.clone(),
            body: crate::Body::Text(body.to_string()),
            markers: markers.to_vec(),
        },
    );
    let link = s.comment_link(&id);
    Ok(Posted { id, link })
}

impl TicketWrites for Handle {
    fn apply(
        &mut self,
        key: &TicketKey,
        step: &Step,
    ) -> (Applied<StepPart>, Option<Posted<CommentId>>) {
        let mut done = Vec::new();
        let mut posted = None;
        if step.note.is_some() || !step.markers.is_empty() {
            let body = step.note.clone().unwrap_or_default();
            match self.note(key, &body, &step.markers) {
                Ok(p) => {
                    posted = Some(p);
                    done.push(StepPart::Note);
                }
                Err(o) => {
                    return (
                        Applied {
                            done,
                            stopped: Some((StepPart::Note, o)),
                        },
                        None,
                    )
                }
            }
        }
        let parts = step
            .unmark
            .iter()
            .map(|m| StepPart::Unmark(m.clone()))
            .chain(step.mark.iter().map(|m| StepPart::Mark(m.clone())))
            .chain(step.close.iter().map(|_| StepPart::Close));
        for part in parts {
            let r = match &part {
                StepPart::Unmark(m) => self.unmark(key, m),
                StepPart::Mark(m) => self.mark(key, m),
                StepPart::Close => {
                    self.close(key, step.close.as_ref().unwrap_or(&CloseReason::Done))
                }
                StepPart::Note => Ok(()),
            };
            if let Err(o) = r {
                return (
                    Applied {
                        done,
                        stopped: Some((part, o)),
                    },
                    posted,
                );
            }
            done.push(part);
        }
        (
            Applied {
                done,
                stopped: None,
            },
            posted,
        )
    }

    fn prepare(&mut self) -> Result<(), Unprepared> {
        self.write(Op::Tickets(TicketsOp::Prepare), |s, ctx| {
            s.prepared = true;
            ctx.wrote(s, Write::Prepare);
            Ok(())
        })
        .map_err(Unprepared::from)
    }

    fn prepare_one(&mut self, mark: &Mark) -> Did {
        self.write(Op::Tickets(TicketsOp::PrepareOne), |s, _| {
            known_mark(mark)?;
            if matches!(mark, Mark::Project(_)) {
                return Err(Outcome::Refused(Refusal::Invalid));
            }
            if !s.prepared_marks.contains(mark) {
                s.prepared_marks.push(mark.clone());
            }
            Ok(())
        })
    }

    fn ensure_mark(&mut self, spec: &MarkSpec) -> Did {
        self.write(Op::Tickets(TicketsOp::EnsureMark), |s, ctx| {
            match s.project_marks.iter_mut().find(|m| m.name == spec.name) {
                Some(m) => *m = spec.clone(),
                None => s.project_marks.push(spec.clone()),
            }
            ctx.wrote(s, Write::EnsureMark(spec.clone()));
            Ok(())
        })
    }

    fn mark(&mut self, key: &TicketKey, m: &Mark) -> Did {
        self.write(Op::Tickets(TicketsOp::Mark), |s, ctx| mark(s, ctx, key, m))
    }

    fn unmark(&mut self, key: &TicketKey, m: &Mark) -> Did {
        self.write(Op::Tickets(TicketsOp::Unmark), |s, ctx| {
            unmark(s, ctx, key, m)
        })
    }

    fn close(&mut self, key: &TicketKey, reason: &CloseReason) -> Did {
        self.close_at(key, reason).map(|_| ())
    }

    fn close_at(&mut self, key: &TicketKey, reason: &CloseReason) -> Closed {
        self.write(Op::Tickets(TicketsOp::Close), |s, ctx| {
            let at = at_ticket(s, key)?;
            close_item(s, ctx, at, reason);
            ctx.wrote(
                s,
                Write::Close {
                    ticket: key.clone(),
                    reason: reason.clone(),
                },
            );
            Ok(s.items[at].closed)
        })
    }

    fn note(&mut self, key: &TicketKey, body: &str, markers: &[Marker]) -> Got<Posted<CommentId>> {
        self.write(Op::Tickets(TicketsOp::Note), |s, ctx| {
            ctx.text_alone(markers.is_empty())?;
            note(s, ctx, key, body, markers)
        })
    }

    fn file(&mut self, new: &NewTicket) -> Got<Posted<TicketKey>> {
        self.write(Op::Tickets(TicketsOp::File), |s, ctx| {
            ctx.text_alone(
                new.markers.is_empty()
                    && new.marks.is_empty()
                    && new.kind.is_none()
                    && new.priority == Priority::Normal
                    && new.target.is_none()
                    && new.parent.is_none(),
            )?;
            let text = text_of(&new.text)?.to_string();
            writable(&new.markers)?;
            for m in &new.marks {
                known_mark(m)?;
            }
            if new.kind == Some(Kind::Unknown) {
                return Err(Outcome::Refused(Refusal::Invalid));
            }
            if let Some(t) = &new.target {
                if !s.targets.iter().any(|x| x.id == *t) {
                    return Err(Outcome::NotFound);
                }
            }
            let parent = match &new.parent {
                Some(p) => Some(at_ticket(s, p)?),
                None => None,
            };
            let (key, blueprint) = s.next_ticket_key();
            let author = Some(ctx.me(s));
            let at = s.new_item(Some(key.clone()), blueprint, author, ctx.now);
            let item = &mut s.items[at];
            item.title = new.title.clone();
            item.text = text;
            item.markers = new.markers.clone();
            item.kind = new.kind.clone();
            item.priority = new.priority;
            item.target = new.target.clone();
            for m in &new.marks {
                if s.items[at].put_mark(m) {
                    event(s, at, ctx, Happened::Marked(m.clone()));
                }
            }
            if let Some(p) = parent {
                s.items[p].parts.push(key.clone());
            }
            ctx.wrote(s, Write::FileTicket(new.clone()));
            let link = s.ticket_link(&key);
            Ok(Posted { id: key, link })
        })
    }

    fn edit(
        &mut self,
        key: &TicketKey,
        title: Option<&str>,
        text: Option<&str>,
        markers: &MarkerEdit,
        since_rev: &crate::Revision,
    ) -> Got<WebLink> {
        self.write(Op::Tickets(TicketsOp::Edit), |s, ctx| {
            ctx.text_alone(markers.set.is_empty() && markers.drop.is_empty())?;
            markers.check()?;
            let at = at_ticket(s, key)?;
            ctx.own(s, s.items[at].author.as_ref())?;
            if revision(s.items[at].revision) != *since_rev {
                return Err(Outcome::Conflict(crate::Lost::Changed));
            }
            let me = ctx.me(s);
            if let Some(t) = title {
                if s.items[at].title != t {
                    let from = std::mem::replace(&mut s.items[at].title, t.to_string());
                    s.items[at].history.push(TicketEvent {
                        what: Happened::Retitled {
                            from,
                            to: t.to_string(),
                        },
                        by: Some(me.clone()),
                        at: Some(ctx.now),
                        mark_id: None,
                    });
                }
            }
            let item = &mut s.items[at];
            if let Some(t) = text {
                item.text = t.to_string();
            }
            edit_markers(&mut item.markers, markers);
            item.editor = Some(me.clone());
            item.edited = Some(ctx.now);
            item.body_edits.push(Edit {
                by: Some(me),
                at: ctx.now,
                after: item.text.clone(),
                markers: item.markers.clone(),
            });
            s.touch(at, ctx.now);
            ctx.wrote(
                s,
                Write::EditTicket {
                    ticket: key.clone(),
                    title: title.map(str::to_string),
                    text: text.map(|t| crate::Body::Text(t.to_string())),
                    markers: markers.clone(),
                    since: since_rev.clone(),
                },
            );
            Ok(s.ticket_link(key))
        })
    }

    fn reopen(&mut self, key: &TicketKey) -> Did {
        self.write(Op::Tickets(TicketsOp::Reopen), |s, ctx| {
            let at = at_ticket(s, key)?;
            if !s.items[at].open {
                let item = &mut s.items[at];
                item.open = true;
                item.reason = None;
                item.closed = None;
                item.closer = None;
                event(s, at, ctx, Happened::Reopened);
            }
            ctx.wrote(
                s,
                Write::Reopen {
                    ticket: key.clone(),
                },
            );
            Ok(())
        })
    }

    fn set_kind(&mut self, key: &TicketKey, kind: Option<&Kind>) -> Did {
        self.write(Op::Tickets(TicketsOp::SetKind), |s, ctx| {
            if kind == Some(&Kind::Unknown) {
                return Err(Outcome::Refused(Refusal::Invalid));
            }
            let at = at_ticket(s, key)?;
            if s.items[at].kind.as_ref() != kind {
                s.items[at].kind = kind.cloned();
                event(s, at, ctx, Happened::Kind(kind.cloned()));
            }
            ctx.wrote(
                s,
                Write::SetKind {
                    ticket: key.clone(),
                    kind: kind.cloned(),
                },
            );
            Ok(())
        })
    }

    fn set_priority(&mut self, key: &TicketKey, p: Priority) -> Did {
        self.write(Op::Tickets(TicketsOp::SetPriority), |s, ctx| {
            let at = at_ticket(s, key)?;
            if s.items[at].priority != p {
                s.items[at].priority = p;
                event(s, at, ctx, Happened::Priority(p));
            }
            ctx.wrote(
                s,
                Write::SetPriority {
                    ticket: key.clone(),
                    priority: p,
                },
            );
            Ok(())
        })
    }

    /// A ticket that would wait on itself, at any remove, is
    /// `Refused(Cycle)`. A split world's tickets cannot wait on a change.
    fn wait_on(&mut self, key: &TicketKey, on: &Item) -> Did {
        self.write(Op::Tickets(TicketsOp::WaitOn), |s, ctx| {
            let at = at_ticket(s, key)?;
            match on {
                Item::Ticket(k) => {
                    at_ticket(s, k)?;
                    if waits_on(s, k, key) {
                        return Err(Outcome::Refused(Refusal::Cycle));
                    }
                }
                Item::Change(no) => {
                    if s.layout == Layout::Split {
                        return Err(Outcome::Refused(Refusal::Unsupported));
                    }
                    s.change_at(*no).ok_or(Outcome::NotFound)?;
                }
            }
            if !s.items[at].blockers.contains(on) {
                s.items[at].blockers.push(on.clone());
                event(s, at, ctx, Happened::BlockerAdded(Ref::Here(on.clone())));
            }
            ctx.wrote(
                s,
                Write::WaitOn {
                    ticket: key.clone(),
                    on: on.clone(),
                },
            );
            Ok(())
        })
    }

    fn stop_waiting(&mut self, key: &TicketKey, on: &Item) -> Did {
        self.write(Op::Tickets(TicketsOp::StopWaiting), |s, ctx| {
            let at = at_ticket(s, key)?;
            if let Some(i) = s.items[at].blockers.iter().position(|b| b == on) {
                s.items[at].blockers.remove(i);
                event(s, at, ctx, Happened::BlockerRemoved(Ref::Here(on.clone())));
            }
            ctx.wrote(
                s,
                Write::StopWaiting {
                    ticket: key.clone(),
                    on: on.clone(),
                },
            );
            Ok(())
        })
    }

    /// An account the world will not assign is `Refused(Invalid)`.
    fn assign(&mut self, key: &TicketKey, who: &AccountId) -> Did {
        self.write(Op::Tickets(TicketsOp::Assign), |s, ctx| {
            let at = at_ticket(s, key)?;
            let login = match s.login_of(who, ctx.realm) {
                Some(a) if a.assignable => a.login.clone(),
                _ => return Err(Outcome::Refused(Refusal::Invalid)),
            };
            let account = s.account(&login, ctx.realm);
            if !s.items[at].assignees.contains(&account) {
                s.items[at].assignees.push(account.clone());
                event(s, at, ctx, Happened::Assigned(account));
            }
            ctx.wrote(
                s,
                Write::Assign {
                    ticket: key.clone(),
                    who: who.clone(),
                },
            );
            Ok(())
        })
    }

    fn add_part(&mut self, parent: &TicketKey, child: &TicketKey) -> Did {
        self.write(Op::Tickets(TicketsOp::AddPart), |s, ctx| {
            let at = at_ticket(s, parent)?;
            at_ticket(s, child)?;
            if part_of(s, child, parent) {
                return Err(Outcome::Refused(Refusal::Cycle));
            }
            // A part has one parent: another parent's is never moved.
            let elsewhere = s
                .items
                .iter()
                .enumerate()
                .any(|(i, item)| i != at && item.parts.contains(child));
            if elsewhere {
                return Err(Outcome::Refused(Refusal::Invalid));
            }
            if !s.items[at].parts.contains(child) {
                s.items[at].parts.push(child.clone());
                s.touch(at, ctx.now);
            }
            ctx.wrote(
                s,
                Write::AddPart {
                    parent: parent.clone(),
                    child: child.clone(),
                },
            );
            Ok(())
        })
    }

    fn remove_part(&mut self, parent: &TicketKey, child: &TicketKey) -> Did {
        self.write(Op::Tickets(TicketsOp::RemovePart), |s, ctx| {
            let at = at_ticket(s, parent)?;
            if let Some(i) = s.items[at].parts.iter().position(|k| k == child) {
                s.items[at].parts.remove(i);
                s.touch(at, ctx.now);
            }
            ctx.wrote(
                s,
                Write::RemovePart {
                    parent: parent.clone(),
                    child: child.clone(),
                },
            );
            Ok(())
        })
    }

    fn new_target(&mut self, title: &str, due: Option<At>) -> Got<TargetId> {
        self.write(Op::Tickets(TicketsOp::NewTarget), |s, ctx| {
            let id = TargetId::new(s.fresh_id("target"));
            s.targets.push(Target {
                id: id.clone(),
                title: title.to_string(),
                due,
                created: ctx.now,
                open: true,
                open_count: 0,
                closed_count: 0,
            });
            s.bump();
            ctx.wrote(
                s,
                Write::NewTarget {
                    title: title.to_string(),
                    due,
                },
            );
            Ok(id)
        })
    }

    fn edit_target(&mut self, id: &TargetId, edit: &TargetEdit) -> Did {
        self.write(Op::Tickets(TicketsOp::EditTarget), |s, ctx| {
            let t = s
                .targets
                .iter_mut()
                .find(|t| t.id == *id)
                .ok_or(Outcome::NotFound)?;
            match edit {
                TargetEdit::Retitle(title) => t.title = title.clone(),
                TargetEdit::Due(due) => t.due = *due,
                TargetEdit::Close => t.open = false,
                TargetEdit::Reopen => t.open = true,
            }
            s.bump();
            ctx.wrote(
                s,
                Write::EditTarget {
                    id: id.clone(),
                    edit: edit.clone(),
                },
            );
            Ok(())
        })
    }

    fn set_target(&mut self, key: &TicketKey, target: Option<&TargetId>) -> Did {
        self.write(Op::Tickets(TicketsOp::SetTarget), |s, ctx| {
            let at = at_ticket(s, key)?;
            if let Some(t) = target {
                if !s.targets.iter().any(|x| x.id == *t) {
                    return Err(Outcome::NotFound);
                }
            }
            if s.items[at].target.as_ref() != target {
                s.items[at].target = target.cloned();
                event(s, at, ctx, Happened::Targeted(target.cloned()));
            }
            ctx.wrote(
                s,
                Write::SetTarget {
                    ticket: key.clone(),
                    target: target.cloned(),
                },
            );
            Ok(())
        })
    }
}
