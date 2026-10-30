//! apply, against stubs of gh, the store and docker that write down each
//! call and fail the ones a test names.

use super::*;
use crate::poll::End;
use crate::read::run::Calls;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A stub answers each call, writes `<name> <args>` and its standard input
/// to calls.log, and exits 1 when a line of fail-<name> is in its args. When
/// want-token-<name> names a file, the stub says one word on that file of the
/// world if it ran with a token; the word is the only thing it writes, so a
/// test can count the processes that were given one without any part of a
/// token reaching a file, a log or an argument.
const STUB: &str = r#"#!/bin/bash
d=$(dirname "$0")
name=$(basename "$0")
input=""
case $name in gh) input=$(cat) ;; esac
printf '%s %s%s\n' "$name" "$*" "${input:+ <<< $input}" >>"$d/calls.log"
if [ -f "$d/want-token-$name" ] && [ -n "${TEHAS_GH_TOKEN:-}" ]; then
  printf 'token\n' >>"$d/$(cat "$d/want-token-$name")"
fi
if [ -f "$d/fail-$name" ]; then
  while IFS= read -r line; do
    p=${line%%|*} w=${line#*|}
    [ "$w" != "$line" ] || w=busy
    case "$* $input" in *"$p"*)
      case "$p" in slow*) sleep "$(cat "$d/slow" 2>/dev/null || echo 5)" ;; esac
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
  "tidy "*) echo "the tidy line" ;;
  "health "*) echo "the health line" ;;
  "store journal-open") cat "$d/open" 2>/dev/null || echo '{"api":1,"verb":"journal-open","ok":true,"actions":[]}' ;;
  "store poll-begin") cat "$d/poll-begin" 2>/dev/null || echo '{"api":1,"verb":"poll-begin","ok":true,"at":2000}' ;;
  "store plan-begin")
    for a; do [ "${prev:-}" != --brief-file ] || { cp "$a" "$d/brief"; stat -c %a "$a" >"$d/brief.mode"; }; prev=$a; done
    echo '{"api":1,"verb":"plan-begin","ok":true}' ;;
  "store plan-answer")
    for a; do [ "${prev:-}" != --text-file ] || cp "$a" "$d/answer"; prev=$a; done
    cat "$d/answered" 2>/dev/null || echo '{"api":1,"verb":"plan-answer","ok":true,"planning":{"answer":"Do A.\n"}}' ;;
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
  "gh api") case "$*" in *"-X POST"*/comments*) echo 99 ;; esac
    case "$2" in
      repos/*/issues/[0-9]*) case "$2" in
          */comments*) ;;
          *) f=${2#*/issues/}; if [ -f "$d/issue-$f" ]; then cat "$d/issue-$f"; else echo '{"id":555}'; fi ;;
        esac ;;
    esac
    case "$2" in
      */issues/[0-9]*/comments*) f=${2#*/issues/}; f=$d/comments-${f%%/*}
        if [ -f "$f" ]; then cat "$f"; else cat "$d/comments" 2>/dev/null || echo '[]'; fi ;;
    esac ;;
  "gh pr") cat "$d/prs" 2>/dev/null || echo '[]' ;;
  "git -C") case "$3" in
      status) cat "$d/dirty" 2>/dev/null ;;
      rev-parse) echo "abc1234" ;;
    esac ;;
  "systemctl --user") case "$*" in
      *LoadState*) cat "$d/loadstate" 2>/dev/null || echo loaded ;;
      *ActiveState*) cat "$d/activestate" 2>/dev/null || echo inactive ;;
    esac ;;
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
    for name in [
        "gh",
        "store",
        "docker",
        "start",
        "status",
        "tidy",
        "health",
        "analytics",
        "git",
        "systemctl",
    ] {
        std::os::unix::fs::symlink(stub, dir.join(name)).unwrap();
    }
    let s = |n: &str| dir.join(n).to_string_lossy().into_owned();
    let w = Writer {
        repo: "example-owner/example-product".into(),
        home: dir.join("home"),
        gh: s("gh"),
        docker: s("docker"),
        calls: Calls {
            home: None,
            bound: Duration::from_secs(10),
            store_bound: Duration::from_secs(10),
            gh: s("gh"),
            store: s("store"),
            gh_count: 0,
            gh_token: None,
        },
        poll_at: 1000,
        row_not_begun: false,
        before: 990,
        bound: Duration::from_secs(10),
        store_retry: Duration::from_secs(1),
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
            health_cmd: s("health"),
            tidy_cmd: s("tidy"),
            analytics: None,
            analytics_wait: Duration::from_secs(5),
            work_dir: dir.join("works/tehas-work").to_string_lossy().into_owned(),
            user_home: dir.join("user").to_string_lossy().into_owned(),
            done_wait: Duration::from_secs(5),
            image: "tehas-sandbox:test".into(),
            plan_cmd: "/opt/plan".into(),
            plan_start_cmd: Some(s("start")),
            pin_cmd: None,
            plan_timeout: 3600,
            plan_max_turns: 20,
            systemd_run: s("start"),
            checkout: dir.join("checkout").to_string_lossy().into_owned(),
            branch: "main".into(),
            git: s("git"),
            index_cmd: "/opt/index".into(),
            index_start_cmd: Some(s("start")),
            fetch_wait: Duration::from_secs(10),
            lock_poll: Duration::from_millis(500),
            launch_poll: Duration::from_millis(500),
        },
        ended: None,
        spend: None,
        status_due: false,
        changed: false,
        gh_writes: 0,
        starts: 0,
        fetched: false,
    };
    World { dir, w }
}

impl World {
    /// The stub's answer for a `slow` pattern comes after `secs` instead of
    /// its own 5 s, so a test that waits out its bound waits for the bound
    /// and not for the stub. Bash's sleep has no fractional form, so 1 is as
    /// short as the answer can be.
    fn slow_after(&self, secs: u64) {
        fs::write(self.dir.join("slow"), secs.to_string()).unwrap();
    }

    fn fail(&self, name: &str, pattern: &str) {
        let p = self.dir.join(format!("fail-{name}"));
        let mut s = fs::read_to_string(&p).unwrap_or_default();
        s.push_str(pattern);
        s.push('\n');
        fs::write(p, s).unwrap();
    }

    /// The lines of a file the stub wrote, one per call that wrote it.
    fn lines_of(&self, file: &str) -> Vec<String> {
        fs::read_to_string(self.dir.join(file))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Make <program>'s stub record a line in <file> of the world when it ran
    /// with `TEHAS_GH_TOKEN` set. One line per process that saw a token.
    fn asks_for_token(&self, program: &str, file: &str) {
        fs::write(
            self.dir.join(format!("want-token-{program}")),
            format!("{file}\n"),
        )
        .unwrap();
    }

    /// The three host commands of a poll - tidy, the status and health - each
    /// recording its word in <file> when it was given a token.
    fn expects_a_token_on_its_host_commands(&self, file: &str) {
        for program in ["tidy", "status", "health"] {
            self.asks_for_token(program, file);
        }
    }

    /// The words the stubs said for the token, one line per process that was
    /// given one.
    fn token_said(&self, file: &str) -> Vec<String> {
        self.lines_of(file)
    }

    fn calls(&self) -> Vec<String> {
        self.lines_of("calls.log")
    }

    /// The gh and docker calls, without the journal's.
    fn writes(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| !c.starts_with("store journal-") && !c.starts_with("git "))
            .collect()
    }

    fn apply(&mut self, actions: Value) -> Applied {
        self.w.apply(actions.as_array().unwrap())
    }
}

/// The three host commands of a poll: tidy as the end of a poll of `dry`
/// sends it, the status, and health as the poll's start sends it. Each prints
/// its own word, so `printed` says which of them answered, and the arguments
/// come from `End`, the code that really starts them.
fn runs_host_commands(w: &mut Writer, dry: bool, printed: &mut Vec<String>) {
    End::host_of(dry).runs_host_commands(w, printed);
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
            "gh issue edit 7 --repo example-owner/example-product --remove-label tehas:ready --add-label needs-human-attention",
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

/// The token both token tests give the poll, and the file their stubs say
/// their word on.
const TOKEN: &str = "ghs_poll";
const GIVEN: &str = "given";

#[test]
fn the_poll_gives_its_host_commands_its_token() {
    // tidy, the status and health are host commands the poll runs through
    // status_of: each of them gets the poll's token as TEHAS_GH_TOKEN, the same
    // value the poll's own gh calls get, so the shim of a gh they run asks for
    // none. The stubs say one word when they saw a token; the poll's taking of
    // it is the case of the same name in read/run.rs, so here the poll starts
    // out holding one.
    let mut t = world();
    t.expects_a_token_on_its_host_commands(GIVEN);
    t.w.calls.gh_token = Some(TOKEN.into());
    let mut printed = Vec::new();
    runs_host_commands(&mut t.w, true, &mut printed);
    assert_eq!(t.token_said(GIVEN), vec!["token"; 3]);
    // The three lines are the three commands' own words, so neither of them
    // stands in for another: the token reaches no output of any of them.
    assert_eq!(
        printed,
        vec!["the tidy line", "the status line", "the health line"]
    );
    // The issue's rule: the token is in no log line and no argument.
    assert!(!t.calls().iter().any(|c| c.contains(TOKEN)));
    // The end's own arguments: a dry poll's tidy is asked for a dry run, the
    // way End sends it and no other way. The name is a path, so the call is
    // named by the word after its last slash.
    let tidy = t.w.host.tidy_cmd.rsplit('/').next().unwrap().to_string();
    assert!(t
        .calls()
        .iter()
        .any(|c| c.starts_with(&format!("{tidy} --dry-run"))));
}

#[test]
fn host_commands_get_none_without_a_token() {
    // With no app named, or no token the poll could take, nothing is given. The
    // gh side of the rule is `a_token_that_could_not_be_had_leaves_the_calls_as_they_were`
    // in read/run.rs.
    let mut t = world();
    t.expects_a_token_on_its_host_commands(GIVEN);
    assert_eq!(t.w.calls.gh_token, None);
    let mut printed = Vec::new();
    runs_host_commands(&mut t.w, true, &mut printed);
    assert_eq!(t.token_said(GIVEN), Vec::<String>::new());
    assert_eq!(
        printed,
        vec!["the tidy line", "the status line", "the health line"]
    );
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
    // The wait between the tries is the thing under test, not its length.
    t.w.store_retry = Duration::from_millis(200);
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
    // Past it is the timeout the test asks for. The stub answers after the
    // bound either way, so the bound is what the test scales with.
    t.w.bound = Duration::from_millis(200);
    t.slow_after(1);
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
fn the_said_and_told_records_follow_a_comment_that_worked() {
    let mut t = world();
    let a = t.apply(said_and_told());
    assert!(a.log.is_empty());
    assert_eq!(fs::read_to_string(said_file(&t)).unwrap(), "conflict:abc");
    assert!(t.calls().contains(&"store park-told 4".to_string()));
}

#[test]
fn a_failed_comment_leaves_the_said_and_told_records_unwritten() {
    let mut t = world();
    t.fail("gh", "issue comment");
    let a = t.apply(said_and_told());
    assert_eq!(
        a.log,
        vec!["WARN: comment on #3 failed", "WARN: comment on #4 failed"]
    );
    no_record_was_made(&t);
}

#[test]
fn a_comment_the_journal_could_not_record_leaves_the_records_unwritten() {
    let mut t = world();
    t.fail("store", "journal-record");
    let a = t.apply(said_and_told());
    assert_eq!(
        a.log,
        vec![
            "WARN: the journal could not record a write on #3, so it was not made",
            "WARN: the journal could not record a write on #4, so it was not made",
        ]
    );
    no_record_was_made(&t);
}

#[test]
fn a_record_that_failed_after_a_good_comment_is_said_in_the_log() {
    // The comment is public and the record is not: each of the two records
    // says so its own way, and neither ends the poll.
    let mut t = world();
    // Order matters: the stub fails a store call on the first pattern of
    // fail-store that matches, so the tell's own refusal comes first and the
    // journal's verbs below it stay ok. The write's parent is gone, so the
    // record of #3 cannot be made; the tell's call answers 1 with no word of
    // its own, as a store that cannot parse its issue number does.
    t.fail("store", "park-told|");
    fs::remove_dir_all(t.w.home.join("state/refresh")).unwrap();
    let a = t.apply(said_and_told());
    assert_eq!(a.log.len(), 2, "{:?}", a.log);
    assert_eq!(
        a.log[0],
        format!("WARN: could not write {}", said_file(&t).display())
    );
    // The tell's call was made and refused, so its record is not there.
    assert!(t.calls().iter().any(|c| c.starts_with("store park-told 4")));
    let told = a.log[1].trim_start_matches("WARN: ");
    let head = "could not record the hold on the park of #4 (";
    assert!(told.starts_with(head), "{told}");
    let reason = &told[head.len()..];
    assert!(
        !reason.starts_with(")"),
        "the WARN of the tell named no word: {told}"
    );
}

/// The queue of the two records of having said something: #3 the refresh
/// hold, #4 the park's tell.
fn said_and_told() -> Value {
    json!([
        {"issue": 3, "refresh_said": "conflict:abc", "say": "Not refreshed."},
        {"issue": 4, "park_told": true, "say": "Not started."},
    ])
}

fn said_file(t: &World) -> PathBuf {
    crate::apply::said_file(&t.w.home, 3)
}

/// Neither record was made: no file, no store verb.
fn no_record_was_made(t: &World) {
    assert!(!said_file(t).exists());
    assert!(!t.calls().iter().any(|c| c.starts_with("store park-told")));
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
fn an_action_of_no_kind_apply_knows_is_left() {
    let mut t = world();
    let odd = json!({"mystery": 8});
    let a = t.apply(json!([odd, {"gate_held": true}]));
    assert_eq!(a.left, vec![odd]);
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
fn an_easy_start_logs_the_easy_merge_and_marks_its_approval() {
    let mut t = world();
    let h = t.w.home.clone();
    fs::write(h.join("state/refresh/7.json"), "{}").unwrap();
    let mut a = start_of(7, "refresh");
    a["verb"] = json!("easy");
    a["log"] = json!("start #7 (easy by alice)");
    a["refresh_pr"] = json!(12);
    a["refresh_url"] = json!("https://github.com/example-owner/example-product/pull/12");
    a["easy"] = json!(true);
    a["refresh_of"] =
        json!({"pr": 12, "head_sha": "c".repeat(40), "base_sha": "d".repeat(40), "branch": "main"});
    let got = t.apply(json!([a.clone()]));
    // The log names the easy merge and the pull request it merges into.
    assert_eq!(
        got.log
            .iter()
            .filter(|l| l.contains("start #7"))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["start #7 (easy by alice)", "start #7: easy merge of PR #12"]
    );
    let w = t.writes();
    // The record carries the store's mark, so the approval cannot be owed a
    // second easy merge (#1031).
    let record = w
        .iter()
        .find(|c| c.starts_with("store approval-record"))
        .expect("the start wrote its approval");
    assert!(record.ends_with("--easy"), "{record}");
    assert!(
        record.contains(&format!(
            "--refresh-pr 12 --refresh-head {}",
            "c".repeat(40)
        )),
        "{record}"
    );
    assert!(w.iter().any(|c| c
        .contains("Tehas chose the easy merge of this issue on `tehas-tehas`")
        && c.contains(
            "merges `main` into https://github.com/example-owner/example-product/pull/12"
        )));
}

#[test]
fn a_full_start_records_no_easy_mark_and_logs_no_easy_merge() {
    let mut t = world();
    let h = t.w.home.clone();
    fs::write(h.join("state/refresh/7.json"), "{}").unwrap();
    let mut a = start_of(7, "refresh");
    a["refresh_pr"] = json!(12);
    a["refresh_url"] = json!("https://github.com/example-owner/example-product/pull/12");
    a["refresh_of"] =
        json!({"pr": 12, "head_sha": "c".repeat(40), "base_sha": "d".repeat(40), "branch": "main"});
    let got = t.apply(json!([a]));
    let w = t.writes();
    let record = w
        .iter()
        .find(|c| c.starts_with("store approval-record"))
        .expect("the start wrote its approval");
    assert!(!record.contains("--easy"), "{record}");
    assert!(
        !got.log.iter().any(|l| l.contains("easy merge")),
        "{:?}",
        got.log
    );
    assert!(!h.join("state/refresh/7.json").exists());
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

/// The tick of `Retry from the issue alone`: the start names no comment of the
/// issue or of its pull request, so with the host reading comments it asks for
/// neither and its record holds neither. Were the guard dropped, this start
/// would read both, and these are the two calls that say it did.
#[test]
fn a_start_of_the_issue_alone_reads_no_comment_of_the_issue_or_its_pull_request() {
    let mut t = world();
    t.w.host.comments = true;
    // Both reads would answer if they were asked for: the issue's comments
    // link one file, the pull request's would go into retry_context.
    fs::write(
        t.dir.join("comments"),
        r#"[{"id": 1, "body": "see https://github.com/user-attachments/assets/2", "user": {"login": "alice"},
            "created_at": "2026-10-26T09:00:00Z", "updated_at": "2026-10-26T09:00:00Z"}]"#,
    )
    .unwrap();
    fs::write(
        t.dir.join("prs"),
        r#"[{"number": 30, "headRefName": "feature/issue-7-x", "body": ""}]"#,
    )
    .unwrap();
    let mut a = start_of(7, "retry-tick");
    a["issue_alone"] = json!(true);
    a["since"] = json!("2026-10-26T09:00:00Z");
    let r = t.apply(json!([a]));
    assert_eq!(r.exit, None);
    let w = t.calls();
    assert!(
        !w.iter().any(|c| c.starts_with("gh pr list")),
        "an alone start asked for the pull requests: {w:?}"
    );
    assert!(
        !w.iter().any(|c| c.contains("issues/7/comments")),
        "an alone start read the comments of the issue: {w:?}"
    );
    let input: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("input.json")).unwrap()).unwrap();
    assert_eq!(
        input,
        json!({"links": ["https://github.com/user-attachments/assets/1"]})
    );
    // A tick of the clean start under the same host asks for both, so the two
    // cases above really are the guard and not a world that reads nothing.
    let mut t = world();
    t.w.host.comments = true;
    fs::write(
        t.dir.join("comments"),
        r#"[{"id": 1, "body": "see https://github.com/user-attachments/assets/2", "user": {"login": "alice"},
            "created_at": "2026-10-26T09:00:00Z", "updated_at": "2026-10-26T09:00:00Z"}]"#,
    )
    .unwrap();
    fs::write(
        t.dir.join("prs"),
        r#"[{"number": 30, "headRefName": "feature/issue-7-x", "body": ""}]"#,
    )
    .unwrap();
    let mut a = start_of(7, "retry-tick");
    a["since"] = json!("2026-10-26T09:00:00Z");
    t.apply(json!([a]));
    let w = t.calls();
    assert!(w.iter().any(|c| c.starts_with("gh pr list")));
    assert!(w.iter().any(|c| c.contains("issues/7/comments")));
    let input: Value =
        serde_json::from_str(&fs::read_to_string(t.dir.join("input.json")).unwrap()).unwrap();
    assert!(input.get("comment_links").is_some());
    assert!(input.get("retry_context").is_some());
}

/// The flag is a bool or nothing: a value that is neither is said in the log
/// and read as the clean start it is not silently made to look like.
#[test]
fn a_start_whose_issue_alone_is_not_a_bool_is_said_and_reads_its_comments() {
    for v in [json!("true"), json!(1), json!("yes")] {
        let mut t = world();
        t.w.host.comments = true;
        fs::write(
            t.dir.join("comments"),
            r#"[{"id": 1, "body": "see https://github.com/user-attachments/assets/2", "user": {"login": "alice"},
                "created_at": "2026-10-26T09:00:00Z", "updated_at": "2026-10-26T09:00:00Z"}]"#,
        )
        .unwrap();
        let mut a = start_of(7, "retry-tick");
        a["issue_alone"] = v.clone();
        a["since"] = json!("2026-10-26T09:00:00Z");
        let r = t.apply(json!([a]));
        assert_eq!(r.exit, None);
        assert!(
            r.log
                .iter()
                .any(|l| l.starts_with("WARN: #7: its start names issue_alone ")),
            "nothing was said of {v}: {:?}",
            r.log
        );
        assert!(t.calls().iter().any(|c| c.contains("issues/7/comments")));
    }
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
    // The wait for the start's record is the thing under test, not its length.
    t.w.host.start_wait = Duration::from_millis(200);
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
    t.w.host.start_wait = Duration::from_millis(200);
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
    // The wait for the lock is the thing under test, not its length.
    t.w.host.lock_poll = Duration::from_millis(200);
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

use super::plan::{no_later_parts, plan_view, q_render};
use tehas_core::question::q_clean;

#[test]
fn an_agents_words_are_masked_before_they_are_posted() {
    // The addresses are built here, not written out: this file is linted.
    let (v4, v6, name) = (
        format!("10.{}.0.12", 0),
        format!("{}12:3456:789a::1", "fd"),
        format!("box.tail1.ts.{}", "net"),
    );
    let t = format!("token ghp_abcdefghijklmnop1234 and API_KEY=hunter2\r\nat {v4} or {v6} on {name}\n<!-- x -->\n- [x] done\nthanks @alice\n\n");
    assert_eq!(
        q_clean(&t, 60000),
        "token ghp_*** and API_KEY=***\nat [private address] or [private address] on [private name]\n<\\!-- x -->\n- ☑ done\nthanks @\u{200b}alice"
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
        "Which?\n\nFiled in this turn: #9\n\n- [ ] A first\n  the first\n- [ ] B\n- [ ] End this planning session\n\nTick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->"
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
fn a_plan_chosen_for_its_issue_is_written_onto_it_and_ends_the_session() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 2, "session": "s-1", "label": "tehas:plan"}),
    );
    turn_ended(
        &t,
        7,
        "0",
        json!({"session": "s-1", "text": "This issue now holds the plan.", "question": null,
               "update": {"title": "feat: faster search", "body": "## Outcome\n\nFast."}}),
    );
    let a = t.apply(json!([{"plan_collect": 7}]));
    assert_eq!(
        a.log,
        vec!["planning #7 ended", "done planning #7 turn=3 rc=0"]
    );
    let w = t.writes();
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --title feat: faster search --body-file - <<< ## Outcome".to_string()));
    assert!(w.contains(&"Fast.".to_string()));
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention".to_string()));
    assert!(w.iter().any(
        |c| c.starts_with("gh issue comment 7") && c.contains("This issue now holds the plan.")
    ));
    assert!(!w.iter().any(|c| c.contains("issues/7/comments")));
    assert!(!t.w.home.join("state/plan-7.json.out").exists());
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
fn a_plan_is_said_before_its_boxes_and_once() {
    let mut t = world();
    plan_row(&t, json!({"turns": 2, "label": "tehas:plan"}));
    let path = t.w.home.join("state/plan-7.json.out");
    fs::write(
        &path,
        json!({"text": "The plan.", "question": {"question": "Which?", "options": []}}).to_string(),
    )
    .unwrap();
    // The boxes do not go up: the plan stays said, the next poll adds them.
    t.fail("gh", "repos/example-owner/example-product/issues/7/comments");
    let a = t.apply(json!([{"plan_post": 7}]));
    assert_eq!(
        a.log,
        vec!["WARN: the planning turn of #7 could not be posted; the next poll tries again"]
    );
    let said = |t: &World| {
        t.writes()
            .iter()
            .filter(|c| c.starts_with("gh issue comment 7"))
            .count()
    };
    assert_eq!(said(&t), 1);
    let kept: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(kept["text"], "");
    assert_eq!(kept["question"]["question"], "Which?");

    fs::remove_file(t.dir.join("fail-gh")).unwrap();
    assert!(t.apply(json!([{"plan_post": 7}])).log.is_empty());
    let w = t.writes();
    assert_eq!(said(&t), 1);
    let plan = w.iter().position(|c| c.starts_with("gh issue comment 7"));
    let boxes = w.iter().rposition(|c| c.contains("/issues/7/comments"));
    assert!(plan < boxes);
}

#[test]
fn a_plan_not_said_stops_its_boxes_too() {
    let mut t = world();
    plan_row(&t, json!({"turns": 2, "label": "tehas:plan"}));
    fs::write(
        t.w.home.join("state/plan-7.json.out"),
        json!({"text": "The plan."}).to_string(),
    )
    .unwrap();
    t.fail("gh", "issue comment 7");
    let a = t.apply(json!([{"plan_post": 7}]));
    assert_eq!(
        a.log,
        vec![
            "WARN: comment on #7 failed",
            "WARN: the planning turn of #7 could not be posted; the next poll tries again"
        ]
    );
    assert!(!t.writes().iter().any(|c| c.contains("/issues/7/comments")));
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
        q_clean(
            &format!("{0}.16.0.1 {1}.64.1.2 {2}.254.0.9 8.8.8.8", 172, 100, 169),
            100
        ),
        "[private address] [private address] [private address] 8.8.8.8"
    );
    assert_eq!(
        q_clean("* [ ] a\n  + [x] b\na@b", 100),
        "* ☐ a\n  + ☑ b\na@\u{200b}b"
    );
    assert_eq!(q_clean("äöå", 2), "äö");
}

fn first_turn(n: u64) -> Value {
    json!({"plan_turn": n, "kind": "first", "label": "tehas:plan", "by": "alice", "refine": false,
        "title": "Make it", "body": "The body.", "log": format!("start planning #{n} (labelled by alice)")})
}

#[test]
fn a_first_turn_writes_its_brief_begins_marks_says_and_starts_its_unit() {
    let mut t = world();
    let a = t.apply(json!([first_turn(7)]));
    assert_eq!(a.exit, None);
    assert_eq!(
        a.log,
        vec![
            "start planning #7 (labelled by alice)",
            "planning #7: turn 1 runs as tehas-plan-7.service"
        ]
    );
    assert_eq!(a.printed, vec!["the status line"]);
    assert_eq!(
        fs::read_to_string(t.dir.join("brief")).unwrap(),
        "# Make it\n\nThe body.\n"
    );
    let w = t.writes();
    assert!(w[0].starts_with("store plan-begin 7 --by alice --label tehas:plan --brief-file "));
    assert_eq!(w[1], "gh issue edit 7 --repo example-owner/example-product --add-label tehas:planning --remove-label tehas:plan --remove-label tehas:question --remove-label needs-human-attention");
    assert!(w[2].contains(
        "(authorized by @alice's `tehas:plan` label). Each turn ends in a comment here."
    ));
    // The unit's script has line ends of its own: the whole log is read.
    let unit = fs::read_to_string(t.dir.join("calls.log")).unwrap();
    assert!(unit.contains("start 7 /bin/bash -c "));
    assert!(unit.contains(" plan-turn "));
    assert!(unit.contains("/logs/dispatch-plan-7.log "));
    assert!(unit.contains("/state/plan-7.json.rc "));
    assert!(unit.contains("/state/plan-7.json.lock /opt/plan 7 --out "));
    assert!(unit.contains("-plan-7.keep --brief "));
    assert!(!unit.contains("--kind refine"));
}

#[test]
fn a_refine_spends_its_park_once_its_brief_is_written_and_says_why() {
    let mut t = world();
    let extra = t.dir.join("refine-brief.md");
    fs::write(&extra, "Refine it.\n\n").unwrap();
    let mut turn = first_turn(7);
    turn["label"] = json!("tehas:refine");
    turn["auto"] = json!("too-big");
    turn["brief_extra"] = json!(extra.to_string_lossy());
    turn["log"] = json!("start refining #7 (its cycle parked as too-big, started by alice)");
    let a = t.apply(json!([{"park_retry": 7, "how": "refine"}, turn.clone()]));
    assert_eq!(
        a.log[0],
        "start refining #7 (its cycle parked as too-big, started by alice)"
    );
    assert_eq!(fs::read_to_string(t.dir.join("brief")).unwrap(),
        "# Make it\n\nThe body.\n\n\n---\n\nRefine it.\n\nThis request is issue #7 itself. The issue you file replaces #7: #7 is not a duplicate of this plan.\n");
    let w = t.writes();
    let spend = w.iter().position(|c| c == "store park-retry 7").unwrap();
    let begin = w
        .iter()
        .position(|c| c.starts_with("store plan-begin 7"))
        .unwrap();
    assert!(spend < begin);
    assert!(w[begin].ends_with(" --auto"));
    assert!(w.iter().any(|c| c.contains("to refine this issue: its cycle parked as `too-big`, and the start of that cycle by @alice authorizes one refine.")));
    // Only a refine session's turn offers to file its plan as a new issue.
    let unit = fs::read_to_string(t.dir.join("calls.log")).unwrap();
    assert!(unit.contains("-plan-7.keep --brief ") && unit.contains(" --kind refine"));
    // A spend the store refuses ends the poll with its brief gone.
    let mut t = world();
    t.fail("store", "park-retry|conflict");
    let a = t.apply(json!([{"park_retry": 7, "how": "refine"}, turn]));
    assert_eq!(a.exit, Some(1));
    assert_eq!(
        a.log,
        vec!["ERROR: the refine of #7 could not be spent on record (conflict), not refining it"]
    );
    assert!(!t.calls().iter().any(|c| c.starts_with("store plan-begin")));
    assert_eq!(fs::read_dir(t.w.home.join("tmp")).unwrap().count(), 0);
}

#[test]
fn the_other_spends_warn_and_go_on() {
    let mut t = world();
    t.fail("store", "park-retry|busy");
    let a =
        t.apply(json!([{"park_retry": 7, "how": "outage"}, {"park_retry": 8, "how": "skipped"}]));
    assert_eq!(
        a.log,
        vec![
            "WARN: the retry of #7 after a provider outage could not be spent on record (busy)",
            "WARN: the refine of #8 could not be spent on record (busy)",
        ]
    );
    // One try for the outage, three for the refine.
    assert_eq!(
        t.calls()
            .iter()
            .filter(|c| c.as_str() == "store park-retry 7")
            .count(),
        1
    );
    assert_eq!(
        t.calls()
            .iter()
            .filter(|c| c.as_str() == "store park-retry 8")
            .count(),
        3
    );
    let mut t = world();
    t.fail("store", "park-retry|unparked");
    assert!(t
        .apply(json!([{"park_retry": 7, "how": "outage"}]))
        .log
        .is_empty());
}

#[test]
fn a_turn_the_store_will_not_begin_waits_or_ends_the_poll() {
    let mut t = world();
    t.fail("store", "plan-begin|running");
    let a = t.apply(json!([first_turn(7)]));
    assert_eq!(a.exit, None);
    assert_eq!(
        a.log[1],
        "skip #7: its planning session could not begin: store: running: stub"
    );
    assert_eq!(a.printed, vec!["the status line"]);
    let mut t = world();
    t.fail("store", "plan-begin|busy");
    let a = t.apply(json!([first_turn(7)]));
    assert_eq!(a.exit, Some(1));
    assert_eq!(
        a.log[1],
        "ERROR: could not write the record of #7, not starting a turn (busy)"
    );
    let mut t = world();
    t.fail("store", "plan-begin|absent");
    assert_eq!(t.apply(json!([first_turn(7)])).exit, Some(78));
}

#[test]
fn a_turn_whose_mark_fails_puts_its_record_back() {
    let mut t = world();
    t.fail("gh", "--add-label tehas:planning");
    let a = t.apply(json!([first_turn(7)]));
    assert_eq!(a.exit, Some(1));
    assert_eq!(
        a.log.last().unwrap(),
        "ERROR: could not mark #7 as planning, not starting a turn"
    );
    assert!(t
        .writes()
        .contains(&"store plan-begin 7 --undo".to_string()));
    assert!(!t.calls().iter().any(|c| c.starts_with("start ")));
    assert_eq!(fs::read_dir(t.w.home.join("tmp")).unwrap().count(), 0);
}

#[test]
fn a_turn_that_cannot_be_started_is_finished_as_not_started() {
    let mut t = world();
    t.fail("start", "plan-turn");
    plan_row(&t, json!({"turns": 0, "label": "tehas:plan"}));
    let a = t.apply(json!([first_turn(7)]));
    assert!(a
        .log
        .contains(&"ERROR: could not start the planning turn of #7".to_string()));
    assert!(t
        .writes()
        .iter()
        .any(|c| c.starts_with("store plan-end 7 --turn 1 --outcome not-started")));
}

#[test]
fn a_next_turn_is_given_the_answer_on_record_with_what_was_filed() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 2, "session": "s-1", "label": "tehas:plan"}),
    );
    fs::write(t.dir.join("answered"), json!({"api": 1, "verb": "plan-answer", "ok": true, "planning": {"answer": "Do A.\n",
        "filed": ["https://github.com/example-owner/example-product/issues/9", "https://github.com/example-owner/example-product/issues/9"]}}).to_string()).unwrap();
    fs::create_dir_all(t.dir.join("works")).unwrap();
    fs::write(t.dir.join("works/.tehas-workspaces"), "").unwrap();
    let a = t.apply(
        json!([{"plan_turn": 7, "kind": "next", "answer": "Do A.", "by": "mill-bot", "reply": 55,
        "log": "planning #7 continues (answer from mill-bot, the fixer)"}]),
    );
    assert_eq!(
        a.log,
        vec![
            "planning #7 continues (answer from mill-bot, the fixer)",
            "planning #7: turn 3 runs as tehas-plan-7.service"
        ]
    );
    assert_eq!(fs::read_to_string(t.dir.join("answer")).unwrap(), "Do A.\n");
    let w = t.writes();
    assert!(w.iter().any(|c| c
        .starts_with("store plan-answer 7 --turn 3 --by mill-bot --text-file ")
        && c.ends_with(" --millwright")));
    assert!(w.contains(&"gh issue edit 7 --repo example-owner/example-product --add-label tehas:planning --remove-label tehas:plan --remove-label tehas:question --remove-label needs-human-attention".to_string()));
    let keep = t.dir.join("works/tehas-work-plan-7.keep");
    assert_eq!(fs::read_to_string(keep.join("answer.md")).unwrap(),
        "Already filed in this session: https://github.com/example-owner/example-product/issues/9\n\nDo A.\n");
    let unit = fs::read_to_string(t.dir.join("calls.log")).unwrap();
    assert!(unit.contains(&format!(
        "--session s-1 --answer {}/answer.md --comment 55",
        keep.display()
    )));
    // No comment on a next turn.
    assert!(!w.iter().any(|c| c.starts_with("gh issue comment")));
    assert!(!unit.contains("--kind refine"));
}

#[test]
fn a_next_turn_of_a_refine_session_says_so() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:refine"}),
    );
    fs::write(
        t.dir.join("answered"),
        json!({"api": 1, "verb": "plan-answer", "ok": true, "planning": {"answer": "Do A.\n"}})
            .to_string(),
    )
    .unwrap();
    fs::create_dir_all(t.dir.join("works")).unwrap();
    fs::write(t.dir.join("works/.tehas-workspaces"), "").unwrap();
    t.apply(
        json!([{"plan_turn": 7, "kind": "next", "answer": "Do A.", "by": "alice",
        "log": "planning #7 continues (answer from alice)"}]),
    );
    let unit = fs::read_to_string(t.dir.join("calls.log")).unwrap();
    assert!(unit.contains("/answer.md --kind refine"));
}

#[test]
fn a_session_at_its_limit_of_turns_is_ended() {
    let mut t = world();
    t.w.host.plan_max_turns = 2;
    plan_row(
        &t,
        json!({"turns": 2, "session": "s-1", "label": "tehas:plan"}),
    );
    let a = t.apply(json!([{"plan_turn": 7, "kind": "next", "answer": "x", "by": "alice", "log": "planning #7 continues (answer from alice)"}]));
    assert_eq!(a.log.last().unwrap(), "planning #7 ended");
    assert!(t
        .writes()
        .iter()
        .any(|c| c.contains("This planning session reached its limit of 2 turns and was ended.")));
    assert!(!t.calls().iter().any(|c| c.starts_with("store plan-answer")));
}

#[test]
fn a_first_turns_brief_is_private_and_its_unit_goes_to_systemd_run() {
    let mut t = world();
    // The wait for the unit's lock is the thing under test, not its length.
    t.w.host.launch_poll = Duration::from_millis(200);
    t.w.host.plan_start_cmd = None;
    t.apply(json!([first_turn(7)]));
    assert_eq!(
        fs::read_to_string(t.dir.join("brief.mode")).unwrap().trim(),
        "600"
    );
    let log = fs::read_to_string(t.dir.join("calls.log")).unwrap();
    assert!(log.contains("start --user --quiet --collect --unit tehas-plan-7 -p RuntimeMaxSec=4500 /bin/bash -c [ ! -r \"$HOME/tehas/env.sh\" ] || . \"$HOME/tehas/env.sh\"; exec \"$@\" env-sh /bin/bash -c log=$1"));
}

#[test]
fn a_turn_runs_from_its_own_copy_of_the_code_when_the_host_has_one() {
    let mut t = world();
    t.w.host.launch_poll = Duration::from_millis(1);
    t.w.host.plan_start_cmd = None;
    t.w.host.pin_cmd = Some("/opt/tehas-cycle".into());
    t.apply(json!([first_turn(7)]));
    let log = fs::read_to_string(t.dir.join("calls.log")).unwrap();
    assert!(
        log.contains(".lock /opt/tehas-cycle pin plan 7 plan 7 --out "),
        "{log}"
    );
}

#[test]
fn a_next_turn_from_a_person_and_one_the_store_refuses() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan"}),
    );
    fs::create_dir_all(t.dir.join("works")).unwrap();
    fs::write(t.dir.join("works/.tehas-workspaces"), "").unwrap();
    t.apply(json!([{"plan_turn": 7, "kind": "next", "answer": "Do A.", "by": "alice", "log": "planning #7 continues (answer from alice)"}]));
    assert!(t.writes().iter().any(|c| c
        .starts_with("store plan-answer 7 --turn 2 --by alice --text-file ")
        && !c.contains("--millwright")));
    assert_eq!(
        fs::read_to_string(t.dir.join("works/tehas-work-plan-7.keep/answer.md")).unwrap(),
        "Do A.\n"
    );
    // An answer the store will not take waits, and the status still runs.
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan"}),
    );
    t.fail("store", "plan-answer|turn");
    let a = t
        .apply(json!([{"plan_turn": 7, "kind": "next", "answer": "x", "by": "alice", "log": "l"}]));
    assert_eq!(a.exit, None);
    assert_eq!(
        a.log[1],
        "skip #7: the answer could not begin a turn: store: turn: stub"
    );
    assert_eq!(a.printed, vec!["the status line"]);
    assert_eq!(fs::read_dir(t.w.home.join("tmp")).unwrap().count(), 0);
}

#[test]
fn a_next_turn_in_a_directory_tehas_did_not_make_starts_nothing() {
    let mut t = world();
    plan_row(
        &t,
        json!({"turns": 1, "session": "s-1", "label": "tehas:plan"}),
    );
    fs::create_dir_all(t.dir.join("works")).unwrap();
    let a = t
        .apply(json!([{"plan_turn": 7, "kind": "next", "answer": "x", "by": "alice", "log": "l"}]));
    assert_eq!(a.exit, Some(1));
    assert!(a
        .log
        .iter()
        .any(|l| l.ends_with("works exists and was not made by tehas, not touching it")));
    assert_eq!(
        a.log.last().unwrap(),
        "ERROR: could not write the answer for #7, not starting a turn"
    );
    assert!(t
        .writes()
        .contains(&"store plan-answer 7 --turn 2 --undo".to_string()));
    assert!(!t
        .calls()
        .iter()
        .any(|c| c.contains("--add-label tehas:planning")));
}

#[test]
fn a_spend_of_no_kind_apply_knows_is_left() {
    let mut t = world();
    let odd = json!({"park_retry": 7, "how": "someday"});
    assert_eq!(t.apply(json!([odd])).left, vec![odd]);
    assert!(t.calls().is_empty());
}

fn gates(n: u64, running: Value) -> Value {
    json!([{"units": n}, {"index_gate": running}, start_of(n, "label")])
}

#[test]
fn a_unit_systemd_cannot_load_ends_the_poll_before_any_mark() {
    let mut t = world();
    t.w.host.start_cmd = None;
    t.w.host.systemctl = t.dir.join("systemctl").to_string_lossy().into_owned();
    fs::write(t.dir.join("loadstate"), "not-found\n").unwrap();
    let a = t.apply(gates(7, json!([3])));
    assert_eq!(a.exit, Some(1));
    assert_eq!(a.log, vec!["ERROR: systemd cannot load tehas-cycle@.service (LoadState: not-found): run install.sh and its units step, and run tehas dispatch as the tehas user's own session"]);
    assert!(!t
        .calls()
        .iter()
        .any(|c| c.contains("--add-label tehas:running")));
    // With a start command of its own, systemd is not asked.
    let mut t = world();
    fs::write(t.dir.join("loadstate"), "not-found\n").unwrap();
    let a = t.apply(gates(7, json!([])));
    assert!(!a.log.iter().any(|l| l.contains("systemd cannot load")));
    assert!(!t.calls().iter().any(|c| c.contains("LoadState")));
}

#[test]
fn a_running_cycle_leaves_the_index_alone_and_the_start_refreshes_the_checkout() {
    let mut t = world();
    // The cycles counted are the locks held when the gate runs.
    let cycles = t.w.home.join("state/cycles");
    fs::create_dir_all(&cycles).unwrap();
    let held: Vec<fs::File> = [3, 5]
        .iter()
        .map(|n| {
            let f = fs::File::create(cycles.join(format!("{n}.lock"))).unwrap();
            f.lock().unwrap();
            f
        })
        .collect();
    let a = t.apply(gates(7, json!([])));
    drop(held);
    assert_eq!(a.exit, None);
    assert_eq!(
        a.log[0],
        "the code index stays as it is: a cycle is running on #3, #5"
    );
    let git: Vec<String> = t
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("git "))
        .collect();
    let co = t.w.host.checkout.clone();
    assert_eq!(
        git,
        vec![
            format!("git -C {co} status --porcelain --untracked-files=no"),
            format!("git -C {co} checkout -q main"),
            format!("git -C {co} fetch -q origin"),
            format!("git -C {co} merge -q --ff-only origin/main"),
        ]
    );
}

#[test]
fn a_dirty_checkout_keeps_its_branch_and_a_failed_fetch_is_said() {
    let mut t = world();
    fs::write(t.dir.join("dirty"), " M src/x\n").unwrap();
    t.fail("git", "fetch");
    let a = t.apply(gates(7, json!([3])));
    let co = t.w.host.checkout.clone();
    assert!(a.log.contains(&format!(
        "WARN: {co} has uncommitted changes, leaving its branch alone"
    )));
    assert!(a
        .log
        .contains(&format!("WARN: could not fast-forward {co}")));
    assert!(!t.calls().iter().any(|c| c.contains("checkout -q main")));
}

#[test]
fn a_busy_index_holds_the_starts_with_0() {
    let mut t = world();
    t.w.host.systemctl = t.dir.join("systemctl").to_string_lossy().into_owned();
    fs::write(t.dir.join("activestate"), "activating\n").unwrap();
    let a = t.apply(gates(7, json!([])));
    assert_eq!(a.exit, Some(0));
    assert_eq!(
        a.log.last().unwrap(),
        "holding: the cycles wait for the code index, which tehas-index.service is refreshing"
    );
    assert!(!t
        .calls()
        .iter()
        .any(|c| c.contains("--add-label tehas:running")));
}

#[test]
fn a_due_index_is_launched_and_holds_and_one_that_ran_lets_the_starts_go() {
    let mut t = world();
    t.w.host.systemctl = t.dir.join("systemctl").to_string_lossy().into_owned();
    let a = t.apply(gates(7, json!([])));
    assert_eq!(a.exit, Some(0));
    assert_eq!(a.log.last().unwrap(), "holding: the code index is refreshed first, as tehas-index.service; the cycles start on the poll after it");
    let log = fs::read_to_string(t.dir.join("calls.log")).unwrap();
    assert!(log.contains("start /bin/bash -c log=$1 rc=$2 again=$3; shift 3;"));
    assert!(log.contains("/opt/index --before-cycle"));
    // A launcher that ran the index to its end: its report, and the starts.
    let mut t = world();
    t.w.host.systemctl = t.dir.join("systemctl").to_string_lossy().into_owned();
    let script = t.dir.join("index-start");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\necho 1 >{}\n",
            t.w.home.join("state/index.rc").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    t.w.host.index_start_cmd = Some(script.to_string_lossy().into_owned());
    fs::write(t.w.home.join("state/dispatch.again"), "").unwrap();
    let a = t.apply(gates(7, json!([])));
    assert_eq!(a.exit, None);
    assert!(a
        .log
        .iter()
        .any(|l| l.starts_with("WARN: the code index was not refreshed, see ")));
    assert!(!t.w.home.join("state/dispatch.again").exists());
    assert!(t
        .calls()
        .iter()
        .any(|c| c.contains("--add-label tehas:running")));
}

#[test]
fn a_current_index_lets_the_starts_go() {
    let mut t = world();
    t.w.host.systemctl = t.dir.join("systemctl").to_string_lossy().into_owned();
    let state = t.w.home.join("state");
    fs::create_dir_all(state.join("index")).unwrap();
    fs::write(state.join("index.json"), r#"{"commit":"abc1234"}"#).unwrap();
    fs::write(state.join("index/code.db"), "x").unwrap();
    let a = t.apply(gates(7, json!([])));
    assert_eq!(a.exit, None);
    assert!(t
        .calls()
        .iter()
        .any(|c| c.contains("--add-label tehas:running")));
    // The start does not fetch a second time.
    assert_eq!(
        t.calls()
            .iter()
            .filter(|c| c.contains(" fetch -q origin"))
            .count(),
        1
    );
}

#[test]
fn a_planning_turn_refreshes_the_checkout_before_its_unit() {
    let mut t = world();
    t.apply(json!([first_turn(7)]));
    let c = t.calls();
    let fetch = c
        .iter()
        .position(|l| l.contains(" fetch -q origin"))
        .unwrap();
    let unit = c
        .iter()
        .position(|l| l.starts_with("start 7 /bin/bash"))
        .unwrap();
    assert!(fetch < unit);
}

#[test]
fn an_index_that_failed_at_the_tip_is_not_launched_again() {
    let mut t = world();
    t.w.host.systemctl = t.dir.join("systemctl").to_string_lossy().into_owned();
    // libexec/index writes the commit with its line end.
    fs::write(t.w.home.join("state/index.failed"), "abc1234\n").unwrap();
    let a = t.apply(gates(7, json!([])));
    assert_eq!(a.exit, None);
    assert!(!fs::read_to_string(t.dir.join("calls.log"))
        .unwrap()
        .contains("--before-cycle"));
    assert!(t
        .calls()
        .iter()
        .any(|c| c.contains("--add-label tehas:running")));
}

#[test]
fn an_index_that_holds_its_lock_holds_the_starts() {
    let mut t = world();
    let lock = fs::File::create(t.w.home.join("state/index.lock")).unwrap();
    lock.lock().unwrap();
    let a = t.apply(gates(7, json!([])));
    assert_eq!(a.exit, Some(0));
    assert_eq!(
        a.log.last().unwrap(),
        "holding: the cycles wait for the code index, which tehas-index.service is refreshing"
    );
}

#[test]
fn a_planning_turns_refresh_does_not_stand_in_for_a_starts() {
    let mut t = world();
    // The waits for the start's record, the stopped lock and the launched
    // unit are the thing under test, not their length.
    t.w.host.start_wait = Duration::from_millis(200);
    t.w.host.lock_poll = Duration::from_millis(200);
    t.w.host.launch_poll = Duration::from_millis(200);
    t.apply(json!([first_turn(7)]));
    t.apply(json!([start_of(8, "label")]));
    assert_eq!(
        t.calls()
            .iter()
            .filter(|c| c.contains(" fetch -q origin"))
            .count(),
        2
    );
}

fn left_open(t: &World, actions: Value) {
    fs::write(
        t.dir.join("open"),
        json!({"api": 1, "verb": "journal-open", "ok": true, "actions": actions}).to_string(),
    )
    .unwrap();
}

fn settled(t: &World) -> Vec<String> {
    t.calls()
        .into_iter()
        .filter(|c| c.starts_with("store journal-settle"))
        .collect()
}

/// An open label-add row of the journal, aimed at its issue and asking its
/// label: `state` 'intended' carries no attempted time, as the store leaves it.
fn open_label_add(id: u64, issue: u64, state: &str) -> Value {
    let mut row = json!({"id": id, "kind": "label-add", "issue": issue, "state": state,
        "intended_at": 990,
        "args": {"gh": ["issue", "edit", issue.to_string(), "--repo", "r", "--add-label", "tehas:running"]}});
    if state != "intended" {
        row["attempted_at"] = json!(991);
    }
    row
}

/// The same row aimed at its issue and taking its label off: the pair the
/// settling of a running cycle looks for.
fn open_label_swap(id: u64, issue: u64) -> Value {
    let mut row = open_label_add(id, issue, "attempted");
    row["args"]["gh"] = json!([
        "issue",
        "edit",
        issue.to_string(),
        "--repo",
        "r",
        "--remove-label",
        "tehas:ready",
        "--add-label",
        "tehas:running"
    ]);
    row
}

#[test]
fn a_poll_the_store_would_not_begin_still_settles_the_journal_it_holds() {
    // The row names the writes a poll makes. Reading the journal, and
    // settling what it holds, need no row: an answer with no time must not
    // stop them, only the writes.
    let mut t = world();
    t.w.poll_at = 0;
    fs::write(
        t.dir.join("poll-begin"),
        r#"{"api":1,"verb":"poll-begin","ok":true}"#,
    )
    .unwrap();
    left_open(
        &t,
        json!([{"id": 3, "kind": "label-add", "issue": 7, "state": "intended", "intended_at": 990,
         "args": {"gh": ["issue", "edit", "7", "--repo", "r", "--add-label", "tehas:running"]}}]),
    );
    let mut log = Vec::new();
    t.w.settle(1000, &mut log);
    assert_eq!(
        log,
        vec!["journal: action 3 (label-add on #7, 10s old) did not happen (it was never attempted); this poll decides it again"]
    );
    assert_eq!(
        settled(&t),
        vec!["store journal-settle --action 3 --abandoned"]
    );
}

#[test]
fn a_poll_begin_answer_without_a_time_makes_no_write() {
    // The row's time is what every record names. An answer without one is a
    // store that cannot begin the row: the write is not made under 0.
    let mut t = world();
    t.w.poll_at = 0;
    fs::write(
        t.dir.join("poll-begin"),
        r#"{"api":1,"verb":"poll-begin","ok":true}"#,
    )
    .unwrap();
    let a = t.apply(json!([label()]));
    assert_eq!(
        a.log,
        vec!["WARN: the state store began no poll row, so no write was made"]
    );
    // Neither the label nor the comment of the action: a gh call would be a
    // write made under no row. The begin of the row is all it asked for.
    assert_eq!(t.writes(), vec!["store poll-begin"], "{:#?}", t.writes());
    // Once for the first write, and no more: poll_at stays 0, so a second
    // begin would mean the poll went on writing.
    let c = t.calls();
    assert_eq!(
        c.iter()
            .filter(|l| l.starts_with("store poll-begin"))
            .count(),
        1
    );
    assert!(!c.iter().any(|l| l.contains("--poll 0")));
}

#[test]
fn a_poll_begin_answer_with_a_time_that_is_not_a_number_makes_no_write() {
    let mut t = world();
    t.w.poll_at = 0;
    fs::write(
        t.dir.join("poll-begin"),
        r#"{"api":1,"verb":"poll-begin","ok":true,"at":"x"}"#,
    )
    .unwrap();
    let a = t.apply(json!([label()]));
    assert_eq!(
        a.log,
        vec!["WARN: the state store began no poll row, so no write was made"]
    );
    // writes(): the gh and docker calls plus the row's begin, so this holds
    // the write back and asks for the row once.
    assert_eq!(t.writes(), vec!["store poll-begin"]);
    assert!(!t.calls().iter().any(|l| l.contains("--poll 0")));
}

#[test]
fn a_poll_begin_answer_with_a_time_of_zero_or_below_makes_no_write() {
    for at in ["0", "-5"] {
        let mut t = world();
        t.w.poll_at = 0;
        fs::write(
            t.dir.join("poll-begin"),
            format!(r#"{{"api":1,"verb":"poll-begin","ok":true,"at":{at}}}"#),
        )
        .unwrap();
        let a = t.apply(json!([label()]));
        assert_eq!(
            a.log,
            vec!["WARN: the state store began no poll row, so no write was made"],
            "at {at}"
        );
        // The write refused before the row was begun: no gh call, and the
        // begin of the row is the only store write of the poll.
        assert_eq!(t.writes(), vec!["store poll-begin"], "at {at}");
        assert!(!t.calls().iter().any(|l| l.contains("--poll 0")), "at {at}");
    }
}

#[test]
fn a_poll_whose_row_the_store_would_not_begin_says_it_once() {
    // Two actions, each with a label and a comment: no write is made, and the
    // poll logs the refusal once and not once per write or per action. The
    // store is asked for the row once per action, since a refused answer
    // leaves no row for the next write of the same action to name.
    let mut t = world();
    t.w.poll_at = 0;
    fs::write(
        t.dir.join("poll-begin"),
        r#"{"api":1,"verb":"poll-begin","ok":true}"#,
    )
    .unwrap();
    let one = |n: u64| {
        json!({"issue": n, "remove": ["tehas:ready"], "add": ["needs-human-attention"],
            "fail": "WARN: label change failed",
            "log": format!("clear #{n}: done"), "say": "Tehas took it off."})
    };
    let a = t.apply(json!([one(7), one(8)]));
    assert_eq!(
        a.log,
        vec!["WARN: the state store began no poll row, so no write was made"]
    );
    // Two asks, one per action: the label of each is refused and its comment
    // never asked for. No record and no gh call anywhere.
    assert_eq!(t.writes(), vec!["store poll-begin", "store poll-begin"]);
    assert!(!t.calls().iter().any(|l| l.starts_with("store journal-")));
    assert!(!t.calls().iter().any(|l| l.starts_with("gh ")));
    assert_eq!(t.w.poll_at, 0);
}

#[test]
fn the_first_write_begins_the_poll_row_once_and_records_under_it() {
    let mut t = world();
    t.w.poll_at = 0;
    let a = t.apply(json!([label()]));
    assert_eq!(a.log, vec!["clear #7: done"]);
    let c = t.calls();
    assert_eq!(
        c.iter()
            .filter(|l| l.starts_with("store poll-begin"))
            .count(),
        1
    );
    assert_eq!(t.w.poll_at, 2000);
    // Both writes of the label: one row, named twice, never 0.
    let polls: Vec<&String> = c
        .iter()
        .filter(|l| l.starts_with("store journal-record"))
        .collect();
    assert_eq!(polls.len(), 2);
    assert!(polls.iter().all(|l| l.contains("--poll 2000")));
}

#[test]
fn a_journal_the_store_would_not_answer_is_not_read_as_an_empty_one() {
    // journal_open says None, not an empty list: a poll that skips the work of
    // a journal holding nothing must still do it when the read failed.
    let mut t = world();
    // The stub's way of refusing a verb: a line naming its word, then exit 1.
    fs::write(t.dir.join("fail-store"), "journal-open|busy\n").unwrap();
    let mut log = Vec::new();
    assert!(t.w.journal_open(&mut log).is_none());
    assert_eq!(
        log,
        ["WARN: the journal's open actions could not be read (busy); the next poll tries again"]
    );
    assert!(settled(&t).is_empty());
}

#[test]
fn a_write_never_attempted_is_abandoned_and_one_that_landed_is_done() {
    let mut t = world();
    left_open(
        &t,
        json!([
            open_label_add(3, 7, "intended"),
            open_label_swap(4, 8),
            open_label_add(5, 9, "attempted"),
        ]),
    );
    fs::write(
        t.dir.join("issue-8"),
        r#"{"labels":[{"name":"tehas:running"}]}"#,
    )
    .unwrap();
    fs::write(
        t.dir.join("issue-9"),
        r#"{"labels":[{"name":"tehas:ready"}]}"#,
    )
    .unwrap();
    let mut log = Vec::new();
    t.w.settle(1000, &mut log);
    assert_eq!(log, vec![
        "journal: action 3 (label-add on #7, 10s old) did not happen (it was never attempted); this poll decides it again",
        "journal: action 4 (label-add on #8, 10s old) was done",
        "journal: action 5 (label-add on #9, 10s old) did not happen (the labels are not as it wanted them); this poll decides it again",
    ]);
    let s = settled(&t);
    assert_eq!(s[0], "store journal-settle --action 3 --abandoned");
    assert!(s[1].starts_with("store journal-settle --action 4 --result ok --evidence "));
    assert_eq!(s[2], "store journal-settle --action 5 --abandoned");
}

#[test]
fn a_comment_is_found_by_its_sum_and_one_not_there_is_not_posted_again() {
    use sha2::{Digest, Sha256};
    let sum: String = Sha256::digest(b"Tehas cycle started.")
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    let open = |id: u64| {
        json!({"id": id, "kind": "comment-post", "issue": 7, "state": "attempted", "intended_at": 1792000000,
        "attempted_at": 1792000000, "args": {"gh": ["issue", "comment", "7"], "input": {"sha256": sum, "bytes": 20}}})
    };
    let mut t = world();
    left_open(&t, json!([open(6)]));
    fs::write(
        t.dir.join("comments"),
        json!([{"id": 41, "body": "Tehas cycle started.", "user": {"login": "tehas-bot"},
        "created_at": "2026-10-15T00:00:00Z", "updated_at": "2026-10-15T00:00:00Z"}])
        .to_string(),
    )
    .unwrap();
    let mut log = Vec::new();
    t.w.settle(1792000010, &mut log);
    assert_eq!(
        log,
        vec!["journal: action 6 (comment-post on #7, 10s old) was done"]
    );
    let mut t = world();
    left_open(&t, json!([open(6)]));
    let mut log = Vec::new();
    t.w.settle(1792000010, &mut log);
    assert!(log[0].contains("did not happen (no comment of the bot with its text is there)"));
    assert!(!t.calls().iter().any(|c| c.starts_with("gh issue comment")));
}

#[test]
fn a_question_posted_as_json_is_found_by_the_sum_of_its_body() {
    let mut t = world();
    let args: Vec<String> = [
        "api",
        "-X",
        "POST",
        "repos/r/issues/7/comments",
        "--input",
        "-",
    ]
    .map(String::from)
    .to_vec();
    let input = json!({"body": "Which?"}).to_string();
    t.w.write(7, "comment-post", args, Some(&input), None);
    let record = t
        .calls()
        .into_iter()
        .find(|c| c.starts_with("store journal-record"))
        .unwrap();
    let call: Value = serde_json::from_str(
        record
            .split(" --args ")
            .nth(1)
            .unwrap()
            .split(" --before ")
            .next()
            .unwrap(),
    )
    .unwrap();
    let mut t = world();
    left_open(
        &t,
        json!([{"id": 6, "kind": "comment-post", "issue": 7, "state": "attempted", "intended_at": 1792000000,
        "attempted_at": 1792000000, "args": call}]),
    );
    // GitHub's clock a few seconds behind the host's.
    fs::write(
        t.dir.join("comments"),
        json!([{"id": 41, "body": "Which?", "user": {"login": "tehas-bot"},
        "created_at": crate::decide::iso(1791999995), "updated_at": crate::decide::iso(1791999995)}])
        .to_string(),
    )
    .unwrap();
    let mut log = Vec::new();
    t.w.settle(1792000010, &mut log);
    assert_eq!(
        log,
        vec!["journal: action 6 (comment-post on #7, 10s old) was done"]
    );
}

#[test]
fn a_malformed_open_action_is_left_open_and_reads_nothing() {
    // None of the three fields may be stood in for by 0: it would settle an
    // unknown row, read issue 0 for ever, or open the comment window at the
    // epoch and call an old comment of the same text the evidence.
    let mut t = world();
    let mut no_issue = open_label_add(4, 7, "attempted");
    no_issue.as_object_mut().unwrap().remove("issue");
    let mut no_time = open_label_add(5, 7, "attempted");
    no_time
        .as_object_mut()
        .unwrap()
        .insert("attempted_at".into(), Value::Null);
    let mut string_id = open_label_add(6, 7, "attempted");
    string_id
        .as_object_mut()
        .unwrap()
        .insert("id".into(), json!("6"));
    // A row whose kind is not text names no kind at all: the line says so
    // with an empty one, and the row is held to have no time until it says
    // it was attempted. 'intended' stands in for the state it should have.
    let mut numeric_kind = open_label_add(8, 7, "intended");
    numeric_kind
        .as_object_mut()
        .unwrap()
        .insert("kind".into(), json!(12));
    // A row whose state is not text is not 'intended', so a time is asked of
    // it: the WARN says so, and the row is left open with no read made of it
    // at all. Its issue is answered by the stub, so were the guard to let it
    // through it would be settled from that read.
    let mut numeric_state = open_label_add(10, 7, "intended");
    numeric_state
        .as_object_mut()
        .unwrap()
        .insert("state".into(), json!(7));
    // A state that is a number is held to the same rule, and the line names
    // neither its state nor anything else of it.
    let mut numeric_state_and_time = open_label_add(11, 7, "intended");
    numeric_state_and_time
        .as_object_mut()
        .unwrap()
        .insert("state".into(), json!(10));
    left_open(
        &t,
        json!([
            no_issue,
            no_time,
            string_id,
            numeric_kind,
            numeric_state,
            numeric_state_and_time
        ]),
    );
    // The stub answers a read of issue 0 with the label the rows want, so a
    // row that fell through the guard would be settled from it.
    fs::write(
        t.dir.join("issue-0"),
        r#"{"labels":[{"name":"tehas:running"}]}"#,
    )
    .unwrap();
    let mut log = Vec::new();
    t.w.settle(1000, &mut log);
    assert_eq!(log, vec![
        "WARN: the journal's attempted action label-add is malformed (issue is not a positive integer; id 4); it is left open for a person to settle with store journal-settle --action N",
        "WARN: the journal's attempted action label-add is malformed (attempted_at is not a positive integer; id 5, #7); it is left open for a person to settle with store journal-settle --action N",
        "WARN: the journal's attempted action label-add is malformed (id is not a positive integer; #7); it is left open for a person to settle with store journal-settle --action N",
        // A kind that is not text names nothing in the line, and a state of
        // 'intended' is still asked for no time, so the row is well formed
        // and abandoned by the read it does not make.
        "journal: action 8 ( on #7, 10s old) did not happen (it was never attempted); this poll decides it again",
        // A state that is not text is not 'intended', so the row is malformed
        // without a time: a text 'intended' beside it would have been settled
        // with none.
        "WARN: the journal's  action label-add is malformed (attempted_at is not a positive integer; id 10, #7); it is left open for a person to settle with store journal-settle --action N",
        // A state that is a number is held to the same rule, and is named by
        // nothing at all in the line.
        "WARN: the journal's  action label-add is malformed (attempted_at is not a positive integer; id 11, #7); it is left open for a person to settle with store journal-settle --action N",
    ]);
    // Nothing of a malformed row was read or settled: an arm of the guard
    // that let one through would have read its issue, and the stub answers
    // issues 0 and 7 with the labels these rows want, so a read of either
    // would settle it. The only row settled is the intended one whose kind is
    // not text: it is well formed.
    assert_eq!(settled(&t), ["store journal-settle --action 8 --abandoned"]);
    assert!(
        !t.calls()
            .iter()
            .any(|c| c.starts_with("gh api repos/example-owner/example-product/issues/0")),
        "a malformed action was read: {:?}",
        t.calls()
    );
    assert!(
        !t.calls()
            .iter()
            .any(|c| c.starts_with("gh api repos/example-owner/example-product/issues/7")),
        "a malformed action was read: {:?}",
        t.calls()
    );
}

#[test]
fn an_intended_action_is_asked_for_no_time() {
    // What only this says: the guard must let an intended row with no time
    // through, and not ask it for one. The rows of the two states, and the
    // reads each one makes, are the ones the older test settles.
    let mut t = world();
    left_open(&t, json!([open_label_add(3, 7, "intended")]));
    let mut log = Vec::new();
    t.w.settle(1000, &mut log);
    assert_eq!(log, vec![
        "journal: action 3 (label-add on #7, 10s old) did not happen (it was never attempted); this poll decides it again",
    ]);
    assert_eq!(settled(&t), ["store journal-settle --action 3 --abandoned"]);
    assert!(
        !t.calls().iter().any(|c| c.starts_with("gh api")),
        "an intended action was read: {:?}",
        t.calls()
    );
}

#[test]
fn an_action_a_read_cannot_settle_stays_open() {
    let mut t = world();
    left_open(
        &t,
        json!([{"id": 3, "kind": "issue-close", "issue": 7, "state": "attempted", "intended_at": 990,
        "attempted_at": 991, "args": {"gh": ["issue", "close", "7"]}}]),
    );
    t.fail("gh", "repos/example-owner/example-product/issues/7");
    let mut log = Vec::new();
    t.w.settle(1000, &mut log);
    assert_eq!(log, vec!["WARN: the journal's action 3 (issue-close on #7, 10s old) could not be settled: gh api repos/example-owner/example-product/issues/7 failed; the next poll tries again"]);
    assert!(settled(&t).is_empty());
}

#[test]
fn a_plans_issue_edit_is_settled_by_its_title() {
    let mut t = world();
    let edit = |id: u64, title: &str| {
        json!({"id": id, "kind": "issue-edit", "issue": 7, "state": "attempted", "intended_at": 990, "attempted_at": 991,
        "args": {"gh": ["issue", "edit", "7", "--repo", "r", "--title", title, "--body-file", "-"]}})
    };
    left_open(
        &t,
        json!([edit(3, "feat: faster search"), edit(4, "feat: other")]),
    );
    fs::write(
        t.dir.join("issue-7"),
        r#"{"title":"feat: faster search","body":"Outcome","labels":[]}"#,
    )
    .unwrap();
    let mut log = Vec::new();
    t.w.settle(1000, &mut log);
    assert_eq!(log, vec![
        "journal: action 3 (issue-edit on #7, 10s old) was done",
        "journal: action 4 (issue-edit on #7, 10s old) did not happen (its body does not say what it replaces); this poll decides it again",
    ]);
}
