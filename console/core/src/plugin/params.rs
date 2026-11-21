//! A tool's `params`: the closed part of JSON schema a tool may use
//! (docs/specs/2026-11-21-plugins-v2-chats.md, "Tools"). [`rules`] reads a
//! schema with the manifest, and [`check`] holds a call's arguments to it
//! with exactly the keywords [`rules`] lets through. No `pattern`: a call
//! runs no regular expression of a plugin's.

use serde_json::{Map, Value};

/// The largest schema, serialized.
pub const SCHEMA_MAX: usize = 4096;
/// The largest arguments of a call, serialized.
pub const ARGS_MAX: usize = 2048;
/// The deepest a schema nests, the top level being 1.
pub const DEPTH_MAX: usize = 6;
const KEYWORDS: [&str; 15] = [
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "maxItems",
    "minItems",
    "maxLength",
    "minLength",
    "enum",
    "const",
    "minimum",
    "maximum",
    "title",
    "description",
];
const TYPES: [&str; 7] = [
    "object", "string", "integer", "number", "boolean", "array", "null",
];

/// The types a schema admits: all of them when it names none. Err: `type`
/// is not one of [`TYPES`] or a list of them, each once.
fn types(s: &Map<String, Value>) -> Result<Vec<&str>, ()> {
    let list: Vec<&Value> = match s.get("type") {
        None => return Ok(TYPES.to_vec()),
        Some(Value::Array(l)) if !l.is_empty() => l.iter().collect(),
        Some(t) => vec![t],
    };
    let mut got: Vec<&str> = Vec::new();
    for t in list {
        match t.as_str() {
            Some(t) if TYPES.contains(&t) && !got.contains(&t) => got.push(t),
            _ => return Err(()),
        }
    }
    Ok(got)
}

fn below(at: &str, key: &str) -> String {
    if at.is_empty() {
        key.to_string()
    } else {
        format!("{at}.{key}")
    }
}

/// Why `schema` is refused as a tool's `params`, each reason once; empty
/// when it is taken.
pub fn rules(schema: &Value) -> Vec<String> {
    if schema.to_string().len() > SCHEMA_MAX {
        return vec![format!("over {SCHEMA_MAX} bytes")];
    }
    let mut out = Vec::new();
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        out.push("the top level is not type object".to_string());
    }
    walk(schema, "", 1, &mut out);
    out
}

fn walk(s: &Value, at: &str, depth: usize, out: &mut Vec<String>) {
    let here = if at.is_empty() { "the top level" } else { at };
    if depth > DEPTH_MAX {
        out.push(format!("{here}: nested deeper than {DEPTH_MAX}"));
        return;
    }
    let Some(m) = s.as_object() else {
        out.push(format!("{here}: expected a schema, an object"));
        return;
    };
    for k in m.keys().filter(|k| !KEYWORDS.contains(&k.as_str())) {
        out.push(format!("{here}: {k} is not allowed"));
    }
    let Ok(admits) = types(m) else {
        out.push(format!(
            "{here}.type: expected one of {}, or a list of them",
            TYPES.join(", ")
        ));
        return;
    };
    let closed = m.contains_key("enum") || m.contains_key("const");
    if admits.contains(&"string") && !closed && !m.contains_key("maxLength") {
        out.push(format!("{here}: a string needs maxLength"));
    }
    if admits.contains(&"array") && !closed && !m.contains_key("maxItems") {
        out.push(format!("{here}: an array needs maxItems"));
    }
    for k in ["maxItems", "minItems", "maxLength", "minLength"] {
        if m.get(k).is_some_and(|v| v.as_u64().is_none()) {
            out.push(format!("{here}.{k}: expected a whole number"));
        }
    }
    for k in ["minimum", "maximum"] {
        if m.get(k).is_some_and(|v| !v.is_number()) {
            out.push(format!("{here}.{k}: expected a number"));
        }
    }
    for k in ["title", "description"] {
        if m.get(k).is_some_and(|v| !v.is_string()) {
            out.push(format!("{here}.{k}: expected a string"));
        }
    }
    if m.get("enum").is_some_and(|v| !v.is_array()) {
        out.push(format!("{here}.enum: expected a list"));
    }
    if m.get("additionalProperties")
        .is_some_and(|v| !v.is_boolean())
    {
        out.push(format!(
            "{here}.additionalProperties: expected true or false"
        ));
    }
    if let Some(r) = m.get("required") {
        let names: Option<Vec<&str>> = r
            .as_array()
            .and_then(|l| l.iter().map(Value::as_str).collect());
        let once = names
            .as_ref()
            .is_some_and(|n| n.iter().enumerate().all(|(i, k)| !n[..i].contains(k)));
        if !once {
            out.push(format!("{here}.required: expected names, each once"));
        }
    }
    match m.get("properties") {
        None => {}
        Some(Value::Object(p)) => {
            for (k, sub) in p {
                walk(sub, &below(at, &format!("properties.{k}")), depth + 1, out);
            }
        }
        Some(_) => out.push(format!("{here}.properties: expected an object")),
    }
    match m.get("items") {
        None => {}
        Some(sub @ Value::Object(_)) => walk(sub, &below(at, "items"), depth + 1, out),
        Some(_) => out.push(format!("{here}.items: expected a single schema")),
    }
}

/// Whether `args` fit `schema`, a schema [`rules`] took: Err is the first
/// reason they do not, naming where.
pub fn check(args: &Value, schema: &Value) -> Result<(), String> {
    if args.to_string().len() > ARGS_MAX {
        return Err(format!("the arguments are over {ARGS_MAX} bytes"));
    }
    fits(args, schema, "")
}

fn is_type(v: &Value, t: &str) -> bool {
    match t {
        "object" => v.is_object(),
        "string" => v.is_string(),
        "integer" => v.as_f64().is_some_and(|n| n.fract() == 0.0),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "array" => v.is_array(),
        "null" => v.is_null(),
        _ => false,
    }
}

fn fits(v: &Value, s: &Value, path: &str) -> Result<(), String> {
    let at = if path.is_empty() {
        "the arguments"
    } else {
        path
    };
    let Some(s) = s.as_object() else {
        return Err(format!("{at}: the schema is not an object"));
    };
    let admits =
        types(s).map_err(|()| format!("{at}: the schema's type is not one tehas reads"))?;
    if !admits.iter().any(|t| is_type(v, t)) {
        return Err(format!("{at}: expected {}", admits.join(" or ")));
    }
    if s.get("const").is_some_and(|c| c != v) {
        return Err(format!("{at}: expected {}", s["const"]));
    }
    if let Some(Value::Array(words)) = s.get("enum") {
        if !words.contains(v) {
            return Err(format!("{at}: expected one of the schema's enum"));
        }
    }
    if let Some(n) = v.as_f64() {
        if s.get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{at}: under {}", s["minimum"]));
        }
        if s.get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{at}: over {}", s["maximum"]));
        }
    }
    if let Some(text) = v.as_str() {
        let len = text.chars().count() as u64;
        if s.get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| len < m)
        {
            return Err(format!("{at}: under {} characters", s["minLength"]));
        }
        if s.get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| len > m)
        {
            return Err(format!("{at}: over {} characters", s["maxLength"]));
        }
    }
    if let Some(list) = v.as_array() {
        let n = list.len() as u64;
        if s.get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{at}: under {} items", s["minItems"]));
        }
        if s.get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{at}: over {} items", s["maxItems"]));
        }
        if let Some(item) = s.get("items") {
            for (i, x) in list.iter().enumerate() {
                fits(x, item, &format!("{path}[{i}]"))?;
            }
        }
    }
    if let Some(m) = v.as_object() {
        let props = s.get("properties").and_then(Value::as_object);
        if let Some(Value::Array(req)) = s.get("required") {
            if let Some(k) = req
                .iter()
                .filter_map(Value::as_str)
                .find(|k| !m.contains_key(*k))
            {
                return Err(format!("{at}: {k} is missing"));
            }
        }
        let open = s.get("additionalProperties") != Some(&Value::Bool(false));
        for (k, x) in m {
            match props.and_then(|p| p.get(k)) {
                Some(sub) => fits(x, sub, &below(path, k))?,
                None if !open => return Err(format!("{at}: {k} is no parameter")),
                None => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The spec's tool: one string up to 200 characters, and nothing else.
    fn find_doc() -> Value {
        json!({"type": "object", "properties": {"q": {"type": "string", "maxLength": 200}}, "required": ["q"], "additionalProperties": false})
    }

    fn every_keyword() -> Value {
        json!({
            "type": "object",
            "title": "Search",
            "description": "What to look for",
            "properties": {
                "q": {"type": "string", "minLength": 1, "maxLength": 20},
                "tags": {"type": "array", "minItems": 1, "maxItems": 3, "items": {"enum": ["api", "ops"]}},
                "limit": {"type": "integer", "minimum": 1, "maximum": 10},
                "score": {"type": ["number", "null"]},
                "exact": {"type": "boolean"},
                "v": {"const": 1}
            },
            "required": ["q"],
            "additionalProperties": false
        })
    }

    #[test]
    fn the_spec_tool_and_every_keyword_are_taken() {
        assert_eq!(rules(&find_doc()), Vec::<String>::new());
        assert_eq!(rules(&every_keyword()), Vec::<String>::new());
    }

    #[test]
    fn a_keyword_outside_the_subset_is_refused() {
        for k in [
            "$ref",
            "oneOf",
            "anyOf",
            "allOf",
            "not",
            "format",
            "$defs",
            "if",
            "patternProperties",
        ] {
            let mut s = find_doc();
            s[k] = json!({});
            assert_eq!(
                rules(&s),
                [format!("the top level: {k} is not allowed")],
                "{k}"
            );
        }
        let mut s = find_doc();
        s["properties"]["q"]["format"] = json!("email");
        assert_eq!(rules(&s), ["properties.q: format is not allowed"]);
    }

    #[test]
    fn a_top_level_that_is_no_object_is_refused() {
        for s in [
            json!({"type": "string", "maxLength": 5}),
            json!({"type": ["object", "null"]}),
            json!({"properties": {}}),
        ] {
            assert!(
                rules(&s).contains(&"the top level is not type object".to_string()),
                "{s}"
            );
        }
        assert_eq!(
            rules(&json!([])),
            [
                "the top level is not type object",
                "the top level: expected a schema, an object"
            ]
        );
    }

    #[test]
    fn a_schema_nested_deeper_than_six_is_refused() {
        let nest = |levels: usize| {
            let mut s = json!({"type": "object"});
            for _ in 1..levels {
                s = json!({"type": "object", "properties": {"a": s}});
            }
            s
        };
        assert_eq!(rules(&nest(DEPTH_MAX)), Vec::<String>::new());
        assert_eq!(
            rules(&nest(DEPTH_MAX + 1)),
            ["properties.a.properties.a.properties.a.properties.a.properties.a.properties.a: nested deeper than 6"]
        );
        let mut items = json!({"type": "object"});
        for _ in 0..DEPTH_MAX {
            items = json!({"type": "array", "maxItems": 1, "items": items});
        }
        let s = json!({"type": "object", "properties": {"a": items}});
        assert_eq!(rules(&s).len(), 1);
    }

    #[test]
    fn a_schema_over_4_kib_is_refused() {
        let mut s = find_doc();
        s["description"] = json!("x".repeat(SCHEMA_MAX));
        assert_eq!(rules(&s), ["over 4096 bytes"]);
    }

    #[test]
    fn a_string_needs_max_length_and_an_array_max_items() {
        let with = |q: Value| json!({"type": "object", "properties": {"q": q}});
        assert_eq!(
            rules(&with(json!({"type": "string"}))),
            ["properties.q: a string needs maxLength"]
        );
        assert_eq!(
            rules(&with(
                json!({"type": "array", "items": {"type": "integer"}})
            )),
            ["properties.q: an array needs maxItems"]
        );
        assert_eq!(
            rules(&with(json!({}))),
            [
                "properties.q: a string needs maxLength",
                "properties.q: an array needs maxItems"
            ]
        );
        assert_eq!(
            rules(&with(json!({"enum": ["a", "b"]}))),
            Vec::<String>::new()
        );
        assert_eq!(
            rules(&with(
                json!({"type": "array", "maxItems": 2, "items": {"type": "string"}})
            )),
            ["properties.q.items: a string needs maxLength"]
        );
    }

    #[test]
    fn a_pattern_is_refused_when_the_manifest_is_read() {
        let s = json!({"type": "object", "properties": {"q": {"type": "string", "maxLength": 9, "pattern": "^a"}}});
        assert_eq!(rules(&s), ["properties.q: pattern is not allowed"]);
    }

    #[test]
    fn a_keyword_of_the_wrong_shape_is_refused() {
        let cases = [
            (json!({"type": "date"}), "properties.a.type: expected one of object, string, integer, number, boolean, array, null, or a list of them"),
            (json!({"type": []}), "properties.a.type: expected one of object, string, integer, number, boolean, array, null, or a list of them"),
            (json!({"type": ["null", "null"]}), "properties.a.type: expected one of object, string, integer, number, boolean, array, null, or a list of them"),
            (json!({"type": "object", "additionalProperties": {}}), "properties.a.additionalProperties: expected true or false"),
            (json!({"type": "array", "maxItems": 2, "items": [{"type": "null"}]}), "properties.a.items: expected a single schema"),
            (json!({"type": "array", "maxItems": -1}), "properties.a.maxItems: expected a whole number"),
            (json!({"type": "integer", "minimum": "1"}), "properties.a.minimum: expected a number"),
            (json!({"type": "object", "required": ["x", "x"]}), "properties.a.required: expected names, each once"),
            (json!({"type": "object", "properties": []}), "properties.a.properties: expected an object"),
            (json!({"type": "null", "title": 1}), "properties.a.title: expected a string"),
            (json!({"type": "null", "enum": "x"}), "properties.a.enum: expected a list"),
            (json!(true), "properties.a: expected a schema, an object"),
        ];
        for (a, why) in cases {
            let s = json!({"type": "object", "properties": {"a": a}});
            assert_eq!(rules(&s), [why], "{a}");
        }
    }

    #[test]
    fn arguments_that_fit_pass() {
        let s = every_keyword();
        for args in [
            json!({"q": "plugins"}),
            json!({"q": "a b", "tags": ["api", "ops"], "limit": 10, "score": null, "exact": true, "v": 1}),
            json!({"q": "x", "limit": 1.0, "score": 0.5}),
        ] {
            assert_eq!(check(&args, &s), Ok(()), "{args}");
        }
        assert_eq!(check(&json!({"q": "x".repeat(200)}), &find_doc()), Ok(()));
    }

    #[test]
    fn arguments_that_do_not_fit_say_where() {
        let s = every_keyword();
        let cases = [
            (json!([]), "the arguments: expected object"),
            (json!({}), "the arguments: q is missing"),
            (
                json!({"q": "x", "other": 1}),
                "the arguments: other is no parameter",
            ),
            (json!({"q": 1}), "q: expected string"),
            (json!({"q": ""}), "q: under 1 characters"),
            (json!({"q": "x".repeat(21)}), "q: over 20 characters"),
            (json!({"q": "x", "tags": []}), "tags: under 1 items"),
            (
                json!({"q": "x", "tags": ["api", "api", "ops", "ops"]}),
                "tags: over 3 items",
            ),
            (
                json!({"q": "x", "tags": ["dev"]}),
                "tags[0]: expected one of the schema's enum",
            ),
            (json!({"q": "x", "limit": 0}), "limit: under 1"),
            (json!({"q": "x", "limit": 11}), "limit: over 10"),
            (json!({"q": "x", "limit": 1.5}), "limit: expected integer"),
            (
                json!({"q": "x", "score": "1"}),
                "score: expected number or null",
            ),
            (json!({"q": "x", "exact": "yes"}), "exact: expected boolean"),
            (json!({"q": "x", "v": 2}), "v: expected 1"),
        ];
        for (args, why) in cases {
            assert_eq!(check(&args, &s), Err(why.to_string()), "{args}");
        }
    }

    #[test]
    fn arguments_over_2_kib_are_refused_before_the_schema() {
        let open = json!({"type": "object"});
        let pad = |n: usize| json!({"q": "x".repeat(n)});
        let fits = ARGS_MAX - pad(0).to_string().len();
        assert_eq!(check(&pad(fits), &open), Ok(()));
        assert_eq!(
            check(&pad(fits + 1), &open),
            Err("the arguments are over 2048 bytes".to_string())
        );
    }
}
