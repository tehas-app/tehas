use crate::given::*;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const LINK: &str = "https://github.com/user-attachments/assets/1";

fn scratch() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "attach-given-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn record() -> Value {
    json!({"links": [LINK], "at": "2026-01-01T00:00:00Z", "by": "alice", "how": "label"})
}

fn comment(source: &str, text: &str) -> Value {
    json!({"source": source, "by": "alice", "at": "2026-01-01T00:20:00Z", "text": text})
}

fn context(entries: Value) -> String {
    let rec = json!({"how": "retry-tick", "retry_context": entries});
    retry_context(&rec, true, "").unwrap().unwrap()
}

fn compose(rec: &Value) -> (Value, String) {
    let (manifest, gap) = left_out_manifest(&json!(7), rec, "why", "GIVEN", None)
        .unwrap()
        .unwrap();
    (serde_json::from_str(&manifest).unwrap(), gap)
}

#[test]
fn the_links_rule_is_the_one_of_the_shell() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lib/attachments.sh");
    let shell = fs::read_to_string(path).unwrap();
    let held = shell.lines().find(|l| l.contains("def links:")).unwrap();
    assert!(held.contains(&LINK_RULE.replace('\\', "\\\\")));
}

#[test]
fn a_comment_is_cut_at_4000_code_points_not_bytes() {
    let text = "\u{1F600}".repeat(5000);
    let out = context(json!([comment("pr-comment:1", &text)]));
    let quoted = out.lines().find(|l| l.starts_with("> ")).unwrap();
    assert_eq!(quoted.chars().count(), 2 + 4000);
}

#[test]
fn the_whole_is_cut_at_32000_code_points_with_a_mark() {
    let text = "\u{e4}".repeat(3990);
    let entries: Vec<Value> = (0..10)
        .map(|i| comment(&format!("pr-comment:{i}"), &text))
        .collect();
    let out = context(json!(entries));
    let body = out.strip_suffix("\n\n[cut]").unwrap();
    assert_eq!(body.chars().count(), 32000);
}

#[test]
fn every_line_of_a_comment_is_quoted_and_a_return_goes() {
    let out = context(json!([comment("pr-comment:1", "one\r\n## two\n\nthree\n")]));
    assert!(out.ends_with("\n> one\n> ## two\n> \n> three\n> \n"));
}

#[test]
fn an_empty_comment_holds_no_line() {
    let out = context(json!([comment("pr-comment:1", "")]));
    assert!(out.ends_with(", by alice, 2026-01-01T00:20:00Z\n\n\n"));
}

#[test]
fn a_source_or_an_account_of_another_shape_is_left_out() {
    for bad in [
        comment("pr-comment:1\n", "x"),
        comment("x\npr-comment:1", "x"),
        json!({"source": "pr-comment:1", "by": "a b", "at": "2026", "text": "x"}),
        json!({"source": "pr-comment:1", "by": "a", "at": "now", "text": "x"}),
        json!({"source": "pr-comment:1", "by": "a", "at": "2026", "text": 1}),
    ] {
        let rec = json!({"how": "retry-tick", "retry_context": [bad]});
        assert_eq!(retry_context(&rec, true, "").unwrap(), None);
    }
}

#[test]
fn the_millwright_is_one_comment_by_its_login_with_or_without_bot() {
    let mut first = comment("millwright:1", "first");
    first["by"] = json!("mw[bot]");
    let mut second = comment("fixer:2", "second");
    second["by"] = json!("mw");
    let rec = json!({"how": "fixer", "retry_context": [comment("fixer:0", "not"), first, second]});
    let out = retry_context(&rec, false, "mw").unwrap().unwrap();
    assert!(out.starts_with("# The fixer before this retry\n"));
    assert!(out.contains("> first") && !out.contains("> second") && !out.contains("> not"));
    assert_eq!(retry_context(&rec, false, "").unwrap(), None);
}

#[test]
fn a_refresh_task_is_not_quoted_and_only_a_refresh_has_one() {
    let entry = json!({"source": "refresh", "text": "Do no other work."});
    let rec = json!({"how": "refresh", "retry_context": [entry]});
    assert_eq!(
        retry_context(&rec, false, "").unwrap().unwrap(),
        "# This refresh\n\nDo no other work.\n"
    );
    let rec = json!({"how": "label", "retry_context": [entry]});
    assert_eq!(retry_context(&rec, false, "").unwrap(), None);
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
    let dir = scratch();
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
    assert!(written.ends_with(",\"leftOutMore\":0,\"files\":[]}\n"));
    assert!(!dir.join("7.manifest.json.new").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_kept_manifest_that_is_no_json_is_a_refusal_and_stays() {
    let dir = scratch();
    let file = dir.join("7.manifest.json");
    let rec = dir.join("record.json");
    fs::write(&rec, record().to_string()).unwrap();
    fs::write(&file, "{").unwrap();
    let out = left_out("7", file.to_str().unwrap(), rec.to_str().unwrap(), "0", "1");
    assert_eq!(out.unwrap_err().0, "unreadable");
    assert_eq!(fs::read_to_string(&file).unwrap(), "{");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_link_a_directory_or_a_cycle_given_attachments_writes_nothing() {
    let dir = scratch();
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
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_manifest_that_cannot_be_written_leaves_no_new_file_and_refuses() {
    let dir = scratch();
    let rec = dir.join("record.json");
    fs::write(&rec, record().to_string()).unwrap();
    let file = dir.join("missing/m.json");
    let out = left_out("7", file.to_str().unwrap(), rec.to_str().unwrap(), "0", "0");
    assert_eq!(out.unwrap_err().0, "unwritable");
    fs::remove_dir_all(dir).unwrap();
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
