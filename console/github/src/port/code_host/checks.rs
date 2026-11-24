//! `CodeHost::checks`: a commit's check runs, the newest of each App and
//! name, then its status contexts, each one's newest state held as a run
//! named by its context, and `Overall::of` all of them. A failure of either
//! read fails the whole: the runs alone are never an answer.

use std::collections::BTreeSet;

use crate::forge::uri;
use crate::{ForgeError, GitHub};
use serde_json::Value;
use tehas_ports::{
    Check, CheckStatus, Checks, CommitId, Conclusion, Got, Outcome, Overall, Unread, ALL_MAX,
};

/// One page of a list the site counts: its count, and each item with the
/// key that names it once in the list.
type Counted<K> = (u64, Vec<(K, Check)>);

impl GitHub {
    /// The runs, REST, `filter=latest`, which gives the newest run of each
    /// App and name and keeps two Apps' runs of one name apart; then the
    /// combined status, the newest state of each context. Each is read whole,
    /// a page of 100 at a time, up to `ALL_MAX`. Asked by name, the runs of
    /// that name (`check_name`) and the context of that name.
    pub(super) fn checks_of(&mut self, at: &CommitId, name: Option<&str>) -> Got<Checks> {
        let named = name.map(|n| format!("&check_name={}", uri(n)));
        let repo = self.repo.clone();
        let runs_at = |page: u32| {
            format!(
                "repos/{repo}/commits/{}/check-runs?filter=latest&per_page=100&page={page}{}",
                at.as_str(),
                named.as_deref().unwrap_or("")
            )
        };
        let mut runs = self.whole("the check runs", runs_at, read_runs)?;
        let statuses_at = |page: u32| {
            format!(
                "repos/{repo}/commits/{}/status?per_page=100&page={page}",
                at.as_str()
            )
        };
        let statuses = self.whole("the statuses", statuses_at, read_statuses)?;
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

    /// A list read whole, page by page, from `path` of each page: the same count
    /// on every page, no key twice, and the items adding up to the count. A
    /// page that moves under the read, or a short page with more to come, is
    /// `Unread(Inconsistent)`; a count past `ALL_MAX`, `Unread(Cut)`.
    fn whole<K: Ord>(
        &mut self,
        what: &str,
        path: impl Fn(u32) -> String,
        read: fn(&str) -> Option<Counted<K>>,
    ) -> Got<Vec<Check>> {
        let mut items = Vec::new();
        let mut keys = BTreeSet::new();
        let mut total = None;
        for page in 1u32.. {
            let out = self
                .call(what, &["api", &path(page)])
                .map_err(|e| self.checks_outcome(&e))?;
            let (count, page) = read(&out).ok_or(Outcome::Unread(Unread::Malformed))?;
            if count > u64::from(ALL_MAX.get()) {
                return Err(Outcome::Unread(Unread::Cut));
            }
            if *total.get_or_insert(count) != count {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            let empty = page.is_empty();
            for (key, item) in page {
                if !keys.insert(key) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                items.push(item);
            }
            let held = items.len() as u64;
            if held == count {
                return Ok(items);
            }
            if held > count || empty {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
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

/// A page of the check runs read: the count the site says it holds, and
/// each run with its id. None for an answer that lacks a field every run
/// has. A conclusion is folded into the port's words: `success` and
/// `neutral` pass, `skipped` is a skip, `failure`, `timed_out` and
/// `startup_failure` fail, `cancelled` is cancelled, and any other word, or
/// a finished run that names none, `Unknown`. A status that is not
/// `in_progress` or `completed` is queued.
fn read_runs(out: &str) -> Option<Counted<u64>> {
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
        runs.push((r.get("id")?.as_u64()?, check));
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
