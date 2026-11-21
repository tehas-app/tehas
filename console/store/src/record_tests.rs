//! record-put, record-file, record-close and records against a real store.

use crate::verbs_tests::*;
use serde_json::Value;
use std::path::Path;

const P: &str = "pageticket";

fn put(home: &Path, id: &str, kind: &str) -> Result<Value, (u8, String)> {
    call(
        home,
        &[
            "record-put",
            "--plugin",
            P,
            "--id",
            id,
            "--kind",
            kind,
            "--by",
            "octo-cat",
        ],
    )
}

fn file(home: &Path, id: &str, issue: &str, sha: &str) -> Result<Value, (u8, String)> {
    call(
        home,
        &[
            "record-file",
            "--plugin",
            P,
            "--id",
            id,
            "--issue",
            issue,
            "--files-sha256",
            sha,
            "--by",
            "octo-cat",
            "--by-id",
            "583231",
        ],
    )
}

fn close(home: &Path, id: &str) -> Result<Value, (u8, String)> {
    call(home, &["record-close", "--plugin", P, "--id", id])
}

fn ids(v: &Value) -> Vec<String> {
    v["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            format!(
                "{}/{}",
                r["plugin"].as_str().unwrap(),
                r["id"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn a_record_is_made_once_and_answers_as_it_is() {
    let home = home("record-put");
    let v = put(&home, "p-1", "prototype").unwrap();
    let r = &v["record"];
    assert_eq!(r["plugin"], P);
    assert_eq!(r["kind"], "prototype");
    assert_eq!(r["created_by"], "octo-cat");
    assert_eq!(r["created_at"], NOW);
    assert!(r["issue"].is_null() && r["files_sha256"].is_null() && r["closed_at"].is_null());
    // The same again keeps the first row, its time included.
    let again = call_at(
        &home,
        NOW + 60,
        &[
            "record-put",
            "--plugin",
            P,
            "--id",
            "p-1",
            "--kind",
            "prototype",
            "--by",
            "other",
        ],
    )
    .unwrap();
    assert_eq!(again["record"], v["record"]);
    // Another kind under the same id is a conflict and writes nothing.
    let (rc, err) = put(&home, "p-1", "note").unwrap_err();
    assert_eq!(rc, 1);
    assert!(err.starts_with("conflict: record pageticket/p-1"), "{err}");
    assert_eq!(count(&home, "plugin_records"), 1);
}

#[test]
fn a_record_is_filed_once_with_its_issue_and_files_together() {
    let home = home("record-file");
    let (rc, err) = file(&home, "p-1", "12", SHA).unwrap_err();
    assert_eq!((rc, err.starts_with("norecord:")), (1, true), "{err}");
    put(&home, "p-1", "prototype").unwrap();
    let v = call_at(
        &home,
        NOW + 5,
        &[
            "record-file",
            "--plugin",
            P,
            "--id",
            "p-1",
            "--issue",
            "12",
            "--files-sha256",
            SHA,
            "--by",
            "octo-cat",
            "--by-id",
            "583231",
        ],
    )
    .unwrap();
    assert_eq!(v["record"]["issue"], 12);
    assert_eq!(v["record"]["filed_by"], "octo-cat");
    assert_eq!(v["record"]["filed_by_id"], 583231);
    assert_eq!(v["record"]["files_sha256"], SHA);
    assert_eq!(v["record"]["filed_at"], NOW + 5);
    // The same filing again answers the record; any other is a conflict.
    assert_eq!(
        file(&home, "p-1", "12", SHA).unwrap()["record"],
        v["record"]
    );
    for (issue, sha) in [("13", SHA), ("12", SHA_B)] {
        let (rc, err) = file(&home, "p-1", issue, sha).unwrap_err();
        assert_eq!(rc, 1);
        assert!(err.contains("is filed already"), "{err}");
    }
    // Another filer of the same issue and files is a conflict too.
    let other = call(
        &home,
        &[
            "record-file",
            "--plugin",
            P,
            "--id",
            "p-1",
            "--issue",
            "12",
            "--files-sha256",
            SHA,
            "--by",
            "someone",
            "--by-id",
            "9",
        ],
    )
    .unwrap_err();
    assert!(other.1.contains("is filed already"), "{other:?}");
    assert_eq!(
        count(
            &home,
            &format!("plugin_records WHERE files_sha256 = '{SHA}'")
        ),
        1
    );
}

#[test]
fn a_closed_record_stays_and_takes_no_filing() {
    let home = home("record-close");
    put(&home, "p-1", "prototype").unwrap();
    let v = close(&home, "p-1").unwrap();
    assert_eq!(v["record"]["closed_at"], NOW);
    let again = call_at(
        &home,
        NOW + 9,
        &["record-close", "--plugin", P, "--id", "p-1"],
    )
    .unwrap();
    assert_eq!(again["record"]["closed_at"], NOW);
    let (rc, err) = file(&home, "p-1", "12", SHA).unwrap_err();
    assert_eq!(rc, 1);
    assert!(err.contains("is closed"), "{err}");
    assert_eq!(close(&home, "p-2").unwrap_err().0, 1);
    assert_eq!(count(&home, "plugin_records"), 1);
}

#[test]
fn records_lists_in_the_order_made_by_plugin_issue_and_open() {
    let home = home("records");
    assert_eq!(
        ids(&call(&home, &["records"]).unwrap()),
        Vec::<String>::new()
    );
    for (at, plugin, id) in [
        (NOW, P, "p-1"),
        (NOW + 1, "floorplan", "map"),
        (NOW + 2, P, "p-2"),
    ] {
        call_at(
            &home,
            at,
            &[
                "record-put",
                "--plugin",
                plugin,
                "--id",
                id,
                "--kind",
                "prototype",
                "--by",
                "octo-cat",
            ],
        )
        .unwrap();
    }
    file(&home, "p-2", "40", SHA).unwrap();
    close(&home, "p-1").unwrap();
    let all = call(&home, &["records"]).unwrap();
    assert_eq!(
        ids(&all),
        ["pageticket/p-1", "floorplan/map", "pageticket/p-2"]
    );
    assert_eq!(
        ids(&call(&home, &["records", "--plugin", P]).unwrap()),
        ["pageticket/p-1", "pageticket/p-2"]
    );
    assert_eq!(
        ids(&call(&home, &["records", "--issue", "40"]).unwrap()),
        ["pageticket/p-2"]
    );
    assert_eq!(
        ids(&call(&home, &["records", "--plugin", P, "--open"]).unwrap()),
        ["pageticket/p-2"]
    );
}
