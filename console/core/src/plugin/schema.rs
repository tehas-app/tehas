//! The schemas of api/ that plugins add, checked with the keywords they use:
//! the tests hold the manifest's reader and `tehas plugin`'s samples to them.

use serde_json::{json, Value};

/// Whether `v` fits `s`. A keyword this does not know fails the test, so
/// none is passed over.
pub fn fits(v: &Value, s: &Value) -> bool {
    let s = s.as_object().unwrap();
    let known = [
        "$schema",
        "$id",
        "title",
        "description",
        "type",
        "const",
        "enum",
        "required",
        "additionalProperties",
        "properties",
        "items",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "pattern",
        "maxItems",
        "uniqueItems",
        "contains",
        "dependentSchemas",
    ];
    for key in s.keys() {
        assert!(known.contains(&key.as_str()), "the schema uses {key}");
    }
    let ty = |t: &Value| match t.as_str().unwrap() {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.is_u64() || v.is_i64(),
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        other => panic!("type {other}"),
    };
    let types = match s.get("type") {
        None => vec![],
        Some(Value::Array(list)) => list.iter().collect(),
        Some(t) => vec![t],
    };
    if !types.is_empty() && !types.into_iter().any(ty) {
        return false;
    }
    if s.get("const").is_some_and(|c| c != v) {
        return false;
    }
    if s.get("enum")
        .and_then(Value::as_array)
        .is_some_and(|words| !words.contains(v))
    {
        return false;
    }
    if let Some(n) = v.as_f64() {
        if s.get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n < m)
            || s.get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|m| n > m)
        {
            return false;
        }
    }
    if let Some(text) = v.as_str() {
        let len = text.chars().count() as u64;
        if s.get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| len < m)
            || s.get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|m| len > m)
        {
            return false;
        }
        if let Some(p) = s.get("pattern").and_then(Value::as_str) {
            if !regex::Regex::new(p).unwrap().is_match(text) {
                return false;
            }
        }
    }
    if let Some(list) = v.as_array() {
        if s.get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|m| list.len() as u64 > m)
        {
            return false;
        }
        if s.get("uniqueItems") == Some(&json!(true))
            && list.iter().enumerate().any(|(i, x)| list[..i].contains(x))
        {
            return false;
        }
        if let Some(item) = s.get("items") {
            if !list.iter().all(|x| fits(x, item)) {
                return false;
            }
        }
        if let Some(c) = s.get("contains") {
            if !list.iter().any(|x| fits(x, c)) {
                return false;
            }
        }
    }
    if let Some(m) = v.as_object() {
        let props = s.get("properties").and_then(Value::as_object);
        if let Some(req) = s.get("required").and_then(Value::as_array) {
            if !req.iter().all(|k| m.contains_key(k.as_str().unwrap())) {
                return false;
            }
        }
        for (k, x) in m {
            match props.and_then(|p| p.get(k)) {
                Some(sub) if !fits(x, sub) => return false,
                Some(_) => {}
                None if s.get("additionalProperties") == Some(&json!(false)) => return false,
                None => {}
            }
        }
        if let Some(deps) = s.get("dependentSchemas").and_then(Value::as_object) {
            for (k, sub) in deps {
                if m.contains_key(k) && !fits(v, sub) {
                    return false;
                }
            }
        }
    }
    true
}

/// A file of api/, read as JSON.
pub fn api(name: &str) -> Value {
    let file = format!("{}/../../api/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap()
}

/// Every object of a schema is closed.
pub fn closed(s: &Value) {
    if s["type"] == "object" {
        assert_eq!(s["additionalProperties"], json!(false), "{s}");
    }
    for key in ["properties", "dependentSchemas"] {
        if let Some(p) = s[key].as_object() {
            p.values().for_each(closed);
        }
    }
    if s["items"].is_object() {
        closed(&s["items"]);
    }
}

#[test]
fn the_samples_of_tehas_plugin_fit_its_schema() {
    let s = api("plugin.schema.json");
    assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(
        s["$id"],
        "https://github.com/tehas-app/tehas/api/plugin.schema.json"
    );
    closed(&s);
    for name in ["sample", "empty", "refused", "ready"] {
        assert!(fits(&api(&format!("plugin.{name}.json")), &s), "{name}");
    }
    let mut built = api("plugin.ready.json");
    built["plugins"][0]["state"] = json!("built");
    assert!(!fits(&built, &s));
    let mut more = api("plugin.sample.json");
    more["plugins"][0]["label"] = json!("Floorplan");
    assert!(!fits(&more, &s));
    let mut v2 = api("plugin.sample.json");
    v2["plugins"][0]["grants"][0] = json!("gate:verify");
    assert!(!fits(&v2, &s));
}

#[test]
fn the_samples_of_tehas_plugin_review_fit_its_schema() {
    let s = api("plugin-review.schema.json");
    assert_eq!(
        s["$id"],
        "https://github.com/tehas-app/tehas/api/plugin-review.schema.json"
    );
    closed(&s);
    for name in ["sample", "refused"] {
        assert!(
            fits(&api(&format!("plugin-review.{name}.json")), &s),
            "{name}"
        );
    }
    let mut more = api("plugin-review.sample.json");
    more["asks"][0]["value"] = json!("x");
    assert!(!fits(&more, &s));
    let mut yes = api("plugin-review.sample.json");
    yes["asks"][0]["granted"] = json!("yes");
    assert!(!fits(&yes, &s));
}

#[test]
fn a_keyword_the_check_does_not_know_fails() {
    let got = std::panic::catch_unwind(|| fits(&json!(1), &json!({"oneOf": []})));
    assert!(got.is_err());
}
