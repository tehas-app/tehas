//! host-start-set, host-start-clear and host-start-list, and the start that
//! spends a row (#1967).

use crate::verbs_tests::*;
use serde_json::json;

#[test]
fn a_host_start_is_set_once_listed_and_cleared() {
    let home = home("host-start");
    let v = ok(&home, &["host-start-set", "12", "--at", "100"]);
    assert_eq!(v["host_start"], json!({"issue": 12, "at": 100}));
    assert_eq!(v["new"], true);
    // Asked again, the row is kept as it was, its time included.
    let v = ok(&home, &["host-start-set", "12", "--at", "200"]);
    assert_eq!(v["host_start"], json!({"issue": 12, "at": 100}));
    assert_eq!(v["new"], false);
    ok(&home, &["host-start-set", "7", "--at", "300"]);
    let v = ok(&home, &["host-start-list"]);
    assert_eq!(
        v["host_starts"],
        json!([{"issue": 7, "at": 300}, {"issue": 12, "at": 100}])
    );
    let v = ok(&home, &["host-start-clear", "12"]);
    assert_eq!(v["cleared"], true);
    assert_eq!(v["host_start"], json!({"issue": 12, "at": 100}));
    let v = ok(&home, &["host-start-clear", "12"]);
    assert_eq!(v["cleared"], false);
    assert_eq!(v["host_start"], json!(null));
    let v = ok(&home, &["host-start-list"]);
    assert_eq!(v["host_starts"], json!([{"issue": 7, "at": 300}]));
}

#[test]
fn a_start_of_the_host_s_approval_spends_its_row() {
    let home = home("host-start-spent");
    ok(&home, &["host-start-set", "10", "--at", "100"]);
    let at = approve(&home, 10, "host", "host");
    let _lock = hold(&home, 10);
    let v = ok(&home, &["cycle-start", "10"]);
    assert_eq!(v["how"], "host");
    assert_eq!(v["started_by"], "host");
    assert_eq!(v["approval_at"], at);
    assert_eq!(count(&home, "host_starts"), 0);
    let v = ok(&home, &["approval-view", "10"]);
    assert_eq!(v["view"]["how"], "host");
    assert_eq!(v["view"]["by"], "host");
}

#[test]
fn any_start_of_the_issue_spends_the_host_s_approval() {
    let home = home("host-start-label");
    ok(&home, &["host-start-set", "10", "--at", "100"]);
    ok(&home, &["host-start-set", "11", "--at", "100"]);
    approve(&home, 10, "alice", "label");
    let _lock = hold(&home, 10);
    ok(&home, &["cycle-start", "10"]);
    let v = ok(&home, &["host-start-list"]);
    assert_eq!(v["host_starts"], json!([{"issue": 11, "at": 100}]));
}

#[test]
fn the_millwright_may_rest_on_the_host_s_approval() {
    let home = home("host-start-base");
    approve(&home, 10, "host", "host");
    let _lock = hold(&home, 10);
    ok(&home, &["cycle-start", "10"]);
    ok(
        &home,
        &["cycle-end", "10", "--attempt", "1", "--status", "failed"],
    );
    approve(&home, 10, "tehas-millwright", "millwright");
    ok(&home, &["cycle-start", "10"]);
    let v = ok(&home, &["approval-current", "10"]);
    assert_eq!(v["rests_on"]["how"], "host");
    assert_eq!(v["millwright_starts"], 1);
}
