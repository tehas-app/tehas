//! The contract of tehas status --json. api/status.schema.json says what the
//! object holds. The samples beside it, api/status.busy.json,
//! api/status.quiet.json, api/status.readonly.json, api/status.pushfault.json,
//! api/status.outsourced.json and api/status.lab.json (TEHAS_TEST_FEATURES set
//! on a lab build), are what console status json (console/src/status.rs) makes
//! of the health files in test/fixtures/status-json/, with the Protolab's list
//! beside one of them and the contractors of outsource beside another, at a
//! fixed time. Only the outsourced sample has `contractors`. A program that
//! reads the object copies the samples into its own tests, so a change here is
//! a change for it: docs/specs/2026-10-24-status-json-contract.md.
//!
//! To write the samples again after a change to the object:
//!   TEHAS_SAMPLES_WRITE=1 cargo test --locked -p console --test status_contract

mod common;

use common::*;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use tehas_core::testdir::TestDir;

/// 2026-10-17T14:00:20Z: forty seconds after the time in the health files.
const NOW: &str = "1792245620";
const SAMPLES: &[&str] = &[
    "busy",
    "quiet",
    "readonly",
    "pushfault",
    "outsourced",
    "lab",
];

fn api(name: &str) -> PathBuf {
    root().join("api").join(name)
}

fn fixtures() -> PathBuf {
    root().join("test/fixtures/status-json")
}

fn schema_file() -> PathBuf {
    api("status.schema.json")
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn sample_file(name: &str) -> String {
    maybe_write();
    read(&api(&format!("status.{name}.json")))
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// jq with these arguments of `input`, its standard output.
fn jq_args(args: &[&str], input: &str) -> String {
    let mut child = Command::new("jq")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The sample as the console makes it, as committed.
fn sample(name: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_console"))
        .args(["status", "json"])
        .arg(fixtures().join(format!("{name}.health.json")))
        .arg(NOW)
        .output()
        .unwrap();
    jq_args(&["."], &String::from_utf8_lossy(&out.stdout))
}

/// The samples are written again once per run when TEHAS_SAMPLES_WRITE asks.
fn maybe_write() {
    static WRITE: Once = Once::new();
    WRITE.call_once(|| {
        if std::env::var_os("TEHAS_SAMPLES_WRITE").is_some_and(|v| !v.is_empty()) {
            for name in SAMPLES {
                fs::write(api(&format!("status.{name}.json")), sample(name)).unwrap();
            }
        }
    });
}

/// The lines test/schema.jq prints for `value` against the schema at `schema`.
fn check_with(schema: &Path, value: &str) -> String {
    jq_args(
        &[
            "-r",
            "--slurpfile",
            "schema",
            &schema.to_string_lossy(),
            "-f",
            &root().join("test/schema.jq").to_string_lossy(),
        ],
        value,
    )
    .trim_end_matches('\n')
    .to_string()
}

fn check(value: &str) -> String {
    check_with(&schema_file(), value)
}

/// A sample with a jq change made to it.
fn bad(name: &str, filter: &str) -> String {
    jq(filter, &sample_file(name))
}

/// Every field the schema names, as a path: paused.local, running.step.
fn schema_paths() -> BTreeSet<String> {
    fn walk(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
        let Some(props) = v.get("properties").and_then(Value::as_object) else {
            return;
        };
        for (k, p) in props {
            let q = format!("{prefix}{k}");
            out.insert(q.clone());
            walk(p.get("items").unwrap_or(p), &format!("{q}."), out);
        }
    }
    let mut out = BTreeSet::new();
    walk(&json(&read(&schema_file())), "", &mut out);
    out
}

/// The paths of a value by its keys alone, indexes left out: running.step.
fn held_paths(v: &Value, prefix: &[String], out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let mut p = prefix.to_vec();
                p.push(k.clone());
                out.insert(p.join("."));
                held_paths(x, &p, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                if !prefix.is_empty() {
                    out.insert(prefix.join("."));
                }
                held_paths(x, prefix, out);
            }
        }
        _ => {}
    }
}

fn schema_description(field: &str) -> String {
    json(&read(&schema_file()))["properties"][field]["description"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn api_status_name_json_is_what_the_console_makes_of_name_health_json() {
    for name in SAMPLES {
        assert_eq!(
            sample_file(name),
            sample(name),
            "{name}: the code prints another object; check the change is only an addition, then write the samples again"
        );
    }
}

#[test]
fn api_status_name_json_fits_the_schema() {
    for name in SAMPLES {
        assert_eq!(check(&sample_file(name)), "", "{name}");
    }
}

#[test]
fn tehas_status_json_prints_what_the_sample_holds_but_for_the_clock() {
    let w = World::new("contract-samples");
    fs::create_dir_all(&w.live).unwrap();
    let clockless = "del(.age, .stale, .demand.oldestWait?)";
    for name in SAMPLES {
        fs::copy(
            fixtures().join(format!("{name}.health.json")),
            w.live_file("health.json"),
        )
        .unwrap();
        for beside in ["protolab", "previews", "contractors"] {
            let to = w.live_file(&format!("{beside}.json"));
            let _ = fs::remove_file(&to);
            let from = fixtures().join(format!("{name}.{beside}.json"));
            if from.exists() {
                fs::copy(from, to).unwrap();
            }
        }
        let r = w.summary(&["--json"]);
        // The health file is days old: stale, so the exit status is 3.
        assert_eq!(r.rc, 3, "{name}: {}", r.out);
        assert_eq!(
            jq(clockless, &r.out),
            jq(clockless, &sample_file(name)),
            "{name}"
        );
    }
}

#[test]
fn tehas_status_json_gives_a_preview_s_address_only_while_it_serves() {
    let w = World::new("contract-previews");
    fs::create_dir_all(&w.live).unwrap();
    fs::copy(
        fixtures().join("busy.health.json"),
        w.live_file("health.json"),
    )
    .unwrap();
    // A preview whose build failed keeps the address it served at before. The
    // address is where a person tries the change, so a stale one is not given.
    w.home_file(
        "live/previews.json",
        r#"{"generatedAt":"2026-10-17T13:58:00Z","previews":[
 {"number":31,"state":"serving","address":"https://uat-31.example.org/"},
 {"number":26,"state":"build-failed","address":"https://uat-26.example.org/"},
 {"number":25,"state":"removing","address":"https://uat-25.example.org/"}]}
"#,
    );
    let r = w.summary(&["--json"]);
    assert_eq!(r.rc, 3, "{}", r.out);
    assert_eq!(
        jq(".previews", &r.out),
        r#"[{"number":31,"state":"serving","url":"https://uat-31.example.org/"},{"number":26,"state":"build-failed"},{"number":25,"state":"removing"}]"#
    );
}

#[test]
fn the_samples_between_them_hold_every_field_of_the_schema() {
    let mut held = BTreeSet::new();
    for name in SAMPLES {
        held_paths(&json(&sample_file(name)), &[], &mut held);
    }
    let paths = schema_paths();
    assert!(paths.len() > 10, "{paths:?}");
    let missing: Vec<_> = paths.difference(&held).cloned().collect();
    assert!(missing.is_empty(), "in no sample: {missing:?}");
}

#[test]
fn the_manual_names_every_field_of_the_schema() {
    let manual = read(&root().join("host/docs/api.md"));
    let mut unnamed = String::new();
    for path in schema_paths() {
        // paused has a row for each of its fields and none of its own; limit,
        // order, running and planning have one row that names their fields.
        let word = match path.split_once('.') {
            _ if path == "paused" => continue,
            Some((head, _))
                if ["limit", "order", "running", "planning", "contractors"].contains(&head) =>
            {
                head
            }
            _ => path.as_str(),
        };
        if !manual.contains(&format!("| `{word}` |")) {
            unnamed.push(' ');
            unnamed.push_str(word);
        }
    }
    assert_eq!(unnamed, "");
}

#[test]
fn the_check_finds_a_field_missing_a_field_of_the_wrong_type_and_a_field_it_does_not_know() {
    let out = check(&bad(
        "busy",
        r#"del(.paused.label) | .age = "40" | .room.later = 1"#,
    ));
    has(&out, ".paused: label is missing");
    has(&out, ".age: string, not integer");
    has(&out, ".room.later: not in the schema");
}

#[test]
fn the_check_finds_a_value_out_of_its_list_under_its_minimum_or_not_a_date_time() {
    let out = check(&bad(
        "busy",
        r#".state = "later" | .demand.waiting = -1 | .running[0].since = "yesterday" | .api = 2"#,
    ));
    has(&out, r#".state: "later" is not one of"#);
    has(&out, ".demand.waiting: -1 is under 0");
    has(&out, r#".running[0].since: "yesterday" is not a date-time"#);
    has(&out, ".api: 2, not 1");
}

/// The object of the case on a running cycle's step and readOnly.
fn step_and_read_only() -> String {
    check(&bad(
        "busy",
        r#".running[0].step = "review" | .running[0].progress.zz = 1 | .readOnly = false"#,
    ))
}

#[test]
fn where_a_running_cycle_s_step_is_goes_only_under_progress_and_read_only_is_only_true() {
    let out = step_and_read_only();
    has(&out, ".running[0].step: not in the schema");
    has(&out, ".running[0].progress.zz: not in the schema");
}

#[test]
fn a_progress_with_nothing_in_it_is_left_out_never_printed_empty() {
    has(
        &check(&bad("busy", ".running[0].progress = {}")),
        ".running[0].progress: fewer than 1 properties",
    );
    // The shell case checked the object of the case before here.
    has(&step_and_read_only(), ".readOnly: false, not true");
}

#[test]
fn a_null_where_the_schema_allows_none_is_refused_and_allowed_where_it_does() {
    let out = check(&bad("busy", ".limit = null | .hold = null"));
    has(&out, ".limit: null, not object");
    lacks(&out, ".hold");
}

#[test]
fn a_schema_keyword_the_check_does_not_know_is_an_error_not_passed_over() {
    let t = TestDir::new("status-contract-schema");
    let schema = t.join("schema.json");
    fs::write(
        &schema,
        jq(".properties.age.multipleOf = 10", &read(&schema_file())),
    )
    .unwrap();
    has(
        &check_with(&schema, &sample_file("busy")),
        ".age: the schema uses multipleOf, which this check does not know",
    );
}

#[test]
fn contractors_is_only_in_the_sample_made_with_the_contractors_file_beside_the_health_file() {
    for name in ["busy", "quiet", "readonly"] {
        assert!(
            json(&sample_file(name)).get("contractors").is_none(),
            "{name}"
        );
    }
    assert_eq!(
        jq(
            ".contractors | [.working, .outsourced, (.list | map(.name))]",
            &sample_file("outsourced")
        ),
        r#"[1,1,["box1","box2","box3"]]"#
    );
}

#[test]
fn the_check_finds_a_contractor_s_status_out_of_its_list_and_a_count_it_does_not_know() {
    let out = check(&bad(
        "outsourced",
        r#".contractors.list[0].status = "away" | .contractors.away = 1"#,
    ));
    has(&out, r#".contractors.list[0].status: "away" is not one of"#);
    has(&out, ".contractors.away: not in the schema");
}

#[test]
fn the_schema_says_running_and_room_count_a_cycle_outsourced_to_a_contractor() {
    has(
        &schema_description("running"),
        "outsourced to a contractor is one of them",
    );
    has(
        &schema_description("room"),
        "outsourced to a contractor takes a place",
    );
}

#[test]
fn the_schema_says_the_fields_of_prototypes_may_change() {
    has(
        &schema_description("prototypes"),
        "may change in a minor release",
    );
}

#[test]
fn the_schema_and_the_manual_say_contractors_may_change_while_outsource_is_being_built() {
    let phrase = "may change while outsource is still being built";
    has(&schema_description("contractors"), phrase);
    let row: String = read(&root().join("host/docs/api.md"))
        .lines()
        .filter(|l| l.contains("| `contractors` |"))
        .collect::<Vec<_>>()
        .join("\n");
    has(&row, phrase);
    has(
        &read(&root().join("host/docs/contract.md")).replace('\n', " "),
        "fields of `contractors` may change while outsource is still being built",
    );
}

#[test]
fn a_sample_s_words_name_no_other_program_than_tehas() {
    // A program copies the samples into its own tests, so a reason in them is
    // read as an example of what a factory says.
    let mut files: Vec<PathBuf> = fs::read_dir(root().join("api"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("status.") && n.ends_with(".json")
        })
        .collect();
    files.extend(
        fs::read_dir(fixtures())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json")),
    );
    assert!(files.len() > SAMPLES.len());
    let named: Vec<_> = files
        .iter()
        .filter(|p| read(p).to_lowercase().contains("industry"))
        .collect();
    assert!(named.is_empty(), "{named:?}");
}
