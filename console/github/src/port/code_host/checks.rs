//! `CodeHost::checks`: a commit's check runs, the newest of each App,
//! workflow and name for GitHub Actions and of each App, check suite and
//! name for any other (Decided 129, as amended 2026-11-25), then its status
//! contexts, each one's newest state held as a run named by its context,
//! and `Overall::of` all of them. A failure of any read fails the whole:
//! the runs alone are never an answer. `CodeHost::branch_checks`: the
//! branch's head and its checks of a name in one GraphQL question, folded
//! the same way.

use std::collections::{BTreeMap, BTreeSet};

use crate::forge::uri;
use crate::{graphql, ForgeError, GitHub};
use serde::Deserialize;
use serde_json::Value;
use tehas_ports::{
    Branch, Check, CheckStatus, Checks, CommitId, Conclusion, Got, Outcome, Overall, Unread,
    ALL_MAX,
};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);

/// One page of a list the site counts: its count, and each item with the
/// key that names it once in the list.
type Counted<K, T = Check> = (u64, Vec<(K, T)>);

/// The line [`GitHub::take_lines`] gives where the site refuses a checks
/// read its workflow runs: an App made before the read needs
/// `actions: read`.
pub const WORKFLOW_RUNS_FORBIDDEN: &str = "the App cannot read workflow runs (actions: read)";

/// The slug of GitHub Actions' App, the one App whose check suites are
/// workflow runs. The same on GitHub Enterprise Server, where the App's id
/// differs: so the App is known by its slug, never by its id.
const ACTIONS: &str = "github-actions";

/// What names a check run: its id, the App that made it, whether that App
/// is GitHub Actions, its check suite (for GitHub Actions, one workflow
/// run) and when it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Run {
    id: u64,
    app: u64,
    actions: bool,
    suite: u64,
    started: Started,
}

/// When a check run started, as the site says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Started {
    /// At this time, in Unix seconds.
    At(i64),
    /// Not yet: no `started_at`, or null, as a queued run (a fresh re-run)
    /// has.
    Not,
    /// A `started_at` that is not a time of the form GitHub writes.
    Bad,
}

/// The workflow a suite of GitHub Actions runs, and the event that ran it.
type Ran = (u64, String);

/// What a check run is folded by. A run of GitHub Actions whose suite the
/// commit's workflow runs map is held by its workflow and event, so a
/// newer run of the same workflow and event, scheduled or re-run,
/// supersedes; any other by its suite, as Decided 129 has it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Fold {
    Workflow {
        app: u64,
        workflow: u64,
        event: String,
        name: String,
    },
    Suite {
        app: u64,
        suite: u64,
        name: String,
    },
}

/// The one question of `branch_checks_of`: the branch's ref, exactly, as
/// `BRANCH` asks it, and on its commit the check suites with a run of the
/// name, each with its App, its workflow run and every run of the name
/// (`checkType: ALL`, as REST's `filter=all`), and the combined status.
const BRANCH_CHECKS: &str = "query($owner: String!, $name: String!, $ref: String!, $check: String!) { repository(owner: $owner, name: $name) { ref(qualifiedName: $ref) { name prefix target { __typename oid ... on Commit { checkSuites(first: 100, filterBy: {checkName: $check}) { totalCount nodes { databaseId app { databaseId slug } workflowRun { event workflow { databaseId } } checkRuns(first: 100, filterBy: {checkName: $check, checkType: ALL}) { totalCount nodes { databaseId name status conclusion startedAt } } } } status { contexts { context state } } } } } } }";

/// The most of a list the one question takes as whole: a list this long
/// may hold more than it gives, so the read is asked the long way.
const ONE_PAGE: usize = 100;

/// What the one question answers: the head and its checks, or that the
/// answer is no whole one and the long way is asked.
enum Asked {
    Whole(Option<(CommitId, Checks)>),
    Long,
}

#[derive(Deserialize)]
struct Data {
    repository: Option<Repo>,
}

#[derive(Deserialize)]
struct Repo {
    #[serde(rename = "ref")]
    at: Option<Ref>,
}

#[derive(Deserialize)]
struct Ref {
    name: Option<String>,
    prefix: Option<String>,
    target: Option<Target>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    #[serde(rename = "__typename")]
    is: Option<String>,
    oid: Option<String>,
    check_suites: Option<Conn<Suite>>,
    status: Option<Combined>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Conn<T> {
    total_count: Option<u64>,
    nodes: Option<Vec<Option<T>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Suite {
    database_id: Option<u64>,
    app: Option<App>,
    workflow_run: Option<WorkflowRun>,
    check_runs: Option<Conn<RunNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct App {
    database_id: Option<u64>,
    slug: Option<String>,
}

#[derive(Deserialize)]
struct WorkflowRun {
    event: Option<String>,
    workflow: Option<Workflow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Workflow {
    database_id: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunNode {
    database_id: Option<u64>,
    name: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    started_at: Option<Value>,
}

#[derive(Deserialize)]
struct Combined {
    contexts: Option<Vec<Option<Context>>>,
}

#[derive(Deserialize)]
struct Context {
    context: Option<String>,
    state: Option<String>,
}

impl GitHub {
    /// `CodeHost::branch_checks` asked by name: one GraphQL question for
    /// the branch's head and its checks of the name, folded as `checks_of`
    /// folds them. An answer that may not be whole (a list of
    /// `ONE_PAGE` or more, or a count past what it gives) or that GitHub
    /// answers with errors, as where the App may not read the workflow
    /// runs, is asked the long way: `branch_at`, then `checks_of`, whose
    /// outcomes and lines it then gives. A rate-limited answer is the
    /// priced budget spent; a call that fails without an answer fails as
    /// `branch_at`'s does. Asked for every check, the long way.
    pub(super) fn branch_checks_of(
        &mut self,
        branch: &Branch,
        name: Option<&str>,
    ) -> Got<Option<(CommitId, Checks)>> {
        let Some(check) = name else {
            return self.branch_checks_long(branch, None);
        };
        match self.branch_checks_asked(branch, check)? {
            Asked::Whole(got) => Ok(got),
            Asked::Long => self.branch_checks_long(branch, name),
        }
    }

    fn branch_checks_long(
        &mut self,
        branch: &Branch,
        name: Option<&str>,
    ) -> Got<Option<(CommitId, Checks)>> {
        let Some(head) = self.branch_at(branch)? else {
            return Ok(None);
        };
        let checks = self.checks_of(&head, name)?;
        Ok(Some((head, checks)))
    }

    fn branch_checks_asked(&mut self, branch: &Branch, check: &str) -> Got<Asked> {
        let (owner, project) = self.owner_name();
        let full = format!("ref=refs/heads/{}", branch.as_str());
        let named = format!("check={check}");
        let query = format!("query={BRANCH_CHECKS}");
        let args = [
            "api", "graphql", "-F", &owner, "-F", &project, "-f", &full, "-f", &named, "-f", &query,
        ];
        let now = (self.now)();
        let out = match self.call_answer("the branch's checks", &args) {
            Ok(out) => out,
            Err((e, said)) => {
                return match said.as_deref().and_then(|s| graphql::refusal(s, now)) {
                    Some(spent @ Outcome::Spent { .. }) => Err(spent),
                    Some(_) => Ok(Asked::Long),
                    None => {
                        let path = format!("repos/{}", self.repo);
                        Err(self.question_outcome(&e, said.as_deref(), &path))
                    }
                };
            }
        };
        match graphql::refusal(&out, now) {
            Some(spent @ Outcome::Spent { .. }) => return Err(spent),
            Some(_) => return Ok(Asked::Long),
            None => {}
        }
        let data: Data = graphql::part(&out, "/data", now)?;
        read_branch_checks(data, branch, check)
    }
}

/// The one question's answer, read: see `branch_checks_of`.
fn read_branch_checks(data: Data, branch: &Branch, check: &str) -> Got<Asked> {
    let Some(at) = data.repository.ok_or(Outcome::NotFound)?.at else {
        return Ok(Asked::Whole(None));
    };
    if at.name.as_deref() != Some(branch.as_str()) || at.prefix.as_deref() != Some("refs/heads/") {
        return Err(INCONSISTENT);
    }
    let target = at.target.ok_or(MALFORMED)?;
    if target.is.as_deref() != Some("Commit") {
        return Err(MALFORMED);
    }
    let head = CommitId::new(target.oid.ok_or(MALFORMED)?).map_err(|_| MALFORMED)?;
    let Some(suites) = whole(target.check_suites.ok_or(MALFORMED)?)? else {
        return Ok(Asked::Long);
    };
    let contexts = match target.status {
        None => Vec::new(),
        Some(c) => c.contexts.ok_or(MALFORMED)?,
    };
    if contexts.len() >= ONE_PAGE {
        return Ok(Asked::Long);
    }
    let mut runs = Vec::new();
    let mut workflows = BTreeMap::new();
    for suite in suites {
        let id = suite.database_id.ok_or(MALFORMED)?;
        let app = suite.app.ok_or(MALFORMED)?;
        let app_id = app.database_id.ok_or(MALFORMED)?;
        let actions = app.slug.as_deref() == Some(ACTIONS);
        if let Some(ran) = suite.workflow_run {
            let workflow = ran.workflow.and_then(|w| w.database_id).ok_or(MALFORMED)?;
            workflows.insert(id, (workflow, ran.event.ok_or(MALFORMED)?));
        }
        let Some(nodes) = whole(suite.check_runs.ok_or(MALFORMED)?)? else {
            return Ok(Asked::Long);
        };
        for r in nodes {
            let status = run_status(
                &r.status.ok_or(MALFORMED)?.to_ascii_lowercase(),
                r.conclusion.map(|c| c.to_ascii_lowercase()).as_deref(),
            );
            let name = r.name.ok_or(MALFORMED)?;
            let run = Run {
                id: r.database_id.ok_or(MALFORMED)?,
                app: app_id,
                actions,
                suite: id,
                started: started(r.started_at.as_ref()),
            };
            if name == check {
                runs.push((run, Check::new(name, status)));
            }
        }
    }
    if runs.len().saturating_add(contexts.len()) > ALL_MAX.get() as usize {
        return Err(Outcome::Unread(Unread::Cut));
    }
    let mut statuses = Vec::new();
    for c in contexts {
        let c = c.ok_or(MALFORMED)?;
        let state = c.state.ok_or(MALFORMED)?.to_ascii_lowercase();
        statuses.push(Check::new(
            c.context.ok_or(MALFORMED)?,
            status_state(&state),
        ));
    }
    let checks = folded(runs, &workflows, statuses, Some(check))?;
    Ok(Asked::Whole(Some((head, checks))))
}

/// The items of a list the one question gave, None where it may not be
/// whole: `ONE_PAGE` or more, or a count past them.
fn whole<T>(conn: Conn<T>) -> Got<Option<Vec<T>>> {
    let total = conn.total_count.ok_or(MALFORMED)?;
    let nodes = conn.nodes.ok_or(MALFORMED)?;
    if nodes.len() >= ONE_PAGE || total != nodes.len() as u64 {
        return Ok(None);
    }
    nodes
        .into_iter()
        .map(|n| n.ok_or(MALFORMED))
        .collect::<Got<Vec<T>>>()
        .map(Some)
}

impl GitHub {
    /// The runs, REST, `filter=all`, every run on the commit, folded by
    /// `newest_of_each`: two Apps' runs of one name, or two workflows' jobs
    /// of one name, stay apart whatever GitHub's own `latest` would keep
    /// (Decided 129), and a newer run of the same workflow, scheduled or
    /// re-run, of the same event, supersedes an older one (amended
    /// 2026-11-25). Which workflow and event each suite of GitHub Actions
    /// runs comes from one paged read of the
    /// commit's workflow runs, never one per suite, asked only when a run of
    /// GitHub Actions is held: see `workflows_of`. Then the combined status,
    /// the newest state of each context. Each is read whole, a page of 100
    /// at a time, and the answer is whole up to `ALL_MAX` runs and contexts
    /// together: past it, `Unread(Cut)`, known from the two first pages'
    /// counts before any further page. Asked by name, the runs of that name
    /// (`check_name`) and the context of that name; the bound holds every
    /// context read.
    pub(super) fn checks_of(&mut self, at: &CommitId, name: Option<&str>) -> Got<Checks> {
        let named = name.map(|n| format!("&check_name={}", uri(n)));
        let repo = self.repo.clone();
        let runs_at = |page: u32| {
            format!(
                "repos/{repo}/commits/{}/check-runs?filter=all&per_page=100&page={page}{}",
                at.as_str(),
                named.as_deref().unwrap_or("")
            )
        };
        let statuses_at = |page: u32| {
            format!(
                "repos/{repo}/commits/{}/status?per_page=100&page={page}",
                at.as_str()
            )
        };
        let first_runs = self.checks_page("the check runs", &runs_at(1), read_runs)?;
        let first_statuses = self.checks_page("the statuses", &statuses_at(1), read_statuses)?;
        if first_runs.0.saturating_add(first_statuses.0) > u64::from(ALL_MAX.get()) {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let runs = self.checks_whole("the check runs", runs_at, read_runs, first_runs)?;
        let workflows = if runs.iter().any(|(run, _)| run.actions) {
            self.workflows_of(at)?
        } else {
            BTreeMap::new()
        };
        let statuses =
            self.checks_whole("the statuses", statuses_at, read_statuses, first_statuses)?;
        let statuses = statuses.into_iter().map(|(_, s)| s).collect();
        folded(runs, &workflows, statuses, name)
    }

    /// The workflow and event of each check suite of GitHub Actions on the
    /// commit: one paged read of `actions/runs?head_sha=`, whole up to
    /// `ALL_MAX` workflow runs, past it `Unread(Cut)`. A call that fails
    /// fails the checks read for this poll: the runs cannot be folded
    /// without it. The site refusing it is `Forbidden`, as for an App
    /// without `actions: read` (its line, `WORKFLOW_RUNS_FORBIDDEN`), and
    /// any other failure `Unread(Inconsistent)`. Two workflow runs that give
    /// one suite two workflows or events are `Unread(Inconsistent)`.
    fn workflows_of(&mut self, at: &CommitId) -> Got<BTreeMap<u64, Ran>> {
        let repo = self.repo.clone();
        let path = |page: u32| {
            format!(
                "repos/{repo}/actions/runs?head_sha={}&per_page=100&page={page}",
                at.as_str()
            )
        };
        let unread = |o: Outcome| match o {
            Outcome::Unread(u) => Outcome::Unread(u),
            _ => Outcome::Unread(Unread::Inconsistent),
        };
        let what = "the workflow runs";
        let runs = match self
            .checks_page(what, &path(1), read_workflow_runs)
            .and_then(|first| self.checks_whole(what, path, read_workflow_runs, first))
        {
            Ok(runs) => runs,
            Err(Outcome::Forbidden) => {
                self.line_once(WORKFLOW_RUNS_FORBIDDEN);
                return Err(Outcome::Forbidden);
            }
            Err(o) => return Err(unread(o)),
        };
        let mut of = BTreeMap::new();
        for (_, (suite, ran)) in runs {
            if *of.entry(suite).or_insert_with(|| ran.clone()) != ran {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
        }
        Ok(of)
    }

    /// One page of a list, read. A count past `ALL_MAX` is `Unread(Cut)`.
    fn checks_page<K, T>(
        &mut self,
        what: &str,
        path: &str,
        read: fn(&str) -> Option<Counted<K, T>>,
    ) -> Got<Counted<K, T>> {
        let out = self
            .call(what, &["api", path])
            .map_err(|e| self.checks_outcome(&e))?;
        let page = read(&out).ok_or(Outcome::Unread(Unread::Malformed))?;
        if page.0 > u64::from(ALL_MAX.get()) {
            return Err(Outcome::Unread(Unread::Cut));
        }
        Ok(page)
    }

    /// A list read whole from its first page on, from `path` of each next
    /// page: the same count on every page, no key twice, and the items adding
    /// up to the count. A page that moves under the read, or a short page
    /// with more to come, is `Unread(Inconsistent)`.
    fn checks_whole<K: Ord + Clone, T>(
        &mut self,
        what: &str,
        path: impl Fn(u32) -> String,
        read: fn(&str) -> Option<Counted<K, T>>,
        first: Counted<K, T>,
    ) -> Got<Vec<(K, T)>> {
        let total = first.0;
        let mut items = Vec::new();
        let mut keys = BTreeSet::new();
        let mut page = first;
        for next in 2u32.. {
            if page.0 != total {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            let empty = page.1.is_empty();
            for (key, item) in page.1 {
                if !keys.insert(key.clone()) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                items.push((key, item));
            }
            let held = items.len() as u64;
            if held == total {
                return Ok(items);
            }
            if held > total || empty {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            page = self.checks_page(what, &path(next), read)?;
        }
        Err(Outcome::Unread(Unread::Inconsistent))
    }

    /// The outcome of a check runs or statuses read that failed. GitHub
    /// answers 422 for a commit the repository does not hold.
    fn checks_outcome(&self, e: &ForgeError) -> Outcome {
        if e.why.trim_start_matches(": ").starts_with("HTTP 422") {
            return Outcome::NotFound;
        }
        self.outcome(e)
    }
}

/// The checks: the newest run of each fold, then the statuses (of `name`
/// alone where it is given), and `Overall::of` all of them.
fn folded(
    runs: Vec<(Run, Check)>,
    workflows: &BTreeMap<u64, Ran>,
    statuses: Vec<Check>,
    name: Option<&str>,
) -> Got<Checks> {
    let mut runs = newest_of_each(runs, workflows)?;
    runs.extend(
        statuses
            .into_iter()
            .filter(|s| name.is_none_or(|n| s.name == n)),
    );
    let mut checks = Checks::default();
    checks.overall = Overall::of(&runs);
    checks.runs = runs;
    Ok(checks)
}

/// The newest run of each fold, in the order the site listed the first of
/// each. A run of GitHub Actions whose suite `workflows` maps is folded by
/// App, workflow, event and name, the newest by `started_at`, then the
/// larger id (Decided 129 as amended 2026-11-25). The event keeps a `push`
/// run, which tests the branch alone, apart from a `pull_request` run,
/// which tests its merge with the base. A run not yet started is the
/// newest of its fold, so its own status (queued, so pending) decides; a
/// start that is no time is `Unread(Malformed)`, as the run cannot be
/// placed. Any other run, of another App or of a suite the workflow runs
/// do not map, keeps 129's fold: by App, suite and name, the newest the
/// highest id. That can make no false pass, and only that suite pays.
fn newest_of_each(runs: Vec<(Run, Check)>, workflows: &BTreeMap<u64, Ran>) -> Got<Vec<Check>> {
    let mut at: BTreeMap<Fold, usize> = BTreeMap::new();
    let mut kept: Vec<((i64, u64), Check)> = Vec::new();
    for (run, check) in runs {
        let name = check.name.clone();
        let (fold, newness) = match workflows.get(&run.suite).filter(|_| run.actions) {
            Some((workflow, event)) => {
                let started = match run.started {
                    Started::At(t) => t,
                    Started::Not => i64::MAX,
                    Started::Bad => return Err(Outcome::Unread(Unread::Malformed)),
                };
                let fold = Fold::Workflow {
                    app: run.app,
                    workflow: *workflow,
                    event: event.clone(),
                    name,
                };
                (fold, (started, run.id))
            }
            None => {
                let fold = Fold::Suite {
                    app: run.app,
                    suite: run.suite,
                    name,
                };
                (fold, (0, run.id))
            }
        };
        match at.get(&fold) {
            Some(&i) if kept[i].0 < newness => kept[i] = (newness, check),
            Some(_) => {}
            None => {
                at.insert(fold, kept.len());
                kept.push((newness, check));
            }
        }
    }
    Ok(kept.into_iter().map(|(_, c)| c).collect())
}

/// A page of the check runs read: the count the site says it holds, and
/// each run with its id, App, check suite and start. None for an answer
/// that lacks a field every run has. A start that is not given, or null,
/// is `Started::Not`, and one that is not a time of the form GitHub writes
/// `Started::Bad` (`newest_of_each` refuses it where it folds by start);
/// an App with no slug is not GitHub Actions. A conclusion is folded into the port's words: `success` and
/// `neutral` pass, `skipped` is a skip, `failure`, `timed_out` and
/// `startup_failure` fail, `cancelled` is cancelled, and any other word, or
/// a finished run that names none, `Unknown`. A status that is not
/// `in_progress` or `completed` is queued.
fn read_runs(out: &str) -> Option<Counted<Run>> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let count = v.get("total_count")?.as_u64()?;
    let mut runs = Vec::new();
    for r in v.get("check_runs")?.as_array()? {
        let text = |k: &str| r.get(k).and_then(Value::as_str);
        let status = run_status(text("status")?, text("conclusion"));
        let check = Check::new(text("name")?.to_string(), status);
        let started = started(r.get("started_at"));
        let run = Run {
            id: r.get("id")?.as_u64()?,
            app: r.pointer("/app/id")?.as_u64()?,
            actions: r.pointer("/app/slug").and_then(Value::as_str) == Some(ACTIONS),
            suite: r.pointer("/check_suite/id")?.as_u64()?,
            started,
        };
        runs.push((run, check));
    }
    Some((count, runs))
}

/// A check run's status and conclusion, in the site's words, as the
/// port's: see `read_runs`.
fn run_status(status: &str, conclusion: Option<&str>) -> CheckStatus {
    match status {
        "completed" => CheckStatus::Done(match conclusion {
            Some("success" | "neutral") => Conclusion::Passed,
            Some("skipped") => Conclusion::Skipped,
            Some("failure" | "timed_out" | "startup_failure") => Conclusion::Failed,
            Some("cancelled") => Conclusion::Cancelled,
            _ => Conclusion::Unknown,
        }),
        "in_progress" => CheckStatus::Running,
        _ => CheckStatus::Queued,
    }
}

/// When a check run started, from its `started_at`: see `read_runs`.
fn started(at: Option<&Value>) -> Started {
    match at {
        None | Some(Value::Null) => Started::Not,
        Some(t) => t
            .as_str()
            .and_then(tehas_core::time::from_iso)
            .map_or(Started::Bad, Started::At),
    }
}

/// A page of the commit's workflow runs: the count the site says it holds,
/// and each run by its id with its check suite, workflow and event. None
/// for an answer that lacks a field every workflow run has.
fn read_workflow_runs(out: &str) -> Option<Counted<u64, (u64, Ran)>> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let count = v.get("total_count")?.as_u64()?;
    let mut runs = Vec::new();
    for r in v.get("workflow_runs")?.as_array()? {
        let id = r.get("id")?.as_u64()?;
        let suite = r.get("check_suite_id")?.as_u64()?;
        let workflow = r.get("workflow_id")?.as_u64()?;
        let event = r.get("event")?.as_str()?.to_string();
        runs.push((id, (suite, (workflow, event))));
    }
    Some((count, runs))
}

/// A page of the combined status: the count of contexts the site says it
/// holds, and each context's newest state as a run named by it. None for
/// an answer that lacks a field every status has. `success` passes,
/// `failure` and `error` fail, `pending` is running, and any other word is
/// a finished run of an `Unknown` end, never a pass.
fn read_statuses(out: &str) -> Option<Counted<String>> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let count = v.get("total_count")?.as_u64()?;
    let mut statuses = Vec::new();
    for s in v.get("statuses")?.as_array()? {
        let text = |k: &str| s.get(k).and_then(Value::as_str);
        let context = text("context")?.to_string();
        let status = status_state(text("state")?);
        statuses.push((context.clone(), Check::new(context, status)));
    }
    Some((count, statuses))
}

/// A status context's state, in the site's words, as a run's: see
/// `read_statuses`.
fn status_state(state: &str) -> CheckStatus {
    match state {
        "success" => CheckStatus::Done(Conclusion::Passed),
        "failure" | "error" => CheckStatus::Done(Conclusion::Failed),
        "pending" => CheckStatus::Running,
        _ => CheckStatus::Done(Conclusion::Unknown),
    }
}
