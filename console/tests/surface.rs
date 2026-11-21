//! The public surface of 1.x, held to api/surface-1.0.txt.
//!
//! The surface is read from the files that already pin each part:
//! api/commands.txt for commands and flags, api/settings-rules.json for
//! settings and roles, host/labels.tsv for labels and api/*.schema.json for
//! the fields of the `--json` objects. What a feature still being built adds
//! is left out (console/core/src/features.rs), unless the baseline had it
//! before the feature became building, and so are the flags
//! api/commands.txt marks internal and the fields host/docs/contract.md says
//! may change.
//!
//! Within 1.x the surface is only added to. A name of the baseline that the
//! surface no longer has fails; a name the surface has and the baseline does
//! not fails until it is appended to the baseline. A second test holds every
//! public command to an exit table in host/docs/contract.md.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use tehas_core::features::{Stage, FEATURES};

const BASELINE: &str = "api/surface-1.0.txt";

/// Fields of a building feature's `--json` output, with the feature. When the
/// feature ships its row goes and the field joins the surface.
const BUILDING_FIELDS: &[(&str, &str)] = &[("status.contractors", "outsource")];

/// Schemas of api/ that are a building feature's whole, with the feature:
/// none of their fields is on the surface until it ships.
const BUILDING_SCHEMAS: &[(&str, &str)] = &[
    ("plugin", "plugins"),
    ("plugin-manifest", "plugins"),
    ("plugin-context", "plugins"),
    ("plugin-review", "plugins"),
    ("plugin-skills", "plugins"),
    ("plugin-verdict", "plugins"),
];

/// Fields whose own fields may change: one driver's steps and the Protolab.
/// The field itself is public, what is under it is not.
const OPEN_FIELDS: &[&str] = &[
    "status.running[].progress",
    "history.runs[].progress",
    "export.runs[].progress",
    "status.prototypes",
];

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(rel)
}

fn read(rel: &str) -> String {
    fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn building(feature: &str) -> bool {
    FEATURES
        .iter()
        .any(|f| f.name == feature && f.stage == Stage::Building)
}

fn building_setting(name: &str) -> bool {
    FEATURES
        .iter()
        .filter(|f| f.stage == Stage::Building)
        .any(|f| f.setting == name || f.settings.contains(&name))
}

/// The lines of a list file, without comments and blank lines.
fn rows(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// `command <cmd> [<sub>]` and `flag <cmd> [<sub>] <flag>` of the public rows
/// of api/commands.txt.
fn commands(text: &str, out: &mut BTreeSet<String>) {
    for line in rows(text) {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.iter().any(|w| w.starts_with("building=")) {
            continue;
        }
        let internal: Vec<&str> = words
            .iter()
            .filter_map(|w| w.strip_prefix("internal="))
            .flat_map(|l| l.split(','))
            .collect();
        let name: Vec<&str> = words
            .iter()
            .take_while(|w| !w.starts_with('-') && !w.contains('='))
            .copied()
            .collect();
        let name = name.join(" ");
        out.insert(format!("command {name}"));
        for flag in words.iter().filter(|w| w.starts_with("--")) {
            if !internal.contains(flag) {
                out.insert(format!("flag {name} {flag}"));
            }
        }
    }
}

/// `setting <NAME>` and `role <ROLE>` of api/settings-rules.json.
fn settings(text: &str, out: &mut BTreeSet<String>) {
    let v: Value = serde_json::from_str(text).expect("api/settings-rules.json");
    let names = ["settings", "host"]
        .iter()
        .flat_map(|k| v[k].as_array().expect(k))
        .map(|row| row["name"].as_str().expect("name"))
        .chain(
            v["required"]
                .as_array()
                .expect("required")
                .iter()
                .map(|n| n.as_str().expect("required")),
        );
    // A name of the baseline stays when its feature became building after
    // it: the surface is only added to (review's settings, decided
    // 2026-11-19, host/docs/contract.md).
    let kept = baseline();
    for name in names.filter(|n| !building_setting(n) || kept.contains(&format!("setting {n}"))) {
        out.insert(format!("setting {name}"));
    }
    for role in v["roles"].as_array().expect("roles") {
        out.insert(format!("role {}", role.as_str().expect("role")));
    }
}

/// `label <name>` of host/labels.tsv.
fn labels(text: &str, out: &mut BTreeSet<String>) {
    for line in rows(text) {
        let name = line.split('\t').next().unwrap_or_default();
        out.insert(format!("label {name}"));
    }
}

/// `field <path>` for every property a schema names, an array's items as `[]`.
fn fields(path: &str, schema: &Value, out: &mut BTreeSet<String>) {
    if let Some(props) = schema["properties"].as_object() {
        for (key, sub) in props {
            let path = format!("{path}.{key}");
            if BUILDING_FIELDS.iter().any(|(f, _)| *f == path) {
                continue;
            }
            if let Some(feature) = path.strip_prefix("api.flags.") {
                if building(feature) {
                    continue;
                }
            }
            out.insert(format!("field {path}"));
            if !OPEN_FIELDS.contains(&path.as_str()) {
                fields(&path, sub, out);
            }
        }
    }
    if schema["items"].is_object() {
        fields(&format!("{path}[]"), &schema["items"], out);
    }
}

/// Each schema of api/, with the name of its object: `status` for
/// status.schema.json.
fn schemas() -> Vec<(String, Value)> {
    let mut files: Vec<PathBuf> = fs::read_dir(repo("api"))
        .expect("api/")
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|file| {
            let name = file.file_name()?.to_str()?;
            let object = name.strip_suffix(".schema.json")?;
            let text = fs::read_to_string(file).expect(name);
            Some((object.to_string(), serde_json::from_str(&text).expect(name)))
        })
        .collect()
}

fn surface() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    commands(&read("api/commands.txt"), &mut out);
    settings(&read("api/settings-rules.json"), &mut out);
    labels(&read("host/labels.tsv"), &mut out);
    for (object, schema) in schemas() {
        let whole = BUILDING_SCHEMAS
            .iter()
            .any(|(o, feature)| *o == object && building(feature));
        if !whole {
            fields(&object, &schema, &mut out);
        }
    }
    out
}

fn baseline() -> Vec<String> {
    rows(&read(BASELINE)).map(str::to_string).collect()
}

#[test]
fn a_building_features_settings_stay_when_the_baseline_had_them() {
    assert!(building_setting("TEHAS_REVIEW"));
    assert!(building_setting("TEHAS_REVIEW_REPO"));
    assert!(!building_setting("TEHAS_REPO"));
    let surface = surface();
    for name in ["setting TEHAS_REVIEW", "setting TEHAS_REVIEW_REPO"] {
        assert!(baseline().iter().any(|n| n == name), "{name}");
        assert!(surface.contains(name), "{name}");
    }
    // A building feature's setting the baseline never had stays out.
    assert!(!surface.contains("setting TEHAS_LEAN"));
}

#[test]
fn no_name_of_the_baseline_is_gone() {
    let surface = surface();
    let gone: Vec<String> = baseline()
        .into_iter()
        .filter(|n| !surface.contains(n))
        .collect();
    assert!(
        gone.is_empty(),
        "{BASELINE}: within 1.x a public name is never removed, renamed or made \
         internal, and a building feature's is not public. Gone:\n{}",
        gone.join("\n")
    );
}

#[test]
fn every_public_name_is_in_the_baseline() {
    let listed: BTreeSet<String> = baseline().into_iter().collect();
    let new: Vec<String> = surface()
        .into_iter()
        .filter(|n| !listed.contains(n))
        .collect();
    assert!(
        new.is_empty(),
        "{BASELINE}: append these lines at its end:\n{}",
        new.join("\n")
    );
}

#[test]
fn the_baseline_lists_each_name_once() {
    let mut seen = BTreeSet::new();
    for name in baseline() {
        assert!(seen.insert(name.clone()), "{BASELINE}: {name} twice");
        let kind = name.split(' ').next().unwrap_or_default();
        assert!(
            ["command", "flag", "setting", "role", "label", "field"].contains(&kind),
            "{BASELINE}: {name:?} is of no kind"
        );
    }
}

#[test]
fn each_building_field_is_of_a_building_feature() {
    for (field, feature) in BUILDING_FIELDS {
        assert!(
            building(feature),
            "{field}: {feature} is not building; take the row out of BUILDING_FIELDS"
        );
    }
    for (object, feature) in BUILDING_SCHEMAS {
        assert!(
            building(feature),
            "{object}: {feature} is not building; take the row out of BUILDING_SCHEMAS"
        );
        assert!(
            repo(&format!("api/{object}.schema.json")).is_file(),
            "no api/{object}.schema.json"
        );
    }
}

#[test]
fn the_open_and_building_fields_are_in_their_schemas() {
    let mut all = BTreeSet::new();
    for (object, schema) in schemas() {
        every_field(&object, &schema, &mut all);
    }
    for field in OPEN_FIELDS
        .iter()
        .chain(BUILDING_FIELDS.iter().map(|(f, _)| f))
    {
        assert!(all.contains(*field), "{field} is in no schema of api/");
    }
}

/// Every property path of a schema, with nothing left out.
fn every_field(path: &str, schema: &Value, out: &mut BTreeSet<String>) {
    if let Some(props) = schema["properties"].as_object() {
        for (key, sub) in props {
            let path = format!("{path}.{key}");
            every_field(&path, sub, out);
            out.insert(path);
        }
    }
    if schema["items"].is_object() {
        every_field(&format!("{path}[]"), &schema["items"], out);
    }
}

/// The commands a `### ` heading of the exit statuses of contract.md names:
/// `tehas freeze and tehas thaw` names freeze and thaw.
fn exit_tables(contract: &str) -> BTreeSet<String> {
    let section = contract
        .split_once("\n## Exit statuses\n")
        .map(|(_, s)| s)
        .expect("host/docs/contract.md has no Exit statuses section");
    let section = section.split("\n## ").next().unwrap_or(section);
    let mut named = BTreeSet::new();
    for heading in section.lines().filter_map(|l| l.strip_prefix("### ")) {
        for part in heading.split(" and ") {
            let mut words = part.split_whitespace();
            if words.next() == Some("tehas") {
                if let Some(command) = words.next() {
                    named.insert(command.to_string());
                }
            }
        }
    }
    named
}

#[test]
fn each_public_command_has_an_exit_table() {
    let tables = exit_tables(&read("host/docs/contract.md"));
    let mut surface = BTreeSet::new();
    commands(&read("api/commands.txt"), &mut surface);
    let missing: Vec<&str> = surface
        .iter()
        .filter_map(|n| n.strip_prefix("command "))
        .filter(|c| !c.contains(' '))
        .filter(|c| !tables.contains(*c))
        .collect();
    assert!(
        missing.is_empty(),
        "host/docs/contract.md: no exit table under Exit statuses for tehas {}",
        missing.join(", tehas ")
    );
}

#[test]
fn a_heading_names_each_command_it_joins() {
    let doc = "# x\n\n## Exit statuses\n\n### tehas freeze and tehas thaw\n\n\
               ### tehas github check and tehas github writes\n\n## After\n\n### tehas nope\n";
    let names: Vec<String> = exit_tables(doc).into_iter().collect();
    assert_eq!(names, ["freeze", "github", "thaw"]);
}

#[test]
fn the_reads_leave_out_what_is_not_public() {
    let mut out = BTreeSet::new();
    commands(
        "# c\nplan --kind --out internal=--kind\noutsource list --json building=outsource\n",
        &mut out,
    );
    let got: Vec<&str> = out.iter().map(String::as_str).collect();
    assert_eq!(got, ["command plan", "flag plan --out"]);

    let mut out = BTreeSet::new();
    let schema: Value = serde_json::from_str(
        r#"{"properties": {
            "contractors": {"type": "array"},
            "running": {"items": {"properties": {"progress": {"properties": {"step": {}}}}}},
            "prototypes": {"properties": {"x": {}}}
        }}"#,
    )
    .unwrap();
    fields("status", &schema, &mut out);
    let got: Vec<&str> = out.iter().map(String::as_str).collect();
    assert_eq!(
        got,
        [
            "field status.prototypes",
            "field status.running",
            "field status.running[].progress",
        ]
    );

    let mut out = BTreeSet::new();
    settings(
        r#"{"required": ["TEHAS_REPO"], "roles": ["OPS"],
            "settings": [{"name": "TEHAS_OUTSOURCE_REPOS"}, {"name": "TEHAS_LEAN"}],
            "host": [{"name": "TEHAS_HOME"}]}"#,
        &mut out,
    );
    let got: Vec<&str> = out.iter().map(String::as_str).collect();
    assert_eq!(
        got,
        ["role OPS", "setting TEHAS_HOME", "setting TEHAS_REPO"]
    );
}
