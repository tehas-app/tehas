//! The image of a run in the report's arguments to `analytics run`, as
//! libexec/cycle passes them: the id on record, when it was built, and the
//! pi-rukas commit of its label.

use serde_json::{json, Value};
use std::fs;
use tehas_cycle::report::{record_image, report, Given};
use tehas_cycle::Facts;

const ID: &str = "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const BUILT: &str = "2026-10-29T14:44:16.123456789Z";

/// The first case of the record fixtures, its record naming `image` for
/// attempt `attempt`, and `built` as the facts' build time.
fn facts(attempt: u64, built: Option<&str>) -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/record.json");
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut f = cases[0]["facts"].clone();
    f["record"]["cycle"]["attempt"] = json!(attempt);
    f["record"]["cycle"]["image"] = json!(ID);
    f["record"]["cycle"]["rukas_commit"] = json!("abcdef1");
    if let Some(b) = built {
        f["image_built"] = json!(b);
    }
    f
}

fn analytics(v: &Value) -> Vec<String> {
    let f = Facts::from_json(v).unwrap();
    let mut gh = Given(v["github"].clone());
    report(&f, &mut gh).analytics
}

#[test]
fn the_run_carries_when_its_image_was_built() {
    let a = analytics(&facts(1, Some(BUILT)));
    assert_eq!(
        a[..6],
        [
            "--image",
            ID,
            "--image-built",
            BUILT,
            "--rukas-image",
            "abcdef1"
        ]
    );
}

#[test]
fn no_build_time_is_passed_without_one_or_without_the_image() {
    let a = analytics(&facts(1, None));
    assert!(!a.contains(&"--image-built".to_string()), "{a:?}");
    assert_eq!(a[..2], ["--image", ID]);
    // A record of another attempt names no image of this run, so its time
    // goes with it.
    let a = analytics(&facts(2, Some(BUILT)));
    assert!(!a.iter().any(|x| x.starts_with("--image")), "{a:?}");
}

#[test]
fn the_image_on_record_is_that_of_the_attempt() {
    let f = facts(1, None);
    let rec = &f["record"];
    assert_eq!(
        record_image(rec, 1),
        (ID.to_string(), "abcdef1".to_string())
    );
    assert_eq!(record_image(rec, 2), (String::new(), String::new()));
    let mut short = rec.clone();
    short["cycle"]["image"] = json!("sha256:short");
    short["cycle"]["rukas_commit"] = json!("XYZ");
    assert_eq!(record_image(&short, 1), (String::new(), String::new()));
}
