// factory_events over dispatch.log and refresh.log: the fixtures, and logs
// the tests write, grow, truncate and rotate.

use mcp::events::{DEFAULT_EVENTS, EVENTS_MAX, READ_MAX, TEXT_CUT};
use mcp::keys::parse_keys;
use mcp::rpc::{handle, Request};
use mcp::tools::{call, Dirs, Output, ToolError};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use tehas_core::time::iso;

const T0: i64 = 1792485967;

fn token() -> String {
    format!("ghp_{}", "abcdefghijklmnopqrstuvwxyzABCDEFGHIJ")
}

fn fixture_dirs() -> Dirs {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    Dirs {
        live: f.join("live"),
        sessions: f.join("sessions"),
        logs: f.join("logs"),
    }
}

struct Env {
    root: PathBuf,
    dirs: Dirs,
}

impl Env {
    fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("mcp-events-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs {
            live: root.join("live"),
            sessions: root.join("mcp"),
            logs: root.join("logs"),
        };
        for d in [&dirs.live, &dirs.sessions, &dirs.logs] {
            std::fs::create_dir_all(d).unwrap();
        }
        Env { root, dirs }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dirs.logs.join(name)
    }

    fn append(&self, name: &str, text: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(name))
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn line(t: i64, text: &str) -> String {
    format!("{} {text}\n", iso(t))
}

fn events_call(dirs: &Dirs, args: Value) -> Result<Value, ToolError> {
    match call(dirs, "factory_events", &args)? {
        Output::Json(v) => Ok(v),
        other => panic!("not JSON: {other:?}"),
    }
}

struct Answer {
    // (log, text) of each event, in the order given.
    events: Vec<(String, String)>,
    next: String,
    gaps: Vec<String>,
    raw: Value,
}

fn poll(dirs: &Dirs, after: Option<&str>, limit: Option<u64>) -> Answer {
    let mut args = json!({});
    if let Some(a) = after {
        args["after"] = json!(a);
    }
    if let Some(l) = limit {
        args["limit"] = json!(l);
    }
    let v = events_call(dirs, args).unwrap();
    assert_eq!(v["api"], 1);
    let events = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["log"].as_str().unwrap().to_string(),
                e["text"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let gaps = v["gaps"]
        .as_array()
        .map(|a| a.iter().map(|g| g.as_str().unwrap().to_string()).collect())
        .unwrap_or_default();
    Answer {
        events,
        next: v["next"].as_str().unwrap().to_string(),
        gaps,
        raw: v,
    }
}

/// Polls until an answer brings nothing, and returns every event seen.
fn drain(dirs: &Dirs, after: &mut String, limit: Option<u64>) -> Vec<(String, String)> {
    let mut all = Vec::new();
    for _ in 0..10_000 {
        let a = poll(dirs, Some(after), limit);
        assert!(a.gaps.is_empty(), "a gap: {}", a.raw);
        *after = a.next;
        if a.events.is_empty() && a.raw["more"] == false {
            return all;
        }
        all.extend(a.events);
    }
    panic!("the polls never ended");
}

fn texts(events: &[(String, String)]) -> Vec<&str> {
    events.iter().map(|(_, t)| t.as_str()).collect()
}

#[test]
fn the_fixtures_give_the_last_events_merged_by_time_and_masked() {
    let v = events_call(&fixture_dirs(), json!({"limit": mcp::events::EVENTS_MAX})).unwrap();
    let events = v["events"].as_array().unwrap();
    assert!(!events.is_empty());
    let times: Vec<&str> = events.iter().filter_map(|e| e["time"].as_str()).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted, "merged by time");
    let logs: Vec<&str> = events.iter().map(|e| e["log"].as_str().unwrap()).collect();
    assert!(logs.contains(&"dispatch") && logs.contains(&"refresh"));
    let text = v.to_string();
    assert!(!text.contains("hunter2"), "a password came out: {text}");
    assert!(text.contains("password=***"), "{text}");
    for e in events {
        let kind = e["kind"].as_str().unwrap();
        assert!(mcp::events::KINDS.contains(&kind), "{e}");
    }
    let kinds = |t: &str| -> Vec<&str> {
        events
            .iter()
            .filter(|e| e["text"].as_str().unwrap().starts_with(t))
            .map(|e| e["kind"].as_str().unwrap())
            .collect()
    };
    assert_eq!(kinds("start #41"), ["start"]);
    assert_eq!(kinds("done #41"), ["done"]);
    assert_eq!(kinds("skip #316"), ["skip"]);
    assert_eq!(kinds("read 1 pull requests"), ["refresh"]);
    assert_eq!(
        kinds("holding: the limit allows 0 cycles at once and 1 are running"),
        ["wait"]
    );
    let start = events
        .iter()
        .find(|e| e["text"].as_str().unwrap().starts_with("start #41"))
        .unwrap();
    assert_eq!(start["issue"], 41);
    assert!(v["next"].as_str().unwrap().len() > 10);
}

#[test]
fn through_the_server_it_is_listed_and_answers() {
    let k = "abcdefghijklmnopqrstuvwxyz012345";
    let keys = parse_keys(k, "").unwrap();
    let send = |body: Value, name: Option<&str>| {
        let auth = format!("Bearer {k}");
        let method = body["method"].as_str().unwrap().to_string();
        let mut headers = vec![
            ("Authorization".to_string(), auth),
            ("MCP-Protocol-Version".to_string(), "2026-07-28".to_string()),
            ("Mcp-Method".to_string(), method),
        ];
        if let Some(n) = name {
            headers.push(("Mcp-Name".to_string(), n.to_string()));
        }
        let bytes = body.to_string().into_bytes();
        let req = Request {
            method: "POST",
            headers,
            body: &bytes,
        };
        let r = handle(&req, &keys, &fixture_dirs(), "v9.9.9");
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        (v, r.tool)
    };
    let meta = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28"});
    let (list, _) = send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {"_meta": meta}}),
        None,
    );
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "factory_events")
        .expect("factory_events is listed")
        .clone();
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert!(tool["outputSchema"]["properties"]["events"].is_object());
    let (r, logged) = send(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "_meta": meta, "name": "factory_events", "arguments": {"limit": 3}}}),
        Some("factory_events"),
    );
    assert_eq!(logged.as_deref(), Some("factory_events"));
    let res = &r["result"];
    assert!(res.get("isError").is_none(), "{res}");
    assert_eq!(
        res["structuredContent"]["events"].as_array().unwrap().len(),
        3
    );
    // Without a key: nothing.
    let bytes = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
        "_meta": meta, "name": "factory_events", "arguments": {}}})
    .to_string()
    .into_bytes();
    let req = Request {
        method: "POST",
        headers: vec![],
        body: &bytes,
    };
    assert_eq!(handle(&req, &keys, &fixture_dirs(), "v9").status, 401);
}

#[test]
fn every_field_of_an_answer_is_in_the_schema() {
    let v = events_call(&fixture_dirs(), json!({})).unwrap();
    let tool = mcp::events::tool();
    let props = tool["outputSchema"]["properties"].as_object().unwrap();
    for k in v.as_object().unwrap().keys() {
        assert!(props.contains_key(k), "{k}");
    }
    let item = props["events"]["items"]["properties"].as_object().unwrap();
    for e in v["events"].as_array().unwrap() {
        for k in e.as_object().unwrap().keys() {
            assert!(item.contains_key(k), "{k}");
        }
    }
    for (k, p) in props.iter().chain(item.iter()) {
        assert!(!p["description"].as_str().unwrap_or("").is_empty(), "{k}");
    }
}

#[test]
fn without_after_the_last_hundred_events_oldest_first() {
    let env = Env::new("last");
    let mut d = String::new();
    for i in 0..250 {
        d.push_str(&line(T0 + i, &format!("skip #{i}: d{i}")));
    }
    env.append("dispatch.log", &d);
    let a = poll(&env.dirs, None, None);
    assert_eq!(a.events.len() as u64, DEFAULT_EVENTS);
    assert_eq!(a.events[0].1, "skip #150: d150");
    assert_eq!(a.events[99].1, "skip #249: d249");
    // The cursor is past the end: nothing more.
    let b = poll(&env.dirs, Some(&a.next), None);
    assert!(b.events.is_empty());
    assert_eq!(b.next, a.next);
    let c = poll(&env.dirs, None, Some(7));
    assert_eq!(c.events.len(), 7);
    assert_eq!(c.events[6].1, "skip #249: d249");
}

#[test]
fn a_follower_sees_each_new_line_once_merged_by_time() {
    let env = Env::new("follow");
    env.append("dispatch.log", &line(T0, "start #1 (labelled by alice)"));
    env.append(
        "refresh.log",
        &line(T0 + 1, "read 1 pull requests, 0 conflict, wrote 0 requests"),
    );
    let a = poll(&env.dirs, None, None);
    assert_eq!(texts(&a.events).len(), 2);
    let mut after = a.next;
    env.append("dispatch.log", &line(T0 + 10, "wait #2: a"));
    env.append("dispatch.log", &line(T0 + 30, "done #1 rc=0 status=merged"));
    env.append(
        "refresh.log",
        &line(
            T0 + 20,
            "read 2 pull requests, 1 conflict, wrote 1 requests",
        ),
    );
    let b = poll(&env.dirs, Some(&after), None);
    assert_eq!(
        texts(&b.events),
        [
            "wait #2: a",
            "read 2 pull requests, 1 conflict, wrote 1 requests",
            "done #1 rc=0 status=merged"
        ]
    );
    after = b.next;
    assert!(drain(&env.dirs, &mut after, None).is_empty());
}

#[test]
fn each_line_is_masked() {
    let env = Env::new("mask");
    env.append(
        "dispatch.log",
        &line(T0, &format!("ERROR: gh api failed with {} for #5", token())),
    );
    let a = poll(&env.dirs, None, None);
    assert_eq!(
        texts(&a.events),
        ["ERROR: gh api failed with ghp_*** for #5"]
    );
    assert_eq!(a.raw["events"][0]["kind"], "error");
    assert!(!a.raw.to_string().contains(&token()));
}

#[test]
fn a_half_written_line_waits_for_its_newline() {
    let env = Env::new("half");
    env.append("dispatch.log", &line(T0, "start #1"));
    let mut after = poll(&env.dirs, None, None).next;
    env.append("dispatch.log", &format!("{} skip #2: half", iso(T0 + 1)));
    assert!(drain(&env.dirs, &mut after, None).is_empty());
    env.append("dispatch.log", " written\n");
    let got = drain(&env.dirs, &mut after, None);
    assert_eq!(texts(&got), ["skip #2: half written"]);
}

#[test]
fn a_rotation_by_rename_loses_nothing() {
    let env = Env::new("rename");
    env.append("dispatch.log", &line(T0, "start #1"));
    let mut after = poll(&env.dirs, None, None).next;
    env.append("dispatch.log", &line(T0 + 1, "wait #2: old one"));
    env.append("dispatch.log", &line(T0 + 2, "wait #3: old two"));
    std::fs::rename(env.path("dispatch.log"), env.path("dispatch.log.1")).unwrap();
    env.append("dispatch.log", &line(T0 + 3, "wait #4: new one"));
    let got = drain(&env.dirs, &mut after, None);
    assert_eq!(
        texts(&got),
        ["wait #2: old one", "wait #3: old two", "wait #4: new one"]
    );
}

#[test]
fn a_rotation_read_one_event_at_a_time_loses_nothing() {
    let env = Env::new("rename-slow");
    env.append("dispatch.log", &line(T0, "start #1"));
    env.append(
        "refresh.log",
        &line(T0, "read 0 pull requests, 0 conflict, wrote 0 requests"),
    );
    let mut after = poll(&env.dirs, None, None).next;
    let mut want = Vec::new();
    for i in 1..=5 {
        let t = format!("skip #{i}: old");
        env.append("dispatch.log", &line(T0 + i, &t));
        want.push(t);
    }
    let a = poll(&env.dirs, Some(&after), Some(2));
    after = a.next;
    let mut got: Vec<String> = a.events.into_iter().map(|(_, t)| t).collect();
    std::fs::rename(env.path("dispatch.log"), env.path("dispatch.log.1")).unwrap();
    for i in 6..=8 {
        let t = format!("skip #{i}: new");
        env.append("dispatch.log", &line(T0 + i, &t));
        want.push(t);
    }
    got.extend(
        drain(&env.dirs, &mut after, Some(1))
            .into_iter()
            .map(|(_, t)| t),
    );
    assert_eq!(got, want);
}

#[test]
fn a_copy_and_truncate_rotation_reads_the_copy_then_the_new_file() {
    let env = Env::new("copytruncate");
    env.append("dispatch.log", &line(T0, "start #1"));
    let mut after = poll(&env.dirs, None, None).next;
    env.append("dispatch.log", &line(T0 + 1, "wait #2: before the copy"));
    std::fs::copy(env.path("dispatch.log"), env.path("dispatch.log.1")).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(env.path("dispatch.log"))
        .unwrap();
    // Longer than what was read before, so only its first bytes tell it apart.
    for i in 0..5 {
        env.append(
            "dispatch.log",
            &line(T0 + 2 + i, &format!("wait #{}: after it", 10 + i)),
        );
    }
    let got = drain(&env.dirs, &mut after, None);
    assert_eq!(got.len(), 6, "{got:?}");
    assert_eq!(got[0].1, "wait #2: before the copy");
    assert_eq!(got[5].1, "wait #14: after it");
}

#[test]
fn a_file_replaced_with_no_old_copy_is_a_gap_read_from_its_start() {
    let env = Env::new("gap");
    env.append("dispatch.log", &line(T0, "start #1"));
    env.append("dispatch.log", &line(T0 + 1, "start #2"));
    let after = poll(&env.dirs, None, None).next;
    std::fs::remove_file(env.path("dispatch.log")).unwrap();
    env.append("dispatch.log", &line(T0 + 5, "start #3"));
    let a = poll(&env.dirs, Some(&after), None);
    assert_eq!(a.gaps, ["dispatch"]);
    assert_eq!(texts(&a.events), ["start #3"]);
    let b = poll(&env.dirs, Some(&a.next), None);
    assert!(b.gaps.is_empty() && b.events.is_empty());
}

#[test]
fn a_truncated_file_is_a_gap_read_from_its_start() {
    let env = Env::new("truncated");
    env.append(
        "dispatch.log",
        &line(T0, "start #1 with a long text to be longer"),
    );
    let after = poll(&env.dirs, None, None).next;
    std::fs::write(env.path("dispatch.log"), line(T0 + 1, "start #2")).unwrap();
    let a = poll(&env.dirs, Some(&after), None);
    assert_eq!(a.gaps, ["dispatch"]);
    assert_eq!(texts(&a.events), ["start #2"]);
}

#[test]
fn a_log_that_appears_later_is_read_from_its_start() {
    let env = Env::new("appears");
    let a = poll(&env.dirs, None, None);
    assert!(a.events.is_empty());
    env.append(
        "refresh.log",
        &line(T0, "read 1 pull requests, 0 conflict, wrote 0 requests"),
    );
    let b = poll(&env.dirs, Some(&a.next), None);
    assert!(b.gaps.is_empty());
    assert_eq!(b.events.len(), 1);
}

#[test]
fn a_big_log_is_read_in_bounded_pieces_each_line_once() {
    let env = Env::new("big");
    let mut after = poll(&env.dirs, None, None).next;
    let mut d = String::new();
    let mut want = Vec::new();
    let mut i = 0;
    while (d.len() as u64) < 3 * READ_MAX {
        let t = format!("skip #{i}: {}", "x".repeat(200));
        d.push_str(&line(T0 + i, &t));
        want.push(t);
        i += 1;
    }
    env.append("dispatch.log", &d);
    let first = poll(&env.dirs, Some(&after), Some(EVENTS_MAX));
    assert!(first.events.len() as u64 <= EVENTS_MAX);
    let got = drain(&env.dirs, &mut after, Some(EVENTS_MAX));
    assert_eq!(
        texts(&got),
        want.iter().map(String::as_str).collect::<Vec<_>>()
    );
}

#[test]
fn a_line_longer_than_a_read_gives_its_start_and_never_stalls() {
    let env = Env::new("long");
    let mut after = poll(&env.dirs, None, None).next;
    let long = "y".repeat(READ_MAX as usize * 2 + 10);
    env.append("dispatch.log", &format!("{long}\n"));
    env.append("dispatch.log", &line(T0, "start #9"));
    let got = drain(&env.dirs, &mut after, Some(1));
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0].1, "y".repeat(TEXT_CUT));
    assert_eq!(got[1].1, "start #9");
}

#[test]
fn a_secret_across_the_end_of_a_read_never_comes_out() {
    let env = Env::new("long-secret");
    let mut after = poll(&env.dirs, None, None).next;
    let cut = READ_MAX as usize - 6;
    env.append(
        "dispatch.log",
        &format!("{} {}\n", "y".repeat(cut), token()),
    );
    env.append("dispatch.log", &line(T0, "start #9"));
    let got = drain(&env.dirs, &mut after, None);
    assert_eq!(texts(&got).last(), Some(&"start #9"));
    for (_, t) in &got {
        assert!(!t.contains("defghij"), "{t}");
    }
}

/// The cursor with one log's offset and skip flag changed, as a caller could.
fn forge(next: &str, log: usize, off: u64, skip: bool) -> String {
    let mut f: Vec<String> = next.split('.').map(String::from).collect();
    f[2 + log * 4] = off.to_string();
    f[3 + log * 4] = if skip { "1" } else { "0" }.into();
    f.join(".")
}

#[test]
fn a_forged_offset_inside_a_line_reads_no_part_of_it() {
    let env = Env::new("forged");
    let secret = format!("WARN: GH_TOKEN=supersecretvalue123456 {}", token());
    env.append("dispatch.log", &line(T0, &"x".repeat(200)));
    env.append("dispatch.log", &line(T0 + 1, &secret));
    let honest = poll(&env.dirs, None, None);
    assert!(!honest.raw.to_string().contains("supersecret"));
    let data = std::fs::read_to_string(env.path("dispatch.log")).unwrap();
    for word in ["hp_abc", "upersecret", "OKEN="] {
        let off = data.find(word).unwrap() as u64;
        for skip in [false, true] {
            let a = events_call(
                &env.dirs,
                json!({"after": forge(&honest.next, 0, off, skip)}),
            );
            let text = format!("{a:?}");
            assert!(!text.contains("abcdefghij"), "{word} {skip}: {text}");
            assert!(!text.contains("upersecret"), "{word} {skip}: {text}");
        }
    }
    // An honest offset at a line start still reads on from there.
    let second = data.find("\n").unwrap() as u64 + 1;
    let a = poll(
        &env.dirs,
        Some(&forge(&honest.next, 0, second, false)),
        None,
    );
    assert!(a.gaps.is_empty(), "{}", a.raw);
    assert_eq!(texts(&a.events), ["WARN: GH_TOKEN=*** ghp_***"]);
}

#[test]
fn a_forged_skip_past_a_read_skips_to_the_next_line() {
    let env = Env::new("forged-skip");
    let mut after = poll(&env.dirs, None, None).next;
    env.append("dispatch.log", &line(T0, &"x".repeat(READ_MAX as usize)));
    env.append("dispatch.log", &line(T0 + 1, &format!("WARN: {}", token())));
    env.append("dispatch.log", &line(T0 + 2, "start #9"));
    let all = drain(&env.dirs, &mut after, None);
    assert_eq!(all.len(), 3);
    let data = std::fs::read_to_string(env.path("dispatch.log")).unwrap();
    let off = data.find("hp_abc").unwrap() as u64;
    let a = poll(&env.dirs, Some(&forge(&after, 0, off, true)), None);
    assert_eq!(texts(&a.events), ["start #9"]);
}

#[test]
fn a_copy_that_cuts_a_line_joins_it_with_its_rest() {
    let env = Env::new("copy-cut");
    let mut held = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env.path("dispatch.log"))
        .unwrap();
    held.write_all(line(T0, "start #1").as_bytes()).unwrap();
    let mut after = poll(&env.dirs, None, None).next;
    held.write_all(format!("{} WARN: half", iso(T0 + 1)).as_bytes())
        .unwrap();
    std::fs::copy(env.path("dispatch.log"), env.path("dispatch.log.1")).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(env.path("dispatch.log"))
        .unwrap();
    assert!(drain(&env.dirs, &mut after, None).is_empty());
    held.write_all(b" a line\n").unwrap();
    held.write_all(line(T0 + 2, "done #1 rc=0 status=merged").as_bytes())
        .unwrap();
    let got = drain(&env.dirs, &mut after, None);
    assert_eq!(
        texts(&got),
        ["WARN: half a line", "done #1 rc=0 status=merged"]
    );
}

#[test]
fn a_copy_that_cuts_a_long_line_skips_its_rest_in_the_new_log() {
    let env = Env::new("copy-cut-long");
    let mut held = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(env.path("dispatch.log"))
        .unwrap();
    let mut after = poll(&env.dirs, None, None).next;
    held.write_all("y".repeat(READ_MAX as usize + 10).as_bytes())
        .unwrap();
    let mut got = drain(&env.dirs, &mut after, None);
    std::fs::copy(env.path("dispatch.log"), env.path("dispatch.log.1")).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(env.path("dispatch.log"))
        .unwrap();
    held.write_all(b"yy").unwrap();
    got.extend(drain(&env.dirs, &mut after, None));
    held.write_all(format!("y\n{}", line(T0, "start #9")).as_bytes())
        .unwrap();
    got.extend(drain(&env.dirs, &mut after, None));
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[1].1, "start #9");
}

#[test]
fn just_after_a_rotation_the_last_events_include_the_copy() {
    let env = Env::new("tail-rotated");
    env.append("dispatch.log", &line(T0, "start #1 (labelled by a)"));
    env.append("dispatch.log", &line(T0 + 1, "wait #2: x"));
    std::fs::rename(env.path("dispatch.log"), env.path("dispatch.log.1")).unwrap();
    std::fs::write(env.path("dispatch.log"), "").unwrap();
    let a = poll(&env.dirs, None, None);
    assert_eq!(texts(&a.events), ["start #1 (labelled by a)", "wait #2: x"]);
    let b = poll(&env.dirs, None, Some(1));
    assert_eq!(texts(&b.events), ["wait #2: x"]);
    let mut after = a.next;
    env.append("dispatch.log", &line(T0 + 2, "wait #3: y"));
    assert_eq!(texts(&drain(&env.dirs, &mut after, None)), ["wait #3: y"]);
}

#[test]
fn a_link_is_not_followed() {
    let env = Env::new("link");
    let outside = env.root.join("secret.txt");
    std::fs::write(&outside, line(T0, "start #1 secret")).unwrap();
    std::os::unix::fs::symlink(&outside, env.path("dispatch.log")).unwrap();
    match events_call(&env.dirs, json!({})) {
        Err(ToolError::File { file, why }) => {
            assert_eq!(file, "dispatch.log");
            assert!(why.contains("not a plain file"), "{why}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn bad_arguments_are_a_tool_error() {
    let env = Env::new("bad");
    env.append("dispatch.log", &line(T0, "start #1"));
    let good = poll(&env.dirs, None, None).next;
    let cases = [
        json!({"limit": 0}),
        json!({"limit": EVENTS_MAX + 1}),
        json!({"limit": "5"}),
        json!({"limit": -1}),
        json!({"limit": 1.5}),
        json!({"after": 5}),
        json!({"after": ""}),
        json!({"after": "x".repeat(300)}),
        json!({"after": "../../etc/passwd"}),
        json!({"after": good.replacen('1', "2", 1).replace('.', ":")}),
        json!({"after": format!("{good}.0")}),
        json!({"after": good.to_uppercase()}),
        json!({"after": good.replace("1.", "9.")}),
        json!({"path": "/etc/passwd"}),
        json!({"file": "dispatch.log"}),
        json!("hit"),
        json!(42),
        json!([1]),
    ];
    for args in cases {
        match events_call(&env.dirs, args.clone()) {
            Err(ToolError::BadArgs(why)) => assert!(!why.is_empty()),
            other => panic!("{args}: {other:?}"),
        }
    }
    // An offset past 64 bits is refused, not wrapped.
    let huge = format!(
        "1.1.{}.0.0000000000000000.0.0.0.0000000000000000",
        "9".repeat(20)
    );
    assert!(matches!(
        events_call(&env.dirs, json!({"after": huge})),
        Err(ToolError::BadArgs(_))
    ));
    assert!(events_call(&env.dirs, json!(null)).is_ok());
}

// The Done-when: a client that polls with the cursor it was given sees every
// line both logs get, once each, in each log's order, across rotations of
// either kind, however the writes and the polls fall.
#[test]
fn a_poller_sees_every_line_once_across_rotations() {
    let env = Env::new("soak");
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut rnd = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let mut after = poll(&env.dirs, None, None).next;
    let mut want: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    let mut got: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    let names = ["dispatch.log", "refresh.log"];
    let mut t = T0;
    // A log rotated since the poller last read to the end. Rotating it again
    // first drains it: with one rotated copy kept, as here, a poller must read
    // the old file before the next rotation takes it away.
    let mut rotated = [false; 2];
    // The rest of a line written in two parts, so a rotation can cut it.
    let mut rest: [Option<String>; 2] = [None, None];
    for step in 0..600 {
        let r = rnd(20);
        if r < 2 {
            let i = rnd(2) as usize;
            if rotated[i] {
                for (j, r) in rest.iter_mut().enumerate() {
                    if let Some(r) = r.take() {
                        env.append(names[j], &r);
                    }
                }
                for (log, text) in drain(&env.dirs, &mut after, Some(3)) {
                    got[usize::from(log == "refresh")].push(text);
                }
                rotated = [false; 2];
            }
            rotated[i] = true;
            let f = names[i];
            if r == 0 {
                // Rename rotation; an older .1 is overwritten.
                if env.path(f).exists() {
                    std::fs::rename(env.path(f), env.path(&format!("{f}.1"))).unwrap();
                }
            } else if env.path(f).exists() {
                std::fs::copy(env.path(f), env.path(&format!("{f}.1"))).unwrap();
                std::fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(env.path(f))
                    .unwrap();
            }
            continue;
        }
        match r {
            2..=4 => {
                let a = poll(&env.dirs, Some(&after), Some(1 + rnd(5)));
                assert!(a.gaps.is_empty(), "step {step}: {}", a.raw);
                for (log, text) in a.events {
                    got[usize::from(log == "refresh")].push(text);
                }
                after = a.next;
            }
            _ => {
                let i = rnd(2) as usize;
                if let Some(r) = rest[i].take() {
                    env.append(names[i], &r);
                    continue;
                }
                t += rnd(3) as i64;
                let text = format!("skip #{step}: line {step} of {}", names[i]);
                let l = line(t, &text);
                if rnd(4) == 0 {
                    let cut = 1 + rnd(l.len() as u64 - 1) as usize;
                    env.append(names[i], &l[..cut]);
                    rest[i] = Some(l[cut..].to_string());
                } else {
                    env.append(names[i], &l);
                }
                want[i].push(text);
            }
        }
    }
    for (i, r) in rest.iter_mut().enumerate() {
        if let Some(r) = r.take() {
            env.append(names[i], &r);
        }
    }
    for (log, text) in drain(&env.dirs, &mut after, None) {
        got[usize::from(log == "refresh")].push(text);
    }
    assert_eq!(got[0], want[0]);
    assert_eq!(got[1], want[1]);
}
