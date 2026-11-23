//! `World`: the in-memory site the fakes share, its builders, its clock and
//! what was written to it.

use std::sync::{Arc, Mutex, MutexGuard};

use super::spec::{BlueprintSpec, ChangeSpec, TicketSpec};
use super::state::{lock, nz, split_key, AccountRec, ChangeRec, Layout, ReleaseRec, State};

use serde::{Deserialize, Serialize};

use crate::{
    Account, AccountKind, At, BlueprintNo, Branch, ChangeEvent, ChangeNo, ChangeState, Check,
    CommentId, CommitId, DeployCredential, Got, Keyring, Kind, Links, Mergeable, PersonKey,
    ProjectName, Realm, Release, RepoPath, Secret, SignInCode, Standing, Tag, TagName, Target,
    TargetId, TicketKey, Token, Write,
};

/// The login of the factory's own account in a `World`.
pub const FACTORY: &str = "factory";
/// The login of the operator's own account in a `World`.
pub const OPERATOR: &str = "operator";
/// The prefix of a ticket's key in a split world: `PROJ-7`.
pub const KEY_PREFIX: &str = "PROJ-";

/// A clock the test sets and moves. The fakes read it; nothing reads the
/// system's.
#[derive(Debug, Clone)]
pub struct Clock(Arc<Mutex<At>>);

impl Clock {
    /// A clock that reads `at` until moved.
    pub fn new(at: At) -> Self {
        Self(Arc::new(Mutex::new(at)))
    }

    /// The time it reads.
    pub fn now(&self) -> At {
        *lock(&self.0)
    }

    /// Set it to `at`.
    pub fn set(&self, at: At) {
        *lock(&self.0) = at;
    }

    /// Move it on by `secs` seconds.
    pub fn advance(&self, secs: i64) {
        let mut at = lock(&self.0);
        *at = At::new(at.secs() + secs);
    }
}

/// One write the fake applied, in our words, and the account it was made
/// as: what a test asserts in place of a site's request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Wrote {
    /// The account the write was made as.
    pub by: Account,
    /// The write.
    pub what: Write,
}

/// An in-memory site: tickets, blueprints, changes, comments with editors,
/// tags, releases, accounts with ids and standing, targets, kinds, links,
/// and a clock the test moves. Cloned, it is the same world.
#[derive(Debug, Clone)]
pub struct World {
    pub(crate) state: Arc<Mutex<State>>,
    clock: Clock,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// A world where every ticket is its own blueprint: `same_item`.
    pub fn new() -> Self {
        Self::same_item()
    }

    /// Every ticket its own blueprint, one number space for tickets,
    /// blueprints and changes.
    pub fn same_item() -> Self {
        Self::of(Layout::SameItem)
    }

    /// Tickets keyed `PROJ-n`, blueprints numbered apart from them from
    /// 101, changes apart from blueprints from 501, and links between
    /// tickets and blueprints kept by the world.
    pub fn split() -> Self {
        Self::of(Layout::Split)
    }

    fn of(layout: Layout) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::new(layout))),
            clock: Clock::new(At::new(1_000_000)),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// The world's clock.
    pub fn clock(&self) -> Clock {
        self.clock.clone()
    }

    /// The time the world's clock reads.
    pub fn now(&self) -> At {
        self.clock.now()
    }

    /// Set the world's clock.
    pub fn set_now(&self, at: At) {
        self.clock.set(at);
    }

    /// Move the world's clock on by `secs` seconds.
    pub fn advance(&self, secs: i64) {
        self.clock.advance(secs);
    }

    /// Every write applied so far, oldest first.
    pub fn wrote(&self) -> Vec<Wrote> {
        self.lock().wrote.clone()
    }

    /// What the world holds, as text, without its record of writes: two
    /// worlds that hold the same read the same.
    pub fn held(&self) -> String {
        let mut s = self.lock();
        let wrote = std::mem::take(&mut s.wrote);
        let text = format!("{:?}", *s);
        s.wrote = wrote;
        text
    }

    /// The project's name, as the world's sites name it.
    pub fn project(&self) -> ProjectName {
        self.lock().project.clone()
    }

    /// The key of ticket `n`: its number where ticket and blueprint are one
    /// item, `PROJ-n` in a split world.
    pub fn key(&self, n: u64) -> TicketKey {
        match self.lock().layout {
            Layout::SameItem => TicketKey::Number(nz(n)),
            Layout::Split => split_key(n),
        }
    }

    /// An account of the world in a realm, by login.
    pub fn account(&self, login: &str, realm: Realm) -> Account {
        self.lock().account(login, realm)
    }

    /// The commit a branch points at.
    pub fn head(&self, branch: &str) -> Option<CommitId> {
        let name = Branch::new(branch).ok()?;
        self.lock().branch_at(&name)
    }

    /// The keys the world's keyring was told were signed out, in order.
    pub fn signed_out_keys(&self) -> Vec<PersonKey> {
        self.lock().signed_out.clone()
    }

    /// Add or change an account: its kind and standing on the project.
    pub fn with_account(self, login: &str, kind: AccountKind, standing: Standing) -> Self {
        self.lock().add_account(login, kind, standing);
        self
    }

    /// Whether an account can be assigned tickets.
    pub fn assignable(self, login: &str, yes: bool) -> Self {
        self.edit_account(login, |a| a.assignable = yes)
    }

    /// Make a name an organisation.
    pub fn organisation(self, login: &str) -> Self {
        self.lock()
            .add_account(login, AccountKind::Unknown, Standing::None);
        self.edit_account(login, |a| a.organisation = true)
    }

    fn edit_account(self, login: &str, f: impl FnOnce(&mut AccountRec)) -> Self {
        {
            let mut s = self.lock();
            if !s.accounts.iter().any(|a| a.login == login) {
                s.add_account(login, AccountKind::Person, Standing::Read);
            }
            if let Some(a) = s.accounts.iter_mut().find(|a| a.login == login) {
                f(a);
            }
        }
        self
    }

    /// A signed-in person: the keyring holds a credential under `key`.
    pub fn person(self, key: &str, login: &str) -> Self {
        {
            let mut s = self.lock();
            if !s.accounts.iter().any(|a| a.login == login) {
                s.add_account(login, AccountKind::Person, Standing::Read);
            }
            s.persons.push((PersonKey::new(key), login.to_string()));
        }
        self
    }

    /// A deploy credential under `key`.
    pub fn deploy(self, key: &str, credential: DeployCredential) -> Self {
        self.lock().deploys.push((PersonKey::new(key), credential));
        self
    }

    /// A token handed in under `key`, for `Actor::Given`.
    pub fn given(self, key: &str, token: Token) -> Self {
        self.lock().given.push((PersonKey::new(key), token));
        self
    }

    /// A finished sign-in that hands back `code` for `login`, good once.
    pub fn sign_in(self, code: &str, login: &str) -> Self {
        {
            let mut s = self.lock();
            if !s.accounts.iter().any(|a| a.login == login) {
                s.add_account(login, AccountKind::Person, Standing::Read);
            }
            s.sign_ins
                .push((SignInCode::new(code), login.to_string(), false));
        }
        self
    }

    /// The projects the factory's grant covers, this world's among them.
    pub fn grant(self, projects: &[&str]) -> Self {
        {
            let mut s = self.lock();
            let own = s.project.clone();
            s.grant_projects = projects.iter().map(|p| ProjectName::new(*p)).collect();
            if !s.grant_projects.contains(&own) {
                s.grant_projects.push(own);
            }
        }
        self
    }

    /// Add a type of ticket the site has.
    pub fn kind(self, kind: Kind) -> Self {
        {
            let mut s = self.lock();
            if !s.kinds.contains(&kind) {
                s.kinds.push(kind);
            }
        }
        self
    }

    /// A target, open, made now.
    pub fn target(self, id: &str, title: &str, due: Option<At>) -> Self {
        let now = self.now();
        self.lock().targets.push(Target {
            id: TargetId::new(id),
            title: title.to_string(),
            due,
            created: now,
            open: true,
            open_count: 0,
            closed_count: 0,
        });
        self
    }

    /// Ticket `n`, built by `f`. Where ticket and blueprint are one item it
    /// is blueprint `n` too.
    pub fn ticket(self, n: u64, f: impl FnOnce(TicketSpec) -> TicketSpec) -> Self {
        let key = self.key(n);
        let spec = f(TicketSpec::new());
        {
            let mut s = self.lock();
            let at = match s.ticket_at(&key) {
                Some(at) => at,
                None => {
                    let blueprint = key.same_item();
                    let author = Some(s.account(FACTORY, s.ticket_realm()));
                    let now = self.clock.now();
                    let at = s.new_item(Some(key.clone()), blueprint, author, now);
                    s.next_ticket = s.next_ticket.max(n + 1);
                    at
                }
            };
            spec.apply(&mut s, at, self.clock.now());
        }
        self
    }

    /// Blueprint `n`, built by `f`. Where ticket and blueprint are one item
    /// it is ticket `n` too.
    pub fn blueprint(self, n: u64, f: impl FnOnce(BlueprintSpec) -> BlueprintSpec) -> Self {
        let no = BlueprintNo::from(nz(n));
        let spec = f(BlueprintSpec::default());
        {
            let mut s = self.lock();
            let at = match s.blueprint_at(no) {
                Some(at) => at,
                None => {
                    let ticket = match s.layout {
                        Layout::SameItem => Some(TicketKey::Number(nz(n))),
                        Layout::Split => None,
                    };
                    let author = Some(s.account(FACTORY, Realm::CodeHost));
                    let at = s.new_item(ticket, Some(no), author, self.clock.now());
                    match s.layout {
                        Layout::SameItem => s.next_ticket = s.next_ticket.max(n + 1),
                        Layout::Split => s.next_blueprint = s.next_blueprint.max(n + 1),
                    }
                    at
                }
            };
            spec.apply(&mut s, at, self.clock.now());
        }
        self
    }

    /// Link ticket `ticket` to blueprint `blueprint`, as the store records
    /// it on a split setup.
    pub fn link(self, ticket: u64, blueprint: u64) -> Self {
        let key = self.key(ticket);
        self.lock()
            .links
            .push((key, BlueprintNo::from(nz(blueprint))));
        self
    }

    /// Change `n`, from branch `head` into the default branch, built by `f`.
    /// The branch is made at a new commit if it is not there.
    pub fn change(self, n: u64, head: &str, f: impl FnOnce(ChangeSpec) -> ChangeSpec) -> Self {
        let spec = f(ChangeSpec::default());
        {
            let mut s = self.lock();
            let now = self.clock.now();
            let head = Branch::new(head).expect("a branch");
            let base = s.default_branch.clone();
            let head_commit = match s.branch_at(&head) {
                Some(c) => c,
                None => {
                    let parent = s.branch_at(&base).into_iter().collect();
                    let by = spec.author.clone().unwrap_or_else(|| FACTORY.into());
                    let c = s.new_commit(parent, "work", &by, Vec::new());
                    s.set_branch(&head, c.clone());
                    c
                }
            };
            let author =
                Some(s.account(spec.author.as_deref().unwrap_or(FACTORY), Realm::CodeHost));
            let seq = s.bump();
            let no = ChangeNo::from(nz(n));
            s.changes.push(ChangeRec {
                no,
                title: spec.title.clone(),
                body: spec.body.clone(),
                markers: Vec::new(),
                state: ChangeState::Open,
                head,
                head_commit,
                base,
                author,
                draft: false,
                merge_state: Mergeable::Yes,
                marks: spec.marks.clone(),
                closes: spec.closes.clone(),
                files: spec.files.clone(),
                reviews: Vec::new(),
                comments: Vec::new(),
                events: Vec::new(),
                created: now,
                edited: None,
                editor: None,
                seq,
            });
            match s.layout {
                Layout::SameItem => s.next_ticket = s.next_ticket.max(n + 1),
                Layout::Split => s.next_change = s.next_change.max(n + 1),
            }
            let at = s.changes.len() - 1;
            for (login, body) in &spec.comments {
                let author = Some(s.account(login, Realm::CodeHost));
                let id = s.new_comment(author, body, Vec::new(), now);
                s.changes[at].comments.push(id);
            }
            for (login, what) in &spec.events {
                let by = Some(s.account(login, Realm::CodeHost));
                s.changes[at].events.push(ChangeEvent {
                    what: what.clone(),
                    by,
                    at: now,
                    committed: None,
                });
            }
            if let Some(state) = &spec.state {
                s.changes[at].state = state.clone();
            }
            if let Some(m) = spec.merge_state {
                s.changes[at].merge_state = m;
            }
        }
        self
    }

    /// A new commit on `branch` by `by`, over the branch's head or, for a
    /// branch not there, over the default branch's; the branch moves to it.
    /// What a push does.
    pub fn push(&self, branch: &str, message: &str, by: &str) -> CommitId {
        let name = Branch::new(branch).expect("a branch");
        let mut s = self.lock();
        let parent = s
            .branch_at(&name)
            .or_else(|| s.branch_at(&s.default_branch.clone()));
        let c = s.new_commit(parent.into_iter().collect(), message, by, Vec::new());
        s.set_branch(&name, c.clone());
        c
    }

    /// A new commit of two parents on `branch`, a merge of `other` into it.
    pub fn push_merge(&self, branch: &str, other: &CommitId, message: &str, by: &str) -> CommitId {
        let name = Branch::new(branch).expect("a branch");
        let mut s = self.lock();
        let parent = s
            .branch_at(&name)
            .or_else(|| s.branch_at(&s.default_branch.clone()));
        let mut parents: Vec<CommitId> = parent.into_iter().collect();
        parents.push(other.clone());
        let c = s.new_commit(parents, message, by, Vec::new());
        s.set_branch(&name, c.clone());
        c
    }

    /// Write a file at a new commit on `branch`.
    pub fn put_file(&self, branch: &str, path: &str, bytes: &[u8]) -> CommitId {
        let name = Branch::new(branch).expect("a branch");
        let path = RepoPath::new(path).expect("a path");
        let mut s = self.lock();
        let parent = s.branch_at(&name);
        let c = s.new_commit(
            parent.into_iter().collect(),
            "a file",
            FACTORY,
            vec![(path, bytes.to_vec())],
        );
        s.set_branch(&name, c.clone());
        c
    }

    /// A check on a commit.
    pub fn check(self, at: &CommitId, check: Check) -> Self {
        self.lock().checks.push((at.clone(), check));
        self
    }

    /// A tag on a commit.
    pub fn tag(self, name: &str, at: &CommitId) -> Self {
        let now = self.now();
        self.lock().tags.push(Tag {
            name: TagName::new(name).expect("a tag"),
            commit: at.clone(),
            at: Some(now),
            annotated: false,
        });
        self
    }

    /// A published release of a tag, with its files.
    pub fn release(self, tag: &str, files: &[(&str, &[u8])]) -> Self {
        let now = self.now();
        {
            let mut s = self.lock();
            let tag = TagName::new(tag).expect("a tag");
            let id = crate::ReleaseId::new(s.fresh_id("release"));
            let link = s.release_link(&tag);
            let mut release = Release {
                id,
                tag: tag.clone(),
                title: tag.as_str().to_string(),
                draft: false,
                prerelease: false,
                published: Some(now),
                files: Vec::new(),
                link,
            };
            let mut bytes = Vec::new();
            for (name, b) in files {
                release.files.push(crate::ReleaseFile {
                    name: crate::FileName::new(*name).expect("a file name"),
                    size: b.len() as u64,
                });
                bytes.push(b.to_vec());
            }
            s.releases.push(ReleaseRec { release, bytes });
        }
        self
    }

    /// Delete a comment, as a person does between a read and a write.
    pub fn delete_comment(&self, id: &CommentId) {
        let mut s = self.lock();
        if let Some(at) = s.comment_at(id) {
            s.comments[at].gone = true;
        }
    }

    /// Edit an item's text as another account does, between a caller's
    /// read and its write: a new revision.
    pub fn edit_elsewhere(&self, key: &TicketKey, text: &str, by: &str) {
        let now = self.now();
        let mut s = self.lock();
        if let Some(at) = s.ticket_at(key) {
            let realm = s.ticket_realm();
            let editor = s.account(by, realm);
            s.items[at].text = text.to_string();
            s.items[at].editor = Some(editor);
            s.items[at].edited = Some(now);
            s.touch(at, now);
        }
    }
}

impl Links for World {
    fn blueprints_of(&self, ticket: &TicketKey) -> Got<Vec<BlueprintNo>> {
        let s = self.lock();
        Ok(match s.layout {
            Layout::SameItem => ticket.same_item().into_iter().collect(),
            Layout::Split => s
                .links
                .iter()
                .filter(|(k, _)| k == ticket)
                .map(|(_, b)| *b)
                .collect(),
        })
    }

    fn ticket_of(&self, blueprint: BlueprintNo) -> Got<Option<TicketKey>> {
        let s = self.lock();
        Ok(match s.layout {
            Layout::SameItem => Some(TicketKey::Number(nz(blueprint.get()))),
            Layout::Split => s
                .links
                .iter()
                .find(|(_, b)| *b == blueprint)
                .map(|(k, _)| k.clone()),
        })
    }
}

/// The world is its own keyring: the persons, deploy credentials and given
/// tokens its builders added, and the keys it was told were signed out.
impl Keyring for World {
    fn secret(&self, key: &PersonKey) -> Option<Secret> {
        let s = self.lock();
        s.persons
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, login)| Secret::new(format!("secret-{login}")))
    }

    fn deploy(&self, key: &PersonKey) -> Option<DeployCredential> {
        let s = self.lock();
        s.deploys
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, c)| c.clone())
    }

    fn given(&self, key: &PersonKey) -> Option<Token> {
        let s = self.lock();
        s.given
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, t)| t.clone())
    }

    fn signed_out(&self, key: &PersonKey) {
        self.lock().signed_out.push(key.clone());
    }
}
