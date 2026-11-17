//! Whether an answer is what its tool's outputSchema says. The schemas of this
//! crate use a small part of JSON Schema, and this checks that part: `type`,
//! `enum`, `properties`, `required`, `additionalProperties`, `propertyNames`,
//! `items`, `minimum` and `maximum`. A keyword it does not know, such as
//! `pattern` or `description`, checks nothing.

use serde_json::{Map, Value};

/// Ok when `v` matches `schema`, else the path of the first value that does
/// not and why, as `steps[0].round: a string, not integer`.
pub fn check(v: &Value, schema: &Value) -> Result<(), String> {
    at(v, schema, "")
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn named(path: &str) -> &str {
    if path.is_empty() {
        "the answer"
    } else {
        path
    }
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(n) if n.is_f64() => "a number",
        Value::Number(_) => "an integer",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn is(v: &Value, ty: &str) -> bool {
    match ty {
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "object" => v.is_object(),
        "array" => v.is_array(),
        "null" => v.is_null(),
        _ => true,
    }
}

fn at(v: &Value, s: &Value, path: &str) -> Result<(), String> {
    let Some(s) = s.as_object() else {
        return Ok(());
    };
    let p = named(path);
    let types: Vec<&str> = match s.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if !types.is_empty() && !types.iter().any(|t| is(v, t)) {
        return Err(format!("{p}: {}, not {}", type_of(v), types.join(" or ")));
    }
    if let Some(Value::Array(words)) = s.get("enum") {
        if !words.contains(v) {
            return Err(format!("{p}: {v} is not one of its words"));
        }
    }
    if let Some(n) = v.as_f64() {
        if s.get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{p}: {v} is under its minimum"));
        }
        if s.get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{p}: {v} is over its maximum"));
        }
    }
    match v {
        Value::Object(m) => object(m, s, path),
        Value::Array(a) => match s.get("items") {
            Some(item) => a
                .iter()
                .enumerate()
                .try_for_each(|(i, x)| at(x, item, &format!("{path}[{i}]"))),
            None => Ok(()),
        },
        _ => Ok(()),
    }
}

fn object(m: &Map<String, Value>, s: &Map<String, Value>, path: &str) -> Result<(), String> {
    if let Some(Value::Array(req)) = s.get("required") {
        if let Some(k) = req
            .iter()
            .filter_map(Value::as_str)
            .find(|k| !m.contains_key(*k))
        {
            return Err(format!("{}: {k} is missing", named(path)));
        }
    }
    let props = s.get("properties").and_then(Value::as_object);
    for (k, x) in m {
        let key = join(path, k);
        if let Some(names) = s.get("propertyNames") {
            at(&Value::String(k.clone()), names, &key)?;
        }
        match props.and_then(|p| p.get(k)) {
            Some(sub) => at(x, sub, &key)?,
            None => match s.get("additionalProperties") {
                Some(Value::Bool(false)) => return Err(format!("{key}: not in the schema")),
                Some(sub @ Value::Object(_)) => at(x, sub, &key)?,
                _ => {}
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_value_of_its_type_and_word_passes() {
        let s = json!({"type": "object", "required": ["api"], "properties": {
            "api": {"type": "integer", "minimum": 1, "maximum": 1},
            "word": {"type": "string", "enum": ["a", "b"]},
            "n": {"type": "number"},
            "list": {"type": "array", "items": {"type": "object", "properties": {
                "x": {"type": "boolean"}}}},
            "counts": {"type": "object", "propertyNames": {"enum": ["a", "b"]},
                       "additionalProperties": {"type": "integer"}},
        }});
        let v = json!({"api": 1, "word": "a", "n": 1, "list": [{"x": true}],
                       "counts": {"a": 2}, "other": "kept"});
        assert_eq!(check(&v, &s), Ok(()));
        assert_eq!(check(&json!({"api": 1, "n": 0.5}), &s), Ok(()));
    }

    #[test]
    fn the_first_value_that_does_not_match_is_named() {
        let s = json!({"type": "object", "required": ["api"], "properties": {
            "api": {"type": "integer", "minimum": 1, "maximum": 1},
            "word": {"type": "string", "enum": ["a", "b"]},
            "list": {"type": "array", "items": {"type": "object", "properties": {
                "x": {"type": "boolean"}}}},
            "counts": {"type": "object", "propertyNames": {"enum": ["a", "b"]},
                       "additionalProperties": {"type": "integer"}},
            "closed": {"type": "object", "additionalProperties": false},
        }});
        for (v, want) in [
            (json!({}), "the answer: api is missing"),
            (json!([]), "the answer: an array, not object"),
            (json!({"api": "1"}), "api: a string, not integer"),
            (json!({"api": 1.5}), "api: a number, not integer"),
            (json!({"api": 2}), "api: 2 is over its maximum"),
            (json!({"api": 0}), "api: 0 is under its minimum"),
            (
                json!({"api": 1, "word": "c"}),
                "word: \"c\" is not one of its words",
            ),
            (
                json!({"api": 1, "word": {"a": 1}}),
                "word: an object, not string",
            ),
            (
                json!({"api": 1, "list": [{"x": true}, {"x": 1}]}),
                "list[1].x: an integer, not boolean",
            ),
            (
                json!({"api": 1, "counts": {"c": 1}}),
                "counts.c: \"c\" is not one of its words",
            ),
            (
                json!({"api": 1, "counts": {"a": "1"}}),
                "counts.a: a string, not integer",
            ),
            (
                json!({"api": 1, "closed": {"z": 1}}),
                "closed.z: not in the schema",
            ),
        ] {
            assert_eq!(check(&v, &s), Err(want.to_string()), "{v}");
        }
    }
}
