//! `CodeHost::checks`: a commit's check runs, the newest of each App,
//! workflow and name for GitHub Actions and of each App, check suite and
//! name for any other (Decided 129, as amended 2026-11-25), then its status
//! contexts, each one's newest state held as a run named by its context,
//! and `Overall::of` all of them. A failure of any read fails the whole:
//! the runs alone are never an answer.

use std::collections::{BTreeMap, BTreeSet};

use crate::forge::uri;
use crate::{ForgeError, GitHub};
use serde_json::Value;
use tehas_ports::{
    Check, CheckStatus, Checks, CommitId, Conclusion, Got, Outcome, Overall, Unread, ALL_MAX,
};

/// One page of a list the site counts: its count, and each item with the
/// key that names it once in the list.
type Counted<K, T = Check> = (u64, Vec<(K, T)>);

/// The slug of GitHub Actions' App, the one App whose check suites are
/// workflow runs.
const ACTIONS: &str = "github-actions";

/// What names a check run: its id, the App that made it, whether that App
/// is GitHub Actions, its check suite (for GitHub Actions, one workflow
/// run) and when it started, in Unix seconds, if the site says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Run {
    id: u64,
    app: u64,
    actions: bool,
    suite: u64,
    started: Option<i64>,
}

/// What a check run is folded by. A run of GitHub Actions whose suite the
/// commit's workflow runs map is held by its workflow, so a newer run of
/// the same workflow, scheduled or re-run, supersedes; any other by its
/// suite, as Decided 129 has it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Fold {
    Workflow {
        app: u64,
        workflow: u64,
        name: String,
    },
    Suite {
        app: u64,
        suite: u64,
        name: String,
    },
}

impl GitHub {
    /// The runs, REST, `filter=all`, every run on the commit, folded by
    /// `newest_of_each`: two Apps' runs of one name, or two workflows' jobs
    /// of one name, stay apart whatever GitHub's own `latest` would keep
    /// (Decided 129), and a newer run of the same workflow, scheduled or
    /// re-run, supersedes an older one (amended 2026-11-25). Which workflow
    /// each suite of GitHub Actions runs comes from one paged read of the
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
        let mut runs = newest_of_each(runs, &workflows)?;
        let statuses =
            self.checks_whole("the statuses", statuses_at, read_statuses, first_statuses)?;
        runs.extend(
            statuses
                .into_iter()
                .map(|(_, s)| s)
                .filter(|s| name.is_none_or(|n| s.name == n)),
        );
        let mut checks = Checks::default();
        checks.overall = Overall::of(&runs);
        checks.runs = runs;
        Ok(checks)
    }

    /// The workflow of each check suite of GitHub Actions on the commit:
    /// one paged read of `actions/runs?head_sha=`, whole up to `ALL_MAX`
    /// workflow runs, past it `Unread(Cut)`. A call that fails leaves the
    /// checks unread for this poll (`Unread(Inconsistent)`), whatever it
    /// failed of: the runs cannot be folded without it. Two workflow runs
    /// that give one suite two workflows are `Unread(Inconsistent)`.
    fn workflows_of(&mut self, at: &CommitId) -> Got<BTreeMap<u64, u64>> {
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
        let first = self
            .checks_page(what, &path(1), read_workflow_runs)
            .map_err(unread)?;
        let runs = self
            .checks_whole(what, path, read_workflow_runs, first)
            .map_err(unread)?;
        let mut of = BTreeMap::new();
        for (_, (suite, workflow)) in runs {
            if *of.entry(suite).or_insert(workflow) != workflow {
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

/// The newest run of each fold, in the order the site listed the first of
/// each. A run of GitHub Actions whose suite `workflows` maps is folded by
/// App, workflow and name, the newest by `started_at`, then the larger id
/// (Decided 129 as amended 2026-11-25); such a run with no start is
/// `Unread(Malformed)`, as it cannot be placed. Any other run, of another
/// App or of a suite the workflow runs do not map, keeps 129's fold: by
/// App, suite and name, the newest the highest id. That can make no false
/// pass, and only that suite pays.
fn newest_of_each(runs: Vec<(Run, Check)>, workflows: &BTreeMap<u64, u64>) -> Got<Vec<Check>> {
    let mut at: BTreeMap<Fold, usize> = BTreeMap::new();
    let mut kept: Vec<((i64, u64), Check)> = Vec::new();
    for (run, check) in runs {
        let name = check.name.clone();
        let (fold, newness) = match workflows.get(&run.suite).filter(|_| run.actions) {
            Some(&workflow) => {
                let started = run.started.ok_or(Outcome::Unread(Unread::Malformed))?;
                let fold = Fold::Workflow {
                    app: run.app,
                    workflow,
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
/// that lacks a field every run has. A start that is not given, or not a
/// time of the form GitHub writes, is None (`newest_of_each` refuses it
/// where it folds by start), and an App with no slug is not GitHub
/// Actions. A conclusion is folded into the port's words: `success` and
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
        let status = match text("status")? {
            "completed" => CheckStatus::Done(match text("conclusion") {
                Some("success" | "neutral") => Conclusion::Passed,
                Some("skipped") => Conclusion::Skipped,
                Some("failure" | "timed_out" | "startup_failure") => Conclusion::Failed,
                Some("cancelled") => Conclusion::Cancelled,
                _ => Conclusion::Unknown,
            }),
            "in_progress" => CheckStatus::Running,
            _ => CheckStatus::Queued,
        };
        let check = Check::new(text("name")?.to_string(), status);
        let started = text("started_at").and_then(tehas_core::time::from_iso);
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

/// A page of the commit's workflow runs: the count the site says it holds,
/// and each run by its id with its check suite and workflow. None for an
/// answer that lacks a field every workflow run has.
fn read_workflow_runs(out: &str) -> Option<Counted<u64, (u64, u64)>> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let count = v.get("total_count")?.as_u64()?;
    let mut runs = Vec::new();
    for r in v.get("workflow_runs")?.as_array()? {
        let id = r.get("id")?.as_u64()?;
        let suite = r.get("check_suite_id")?.as_u64()?;
        let workflow = r.get("workflow_id")?.as_u64()?;
        runs.push((id, (suite, workflow)));
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
        let status = match text("state")? {
            "success" => CheckStatus::Done(Conclusion::Passed),
            "failure" | "error" => CheckStatus::Done(Conclusion::Failed),
            "pending" => CheckStatus::Running,
            _ => CheckStatus::Done(Conclusion::Unknown),
        };
        statuses.push((context.clone(), Check::new(context, status)));
    }
    Some((count, statuses))
}
