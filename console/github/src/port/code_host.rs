//! The code host port, `CodeHost` and `CodeHostWrites`: changes, branches,
//! tags and releases. Answered: what the code host says of the project
//! (`facts`), the changes a filter takes, with their bodies when it asks
//! (`changes`), one change whole (`change`, in `change.rs`), the check
//! runs and status contexts on a commit (`checks`, in `checks.rs`), and a
//! pull request's timeline and the commit a branch points at (`activity`
//! and `branch`, in `activity.rs`). Every other read answers
//! `Refused(Unsupported)` before any request.

mod activity;
mod change;
mod checks;
mod numbers;
mod remote;
pub use checks::WORKFLOW_RUNS_FORBIDDEN;
pub(super) use remote::parts;
pub use remote::project_of;

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use super::mergeable::mergeable;
use super::unsupported;
use crate::{markers, ForgeError, GitHub};
use serde_json::Value;
use tehas_ports::{
    Access, Account, AccountId, AccountKind, At, Author, BlueprintNo, Bound, Branch, Change,
    ChangeDelta, ChangeDigest, ChangeEvent, ChangeFilter, ChangeNo, ChangeState, ChangeStates,
    ChangeToken, ChangedFile, Checks, CodeHost, CommitId, Compared, Cursor, Did, Facts, FileName,
    Got, ItemName, Login, Mark, MarkName, Outcome, Pool, ProjectName, Realm, Ref, Release, Remote,
    RepoPath, Rev, Size, Tag, TagName, Took, Unread, WebLink, Whole, Window,
};

impl CodeHost for GitHub {
    /// The repository's REST read.
    fn facts(&mut self) -> Got<Facts> {
        let path = format!("repos/{}", self.repo);
        let out = self
            .call("the repository", &["api", &path])
            .map_err(|e| self.outcome(&e))?;
        read_facts(&out, &self.repo).ok_or(Outcome::Unread(Unread::Malformed))
    }

    /// The branch's ref, one GraphQL question: see `activity.rs`.
    fn branch(&mut self, name: &Branch) -> Got<Option<CommitId>> {
        self.branch_at(name)
    }

    fn compare(&mut self, _: &CommitId, _: &CommitId) -> Got<Compared> {
        unsupported()
    }

    fn file(&mut self, _: &RepoPath, _: &Rev, _: u64) -> Got<Vec<u8>> {
        unsupported()
    }

    /// One GraphQL question a page of the repository's pull requests, oldest
    /// first, so a change opened while the pages are read lands on a later
    /// page and none moves past the cursor. With a since, newest updated
    /// first, to the first pull request updated before the latest since (a
    /// merge updates its pull request); the times must not rise down the
    /// list, the total must hold, and past one page the first is asked
    /// again and must hold no pull request the read has not seen. The states, the branch and the
    /// labels of the marks carried narrow the question; the author (by
    /// account id), the blueprints closed, the times and every mark carried
    /// narrow what it answers. A filter that names numbers is asked by
    /// number instead, the rest applied to what that answers: see
    /// `numbers.rs`. A mark no label can be is
    /// `Refused(Invalid)` before any request. Read to the end, or to
    /// one past the bound, or `PAGES_MAX` pages: past those, `Unread(Cut)`.
    /// In the order of their numbers. With bodies, each page's question asks
    /// the body too, no request more, 100 a page still: a body is read as
    /// `change` reads it.
    fn changes(&mut self, filter: &ChangeFilter, bound: Bound) -> Got<Whole<ChangeDigest>> {
        let labels = filter
            .carrying
            .iter()
            .map(|m| super::label_of(m).map(|l| Value::String(l).to_string()))
            .collect::<Got<Vec<String>>>()?;
        if !filter.numbers.is_empty() {
            return self.changes_by_number(filter, bound);
        }
        let me = matches!(filter.author, Some(Author::Me));
        let since = filter.merged_since.max(filter.updated_since);
        let query = format!(
            "query={}",
            changes_query(filter.states, me, &labels, since.is_some(), filter.bodies)
        );
        let (owner, name) = self.repo.split_once('/').unwrap_or((&self.repo, ""));
        let (owner, name) = (format!("owner={owner}"), format!("name={name}"));
        let head = filter.head.as_ref().map(|h| format!("head={}", h.as_str()));
        let ask = |gh: &mut Self, after: Option<&String>| -> Got<Page> {
            let from = after.map(|c| format!("after={c}"));
            let mut args = vec!["api", "graphql", "-f", &owner, "-f", &name];
            for var in [&head, &from].into_iter().flatten() {
                args.extend(["-f", var.as_str()]);
            }
            args.extend(["-f", &query]);
            match gh.call("the pull requests", &args) {
                Ok(out) => read_page(&out, me, filter, gh.bot.as_ref()),
                Err(e) => Err(gh.query_outcome(&e)),
            }
        };
        let mut taken = Vec::new();
        let mut numbers = BTreeSet::new();
        let mut cursors = BTreeSet::new();
        let mut after: Option<String> = None;
        let mut total = None;
        let mut last: Option<At> = None;
        for pages in 1.. {
            let page = ask(self, after.as_ref())?;
            if since.is_some() && *total.get_or_insert(page.total) != page.total {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            for at in &page.updated {
                if last.is_some_and(|l| *at > l) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                last = Some(*at);
            }
            for no in page.numbers {
                if !numbers.insert(no) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
            }
            for pull in page.pulls {
                let mut digest = pull.digest;
                digest.marks = self.marks_of(&pull.labels);
                if !filter.carrying.iter().all(|m| digest.marks.contains(m)) {
                    continue;
                }
                taken.push(digest);
                if taken.len() > bound.get() as usize {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
            let past = since.is_some_and(|s| last.is_some_and(|l| l < s));
            match page.next {
                None if numbers.len() as u64 == page.total => break,
                None => return Err(Outcome::Unread(Unread::Inconsistent)),
                Some(_) if past && pages == 1 => break,
                Some(_) if past && ask(self, None)?.numbers.iter().all(|n| numbers.contains(n)) => {
                    break
                }
                Some(_) if past => return Err(Outcome::Unread(Unread::Inconsistent)),
                Some(c) if !cursors.insert(c.clone()) => {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                Some(_) if pages == PAGES_MAX => return Err(Outcome::Unread(Unread::Cut)),
                Some(c) => after = Some(c),
            }
        }
        taken.sort_by_key(|d| d.no);
        Ok(Whole::new(taken))
    }

    /// The pull request read whole: see `change.rs`.
    fn change(&mut self, no: ChangeNo, bound: Bound) -> Got<Change> {
        self.change_whole(no, bound)
    }

    fn changed_files(&mut self, _: ChangeNo, _: Bound) -> Got<Whole<ChangedFile>> {
        unsupported()
    }

    /// The pull request's timeline, every kind, paged: see `activity.rs`.
    fn activity(&mut self, no: ChangeNo, window: Window) -> Got<Took<ChangeEvent>> {
        self.activity_of(no, window)
    }

    /// The commit's check runs and status contexts, whole up to `ALL_MAX`
    /// together: see `checks.rs`.
    fn checks(&mut self, at: &CommitId, name: Option<&str>) -> Got<Checks> {
        self.checks_of(at, name)
    }

    fn tags(&mut self, _: Bound) -> Got<Whole<Tag>> {
        unsupported()
    }

    fn recent_tags(&mut self, _: NonZeroU32) -> Got<Took<Tag>> {
        unsupported()
    }

    fn resolve_tag(&mut self, _: &TagName) -> Got<CommitId> {
        unsupported()
    }

    fn releases(&mut self, _: Bound) -> Got<Whole<Release>> {
        unsupported()
    }

    fn recent_releases(&mut self, _: NonZeroU32) -> Got<Took<Release>> {
        unsupported()
    }

    fn latest_release(&mut self) -> Got<Option<Release>> {
        unsupported()
    }

    fn release(&mut self, _: &TagName) -> Got<Release> {
        unsupported()
    }

    fn download(&mut self, _: &TagName, _: &FileName, _: u64, _: &mut dyn std::io::Write) -> Did {
        unsupported()
    }

    fn archive(&mut self, _: &CommitId, _: u64, _: &mut dyn std::io::Write) -> Did {
        unsupported()
    }

    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        unsupported()
    }

    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<ChangeDelta> {
        unsupported()
    }

    /// See `code_host/remote.rs`: no request, no credential.
    fn remote(&mut self, access: Access) -> Got<Remote> {
        self.git_remote(access)
    }
}

impl GitHub {
    /// The outcome of a GraphQL question that failed. gh exits 1 with no
    /// word of its own when the answer holds errors, as for a project that
    /// is gone or a token that may not read its pull requests. The REST read
    /// of the same list settles it: its `NotFound` or `Forbidden` answers,
    /// and anything else is `Unread(Inconsistent)`, a list with an error in
    /// it never answered as whole. A spent budget is the pool of priced
    /// reads.
    fn query_outcome(&mut self, e: &ForgeError) -> Outcome {
        if e.why.trim_start_matches(": ") == "gh exited 1" {
            let path = format!("repos/{}/pulls?per_page=1&state=all", self.repo);
            return match self.call("the pull requests", &["api", &path]) {
                Ok(_) => Outcome::Unread(Unread::Inconsistent),
                Err(e) => match self.outcome(&e) {
                    o @ (Outcome::NotFound | Outcome::Forbidden) => o,
                    _ => Outcome::Unread(Unread::Inconsistent),
                },
            };
        }
        match self.outcome(e) {
            Outcome::Spent { until, .. } => Outcome::Spent {
                pool: Pool::Priced,
                until,
            },
            o => o,
        }
    }

    /// The marks of a change's labels, in the site's order: a word of the
    /// table, or the project's own mark. A name no mark can have is left
    /// out and kept once for the caller to report, as a ticket's are.
    fn marks_of(&mut self, labels: &[String]) -> Vec<Mark> {
        let theirs = labels
            .iter()
            .filter(|l| crate::table::word(l).is_none())
            .cloned();
        let read = MarkName::read_list(theirs);
        for name in read.left_out {
            if !self.left_out.contains(&name) {
                self.left_out.push(name);
            }
        }
        labels
            .iter()
            .filter_map(|l| {
                crate::table::word(l).or_else(|| {
                    read.kept
                        .iter()
                        .find(|k| k.as_str() == l)
                        .cloned()
                        .map(Mark::Project)
                })
            })
            .collect()
    }
}

/// The facts of the repository's REST read. None for an answer that lacks
/// a field every repository has. An internal repository is not public. The
/// name is `repo`, the configured spelling, where GitHub's differs from it
/// in case alone, and GitHub's where the project moved.
fn read_facts(out: &str, repo: &str) -> Option<Facts> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let flag = |k: &str| v.get(k).and_then(Value::as_bool);
    let text = |k: &str| v.get(k).and_then(Value::as_str);
    let name = text("full_name")?;
    let name = if name.eq_ignore_ascii_case(repo) {
        repo
    } else {
        name
    };
    let mut facts = Facts::new(
        ProjectName::new(name),
        Branch::new(text("default_branch")?).ok()?,
    );
    facts.public = !flag("private")?;
    facts.tracker_on = flag("has_issues")?;
    facts.archived = flag("archived")?;
    Some(facts)
}

/// The GraphQL question of one page of pull requests in `states`, carrying
/// one of `labels` (each a GraphQL string) when given, with the account
/// asking when `me`. Newest updated first when `recent`, else oldest created first.
/// Each pull request's body too when `bodies`.
fn changes_query(
    states: ChangeStates,
    me: bool,
    labels: &[String],
    recent: bool,
    bodies: bool,
) -> String {
    let states = match states {
        ChangeStates::Open => "OPEN",
        ChangeStates::Merged => "MERGED",
        ChangeStates::Closed => "CLOSED",
        ChangeStates::All => "OPEN, CLOSED, MERGED",
    };
    let viewer = if me { "viewer { databaseId } " } else { "" };
    let labels = match labels.is_empty() {
        true => String::new(),
        false => format!(", labels: [{}]", labels.join(", ")),
    };
    let order = match recent {
        true => "UPDATED_AT, direction: DESC",
        false => "CREATED_AT, direction: ASC",
    };
    let fields = pull_fields(bodies);
    format!(
        "query($owner: String!, $name: String!, $head: String, $after: String) {{ {viewer}repository(owner: $owner, name: $name) {{ databaseId pullRequests(first: 100, after: $after, headRefName: $head{labels}, states: [{states}], orderBy: {{field: {order}}}) {{ totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ {fields} }} }} }} }}"
    )
}

/// What `PullNode::read` reads of a pull request, its body too when `bodies`:
/// the same of a page of the list and of a pull request asked by number.
fn pull_fields(bodies: bool) -> String {
    let account = "{ __typename login ... on User { databaseId } ... on Bot { databaseId } }";
    let body = if bodies { " body" } else { "" };
    format!(
        "number title state isDraft isCrossRepository headRepository {{ databaseId }} headRefName headRefOid baseRefName baseRefOid createdAt updatedAt mergedAt closedAt lastEditedAt additions deletions changedFiles mergeable mergeStateStatus url author {account} editor {account} mergedBy {account} mergeCommit {{ oid }}{body} labels(first: 100) {{ totalCount nodes {{ name }} }} closingIssuesReferences(first: 100) {{ totalCount nodes {{ number repository {{ databaseId nameWithOwner }} }} }}"
    )
}

/// The most pages of pull requests one `changes` reads: a thousand pull
/// requests. A list longer than that is `Unread(Cut)`, as a bound is.
const PAGES_MAX: u32 = 10;

/// One page of pull requests, read.
struct Page {
    /// How many pull requests the question takes, on every page.
    total: u64,
    /// The cursor of the next page, None on the last.
    next: Option<String>,
    /// The number of every pull request on the page, taken or not.
    numbers: Vec<ChangeNo>,
    /// The updated time of every pull request on the page, when the filter
    /// carries a since.
    updated: Vec<At>,
    /// The pull requests the filter takes, each read whole.
    pulls: Vec<Pull>,
}

/// One pull request, read: its digest with no marks yet, and its labels to
/// read them from.
struct Pull {
    digest: ChangeDigest,
    labels: Vec<String>,
}

/// What the filter needs of a pull request to decide, read before the rest
/// of it: the rest is read only of one the filter takes.
struct Lead<'a> {
    no: ChangeNo,
    state: &'a str,
    merged: Option<At>,
    author: Option<AccountId>,
    head: Option<&'a str>,
    updated: Option<At>,
    closes: Vec<Ref<BlueprintNo>>,
}

/// Whether the filter takes a pull request. The states and the branch
/// were asked of the site, and are held again here.
fn takes(f: &ChangeFilter, me: Option<&AccountId>, l: &Lead) -> bool {
    let state = matches!(
        (f.states, l.state),
        (ChangeStates::All, _)
            | (ChangeStates::Open, "OPEN")
            | (ChangeStates::Merged, "MERGED")
            | (ChangeStates::Closed, "CLOSED")
    );
    let author = match &f.author {
        None => true,
        Some(Author::Me) => me.is_some() && l.author.as_ref() == me,
        Some(Author::Account(id)) => l.author.as_ref() == Some(id),
    };
    state
        && author
        && f.merged_since
            .is_none_or(|since| l.merged.is_some_and(|at| at >= since))
        && f.head.as_ref().is_none_or(|h| l.head == Some(h.as_str()))
        && (f.closing.is_empty()
            || l.closes
                .iter()
                .any(|c| matches!(c, Ref::Here(n) if f.closing.contains(n))))
        && f.updated_since
            .is_none_or(|since| l.updated.is_some_and(|at| at >= since))
        && (f.numbers.is_empty() || f.numbers.contains(&l.no))
}

/// A page of the pull requests question. Each node's lead is read and the
/// filter decides; only a node it takes is read whole, so one it leaves
/// out never fails the list for a part it was not asked for. Every node
/// has a number, or the page is `Unread(Inconsistent)`. Read whole, an
/// answer that lacks a field every pull request has, or holds a value
/// outside its type, is `Unread(Malformed)`; one whose labels or closed
/// issues are more than one page holds is `Unread(Cut)`. With bodies, a
/// node with no body as text is `Unread(Malformed)`, never an empty body.
/// A page with
/// errors, or a page that says more follows and holds none, is
/// `Unread(Inconsistent)`. A bot's login is written as GraphQL writes it,
/// `name`, so one account has one login whichever read named it.
fn read_page(
    out: &str,
    me: bool,
    f: &ChangeFilter,
    bot: Option<&crate::tickets::Bot>,
) -> Got<Page> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    tehas_core::accounts::vouch(&mut v);
    if v.get("errors").is_some_and(|e| !e.is_null()) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let repo = v.pointer("/data/repository").ok_or(malformed.clone())?;
    let here = repo.get("databaseId").and_then(Value::as_u64);
    let list = repo.get("pullRequests").ok_or(malformed.clone())?;
    let total = list
        .get("totalCount")
        .and_then(Value::as_u64)
        .ok_or(malformed.clone())?;
    let more = list
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        .ok_or(malformed.clone())?;
    let next = match (more, list.pointer("/pageInfo/endCursor")) {
        (false, _) => None,
        (true, Some(Value::String(c))) => Some(c.clone()),
        (true, _) => return Err(malformed),
    };
    let viewer = match (me, v.pointer("/data/viewer/databaseId")) {
        (false, _) => None,
        (true, Some(id)) => Some(AccountId::new(
            id.as_u64().ok_or(malformed.clone())?.to_string(),
        )),
        (true, None) => return Err(malformed),
    };
    let nodes = list
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(malformed.clone())?;
    if more && nodes.is_empty() {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let mut numbers = Vec::with_capacity(nodes.len());
    let mut updated = Vec::new();
    let mut pulls = Vec::new();
    for n in nodes {
        let read = PullNode { node: n }.read(here, viewer.as_ref(), f, bot)?;
        numbers.push(read.no);
        updated.extend(read.updated);
        pulls.extend(read.pull);
    }
    Ok(Page {
        total,
        next,
        numbers,
        updated,
        pulls,
    })
}

/// One pull request as the list or the question by number reads it:
/// its number, its updated time when the filter carries a since, and,
/// when the filter takes it, the pull request read whole.
struct Read {
    no: ChangeNo,
    updated: Option<At>,
    pull: Option<Pull>,
}

/// A node of the pull requests question, or of the question by number,
/// as GitHub answered it.
struct PullNode<'a> {
    node: &'a Value,
}

impl PullNode<'_> {
    /// The node, `here` the repository's id and `viewer` the account
    /// asking: its lead is read and the filter decides; only a node it
    /// takes is read whole, as `read_page` says.
    fn read(
        &self,
        here: Option<u64>,
        viewer: Option<&AccountId>,
        f: &ChangeFilter,
        bot: Option<&crate::tickets::Bot>,
    ) -> Got<Read> {
        let n = self.node;
        let malformed = Outcome::Unread(Unread::Malformed);
        let time = |x: Option<&Value>| -> Got<Option<At>> {
            match x {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => tehas_core::time::from_iso(s)
                    .map(At::new)
                    .map(Some)
                    .ok_or(malformed.clone()),
                Some(_) => Err(malformed.clone()),
            }
        };
        let account = |x: Option<&Value>| -> Got<Option<Account>> {
            let o = match x {
                None | Some(Value::Null) => return Ok(None),
                Some(Value::Object(o)) => o,
                Some(_) => return Err(malformed.clone()),
            };
            let login = o
                .get("login")
                .and_then(Value::as_str)
                .ok_or(malformed.clone())?;
            let kind = o.get("__typename").and_then(Value::as_str);
            let id = match o.get("databaseId") {
                None | Some(Value::Null) => None,
                Some(i) => Some(i.as_u64().ok_or(malformed.clone())?),
            };
            let login = Login::new(tehas_core::accounts::folded(tehas_core::accounts::account(
                login,
            )));
            let facts = crate::gh::Facts {
                id,
                kind: kind.map(str::to_string),
            };
            let kind = match kind {
                _ if bot.is_some_and(|b| b.is(&facts)) => AccountKind::Ours,
                Some("User") => AccountKind::Person,
                Some("Bot") => AccountKind::Bot,
                _ => AccountKind::Unknown,
            };
            let mut account = Account::new(Realm::CodeHost, login, kind);
            account.id = id.map(|i| AccountId::new(i.to_string()));
            Ok(Some(account))
        };
        let text = |k: &str| n.get(k).and_then(Value::as_str).ok_or(malformed.clone());
        let flag = |k: &str| n.get(k).and_then(Value::as_bool).ok_or(malformed.clone());
        let at = |k: &str| time(n.get(k));
        let who = |k: &str| account(n.get(k));
        let count = |k: &str| {
            n.get(k)
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(malformed.clone())
        };
        let whole = |k: &str| -> Got<Vec<&Value>> {
            let part = n.get(k).ok_or(malformed.clone())?;
            let nodes = part
                .get("nodes")
                .and_then(Value::as_array)
                .ok_or(malformed.clone())?;
            match part.get("totalCount").and_then(Value::as_u64) {
                Some(t) if t == nodes.len() as u64 => Ok(nodes.iter().collect()),
                Some(t) if t > nodes.len() as u64 => Err(Outcome::Unread(Unread::Cut)),
                _ => Err(Outcome::Unread(Unread::Inconsistent)),
            }
        };
        let closes = || -> Got<Vec<Ref<BlueprintNo>>> {
            let mut closes = Vec::new();
            for issue in whole("closingIssuesReferences")? {
                let number = issue
                    .get("number")
                    .and_then(Value::as_u64)
                    .and_then(BlueprintNo::new)
                    .ok_or(malformed.clone())?;
                let repo = issue.get("repository").ok_or(malformed.clone())?;
                let id = repo.get("databaseId").and_then(Value::as_u64);
                closes.push(match (id, here) {
                    (Some(id), Some(here)) if id == here => Ref::Here(number),
                    (Some(_), Some(_)) => Ref::Elsewhere {
                        project: ProjectName::new(
                            repo.get("nameWithOwner")
                                .and_then(Value::as_str)
                                .ok_or(malformed.clone())?,
                        ),
                        item: ItemName::new(number.get().to_string()),
                    },
                    _ => return Err(malformed.clone()),
                });
            }
            Ok(closes)
        };

        // The lead: the number, the state, and what the filter asks.
        let no = n
            .get("number")
            .and_then(Value::as_u64)
            .and_then(ChangeNo::new)
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        let state = text("state")?;
        if !matches!(state, "OPEN" | "MERGED" | "CLOSED") {
            return Err(malformed);
        }
        let lead = Lead {
            no,
            state,
            merged: match (&f.merged_since, state) {
                (Some(_), "MERGED") => Some(at("mergedAt")?.ok_or(malformed.clone())?),
                _ => None,
            },
            author: match f.author {
                Some(_) => who("author")?.and_then(|a| a.id),
                None => None,
            },
            head: match f.head {
                Some(_) => Some(text("headRefName")?),
                None => None,
            },
            updated: match f.updated_since.or(f.merged_since) {
                Some(_) => Some(at("updatedAt")?.ok_or(malformed.clone())?),
                None => None,
            },
            closes: if f.closing.is_empty() {
                Vec::new()
            } else {
                closes()?
            },
        };
        let updated = lead.updated;
        if !takes(f, viewer, &lead) {
            return Ok(Read {
                no,
                updated,
                pull: None,
            });
        }

        // Taken: the rest, whole.
        let state = match state {
            "OPEN" => ChangeState::Open,
            "MERGED" => ChangeState::Merged {
                at: at("mergedAt")?.ok_or(malformed.clone())?,
                by: who("mergedBy")?,
                commit: match n.get("mergeCommit") {
                    Some(Value::Null) => None,
                    Some(c) => Some(
                        c.get("oid")
                            .and_then(Value::as_str)
                            .and_then(|c| CommitId::new(c).ok())
                            .ok_or(malformed.clone())?,
                    ),
                    None => return Err(malformed),
                },
            },
            _ => ChangeState::Closed {
                at: at("closedAt")?.ok_or(malformed.clone())?,
            },
        };
        let merge_state = mergeable(text("mergeable")?, text("mergeStateStatus")?);
        let labels = whole("labels")?
            .into_iter()
            .map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or(malformed.clone())?;
        let edited = at("lastEditedAt")?;
        let mut digest = ChangeDigest::new(
            no,
            state,
            Branch::new(text("headRefName")?).map_err(|_| malformed.clone())?,
            CommitId::new(text("headRefOid")?).map_err(|_| malformed.clone())?,
            Branch::new(text("baseRefName")?).map_err(|_| malformed.clone())?,
            CommitId::new(text("baseRefOid")?).map_err(|_| malformed.clone())?,
            at("createdAt")?.ok_or(malformed.clone())?,
            at("updatedAt")?.ok_or(malformed.clone())?,
            WebLink::new(text("url")?).map_err(|_| malformed.clone())?,
        );
        digest.title = text("title")?.to_string();
        digest.author = who("author")?;
        // From a fork unless GitHub says it is not and its head repository is
        // this one: a deleted head repository, or one GitHub does not name,
        // fails closed.
        let head_here = match n.get("headRepository") {
            None => return Err(malformed.clone()),
            Some(Value::Null) => false,
            Some(r) => {
                let id = r.get("databaseId").and_then(Value::as_u64);
                id.ok_or(malformed.clone())?;
                here.is_some() && id == here
            }
        };
        digest.from_fork = flag("isCrossRepository")? || !head_here;
        digest.draft = flag("isDraft")?;
        digest.merge_state = merge_state;
        digest.closes = closes()?;
        let mut size = Size::default();
        size.files = count("changedFiles")?;
        size.added = count("additions")?;
        size.deleted = count("deletions")?;
        digest.size = Some(size);
        digest.edited = edited;
        digest.editor = match edited {
            Some(_) => who("editor")?,
            None => None,
        };
        if f.bodies {
            let raw = n
                .get("body")
                .and_then(Value::as_str)
                .ok_or(malformed.clone())?;
            let item = BlueprintNo::new(no.get()).ok_or(malformed.clone())?;
            let read = markers::read(raw, Some(item));
            digest.says_closes = change::says_closes(&read.text);
            digest.body = Some(read.text);
        }
        Ok(Read {
            no,
            updated,
            pull: Some(Pull { digest, labels }),
        })
    }
}
