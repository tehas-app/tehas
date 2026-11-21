//! The three commands around the factory's state: tehasbot status writes the
//! queue snapshot and runs tehasbot health beside it, and `tehas status`
//! prints the health file.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::process::{Command, Stdio};
use tehas_core::time::{iso, now};

/// jq -r `filter` of `input`, its standard output without the last newline.
fn jq_r(filter: &str, input: &str) -> String {
    let mut child = Command::new("jq")
        .args(["-r", filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

fn live_text(w: &World, name: &str) -> String {
    fs::read_to_string(w.live_file(name)).unwrap_or_default()
}

/// jq -r of live/status.json.
fn sr(w: &World, filter: &str) -> String {
    jq_r(filter, &live_text(w, "status.json"))
}

/// jq -c of live/status.json.
fn sc(w: &World, filter: &str) -> String {
    jq(filter, &live_text(w, "status.json"))
}

/// jq -r of live/health.json.
fn hr(w: &World, filter: &str) -> String {
    jq_r(filter, &live_text(w, "health.json"))
}

/// jq -c of live/health.json.
fn hc(w: &World, filter: &str) -> String {
    jq(filter, &live_text(w, "health.json"))
}

/// The gh calls that hold `pat`.
fn gh_count(w: &World, pat: &str) -> usize {
    w.gh().lines().filter(|l| l.contains(pat)).count()
}

/// `$status && $summary`.
fn status_and_summary(w: &World) -> Run {
    w.sh(&format!("{} && {}", w.status_cmd(), w.summary_cmd()))
}

/// `$status; $summary`.
fn status_then_summary(w: &World) -> Run {
    w.sh(&format!("{}; {}", w.status_cmd(), w.summary_cmd()))
}

#[track_caller]
fn ok(r: &Run) {
    assert_eq!(r.rc, 0, "{}", r.out);
}

fn write_json(w: &World, rel: &str, v: &Value) {
    w.fx(rel, &format!("{v}\n"));
}

fn issues(w: &World, list: &[String]) {
    w.fx("issues.json", &format!("[{}]\n", list.join(",")));
}

/// A merged closer of issue `n`, with no labels field, as jq -n wrote them.
fn merged_node(n: u64) -> Value {
    json!({"number": n, "title": "t", "url": "u", "updatedAt": "2026-01-02T00:00:00Z",
        "closedAt": "2026-01-02T00:00:00Z", "timelineItems": {"nodes": [{"closer": {"merged": true}}]}})
}

/// The count answer as text, whatever it is.
fn count_answer(w: &World, raw: &str) {
    w.fx(
        "count.json",
        &format!(r#"{{"data":{{"search":{{"issueCount":{raw}}}}}}}"#),
    );
}

/// A field of live/status.json set to the time `ago` seconds back.
fn status_aged(w: &World, fields: &[&str], ago: i64) {
    let mut v = w.live_json("status.json");
    for f in fields {
        v[*f] = json!(iso(now() - ago));
    }
    fs::write(w.live_file("status.json"), v.to_string()).unwrap();
}

/// The lists of work in the snapshot.
fn lists(w: &World) -> String {
    sc(
        w,
        "[.running, .queued, .awaiting, .stopped, .planned, .planning]",
    )
}

/// The completed work in the snapshot.
fn done_part(w: &World) -> String {
    sc(w, "[.completedAt, .completed, .completedCount]")
}

/// One tehas:pause event on the control issue, by `login` or by no account.
fn pause_events(w: &World, event: &str, login: &str) {
    let actor = if login.is_empty() {
        Value::Null
    } else {
        json!({"login": login})
    };
    write_json(
        w,
        "events.json",
        &json!([{"event": event, "label": {"name": "tehas:pause"}, "actor": actor,
            "created_at": "2026-01-02T00:00:00Z"}]),
    );
}

/// A lock file under the state, held until the File is dropped.
fn hold(w: &World, rel: &str) -> File {
    let p = w.home.join("state").join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .unwrap();
    f.try_lock().unwrap();
    f
}

#[track_caller]
fn store_must(w: &World, args: &[&str]) {
    let r = w.store(args);
    assert_eq!(r.rc, 0, "the store refused {args:?}: {}", r.out);
}

/// A planning turn in flight (t_plan): its session running on the state
/// store after `before` turns, the turn started `ago` seconds back, and its
/// lock held until the File is dropped, as the turn's unit holds it.
fn turn(w: &World, n: u64, before: u32, ago: i64) -> File {
    let brief = w.t().join("plan-brief.md");
    fs::write(&brief, "# Faster search\n\nMake it fast.\n").unwrap();
    let brief = brief.to_string_lossy().into_owned();
    let n_s = n.to_string();
    store_must(
        w,
        &[
            "plan-begin",
            &n_s,
            "--by",
            "alice",
            "--label",
            "tehas:plan",
            "--brief-file",
            &brief,
        ],
    );
    let answer = |t: u32, at: Option<i64>| {
        let t = t.to_string();
        let args = [
            "plan-answer",
            &n_s,
            "--turn",
            &t,
            "--by",
            "alice",
            "--text-file",
            &brief,
        ];
        let r = match at {
            Some(e) => w.store_at(e, &args),
            None => w.store(&args),
        };
        assert_eq!(r.rc, 0, "the store refused {args:?}: {}", r.out);
    };
    for t in 1..=before {
        if t > 1 {
            answer(t, None);
        }
        let ts = t.to_string();
        let end = [
            "plan-end",
            &n_s,
            "--turn",
            &ts,
            "--outcome",
            "finished",
            "--rc",
            "0",
            "--session",
            "abc",
        ];
        store_must(w, &end);
        if t < before {
            store_must(w, &["plan-end", &n_s, "--posted"]);
        }
    }
    if before > 0 {
        store_must(w, &["plan-end", &n_s, "--posted"]);
        answer(before + 1, Some(now() - ago));
    }
    hold(w, &format!("plan-{n}.json.lock"))
}

/// A cycle of issue `n` that ended as `status`, started and ended so many
/// seconds ago.
fn ended(w: &World, n: u64, status: &str, started: i64, updated: i64) {
    w.one(n, false, status, "develop", Some(started), Some(updated));
}

#[test]
fn status_writes_the_snapshot_and_the_health_file_without_a_command_on_path() {
    let w = World::new("snapshot");
    let r = w.status();
    assert_eq!(r.rc, 0);
    assert_eq!(r.out, "");
    assert_eq!(sr(&w, ".queued | length"), "2");
    assert_eq!(hr(&w, ".status"), "pass");
    assert_eq!(hr(&w, ".checks.queue.review"), "1");
}

#[test]
fn status_with_a_setting_not_given_stops_with_78_before_github_and_before_a_write_however_it_is_started(
) {
    let w = World::new("unset");
    let home = format!("TEHAS_HOME={}", w.home.display());
    let all = [
        home.as_str(),
        "TEHAS_REPO=acme/widget",
        "TEHAS_CONTROL_ISSUE=1",
        "TEHAS_ALLOWED_LABELERS=boss",
    ];
    for name in [
        "TEHAS_HOME",
        "TEHAS_REPO",
        "TEHAS_CONTROL_ISSUE",
        "TEHAS_ALLOWED_LABELERS",
    ] {
        let given: Vec<&str> = all
            .iter()
            .copied()
            .filter(|w| !w.starts_with(&format!("{name}=")))
            .collect();
        let r = w.sh(&format!(
            "env -u {name} {} '{}' tehasbot status",
            given.join(" "),
            w.t().join("tree/libexec/console").display()
        ));
        assert_eq!(r.rc, 78, "{name}: {}", r.out);
        has(&r.out, &format!("{name} is not set"));
        assert_eq!(w.gh(), "");
        assert!(
            !w.live.exists(),
            "{name}: status wrote {}",
            w.live.display()
        );
    }
}

#[test]
fn status_writes_the_open_epics_and_their_sub_issues_and_not_an_issue_without_any() {
    let w = World::new("epics");
    w.epics_page(&[
        r#"{"number":7,"title":"An epic","url":"u7","subIssuesSummary":{"total":2,"completed":1},
  "subIssues":{"nodes":[{"number":8,"title":"Done","url":"u8","state":"CLOSED","repository":{"nameWithOwner":"acme/widget"}},
  {"number":9,"title":"To do","url":"u9","state":"OPEN","repository":{"nameWithOwner":"acme/widget"}}]}}"#
            .to_string(),
        r#"{"number":3,"title":"Plain","url":"u3","subIssuesSummary":{"total":0,"completed":0},"subIssues":{"nodes":[]}}"#
            .to_string(),
    ]);
    let r = w.status();
    ok(&r);
    let epics = live_text(&w, "epics.json");
    assert_eq!(
        jq("[.repo, (.epics | map(.number))]", &epics),
        r#"["acme/widget",[7]]"#
    );
    assert_eq!(
        jq(".epics[0].subIssues | map([.number, .state])", &epics),
        r#"[[8,"closed"],[9,"open"]]"#
    );
    assert_eq!(
        jq(".epics[0] | [.total, .completed, .more]", &epics),
        "[2,1,0]"
    );
}

#[test]
fn an_open_issue_labelled_epic_with_no_sub_issues_is_listed_apart_and_the_health_file_warns_of_it()
{
    let w = World::new("unlinked");
    w.epics_page(&[
        r#"{"number":7,"title":"An epic","url":"u7","labels":{"nodes":[{"name":"epic"}]},"subIssuesSummary":{"total":1,"completed":0},
  "subIssues":{"nodes":[{"number":8,"title":"To do","url":"u8","state":"OPEN","repository":{"nameWithOwner":"acme/widget"}}]}}"#
            .to_string(),
        r#"{"number":5,"title":"EPIC: Unlinked","url":"u5","labels":{"nodes":[{"name":"epic"}]},"subIssuesSummary":{"total":0,"completed":0},"subIssues":{"nodes":[]}}"#
            .to_string(),
        r#"{"number":3,"title":"Plain","url":"u3","labels":{"nodes":[]},"subIssuesSummary":{"total":0,"completed":0},"subIssues":{"nodes":[]}}"#
            .to_string(),
    ]);
    let r = w.status();
    ok(&r);
    assert_eq!(
        jq(
            "[(.epics | map(.number)), .unlinked]",
            &live_text(&w, "epics.json")
        ),
        "[[7],[5]]"
    );
    assert_eq!(hr(&w, ".status"), "warn");
    assert_eq!(hc(&w, ".checks.epics.unlinked"), "[5]");
    has(&hr(&w, ".output"), "Labelled epic with no sub-issues: #5.");
}

#[test]
fn a_read_of_the_epics_that_fails_leaves_the_file_as_it_was_and_the_snapshot_is_still_written() {
    let w = World::new("epics-down");
    w.home_file("live/epics.json", "{\"epics\":\"before\"}\n");
    w.touch("epics-down");
    let r = w.status();
    ok(&r);
    assert_eq!(jq(".epics", &live_text(&w, "epics.json")), r#""before""#);
    assert_eq!(hc(&w, ".checks.epics"), "null");
    assert_eq!(hr(&w, ".status"), "pass");
    assert_eq!(sr(&w, ".queued | length"), "2");
    let left = fs::read_dir(&w.live)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("epics.json.")
        })
        .count();
    assert_eq!(left, 0);
}

#[test]
fn no_failed_tehas_unit_is_no_warning() {
    let w = World::new("units-none");
    w.touch("failed-units");
    w.status();
    assert_eq!(hr(&w, ".status"), "pass");
    assert_eq!(hc(&w, ".checks.units"), "[]");
    has(
        &w.calls("systemctl"),
        "--user --failed --plain --no-legend tehas-*",
    );
}

#[test]
fn a_failed_tehas_dispatch_path_is_a_warning_that_names_it() {
    let w = World::new("units-failed");
    w.fx(
        "failed-units",
        "tehas-dispatch.path loaded failed failed Tehas: a poll as soon as a cycle has ended\n",
    );
    w.status();
    assert_eq!(hr(&w, ".status"), "warn");
    assert_eq!(hc(&w, ".checks.units"), r#"["tehas-dispatch.path"]"#);
    has(&hr(&w, ".output"), "systemd gave up on tehas-dispatch.path");
}

#[test]
fn systemd_not_answering_the_failed_units_is_no_check_and_no_warning() {
    let w = World::new("units-down");
    w.set_env(&["SYSTEMCTL_RC=1"]);
    w.status();
    assert_eq!(hc(&w, ".checks.units"), "null");
    assert_eq!(hr(&w, ".status"), "pass");
    has(
        &w.calls("systemctl"),
        "--user --failed --plain --no-legend tehas-*",
    );
}

#[test]
fn a_closed_issue_is_not_in_line_being_worked_on_or_stopped_whatever_label_it_carries() {
    let w = World::new("closed");
    issues(
        &w,
        &[
            issue(3, "tehas:ready", "OPEN"),
            issue(4, "tehas:ready", "CLOSED"),
            issue(5, "tehas:awaiting-merge", "OPEN"),
            issue(6, "needs-human-attention", "OPEN"),
            issue(8, "tehas:running", "CLOSED"),
            issue(9, "needs-human-attention", "CLOSED"),
            issue(10, "tehas:question", "CLOSED"),
        ],
    );
    ok(&w.status());
    assert_eq!(sr(&w, ".queued | length"), "1");
    assert_eq!(sr(&w, ".running | length"), "0");
    assert_eq!(sr(&w, ".stopped | length"), "1");
    assert_eq!(sr(&w, ".awaiting | length"), "1");
    assert_eq!(sr(&w, "[.queued[].number] | join(\",\")"), "3");
    assert_eq!(hr(&w, ".status"), "pass");
}

// The control issue holds the pause switch and no outcome: whoever labels it,
// it is not work, so it is not counted in line, worked on, stopped or awaiting.
#[test]
fn the_control_issue_is_not_counted_in_line_worked_on_stopped_or_awaiting_whatever_label_it_carries(
) {
    let w = World::new("control");
    issues(
        &w,
        &[
            issue(1, "tehas:ready", "OPEN"),
            issue(3, "tehas:ready", "OPEN"),
            issue(1, "tehas:running", "OPEN"),
            issue(1, "needs-human-attention", "OPEN"),
            issue(1, "tehas:awaiting-merge", "OPEN"),
            issue(3, "tehas:awaiting-merge", "OPEN"),
            issue(6, "needs-human-attention", "OPEN"),
        ],
    );
    ok(&w.status());
    assert_eq!(
        sc(&w, "[.queued, .running, .stopped, .awaiting] | map(length)"),
        "[1,0,1,1]"
    );
    assert_eq!(sr(&w, "[.running[]?.number] | join(\",\")"), "");
    assert_eq!(sr(&w, "[.queued[].number] | join(\",\")"), "3");
    assert_eq!(sr(&w, "[.stopped[].number] | join(\",\")"), "6");
    assert_eq!(sr(&w, "[.awaiting[].number] | join(\",\")"), "3");
}

// The same numbers with the switch labelled tehas:pause, which is how the
// control issue is normally seen: the pause is reported, the issue is not.
#[test]
fn and_it_stays_out_of_the_counts_while_the_factory_is_paused_by_its_label() {
    let w = World::new("control-paused");
    issues(
        &w,
        &[
            issue(1, "tehas:ready", "OPEN"),
            issue(3, "tehas:ready", "OPEN"),
        ],
    );
    w.fx("paused", "true\n");
    w.status();
    assert_eq!(sr(&w, ".paused"), "true");
    assert_eq!(
        sc(&w, "[.queued, .running, .stopped, .awaiting] | map(length)"),
        "[1,0,0,0]"
    );
    assert_eq!(sr(&w, "[.queued[].number] | join(\",\")"), "3");
}

// A cycle starts with --restart and pi-rukas takes needs-human-attention off
// once it runs: until then the issue carries both labels.
#[test]
fn an_issue_a_cycle_is_running_on_is_not_stopped_whatever_else_it_carries() {
    let w = World::new("running-stopped");
    w.fx(
        "issues.json",
        r#"[{"number":6,"title":"t","url":"u","state":"OPEN","labels":[{"name":"tehas:running"},{"name":"needs-human-attention"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":null}]"#,
    );
    w.status();
    assert_eq!(sr(&w, ".running | length"), "1");
    assert_eq!(sr(&w, ".stopped | length"), "0");
}

#[test]
fn planning_lists_and_counts_the_open_issues_labelled_tehas_plan_or_tehas_planning_from_the_same_search(
) {
    let w = World::new("planning");
    issues(
        &w,
        &[
            issue(3, "tehas:ready", "OPEN"),
            issue(7, "tehas:plan", "OPEN"),
            issue(8, "tehas:planning", "OPEN"),
            issue(9, "tehas:plan", "CLOSED"),
            issue(1, "tehas:planning", "OPEN"),
            issue(10, "tehas:question", "OPEN"),
        ],
    );
    ok(&w.status());
    assert_eq!(sr(&w, ".planning"), "2");
    // 1 is the control issue.
    assert_eq!(sc(&w, ".planned | map(.number)"), "[7,8]");
    assert_eq!(sr(&w, ".queued | length"), "1");
    assert_eq!(gh_count(&w, "--search"), 1);
    let search: String = w
        .gh()
        .lines()
        .filter(|l| l.contains("--search"))
        .collect::<Vec<_>>()
        .join("\n");
    has(
        &search,
        r#"label:"tehas:ready","tehas:running","tehas:awaiting-merge","needs-human-attention","tehas:plan","tehas:planning" "#,
    );
    w.reset();
    w.status();
    assert_eq!(sr(&w, ".planning"), "0");
    assert_eq!(sc(&w, ".planned"), "[]");
}

#[test]
fn the_breakroom_s_words_are_counted_over_every_issue_labelled_for_the_factory_or_not() {
    let w = World::new("buzzwords");
    w.fx(
        "all.json",
        r#"[{"title": "Dashboard shows the Queue", "labels": [{"name": "tehas:ready"}]},
 {"title": "A side-project: migrate the dashboard to v2", "labels": [{"name": "sidework"}, {"name": "Help Wanted"}]},
 {"title": "queue, QUEUE; dashboard!", "labels": []},
 {"title": null}, 7]
"#,
    );
    ok(&w.status());
    assert_eq!(
        sc(&w, ".buzzwords[:2]"),
        r#"[{"word":"dashboard","count":3},{"word":"queue","count":3}]"#
    );
    // Whole words in lower case, a hyphen kept, two letters too few; no title
    // and no issue number is published with them.
    assert_eq!(
        sr(&w, "[.buzzwords[].word] | sort | join(\" \")"),
        "dashboard help migrate queue ready shows side-project sidework tehas the wanted"
    );
    assert_eq!(
        sr(&w, ".buzzwords[] | select(.word == \"sidework\") | .count"),
        "1"
    );
    has(
        &w.gh(),
        "issue list --repo acme/widget --state all --limit 1000 --json title,labels",
    );
    assert_eq!(sr(&w, ".queued | length"), "2");
}

#[test]
fn at_most_a_hundred_words_are_published_the_most_used_first() {
    let w = World::new("buzzwords-100");
    let all: Vec<Value> = (0..150)
        .map(|i| json!({"title": format!("word{i} ").repeat(150 - i), "labels": []}))
        .collect();
    write_json(&w, "all.json", &json!(all));
    w.status();
    assert_eq!(sr(&w, ".buzzwords | length"), "100");
    assert_eq!(sc(&w, ".buzzwords[0]"), r#"{"word":"word0","count":150}"#);
}

#[test]
fn a_list_of_every_issue_past_the_limit_of_one_argument_still_gives_a_snapshot() {
    let w = World::new("buzzwords-big");
    // 1500 titles of 100 characters: over 128 KiB, which no one argument can
    // hold.
    let all: Vec<Value> = (0..1500)
        .map(|i| json!({"title": format!("word{i} {}", "x".repeat(100)), "labels": [{"name": "bug"}]}))
        .collect();
    write_json(&w, "all.json", &json!(all));
    ok(&w.status());
    assert_eq!(sr(&w, ".buzzwords | length"), "100");
    assert_eq!(sr(&w, ".queued | length"), "2");
}

#[test]
fn a_queue_snapshot_past_the_limit_of_one_argument_still_gives_a_health_file() {
    let w = World::new("queue-big");
    // 800 issues in line with titles of 200 characters: a status.json over
    // 128 KiB.
    let list: Vec<Value> = (0..800)
        .map(|i| {
            json!({"number": i + 100, "title": format!("t{i} {}", "x".repeat(200)), "state": "OPEN",
                "url": format!("https://github.com/acme/widget/issues/{}", i + 100),
                "updatedAt": "2026-10-22T00:00:00Z", "closedAt": null,
                "labels": [{"name": "tehas:ready"}]})
        })
        .collect();
    write_json(&w, "issues.json", &json!(list));
    ok(&w.status());
    assert!(fs::metadata(w.live_file("status.json")).unwrap().len() > 131072);
    assert_eq!(hr(&w, ".status"), "pass");
    assert_eq!(hr(&w, ".checks.queue.inLine"), "800");
}

#[test]
fn without_the_list_of_every_issue_the_snapshot_is_written_with_no_words() {
    let w = World::new("all-down");
    w.touch("all-down");
    ok(&w.status());
    assert_eq!(sr(&w, ".queued | length"), "2");
    assert_eq!(sr(&w, "has(\"buzzwords\")"), "false");
    w.fx("all.json", "{\"message\": \"not a list\"}\n");
    w.rm_fx("all-down");
    w.status();
    assert_eq!(
        sr(&w, "[(.queued | length), has(\"buzzwords\")] | join(\" \")"),
        "2 false"
    );
}

#[test]
fn every_issue_is_read_for_the_words_at_most_once_an_hour_the_words_before_kept_between_reads() {
    let w = World::new("buzzwords-hourly");
    let an_hour_ago = || {
        w.home_file("state/buzzwords.at", &format!("{}\n", now() - 3600));
    };
    w.fx(
        "all.json",
        "[{\"title\": \"Dashboard dashboard\", \"labels\": []}]\n",
    );
    w.status();
    assert_eq!(sc(&w, ".buzzwords"), r#"[{"word":"dashboard","count":2}]"#);
    w.fx("all.json", "[{\"title\": \"Queue\", \"labels\": []}]\n");
    w.clear_calls();
    ok(&w.status());
    assert_eq!(gh_count(&w, "--limit 1000"), 0);
    assert_eq!(sc(&w, ".buzzwords"), r#"[{"word":"dashboard","count":2}]"#);
    // An hour on, the list is read again.
    an_hour_ago();
    w.status();
    assert_eq!(gh_count(&w, "--limit 1000"), 1);
    assert_eq!(sc(&w, ".buzzwords"), r#"[{"word":"queue","count":1}]"#);
    // A read that fails keeps the words before, and the next run asks again.
    an_hour_ago();
    w.touch("all-down");
    w.status();
    assert_eq!(sc(&w, ".buzzwords"), r#"[{"word":"queue","count":1}]"#);
    w.rm_fx("all-down");
    w.clear_calls();
    w.status();
    assert_eq!(gh_count(&w, "--limit 1000"), 1);
}

#[test]
fn completed_is_the_closed_issues_a_merged_pull_request_closed_newest_first() {
    let w = World::new("completed");
    w.shipped(&[
        closed(20, "2026-01-02T00:00:00Z", r#"{"merged":true}"#, ""),
        closed(
            21,
            "2026-01-03T00:00:00Z",
            r#"{"merged":true}"#,
            "tehas:shipped",
        ),
        closed(22, "2026-01-04T00:00:00Z", r#"{"merged":false}"#, ""),
        closed(23, "2026-01-05T00:00:00Z", "null", ""),
        closed(24, "2026-01-06T00:00:00Z", "{}", ""),
    ]);
    ok(&w.status());
    assert_eq!(sc(&w, "[.completed[].number]"), "[21,20]");
    assert_eq!(
        sc(&w, ".completed[0] | keys"),
        r#"["closedAt","createdAt","number","title","updatedAt","url"]"#
    );
    assert_eq!(
        sc(&w, ".completed[0] | [.title, .url, .createdAt]"),
        r#"["t","u","2025-12-01T00:00:00Z"]"#
    );
    // The stub answers whatever it is asked, so the fields must be asked for.
    let gh = w.gh();
    has(&gh, "number title url createdAt updatedAt closedAt labels(");
    has(
        &gh,
        "--json number,title,url,state,labels,createdAt,updatedAt,closedAt",
    );
    has(&sr(&w, ".links.completed"), "label%3Atehas%3Ashipped");
    has(
        &gh,
        "q=repo:acme/widget is:issue is:closed linked:pr sort:updated-desc",
    );
    // The count comes from the search for the label, whose answer says 0 here.
    has(
        &gh,
        "q=repo:acme/widget is:issue is:closed label:tehas:shipped",
    );
    has(&gh, "first: 0) { issueCount }");
    assert_eq!(sr(&w, ".completedCount"), "0");
}

#[test]
fn completed_count_is_every_closed_issue_labelled_tehas_shipped_though_the_list_holds_only_the_latest_hundred(
) {
    let w = World::new("completed-count");
    let nodes: Vec<Value> = (0..50)
        .map(|i| {
            let mut n = merged_node(i + 100);
            n["labels"] = json!({"nodes": []});
            n
        })
        .collect();
    write_json(
        &w,
        "shipped.json",
        &json!({"data": {"search": {"nodes": nodes}}}),
    );
    w.shipped_count(138);
    ok(&w.status());
    assert_eq!(sr(&w, ".completed | length"), "50");
    assert_eq!(sr(&w, ".completedCount"), "138");
    // The two reads of shipped work, and the read of the epics beside them.
    assert_eq!(gh_count(&w, "api graphql -f q="), 2);
    assert_eq!(gh_count(&w, "api graphql"), 3);
}

// The figure is published as a whole number of issues, or no key at all: an
// answer that is not one is dropped rather than published for a reader to
// judge, and 0 is a published answer, not an absent one. The bounds are the
// ones host/fun/business-view.js accepts, so both sides of the pair are
// pinned here.
#[test]
fn completed_count_is_published_only_as_a_whole_number_of_issues() {
    let w = World::new("count-whole");
    w.one_shipped();
    for answer in ["0", "1", "9007199254740991"] {
        count_answer(&w, answer);
        ok(&w.status());
        assert_eq!(sr(&w, ".completedCount | type"), "number");
        assert_eq!(sr(&w, ".completedCount"), answer);
    }
    // -138 and 1e400 are numbers GitHub could answer and no reader can use.
    for answer in ["-138", "-0.5", "1e400", "1E+999", "9007199254740992"] {
        count_answer(&w, answer);
        ok(&w.status());
        assert_eq!(sr(&w, "has(\"completedCount\")"), "false", "{answer}");
    }
}

// A gh that fails the call outright, with an answer that would read well
// above every bound: the figure is left out as if the search had given no
// number.
#[test]
fn a_count_call_that_fails_leaves_the_field_out() {
    let w = World::new("count-down");
    w.one_shipped();
    w.shipped_count(138);
    w.touch("count-down");
    ok(&w.status());
    assert_eq!(sr(&w, "has(\"completedCount\") "), "false");
    assert_eq!(sc(&w, "[.completed[].number]"), "[20]");
}

#[test]
fn a_count_that_cannot_be_read_leaves_the_field_out_and_the_rest_of_the_snapshot_is_written() {
    let w = World::new("count-bad");
    w.one_shipped();
    // The count that cannot be read, however gh answers: the snapshot is
    // written as it was before the field, and the list stands as it is.
    for answer in [
        "",
        "not json",
        r#"{"errors": [{"message": "rate limited"}]}"#,
        r#"{"data": {}}"#,
        r#"{"data": {"search": {}}}"#,
        r#"{"data": {"search": {"issueCount": null}}}"#,
        r#"{"data": {"search": {"issueCount": "138"}}}"#,
        r#"{"data": {"search": {"nodes": []}}}"#,
    ] {
        w.fx("count.json", &format!("{answer}\n"));
        ok(&w.status());
        assert_eq!(sr(&w, "has(\"completedCount\")"), "false", "{answer}");
        assert_eq!(sc(&w, "[.completed[].number]"), "[20]");
        assert_eq!(sr(&w, ".queued | length"), "2");
    }
}

#[test]
fn a_shipped_issue_without_the_label_is_given_it_once() {
    let w = World::new("label-once");
    w.one_shipped();
    w.status();
    assert_eq!(gh_count(&w, "issue edit"), 1);
    has(
        &w.gh(),
        "issue edit 20 --repo acme/widget --add-label tehas:shipped",
    );
}

#[test]
fn a_shipped_issue_is_given_the_label_whatever_else_it_carries_and_once() {
    // The read answers by the closer and not by the labels, so an issue that
    // carries another label is shipped and edited like one that carries none:
    // three issues, three edits, the label itself absent from all of them.
    let w = World::new("label-other");
    let nodes: Vec<Value> = (6..9)
        .map(|i| {
            let mut n = merged_node(i);
            n["labels"] = json!({"nodes": [{"name": "bug"}]});
            n
        })
        .collect();
    write_json(
        &w,
        "shipped.json",
        &json!({"data": {"search": {"nodes": nodes}}}),
    );
    w.status();
    assert_eq!(sc(&w, "[.completed[].number] | sort"), "[6,7,8]");
    assert_eq!(gh_count(&w, "issue edit"), 3);
}

#[test]
fn an_issue_already_labelled_tehas_shipped_is_not_asked_for_it_whatever_the_poll() {
    // The read names the labels of every issue it lists, so the edit is asked
    // for only an issue that does not carry the label. gh issue edit is no
    // no-op: it reads the issue and sends a mutation every time, and one poll
    // used to pay 45 of them for issues that all held the label already
    // (#731).
    let w = World::new("label-held");
    w.shipped(&[closed(
        20,
        "2026-01-02T00:00:00Z",
        r#"{"merged":true}"#,
        "tehas:shipped",
    )]);
    ok(&w.status());
    assert_eq!(sc(&w, "[.completed[].number]"), "[20]");
    assert_eq!(gh_count(&w, "issue edit"), 0);
}

// One of three that shipped without it is the issue the poll edits, and the
// only one: the two that hold the label are left alone.
#[test]
fn of_three_shipped_issues_only_the_one_that_lacks_the_label_is_edited() {
    let w = World::new("label-one-of-three");
    w.shipped(&[
        closed(
            20,
            "2026-01-02T00:00:00Z",
            r#"{"merged":true}"#,
            "tehas:shipped",
        ),
        closed(21, "2026-01-03T00:00:00Z", r#"{"merged":true}"#, ""),
        closed(
            22,
            "2026-01-04T00:00:00Z",
            r#"{"merged":true}"#,
            "tehas:shipped",
        ),
    ]);
    ok(&w.status());
    assert_eq!(sc(&w, "[.completed[].number] | sort"), "[20,21,22]");
    assert_eq!(gh_count(&w, "issue edit"), 1);
    let gh = w.gh();
    has(
        &gh,
        "issue edit 21 --repo acme/widget --add-label tehas:shipped",
    );
    lacks(&gh, "issue edit 20 --repo");
    lacks(&gh, "issue edit 22 --repo");
}

// Of three shipped issues the poll edits the two that lack the label and the
// one whose labels are no list (issue #1083).
#[test]
fn the_issues_to_label_are_the_ones_the_read_says_lack_the_label() {
    let w = World::new("label-lacking");
    w.shipped(&[
        closed(
            20,
            "2026-01-02T00:00:00Z",
            r#"{"merged":true}"#,
            "tehas:shipped",
        ),
        closed(21, "2026-01-03T00:00:00Z", r#"{"merged":true}"#, ""),
        no_labels(22, "2026-01-04T00:00:00Z", r#"{"merged":true}"#),
    ]);
    ok(&w.status());
    assert_eq!(sc(&w, "[.completed[].number] | sort"), "[20,21,22]");
    assert_eq!(gh_count(&w, "issue edit"), 2);
    let gh = w.gh();
    has(
        &gh,
        "issue edit 21 --repo acme/widget --add-label tehas:shipped",
    );
    has(
        &gh,
        "issue edit 22 --repo acme/widget --add-label tehas:shipped",
    );
    lacks(&gh, "issue edit 20 --repo");
}

// A label the read could not read is a missing label: the edit is sent, as
// it was before the query read the labels at all.
#[test]
fn a_shipped_issue_whose_labels_the_read_did_not_answer_is_edited() {
    let w = World::new("label-unread");
    w.shipped(&[no_labels(20, "2026-01-02T00:00:00Z", r#"{"merged":true}"#)]);
    ok(&w.status());
    assert_eq!(sc(&w, "[.completed[].number]"), "[20]");
    assert_eq!(gh_count(&w, "issue edit"), 1);
    has(
        &w.gh(),
        "issue edit 20 --repo acme/widget --add-label tehas:shipped",
    );
}

#[test]
fn the_control_issue_is_not_listed_as_shipped_and_not_given_the_label_whatever_the_read_answers() {
    // It holds the pause switch and is not work: tehasbot status leaves the
    // number TEHAS_CONTROL_ISSUE names out of every list and every count, and
    // tehas tidy follows the same rule when it puts the label on.
    let w = World::new("label-control");
    write_json(
        &w,
        "shipped.json",
        &json!({"data": {"search": {"nodes": [merged_node(1), merged_node(20)]}}}),
    );
    w.status();
    assert_eq!(sc(&w, "[.completed[].number]"), "[20]");
    assert_eq!(gh_count(&w, "issue edit"), 1);
    lacks(&w.gh(), "issue edit 1 --repo");
}

#[test]
fn an_answer_that_carries_errors_is_no_snapshot_even_where_it_holds_a_merged_closer() {
    // The same refusal tehas tidy makes of an answer with errors beside its
    // data: the two commands read one acceptance rule, so neither may publish
    // what the other refuses. The snapshot of the run before is the thing that
    // must stay.
    let w = World::new("errors");
    w.status();
    let before = sc(&w, ".completed");
    write_json(
        &w,
        "shipped.json",
        &json!({"errors": [{"message": "declined"}], "data": {"search": {"nodes": [merged_node(20)]}}}),
    );
    w.status();
    assert_eq!(sc(&w, ".completed"), before);
    assert_eq!(gh_count(&w, "issue edit"), 0);
}

#[test]
fn an_issue_closed_with_its_label_still_on_is_not_counted_as_shipped_for_the_label_alone() {
    let w = World::new("closed-label");
    issues(&w, &[issue(5, "tehas:awaiting-merge", "CLOSED")]);
    w.status();
    assert_eq!(sc(&w, ".completed"), "[]");
    assert_eq!(sr(&w, ".completedCount"), "0");
}

#[test]
fn a_label_that_cannot_be_given_leaves_the_snapshot_written() {
    let w = World::new("edit-fails");
    w.shipped(&[closed(20, "2026-01-02T00:00:00Z", r#"{"merged":true}"#, "")]);
    w.set_env(&["GH_EDIT_FAILS=1"]);
    ok(&w.status());
    assert_eq!(sc(&w, "[.completed[].number]"), "[20]");
}

#[test]
fn without_the_search_for_shipped_work_the_completed_part_is_kept_and_said_stale() {
    let w = World::new("shipped-down");
    w.status();
    let lists_before = lists(&w);
    let done_before = done_part(&w);
    w.touch("shipped-down");
    let r = w.status();
    ok(&r);
    assert_eq!(lists(&w), lists_before);
    assert_eq!(done_part(&w), done_before);
    assert_eq!(sc(&w, ".stale"), r#"["completed"]"#);
    has(&r.out, "WARN status: gh api graphql failed");
    w.fx(
        "shipped.json",
        "{\"errors\": [{\"message\": \"rate limited\"}]}\n",
    );
    w.rm_fx("shipped-down");
    w.status();
    assert_eq!(done_part(&w), done_before);
    assert_eq!(sc(&w, ".stale"), r#"["completed"]"#);
}

#[test]
fn a_list_of_issues_that_cannot_be_read_keeps_the_lists_of_work_and_says_why() {
    // The read of the list of issues in work is the one the lists rest on:
    // they are the file's before, said stale, and the rest of the snapshot is
    // this run's.
    let w = World::new("issues-down");
    w.status();
    let lists_before = lists(&w);
    let read_at = sr(&w, ".queueAt");
    fs::remove_file(w.live_file("health.json")).unwrap();
    // The stub answers the search of the issues in work from issues.json and
    // the word cloud of every issue from all.json: only the first file is
    // gone here, so the read of the lists fails and the one it can do without
    // does not.
    w.rm_fx("issues.json");
    w.fx("paused", "true\n");
    let r = w.status();
    ok(&r);
    has(
        &w.gh(),
        "issue list --repo acme/widget --state all --limit 200",
    );
    has(&r.out, "WARN status: gh issue list failed");
    assert_eq!(lists(&w), lists_before);
    assert_eq!(sc(&w, ".stale"), r#"["queue"]"#);
    assert_eq!(sr(&w, ".queueAt"), read_at);
    // What was read is fresh: the pause on the control issue.
    assert_eq!(sr(&w, ".paused"), "true");
    let failures: Vec<String> = w
        .read_home("state/github-failures")
        .lines()
        .map(|l| l.split('\t').nth(1).unwrap_or(l).to_string())
        .collect();
    assert_eq!(failures.join("\n"), "WARN status: gh issue list failed");
    assert_eq!(hr(&w, ".checks.github.failed"), "1");
}

#[test]
fn an_answer_of_what_closed_these_issues_that_is_no_list_keeps_the_completed_part() {
    let w = World::new("shipped-no-list");
    w.status();
    let done_before = done_part(&w);
    w.fx("shipped.json", "{\"data\":{}}\n");
    let r = w.status();
    ok(&r);
    has(
        &r.out,
        "WARN status: the answer of what closed these issues was no list, its part of the snapshot is stale",
    );
    assert_eq!(done_part(&w), done_before);
    assert_eq!(sc(&w, ".stale"), r#"["completed"]"#);
    assert_eq!(hr(&w, ".status"), "pass");
}

#[test]
fn a_read_on_a_bad_minute_is_tried_once_more_and_the_snapshot_is_whole() {
    let w = World::new("bad-minute");
    w.status();
    fs::remove_file(w.live_file("health.json")).unwrap();
    // As the shell case had it: the calls are counted from the start of the
    // case, so call 1 is the first run's.
    w.gh_fail(1, 1, "HTTP 502: Bad Gateway");
    let r = w.status();
    ok(&r);
    assert_eq!(gh_count(&w, "--limit 200"), 2);
    assert_eq!(sc(&w, ".stale"), "null");
    lacks(&r.out, "WARN status");
    assert!(!w.home.join("state/github-failures").exists());
}

#[test]
fn with_github_down_the_lists_are_kept_and_health_names_the_failed_calls() {
    let w = World::new("gh-down");
    w.status();
    let lists_before = lists(&w);
    // And when the word list was read, so this run asks for it too.
    fs::remove_file(w.live_file("health.json")).unwrap();
    fs::remove_file(w.home.join("state/buzzwords.at")).unwrap();
    w.touch("gh-down");
    let r = status_and_summary(&w);
    ok(&r);
    assert_eq!(lists(&w), lists_before);
    assert_eq!(sc(&w, ".stale"), r#"["queue","completed","pause"]"#);
    assert_eq!(hr(&w, ".status"), "warn");
    has(
        &hr(&w, ".output"),
        "GitHub failed 6 calls in the last 10 min, the last: WARN status: gh ",
    );
    has(
        &r.out,
        "queue     stale: queue and completed and pause kept from an earlier read, GitHub did not answer",
    );
    has(&r.out, "github    6 failed calls in the last 10 min");
}

#[test]
fn given_the_poll_s_name_status_leaves_the_health_file_to_the_poll() {
    let w = World::new("from-poll");
    let st = w.status_cmd();
    w.set_env(&[&format!("TEHAS_STATUS_CMD={st}")]);
    let r = w.sh(&format!("unset TEHAS_FROM_POLL; {st}"));
    ok(&r);
    assert_eq!(hr(&w, ".status"), "pass");
    fs::remove_file(w.live_file("health.json")).unwrap();
    let r = w.sh(&format!("export TEHAS_FROM_POLL=1; {st}"));
    ok(&r);
    assert!(!w.live_file("health.json").exists());
    let r = w.sh(&format!("export TEHAS_FROM_POLL=; {st}"));
    ok(&r);
    assert_eq!(hr(&w, ".status"), "pass");
}

// The name of the token is not the name of the poll: a host whose poll holds
// no App token, or whose token read failed, still has health to refresh.
#[test]
fn the_poll_s_token_alone_does_not_leave_the_health_file_to_the_poll() {
    let w = World::new("poll-token");
    let st = w.status_cmd();
    let _ = fs::remove_file(w.live_file("health.json"));
    w.set_env(&[&format!("TEHAS_STATUS_CMD={st}")]);
    ok(&w.sh(&format!("export TEHAS_GH_TOKEN=ghs_poll; {st}")));
    assert_eq!(hr(&w, ".status"), "pass");
}

#[test]
fn the_poll_s_name_leaves_the_analytics_of_the_trap_alone() {
    let w = World::new("poll-analytics");
    w.stub_analytics(r#"printf "%s\n" "{\"report\":1}""#);
    let st = w.status_cmd();
    w.set_env(&[&format!("TEHAS_STATUS_CMD={st}"), "TEHAS_ANALYTICS=on"]);
    ok(&w.sh(&format!("export TEHAS_FROM_POLL=1; {st}")));
    assert_eq!(w.calls("analytics").lines().count(), 3);
}

#[test]
fn with_github_down_for_three_hours_health_warns_and_the_queue_line_says_how_old_it_is() {
    let w = World::new("gh-down-hours");
    w.status();
    status_aged(&w, &["generatedAt", "queueAt"], 3 * 3600 + 300);
    w.touch("gh-down");
    let r = status_and_summary(&w);
    ok(&r);
    assert_eq!(hr(&w, ".status"), "warn");
    assert_eq!(
        hr(&w, ".output"),
        "The queue snapshot is 3 h old: GitHub did not answer the last polls."
    );
    has(
        &r.out,
        "factory   warn: The queue snapshot is 3 h old: GitHub did not answer the last polls.",
    );
    has(
        &r.out,
        "queue     2 in line, 0 building, 1 in review, 1 stopped (3 h ago)",
    );
}

#[test]
fn a_snapshot_of_nine_minutes_passes_and_a_failed_cycle_is_still_said_first() {
    let w = World::new("nine-minutes");
    w.status();
    status_aged(&w, &["generatedAt"], 9 * 60);
    w.health();
    assert_eq!(hr(&w, ".status"), "pass");
    status_aged(&w, &["generatedAt"], 11 * 60);
    w.health();
    assert_eq!(
        hr(&w, ".output"),
        "The queue snapshot is 11 min old: GitHub did not answer the last polls."
    );
    w.cycle(false, "aborted", "develop");
    w.health();
    assert_eq!(hr(&w, ".status"), "fail");
}

#[test]
fn status_with_invalid_settings_writes_nothing_exit_78() {
    let w = World::new("invalid");
    w.instance(&["TEHAS_CONTROL_ISSUE=1"]);
    let r = w.status();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(w.gh(), "");
    assert!(!w.live.exists());
}

#[test]
fn summary_an_idle_factory_in_three_lines_exit_0() {
    let w = World::new("idle");
    let r = status_and_summary(&w);
    ok(&r);
    assert_eq!(r.out.lines().count(), 4, "{}", r.out);
    has(&r.out, "factory   pass: Idle, taking work. (");
    has(&r.out, "s ago)");
    has(
        &r.out,
        "queue     2 in line, 0 building, 1 in review, 1 stopped",
    );
    has(&r.out, "cycle     none yet");
    has(&r.out, "slice     unknown: no poll has reported in 10 min");
}

#[test]
fn summary_the_poll_line_under_the_factory_line_from_the_last_poll_on_record() {
    let w = World::new("poll-line");
    lacks(&status_and_summary(&w).out, "poll      ");
    let at = now();
    let r = w.store_at(
        at,
        &[
            "poll-record",
            "--started",
            &(at - 30).to_string(),
            "--cpu-ms",
            "3100",
            "--gh",
            "3",
            "--writes-log",
            &w.t().join("no-such.log").to_string_lossy(),
        ],
    );
    assert_eq!(r.rc, 0, "no poll recorded: {}", r.out);
    let r = w.summary(&[]);
    ok(&r);
    let lines: Vec<&str> = r.out.lines().collect();
    has(lines[1], "poll      last 3");
    has(
        &r.out,
        " s wall, 3.1 s CPU, 3 gh; last hour 1 poll, 3.1 s CPU",
    );
    has(lines[2], "queue     ");
}

#[test]
fn summary_the_last_tehas_check_product_when_one_ran() {
    let w = World::new("check-product");
    lacks(&status_and_summary(&w).out, "product");
    let at = iso(now() - 300);
    w.home_file(
        "state/check-product.json",
        &format!(r#"{{"v":1,"at":"{at}","ok":true,"failures":[],"sandbox":null}}"#),
    );
    has(
        &w.summary(&[]).out,
        "product   keeps the contract, checked 5 min ago",
    );
    w.home_file(
        "state/check-product.json",
        &format!(
            r#"{{"v":1,"at":"{at}","ok":false,"failures":["no AGENTS.md","no gate"],"sandbox":null}}"#
        ),
    );
    let r = w.summary(&[]);
    ok(&r);
    has(
        &r.out,
        "product   2 to fix: no AGENTS.md; no gate, checked 5 min ago",
    );
}

#[test]
fn summary_a_running_cycle() {
    let w = World::new("running");
    w.cycle(true, "running", "develop");
    let r = status_and_summary(&w);
    ok(&r);
    has(&r.out, "factory   pass: Working on #12.");
    has(
        &r.out,
        "cycle     #12 running, step develop, started 10 min ago",
    );
}

#[test]
fn summary_a_parked_cycle_is_not_a_failure() {
    let w = World::new("parked");
    w.cycle(false, "handoff", "review");
    let r = status_and_summary(&w);
    ok(&r);
    has(&r.out, "cycle     #12 handoff at step review, 3 min ago");
}

#[test]
fn summary_a_paused_factory_says_so_exit_0() {
    let w = World::new("paused");
    w.fx("paused", "true\n");
    let r = status_and_summary(&w);
    ok(&r);
    has(&r.out, "factory   warn: The factory is paused.");
    has(&r.out, "1 stopped, paused");
}

#[test]
fn status_writes_the_pause_switch_the_flag_with_its_time_and_reason_never_who_wrote_it() {
    let w = World::new("pause-flag");
    w.home_file(
        "state/pause.json",
        "{\"by\":\"boss\",\"at\":\"2026-10-15T10:00:00Z\",\"reason\":\"waiting for the gate\"}\n",
    );
    let r = status_and_summary(&w);
    ok(&r);
    assert_eq!(sr(&w, ".paused"), "true");
    assert_eq!(sr(&w, ".pause.flag | keys | join(\",\")"), "at,reason");
    assert_eq!(sr(&w, ".pause.flag.reason"), "waiting for the gate");
    assert_eq!(sr(&w, ".pause.label"), "false");
    lacks(&sc(&w, ".pause"), "boss");
    assert_eq!(hr(&w, ".status"), "warn");
    assert_eq!(hr(&w, ".output"), "The factory is paused.");
    assert_eq!(
        hr(&w, ".checks.pause.flag | keys | join(\",\")"),
        "at,reason"
    );
    has(&r.out, "pause     flag, written");
    has(&r.out, "waiting for the gate");
    has(&r.out, "1 stopped, paused");
}

#[test]
fn the_label_alone_is_a_pause_the_health_file_names_with_no_flag() {
    let w = World::new("pause-label");
    w.fx("paused", "true\n");
    let r = status_and_summary(&w);
    assert_eq!(sc(&w, ".pause.flag"), "null");
    assert_eq!(sr(&w, ".pause.label"), "true");
    assert_eq!(hc(&w, ".checks.pause.flag"), "null");
    has(&r.out, "pause     label: tehas:pause on the control issue");
    lacks(&r.out, "pause     flag");
}

#[test]
fn neither_switch_set_no_pause_line_and_a_pause_object_that_says_so() {
    let w = World::new("pause-none");
    let r = status_and_summary(&w);
    assert_eq!(sr(&w, ".paused"), "false");
    assert_eq!(
        sc(&w, ".pause"),
        r#"{"flag":null,"label":false,"lifted":false}"#
    );
    assert_eq!(
        hc(&w, ".checks.pause"),
        r#"{"flag":null,"label":false,"lifted":false}"#
    );
    lacks(&r.out, "pause     ");
    assert_eq!(hr(&w, ".status"), "pass");
}

// The label works one way: its absence is a resume only when an allowed
// account took it off. The reporting says what the poll acts on.
#[test]
fn a_pause_a_stranger_took_off_is_published_as_paused_with_a_line_of_its_own() {
    let w = World::new("pause-stranger");
    pause_events(&w, "unlabeled", "tehas-bot");
    let r = status_and_summary(&w);
    assert_eq!(sr(&w, ".paused"), "true");
    assert_eq!(
        sc(&w, ".pause"),
        r#"{"flag":null,"label":false,"lifted":true}"#
    );
    assert_eq!(
        hc(&w, ".checks.pause"),
        r#"{"flag":null,"label":false,"lifted":true}"#
    );
    assert_eq!(hr(&w, ".status"), "warn");
    assert_eq!(hr(&w, ".output"), "The factory is paused.");
    has(
        &r.out,
        "pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off",
    );
    lacks(&r.out, "pause     label: tehas:pause on the control issue");
}

#[test]
fn a_pause_an_allowed_account_took_off_is_not_a_pause_and_a_pause_event_with_no_account_is() {
    let w = World::new("pause-allowed");
    pause_events(&w, "unlabeled", "alice");
    w.status();
    assert_eq!(sr(&w, ".pause.lifted"), "false");
    for actor in ["", "mallory"] {
        pause_events(&w, "unlabeled", actor);
        w.status();
        assert_eq!(sr(&w, ".pause.lifted"), "true", "{actor:?}");
    }
}

// The label is gone and the last pause event only applied it: the label
// object was deleted or renamed, and no account was asked to lift the pause.
#[test]
fn a_pause_no_event_took_off_is_published_as_paused() {
    let w = World::new("pause-labeled");
    pause_events(&w, "labeled", "mallory");
    let r = status_and_summary(&w);
    assert_eq!(sr(&w, ".paused"), "true");
    assert_eq!(sr(&w, ".pause.lifted"), "true");
    assert_eq!(hr(&w, ".status"), "warn");
    has(
        &r.out,
        "pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off",
    );
}

#[test]
fn labels_that_cannot_be_read_take_the_pause_from_its_events_then_from_the_file_before() {
    let w = World::new("pause-unread");
    let on = r#"{"flag":null,"label":true,"lifted":false}"#;
    w.fx("paused", "true\n");
    w.status();
    assert_eq!(sc(&w, ".pause"), on);
    w.touch("control-down");
    pause_events(&w, "labeled", "alice");
    w.status();
    assert_eq!(sc(&w, ".pause"), on);
    assert_eq!(sc(&w, ".stale"), "null");
    w.rm_fx("events.json");
    let r = status_and_summary(&w);
    assert_eq!(sr(&w, ".paused"), "true");
    assert_eq!(sc(&w, ".pause"), on);
    assert_eq!(sc(&w, ".stale"), r#"["pause"]"#);
    assert_eq!(hr(&w, ".checks.pause.label"), "true");
    has(&r.out, "pause     label: tehas:pause on the control issue");
    // A run that reads the labels again says what it read.
    w.rm_fx("control-down");
    w.fx("paused", "false\n");
    w.fx("events.json", "[]\n");
    w.status();
    assert_eq!(sr(&w, ".paused"), "false");
    assert_eq!(sc(&w, ".stale"), "null");
}

#[test]
fn a_flag_that_cannot_be_read_still_holds_the_factory_paused() {
    let w = World::new("pause-bad-flag");
    w.home_file("state/pause.json", "not a flag\n");
    let r = status_and_summary(&w);
    assert_eq!(sr(&w, ".paused"), "true");
    assert_eq!(sc(&w, ".pause.flag"), r#"{"at":null,"reason":null}"#);
    assert_eq!(hr(&w, ".status"), "warn");
    has(&r.out, "pause     flag, written at an unknown time");
}

#[test]
fn summary_a_failed_cycle_is_exit_3_and_still_printed() {
    let w = World::new("failed");
    w.cycle(false, "aborted", "develop");
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "factory   fail: The last cycle failed: #12 ended as aborted.",
    );
    has(&r.out, "cycle     #12 aborted at step develop");
}

#[test]
fn summary_two_running_cycles_are_both_named_each_on_a_line() {
    let w = World::new("two-running");
    w.one(12, true, "running", "develop", None, None);
    w.one(14, true, "running", "plan", None, None);
    let r = status_and_summary(&w);
    ok(&r);
    has(&r.out, "factory   pass: Working on #12, #14.");
    has(&r.out, "cycle     #12 running, step develop");
    has(&r.out, "cycle     #14 running, step plan");
    assert_eq!(
        hc(&w, "[.checks.cycle.issue, (.checks.cycles | map(.issue))]"),
        "[12,[12,14]]"
    );
}

#[test]
fn summary_a_planning_turn_in_flight_has_its_own_line() {
    let w = World::new("plan-turn");
    let lock = turn(&w, 7, 1, 720);
    let r = status_and_summary(&w);
    ok(&r);
    has(&r.out, "plan      #7 turn 2 running, started 12 min ago");
    assert_eq!(hc(&w, ".checks.plans | map([.issue, .turn])"), "[[7,2]]");
    let r = w.summary(&["--json"]);
    assert_eq!(
        jq(".planning | map([.issue, .turn, (.since | type)])", &r.out),
        r#"[[7,2,"string"]]"#
    );
    drop(lock);
}

#[test]
fn summary_a_turn_whose_lock_is_free_or_that_left_its_status_is_not_shown_as_running() {
    let w = World::new("plan-free");
    drop(turn(&w, 7, 1, 720));
    lacks(&status_and_summary(&w).out, "plan      ");
    assert_eq!(jq(".planning", &w.summary(&["--json"]).out), "[]");
    let lock = hold(&w, "plan-7.json.lock");
    w.home_file("state/plan-7.json.rc", "0\n");
    lacks(&status_and_summary(&w).out, "plan      ");
    drop(lock);
    // A session that runs no turn on record, whatever holds the lock.
    let _ = w.store(&[
        "plan-end",
        "7",
        "--turn",
        "2",
        "--outcome",
        "finished",
        "--rc",
        "0",
        "--session",
        "abc",
    ]);
    let _ = fs::remove_file(w.home.join("state/plan-7.json.rc"));
    let lock = hold(&w, "plan-7.json.lock");
    lacks(&status_and_summary(&w).out, "plan      ");
    drop(lock);
}

#[test]
fn health_a_store_that_cannot_list_the_planning_sessions_writes_no_health_file() {
    let w = World::new("health-store");
    w.status();
    assert!(w.live_file("health.json").exists());
    fs::remove_file(w.live_file("health.json")).unwrap();
    w.store_answers("plan-show", 1, "store: busy #0: the database is locked", 0);
    let r = w.health();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "the planning sessions could not be read from the state store",
    );
    assert!(!w.live_file("health.json").exists());
    w.store_answers(
        "plan-show",
        78,
        "store: corrupt: the file is not a database",
        0,
    );
    let r = w.health();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(!w.live_file("health.json").exists());
    fs::remove_file(w.home.join("libexec/store")).unwrap();
    let r = w.health();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "ERROR: the state store is not installed (run install.sh)",
    );
}

#[test]
fn health_no_console_beside_the_command_is_78_and_says_to_install() {
    // The wrapper's own answer, as its siblings give it: the file would
    // otherwise age in silence, which is the failure the manual names for a
    // stopped dispatcher. The host where install.sh put no console beside the
    // command: no console in the directory of the copy, and no name in the
    // environment.
    let w = World::new("no-console");
    let dir = w.t().join("no-console");
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::create_dir_all(dir.join("libexec")).unwrap();
    fs::copy(root().join("bin/tehasbot"), dir.join("bin/tehasbot")).unwrap();
    let _ = fs::remove_file(w.t().join("bin/console"));
    w.set_env(&["TEHAS_CONSOLE=", "TEHAS_CONSOLE=", "PATH=/usr/bin:/bin"]);
    let r = w.sh(&format!(
        "unset TEHAS_CONSOLE; TEHAS_HOME={} {}/bin/tehasbot health",
        w.home.display(),
        dir.display()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!(
            "tehasbot: no console at {}/libexec/console: run install.sh",
            dir.display()
        ),
    );
    assert!(!w.live_file("health.json").exists());
}

#[test]
fn summary_the_last_cycle_failed_and_the_cycle_beside_it_started_earlier_still_failed() {
    let w = World::new("failed-beside");
    w.one(14, true, "running", "plan", Some(600), Some(5));
    w.one(12, false, "aborted", "develop", Some(700), Some(180));
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(
        &r.out,
        "factory   fail: The last cycle failed: #12 ended as aborted.",
    );
}

#[test]
fn summary_a_cycle_that_started_after_the_last_one_failed_clears_the_failure() {
    let w = World::new("failed-cleared");
    w.one(14, true, "running", "plan", Some(60), Some(5));
    w.one(12, false, "aborted", "develop", Some(700), Some(180));
    let r = status_then_summary(&w);
    ok(&r);
    has(&r.out, "factory   pass: Working on #14.");
}

#[test]
fn a_cycle_that_stopped_writing_is_lost_and_the_factory_says_so() {
    let w = World::new("lost");
    w.one(9, true, "running", "develop", Some(900), Some(300));
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "The last cycle failed: #9 ended as lost.");
    assert_eq!(
        jq(
            "[(.cycles | length), .last.status]",
            &live_text(&w, "running.json")
        ),
        r#"[0,"lost"]"#
    );
}

#[test]
fn a_cycle_that_stopped_writing_while_its_lock_is_held_stays_running_a_free_lock_is_lost() {
    let w = World::new("lost-locked");
    w.one(9, true, "running", "develop", Some(900), Some(300));
    let lock = hold(&w, "cycles/9.lock");
    let r = status_then_summary(&w);
    ok(&r);
    has(&r.out, "factory   pass: Working on #9.");
    assert_eq!(jq_r(".status", &live_text(&w, "cycles/9.json")), "running");
    drop(lock);
    let r = status_then_summary(&w);
    assert_eq!(r.rc, 3, "{}", r.out);
    has(&r.out, "The last cycle failed: #9 ended as lost.");
}

#[test]
fn an_abort_a_merge_overtook_ten_seconds_later_passes_and_checks_failed_still_names_it() {
    let w = World::new("overtaken");
    w.status();
    ended(&w, 12, "aborted", 900, 20);
    w.health();
    assert_eq!(hr(&w, ".status"), "fail");
    ended(&w, 13, "merged", 800, 10);
    w.health();
    assert_eq!(hr(&w, ".status"), "pass");
    assert!(!w.live_file("cycles/12.json").exists());
    assert_eq!(
        hc(&w, "[.checks.failed[] | [.issue, .status, .step]]"),
        r#"[[12,"aborted","develop"]]"#
    );
    assert_eq!(
        hr(
            &w,
            ".checks.failed[0].time | fromdateiso8601 | (now - .) < 60"
        ),
        "true"
    );
}

#[test]
fn two_endings_that_arrive_together_are_both_recorded() {
    let w = World::new("two-endings");
    w.status();
    ended(&w, 12, "killed", 900, 20);
    ended(&w, 13, "merged", 800, 10);
    w.health();
    assert_eq!(hr(&w, ".status"), "pass");
    assert_eq!(hc(&w, "[.checks.failed[].issue]"), "[12]");
    assert_eq!(jq("[.[].issue]", &live_text(&w, "endings.json")), "[12,13]");
}

#[test]
fn the_endings_keep_the_last_ten_and_a_failure_eleven_endings_ago_is_gone() {
    let w = World::new("endings-ten");
    w.status();
    ended(&w, 20, "aborted", 900, 300);
    w.health();
    for n in 21..=30 {
        ended(&w, n, "merged", 800, 300 - n as i64);
        w.health();
    }
    assert_eq!(jq("length", &live_text(&w, "endings.json")), "10");
    assert_eq!(hc(&w, ".checks.failed"), "[]");
    let names: Vec<String> = fs::read_dir(w.live_file("cycles"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["30.json"]);
}

#[test]
fn an_endings_file_that_is_not_a_list_is_written_again() {
    let w = World::new("endings-bad");
    w.status();
    fs::write(w.live_file("endings.json"), "{\"not\":\"a list\"}\n").unwrap();
    ended(&w, 12, "aborted", 900, 20);
    w.health();
    assert_eq!(jq("[.[].issue]", &live_text(&w, "endings.json")), "[12]");
    assert_eq!(hc(&w, "[.checks.failed[].issue]"), "[12]");
}
