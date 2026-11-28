//! The builders a test makes a world's tickets, blueprints and changes
//! with.

use super::state::{nz, Layout, State};
use super::world::OPERATOR;
use crate::{
    Act, At, BlueprintNo, ChangeState, ChangedFile, CloseReason, Closer, Happened, Item, Kind,
    Mark, Marker, Mergeable, Priority, Realm, RepoPath, TargetId, TicketEvent, TicketKey,
};

/// A ticket as a test builds it.
#[derive(Debug, Clone, Default)]
pub struct TicketSpec {
    pub(crate) title: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) markers: Vec<Marker>,
    pub(crate) marks: Vec<(Mark, Option<String>)>,
    pub(crate) closed: Option<CloseReason>,
    pub(crate) closer: Option<Closer>,
    pub(crate) kind: Option<Kind>,
    pub(crate) priority: Option<Priority>,
    pub(crate) target: Option<TargetId>,
    pub(crate) author: Option<String>,
    pub(crate) comments: Vec<(String, String, Vec<Marker>)>,
    pub(crate) notes: Vec<(u64, String, At, String)>,
    pub(crate) assignees: Vec<String>,
    pub(crate) parts: Vec<TicketKey>,
    pub(crate) blockers: Vec<Item>,
    pub(crate) events: Vec<(Happened, Option<String>, Option<At>)>,
    pub(crate) attachments: Vec<(Option<String>, Vec<u8>)>,
}

impl TicketSpec {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Its title.
    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Its text.
    pub fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    /// One of our markers in its text.
    pub fn marker(mut self, marker: Marker) -> Self {
        self.markers.push(marker);
        self
    }

    /// A mark it carries, put on by the factory with no event.
    pub fn mark(mut self, mark: Mark) -> Self {
        self.marks.push((mark, None));
        self
    }

    /// An act put on by `login`, with its event in the history.
    pub fn act(mut self, act: Act, login: &str) -> Self {
        self.marks.push((Mark::Act(act), Some(login.to_string())));
        self
    }

    /// A mark put on by `login`, with its event in the history.
    pub fn marked(mut self, mark: Mark, login: &str) -> Self {
        self.marks.push((mark, Some(login.to_string())));
        self
    }

    /// Closed, for this reason.
    pub fn closed(mut self, reason: CloseReason) -> Self {
        self.closed = Some(reason);
        self
    }

    /// Closed, for this reason, by `closer` in place of the operator's
    /// account: a commit that said it closes it, say.
    pub fn closed_by(mut self, reason: CloseReason, closer: Closer) -> Self {
        self.closed = Some(reason);
        self.closer = Some(closer);
        self
    }

    /// Its type.
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Its priority.
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Its target.
    pub fn target(mut self, id: &str) -> Self {
        self.target = Some(TargetId::new(id));
        self
    }

    /// Who filed it.
    pub fn author(mut self, login: &str) -> Self {
        self.author = Some(login.to_string());
        self
    }

    /// A comment by `login`: a note on the ticket, or, where ticket and
    /// blueprint are one item, a comment on the item.
    pub fn comment(mut self, login: &str, body: &str) -> Self {
        self.comments
            .push((login.to_string(), body.to_string(), Vec::new()));
        self
    }

    /// A comment by `login` with our markers beside its body.
    pub fn comment_marked(mut self, login: &str, body: &str, markers: Vec<Marker>) -> Self {
        self.comments
            .push((login.to_string(), body.to_string(), markers));
        self
    }

    /// A comment numbered `id` by the site, by `login`, written at `at`.
    pub fn note(mut self, id: u64, login: &str, at: At, body: &str) -> Self {
        self.notes
            .push((id, login.to_string(), at, body.to_string()));
        self
    }

    /// An account it is assigned to.
    pub fn assignee(mut self, login: &str) -> Self {
        self.assignees.push(login.to_string());
        self
    }

    /// A part, by key.
    pub fn part(mut self, key: TicketKey) -> Self {
        self.parts.push(key);
        self
    }

    /// What it waits on.
    pub fn blocker(mut self, on: Item) -> Self {
        self.blockers.push(on);
        self
    }

    /// An event of its history, as given.
    pub fn event(mut self, what: Happened, by: Option<&str>, at: Option<At>) -> Self {
        self.events.push((what, by.map(str::to_string), at));
        self
    }

    /// `n` events of its history, a mark on and off by turns, a second
    /// apart from `from`.
    pub fn events(mut self, n: u32, from: At) -> Self {
        for i in 0..n {
            let what = if i % 2 == 0 {
                Happened::Marked(Mark::Epic)
            } else {
                Happened::Unmarked(Mark::Epic)
            };
            let at = At::new(from.secs() + i64::from(i));
            self.events
                .push((what, Some(OPERATOR.to_string()), Some(at)));
        }
        self
    }

    /// A file its text links to on the site's store.
    pub fn attachment(mut self, name: &str, bytes: &[u8]) -> Self {
        self.attachments
            .push((Some(name.to_string()), bytes.to_vec()));
        self
    }

    /// A file pasted into its text, stored as an image with no name.
    pub fn pasted(mut self, bytes: &[u8]) -> Self {
        self.attachments.push((None, bytes.to_vec()));
        self
    }

    pub(crate) fn apply(self, s: &mut State, at: usize, now: At) {
        let realm = s.ticket_realm();
        if let Some(login) = &self.author {
            s.items[at].author = Some(s.account(login, realm));
        }
        let item = &mut s.items[at];
        if let Some(t) = self.title {
            item.title = t;
        }
        if let Some(t) = self.text {
            item.text = t;
        }
        item.markers.extend(self.markers);
        if let Some(k) = self.kind {
            item.kind = Some(k);
        }
        if let Some(p) = self.priority {
            item.priority = p;
        }
        if let Some(t) = self.target {
            item.target = Some(t);
        }
        item.parts.extend(self.parts);
        item.blockers.extend(self.blockers);
        for (mark, by) in self.marks {
            if let Some(login) = by {
                let by = Some(s.account(&login, realm));
                let mut e = TicketEvent::new(Happened::Marked(mark.clone()));
                e.by = by;
                e.at = Some(now);
                e.mark_id = s.mark_id(&mark);
                s.items[at].history.push(e);
            }
            s.items[at].put_mark(&mark);
        }
        for (what, by, when) in self.events {
            let by = by.map(|l| s.account(&l, realm));
            let mut e = TicketEvent::new(what);
            e.by = by;
            e.at = when;
            s.items[at].history.push(e);
        }
        for login in self.assignees {
            let a = s.account(&login, realm);
            s.items[at].assignees.push(a);
        }
        let comment_realm = if s.layout == Layout::SameItem || s.items[at].blueprint.is_some() {
            Realm::CodeHost
        } else {
            realm
        };
        for (login, body, markers) in self.comments {
            let author = Some(s.account(&login, comment_realm));
            let raw = markers.is_empty().then(|| body.clone());
            let id = s.new_comment(author, &body, markers, raw, now);
            s.items[at].comments.push(id);
        }
        for (id, login, created, body) in self.notes {
            let author = Some(s.account(&login, comment_realm));
            let id = s.new_comment_at(
                author,
                crate::CommentId::new(id.to_string()),
                &body,
                created,
                None,
            );
            s.items[at].comments.push(id);
        }
        for (name, bytes) in self.attachments {
            s.attach(at, name, bytes);
        }
        if let Some(reason) = self.closed {
            let closer = Some(match self.closer {
                Some(closer) => closer,
                None => Closer::Account(s.account(OPERATOR, realm)),
            });
            let item = &mut s.items[at];
            item.open = false;
            item.reason = Some(reason.clone());
            item.closed = Some(now);
            item.closer = closer.clone();
            let mut e = TicketEvent::new(Happened::Closed { reason, closer });
            e.at = Some(now);
            item.history.push(e);
        }
        s.touch(at, now);
    }
}

/// A blueprint as a test builds it.
#[derive(Debug, Clone, Default)]
pub struct BlueprintSpec {
    pub(crate) title: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) markers: Vec<Marker>,
    pub(crate) comments: Vec<(String, String, Vec<Marker>)>,
    pub(crate) comments_as: Vec<(crate::Account, String)>,
    pub(crate) notes: Vec<(u64, String, At, String)>,
    pub(crate) closed: bool,
    pub(crate) author: Option<String>,
    pub(crate) attachments: Vec<(Option<String>, Vec<u8>)>,
}

impl BlueprintSpec {
    /// Its title.
    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Its body.
    pub fn body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }

    /// One of our markers in its body, in the order given.
    pub fn marker(mut self, marker: Marker) -> Self {
        self.markers.push(marker);
        self
    }

    /// A comment by `login`.
    pub fn comment(mut self, login: &str, body: &str) -> Self {
        self.comments
            .push((login.to_string(), body.to_string(), Vec::new()));
        self
    }

    /// A comment by `login` with our markers beside its body.
    pub fn comment_marked(mut self, login: &str, body: &str, markers: Vec<Marker>) -> Self {
        self.comments
            .push((login.to_string(), body.to_string(), markers));
        self
    }

    /// A comment by `author` as given, after those by login: an account
    /// whose id is not the world's for its login, as one that holds the
    /// factory's login under another id.
    pub fn comment_as(mut self, author: crate::Account, body: &str) -> Self {
        self.comments_as.push((author, body.to_string()));
        self
    }

    /// A comment numbered `id` by the site, by `login`, written at `at`.
    pub fn note(mut self, id: u64, login: &str, at: At, body: &str) -> Self {
        self.notes
            .push((id, login.to_string(), at, body.to_string()));
        self
    }

    /// Closed.
    pub fn closed(mut self) -> Self {
        self.closed = true;
        self
    }

    /// Who opened it.
    pub fn author(mut self, login: &str) -> Self {
        self.author = Some(login.to_string());
        self
    }

    /// A file its body links to on the site's store.
    pub fn attachment(mut self, name: &str, bytes: &[u8]) -> Self {
        self.attachments
            .push((Some(name.to_string()), bytes.to_vec()));
        self
    }

    /// A file pasted into its body, stored as an image with no name.
    pub fn pasted(mut self, bytes: &[u8]) -> Self {
        self.attachments.push((None, bytes.to_vec()));
        self
    }

    pub(crate) fn apply(self, s: &mut State, at: usize, now: At) {
        if let Some(login) = &self.author {
            s.items[at].author = Some(s.account(login, Realm::CodeHost));
        }
        let item = &mut s.items[at];
        if let Some(t) = self.title {
            item.title = t;
        }
        if let Some(b) = self.body {
            item.text = b;
        }
        item.markers.extend(self.markers);
        for (login, body, markers) in self.comments {
            let author = Some(s.account(&login, Realm::CodeHost));
            let raw = markers.is_empty().then(|| body.clone());
            let id = s.new_comment(author, &body, markers, raw, now);
            s.items[at].comments.push(id);
        }
        for (author, body) in self.comments_as {
            let id = s.new_comment(Some(author), &body, Vec::new(), Some(body.clone()), now);
            s.items[at].comments.push(id);
        }
        for (id, login, created, body) in self.notes {
            let author = Some(s.account(&login, Realm::CodeHost));
            let id = s.new_comment_at(
                author,
                crate::CommentId::new(id.to_string()),
                &body,
                created,
                None,
            );
            s.items[at].comments.push(id);
        }
        for (name, bytes) in self.attachments {
            s.attach(at, name, bytes);
        }
        if self.closed {
            let item = &mut s.items[at];
            item.open = false;
            item.reason = Some(CloseReason::Done);
            item.closed = Some(now);
        }
        s.touch(at, now);
    }
}

/// A change as a test builds it.
#[derive(Debug, Clone, Default)]
pub struct ChangeSpec {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) author: Option<String>,
    pub(crate) marks: Vec<Mark>,
    pub(crate) closes: Vec<BlueprintNo>,
    pub(crate) files: Vec<ChangedFile>,
    pub(crate) comments: Vec<(String, String)>,
    pub(crate) reviews: Vec<(u64, String, At, String)>,
    pub(crate) notes: Vec<super::state::NoteRec>,
    pub(crate) events: Vec<(String, crate::ChangeHappened)>,
    pub(crate) state: Option<ChangeState>,
    pub(crate) merge_state: Option<Mergeable>,
    pub(crate) fork: bool,
}

impl ChangeSpec {
    /// Its title.
    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// Its body.
    pub fn body(mut self, body: &str) -> Self {
        self.body = body.to_string();
        self
    }

    /// Who opened it.
    pub fn author(mut self, login: &str) -> Self {
        self.author = Some(login.to_string());
        self
    }

    /// A mark it carries.
    pub fn mark(mut self, mark: Mark) -> Self {
        self.marks.push(mark);
        self
    }

    /// A blueprint it closes.
    pub fn closes(mut self, no: u64) -> Self {
        self.closes.push(BlueprintNo::from(nz(no)));
        self
    }

    /// A file it changes.
    pub fn file(mut self, path: &str, added: u32, deleted: u32) -> Self {
        self.files.push(ChangedFile {
            path: RepoPath::new(path).expect("a path"),
            added,
            deleted,
        });
        self
    }

    /// A comment by `login`.
    pub fn comment(mut self, login: &str, body: &str) -> Self {
        self.comments.push((login.to_string(), body.to_string()));
        self
    }

    /// A review its reviewer submitted: numbered `id`, by `login`, at
    /// `at`, with this text.
    pub fn review(mut self, id: u64, login: &str, at: At, body: &str) -> Self {
        self.reviews
            .push((id, login.to_string(), at, body.to_string()));
        self
    }

    /// A note its reviewer left on a file: numbered `id`, by `login`, at
    /// `at`, on `path` at `line`, with this text.
    pub fn note(mut self, id: u64, login: &str, at: At, body: &str, path: &str, line: u32) -> Self {
        self.notes.push(super::state::NoteRec {
            id,
            login: login.to_string(),
            at,
            edited: None,
            body: body.to_string(),
            path: Some(RepoPath::new(path).expect("a path")),
            line: Some(line),
        });
        self
    }

    /// A note with no file to name.
    pub fn note_free(mut self, id: u64, login: &str, at: At, body: &str) -> Self {
        self.notes.push(super::state::NoteRec {
            id,
            login: login.to_string(),
            at,
            edited: None,
            body: body.to_string(),
            path: None,
            line: None,
        });
        self
    }

    /// An event of its activity, by `login`, now.
    pub fn event(mut self, login: &str, what: crate::ChangeHappened) -> Self {
        self.events.push((login.to_string(), what));
        self
    }

    /// Its state.
    pub fn state(mut self, state: ChangeState) -> Self {
        self.state = Some(state);
        self
    }

    /// Whether it can merge.
    pub fn merge_state(mut self, m: Mergeable) -> Self {
        self.merge_state = Some(m);
        self
    }

    /// Whether the head lives in another repository: a fork's change, whose
    /// head is no branch of this one.
    pub fn fork(mut self, yes: bool) -> Self {
        self.fork = yes;
        self
    }
}
