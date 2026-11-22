//! What tehas reads of pi-rukas's headless runner's --capabilities answer:
//! the caps verbs of tehas-cycle, run as the built program.

mod headless;

use headless::*;
use std::fs;
use std::path::PathBuf;
use tehas_core::testdir::TestDir;
use tehas_core::testing::copy_exe;

/// The result schema tehas-cycle takes.
const RESULT: u64 = 5;

/// What tehas work gives a cycle's container, of PI_ENSEMBLE_* names.
const GIVEN: &str = "PI_ENSEMBLE_SANDBOX_MODE PI_ENSEMBLE_WORKSPACE_ROOT PI_ENSEMBLE_MAX_WORKSTREAMS PI_ENSEMBLE_VERIFY_TIMEOUT_MS
  PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER";

/// The --capabilities answers of the three runners.
fn caps() -> String {
    read(fx().join("capabilities.json"))
}
fn ncaps() -> String {
    read(nx().join("capabilities.json"))
}
fn scaps() -> String {
    read(sx().join("capabilities.json"))
}

/// `caps unsupported <flags> <names> <features>` of an answer.
fn unsupported(answer: &str, flags: &str, names: &str, features: &str) -> Out {
    exec(
        &["caps", "unsupported", flags, names, features],
        Some(answer),
    )
}

fn reads_images(answer: &str) -> String {
    exec(&["caps", "reads-images"], Some(answer)).out
}

fn mode(answer: &str, feature: &str) -> Out {
    exec(&["caps", "mode", feature], Some(answer))
}

fn expired(answer: &str, workarounds: &str) -> String {
    exec(&["caps", "expired", workarounds], Some(answer)).out
}

/// The pointer of the input of that name in a --capabilities answer.
fn input_at(answer: &str, name: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(answer).unwrap();
    let i = v["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .position(|x| x["name"] == name)
        .unwrap();
    format!("/inputs/{i}")
}

/// A tehas-cycle in libexec/ of a scratch tree, beside the lib/ whose
/// polyfills it reads, and lib/polyfill-someFeature.sh there.
fn beside(t: &TestDir) -> PathBuf {
    fs::create_dir_all(t.join("lib")).unwrap();
    fs::create_dir_all(t.join("libexec")).unwrap();
    let program = t.join("libexec/tehas-cycle");
    copy_exe(CYCLE, &program).unwrap();
    fs::write(t.join("lib/polyfill-someFeature.sh"), "").unwrap();
    program
}

#[test]
fn the_runner_of_v0140_on_result_schema_5_supports_what_tehas_gives_a_cycle() {
    let given = format!("{GIVEN} PI_ENSEMBLE_EXTRA_IMAGE_DIRS");
    let flags = "--timeout 360 --json --restart".to_string();
    assert_eq!(unsupported(&scaps(), &flags, &given, "").out, "");
}

#[test]
fn the_result_schema_the_bash_reads_is_the_one_tehascycle_takes() {
    let answer = edit(
        &scaps(),
        "/contracts/resultSchema/version",
        Some(&RESULT.to_string()),
    );
    assert_eq!(unsupported(&answer, "--json", "", "").out, "");
}

#[test]
fn a_runner_on_result_schema_3_is_refused_for_that_alone() {
    // These runners came before the suite tier and its bound.
    let old_given = GIVEN.replacen(" PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS", "", 1);
    let names = format!("{old_given} PI_ENSEMBLE_EXTRA_IMAGE_DIRS");
    let flags = "--timeout 360 --json --restart".to_string();
    assert_eq!(
        unsupported(&caps(), &flags, &names, "").out,
        "result schema version 3, not 5"
    );
    assert_eq!(
        unsupported(&ncaps(), &flags, &names, "").out,
        "result schema version 3, not 5"
    );
}

#[test]
fn a_roles_model_and_provider_are_inputs_of_their_family() {
    let names = "PI_ENSEMBLE_MODEL_DEVELOPER PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST";
    assert_eq!(unsupported(&scaps(), "--json", names, "").out, "");
}

#[test]
fn a_name_the_build_does_not_read_one_it_sets_on_its_own_children_and_one_of_its_wrapper_are_each_named(
) {
    let names = "PI_ENSEMBLE_NOT_A_LEVER PI_ENSEMBLE_SUBAGENT_MODE PI_ENSEMBLE_NO_SSH";
    assert_eq!(
        unsupported(&scaps(), "--json", names, "").out,
        "the input PI_ENSEMBLE_NOT_A_LEVER
the input PI_ENSEMBLE_SUBAGENT_MODE
the input PI_ENSEMBLE_NO_SSH"
    );
}

#[test]
fn only_the_flags_are_checked_not_their_values() {
    let flags = "--timeout 360 --json --bogus".to_string();
    assert_eq!(
        unsupported(&scaps(), &flags, "", "").out,
        "the flag --bogus"
    );
}

#[test]
fn each_contract_version_is_checked() {
    let mut answer = edit(&scaps(), "/contracts/exitCodes/version", Some("2"));
    answer = edit(&answer, "/contracts/resultSchema/version", Some("6"));
    answer = edit(&answer, "/contracts/ndjson/stable/version", Some("9"));
    assert_eq!(
        unsupported(&answer, "", "", "").out,
        "exit codes version 2, not 1
result schema version 6, not 5
stable NDJSON version 9, not 1"
    );
    let out = unsupported("{}", "", "", "").out;
    assert!(out.contains("exit codes version null"), "{out}");
}

#[test]
fn an_answer_that_is_not_an_object_is_no_answer() {
    for bad in ["", "not json", "[]", "7", "null"] {
        assert_eq!(unsupported(bad, "", "", "").code, 1, "{bad}");
    }
}

#[test]
fn a_name_in_the_answer_is_a_pattern_only_of_name_characters() {
    let answer = r#"{"contracts":{"exitCodes":{"version":1},"resultSchema":{"version":5},"ndjson":{"stable":{"version":1}}},
  "inputs":[{"name":"PI_ENSEMBLE_.*","scope":"cycle"},{"name":"PI_ENSEMBLE_A|PI_ENSEMBLE_B","scope":"cycle"}]}"#;
    assert_eq!(
        unsupported(answer, "", "PI_ENSEMBLE_A", "").out,
        "the input PI_ENSEMBLE_A"
    );
}

#[test]
fn what_is_said_of_the_answer_is_plain_characters() {
    let answer = r#"{"contracts":{"exitCodes":{"version":"1\u001b[31m;x|y"}}}"#;
    let out = unsupported(answer, "", "", "").out;
    assert_eq!(
        out.lines().next().unwrap_or(""),
        "exit codes version 131mxy, not 1"
    );
}

#[test]
fn a_family_that_lists_its_members_takes_only_those() {
    let names = "PI_ENSEMBLE_MODEL_DEVELOPER PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST PI_ENSEMBLE_MODEL_LENS_SECURITY PI_ENSEMBLE_TOKEN_BUDGET_OPS";
    assert_eq!(unsupported(&scaps(), "--json", names, "").out, "");
    let names =
        "PI_ENSEMBLE_MODEL_REVIEWER PI_ENSEMBLE_MODEL_LENS_STYLE PI_ENSEMBLE_MODEL_DEVELOPERX";
    assert_eq!(
        unsupported(&scaps(), "--json", names, "").out,
        "the input PI_ENSEMBLE_MODEL_REVIEWER
the input PI_ENSEMBLE_MODEL_LENS_STYLE
the input PI_ENSEMBLE_MODEL_DEVELOPERX"
    );
    // A family that lists no members, as v0.12.80's did: any name characters.
    let s = scaps();
    let at = input_at(&s, "PI_ENSEMBLE_MODEL_<ROLE>");
    let answer = edit(&s, &format!("{at}/members"), None);
    assert_eq!(
        unsupported(&answer, "--json", "PI_ENSEMBLE_MODEL_REVIEWER", "").out,
        ""
    );
}

#[test]
fn a_member_is_only_of_name_characters() {
    let answer = r#"{"contracts":{"exitCodes":{"version":1},"resultSchema":{"version":5},"ndjson":{"stable":{"version":1}}},
  "inputs":[{"name":"PI_ENSEMBLE_MODEL_<ROLE>","scope":"cycle","members":["A|.*","OPS"]}]}"#;
    let names = "PI_ENSEMBLE_MODEL_A PI_ENSEMBLE_MODEL_X PI_ENSEMBLE_MODEL_OPS";
    assert_eq!(
        unsupported(answer, "", names, "").out,
        "the input PI_ENSEMBLE_MODEL_A
the input PI_ENSEMBLE_MODEL_X"
    );
}

#[test]
fn an_image_reads_issue_images_when_it_lists_piensembleissueimages_for_manifest_format_1() {
    let s = scaps();
    assert_eq!(reads_images(&s), "1");
    let at = input_at(&s, "PI_ENSEMBLE_ISSUE_IMAGES");
    let v2 = edit(&s, &format!("{at}/manifestVersion"), Some("2"));
    assert_eq!(reads_images(&v2), "0");
    let without = edit(&s, &at, None);
    assert_eq!(reads_images(&without), "0");
    for bad in ["", "not json", "[]"] {
        assert_eq!(reads_images(bad), "0", "{bad}");
    }
}

#[test]
fn a_feature_is_native_where_the_answer_sets_it_to_true_planturns_in_v0140_not_in_01280() {
    assert_eq!(mode(&scaps(), "planTurns").out, "native");
    assert_eq!(mode(&caps(), "planTurns").out, "off");
    assert_eq!(
        mode(r#"{"features":{"planTurns":false}}"#, "planTurns").out,
        "off"
    );
    assert_eq!(
        mode(r#"{"features":{"planTurns":"true"}}"#, "planTurns").out,
        "off"
    );
    for bad in ["", "not json", "[]", r#"{"features":[]}"#] {
        // t_run compared output and error as one.
        let o = mode(bad, "planTurns");
        assert_eq!(o.merged(), "error", "{bad}");
        assert_eq!(o.err, "", "{bad}");
        assert_eq!(o.code, 1, "{bad}");
    }
}

#[test]
fn a_feature_the_answer_lacks_is_a_polyfill_where_tehas_carries_one_and_native_wins_over_it() {
    // The polyfills are the ones beside the program's libexec/.
    let t = scratch();
    let program = beside(&t);
    let said: Vec<String> = [
        ("someFeature", r#"{"features":{}}"#),
        ("someFeature", r#"{"features":{"someFeature":true}}"#),
        ("otherFeature", "{}"),
    ]
    .iter()
    .map(|(f, a)| {
        let o = exec_at(&program, &["caps", "mode", f], Some(a));
        assert_eq!(o.err, "", "{f}");
        format!("{}{}", o.raw, o.err)
    })
    .collect();
    // One t_run of the three: their output and error as one.
    assert_eq!(
        said.concat().trim_end_matches('\n'),
        "polyfill\nnative\noff"
    );
}

#[test]
fn an_answer_that_cannot_be_read_is_error_where_tehas_carries_a_polyfill_not_polyfill() {
    let t = scratch();
    let program = beside(&t);
    for bad in ["", "not json", "[]", r#"{"features":[]}"#] {
        let o = exec_at(&program, &["caps", "mode", "someFeature"], Some(bad));
        assert_eq!(o.merged(), "error", "{bad}");
        assert_eq!(o.err, "", "{bad}");
        assert_eq!(o.code, 1, "{bad}");
    }
}

#[test]
fn a_name_that_is_no_feature_name_is_off_with_status_1_and_names_no_file() {
    let t = scratch();
    let program = beside(&t);
    fs::write(t.join("lib/polyfill-.sh"), "").unwrap();
    for bad in ["", "../x", "Plan", "a-b", "a b"] {
        let o = exec_at(&program, &["caps", "mode", bad], Some("{}"));
        assert_eq!(o.merged(), "off", "{bad}");
        assert_eq!(o.err, "", "{bad}");
        assert_eq!(o.code, 1, "{bad}");
    }
}

#[test]
fn the_features_this_command_relies_on_are_checked_and_only_those_asked_for() {
    let asked = "handoffChannel cycleDeadline restartClearsAttention";
    let s = scaps();
    assert_eq!(unsupported(&s, "--json", "", asked).out, "");
    let mut answer = edit(&s, "/features/handoffChannel", None);
    answer = edit(&answer, "/features/cycleDeadline", Some(r#""yes""#));
    assert_eq!(
        unsupported(&answer, "--json", "", asked).out,
        "the feature handoffChannel
the feature cycleDeadline"
    );
    let none = edit(&s, "/features", None);
    assert_eq!(unsupported(&none, "--json", "", "").out, "");
    let list = edit(&s, "/features", Some("[1]"));
    assert_eq!(
        unsupported(&list, "--json", "", "handoffChannel").out,
        "the feature handoffChannel"
    );
}

#[test]
fn a_workaround_expires_at_the_release_that_fixes_it() {
    let s = scaps();
    assert_eq!(expired(&s, ""), "");
    assert_eq!(
        expired(
            &s,
            "PI_ENSEMBLE_COMMENT_RETENTION@0.12.80 TEHAS_WORK_IMAGES@0.12.81 PI_ENSEMBLE_X@0.9.100"
        ),
        "PI_ENSEMBLE_COMMENT_RETENTION works around a pi-rukas defect fixed in 0.12.80, and the image's pi-rukas is 0.14.0
TEHAS_WORK_IMAGES works around a pi-rukas defect fixed in 0.12.81, and the image's pi-rukas is 0.14.0
PI_ENSEMBLE_X works around a pi-rukas defect fixed in 0.9.100, and the image's pi-rukas is 0.14.0"
    );
    let older = edit(&s, "/version", Some(r#""0.13.1""#));
    assert_eq!(
        expired(&older, "TEHAS_WORK_IMAGES@0.12.81"),
        "TEHAS_WORK_IMAGES works around a pi-rukas defect fixed in 0.12.81, and the image's pi-rukas is 0.13.1"
    );
    for bad in [
        edit(&s, "/version", Some("null")),
        edit(&s, "/version", Some(r#""0.12.80; rm""#)),
        edit(&s, "/version", None),
    ] {
        let out = expired(&bad, "TEHAS_WORK_IMAGES@0.12.81");
        assert!(out.contains("version could not be read"), "{out}");
    }
}
