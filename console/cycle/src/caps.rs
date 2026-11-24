//! What tehas reads of the image's `--capabilities` answer. Only the contract
//! the runner publishes is read (docs/headless.md in pi-rukas). The answer is
//! one JSON object: anything else, or a value of the wrong type on a path this
//! must walk, is an answer that cannot be read.

use crate::loose::{values, Field, Loose, Many, Node};
use crate::runner::{num, RESULT_SCHEMA};
use regex::Regex;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use tehas_core::driver::Driver;

const EXIT_CODES: u64 = 1;
const NDJSON_STABLE: u64 = 1;

static ABSENT: Field = Field::Absent;

/// The answer, the fields of it this reads.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Caps {
    contracts: Node<Contracts>,
    features: Node<BTreeMap<String, Field>>,
    verbs: Node<BTreeMap<String, Node<Verb>>>,
    inputs: Many<Node<Input>>,
    version: Loose<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Contracts {
    exit_codes: Node<Versioned>,
    result_schema: Node<Versioned>,
    ndjson: Node<Ndjson>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Ndjson {
    stable: Node<Versioned>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Versioned {
    version: Field,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Verb {
    flags: Many<Loose<String>>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Input {
    scope: Loose<String>,
    name: Loose<String>,
    members: Many<Loose<String>>,
    manifest_version: Loose<f64>,
}

fn answer(text: &str) -> Option<Caps> {
    let mut all = values::<Node<Caps>>(text)?;
    if all.len() != 1 {
        return None;
    }
    all.pop()?.into_object()
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(' ').filter(|w| !w.is_empty())
}

/// One step of a path through an object or null: Some(None) past nothing or
/// null, None where it runs through anything else.
fn step<T>(n: Option<&Node<T>>) -> Option<Option<&T>> {
    match n {
        None | Some(Node::Absent | Node::Null) => Some(None),
        Some(Node::Object(t)) => Some(Some(t)),
        Some(Node::False | Node::Other) => None,
    }
}

fn is_num(v: &Field, want: u64) -> bool {
    matches!(v, Field::Num(x) if x.as_f64() == Some(want as f64))
}

fn name_chars(s: &str, extra: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || extra.contains(c))
}

/// The pattern of one input name. A family such as PI_ENSEMBLE_MODEL_<ROLE>
/// takes its listed members, or any suffix of name characters when the build
/// lists none.
fn family(name: &str, members: &Many<Loose<String>>) -> Option<Regex> {
    let ids: Vec<&str> = members
        .items()
        .iter()
        .filter_map(|m| m.get().map(String::as_str))
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

/// The inputs a host may set, or with `any` every input of any scope.
fn inputs_of(caps: &Caps, any: bool) -> Vec<Regex> {
    caps.inputs
        .items()
        .iter()
        .filter_map(Node::object)
        .filter(|i| any || matches!(i.scope.str(), "cycle" | "sandbox"))
        .filter_map(|i| {
            let name = i.name.get()?;
            name_chars(name, "<>")
                .then(|| family(name, &i.members))
                .flatten()
        })
        .collect()
}

fn inputs(caps: &Caps) -> Vec<Regex> {
    inputs_of(caps, false)
}

/// The image label that names the polyfills an image carries.
pub const POLYFILLS: &str = "tehas.rukas.polyfills";

/// The polyfills a `tehas.rukas.polyfills` label names: entries
/// `<name>@<version>` split on spaces, commas and newlines. An entry with no
/// version, or a name that is not lower-case letters, digits and dashes
/// starting with a letter, names none.
pub fn polyfills(label: &str) -> Vec<&str> {
    label
        .split([' ', ',', '\n'])
        .filter_map(|w| w.split_once('@'))
        .filter(|(n, v)| {
            !v.is_empty()
                && n.len() <= 64
                && n.starts_with(|c: char| c.is_ascii_lowercase())
                && n.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .map(|(n, _)| n)
        .collect()
}

/// The inputs the polyfills of an image declare, each with its polyfill.
/// `labels` is the image's labels as one JSON object. A polyfill named in
/// `tehas.rukas.polyfills` declares its inputs in the label
/// `tehas.rukas.polyfill.<name>.inputs`, names split on spaces and commas.
/// Only a PI_ENSEMBLE_ name counts, written out in full, and only one the
/// answer does not list in any scope: a polyfill adds an input the runner
/// does not know, it does not open one the runner keeps to itself. Nothing
/// when the answer or the labels cannot be read.
pub fn polyfill_inputs(text: &str, labels: &str) -> Vec<(String, String)> {
    let Some(caps) = answer(text) else {
        return vec![];
    };
    let Ok(Node::Object(labels)) =
        serde_json::from_str::<Node<BTreeMap<String, Loose<String>>>>(labels.trim())
    else {
        return vec![];
    };
    let label = |k: &str| labels.get(k).map_or("", Loose::str);
    let known = inputs_of(&caps, true);
    let list = label(POLYFILLS);
    let mut out = vec![];
    for p in polyfills(list) {
        let key = format!("tehas.rukas.polyfill.{p}.inputs");
        let declared = label(&key);
        for n in declared.split([' ', ',', '\n']) {
            let ok = n.len() > "PI_ENSEMBLE_".len()
                && n.starts_with("PI_ENSEMBLE_")
                && name_chars(n, "")
                && !known.iter().any(|re| re.is_match(n));
            if ok && !out.iter().any(|(i, _)| i == n) {
                out.push((n.to_string(), p.to_string()));
            }
        }
    }
    out
}

/// What a contract's version says when it is not `want`, of the version a
/// path came to (None where it ran through a value of another type).
fn version(got: Option<&Field>, want: u64, what: &str) -> Option<Option<String>> {
    let got = got?;
    if is_num(got, want) {
        return Some(None);
    }
    let shown = match got {
        Field::Str(s) => s.clone(),
        Field::Num(x) => num(x),
        v => v.json(),
    };
    Some(Some(format!("{what} version {shown}, not {want}")))
}

/// The version at the end of a path of the contracts: Field::Absent past
/// nothing or null, None through a value of another type.
fn contract<'a>(
    caps: &'a Caps,
    path: impl Fn(&'a Contracts) -> Option<Option<&'a Versioned>>,
) -> Option<&'a Field> {
    let c = step(Some(&caps.contracts))?;
    let v = match c {
        Some(c) => path(c)?,
        None => None,
    };
    Some(v.map_or(&ABSENT, |v| &v.version))
}

/// What the answer does not support, one line each, in plain characters: a
/// contract version other than the one read here, a flag in `flags` that the
/// verb `verb` does not take (every one, when the answer has no such verb), a name in
/// `names` that is no input a host may set, a name in `features` the answer
/// does not set to true. For `work-lean` the answer must also say that it
/// writes the result schema read here. None when the answer cannot be read.
pub fn unsupported(
    text: &str,
    verb: &str,
    flags: &str,
    names: &str,
    features: &str,
) -> Option<String> {
    let caps = answer(text)?;
    let verbs = step(Some(&caps.verbs))?;
    let this = step(verbs.and_then(|v| v.get(verb)))?;
    let have: Vec<&str> = this
        .map(|v| v.flags.items())
        .unwrap_or_default()
        .iter()
        .filter_map(|f| f.get().map(String::as_str))
        .collect();
    let ok = inputs(&caps);
    let mut lines: Vec<String> = [
        (
            contract(&caps, |c| step(Some(&c.exit_codes))),
            EXIT_CODES,
            "exit codes",
        ),
        (
            contract(&caps, |c| step(Some(&c.result_schema))),
            RESULT_SCHEMA,
            "result schema",
        ),
        (
            contract(&caps, |c| match step(Some(&c.ndjson))? {
                Some(n) => step(Some(&n.stable)),
                None => Some(None),
            }),
            NDJSON_STABLE,
            "stable NDJSON",
        ),
    ]
    .into_iter()
    .map(|(got, want, what)| version(got, want, what))
    .collect::<Option<Vec<_>>>()?
    .into_iter()
    .flatten()
    .collect();
    let lean_schema = caps
        .features
        .object()
        .and_then(|f| f.get("workLeanResultSchema"))
        .unwrap_or(&ABSENT);
    if verb == Driver::Lean.verb() && !is_num(lean_schema, RESULT_SCHEMA) {
        lines.push(format!("the feature workLeanResultSchema {RESULT_SCHEMA}"));
    }
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
    let set = caps.features.object();
    lines.extend(
        words(features)
            .filter(|f| set.and_then(|s| s.get(*f)) != Some(&Field::Bool(true)))
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
        .and_then(|c| c.version.get())
        .map(String::as_str)
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
    let features = match &caps.features {
        Node::Absent => None,
        Node::Object(f) => Some(f),
        _ => return ("error", 1),
    };
    if features.and_then(|f| f.get(feature)) == Some(&Field::Bool(true)) {
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
    let Some(v) = caps.verbs.object().and_then(|o| o.get(verb)) else {
        return false;
    };
    let Some(v) = v.object() else {
        return false;
    };
    let have: Vec<&str> = match &v.flags {
        Many::Absent | Many::Null | Many::False => vec![],
        Many::List(a) => a
            .iter()
            .filter_map(|f| f.get().map(String::as_str))
            .collect(),
        _ => return false,
    };
    words(flags).all(|f| have.contains(&f))
}

/// Whether the answer lists PI_ENSEMBLE_ISSUE_IMAGES for the manifest format
/// tehas writes (manifestVersion 1).
pub fn reads_images(text: &str) -> bool {
    answer(text).is_some_and(|caps| {
        caps.inputs
            .items()
            .iter()
            .filter_map(Node::object)
            .any(|i| {
                i.name.str() == "PI_ENSEMBLE_ISSUE_IMAGES" && i.manifest_version.get() == Some(&1.0)
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
                "work",
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
            unsupported(a, "work", "", "", "").unwrap(),
            "result schema version 5, not 5\nstable NDJSON version 9, not 1\n"
        );
        assert_eq!(
            unsupported("{}", "work", "", "", "").unwrap(),
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
            assert_eq!(unsupported(bad, "work", "", "", ""), None, "{bad}");
        }
    }

    #[test]
    fn only_one_object_is_an_answer() {
        for bad in ["", "not json", "[]", "7", "null", "{} {}", "{} x"] {
            assert_eq!(unsupported(bad, "work", "", "", ""), None, "{bad}");
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
            unsupported(&a, "work", "", "M_OPS_X M_A_X M_B_X F_ANY_1 F_ H_A", "").unwrap(),
            "the input M_A_X\nthe input M_B_X\nthe input F_\nthe input H_A\n"
        );
    }

    #[test]
    fn a_word_may_end_in_a_newline() {
        let a = with(r#""inputs":[{"name":"P_A","scope":"cycle"}]"#);
        assert_eq!(unsupported(&a, "work", "", "P_A\n", "").unwrap(), "");
    }

    #[test]
    fn a_name_of_the_answer_is_no_pattern() {
        let a = with(
            r#""inputs":[{"name":"P_.*","scope":"cycle"},{"name":"P_A|P_B","scope":"cycle"}]"#,
        );
        assert_eq!(
            unsupported(&a, "work", "", "P_A", "").unwrap(),
            "the input P_A\n"
        );
    }

    #[test]
    fn words_split_on_spaces_and_flag_values_are_not_flags() {
        let a = with(r#""verbs":{"work":{"flags":["--json",7]}}"#);
        assert_eq!(
            unsupported(&a, "work", "  --timeout 360  --json ", "", "").unwrap(),
            "the flag --timeout\n"
        );
    }

    #[test]
    fn work_lean_is_asked_its_own_flags_and_its_result_schema() {
        let a = with(
            r#""verbs":{"work":{"flags":["--json","--timeout","--restart"]},"work-lean":{"flags":["--json","--timeout"]}},
            "features":{"workLeanResultSchema":5,"leanHandoffHost":true}"#,
        );
        assert_eq!(
            unsupported(
                &a,
                "work-lean",
                "--timeout 360 --json",
                "",
                "leanHandoffHost"
            )
            .unwrap(),
            ""
        );
        assert_eq!(
            unsupported(&a, "work-lean", "--timeout 360 --json --restart", "", "").unwrap(),
            "the flag --restart\n"
        );
        let old = with(
            r#""verbs":{"work":{"flags":["--json","--timeout"]}},"features":{"workLeanResultSchema":4}"#,
        );
        assert_eq!(
            unsupported(&old, "work-lean", "--timeout 360 --json", "", "leanDeadline").unwrap(),
            "the feature workLeanResultSchema 5\nthe flag --timeout\nthe flag --json\nthe feature leanDeadline\n"
        );
        assert_eq!(
            unsupported(&old, "work", "--timeout 360 --json", "", "").unwrap(),
            ""
        );
    }

    #[test]
    fn what_is_said_is_plain_characters() {
        let a = "{\"contracts\":{\"exitCodes\":{\"version\":\"1\\u001b[31m;x|y\"}}}";
        assert!(unsupported(a, "work", "", "", "")
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

    fn labels(list: &str, name: &str, inputs: &str) -> String {
        serde_json::json!({
            "tehas.rukas": "abc",
            "tehas.rukas.polyfills": list,
            format!("tehas.rukas.polyfill.{name}.inputs"): inputs,
        })
        .to_string()
    }

    #[test]
    fn a_polyfill_is_a_name_and_a_version() {
        assert_eq!(
            polyfills("issue-comments@0.1.0 example@1,x9@2\n"),
            ["issue-comments", "example", "x9"]
        );
        for no in [
            "",
            "<no value>",
            "a",
            "a@",
            "@1",
            "A@1",
            "a_b@1",
            "9a@1",
            "a/b@1",
        ] {
            assert!(polyfills(no).is_empty(), "{no}");
        }
    }

    #[test]
    fn a_polyfill_the_image_names_declares_its_inputs() {
        let caps = fixture();
        let l = labels(
            "issue-comments@0.1.0 example@0.1.0",
            "example",
            "PI_ENSEMBLE_EXAMPLE_URL, PI_ENSEMBLE_EXAMPLE_TOKEN\nPI_ENSEMBLE_EXAMPLE_URL",
        );
        assert_eq!(
            polyfill_inputs(&caps, &l),
            [
                ("PI_ENSEMBLE_EXAMPLE_URL".into(), "example".into()),
                ("PI_ENSEMBLE_EXAMPLE_TOKEN".into(), "example".into()),
            ]
        );
    }

    #[test]
    fn inputs_of_a_polyfill_the_image_does_not_name_are_not_declared() {
        let caps = fixture();
        for l in [
            labels("other@1", "example", "PI_ENSEMBLE_EXAMPLE_URL"),
            labels("example", "example", "PI_ENSEMBLE_EXAMPLE_URL"),
            labels("", "example", "PI_ENSEMBLE_EXAMPLE_URL"),
            r#"{"tehas.rukas.polyfill.example.inputs":"PI_ENSEMBLE_EXAMPLE_URL"}"#.into(),
            "example@1".into(),
            "null".into(),
            "".into(),
        ] {
            assert!(polyfill_inputs(&caps, &l).is_empty(), "{l}");
        }
        let l = labels("example@1", "example", "PI_ENSEMBLE_EXAMPLE_URL");
        assert!(polyfill_inputs("not json", &l).is_empty());
    }

    #[test]
    fn a_polyfill_declares_only_plain_names_the_runner_does_not_know() {
        let a = with(
            r#""inputs":[{"name":"PI_ENSEMBLE_SUBAGENT_MODE","scope":"internal"},
            {"name":"PI_ENSEMBLE_MODEL_<ROLE>","scope":"cycle"}]"#,
        );
        let l = labels(
            "pk@1",
            "pk",
            "PI_ENSEMBLE_SUBAGENT_MODE PI_ENSEMBLE_MODEL_OPS PI_ENSEMBLE_ PI_ENSEMBLE_K_<X> \
             PI_ENSEMBLE_K.* GH_TOKEN TEHAS_X pi_ensemble_k PI_ENSEMBLE_KEPT",
        );
        assert_eq!(
            polyfill_inputs(&a, &l),
            [("PI_ENSEMBLE_KEPT".into(), "pk".into())]
        );
    }
}
