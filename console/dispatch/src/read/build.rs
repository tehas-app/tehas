//! snapshot(): what one poll would read, in the JSON shape Snapshot::parse
//! takes. GitHub in one paged query and the per-issue reads the rules make of
//! the issues they decide about, the store's read verbs, and this host's files
//! and locks. Nothing is written.

use super::run::{Calls, GhFail};
use super::{github, host, models, store};
use crate::decide::has;
use crate::settings::Settings;
use crate::text;
use crate::text::closing_numbers;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tehas_core::events::{pause_holds, Events};

/// Where the snapshot reads from, and with what.
pub struct Sources<'a> {
    pub home: &'a Path,
    pub calls: Calls,
    pub systemctl: String,
    pub now: i64,
    pub models: models::Models,
    /// The subagent provider's endpoint, url and key, as the probe found it.
    pub endpoint: (String, String),
}

/// The facts a decision names in needs that are read only when it asks:
/// the sandbox image's and the model's answers to images, whether the
/// image takes --continue-pr, and what the running cycles' branches change.
/// Returns whether any was read.
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
    if needs.contains(&"overlap") && snap.get("overlap").is_none() {
        let running: Vec<u64> = snap["running"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["issue"].as_u64())
            .collect();
        let checkout = std::env::var("TEHAS_WORK_REPO").unwrap_or_default();
        let default = std::env::var("TEHAS_BRANCH").unwrap_or_else(|_| "main".into());
        snap["overlap"] = host::changed(&checkout, &default, &running);
        read = true;
    }
    read
}

/// An open issue of the issue type `name` carries `label` from here on, as
/// though GitHub had given it: every rule that holds an issue on the label
/// holds it on the type too. The label itself still counts.
fn typed_as_label(open: &mut [github::Open], name: &str, label: &str) {
    for o in open {
        if o.node.pointer("/issueType/name").and_then(Value::as_str) == Some(name)
            && !has(&o.labels, label)
        {
            o.labels.push(label.to_string());
        }
    }
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
    if c.read_only {
        s["read_only_ran"] = json!(tehas_core::read_only::recorded(home));
    }
    s["refine_brief"] = json!(Path::new(&c.libexec)
        .join("../lib/refine-brief.md")
        .metadata()
        .is_ok_and(|m| m.len() > 0));

    let calls = &mut src.calls;
    s["order"] = store::order(calls);
    let types = !c.improvement_type.is_empty();
    let mut q = github::queue(calls, &c.repo, types)?;
    s["repo"] = json!(q.repo);
    if let Some(labels) = q.labels.take() {
        s["tehas_labels"] = labels;
    }
    // Which issues the page cut short is read of the labels as GitHub gave
    // them, before an issue of the improvement type is given the label below.
    let dropped_of_page: Vec<u64> = q
        .open
        .iter()
        .filter(|o| github::dropped_events(o, &[github::RUNNING]))
        .map(|o| o.number)
        .collect();
    if types {
        s["issue_types"] = json!(q.issue_types);
        typed_as_label(&mut q.open, &c.improvement_type, crate::decide::IMPROVEMENT);
    }
    s["user"] = json!({"login": c.bot_login});
    s["bot_id"] = json!(tehas_core::accounts::read(home)
        .get(tehas_core::accounts::account(&c.bot_login))
        .copied());
    // What rule 6 holds an issue on out of an open pull request's body:
    // every open pull request of `own_pull` that names it with a closing
    // keyword, by number, whatever base it targets. GitHub's own links are
    // kept to the same pull requests.
    if let Some(pulls) = &q.open_pulls {
        names_from_bodies(&mut s, pulls, &q.open, c);
    }
    let open: BTreeSet<u64> = q.open.iter().map(|o| o.number).collect();
    let mut issues = Map::new();
    for o in &q.open {
        let mut i = github::issue(o);
        if let Some(prs) = i.get_mut("prs").and_then(Value::as_array_mut) {
            prs.retain(|p| own_pull(p, c));
        }
        issues.insert(o.number.to_string(), Value::Object(i));
    }
    // Those the page left out are read as the bash reads them, side by side.
    // The bot's `RUNNING` mark is the one label the queue does not ask about:
    // every running cycle carries it, the bot applies it alone, and the rules
    // read the mark and the lock file rather than its events. The control
    // issue is asked about by name below, with no label exempt.
    // The open issues the poll labels `epic`. Whether a person took the
    // label off is read of their events: a page whose timeline holds no
    // event of the label may have cut a removal off, so it is read whole.
    let epic_titled: Vec<&github::Open> = q
        .open
        .iter()
        .filter(|o| o.number != c.control && github::epic_titled(o))
        .collect();
    s["epic_titled"] = epic_titled.iter().map(|o| json!(o.number)).collect();
    let dropped: Vec<u64> = q
        .open
        .iter()
        .filter(|o| {
            dropped_of_page.contains(&o.number)
                || (epic_titled.iter().any(|e| e.number == o.number)
                    && !github::has_event_of(o, text::EPIC))
        })
        .map(|o| o.number)
        .collect();
    for (n, r) in dropped.iter().zip(events_all(calls, &c.repo, &dropped)) {
        issues[&n.to_string()]["events"] = r;
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
        let authors = [bot(&s)];
        let lists: Vec<Value> = authors
            .iter()
            .map(|author| {
                github::list(
                    calls,
                    &[
                        "pr",
                        "list",
                        "--repo",
                        &c.repo,
                        "--author",
                        author,
                        "--state",
                        "all",
                        "--json",
                        "url,state,body,headRefName,createdAt",
                        "--limit",
                        "100",
                    ],
                )
            })
            .collect();
        s["bot_prs"] = joined_lists(lists);
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
    s["leftovers"] = host::leftovers(home, &work_dir, &user_home, src.now, c.keep_days);
    // The queue is every open issue, so one it does not hold is closed: a
    // closed issue never resumes, and what it left goes at once.
    for l in s["leftovers"].as_array_mut().into_iter().flatten() {
        let n = l["issue"].as_u64().unwrap_or(0);
        l["closed"] = json!(!open.contains(&n));
    }
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
    // The requests are read before the list: with the setting off, a request
    // `tehasbot refresh --pr N` wrote starts in the lane, and the clean-up of
    // the marks runs while it waits. A request without that marker does not.
    let (requests, refresh_warn) = host::requests(home);
    if !refresh_warn.is_empty() {
        s["refresh_warn"] = json!(refresh_warn);
    }
    let pr_asked = requests
        .iter()
        .any(|(_, r)| r.get("asked").and_then(Value::as_str) == Some("pr"));
    if c.refresh_conflicts || !from.is_empty() || pr_asked {
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
    // The events of the refreshing pull requests are read side by side, 8 at a
    // time, as the issues that lost the mark are read above (#1423).
    let refreshing = numbers(&s["refreshing"]);
    for (pr, r) in refreshing
        .iter()
        .zip(events_all(calls, &c.repo, &refreshing))
    {
        entry(&mut issues, *pr)["events"] = r;
    }
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

    // The questions waiting for an answer, the blocked candidates and the
    // candidates whose park may lift read their comments in one read of the
    // whole list rather than of one issue at a time: one aliased query a chunk
    // answers every asking issue, every blocked candidate the question read does
    // not cover (#1198), and every park that may lift (#1773), and one call
    // reads the states of the blockers the whole queue names (#1161).
    let blocked = blocked_list(&q, &candidates, &asking);
    // The list is built from the park answers the batch above already holds, so
    // it is planned after that run and before this one.
    let lifting = lifting_parks(&asked, &q, &candidates, &asking, c);
    let wanted: Vec<u64> = asking
        .iter()
        .chain(&blocked)
        .chain(&lifting)
        .copied()
        .collect();
    let mut read = github::comments_of(calls, &c.repo, &wanted).into_iter();
    let mut asked_of: BTreeMap<u64, Value> = BTreeMap::new();
    for (n, comments) in asking.iter().zip(read.by_ref()) {
        let question = store::question(&asked, *n);
        let i = entry(&mut issues, *n);
        i["question"] = question;
        i["comments"] = comments.clone();
        asked_of.insert(*n, comments);
    }
    let comments: BTreeMap<u64, Value> = blocked.into_iter().zip(read.by_ref()).collect();
    // What the batch answered of the parks that may lift, taken in the order
    // `lifting` holds them: the batch's iterator still has exactly that many
    // answers left, and an issue the question read answered first keeps that
    // answer rather than a second one for the same number.
    let lifted: BTreeMap<u64, Value> = lifting.into_iter().zip(read).collect();
    let states = blocker_reads(calls, &c.repo, &q, &candidates, &comments, &asked_of);

    // The candidates of rule 6: what their start rests on. Membership of the
    // blocked batch is decided once, in `blocked_list`, and of the lifting
    // batch once, in `lifting_parks`, and the rule's own tests are those two
    // predicates, so the batches and the rule cannot disagree about an issue.
    for n in candidates {
        let park = store::park(&asked, n);
        let (shaped, _) = store::approval(&asked, n);
        let i = entry(&mut issues, n);
        i["park"] = park;
        i["approval"] = shaped;
        if !is_blocked(&q, n) {
            if i.get("comments").is_none() {
                if let Some(list) = lifted.get(&n) {
                    i["comments"] = list.clone();
                }
            }
            continue;
        }
        // The comments are the batch's, or the question read's when that read
        // answered the issue first; the one read paid for both.
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

/// The blocked candidates of rule 6 whose comments the poll reads beside the
/// questions': each blocked one the question read does not already cover.
///
/// Membership is decided once, by `is_blocked`, so the batch and the rule that
/// consumes it cannot disagree about which issue is blocked. The page is the
/// one source of labels; an issue it did not answer for is not a blocked
/// candidate. An asking issue is read once, by the question read, but it stays
/// a candidate of rule 6: the rule writes its comments and its blockers from
/// the answer it already has.
fn blocked_list(q: &github::Queue, candidates: &[u64], asking: &[u64]) -> Vec<u64> {
    candidates
        .iter()
        .copied()
        .filter(|n| is_blocked(q, *n) && !asking.contains(n))
        .collect()
}

/// Whether the millwright's note, or a Retry ticked in the park's question, may
/// lift `park`'s hold: the test rule 6 applies to a candidate, held here so the
/// batch of comment reads and the rule ask the same issues (#1773). The read
/// says nothing either way, so a park whose read failed is no park to lift on.
fn park_lifts(park: &Value, c: &Settings) -> bool {
    park.is_object()
        && park.get("failed").is_none()
        && ((!c.millwright.is_empty() && (c.work_images || c.work_files))
            || park.get("question").is_some_and(|q| !q.is_null()))
}

/// The unblocked candidates of rule 6 whose comments the poll reads beside the
/// questions': each unblocked one the question read does not already cover, and
/// whose park may lift. Membership is decided once, by `park_lifts`, so the
/// batch and the rule that consumes it cannot disagree about which issue's park
/// may lift. An asking issue is read once, by the question read.
fn lifting_parks(
    asked: &store::Asked,
    q: &github::Queue,
    candidates: &[u64],
    asking: &[u64],
    c: &Settings,
) -> Vec<u64> {
    candidates
        .iter()
        .copied()
        .filter(|n| {
            !is_blocked(q, *n) && !asking.contains(n) && park_lifts(&store::park(asked, *n), c)
        })
        .collect()
}

/// The states of the blockers the blocked candidates' comments name, in one
/// call for the union of them (#1161).
///
/// The comment read answers every number it was given, in order, so a missing
/// answer is a broken contract rather than a value to paper over. The union
/// covers what the whole queue names, including the issues whose comments the
/// question read gave. A queue that names no blocker is a list of none, and
/// the poll pays no call for it.
fn blocker_reads(
    calls: &mut Calls,
    repo: &str,
    q: &github::Queue,
    candidates: &[u64],
    comments: &BTreeMap<u64, Value>,
    asked_of: &BTreeMap<u64, Value>,
) -> BTreeMap<u64, String> {
    let mut union: Vec<u64> = Vec::new();
    for n in candidates {
        if !is_blocked(q, *n) {
            continue;
        }
        let Some(answer) = comments.get(n).or_else(|| asked_of.get(n)) else {
            continue;
        };
        union.extend(crate::decide::named_blockers(answer));
    }
    union.sort_unstable();
    union.dedup();
    // The states arrive by number, so an issue's map is built from the typed
    // answer rather than from stringified JSON: a number GitHub did not answer
    // is simply absent, which is what decide reads as "unreadable".
    github::blocker_states(calls, repo, &union)
        .into_iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string())))
        .collect()
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

/// Whether a pull request may hold an issue: its branch is in the product
/// repository and the factory or an allowed account opened it. Anyone can open
/// a pull request from a fork with any body, so no other one holds an issue.
fn own_pull(p: &Value, c: &Settings) -> bool {
    let s = |ptr: &str| p.pointer(ptr).and_then(Value::as_str).unwrap_or("");
    let author = s("/author/login");
    p.get("isCrossRepository") == Some(&json!(false))
        && !c.repo.is_empty()
        && s("/headRepository/nameWithOwner").eq_ignore_ascii_case(&c.repo)
        && (tehas_core::accounts::factorys(author, &c.bot_login)
            || c.allowed(crate::settings::account(author)))
}

/// Which open pull requests name an open issue with a closing keyword in their
/// own description, written to `pulls_name` of the snapshot by issue number.
///
/// A body is read once per keyword it holds, not once per open issue: a page of
/// 25 issues over 100 bodies is 2500 matches otherwise, and most descriptions
/// hold no keyword at all. The nodes are what the page answered: a list of no
/// shape a rule reads holds nothing, and no issue is held on a body no poll read.
fn names_from_bodies(s: &mut Value, pulls: &Value, open: &[github::Open], c: &Settings) {
    let Some(nodes) = pulls.as_array() else {
        return;
    };
    let mentions: BTreeMap<u64, Vec<u64>> = nodes
        .iter()
        .filter(|p| own_pull(p, c))
        .filter_map(|p| {
            let pr = p.get("number").and_then(Value::as_u64)?;
            // A body of no shape a rule reads, none or absent, is a description
            // that closes nothing.
            let body = p.get("body").and_then(Value::as_str).unwrap_or("");
            Some((pr, closing_numbers(body)))
        })
        .collect();
    let mut names: Map<String, Value> = Map::new();
    for o in open {
        let hits: Vec<u64> = mentions
            .iter()
            .filter(|(pr, hits)| **pr != o.number && hits.contains(&o.number))
            .map(|(pr, _)| *pr)
            .collect();
        if !hits.is_empty() {
            names.insert(o.number.to_string(), json!(hits));
        }
    }
    if !names.is_empty() {
        s["pulls_name"] = Value::Object(names);
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

/// `rest_events` for several issues or pull requests, read side by side through
/// `Calls::gh_all`: one answer per number, in the order given.
fn events_all(calls: &mut Calls, repo: &str, ns: &[u64]) -> Vec<Value> {
    let reads: Vec<Vec<String>> = ns
        .iter()
        .map(|n| {
            vec![
                "api".into(),
                format!("repos/{repo}/issues/{n}/events?per_page=100"),
                "--paginate".into(),
            ]
        })
        .collect();
    calls
        .gh_all(&reads)
        .into_iter()
        .map(|r| r.unwrap_or_else(|_| json!({"failed": true})))
        .collect()
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

/// The lists of each author as one, or the first that failed or is no list.
fn joined_lists(lists: Vec<Value>) -> Value {
    let mut all = Vec::new();
    for l in lists {
        match l {
            Value::Array(a) => {
                for p in a {
                    let url = p.get("url");
                    if url.is_none() || !all.iter().any(|q: &Value| q.get("url") == url) {
                        all.push(p);
                    }
                }
            }
            other => return other,
        }
    }
    Value::Array(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::models::Models;
    use crate::read::run;
    use std::time::Duration;

    #[test]
    fn the_bots_pull_requests_are_the_lists_of_both_authors() {
        let a = json!({"url": "https://github.com/a/b/pull/1"});
        let b = json!({"url": "https://github.com/a/b/pull/2"});
        assert_eq!(
            joined_lists(vec![json!([a.clone()]), json!([b.clone(), a.clone()])]),
            json!([a, b])
        );
        let failed = json!({"failed": true});
        assert_eq!(joined_lists(vec![json!([a]), failed.clone()]), failed);
    }
    use crate::snapshot::Snapshot;

    /// What separates two calls in a stub's record: a word of nothing that no
    /// argument of a real gh or store call contains, named once so that neither
    /// the body of a stub nor the reader of the record spells an escape.
    const RECORD_SEP: &str = "»tehas-record«";
    /// Separates the verb `batch` from the request it was given on standard
    /// input, inside one record of the store's calls.
    const REQUEST_SEP: &str = "«tehas-request»";
    /// Ends the request of a run of the batch, where the verb of the call whose
    /// record follows the run begins.
    const RUST_SEP: &str = "«tehas-rust»";

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
        // The page answers no open pull request here, so a world's poll holds
        // nothing on a body; `openPulls` is written beside the nodes below.
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
                "closedRunning": {"nodes": []},
                // The page's own list of open pull requests, empty here so a
                // world's poll holds nothing on a body. A case that wants one
                // writes `openPulls` into its own `queue`.
                "openPulls": {"nodes": []}
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
        Read {
            word: "tehas:refreshing",
            fixture: "refreshing",
            fails: false,
        },
    ];

    /// Whether a table entry names a file to read rather than holding its bytes.
    fn is_fixture(answer: &str) -> bool {
        !answer.starts_with('{')
    }

    /// The home of a case. `broken` names issues whose comment read is refused,
    /// which is how a case makes one read of the batch fail. The file names the
    /// status the read meets: `HTTP 500`, which the rule of `retry` keeps as the
    /// answer rather than the `HTTP 502` it owes a retry to, so the case speaks of
    /// the one call the poll refused and pays no retry for it. A case that wants
    /// the retried class names a 5xx that is not a 500; the rule is the thing that
    /// decides, and `retry.rs` says which status belongs to which class.
    fn host(world: &Value, broken: &[u64]) -> tehas_core::testdir::TestDir {
        let dir = tehas_core::testdir::TestDir::new("build-poll");
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
        assert_eq!(words.len(), 7, "the stub's list holds one word a read");
        // One word a line; the stub reads it with IFS set to the newline alone,
        // so a word that holds a space stays one word.
        std::fs::write(dir.join("words"), words.join("\n")).unwrap();
        std::fs::write(
            dir.join("store-words"),
            ["plan-show", "history-show", "park-show"].join("\n"),
        )
        .unwrap();
        // A world whose issues are parked names them in `parks`, and the store
        // answers their `park-show` and `approval-current` with the records of
        // the lifted-park case rather than the quiet ones. The answer of each
        // issue is the single line the store itself writes for the verb, held in
        // a file beside the programs named for the call and its number.
        for n in world["parks"].as_array().into_iter().flatten() {
            let Some(n) = n.as_u64() else {
                continue;
            };
            let sum = tehas_core::approval::body_sha256(&json!({"body": "Words."}));
            let at = world["ready_at"].as_str().unwrap_or("2026-10-30T23:58:00Z");
            std::fs::write(dir.join(format!(".park-show-{n}")), park_answer(&sum)).unwrap();
            std::fs::write(
                dir.join(format!(".approval-current-{n}")),
                approval_answer(at, &sum),
            )
            .unwrap();
        }
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
        // A read of one issue that is refused is held in `broken-<n>`, whose
        // single line the stub prints on the error stream before it leaves 1. The
        // status it names and what the rule makes of it are `host`'s to say.
        for n in broken {
            std::fs::write(
                dir.join(format!("broken-{n}")),
                "HTTP 500: Internal Server Error\n",
            )
            .unwrap();
        }
        fixture(&dir, "queue", &world["queue"]);
        fixture(&dir, "states", &world["states"]);
        fixture(&dir, "control", &world["control"]);
        if let Some(r) = world.get("refreshing") {
            fixture(&dir, "refreshing", r);
        }
        // The events reads named in `together` wait, each, until every one of
        // them has started: they answer only when they run side by side, and
        // a read of them one at a time gives up and is refused. The wait is
        // for the count, bounded well inside the call's own bound.
        for n in world["together"].as_array().into_iter().flatten() {
            std::fs::write(dir.join(format!("together-{n}")), "").unwrap();
        }
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
        .*) keys=$(printf '%s' "$line" | grep -o 'i[0-9]*: issue(number:' | sed 's/^i\([0-9]*\):.*/\1/')
            if [ -n "$keys" ]; then
              # A chunk of the comment read: one alias an issue, and a refused
              # issue refuses the whole chunk, as GitHub refuses the one query.
              acc='{}'
              for key in $keys; do
                [ -f "$here/../broken-$key" ] && { printf '%s' "$(cat "$here/../broken-$key")" >&2; printf '{}' >"$out"; answer 1; }
                [ -f "$here/../$a$key" ] || { printf '{}' >"$out"; answer 1; }
                acc=$(jq -c --arg k "i$key" --argjson acc "$acc" '$acc + {($k): .data.repository.issue}' "$here/../$a$key")
              done
              printf '{"data":{"repository":%s}}' "$acc" >"$out"
              answer 0
            fi
            key=$(printf '%s' "$line" | sed -n 's|.*/issues/\([0-9]*\)/events.*|\1|p')
            [ -n "$key" ] || key=$(printf '%s' "$line" | sed -n 's/.*number=\([0-9]*\).*/\1/p')
            f="$here/../$a$key"
            [ -f "$here/../broken-$key" ] && { printf '%s' "$(cat "$here/../broken-$key")" >&2; printf '{}' >"$out"; answer 1; }
            if [ -n "$key" ] && [ -f "$here/../together-$key" ]; then
              : >"$here/../started-$key"
              want=$(ls "$here/.." | grep -c '^together-')
              tries=0
              while [ "$(ls "$here/.." | grep -c '^started-')" -lt "$want" ]; do
                tries=$((tries + 1))
                [ "$tries" -le 100 ] || { printf 'alone' >&2; printf '{}' >"$out"; answer 1; }
                sleep 0.05
              done
            fi
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
        let store_body = String::from(
            r#"# `batch` is the one verb whose request is on standard input rather than in
# the arguments, as `store batch` reads it, so the request is taken from there
# and the words of the call are just the verb.
case "$line" in
  batch*) req=$(mktemp "$here/../store-req.XXXXXX")
    # The request arrives on the poll's standard input, after the verb: read whole
    # here, appended to the record of the call the header opened, and kept for the
    # answer below. The separators are substituted by `stub`, so the answer of a
    # run cannot write one of them itself.
    read -r batch_request
    printf '%s' "{sep}{req}$batch_request{rust}" >>"$here/{name}-calls"
    printf '%s' "$batch_request" >"$req"
    acc=[]
    # One answer per call of the request, in its order. A record a case wrote
    # beside the programs, named for the verb and its number, answers that call
    # with the very bytes the single call would have printed; a call with no
    # record of its own gets the quiet answer this host has always given. The
    # line goes into the answer as text, as `store batch` puts a verb's line in
    # `out` -- which is why the record's bytes are handed to `jq` as an argument
    # rather than read by it, and are not parsed on the way.
    n=$(jq 'length' "$req")
    i=0
    while [ "$i" -lt "$n" ]; do
      verb=$(jq -r ".[$i][0] // \"\"" "$req")
      num=$(jq -r ".[$i][1] // \"\"" "$req")
      f="$here/../.$verb-$num"
      if [ -f "$f" ]; then
        said=$(cat "$f")
      else
        case "$verb" in
          plan-show) said='{"api":1,"ok":true,"verb":"plan-show","planning":null}' ;;
          history-show) said='{"api":1,"ok":true,"verb":"history-show","rows":[]}' ;;
          approval-current) said='{"api":1,"ok":true,"verb":"approval-current","current":null}' ;;
          park-show) said='{"api":1,"ok":true,"verb":"park-show","park":null}' ;;
          question-known) said='{"api":1,"ok":true,"verb":"question-known","known":true}' ;;
          *) said='{"api":1,"ok":true,"verb":"'"$verb"'"}' ;;
        esac
      fi
      acc=$(printf '%s' "$acc" | jq -c --arg out "$said" '. + [{rc: 0, out: $out}]')
      i=$((i + 1))
    done
    rm -f "$req"
    printf '{"api":1,"ok":true,"verb":"batch","answers":%s}' "$acc" >"$out"
    answer 0 ;;
  $(sed -n 1p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"plan-show","sessions":[]}' >"$out" ;;
  cycle-show*) printf '%s' '{"api":1,"ok":true,"verb":"cycle-show","cycles":[]}' >"$out" ;;
  $(sed -n 2p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"history-show","rows":[]}' >"$out" ;;
  $(sed -n 3p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"park-show","parks":[]}' >"$out" ;;
  approval-current*) printf '%s' '{"api":1,"ok":true,"verb":"approval-current","current":null}' >"$out" ;;
  # The answer of any other verb is the one line of its own: a store of this
  # host has nothing to say. The leading dot is spelled out so that the body
  # file `tehas_core::testing` leaves beside the program cannot answer for a
  # read of one issue, whose name begins the same way.
  .store*.body | *.body) printf '{}' >"$out" ;;
  *) printf '%s' "{\"api\":1,\"ok\":true,\"verb\":\"$1\"}" >"$out" ;;
esac"#,
        );
        stub(&dir.join("bin"), "store", &store_body);
        dir
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
            // The running list of answers the stub of the store builds, one
            // element per call of the batch's request. The request of a run of
            // the batch is read here, where the poll's standard input is still
            // open: it answers the calls below and is recorded beside the verb,
            // so a case sees what each run asked.
            "acc=[]".to_string(),
            // The record of the call is the whole call as it was asked. `batch`
            // carries its request on standard input, as the store reads it, and
            // the request is recorded beside the verb: a case counts the runs of
            // the batch and sees the calls each one asked.
            format!(
                "printf '%s{sep}' \"$*\" >>\"$here/{name}-calls\"; printf '%s' \"$*\" >\"$args\"",
                sep = RECORD_SEP,
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
        // A call is one record: the arguments, and for `batch` the request the
        // poll wrote on its standard input, which the body reads. They are joined
        // with the separator that keeps a record of its own, so `calls_of` reads
        // one entry a call and sees the request beside the verb that asked it.
        let script = lines
            .join("\n")
            .replace("{name}", name)
            .replace("{sep}", RECORD_SEP)
            .replace("{req}", REQUEST_SEP)
            .replace("{rust}", RUST_SEP)
            + "\n";
        tehas_core::testing::write_exe(bin.join(name), script).unwrap();
    }

    /// Whether a comment call asks for the comments of #n: its alias in the
    /// chunk's one query.
    fn asks(call: &str, n: u64) -> bool {
        call.contains(&format!("i{n}: issue(number: {n})"))
    }

    /// The blocked world widened to `count` issues from #21, each named by the
    /// page, labelled blocked and held by #11: enough of them to fill more than
    /// one chunk of the comment read.
    fn wide_world(count: u64) -> Value {
        let mut w = world();
        let template = w["queue"]["data"]["repository"]["issues"]["nodes"][0].clone();
        let comment = w["comments"]["21"].clone();
        let labeled = w["events"]["21"].clone();
        let numbers: Vec<u64> = (21..21 + count).collect();
        w["queue"]["data"]["repository"]["issues"]["nodes"] = numbers
            .iter()
            .map(|n| {
                let mut node = template.clone();
                node["number"] = json!(n);
                node["title"] = json!(format!("issue {n}"));
                node
            })
            .collect();
        w["blocked"] = numbers
            .iter()
            .map(|n| json!({"number": n, "labels": [{"name": crate::decide::BLOCKED}]}))
            .collect();
        w["events"] = numbers
            .iter()
            .map(|n| (n.to_string(), labeled.clone()))
            .collect::<serde_json::Map<String, Value>>()
            .into();
        w["comments"] = numbers
            .iter()
            .map(|n| (n.to_string(), comment.clone()))
            .collect::<serde_json::Map<String, Value>>()
            .into();
        w
    }

    /// A world whose blocked issues all could start: the control issue's events
    /// are read, and every blocker is closed.
    fn startable(mut w: Value) -> Value {
        w["events"]["1"] = json!([]);
        // The markers are an allowed account's, so the rule trusts them.
        for (_, c) in w["comments"].as_object_mut().into_iter().flatten() {
            c["data"]["repository"]["issue"]["comments"]["nodes"][0]["author"]["login"] =
                json!("alice");
        }
        w["states"] = json!({"data": {"repository": {
            "b11": {"state": "CLOSED"}, "b12": {"state": "CLOSED"},
            "b13": {"state": "CLOSED"}}}});
        w
    }

    /// The world of the lifted-park case: #21 to #23 sit open on the queue's
    /// page with `tehas:ready` and no other flow, so rule 6 reads each of them
    /// and none of them is in the blocked or the asking batch. `at` is when the
    /// millwright applied the label, and `note_at` ten minutes before it is when
    /// its guidance comment was written.
    fn parked_world(count: u64, at: &str, note_at: &str) -> Value {
        let mut w = world();
        let numbers: Vec<u64> = (21..21 + count).collect();
        let node = |n: u64| {
            json!({
                "number": n, "title": format!("issue {n}"), "body": "Words.",
                "createdAt": "2026-10-01T00:00:00Z", "author": {"login": "alice"},
                "lastEditedAt": null, "editor": null,
                "labels": {"nodes": [{"name": "tehas:ready"}]},
                // The page's timeline holds the label's event, so the poll
                // trusts the page and pays no read by name for these issues.
                "timelineItems": {"nodes": [
                    {"__typename": "LabeledEvent",
                     "createdAt": at,
                     "actor": {"login": "tehas-fixer"}, "label": {"name": "tehas:ready"}}
                ]},
                "closedByPullRequestsReferences": {"nodes": []}
            })
        };
        // The millwright's comment of guidance for the issue: what `change_note`
        // lifts the park's hold on, and the read the batch now pays once.
        let note = |n: u64| {
            json!({"data": {"repository": {"issue": {"comments": {"nodes": [
                {"databaseId": 600 + n, "author": {"login": "tehas-fixer"},
                 "body": format!("<!-- tehas:millwright issue={n} action=change -->\\nDo not run the whole suite."),
                 "createdAt": note_at, "lastEditedAt": null, "editor": null,
                 "userContentEdits": {"totalCount": 0, "nodes": []}}
            ]}}}}})
        };
        w["queue"]["data"]["repository"]["issues"]["nodes"] =
            Value::Array(numbers.iter().map(|n| node(*n)).collect());
        w["ready"] = numbers
            .iter()
            .map(|n| json!({"number": n, "labels": [{"name": "tehas:ready"}]}))
            .collect();
        w["blocked"] = json!([]);
        w["states"] = json!({"data": {"repository": {}}});
        w["comments"] = numbers
            .iter()
            .map(|n| (n.to_string(), note(*n)))
            .collect::<serde_json::Map<String, Value>>()
            .into();
        // The events of the control issue are read by name, so its answer is
        // the empty list rather than the blocked world's label events.
        w["events"] = json!({"1": []});
        w["parks"] = Value::Array(numbers.iter().map(|n| json!(n)).collect());
        w["ready_at"] = json!(at);
        w
    }

    /// The settings of the lifted-park case: the millwright has an account, and
    /// a cycle is given the files its issue links, which is what makes the
    /// millwright's comment of guidance able to lift a park's hold.
    fn millwright_settings() -> Settings {
        let mut c = settings();
        c.millwright = "tehas-fixer".into();
        c.work_files = true;
        c
    }

    /// The store's answer of one `park-show <n>`: a park of the issue's own,
    /// told, with no retries and its question off, so only the millwright's
    /// comment of guidance can lift its hold.
    fn park_answer(sum: &str) -> String {
        format!(
            "{{\"api\":1,\"ok\":true,\"verb\":\"park-show\",\"park\":{{\"reason\":\"intent-park\",\"body_sha256\":\"{sum}\",\"retries\":0,\"question\":null,\"told_at\":\"2026-10-01T03:00:00Z\"}}}}"
        )
    }

    /// The store's answer of one `approval-current <n>`: the fixer's start on
    /// the text `sum` of the issue, resting on alice's approval of the same
    /// text, with a start left to the fixer. `at` is the instant the millwright
    /// applied the label, so the label and the approval cover one text.
    fn approval_answer(at: &str, sum: &str) -> String {
        let at = tehas_core::time::from_iso(at).unwrap_or_default();
        format!(
"{{\"api\":1,\"ok\":true,\"verb\":\"approval-current\",\"current\":{{\"at\":{at},\"by\":\"tehas-fixer\",\"how\":\"millwright\",\"body_sha256\":\"{sum}\",\"input\":{{}}}},\"rests_on\":{{\"at\":{at},\"by\":\"alice\",\"how\":\"label\",\"body_sha256\":\"{sum}\"}},\"millwright_starts\":1,\"fixer_starts\":1}}"
        )
    }

    /// A snapshot given the room of a quiet host, as the fixtures' world has it:
    /// a slice with nothing in it and disk to spare, so what the rules decide
    /// rests on the issues alone.
    fn roomy(mut snap: Value) -> Value {
        const GIB: u64 = 1_073_741_824;
        snap["slice"] = json!({
            "show": {"FragmentPath": "/etc/systemd/system/tehas.slice", "ActiveState": "active",
                     "MemoryMax": (20 * GIB).to_string()},
            "docker": "systemd 2",
            "controllers": "cpuset cpu io memory hugetlb pids rdma misc",
            "cpus": "8",
            "root": "/sys/fs/cgroup",
            "scopes": {},
            "pressure": "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0",
        });
        snap["disk_kb"] = json!(40 * 1_048_576u64);
        snap
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

    /// The runs of `store batch` the poll asked, as one call per run: the verb,
    /// the separator of the request, and the request itself. A `park-show` of the
    /// lists owed a run of its own is not a run of the batch.
    fn batch_runs(store: &[String]) -> Vec<String> {
        // The record of a run of the batch holds the verb, the separator, and the
        // request: `batch [..]` is the call, and its request is what a case reads.
        store
            .iter()
            .filter(|c| c.contains(REQUEST_SEP))
            .cloned()
            .collect()
    }

    /// The calls the runs of `store batch` asked, as one list: each element a
    /// `[verb, argument]` pair, in the order the store was asked for them.
    fn batched_calls(store: &[String]) -> Vec<Vec<String>> {
        batch_runs(store)
            .iter()
            .flat_map(|c| {
                // `batch«tehas-request»[..]park-show --automatic`: the request is
                // the JSON array between the separator and the verb of the call
                // whose record follows it.
                let after_verb = c.split_once(REQUEST_SEP).map_or("", |(_, r)| r);
                let request = after_verb
                    .split_once(RUST_SEP)
                    .map_or(after_verb, |(r, _)| r);
                let Ok(Value::Array(calls)) = serde_json::from_str(request) else {
                    return Vec::new();
                };
                calls
                    .into_iter()
                    .filter_map(|c| {
                        let pair = c.as_array()?.clone();
                        (pair.len() == 2).then(|| {
                            pair.iter()
                                .map(|a| a.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                    })
                    .collect()
            })
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
    /// as long as this call, so a case cannot leave it behind. The clock of
    /// `run.rs` is held for the whole of it: the poll's reads are measured against
    /// that clock, and a retry case of `run.rs` that set it while one of them ran
    /// would be the reason the read gave up.
    fn poll(world: &Value, broken: &[u64]) -> (Value, Vec<String>, u32, Vec<String>) {
        poll_with(world, broken, &settings())
    }

    /// `poll`, under settings of the case's own.
    fn poll_with(
        world: &Value,
        broken: &[u64],
        c: &Settings,
    ) -> (Value, Vec<String>, u32, Vec<String>) {
        let _clock = crate::read::run::no_case_clock();
        let dir = host(world, broken);
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, c) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        let gh = src.calls.gh_count;
        (snap, calls_of(&dir, "gh"), gh, calls_of(&dir, "store"))
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
                retry: run::RetryPolicy::default(),
            },
            systemctl: "true".into(),
            now: 1_793_404_800,
            models: Models {
                file: tehas_core::models::ModelFile { json: Value::Null },
                used: Vec::new(),
                provider: String::new(),
                model: String::new(),
            },
            endpoint: (String::new(), String::new()),
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

        // One aliased query stood for the whole list, and it asked only of the
        // blocked issues: a per-issue read pays `blocked_len` calls, and a batch
        // widened past the list carries a number no rule reads (#1198).
        let comment_calls = queries(&calls, COMMENT_MARK);
        assert_eq!(
            comment_calls.len(),
            1,
            "one comment call for the {blocked_len} blocked issues: {calls:?}"
        );
        for n in 21..=23 {
            assert!(
                asks(&comment_calls[0], n),
                "issue {n} is asked for its comments: {}",
                comment_calls[0]
            );
        }
        assert_eq!(
            comment_calls[0].matches(": issue(number:").count(),
            blocked_len,
            "the query asks of the blocked issues and of no other"
        );
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
        // The store is still asked once for the per-issue reads of the three
        // issues: batching the two reads of rule 6 did not turn one run of
        // `store batch` into three, and every park and approval of the list is
        // asked in one of its requests rather than by a call of its own.
        let runs = batch_runs(&store);
        let asked = batched_calls(&store);
        for n in 21..=23 {
            assert!(
                asked
                    .iter()
                    .any(|c| c == &["park-show".to_string(), n.to_string()]),
                "issue {n}'s park is asked in a run of the batch: {store:?}"
            );
            assert!(
                asked
                    .iter()
                    .any(|c| c == &["approval-current".to_string(), n.to_string()]),
                "issue {n}'s approval is asked in a run of the batch: {store:?}"
            );
        }
        // The three issues are two reads apiece: the runs that asked them are
        // fewer than the calls, so the list went in batches and not one apiece.
        assert!(
            runs.len() < asked.len(),
            "{} runs of the store answered the {asked:?} of the batch",
            runs.len()
        );
    }

    /// With TEHAS_ISSUE_TYPES mapping improvement, the page asks each issue's
    /// type and the owner's types, and an issue of the type carries
    /// `tehas:improvement` in every read the rules take of it, with no read of
    /// its events by name. Without the setting the page asks neither.
    #[test]
    fn an_issue_of_the_improvement_type_carries_the_label_and_costs_no_read() {
        let mut w = world();
        let typed = |n: u64, t: &str| {
            json!({"number": n, "title": format!("issue {n}"), "body": "",
                "createdAt": "2026-10-01T00:00:00Z", "author": {"login": "alice"},
                "lastEditedAt": null, "editor": null, "labels": {"nodes": []},
                "issueType": {"name": t}, "timelineItems": {"nodes": []},
                "closedByPullRequestsReferences": {"nodes": []}})
        };
        let repo = &mut w["queue"]["data"]["repository"];
        repo["issueTypes"] = json!({"nodes": [{"name": "Task"}, {"name": "Improvement"}]});
        let nodes = repo["issues"]["nodes"].as_array_mut().unwrap();
        nodes.push(typed(40, "Improvement"));
        nodes.push(typed(41, "Task"));
        let mut c = settings();
        c.improvement_type = "Improvement".into();
        let (snap, calls, _, _) = poll_with(&w, &[], &c);
        assert_eq!(snap["issue_types"], json!(["Task", "Improvement"]));
        let improvement = json!([{"name": crate::decide::IMPROVEMENT}]);
        assert_eq!(snap["issues"]["40"]["work"]["labels"], improvement);
        assert_eq!(snap["issues"]["40"]["view"]["labels"], improvement);
        assert_eq!(snap["issues"]["41"]["work"]["labels"], json!([]));
        assert!(!calls.iter().any(|c| c.contains("issues/40/events")));
        let page = queries(&calls, "hasNextPage");
        assert!(
            page[0].contains("issueType { name }") && page[0].contains("issueTypes(first: 50)")
        );

        let (snap, calls, _, _) = poll(&w, &[]);
        assert_eq!(snap["issues"]["40"]["work"]["labels"], json!([]));
        assert!(snap.get("issue_types").is_none());
        assert!(!queries(&calls, "hasNextPage")[0].contains("issueType"));
    }

    /// An open issue titled with the epic marker that lacks `epic` is listed for
    /// the label. Its events are read whole when the page holds none of the
    /// label, so a removal the page cut off is still seen; one the page does
    /// show, and an issue that carries the label, cost no read.
    #[test]
    fn an_epic_titled_issue_is_listed_and_its_events_read_whole_when_the_page_shows_none() {
        let mut w = world();
        let epic = |n: u64, labels: Value, timeline: Value| {
            json!({"number": n, "title": "EPIC: Billing rework", "body": "",
                "createdAt": "2026-10-01T00:00:00Z", "author": {"login": "alice"},
                "lastEditedAt": null, "editor": null, "labels": {"nodes": labels},
                "timelineItems": {"nodes": timeline},
                "closedByPullRequestsReferences": {"nodes": []}})
        };
        let removed = json!([{"__typename": "UnlabeledEvent", "createdAt": "2026-10-02T00:00:00Z",
            "actor": {"login": "alice"}, "label": {"name": "epic"}}]);
        let carried = json!([{"__typename": "LabeledEvent", "createdAt": "2026-10-02T00:00:00Z",
            "actor": {"login": "alice"}, "label": {"name": "epic"}}]);
        let nodes = w["queue"]["data"]["repository"]["issues"]["nodes"]
            .as_array_mut()
            .unwrap();
        nodes.push(epic(30, json!([]), json!([])));
        nodes.push(epic(31, json!([]), removed));
        nodes.push(epic(32, json!([{"name": "epic"}]), carried));
        let mut title = epic(33, json!([]), json!([]));
        title["title"] = json!("feat: a part of epic 30");
        nodes.push(title);
        w["events"]["30"] = json!([{"event": "unlabeled", "label": {"name": "epic"},
            "actor": {"login": "bob"}, "created_at": "2026-09-01T00:00:00Z"}]);
        let (snap, calls, _, _) = poll(&w, &[]);
        assert_eq!(snap["epic_titled"], json!([30, 31]));
        for n in 30..=33 {
            let read = calls
                .iter()
                .filter(|c| c.contains(&format!("issues/{n}/events?")))
                .count();
            assert_eq!(read, usize::from(n == 30), "#{n}: {calls:?}");
        }
        assert_eq!(snap["issues"]["30"]["events"], w["events"]["30"]);
    }

    /// The refreshing sweep reads the events of its pull requests side by side,
    /// as the read of the issues that lost the mark does (#1423): the stub
    /// answers the events of #31, #32 and #33 only once all three have started,
    /// so a sweep that reads them one at a time is refused, and each pull
    /// request keeps its own answer. A refused read is `{"failed": true}`.
    #[test]
    fn the_refreshing_sweep_reads_the_events_of_its_pull_requests_side_by_side() {
        let mut w = world();
        let labeled = |n: u64| {
            json!([{"event": "labeled", "label": {"name": "tehas:refreshing"},
                    "actor": {"login": "tehas-bot"}, "created_at": format!("2026-10-0{}T00:00:00Z", n - 30)}])
        };
        w["refreshing"] = json!([31, 32, 33, 34].map(|n| json!({"number": n,
            "labels": [{"name": "tehas:refreshing"}]})));
        for n in 31..=33u64 {
            w["events"][n.to_string()] = labeled(n);
        }
        w["together"] = json!([31, 32, 33]);
        // #34 has no events to give, so its read is refused.
        let mut s = settings();
        s.refresh_conflicts = true;
        let (snap, calls, gh_count, _) = poll_with(&w, &[], &s);

        for n in 31..=33u64 {
            assert_eq!(
                snap["issues"][n.to_string()]["events"],
                labeled(n),
                "pull request {n} keeps its own events: {calls:?}"
            );
        }
        assert_eq!(snap["issues"]["34"]["events"], json!({"failed": true}));
        for n in 31..=34u64 {
            let path = format!("repos/acme/widget/issues/{n}/events?per_page=100");
            assert!(
                calls.iter().any(|c| c.contains(&path)),
                "pull request {n} has its events read: {calls:?}"
            );
        }
        assert_eq!(
            gh_count as usize,
            calls.len(),
            "the poll counted every gh call it made"
        );
    }

    /// With `TEHAS_REFRESH_CONFLICTS` off and no refresh on record, the
    /// pull requests marked tehas:refreshing are listed while a request
    /// `tehasbot refresh --pr N` wrote waits, and not for a request without
    /// that marker.
    #[test]
    fn the_refreshing_list_is_read_with_the_setting_off_while_a_pr_request_waits() {
        for (body, read) in [(r#"{"asked":"pr"}"#, true), (r#"{"at":"x"}"#, false)] {
            let _clock = crate::read::run::no_case_clock();
            let mut w = world();
            w["refreshing"] = json!([]);
            let dir = host(&w, &[]);
            std::fs::write(dir.join("state/refresh/40.json"), body).unwrap();
            let mut src = sources(&dir);
            let s = settings();
            assert!(!s.refresh_conflicts, "the setting is off");
            let snap = match snapshot(&mut src, &s) {
                Ok(s) => s,
                Err(f) => panic!("the stubs answer: {}", f.line),
            };
            let calls = calls_of(&dir, "gh");
            assert_eq!(
                calls.iter().any(|c| c.contains("tehas:refreshing")),
                read,
                "{body}: {calls:?}"
            );
            assert_eq!(snap.get("refreshing").is_some(), read, "{body}");
            assert_eq!(snap["refresh_requests"], json!([40]));
        }
    }

    /// The record of a plan's start is read for the issues the rule can start,
    /// each its own, and it reaches the type the rule reads.
    #[test]
    fn a_candidate_is_given_the_record_of_its_plan_and_no_other_is() {
        // The clock of `run.rs` stands for the reads of this host, as it does for
        // every poll of these cases.
        let _clock = crate::read::run::no_case_clock();
        let dir = host(&world(), &[]);
        std::fs::create_dir_all(dir.join("state/plan-start")).unwrap();
        for n in [21, 99] {
            let body = format!(r#"{{"by":"alice","sum":"{n}"}}"#);
            std::fs::write(host::plan_start_file(&dir, n), body).unwrap();
        }
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
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

    /// A chunk whose comment query GitHub refused is a failed read for every
    /// issue of it, not an empty one: the snapshot holds `{"failed": true}` for
    /// each, which `Snapshot::parse` turns into no comments at all, and decide
    /// ends the poll on it rather than reading an issue as one that names no
    /// blocker, so a hold stays a hold and nothing starts.
    #[test]
    fn a_failed_chunk_holds_every_issue_of_it_and_starts_nothing() {
        let w = startable(world());
        // With every read answered, the poll goes as far as a start.
        let (snap, _, _, _) = poll(&w, &[]);
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap)), &settings(), true);
        assert!(
            !d.picked.is_empty(),
            "the answered world starts: {:?}",
            d.log
        );
        // #22's read is refused, and #22 shares its chunk with #21 and #23.
        let (snap, calls, _, _) = poll(&w, &[22]);
        let parsed = Snapshot::parse(&snap);
        for n in 21..=23 {
            assert_eq!(
                snap["issues"][n.to_string()]["comments"],
                json!({"failed": true}),
                "issue {n} shares the refused chunk"
            );
            assert!(
                parsed.issues[&n].comments.is_none(),
                "issue {n} has no comments to decide on"
            );
        }
        // The chunk met an HTTP 500, which the rule of `retry` owes its retries
        // to: the one query is asked again whole, never one issue at a time, so
        // every comment call of the poll is that same chunk.
        let comment_calls = queries(&calls, COMMENT_MARK);
        assert_eq!(
            comment_calls.len(),
            3,
            "the chunk and its two retries: {calls:?}"
        );
        assert!(
            comment_calls.iter().all(|c| c == &comment_calls[0]),
            "a retry asks the chunk again whole: {comment_calls:?}"
        );
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap.clone())), &settings(), true);
        assert!(d.picked.is_empty(), "nothing starts: {:?}", d.picked);
        assert_ne!(d.exit, 0, "the poll ends on the failed read: {:?}", d.log);
        assert!(
            d.log.iter().any(|l| l.starts_with("ERROR:")),
            "the poll says the read failed: {:?}",
            d.log
        );
    }

    /// The comment read asks ten issues a query: twelve blocked issues are two
    /// chunks, each issue asked once. A chunk that failed holds its own issues
    /// and spoils none of the other chunk's answers.
    #[test]
    fn the_comment_read_asks_ten_issues_a_query_and_a_failed_chunk_spoils_only_its_own() {
        let w = startable(wide_world(12));
        assert_eq!(github::COMMENT_CHUNK, 10, "the chunk this case is cut to");
        let (snap, calls, _, _) = poll(&w, &[]);
        let comment_calls = queries(&calls, COMMENT_MARK);
        assert_eq!(comment_calls.len(), 2, "two chunks: {calls:?}");
        for n in 21..=32 {
            assert_eq!(
                comment_calls.iter().filter(|c| asks(c, n)).count(),
                1,
                "issue {n} is asked once"
            );
            assert_eq!(
                snap["issues"][n.to_string()]["comments"],
                w["comments"][n.to_string()]
            );
        }
        let sizes: Vec<usize> = comment_calls
            .iter()
            .map(|c| c.matches(": issue(number:").count())
            .collect();
        assert_eq!(sizes.iter().max(), Some(&10), "{sizes:?}");
        assert_eq!(sizes.iter().sum::<usize>(), 12, "{sizes:?}");

        // #32 is refused: its chunk is #31 and #32.
        let (snap, _, _, _) = poll(&w, &[32]);
        for n in 21..=30 {
            assert_eq!(
                snap["issues"][n.to_string()]["comments"],
                w["comments"][n.to_string()],
                "issue {n} keeps the answer of its own chunk"
            );
        }
        for n in [31u64, 32] {
            assert_eq!(
                snap["issues"][n.to_string()]["comments"],
                json!({"failed": true})
            );
        }
        // The answered chunk's issues start as they would have; the refused
        // chunk's are held.
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap.clone())), &settings(), true);
        assert!(
            !d.picked.is_empty(),
            "the answered chunk starts: {:?}",
            d.log
        );
        assert!(
            !d.picked.iter().any(|n| [31, 32].contains(n)),
            "the refused chunk's issues are held: {:?}",
            d.picked
        );

        // #21 is refused: its chunk is #21 to #30, which the rule reaches first.
        let (snap, _, _, _) = poll(&w, &[21]);
        for n in 21..=30 {
            assert_eq!(
                snap["issues"][n.to_string()]["comments"],
                json!({"failed": true}),
                "issue {n} shares the refused chunk"
            );
        }
        for n in [31u64, 32] {
            assert_eq!(
                snap["issues"][n.to_string()]["comments"],
                w["comments"][n.to_string()],
                "issue {n} keeps the answer of its own chunk"
            );
        }
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap.clone())), &settings(), true);
        assert!(d.picked.is_empty(), "nothing starts: {:?}", d.picked);
        assert_ne!(d.exit, 0, "the poll ends on the failed chunk: {:?}", d.log);
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
        // The question read and the blocked read are one query: the asking
        // issue is in it once, beside the two blocked issues it does not cover.
        assert_eq!(comment_calls.len(), 1, "{calls:?}");
        assert_eq!(
            comment_calls[0].matches("i22: issue(number: 22)").count(),
            1,
            "the double-labelled issue is asked for its comments once: {calls:?}"
        );
        assert_eq!(comment_calls[0].matches(": issue(number:").count(), 3);
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

    /// Three parked issues whose park may lift read their comments in the one
    /// batched call the asking and the blocked issues are read in: three of them
    /// cost the same gh calls as one, and the poll pays no read of one issue at a
    /// time (#1773). Each keeps its own answer, and what the rule lifts on is the
    /// comment the batch read, so the hold lets the issue start.
    ///
    /// The count is the contract, not the snapshot's shape: a read that went back
    /// to one `github::comments` call per lifted park pays one gh process for
    /// each, and `calls.gh_count` moves with the number of issues.
    #[test]
    fn the_lifted_parks_read_their_comments_in_the_one_batched_call() {
        // The millwright applied `tehas:ready` at 23:58 and wrote its comment of
        // guidance in the ten minutes before, which is what `change_note` lifts
        // the park's hold on.
        let at = "2026-10-30T23:58:00Z";
        let note_at = "2026-10-30T23:55:00Z";
        let mut counts: Vec<(u32, usize, Vec<usize>)> = Vec::new();
        // The runs of `store batch` one poll of this host asks, counted once here
        // and asserted again for every width of the park list: three parks pay
        // what one does, and neither number is read from the world.
        let mut one_batch_runs: Vec<String> = Vec::new();
        for count in [1u64, 3] {
            let w = parked_world(count, at, note_at);
            let (snap, calls, gh_count, store) = poll_with(&w, &[], &millwright_settings());
            if count == 1 {
                one_batch_runs = batch_runs(&store);
                assert!(
                    !one_batch_runs.is_empty(),
                    "the poll asks the store's batch: {store:?}"
                );
            }
            let comment_calls = queries(&calls, COMMENT_MARK);
            let parks: Vec<Value> = (21..21 + count)
                .map(|n| snap["issues"][n.to_string()]["park"].clone())
                .collect();
            assert_eq!(
                parks.iter().filter(|p| p.is_object()).count(),
                count as usize,
                "{count} parks are read from the store: {parks:?}"
            );
            counts.push((
                gh_count,
                comment_calls.len(),
                comment_calls
                    .iter()
                    .map(|c| c.matches(": issue(number:").count())
                    .collect::<Vec<usize>>(),
            ));

            // One call of the chunked read, not one per issue, and it asked each
            // of the parked issues exactly once.
            assert_eq!(
                comment_calls.len(),
                1,
                "{count} lifted parks are one comment call: {calls:?}"
            );
            assert_eq!(
                comment_calls[0].matches(": issue(number:").count(),
                count as usize,
                "the query asks of the lifted parks and of no other: {}",
                comment_calls[0]
            );
            for n in 21..21 + count {
                assert!(
                    asks(&comment_calls[0], n),
                    "issue {n} is asked for its comments: {}",
                    comment_calls[0]
                );
                assert_eq!(
                    snap["issues"][n.to_string()]["comments"],
                    w["comments"][n.to_string()],
                    "issue {n} keeps the answer the batch gave"
                );
            }
            // The per-issue read is gone: no call asks one issue by the single
            // query's own argument, only the aliased chunk of the batch.
            assert!(
                !calls.iter().any(|c| c.contains("number=2")),
                "no per-issue comment read is paid: {calls:?}"
            );
            // The parks and the approvals are asked in the batch, not one call
            // apiece, and the runs the store was asked in do not move with the
            // number of issues.
            let calls_of_runs = batched_calls(&store);
            for n in 21..21 + count {
                assert!(
                    calls_of_runs
                        .iter()
                        .any(|c| c == &["park-show".to_string(), n.to_string()]),
                    "issue {n}'s park is asked in the batch: {store:?}"
                );
                assert!(
                    calls_of_runs
                        .iter()
                        .any(|c| c == &["approval-current".to_string(), n.to_string()]),
                    "issue {n}'s approval is asked in the batch: {store:?}"
                );
            }
            // The runs are the same shape whatever the width of the park list:
            // the parks go in the run that asks the candidates, so three pay what
            // one does. Only the request of each run differs, by its calls.
            let shape = |runs: &Vec<String>| {
                runs.iter()
                    .map(|c| {
                        c.split_once(REQUEST_SEP)
                            .map_or_else(|| c.clone(), |(v, _)| format!("{v}{REQUEST_SEP}"))
                    })
                    .collect::<Vec<String>>()
            };
            assert_eq!(
                shape(&batch_runs(&store)),
                shape(&one_batch_runs),
                "{count} lifted parks pay the runs one does: {store:?}"
            );

            // The decision the comments carry: the hold lifts and the issue starts.
            let d = crate::decide::decide(
                &Snapshot::parse(&roomy(snap.clone())),
                &millwright_settings(),
                true,
            );
            let picked: Vec<u64> = (21..21 + count).collect();
            assert_eq!(d.picked, picked, "every lifted park starts: {:?}", d.log);
            for n in 21..21 + count {
                assert!(
                    d.log.iter().any(|l| l
                        == &format!("lift #{n}: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment")),
                    "issue {n} says what lifted it: {:?}",
                    d.log
                );
            }
        }
        // Three parks cost what one does: the poll's own count, the number of
        // comment calls and the width of that one query all held still.
        let (one, one_calls, _) = &counts[0];
        let (three, three_calls, three_sizes) = &counts[1];
        assert_eq!(
            three, one,
            "three lifted parks pay the gh calls one does: {counts:?}"
        );
        assert_eq!(three_calls, one_calls, "{counts:?}");
        assert_eq!(three_sizes, &vec![3], "{counts:?}");
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
        assert_eq!(comment_calls.len(), 1, "{calls:?}");
        assert!(asks(&comment_calls[0], 21) && asks(&comment_calls[0], 22));
        assert!(
            !comment_calls.iter().any(|c| asks(c, 23)),
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

    /// A pull request node of the page as GitHub gives it, its body the field
    /// the hold of rule 6 reads.
    fn pull(n: u64, body: &str) -> Value {
        pull_body(n, json!(body))
    }

    /// The same with a body of any shape, for a page that answers something
    /// other than a string: GitHub gives null for a pull request with no
    /// description at all.
    fn pull_body(n: u64, body: Value) -> Value {
        json!({"number": n, "title": format!("pull {n}"), "body": body,
               "headRefName": format!("feature/other-{n}"),
               "isCrossRepository": false, "headRepository": {"nameWithOwner": "acme/widget"},
               "author": {"login": "alice"}, "labels": {"nodes": []}})
    }

    /// A pull request that is not the repository's own names nothing: one from
    /// a fork, one of an account neither the factory nor allowed, and one whose
    /// author or head cannot be read. Each closes #21 and #22 in its body, and
    /// neither issue is held.
    #[test]
    fn a_pull_request_not_of_the_repository_names_nothing() {
        type Change = fn(&mut Value);
        let cases: [(&str, Change); 6] = [
            ("an outsider's fork", |p| {
                p["isCrossRepository"] = json!(true);
                p["headRepository"] = json!({"nameWithOwner": "mallory/widget"});
                p["author"] = json!({"login": "mallory"});
            }),
            ("an allowed account's fork", |p| {
                p["isCrossRepository"] = json!(true);
                p["headRepository"] = json!({"nameWithOwner": "alice/widget"});
            }),
            ("an outsider on a branch of the repository", |p| {
                p["author"] = json!({"login": "mallory"});
            }),
            ("a head of another repository", |p| {
                p["headRepository"] = json!({"nameWithOwner": "acme/other"});
            }),
            ("no author", |p| {
                p["author"] = Value::Null;
            }),
            ("no cross-repository answer", |p| {
                p.as_object_mut().unwrap().remove("isCrossRepository");
            }),
        ];
        for (what, change) in cases {
            let mut w = world();
            let mut p = pull(90, "Closes #21, #22");
            change(&mut p);
            w["queue"]["data"]["repository"]["openPulls"]["nodes"] = json!([p]);
            let (snap, _, _, _) = poll(&w, &[]);
            assert_eq!(snap.get("pulls_name"), None, "{what} names nothing");
            assert!(
                Snapshot::parse(&snap).pulls_name.is_empty(),
                "{what} holds nothing"
            );
        }
    }

    /// The factory's own pull request and an allowed account's, both on a
    /// branch of the repository, name what their bodies close.
    #[test]
    fn the_factorys_and_an_allowed_accounts_pull_requests_name_their_issues() {
        let mut w = world();
        let mut bot = pull(90, "Closes #21");
        bot["author"] = json!({"login": "app/tehas-bot"});
        let mut person = pull(91, "Fixes #22");
        person["headRepository"] = json!({"nameWithOwner": "Acme/Widget"});
        w["queue"]["data"]["repository"]["openPulls"]["nodes"] = json!([bot, person]);
        let (snap, _, _, _) = poll(&w, &[]);
        assert_eq!(snap["pulls_name"], json!({"21": [90], "22": [91]}));
    }

    /// GitHub links a fork's pull request that closes an issue too: the linked
    /// list keeps only the repository's own, so an outsider's holds nothing.
    #[test]
    fn a_linked_pull_request_not_of_the_repository_is_left_out() {
        let mut w = world();
        let own = json!({"url": "https://github.com/acme/widget/pull/90", "state": "OPEN",
            "isCrossRepository": false, "author": {"login": "tehas-bot"},
            "headRepository": {"nameWithOwner": "acme/widget"}});
        let fork = json!({"url": "https://github.com/acme/widget/pull/91", "state": "OPEN",
            "isCrossRepository": true, "author": {"login": "mallory"},
            "headRepository": {"nameWithOwner": "mallory/widget"}});
        let nodes = w["queue"]["data"]["repository"]["issues"]["nodes"]
            .as_array_mut()
            .unwrap();
        nodes[0]["closedByPullRequestsReferences"]["nodes"] = json!([fork, own]);
        nodes[1]["closedByPullRequestsReferences"]["nodes"] = json!([fork]);
        let (snap, _, _, _) = poll(&w, &[]);
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.issues[&21].prs, Some(vec![own]));
        assert_eq!(parsed.issues[&22].prs, Some(vec![]));
    }

    /// The page's own bodies are what hold an issue a pull request into another
    /// branch closes: GitHub records a closing reference only for a pull
    /// request into the default branch, so an issue whose
    /// `closedByPullRequestsReferences` is empty is held by the body alone,
    /// and `Snapshot::parse` carries the numbers to `pulls_name` for rule 6.
    #[test]
    fn an_open_pull_request_naming_the_issue_in_its_body_is_written_as_holding_it() {
        let mut w = world();
        w["queue"]["data"]["repository"]["openPulls"]["nodes"] = json!([
            pull(90, "Closes #21. Fixes #22."),
            // A body that names no keyword, one that names a number with a
            // colon but is not one of the closing words, one that names a
            // longer number, and one of no body at all: none holds anything.
            pull(91, "See #21 and issue 22 for background"),
            pull(92, "Resolves #212, which is not #21"),
            {"number": 93, "title": "no body", "headRefName": "x",
             "author": {"login": "alice"}, "labels": {"nodes": []}},
        ]);
        let (snap, _, _, _) = poll(&w, &[]);
        assert_eq!(
            snap["pulls_name"],
            json!({"21": [90], "22": [90]}),
            "the bodies name #21 and #22, and no other number"
        );
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.pulls_name[&21], vec![90]);
        assert_eq!(parsed.pulls_name[&22], vec![90]);
        assert!(
            !parsed.pulls_name.contains_key(&23),
            "an issue no body names is absent: {:?}",
            parsed.pulls_name
        );
    }

    /// The hold reads only the open pull requests the page answered: a page
    /// GitHub cut before `openPulls` writes no names, so no issue is held by a
    /// body no poll read.
    #[test]
    fn a_page_that_answered_no_open_pull_requests_writes_no_names() {
        let mut w = world();
        w["queue"]["data"]["repository"]
            .as_object_mut()
            .unwrap()
            .remove("openPulls");
        let (snap, _, _, _) = poll(&w, &[]);
        assert_eq!(snap.get("pulls_name"), None);
        assert!(Snapshot::parse(&snap).pulls_name.is_empty());
    }

    /// A body of no shape a rule reads closes nothing: a description that is
    /// null, a number, a list or an object is no text with a keyword in it, and
    /// none of them holds an issue.
    #[test]
    fn a_body_of_no_shape_a_rule_reads_closes_nothing() {
        for body in [
            Value::Null,
            json!(7),
            json!(["Closes #21"]),
            json!({"x": "Closes #21"}),
            json!(true),
        ] {
            let mut w = world();
            w["queue"]["data"]["repository"]["openPulls"]["nodes"] =
                json!([pull_body(90, body.clone())]);
            let (snap, _, _, _) = poll(&w, &[]);
            assert_eq!(
                snap.get("pulls_name"),
                None,
                "a body of {body} names nothing"
            );
            assert!(
                Snapshot::parse(&snap).pulls_name.is_empty(),
                "a body of {body} holds nothing"
            );
        }
    }

    /// A page whose `openPulls` answers no list of nodes is a list nothing is
    /// known of: no body is read, so no issue is held on one.
    #[test]
    fn a_page_whose_pull_requests_are_no_list_writes_no_names() {
        for nodes in [Value::Null, json!(7), json!({"number": 90})] {
            let mut w = world();
            w["queue"]["data"]["repository"]["openPulls"] = json!({"nodes": nodes});
            let (snap, _, _, _) = poll(&w, &[]);
            assert_eq!(
                snap.get("pulls_name"),
                None,
                "nodes of {nodes} name nothing"
            );
            assert!(
                Snapshot::parse(&snap).pulls_name.is_empty(),
                "nodes of {nodes} hold nothing"
            );
        }
    }

    /// The numbers of `pulls_name` are read the way every other number of the
    /// snapshot is: a key or a hit of a numeric string reads as the number it
    /// is, and one of no shape a reader takes is dropped with its own entry,
    /// never counted as a pull request that holds the issue.
    #[test]
    fn the_numbers_of_pulls_name_read_as_the_snapshot_reads_a_number() {
        let snap = json!({"pulls_name": {
            "7": [90.5, "91", 92],
            "8": ["93"],
            "9": [],
            "x": [94],
            "10": "93",
            "11": ["not a number"],
        }});
        let parsed = Snapshot::parse(&snap);
        assert_eq!(
            parsed.pulls_name[&7],
            vec![91, 92],
            "a hit that is no number is dropped, the rest of the entry is read"
        );
        assert_eq!(parsed.pulls_name[&8], vec![93], "a hit of a string");
        assert_eq!(
            parsed.pulls_name.get(&9),
            None,
            "an entry of no pull requests holds an issue on nothing"
        );
        assert_eq!(parsed.pulls_name.get(&10), None, "hits of no list");
        assert_eq!(
            parsed.pulls_name.get(&11),
            None,
            "an entry whose every hit is unreadable"
        );
        assert!(
            !parsed.pulls_name.contains_key(&0),
            "a key of no number names no issue: {:?}",
            parsed.pulls_name
        );
    }
}
