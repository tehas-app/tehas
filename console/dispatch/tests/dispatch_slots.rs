//! Slots, the CPU ceiling, the start order and the cap on unreviewed work,
//! where a poll's process says more than its decision: the index, the
//! store's start order, what gh is asked and a start that fails.

mod common;

use common::*;

#[test]
fn the_index_is_refreshed_once_before_the_first_start() {
    let w = World::new("slots-index");
    w.slots(2, &[]);
    w.ready(&[7, 8]);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.started(), "7 8 ");
    assert_eq!(w.calls("fake-index").trim_end(), "--before-cycle");
    assert!(!w.t().join("calls/index.late").exists());
}

// nproc is stubbed to 8, and reset()'s slice has room for ten.
#[test]
fn the_ceiling_defaults_to_the_stubbed_hosts_cpus_minus_one_seven() {
    let w = World::new("slots-ceiling");
    w.instance(MINIMAL);
    w.ready(&[7, 8, 9, 10, 11, 12, 13, 14]);
    for n in 20..=26 {
        w.hold(n);
    }
    let r = w.poll();
    assert_eq!(w.calls("fake-start"), "");
    has(&r.out, "holding: 7 cycles are running (limit 7)");
}

#[test]
fn the_start_order_on_the_store_puts_an_issue_first() {
    let w = World::new("slots-order");
    w.slots(2, &[]);
    w.ready(&[3, 8]);
    w.store_ok(&["order-set", "--issues", "8"]);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.started(), "8 3 ");
}

#[test]
fn with_only_a_ready_issue_outside_the_start_order_on_the_store_waits() {
    let w = World::new("slots-only");
    w.slots(2, &[]);
    w.ready(&[3, 8]);
    w.store_ok(&["order-set", "--issues", "8", "--only"]);
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(w.started(), "8 ");
    has(&r.out, "wait #3: not in the start order (--only)");
}

#[test]
fn a_start_order_that_expired_orders_nothing() {
    let w = World::new("slots-expired");
    w.slots(2, &[]);
    w.ready(&[3, 8]);
    w.store_ok(&["order-set", "--issues", "8", "--only", "--until", "1000"]);
    let r = w.poll();
    assert_eq!(r.rc, 0);
    assert_eq!(w.started(), "3 8 ");
    has(
        &r.out,
        "order expired at 1970-01-01T00:16:40Z: ready issues start lowest number first",
    );
}

#[test]
fn with_every_slot_taken_nothing_is_started_and_no_ready_issue_is_looked_at() {
    let w = World::new("slots-full");
    w.slots(2, &[]);
    w.ready(&[7, 8]);
    w.hold(9);
    w.hold(10);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.calls("fake-start"), "");
    // The demand reads the list and who labelled each issue; nothing else is
    // asked. One page, and nothing per issue.
    let gh = w.gh();
    let per_issue = gh.lines().filter(|l| {
        matches!(
            *l,
            "api repos/acme/widget/issues/7" | "api repos/acme/widget/issues/8"
        ) || l.starts_with("issue view 7 ")
            || l.starts_with("issue view 8 ")
    });
    assert_eq!(per_issue.count(), 0, "{gh}");
    lacks(&gh, "awaiting-merge");
    has(
        &w.dispatch(&["--dry-run"]).out,
        "holding: 2 cycles are running (limit 2)",
    );
}

// The fixture of this name checks the dry run's decision; this one the
// real poll's.
#[test]
fn at_the_cap_nothing_is_started_whatever_slots_are_free() {
    let w = World::new("slots-cap");
    w.slots(3, &["TEHAS_MAX_AWAITING=2"]);
    w.ready(&[7, 8]);
    w.fx("awaiting.json", r#"[{"number":5}]"#);
    w.hold(9);
    assert_eq!(w.poll().rc, 0);
    assert_eq!(w.calls("fake-start"), "");
    has(
        &w.dispatch(&["--dry-run"]).out,
        "holding: 1 issues await merge and 1 cycles are running (limit 2)",
    );
}

#[test]
fn the_index_is_not_refreshed_under_a_running_cycle() {
    let w = World::new("slots-index-held");
    w.slots(2, &[]);
    w.ready(&[7]);
    w.hold(9);
    let r = w.poll();
    assert_eq!(w.calls("fake-start").trim_end(), "7");
    assert_eq!(w.calls("fake-index"), "");
    has(&r.out, "the code index stays as it is: a cycle is running");
}

#[test]
fn a_cycle_that_does_not_start_does_not_stop_the_next_one_and_the_poll_ends_with_1() {
    let w = World::new("slots-start-fails");
    w.slots(2, &[]);
    w.ready(&[7, 8]);
    // fake-start fails for issue 7 only.
    w.stub(
        "fake-start",
        r#"[ "$1" != 7 ] || exit 1
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >/dev/null"#,
    );
    assert_eq!(w.poll().rc, 1);
    let gh = w.gh();
    has(
        &gh,
        "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention",
    );
    lacks(&gh, "issue edit 8 --repo acme/widget --remove-label");
    assert_eq!(w.started(), "7 8 ");
}
