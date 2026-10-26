//! Rule 2's clean-ups: what a poll stops, clears and removes before it
//! decides any start, a paused poll too. A quiet cycle, a finished one, a
//! mark with no cycle, a tehas:refreshing with no refresh, and what an old
//! cycle left on this host.

use super::{has, or_unknown, Poll, Step};
use crate::events::label_actor;
use crate::snapshot::Cycle;
use regex::Regex;
use serde_json::{json, Value};

const QUIET_BOUND: u64 = 1200;
const QUIET_RETRY: u64 = 600;
const QUIET_ASKS: u64 = 6;

/// date -u -d @<secs> +%FT%TZ.
pub(super) fn iso(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// What the poll does now about an episode asked about `times` times, last
/// at `asked`.
fn quiet_due(times: u64, asked: u64, now: u64) -> &'static str {
    let next = if times <= QUIET_ASKS {
        asked + QUIET_RETRY
    } else {
        asked + QUIET_RETRY * QUIET_ASKS
    };
    if times == 0 {
        "stop"
    } else if now + 1 < next {
        "wait"
    } else if times < QUIET_ASKS {
        "again"
    } else {
        "line"
    }
}

/// tehas_names_issue: a pull request of issue `n` by its branch or its body.
fn names_issue(pr: &Value, n: u64) -> bool {
    let text = |k: &str| match pr.get(k) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    };
    let head = text("headRefName");
    let branch = format!("feature/issue-{n}");
    if head == branch || head.starts_with(&format!("{branch}-")) {
        return true;
    }
    Regex::new(&format!(
        r"(?i)\b(close[sd]?|fix(e[sd])?|resolve[sd]?):?\s+#{n}([^0-9]|$)"
    ))
    .is_ok_and(|r| r.is_match(&text("body")))
}

impl Poll<'_> {
    pub(super) fn clean_up(&mut self, bot: &str) -> Step {
        let running: Vec<u64> = self.s.running.iter().map(|(n, _)| *n).collect();
        if !running.is_empty() {
            if let Err(e) = &self.s.cycles {
                let rc = if e.rc == 1 { 1 } else { 78 };
                let word = e.word.clone();
                return Err(self.fail(
                    format!("ERROR: the state store could not list the running cycles ({word})"),
                    rc,
                ));
            }
        }
        self.quiet(&running);
        self.finished(&running)?;
        self.marked()?;
        self.refreshing(bot)?;
        self.sweep();
        Ok(())
    }

    fn cycle(&self, n: u64) -> Cycle {
        self.s
            .cycles
            .as_ref()
            .ok()
            .and_then(|c| c.get(&n).cloned())
            .unwrap_or_default()
    }

    /// A cycle that holds its lock and has not been heard from for 20
    /// minutes is stopped, and asked again, spaced, until it is a person's.
    fn quiet(&mut self, running: &[u64]) {
        for &n in running {
            let r = self.cycle(n);
            let heard = r.heard.max(r.took);
            if heard == 0 {
                continue;
            }
            let Some(now) = self.s.now else {
                self.need("now");
                return;
            };
            if now.saturating_sub(heard) <= QUIET_BOUND {
                continue;
            }
            let why = format!("the cycle for #{n} holds its lock but has not been heard from since {}, more than {QUIET_BOUND}s", iso(heard));
            if r.took == 0 {
                self.log(format!("WARN: {why}, and the state store has no open record of it, so the poll does not stop it: if it stays, a person can, with tehas stop {n} or systemctl --user stop tehas-cycle@{n}.service"));
                continue;
            }
            let asks = if r.said_for == heard.to_string() && r.asked >= r.quiet_at {
                r.times
            } else {
                0
            };
            let due = quiet_due(asks, r.asked, now);
            let unit = format!("tehas-cycle@{n}.service");
            if self.dry {
                match due {
                    "stop" => self.log(format!("would stop {unit}: {why}")),
                    "again" => self.log(format!("would ask {unit} to stop again: {why}")),
                    "line" => self.log(format!(
                        "would say that {unit} needs a person and ask no more: {why}"
                    )),
                    _ => {}
                }
                continue;
            }
            let mut a = json!({"quiet": n, "said_for": heard});
            match due {
                "stop" => {
                    self.log(format!("WARN: {why}: stopping {unit}"));
                    a["stop"] = json!(true);
                }
                "again" => {
                    self.log(format!("WARN: {why}: the poll asks {unit} to stop again"));
                    a["stop"] = json!(true);
                }
                "line" => {
                    self.log(format!("WARN: {why}: the poll asked {QUIET_ASKS} times to stop {unit} and asks no more: it needs a person, who can stop it with systemctl --user stop {unit}"));
                    a["asked"] = json!(true);
                    a["said_only"] = json!(asks != QUIET_ASKS);
                }
                _ => {}
            }
            self.act(a);
        }
    }

    /// A running cycle whose issue is closed, or whose pull request opened
    /// since it began is merged or closed, is stopped, and its slot is free.
    fn finished(&mut self, running: &[u64]) -> Step {
        if running.is_empty() {
            return Ok(());
        }
        let prs = match &self.s.bot_prs {
            None => {
                self.need("bot_prs");
                return Ok(());
            }
            Some(v) if v.get("failed") == Some(&json!(true)) => {
                return Err(self.fail("ERROR: gh pr list failed", 1))
            }
            Some(Value::Array(a)) => a.clone(),
            Some(_) => {
                return Err(self.fail("ERROR: unreadable list of the bot's pull requests", 1))
            }
        };
        let url_ok = Regex::new("^https://[A-Za-z0-9./_-]+$").unwrap();
        for &n in running {
            let took = self.cycle(n).took;
            if took == 0 {
                continue;
            }
            let Some(work) = self.s.issues.get(&n).and_then(|i| i.work.clone()) else {
                let repo = self.c.repo.clone();
                return Err(self.fail(format!("ERROR: gh api repos/{repo}/issues/{n} failed"), 1));
            };
            let state = work
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let (why, text) = if state == "closed" {
                ("closed", "its issue is closed".to_string())
            } else {
                let since = iso(took);
                let created = |p: &Value| {
                    p.get("createdAt")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                };
                let mut mine: Vec<&Value> = prs
                    .iter()
                    .filter(|p| p.is_object() && created(p) >= since && names_issue(p, n))
                    .collect();
                mine.sort_by_key(|p| created(p));
                let last = mine.last();
                let field = |k: &str| {
                    last.and_then(|p| p.get(k))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                };
                let url = Some(field("url"))
                    .filter(|u| url_ok.is_match(u))
                    .unwrap_or("");
                match field("state") {
                    "MERGED" => ("merged", format!("its pull request {url} is merged")),
                    "CLOSED" => (
                        "pr-closed",
                        format!("its pull request {url} was closed without a merge"),
                    ),
                    _ => continue,
                }
            };
            if self.dry {
                self.log(format!("would stop tehas-cycle@{n}.service: {text}"));
                continue;
            }
            self.log(format!("stop #{n}: {text}"));
            self.act(json!({"stop_done": n, "why": why}));
            self.stopped.push(n);
        }
        Ok(())
    }

    /// An issue marked tehas:running with no cycle behind it: a closed one
    /// loses the mark, an open one is handed to a person.
    fn marked(&mut self) -> Step {
        let Some(marked) = &self.s.marked else {
            self.need("marked");
            return Ok(());
        };
        let list = match marked {
            v if v.get("failed") == Some(&json!(true)) => {
                return Err(self.fail("ERROR: gh issue list failed", 1))
            }
            Value::Array(a) => a.clone(),
            _ => return Err(self.fail("ERROR: unreadable list of issues marked as running", 1)),
        };
        let mut numbers = Vec::new();
        for i in &list {
            match i.get("number").and_then(Value::as_u64) {
                Some(n) => numbers.push((n, i.clone())),
                None => {
                    return Err(self.fail("ERROR: unreadable list of issues marked as running", 1))
                }
            }
        }
        numbers.sort_by_key(|(n, _)| *n);
        for (n, i) in numbers {
            if self.running(n) || self.stopped.contains(&n) {
                continue;
            }
            if !self.dry {
                self.act(json!({"reap": format!("tehas-work-{n}")}));
            }
            if i.get("state").and_then(Value::as_str) == Some("CLOSED") {
                let line = format!(
                    "clear #{n}: it is closed, marked tehas:running, and no cycle holds its lock"
                );
                if self.dry {
                    self.log(format!("would {line}"));
                } else {
                    self.log(line);
                    self.act(json!({"issue": n, "remove": ["tehas:running"]}));
                }
                continue;
            }
            let Some(work) = self.s.issues.get(&n).and_then(|i| i.work.clone()) else {
                let repo = self.c.repo.clone();
                return Err(self.fail(format!("ERROR: gh api repos/{repo}/issues/{n} failed"), 1));
            };
            let labels = match work.get("labels") {
                _ if !work.is_object() => None,
                None | Some(Value::Null) => Some(Vec::new()),
                Some(Value::Array(a)) => Some(
                    a.iter()
                        .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
                        .collect(),
                ),
                Some(_) => None,
            };
            let Some(labels) = labels else {
                return Err(self.fail(
                    format!("ERROR: GitHub's answer for #{n} could not be read"),
                    1,
                ));
            };
            if !has(&labels, "tehas:running") {
                self.log(format!(
                    "skip #{n}: tehas:running is gone from it since the list was read"
                ));
                continue;
            }
            let line = format!("park #{n}: it is marked tehas:running and no cycle holds its lock");
            if self.dry {
                self.log(format!("would {line}"));
                continue;
            }
            let Some(host) = self.s.hostname.clone() else {
                self.need("hostname");
                continue;
            };
            self.log(line);
            self.act(json!({
                "issue": n, "remove": ["tehas:running"], "add": ["needs-human-attention"],
                "say": format!("Tehas found this issue marked `tehas:running` with no cycle behind it on `{host}`: the cycle ended without changing the label, or never started. Look for a pull request it may have opened. The run log is `~/tehas/logs/dispatch-work-{n}.log` there. To retry, apply `tehas:ready`. If it is on the issue already, nothing more is needed."),
            }));
        }
        Ok(())
    }

    /// A pull request marked tehas:refreshing whose refresh is not running,
    /// or whose mark the bot did not apply, loses the mark.
    fn refreshing(&mut self, bot: &str) -> Step {
        let history = match &self.s.refresh_history {
            None => {
                self.need("refresh_history");
                return Ok(());
            }
            Some(Ok(h)) => h.clone(),
            Some(Err(e)) => {
                let word = e.word.clone();
                self.log(format!("WARN: the history of refreshes could not be read ({word}), so no pull request is cleared of tehas:refreshing this poll"));
                return Ok(());
            }
        };
        let mut from = history;
        from.extend(
            self.s
                .running
                .iter()
                .filter(|(_, how)| how == "refresh")
                .map(|(n, _)| *n),
        );
        from.sort_unstable();
        from.dedup();
        if !self.c.refresh_conflicts && from.is_empty() {
            return Ok(());
        }
        let list = match &self.s.refreshing {
            None => {
                self.need("refreshing");
                return Ok(());
            }
            Some(v) if v.get("failed") == Some(&json!(true)) => {
                return Err(self.fail("ERROR: gh pr list failed", 1))
            }
            Some(Value::Array(a)) => a.clone(),
            Some(_) => {
                return Err(self.fail(
                    "ERROR: unreadable list of pull requests marked as refreshing",
                    1,
                ))
            }
        };
        let mut prs: Vec<u64> = list
            .iter()
            .filter(|p| {
                p.get("labels").and_then(Value::as_array).is_some_and(|l| {
                    l.iter()
                        .any(|x| x.get("name").and_then(Value::as_str) == Some("tehas:refreshing"))
                })
            })
            .filter_map(|p| p.get("number").and_then(Value::as_u64))
            .collect();
        let mut owner = std::collections::BTreeMap::new();
        if !prs.is_empty() {
            for &n in &from {
                let Some(current) = self.s.issues.get(&n).and_then(|i| i.current.clone()) else {
                    self.need("current");
                    return Ok(());
                };
                match current {
                    Err(e) => {
                        self.log(format!("WARN: the approval of #{n} could not be read ({}), so no pull request is cleared of tehas:refreshing this poll", e.word));
                        prs.clear();
                        break;
                    }
                    Ok(Some(c)) if c.get("how").and_then(Value::as_str) == Some("refresh") => {
                        let pr = match c.get("refresh_pr") {
                            Some(Value::Number(x)) => x.to_string(),
                            Some(Value::String(x)) => x.clone(),
                            _ => String::new(),
                        };
                        let ok = !pr.is_empty()
                            && pr.len() <= 10
                            && !pr.starts_with('0')
                            && pr.bytes().all(|b| b.is_ascii_digit());
                        if ok {
                            owner.insert(pr.parse::<u64>().unwrap_or(0), n);
                        }
                    }
                    Ok(_) => {}
                }
            }
        }
        for pr in prs {
            let why = match owner.get(&pr) {
                None => "no refresh on this host names it".to_string(),
                Some(&o) if !self.running(o) => format!("no cycle runs on #{o}"),
                Some(_) => {
                    let ev = self.events(pr)?;
                    let (who, _) = label_actor(&ev, "tehas:refreshing");
                    if who == bot {
                        continue;
                    }
                    format!("it was applied by '{}', not by tehas", or_unknown(&who))
                }
            };
            if self.dry {
                self.log(format!(
                    "would take tehas:refreshing off pull request #{pr}: {why}"
                ));
                continue;
            }
            self.act(json!({
                "pr": pr, "remove": ["tehas:refreshing"],
                "log": format!("clear pull request #{pr}: tehas:refreshing off, {why}"),
            }));
        }
        Ok(())
    }

    /// What a cycle left for an issue goes once nothing has used it for 7
    /// days, three issues a poll at most.
    fn sweep(&mut self) {
        let mut seen = Vec::new();
        let mut count = 0;
        let Some(leftovers) = self.s.leftovers.clone() else {
            self.need("leftovers");
            return;
        };
        for l in leftovers {
            let n = l.issue;
            if seen.contains(&n) {
                continue;
            }
            seen.push(n);
            if count >= 3 {
                return;
            }
            if self.running(n) {
                continue;
            }
            if !l.lock_ok {
                self.log(format!(
                    "WARN: the lock of #{n} is no lock file, so nothing of it was removed"
                ));
                continue;
            }
            if l.young {
                continue;
            }
            count += 1;
            let line = format!("what #{n} left on this host: no cycle has run on it for 7 days");
            if self.dry {
                self.log(format!("would remove {line}"));
            } else {
                self.act(json!({"sweep": n, "log": format!("removed {line}")}));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_is_date_u() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_761_436_800), "2025-10-26T00:00:00Z");
        assert_eq!(iso(951_782_400 + 3661), "2000-02-29T01:01:01Z");
    }

    #[test]
    fn a_pull_request_names_its_issue() {
        let pr = |head: &str, body: &str| json!({"headRefName": head, "body": body});
        assert!(names_issue(&pr("feature/issue-9-x", ""), 9));
        assert!(!names_issue(&pr("feature/issue-91", ""), 9));
        assert!(names_issue(&pr("", "Fixes #9"), 9));
        assert!(!names_issue(&pr("", "Fixes #91"), 9));
        assert!(names_issue(&pr("", "closes: #9."), 9));
    }
}
