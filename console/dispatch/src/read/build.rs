//! snapshot(): what one poll would read, in the JSON shape Snapshot::parse
//! takes. GitHub in one paged query and the per-issue reads the rules make of
//! the issues they decide about, the store's read verbs, and this host's files
//! and locks. Nothing is written.

use super::run::{self, Calls, GhFail};
use super::{github, host, models, store};
use crate::decide::has;
use crate::events::{pause_holds, Events};
use crate::settings::Settings;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
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

/// The snapshot, or the line the poll logs when a read it cannot do
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
    // What the titles of the open pull requests rest on (console pr-title).
    if let Some(pulls) = &q.open_pulls {
        s["open_pulls"] = pulls.clone();
        s["open_labels"] = q
            .open
            .iter()
            .map(|o| (o.number.to_string(), json!(o.labels)))
            .collect::<Map<_, _>>()
            .into();
    }
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
    for n in &candidates {
        if let Some(record) = host::plan_start(home, *n) {
            entry(&mut issues, *n)["plan_start"] = record;
        }
    }
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
    let mut asked_of: BTreeMap<u64, Value> = BTreeMap::new();
    for (n, comments) in asking.iter().zip(read) {
        let question = store::question(&asked, *n);
        let i = entry(&mut issues, *n);
        i["question"] = question;
        i["comments"] = comments.clone();
        asked_of.insert(*n, comments);
    }

    // The blocked candidates read their comments and their blockers too, and
    // both reads are asked of the whole list rather than of one issue at a
    // time: one run of the comment read answers every blocked candidate, and
    // one call reads the states of the blockers the whole queue names (#1161).
    let (comments, states) = blocked_reads(calls, &c.repo, &q, &candidates, &asked_of);

    // The candidates of rule 6: what their start rests on. Membership of the
    // blocked batch is decided once, in `blocked_reads`, and the rule's own test
    // is the same predicate, so the two cannot disagree about an issue.
    for n in candidates {
        let park = store::park(&asked, n);
        let (shaped, _) = store::approval(&asked, n);
        // The millwright's note may lift a park's hold. A parked issue that
        // asks nothing has had no comment read.
        let lifts = park.is_object()
            && park.get("failed").is_none()
            && !c.millwright.is_empty()
            && (c.work_images || c.work_files);
        let i = entry(&mut issues, n);
        i["park"] = park;
        i["approval"] = shaped;
        if !is_blocked(&q, n) {
            if lifts && i.get("comments").is_none() {
                i["comments"] = github::comments(calls, &c.repo, n);
            }
            continue;
        }
        // The comments are the batch's, or the question read's when that read
        // answered the issue first; `blocked_reads` paid for the one it did not.
        let read = comments.get(&n).cloned();
        let held = issues
            .get(&n.to_string())
            .and_then(|i| i.get("comments").cloned());
        let answer = read.as_ref().or(held.as_ref()).unwrap_or_else(|| {
            comments
                .get(&n)
                .expect("the batch answered every blocked candidate")
        });
        let own = crate::decide::named_blockers(answer);
        let i = entry(&mut issues, n);
        i["comments"] = read.or(held).expect("a blocked issue's comments were read");
        // A number GitHub gave no state for is absent, which is what decide
        // reads as "the state of #b could not be read": not the same as naming
        // no blocker at all, which would let the issue start.
        i["blockers"] = Value::Object(
            own.iter()
                .filter_map(|b| {
                    states
                        .get(b)
                        .map(|s| (b.to_string(), Value::String(s.clone())))
                })
                .collect(),
        );
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

/// Whether `n` is held by `tehas:blocked` on the queue's page. The page is the
/// one source of the label, so an issue it did not answer for is not blocked:
/// it is read as no other rule reads it, rather than written as blocked with
/// nothing behind it.
fn is_blocked(q: &github::Queue, n: u64) -> bool {
    q.open
        .iter()
        .any(|o| o.number == n && has(&o.labels, crate::decide::BLOCKED))
}

/// The blocked candidates of rule 6, and the two reads they cost: the comments
/// of each, and the states of the blockers those comments name (#1161).
///
/// Membership is decided once, by `is_blocked`, so the batch and the rule that
/// consumes it cannot disagree about which issue is blocked.
///
/// An issue the question read already answered keeps that answer and pays no
/// second call for it; the rule reads its comments out of the issue. The
/// comment read answers every number it was given, in order, so a missing
/// answer is a broken contract rather than a value to paper over. A queue that
/// names no blocker is a list of none, and the poll pays no call for it.
fn blocked_reads(
    calls: &mut Calls,
    repo: &str,
    q: &github::Queue,
    candidates: &[u64],
    asked_of: &BTreeMap<u64, Value>,
) -> (BTreeMap<u64, Value>, BTreeMap<u64, String>) {
    let blocked: Vec<u64> = candidates
        .iter()
        .copied()
        .filter(|n| {
            // The page is the one source of labels; an issue it did not answer
            // for is not a blocked candidate. An issue the question read already
            // gave its comments is not asked again either, but it stays a
            // candidate of rule 6: the rule writes its comments and its blockers
            // from the answer it already has.
            is_blocked(q, *n) && !asked_of.contains_key(n)
        })
        .collect();
    let read = github::comments_of(calls, repo, &blocked);
    let mut comments: BTreeMap<u64, Value> = BTreeMap::new();
    let mut union: Vec<u64> = Vec::new();
    for (n, answer) in blocked.into_iter().zip(read) {
        comments.insert(n, answer);
    }
    // The union covers what the whole queue names, including the issues whose
    // comments the question read gave: their blockers are named from that
    // answer, so no second call is paid for their comments but their blocker
    // states are still read in this one call.
    for n in candidates {
        if !is_blocked(q, *n) {
            continue;
        }
        let answer = match comments.get(n) {
            Some(c) => c.clone(),
            None => match asked_of.get(n) {
                Some(c) => c.clone(),
                None => continue,
            },
        };
        union.extend(crate::decide::named_blockers(&answer));
    }
    union.sort_unstable();
    union.dedup();
    // The states arrive by number, so an issue's map is built from the typed
    // answer rather than from stringified JSON: a number GitHub did not answer
    // is simply absent, which is what decide reads as "unreadable".
    let states: BTreeMap<u64, String> = github::blocker_states(calls, repo, &union)
        .into_iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string())))
        .collect();
    (comments, states)
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
pub fn rest_events(calls: &mut Calls, repo: &str, n: u64) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::models::Models;
    use crate::snapshot::Snapshot;
    use std::os::unix::fs::PermissionsExt;

    /// What separates two calls in a stub's record: a word of nothing that no
    /// argument of a real gh or store call contains, named once so that neither
    /// the body of a stub nor the reader of the record spells an escape.
    const RECORD_SEP: &str = "«tehas-record-separator»";

    /// The word only the comment query carries, and the word only the blocker
    /// query carries: the shape a poll's calls are counted by, so a per-issue
    /// read cannot answer for a batched one by carrying a shared word.
    const COMMENT_MARK: &str = "issue(number:";
    const BLOCKER_MARK: &str = "issueOrPullRequest";

    /// The blocked world of a poll: issues #21, #22 and #23, open on the queue's
    /// page and all carrying `tehas:blocked`, each held by the marker of its own
    /// newest comment. #21 names #11, #22 names #11 and #12, and #23 names #13
    /// alone: a blocker no other issue names, so a per-issue blocker read costs
    /// calls that a merged one does not. The page gives the label's event, so
    /// the poll trusts the page and pays no read by name for these issues.
    fn world() -> Value {
        let node = |n: u64| {
            json!({
                "number": n, "title": format!("issue {n}"), "body": "a body",
                "createdAt": "2026-10-01T00:00:00Z", "author": {"login": "alice"},
                "lastEditedAt": null, "editor": null,
                "labels": {"nodes": [{"name": crate::decide::BLOCKED}]},
                // The page's timeline holds the label's event, so the poll
                // trusts the page: no read by name is paid for these issues.
                "timelineItems": {"nodes": [
                    {"__typename": "LabeledEvent",
                     "createdAt": "2026-10-02T00:00:00Z",
                     "actor": {"login": "alice"}, "label": {"name": crate::decide::BLOCKED}}
                ]},
                "closedByPullRequestsReferences": {"nodes": []}
            })
        };
        let comment = |body: &str| {
            json!({"data": {"repository": {"issue": {"comments": {"nodes": [
                {"databaseId": 500, "author": {"login": "tehas-bot"}, "body": body,
                 "createdAt": "2026-10-03T00:00:00Z", "lastEditedAt": null, "editor": null,
                 "userContentEdits": {"totalCount": 0, "nodes": []}}
            ]}}}}})
        };
        let labeled = || {
            json!([{"event": "labeled", "label": {"name": crate::decide::BLOCKED},
                    "actor": {"login": "alice"}, "created_at": "2026-10-02T00:00:00Z"}])
        };
        json!({
            "queue": {"data": {"repository": {
                "nameWithOwner": "acme/widget",
                "issues": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                           "nodes": [node(21), node(22), node(23)]},
                "closedRunning": {"nodes": []}
            }}},
            // The three issues are the candidates of rule 6. The control issue
            // is not in the page, so its labels and its events are read by name,
            // and nothing of it holds the pause.
            "blocked": [{"number": 21, "labels": [{"name": crate::decide::BLOCKED}]},
                        {"number": 22, "labels": [{"name": crate::decide::BLOCKED}]},
                        {"number": 23, "labels": [{"name": crate::decide::BLOCKED}]}],
            "control": {"labels": []},
            "events": {"21": labeled(), "22": labeled(), "23": labeled()},
            "comments": {"21": comment("<!-- tehas:blocked by=11 -->"),
                         "22": comment("<!-- tehas:blocked by=11,12 -->"),
                         "23": comment("<!-- tehas:blocked by=13 -->")},
            "states": {"data": {"repository": {
                "b11": {"state": "OPEN"}, "b12": {"state": "CLOSED"},
                "b13": {"state": "OPEN"}}}}
        })
    }

    /// One answer of the world, as the file the stub reads for it.
    fn fixture(dir: &Path, name: &str, v: &Value) {
        std::fs::write(dir.join(format!(".{name}")), v.to_string()).unwrap();
    }

    /// The reads a poll of the blocked world makes, in the order the stub asks
    /// them. A `gh` read is told apart by the word only its own arguments carry,
    /// and the first word a call holds picks the answer, so the order of this
    /// table is the disambiguation. `fixture` holds either the bytes of the
    /// answer, or the name of a file beside the programs to read; a name ending
    /// in `-` takes the number out of the call, because the comment read and the
    /// events read answer one issue at a time. A read whose `fails` is set is
    /// answered as a call GitHub refused. A new read of the poll costs one entry
    /// here and nothing else.
    struct Read {
        /// The word a call's arguments must hold for this entry to answer it.
        word: &'static str,
        fixture: &'static str,
        fails: bool,
    }

    const READS: &[Read] = &[
        Read {
            word: COMMENT_MARK,
            fixture: "issues-",
            fails: false,
        },
        Read {
            word: "check-runs",
            fixture: r#"{"check_runs":[]}"#,
            fails: false,
        },
        Read {
            word: "events?per_page=100",
            fixture: "events-",
            fails: false,
        },
        Read {
            word: "issue view",
            fixture: "control",
            fails: false,
        },
        Read {
            word: BLOCKER_MARK,
            fixture: "states",
            fails: false,
        },
        Read {
            word: "hasNextPage",
            fixture: "queue",
            fails: false,
        },
    ];

    /// Whether a table entry names a file to read rather than holding its bytes.
    fn is_fixture(answer: &str) -> bool {
        !answer.starts_with('{')
    }

    /// The next name of a poll's directory: a case takes one, so no two cases of
    /// a run share a host. The parent is cleared once, by the first case of a
    /// run, so a run does not accumulate the hosts of the runs before it.
    static NEXT_POLL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// The home of a case: a stub `gh` and a stub `store` that record the
    /// arguments they are given and answer the world. The world travels in files
    /// beside the programs, so no answer of a case is part of the source of a
    /// program, and a poll is a directory of its own, so nothing of one case is
    /// there for another to read. The directory is removed when the `Host` is
    /// dropped, so a case that fails part way through leaves no stub behind.
    struct Host {
        dir: std::path::PathBuf,
    }

    impl Drop for Host {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The home of a case. `broken` names issues whose comment read is refused,
    /// which is how a case makes one read of the batch fail.
    fn host(world: &Value, broken: &[u64]) -> Host {
        let root = std::env::temp_dir().join(format!("tehas-build-{}", std::process::id()));
        // Each case takes the next name of its own, so two cases of one run can
        // exist at once instead of reaching into the same directory. The first
        // case of a run clears the parent, so a run leaves nothing of the runs
        // before it.
        let dir = {
            let n = NEXT_POLL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n == 0 {
                let _ = std::fs::remove_dir_all(&root);
            }
            root.join(format!("poll-{n}"))
        };
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::create_dir_all(dir.join("state/cycles")).unwrap();
        std::fs::create_dir_all(dir.join("state/refresh")).unwrap();
        std::fs::create_dir_all(dir.join("logs")).unwrap();

        // The stub's own list of words is held in w-10.. and asked of the table
        // by `tr`: the first word a call's arguments hold picks the answer, so
        // the order of the table is the disambiguation, and a word may hold a
        // space (`issue view`) or a `?`, so the shell sees the list as one word
        // per line and never splits it further.
        let words: Vec<&'static str> = READS.iter().map(|r| r.word).collect();
        assert_eq!(words.len(), 6, "the stub's list holds one word a read");
        // One word a line; the stub reads it with IFS set to the newline alone,
        // so a word that holds a space stays one word.
        std::fs::write(dir.join("words"), words.join("\n")).unwrap();
        std::fs::write(
            dir.join("store-words"),
            ["plan-show", "history-show", "park-show"].join("\n"),
        )
        .unwrap();
        for (n, r) in READS.iter().enumerate() {
            let answer = format!(
                "{}{}",
                if r.fails { "!" } else { "" },
                if is_fixture(r.fixture) {
                    format!(".{}", r.fixture)
                } else {
                    r.fixture.to_string()
                }
            );
            std::fs::write(dir.join(format!("w-{}", n + 1)), answer).unwrap();
        }
        // A refused read of one issue is held in `broken-<n>`: the stub reads it
        // before the answer and leaves 1, which is what a gh call GitHub refused
        // looks like.
        for n in broken {
            std::fs::write(dir.join(format!("broken-{n}")), "!").unwrap();
        }
        fixture(&dir, "queue", &world["queue"]);
        fixture(&dir, "states", &world["states"]);
        fixture(&dir, "control", &world["control"]);
        for (n, events) in world["events"].as_object().into_iter().flatten() {
            fixture(&dir, &format!("events-{n}"), events);
        }
        for (n, comments) in world["comments"].as_object().into_iter().flatten() {
            fixture(&dir, &format!("issues-{n}"), comments);
        }

        // `answer` takes the status its caller leaves: 0 for an answer, 1 for
        // the fixture that asked to be refused.
        stub(
            &dir.join("bin"),
            "gh",
            r#"i=0
OLDIFS=$IFS
IFS=$(printf '\nX')
IFS=${IFS%X}
for word in $(cat "$here/../words"); do
  i=$((i + 1))
  a=$(cat "$here/../w-$i")
  case "$line" in
    *"$word"*)
      case "$a" in
        .*) key=$(printf '%s' "$line" | sed -n 's|.*/issues/\([0-9]*\)/events.*|\1|p')
            [ -n "$key" ] || key=$(printf '%s' "$line" | sed -n 's/.*number=\([0-9]*\).*/\1/p')
            f="$here/../$a$key"
            [ -f "$here/../broken-$key" ] && { printf '{}' >"$out"; answer 1; }
            [ -f "$f" ] || { printf '{}' >"$out"; answer 1; }
            cat "$f" >"$out" ;;
        *) printf '%s' "$a" >"$out" ;;
      esac
      answer 0 ;;
  esac
done
IFS=$OLDIFS
printf '{}' >"$out""#,
        );
        // The store says it holds nothing. `batch` answers its own request the
        // way the store does: the shape of that answer is the store's contract,
        // tested beside the store, so here a quiet host's store is enough. The
        // words the store is asked by are held in `store-words` so that a case
        // can widen them without a second place to edit.

        stub(
            &dir.join("bin"),
            "store",
            r#"case "$line" in
  batch) printf '%s' "${line#batch }" >"$args.req"
    jq -c '[ .[] | {rc: 0, out: (
        if .[0] == "plan-show" then
          "{\"api\":1,\"ok\":true,\"verb\":\"plan-show\",\"planning\":null}"
        elif .[0] == "park-show" then
          "{\"api\":1,\"ok\":true,\"verb\":\"park-show\",\"park\":null}"
        elif .[0] == "history-show" then
          "{\"api\":1,\"ok\":true,\"verb\":\"history-show\",\"rows\":[]}"
        else
          "{\"api\":1,\"ok\":true,\"verb\":\"approval-current\",\"current\":null}"
        end)}]' "$args.req" >"$out"
    answer 0 ;;
  $(sed -n 1p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"plan-show","sessions":[]}' >"$out" ;;
  cycle-show*) printf '%s' '{"api":1,"ok":true,"verb":"cycle-show","cycles":[]}' >"$out" ;;
  $(sed -n 2p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"history-show","rows":[]}' >"$out" ;;
  $(sed -n 3p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"park-show","parks":[]}' >"$out" ;;
  approval-current*) printf '%s' '{"api":1,"ok":true,"verb":"approval-current","current":null}' >"$out" ;;
  *) printf '%s' "{\"api\":1,\"ok\":true,\"verb\":\"$1\"}" >"$out" ;;
esac"#,
        );
        Host { dir }
    }

    /// A stub program of `name` in `bin`. It records the arguments it is given
    /// and then runs `body`, which reads them from `$line` and writes its answer
    /// to a file of its own that only `answer` reads. The poll runs its calls
    /// side by side, so nothing of a call is shared: `mktemp` gives the arguments
    /// and the answer each a path of their own, no call waits for another --
    /// which is the point of the batching this file is about -- and no call
    /// reads what another left behind. `RECORD_SEP` separates the calls in the
    /// record, held in a name so that neither this program nor its reader spells
    /// an escape.
    fn stub(bin: &Path, name: &str, body: &str) {
        let mut lines: Vec<String> = [
            "#!/bin/sh".to_string(),
            format!("here='{}'", bin.display()),
            "args=$(mktemp \"$here/{name}-args.XXXXXX\")".to_string(),
            "out=$(mktemp \"$here/{name}-out.XXXXXX\")".to_string(),
            format!(
                "printf '%s{sep}' \"$*\" >>\"$here/{name}-calls\"; printf '%s' \"$*\" >\"$args\"",
                sep = RECORD_SEP
            ),
            "line=$(cat \"$args\")".to_string(),
            "rm -f \"$args\"".to_string(),
            // The body writes `$out` and leaves through `answer`, which prints
            // it and exits with the status it is given: 0 for an answer, 1 for
            // a read that must look refused. A body that finds nothing writes
            // `{}` and answers that.
            "answer() {".to_string(),
            "  cat \"$out\"".to_string(),
            "  rm -f \"$out\"".to_string(),
            "  exit \"${1:-0}\"".to_string(),
            "}".to_string(),
        ]
        .to_vec();
        lines.extend(body.lines().map(str::to_string));
        lines.push("answer 0".to_string());
        std::fs::write(bin.join(name), lines.join("\n") + "\n").unwrap();
        std::fs::set_permissions(bin.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The calls the stub of `name` recorded, in the order it wrote them.
    fn calls_of(dir: &Path, name: &str) -> Vec<String> {
        std::fs::read_to_string(dir.join("bin").join(format!("{name}-calls")))
            .unwrap_or_default()
            .split(RECORD_SEP)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The gh calls of the poll that hold `mark`: the query's own word, not a
    /// number, so a per-issue read cannot answer for a batched one by sharing a
    /// word with another query.
    fn queries(calls: &[String], mark: &str) -> Vec<String> {
        calls
            .iter()
            .filter(|c| c.starts_with("api graphql") && c.contains(mark))
            .cloned()
            .collect()
    }

    /// The whole poll of a world: the snapshot, the calls the stubs recorded, what
    /// the poll counted of them, and what it asked the store. The host lives only
    /// as long as this call, so a case cannot leave it behind.
    fn poll(world: &Value, broken: &[u64]) -> (Value, Vec<String>, u32, Vec<String>) {
        let dir = host(world, broken);
        let mut src = sources(&dir.dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.0),
        };
        let gh = src.calls.gh_count;
        (
            snap,
            calls_of(&dir.dir, "gh"),
            gh,
            calls_of(&dir.dir, "store"),
        )
    }

    fn sources(dir: &Path) -> Sources<'_> {
        Sources {
            home: dir,
            calls: run::Calls {
                home: Some(dir.to_string_lossy().into_owned()),
                bound: Duration::from_secs(10),
                store_bound: Duration::from_secs(10),
                gh: dir.join("bin").join("gh").to_string_lossy().into_owned(),
                store: dir.join("bin").join("store").to_string_lossy().into_owned(),
                gh_count: 0,
                gh_token: None,
            },
            systemctl: "true".into(),
            now: 1_793_404_800,
            models: Models {
                file: Value::Null,
                used: Vec::new(),
                provider: String::new(),
                model: String::new(),
            },
            endpoint: (String::new(), String::new()),
            gate: false,
        }
    }

    /// The states of the blockers a case expects, as `Issue::blockers` holds
    /// them.
    fn states(want: &[(u64, &str)]) -> BTreeMap<u64, String> {
        want.iter().map(|(n, s)| (*n, (*s).to_string())).collect()
    }

    fn settings() -> Settings {
        Settings::parse(&json!({
            "TEHAS_REPO": "acme/widget",
            "TEHAS_CONTROL_ISSUE": 1,
            "TEHAS_BOT_LOGIN": "tehas-bot",
            "TEHAS_ALLOWED_LABELERS": "alice",
        }))
    }

    /// Two blocked issues that share a blocker read their comments side by side
    /// and their blockers once: the poll pays one call for the comments of the
    /// whole list and one call for the states of every blocker the queue names,
    /// and each issue is given only the numbers its own comment names (#1161).
    ///
    /// The contract is asserted against the shape of the calls and against
    /// `Snapshot::parse`, the type the rules actually read: a read that went
    /// back to one gh process per issue, a call for a number no rule asked
    /// about, or an answer whose shape `parse` cannot type, all fail here.
    #[test]
    fn the_blocked_issues_read_their_comments_side_by_side_and_their_blockers_once() {
        let w = world();
        let blocked_len = w["blocked"].as_array().unwrap().len();
        let (snap, calls, gh_count, store) = poll(&w, &[]);

        // One run of the comment read stood for the whole list, and it asked
        // only of the blocked issues: a per-issue read pays `blocked_len` calls,
        // and a batch widened past the list carries a number no rule reads.
        let comment_calls = queries(&calls, COMMENT_MARK);
        assert_eq!(
            comment_calls.len(),
            blocked_len,
            "one comment call per blocked issue, none for any other: {calls:?}"
        );
        for n in 21..=23 {
            let mine: Vec<&String> = comment_calls
                .iter()
                .filter(|c| c.contains(&format!("number={n}")))
                .collect();
            assert_eq!(mine.len(), 1, "issue {n} is asked for its comments once");
        }
        // The states of every blocker are one call. #23 names #13, which no
        // other issue names, so a read of the same list one issue at a time
        // costs three calls here, not one.
        let blocker_calls = queries(&calls, BLOCKER_MARK);
        assert_eq!(
            blocker_calls.len(),
            1,
            "the states of the blockers are one call: {calls:?}"
        );
        for b in [11u64, 12, 13] {
            assert!(
                blocker_calls[0].contains(&format!("b{b}: issueOrPullRequest(number: {b})")),
                "the one call names blocker {b}: {}",
                blocker_calls[0]
            );
        }
        // The poll's own record of what it paid agrees with the stub's.
        assert_eq!(
            gh_count as usize,
            calls.len(),
            "the poll counted every gh call it made"
        );

        // Each issue holds its own numbers, from that one answer, and the shape
        // is what the reader's types say it is.
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.issues[&21].blockers, states(&[(11, "OPEN")]));
        assert_eq!(
            parsed.issues[&22].blockers,
            states(&[(11, "OPEN"), (12, "CLOSED")])
        );
        assert_eq!(parsed.issues[&23].blockers, states(&[(13, "OPEN")]));
        for n in 21..=23 {
            assert!(
                parsed.issues[&n].comments.is_some(),
                "issue {n} has the comments the poll read"
            );
            assert_eq!(
                snap["issues"][n.to_string()]["comments"],
                w["comments"][n.to_string()],
                "issue {n} keeps its own comments"
            );
        }
        // The store is still asked once for the per-issue reads: batching the
        // two reads of rule 6 did not turn one run of `store batch` into two.
        assert_eq!(
            store.iter().filter(|c| c.starts_with("batch")).count(),
            1,
            "the per-issue reads are one run of the store"
        );
    }

    /// The record of a plan's start is read for the issues the rule can start,
    /// each its own, and it reaches the type the rule reads.
    #[test]
    fn a_candidate_is_given_the_record_of_its_plan_and_no_other_is() {
        let dir = host(&world(), &[]);
        std::fs::create_dir_all(dir.dir.join("state/plan-start")).unwrap();
        for n in [21, 99] {
            let body = format!(r#"{{"by":"alice","sum":"{n}"}}"#);
            std::fs::write(host::plan_start_file(&dir.dir, n), body).unwrap();
        }
        let mut src = sources(&dir.dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.0),
        };
        let parsed = Snapshot::parse(&snap);
        assert_eq!(
            parsed.issues[&21].plan_start,
            Some(json!({"by": "alice", "sum": "21"}))
        );
        assert_eq!(parsed.issues[&22].plan_start, None);
        assert!(!parsed.issues.contains_key(&99), "no candidate, no read");
    }

    /// A comment that names nothing asks nothing of the blockers: the poll pays
    /// no call for a list of none, and the issue's blockers are empty rather
    /// than unreadable.
    #[test]
    fn a_blocked_issue_with_nothing_to_name_asks_nothing_of_the_blockers() {
        let mut world = world();
        // The one blocked issue of the queue, and its newest comment names no
        // blocker at all.
        world["queue"]["data"]["repository"]["issues"]["nodes"] =
            json!([world["queue"]["data"]["repository"]["issues"]["nodes"][0]]);
        world["blocked"] = json!([{"number": 21, "labels": [{"name": crate::decide::BLOCKED}]}]);
        world["events"] = json!({"21": world["events"]["21"]});
        world["comments"] = json!({"21": {"data": {"repository": {"issue": {"comments":
                {"nodes": [{"body": "no marker here"}]}}}}}});
        let (snap, calls, _, _) = poll(&world, &[]);
        assert!(
            queries(&calls, BLOCKER_MARK).is_empty(),
            "no blocker is named, so none is asked: {calls:?}"
        );
        assert_eq!(
            queries(&calls, COMMENT_MARK).len(),
            1,
            "the one blocked issue is read once: {calls:?}"
        );
        let parsed = Snapshot::parse(&snap);
        assert!(parsed.issues[&21].blockers.is_empty());
        assert!(parsed.issues[&21].comments.is_some());
        assert_eq!(snap["issues"]["21"]["comments"], world["comments"]["21"]);
    }

    /// A blocked issue whose comment read GitHub refused is a failed read, not
    /// an empty one: the snapshot holds `{"failed": true}`, which
    /// `Snapshot::parse` turns into no comments at all, and decide ends the
    /// poll on it rather than reading the issue as one that names no blocker.
    #[test]
    fn a_blocked_issue_whose_comment_read_failed_is_a_failed_read() {
        let w = world();
        let (snap, calls, _, _) = poll(&w, &[22]);
        assert_eq!(snap["issues"]["22"]["comments"], json!({"failed": true}));
        let parsed = Snapshot::parse(&snap);
        assert!(
            parsed.issues[&22].comments.is_none(),
            "the issue whose read failed has no comments to decide on"
        );
        assert!(
            parsed.issues[&21].comments.is_some(),
            "the issues whose reads answered still have theirs"
        );
        // One refused call did not cost the poll a second run of the read.
        assert_eq!(
            queries(&calls, COMMENT_MARK).len(),
            3,
            "each blocked issue is asked once, failure and all: {calls:?}"
        );
    }

    /// A merged blocker call that answers nothing leaves every blocked issue
    /// with no state at all, which is what decide reads as "the state of #b
    /// could not be read" -- and not as "this issue names no blocker", which
    /// would let it start.
    #[test]
    fn a_blocker_the_merged_call_did_not_answer_is_left_unread() {
        let mut w = world();
        // The merged call answers #11 and #12 but not #13, which #23 names.
        w["states"] = json!({"data": {"repository": {
            "b11": {"state": "OPEN"}, "b12": {"state": "CLOSED"}}}});
        let (snap, _, _, _) = poll(&w, &[]);
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.issues[&23].blockers, states(&[]));
        assert_eq!(
            snap["issues"]["23"]["blockers"],
            json!({}),
            "#23 was asked of the call and answered nothing"
        );
        // The two issues whose blockers were answered keep both states.
        assert_eq!(
            parsed.issues[&22].blockers,
            states(&[(11, "OPEN"), (12, "CLOSED")])
        );
    }

    /// A blocked issue that is also asking has had its comments read by the
    /// question read already, so the blocked batch pays no second call for it
    /// and publishes the answer it has.
    #[test]
    fn a_blocked_issue_the_question_read_answered_pays_no_second_comment_call() {
        let mut w = world();
        // #22 carries both labels, so the question read gives its comments.
        w["queue"]["data"]["repository"]["issues"]["nodes"][1]["labels"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "tehas:question"}));
        w["asking"] = json!([{"number": 22, "labels": [{"name": "tehas:question"}]}]);
        let (snap, calls, _, _) = poll(&w, &[]);
        let comment_calls = queries(&calls, COMMENT_MARK);
        // Three blocked issues, one of them already answered by the question
        // read: the poll pays for two, not three.
        assert_eq!(
            comment_calls
                .iter()
                .filter(|c| c.contains("number=22"))
                .count(),
            1,
            "the double-labelled issue is asked for its comments once: {calls:?}"
        );
        assert_eq!(comment_calls.len(), 3, "{calls:?}");
        assert_eq!(
            snap["issues"]["22"]["comments"], w["comments"]["22"],
            "the issue keeps the answer the question read gave"
        );
        let parsed = Snapshot::parse(&snap);
        assert!(parsed.issues[&22].comments.is_some());
        assert_eq!(
            parsed.issues[&22].blockers,
            states(&[(11, "OPEN"), (12, "CLOSED")])
        );
    }

    /// An issue the blocked list carries but the queue's page left out is not a
    /// blocked candidate: the batch and rule 6 test the same predicate, so an
    /// issue one of them holds is never written as blocked by the other. The
    /// page, not the list, is what makes an issue blocked.
    #[test]
    fn an_issue_off_the_page_is_not_batched_and_not_written_as_blocked() {
        let mut w = world();
        // #23 stays a candidate of rule 6 and keeps the label in the blocked
        // list, but the page did not answer for it.
        w["queue"]["data"]["repository"]["issues"]["nodes"] = json!([
            w["queue"]["data"]["repository"]["issues"]["nodes"][0],
            w["queue"]["data"]["repository"]["issues"]["nodes"][1]
        ]);
        let (snap, calls, _, _) = poll(&w, &[]);
        let comment_calls = queries(&calls, COMMENT_MARK);
        assert_eq!(comment_calls.len(), 2, "{calls:?}");
        assert!(
            !comment_calls.iter().any(|c| c.contains("number=23")),
            "an issue off the page is not asked: {calls:?}"
        );
        // Its blockers are not asked of the merged call either: the union holds
        // only the blockers the two issues on the page name.
        let blocker_calls = queries(&calls, BLOCKER_MARK);
        assert_eq!(blocker_calls.len(), 1, "{calls:?}");
        assert!(
            !blocker_calls[0].contains("b13: issueOrPullRequest(number: 13)"),
            "#23 is not blocked, so the blocker only it names is not asked: {}",
            blocker_calls[0]
        );
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.issues[&21].blockers, states(&[(11, "OPEN")]));
        // Rule 6 wrote no comments and no blockers for it: the page holds the
        // label, and the page does not hold the issue, so nothing of it is read
        // and it is not in the snapshot at all.
        assert!(
            !parsed.issues.contains_key(&23),
            "an issue off the page is not written as blocked: {:?}",
            parsed.issues.keys().collect::<Vec<_>>()
        );
        assert_eq!(snap["issues"].get("23"), None);
    }
}
