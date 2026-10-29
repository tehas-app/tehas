// factory_park over a snapshot and a logs dir written by the tests.

use mcp::tools::{list, Dirs, Output, ToolError};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tehas_core::question::park_sig;
use tehas_core::time::{iso, now};

const RESULT: &str = "work-7-261029-085206.jsonl.result.json";

fn address() -> String {
    [10, 1, 2, 3].map(|n: u8| n.to_string()).join(".")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/park")
}

struct Env {
    root: PathBuf,
    dirs: Dirs,
}

impl Env {
    fn new(name: &str, sessions: &[Value]) -> Env {
        let root = std::env::temp_dir().join(format!("mcp-park-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (live, snap, logs) = (root.join("live"), root.join("mcp"), root.join("logs"));
        for d in [&live, &snap, &logs] {
            std::fs::create_dir_all(d).unwrap();
        }
        // The fixture names no private address; the one the mask must take
        // out is put in here.
        let text = std::fs::read_to_string(fixture().join(RESULT)).unwrap();
        std::fs::write(logs.join(RESULT), text.replace("@ADDRESS@", &address())).unwrap();
        let mut text =
            json!({"api": 1, "days": 30, "generatedAt": iso(now()), "sessions": 1}).to_string();
        for s in sessions {
            text.push('\n');
            text.push_str(&s.to_string());
        }
        std::fs::write(snap.join("sessions.jsonl"), text + "\n").unwrap();
        Env {
            root,
            dirs: Dirs {
                live,
                sessions: snap,
                logs,
            },
        }
    }

    fn call(&self, args: Value) -> Result<Value, ToolError> {
        match mcp::tools::call(&self.dirs, "factory_park", &args)? {
            Output::Json(v) => Ok(v),
            other => panic!("factory_park gave {other:?}"),
        }
    }

    fn write_log(&self, name: &str, text: &str) {
        std::fs::write(self.dirs.logs.join(name), text).unwrap();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// A work run of issue 7 that handed off at the verify of develop, as the
// snapshot of a host has it, with its result log.
fn parked(started: i64) -> Value {
    json!({
        "id": format!("work-7-{started}"), "kind": "work", "issue": 7, "attempt": 2,
        "how": "label", "started": iso(started), "ended": iso(started + 600),
        "seconds": 600, "outcome": "handoff", "status": "handoff", "exitCode": 10,
        "lastStep": "develop", "cap": "verify-failed:develop", "failureKind": "verify",
        "providerOutage": false,
        "logs": [
            {"word": "result", "name": RESULT, "bytes": 100},
            {"word": "dispatch", "name": "dispatch-work-7.log", "bytes": 10},
        ],
    })
}

fn run(kind: &str, issue: i64, started: i64, outcome: &str) -> Value {
    json!({
        "id": format!("{kind}-{issue}-{started}"), "kind": kind, "issue": issue,
        "started": iso(started), "ended": iso(started + 60), "outcome": outcome,
        "lastStep": "lens-review", "logs": [],
    })
}

fn err_text(e: ToolError) -> String {
    match e {
        ToolError::BadArgs(w) => w,
        ToolError::File { file, why } => format!("{file} {why}"),
        ToolError::Unknown(n) => panic!("unknown {n}"),
    }
}

#[test]
fn a_parked_issue_answers_its_step_kind_sig_and_failing_checks() {
    let t = now() - 3600;
    let env = Env::new("parked", &[parked(t)]);
    let out = env.call(json!({"issue": 7})).unwrap();
    assert_eq!(out["api"], 1);
    assert_eq!(out["issue"], 7);
    assert_eq!(out["id"], format!("work-7-{t}"));
    assert_eq!(out["outcome"], "handoff");
    assert_eq!(out["status"], "handoff");
    assert_eq!(out["exitCode"], 10);
    assert_eq!(out["step"], "develop");
    assert_eq!(out["kind"], "verify");
    assert_eq!(out["cap"], "verify-failed:develop");
    assert_eq!(out["provider"], false);
    assert_eq!(out["latest"], true);
    // The marker of the park comment: the same hash of the same fields.
    assert_eq!(
        out["sig"],
        park_sig("develop", "verify-failed:develop", "", "verify", "")
    );
    assert_eq!(out["result"], "read");
    assert_eq!(out["causeClass"], "change");
    assert!(out["causeReason"]
        .as_str()
        .unwrap()
        .starts_with("the verify command failed"));
    assert!(out["userAction"].as_str().unwrap().contains("/work 7"));
    let checks: Vec<&str> = out["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(
        checks,
        vec![
            "verify command `bash scripts/verify.sh` failed: reached [private address]",
            "FAIL: changed units are reported: expected '', got 'units tehas-mcp.service'",
            "test.sh: failed checks (1), not run (0)",
            "FAILED: test/drain_test.sh: asked models.lan:8086 for a model",
        ]
    );
    assert!(out.get("checksCut").is_none());
    let text = out.to_string();
    assert!(!text.contains(&address()), "{text}");
    for bad in [
        "ghp_",
        "SECRET",
        "stateFile",
        "work-state",
        RESULT,
        "workspace's tests",
        "5 failed",
    ] {
        assert!(!text.contains(bad), "{bad} in {text}");
    }
    assert!(out["message"].as_str().unwrap().contains("GITHUB_TOKEN="));
}

#[test]
fn the_newest_stop_is_answered_and_says_whether_a_run_came_after() {
    let t = now() - 7200;
    let mut older = parked(t);
    older["cap"] = json!("review-incomplete");
    older["lastStep"] = json!("lens-review");
    older["failureKind"] = json!("process");
    older["logs"] = json!([]);
    // Newest first, as the host writes it.
    let env = Env::new(
        "newest",
        &[
            run("work", 7, t + 3000, "merged"),
            run("plan", 7, t + 2000, "handoff"),
            parked(t + 1000),
            older,
        ],
    );
    let out = env.call(json!({"issue": 7})).unwrap();
    assert_eq!(out["id"], format!("work-7-{}", t + 1000));
    assert_eq!(out["cap"], "verify-failed:develop");
    assert_eq!(out["latest"], false);
}

#[test]
fn every_stop_outcome_counts_and_the_others_do_not() {
    let t = now() - 3600;
    for outcome in ["handoff", "parked", "failed", "refused"] {
        let env = Env::new(&format!("stop-{outcome}"), &[run("work", 7, t, outcome)]);
        let out = env.call(json!({"issue": 7})).unwrap();
        assert_eq!(out["outcome"], outcome);
        assert_eq!(out["result"], "none");
        assert!(out.get("checks").is_none());
        assert_eq!(out["sig"], park_sig("lens-review", "", "", "", ""));
    }
    for outcome in ["merged", "pr", "stopped"] {
        let env = Env::new(&format!("go-{outcome}"), &[run("work", 7, t, outcome)]);
        let e = err_text(env.call(json!({"issue": 7})).unwrap_err());
        assert!(e.contains("issue 7"), "{e}");
    }
}

#[test]
fn an_issue_with_no_stop_is_a_tool_error_that_names_it() {
    let t = now() - 3600;
    let env = Env::new(
        "nostop",
        &[run("work", 8, t, "merged"), run("plan", 9, t, "handoff")],
    );
    for issue in [8, 9, 10] {
        let e = err_text(env.call(json!({ "issue": issue })).unwrap_err());
        assert_eq!(
            e,
            format!("issue {issue} has no stopped work run in the last 30 days")
        );
    }
}

#[test]
fn a_step_left_out_is_signed_as_the_cycle_signs_it() {
    let t = now() - 3600;
    let mut s = run("work", 7, t, "parked");
    s.as_object_mut().unwrap().remove("lastStep");
    let env = Env::new("nostep", &[s]);
    let out = env.call(json!({"issue": 7})).unwrap();
    assert!(out.get("step").is_none());
    assert_eq!(
        out["sig"],
        "7e45afdeccfc3eb5505b1de691f6392e508199ca4275e92c42fdb17d51ad87ec"
    );
}

#[test]
fn arguments_are_checked() {
    let env = Env::new("args", &[parked(now() - 60)]);
    for args in [
        json!({}),
        json!({"issue": 0}),
        json!({"issue": -1}),
        json!({"issue": "7"}),
        json!({"issue": 1.5}),
        json!({"issue": 1_000_000_000u64}),
        json!({"issue": 7, "path": "/etc/passwd"}),
        json!({"issue": 7, "file": "result"}),
        json!("7"),
        json!(null),
        json!([7]),
    ] {
        let e = env.call(args.clone()).unwrap_err();
        assert!(matches!(e, ToolError::BadArgs(_)), "{args}: {e:?}");
    }
    assert!(env.call(json!({"issue": 999_999_999})).is_err());
    assert!(env.call(json!({"issue": 7})).is_ok());
}

#[test]
fn a_result_log_is_opened_only_by_its_own_name() {
    let t = now() - 3600;
    for (i, name) in [
        "../mcp/sessions.jsonl",
        "/etc/passwd",
        "work-8-261029-085206.jsonl.result.json",
        "work-7-261029-085206.jsonl",
        "dispatch.log",
    ]
    .iter()
    .enumerate()
    {
        let mut s = parked(t);
        s["logs"] = json!([{"word": "result", "name": name, "bytes": 1}]);
        let env = Env::new(&format!("name-{i}"), &[s]);
        if !name.starts_with('/') && !name.starts_with("..") {
            env.write_log(name, r#"{"issues":[{"issue":7,"causeReason":"PLANTED"}]}"#);
        }
        let out = env.call(json!({"issue": 7})).unwrap();
        assert_eq!(out["result"], "unreadable", "{name}");
        assert!(!out.to_string().contains("PLANTED"), "{name}");
        assert!(!out.to_string().contains("api\":1,\"days"), "{name}");
        assert_eq!(out["cap"], "verify-failed:develop");
    }
}

#[test]
fn a_result_that_is_gone_large_a_link_or_not_json_is_said_so() {
    let t = now() - 3600;
    let env = Env::new("gone", &[parked(t)]);
    std::fs::remove_file(env.dirs.logs.join(RESULT)).unwrap();
    assert_eq!(env.call(json!({"issue": 7})).unwrap()["result"], "gone");

    let env = Env::new("large", &[parked(t)]);
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(env.dirs.logs.join(RESULT))
        .unwrap();
    f.set_len(1 << 40).unwrap();
    assert_eq!(
        env.call(json!({"issue": 7})).unwrap()["result"],
        "unreadable"
    );

    let env = Env::new("link", &[parked(t)]);
    let real = env.dirs.logs.join("real.json");
    std::fs::rename(env.dirs.logs.join(RESULT), &real).unwrap();
    std::os::unix::fs::symlink(&real, env.dirs.logs.join(RESULT)).unwrap();
    let out = env.call(json!({"issue": 7})).unwrap();
    assert_eq!(out["result"], "unreadable");
    assert!(out.get("checks").is_none());

    let env = Env::new("notjson", &[parked(t)]);
    env.write_log(RESULT, "not json\n");
    assert_eq!(
        env.call(json!({"issue": 7})).unwrap()["result"],
        "unreadable"
    );

    // A result of another issue only: nothing of it is this run's.
    let env = Env::new("other", &[parked(t)]);
    env.write_log(
        RESULT,
        r#"{"issues":[{"issue":8,"causeReason":"PLANTED"}]}"#,
    );
    let out = env.call(json!({"issue": 7})).unwrap();
    assert_eq!(out["result"], "unreadable");
    assert!(!out.to_string().contains("PLANTED"));
}

#[test]
fn checks_and_texts_are_bounded() {
    let t = now() - 3600;
    let env = Env::new("bounded", &[parked(t)]);
    let long = "x".repeat(5000);
    let evidence: Vec<String> = (0..60).map(|i| format!("FAIL: case {i} {long}")).collect();
    env.write_log(
        RESULT,
        &json!({"issues": [{"issue": 7, "causeEvidence": evidence,
            "causeReason": long, "userAction": long,
            "failure": {"message": format!("line one\nline two {long}"),
                        "evidence": evidence.join("\n")}}]})
        .to_string(),
    );
    let out = env.call(json!({"issue": 7})).unwrap();
    let checks = out["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 20);
    assert_eq!(out["checksCut"], true);
    for c in checks {
        assert_eq!(c.as_str().unwrap().chars().count(), 400);
    }
    for k in ["causeReason", "userAction", "message"] {
        let s = out[k].as_str().unwrap();
        assert!(s.chars().count() <= 400, "{k}");
        assert!(!s.contains('\n'), "{k}");
    }
    assert!(out["message"]
        .as_str()
        .unwrap()
        .starts_with("line one line two"));
}

#[test]
fn a_field_of_another_type_is_left_out() {
    let t = now() - 3600;
    let env = Env::new("types", &[parked(t)]);
    env.write_log(
        RESULT,
        r#"{"issues":[{"issue":7,"causeEvidence":"FAIL: a string","causeClass":"two words",
            "causeReason":{"x":1},"failure":{"message":7,"evidence":["FAIL: x"]}}]}"#,
    );
    let out = env.call(json!({"issue": 7})).unwrap();
    assert_eq!(out["result"], "read");
    for k in [
        "checks",
        "causeClass",
        "causeReason",
        "message",
        "userAction",
    ] {
        assert!(out.get(k).is_none(), "{k}: {out}");
    }
}

fn schema_paths(s: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    if let Some(props) = s["properties"].as_object() {
        for (k, v) in props {
            let p = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            assert!(
                !v["description"].as_str().unwrap_or("").is_empty(),
                "{p} is not described"
            );
            out.insert(p.clone());
            schema_paths(v, &p, out);
        }
    }
}

#[test]
fn the_answer_is_its_schema() {
    let tool = list()
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "factory_park")
        .cloned()
        .expect("factory_park is listed");
    assert_eq!(tool["inputSchema"]["required"], json!(["issue"]));
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    let mut want = BTreeSet::new();
    schema_paths(&tool["outputSchema"], "", &mut want);
    let mut s = parked(now() - 3600);
    s["parkReason"] = json!("x-y");
    s["failureCause"] = json!("self-killed:timeout");
    s["failedStep"] = json!("develop");
    let env = Env::new("schema", &[s]);
    let out = env.call(json!({"issue": 7})).unwrap();
    let got: BTreeSet<String> = out.as_object().unwrap().keys().cloned().collect();
    // checksCut needs more than 20 lines; the bound test shows it.
    let mut all = got.clone();
    all.insert("checksCut".into());
    assert_eq!(all, want);
}
