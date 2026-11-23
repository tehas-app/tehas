//! #2729 AC2: the contract of `tehas status --json` carries the analytics
//! field. api/status.schema.json names it, every sample of the object holds
//! it, and the manual has its row.
//!
//! The object is closed (api/status.schema.json, `additionalProperties:
//! false`), so a field the code prints that the schema does not name is a
//! broken contract for the program that copies the samples into its own tests
//! (docs/specs/2026-10-24-status-json-contract.md). Today nothing prints it.

mod common;

use common::*;
use serde_json::Value;
use std::path::Path;

/// The six samples beside the schema, as console/tests/status_contract.rs
/// names them, with what each is made of: the outsourced one is the factory
/// with outsource on, the lab one a lab build, and the rest ask for the
/// default, which the load gives analytics (rules.rs:344).
const SAMPLES: &[(&str, &str)] = &[
    ("busy", "on"),
    ("quiet", "on"),
    ("readonly", "on"),
    ("pushfault", "on"),
    ("outsourced", "on"),
    ("lab", "on"),
];

/// 2026-10-17T14:00:20Z: forty seconds after the time in the health files.
const NOW: &str = "1792245620";

fn api(name: &str) -> std::path::PathBuf {
    root().join("api").join(name)
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn schema() -> Value {
    json(&read(&api("status.schema.json")))
}

/// The sample as the console makes it of its health file, not as it stands in
/// api/: what the code prints today.
fn printed(name: &str) -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_console"))
        .args(["status", "json"])
        .arg(
            root()
                .join("test/fixtures/status-json")
                .join(format!("{name}.health.json")),
        )
        .arg(NOW)
        .output()
        .unwrap();
    jq(".", &String::from_utf8_lossy(&out.stdout))
}

/// A field of the schema, as the check of it would name it: "no such field"
/// when the closed object does not know the word at all.
#[test]
fn ac2_the_schema_names_analytics_as_on_or_off() {
    let got = &schema()["properties"]["analytics"];
    assert_eq!(
        got["enum"],
        json(r#"["on","off"]"#),
        "the schema says: {got}"
    );
    // Every sample must hold it, so a program may count on reading it.
    assert!(
        schema()["required"]
            .as_array()
            .unwrap()
            .contains(&json("\"analytics\"")),
        "analytics is not required: {:?}",
        schema()["required"]
    );
}

#[test]
fn ac2_every_sample_holds_the_field_the_console_prints_it() {
    for (name, want) in SAMPLES {
        let said = format!("\"{want}\"");
        let file = read(&api(&format!("status.{name}.json")));
        assert_eq!(
            jq(".analytics", &file),
            said,
            "{name}: the file beside the schema holds no analytics"
        );
        assert_eq!(
            jq(".analytics", &printed(name)),
            said,
            "{name}: the console prints no analytics"
        );
    }
}

/// A field of a closed object that the schema does not name is a fault the
/// check reports; the same words are what a sample with no field must not
/// answer with.
#[test]
fn ac2_the_check_reads_the_field_and_refuses_a_word_that_is_not_on_or_off() {
    let good = jq(".analytics = \"on\"", &printed("busy"));
    assert_eq!(
        schema_errors("status", &good),
        "",
        "a sample with analytics does not fit the schema"
    );
    let bad = jq(".analytics = \"maybe\"", &printed("busy"));
    has(
        &schema_errors("status", &bad),
        ".analytics: \"maybe\" is not one of",
    );
}

/// The manual names every field of the schema with a row of its own
/// (console/tests/status_contract.rs:265), and today says the feature only
/// names the command, not that analytics are on (host/docs/api.md:1154).
#[test]
fn ac2_the_manual_has_a_row_for_the_field() {
    let manual = read(&root().join("host/docs/api.md"));
    has(&manual, "| `analytics` |");
}
