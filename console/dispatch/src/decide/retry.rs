//! Rules 4, 4a and 4b: a cycle started by a retry ticked in a park's
//! question, by the one retry a provider outage is owed, and by a refresh
//! request. Each takes the room rules 2 and 3 left, as libexec/dispatch does.

use super::clean::iso;
use super::{fixer_base, has, or_unknown, End, Poll, Room, Start, Step};
use crate::questions;
use crate::text;
use serde_json::{json, Map, Value};

fn labels(v: &Value) -> Vec<String> {
    v.get("labels")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn hex40(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .is_some_and(|s| s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// tehas_refresh_rule's refresh_why: what keeps a pull request from being
/// refreshed, empty when nothing does.
fn refresh_why(pr: &Value, bot: &str, repo: &str, control: u64) -> String {
    let names = |v: &Value| -> Vec<String> {
        v.pointer("/labels/nodes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let s = |v: &Value, p: &str| {
        v.pointer(p)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let Some(num) = pr.get("number").and_then(Value::as_u64) else {
        return "it cannot be read".into();
    };
    let closing = pr
        .pointer("/closingIssuesReferences/nodes")
        .and_then(Value::as_array);
    if s(pr, "/author/login") != bot.strip_suffix("[bot]").unwrap_or(bot) {
        "it is not the bot's".into()
    } else if pr.get("isCrossRepository") != Some(&json!(false))
        || !s(pr, "/headRepository/nameWithOwner").eq_ignore_ascii_case(repo)
    {
        format!("its branch is not in {repo}")
    } else if !matches!(pr.get("isDraft"), Some(&json!(true)) | Some(&json!(false))) {
        "its draft state cannot be read".into()
    } else if pr
        .pointer("/closingIssuesReferences/totalCount")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        != 1.0
        || closing.map_or(0, Vec::len) != 1
    {
        "it does not close exactly one issue".into()
    } else {
        let i = &closing.unwrap()[0];
        let Some(n) = i.get("number").and_then(Value::as_u64) else {
            return "the issue it closes cannot be read".into();
        };
        let l = names(i);
        let open: Vec<Option<u64>> = i
            .pointer("/closedByPullRequestsReferences/nodes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|p| p.get("state").and_then(Value::as_str) == Some("OPEN"))
                    .map(|p| p.get("number").and_then(Value::as_u64))
                    .collect()
            })
            .unwrap_or_default();
        if n == control {
            "it closes the control issue".into()
        } else if i.get("state").and_then(Value::as_str) != Some("OPEN") {
            format!("#{n} is not open")
        } else if !has(&l, "tehas:awaiting-merge") && !has(&l, "needs-human-attention") {
            format!("#{n} carries neither tehas:awaiting-merge nor needs-human-attention")
        } else if has(&l, "tehas:running") {
            format!("#{n} carries tehas:running")
        } else if open != [Some(num)] {
            format!("it is not the one open pull request that closes #{n}")
        } else {
            String::new()
        }
    }
}

/// The largest start this rule can put a date on, as the other rules that
/// read a start from the store take it: 9999-12-31T23:59:59Z.
const MAX_START: u64 = 99_999_999_999;

/// The shape `iso` writes: 20 bytes of `%FT%TZ`, so that the park rule below
/// may order a formatted start against the approval's stamp as text. An empty
/// stamp is no instant, and an approval that names none is not held on it.
fn is_instant(s: &str) -> bool {
    let b = s.as_bytes();
    let some = |from: usize, to: usize| b[from..to].iter().all(|c| c.is_ascii_digit());
    b.len() == 20
        && some(0, 4)
        && b[4] == b'-'
        && some(5, 7)
        && b[7] == b'-'
        && some(8, 10)
        && b[10] == b'T'
        && some(11, 13)
        && b[13] == b':'
        && some(14, 16)
        && b[16] == b':'
        && some(17, 19)
        && b[19] == b'Z'
}

/// A cycle_history row: the parked start of a refresh since `at`, the approval
/// its pull request rests on. Tells apart whether a park since `at` holds the
/// refresh, and whether any park it held rests on a start the store did not
/// give. `at` is the approval's `%FT%TZ` stamp, as
/// `read::store::approval_shape` renders it: a start is ordered against it as
/// text, so a stamp of any other shape holds rather than orders wrongly. A
/// start the row does not give is one this rule cannot date, and a park of a
/// row it cannot date is held: unknown is not old, and the second answer lets
/// the poll say so instead of reading as a park it dated. A `parked` that is
/// not JSON true is no park; the store answers that column as a bool or NULL,
/// and the importer takes only a boolean for it. The bash copy of this rule,
/// lib/refresh.sh, keeps its (.started // 0).
fn parked(history: &[Value], at: &str) -> (bool, bool) {
    // No instant on the record is no date to measure a park from; the rule of
    // the approval itself has already said what to do with such a record.
    if at.is_empty() {
        return (false, false);
    }
    // A stamp that is not the shape of an instant orders nothing. A park is
    // held on it: holding is the side this rule is for, and the row is not the
    // unread part of it, so it is not named as one.
    if !is_instant(at) {
        return (true, false);
    }
    let mut held = false;
    let mut unread = false;
    for h in history {
        if h.get("how").and_then(Value::as_str) != Some("refresh")
            || h.get("parked") != Some(&json!(true))
        {
            continue;
        }
        match h.get("started") {
            Some(v) => match v.as_u64().filter(|s| *s <= MAX_START) {
                // A park from the approval's second on holds the refresh.
                Some(started) => held |= iso(started).as_str() >= at,
                // A start that is not a count of seconds this rule can date,
                // or one past the last date it can write: held as the row with
                // no start is, and named as one the store did not give.
                None => {
                    held = true;
                    unread = true;
                }
            },
            // No key at all: no start on record.
            None => {
                held = true;
                unread = true;
            }
        }
    }
    (held, unread)
}

/// tehas_refresh_count: the refreshes a record counts on that approval.
fn refresh_count(record: &Value, base: &Value) -> Option<u64> {
    let same = |r: &Value| {
        ["at", "by"]
            .iter()
            .all(|k| r.get(*k).unwrap_or(&Value::Null) == base.get(*k).unwrap_or(&Value::Null))
    };
    if record.get("how").and_then(Value::as_str) == Some("refresh") {
        // jq's (.rests_on | {at, by}) fails on anything but an object or null.
        let rests = record.get("rests_on").unwrap_or(&Value::Null);
        if !rests.is_object() && !rests.is_null() {
            return None;
        }
        if !same(rests) {
            return Some(0);
        }
        let n = record.get("refreshes")?.as_f64()?;
        return (n >= 1.0 && n.fract() == 0.0 && n < 1_000_000.0).then_some(n as u64);
    }
    Some(0)
}

impl Poll<'_> {
    /// control_ask: the control issue is never worked on; said once a poll.
    pub(super) fn control_ask(&mut self, n: u64, what: &str) -> bool {
        if n != self.c.control {
            return false;
        }
        if !self.control_skipped {
            self.log(format!(
                "skip #{n}: the control issue is never worked on ({what})"
            ));
            self.control_skipped = true;
        }
        true
    }

    pub(super) fn untick(&mut self, n: u64, ans: &Value, note: Option<&str>) {
        let mut a = json!({
            "untick": n, "comment": ans["comment"],
            "body": questions::untick(ans["body"].as_str().unwrap_or("")),
            "fail": format!("WARN: could not untick the question on #{n}"),
        });
        if let Some(note) = note {
            a["say"] = json!(note);
        }
        self.act(a);
    }

    /// Rule 4: the questions that wait for an answer. A box ticked to retry
    /// a parked cycle starts it, and the poll ends there.
    pub(super) fn questions(&mut self, room: &Room, bot: &str) -> Step {
        let fresh = self.plan_turns();
        let Some(asking) = self.s.asking.clone() else {
            self.need("asking");
            return Ok(());
        };
        if asking.get("failed") == Some(&json!(true)) {
            return Err(self.fail("ERROR: gh issue list failed", 1));
        }
        let Some(list) = asking.as_array() else {
            return Err(self.fail("ERROR: unreadable list of issues with a question", 1));
        };
        let mut numbers: Vec<(u64, Vec<String>)> = Vec::new();
        for i in list {
            let Some(n) = i.get("number").and_then(Value::as_u64) else {
                return Err(self.fail("ERROR: unreadable list of issues with a question", 1));
            };
            numbers.push((n, labels(i)));
        }
        numbers.sort_by_key(|(n, _)| *n);
        let asked: Vec<u64> = numbers.iter().map(|(n, _)| *n).collect();
        self.plan_over(&fresh, &asked);
        for (n, have) in numbers {
            if self.control_ask(n, "its question was answered") {
                continue;
            }
            let Some(q) = self.s.issues.get(&n).and_then(|i| i.question.clone()) else {
                self.need("question");
                continue;
            };
            let q = match q {
                Ok(q) => q,
                Err(e) if e.rc == 1 => {
                    self.log(format!(
                        "skip #{n}: its question on record could not be read ({})",
                        e.word
                    ));
                    continue;
                }
                Err(e) => {
                    return Err(self.fail(
                        format!("ERROR: the state store cannot be used: {}", e.line),
                        78,
                    ))
                }
            };
            let mut qkind = q
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut qid = match q.get("id") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(x)) => x.to_string(),
                _ => String::new(),
            };
            if qkind != "park" && qkind != "plan" {
                qid.clear();
                qkind.clear();
            }
            let Some(comments) = self.s.issues.get(&n).and_then(|i| i.comments.clone()) else {
                return Err(self.fail("ERROR: gh api graphql failed", 1));
            };
            let Some(mut ans) =
                questions::answer(&comments, bot, &self.c.allowed, &qid, &self.c.millwright)
            else {
                return Err(self.fail(format!("ERROR: unreadable comments of #{n}"), 1));
            };
            if ans["asked"] != json!(true) {
                if !qid.is_empty() {
                    self.log(format!(
                        "skip #{n}: its question is not among the last 100 comments"
                    ));
                }
                continue;
            }
            let kind = ans["kind"].as_str().unwrap_or("").to_string();
            if !qid.is_empty() && (ans["issue"] != json!(n) || kind != qkind) {
                self.log(format!(
                    "skip #{n}: the question on record does not belong to this issue"
                ));
                continue;
            }
            let mut tick = ans["tick"]["status"].as_str().unwrap_or("").to_string();
            let by = ans["tick"]["by"].as_str().unwrap_or("").to_string();
            if tick == "fixer" && kind != "plan" {
                tick = "stranger".into();
            }
            if tick == "stranger" || tick == "many" {
                self.log(format!(
                    "skip #{n}: boxes ticked by '{}' are no answer ({tick})",
                    or_unknown(&by)
                ));
                if !self.dry {
                    let note = if tick == "many" {
                        "More than one box was ticked, so nothing was chosen. Tick one.".to_string()
                    } else {
                        format!("A box was ticked by an account that may not answer for this factory ({}). It was unticked.", or_unknown(&by))
                    };
                    self.untick(n, &ans, Some(&note));
                }
                tick = "none".into();
                ans["tick"]["status"] = json!("none");
            }
            if kind == "plan" {
                // A turn this poll collects or posts asks a new question: the
                // one read before it answers nothing. A turn still in flight
                // posts none, and its issue is read as the bash reads it.
                let in_flight = self.s.sessions.iter().any(|s| s.issue == n && s.in_flight);
                if fresh.contains(&n) && !in_flight {
                    continue;
                }
                let row = q.get("plan").cloned().unwrap_or(Value::Null);
                self.plan_answer(n, &ans, &tick, &row, room)?;
                continue;
            }
            if kind != "park" || tick != "one" {
                continue;
            }
            self.park_ticked.push(n);
            if qkind != "park" {
                self.log(format!(
                    "skip #{n}: a box was ticked in a question this host has no record of"
                ));
                if !self.dry {
                    self.untick(n, &ans, Some("This question is not one the factory is waiting on. To retry, apply `tehas:ready`."));
                }
                continue;
            }
            // What was ticked is read from the label of the ticked box, which
            // questions::answer read from the comment's own line: the order of
            // the boxes decides nothing, and the two polls read the same list.
            // The third box is a retry as the first is; what differs is that
            // its record names no comment of the issue or of the pull request.
            let ticked = |label: &str| {
                ans["tick"]["index"]
                    .as_u64()
                    .and_then(|i| ans["labels"].get(usize::try_from(i).unwrap_or(usize::MAX)))
                    .is_some_and(|l| l.as_str().is_some_and(|s| s == label))
            };
            let clean = ticked(tehas_core::question::BOX_CLEAN);
            let issue_alone = ticked(tehas_core::question::BOX_ALONE);
            if !clean && !issue_alone {
                if !self.dry {
                    self.act(json!({"issue": n, "remove": ["tehas:question"],
                        "fail": Self::label_fail(n, &["tehas:question"], &[]),
                        "answered": {"id": qid, "by": by}}));
                }
                continue;
            }
            let parked = !has(&have, "tehas:running")
                && !has(&have, "tehas:awaiting-merge")
                && has(&have, "needs-human-attention");
            if !parked {
                self.log(format!(
                    "skip #{n}: a retry was ticked but the issue is not parked"
                ));
                if !self.dry {
                    self.untick(
                        n,
                        &ans,
                        Some("Retry was ticked, but this issue is not parked, so nothing was started."),
                    );
                }
                continue;
            }
            if !self.work_ok(n)? || !self.epic_ok(n, "tehas:ready", &have) {
                continue;
            }
            if has(&have, "cross-project") && has(&have, "tehas:improvement") {
                self.log(format!("skip #{n}: it carries cross-project and tehas:improvement, so its fix needs a change in another repository too, and one cycle here cannot make it"));
                continue;
            }
            let at = ans["tick"]["at"].as_str().unwrap_or("").to_string();
            if !self.unchanged_since(
                n,
                &at,
                &format!("{by} ticked retry"),
                "untick the box and tick it again",
            )? {
                continue;
            }
            if room.want == 0 {
                if self.dry {
                    self.log(room.hold.clone());
                }
                continue;
            }
            self.mesh_ok()?;
            if !self.image_ok(n)? || !self.image_missing_ok(n) {
                continue;
            }
            if self.dry {
                self.log(format!("would run a cycle for #{n} (retry ticked by {by})"));
                return Err(End(0));
            }
            let mut remove = vec!["tehas:question".to_string()];
            for l in ["tehas:ready", "tehas:millwright-escalated"] {
                if has(&have, l) {
                    remove.push(l.into());
                }
            }
            let mut extra = Map::new();
            extra.insert("comment".into(), ans["comment"].clone());
            extra.insert("since".into(), ans["asked_at"].clone());
            if issue_alone {
                extra.insert("issue_alone".into(), json!(true));
            }
            self.d.picked.push(n);
            self.act_gates(n);
            self.act_start(
                n,
                Start {
                    starter: by,
                    verb: "retry ticked".into(),
                    how: "retry tick".into(),
                    given_at: at,
                    given_how: "retry-tick".into(),
                    remove,
                    fixer: None,
                    extra,
                },
            );
            self.untick(n, &ans, None);
            if let Some(a) = self.d.actions.last_mut() {
                a["fail"] = json!(format!("WARN: could not untick the retry on #{n}; its question is no longer on record, so it starts nothing more"));
            }
            return Err(End(0));
        }
        Ok(())
    }

    /// mesh_ok: a model endpoint that does not answer ends the poll; a dry
    /// run says why.
    pub(super) fn mesh_ok(&mut self) -> Step {
        if self.s.endpoint_hold.is_empty() {
            return Ok(());
        }
        if self.dry {
            let h = self.s.endpoint_hold.clone();
            self.log(h);
        }
        Err(End(0))
    }

    /// Rule 4a: a cycle a provider outage parked starts again once, on the
    /// word of whoever started it.
    pub(super) fn outage(&mut self, room: &Room) -> Step {
        if room.want <= 0 {
            return Ok(());
        }
        let owed = match self.s.owed.clone() {
            None => {
                self.need("owed");
                return Ok(());
            }
            Some(Err(e)) => {
                let rc = if e.rc == 1 { 1 } else { 78 };
                return Err(self.fail(
                    format!(
                        "ERROR: the state store could not list the parks owed a retry ({})",
                        e.word
                    ),
                    rc,
                ));
            }
            Some(Ok(o)) => o,
        };
        for p in owed {
            let (Some(n), Some(started)) = (
                p.get("issue").and_then(Value::as_u64),
                p.get("started_at").and_then(Value::as_u64),
            ) else {
                continue;
            };
            if n == 0 || n > 9_999_999_999 || started > 99_999_999_999 {
                continue;
            }
            let by = p
                .get("started_by")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if self.running(n) {
                continue;
            }
            let Some(view) = self.s.issues.get(&n).and_then(|i| i.view.clone()) else {
                self.need("view");
                continue;
            };
            if view.get("failed") == Some(&json!(true)) {
                return Err(self.fail("ERROR: gh issue view failed", 1));
            }
            let have = labels(&view);
            if has(&have, "tehas:ready") {
                continue;
            }
            if self.control_ask(n, "a cycle its provider outage parked asked to start again") {
                continue;
            }
            let held = view.get("state").and_then(Value::as_str) != Some("OPEN")
                || !has(&have, "needs-human-attention")
                || has(&have, "tehas:running")
                || self.plan_open(n)
                || !self.c.allowed(&by)
                || !self.unchanged_since(
                    n,
                    &iso(started),
                    "its last cycle began",
                    "apply tehas:ready",
                )?;
            if held {
                self.log(format!("skip #{n}: its retry after a provider outage is not started; it waits for a person"));
                if !self.dry {
                    self.act(json!({"park_retry": n, "how": "outage"}));
                }
                continue;
            }
            if self.dry {
                self.log(format!(
                    "would run a cycle for #{n} (retry after a provider outage, started by {by})"
                ));
                return Err(End(0));
            }
            let remove = if has(&have, "tehas:question") {
                vec!["tehas:question".to_string()]
            } else {
                Vec::new()
            };
            self.act_gates(n);
            self.act(json!({"issue": n, "say": "Tehas starts this issue again on its own: its last cycle was parked by a provider outage, and the model endpoint answers again. It does this once. If the new cycle is parked the same way, it waits for a person."}));
            self.d.picked.push(n);
            self.act_start(
                n,
                Start {
                    starter: by,
                    verb: "requeued".into(),
                    how: "start of the cycle a provider outage parked".into(),
                    given_at: iso(started),
                    given_how: "outage-retry".into(),
                    remove,
                    fixer: None,
                    extra: Map::new(),
                },
            );
            return Err(End(0));
        }
        Ok(())
    }

    /// refresh_held: why no refresh may start on this host, empty when one
    /// may.
    pub(super) fn refresh_held(&mut self) -> Option<String> {
        if !self.c.refresh_conflicts {
            return Some("TEHAS_REFRESH_CONFLICTS is not 1".into());
        }
        match self.s.continues {
            None => {
                self.need("continues");
                None
            }
            Some(true) => Some(String::new()),
            Some(false) => Some(format!(
                "the pi-rukas in the sandbox image {} does not take --continue-pr",
                self.c.image
            )),
        }
    }

    fn refresh_drop(&mut self, n: u64, why: &str) -> Step<Option<Start>> {
        self.log(format!("drop the refresh of #{n}: {why}"));
        if !self.dry {
            self.act(json!({"refresh_drop": n}));
        }
        Ok(None)
    }

    /// refresh_say: a hold a person can act on, said once for each key.
    fn refresh_say(&mut self, n: u64, key: &str, why: &str) -> Step<Option<Start>> {
        self.refresh_drop(n, why)?;
        let Some(said) = self.s.issues.get(&n).and_then(|i| i.refresh_said.clone()) else {
            self.need("refresh_said");
            return Ok(None);
        };
        if !self.dry && said != key {
            self.act(json!({
                "issue": n, "refresh_said": key,
                "say": format!("Tehas does not refresh this issue's pull request on its own: {why}. It conflicts with the branch, and a person resolves it or applies `tehas:ready` to start a fresh cycle."),
            }));
        }
        Ok(None)
    }

    /// Rule 4b, refresh_ok: the start of a refresh request, or None when it
    /// waits or goes.
    pub(super) fn refresh_ok(&mut self, n: u64, bot: &str) -> Step<Option<Start>> {
        let Some(held) = self.refresh_held() else {
            return Ok(None);
        };
        if !held.is_empty() {
            if !self.refresh_said {
                self.log(format!("hold refreshes: {held}"));
            }
            self.refresh_said = true;
            return Ok(None);
        }
        let Some(req) = self.s.issues.get(&n).and_then(|i| i.request.clone()) else {
            self.need("request");
            return Ok(None);
        };
        if !(req.get("pr").is_some_and(Value::is_number)
            && hex40(req.get("head_sha"))
            && hex40(req.get("base_sha")))
        {
            return self.refresh_drop(n, "its request cannot be read");
        }
        let pr = req["pr"].as_u64().unwrap_or(0);
        if self.running(n) {
            return self.refresh_drop(n, "a cycle runs on it");
        }
        if self.plan_open(n) {
            return self.refresh_drop(n, "a planning session is open on it");
        }
        if !self.work_ok(n)? {
            return self.refresh_drop(n, "it is not an open issue");
        }
        let have = labels(&self.work(n));
        if !has(&have, "tehas:awaiting-merge") && !has(&have, "needs-human-attention") {
            return self.refresh_drop(
                n,
                "it carries neither tehas:awaiting-merge nor needs-human-attention",
            );
        }
        if has(&have, "needs-human-attention") && has(&have, "tehas:ready") {
            return self.refresh_drop(n, "it was handed off and carries tehas:ready");
        }
        for l in [
            "tehas:running",
            "tehas:millwright-escalated",
            "tehas:plan",
            "tehas:planning",
        ] {
            if has(&have, l) {
                return self.refresh_drop(n, &format!("it carries {l}"));
            }
        }
        let none = "this host has no approval of a person's that its pull request rests on";
        let record = match self.s.issues.get(&n).and_then(|i| i.approval.clone()) {
            None => {
                self.need("approval");
                return Ok(None);
            }
            Some(Err(())) => {
                self.log(format!("wait #{n}: its approval could not be read"));
                return Ok(None);
            }
            Some(Ok(None)) => return self.refresh_drop(n, none),
            Some(Ok(Some(r))) => r,
        };
        let Some(rests) =
            fixer_base(&record).filter(|b| self.c.allowed(b["by"].as_str().unwrap_or("")))
        else {
            return self.refresh_drop(n, none);
        };
        let count = refresh_count(&record, &rests);
        let by = rests["by"].as_str().unwrap_or("").to_string();
        let at = rests["at"].as_str().unwrap_or("").to_string();
        if !self.unchanged_since(
            n,
            &at,
            &format!("{by} approved it"),
            "a person applies tehas:ready",
        )? {
            return self.refresh_drop(
                n,
                &format!("its title or text was changed since {by} approved it"),
            );
        }
        if text::body_sha256(&self.text(n)) != rests["body_sha256"].as_str().unwrap_or("") {
            return self.refresh_drop(n, &format!("its text is not the text {by} approved"));
        }
        let Some(node) = self.s.pulls.get(&pr).cloned() else {
            self.need("pull");
            return Ok(None);
        };
        if node.get("failed") == Some(&json!(true)) {
            return Err(self.fail("ERROR: gh api graphql failed", 1));
        }
        if !node.is_object() {
            return self.refresh_drop(n, &format!("pull request #{pr} cannot be read"));
        }
        let number = node
            .get("number")
            .map(|x| x.to_string())
            .unwrap_or("null".into());
        let rule = refresh_why(&node, bot, &self.c.repo, self.c.control);
        let st = |k: &str| node.get(k).and_then(Value::as_str).unwrap_or("");
        let why = if st("state") != "OPEN" {
            format!("pull request #{number} is not open")
        } else if !rule.is_empty() {
            format!("pull request #{number}: {rule}")
        } else if node
            .pointer("/closingIssuesReferences/nodes/0/number")
            .and_then(Value::as_u64)
            != Some(n)
        {
            format!("pull request #{number} does not close #{n}")
        } else if node.get("headRefOid") != req.get("head_sha") {
            format!("pull request #{number} moved since the request")
        } else if st("mergeable") == "UNKNOWN" {
            "wait".into()
        } else if st("mergeable") != "CONFLICTING" {
            format!("pull request #{number} no longer conflicts")
        } else if node
            .pointer("/labels/nodes")
            .and_then(Value::as_array)
            .is_some_and(|a| {
                a.iter()
                    .any(|l| l.get("name").and_then(Value::as_str) == Some("tehas:refreshing"))
            })
        {
            "busy".into()
        } else {
            String::new()
        };
        match why.as_str() {
            "" => {}
            "wait" => {
                self.log(format!(
                    "wait #{n}: GitHub has not said yet whether pull request #{pr} conflicts"
                ));
                return Ok(None);
            }
            "busy" => {
                self.log(format!(
                    "wait #{n}: pull request #{pr} carries tehas:refreshing"
                ));
                return Ok(None);
            }
            w => return self.refresh_drop(n, w),
        }
        let Some(count) = count else {
            return self.refresh_drop(
                n,
                "its approval record does not say how many refreshes there were",
            );
        };
        if count as i64 >= self.c.refresh_max {
            return self.refresh_say(
                n,
                &format!("cap {at} {count}"),
                &format!("it was refreshed {count} times since {by} approved it, the limit"),
            );
        }
        let history = match self.s.issues.get(&n).and_then(|i| i.history.clone()) {
            None => {
                self.need("history");
                return Ok(None);
            }
            Some(Err(())) => {
                self.log(format!("wait #{n}: its history could not be read"));
                return Ok(None);
            }
            Some(Ok(h)) => h,
        };
        // The last of the holds: a refresh that parked since the approval is
        // not offered again. One whose history cannot be read waits, above.
        let (parked, unread) = parked(&history, &at);
        if parked {
            let why = if unread {
                format!("a refresh of it since {by} approved it parked, its start unread, so it is held")
            } else {
                format!("a refresh of it since {by} approved it parked")
            };
            return self.refresh_say(n, &format!("park {at}"), &why);
        }
        if !self.image_ok(n)? || !self.image_missing_ok(n) {
            return Ok(None);
        }
        let branch: String = node
            .get("baseRefName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
            .collect();
        let url = Some(st("url"))
            .filter(|u| {
                u.starts_with("https://")
                    && u[8..].bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'/' | b'_' | b'-')
                    })
                    && u.len() > 8
            })
            .unwrap_or("");
        let mut remove = Vec::new();
        for l in ["tehas:awaiting-merge", "tehas:question", "tehas:ready"] {
            if has(&have, l) {
                remove.push(l.to_string());
            }
        }
        let mut extra = Map::new();
        extra.insert("refresh_pr".into(), json!(pr));
        extra.insert("refresh_url".into(), json!(url));
        extra.insert(
            "refresh_of".into(),
            json!({"pr": pr, "head_sha": req["head_sha"], "base_sha": req["base_sha"],
                "branch": if branch.is_empty() { self.c.branch.clone() } else { branch }}),
        );
        Ok(Some(Start {
            starter: by,
            verb: "refresh".into(),
            how: format!("approval of {at}"),
            given_at: at,
            given_how: "refresh".into(),
            remove,
            fixer: None,
            extra,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_counts_on_the_same_approval_only() {
        let base = json!({"at": "2026-10-01T00:00:00Z", "by": "alice"});
        let r = json!({"how": "refresh", "refreshes": 2, "rests_on": {"at": "2026-10-01T00:00:00Z", "by": "alice"}});
        assert_eq!(refresh_count(&r, &base), Some(2));
        assert_eq!(refresh_count(&json!({"how": "label"}), &base), Some(0));
        let other = json!({"how": "refresh", "refreshes": 2, "rests_on": {"at": "2026-09-01T00:00:00Z", "by": "alice"}});
        assert_eq!(refresh_count(&other, &base), Some(0));
        let bad = json!({"how": "refresh", "refreshes": 0, "rests_on": base});
        assert_eq!(refresh_count(&bad, &base), None);
    }

    #[test]
    fn a_park_holds_a_refresh_only_from_the_approval_on() {
        let at = "2026-10-01T00:00:00Z";
        let park = |started: Value| json!({"how": "refresh", "parked": true, "started": started});
        // [why, row, holds]: one row, and whether it holds a refresh.
        let cases: &[(&str, Value, bool)] = &[
            (
                "a park from the approval's second on",
                park(json!(1790899201)),
                true,
            ),
            (
                "a park before the approval: of an older call",
                park(json!(1790769600)),
                false,
            ),
            ("a park with no start on record", park(Value::Null), true),
            (
                "a park whose row has no start at all",
                json!({"how": "refresh", "parked": true}),
                true,
            ),
            (
                "a start that is not a count of seconds",
                park(json!("1790899201")),
                true,
            ),
            ("a start that is negative", park(json!(-1)), true),
            (
                "a start past the last date iso writes",
                park(json!(100_000_000_000u64)),
                true,
            ),
            (
                "a park that is not JSON true",
                json!({"how": "refresh", "parked": "true", "started": 1790899201}),
                false,
            ),
            (
                "a park that is not parked",
                json!({"how": "refresh", "parked": false, "started": 1790899201}),
                false,
            ),
            (
                "the park of a cycle that was no refresh",
                json!({"how": "label", "parked": true, "started": 1790899201}),
                false,
            ),
        ];
        for (why, row, holds) in cases {
            // A park held on a start the rule cannot read is told apart from
            // one it dated, so the poll can say which it answered: a row that
            // gives no start, or one the bound has just struck out.
            let readable = row
                .get("started")
                .and_then(Value::as_u64)
                .is_some_and(|s| s <= MAX_START);
            assert_eq!(
                parked(std::slice::from_ref(row), at),
                (*holds, *holds && !readable),
                "{why}"
            );
        }
        assert!(!parked(&[], at).0);
    }

    #[test]
    fn an_approval_that_names_no_instant_orders_no_park() {
        // The rule orders a park's start against the approval's stamp as text,
        // so a stamp of any other shape holds, and none at all is no date to
        // measure a park from.
        let row = |started| json!({"how": "refresh", "parked": true, "started": started});
        for at in ["2026-10-01Z", "2026-10-1", "Z", "2026-10-01T00:00:00"] {
            assert!(parked(&[row(1790769600)], at).0, "{at}");
        }
        assert!(!parked(&[row(1790769600)], "").0);
    }
}
