use crate::given::*;
use serde_json::{json, Value};
use std::fs;
use tehas_core::testdir::TestDir;

const LINK: &str = "https://github.com/user-attachments/assets/1";

fn record() -> Value {
    json!({"links": [LINK], "at": "2026-01-01T00:00:00Z", "by": "alice", "how": "label"})
}

fn compose(rec: &Value) -> (Value, String) {
    let (manifest, gap) = left_out_manifest(&json!(7), rec, "why", "GIVEN", None)
        .unwrap()
        .unwrap();
    (serde_json::from_str(&manifest).unwrap(), gap)
}

#[test]
fn the_links_rule_is_the_one_of_the_dispatcher() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../dispatch/src/text.rs");
    assert!(fs::read_to_string(path).unwrap().contains(LINK_RULE));
}

#[test]
fn a_link_ends_before_a_part_that_would_leave_the_folder() {
    let cases = [
        ("https://github.com/o/r/files/1/../../raw/main/a.json", None),
        ("https://github.com/o/r/files/1/./a.json", None),
        ("https://github.com/o/r/files/1/%2e%2e/raw/a.json", None),
        ("https://github.com/o/r/files/1/.%2E/raw/a.json", None),
        ("https://github.com/o/r/files/1/..?x=1", None),
        ("https://github.com/o/../files/1/a.json", None),
        ("https://github.com/./r/files/1/a.json", None),
        (
            "https://github.com/user-attachments/files/../../o/r/raw/a.json",
            None,
        ),
        (
            "https://github.com/user-attachments/files/1/../../o/r/raw/a.json",
            Some("https://github.com/user-attachments/files/1"),
        ),
        (
            "https://github.com/o/r/files/1/a/../../../raw/a.json",
            Some("https://github.com/o/r/files/1/a"),
        ),
        (
            "https://github.com/o/r/files/1/a%2f..%2fb.json",
            Some("https://github.com/o/r/files/1/a"),
        ),
        (
            "https://github.com/o/r/files/1/a%5c..%5cb.json",
            Some("https://github.com/o/r/files/1/a"),
        ),
        ("https://user-images.githubusercontent.com/../x.png", None),
    ];
    for (text, found) in cases {
        assert!(!is_link(text), "{text}");
        let want: Vec<String> = found.into_iter().map(String::from).collect();
        assert_eq!(text_links(text), want, "{text}");
    }
    for link in [
        "https://github.com/o/r/files/1/.hidden.json",
        "https://github.com/o/r/files/1/a..b/...c.json",
        "https://github.com/o.r/r.s/files/1/a%20b.json?x=../y",
        "https://github.com/user-attachments/assets/1",
    ] {
        assert!(is_link(link), "{link}");
        assert_eq!(text_links(link), [link]);
    }
}

#[test]
fn a_list_of_more_than_a_hundred_keeps_a_hundred_and_counts_the_rest() {
    let mut rec = record();
    rec["links"] = (0..103)
        .map(|i| format!("{LINK}{i}"))
        .collect::<Vec<_>>()
        .into();
    let (manifest, gap) = compose(&rec);
    assert_eq!(manifest["leftOut"].as_array().unwrap().len(), 100);
    assert_eq!(manifest["leftOutMore"], 3);
    assert!(gap.contains("lists 103 attachments"));
    assert!(gap.ends_with("; its manifest lists the first 100 of them"));
}

#[test]
fn a_hundred_are_all_listed_and_more_is_zero() {
    let mut rec = record();
    rec["links"] = (0..100)
        .map(|i| format!("{LINK}{i}"))
        .collect::<Vec<_>>()
        .into();
    let (manifest, gap) = compose(&rec);
    assert_eq!(manifest["leftOutMore"], 0);
    assert!(!gap.contains("first"));
    assert_eq!(
        manifest["linksUnavailable"],
        "This instance gives a cycle none (GIVEN): no directory is mounted and PI_ENSEMBLE_EXTRA_IMAGE_DIRS is not set."
    );
}

#[test]
fn a_link_is_cut_at_512_and_counted_once() {
    let long = format!("{LINK}{}", "a".repeat(600));
    let mut rec = record();
    rec["links"] = json!([long, long, "https://example.test/a", 7]);
    let (manifest, gap) = compose(&rec);
    assert_eq!(manifest["leftOut"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["leftOut"][0]["link"].as_str().unwrap().len(), 512);
    assert!(gap.contains("lists 1 attachment and"));
}

#[test]
fn a_comments_link_says_where_it_came_from() {
    let mut rec = record();
    rec["links"] = json!([]);
    rec["comment_links"] = json!([
        {"link": LINK, "source": "comment:5", "by": "bob"},
        {"link": LINK, "source": "pr-comment:6", "by": "bob"},
        {"link": "https://github.com/user-attachments/assets/2", "source": "answer:1", "by": "bob"},
    ]);
    let (manifest, _) = compose(&rec);
    assert_eq!(manifest["leftOut"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["leftOut"][0]["source"], "comment:5");
}

#[test]
fn a_record_of_another_shape_is_an_error_and_no_link_is_nothing() {
    let mut rec = record();
    rec["how"] = json!("rumour");
    assert!(left_out_manifest(&json!(7), &rec, "r", "g", None).is_err());
    let mut rec = record();
    rec["links"] = json!([]);
    assert_eq!(
        left_out_manifest(&json!(7), &rec, "r", "g", None).unwrap(),
        None
    );
}

#[test]
fn an_earlier_manifest_of_the_approval_keeps_its_entries_in_its_order() {
    let dir = TestDir::new("attach-given");
    let file = dir.join("7.manifest.json");
    let rec = dir.join("record.json");
    fs::write(&rec, record().to_string()).unwrap();
    fs::write(
        &file,
        r#"{"images":[{"zeta":1,"file":"image-1.png","alpha":"\u007f"}],"files":[],"approval":{"how":"label","by":"alice","at":"2026-01-01T00:00:00Z"}}"#,
    )
    .unwrap();
    let path = file.to_str().unwrap();
    let gap = left_out("7", path, rec.to_str().unwrap(), "0", "0").unwrap();
    assert!(gap.unwrap().starts_with("the approval record of issue 7 "));
    let written = fs::read_to_string(&file).unwrap();
    assert!(written.starts_with(
        r#"{"issue":7,"images":[{"zeta":1,"file":"image-1.png","alpha":"\u007f"}],"approval":{"at":"#
    ));
    assert!(written
        .contains(",\"leftOutMore\":0,\"files\":[],\"linksUnavailable\":\"This instance gives"));
    assert!(!dir.join("7.manifest.json.new").exists());
}

#[test]
fn a_kept_manifest_that_is_no_json_is_a_refusal_and_stays() {
    let dir = TestDir::new("attach-given");
    let file = dir.join("7.manifest.json");
    let rec = dir.join("record.json");
    fs::write(&rec, record().to_string()).unwrap();
    fs::write(&file, "{").unwrap();
    let out = left_out("7", file.to_str().unwrap(), rec.to_str().unwrap(), "0", "1");
    assert_eq!(out.unwrap_err().0, "unreadable");
    assert_eq!(fs::read_to_string(&file).unwrap(), "{");
}

#[test]
fn a_link_a_directory_or_a_cycle_given_attachments_writes_nothing() {
    let dir = TestDir::new("attach-given");
    let rec = dir.join("record.json");
    fs::write(&rec, record().to_string()).unwrap();
    let rec = rec.to_str().unwrap();
    let file = dir.join("m.json");
    let path = file.to_str().unwrap();
    assert_eq!(left_out("7", path, rec, "1", "1").unwrap(), None);
    assert!(!file.exists());
    let inner = dir.join("d");
    fs::create_dir(&inner).unwrap();
    assert_eq!(
        left_out("7", inner.to_str().unwrap(), rec, "0", "0").unwrap(),
        None
    );
}

#[test]
fn a_manifest_that_cannot_be_written_leaves_no_new_file_and_refuses() {
    let dir = TestDir::new("attach-given");
    let rec = dir.join("record.json");
    fs::write(&rec, record().to_string()).unwrap();
    let file = dir.join("missing/m.json");
    let out = left_out("7", file.to_str().unwrap(), rec.to_str().unwrap(), "0", "0");
    assert_eq!(out.unwrap_err().0, "unwritable");
}

#[test]
fn the_same_link_of_the_same_approval_must_keep_its_bytes() {
    let approval = json!({"at": "1", "by": "a", "how": "label"});
    let m = |sum: &str, a: &Value| {
        json!({"images": [{"link": LINK, "sha256": sum}], "approval": a}).to_string()
    };
    assert_eq!(changed(&m("a", &approval), &m("a", &approval)), None);
    assert_eq!(
        changed(&m("a", &approval), &m("b", &approval)).as_deref(),
        Some(LINK)
    );
    assert_eq!(changed(&m("a", &json!(null)), &m("b", &json!(null))), None);
    assert_eq!(
        changed(&m("a", &approval), &m("b", &json!({"at": "2"}))),
        None
    );
    assert_eq!(changed("{", &m("b", &approval)), None);
}

#[test]
fn said_names_what_was_given_and_what_was_left_out() {
    let m = json!({"images": [{"link": "L", "file": "image-1.png", "sha256": "S", "source": "comment:3"}],
        "files": [], "leftOut": [{"link": "M", "reason": "Too big.", "source": "body"}], "leftOutMore": 2});
    assert_eq!(
        said(&m, "cycle", "1", "B").unwrap().unwrap(),
        "Tehas gave this cycle 1 image from the issue and its comments.\n\n- L (comment 3): image-1.png, sha256 S\n\nLeft out:\n- M: Too big.\nand 2 more, in the manifest"
    );
    let none = json!({"images": [], "leftOut": []});
    assert_eq!(said(&none, "cycle", "1", "B").unwrap(), None);
}
