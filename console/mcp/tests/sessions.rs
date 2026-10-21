// The session tools over a snapshot written by the tests.

use mcp::sessions::{find, links};
use mcp::snapshot::Snapshot;
use mcp::tools::{list, Dirs, Output, ToolError};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn iso(t: i64) -> String {
    let (days, s) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

// A live dir and a sessions dir, removed when dropped. The logs dir is never
// made: no tool here may open it.
struct Env {
    root: PathBuf,
    dirs: Dirs,
}

impl Env {
    fn new(name: &str, repo: Option<&str>, sessions: &[Value]) -> Env {
        let root = std::env::temp_dir().join(format!("mcp-sess-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (live, snap) = (root.join("live"), root.join("mcp"));
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&snap).unwrap();
        if let Some(r) = repo {
            std::fs::write(live.join("status.json"), json!({"repo": r}).to_string()).unwrap();
        }
        let mut text =
            json!({"api": 1, "days": 30, "generatedAt": iso(now()), "sessions": 1}).to_string();
        for s in sessions {
            text.push('\n');
            text.push_str(&s.to_string());
        }
        std::fs::write(snap.join("sessions.jsonl"), text + "\n").unwrap();
        let dirs = Dirs {
            live,
            sessions: snap,
            logs: root.join("no-logs"),
        };
        Env { root, dirs }
    }

    fn call(&self, name: &str, args: Value) -> Result<Value, ToolError> {
        match mcp::tools::call(&self.dirs, name, &args)? {
            Output::Json(v) => Ok(v),
            other => panic!("{name} gave {other:?}"),
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// A session started `ago` seconds ago. Fields in `extra` replace the defaults.
fn sess(kind: &str, issue: i64, ago: i64, extra: Value) -> Value {
    let t = now() - ago;
    let mut s = json!({
        "id": format!("{kind}-{issue}-{t}"), "kind": kind, "issue": issue, "attempt": 1,
        "started": iso(t), "ended": iso(t + 60), "seconds": 60, "outcome": "merged",
        "tehasVersion": "v0.7.31", "rukasVersion": "v0.14.0",
        "steps": [], "roles": [], "logs": [],
    });
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

fn ids(out: &Value) -> Vec<String> {
    out["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn filters_combine() {
    let all = [
        sess(
            "work",
            1,
            100,
            json!({"outcome": "failed", "rukasVersion": "v0.14.0"}),
        ),
        sess(
            "work",
            2,
            200,
            json!({"outcome": "failed", "rukasVersion": "v0.13.0"}),
        ),
        sess(
            "work",
            3,
            300,
            json!({"outcome": "merged", "rukasVersion": "v0.14.0"}),
        ),
        sess(
            "work",
            4,
            400,
            json!({"outcome": "failed", "rukasCommit": "v0.14.0"}),
        ),
        sess(
            "plan",
            5,
            500,
            json!({"outcome": "failed", "how": "refine", "rukasVersion": "v0.1.0"}),
        ),
    ];
    let env = Env::new("filters", Some("a/b"), &all);
    let out = env
        .call(
            "factory_sessions",
            json!({"outcome": "failed", "rukasVersion": "v0.14.0"}),
        )
        .unwrap();
    assert_eq!(
        ids(&out),
        [
            all[0]["id"].as_str().unwrap(),
            all[3]["id"].as_str().unwrap()
        ]
    );
    let refine = env
        .call("factory_sessions", json!({"how": "refine"}))
        .unwrap();
    assert_eq!(ids(&refine), [all[4]["id"].as_str().unwrap()]);
    let one = env
        .call("factory_sessions", json!({"issue": 2, "kind": "work"}))
        .unwrap();
    assert_eq!(ids(&one), [all[1]["id"].as_str().unwrap()]);
    let kind = env
        .call("factory_sessions", json!({"failureKind": "x"}))
        .unwrap();
    assert!(ids(&kind).is_empty());
}

#[test]
fn bad_filters_are_tool_errors() {
    let env = Env::new("badfilters", Some("a/b"), &[]);
    for a in [
        json!({"outcome": "nope"}),
        json!({"kind": "x"}),
        json!({"how": "x"}),
        json!({"failureKind": "a b"}),
        json!({"tehasVersion": ""}),
        json!({"rukasVersion": "x".repeat(65)}),
        json!({"issue": 0}),
        json!({"limit": 0}),
        json!({"limit": 201}),
        json!({"before": "../x"}),
    ] {
        assert!(
            matches!(
                env.call("factory_sessions", a.clone()),
                Err(ToolError::BadArgs(_))
            ),
            "{a}"
        );
    }
}

#[test]
fn paging() {
    let all: Vec<Value> = (1..=5)
        .map(|i| sess("work", i, i * 100, json!({})))
        .collect();
    let env = Env::new("paging", Some("a/b"), &all);
    let want: Vec<String> = all
        .iter()
        .map(|s| s["id"].as_str().unwrap().into())
        .collect();
    let mut got: Vec<String> = Vec::new();
    let mut args = json!({"limit": 2});
    let mut pages = 0;
    loop {
        let out = env.call("factory_sessions", args.clone()).unwrap();
        got.extend(ids(&out));
        pages += 1;
        match out.get("next") {
            Some(n) => {
                assert_eq!(n.as_str(), got.last().map(String::as_str));
                args = json!({"limit": 2, "before": n});
            }
            None => break,
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(got, want);
    let exact = env.call("factory_sessions", json!({"limit": 5})).unwrap();
    assert!(exact.get("next").is_none());
    let missing = format!("work-9-{}", now() - 5);
    assert!(matches!(
        env.call("factory_sessions", json!({"before": missing})),
        Err(ToolError::BadArgs(_))
    ));
}

#[test]
fn a_missing_before_names_the_snapshot_days() {
    let env = Env::new("before-days", Some("a/b"), &[sess("work", 1, 60, json!({}))]);
    let path = env.dirs.sessions.join("sessions.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replacen("\"days\":30", "\"days\":7", 1)).unwrap();
    let missing = format!("work-9-{}", now() - 5);
    match env.call("factory_sessions", json!({"before": missing})) {
        Err(ToolError::BadArgs(why)) => {
            assert_eq!(why, "before is not a session of the last 7 days")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn window() {
    let old = sess("work", 1, 31 * 86400, json!({}));
    let env = Env::new(
        "window",
        Some("a/b"),
        &[sess("work", 2, 60, json!({})), old.clone()],
    );
    let out = env.call("factory_sessions", json!({})).unwrap();
    assert_eq!(ids(&out).len(), 1);
    assert!(matches!(
        env.call("factory_session", json!({"id": old["id"]})),
        Err(ToolError::BadArgs(_))
    ));
}

#[test]
fn bad_ids() {
    // Neither the snapshot nor the logs dir exists.
    let dirs = Dirs {
        live: "/nonexistent/live".into(),
        sessions: "/nonexistent/mcp".into(),
        logs: "/nonexistent/logs".into(),
    };
    for id in ["work-1-1/../x", "work-01-123456789", "plan-1-12", ""] {
        match mcp::tools::call(&dirs, "factory_session", &json!({"id": id})) {
            Err(ToolError::BadArgs(_)) => {}
            other => panic!("{id:?}: {other:?}"),
        }
    }
    let env = Env::new("badids", Some("a/b"), &[]);
    let snap = Snapshot::open(&env.dirs.sessions, now()).unwrap();
    assert!(matches!(find(&snap, "../x"), Err(ToolError::BadArgs(_))));
}

#[test]
fn links_and_odd_repos() {
    assert_eq!(
        links(Some("tehas-app/tehas"), 12, Some(34)),
        (
            Some("https://github.com/tehas-app/tehas/issues/12".into()),
            Some("https://github.com/tehas-app/tehas/pull/34".into())
        )
    );
    assert_eq!(links(Some("tehas-app/tehas"), 12, None).1, None);
    for r in ["a/b/c", "../x", "a b/c", "a/..", "a/b..c", "", "/b"] {
        assert_eq!(links(Some(r), 1, Some(2)), (None, None), "{r:?}");
    }
    assert_eq!(links(None, 1, Some(2)), (None, None));

    let s = sess("work", 7, 60, json!({"pr": 8}));
    let bare = sess("work", 9, 120, json!({}));
    for (name, repo, linked) in [
        ("good", Some("tehas-app/tehas"), true),
        ("odd", Some("a/b/c"), false),
        ("none", None, false),
    ] {
        let env = Env::new(name, repo, &[s.clone(), bare.clone()]);
        let out = env.call("factory_sessions", json!({})).unwrap();
        let (a, b) = (&out["sessions"][0], &out["sessions"][1]);
        assert_eq!(a.get("issueUrl").is_some(), linked, "{name}");
        assert_eq!(a.get("prUrl").is_some(), linked, "{name}");
        assert_eq!(b.get("issueUrl").is_some(), linked, "{name}");
        assert!(b.get("prUrl").is_none(), "{name}");
        let one = env.call("factory_session", json!({"id": s["id"]})).unwrap();
        assert_eq!(one["session"].get("prUrl").is_some(), linked, "{name}");
    }
}

fn paths(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                out.insert(p.clone());
                paths(x, &p, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                paths(x, &format!("{prefix}[]"), out);
            }
        }
        _ => {}
    }
}

fn key_paths(v: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    paths(v, "", &mut out);
    out
}

// The paths an outputSchema names. An object with additionalProperties is a
// map: its keys come out as `*`.
fn schema_paths(s: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    if let Some(props) = s["properties"].as_object() {
        for (k, v) in props {
            let p = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            out.insert(p.clone());
            schema_paths(v, &p, out);
        }
    }
    if s["additionalProperties"].is_object() {
        out.insert(format!("{prefix}.*"));
    }
    if s["type"] == "array" {
        schema_paths(&s["items"], &format!("{prefix}[]"), out);
    }
}

// Output paths with the keys under a map replaced by `*`.
fn normal(paths: BTreeSet<String>) -> BTreeSet<String> {
    paths
        .into_iter()
        .map(|p| {
            let mut out = String::new();
            let mut map = false;
            for (i, seg) in p.split('.').enumerate() {
                if i > 0 {
                    out.push('.');
                }
                out.push_str(if map { "*" } else { seg });
                map = seg.ends_with("outcomes") || seg.ends_with("failureKinds");
            }
            out
        })
        .collect()
}

fn full_session(kind: &str, issue: i64, ago: i64) -> Value {
    let mut extra = Map::new();
    for (k, v) in [
        ("how", json!("refine")),
        ("queueWait", json!(3)),
        ("exitCode", json!(1)),
        ("status", json!("failed")),
        ("lastStep", json!("review")),
        ("failedStep", json!("review")),
        ("cap", json!("time")),
        ("parkReason", json!("loop")),
        ("reviewHeld", json!(true)),
        ("failureKind", json!("model")),
        ("failureCause", json!("the model said no")),
        ("providerOutage", json!(false)),
        ("endpoint", json!("e")),
        ("pr", json!(8)),
        ("tehasCommit", json!("abc")),
        ("rukasCommit", json!("def")),
        ("image", json!("i")),
        ("resultSchema", json!(4)),
        ("input", json!(1)),
        ("output", json!(2)),
        ("cacheRead", json!(3)),
        ("cacheWrite", json!(4)),
        ("turns", json!(5)),
        ("dispatches", json!(6)),
        ("cost", json!(0.5)),
        ("runnerMs", json!(7)),
        ("oomKills", json!(0)),
        ("forksRefused", json!(1)),
        ("concurrent", json!(2)),
        ("outcome", json!("failed")),
        (
            "steps",
            json!([{"step": "s", "round": 1, "started": "2026-10-20T00:00:00Z",
                          "ended": "2026-10-20T00:01:00Z", "seconds": 60}]),
        ),
        (
            "roles",
            json!([{"role": "r", "model": "m", "provider": "p", "dispatches": 1,
                          "turns": 2, "input": 3, "output": 4, "cacheRead": 5,
                          "cacheWrite": 6, "cost": 0.1, "ms": 7}]),
        ),
        (
            "logs",
            json!([{"word": "developer", "name": "cycle-1.log", "bytes": 99}]),
        ),
        (
            "filed",
            json!([{"issue": 20, "url": "https://github.com/a/b/issues/20"}]),
        ),
        ("secret", json!("SECRET")),
    ] {
        extra.insert(k.into(), v);
    }
    sess(kind, issue, ago, Value::Object(extra))
}

#[test]
fn allowlist() {
    let s = full_session("plan", 3, 60);
    let env = Env::new("allow", Some("a/b"), std::slice::from_ref(&s));
    let one = env.call("factory_session", json!({"id": s["id"]})).unwrap();
    let all = env.call("factory_sessions", json!({})).unwrap();
    for out in [&one, &all] {
        let text = out.to_string();
        assert!(
            !text.contains("SECRET") && !text.contains("cycle-1.log"),
            "{text}"
        );
        for p in key_paths(out) {
            assert!(!p.ends_with(".secret") && !p.ends_with(".name"), "{p}");
        }
    }
    assert_eq!(
        one["session"]["logs"],
        json!([{"file": "developer", "bytes": 99}])
    );
    assert_eq!(one["session"]["filed"][0]["issue"], 20);
}

fn schema_of(name: &str) -> Value {
    list()
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap()["outputSchema"]
        .clone()
}

#[test]
fn schema_matches_output() {
    let a = full_session("plan", 3, 60);
    let b = full_session("work", 4, 120);
    let env = Env::new("schema", Some("a/b"), &[a.clone(), b.clone()]);
    let cases = [
        ("factory_sessions", json!({"limit": 1})),
        ("factory_session", json!({"id": a["id"]})),
        ("factory_failures", json!({})),
    ];
    for (name, args) in cases {
        let out = env.call(name, args).unwrap();
        let mut want = BTreeSet::new();
        schema_paths(&schema_of(name), "", &mut want);
        assert_eq!(normal(key_paths(&out)), want, "{name}");
    }
}

#[test]
fn inputs_described() {
    for name in ["factory_sessions", "factory_session", "factory_failures"] {
        let tool = list()
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()
            .clone();
        for (k, v) in tool["inputSchema"]["properties"].as_object().unwrap() {
            assert!(
                !v["description"].as_str().unwrap_or("").is_empty(),
                "{name}.{k}"
            );
        }
        fn described(s: &Value, at: &str) {
            if let Some(p) = s["properties"].as_object() {
                for (k, v) in p {
                    assert!(
                        !v["description"].as_str().unwrap_or("").is_empty()
                            || v["type"] == "object",
                        "{at}.{k}"
                    );
                    described(v, &format!("{at}.{k}"));
                }
            }
            if s["type"] == "array" {
                described(&s["items"], &format!("{at}[]"));
            }
        }
        described(&tool["outputSchema"], name);
    }
}

#[test]
fn no_snapshot_is_a_tool_error() {
    let env = Env::new("nosnap", Some("a/b"), &[]);
    std::fs::remove_file(env.dirs.sessions.join("sessions.jsonl")).unwrap();
    for (name, args) in [
        ("factory_sessions", json!({})),
        ("factory_session", json!({"id": "work-1-1234567890"})),
        ("factory_failures", json!({})),
    ] {
        match env.call(name, args) {
            Err(ToolError::File { file, .. }) => assert_eq!(file, "sessions.jsonl"),
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn failures_per_version_pair() {
    let s = |kind, issue, ago, extra| sess(kind, issue, ago, extra);
    let rows = [
        s(
            "work",
            1,
            10,
            json!({"outcome": "failed", "failureKind": "model", "failureCause": "c1"}),
        ),
        s("work", 2, 20, json!({"outcome": "merged"})),
        s(
            "work",
            3,
            30,
            json!({"outcome": "failed", "failureKind": "model", "failureCause": "c1"}),
        ),
        s(
            "work",
            4,
            40,
            json!({"outcome": "failed", "failureKind": "git", "failureCause": "c2",
                              "tehasVersion": "v0.7.30", "rukasVersion": null, "rukasCommit": "abc"}),
        ),
        s(
            "work",
            5,
            50,
            json!({"outcome": "pr", "tehasVersion": "v0.7.30", "rukasVersion": null, "rukasCommit": "abc"}),
        ),
        s(
            "plan",
            6,
            60,
            json!({"outcome": "failed", "failureKind": "model", "failureCause": "c1"}),
        ),
    ];
    let env = Env::new("pairs", Some("tehas-app/tehas"), &rows);
    let out = env.call("factory_failures", json!({})).unwrap();
    assert_eq!(out["days"], 30);
    assert_eq!(out["stale"], false);
    let v = out["versions"].as_array().unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0]["tehasVersion"], "v0.7.31");
    assert_eq!(v[0]["rukasVersion"], "v0.14.0");
    assert_eq!(v[0]["runs"], 3);
    assert_eq!(v[0]["outcomes"], json!({"failed": 2, "merged": 1}));
    assert_eq!(v[0]["failureKinds"], json!({"model": 2}));
    assert_eq!(v[0]["causes"][0]["cause"], "c1");
    assert_eq!(v[0]["causes"][0]["kind"], "model");
    assert_eq!(v[0]["causes"][0]["n"], 2);
    assert_eq!(
        v[0]["causes"][0]["issues"],
        json!([
            "https://github.com/tehas-app/tehas/issues/1",
            "https://github.com/tehas-app/tehas/issues/3"
        ])
    );
    assert_eq!(v[1]["tehasVersion"], "v0.7.30");
    assert_eq!(v[1]["rukasVersion"], "abc");
    assert_eq!(v[1]["runs"], 2);
    assert_eq!(v[1]["outcomes"], json!({"failed": 1, "pr": 1}));
}

#[test]
fn top_causes_bounded() {
    let mut rows = Vec::new();
    // Seven causes; c0 failed twelve issues, c1..c6 one each.
    for i in 0..12 {
        rows.push(sess(
            "work",
            100 + i,
            10 + i,
            json!({"outcome": "failed", "failureCause": "c0", "failureKind": "k"}),
        ));
    }
    // The same issue again does not add a second link.
    rows.push(sess(
        "work",
        100,
        500,
        json!({"outcome": "failed", "failureCause": "c0", "failureKind": "k"}),
    ));
    for i in 1..7 {
        rows.push(sess(
            "work",
            200 + i,
            600 + i,
            json!({"outcome": "failed", "failureCause": format!("c{i}")}),
        ));
    }
    let env = Env::new("causes", Some("a/b"), &rows);
    let out = env.call("factory_failures", json!({})).unwrap();
    let causes = out["versions"][0]["causes"].as_array().unwrap();
    assert_eq!(causes.len(), 5);
    assert_eq!(causes[0]["cause"], "c0");
    assert_eq!(causes[0]["n"], 13);
    let issues = causes[0]["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 10);
    assert_eq!(issues[0], "https://github.com/a/b/issues/100");
    let unique: BTreeSet<&str> = issues.iter().map(|i| i.as_str().unwrap()).collect();
    assert_eq!(unique.len(), 10);
}

#[test]
fn failures_without_repo() {
    let rows = [
        sess(
            "work",
            1,
            10,
            json!({"outcome": "failed", "failureCause": "c", "failureKind": "k"}),
        ),
        sess("work", 2, 20, json!({"outcome": "merged"})),
    ];
    for repo in [None, Some("a/b/c")] {
        let env = Env::new("norepo", repo, &rows);
        let out = env.call("factory_failures", json!({})).unwrap();
        let v = &out["versions"][0];
        assert_eq!(v["runs"], 2);
        assert_eq!(v["causes"][0]["issues"], json!([]));
        assert_eq!(v["causes"][0]["n"], 1);
    }
}
