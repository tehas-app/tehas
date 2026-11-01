//! What tehas reads of the image's `--capabilities` answer. Only the contract
//! the runner publishes is read (docs/headless.md in pi-rukas). The answer is
//! one JSON object: anything else, or a value of the wrong type on a path this
//! must walk, is an answer that cannot be read.

use crate::runner::{num, values, RESULT_SCHEMA};
use regex::Regex;
use serde_json::Value;
use std::path::Path;

const EXIT_CODES: u64 = 1;
const NDJSON_STABLE: u64 = 1;

static NULL: Value = Value::Null;

fn answer(text: &str) -> Option<Value> {
    let mut all = values(text)?;
    (all.len() == 1 && all[0].is_object()).then(|| all.remove(0))
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(' ').filter(|w| !w.is_empty())
}

/// `.[]?`: the members of an array or an object, nothing of anything else.
fn items(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(Value::Object(o)) => o.values().collect(),
        _ => vec![],
    }
}

/// A path that runs through an object or null, or None when it runs through
/// anything else.
fn dig<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |cur, k| match cur {
        Value::Object(o) => Some(o.get(*k).unwrap_or(&NULL)),
        Value::Null => Some(&NULL),
        _ => None,
    })
}

fn is_num(v: Option<&Value>, want: u64) -> bool {
    v.and_then(Value::as_f64) == Some(want as f64)
}

fn name_chars(s: &str, extra: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || extra.contains(c))
}

/// The pattern of one input name. A family such as PI_ENSEMBLE_MODEL_<ROLE>
/// takes its listed members, or any suffix of name characters when the build
/// lists none.
fn family(name: &str, members: Option<&Value>) -> Option<Regex> {
    let ids: Vec<&str> = items(members)
        .into_iter()
        .filter_map(Value::as_str)
        .filter(|m| name_chars(m, ""))
        .collect();
    let with = if ids.is_empty() {
        "[A-Z0-9_]+".to_string()
    } else {
        format!("({})", ids.join("|"))
    };
    let holes = Regex::new("<[A-Z]+>").ok()?;
    let pattern = holes.replace_all(name, regex::NoExpand(&with));
    // jq's `$` also matches before a last newline, and a list of names may
    // carry one on a word.
    Regex::new(&format!("^{pattern}\n?$")).ok()
}

fn inputs(caps: &Value) -> Vec<Regex> {
    items(caps.get("inputs"))
        .into_iter()
        .filter(|i| i.is_object())
        .filter(|i| {
            matches!(
                i.get("scope").and_then(Value::as_str),
                Some("cycle" | "sandbox")
            )
        })
        .filter_map(|i| {
            let name = i.get("name")?.as_str()?;
            name_chars(name, "<>")
                .then(|| family(name, i.get("members")))
                .flatten()
        })
        .collect()
}

fn version(caps: &Value, path: &[&str], want: u64, what: &str) -> Option<Option<String>> {
    let got = dig(caps, path)?;
    if is_num(Some(got), want) {
        return Some(None);
    }
    let shown = match got {
        Value::String(s) => s.clone(),
        Value::Number(_) => num(got),
        v => v.to_string(),
    };
    Some(Some(format!("{what} version {shown}, not {want}")))
}

/// What the answer does not support, one line each, in plain characters: a
/// contract version other than the one read here, a flag in `flags` that the
/// work verb does not take, a name in `names` that is no input a host may set,
/// a name in `features` the answer does not set to true. None when the answer
/// cannot be read.
pub fn unsupported(text: &str, flags: &str, names: &str, features: &str) -> Option<String> {
    let caps = answer(text)?;
    let flags_taken = dig(&caps, &["verbs", "work", "flags"])?;
    let have: Vec<&str> = items(Some(flags_taken))
        .into_iter()
        .filter_map(Value::as_str)
        .collect();
    let ok = inputs(&caps);
    let mut lines: Vec<String> = [
        (
            &["contracts", "exitCodes", "version"][..],
            EXIT_CODES,
            "exit codes",
        ),
        (
            &["contracts", "resultSchema", "version"][..],
            RESULT_SCHEMA,
            "result schema",
        ),
        (
            &["contracts", "ndjson", "stable", "version"][..],
            NDJSON_STABLE,
            "stable NDJSON",
        ),
    ]
    .into_iter()
    .map(|(path, want, what)| version(&caps, path, want, what))
    .collect::<Option<Vec<_>>>()?
    .into_iter()
    .flatten()
    .collect();
    lines.extend(
        words(flags)
            .filter(|f| f.starts_with("--") && !have.contains(f))
            .map(|f| format!("the flag {f}")),
    );
    lines.extend(
        words(names)
            .filter(|n| !ok.iter().any(|re| re.is_match(n)))
            .map(|n| format!("the input {n}")),
    );
    let set = caps.get("features");
    lines.extend(
        words(features)
            .filter(|f| set.and_then(|s| s.get(f)) != Some(&Value::Bool(true)))
            .map(|f| format!("the feature {f}")),
    );
    Some(
        lines
            .iter()
            .map(|l| format!("{l}\n"))
            .collect::<String>()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "_ .,<>\n-".contains(*c))
            .collect(),
    )
}

fn readable_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    (2..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| (1..=6).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_digit()))
}

/// Whether version `want` is `have` or earlier, as `sort -V` orders them: part
/// by part as numbers, then as text.
fn reached(want: &str, have: &str) -> bool {
    let key = |v: &str| -> Option<Vec<u64>> { v.split('.').map(|p| p.parse().ok()).collect() };
    match (key(want), key(have)) {
        (Some(w), Some(h)) => (w, want) <= (h, have),
        _ => false,
    }
}

/// The workarounds (NAME@VERSION, space separated) the image's pi-rukas no
/// longer needs: its version is VERSION or later. A version that cannot be
/// read expires them all, said once: whether the fix is there is not known.
pub fn expired(text: &str, workarounds: &str) -> Vec<String> {
    if workarounds.is_empty() {
        return vec![];
    }
    let caps = answer(text);
    let have = caps
        .as_ref()
        .and_then(|c| c.get("version"))
        .and_then(Value::as_str)
        .filter(|v| readable_version(v));
    let Some(have) = have else {
        return vec![format!(
            "the image's pi-rukas version could not be read, so it is not known whether {workarounds} are still needed"
        )];
    };
    workarounds
        .split_whitespace()
        .filter_map(|entry| {
            let (name, want) = entry.rsplit_once('@').unwrap_or((entry, entry));
            reached(want, have).then(|| {
                format!("{name} works around a pi-rukas defect fixed in {want}, and the image's pi-rukas is {have}")
            })
        })
        .collect()
}

fn feature_name(f: &str) -> bool {
    f.len() <= 64
        && f.starts_with(|c: char| c.is_ascii_lowercase())
        && f.chars().all(|c| c.is_ascii_alphanumeric())
}

/// How tehas gets a feature, a key of the answer's features, and the status
/// to exit with: `native` when the answer sets it to true, `polyfill` when
/// `lib` holds polyfill-<feature>.sh, `off` otherwise. Every check of a
/// pi-rukas feature asks this, so a polyfill is used only until the image has
/// the feature, and scripts/polyfills.sh can tell when to remove it.
///
/// A failed probe reads as no feature of any kind, so it is said as `error`:
/// only an object whose features, if it has them, are an object can be read.
/// An object with no `features` key is the answer of an image that has none.
pub fn mode(text: &str, feature: &str, lib: Option<&Path>) -> (&'static str, i32) {
    if !feature_name(feature) {
        return ("off", 1);
    }
    let Some(caps) = answer(text) else {
        return ("error", 1);
    };
    let features = caps.get("features");
    if features.is_some_and(|f| !f.is_object()) {
        return ("error", 1);
    }
    if features.and_then(|f| f.get(feature)) == Some(&Value::Bool(true)) {
        ("native", 0)
    } else if lib.is_some_and(|l| l.join(format!("polyfill-{feature}.sh")).is_file()) {
        ("polyfill", 0)
    } else {
        ("off", 0)
    }
}

/// Whether the answer names `verb` with every flag in `flags`. The verbs are a
/// different key of the answer from the features `mode` reads.
pub fn verb(text: &str, verb: &str, flags: &str) -> bool {
    let Some(caps) = answer(text) else {
        return false;
    };
    let Some(v) = caps.get("verbs").and_then(|o| o.get(verb)) else {
        return false;
    };
    if !v.is_object() {
        return false;
    }
    let have: Vec<&str> = match v.get("flags") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => vec![],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => return false,
    };
    words(flags).all(|f| have.contains(&f))
}

/// Whether the answer lists PI_ENSEMBLE_ISSUE_IMAGES for the manifest format
/// tehas writes (manifestVersion 1).
pub fn reads_images(text: &str) -> bool {
    answer(text).is_some_and(|caps| {
        items(caps.get("inputs")).into_iter().any(|i| {
            i.is_object()
                && i.get("name").and_then(Value::as_str) == Some("PI_ENSEMBLE_ISSUE_IMAGES")
                && is_num(i.get("manifestVersion"), 1)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#""contracts":{"exitCodes":{"version":1},"resultSchema":{"version":5},"ndjson":{"stable":{"version":1}}}"#;

    fn with(rest: &str) -> String {
        format!("{{{GOOD},{rest}}}")
    }

    fn fixture() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/fixtures/headless-c5ddc0a/capabilities.json"
        ))
        .unwrap()
    }

    #[test]
    fn the_fixture_answer_takes_what_a_cycle_is_given() {
        let caps = fixture();
        assert_eq!(
            unsupported(
                &caps,
                "--timeout 360 --json --restart",
                "PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_MODEL_DEVELOPER",
                "handoffChannel"
            )
            .unwrap(),
            ""
        );
        assert!(reads_images(&caps));
        assert_eq!(mode(&caps, "planTurns", None), ("native", 0));
    }

    #[test]
    fn each_contract_version_is_compared_as_a_number() {
        let a = r#"{"contracts":{"exitCodes":{"version":1.0},"resultSchema":{"version":"5"},"ndjson":{"stable":{"version":9}}}}"#;
        assert_eq!(
            unsupported(a, "", "", "").unwrap(),
            "result schema version 5, not 5\nstable NDJSON version 9, not 1\n"
        );
        assert_eq!(
            unsupported("{}", "", "", "").unwrap(),
            "exit codes version null, not 1\nresult schema version null, not 5\nstable NDJSON version null, not 1\n"
        );
    }

    #[test]
    fn a_path_through_a_wrong_type_is_no_answer() {
        for bad in [
            r#"{"contracts":7}"#,
            r#"{"contracts":{"ndjson":{"stable":"x"}}}"#,
            r#"{"verbs":7}"#,
            r#"{"verbs":{"work":[]}}"#,
        ] {
            assert_eq!(unsupported(bad, "", "", ""), None, "{bad}");
        }
    }

    #[test]
    fn only_one_object_is_an_answer() {
        for bad in ["", "not json", "[]", "7", "null", "{} {}", "{} x"] {
            assert_eq!(unsupported(bad, "", "", ""), None, "{bad}");
            assert_eq!(mode(bad, "planTurns", None), ("error", 1), "{bad}");
            assert!(!verb(bad, "work", ""), "{bad}");
            assert!(!reads_images(bad), "{bad}");
        }
    }

    #[test]
    fn a_family_takes_its_members_or_any_name_characters() {
        let a = with(
            r#""inputs":[{"name":"M_<ROLE>_X","scope":"cycle","members":["A|.*","OPS",7]},
            {"name":"F_<ROLE>","scope":"sandbox"},{"name":"H_<ROLE>","scope":"other"}]"#,
        );
        assert_eq!(
            unsupported(&a, "", "M_OPS_X M_A_X M_B_X F_ANY_1 F_ H_A", "").unwrap(),
            "the input M_A_X\nthe input M_B_X\nthe input F_\nthe input H_A\n"
        );
    }

    #[test]
    fn a_word_may_end_in_a_newline() {
        let a = with(r#""inputs":[{"name":"P_A","scope":"cycle"}]"#);
        assert_eq!(unsupported(&a, "", "P_A\n", "").unwrap(), "");
    }

    #[test]
    fn a_name_of_the_answer_is_no_pattern() {
        let a = with(
            r#""inputs":[{"name":"P_.*","scope":"cycle"},{"name":"P_A|P_B","scope":"cycle"}]"#,
        );
        assert_eq!(unsupported(&a, "", "P_A", "").unwrap(), "the input P_A\n");
    }

    #[test]
    fn words_split_on_spaces_and_flag_values_are_not_flags() {
        let a = with(r#""verbs":{"work":{"flags":["--json",7]}}"#);
        assert_eq!(
            unsupported(&a, "  --timeout 360  --json ", "", "").unwrap(),
            "the flag --timeout\n"
        );
    }

    #[test]
    fn what_is_said_is_plain_characters() {
        let a = "{\"contracts\":{\"exitCodes\":{\"version\":\"1\\u001b[31m;x|y\"}}}";
        assert!(unsupported(a, "", "", "")
            .unwrap()
            .starts_with("exit codes version 131mxy, not 1\n"));
    }

    #[test]
    fn a_workaround_expires_as_sort_v_orders_versions() {
        for (want, have, yes) in [
            ("0.12.80", "0.12.80", true),
            ("0.9.100", "0.12.0", true),
            ("0.12.81", "0.12.80", false),
            ("1.2", "1.2.0", true),
            ("1.2.0", "1.2", false),
            ("1.2", "1.02", false),
            ("1.02", "1.2", true),
        ] {
            assert_eq!(reached(want, have), yes, "{want} {have}");
        }
    }

    #[test]
    fn a_version_that_cannot_be_read_expires_all_of_them_once() {
        for bad in [
            r#"{"version":null}"#,
            r#"{"version":"0.1; rm"}"#,
            "{}",
            r#"{"version":"1"}"#,
        ] {
            let said = expired(bad, "A@1.0 B@2.0");
            assert_eq!(said.len(), 1);
            assert!(said[0].contains("whether A@1.0 B@2.0 are still needed"));
        }
        assert!(expired("not json", "").is_empty());
    }

    #[test]
    fn a_feature_name_is_checked_before_the_answer() {
        for bad in ["", "../x", "Plan", "a-b", "a b"] {
            assert_eq!(mode("not json", bad, None), ("off", 1), "{bad}");
        }
        assert_eq!(mode("{}", &"a".repeat(64), None), ("off", 0));
        assert_eq!(mode("{}", &"a".repeat(65), None), ("off", 1));
    }

    #[test]
    fn features_of_null_or_of_a_list_cannot_be_read() {
        assert_eq!(mode(r#"{"features":null}"#, "x", None), ("error", 1));
        assert_eq!(mode(r#"{"features":[]}"#, "x", None), ("error", 1));
        assert_eq!(mode(r#"{"features":{"x":"true"}}"#, "x", None), ("off", 0));
    }

    #[test]
    fn a_verb_needs_an_object_and_every_flag() {
        let a = r#"{"verbs":{"plan":{"flags":["--issue"]},"bare":{},"none":{"flags":null},"odd":{"flags":"x"},"no":7}}"#;
        assert!(verb(a, "plan", "--issue"));
        assert!(!verb(a, "plan", "--issue --json"));
        assert!(verb(a, "bare", ""));
        assert!(verb(a, "none", ""));
        assert!(!verb(a, "odd", ""));
        assert!(!verb(a, "no", ""));
        assert!(!verb(a, "gone", ""));
        assert!(!verb(r#"{"verbs":7}"#, "plan", ""));
    }

    #[test]
    fn images_are_read_only_for_manifest_format_one() {
        let say = |m: &str| format!(r#"{{"inputs":[7,{{"name":"PI_ENSEMBLE_ISSUE_IMAGES"{m}}}]}}"#);
        assert!(reads_images(&say(r#","manifestVersion":1"#)));
        assert!(!reads_images(&say(r#","manifestVersion":2"#)));
        assert!(!reads_images(&say("")));
    }
}
