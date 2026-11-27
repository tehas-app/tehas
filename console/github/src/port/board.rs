//! The joined reads, `Board`. Answered: `open_changes` for an ask that
//! needs no blueprint and no body, so no marker: the changes a filter of
//! states and branch takes, each with its files and the other open changes
//! that close the same blueprints, in one GraphQL question a page; for an
//! ask of numbers, with the blueprints each closes, in `numbers.rs`; and
//! `open_work` for the queue's ask, in `work.rs`. The rest is
//! `Refused(Unsupported)` before any request.

mod numbers;
mod work;

use std::collections::HashSet;

use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::mergeable::mergeable;
use super::unsupported;
use crate::{gh, GitHub};
use tehas_ports::{
    Account, At, BlueprintNo, Board, BoardAsk, Bound, Branch, ChangeAsk, ChangeDigest,
    ChangeFilter, ChangeItem, ChangeNo, ChangeState, ChangeStates, ChangedFile, CommitId, Got,
    ItemName, OpenWork, Outcome, ProjectName, Ref, RepoPath, Size, TargetId, TicketKey, Unread,
    WebLink, Whole, WorkItem,
};

/// The changes one page asks for, as the dispatcher's read of open pull
/// requests asks today.
const PULL_PAGE: usize = 100;

/// The most pages a list of changes reads, the dispatcher's `PULL_PAGES`.
/// A list longer than that is `Unread(TooLarge)`.
const PULL_PAGES: usize = 50;

/// The files one page of a change asks for.
const FILE_PAGE: usize = 100;

/// The blueprints a change's `closes` is read to, and a change's labels;
/// past either the change is `Unread(TooLarge)`. GitHub keeps at most 100
/// labels on an item.
const CLOSES_MAX: usize = 50;
const LABELS_MAX: usize = 100;

/// The fields of an account, as the forge's other GraphQL questions ask.
const ACCOUNT: &str = "__typename login ...on User { databaseId } ...on Bot { databaseId }";

/// One page of the changes a filter takes. `{STATES}`, `{HEAD_VAR}`,
/// `{HEAD}`, `{FILES}` and the page size are filled in by
/// [`changes_query`], `{PULL}` by [`pull_fields`].
const CHANGES: &str = "query($owner: String!, $name: String!, $cursor: String{HEAD_VAR}) { repository(owner: $owner, name: $name) { databaseId pullRequests(states: [{STATES}], first: {PULL_PAGE}, after: $cursor, orderBy: {field: CREATED_AT, direction: ASC}{HEAD}) { totalCount pageInfo { hasNextPage endCursor } nodes { {PULL}{FILES} } } } }";

/// What a change's digest is read from, a [`Pull`]: `{ACCOUNT}`, the
/// bounds and what a [`PullAsk`] adds are filled in by [`pull_fields_of`].
const PULL: &str = "number title url state isDraft isCrossRepository headRefName headRefOid baseRefName baseRefOid mergeable mergeStateStatus createdAt updatedAt lastEditedAt mergedAt closedAt additions deletions changedFiles author { {ACCOUNT} }{RELATION} editor { {ACCOUNT} } mergedBy { {ACCOUNT} } mergeCommit { oid }{RENAMES}{HEAD_REPO}{BODY} labels(first: {LABELS_MAX}) { totalCount nodes { name } } closingIssuesReferences(first: {CLOSES_MAX}) { totalCount nodes { number repository { databaseId nameWithOwner } } }";

/// The newest retitling, read with the body's last edit as when the change
/// was last edited.
const RENAMES: &str = " renames: timelineItems(itemTypes: [RENAMED_TITLE_EVENT], last: 1) { nodes { ... on RenamedTitleEvent { createdAt actor { {ACCOUNT} } } } }";

/// The author's relation to the project, the same words as REST's
/// `author_association`, asked where [`PullAsk::author_relation`] says.
const RELATION: &str = " authorAssociation";

/// What a question asks of each pull request past the fields every one
/// asks, and so what [`GitHub::pull_read`] reads of it.
#[derive(Clone, Copy)]
pub(super) struct PullAsk {
    /// The newest retitling, so `edited` is the newer of it and the body's
    /// edit, not the body's alone.
    pub(super) retitles: bool,
    /// The head repository's id, so a change is from a fork unless its head
    /// is this repository, not only when GitHub says so.
    pub(super) head_repo: bool,
    /// The author's relation to the project, so a reader of a list says
    /// whose the change is without a call for each author.
    pub(super) author_relation: bool,
    /// The body, left in [`Pull::body`] for the caller to read.
    pub(super) body: bool,
    /// What labels or closed issues past what the question asks are, as
    /// each port's recorded samples say: the board's `TooLarge`, the code
    /// host's `Cut`.
    pub(super) past: Unread,
}

impl PullAsk {
    /// What the board's questions ask.
    pub(super) const BOARD: PullAsk = PullAsk {
        retitles: true,
        head_repo: false,
        author_relation: false,
        body: false,
        past: Unread::TooLarge,
    };
}

/// The fields of a pull request the board's questions ask.
pub(super) fn pull_fields() -> String {
    pull_fields_of(PullAsk::BOARD)
}

/// The fields of a pull request, with what `ask` adds.
pub(super) fn pull_fields_of(ask: PullAsk) -> String {
    let renames = if ask.retitles { RENAMES } else { "" };
    let head_repo = if ask.head_repo {
        " headRepository { databaseId }"
    } else {
        ""
    };
    let body = if ask.body { " body" } else { "" };
    let relation = if ask.author_relation { RELATION } else { "" };
    PULL.replace("{RENAMES}", renames)
        .replace("{RELATION}", relation)
        .replace("{HEAD_REPO}", head_repo)
        .replace("{BODY}", body)
        .replace("{LABELS_MAX}", &LABELS_MAX.to_string())
        .replace("{CLOSES_MAX}", &CLOSES_MAX.to_string())
        .replace("{ACCOUNT}", ACCOUNT)
}

/// A change's files, a page, nested in [`CHANGES`] or asked after it.
const FILES: &str = "files(first: {FILE_PAGE}, after: $files) { totalCount pageInfo { hasNextPage endCursor } nodes { path additions deletions } }";

/// The files of one change after the page [`CHANGES`] gave.
const MORE_FILES: &str = "query($owner: String!, $name: String!, $number: Int!, $files: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { {FILES} } } }";

fn files_field() -> String {
    FILES.replace("{FILE_PAGE}", &FILE_PAGE.to_string())
}

/// The question of one page of changes in `states`, from `head` alone when
/// given, with each change's first page of files when `files`.
fn changes_query(states: ChangeStates, head: bool, files: bool) -> String {
    let states = match states {
        ChangeStates::Open => "OPEN",
        ChangeStates::Merged => "MERGED",
        ChangeStates::Closed => "CLOSED",
        ChangeStates::All => "OPEN, MERGED, CLOSED",
    };
    let (head_var, head_arg) = if head {
        (", $head: String!", ", headRefName: $head")
    } else {
        ("", "")
    };
    let files = if files {
        format!(" {}", files_field().replace(", after: $files", ""))
    } else {
        String::new()
    };
    CHANGES
        .replace("{HEAD_VAR}", head_var)
        .replace("{HEAD}", head_arg)
        .replace("{STATES}", states)
        .replace("{FILES}", &files)
        .replace("{PULL_PAGE}", &PULL_PAGE.to_string())
        .replace("{PULL}", &pull_fields())
}

fn more_files_query() -> String {
    MORE_FILES.replace("{FILES}", &files_field())
}

impl Board for GitHub {
    /// The open issues, 25 a question, each with its digest, text,
    /// blueprint, parts, blockers, assignees and mentions, its marks and
    /// retitles when asked and its open closing changes when asked. Asked
    /// at `Depth::Cut { notes: 0 }`, of every open ticket; any other ask is
    /// `Refused(Unsupported)` before any request.
    fn open_work(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork> {
        self.open_work_pages(ask, bound)
    }

    fn work_item(&mut self, _: &TicketKey, _: &BoardAsk) -> Got<WorkItem> {
        unsupported()
    }

    fn closed_work(
        &mut self,
        _: &BoardAsk,
        _: At,
        _: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>> {
        unsupported()
    }

    fn target_work(
        &mut self,
        _: &BoardAsk,
        _: &TargetId,
        _: Bound,
    ) -> Got<Whole<(TicketKey, Got<WorkItem>)>> {
        unsupported()
    }

    /// The changes in number order, one question a page of 100, each with
    /// its first 100 files when asked; a change with more files is read on
    /// alone. Answered for a filter of states and branch, with no bodies,
    /// and an ask of files and siblings; siblings only beside every open
    /// change, which they are read from. Any other ask, a blueprint's or a
    /// body's, is `Refused(Unsupported)` before any request. An ask of
    /// numbers and states, with no files and no events, is answered as
    /// `numbers.rs` says, with what each change closes and its siblings.
    fn open_changes(&mut self, ask: &ChangeAsk, bound: Bound) -> Got<Whole<ChangeItem>> {
        if numbers::asked(ask) {
            return self.numbered_changes(ask, bound);
        }
        if !answered(ask) {
            return unsupported();
        }
        let mut read = self.change_pages(&ask.filter, ask.files.is_some(), bound)?;
        read.sort_by_key(|(c, _)| c.no);
        if read.windows(2).any(|w| w[0].0.no == w[1].0.no) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let mut items = Vec::with_capacity(read.len());
        for (change, first) in &read {
            let files = match (ask.files, first) {
                (Some(each), Some(first)) => Some(self.files_of(change.no, first, each)?),
                (Some(_), None) => return Err(Outcome::Unread(Unread::Malformed)),
                (None, _) => None,
            };
            let siblings = match ask.siblings {
                Some(each) => siblings(change, &read, each)?,
                None => Vec::new(),
            };
            let mut item = ChangeItem::new(change.clone());
            item.files = files;
            item.siblings = siblings;
            items.push(item);
        }
        Ok(Whole::new(items))
    }
}

/// Whether this adapter answers the ask: no blueprint, body or event, and a
/// filter of states and branch. Siblings are read from the list itself, so
/// they are answered beside every open change alone.
fn answered(ask: &ChangeAsk) -> bool {
    let f = &ask.filter;
    let narrow = f.author.is_some()
        || !f.closing.is_empty()
        || f.merged_since.is_some()
        || f.updated_since.is_some()
        || !f.numbers.is_empty()
        || !f.carrying.is_empty()
        || f.bodies;
    let every_open = *f == ChangeFilter::default();
    !narrow && !ask.closing && ask.activity.is_none() && (ask.siblings.is_none() || every_open)
}

/// The other open changes of `all` that close a blueprint of this project
/// `change` closes, in number order, `change` itself counted where it is
/// open. More than `each` fails the read.
fn siblings(
    change: &ChangeDigest,
    all: &[(ChangeDigest, Option<FileConn>)],
    each: Bound,
) -> Got<Vec<ChangeDigest>> {
    let here: Vec<BlueprintNo> = change.closes.iter().filter_map(here_of).collect();
    if here.is_empty() {
        return Ok(Vec::new());
    }
    let same: Vec<&ChangeDigest> = all
        .iter()
        .map(|(c, _)| c)
        .filter(|c| c.state == ChangeState::Open)
        .filter(|c| {
            c.closes
                .iter()
                .filter_map(here_of)
                .any(|n| here.contains(&n))
        })
        .collect();
    if same.len() > each.get() as usize {
        return Err(Outcome::Unread(Unread::Cut));
    }
    Ok(same
        .into_iter()
        .filter(|c| c.no != change.no)
        .cloned()
        .collect())
}

fn here_of(r: &Ref<BlueprintNo>) -> Option<BlueprintNo> {
    match r {
        Ref::Here(no) => Some(*no),
        Ref::Elsewhere { .. } => None,
    }
}

impl GitHub {
    /// Every change the filter takes, each with its first page of files
    /// when `files`, in the site's order: whole, or `Unread(Cut)` once the
    /// site's count or the pages read pass `bound`.
    fn change_pages(
        &mut self,
        filter: &ChangeFilter,
        files: bool,
        bound: Bound,
    ) -> Got<Vec<(ChangeDigest, Option<FileConn>)>> {
        let (owner, name) = self.owner_name();
        let query = format!(
            "query={}",
            changes_query(filter.states, filter.head.is_some(), files)
        );
        let head = filter.head.as_ref().map(|b| format!("head={}", b.as_str()));
        let mut read = Vec::new();
        let mut total = None;
        let mut cursor: Option<String> = None;
        let mut seen = HashSet::new();
        for _ in 0..PULL_PAGES {
            let at = cursor.as_ref().map(|c| format!("cursor={c}"));
            let mut args = vec!["api", "graphql", "-F", &owner, "-F", &name, "-f", &query];
            for field in [&head, &at].into_iter().flatten() {
                args.extend(["-f", field.as_str()]);
            }
            let out = self
                .call("the open changes", &args)
                .map_err(|e| self.outcome(&e))?;
            let page = answer::<PullsRepo>(&out)?.pull_requests;
            if *total.get_or_insert(page.total_count) != page.total_count {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            if page.total_count > u64::from(bound.get()) {
                return Err(Outcome::Unread(Unread::Cut));
            }
            for node in page.nodes {
                let mut pull = node.ok_or(Outcome::Unread(Unread::Inconsistent))?;
                let first = pull.files.take();
                read.push((self.pull_digest(pull, filter.states)?, first));
            }
            if read.len() > bound.get() as usize {
                return Err(Outcome::Unread(Unread::Cut));
            }
            match next(&page.page_info, &mut seen)? {
                Some(c) => cursor = Some(c),
                None if read.len() as u64 == page.total_count => return Ok(read),
                None => return Err(Outcome::Unread(Unread::Inconsistent)),
            }
        }
        Err(Outcome::Unread(Unread::TooLarge))
    }

    /// A change's files, whole up to `each`: the page its list gave, then
    /// the pages after it, one question each.
    fn files_of(&mut self, no: ChangeNo, first: &FileConn, each: Bound) -> Got<Whole<ChangedFile>> {
        if first.total_count > u64::from(each.get()) {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let (owner, name) = self.owner_name();
        let query = format!("query={}", more_files_query());
        let number = format!("number={}", no.get());
        let mut files = Vec::new();
        let mut page = first.clone();
        let mut seen = HashSet::new();
        // A page past the count the first page gave is an answer that
        // does not add up, so the loop always ends.
        for _ in 0..=first.total_count as usize / FILE_PAGE + 1 {
            if page.total_count != first.total_count {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            for f in page.nodes {
                let f = f.ok_or(Outcome::Unread(Unread::Inconsistent))?;
                let mut file = ChangedFile::new(RepoPath::new(f.path).map_err(|_| malformed())?);
                file.added = count(f.additions)?;
                file.deleted = count(f.deletions)?;
                files.push(file);
            }
            let Some(cursor) = next(&page.page_info, &mut seen)? else {
                if files.len() as u64 != first.total_count {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                return Ok(Whole::new(files));
            };
            let after = format!("files={cursor}");
            let out = self
                .call(
                    "a change's files",
                    &[
                        "api", "graphql", "-F", &owner, "-F", &name, "-F", &number, "-f", &query,
                        "-f", &after,
                    ],
                )
                .map_err(|e| self.outcome(&e))?;
            // The list named this change: none here, the two answers
            // disagree. One gone since fails the call itself, as gh exits
            // non-zero on the error that says so.
            page = answer::<OnePull>(&out)?
                .pull_request
                .ok_or(Outcome::Unread(Unread::Inconsistent))?
                .files;
        }
        Err(Outcome::Unread(Unread::Inconsistent))
    }

    /// A pull request the board asked as a change's digest, in the states
    /// asked: [`GitHub::pull_read`] of [`PullAsk::BOARD`].
    pub(super) fn pull_digest(&mut self, p: Pull, asked: ChangeStates) -> Got<ChangeDigest> {
        self.pull_read(p, asked, PullAsk::BOARD)
    }

    /// A pull request as a change's digest, in the states asked, `ask`
    /// what its question asked: the adapter's one reader of a pull request
    /// node. A node of another state, or with no number, does not add up.
    /// Labels or closed issues past what one question asks are
    /// `Unread` as `ask.past` says. A field `ask` names that the answer leaves out
    /// is `Unread(Malformed)`. The body is left to the caller.
    pub(super) fn pull_read(
        &mut self,
        p: Pull,
        asked: ChangeStates,
        ask: PullAsk,
    ) -> Got<ChangeDigest> {
        let no = p
            .number
            .and_then(ChangeNo::new)
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        let state = match p.state.as_str() {
            "OPEN" => ChangeState::Open,
            "MERGED" => ChangeState::Merged {
                at: time(p.merged_at.as_deref())?,
                by: p
                    .merged_by
                    .as_ref()
                    .and_then(|a| a.account(self.bot.as_ref())),
                commit: p.merge_commit.map(|c| commit(Some(c.oid))).transpose()?,
            },
            "CLOSED" => ChangeState::Closed {
                at: time(p.closed_at.as_deref())?,
            },
            _ => return Err(malformed()),
        };
        let fits = matches!(
            (asked, &state),
            (ChangeStates::All, _)
                | (ChangeStates::Open, ChangeState::Open)
                | (ChangeStates::Merged, ChangeState::Merged { .. })
                | (ChangeStates::Closed, ChangeState::Closed { .. })
        );
        if !fits {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        if p.labels.total_count as usize > LABELS_MAX
            || p.closing_issues_references.total_count as usize > CLOSES_MAX
        {
            return Err(Outcome::Unread(ask.past));
        }
        let labels = p
            .labels
            .nodes
            .into_iter()
            .map(|l| l.map(|l| l.name))
            .collect::<Option<Vec<String>>>()
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        if labels.len() as u64 != p.labels.total_count {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let closes = self.closes(p.closing_issues_references, ask.past)?;
        let renamed = match (ask.retitles, p.renames) {
            (true, Some(r)) => r.nodes.into_iter().flatten().last(),
            (true, None) => return Err(malformed()),
            (false, _) => None,
        };
        let (edited, editor) = edited(
            p.last_edited_at.as_deref(),
            p.editor.as_ref(),
            renamed,
            self.bot.as_ref(),
        )?;
        // From a fork unless GitHub says it is not and, where the question
        // asks it, its head repository is this one: a deleted head
        // repository, or one GitHub does not name, fails closed.
        let from_fork = match (ask.head_repo, p.head_repository) {
            (false, _) => p.is_cross_repository,
            (true, None) => return Err(malformed()),
            (true, Some(None)) => true,
            (true, Some(Some(r))) => p.is_cross_repository || !self.is_here_id(r.database_id)?,
        };
        let head = Branch::new(p.head_ref_name).map_err(|_| malformed())?;
        let head_commit = commit(Some(p.head_ref_oid))?;
        let base = Branch::new(p.base_ref_name).map_err(|_| malformed())?;
        let base_commit = commit(Some(p.base_ref_oid))?;
        let marks = self.read_marks(&labels).into_iter().flatten().collect();
        let mut size = Size::default();
        size.files = count(p.changed_files)?;
        size.added = count(p.additions)?;
        size.deleted = count(p.deletions)?;
        let created = time(Some(&p.created_at))?;
        let updated = time(Some(&p.updated_at))?;
        let link = WebLink::new(p.url).map_err(|_| malformed())?;
        let mut digest = ChangeDigest::new(
            no,
            state,
            head,
            head_commit,
            base,
            base_commit,
            created,
            updated,
            link,
        );
        digest.title = p.title;
        digest.author = p.author.as_ref().and_then(|a| a.account(self.bot.as_ref()));
        digest.from_fork = from_fork;
        digest.draft = p.is_draft;
        digest.merge_state = mergeable(&p.mergeable, &p.merge_state_status);
        digest.marks = marks;
        digest.closes = closes;
        digest.size = Some(size);
        digest.edited = edited;
        digest.editor = editor;
        Ok(digest)
    }

    /// A pull request node's closed issues read alone, as a lead the
    /// filter decides on reads them: by the rule [`GitHub::pull_read`]
    /// reads them with.
    pub(super) fn pull_closes(&self, c: PullCloses, past: Unread) -> Got<Vec<Ref<BlueprintNo>>> {
        self.closes(c.closing_issues_references, past)
    }

    /// The blueprints a change closes, whole: `Unread` as `past` says past
    /// what one question asks, `Unread(Inconsistent)` for a null issue or a
    /// count the issues do not add up to.
    fn closes(&self, closing: Counted<Closing>, past: Unread) -> Got<Vec<Ref<BlueprintNo>>> {
        if closing.total_count as usize > CLOSES_MAX {
            return Err(Outcome::Unread(past));
        }
        let mut closes = Vec::with_capacity(closing.nodes.len());
        for c in closing.nodes {
            let c = c.ok_or(Outcome::Unread(Unread::Inconsistent))?;
            closes.push(self.closed_ref(c)?);
        }
        if closes.len() as u64 != closing.total_count {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        Ok(closes)
    }

    /// The blueprint a change closes: this project's by its number, another
    /// project's by its names.
    fn closed_ref(&self, c: Closing) -> Got<Ref<BlueprintNo>> {
        let no = c
            .number
            .and_then(BlueprintNo::new)
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        if self.is_here_id(c.repository.database_id)? {
            Ok(Ref::Here(no))
        } else {
            Ok(Ref::Elsewhere {
                project: ProjectName::new(c.repository.name_with_owner),
                item: ItemName::new(no.get().to_string()),
            })
        }
    }
}

/// The repository of a GraphQL answer, read whole. gh exits non-zero on
/// an answer that carries an error, so its outcome is the failed call's;
/// an error in an answer handed over anyway still gives no partial list.
fn answer<T: DeserializeOwned>(out: &str) -> Got<T> {
    let mut v: serde_json::Value = serde_json::from_str(out.trim()).map_err(|_| malformed())?;
    tehas_core::accounts::vouch(&mut v);
    let a: Answer<T> = serde_json::from_value(v).map_err(|_| malformed())?;
    if !a.errors.is_empty() {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    a.data.and_then(|d| d.repository).ok_or_else(malformed)
}

/// The cursor of the next page, None after the last. A page that says
/// there is more and names no cursor, or names one read before, does not
/// add up.
fn next(page: &PageInfo, seen: &mut HashSet<String>) -> Got<Option<String>> {
    match (page.has_next_page, &page.end_cursor) {
        (false, _) => Ok(None),
        (true, Some(c)) if seen.insert(c.clone()) => Ok(Some(c.clone())),
        (true, _) => Err(Outcome::Unread(Unread::Inconsistent)),
    }
}

/// When the title or the body was last edited, and by whom: the newer of
/// the body's last edit and the newest retitling.
fn edited(
    body_at: Option<&str>,
    body_by: Option<&Actor>,
    renamed: Option<Renamed>,
    bot: Option<&crate::tickets::Bot>,
) -> Got<(Option<At>, Option<Account>)> {
    let body = match body_at {
        Some(t) => Some((time(Some(t))?, body_by.and_then(|a| a.account(bot)))),
        None => None,
    };
    let title = match renamed {
        Some(r) => Some((
            time(r.created_at.as_deref())?,
            r.actor.as_ref().and_then(|a| a.account(bot)),
        )),
        None => None,
    };
    let newest = match (body, title) {
        (Some(b), Some(t)) if t.0 > b.0 => Some(t),
        (Some(b), _) => Some(b),
        (None, t) => t,
    };
    Ok(match newest {
        Some((at, by)) => (Some(at), by),
        None => (None, None),
    })
}

fn malformed() -> Outcome {
    Outcome::Unread(Unread::Malformed)
}

fn time(t: Option<&str>) -> Got<At> {
    t.and_then(tehas_core::time::from_iso)
        .map(At::new)
        .ok_or_else(malformed)
}

fn commit(oid: Option<String>) -> Got<CommitId> {
    oid.and_then(|o| CommitId::new(o).ok())
        .ok_or_else(malformed)
}

fn count(n: u64) -> Got<u32> {
    u32::try_from(n).map_err(|_| malformed())
}

/// A GraphQL answer: its data, and the errors that came with it.
#[derive(Deserialize)]
struct Answer<T> {
    data: Option<Data<T>>,
    #[serde(default)]
    errors: Vec<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct Data<T> {
    repository: Option<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullsRepo {
    pull_requests: Conn<Pull>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnePull {
    pull_request: Option<FilesOf>,
}

#[derive(Deserialize)]
struct FilesOf {
    files: FileConn,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Conn<T> {
    total_count: u64,
    page_info: PageInfo,
    nodes: Vec<Option<T>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

/// A list read whole in one answer: no page of its own.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Counted<T> {
    total_count: u64,
    nodes: Vec<Option<T>>,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<Option<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Pull {
    number: Option<u64>,
    title: String,
    url: String,
    state: String,
    is_draft: bool,
    is_cross_repository: bool,
    head_ref_name: String,
    head_ref_oid: String,
    base_ref_name: String,
    base_ref_oid: String,
    mergeable: String,
    merge_state_status: String,
    created_at: String,
    updated_at: String,
    last_edited_at: Option<String>,
    merged_at: Option<String>,
    closed_at: Option<String>,
    additions: u64,
    deletions: u64,
    changed_files: u64,
    author: Option<Actor>,
    editor: Option<Actor>,
    merged_by: Option<Actor>,
    #[serde(deserialize_with = "crate::ticket::present")]
    merge_commit: Option<Oid>,
    /// Where [`PullAsk::retitles`] asks it.
    #[serde(default)]
    renames: Option<Nodes<Renamed>>,
    /// Where [`PullAsk::head_repo`] asks it: `Some(None)` for a deleted one.
    #[serde(default, deserialize_with = "crate::ticket::given")]
    head_repository: Option<Option<HeadRepo>>,
    /// Where [`PullAsk::body`] asks it.
    #[serde(default)]
    pub(super) body: Option<String>,
    labels: Counted<Label>,
    closing_issues_references: Counted<Closing>,
    #[serde(default)]
    files: Option<FileConn>,
    /// The repository it belongs to, where a question asks it.
    #[serde(default)]
    repository: Option<NamedRepo>,
}

/// The closed issues of a pull request node, and nothing else of it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PullCloses {
    closing_issues_references: Counted<Closing>,
}

type FileConn = Conn<File>;

#[derive(Clone, Deserialize)]
struct File {
    path: String,
    additions: u64,
    deletions: u64,
}

#[derive(Deserialize)]
struct HeadRepo {
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
}

#[derive(Deserialize)]
struct Oid {
    oid: String,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

#[derive(Deserialize)]
struct Closing {
    number: Option<u64>,
    repository: NamedRepo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NamedRepo {
    #[serde(rename = "databaseId")]
    pub(super) database_id: Option<u64>,
    pub(super) name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Renamed {
    created_at: Option<String>,
    actor: Option<Actor>,
}

/// An account as a GraphQL answer names it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Actor {
    #[serde(rename = "__typename")]
    typename: Option<String>,
    login: String,
    database_id: Option<u64>,
}

impl Actor {
    fn account(&self, bot: Option<&crate::tickets::Bot>) -> Option<Account> {
        let facts = gh::Facts {
            id: self.database_id,
            kind: self.typename.clone(),
        };
        crate::tickets::account(&self.login, &facts, bot)
    }
}
