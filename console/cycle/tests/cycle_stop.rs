//! A stop or a hangup while a cycle runs, and the calls to GitHub it makes
//! again when they are refused.

mod common;

use common::{count_lines, has, lacks, said, site, through, twin, Run, World};
use serde_json::{json, Value};
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};
use tehas_ports::fake::{BlueprintsOp, Fault, Faulty, Op, TicketsOp};
use tehas_ports::Refusal;

const PR9: &str = "https://github.com/acme/widget/pull/9";

fn pr9(w: &World) {
    w.fx("pr.txt", &format!("{PR9}\n"));
}

/// The runner notes the cycle's pid (its parent) before what it does.
const PID: &str = r#"echo "$PPID" >"$T/calls/cycle.pid""#;

/// A runner whose trap of TERM is set before it says it is ready.
const TRAPPED: &str = r#"sleep 30 & s=$!
trap "kill \$s; : >\"\$T/calls/work.term\"; exit 2" TERM
: >"$T/calls/work.ready"
wait "$s""#;

/// The writes of the report of a runner that exits 2, as one does on a stop,
/// made in process: the report after a stop decides the same writes, and
/// makes each with one try (apply.rs, `after_a_stop_each_write_is_one_try`).
fn stopped_said(name: &str) -> String {
    let w = World::new(name);
    w.work("exit 2");
    said(&w.report_writes(&site()))
}

/// A handoff on feature/x with pull request #9 open.
fn handed_off(w: &mut World) {
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(w);
}

/// A cycle of #7 sent `signal` once `calls/<ready>` is there, the wait for it
/// bounded so a cycle that never gets there fails the case rather than hang.
/// The run, and how long the cycle took to end after the signal.
fn signalled(w: &World, ready: &str, signal: &str) -> (Run, Duration) {
    let calls = w.t().join("calls");
    std::thread::scope(|s| {
        let sender = s.spawn(|| {
            for _ in 0..100 {
                if calls.join(ready).exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let pid = fs::read_to_string(calls.join("cycle.pid")).expect("no pid of the cycle");
            let pid = pid.trim().to_string();
            let comm = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            assert_eq!(comm.trim(), "tehas-cycle", "the pid is the cycle's");
            let at = Instant::now();
            let ok = Command::new("kill")
                .args([&format!("-{signal}"), &pid])
                .status()
                .unwrap();
            assert!(ok.success());
            at
        });
        let r = w.run7();
        let at = sender.join().unwrap();
        (r, at.elapsed())
    })
}

#[test]
fn a_stop_is_passed_on_to_the_runner_and_the_cycle_is_still_reported() {
    let w = World::new("st-term");
    w.work(&format!("{PID}\n{TRAPPED}"));
    let (r, _) = signalled(&w, "work.ready", "TERM");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(w.t().join("calls/work.term").exists());
    has(
        &stopped_said("st-term-writes"),
        "unmark State(Running)\nmark State(NeedsPerson)",
    );
    has(&w.gh(), "(runner exit 2)");
}

#[test]
fn a_stop_that_arrives_while_the_report_is_made_does_not_cut_the_report_short() {
    let w = World::new("st-report");
    // The first mark taken off takes a second and says when it starts.
    w.touch("edit-slow");
    w.work(PID);
    let (r, _) = signalled(&w, "edit.started", "TERM");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(w.t().join("calls/edit.started").exists());
    // After the stop the park's question is still asked, and kept: its mark
    // went on, or the question would be taken off the record.
    has(&w.gh(), "Tehas cycle stopped without a pull request");
    assert_eq!(w.park()["question"], 4002);
    let q = w.row(&["question-known", "7", "--id", "4002"]);
    assert_eq!(json!([q["known"], q["answered"]]), json!([true, false]));
    // The writes it makes, in tehas's terms.
    let (_, _, said) = twin("st-report-writes", |w| w.work(PID));
    has(&said, "unmark State(Running)\nmark State(NeedsPerson)");
    has(&said, "mark State(Question)");
}

#[test]
fn a_stop_while_github_does_not_answer_is_reported_with_one_bounded_try_a_call_and_no_status_refresh(
) {
    let mut w = World::new("st-hangs");
    w.env("TEHAS_CYCLE_STOP_GH_WAIT", "1");
    w.ended("running", "develop", "feature/x", "");
    // GitHub stops answering once the cycle runs.
    w.work(&format!(
        r#"{PID}
sleep 30 & s=$!
trap "kill \$s; exit 2" TERM
: >"$T/fx/gh-hangs"
: >"$T/calls/work.ready"
wait "$s""#
    ));
    let (r, took) = signalled(&w, "work.ready", "TERM");
    assert_eq!(r.rc, 0, "{}", r.out);
    // Four calls, each cut after a second. Three tries of each, or the
    // default bound of 20 seconds, would take far longer. The report ignores
    // TERM, so a bound that only sent TERM would not cut a call.
    assert!(took <= Duration::from_secs(15), "{took:?}");
    // One try of each write after a stop: apply.rs,
    // `after_a_stop_each_write_is_one_try`.
    let gh = w.gh();
    assert_eq!(count_lines(&gh, "pr list "), 1);
    assert_eq!(w.calls("fake-status"), "");
    // A call cut at its bound is a write that failed, or one GitHub may have
    // taken that no read could settle (the read hangs too): under load either.
    assert!(
        r.out.contains("WARN: label change on #7 failed")
            || r.out.contains("WARN: a write on #7 may have been posted"),
        "{}",
        r.out
    );
}

#[test]
fn a_cycle_that_was_not_stopped_still_refreshes_the_status_and_tries_three_times() {
    let w = World::new("st-notstopped");
    w.run7();
    assert_eq!(w.calls("fake-status").lines().count(), 1);
    let (_, _, sent) = through("st-notstopped-writes", |_| {}, refuse_unmark);
    assert_eq!(count_lines(&sent, "unmark State(Running)"), 3);
}

#[test]
fn a_hangup_is_a_stop_too_the_cycle_is_still_reported() {
    let w = World::new("st-hup");
    w.work(&format!("{PID}\n{TRAPPED}"));
    let (r, _) = signalled(&w, "work.ready", "HUP");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(w.t().join("calls/work.term").exists());
    has(
        &stopped_said("st-hup-writes"),
        "unmark State(Running)\nmark State(NeedsPerson)",
    );
}

/// The fake, refusing every mark taken off the issue.
fn refuse_unmark(f: tehas_ports::fake::FakeSite) -> Faulty<tehas_ports::fake::FakeSite> {
    Faulty::new(f).on(
        Op::Tickets(TicketsOp::Unmark),
        Fault::Refused(Refusal::Invalid),
    )
}

#[test]
fn a_label_change_that_github_refuses_once_is_made_again() {
    let (_, applied, sent) = through("st-label-once", handed_off, |f| refuse_unmark(f).times(1));
    assert_eq!(count_lines(&sent, "unmark State(Running)"), 2);
    has(&applied, "unmark State(Running)\nmark State(AwaitingMerge)");
    lacks(&applied, "could not be changed");
    has(&applied, "Pull request waiting for a human");
}

#[test]
fn after_three_refusals_it_gives_up_says_so_in_the_log_and_still_reports() {
    let (_, applied, sent) = through("st-label-three", handed_off, refuse_unmark);
    assert_eq!(count_lines(&sent, "unmark State(Running)"), 3);
    lacks(&sent, "mark State(AwaitingMerge)");
    has(&applied, "Pull request waiting for a human");
}

#[test]
fn a_run_that_ended_as_st_is_not_said_to_wait_for_a_merge_though_a_pull_request_is_open() {
    for st in ["failed", "aborted", "unknown", "running"] {
        let (w, r, said) = twin("st-not-merge", |w| {
            w.ended(st, "review", "feature/x", "");
            pr9(w);
        });
        assert_eq!(r.rc, 0, "{st}");
        let gh = w.gh();
        lacks(&said, "State(AwaitingMerge)");
        has(&said, "unmark State(Running)\nmark State(NeedsPerson)");
        has(
            &gh,
            "Its branch has an open pull request: https://github.com/acme/widget/pull/9",
        );
        has(
            &gh,
            &format!("Tehas cycle stopped with pull request #9 open: status `{st}`"),
        );
        lacks(&gh, "without a pull request");
    }
}

#[test]
fn a_stop_at_commit_pr_with_its_draft_pull_request_open_names_it_as_a_draft() {
    let (w, r, said) = twin("st-draft", |w| {
        w.ended("running", "commit-pr", "feature/x", "");
        w.fx("pr.txt", "https://github.com/acme/widget/pull/67\n");
        w.touch("pr-draft");
    });
    assert_eq!(r.rc, 0);
    let gh = w.gh();
    has(&gh, "pr list --repo acme/widget --head feature/x --author tehas-bot[bot] --state all --json url,state,body,headRefName,isDraft,createdAt");
    has(&gh, "Tehas cycle stopped with pull request #67 open (draft): status `running`, last step `commit-pr`");
    has(&said, "unmark State(Running)\nmark State(NeedsPerson)");
    lacks(&gh, "without a pull request");
}

#[test]
fn a_stop_at_commit_pr_with_no_pull_request_says_it_has_none() {
    let w = World::new("st-nopr");
    w.ended("running", "commit-pr", "feature/x", "");
    w.fx("pr.json", "[]\n");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    has(
        &w.gh(),
        "Tehas cycle stopped without a pull request: status `running`, last step `commit-pr`",
    );
    lacks(&w.gh(), "open pull request");
}

fn ended_at(w: &World) -> bool {
    !w.row(&["cycle-show", "7"])["cycle"]["ended_at"].is_null()
}

/// The kinds and states of the journal's open actions.
fn journal_open(w: &World) -> Value {
    let open = w.store_ok(&["journal-open"]);
    let rows: Vec<Value> = open["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| json!([a["kind"], a["state"]]))
        .collect();
    json!(rows)
}

/// The report's note on the issue, as `said` says it.
const NOTE: &str = "note: ";

/// The fake, its notes on the issue failing as `fault` says, `times` times.
fn notes_fail(
    fault: Fault,
    times: u32,
) -> impl Fn(tehas_ports::fake::FakeSite) -> Faulty<tehas_ports::fake::FakeSite> {
    move |f| {
        let site = Faulty::new(f).on(Op::Tickets(TicketsOp::Note), fault);
        if times > 0 {
            site.times(times)
        } else {
            site
        }
    }
}

#[test]
fn a_comment_that_github_refuses_once_is_posted_on_the_second_try() {
    let mut w = World::new("st-comment-once");
    handed_off(&mut w);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert!(ended_at(&w));
    let (_, applied, sent) = through(
        "st-comment-once-writes",
        handed_off,
        notes_fail(Fault::Refused(Refusal::Invalid), 1),
    );
    assert_eq!(count_lines(&sent, NOTE), 2);
    assert_eq!(count_lines(&applied, NOTE), 1);
    has(&applied, "Pull request waiting for a human");
}

#[test]
fn after_three_refusals_the_comment_is_said_once_and_the_cycle_still_ends_its_record() {
    let mut w = World::new("st-comment-three");
    handed_off(&mut w);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert!(ended_at(&w));
    let (_, applied, sent) = through(
        "st-comment-three-writes",
        handed_off,
        notes_fail(Fault::Refused(Refusal::Invalid), 0),
    );
    assert_eq!(count_lines(&sent, NOTE), 3);
    assert_eq!(count_lines(&applied, NOTE), 0);
}

#[test]
fn a_comment_github_answered_with_a_5xx_and_did_not_take_is_posted_again_after_the_read() {
    let (w, applied, sent) = through(
        "st-5xx-not-taken",
        handed_off,
        notes_fail(Fault::Server { applied: false }, 1),
    );
    assert_eq!(count_lines(&sent, NOTE), 2);
    assert_eq!(count_lines(&applied, NOTE), 1);
    // The row of the note that did not land is closed as not sent.
    assert_eq!(journal_open(&w), json!([]));
}

#[test]
fn a_comment_github_answered_with_a_5xx_and_took_is_not_posted_again() {
    let (w, applied, sent) = through(
        "st-5xx-taken",
        handed_off,
        notes_fail(Fault::Server { applied: true }, 1),
    );
    assert_eq!(count_lines(&sent, NOTE), 1);
    assert_eq!(count_lines(&applied, NOTE), 1);
    // Read back and settled: no row is left open.
    assert_eq!(journal_open(&w), json!([]));
}

#[test]
fn a_comment_github_answered_with_a_5xx_that_no_read_settles_is_not_posted_again() {
    let mut w = World::new("st-5xx-unsettled");
    handed_off(&mut w);
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert!(ended_at(&w));
    let (w, _, sent) = through("st-5xx-unsettled-writes", handed_off, |f| {
        notes_fail(Fault::Server { applied: true }, 0)(f)
            .on(Op::Blueprints(BlueprintsOp::Comments), Fault::Unreachable)
    });
    assert_eq!(count_lines(&sent, NOTE), 1);
    assert_eq!(journal_open(&w), json!([["comment-post", "unknown"]]));
}

#[test]
fn a_list_of_pull_requests_that_github_refuses_once_is_asked_for_again() {
    let (w, _, said) = twin("st-list-once", |w| {
        handed_off(w);
        w.fx("pr-fails-times", "1\n");
    });
    assert_eq!(count_lines(&w.gh(), "pr list "), 2);
    has(&said, "unmark State(Running)\nmark State(AwaitingMerge)");
}

/// A handoff on feature/x whose pull requests GitHub would not list.
fn unlisted(w: &World) -> Run {
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(w);
    w.fx("pr-fails-times", "3\n");
    w.run7()
}

#[test]
fn after_three_refusals_the_cycle_is_parked_as_one_whose_pull_request_could_not_be_looked_up() {
    let w = World::new("st-list-three");
    let r = unlisted(&w);
    let twin = World::new("st-list-three-writes");
    twin.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&twin);
    twin.fx("pr-fails-times", "3\n");
    let said = said(&twin.report_writes(&site()));
    let gh = w.gh();
    assert_eq!(count_lines(&gh, "pr list "), 3);
    has(
        &r.out,
        "WARN: the pull requests of feature/x could not be read",
    );
    has(&said, "unmark State(Running)\nmark State(NeedsPerson)");
    has(&gh, "its pull request could not be looked up: GitHub did not list the pull requests of the branch `feature/x`");
    lacks(&gh, "stopped without a pull request");
    lacks(&said, "State(AwaitingMerge)");
}

// The two cases after it go on from where the one above left the host.
#[test]
fn the_branch_whose_pull_request_could_not_be_looked_up_stays_on_record() {
    let w = World::new("st-branch-kept");
    unlisted(&w);
    assert_eq!(w.row(&["cycle-show", "7"])["cycle"]["branch"], "feature/x");
}

#[test]
fn the_branch_is_on_record_with_every_end_and_an_earlier_cycles_is_not_the_next_ones() {
    let w = World::new("st-branch-each");
    unlisted(&w);
    w.ended("handoff", "commit-pr", "feature/y", "");
    w.run7();
    let c = w.row(&["cycle-show", "7"])["cycle"].clone();
    assert_eq!(json!([c["attempt"], c["branch"]]), json!([2, "feature/y"]));
    // The second cycle's report, in process after the same first cycle.
    let twin = World::new("st-branch-each-writes");
    unlisted(&twin);
    twin.ended("handoff", "commit-pr", "feature/y", "");
    has(
        &said(&twin.report_writes(&site())),
        "Pull request waiting for a human",
    );
    fs::remove_file(w.fx.join("result.json")).unwrap();
    w.run7();
    let c = w.row(&["cycle-show", "7"])["cycle"].clone();
    assert_eq!(json!([c["attempt"], c["branch"]]), json!([3, null]));
}

#[test]
fn a_branch_name_the_container_wrote_is_reduced_before_it_is_recorded() {
    let w = World::new("st-branch-reduced");
    w.ended("handoff", "commit-pr", "feature/x`;@y$z", "");
    w.fx("pr-fails-times", "3\n");
    w.run7();
    assert_eq!(
        w.row(&["cycle-show", "7"])["cycle"]["branch"],
        "feature/xyz"
    );
    has(&w.gh(), "the branch `feature/xyz`");
}

#[test]
fn when_the_label_cannot_be_changed_the_comment_says_so() {
    let (_, applied, _) = through("st-label-said", handed_off, refuse_unmark);
    has(&applied, "Pull request waiting for a human: https://github.com/acme/widget/pull/9 The label of this issue could not be changed");
}

#[test]
fn and_a_comment_after_a_label_change_that_worked_does_not() {
    let w = World::new("st-label-fine");
    w.ended("handoff", "commit-pr", "feature/x", "");
    pr9(&w);
    w.run7();
    lacks(&w.gh(), "could not be changed");
}

#[test]
fn the_question_label_is_tried_three_times() {
    // The park's own mark goes on; the question's does not.
    let (_, applied, sent) = through(
        "st-question",
        |_| {},
        |f| {
            Faulty::new(f)
                .on(
                    Op::Tickets(TicketsOp::Mark),
                    Fault::Refused(Refusal::Invalid),
                )
                .after(1)
        },
    );
    assert_eq!(count_lines(&sent, "mark State(Question)"), 3);
    has(&applied, "A reply to the choices above does nothing");
}

#[test]
fn a_cycle_that_parks_takes_the_build_output_of_its_kept_workspace_as_it_ends() {
    let w = World::new("st-clean");
    w.ended("failed", "develop", "", "");
    w.run7();
    has(
        &w.calls("fake-console"),
        &format!("clean ended {} target node_modules", w.workdir().display()),
    );
}

#[test]
fn and_a_failure_to_do_so_is_a_warning() {
    let w = World::new("st-clean-fails");
    w.ended("failed", "develop", "", "");
    w.stub("fake-console", "exit 1");
    let r = w.run7();
    has(&r.out, "was not removed");
}
