//! The state a `World` holds, and the rules every port of the fake
//! shares over it: numbering, links, commits, accounts.

use std::num::NonZeroU64;
use std::sync::{Mutex, MutexGuard};

use crate::{
    Account, AccountKind, At, Attachment, BlueprintNo, Branch, ChangeEvent, ChangeNo, ChangeState,
    ChangedFile, Check, CloseReason, Closer, CommentId, CommitId, DeployCredential, Edit, GrantId,
    Item, KeyText, Kind, Mark, MarkSpec, Marker, Mergeable, PersonKey, Priority, ProjectName,
    Proposal, Realm, Release, RepoPath, Review, SignInCode, Standing, Status, Tag, TagName, Target,
    TargetId, TicketEvent, TicketKey, Token, WebLink,
};

use super::world::{Wrote, FACTORY, KEY_PREFIX, OPERATOR};

/// Which setup a world answers as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// Every ticket is its own blueprint, and tickets, blueprints and
    /// changes share one number space.
    SameItem,
    /// Tickets are keyed `PROJ-n`; blueprints and changes are numbered
    /// apart from them and from each other.
    Split,
}

/// A ticket, a blueprint, or both where they are one item.
#[derive(Debug, Clone)]
pub(crate) struct ItemRec {
    pub ticket: Option<TicketKey>,
    pub blueprint: Option<BlueprintNo>,
    pub title: String,
    pub text: String,
    pub markers: Vec<Marker>,
    pub open: bool,
    pub reason: Option<CloseReason>,
    pub closed: Option<At>,
    pub closer: Option<Closer>,
    pub kind: Option<Kind>,
    pub priority: Priority,
    pub marks: Vec<Mark>,
    pub target: Option<TargetId>,
    pub author: Option<Account>,
    pub editor: Option<Account>,
    pub created: At,
    pub updated: At,
    pub edited: Option<At>,
    pub revision: u64,
    pub seq: u64,
    pub history: Vec<TicketEvent>,
    pub comments: Vec<CommentId>,
    pub body_edits: Vec<Edit>,
    pub parts: Vec<TicketKey>,
    pub blockers: Vec<Item>,
    pub assignees: Vec<Account>,
    pub mentioned_by: Vec<ChangeNo>,
    pub attachments: Vec<(Attachment, Vec<u8>)>,
}

/// A comment on a blueprint, a note on a ticket, or a comment on a change.
#[derive(Debug, Clone)]
pub(crate) struct CommentRec {
    pub id: CommentId,
    pub author: Option<Account>,
    pub editor: Option<Account>,
    pub created: At,
    pub edited: Option<At>,
    pub body: String,
    pub markers: Vec<Marker>,
    pub revision: u64,
    pub seq: u64,
    pub edits: Vec<Edit>,
    pub gone: bool,
    pub link: WebLink,
}

/// A change.
#[derive(Debug, Clone)]
pub(crate) struct ChangeRec {
    pub no: ChangeNo,
    pub title: String,
    pub body: String,
    pub markers: Vec<Marker>,
    pub state: ChangeState,
    pub head: Branch,
    pub head_commit: CommitId,
    pub base: Branch,
    pub author: Option<Account>,
    pub draft: bool,
    pub merge_state: Mergeable,
    pub marks: Vec<Mark>,
    pub closes: Vec<BlueprintNo>,
    pub files: Vec<ChangedFile>,
    pub reviews: Vec<Review>,
    pub comments: Vec<CommentId>,
    pub events: Vec<ChangeEvent>,
    pub created: At,
    pub edited: Option<At>,
    pub editor: Option<Account>,
    pub seq: u64,
}

/// A commit: its parents, message, author and every file at it.
#[derive(Debug, Clone)]
pub(crate) struct CommitRec {
    pub id: CommitId,
    pub parents: Vec<CommitId>,
    pub message: String,
    pub by: String,
    pub files: Vec<(RepoPath, Vec<u8>)>,
}

/// An account of the world, by login, in every realm it serves.
#[derive(Debug, Clone)]
pub(crate) struct AccountRec {
    pub login: String,
    pub kind: AccountKind,
    pub standing: Standing,
    pub assignable: bool,
    pub organisation: bool,
}

/// A release and the bytes of its files.
#[derive(Debug, Clone)]
pub(crate) struct ReleaseRec {
    pub release: Release,
    pub bytes: Vec<Vec<u8>>,
}

/// A proposal the world opened a change for, and the head it opened at.
#[derive(Debug, Clone)]
pub(crate) struct ProposalRec {
    pub proposal: Proposal,
    pub change: ChangeNo,
    pub head: CommitId,
}

/// Everything a world holds.
#[derive(Debug)]
pub(crate) struct State {
    pub layout: Layout,
    pub project: ProjectName,
    pub public: bool,
    pub default_branch: Branch,
    pub items: Vec<ItemRec>,
    pub comments: Vec<CommentRec>,
    pub changes: Vec<ChangeRec>,
    pub accounts: Vec<AccountRec>,
    pub targets: Vec<Target>,
    pub kinds: Vec<Kind>,
    pub prepared: bool,
    pub project_marks: Vec<MarkSpec>,
    pub commits: Vec<CommitRec>,
    pub branches: Vec<(Branch, CommitId)>,
    pub tags: Vec<Tag>,
    pub releases: Vec<ReleaseRec>,
    pub checks: Vec<(CommitId, Check)>,
    pub statuses: Vec<(CommitId, Status)>,
    pub links: Vec<(TicketKey, BlueprintNo)>,
    pub proposals: Vec<ProposalRec>,
    pub persons: Vec<(PersonKey, String)>,
    pub deploys: Vec<(PersonKey, DeployCredential)>,
    pub given: Vec<(PersonKey, Token)>,
    pub signed_out: Vec<PersonKey>,
    pub sign_ins: Vec<(SignInCode, String, bool)>,
    pub grant: GrantId,
    pub grant_projects: Vec<ProjectName>,
    pub wrote: Vec<Wrote>,
    pub seq: u64,
    pub next_id: u64,
    pub next_ticket: u64,
    pub next_blueprint: u64,
    pub next_change: u64,
    pub next_commit: u64,
}

/// Where the first blueprint and change of a split world are numbered, so
/// a caller that takes one for another finds nothing.
const SPLIT_BLUEPRINTS: u64 = 101;
const SPLIT_CHANGES: u64 = 501;

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn nz(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap_or(NonZeroU64::MIN)
}

fn link(path: String) -> WebLink {
    WebLink::new(format!("https://fake.test/{path}"))
        .unwrap_or_else(|_| WebLink::new("https://fake.test/").expect("a fixed link"))
}

impl State {
    pub(crate) fn new(layout: Layout) -> Self {
        let (blueprints, changes) = match layout {
            Layout::SameItem => (1, 1),
            Layout::Split => (SPLIT_BLUEPRINTS, SPLIT_CHANGES),
        };
        let mut state = State {
            layout,
            project: ProjectName::new("fake/project"),
            public: true,
            default_branch: Branch::new("main").expect("a branch"),
            items: Vec::new(),
            comments: Vec::new(),
            changes: Vec::new(),
            accounts: Vec::new(),
            targets: Vec::new(),
            kinds: vec![Kind::Feature, Kind::Bug, Kind::Task, Kind::Epic],
            prepared: false,
            project_marks: Vec::new(),
            commits: Vec::new(),
            branches: Vec::new(),
            tags: Vec::new(),
            releases: Vec::new(),
            checks: Vec::new(),
            statuses: Vec::new(),
            links: Vec::new(),
            proposals: Vec::new(),
            persons: Vec::new(),
            deploys: Vec::new(),
            given: Vec::new(),
            signed_out: Vec::new(),
            sign_ins: Vec::new(),
            grant: GrantId::new("grant-1"),
            grant_projects: Vec::new(),
            wrote: Vec::new(),
            seq: 0,
            next_id: 1,
            next_ticket: 1,
            next_blueprint: blueprints,
            next_change: changes,
            next_commit: 1,
        };
        state.grant_projects.push(state.project.clone());
        state.add_account(FACTORY, AccountKind::Ours, Standing::Write);
        state.add_account(OPERATOR, AccountKind::Person, Standing::Admin);
        let root = state.new_commit(Vec::new(), "the first commit", FACTORY, Vec::new());
        let main = state.default_branch.clone();
        state.branches.push((main, root));
        state
    }

    pub fn add_account(&mut self, login: &str, kind: AccountKind, standing: Standing) {
        match self.accounts.iter_mut().find(|a| a.login == login) {
            Some(a) => {
                a.kind = kind;
                a.standing = standing;
            }
            None => self.accounts.push(AccountRec {
                login: login.to_string(),
                kind,
                standing,
                assignable: standing != Standing::None,
                organisation: false,
            }),
        }
    }

    /// The realm of the tickets port's accounts.
    pub fn ticket_realm(&self) -> Realm {
        match self.layout {
            Layout::SameItem => Realm::CodeHost,
            Layout::Split => Realm::Tickets,
        }
    }

    /// An account by login, in a realm. Its id is the realm's own.
    pub fn account(&self, login: &str, realm: Realm) -> Account {
        let kind = self
            .accounts
            .iter()
            .find(|a| a.login == login)
            .map_or(AccountKind::Person, |a| a.kind);
        Account {
            realm,
            login: crate::Login::new(login),
            id: Some(account_id(login, realm)),
            kind,
        }
    }

    /// The login an account id names in a realm, if the world has it.
    pub fn login_of(&self, id: &crate::AccountId, realm: Realm) -> Option<&AccountRec> {
        self.accounts
            .iter()
            .find(|a| account_id(&a.login, realm) == *id)
    }

    /// Bump the world's version, the one every change token and cursor
    /// reads.
    pub fn bump(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    pub fn fresh_id(&mut self, what: &str) -> String {
        let id = format!("{what}-{}", self.next_id);
        self.next_id += 1;
        id
    }

    pub fn ticket_link(&self, key: &TicketKey) -> WebLink {
        let project = self.project.as_str();
        match key {
            TicketKey::Number(n) => link(format!("{project}/t/{n}")),
            TicketKey::Key(k) => link(format!("{project}/t/{}", k.as_str())),
        }
    }

    pub fn blueprint_link(&self, no: BlueprintNo) -> WebLink {
        link(format!("{}/b/{}", self.project.as_str(), no.get()))
    }

    pub fn change_link(&self, no: ChangeNo) -> WebLink {
        link(format!("{}/c/{}", self.project.as_str(), no.get()))
    }

    pub fn comment_link(&self, id: &CommentId) -> WebLink {
        link(format!("{}/n/{}", self.project.as_str(), id.as_str()))
    }

    pub fn release_link(&self, tag: &TagName) -> WebLink {
        link(format!("{}/r/{}", self.project.as_str(), tag.as_str()))
    }

    /// The item of a ticket.
    pub fn ticket_at(&self, key: &TicketKey) -> Option<usize> {
        self.items
            .iter()
            .position(|i| i.ticket.as_ref() == Some(key))
    }

    /// The item of a blueprint.
    pub fn blueprint_at(&self, no: BlueprintNo) -> Option<usize> {
        self.items.iter().position(|i| i.blueprint == Some(no))
    }

    pub fn change_at(&self, no: ChangeNo) -> Option<usize> {
        self.changes.iter().position(|c| c.no == no)
    }

    pub fn comment_at(&self, id: &CommentId) -> Option<usize> {
        self.comments.iter().position(|c| c.id == *id)
    }

    pub fn commit(&self, id: &CommitId) -> Option<&CommitRec> {
        self.commits.iter().find(|c| c.id == *id)
    }

    pub fn branch_at(&self, name: &Branch) -> Option<CommitId> {
        self.branches
            .iter()
            .find(|(b, _)| b == name)
            .map(|(_, c)| c.clone())
    }

    /// Point a branch at a commit, made if not there. An open change from
    /// the branch follows it.
    pub fn set_branch(&mut self, name: &Branch, at: CommitId) {
        match self.branches.iter_mut().find(|(b, _)| b == name) {
            Some((_, c)) => *c = at.clone(),
            None => self.branches.push((name.clone(), at.clone())),
        }
        for change in &mut self.changes {
            if change.head == *name && change.state == ChangeState::Open {
                change.head_commit = at.clone();
            }
        }
    }

    /// A new commit over `parents`, its files the first parent's with
    /// `files` written over them. Its id comes from a counter, never a
    /// digest.
    pub fn new_commit(
        &mut self,
        parents: Vec<CommitId>,
        message: &str,
        by: &str,
        files: Vec<(RepoPath, Vec<u8>)>,
    ) -> CommitId {
        let id = CommitId::new(format!("{:040x}", self.next_commit)).expect("forty hex digits");
        self.next_commit += 1;
        let mut tree = parents
            .first()
            .and_then(|p| self.commit(p))
            .map(|c| c.files.clone())
            .unwrap_or_default();
        for (path, bytes) in files {
            match tree.iter_mut().find(|(p, _)| *p == path) {
                Some((_, b)) => *b = bytes,
                None => tree.push((path, bytes)),
            }
        }
        self.commits.push(CommitRec {
            id: id.clone(),
            parents,
            message: message.to_string(),
            by: by.to_string(),
            files: tree,
        });
        id
    }

    /// Every commit reachable from `from`, itself included.
    pub fn ancestors(&self, from: &CommitId) -> Vec<CommitId> {
        let mut seen: Vec<CommitId> = Vec::new();
        let mut todo = vec![from.clone()];
        while let Some(id) = todo.pop() {
            if seen.contains(&id) {
                continue;
            }
            if let Some(c) = self.commit(&id) {
                todo.extend(c.parents.iter().cloned());
            }
            seen.push(id);
        }
        seen
    }

    /// A new ticket's key, and where ticket and blueprint are one item, its
    /// blueprint's number.
    pub fn next_ticket_key(&mut self) -> (TicketKey, Option<BlueprintNo>) {
        match self.layout {
            Layout::SameItem => {
                let n = self.next_shared();
                (TicketKey::Number(nz(n)), BlueprintNo::new(n))
            }
            Layout::Split => {
                let n = self.next_ticket;
                self.next_ticket += 1;
                (split_key(n), None)
            }
        }
    }

    pub fn next_blueprint_no(&mut self) -> (BlueprintNo, Option<TicketKey>) {
        match self.layout {
            Layout::SameItem => {
                let n = self.next_shared();
                (BlueprintNo::from(nz(n)), Some(TicketKey::Number(nz(n))))
            }
            Layout::Split => {
                let n = self.next_blueprint;
                self.next_blueprint += 1;
                (BlueprintNo::from(nz(n)), None)
            }
        }
    }

    pub fn next_change_no(&mut self) -> ChangeNo {
        match self.layout {
            Layout::SameItem => ChangeNo::from(nz(self.next_shared())),
            Layout::Split => {
                let n = self.next_change;
                self.next_change += 1;
                ChangeNo::from(nz(n))
            }
        }
    }

    /// The next number of the one space a same-item world shares.
    fn next_shared(&mut self) -> u64 {
        let used = self
            .items
            .iter()
            .filter_map(|i| i.blueprint.map(BlueprintNo::get))
            .chain(self.changes.iter().map(|c| c.no.get()))
            .max()
            .unwrap_or(0);
        let n = self.next_ticket.max(used + 1);
        self.next_ticket = n + 1;
        n
    }

    /// A new item, ticket, blueprint or both, open, made now by `author`.
    pub fn new_item(
        &mut self,
        ticket: Option<TicketKey>,
        blueprint: Option<BlueprintNo>,
        author: Option<Account>,
        now: At,
    ) -> usize {
        let seq = self.bump();
        self.items.push(ItemRec {
            ticket,
            blueprint,
            title: String::new(),
            text: String::new(),
            markers: Vec::new(),
            open: true,
            reason: None,
            closed: None,
            closer: None,
            kind: None,
            priority: Priority::Normal,
            marks: Vec::new(),
            target: None,
            author,
            editor: None,
            created: now,
            updated: now,
            edited: None,
            revision: 1,
            seq,
            history: Vec::new(),
            comments: Vec::new(),
            body_edits: Vec::new(),
            parts: Vec::new(),
            blockers: Vec::new(),
            assignees: Vec::new(),
            mentioned_by: Vec::new(),
            attachments: Vec::new(),
        });
        self.items.len() - 1
    }

    /// A new comment, kept on the item or the change the caller names.
    pub fn new_comment(
        &mut self,
        author: Option<Account>,
        body: &str,
        markers: Vec<Marker>,
        now: At,
    ) -> CommentId {
        let id = CommentId::new(self.fresh_id("comment"));
        let seq = self.bump();
        let link = self.comment_link(&id);
        self.comments.push(CommentRec {
            id: id.clone(),
            author,
            editor: None,
            created: now,
            edited: None,
            body: body.to_string(),
            markers,
            revision: 1,
            seq,
            edits: Vec::new(),
            gone: false,
            link,
        });
        id
    }

    /// The item changed: a new revision, seen by the next delta.
    pub fn touch(&mut self, at: usize, now: At) {
        let seq = self.bump();
        let item = &mut self.items[at];
        item.revision += 1;
        item.seq = seq;
        item.updated = now;
    }
}

/// An account's id in a realm: the world's own spelling, stable across
/// renames because the world never renames.
pub(crate) fn account_id(login: &str, realm: Realm) -> crate::AccountId {
    match realm {
        Realm::CodeHost => crate::AccountId::new(format!("id-{login}")),
        Realm::Tickets => crate::AccountId::new(format!("tid-{login}")),
    }
}

/// A split world's ticket key of number `n`.
pub(crate) fn split_key(n: u64) -> TicketKey {
    TicketKey::Key(KeyText::new(format!("{KEY_PREFIX}{n}")).expect("a key"))
}
