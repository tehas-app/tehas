//! apply, against stubs of gh, the store and docker that write down each
//! call and fail the ones a test names.

use super::*;
use crate::read::run::Calls;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A stub answers each call, writes `<name> <args>` and its standard input
/// to calls.log, and exits 1 when a line of fail-<name> is in its args.
const STUB: &str = r#"#!/bin/bash
d=$(dirname "$0")
name=$(basename "$0")
input=""
case $name in gh) input=$(cat) ;; esac
printf '%s %s%s\n' "$name" "$*" "${input:+ <<< $input}" >>"$d/calls.log"
if [ -f "$d/fail-$name" ]; then
  while IFS= read -r line; do
    p=${line%%|*} w=${line#*|}
    [ "$w" != "$line" ] || w=busy
    case "$* $input" in *"$p"*)
      case "$p" in slow*) sleep 5 ;; esac
      case "$*" in *DELETE*) echo "gh: Not Found (HTTP 404)" >&2 ;; *) echo "store: $w: stub" >&2 ;; esac
      case $w in absent | unreadable) exit 78 ;; esac
      exit 1 ;;
    esac
  done <"$d/fail-$name"
fi
case "$name $1" in
  "store journal-record") n=$(($(cat "$d/n" 2>/dev/null || echo 0) + 1)); echo "$n" >"$d/n"
    echo "{\"api\":1,\"verb\":\"journal-record\",\"ok\":true,\"action\":$n}" ;;
  "store cycle-show") a=0; [ ! -f "$d/started" ] || a=1
    echo "{\"api\":1,\"verb\":\"cycle-show\",\"ok\":true,\"cycle\":{\"attempt\":$a}}" ;;
  "store park-show") echo "{\"api\":1,\"verb\":\"park-show\",\"ok\":true,\"park\":$(cat "$d/park" 2>/dev/null || echo null)}" ;;
  "store approval-record")
    while [ $# -gt 0 ]; do [ "$1" != --input-file ] || cp "$2" "$d/input.json"; shift; done
    echo '{"api":1,"verb":"approval-record","ok":true}' ;;
  "start "*) : >"$d/started" ;;
  "status "*) echo "the status line" ;;
  "store plan-show") cat "$d/plan" 2>/dev/null || echo '{"api":1,"verb":"plan-show","ok":true,"planning":null}' ;;
  "store plan-end") case "$*" in
      *--turn*) if [ -f "$d/plan" ]; then sed 's/"verb":"plan-show"/"verb":"plan-end"/' "$d/plan"; else echo '{"api":1,"verb":"plan-end","ok":true}'; fi ;;
      *) echo '{"api":1,"verb":"plan-end","ok":true}' ;;
    esac ;;
  "store cycle-note") cat "$d/note" 2>/dev/null || echo '{"api":1,"verb":"cycle-note","ok":true,"times":0,"stop_asked_at":0}' ;;
  "store plan-note") echo "{\"api\":1,\"verb\":\"plan-note\",\"ok\":true,\"said\":$(cat "$d/said" 2>/dev/null || echo true)}" ;;
  "store "*) echo "{\"api\":1,\"verb\":\"$1\",\"ok\":true}" ;;
  "docker rm") echo "$3" ;;
  "gh issue") case "$2 $*" in
      "view "*"--json labels"*) cat "$d/labels-$3" 2>/dev/null || echo '{"labels":[]}' ;;
      "view "*"--json body"*) cat "$d/body-$3" 2>/dev/null || echo '{"body":""}' ;;
    esac ;;
  "gh api") case "$*" in *"-X POST"*/comments*) echo '{"id":99}' ;; esac
    case "$2" in
      repos/*/issues/[0-9]*) case "$2" in */comments*) ;; *) echo '{"id":555}' ;; esac ;;
    esac
    case "$2" in
      */issues/[0-9]*/comments*) f=${2#*/issues/}; f=$d/comments-${f%%/*}
        if [ -f "$f" ]; then cat "$f"; else cat "$d/comments" 2>/dev/null || echo '[]'; fi ;;
    esac ;;
  "gh pr") cat "$d/prs" 2>/dev/null || echo '[]' ;;
  "analytics "*) printf '%s\n' "$*" >>"$d/events"
    while [ $# -gt 0 ]; do
      [ "$1" != --facts ] || { cp "$2" "$d/facts.json"; stat -c %a "$2" >"$d/facts.mode"; }
      shift
    done ;;
esac
"#;

struct World {
    dir: PathBuf,
    w: Writer,
}

fn world() -> World {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "tehas-apply-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("home/state/refresh")).unwrap();
    // One stub file for the process, written before any test runs it, and
    // a link to it in each world: a file written while another thread forks
    // can be held open for writing and refuse to run.
    static ONCE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let stub = ONCE.get_or_init(|| {
        let p = std::env::temp_dir().join(format!("tehas-apply-stub-{}", std::process::id()));
        fs::write(&p, STUB).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p
    });
    for name in ["gh", "store", "docker", "start", "status", "analytics"] {
        std::os::unix::fs::symlink(stub, dir.join(name)).unwrap();
    }
    let s = |n: &str| dir.join(n).to_string_lossy().into_owned();
    let w = Writer {
        repo: "example-owner/example-product".into(),
        home: dir.join("home"),
        gh: s("gh"),
        docker: s("docker"),
        calls: Calls {
            bound: Duration::from_secs(10),
            store_bound: Duration::from_secs(10),
            gh: s("gh"),
            store: s("store"),
            gh_count: 0,
        },
        poll_at: 1000,
        before: 990,
        bound: Duration::from_secs(10),
        host: start::Host {
            name: "tehas-tehas".into(),
            comments: false,
            allowed: vec!["alice".into()],
            millwright: "mill-bot".into(),
            bot: "tehas-bot".into(),
            systemctl: s("start"),
            start_cmd: Some(s("start")),
            stop_cmd: Some(s("start")),
            start_wait: Duration::from_secs(2),
            status_cmd: s("status"),
            analytics: None,
            analytics_wait: Duration::from_secs(5),
            work_dir: dir.join("works/tehas-work").to_string_lossy().into_owned(),
            user_home: dir.join("user").to_string_lossy().into_owned(),
            done_wait: Duration::from_secs(5),
            image: "tehas-sandbox:test".into(),
        },
        ended: None,
    };
    World { dir, w }
}

impl World {
    fn fail(&self, name: &str, pattern: &str) {
        let p = self.dir.join(format!("fail-{name}"));
        let mut s = fs::read_to_string(&p).unwrap_or_default();
        s.push_str(pattern);
        s.push('\n');
        fs::write(p, s).unwrap();
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The gh and docker calls, without the journal's.
    fn writes(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| !c.starts_with("store journal-"))
            .collect()
    }

    fn apply(&mut self, actions: Value) -> Applied {
        self.w.apply(actions.as_array().unwrap())
    }
}

fn label() -> Value {
    json!({"issue": 7, "remove": ["tehas:ready"], "add": ["needs-human-attention"],
        "fail": "WARN: label change on #7 failed: --remove-label tehas:ready --add-label needs-human-attention",
        "log": "clear #7: done", "say": "Tehas took it off."})
}

#[test]
fn a_label_change_that_worked_is_followed_by_its_line_and_comment() {
    let mut t = world();
    let a = t.apply(json!([label()]));
    assert_eq!(a.log, vec!["clear #7: done"]);
    assert!(a.left.is_empty());
    assert_eq!(
        t.writes(),
        vec![
            "gh issue edit 7 --repo example-owner/example-product --add-label needs-human-attention --remove-label tehas:ready",
            "gh issue comment 7 --repo example-owner/example-product --body-file - <<< Tehas took it off.",
        ]
    );
}

#[test]
fn a_failed_label_change_logs_its_line_and_says_nothing() {
    let mut t = world();
    t.fail("gh", "issue edit");
    let a = t.apply(json!([label()]));
    assert_eq!(
        a.log,
        vec!["WARN: label change on #7 failed: --remove-label tehas:ready --add-label needs-human-attention"]
    );
    assert!(!t.calls().iter().any(|c| c.contains("issue comment")));
}

#[test]
fn every_write_is_journaled_before_and_settled_after() {
    let mut t = world();
    t.fail("gh", "issue comment");
    let a = t.apply(json!([label()]));
    assert_eq!(a.log, vec!["clear #7: done", "WARN: comment on #7 failed"]);
    let c = t.calls();
    let steps: Vec<&str> = c
        .iter()
        .map(|l| l.split(' ').take(2).collect::<Vec<_>>().join(" "))
        .map(|s| match s.as_str() {
            "store journal-record" => "record",
            "store journal-attempt" => "attempt",
            "store journal-settle" => "settle",
            "gh issue" => "gh",
            _ => "other",
        })
        .collect();
    assert_eq!(
        steps,
        vec!["record", "attempt", "gh", "settle", "record", "attempt", "gh", "settle"]
    );
    assert!(c[0].contains("--poll 1000 --kind label-add --issue 7"));
    assert!(c[0].contains("--before 990"));
    assert!(c[3].ends_with("--action 1 --result ok"));
    assert!(c[4].contains("--kind comment-post"));
    assert!(c[7].ends_with("--action 2 --result read-failed"));
}

#[test]
fn a_write_the_journal_cannot_record_is_not_made() {
    let mut t = world();
    t.fail("store", "journal-record");
    let a = t.apply(json!([label()]));
    assert_eq!(
        a.log,
        vec!["WARN: the journal could not record a write on #7, so it was not made"]
    );
    assert!(t.writes().is_empty());
}

#[test]
fn an_answered_question_is_recorded_after_its_label_and_retried_when_busy() {
    let mut t = world();
    let act = json!([{"issue": 8, "remove": ["tehas:question"], "fail": "x",
        "answered": {"id": 41, "by": "alice"}}]);
    assert!(t.apply(act.clone()).log.is_empty());
    assert!(t
        .calls()
        .contains(&"store question-answered 8 --id 41 --by alice".to_string()));
    let mut t = world();
    t.fail("store", "question-answered");
    let a = t.apply(act);
    assert_eq!(
        a.log,
        vec!["WARN: could not record the answer to the question of #8 (busy)"]
    );
    assert_eq!(
        t.calls()
            .iter()
            .filter(|c| c.starts_with("store question-answered"))
            .count(),
        3
    );
}

#[test]
fn an_untick_is_followed_by_its_note_only_when_it_worked() {
    let mut t = world();
    let act = json!([{"untick": 9, "comment": 55, "body": "- [ ] Retry", "say": "Not now.",
        "fail": "WARN: could not untick the question on #9"}]);
    t.apply(act.clone());
    assert_eq!(
        t.writes(),
        vec![
            r#"gh api -X PATCH repos/example-owner/example-product/issues/comments/55 --input - <<< {"body":"- [ ] Retry"}"#,
            "gh issue comment 9 --repo example-owner/example-product --body-file - <<< Not now.",
        ]
    );
    let mut t = world();
    t.fail("gh", "PATCH");
    assert_eq!(
        t.apply(act).log,
        vec!["WARN: could not untick the question on #9"]
    );
    assert!(!t.calls().iter().any(|c| c.contains("issue comment")));
}

#[test]
fn a_pull_request_without_the_mark_is_unmarked_already() {
    let act = json!([{"pr": 12, "remove": ["tehas:refreshing"], "log": "unmark #12"}]);
    let mut t = world();
    assert_eq!(t.apply(act.clone()).log, vec!["unmark #12"]);
    // GitHub's Not Found: the mark is not there, which is what was wanted.
    let mut t = world();
    t.fail("gh", "DELETE");
    assert_eq!(t.apply(act.clone()).log, vec!["unmark #12"]);
    assert!(t.calls().last().unwrap().ends_with("--result empty"));
}

#[test]
fn a_call_that_does_not_answer_in_its_bound_is_timed_out() {
    let mut t = world();
    t.fail("gh", "slow");
    t.w.bound = Duration::from_secs(1);
    let a = t.apply(json!([{"issue": 7, "say": "slow words"}]));
    assert_eq!(a.log, vec!["WARN: comment on #7 failed"]);
    assert!(t.calls().last().unwrap().ends_with("--result timed-out"));
}

#[test]
fn a_write_whose_attempt_is_not_recorded_is_not_made() {
    let mut t = world();
    t.fail("store", "journal-attempt");
    let a = t.apply(json!([{"issue": 7, "say": "words"}]));
    assert_eq!(
        a.log,
        vec!["WARN: the journal could not record a write on #7, so it was not made"]
    );
    assert!(t.writes().is_empty());
}

#[test]
fn the_journal_holds_a_body_by_its_sum_and_length_only() {
    let mut t = world();
    t.apply(json!([{"issue": 7, "say": "a secret-free but long body"}]));
    let record = &t.calls()[0];
    assert!(!record.contains("long body"));
    assert!(record.contains(r#""bytes":27"#));
    assert!(record.contains(r#""sha256":""#));
}

#[test]
fn an_untick_logs_the_line_its_site_gave() {
    let mut t = world();
    t.fail("gh", "PATCH");
    let a = t.apply(json!([{"untick": 9, "comment": 55, "body": "x",
        "fail": "WARN: could not untick the retry on #9; its question is no longer on record, so it starts nothing more"}]));
    assert_eq!(
        a.log,
        vec!["WARN: could not untick the retry on #9; its question is no longer on record, so it starts nothing more"]
    );
}

#[test]
fn the_said_and_told_records_are_written_whatever_the_comment_did() {
    let mut t = world();
    t.fail("gh", "issue comment");
    let a = t.apply(json!([
        {"issue": 3, "refresh_said": "conflict:abc", "say": "Not refreshed."},
        {"issue": 4, "park_told": true, "say": "Not started."},
    ]));
    assert_eq!(
        a.log,
        vec!["WARN: comment on #3 failed", "WARN: comment on #4 failed"]
    );
    assert_eq!(
        fs::read_to_string(t.w.home.join("state/refresh/3.said")).unwrap(),
        "conflict:abc"
    );
    assert!(t.calls().contains(&"store park-told 4".to_string()));
}

#[test]
fn a_plan_note_says_its_words_only_the_first_time() {
    let act = json!([{"plan_note": 5, "kind": "held", "id": 77, "say": "A person ticks it."}]);
    let mut t = world();
    t.apply(act.clone());
    assert!(t
        .calls()
        .contains(&"store plan-note 5 --kind held --id 77".to_string()));
    assert!(t
        .calls()
        .iter()
        .any(|c| c.starts_with("gh issue comment 5")));
    let mut t = world();
    fs::write(t.dir.join("said"), "false").unwrap();
    t.apply(act);
    assert!(!t.calls().iter().any(|c| c.starts_with("gh issue comment")));
}

#[test]
fn the_files_of_the_hold_the_gate_and_the_branch_check() {
    let mut t = world();
    let h = t.w.home.clone();
    fs::write(h.join("state/refresh/6.json"), "{}").unwrap();
    t.apply(json!([
        {"refresh_drop": 6},
        {"refresh_held": "hold refreshes: busy"},
        {"gate_held": true},
        {"branch_note": "main is red"},
    ]));
    assert!(!h.join("state/refresh/6.json").exists());
    assert_eq!(
        fs::read_to_string(h.join("state/refresh/.held")).unwrap(),
        "hold refreshes: busy"
    );
    assert!(h.join("state/low-gate.held").exists());
    assert_eq!(
        fs::read_to_string(h.join("state/branch-check")).unwrap(),
        "main is red\n"
    );
    let a = t.apply(json!([{"refresh_held": ""}, {"gate_held": false}, {"gate_held": false}]));
    assert!(a.log.is_empty());
    assert!(!h.join("state/refresh/.held").exists());
    assert!(!h.join("state/low-gate.held").exists());
    assert!(t.calls().is_empty());
}

#[test]
fn a_container_removed_is_said_and_one_not_there_is_not() {
    let mut t = world();
    let a = t.apply(json!([{"reap": "tehas-work-5", "why": "no cycle holds the lock of #5"}]));
    assert_eq!(
        a.log,
        vec!["removed the container tehas-work-5: no cycle holds the lock of #5"]
    );
    let mut t = world();
    t.fail("docker", "rm");
    assert!(t
        .apply(json!([{"reap": "tehas-work-5", "why": "x"}]))
        .log
        .is_empty());
}

#[test]
fn the_actions_of_the_later_parts_are_left_in_order() {
    let mut t = world();
    let turn = json!({"plan_turn": 8, "kind": "first", "label": "tehas:plan", "by": "alice"});
    let spend = json!({"park_retry": 9});
    let a = t.apply(json!([turn, {"gate_held": true}, spend]));
    assert_eq!(a.left, vec![turn, spend]);
}

fn start_of(n: u64, given: &str) -> Value {
    json!({"start": n, "by": "alice", "how": "`tehas:ready` label", "at": "2026-10-26T10:00:00Z",
        "given": given, "remove": ["tehas:ready"], "verb": "labelled", "text": true,
        "body_sha256": "a".repeat(64), "title_sha256": "b".repeat(64),
        "links": ["https://github.com/user-attachments/assets/1"]})
}

/// The calls a test reads, each as its program and first word.
fn steps(t: &World) -> Vec<String> {
    t.writes()
        .iter()
        .map(|c| c.split(' ').take(2).collect::<Vec<_>>().join(" "))
        .collect()
}

#[test]
fn a_start_records_marks_says_and_starts_in_that_order() {
    let mut t = world();
    let a = t.apply(json!([start_of(7, "label")]));
    assert_eq!(a.exit, None);
    assert_eq!(a.log, vec!["start #7 (labelled by alice)"]);
    assert_eq!(a.printed, vec!["the status line"]);
    assert_eq!(
        steps(&t),
        vec![
            "store approval-record",
            "store cycle-show",
            "store park-show",
            "gh issue",
            "gh issue",
            "start 7",
            "store cycle-show",
            "status ",
        ]
    );
    let c = t.writes();
    assert!(c[0].contains("approval-record 7 --at 1793008800 --by alice --how label"));
    assert!(c[0].contains(&format!("--body-sha {} ", "a".repeat(64))));
    assert!(c[0].contains("--label tehas:ready"));
    assert!(c[0].contains(&format!("--title-sha {}", "b".repeat(64))));
    assert_eq!(c[3], "gh issue edit 7 --repo example-owner/example-product --add-label tehas:running --remove-label tehas:ready");
    assert!(c[4].contains("Tehas cycle started on `tehas-tehas` at "));
    assert!(c[4].contains("(authorized by @alice's `tehas:ready` label)"));
    let input: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("input.json")).unwrap()).unwrap();
    assert_eq!(
        input,
        json!({"links": ["https://github.com/user-attachments/assets/1"]})
    );
    // The input file goes once the store has it.
    assert_eq!(fs::read_dir(t.w.home.join("tmp")).unwrap().count(), 0);
}

#[test]
fn an_approval_the_store_refuses_starts_nothing() {
    let mut t = world();
    t.fail("store", "approval-record 7|busy");
    let a = t.apply(json!([start_of(7, "label"), start_of(8, "label")]));
    assert_eq!(a.exit, Some(1));
    assert_eq!(
        &a.log[..2],
        &[
            "start #7 (labelled by alice)",
            "ERROR: could not write the approval record of #7 (busy), not starting a cycle"
        ]
    );
    assert!(!t.writes().iter().any(|c| c.contains("issue edit 7")));
    // The issue after it still starts: the bash goes on to the next one.
    assert!(t.writes().iter().any(|c| c.contains("issue edit 8")));
}

#[test]
fn an_approval_that_started_a_cycle_already_is_skipped() {
    let mut t = world();
    t.fail("store", "approval-record|used");
    let a = t.apply(json!([start_of(7, "label")]));
    assert_eq!(a.exit, None);
    assert_eq!(
        a.log,
        vec![
            "start #7 (labelled by alice)",
            "skip #7: its approval of 2026-10-26T10:00:00Z started a cycle already"
        ]
    );
}

#[test]
fn a_store_that_cannot_be_used_ends_the_poll_with_78_before_any_mark() {
    let mut t = world();
    t.fail("store", "approval-record|absent");
    let a = t.apply(json!([start_of(7, "label"), {"gate_held": true}]));
    assert_eq!(a.exit, Some(78));
    assert!(a.log[1].starts_with(
        "ERROR: the state store cannot be used, not starting a cycle for #7: store: absent"
    ));
    assert!(a.printed.is_empty());
    assert!(!t.w.home.join("state/low-gate.held").exists());
}

#[test]
fn a_mark_that_fails_takes_the_approval_back_and_starts_nothing() {
    let mut t = world();
    t.fail("gh", "--add-label tehas:running");
    let a = t.apply(json!([start_of(7, "label")]));
    assert_eq!(a.exit, Some(1));
    assert_eq!(
        a.log.last().unwrap(),
        "ERROR: could not mark #7 as running, not starting a cycle"
    );
    assert!(t
        .writes()
        .iter()
        .any(|c| c.starts_with("store approval-record 7 --undo --at 1793008800")));
    assert!(!t.calls().iter().any(|c| c.starts_with("start ")));
}

#[test]
fn a_unit_that_never_says_it_started_is_put_back_and_said() {
    let mut t = world();
    t.fail("start", "7");
    let a = t.apply(json!([start_of(7, "label")]));
    assert_eq!(a.exit, Some(1));
    assert_eq!(
        a.log.last().unwrap(),
        "ERROR: the cycle for #7 did not start"
    );
    let w = t.writes();
    assert!(w
        .iter()
        .any(|c| c.starts_with("store approval-record 7 --undo")));
    assert!(w.iter().any(|c| c == "gh issue edit 7 --repo example-owner/example-product --remove-label tehas:running --add-label needs-human-attention"));
    assert!(w.last().unwrap().starts_with("status"));
    assert!(w
        .iter()
        .any(|c| c.contains("`tehas-cycle@7.service` did not come up")));
}

#[test]
fn a_retry_unticks_only_after_its_start_worked() {
    let untick = json!({"untick": 7, "comment": 55, "body": "- [ ] Retry", "fail": "x"});
    let mut retry = start_of(7, "retry-tick");
    retry["comment"] = json!(55);
    let mut t = world();
    let a = t.apply(json!([retry.clone(), untick.clone()]));
    assert_eq!(a.exit, None);
    assert!(t.writes().iter().any(|c| c.contains("PATCH")));
    assert!(t.writes()[0].contains("--comment-id 55"));
    assert!(!t.writes()[0].contains("--label"));
    let mut t = world();
    t.fail("start", "7");
    let a = t.apply(json!([retry, untick]));
    assert_eq!(a.exit, Some(1));
    assert!(!t.writes().iter().any(|c| c.contains("PATCH")));
}

#[test]
fn a_start_takes_its_park_question_off_and_spends_the_retry_after_the_mark() {
    let mut t = world();
    fs::write(t.dir.join("park"), r#"{"question": 41}"#).unwrap();
    t.apply(json!([start_of(7, "label")]));
    let s = steps(&t);
    let w = t.writes();
    let withdraw = w
        .iter()
        .position(|c| c == "store question-answered 7 --id 41 --withdraw")
        .unwrap();
    let mark = w
        .iter()
        .position(|c| c.contains("--add-label tehas:running"))
        .unwrap();
    let spend = s.iter().position(|c| c == "store park-retry").unwrap();
    assert!(withdraw < mark && mark < spend);
    // A mark that fails puts the question back.
    let mut t = world();
    fs::write(t.dir.join("park"), r#"{"question": 41}"#).unwrap();
    t.fail("gh", "--add-label tehas:running");
    t.apply(json!([start_of(7, "label")]));
    assert!(t
        .writes()
        .iter()
        .any(|c| c == "store question-asked 7 --id 41 --kind park"));
}

#[test]
fn the_approval_names_the_comments_an_allowed_account_wrote_before_it() {
    let mut t = world();
    t.w.host.comments = true;
    let comments = r#"[{"id": 1, "body": "see https://github.com/user-attachments/assets/2", "user": {"login": "alice"},
        "created_at": "2026-10-26T09:00:00Z", "updated_at": "2026-10-26T09:00:00Z"},
      {"id": 2, "body": "https://github.com/user-attachments/assets/3", "user": {"login": "mallory"},
        "created_at": "2026-10-26T09:00:00Z", "updated_at": "2026-10-26T09:00:00Z"},
      {"id": 3, "body": "later https://github.com/user-attachments/assets/4", "user": {"login": "alice"},
        "created_at": "2026-10-26T11:00:00Z", "updated_at": "2026-10-26T11:00:00Z"}]"#;
    fs::write(t.dir.join("comments"), comments).unwrap();
    t.apply(json!([start_of(7, "label")]));
    let input: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("input.json")).unwrap()).unwrap();
    assert_eq!(
        input["comment_links"],
        json!([{"link": "https://github.com/user-attachments/assets/2", "source": "comment:1", "by": "alice"}])
    );
}

#[test]
fn a_start_with_no_text_read_records_nothing() {
    let mut t = world();
    let mut a = start_of(7, "label");
    a["text"] = json!(false);
    let r = t.apply(json!([a]));
    assert_eq!(
        r.log[1],
        "ERROR: could not write the approval record of #7, not starting a cycle"
    );
    assert!(!t
        .calls()
        .iter()
        .any(|c| c.starts_with("store approval-record")));
}

#[test]
fn a_refresh_marks_its_pull_request_first_and_its_approval_names_what_it_refreshes() {
    let mut t = world();
    let h = t.w.home.clone();
    fs::write(h.join("state/refresh/7.json"), "{}").unwrap();
    let mut a = start_of(7, "refresh");
    a["how"] = json!("approval");
    a["refresh_pr"] = json!(12);
    a["refresh_url"] = json!("https://github.com/example-owner/example-product/pull/12");
    a["refresh_of"] =
        json!({"pr": 12, "head_sha": "c".repeat(40), "base_sha": "d".repeat(40), "branch": "main"});
    t.apply(json!([a.clone()]));
    let w = t.writes();
    assert!(w[0].starts_with("gh api -X POST repos/example-owner/example-product/issues/12/labels -f labels[]=tehas:refreshing"));
    // A refresh is on record at the time the poll decided it.
    assert!(!w[1].contains("--at 1793008800"));
    assert!(!w[1].contains("--label"));
    assert!(w[1].contains(&format!(
        "--refresh-pr 12 --refresh-head {}",
        "c".repeat(40)
    )));
    assert!(!h.join("state/refresh/7.json").exists());
    assert!(w
        .iter()
        .any(|c| c.contains("Tehas refresh started on `tehas-tehas`")
            && c.contains("pull/12 conflicts with `main`")
            && c.contains("rests on @alice's approval")));
    let input: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("input.json")).unwrap()).unwrap();
    assert_eq!(input["refresh_of"]["pr"], 12);
    assert_eq!(input["retry_context"][0]["source"], "refresh");
    assert!(input["retry_context"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("This pull request conflicts with `main`."));
    // A mark that fails records nothing and starts nothing.
    let mut t = world();
    t.fail("gh", "POST");
    let r = t.apply(json!([a]));
    assert_eq!(
        r.log[1],
        "ERROR: could not mark pull request #12 as refreshing, not starting a cycle"
    );
    assert!(!t
        .calls()
        .iter()
        .any(|c| c.starts_with("store approval-record")));
}

#[test]
fn a_time_the_approval_cannot_be_dated_by_is_now() {
    let mut t = world();
    let mut a = start_of(7, "label");
    a["at"] = json!("yesterday");
    t.apply(json!([a]));
    let at: i64 = t.writes()[0]
        .split(" --at ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!((at - now_secs()).abs() < 60);
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn only_a_comment_id_of_its_grammar_is_named() {
    for (id, named) in [
        (json!(55), true),
        (json!("055"), false),
        (json!("12345678901234567"), false),
        (json!("5x"), false),
    ] {
        let mut t = world();
        let mut a = start_of(7, "retry-tick");
        a["comment"] = id.clone();
        t.apply(json!([a]));
        assert_eq!(t.writes()[0].contains("--comment-id"), named, "{id}");
    }
}

#[test]
fn a_retry_names_what_an_allowed_account_said_on_its_pull_requests_since_the_park() {
    let mut t = world();
    t.w.host.comments = true;
    fs::write(
        t.dir.join("prs"),
        r#"[{"number": 30, "headRefName": "feature/issue-7-x", "body": ""},
        {"number": 31, "headRefName": "feature/issue-71", "body": "Closes #71"},
        {"number": 32, "headRefName": "other", "body": "Fixes #7."}]"#,
    )
    .unwrap();
    let c = |id: u64, at: &str, body: &str| json!({"id": id, "body": body, "user": {"login": "alice"}, "created_at": at, "updated_at": at});
    fs::write(
        t.dir.join("comments-30"),
        json!([
            c(1, "2026-10-26T08:00:00Z", "before the park"),
            c(
                2,
                "2026-10-26T09:30:00Z",
                "see https://github.com/user-attachments/assets/9\r\nplease"
            )
        ])
        .to_string(),
    )
    .unwrap();
    fs::write(
        t.dir.join("comments-32"),
        json!([c(3, "2026-10-26T09:40:00Z", &"x".repeat(5000))]).to_string(),
    )
    .unwrap();
    fs::write(
        t.dir.join("comments-31"),
        json!([c(4, "2026-10-26T09:40:00Z", "not this issue's")]).to_string(),
    )
    .unwrap();
    let mut a = start_of(7, "retry-tick");
    a["since"] = json!("2026-10-26T09:00:00Z");
    t.apply(json!([a]));
    assert!(t.calls().iter().any(|c| c.starts_with(
        "gh pr list --repo example-owner/example-product --author tehas-bot --state all"
    )));
    assert!(!t.calls().iter().any(|c| c.contains("issues/31/comments")));
    let input: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("input.json")).unwrap()).unwrap();
    let ctx = input["retry_context"].as_array().unwrap();
    assert_eq!(ctx.len(), 2);
    assert_eq!(ctx[0]["source"], "pr-comment:2");
    assert_eq!(
        ctx[0]["text"],
        "see https://github.com/user-attachments/assets/9\nplease"
    );
    assert_eq!(ctx[1]["text"].as_str().unwrap().len(), 4000);
    assert_eq!(
        input["comment_links"],
        json!([{"link": "https://github.com/user-attachments/assets/9", "source": "pr-comment:2", "by": "alice"}])
    );
    // A list that cannot be read starts nothing.
    let mut t = world();
    t.w.host.comments = true;
    t.fail("gh", "pr list");
    let r = t.apply(json!([start_of(7, "retry-tick")]));
    assert_eq!(
        r.log[1],
        "ERROR: the comments on the pull request of #7 could not be read"
    );
}

#[test]
fn a_millwright_start_names_its_note_of_the_ten_minutes_before_the_label() {
    let mut t = world();
    let c = |id: u64, who: &str, at: &str, body: &str| json!({"id": id, "body": body, "user": {"login": who}, "created_at": at, "updated_at": at});
    let marker = "<!-- tehas:millwright issue=7 action=retry -->";
    fs::write(
        t.dir.join("comments"),
        json!([
            c(
                1,
                "mill-bot",
                "2026-10-26T09:45:00Z",
                &format!("too early {marker}")
            ),
            c(
                2,
                "mill-bot",
                "2026-10-26T09:52:00Z",
                &format!("first {marker}")
            ),
            c(
                3,
                "mill-bot",
                "2026-10-26T09:55:00Z",
                &format!("the one {marker}")
            ),
            c(
                4,
                "alice",
                "2026-10-26T09:56:00Z",
                &format!("a person {marker}")
            ),
            c(5, "mill-bot", "2026-10-26T09:57:00Z", "no marker"),
        ])
        .to_string(),
    )
    .unwrap();
    t.apply(json!([start_of(7, "millwright")]));
    assert!(t
        .writes()
        .iter()
        .any(|c| c.starts_with("store approval-record") && c.contains("--label tehas:ready")));
    let input: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("input.json")).unwrap()).unwrap();
    assert_eq!(
        input["retry_context"],
        json!([{"source": "millwright:3", "by": "mill-bot", "at": "2026-10-26T09:55:00Z", "text": format!("the one {marker}")}])
    );
    assert!(input.get("comment_links").is_none());
}

#[test]
fn a_unit_that_did_not_start_is_stopped_when_its_approval_stays() {
    let mut t = world();
    t.fail("start", "7");
    t.fail("store", "--undo|conflict");
    let a = t.apply(json!([start_of(7, "label")]));
    assert!(a.log.contains(
        &"WARN: could not take the approval of #7 off the record (conflict)".to_string()
    ));
    assert!(a.log.contains(
        &"WARN: could not stop tehas-cycle@7.service: a late start may still use its approval"
            .to_string()
    ));
    let mut t = world();
    t.fail("store", "--undo|conflict");
    fs::write(t.dir.join("fail-start"), "x\n").unwrap();
    t.w.host.start_cmd = Some(t.dir.join("gh").to_string_lossy().into_owned());
    t.fail("gh", "7");
    t.w.host.stop_cmd = Some(t.dir.join("status").to_string_lossy().into_owned());
    let a = t.apply(json!([start_of(8, "label")]));
    assert!(!a.log.iter().any(|l| l.starts_with("WARN: could not stop")));
}

#[test]
fn a_store_unusable_after_a_mark_ends_the_starts_with_1() {
    let mut t = world();
    t.fail("store", "approval-record 8|absent");
    let a = t.apply(json!([
        start_of(7, "label"),
        start_of(8, "label"),
        start_of(9, "label")
    ]));
    assert_eq!(a.exit, Some(1));
    assert!(a
        .log
        .last()
        .unwrap()
        .starts_with("ERROR: the state store cannot be used, not starting a cycle for #8"));
    assert!(!t.calls().iter().any(|c| c.contains("approval-record 9")));
    assert_eq!(a.printed, vec!["the status line"]);
}

#[test]
fn a_start_tells_the_analytics_when_it_was_ready_and_that_it_started() {
    let mut t = world();
    t.w.host.analytics = Some(t.dir.join("analytics").to_string_lossy().into_owned());
    t.apply(json!([start_of(7, "label")]));
    assert_eq!(
        fs::read_to_string(t.dir.join("events")).unwrap(),
        "event 7 ready --at 1793008800\nevent 7 started\n"
    );
    let mut t = world();
    t.w.host.analytics = Some(t.dir.join("analytics").to_string_lossy().into_owned());
    t.fail("analytics", "ready");
    let a = t.apply(json!([start_of(7, "label")]));
    assert!(a.log.contains(&"WARN analytics event: exit 1".to_string()));
}

#[test]
fn a_lock_held_without_a_start_on_record_is_taken_as_started() {
    let mut t = world();
    t.w.host.start_wait = Duration::from_millis(600);
    // The start stub makes no record: a unit that holds its lock and has not
    // said so.
    t.w.host.start_cmd = Some(t.dir.join("status").to_string_lossy().into_owned());
    let cycles = t.w.home.join("state/cycles");
    fs::create_dir_all(&cycles).unwrap();
    let lock = fs::File::create(cycles.join("7.lock")).unwrap();
    lock.lock().unwrap();
    let a = t.apply(json!([start_of(7, "label")]));
    assert_eq!(a.exit, None);
    assert!(a.log.contains(
        &"WARN: the cycle for #7 holds its lock but did not say so in 0s; taking it as started"
            .to_string()
    ));
}

fn quiet_of(n: u64) -> Value {
    json!({"quiet": n, "said_for": 1793000000, "stop": true,
        "log": format!("WARN: predicted line for #{n}")})
}

#[test]
fn a_quiet_cycle_is_noted_then_stopped_as_the_answer_says() {
    let mut t = world();
    let a = t.apply(json!([quiet_of(7)]));
    let line = "WARN: the cycle for #7 holds its lock but has not been heard from since 2026-10-26T07:33:20Z, more than 1200s: stopping tehas-cycle@7.service";
    assert_eq!(a.log, vec![line]);
    assert_eq!(
        t.writes(),
        vec![
            "store cycle-note 7 --kind quiet --said-for 1793000000",
            "store cycle-stop-asked 7",
            "start 7",
        ]
    );
}

#[test]
fn a_quiet_cycle_whose_record_says_wait_is_left_and_one_asked_six_times_is_a_persons() {
    let mut t = world();
    let at = now_secs();
    fs::write(
        t.dir.join("note"),
        format!(r#"{{"api":1,"verb":"cycle-note","ok":true,"times":2,"stop_asked_at":{at}}}"#),
    )
    .unwrap();
    let a = t.apply(json!([quiet_of(7)]));
    assert!(a.log.is_empty());
    assert_eq!(t.writes().len(), 1);
    let mut t = world();
    fs::write(
        t.dir.join("note"),
        r#"{"api":1,"verb":"cycle-note","ok":true,"times":6,"stop_asked_at":1}"#,
    )
    .unwrap();
    let a = t.apply(json!([quiet_of(7)]));
    assert!(a.log[0].ends_with("the poll asked 6 times to stop tehas-cycle@7.service and asks no more: it needs a person, who can stop it with systemctl --user stop tehas-cycle@7.service"));
    assert_eq!(t.writes()[1], "store cycle-stop-asked 7");
    assert!(!t.calls().iter().any(|c| c.starts_with("start ")));
}

#[test]
fn a_quiet_cycle_the_store_cannot_note() {
    let mut t = world();
    t.fail("store", "cycle-note|nocycle");
    let a = t.apply(json!([quiet_of(7)]));
    assert!(a.log[0].ends_with("and the state store has no open record of it, so the poll does not stop it: if it stays, a person can, with tehas stop 7 or systemctl --user stop tehas-cycle@7.service"));
    // A store that cannot be used ends the poll there.
    let mut t = world();
    t.fail("store", "cycle-note|absent");
    let a = t.apply(json!([quiet_of(7), {"gate_held": true}]));
    assert_eq!(a.exit, Some(78));
    assert!(a.log[0].starts_with("ERROR: the state store cannot be used: store: absent"));
    assert!(!t.w.home.join("state/low-gate.held").exists());
}

#[test]
fn a_finished_cycle_is_told_why_and_stopped() {
    let mut t = world();
    fs::create_dir_all(t.w.home.join("state/cycles")).unwrap();
    let a = t.apply(json!([{"stop_done": 7, "why": "merged"}]));
    assert!(a.log.is_empty());
    assert_eq!(
        fs::read_to_string(t.w.home.join("state/cycles/7.done")).unwrap(),
        "merged\n"
    );
    assert!(t.calls().contains(&"start 7".to_string()));
    let mut t = world();
    fs::create_dir_all(t.w.home.join("state/cycles")).unwrap();
    t.fail("start", "7");
    let a = t.apply(json!([{"stop_done": 7, "why": "closed"}]));
    assert_eq!(a.log, vec!["WARN: could not stop tehas-cycle@7.service"]);
}

#[test]
fn a_sweep_removes_an_issues_names_under_its_lock_and_no_other_issues() {
    let mut t = world();
    let works = t.dir.join("works");
    fs::create_dir_all(works.join("tehas-work-5/src")).unwrap();
    fs::create_dir_all(works.join("tehas-work-51")).unwrap();
    fs::write(works.join(".tehas-workspaces"), "").unwrap();
    fs::write(works.join("tehas-work-5.state"), "x").unwrap();
    let runs = t.dir.join("user/.pi/agent/ensemble-runs");
    fs::create_dir_all(runs.join("issue-5")).unwrap();
    fs::create_dir_all(t.w.home.join("live")).unwrap();
    fs::write(t.w.home.join("live/console-5.txt"), "x").unwrap();
    fs::create_dir_all(t.w.home.join("state/cycles")).unwrap();
    let a = t.apply(json!([{"sweep": 5, "log": "removed what #5 left on this host: no cycle has run on it for 7 days"}]));
    assert_eq!(
        a.log,
        vec!["removed what #5 left on this host: no cycle has run on it for 7 days"]
    );
    assert!(!works.join("tehas-work-5").exists());
    assert!(!works.join("tehas-work-5.state").exists());
    assert!(!runs.join("issue-5").exists());
    assert!(!t.w.home.join("live/console-5.txt").exists());
    assert!(works.join("tehas-work-51").exists());
}

#[test]
fn a_sweep_whose_lock_a_cycle_holds_touches_nothing() {
    let mut t = world();
    fs::create_dir_all(t.w.home.join("state/cycles")).unwrap();
    fs::create_dir_all(t.w.home.join("live")).unwrap();
    fs::write(t.w.home.join("live/console-5.txt"), "x").unwrap();
    let lock = fs::File::create(t.w.home.join("state/cycles/5.lock")).unwrap();
    lock.lock().unwrap();
    let a = t.apply(json!([{"sweep": 5, "log": "removed"}]));
    assert_eq!(
        a.log,
        vec!["WARN: could not take the lock of #5, so nothing of it was removed"]
    );
    assert!(t.w.home.join("live/console-5.txt").exists());
}

#[test]
fn a_quiet_cycle_asked_again_and_one_that_was_said_for_only() {
    let mut t = world();
    fs::write(
        t.dir.join("note"),
        r#"{"api":1,"verb":"cycle-note","ok":true,"times":2,"stop_asked_at":1}"#,
    )
    .unwrap();
    let a = t.apply(json!([quiet_of(7)]));
    assert!(a.log[0].ends_with(": the poll asks tehas-cycle@7.service to stop again"));
    assert!(t.calls().contains(&"start 7".to_string()));
    // Past the six asks, by more: the line is said only, not counted again.
    let mut t = world();
    fs::write(
        t.dir.join("note"),
        r#"{"api":1,"verb":"cycle-note","ok":true,"times":7,"stop_asked_at":1}"#,
    )
    .unwrap();
    t.apply(json!([quiet_of(7)]));
    assert!(t
        .writes()
        .contains(&"store cycle-stop-asked 7 --said-only".to_string()));
}

#[test]
fn a_quiet_stop_that_fails_is_said_with_its_exit() {
    let mut t = world();
    t.fail("start", "7");
    let a = t.apply(json!([quiet_of(7)]));
    assert_eq!(
        a.log.last().unwrap(),
        "WARN: could not stop tehas-cycle@7.service (exit 1): it holds its slot, and the poll asks again"
    );
}

#[test]
fn a_finished_cycle_that_keeps_its_lock_is_said() {
    let mut t = world();
    let cycles = t.w.home.join("state/cycles");
    fs::create_dir_all(&cycles).unwrap();
    let lock = fs::File::create(cycles.join("7.lock")).unwrap();
    lock.lock().unwrap();
    let a = t.apply(json!([{"stop_done": 7, "why": "merged"}]));
    assert_eq!(
        a.log,
        vec!["WARN: the cycle for #7 still holds its lock after its stop"]
    );
}

#[test]
fn a_sweep_removes_a_link_and_never_what_it_points_at() {
    let mut t = world();
    let keep = t.dir.join("keep");
    fs::create_dir_all(keep.join("inside")).unwrap();
    let runs = t.dir.join("user/.pi/agent/ensemble-runs");
    fs::create_dir_all(&runs).unwrap();
    std::os::unix::fs::symlink(&keep, runs.join("issue-5")).unwrap();
    std::os::unix::fs::symlink(t.dir.join("nowhere"), runs.join("plan-5")).unwrap();
    fs::create_dir_all(t.w.home.join("state/cycles")).unwrap();
    let a = t.apply(json!([{"sweep": 5, "log": "removed what #5 left"}]));
    assert_eq!(a.log, vec!["removed what #5 left"]);
    assert!(fs::symlink_metadata(runs.join("issue-5")).is_err());
    assert!(fs::symlink_metadata(runs.join("plan-5")).is_err());
    assert!(keep.join("inside").is_dir());
}

use super::plan::{no_later_parts, plan_view, q_clean, q_render};

#[test]
fn an_agents_words_are_masked_before_they_are_posted() {
    let t = "token ghp_abcdefghijklmnop1234 and API_KEY=hunter2\r\nat 10.0.0.12 or fd12:3456:789a::1 on box.tail1.ts.net\n<!-- x -->\n- [x] done\nthanks @alice\n\n";
    assert_eq!(
        q_clean(t, 60000),
        "token ghp_*** and API_KEY=***\nat [private address] or [private address] on [private name]\n<\\!-- x -->\n- \\[x] done\nthanks @\u{200b}alice"
    );
    assert_eq!(q_clean("abcdef", 3), "abc");
    assert_eq!(q_clean("", 10), "");
}

#[test]
fn a_planning_question_is_rendered_as_the_bash_renders_it() {
    let out = json!({"text": "Two ways.", "question": {"question": "Which?", "options": [
        {"label": "A\nfirst", "description": "the first"}, {"label": "B"}]},
        "marker": "<!-- pi-rukas:handoff issue=7 cap=plan -->"});
    assert_eq!(
        q_render(7, 2, &out, "Filed in this turn: #9"),
        "<!-- pi-rukas:handoff issue=7 cap=plan -->\nTwo ways.\n\nWhich?\n\nFiled in this turn: #9\n\n- [ ] A first\n  the first\n- [ ] B\n- [ ] End this planning session\n\nTick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->"
    );
    let many: Vec<Value> = (0..22).map(|i| json!({"label": format!("o{i}")})).collect();
    let r = q_render(
        7,
        1,
        &json!({"text": "", "question": {"question": "", "options": many}}),
        "",
    );
    assert!(r.contains("Options left out: 2. A reply can name another choice."));
    assert_eq!(r.matches("- [ ] o").count(), 20);
    // Another issue's marker is no marker.
    assert!(!q_render(8, 1, &out, "").starts_with("<!-- pi-rukas"));
}

#[test]
fn later_parts_of_none_and_of_something() {
    assert!(no_later_parts(
        "## Plan\nx\n## Later parts\nNone.\n## Next\nmore"
    ));
    assert!(no_later_parts("**Later parts:** none"));
    assert!(!no_later_parts("## Later parts\n- the rest"));
    assert!(!no_later_parts("no section at all"));
    assert!(no_later_parts("Later parts (if any): None"));
}

fn plan_row(t: &World, planning: Value) {
    fs::write(
        t.dir.join("plan"),
        json!({"api": 1, "verb": "plan-show", "ok": true, "planning": planning}).to_string(),
    )
    .unwrap();
}

fn turn_ended(t: &World, n: u64, rc: &str, out: Value) {
    fs::write(
        t.w.home.join(format!("state/plan-{n}.json.rc")),
        format!("{rc}\n"),
    )
    .unwrap();
    fs::write(
        t.w.home.join(format!("state/plan-{n}.json.out")),
        out.to_string(),
    )
    .unwrap();
}

#[test]
fn a_turn_that_ended_is_recorded_and_its_question_posted() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan"}),
    );
    turn_ended(
        &t,
        7,
        "0",
        json!({"session": "s-2", "text": "Which?", "question": {"question": "Q", "options": [{"label": "A"}]},
        "filed": ["https://github.com/example-owner/example-product/issues/9", "no address"]}),
    );
    let a = t.apply(json!([{"plan_collect": 7}]));
    assert_eq!(a.log, vec!["done planning #7 turn=2 rc=0"]);
    let w = t.writes();
    let end = w
        .iter()
        .find(|c| c.starts_with("store plan-end 7 --turn"))
        .unwrap();
    assert!(end.contains("--turn 2 --outcome finished --rc 0 --session s-2 --option A --filed https://github.com/example-owner/example-product/issues/9 --out "));
    assert!(w.iter().any(|c| c.starts_with(
        "gh api -X POST repos/example-owner/example-product/issues/7/comments --input -"
    ) && c.contains(
        "Filed in this turn: https://github.com/example-owner/example-product/issues/9"
    )));
    assert!(w.contains(&"store question-asked 7 --id 99 --kind plan".to_string()));
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --remove-label tehas:planning --add-label tehas:question".to_string()));
    assert!(w.contains(&"store plan-end 7 --posted".to_string()));
    assert!(!t.w.home.join("state/plan-7.json.rc").exists());
    assert!(!t.w.home.join("state/plan-7.json.out").exists());
}

#[test]
fn a_turn_that_left_no_session_goes_to_a_person() {
    let mut t = world();
    plan_row(&t, json!({"turns": 0, "label": "tehas:plan"}));
    turn_ended(&t, 7, "3", json!({}));
    let a = t.apply(json!([{"plan_collect": 7}]));
    assert_eq!(a.log, vec!["done planning #7 turn=1 rc=3, no session"]);
    let w = t.writes();
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --remove-label tehas:plan --remove-label tehas:planning --add-label needs-human-attention".to_string()));
    assert!(w.contains(&"store plan-end 7 --turn 1 --outcome failed --rc 3".to_string()));
    assert!(w
        .iter()
        .any(|c| c
            .contains("The planning session could not start (the runner exited with status 3).")));
}

#[test]
fn a_failed_turn_of_a_session_can_be_run_again() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 2, "session": "s-1", "label": "tehas:plan", "failed": true}),
    );
    turn_ended(&t, 7, "1", json!({}));
    let a = t.apply(json!([{"plan_collect": 7}]));
    assert_eq!(a.log, vec!["done planning #7 turn=3 rc=1"]);
    let w = t.writes();
    assert!(w
        .iter()
        .any(|c| c
            .starts_with("store plan-end 7 --turn 3 --outcome failed --rc 1 --session s-1 --out")));
    assert!(w
        .iter()
        .any(|c| c.contains("This planning turn did not finish: it exited with status 1.")));
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention".to_string()));
}

#[test]
fn a_turn_cut_off_is_reaped_and_finished_as_interrupted() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan"}),
    );
    let a = t.apply(json!([{"plan_collect": 7}]));
    assert_eq!(
        a.log,
        vec![
            "removed the container tehas-plan-7: the planning turn of #7 was cut off",
            "planning #7: a turn was cut off",
            "done planning #7 turn=2 rc=interrupted"
        ]
    );
    assert!(t
        .writes()
        .iter()
        .any(|c| c.starts_with("store plan-end 7 --turn 2 --outcome interrupted --session s-1")));
}

#[test]
fn a_session_pi_lost_is_ended_and_said_once() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan", "auto": true}),
    );
    turn_ended(&t, 7, "1", json!({"lost": true}));
    let a = t.apply(json!([{"plan_collect": 7}]));
    assert_eq!(
        a.log,
        vec![
            "planning #7: Pi no longer has session s-1",
            "planning #7 ended"
        ]
    );
    let w = t.writes();
    assert!(w.contains(&"store plan-end 7 --turn 2 --outcome lost --rc 1".to_string()));
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --add-label needs-human-attention".to_string()));
    assert!(w.contains(&"store plan-end 7 --close".to_string()));
    // A close the store refuses says nothing yet: the next poll ends it.
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan"}),
    );
    turn_ended(&t, 7, "1", json!({"lost": true}));
    t.fail("store", "--close|busy");
    t.apply(json!([{"plan_collect": 7}]));
    assert!(!t
        .writes()
        .iter()
        .any(|c| c.contains("pi-rukas has no record of it")));
}

#[test]
fn a_refine_that_filed_replaces_its_issue() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:refine"}),
    );
    fs::write(
        t.dir.join("body-9"),
        json!({"body": "The plan.\n## Later parts\nNone"}).to_string(),
    )
    .unwrap();
    turn_ended(
        &t,
        7,
        "0",
        json!({"session": "s-1", "text": "Done.", "filed": ["https://github.com/example-owner/example-product/issues/9"]}),
    );
    let a = t.apply(json!([{"plan_collect": 7}]));
    assert_eq!(a.log, vec!["planning #7 ended: tehas:replaced by https://github.com/example-owner/example-product/issues/9", "done planning #7 turn=2 rc=0"]);
    let w = t.writes();
    assert!(w.iter().any(|c| c.starts_with(
        "gh issue edit 9 --repo example-owner/example-product --body-file - <<< Replaces #7"
    )));
    assert!(w.contains(&"gh api -X POST repos/example-owner/example-product/issues/7/sub_issues -F sub_issue_id=555".to_string()));
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --remove-label tehas:question --remove-label tehas:planning --remove-label tehas:refined --add-label tehas:replaced".to_string()));
    assert!(w
        .iter()
        .any(|c| c.starts_with("gh issue comment 7") && c.contains("Replaced by #9.")));
    assert!(w.contains(
        &"gh issue close 7 --repo example-owner/example-product --reason not planned".to_string()
    ));
    let ledger = fs::read_to_string(t.w.home.join("state/refined.jsonl")).unwrap();
    assert!(ledger.contains(
        r#""source":7,"filed":9,"url":"https://github.com/example-owner/example-product/issues/9"}"#
    ));
}

#[test]
fn a_refine_with_later_parts_is_refined_and_stays_open() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:refine"}),
    );
    fs::write(
        t.dir.join("body-9"),
        json!({"body": "## Later parts\n- part two"}).to_string(),
    )
    .unwrap();
    turn_ended(
        &t,
        7,
        "0",
        json!({"session": "s-1", "filed": ["https://github.com/example-owner/example-product/issues/9"]}),
    );
    t.apply(json!([{"plan_collect": 7}]));
    let w = t.writes();
    assert!(w
        .iter()
        .any(|c| c.ends_with("--remove-label tehas:replaced --add-label tehas:refined")));
    assert!(!w.iter().any(|c| c.starts_with("gh issue close")));
}

#[test]
fn a_session_ended_by_a_person_is_closed_and_said() {
    let mut t = world();
    let a = t.apply(json!([{"plan_end": 7, "say": "Planning session ended by alice."}]));
    assert_eq!(a.log, vec!["planning #7 ended"]);
    let w = t.writes();
    assert_eq!(w[0], "gh issue edit 7 --repo example-owner/example-product --remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention");
    assert_eq!(w[1], "store plan-end 7 --close");
    assert!(w[2].ends_with("<<< Planning session ended by alice."));
}

#[test]
fn the_plan_view_is_the_whole_shape_the_analytics_reads() {
    let v = plan_view(
        &json!({"session": "s-1", "turns": 2, "options": ["A"], "pending": false, "failed": false,
        "running": true, "started_at": 1793000000, "comment_id": 55, "lost": false, "by": "alice",
        "label": "tehas:plan", "answers": [{"turn": 2, "by": "mill-bot", "millwright": true}], "filed": [],
        "notes": {"held": [1]}}),
    );
    assert_eq!(
        v,
        json!({"session": "s-1", "turns": 2, "options": ["A"], "pending": false, "failed": false,
        "running": true, "startedAt": "2026-10-26T07:33:20Z", "comment": 55, "by": "alice", "label": "tehas:plan",
        "answers": [{"turn": 2, "by": "mill-bot", "fixer": true}], "held": [1]})
    );
    assert_eq!(plan_view(&json!(null)), json!(null));
}

#[test]
fn a_turns_end_tells_the_analytics_with_the_whole_view_in_a_private_file() {
    let mut t = world();
    t.w.host.analytics = Some(t.dir.join("analytics").to_string_lossy().into_owned());
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan", "running": true, "started_at": 1793000000}),
    );
    turn_ended(&t, 7, "0", json!({"session": "s-1", "text": "Q"}));
    t.apply(json!([{"plan_collect": 7}]));
    let events = fs::read_to_string(t.dir.join("events")).unwrap();
    assert!(events.starts_with("plan 7 --facts "));
    assert!(events.trim_end().ends_with("--rc 0"));
    let facts: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("facts.json")).unwrap()).unwrap();
    assert_eq!(facts["running"], true);
    assert_eq!(facts["startedAt"], "2026-10-26T07:33:20Z");
    assert_eq!(facts["turns"], 1);
    assert_eq!(
        fs::read_to_string(t.dir.join("facts.mode")).unwrap().trim(),
        "600"
    );
    assert_eq!(fs::read_dir(t.w.home.join("tmp")).unwrap().count(), 0);
}

#[test]
fn an_option_labelled_false_is_recorded_as_none() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 0, "session": "", "label": "tehas:plan"}),
    );
    turn_ended(
        &t,
        7,
        "0",
        json!({"session": "s-1", "question": {"options": [{"label": false}, {"label": 3}, "x"]}}),
    );
    t.apply(json!([{"plan_collect": 7}]));
    assert!(t
        .writes()
        .iter()
        .any(|c| c.contains("--option - --option 3 --option - --out")));
}

#[test]
fn a_question_already_posted_is_not_posted_again() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 2, "label": "tehas:plan", "comment_id": 41}),
    );
    fs::write(
        t.w.home.join("state/plan-7.json.out"),
        json!({"text": "Q"}).to_string(),
    )
    .unwrap();
    assert!(t.apply(json!([{"plan_post": 7}])).log.is_empty());
    let w = t.writes();
    assert!(!w.iter().any(|c| c.contains("/comments")));
    assert!(w.contains(&"store plan-end 7 --posted".to_string()));
    // A record the store will not update after the post is said, and the
    // labels wait for the next poll.
    let mut t = world();
    plan_row(&t, json!({"turns": 2, "label": "tehas:plan"}));
    fs::write(
        t.w.home.join("state/plan-7.json.out"),
        json!({"text": "Q"}).to_string(),
    )
    .unwrap();
    t.fail("store", "question-asked|conflict");
    let a = t.apply(json!([{"plan_post": 7}]));
    assert_eq!(
        a.log,
        vec![
            "WARN: the record of #7 could not be updated after its question was posted (conflict)"
        ]
    );
    assert!(!t
        .writes()
        .iter()
        .any(|c| c.contains("--add-label tehas:question")));
}

#[test]
fn a_refine_that_filed_elsewhere_or_again_is_refined_once() {
    // An issue of another repository is said, never edited: the original stays.
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:refine"}),
    );
    turn_ended(
        &t,
        7,
        "0",
        json!({"session": "s-1", "filed": ["https://github.com/other/place/issues/3"]}),
    );
    t.apply(json!([{"plan_collect": 7}]));
    let w = t.writes();
    assert!(w.iter().any(|c| c.ends_with("--add-label tehas:refined")));
    assert!(w
        .iter()
        .any(|c| c.contains("Replaced by https://github.com/other/place/issues/3.")));
    assert!(!w.iter().any(|c| c.starts_with("gh issue edit 3")));
    // A second poll after a body that says Replaces already, and the mark on:
    // nothing is edited or said again.
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:refine"}),
    );
    fs::write(
        t.dir.join("body-9"),
        json!({"body": "Replaces #7\r\n\nThe plan.\n## Later parts\nNone"}).to_string(),
    )
    .unwrap();
    fs::write(
        t.dir.join("labels-7"),
        json!({"labels": [{"name": "tehas:replaced"}]}).to_string(),
    )
    .unwrap();
    fs::write(
        t.w.home.join("state/plan-7.json.out"),
        json!({"filed": ["https://github.com/example-owner/example-product/issues/9"]}).to_string(),
    )
    .unwrap();
    plan_row(
        &t,
        json!({"turns": 1, "label": "tehas:refine", "comment_id": null}),
    );
    t.apply(json!([{"plan_post": 7}]));
    let w = t.writes();
    assert!(!w
        .iter()
        .any(|c| c.starts_with("gh issue edit 9") || c.contains("issue comment")));
    assert!(w.iter().any(|c| c.starts_with("gh issue close 7")));
}

#[test]
fn a_repository_named_issues_is_its_own() {
    let mut t = world();
    t.w.repo = "owner/issues".into();
    plan_row(&t, json!({"turns": 1, "label": "tehas:refine"}));
    fs::write(t.dir.join("body-5"), json!({"body": "x"}).to_string()).unwrap();
    fs::write(
        t.w.home.join("state/plan-7.json.out"),
        json!({"filed": ["https://github.com/owner/issues/issues/5"]}).to_string(),
    )
    .unwrap();
    t.apply(json!([{"plan_post": 7}]));
    assert!(t
        .writes()
        .iter()
        .any(|c| c.starts_with("gh issue edit 5 --repo owner/issues --body-file -")));
}

#[test]
fn the_refined_ledger_keeps_its_last_500_lines() {
    let mut t = world();
    let ledger = t.w.home.join("state/refined.jsonl");
    fs::write(
        &ledger,
        (0..500)
            .map(|i| format!("{{\"old\":{i}}}\n"))
            .collect::<String>(),
    )
    .unwrap();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:refine"}),
    );
    turn_ended(
        &t,
        7,
        "0",
        json!({"session": "s-1", "filed": ["https://github.com/example-owner/example-product/issues/9"]}),
    );
    t.apply(json!([{"plan_collect": 7}]));
    let lines: Vec<String> = fs::read_to_string(&ledger)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 500);
    assert_eq!(lines[0], r#"{"old":1}"#);
    assert!(lines[499].contains(r#""filed":9"#));
}

#[test]
fn more_of_what_q_clean_masks() {
    assert_eq!(
        q_clean("postgres://u:pw@db/x github_pat_abcdefghijklmnopqrst", 100),
        "postgres://u:***@\u{200b}db/x github_pat_***"
    );
    assert_eq!(
        q_clean("172.16.0.1 100.64.1.2 169.254.0.9 8.8.8.8", 100),
        "[private address] [private address] [private address] 8.8.8.8"
    );
    assert_eq!(
        q_clean("* [ ] a\n  + [x] b\na@b", 100),
        "* \\[ ] a\n  + \\[x] b\na@\u{200b}b"
    );
    assert_eq!(q_clean("äöå", 2), "äö");
}
