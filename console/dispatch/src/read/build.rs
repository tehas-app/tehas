//! snapshot(): what one poll would read, in the JSON shape Snapshot::parse
//! takes. GitHub in one paged query and the per-issue reads the rules make of
//! the issues they decide about, the store's read verbs, and this host's files
//! and locks. Nothing is written.

use super::run::{Calls, GhFail};
use super::{github, host, store};
use crate::settings::Settings;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

/// Where the snapshot reads from, and with what.
pub struct Sources<'a> {
    pub home: &'a Path,
    pub calls: Calls,
    pub systemctl: String,
    pub now: i64,
}

fn has(labels: &[String], l: &str) -> bool {
    labels.iter().any(|x| x == l)
}

fn listed(open: &[github::Open], label: &str) -> Value {
    Value::Array(
        open.iter()
            .filter(|o| has(&o.labels, label))
            .map(|o| {
                json!({"number": o.number,
                    "labels": o.labels.iter().map(|l| json!({"name": l})).collect::<Vec<_>>()})
            })
            .collect(),
    )
}

/// The snapshot, or the line the bash poll logs when a read it cannot do
/// without failed.
pub fn snapshot(src: &mut Sources, c: &Settings) -> Result<Value, GhFail> {
    let home = src.home;
    let mut s = json!({"now": src.now, "hostname": host::hostname()});
    s["pause_flag"] = host::pause(home);
    s["limit"] = host::limit(home, src.now);
    s["last_why"] = json!(host::last_why(home));
    s["branch_note"] = json!(host::head(&home.join("state/branch-check"), 4096));
    s["refresh_held"] = json!(host::head(&home.join("state/refresh/.held"), 300));
    s["refine_brief"] = json!(home
        .join("lib/refine-brief.md")
        .metadata()
        .is_ok_and(|m| m.len() > 0));

    let calls = &mut src.calls;
    let q = github::queue(calls, &c.repo)?;
    s["repo"] = json!(q.repo);
    s["user"] = if c.bot_login.is_empty() {
        calls
            .gh(&["api", "user"])
            .unwrap_or_else(|_| json!({"failed": true}))
    } else {
        json!({"login": c.bot_login})
    };
    let open: BTreeSet<u64> = q.open.iter().map(|o| o.number).collect();
    let mut issues = Map::new();
    for o in &q.open {
        issues.insert(o.number.to_string(), Value::Object(github::issue(o)));
    }
    match q.open.iter().find(|o| o.number == c.control) {
        Some(o) => {
            s["control"] = json!({"labels": o.labels.iter().map(|l| json!({"name": l})).collect::<Vec<_>>(),
            "events": github::events(&o.node)})
        }
        None => s["control"] = json!({"labels": [], "events": []}),
    }
    for (key, label) in [
        ("ready", "tehas:ready"),
        ("blocked", "tehas:blocked"),
        ("asking", "tehas:question"),
        ("plans", "tehas:plan"),
        ("refines", "tehas:refine"),
    ] {
        s[key] = listed(&q.open, label);
    }
    s["awaiting"] = Value::Array(
        q.open
            .iter()
            .filter(|o| has(&o.labels, "tehas:awaiting-merge"))
            .map(|o| json!({"number": o.number}))
            .collect(),
    );
    let mut marked: Vec<Value> = q
        .open
        .iter()
        .filter(|o| has(&o.labels, "tehas:running"))
        .map(|o| json!({"number": o.number, "state": "OPEN"}))
        .collect();
    marked.extend(
        q.closed_running
            .iter()
            .map(|n| json!({"number": n, "state": "CLOSED"})),
    );
    s["marked"] = Value::Array(marked);

    // The store and the locks.
    let sessions = store::sessions(calls);
    if let Some(list) = sessions.as_array() {
        let list: Vec<Value> = list
            .iter()
            .map(|x| {
                let mut x = x.clone();
                if x.get("running") == Some(&json!(true)) {
                    let n = x["issue"].as_u64().unwrap_or(0);
                    let (flight, rc) = host::turn(home, n, &src.systemctl);
                    x["in_flight"] = json!(flight);
                    x["rc"] = json!(rc);
                }
                x
            })
            .collect();
        s["plan_open"] = Value::Array(list);
    } else {
        s["plan_open"] = sessions;
    }
    let locks = host::running(home);
    let mut running: Vec<Value> = locks
        .iter()
        .map(|n| json!({"issue": n, "how": ""}))
        .collect();
    if !locks.is_empty() {
        match store::cycles(calls) {
            Ok(records) => {
                for r in running.iter_mut() {
                    if let Some(rec) = records.iter().find(|x| x["issue"] == r["issue"]) {
                        *r = rec.clone();
                    }
                }
            }
            Err(e) => s["cycles_failed"] = e,
        }
        s["bot_prs"] = github::list(
            calls,
            &[
                "pr",
                "list",
                "--repo",
                &c.repo,
                "--author",
                &bot(&s),
                "--state",
                "all",
                "--json",
                "url,state,body,headRefName,createdAt",
                "--limit",
                "100",
            ],
        );
    }
    for n in &locks {
        if !open.contains(n) {
            closed(&mut issues, *n);
        }
    }
    s["running"] = Value::Array(running.clone());
    s["slice"] = host::slice(home, &locks);
    s["disk_kb"] = host::disk(home);

    // Refreshes: the history, the marks, the requests and what they read.
    let history = store::refreshed(calls);
    let mut from: Vec<u64> = history
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    from.extend(
        running
            .iter()
            .filter(|r| r["how"] == "refresh")
            .filter_map(|r| r["issue"].as_u64()),
    );
    s["refresh_history"] = history;
    if c.refresh_conflicts || !from.is_empty() {
        s["refreshing"] = github::list(
            calls,
            &[
                "pr",
                "list",
                "--repo",
                &c.repo,
                "--state",
                "open",
                "--label",
                "tehas:refreshing",
                "--limit",
                "1000",
                "--json",
                "number,labels",
            ],
        );
    }
    for n in &from {
        let (_, current) = store::approval(calls, *n);
        entry(&mut issues, *n)["current"] = current;
    }
    let requests = host::requests(home);
    s["refresh_requests"] = json!(requests.iter().map(|(n, _)| n).collect::<Vec<_>>());
    for (n, req) in &requests {
        if !open.contains(n) {
            closed(&mut issues, *n);
        }
        let i = entry(&mut issues, *n);
        i["request"] = req.clone();
        i["refresh_said"] = json!(host::head(
            &home.join(format!("state/refresh/{n}.said")),
            200
        ));
        let (shaped, _) = store::approval(calls, *n);
        let i = entry(&mut issues, *n);
        i["approval"] = shaped;
        i["history"] = store::history(calls, *n);
        if let Some(pr) = req.get("pr").and_then(Value::as_u64) {
            let p = github::pull(calls, &c.repo, pr);
            s["pulls"][pr.to_string()] = p;
        }
    }

    // The questions waiting for an answer.
    for n in numbers(&s["asking"]) {
        let question = store::question(calls, n);
        let comments = github::comments(calls, &c.repo, n);
        let i = entry(&mut issues, n);
        i["question"] = question;
        i["comments"] = comments;
    }

    // The candidates of rule 6: what their start rests on.
    let mut candidates: BTreeSet<u64> = numbers(&s["ready"]).into_iter().collect();
    candidates.extend(numbers(&s["blocked"]));
    for n in candidates {
        let labels = q
            .open
            .iter()
            .find(|o| o.number == n)
            .map(|o| o.labels.clone())
            .unwrap_or_default();
        let park = store::park(calls, n);
        let (shaped, _) = store::approval(calls, n);
        let i = entry(&mut issues, n);
        i["park"] = park;
        i["approval"] = shaped;
        if has(&labels, "tehas:blocked") && i.get("comments").is_none() {
            let comments = github::comments(calls, &c.repo, n);
            let named = crate::decide::named_blockers(&comments);
            let states = github::blocker_states(calls, &c.repo, &named);
            let i = entry(&mut issues, n);
            i["comments"] = comments;
            i["blockers"] = Value::Object(states);
        }
    }

    // The parks owed a retry or a refine, and what they read of the issue.
    s["owed"] = store::owed(calls, "--automatic");
    s["refines_owed"] = store::owed(calls, "--refine");
    for key in ["owed", "refines_owed"] {
        for n in s[key]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|p| p["issue"].as_u64())
        {
            if !open.contains(&n) {
                closed(&mut issues, n);
            }
        }
    }

    if !c.branch_check.is_empty() {
        let path = format!(
            "repos/{}/commits/{}/check-runs?check_name={}&filter=latest",
            c.repo,
            c.branch,
            uri(&c.branch_check)
        );
        s["checks"] = calls
            .gh(&["api", &path])
            .unwrap_or_else(|_| json!({"failed": true}));
    }
    s["issues"] = Value::Object(issues);
    Ok(s)
}

/// An issue the rules ask about that is not open: closed, as far as the poll
/// can tell without asking.
fn closed(issues: &mut Map<String, Value>, n: u64) {
    issues.entry(n.to_string()).or_insert_with(|| {
        json!({"work": {"number": n, "state": "closed"}, "view": {"state": "CLOSED", "labels": []},
            "edit": {"lastEditedAt": null, "editor": null}, "events": [], "prs": []})
    });
}

fn bot(s: &Value) -> String {
    s.pointer("/user/login")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn numbers(list: &Value) -> Vec<u64> {
    list.as_array()
        .map(|a| a.iter().filter_map(|i| i["number"].as_u64()).collect())
        .unwrap_or_default()
}

fn entry(issues: &mut Map<String, Value>, n: u64) -> &mut Value {
    issues.entry(n.to_string()).or_insert_with(|| json!({}))
}

/// jq's @uri.
fn uri(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
