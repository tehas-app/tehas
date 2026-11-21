//! The issue events tehasbot status records in the factory's analytics:
//! awaiting, stopped, merged and closed, each when the snapshot it writes
//! differs from the one before.

mod common;

use common::*;

/// The answers of gh for these cases: the search and the shipped work from
/// the fixtures, nothing else open, the epics read empty.
const GH: &str = r####"case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) echo "[]" ;;
  "issue view"*) echo "" ;;
  "issue edit"*) ;;
  "api repos/"*) echo "[]" ;;
  "api graphql"*subIssues*) echo "{\"data\":{\"repository\":{\"issues\":{\"nodes\":[]}}}}" ;;
  "api graphql"*) cat "$T/shipped.json" ;;
esac"####;

/// The analytics of every run: the report, the sessions and the outcomes
/// answered, every event taken.
const ANALYTICS: &str = r####"[ "$1" != report ] || echo "{\"report\":1}"
[ "$1" != sessions ] || { mkdir -p "$3" && chmod 700 "$3" && echo "{\"api\":1,\"days\":30}" >"$3/sessions.jsonl"; }
[ "$1" != outcomes ] || { mkdir -p "$3" && chmod 700 "$3" && echo "{\"api\":1,\"by\":\"version\"}" >"$3/outcomes.jsonl"; }"####;

struct Ev {
    w: World,
    now: i64,
}

impl Ev {
    fn new(name: &str) -> Ev {
        let w = World::new(&format!("events-{name}"));
        w.gh_stub(GH);
        w.stub("systemctl", "");
        let ev = Ev {
            w,
            now: tehas_core::time::now(),
        };
        ev.reset();
        ev
    }

    fn reset(&self) {
        self.w.reset();
        self.issues(&[
            self.issue(3, "tehas:ready"),
            self.issue(5, "tehas:awaiting-merge"),
        ]);
        self.w.shipped(&[]);
        self.w.stub_analytics(ANALYTICS);
    }

    fn ago(&self, s: i64) -> String {
        tehas_core::time::iso(self.now - s)
    }

    fn at(&self, s: i64) -> i64 {
        self.now - s
    }

    fn issue(&self, n: u64, label: &str) -> String {
        self.issue_in(n, label, "OPEN", None)
    }

    fn closed(&self, n: u64, label: &str, ago: i64) -> String {
        self.issue_in(n, label, "CLOSED", Some(ago))
    }

    fn issue_in(&self, n: u64, label: &str, state: &str, closed: Option<i64>) -> String {
        let closed = closed.map_or("null".to_string(), |s| format!("\"{}\"", self.ago(s)));
        format!(
            r#"{{"number":{n},"title":"t","url":"u","state":"{state}","labels":[{{"name":"{label}"}}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":{closed}}}"#
        )
    }

    fn issues(&self, list: &[String]) {
        self.w.fx("issues.json", &format!("[{}]\n", list.join(",")));
    }

    /// An issue a merged pull request closed `ago` seconds ago.
    fn merged(&self, n: u64, ago: i64) -> String {
        let at = self.ago(ago);
        format!(
            r#"{{"number":{n},"title":"t","url":"u","updatedAt":"{at}","closedAt":"{at}","labels":{{"nodes":[{{"name":"tehas:shipped"}}]}},"timelineItems":{{"nodes":[{{"closer":{{"merged":true}}}}]}}}}"#
        )
    }

    /// The calls that record events: every run also asks for the report, the
    /// sessions and the outcomes, which are left out.
    fn events(&self) -> String {
        self.w
            .calls("analytics")
            .lines()
            .filter(|l| {
                !(l.starts_with("report --json")
                    || l.starts_with("sessions --out ")
                    || l.starts_with("outcomes --out "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn events_sorted(&self) -> String {
        let text = self.events();
        let mut lines: Vec<_> = text.lines().collect();
        lines.sort();
        lines.join("\n")
    }

    /// The next run, its calls only.
    fn again(&self) -> Run {
        let _ = std::fs::remove_file(self.w.t().join("calls/analytics"));
        self.w.status()
    }

    fn awaiting(&self) -> usize {
        self.w.live_json("status.json")["awaiting"]
            .as_array()
            .map_or(0, |a| a.len())
    }
}

#[test]
fn a_first_run_with_no_snapshot_before_it_records_no_awaiting_or_stopped() {
    let e = Ev::new("first");
    e.issues(&[
        e.issue(5, "tehas:awaiting-merge"),
        e.issue(6, "needs-human-attention"),
    ]);
    let r = e.w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(e.awaiting(), 1);
    assert_eq!(e.events(), "");
}

/// The run of the newly awaiting case: #3 newly awaiting, #6 newly stopped.
fn newly(e: &Ev) -> Run {
    e.w.status();
    e.issues(&[
        e.issue(3, "tehas:awaiting-merge"),
        e.issue(5, "tehas:awaiting-merge"),
        e.issue(6, "needs-human-attention"),
    ]);
    e.again()
}

#[test]
fn an_issue_that_is_newly_awaiting_or_newly_stopped_is_recorded_once_at_the_time_of_the_run() {
    let e = Ev::new("newly");
    let r = newly(&e);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(e.events_sorted(), "event 3 awaiting\nevent 6 stopped");
    assert_eq!(r.out, "");
}

#[test]
fn an_unchanged_snapshot_records_nothing() {
    let e = Ev::new("unchanged");
    newly(&e);
    e.again();
    assert_eq!(e.events(), "");
}

#[test]
fn a_merge_is_recorded_at_the_time_the_issue_was_closed_the_first_time_it_is_completed() {
    let e = Ev::new("merge");
    e.w.status();
    e.w.shipped(&[e.merged(20, 3600)]);
    e.again();
    assert_eq!(e.events(), format!("event 20 merged --at {}", e.at(3600)));
    e.w.shipped(&[e.merged(21, 60), e.merged(20, 3600)]);
    e.again();
    assert_eq!(e.events(), format!("event 21 merged --at {}", e.at(60)));
}

#[test]
fn a_first_run_records_the_merges_of_the_last_ten_days_with_their_times_and_none_older() {
    let e = Ev::new("ten-days");
    e.w.shipped(&[e.merged(20, 3600), e.merged(19, 11 * 86400)]);
    e.w.status();
    assert_eq!(e.events(), format!("event 20 merged --at {}", e.at(3600)));
}

#[test]
fn an_issue_of_the_snapshot_before_that_is_closed_and_not_merged_is_recorded_as_closed_at_its_time()
{
    let e = Ev::new("closed");
    e.issues(&[
        e.issue(3, "tehas:ready"),
        e.issue(6, "needs-human-attention"),
        e.issue(8, "tehas:running"),
    ]);
    e.w.status();
    e.issues(&[
        e.closed(3, "tehas:ready", 120),
        e.closed(6, "needs-human-attention", 60),
        e.issue(8, "tehas:running"),
        e.closed(9, "tehas:ready", 30),
    ]);
    e.w.shipped(&[e.merged(6, 60)]);
    e.again();
    assert_eq!(
        e.events_sorted(),
        format!(
            "event 3 closed --at {}\nevent 6 merged --at {}",
            e.at(120),
            e.at(60)
        )
    );
    e.again();
    assert_eq!(e.events(), "");
}

// The shell file had this case twice, word for word; it runs once here.
#[test]
fn an_issue_that_was_awaiting_a_merge_and_is_closed_not_yet_completed_is_not_recorded_as_closed() {
    let e = Ev::new("awaiting-closed");
    e.issues(&[
        e.issue(5, "tehas:awaiting-merge"),
        e.issue(6, "needs-human-attention"),
    ]);
    e.w.status();
    // 6 is awaiting and stopped at once, as a handoff with a pull request can be.
    e.issues(&[
        e.issue(5, "tehas:awaiting-merge"),
        e.issue(6, "needs-human-attention"),
        e.issue(6, "tehas:awaiting-merge"),
    ]);
    e.again();
    e.issues(&[
        e.closed(5, "tehas:awaiting-merge", 60),
        e.closed(6, "needs-human-attention", 60),
    ]);
    e.again();
    assert_eq!(e.events(), "");
    e.w.shipped(&[e.merged(5, 60), e.merged(6, 60)]);
    e.again();
    assert_eq!(
        e.events_sorted(),
        format!(
            "event 5 merged --at {}\nevent 6 merged --at {}",
            e.at(60),
            e.at(60)
        )
    );
}

#[test]
fn a_first_run_records_the_closed_issues_it_is_shown_with_their_times() {
    let e = Ev::new("first-closed");
    e.issues(&[
        e.closed(3, "tehas:ready", 120),
        e.closed(1, "tehas:ready", 60),
        e.closed(4, "tehas:ready", 11 * 86400),
    ]);
    e.w.status();
    assert_eq!(e.events(), format!("event 3 closed --at {}", e.at(120)));
}

#[test]
fn a_snapshot_before_more_than_an_hour_old_counts_as_none() {
    let e = Ev::new("old");
    e.w.status();
    let mut v = e.w.live_json("status.json");
    v["generatedAt"] = e.ago(3601).into();
    std::fs::write(e.w.live_file("status.json"), v.to_string()).unwrap();
    e.issues(&[
        e.issue(3, "tehas:awaiting-merge"),
        e.issue(5, "tehas:awaiting-merge"),
    ]);
    e.again();
    assert_eq!(e.events(), "");
}

#[test]
fn a_snapshot_before_that_is_not_one_counts_as_none() {
    let e = Ev::new("not-one");
    e.w.home_file("live/status.json", "not json\n");
    e.issues(&[e.issue(3, "tehas:awaiting-merge")]);
    let r = e.w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(e.awaiting(), 1);
    assert_eq!(e.events(), "");
}

#[test]
fn the_control_issue_is_never_recorded() {
    let e = Ev::new("control");
    e.w.status();
    e.issues(&[
        e.issue(1, "tehas:awaiting-merge"),
        e.issue(1, "needs-human-attention"),
    ]);
    e.again();
    assert_eq!(e.events(), "");
}

#[test]
fn with_tehas_analytics_off_nothing_is_recorded() {
    let e = Ev::new("off");
    e.w.minimal(&["TEHAS_ANALYTICS=off"]);
    e.w.status();
    e.issues(&[e.issue(3, "tehas:awaiting-merge")]);
    e.w.shipped(&[e.merged(20, 60)]);
    let r = e.again();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(e.awaiting(), 1);
    assert_eq!(e.events(), "");
}

#[test]
fn a_snapshot_with_a_part_kept_from_the_run_before_records_nothing() {
    let e = Ev::new("kept");
    e.issues(&[e.issue(3, "tehas:ready")]);
    e.w.status();
    e.issues(&[e.issue(3, "tehas:awaiting-merge")]);
    e.w.fx(
        "shipped.json",
        r#"{"errors": [{"message": "rate limited"}]}"#,
    );
    let r = e.again();
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = e.w.live_json("status.json");
    assert_eq!(
        serde_json::json!([e.awaiting(), v["stale"]]),
        serde_json::json!([1, ["completed"]])
    );
    assert_eq!(e.events(), "");
}

#[test]
fn a_failing_analytics_is_a_warning_on_standard_error_and_the_snapshot_is_written() {
    let e = Ev::new("failing");
    e.w.status();
    e.w.stub_analytics("exit 70");
    e.issues(&[e.issue(3, "tehas:awaiting-merge")]);
    let r = e.again();
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "WARN analytics event: exit 70");
    assert_eq!(e.awaiting(), 1);
    assert_eq!(e.w.live_json("health.json")["status"], "pass");
}

#[test]
fn an_analytics_that_hangs_costs_at_most_three_calls_time_said_once() {
    let e = Ev::new("hangs");
    let nodes: Vec<_> = (20..30).map(|n| e.merged(n, 60)).collect();
    e.w.shipped(&nodes);
    e.w.stub_analytics("exec sleep 30");
    e.w.set_env(&["TEHAS_ANALYTICS_WAIT=1"]);
    let r = e.w.status();
    assert_eq!(r.rc, 0, "{}", r.out);
    // Each call is cut at the 1 s limit, so three calls is three calls' time.
    // A count, not a clock: the clock fails on a loaded runner (#655).
    let said = |s: &str| r.out.lines().filter(|l| l.contains(s)).count();
    assert_eq!(said("WARN analytics event: exit"), 3, "{}", r.out);
    has(&r.out, "WARN analytics event: exit 124");
    has(
        &r.out,
        "WARN analytics: out of time, the other events of this run are left out",
    );
    assert_eq!(said("out of time"), 1, "{}", r.out);
    assert_eq!(
        e.w.live_json("status.json")["completed"]
            .as_array()
            .map_or(0, |a| a.len()),
        10
    );
}
