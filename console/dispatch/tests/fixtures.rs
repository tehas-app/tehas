//! The dispatcher's rules as fixtures: each case of a test/dispatch_*_test.sh
//! that decides something, ported with the same expectations. A suite is one
//! file in tests/fixtures/. Its cases start from the world test/dispatch_lib.sh's
//! reset() makes, and a few shorthands stand for its helpers.

use serde_json::{json, Map, Value};
use std::fs;
use tehas_dispatch::{decide, Settings, Snapshot};

const GIB: u64 = 1_073_741_824;

/// The world of reset(): one issue, #7, labelled tehas:ready by alice; one
/// slot; a slice of 20 GiB with nothing in it; 40 GB of disk.
fn base() -> (Value, Value) {
    let settings = json!({
        "TEHAS_REPO": "acme/widget", "TEHAS_CONTROL_ISSUE": 1,
        "TEHAS_ALLOWED_LABELERS": "alice", "TEHAS_TICKET_CONCURRENCY": 1,
    });
    let snapshot = json!({
        "repo": "acme/widget",
        "user": {"login": "tehas-bot"},
        "control": {"labels": [], "events": []},
        "plan_open": [],
        "running": [],
        "ready": [{"number": 7, "labels": [{"name": "tehas:ready"}]}],
        "blocked": [],
        "awaiting": [],
        "limit": {"state": "none"},
        "slice": {
            "show": {"FragmentPath": "/etc/systemd/system/tehas.slice", "ActiveState": "active",
                     "MemoryMax": (20 * GIB).to_string()},
            "docker": "systemd 2",
            "controllers": "cpuset cpu io memory hugetlb pids rdma misc",
            "cpus": "8",
            "root": "/sys/fs/cgroup",
            "scopes": {},
            "pressure": pressure("0.00", "0.00"),
        },
        "endpoint": {},
        "disk_kb": 40 * 1_048_576,
        "issues": {},
    });
    (settings, snapshot)
}

fn pressure(full: &str, some: &str) -> String {
    format!("some avg10=0.00 avg60={some} avg300=0.00 total=0\nfull avg10=0.00 avg60={full} avg300=0.00 total=0\n")
}

/// slice_child: one container in the slice.
fn scope(anon: u64, slab: u64, low: u64) -> Value {
    json!({
        "stat": format!("anon {anon}\nfile 0\nkernel {slab}\nkernel_stack 0\nshmem 0\nslab_reclaimable {slab}\nslab_unreclaimable 0\n"),
        "low": format!("{low}\n"),
    })
}

fn id(c: &str) -> String {
    c.repeat(64)
}

/// Objects merge key by key; anything else is replaced.
fn merge(a: &mut Value, b: &Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (a, b) => *a = b.clone(),
    }
}

fn ready_event(by: &str, at: Option<&str>) -> Value {
    let mut e = json!({"event": "labeled", "label": {"name": "tehas:ready"}, "actor": {"login": by}});
    if let Some(at) = at {
        e["created_at"] = json!(at);
    }
    e
}

/// The shorthands of a case, written into its snapshot.
fn expand(c: &Value, snap: &mut Value) {
    let issues = |snap: &mut Value| -> Map<String, Value> {
        snap["issues"].as_object().cloned().unwrap_or_default()
    };
    // ready: "7", "8!": the queue as ready() writes it, each labelled by
    // alice; "8!" is urgent, by alice too.
    if let Some(list) = c.get("ready_n").and_then(Value::as_array) {
        let mut ready = Vec::new();
        let mut is = issues(snap);
        for n in list {
            let n = n.as_str().map(str::to_string).unwrap_or_else(|| n.to_string());
            let num: u64 = n.trim_end_matches('!').parse().unwrap();
            let mut labels = vec![json!({"name": "tehas:ready"})];
            let mut events = vec![ready_event("alice", None)];
            if n.ends_with('!') {
                labels.push(json!({"name": "tehas:urgent"}));
                events.push(json!({"event": "labeled", "label": {"name": "tehas:urgent"}, "actor": {"login": "alice"}}));
            }
            ready.push(json!({"number": num, "labels": labels}));
            is.insert(num.to_string(), json!({"events": events}));
        }
        snap["ready"] = json!(ready);
        snap["issues"] = Value::Object(is);
    }
    // queue: "7", "9 tehas:next": each labelled tehas:ready by alice at
    // 2026-10-01T00:00:00Z, with the labels named after the number.
    if let Some(list) = c.get("queue").and_then(Value::as_array) {
        let mut ready = Vec::new();
        let mut is = issues(snap);
        for arg in list {
            let arg = arg.as_str().map(str::to_string).unwrap_or_else(|| arg.to_string());
            let mut w = arg.split(' ');
            let num: u64 = w.next().unwrap().parse().unwrap();
            let mut labels = vec![json!({"name": "tehas:ready"})];
            labels.extend(w.map(|l| json!({"name": l})));
            ready.push(json!({"number": num, "labels": labels}));
            is.insert(
                num.to_string(),
                json!({"events": [ready_event("alice", Some("2026-10-01T00:00:00Z"))]}),
            );
        }
        snap["ready"] = json!(ready);
        snap["issues"] = Value::Object(is);
    }
    // label_events: tehas:ready by alice, then each label in turn, a minute
    // apart; "-<label>=<login>" takes it off.
    if let Some(m) = c.get("label_events").and_then(Value::as_object) {
        let mut is = issues(snap);
        for (n, list) in m {
            let mut ev = vec![ready_event("alice", Some("2026-10-01T00:00:00Z"))];
            for (k, l) in list.as_array().unwrap().iter().enumerate() {
                let l = l.as_str().unwrap();
                let (kind, l) = match l.strip_prefix('-') {
                    Some(r) => ("unlabeled", r),
                    None => ("labeled", l),
                };
                let (label, who) = l.split_once('=').unwrap();
                let actor = if who.is_empty() { Value::Null } else { json!({"login": who}) };
                ev.push(json!({"event": kind, "label": {"name": label}, "actor": actor,
                               "created_at": format!("2026-10-01T00:0{}:00Z", k + 1)}));
            }
            let mut i = is.get(n).cloned().unwrap_or_else(|| json!({}));
            i["events"] = json!(ev);
            is.insert(n.clone(), i);
        }
        snap["issues"] = Value::Object(is);
    }
    // urgent_by: tehas:ready by alice, then tehas:urgent by the login; an
    // empty login names nobody.
    if let Some(m) = c.get("urgent_by").and_then(Value::as_object) {
        let mut is = issues(snap);
        for (n, who) in m {
            let who = who.as_str().unwrap();
            let actor = if who.is_empty() { Value::Null } else { json!({"login": who}) };
            let mut i = is.get(n).cloned().unwrap_or_else(|| json!({}));
            i["events"] = json!([ready_event("alice", None),
                {"event": "labeled", "label": {"name": "tehas:urgent"}, "actor": actor}]);
            is.insert(n.clone(), i);
        }
        snap["issues"] = Value::Object(is);
    }
    // hold: cycles that hold their lock.
    if let Some(list) = c.get("hold").and_then(Value::as_array) {
        let mut running = snap["running"].as_array().cloned().unwrap_or_default();
        running.extend(list.iter().map(|n| json!({"issue": n, "how": "label"})));
        snap["running"] = json!(running);
    }
    // other: a container in the slice that is no cycle's, using the bytes.
    if let Some(b) = c.get("other").and_then(Value::as_u64) {
        snap["slice"]["scopes"][format!("docker-{}.scope", id("d"))] = scope(b, 0, 2 * GIB);
    }
    // cid: a running cycle's container: [issue, id character, bytes used].
    if let Some(list) = c.get("cid").and_then(Value::as_array) {
        for x in list {
            let (ch, b) = (x[1].as_str().unwrap(), x[2].as_u64().unwrap());
            snap["slice"]["scopes"][format!("docker-{}.scope", id(ch))] = scope(b, 0, 2 * GIB);
            let mut cids = snap["slice"]["cids"].as_array().cloned().unwrap_or_default();
            cids.push(json!(id(ch)));
            snap["slice"]["cids"] = json!(cids);
        }
    }
    // children: containers in the slice, [id character, anon, slab, low]
    // as slice_child writes them.
    if let Some(list) = c.get("children").and_then(Value::as_array) {
        for x in list {
            let (ch, anon, slab, low) = (x[0].as_str().unwrap(), x[1].as_u64().unwrap(), x[2].as_u64().unwrap(), x[3].as_u64().unwrap());
            snap["slice"]["scopes"][format!("docker-{}.scope", id(ch))] = scope(anon, slab, low);
        }
    }
    // slice_state: full avg60 and some avg60 of the slice's pressure.
    if let Some(p) = c.get("slice_state").and_then(Value::as_array) {
        snap["slice"]["pressure"] = json!(pressure(p[0].as_str().unwrap(), p.get(1).and_then(Value::as_str).unwrap_or("0.00")));
    }
    // max: the slice's MemoryMax in bytes.
    if let Some(m) = c.get("max").and_then(Value::as_u64) {
        snap["slice"]["show"]["MemoryMax"] = json!(m.to_string());
    }
    // no_slice: the root step was never run.
    if c.get("no_slice") == Some(&json!(true)) {
        snap["slice"]["show"] = json!({"FragmentPath": "", "ActiveState": "inactive", "MemoryMax": "infinity"});
    }
    // The defaults of every issue the queue names: labelled by alice, an open
    // issue, nothing edited, no pull request.
    let numbers: Vec<u64> = ["ready", "blocked"]
        .iter()
        .flat_map(|k| snap[*k].as_array().cloned().unwrap_or_default())
        .filter_map(|i| i["number"].as_u64().or_else(|| i["number"].as_str()?.parse().ok()))
        .collect();
    let mut is = issues(snap);
    for n in numbers {
        let i = is.entry(n.to_string()).or_insert_with(|| json!({}));
        if i.get("events").is_none() {
            i["events"] = json!([ready_event("alice", None)]);
        }
    }
    for (n, i) in is.iter_mut() {
        let defaults = json!({
            "edit": {"lastEditedAt": null, "editor": null},
            "work": {"number": n.parse::<u64>().unwrap(), "state": "open"},
            "prs": [],
            "comments": {"failed": true},
        });
        for (k, v) in defaults.as_object().unwrap() {
            if i.get(k).is_none() {
                i[k] = v.clone();
            }
        }
    }
    snap["issues"] = Value::Object(is);
}

/// `want` is a part of `got`: every key of an object, every item of a list
/// in order.
fn within(want: &Value, got: &Value) -> bool {
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => {
            w.iter().all(|(k, v)| g.get(k).is_some_and(|x| within(v, x)))
        }
        _ => want == got,
    }
}

fn run_suite(name: &str) {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let suite: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap())
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut failures = Vec::new();
    for c in suite["cases"].as_array().unwrap() {
        let name = c["case"].as_str().unwrap();
        let (mut settings, mut snap) = base();
        if let Some(s) = c.get("settings") {
            merge(&mut settings, s);
        }
        if let Some(s) = c.get("snapshot") {
            merge(&mut snap, s);
        }
        expand(c, &mut snap);
        if let Some(s) = c.get("after") {
            merge(&mut snap, s);
        }
        let dry = c.get("dry") == Some(&json!(true));
        let d = decide(&Snapshot::parse(&snap), &Settings::parse(&settings), dry);
        let e = &c["expect"];
        let mut bad = Vec::new();
        let log = d.log.join("\n");
        if let Some(x) = e.get("exit") {
            if x != &json!(d.exit) {
                bad.push(format!("exit {} not {x}", d.exit));
            }
        }
        if let Some(x) = e.get("picked") {
            if x != &json!(d.picked) {
                bad.push(format!("picked {:?} not {x}", d.picked));
            }
        }
        for x in e.get("log").and_then(Value::as_array).into_iter().flatten() {
            if !log.contains(x.as_str().unwrap()) {
                bad.push(format!("the log lacks {x}"));
            }
        }
        for x in e.get("log_lacks").and_then(Value::as_array).into_iter().flatten() {
            if log.contains(x.as_str().unwrap()) {
                bad.push(format!("the log has {x}"));
            }
        }
        if let Some(x) = e.get("log_count") {
            for (line, n) in x.as_object().unwrap() {
                let got = d.log.iter().filter(|l| l.contains(line.as_str())).count();
                if json!(got) != *n {
                    bad.push(format!("{line:?} logged {got} times, not {n}"));
                }
            }
        }
        for x in e.get("actions").and_then(Value::as_array).into_iter().flatten() {
            if !d.actions.iter().any(|a| within(x, a)) {
                bad.push(format!("no action {x}"));
            }
        }
        for x in e.get("no_actions").and_then(Value::as_array).into_iter().flatten() {
            if d.actions.iter().any(|a| within(x, a)) {
                bad.push(format!("an action {x}"));
            }
        }
        if e.get("nothing") == Some(&json!(true)) && !d.actions.is_empty() {
            bad.push(format!("actions {:?}", d.actions));
        }
        if let Some(x) = e.get("report") {
            let ok = match (x, &d.report) {
                (Value::Null, None) => true,
                (x, Some(r)) => within(x, r),
                _ => false,
            };
            if !ok {
                bad.push(format!("report {:?} not {x}", d.report));
            }
        }
        if let Some(x) = e.get("demand") {
            let ok = match (x, &d.demand) {
                (Value::Null, Some(Value::Null)) => true,
                (x, Some(r)) if !x.is_null() => within(x, r),
                _ => false,
            };
            if !ok {
                bad.push(format!("demand {:?} not {x}", d.demand));
            }
        }
        if !d.needs.is_empty() {
            bad.push(format!("needs {:?}", d.needs));
        }
        if !bad.is_empty() {
            failures.push(format!("{name}:\n  {}\n  log: {:?}\n  actions: {:?}", bad.join("\n  "), d.log, d.actions));
        }
    }
    assert!(failures.is_empty(), "{name}: {} failed\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn slots() {
    run_suite("slots");
}
