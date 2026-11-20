//! What holds a refresh, and the tehas:refreshing mark, where it takes more
//! than one poll or the calls the poll makes. The decisions alone are cases
//! of tests/fixtures/refresh.json.

mod common;
#[allow(dead_code)] // the helpers are shared with dispatch_refresh.rs
mod refresh;

use common::*;
use refresh::*;

#[test]
fn at_its_cap_a_refresh_is_held_and_said_on_the_issue_once() {
    let w = refresh_world("hold-cap", &[]);
    refresh_record(&w, 2, false);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(
        &r.out,
        "drop the refresh of #7: it was refreshed 2 times since alice approved it, the limit",
    );
    assert_eq!(said_no_refresh(&w), 1);
    request(&w, "2026-10-17T10:00:00Z");
    w.poll();
    assert_eq!(said_no_refresh(&w), 1);
}

#[test]
fn after_a_refresh_that_parked_a_refresh_is_held_and_said_on_the_issue_once() {
    let w = refresh_world("hold-parked", &[]);
    refresh_record(&w, 1, true);
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, "a refresh of it since alice approved it parked");
    assert_eq!(said_no_refresh(&w), 1);
    request(&w, "2026-10-17T10:00:00Z");
    w.poll();
    assert_eq!(said_no_refresh(&w), 1);
}

#[test]
fn after_a_parked_refresh_tehas_ready_does_what_the_park_said_nothing_while_the_pull_request_is_open_a_cycle_once_it_is_closed(
) {
    let w = refresh_world("hold-parked-ready", &[]);
    refresh_record(&w, 1, true);
    w.poll();
    has(&w.gh(), "issue comment 7 --repo acme/widget --body Tehas does not refresh this issue's pull request on its own: a refresh of it since alice approved it parked. It conflicts with the branch: a person resolves the conflict, or closes the pull request and then applies `tehas:ready` to start a fresh cycle.");
    // A person applies tehas:ready with pull request 50 still open.
    issue7(&w, &["tehas:ready"]);
    w.fx(
        "ready.json",
        r#"[{"number":7,"labels":[{"name":"tehas:awaiting-merge"},{"name":"tehas:ready"}]}]"#,
    );
    w.fx(
        "events-7.json",
        r#"[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-10-17T11:00:00Z"}]"#,
    );
    let prs = |state: &str| {
        w.fx(
            "prs-7.json",
            &format!(
                r#"{{"data":{{"repository":{{"issue":{{"closedByPullRequestsReferences":{{"nodes":[{{"url":"https://github.com/acme/widget/pull/50","state":"{state}"}}]}}}}}}}}}}"#
            ),
        )
    };
    prs("OPEN");
    w.poll();
    assert_eq!(w.started(), "");
    has(
        &w.gh(),
        "issue edit 7 --repo acme/widget --remove-label tehas:ready",
    );
    has(&w.gh(), "issue comment 7 --repo acme/widget --body Tehas took `tehas:ready` off this issue and started nothing: https://github.com/acme/widget/pull/50 is open and closes it. Merge or close that pull request, then apply `tehas:ready` if work is still needed.");
    // The person closes pull request 50, as the park said, then applies
    // tehas:ready.
    prs("CLOSED");
    pull(&w, |p| p["state"] = "CLOSED".into());
    let _ = std::fs::remove_file(refresh_dir(&w).join("7.json"));
    w.poll();
    assert_eq!(w.started(), "7 ");
    assert_eq!(said_no_refresh(&w), 1);
}

#[test]
fn refreshes_go_after_the_start_order_and_before_the_rest() {
    let w = refresh_world("hold-order", &[]);
    w.slots(4, &[]);
    w.queue(&["3", "8", "9"]);
    w.store_ok(&["order-set", "--issues", "8"]);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "8 7 3 9 ");
}

#[test]
fn a_host_that_never_refreshed_does_not_ask_for_the_mark() {
    let w = World::new("hold-never");
    w.poll();
    lacks(&w.gh(), "tehas:refreshing");
}
