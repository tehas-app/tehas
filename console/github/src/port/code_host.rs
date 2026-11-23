//! The code host port, `CodeHost` and `CodeHostWrites`: changes, branches,
//! tags and releases. Answered: what the code host says of the project
//! (`facts`), the changes a filter takes without their bodies (`changes`),
//! and the check runs on a commit (`checks`). Every other read, and a
//! filter that asks for bodies, answers `Refused(Unsupported)` before any
//! request: a body is read without our markers and closing lines, which no
//! reader of this adapter takes out yet.

use std::collections::BTreeSet;

use super::unsupported;
use crate::forge::uri;
use crate::{ForgeError, GitHub};
use serde_json::Value;
use tehas_ports::{
    Access, Account, AccountId, AccountKind, At, Author, BlueprintNo, Bound, Branch, Change,
    ChangeDelta, ChangeDigest, ChangeEvent, ChangeFilter, ChangeNo, ChangeState, ChangeStates,
    ChangeToken, ChangedFile, Check, CheckStatus, Checks, CodeHost, CommitId, Compared, Conclusion,
    Cursor, Did, Facts, FileName, Got, ItemName, Login, Mark, MarkName, Mergeable, NotMergeable,
    Outcome, Overall, Pool, ProjectName, Realm, Ref, Release, Remote, RepoPath, Rev, Size, Tag,
    TagName, Took, Unread, WebLink, Whole, Window, ALL_MAX,
};

impl CodeHost for GitHub {
    /// The repository's REST read.
    fn facts(&mut self) -> Got<Facts> {
        let path = format!("repos/{}", self.repo);
        let out = self
            .call("the repository", &["api", &path])
            .map_err(|e| self.outcome(&e))?;
        read_facts(&out).ok_or(Outcome::Unread(Unread::Malformed))
    }

    fn branch(&mut self, _: &Branch) -> Got<Option<CommitId>> {
        unsupported()
    }

    fn compare(&mut self, _: &CommitId, _: &CommitId) -> Got<Compared> {
        unsupported()
    }

    fn file(&mut self, _: &RepoPath, _: &Rev, _: u64) -> Got<Vec<u8>> {
        unsupported()
    }

    /// One GraphQL question a page of the repository's pull requests, oldest
    /// first, so a change opened while the pages are read lands on a later
    /// page and none moves past the cursor. The states and the branch narrow
    /// the question; the author (by account id), the blueprints closed, the
    /// numbers and the times narrow what it answers. Read to the end, or to
    /// one past the bound. In the order of their numbers.
    fn changes(&mut self, filter: &ChangeFilter, bound: Bound) -> Got<Whole<ChangeDigest>> {
        if filter.bodies {
            return unsupported();
        }
        let me = matches!(filter.author, Some(Author::Me));
        let query = format!("query={}", changes_query(filter.states, me));
        let (owner, name) = self.repo.split_once('/').unwrap_or((&self.repo, ""));
        let (owner, name) = (format!("owner={owner}"), format!("name={name}"));
        let head = filter.head.as_ref().map(|h| format!("head={}", h.as_str()));
        let mut taken = Vec::new();
        let mut numbers = BTreeSet::new();
        let mut cursors = BTreeSet::new();
        let mut after: Option<String> = None;
        loop {
            let from = after.as_ref().map(|c| format!("after={c}"));
            let mut args = vec!["api", "graphql", "-f", &owner, "-f", &name];
            for var in [&head, &from].into_iter().flatten() {
                args.extend(["-f", var.as_str()]);
            }
            args.extend(["-f", &query]);
            let out = self
                .call("the pull requests", &args)
                .map_err(|e| self.query_outcome(&e))?;
            let page = read_page(&out, me)?;
            for pull in page.pulls {
                if !numbers.insert(pull.digest.no) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                if !takes(filter, page.me.as_ref(), &pull) {
                    continue;
                }
                let mut digest = pull.digest;
                digest.marks = self.marks_of(&pull.labels);
                taken.push(digest);
                if taken.len() > bound.get() as usize {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
            match page.next {
                None if numbers.len() as u64 == page.total => break,
                None => return Err(Outcome::Unread(Unread::Inconsistent)),
                Some(c) if !cursors.insert(c.clone()) => {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                Some(c) => after = Some(c),
            }
        }
        taken.sort_by_key(|d| d.no);
        Ok(Whole::new(taken))
    }

    fn change(&mut self, _: ChangeNo, _: Bound) -> Got<Change> {
        unsupported()
    }

    fn changed_files(&mut self, _: ChangeNo, _: Bound) -> Got<Whole<ChangedFile>> {
        unsupported()
    }

    fn activity(&mut self, _: ChangeNo, _: Window) -> Got<Took<ChangeEvent>> {
        unsupported()
    }

    /// The commit's latest check runs, REST, a page of 100 at a time, read
    /// whole up to `ALL_MAX`. The overall is `Fail` when a run failed, was
    /// cancelled or ended in a conclusion this adapter does not fold,
    /// `Pending` while one has not finished, `Pass` when every run passed or
    /// was skipped, and `None` with no run.
    fn checks(&mut self, at: &CommitId, name: Option<&str>) -> Got<Checks> {
        let named = name.map(|n| format!("&check_name={}", uri(n)));
        let mut runs = Vec::new();
        let mut ids = BTreeSet::new();
        let mut total = None;
        for page in 1u32.. {
            let path = format!(
                "repos/{}/commits/{}/check-runs?per_page=100&page={page}{}",
                self.repo,
                at.as_str(),
                named.as_deref().unwrap_or("")
            );
            let out = self
                .call("the check runs", &["api", &path])
                .map_err(|e| self.checks_outcome(&e))?;
            let (count, read) = read_runs(&out).ok_or(Outcome::Unread(Unread::Malformed))?;
            if count > u64::from(ALL_MAX.get()) {
                return Err(Outcome::Unread(Unread::Cut));
            }
            if *total.get_or_insert(count) != count {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            let empty = read.is_empty();
            for (id, run) in read {
                if !ids.insert(id) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                runs.push(run);
            }
            let held = runs.len() as u64;
            if held == count {
                break;
            }
            if held > count || empty {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
        }
        let overall = overall(&runs);
        Ok(Checks { runs, overall })
    }

    fn tags(&mut self, _: Bound) -> Got<Whole<Tag>> {
        unsupported()
    }

    fn resolve_tag(&mut self, _: &TagName) -> Got<CommitId> {
        unsupported()
    }

    fn releases(&mut self, _: Bound) -> Got<Whole<Release>> {
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

    fn remote(&mut self, _: Access) -> Got<Remote> {
        unsupported()
    }
}

impl GitHub {
    /// The outcome of a GraphQL question that failed. gh exits 1 with no
    /// word of its own when the answer holds errors, so that is
    /// `Unread(Inconsistent)`: a list with an error in it is never answered
    /// as whole. A spent budget is the pool of priced reads.
    fn query_outcome(&self, e: &ForgeError) -> Outcome {
        if e.why.trim_start_matches(": ") == "gh exited 1" {
            return Outcome::Unread(Unread::Inconsistent);
        }
        match self.outcome(e) {
            Outcome::Spent { until, .. } => Outcome::Spent {
                pool: Pool::Priced,
                until,
            },
            o => o,
        }
    }

    /// The outcome of a check runs read that failed. GitHub answers 422 for
    /// a commit the repository does not hold.
    fn checks_outcome(&self, e: &ForgeError) -> Outcome {
        if e.why.trim_start_matches(": ").starts_with("HTTP 422") {
            return Outcome::NotFound;
        }
        self.outcome(e)
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
/// a field every repository has. An internal repository is not public.
fn read_facts(out: &str) -> Option<Facts> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let flag = |k: &str| v.get(k).and_then(Value::as_bool);
    let text = |k: &str| v.get(k).and_then(Value::as_str);
    Some(Facts {
        name: ProjectName::new(text("full_name")?),
        public: !flag("private")?,
        tracker_on: flag("has_issues")?,
        archived: flag("archived")?,
        default_branch: Branch::new(text("default_branch")?).ok()?,
    })
}

/// The GraphQL question of one page of pull requests in `states`, with the
/// account asking when `me`.
fn changes_query(states: ChangeStates, me: bool) -> String {
    let states = match states {
        ChangeStates::Open => "OPEN",
        ChangeStates::Merged => "MERGED",
        ChangeStates::Closed => "CLOSED",
        ChangeStates::All => "OPEN, CLOSED, MERGED",
    };
    let viewer = if me { "viewer { databaseId } " } else { "" };
    let account = "{ __typename login ... on User { databaseId } ... on Bot { databaseId } }";
    format!(
        "query($owner: String!, $name: String!, $head: String, $after: String) {{ {viewer}repository(owner: $owner, name: $name) {{ databaseId pullRequests(first: 100, after: $after, headRefName: $head, states: [{states}], orderBy: {{field: CREATED_AT, direction: ASC}}) {{ totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ number title state isDraft isCrossRepository headRefName headRefOid baseRefName createdAt updatedAt mergedAt closedAt lastEditedAt additions deletions changedFiles mergeable mergeStateStatus url author {account} editor {account} mergedBy {account} mergeCommit {{ oid }} labels(first: 100) {{ totalCount nodes {{ name }} }} closingIssuesReferences(first: 100) {{ totalCount nodes {{ number repository {{ databaseId nameWithOwner }} }} }} }} }} }} }}"
    )
}

/// One page of pull requests, read.
struct Page {
    /// How many pull requests the question takes, on every page.
    total: u64,
    /// The cursor of the next page, None on the last.
    next: Option<String>,
    /// The id of the account asking, when the question named it.
    me: Option<AccountId>,
    pulls: Vec<Pull>,
}

/// One pull request, read: its digest with no marks yet, its labels to read
/// them from, and when the site last saw it change.
struct Pull {
    digest: ChangeDigest,
    labels: Vec<String>,
    updated: At,
}

/// Whether the filter takes a pull request. The states and the branch
/// were asked of the site, and are held again here.
fn takes(f: &ChangeFilter, me: Option<&AccountId>, p: &Pull) -> bool {
    let d = &p.digest;
    let state = matches!(
        (f.states, &d.state),
        (ChangeStates::All, _)
            | (ChangeStates::Open, ChangeState::Open)
            | (ChangeStates::Merged, ChangeState::Merged { .. })
            | (ChangeStates::Closed, ChangeState::Closed { .. })
    );
    let by = d.author.as_ref().and_then(|a| a.id.as_ref());
    let author = match &f.author {
        None => true,
        Some(Author::Me) => me.is_some() && by == me,
        Some(Author::Account(id)) => by == Some(id),
    };
    let merged = f
        .merged_since
        .is_none_or(|since| matches!(d.state, ChangeState::Merged { at, .. } if at >= since));
    state
        && author
        && merged
        && f.head.as_ref().is_none_or(|h| d.head == *h)
        && (f.closing.is_empty()
            || d.closes
                .iter()
                .any(|c| matches!(c, Ref::Here(n) if f.closing.contains(n))))
        && f.updated_since.is_none_or(|since| p.updated >= since)
        && (f.numbers.is_empty() || f.numbers.contains(&d.no))
}

/// A page of the pull requests question. An answer that lacks a field
/// every pull request has, or holds a value outside its type, is
/// `Unread(Malformed)`; one whose labels or closed issues are more than
/// one page holds is `Unread(Cut)`; a node with no number, a page with
/// errors, or a page that says more follows and holds none is
/// `Unread(Inconsistent)`. A bot's login is written as REST
/// writes it, `name[bot]`, so one account has one login whichever read
/// named it.
fn read_page(out: &str, me: bool) -> Got<Page> {
    let malformed = Outcome::Unread(Unread::Malformed);
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    tehas_core::accounts::vouch(&mut v);
    if v.get("errors").is_some_and(|e| !e.is_null()) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let time = |x: Option<&Value>| -> Result<Option<At>, ()> {
        match x {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => tehas_core::time::from_iso(s)
                .map(At::new)
                .map(Some)
                .ok_or(()),
            Some(_) => Err(()),
        }
    };
    let account = |x: Option<&Value>| -> Result<Option<Account>, ()> {
        let o = match x {
            None | Some(Value::Null) => return Ok(None),
            Some(Value::Object(o)) => o,
            Some(_) => return Err(()),
        };
        let login = o.get("login").and_then(Value::as_str).ok_or(())?;
        let kind = o.get("__typename").and_then(Value::as_str);
        let id = match o.get("databaseId") {
            None | Some(Value::Null) => None,
            Some(i) => Some(i.as_u64().ok_or(())?),
        };
        let login = match kind {
            Some("Bot") if !login.ends_with("[bot]") => format!("{login}[bot]"),
            _ => login.to_string(),
        };
        Ok(Some(Account {
            realm: Realm::CodeHost,
            login: Login::new(tehas_core::accounts::folded(&login)),
            id: id.map(|i| AccountId::new(i.to_string())),
            kind: match kind {
                Some("User") => AccountKind::Person,
                Some("Bot") => AccountKind::Bot,
                _ => AccountKind::Unknown,
            },
        }))
    };
    let count = |x: Option<&Value>| {
        x.and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
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
    let me = match (me, v.pointer("/data/viewer/databaseId")) {
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
    let mut pulls = Vec::with_capacity(nodes.len());
    for n in nodes {
        let text = |k: &str| n.get(k).and_then(Value::as_str).ok_or(malformed.clone());
        let flag = |k: &str| n.get(k).and_then(Value::as_bool).ok_or(malformed.clone());
        let at = |k: &str| time(n.get(k)).map_err(|_| malformed.clone());
        let who = |k: &str| account(n.get(k)).map_err(|_| malformed.clone());
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
        let no = n
            .get("number")
            .and_then(Value::as_u64)
            .and_then(ChangeNo::new)
            .ok_or(Outcome::Unread(Unread::Inconsistent))?;
        let state = match text("state")? {
            "OPEN" => ChangeState::Open,
            "MERGED" => ChangeState::Merged {
                at: at("mergedAt")?.ok_or(malformed.clone())?,
                by: who("mergedBy")?,
                commit: n
                    .pointer("/mergeCommit/oid")
                    .and_then(Value::as_str)
                    .and_then(|c| CommitId::new(c).ok())
                    .ok_or(malformed.clone())?,
            },
            "CLOSED" => ChangeState::Closed {
                at: at("closedAt")?.ok_or(malformed.clone())?,
            },
            _ => return Err(malformed),
        };
        let merge_state = match (text("mergeable")?, text("mergeStateStatus")?) {
            ("CONFLICTING", _) | ("MERGEABLE", "DIRTY") => Mergeable::No(NotMergeable::Conflict),
            ("UNKNOWN", _) | ("MERGEABLE", "UNKNOWN") => Mergeable::Unknown,
            ("MERGEABLE", "CLEAN" | "HAS_HOOKS" | "UNSTABLE") => Mergeable::Yes,
            ("MERGEABLE", "BEHIND") => Mergeable::No(NotMergeable::Behind),
            ("MERGEABLE", "BLOCKED") => Mergeable::No(NotMergeable::Blocked),
            ("MERGEABLE", "DRAFT") => Mergeable::No(NotMergeable::Draft),
            _ => Mergeable::No(NotMergeable::Unknown),
        };
        let labels = whole("labels")?
            .into_iter()
            .map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or(malformed.clone())?;
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
                _ => return Err(malformed),
            });
        }
        let edited = at("lastEditedAt")?;
        let digest = ChangeDigest {
            no,
            title: text("title")?.to_string(),
            state,
            head: Branch::new(text("headRefName")?).map_err(|_| malformed.clone())?,
            head_commit: CommitId::new(text("headRefOid")?).map_err(|_| malformed.clone())?,
            base: Branch::new(text("baseRefName")?).map_err(|_| malformed.clone())?,
            author: who("author")?,
            from_fork: flag("isCrossRepository")?,
            draft: flag("isDraft")?,
            merge_state,
            marks: Vec::new(),
            closes,
            size: Some(Size {
                files: count(n.get("changedFiles")).ok_or(malformed.clone())?,
                added: count(n.get("additions")).ok_or(malformed.clone())?,
                deleted: count(n.get("deletions")).ok_or(malformed.clone())?,
            }),
            body: None,
            created: at("createdAt")?.ok_or(malformed.clone())?,
            edited,
            editor: match edited {
                Some(_) => who("editor")?,
                None => None,
            },
            link: WebLink::new(text("url")?).map_err(|_| malformed.clone())?,
        };
        pulls.push(Pull {
            digest,
            labels,
            updated: at("updatedAt")?.ok_or(malformed.clone())?,
        });
    }
    Ok(Page {
        total,
        next,
        me,
        pulls,
    })
}

/// A page of the check runs read: the count the site says it holds, and
/// each run with its id. None for an answer that lacks a field every run
/// has. A conclusion is folded into the port's words: `neutral` is a skip,
/// `timed_out` and `startup_failure` failures, and any other word, a
/// finished run with none among them, `Unknown`. A status that is not
/// `in_progress` or `completed` is queued.
fn read_runs(out: &str) -> Option<(u64, Vec<(u64, Check)>)> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let count = v.get("total_count")?.as_u64()?;
    let mut runs = Vec::new();
    for r in v.get("check_runs")?.as_array()? {
        let text = |k: &str| r.get(k).and_then(Value::as_str);
        let status = match text("status")? {
            "completed" => CheckStatus::Done(match text("conclusion") {
                Some("success") => Conclusion::Passed,
                Some("skipped" | "neutral") => Conclusion::Skipped,
                Some("failure" | "timed_out" | "startup_failure") => Conclusion::Failed,
                Some("cancelled") => Conclusion::Cancelled,
                _ => Conclusion::Unknown,
            }),
            "in_progress" => CheckStatus::Running,
            _ => CheckStatus::Queued,
        };
        let check = Check {
            name: text("name")?.to_string(),
            status,
        };
        runs.push((r.get("id")?.as_u64()?, check));
    }
    Some((count, runs))
}

/// What the runs add up to. Never `Pass` but when every run passed or was
/// skipped.
fn overall(runs: &[Check]) -> Overall {
    let failed = runs.iter().any(|r| {
        matches!(
            r.status,
            CheckStatus::Done(Conclusion::Failed | Conclusion::Cancelled | Conclusion::Unknown)
        )
    });
    let pending = runs
        .iter()
        .any(|r| !matches!(r.status, CheckStatus::Done(_)));
    if runs.is_empty() {
        Overall::None
    } else if failed {
        Overall::Fail
    } else if pending {
        Overall::Pending
    } else {
        Overall::Pass
    }
}
