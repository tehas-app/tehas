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
  "store plan-note") echo "{\"api\":1,\"verb\":\"plan-note\",\"ok\":true,\"said\":$(cat "$d/said" 2>/dev/null || echo true)}" ;;
  "store "*) echo "{\"api\":1,\"verb\":\"$1\",\"ok\":true}" ;;
  "docker rm") echo "$3" ;;
  "gh api") case "$2" in *comments*) cat "$d/comments" 2>/dev/null || echo '[]' ;; esac ;;
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
    for name in ["gh", "store", "docker", "start", "status"] {
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
        },
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
    let stop = json!({"stop_done": 8, "why": "merged"});
    let sweep = json!({"sweep": 9, "log": "removed what #9 left"});
    let a = t.apply(json!([stop, {"gate_held": true}, sweep]));
    assert_eq!(a.left, vec![stop, sweep]);
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
