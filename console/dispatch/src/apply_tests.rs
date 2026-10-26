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
if [ "$name" != store ] && [ "$name" != docker ]; then input=$(cat); fi
printf '%s %s%s\n' "$name" "$*" "${input:+ <<< $input}" >>"$d/calls.log"
if [ -f "$d/fail-$name" ]; then
  while IFS= read -r p; do
    case "$* $input" in *"$p"*)
      case "$p" in slow*) sleep 5 ;; esac
      case "$*" in *DELETE*) echo "gh: Not Found (HTTP 404)" >&2 ;; *) echo "store: busy: stub" >&2 ;; esac
      exit 1 ;;
    esac
  done <"$d/fail-$name"
fi
case "$name $1" in
  "store journal-record") n=$(($(cat "$d/n" 2>/dev/null || echo 0) + 1)); echo "$n" >"$d/n"
    echo "{\"api\":1,\"verb\":\"journal-record\",\"ok\":true,\"action\":$n}" ;;
  "store plan-note") echo "{\"api\":1,\"verb\":\"plan-note\",\"ok\":true,\"said\":$(cat "$d/said" 2>/dev/null || echo true)}" ;;
  "store "*) echo "{\"api\":1,\"verb\":\"$1\",\"ok\":true}" ;;
  "docker rm") echo "$3" ;;
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
    for name in ["gh", "store", "docker"] {
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
    let start = json!({"start": 7, "by": "alice", "given": "label"});
    let stop = json!({"stop_done": 8, "why": "merged"});
    let a = t.apply(json!([start, {"gate_held": true}, stop]));
    assert_eq!(a.left, vec![start, stop]);
}
