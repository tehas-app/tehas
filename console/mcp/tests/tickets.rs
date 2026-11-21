// The ticket tools over files as `console tickets` writes them.

use mcp::tickets::{call, entries, serves, ONE, SEARCH};
use mcp::tools::Scope;
use serde_json::{json, Value};
use std::path::Path;
use tehas_core::testdir::TestDir;

fn put(dir: &Path, n: u64, t: Value) {
    std::fs::create_dir_all(dir.join("tickets")).unwrap();
    std::fs::write(dir.join(format!("tickets/{n}.json")), t.to_string()).unwrap();
}

fn fixture(name: &str) -> TestDir {
    let d = TestDir::new(name);
    put(
        &d,
        5,
        json!({"api": 1, "kind": "issue", "number": 5, "title": "Why the gate\u{202e} is slow",
        "url": "https://github.com/acme/widget/issues/5", "state": "closed", "stateReason": "completed",
        "updatedAt": "2026-11-02T00:00:00Z", "author": "alice", "labels": ["epic"], "parent": 2,
        "subIssues": [{"number": 9, "title": "Part", "state": "open"}],
        "body": "The gate is slow because of ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ.", "commentsTotal": 1,
        "comments": [{"author": "bob", "createdAt": "2026-11-02T00:00:00Z", "body": "Agreed, CACHE it."}]}),
    );
    put(
        &d,
        8,
        json!({"api": 1, "kind": "pull", "number": 8, "title": "Cache the gate", "state": "merged",
        "updatedAt": "2026-11-03T00:00:00Z", "author": "bot", "labels": [], "mergeCommit": "abc123",
        "closes": [5], "body": "Closes #5", "commentsTotal": 0, "comments": []}),
    );
    put(
        &d,
        11,
        json!({"api": 1, "kind": "issue", "number": 11, "title": "Unrelated", "state": "open",
        "updatedAt": "2026-11-04T00:00:00Z", "body": "", "comments": []}),
    );
    std::fs::write(
        d.join("tickets/.cursor.json"),
        r#"{"since":"2026-11-04T00:00:00Z","read":"2026-11-21T12:00:00Z"}"#,
    )
    .unwrap();
    d
}

#[test]
fn a_ticket_is_served_masked_with_its_links() {
    let d = fixture("mcp-ticket-one");
    let v = call(&d, ONE, &json!({"number": 5})).unwrap();
    assert_eq!(v["kind"], "issue");
    assert_eq!(v["title"], "Why the gate is slow");
    assert_eq!(v["parent"], 2);
    assert_eq!(
        v["subIssues"],
        json!([{"number": 9, "state": "open", "title": "Part"}])
    );
    assert_eq!(v["labels"], json!(["epic"]));
    assert_eq!(v["body"], "The gate is slow because of ghp_***.");
    assert_eq!(v["comments"][0]["author"], "bob");
    assert_eq!(v["readUpTo"], "2026-11-04T00:00:00Z");
    assert!(v["untrusted"].as_str().unwrap().contains("third-party"));
    let p = call(&d, ONE, &json!({"number": 8})).unwrap();
    assert_eq!(p["closes"], json!([5]));
    assert_eq!(p["mergeCommit"], "abc123");
    let why = call(&d, ONE, &json!({"number": 6})).unwrap_err();
    assert!(why.contains("#6 has not been read"), "{why}");
    for bad in [
        json!({}),
        json!({"number": 0}),
        json!({"number": "5"}),
        json!({"number": 5, "x": 1}),
    ] {
        assert!(call(&d, ONE, &bad).is_err(), "{bad}");
    }
}

#[test]
fn a_long_body_is_cut_and_says_so() {
    let d = TestDir::new("mcp-ticket-cut");
    put(
        &d,
        3,
        json!({"kind": "issue", "number": 3, "state": "open", "body": "x".repeat(30_000)}),
    );
    let v = call(&d, ONE, &json!({"number": 3})).unwrap();
    assert_eq!(v["body"].as_str().unwrap().len(), 20_000);
    assert_eq!(v["bodyCut"], true);
}

#[test]
fn a_search_finds_the_text_in_any_case_newest_change_first() {
    let d = fixture("mcp-ticket-search");
    let v = call(&d, SEARCH, &json!({"grep": "cache"})).unwrap();
    let got: Vec<(u64, Value)> = v["tickets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["number"].as_u64().unwrap(), t["in"].clone()))
        .collect();
    assert_eq!(got, vec![(8, json!(["title"])), (5, json!(["comments"]))]);
    assert_eq!(v["read"], 3);
    assert_eq!(v["more"], false);
    let v = call(&d, SEARCH, &json!({"grep": "gate", "kind": "pull"})).unwrap();
    assert_eq!(v["tickets"].as_array().unwrap().len(), 1);
    let v = call(&d, SEARCH, &json!({"grep": "gate", "state": "closed"})).unwrap();
    assert_eq!(v["tickets"][0]["number"], 5);
    let v = call(&d, SEARCH, &json!({"grep": "gate", "limit": 1})).unwrap();
    assert_eq!(v["more"], true);
    // A masked secret is not found by its value.
    let v = call(
        &d,
        SEARCH,
        &json!({"grep": "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ"}),
    )
    .unwrap();
    assert_eq!(v["tickets"], json!([]), "a secret is found by its value");
    for bad in [
        json!({}),
        json!({"grep": "x"}),
        json!({"grep": "gate", "kind": "pr"}),
        json!({"grep": "gate", "limit": 51}),
    ] {
        assert!(call(&d, SEARCH, &bad).is_err(), "{bad}");
    }
    let empty = TestDir::new("mcp-ticket-none");
    assert!(call(&empty, SEARCH, &json!({"grep": "gate"}))
        .unwrap_err()
        .contains("no ticket has been read"));
}

#[test]
fn only_a_full_key_is_served_them() {
    for name in [ONE, SEARCH] {
        assert!(serves(Scope::Full, name));
        assert!(!serves(Scope::Dashboard, name));
    }
    assert_eq!(entries(Scope::Full).len(), 2);
    assert!(entries(Scope::Dashboard).is_empty());
}
