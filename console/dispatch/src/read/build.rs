//! snapshot(): what one poll would read, in the JSON shape Snapshot::parse
//! takes. GitHub in one paged query and the per-issue reads the rules make of
//! the issues they decide about, the store's read verbs, and this host's files
//! and locks. Nothing is written.

use super::run::{self, Calls, GhFail};
use super::{github, host, models, store};
use crate::events::{pause_holds, Events};
use crate::settings::Settings;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

/// Where the snapshot reads from, and with what.
pub struct Sources<'a> {
    pub home: &'a Path,
    pub calls: Calls,
    pub systemctl: String,
    pub now: i64,
    pub models: models::Models,
    /// The subagent provider's endpoint, url and key, as the probe found it.
    pub endpoint: (String, String),
    /// Whether the low gate may be run: a poll's, never the shadow's, which
    /// the poll has run already.
    pub gate: bool,
}

/// The facts a decision names in needs that are read only when it asks:
/// the sandbox image's and the model's answers to images, and whether the
/// image takes --continue-pr. Returns whether any was read.
pub fn fill(src: &Sources, snap: &mut Value, needs: &[&str]) -> bool {
    let mut read = false;
    if needs.contains(&"images") && snap.get("images").is_none() {
        snap["images"] = models::images(&src.models, &src.endpoint.0, &src.endpoint.1);
        read = true;
    }
    if needs.contains(&"continues") && snap.get("continues").is_none() {
        snap["continues"] = json!(models::continues());
        read = true;
    }
    if needs.contains(&"gate") && snap.get("gate").is_none() && src.gate {
        snap["gate"] = low_gate(src.home);
        read = true;
    }
    read
}

/// low_gate's run: TEHAS_LOW_GATE in an empty environment but PATH, its
/// output in logs/low-gate.log, bounded by TEHAS_LOW_GATE_WAIT; its exit
/// status as timeout gives it (124 out of time, 126 not runnable, 127 not
/// there, 128 and the signal), and state/low-gate.held as the bash reads it:
/// a link is "other", anything else there is "file".
fn low_gate(home: &Path) -> Value {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let file = home.join("state/low-gate.held");
    let held = match std::fs::symlink_metadata(&file) {
        Err(_) => "",
        Ok(m) if m.file_type().is_symlink() => "other",
        Ok(_) => "file",
    };
    let gate = std::env::var("TEHAS_LOW_GATE").unwrap_or_default();
    // lib/config.sh's default.
    let wait = std::env::var("TEHAS_LOW_GATE_WAIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    // A log that cannot be written is a redirection that failed: the shell
    // answers 1 and runs nothing.
    let Ok(out) = std::fs::File::create(home.join("logs/low-gate.log")) else {
        return json!({"answer": 1, "held": held});
    };
    let Ok(err) = out.try_clone() else {
        return json!({"answer": 1, "held": held});
    };
    let child = Command::new(&gate)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0)
        .spawn();
    let answer = match child {
        // EACCES and ENOEXEC: there, and not runnable.
        Err(e)
            if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(8) =>
        {
            126
        }
        Err(_) => 127,
        // As timeout -k 10: TERM at the bound, KILL ten seconds on.
        Ok(mut c) => run::wait(
            &mut c,
            Duration::from_secs(wait),
            Duration::from_secs(10),
            true,
        )
        .unwrap_or(124),
    };
    json!({"answer": answer, "held": held})
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
    // As $(head -c 1000) leaves it: no final newlines.
    s["branch_note"] = json!(host::head(&home.join("state/branch-check"), 1000)
        .trim_end_matches('\n')
        .to_string());
    s["refresh_held"] = json!(host::head(&home.join("state/refresh/.held"), 300));
    s["refine_brief"] = json!(Path::new(&c.libexec)
        .join("../lib/refine-brief.md")
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
    // Those the page left out are read as the bash reads them, side by side.
    // The bot's `RUNNING` mark is the one label the queue does not ask about:
    // every running cycle carries it, the bot applies it alone, and the rules
    // read the mark and the lock file rather than its events. The control
    // issue is asked about by name below, with no label exempt.
    let dropped: Vec<u64> = q
        .open
        .iter()
        .filter(|o| github::dropped_events(o, &[github::RUNNING]))
        .map(|o| o.number)
        .collect();
    let reads: Vec<Vec<String>> = dropped
        .iter()
        .map(|n| {
            vec![
                "api".into(),
                format!("repos/{}/issues/{n}/events?per_page=100", c.repo),
                "--paginate".into(),
            ]
        })
        .collect();
    for (n, r) in dropped.iter().zip(calls.gh_all(&reads)) {
        issues[&n.to_string()]["events"] = r.unwrap_or_else(|_| json!({"failed": true}));
    }
    match q.open.iter().find(|o| o.number == c.control) {
        Some(o) => {
            // The control issue asks about every label it carries, so the
            // mark's exemption cannot reach it: the page's timeline is kept
            // only when the guard vouches for it, and one cut short of the
            // last `tehas:pause` event is read again rather than decide the
            // pause. A page that left the control issue's events out entirely
            // is already in `dropped` and holds its REST read. The guard runs
            // again here, with nothing exempt, and its answer for the queue is
            // kept beside the issue: the two differ only for an issue whose
            // labels are the mark alone, and reading the timeline once and
            // sharing it would give the control issue the queue's exemption.
            let events = if github::dropped_events(o, &[]) {
                issues
                    .get(&o.number.to_string())
                    .and_then(|i| i.get("events"))
                    .cloned()
                    .unwrap_or_else(|| github::events(&o.node))
            } else {
                github::events(&o.node)
            };
            s["control"] = json!({"labels": o.labels.iter().map(|l| json!({"name": l})).collect::<Vec<_>>(),
            "events": events})
        }
        None => {
            let n = c.control.to_string();
            let labels = calls
                .gh(&["issue", "view", &n, "--repo", &c.repo, "--json", "labels"])
                .ok()
                .and_then(|v| v.get("labels").cloned())
                .unwrap_or_else(|| json!({"failed": true}));
            s["control"] =
                json!({"labels": labels, "events": rest_events(calls, &c.repo, c.control)});
        }
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
        .filter(|o| has(&o.labels, github::RUNNING))
        .map(|o| json!({"number": o.number, "state": "OPEN"}))
        .collect();
    marked.extend(
        q.closed_running
            .iter()
            .map(|n| json!({"number": n, "state": "CLOSED"})),
    );
    s["marked"] = Value::Array(marked);
    s["planning"] = q
        .open
        .iter()
        .filter(|o| has(&o.labels, "tehas:planning"))
        .map(|o| json!(o.number))
        .collect();

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
    let mut refresh_rows: Vec<u64> = Vec::new();
    if !locks.is_empty() {
        match store::cycles(calls) {
            Ok(records) => {
                // took_how holds every row the store lists, held or not.
                refresh_rows = records
                    .iter()
                    .filter(|r| r["how"] == "refresh")
                    .filter_map(|r| r["issue"].as_u64())
                    .collect();
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
    let (endpoint, url, key) = models::endpoint(&src.models);
    s["endpoint"] = endpoint;
    src.endpoint = (url, key);
    let work_dir = std::env::var("TEHAS_WORK_DIR").unwrap_or_default();
    let user_home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    s["leftovers"] = host::leftovers(home, &work_dir, &user_home, src.now);
    s["disk_kb"] = host::disk(home);

    // The per-issue reads the snapshot makes, gathered before any of them is
    // asked: the rules name the issues they ask the store about, and all of
    // those names are known here from the queue, the locks and the requests. One
    // run of `store batch` answers the whole list (#941), where the poll used to
    // start the store once per question -- 49 processes for one idle poll. The
    // list is not deduped here: `gather` sorts and dedups it before it asks.
    let mut per_issue: Vec<store::Read> = Vec::new();
    // A function rather than a closure: the list is whatever the caller has, and
    // a closure cannot name its type parameter.
    fn want<I: IntoIterator<Item = store::Read>>(per_issue: &mut Vec<store::Read>, reads: I) {
        per_issue.extend(reads);
    }

    // The running cycles a fed start began count against the feed's cap.
    if c.millwright_feed {
        want(
            &mut per_issue,
            locks.iter().copied().map(store::Read::Approval),
        );
    }

    // Refreshes: the history, the marks, the requests and what they read.
    let history = store::refreshed(calls);
    let mut from: Vec<u64> = history
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    from.extend(&refresh_rows);
    from.sort_unstable();
    from.dedup();
    want(
        &mut per_issue,
        from.iter().copied().map(store::Read::Approval),
    );
    // A refresh row with no lock counts as the bash's took_how does: in the
    // issues a refresh of this host may own.
    s["refresh_history"] = match history {
        Value::Array(_) => json!(from),
        e => e,
    };
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
    for pr in s["refreshing"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|p| p["number"].as_u64())
    {
        entry(&mut issues, pr)["events"] = rest_events(calls, &c.repo, pr);
    }
    let requests = host::requests(home);
    want(
        &mut per_issue,
        requests.iter().map(|(n, _)| store::Read::Approval(*n)),
    );
    // The asking issues, the candidates of rule 6, and what each owes.
    let asking = numbers(&s["asking"]);
    want(
        &mut per_issue,
        asking
            .iter()
            .flat_map(|n| [store::Read::Question(*n), store::Read::Park(*n)]),
    );
    let mut candidates: BTreeSet<u64> = numbers(&s["ready"]).into_iter().collect();
    candidates.extend(numbers(&s["blocked"]));
    let candidates: Vec<u64> = candidates.into_iter().collect();
    want(
        &mut per_issue,
        candidates
            .iter()
            .flat_map(|n| [store::Read::Park(*n), store::Read::Approval(*n)]),
    );
    // The driver of a retry is on its record, whichever rule starts it. Only an
    // issue that carries the label costs a read.
    let lean: Vec<u64> = q
        .open
        .iter()
        .filter(|o| has(&o.labels, crate::decide::LEAN))
        .map(|o| o.number)
        .collect();
    want(
        &mut per_issue,
        lean.iter().copied().map(store::Read::Approval),
    );
    // One run of the store answers all of them, and the answers are read below
    // as the single calls were read. A poll that ends at the pause below pays
    // this batch for the refresh reads above the pause and nothing else: the
    // rules after it are not read at all.
    let mut asked = store::Asked::empty();
    store::gather(calls, &mut asked, &per_issue);

    for n in &from {
        let (_, current) = store::approval(&asked, *n);
        entry(&mut issues, *n)["current"] = current;
    }
    for n in lean {
        entry(&mut issues, n)["approval"] = store::approval(&asked, n).0;
    }
    if c.millwright_feed {
        s["fed"] = store::fed(&asked, &locks);
    }
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
        let (shaped, _) = store::approval(&asked, *n);
        let i = entry(&mut issues, *n);
        i["approval"] = shaped;
        i["history"] = store::history(calls, *n);
        if let Some(pr) = req.get("pr").and_then(Value::as_u64) {
            let p = github::pull(calls, &c.repo, pr);
            s["pulls"][pr.to_string()] = p;
        }
    }

    // A paused poll ends after its clean-up: what only the rules after the
    // pause read is not read.
    if paused(&s, c) {
        s["issues"] = Value::Object(issues);
        return Ok(s);
    }

    // The questions waiting for an answer.
    let read = github::comments_of(calls, &c.repo, &asking);
    for (n, comments) in asking.into_iter().zip(read) {
        let question = store::question(&asked, n);
        let i = entry(&mut issues, n);
        i["question"] = question;
        i["comments"] = comments;
    }

    // The candidates of rule 6: what their start rests on.
    for n in candidates {
        let labels = q
            .open
            .iter()
            .find(|o| o.number == n)
            .map(|o| o.labels.clone())
            .unwrap_or_default();
        let park = store::park(&asked, n);
        let (shaped, _) = store::approval(&asked, n);
        let i = entry(&mut issues, n);
        i["park"] = park;
        i["approval"] = shaped;
        if has(&labels, "tehas:blocked") {
            let comments = match i.get("comments") {
                Some(c) => c.clone(),
                None => github::comments(calls, &c.repo, n),
            };
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

/// Whether the poll stops at the pause, as decide's pause_flag and
/// pause_label find it. A read that failed is not a pause: decide ends the
/// poll on it.
fn paused(s: &Value, c: &Settings) -> bool {
    if !s["pause_flag"].is_null() {
        return true;
    }
    let control = &s["control"];
    if control["labels"]
        .as_array()
        .is_some_and(|l| l.iter().any(|x| x["name"] == "tehas:pause"))
    {
        return true;
    }
    match Events::parse(&control["events"]) {
        Events::Read(events) => pause_holds(&events, &|l| c.allowed(l)).is_some(),
        _ => false,
    }
}

/// An issue the rules ask about that is not open: closed, as far as the poll
/// can tell without asking.
fn closed(issues: &mut Map<String, Value>, n: u64) {
    let i = entry(issues, n);
    let defaults = json!({"work": {"number": n, "state": "closed"}, "view": {"state": "CLOSED", "labels": []},
        "edit": {"lastEditedAt": null, "editor": null}, "events": [], "prs": []});
    for (k, v) in defaults.as_object().into_iter().flatten() {
        if i.get(k).is_none() {
            i[k.as_str()] = v.clone();
        }
    }
}

/// The label events of an issue or pull request as the bash reads them:
/// the REST events, all pages; {"failed": true} when the call failed.
fn rest_events(calls: &mut Calls, repo: &str, n: u64) -> Value {
    let path = format!("repos/{repo}/issues/{n}/events?per_page=100");
    calls
        .gh(&["api", &path, "--paginate"])
        .unwrap_or_else(|_| json!({"failed": true}))
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
