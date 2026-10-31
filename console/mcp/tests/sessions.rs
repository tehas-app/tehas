// The session tools over a snapshot written by the tests.

use mcp::sessions::{find, links};
use mcp::snapshot::Snapshot;
use mcp::tools::{list_scoped, Dirs, Output, Scope, ToolError};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;
use tehas_core::time::{iso, now};

// A live dir and a sessions dir, removed when dropped. The logs dir is never
// made: no tool here may open it.
struct Env {
    root: PathBuf,
    dirs: Dirs,
}

impl Env {
    fn new(name: &str, repo: Option<&str>, sessions: &[Value]) -> Env {
        Self::built(name, repo, sessions, None, None)
    }

    fn built(
        name: &str,
        repo: Option<&str>,
        sessions: &[Value],
        stages: Option<&[(&str, Value)]>,
        running: Option<&[Value]>,
    ) -> Env {
        let root = std::env::temp_dir().join(format!("mcp-sess-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (live, snap) = (root.join("live"), root.join("mcp"));
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&snap).unwrap();
        if let Some(r) = repo {
            std::fs::write(live.join("status.json"), json!({"repo": r}).to_string()).unwrap();
        }
        // A factory that lists issues and runs a cycle: `factory_issue` reads
        // these two files too, and a test that does not care has neither.
        if let Some(rows) = stages {
            std::fs::write(live.join("status.json"), stages_text(rows, repo)).unwrap();
        }
        if let Some(cycles) = running {
            std::fs::write(
                live.join("running.json"),
                json!({"cycles": cycles}).to_string(),
            )
            .unwrap();
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
            sessions: snap.clone(),
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

// The dashboard's stage lists, with the keys a cycle's row can carry. A row
// always has its number, title and link, and the planted keys a stage row of
// the host's own file has and this tool must never copy.
fn stages_text(rows: &[(&str, Value)], repo: Option<&str>) -> String {
    let mut m = Map::new();
    if let Some(r) = repo {
        m.insert("repo".into(), json!(r));
    }
    m.insert("generatedAt".into(), json!(iso(now())));
    m.insert("paused".into(), json!(false));
    m.insert("planning".into(), json!(0));
    // Keys the tool is not given: an answer that holds one of them is caught
    // by the tests that read the whole answer as text.
    m.insert(
        "buzzwords".into(),
        json!([{"word": "SECRET-WORD", "count": 3}]),
    );
    m.insert("secretKey".into(), json!("SECRET"));
    for (stage, rows_of_stage) in rows {
        let rows = rows_of_stage.as_array().expect("a list of rows");
        let rows: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "number": r["number"], "title": r["title"],
                    "url": r["url"], "updatedAt": "2026-10-20T08:00:00Z",
                    "closedAt": r["closedAt"], "labels": ["SECRET-LABEL"],
                    "body": "SECRET-BODY",
                })
            })
            .collect();
        m.insert(stage.to_string(), Value::Array(rows));
    }
    Value::Object(m).to_string()
}

fn sess_at(kind: &str, issue: i64, started: i64, extra: Value) -> Value {
    let mut s = json!({
        "id": format!("{kind}-{issue}-{started}"), "kind": kind, "issue": issue, "attempt": 1,
        "started": iso(started), "ended": iso(started + 60), "seconds": 60,
        "outcome": "merged",
        "tehasVersion": "v0.7.31", "rukasVersion": "v0.14.0",
        "steps": [], "roles": [], "logs": [],
    });
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

// A session started `ago` seconds ago. Fields in `extra` replace the defaults.
fn sess(kind: &str, issue: i64, ago: i64, extra: Value) -> Value {
    sess_at(kind, issue, now() - ago, extra)
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
fn a_how_filter_of_either_millwright_word_matches_both() {
    let all = [
        sess("work", 1, 100, json!({"how": "millwright"})),
        sess("work", 2, 200, json!({"how": "fixer"})),
        sess("work", 3, 300, json!({"how": "label"})),
        // A run of a word outside the class is not swept into it by a filter
        // of the class: `hand` is a cycle run by hand, `refresh` a restart on
        // a person's request.
        sess("work", 7, 700, json!({"how": "hand"})),
        sess("work", 8, 800, json!({"how": "refresh"})),
    ];
    let env = Env::new("how", Some("a/b"), &all);
    let both = [
        all[0]["id"].as_str().unwrap(),
        all[1]["id"].as_str().unwrap(),
    ];
    for word in ["millwright", "fixer"] {
        let out = env.call("factory_sessions", json!({"how": word})).unwrap();
        assert_eq!(ids(&out), both, "{word}");
        // The words returned are the ones the records say, not the filter's:
        // both of the pair, whatever order they come back in.
        let returned: BTreeSet<&str> = out["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["how"].as_str().unwrap())
            .collect();
        assert_eq!(
            returned,
            ["millwright", "fixer"].into_iter().collect(),
            "{word}"
        );
    }
    let label = env
        .call("factory_sessions", json!({"how": "label"}))
        .unwrap();
    assert_eq!(ids(&label), [all[2]["id"].as_str().unwrap()]);
    // A word outside the class is not swept into it.
    for w in ["hand", "refresh"] {
        let out = env.call("factory_sessions", json!({"how": w})).unwrap();
        assert_eq!(ids(&out).len(), 1, "{w}");
    }
}

#[test]
fn the_how_schema_accepts_every_word_of_the_filter() {
    // The accepted words come from the tool's own schema, so a word dropped
    // from the filter's list shows up here instead of failing at runtime.
    let tool = list_scoped(Scope::Full)
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "factory_sessions")
        .unwrap()
        .clone();
    let accepted: BTreeSet<&str> = tool["inputSchema"]["properties"]["how"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert_eq!(
        accepted,
        mcp::sessions::HOWS.iter().copied().collect::<BTreeSet<_>>()
    );
    // The class words must be accepted, or the equivalence is dead code. The
    // pair is the one in tehas_core, which the store's rows and analytics'
    // marks read too, so this also pins those crates to the tool's words.
    assert_eq!(
        mcp::sessions::MILLWRIGHT_WORDS,
        tehas_core::grammar::MILLWRIGHT_WORDS
    );
    assert!(mcp::sessions::MILLWRIGHT_WORDS
        .iter()
        .all(|w| accepted.contains(w)));
}

#[test]
fn every_how_word_outside_the_pair_is_matched_exactly() {
    // Each word of the filter's list outside the millwright pair finds its own
    // records and no others; the pair's equivalence is the test above.
    let all: Vec<Value> = mcp::sessions::HOWS
        .iter()
        .filter(|w| !mcp::sessions::MILLWRIGHT_WORDS.contains(w))
        .map(|w| sess("work", 1, 100, json!({"how": w})))
        .collect();
    let env = Env::new("how-words", Some("a/b"), &all);
    for s in &all {
        let want = s["how"].as_str().unwrap();
        let out = env.call("factory_sessions", json!({"how": want})).unwrap();
        assert_eq!(ids(&out), [s["id"].as_str().unwrap()], "{want}");
    }
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
    let env = Env::new(
        "before-days",
        Some("a/b"),
        &[sess("work", 1, 60, json!({}))],
    );
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
        ("imageBuilt", json!("2026-10-19T08:00:00Z")),
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
    list_scoped(Scope::Full)
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
        ("factory_failures", json!({"by": "image"})),
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
        let tool = list_scoped(Scope::Full)
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

// factory_issue: one issue's story, joined here from the live files and the
// snapshot. The rows of a stage are built with the keys the host's own file
// gives them, so a leak of a label or an issue body is caught here.

fn row(number: i64, closed: bool) -> Value {
    json!({
        "number": number, "title": format!("Issue {number}"),
        "url": format!("https://github.com/a/b/issues/{number}"),
        "closedAt": if closed { json!("2026-10-19T18:00:00Z") } else { json!(null) },
    })
}

fn cycle(issue: i64, step: &str, status: &str) -> Value {
    json!({
        "issue": issue, "running": true, "status": status, "step": step,
        "statusText": "SECRET-TEXT", "startedAt": 1792483801, "updatedAt": 1792486058,
        "console": "console-7.txt", "secretKey": "SECRET",
    })
}

// An issue of several attempts: the runs come oldest first, the counts name
// the oldest and the newest, and nothing of the row or the cycle leaks out.
#[test]
fn an_issue_with_several_attempts() {
    // A snapshot holds its sessions newest first, so the host would write
    // these three in this order. Attempts rise with them, and the id carries
    // the second it started: the answer must be the other way round.
    let t = now();
    let rows = [
        sess_at(
            "plan",
            7,
            t - 100,
            json!({"attempt": 3, "how": "refine", "outcome": "handoff", "parkReason": "loop"}),
        ),
        sess_at(
            "work",
            7,
            t - 200,
            json!({"attempt": 2, "how": "retry-tick", "outcome": "pr", "pr": 21}),
        ),
        sess_at(
            "work",
            7,
            t - 300,
            json!({"attempt": 1, "how": "label", "outcome": "failed",
                   "failedStep": "develop", "failureKind": "model",
                   "failureCause": "the cause of a failure", "seconds": 300}),
        ),
        // Another issue's run, and one outside the window: neither is here.
        sess("work", 8, 150, json!({})),
        sess("work", 7, 31 * 86400, json!({"attempt": 4})),
    ];
    let env = Env::built(
        "story",
        Some("a/b"),
        &rows,
        Some(&[("awaiting", json!([row(7, false)]))]),
        None,
    );
    let out = env.call("factory_issue", json!({"issue": 7})).unwrap();
    assert_eq!(out["api"], 1);
    assert_eq!(out["issue"], 7);
    assert_eq!(
        out["issueUrl"], "https://github.com/a/b/issues/7",
        "{:?}",
        out["issueUrl"]
    );
    assert_eq!(out["stage"], "awaiting");
    assert_eq!(out["title"], "Issue 7");
    assert!(out.get("running").is_none());
    let attempts = out["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 3);
    // Oldest first, so the attempts read 1, 2, 3.
    assert_eq!(
        attempts
            .iter()
            .map(|a| a["attempt"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(attempts[0]["id"], rows[2]["id"]);
    assert_eq!(attempts[0]["kind"], "work");
    // The seconds the run took are its own, not the gap to the next one.
    assert_eq!(attempts[0]["seconds"], 300);
    assert_eq!(attempts[0]["kind"], "work");
    assert_eq!(attempts[0]["how"], "label");
    assert_eq!(attempts[0]["outcome"], "failed");
    assert_eq!(attempts[0]["seconds"], 300);
    assert_eq!(attempts[0]["failedStep"], "develop");
    assert_eq!(attempts[0]["failureKind"], "model");
    assert_eq!(attempts[0]["failureCause"], "the cause of a failure");
    assert_eq!(attempts[2]["attempt"], 3);
    assert_eq!(attempts[2]["kind"], "plan");
    assert_eq!(attempts[2]["parkReason"], "loop");
    // The id is served, so one answer reaches factory_session and factory_log.
    assert_eq!(attempts[2]["id"], rows[0]["id"]);
    // The attempt's own fields and no other, whatever else the record holds.
    // The snapshot's lines carry versions, steps, roles and logs among the
    // fields this tool is not given, so the keys of an answer are named here in
    // full. A field without a value is left out, and the three runs differ
    // only in it: the failing first one has the failure's three words, the
    // second has a pull request's link, the third was parked.
    let keys = |a: &Value| {
        a.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
    };
    let named = |extra: &[&str]| -> BTreeSet<String> {
        [
            "id", "kind", "attempt", "how", "started", "ended", "seconds", "outcome",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(String::from)
        .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        keys(&attempts[0]),
        named(&["failedStep", "failureKind", "failureCause"])
    );
    assert_eq!(keys(&attempts[1]), named(&["prUrl"]));
    assert_eq!(keys(&attempts[2]), named(&["parkReason"]));
    assert_eq!(attempts[1]["prUrl"], "https://github.com/a/b/pull/21");
    assert_eq!(out["firstSeen"], attempts[0]["started"]);
    assert_eq!(out["lastSeen"], attempts[2]["started"]);
    assert_eq!(out["outcome"], "handoff");
    let text = out.to_string();
    for bad in [
        "SECRET",
        "labels",
        "body",
        "buzzwords",
        "/home/x",
        "statusText",
        "tehasVersion",
    ] {
        assert!(!text.contains(bad), "{bad}: {text}");
    }
}

// A cycle that owns the issue gives its step and status, and counts as the
// `running` stage when the dashboard's lists do not hold the issue.
#[test]
fn an_issue_with_a_running_cycle() {
    let env = Env::built(
        "running",
        Some("a/b"),
        &[],
        Some(&[("queued", json!([row(9, false)]))]),
        Some(&[cycle(7, "develop", "running")]),
    );
    let out = env.call("factory_issue", json!({"issue": 7})).unwrap();
    assert_eq!(out["stage"], "running");
    assert_eq!(out["running"]["step"], "develop");
    assert_eq!(out["running"]["status"], "running");
    assert!(out["running"].get("statusText").is_none());
    assert!(out["running"].get("console").is_none());
    // No runs yet: the list is there and empty, and it says nothing else.
    assert_eq!(out["attempts"], json!([]));
    assert!(out.get("firstSeen").is_none());
    assert!(out.get("lastSeen").is_none());
    assert!(out.get("outcome").is_none());
    assert!(out.get("title").is_none(), "{out}");

    // The dashboard's own list wins over the running file.
    let both = Env::built(
        "running-stage",
        Some("a/b"),
        &[],
        Some(&[("running", json!([row(7, false)]))]),
        Some(&[cycle(7, "review", "stalled")]),
    );
    let out = both.call("factory_issue", json!({"issue": 7})).unwrap();
    assert_eq!(out["stage"], "running");
    assert_eq!(out["title"], "Issue 7");
    assert_eq!(out["running"]["step"], "review");
    assert_eq!(out["running"]["status"], "stalled");
}

// A run whose issue the dashboard lists nowhere is still an answer.
#[test]
fn an_issue_with_only_runs() {
    let s = sess("work", 42, 60, json!({"outcome": "merged"}));
    let env = Env::built("runs-only", Some("a/b"), &[s], None, None);
    let out = env.call("factory_issue", json!({"issue": 42})).unwrap();
    assert!(out.get("stage").is_none(), "{out}");
    assert!(out.get("title").is_none(), "{out}");
    assert!(out.get("running").is_none(), "{out}");
    assert_eq!(out["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(out["outcome"], "merged");
}

// The numbers of the poll's plans, since status.json gives only a count.
#[test]
fn an_issue_being_planned() {
    let root = std::env::temp_dir().join(format!("mcp-plan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (live, snap) = (root.join("live"), root.join("mcp"));
    std::fs::create_dir_all(&live).unwrap();
    std::fs::create_dir_all(&snap).unwrap();
    std::fs::write(
        live.join("status.json"),
        json!({"repo": "a/b", "planning": 1}).to_string(),
    )
    .unwrap();
    std::fs::write(
        live.join("health.json"),
        json!({"checks": {"plans": [{"issue": 6, "secretKey": "SECRET"}]}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        snap.join("sessions.jsonl"),
        format!(
            "{}\n",
            json!({"api": 1, "days": 30, "generatedAt": iso(now()), "sessions": 0})
        ),
    )
    .unwrap();
    let dirs = Dirs {
        live,
        sessions: snap,
        logs: root.join("no-logs"),
    };
    let out = match mcp::tools::call(&dirs, "factory_issue", &json!({"issue": 6})) {
        Ok(Output::Json(v)) => v,
        other => panic!("not JSON: {other:?}"),
    };
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(out["stage"], "planning");
    assert_eq!(out["attempts"], json!([]));
    assert!(out.get("title").is_none(), "{out}");
    assert!(!out.to_string().contains("SECRET"));
}

#[test]
fn an_unknown_issue_is_a_tool_error_that_names_it() {
    let env = Env::built(
        "unknown",
        Some("a/b"),
        &[sess("work", 7, 60, json!({}))],
        Some(&[("queued", json!([row(9, false)]))]),
        Some(&[cycle(7, "develop", "running")]),
    );
    for issue in [1, 8, 100] {
        match env.call("factory_issue", json!({"issue": issue})) {
            Err(ToolError::BadArgs(why)) => {
                assert!(why.contains(&issue.to_string()), "{issue}: {why}");
                assert!(why.contains("30 days"), "{why}");
            }
            other => panic!("{issue}: {other:?}"),
        }
    }
    // A cycle the host has stopped does not make its issue known to the tool.
    let stopped = Env::built(
        "not-running",
        Some("a/b"),
        &[],
        None,
        Some(&[json!({"issue": 3, "running": false, "status": "merged",
                      "step": "merged", "secretKey": "SECRET"})]),
    );
    assert!(matches!(
        stopped.call("factory_issue", json!({"issue": 3})),
        Err(ToolError::BadArgs(_))
    ));
}

#[test]
fn the_issue_argument_is_checked() {
    let env = Env::new("badissue", Some("a/b"), &[]);
    for a in [
        json!({}),
        json!({"issue": "7"}),
        json!({"issue": 0}),
        json!({"issue": -1}),
        json!({"issue": 1.5}),
        json!({"issue": 7, "extra": 1}),
        json!({"issue": 7, "stage": "queued"}),
    ] {
        assert!(
            matches!(
                env.call("factory_issue", a.clone()),
                Err(ToolError::BadArgs(_))
            ),
            "{a}"
        );
    }
    // A missing tool is unknown, not a bad argument.
    let dirs = Dirs {
        live: "/nonexistent/live".into(),
        sessions: "/nonexistent/mcp".into(),
        logs: "/nonexistent/logs".into(),
    };
    assert!(matches!(
        mcp::tools::call(&dirs, "factory_delete", &json!({})),
        Err(ToolError::Unknown(_))
    ));
}

// Without the snapshot there is nothing to answer with, whatever the files
// under live say.
#[test]
fn no_snapshot_is_a_tool_error_for_the_story() {
    let env = Env::built(
        "nosnap-issue",
        Some("a/b"),
        &[sess("work", 7, 60, json!({}))],
        None,
        None,
    );
    std::fs::remove_file(env.dirs.sessions.join("sessions.jsonl")).unwrap();
    match env.call("factory_issue", json!({"issue": 7})) {
        Err(ToolError::File { file, .. }) => assert_eq!(file, "sessions.jsonl"),
        other => panic!("{other:?}"),
    }
}

// A repository the dashboard does not publish well leaves the answer with no
// links, and a run of an issue whose pull request is none has no prUrl.
#[test]
fn without_a_valid_repo_the_story_has_no_links() {
    let rows = [sess("work", 7, 60, json!({"pr": 21}))];
    for repo in [None, Some("a/b/c")] {
        let env = Env::built(
            "nolinks",
            repo,
            &rows,
            Some(&[("stopped", json!([row(7, false)]))]),
            None,
        );
        let out = env.call("factory_issue", json!({"issue": 7})).unwrap();
        assert_eq!(out["stage"], "stopped");
        assert!(out.get("issueUrl").is_none(), "{repo:?}: {out}");
        assert!(out["attempts"][0].get("prUrl").is_none(), "{repo:?}: {out}");
    }
}

#[test]
fn the_stage_is_the_first_list_that_holds_the_issue() {
    // Every stage answers with its own word, in the order factory_issues gives
    // them, and an issue in two of them is listed under the earlier one.
    for stage in ["queued", "running", "awaiting", "stopped", "completed"] {
        let env = Env::built(
            "order",
            Some("a/b"),
            &[],
            Some(&[(stage, json!([row(11, false)]))]),
            None,
        );
        let out = env.call("factory_issue", json!({"issue": 11})).unwrap();
        assert_eq!(out["stage"], stage);
        assert_eq!(out["title"], "Issue 11");
    }
    let two = Env::built(
        "two-stages",
        Some("a/b"),
        &[],
        Some(&[
            ("completed", json!([row(11, true)])),
            ("stopped", json!([row(11, false)])),
        ]),
        None,
    );
    let out = two.call("factory_issue", json!({"issue": 11})).unwrap();
    assert_eq!(out["stage"], "stopped");
}

// A snapshot is a file on disk, so a malformed or hand-edited line is one the
// tool must survive: a named key that holds an object or an array is left out
// rather than served, and no subtree of the record comes out with it.
#[test]
fn a_composite_value_in_a_run_is_left_out_of_the_answer() {
    let good = sess("work", 7, 60, json!({"outcome": "merged"}));
    let bad = sess(
        "work",
        7,
        120,
        json!({
            "id": {"nested": "SECRET-ID"},
            "kind": ["work"],
            "attempt": {"n": 1},
            "how": ["label"],
            "outcome": {"word": "failed"},
            "failedStep": ["develop"],
            "failureKind": {"k": "model"},
            "failureCause": ["c"],
            "parkReason": {"r": "loop"},
            "seconds": [1],
        }),
    );
    let env = Env::built(
        "composite",
        Some("a/b"),
        &[good.clone(), bad.clone()],
        Some(&[("stopped", json!([row(7, false)]))]),
        None,
    );
    let out = env.call("factory_issue", json!({"issue": 7})).unwrap();
    let attempts = out["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    // The composite values are gone and the scalars of the same line are not,
    // except the times, which the defaults keep. An object is not an id, so
    // the line has none.
    assert_eq!(
        attempts[0],
        json!({"started": bad["started"], "ended": bad["ended"]})
    );
    for key in [
        "kind",
        "attempt",
        "how",
        "outcome",
        "failedStep",
        "failureKind",
        "failureCause",
        "parkReason",
        "seconds",
        "id",
    ] {
        assert!(attempts[0].get(key).is_none(), "{key}: {}", attempts[0]);
    }
    assert_eq!(attempts[1]["id"], good["id"]);
    assert_eq!(attempts[1]["outcome"], "merged");
    // The times and the outcome name the newest run that has them.
    assert_eq!(out["lastSeen"], good["started"]);
    assert_eq!(out["outcome"], "merged");
    assert!(!out.to_string().contains("SECRET"), "{out}");
}

// The paths the tool's outputSchema names are the paths its answer can have,
// and every one of them comes out of some case.
#[test]
fn the_storys_schema_names_every_field_of_an_answer() {
    let ended = sess(
        "plan",
        7,
        120,
        json!({
            "attempt": 1, "how": "fixer", "outcome": "failed", "failedStep": "plan",
            "failureKind": "model", "failureCause": "c", "parkReason": "loop", "pr": 8,
        }),
    );
    let running = sess("work", 7, 60, json!({"attempt": 2, "how": "millwright"}));
    let env = Env::built(
        "story-schema",
        Some("a/b"),
        &[ended, running],
        Some(&[("running", json!([row(7, false)]))]),
        Some(&[cycle(7, "plan", "running")]),
    );
    let name = "factory_issue";
    // A cycle row that holds its status and not its step: `pick` drops the
    // missing key, and `running.step` stays only in the schema. The tool's
    // fixtures have a snapshot of no runs, so this is the case that answers
    // the path beside the stage.
    let thin = Env::built(
        "story-thin-running",
        Some("a/b"),
        &[],
        Some(&[("running", json!([row(7, false)]))]),
        Some(&[json!({"issue": 7, "running": true, "status": "running"})]),
    );
    let mut thin_got = BTreeSet::new();
    paths(
        &thin.call(name, json!({"issue": 7})).unwrap(),
        "",
        &mut thin_got,
    );
    assert!(thin_got.contains("running.status"), "{thin_got:?}");
    assert!(!thin_got.contains("running.step"), "{thin_got:?}");
    let out = env.call(name, json!({"issue": 7})).unwrap();
    let mut want = BTreeSet::new();
    schema_paths(&schema_of(name), "", &mut want);
    let mut got = BTreeSet::new();
    paths(&out, "", &mut got);
    for p in &got {
        assert!(want.contains(p) || p == "api", "{p} is not in its schema");
    }
    if let Some(p) = want.difference(&got).next() {
        panic!("{name}: {p} is in its schema and never comes out");
    }
    assert!(out["attempts"].is_array());
}

const IMG_A: &str = "sha256:aaaaaaaaaaaa1111111111111111111111111111111111111111111111111111";
const IMG_B: &str = "sha256:bbbbbbbbbbbb2222222222222222222222222222222222222222222222222222";

fn imaged(issue: i64, ago: i64, image: &str, extra: Value) -> Value {
    let built = if image == IMG_A {
        "2026-10-20T10:00:00Z"
    } else {
        "2026-10-25T10:00:00Z"
    };
    let mut s = sess("work", issue, ago, extra);
    s["image"] = json!(image);
    s["imageBuilt"] = json!(built);
    s
}

#[test]
fn a_summary_names_the_image_and_when_it_was_built() {
    let env = Env::new(
        "img-summary",
        Some("a/b"),
        &[
            imaged(1, 10, IMG_A, json!({})),
            sess("plan", 2, 20, json!({})),
        ],
    );
    let out = env.call("factory_sessions", json!({})).unwrap();
    let s = &out["sessions"];
    assert_eq!(s[0]["image"], IMG_A);
    assert_eq!(s[0]["imageBuilt"], "2026-10-20T10:00:00Z");
    // A run of before images were recorded has neither.
    assert!(s[1].get("image").is_none() && s[1].get("imageBuilt").is_none());
}

#[test]
fn the_image_filter_matches_an_id_or_its_start() {
    let all = [
        imaged(1, 10, IMG_A, json!({})),
        imaged(2, 20, IMG_B, json!({})),
        imaged(3, 30, IMG_A, json!({})),
        sess("work", 4, 40, json!({})),
    ];
    let env = Env::new("img-filter", Some("a/b"), &all);
    let want = [
        all[0]["id"].as_str().unwrap(),
        all[2]["id"].as_str().unwrap(),
    ];
    for image in [IMG_A, &IMG_A[7..], &IMG_A[..19], &IMG_A[7..19]] {
        let out = env
            .call("factory_sessions", json!({"image": image}))
            .unwrap();
        assert_eq!(ids(&out), want, "{image}");
    }
    for bad in [
        json!("sha256:"),
        json!("aaaaaaaaaaa"),
        json!("sha256:AAAAAAAAAAAA"),
        json!(format!("{IMG_A}0")),
        json!("sha256:aaaaaaaaaaaz"),
        json!("../aaaaaaaaaaaa"),
        json!(12),
    ] {
        assert!(
            matches!(
                env.call("factory_sessions", json!({"image": bad})),
                Err(ToolError::BadArgs(_))
            ),
            "{bad}"
        );
    }
}

// Two builds of the same pi-rukas version, told apart by their image.
#[test]
fn failures_compare_two_builds_of_one_version() {
    let rows = [
        imaged(
            1,
            10,
            IMG_B,
            json!({"outcome": "failed", "failureKind": "verify", "failureCause": "c1"}),
        ),
        imaged(2, 20, IMG_B, json!({"outcome": "merged"})),
        imaged(3, 30, IMG_B, json!({"outcome": "merged"})),
        imaged(
            4,
            40,
            IMG_A,
            json!({"outcome": "failed", "failureKind": "cap", "failureCause": "c2"}),
        ),
        sess(
            "work",
            5,
            50,
            json!({"outcome": "failed", "rukasVersion": "v0.13.0"}),
        ),
    ];
    let env = Env::new("img-failures", Some("a/b"), &rows);

    let by_version = env.call("factory_failures", json!({})).unwrap();
    let v = by_version["versions"].as_array().unwrap();
    assert_eq!(v[0]["runs"], 4);
    assert!(v[0].get("image").is_none());

    let out = env
        .call("factory_failures", json!({"by": "image"}))
        .unwrap();
    let v = out["versions"].as_array().unwrap();
    assert_eq!(v.len(), 3);
    assert_eq!(v[0]["rukasVersion"], "v0.14.0");
    assert_eq!(v[0]["image"], IMG_B);
    assert_eq!(v[0]["imageBuilt"], "2026-10-25T10:00:00Z");
    assert_eq!(v[0]["runs"], 3);
    assert_eq!(v[0]["outcomes"], json!({"failed": 1, "merged": 2}));
    let a = v.iter().find(|p| p["image"] == IMG_A).unwrap();
    assert_eq!(a["rukasVersion"], "v0.14.0");
    assert_eq!(a["imageBuilt"], "2026-10-20T10:00:00Z");
    assert_eq!(a["failureKinds"], json!({"cap": 1}));
    // A run with no image on record is a group of its own, without one.
    let none = v.iter().find(|p| p["rukasVersion"] == "v0.13.0").unwrap();
    assert!(none.get("image").is_none() && none.get("imageBuilt").is_none());

    let one = env
        .call(
            "factory_failures",
            json!({"by": "image", "rukasVersion": "v0.14.0"}),
        )
        .unwrap();
    assert_eq!(one["versions"].as_array().unwrap().len(), 2);
    let commit = env
        .call("factory_failures", json!({"rukasVersion": "v0.13.0"}))
        .unwrap();
    assert_eq!(commit["versions"][0]["runs"], 1);

    for bad in [
        json!({"by": "week"}),
        json!({"by": 1}),
        json!({"rukasVersion": ""}),
        json!({"rukasVersion": "x".repeat(65)}),
    ] {
        assert!(
            matches!(
                env.call("factory_failures", bad.clone()),
                Err(ToolError::BadArgs(_))
            ),
            "{bad}"
        );
    }
}
