//! The contract of the events (docs/specs/2026-11-22-events.md, "Schemas").
//! The samples in api/, `event-<name>.full.json` and `.bare.json`, the
//! gate's `event-<name>-before.*.json`, `gate-answer.*.json` and
//! `events.txt`, are written by the code of `tehas_ports::events`. Each is
//! checked against its schema, read back unchanged, and held to the fields
//! of its payload. The schemas say what `data` is with `$ref`, `allOf`,
//! `if`, `then` and `oneOf`, which test/schema.jq does not know: `errors`
//! here takes those apart and hands the rest to it.
//!
//! To write the samples again after a change to a payload:
//!   TEHAS_SAMPLES_WRITE=1 cargo test --locked -p console --test events_contract

mod common;

use common::{root, schema_value_errors};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Once;
use tehas_ports::events::*;

fn api(name: &str) -> PathBuf {
    root().join("api").join(name)
}

fn slug(name: Name) -> String {
    name.as_str().replace('.', "-")
}

// The text serde_json writes of a value, as the files of api/ hold it.
macro_rules! pretty {
    ($v:expr) => {
        serde_json::to_string_pretty($v).unwrap() + "\n"
    };
}

fn refusal() -> GateAnswer {
    GateAnswer::refuse(Why::new("the disk is nearly full").unwrap())
}

/// Every file of api/ the code writes, by name, with its text.
fn written() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for s in samples() {
        out.push((
            format!("event-{}.full.json", slug(s.name)),
            pretty!(&s.full),
        ));
        out.push((
            format!("event-{}.bare.json", slug(s.name)),
            pretty!(&s.bare),
        ));
    }
    for g in gate_samples() {
        out.push((
            format!("event-{}-before.full.json", slug(g.name)),
            pretty!(&g.full),
        ));
        out.push((
            format!("event-{}-before.bare.json", slug(g.name)),
            pretty!(&g.bare),
        ));
    }
    out.push(("gate-answer.full.json".into(), pretty!(&refusal())));
    out.push((
        "gate-answer.bare.json".into(),
        pretty!(&GateAnswer::allow()),
    ));
    let names: String = Name::ALL
        .iter()
        .map(|n| format!("{}\n", n.as_str()))
        .collect();
    out.push(("events.txt".into(), names));
    out
}

/// The samples are written again once per run when TEHAS_SAMPLES_WRITE asks.
fn maybe_write() {
    static WRITE: Once = Once::new();
    WRITE.call_once(|| {
        if std::env::var_os("TEHAS_SAMPLES_WRITE").is_some_and(|v| !v.is_empty()) {
            for (name, text) in written() {
                fs::write(api(&name), text).unwrap();
            }
        }
    });
}

// The helpers take JSON as text, as the files of api/ hold it, and parse it
// where they look into it.

fn load(name: &str) -> String {
    fs::read_to_string(api(name)).unwrap_or_else(|e| panic!("api/{name}: {e}"))
}

macro_rules! parse {
    ($text:expr) => {
        serde_json::from_str::<serde_json::Value>($text).unwrap()
    };
}

// Whether a schema, or one inside it, uses a keyword test/schema.jq does not
// know. A field of that name only makes the check take it apart.
fn combines(schema: &str) -> bool {
    let compact = parse!(schema).to_string();
    ["\"$ref\":", "\"allOf\":", "\"if\":", "\"oneOf\":"]
        .iter()
        .any(|k| compact.contains(k))
}

/// The ways `value` breaks `schema`: none when it fits. `$ref` names a file
/// of api/.
fn errors(schema: &str, value: &str) -> Vec<String> {
    let (schema, value) = (parse!(schema), parse!(value));
    let Value::Object(s) = &schema else {
        return match schema {
            Value::Bool(false) => vec![format!("{value}: refused")],
            _ => Vec::new(),
        };
    };
    if !combines(&schema.to_string()) {
        let out = schema_value_errors(&schema.to_string(), &value.to_string());
        return out.lines().map(String::from).collect();
    }
    let v = value.to_string();
    let mut rest = s.clone();
    let mut out = Vec::new();
    if let Some(r) = rest.remove("$ref") {
        out.extend(errors(
            &load(r.as_str().expect("a $ref is a file name")),
            &v,
        ));
    }
    if let Some(Value::Array(all)) = rest.remove("allOf") {
        for sub in &all {
            out.extend(errors(&sub.to_string(), &v));
        }
    }
    if let Some(cond) = rest.remove("if") {
        let (then, other) = (rest.remove("then"), rest.remove("else"));
        let held = errors(&cond.to_string(), &v);
        assert!(
            !held
                .iter()
                .any(|e| e.contains("does not know") || e.contains("is not checked")),
            "an if this cannot read: {held:?}"
        );
        let branch = if held.is_empty() { then } else { other };
        if let Some(b) = branch {
            out.extend(errors(&b.to_string(), &v));
        }
    }
    if let Some(Value::Array(one)) = rest.remove("oneOf") {
        let fit = one
            .iter()
            .filter(|o| errors(&o.to_string(), &v).is_empty())
            .count();
        if fit != 1 {
            out.push(format!("{v}: {fit} of oneOf fit, not 1"));
        }
    }
    if let Some(Value::Object(props)) = rest.get_mut("properties") {
        for (k, p) in props.iter_mut() {
            if combines(&p.to_string()) {
                if let Some(x) = value.get(k) {
                    let e = errors(&p.to_string(), &x.to_string());
                    out.extend(e.into_iter().map(|e| format!(".{k}: {e}")));
                }
                *p = json!({});
            }
        }
    }
    if let Some(items) = rest.get_mut("items") {
        if combines(&items.to_string()) {
            for x in value.as_array().into_iter().flatten() {
                out.extend(errors(&items.to_string(), &x.to_string()));
            }
            *items = json!({});
        }
    }
    let rest = Value::Object(rest).to_string();
    assert!(
        !combines(&rest),
        "a keyword this does not take apart: {rest}"
    );
    out.extend(errors(&rest, &v));
    out
}

macro_rules! fits {
    ($schema:expr, $value:expr) => {
        errors(&load($schema), &$value.to_string())
    };
}

#[test]
fn the_files_of_api_are_written_from_the_samples() {
    maybe_write();
    for (name, text) in written() {
        let file = fs::read_to_string(api(&name)).unwrap_or_else(|e| panic!("api/{name}: {e}"));
        assert_eq!(file, text, "api/{name} is not what the code writes");
    }
}

#[test]
fn every_sample_fits_its_schema_and_reads_back_unchanged() {
    maybe_write();
    for s in samples() {
        for side in ["full", "bare"] {
            let name = format!("event-{}.{side}.json", slug(s.name));
            let v = parse!(&load(&name));
            assert_eq!(
                fits!("event.schema.json", &v),
                Vec::<String>::new(),
                "{name}"
            );
            assert_eq!(
                fits!("event-page.schema.json", &v),
                Vec::<String>::new(),
                "{name}"
            );
            let data = v["data"].clone();
            let own = format!("event-{}.schema.json", slug(s.name));
            assert_eq!(fits!(&own, &data), Vec::<String>::new(), "{name}");
            let e: Envelope = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(e.name(), s.name);
            assert_eq!(serde_json::to_value(&e).unwrap(), v, "{name}");
        }
    }
    for g in gate_samples() {
        for side in ["full", "bare"] {
            let name = format!("event-{}-before.{side}.json", slug(g.name));
            let v = parse!(&load(&name));
            assert_eq!(
                fits!("event.schema.json", &v),
                Vec::<String>::new(),
                "{name}"
            );
            assert_ne!(
                fits!("event-page.schema.json", &v),
                Vec::<String>::new(),
                "{name}"
            );
            let a: Ask<StartAsk> = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(serde_json::to_value(&a).unwrap(), v, "{name}");
        }
    }
    for side in ["full", "bare"] {
        let name = format!("gate-answer.{side}.json");
        let v = parse!(&load(&name));
        assert_eq!(
            fits!("gate-answer.schema.json", &v),
            Vec::<String>::new(),
            "{name}"
        );
        let a: GateAnswer = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(serde_json::to_value(&a).unwrap(), v, "{name}");
    }
}

macro_rules! keys {
    ($v:expr) => {
        $v.as_object()
            .map(|m| m.keys().cloned().collect::<BTreeSet<String>>())
            .unwrap_or_default()
    };
}

macro_rules! strings {
    ($v:expr) => {
        $v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(String::from))
            .collect::<BTreeSet<String>>()
    };
}

// The properties are the fields of the full and bare samples, and the bare
// sample has exactly the required ones: `required`, and the `required` of
// the one branch of `oneOf` it takes, as a limit change takes `set` or
// `observed`.
fn holds_its_fields(schema: &str, full: &str, bare: &str) {
    let (full, bare) = (parse!(full), parse!(bare));
    let s = parse!(&load(schema));
    let all: BTreeSet<String> = keys!(full).union(&keys!(bare)).cloned().collect();
    assert_eq!(keys!(s["properties"]), all, "{schema}: properties");
    let mut required = strings!(&s["required"]);
    for branch in s["oneOf"].as_array().into_iter().flatten() {
        let r = strings!(&branch["required"]);
        if r.iter().all(|k| bare.get(k).is_some()) {
            required.extend(r);
        }
    }
    assert_eq!(
        keys!(bare),
        required,
        "{schema}: the bare sample is the required"
    );
}

#[test]
fn each_schema_holds_exactly_its_payloads_fields() {
    maybe_write();
    for s in samples() {
        let n = slug(s.name);
        let (full, bare) = (
            parse!(&load(&format!("event-{n}.full.json"))),
            parse!(&load(&format!("event-{n}.bare.json"))),
        );
        holds_its_fields(
            &format!("event-{n}.schema.json"),
            &full["data"].to_string(),
            &bare["data"].to_string(),
        );
    }
    for g in gate_samples() {
        let n = slug(g.name);
        let (full, bare) = (
            parse!(&load(&format!("event-{n}-before.full.json"))),
            parse!(&load(&format!("event-{n}-before.bare.json"))),
        );
        holds_its_fields(
            &format!("event-{n}-before.schema.json"),
            &full["data"].to_string(),
            &bare["data"].to_string(),
        );
    }
    holds_its_fields(
        "gate-answer.schema.json",
        &load("gate-answer.full.json"),
        &load("gate-answer.bare.json"),
    );
}

fn schema_files() -> Vec<String> {
    let mut out = vec![
        "event.schema.json".to_string(),
        "event-page.schema.json".into(),
        "gate-answer.schema.json".into(),
    ];
    out.extend(
        samples()
            .iter()
            .map(|s| format!("event-{}.schema.json", slug(s.name))),
    );
    out.extend(
        gate_samples()
            .iter()
            .map(|g| format!("event-{}-before.schema.json", slug(g.name))),
    );
    out
}

#[test]
fn every_schema_is_2020_12_and_names_this_test() {
    for name in schema_files() {
        let s = parse!(&load(&name));
        assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(
            s["$id"],
            format!("https://github.com/tehas-app/tehas/api/{name}")
        );
        let d = s["description"].as_str().unwrap_or_default();
        assert!(d.contains("console/tests/events_contract.rs"), "{name}");
        if s.get("$ref").is_none() {
            assert_eq!(s["additionalProperties"], false, "{name} is closed");
        }
    }
}

// The rules of the envelope's allOf: (event, when) of each `if`.
fn rules() -> Vec<(Option<String>, String)> {
    let env = parse!(&load("event.schema.json"));
    env["allOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let p = &r["if"]["properties"];
            (
                p["event"]["const"].as_str().map(String::from),
                p["when"]["const"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn the_names_are_one_list_that_every_side_reads() {
    maybe_write();
    let list: Vec<String> = fs::read_to_string(api("events.txt"))
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    let all: Vec<String> = Name::ALL.iter().map(|n| n.as_str().to_string()).collect();
    assert_eq!(list, all);
    let env = parse!(&load("event.schema.json"));
    assert_eq!(env["properties"]["event"]["enum"], json!(all));
    let gates: Vec<String> = gate_samples()
        .iter()
        .map(|g| g.name.as_str().to_string())
        .collect();
    let mut want: Vec<(Option<String>, String)> = all
        .iter()
        .map(|n| (Some(n.clone()), "after".to_string()))
        .collect();
    want.extend(
        gates
            .iter()
            .map(|n| (Some(n.clone()), "before".to_string())),
    );
    want.push((None, "before".to_string()));
    assert_eq!(rules(), want);
    let last = env["allOf"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["then"]["properties"]["event"]["enum"], json!(gates));
    for n in Name::ALL {
        for f in ["schema", "full", "bare"] {
            assert!(
                api(&format!("event-{}.{f}.json", slug(n))).is_file(),
                "{n:?} {f}"
            );
        }
    }
}

// The Events section of the manual: from its heading to the next of its
// rank.
fn manual_events() -> String {
    let page = fs::read_to_string(root().join("host/docs/api.md")).unwrap();
    let start = page
        .find("\n## Events\n")
        .expect("api.md has an Events section");
    let rest = &page[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    rest[..end].to_string()
}

#[test]
fn the_manual_lists_every_name_and_field() {
    let section = manual_events();
    let listed: Vec<&str> = section
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .filter_map(|l| l.split_once("` |").map(|(n, _)| n))
        .filter(|n| n.contains('.') && !n.contains(' '))
        .collect();
    let all: Vec<&str> = Name::ALL.iter().map(|n| n.as_str()).collect();
    assert_eq!(listed, all, "the list of the Events section");
    for name in schema_files() {
        for field in keys!(parse!(&load(&name))["properties"]) {
            assert!(
                section.contains(&format!("`{field}`")),
                "the Events section names `{field}` of {name}"
            );
        }
    }
}

macro_rules! sample_value {
    ($name:expr, $side:expr) => {
        parse!(&load(&format!("event-{}.{}.json", slug($name), $side)))
    };
}

macro_rules! refused {
    ($schema:expr, $v:expr, $why:expr $(,)?) => {
        assert_ne!(fits!($schema, $v), Vec::<String>::new(), "{}: {}", $why, $v)
    };
}

#[test]
fn the_envelope_refuses_what_is_no_event() {
    maybe_write();
    let ended = sample_value!(Name::CycleEnded, "full");
    let started = sample_value!(Name::CycleStarted, "full");

    let mut other = ended.clone();
    other["data"] = started["data"].clone();
    refused!("event.schema.json", &other, "a data of another name");

    for at in [
        "2026-11-22T13:20:00.5Z",
        "2026-11-22T13:20:00+02:00",
        "2026-11-22 13:20:00Z",
    ] {
        let mut v = ended.clone();
        v["at"] = json!(at);
        refused!("event.schema.json", &v, "a time of another shape");
        let mut v = ended.clone();
        v["data"]["ended"] = json!(at);
        refused!("event.schema.json", &v, "a payload's time of another shape");
    }

    let mut before = ended.clone();
    before["when"] = json!("before");
    refused!("event-page.schema.json", &before, "a page envelope before");
    refused!("event.schema.json", &before, "before a point with no gate");
    let mut before = started.clone();
    before["when"] = json!("before");
    refused!("event-page.schema.json", &before, "a gate on a page");
    refused!("event.schema.json", &before, "a gate with a notify payload");

    let mut v = ended.clone();
    v["key"] = json!("5");
    refused!("event.schema.json", &v, "a key of another shape");
    let mut v = ended.clone();
    v["event"] = json!("ticket.reopened");
    refused!("event.schema.json", &v, "a name not in the list");
    let mut v = ended.clone();
    v["api"] = json!(2);
    refused!("event.schema.json", &v, "another api");
    let mut v = ended.clone();
    v["data"]["signalled"] = json!(false);
    refused!("event.schema.json", &v, "a false that is left out");
    let mut v = ended.clone();
    v["data"]["status"] = Value::Null;
    refused!("event.schema.json", &v, "a null");
    let mut v = ended.clone();
    v["source"] = json!("Tehas Tehas");
    refused!("event.schema.json", &v, "a source of another shape");

    let limit = sample_value!(Name::FactoryLimitChanged, "full");
    let mut both = limit.clone();
    both["data"]["observed"] = limit["data"]["set"].clone();
    refused!("event.schema.json", &both, "a limit with set and observed");
    let mut neither = limit.clone();
    neither["data"].as_object_mut().unwrap().remove("set");
    refused!("event.schema.json", &neither, "a limit with neither");
    assert!(serde_json::from_value::<Envelope>(both).is_ok_and(|e| e.event == Event::Unknown));

    let mut ticket = sample_value!(Name::TicketClosed, "full");
    ticket["data"]["ticket"] = json!({"Number": 12, "Key": "OPS-12"});
    refused!(
        "event.schema.json",
        &ticket,
        "a ticket with a number and a key",
    );
}

#[test]
fn a_gate_answer_refuses_with_a_short_why() {
    refused!(
        "gate-answer.schema.json",
        &json!({"api": 1, "answer": "refuse"}),
        "a refusal with no why",
    );
    // The schema refuses what Rust's char::is_control does, and no more.
    for c in (0..=0x10ffffu32)
        .filter_map(char::from_u32)
        .filter(|c| c.is_control())
    {
        assert!(Why::new(format!("a{c}b")).is_err());
        assert!(matches!(c as u32, 0..=0x1f | 0x7f..=0x9f), "{c:?}");
    }
    assert!((0..=0x1fu32)
        .chain(0x7f..=0x9f)
        .all(|n| char::from_u32(n).is_some_and(char::is_control)));
    for ok in ["a\u{a0}b", "a\u{2028}b", "é"] {
        let v = json!({"api": 1, "answer": "refuse", "why": ok});
        assert_eq!(fits!("gate-answer.schema.json", v), Vec::<String>::new());
        assert!(Why::new(ok).is_ok());
    }
    for why in [
        "x".repeat(141),
        "a\nb".to_string(),
        String::new(),
        "a\u{7f}b".to_string(),
        "a\u{85}b".to_string(),
        "a\u{9f}b".to_string(),
    ] {
        refused!(
            "gate-answer.schema.json",
            &json!({"api": 1, "answer": "refuse", "why": why}),
            "a why of another shape",
        );
    }
    refused!(
        "gate-answer.schema.json",
        &json!({"api": 2, "answer": "allow"}),
        "another api"
    );
    refused!(
        "gate-answer.schema.json",
        &json!({"api": 1, "answer": "maybe"}),
        "another word",
    );
    let long = json!({"api": 1, "answer": "refuse", "why": "x".repeat(140)});
    assert_eq!(
        fits!("gate-answer.schema.json", &long),
        Vec::<String>::new()
    );
    assert!(serde_json::from_value::<GateAnswer>(long).is_ok());
}
