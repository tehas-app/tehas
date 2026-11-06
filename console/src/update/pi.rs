//! Pi's settings and model list, made from the instance for install.sh.
//!
//!   console update pi <instance> <generic settings> <provider> <model> <out>
//!
//! Writes <out>/settings.json and <out>/models.json and exits 0.
//!
//! settings.json is the instance's own when it has one it can read, else the
//! generic one, with `defaultProvider` and `defaultModel` set to <provider> and
//! <model> where it leaves them out: the session model is named once, by
//! PI_ENSEMBLE_SUBAGENT_PROVIDER and PI_ENSEMBLE_SUBAGENT_MODEL.
//!
//! models.json is the instance's, with the keys Pi wants and the instance may
//! leave out given today's defaults: each provider's `compat`, each model's
//! `name` (its id), `input`, `reasoning` and `cost`. A key the instance gives is
//! kept. As with jq's `//=`, a key set to null or false counts as left out.
//!
//! Exit: 78 a file is not JSON or not shaped as Pi reads it (the line names
//! it), 1 a file could not be written, 64 usage.

use std::fs;
use std::path::Path;

use serde_json::{json, Map, Value};

use super::tell;

pub fn verb(given: &[String]) -> u8 {
    let [instance, generic, provider, model, out] = given else {
        tell("console update pi: usage: <instance> <generic settings> <provider> <model> <out>");
        return 64;
    };
    let own = Path::new(instance).join("settings.json");
    let (from, text) = match fs::read_to_string(&own) {
        Ok(text) => (own, Ok(text)),
        Err(_) => (
            Path::new(generic).to_path_buf(),
            fs::read_to_string(generic),
        ),
    };
    let settings = text
        .map_err(|_| "cannot be read".to_string())
        .and_then(|t| settings(&t, provider, model));
    let models_from = Path::new(instance).join("models.json");
    let models = fs::read_to_string(&models_from)
        .map_err(|_| "cannot be read".to_string())
        .and_then(|t| models(&t));
    for (file, made) in [(&from, &settings), (&models_from, &models)] {
        if let Err(why) = made {
            tell(&format!(
                "tehas: {} {why}; nothing was installed",
                file.display()
            ));
            return 78;
        }
    }
    for (name, made) in [("settings.json", settings), ("models.json", models)] {
        let path = Path::new(out).join(name);
        if fs::write(&path, made.unwrap_or_default()).is_err() {
            tell(&format!("tehas: could not write {}", path.display()));
            return 1;
        }
    }
    0
}

fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|_| "is not valid JSON".to_string())
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

/// jq's `.key //= value` on an object.
fn default(object: &mut Map<String, Value>, key: &str, value: Value) {
    if matches!(
        object.get(key),
        None | Some(Value::Null) | Some(Value::Bool(false))
    ) {
        object.insert(key.to_string(), value);
    }
}

fn settings(text: &str, provider: &str, model: &str) -> Result<String, String> {
    let mut value = parse(text)?;
    let object = value
        .as_object_mut()
        .ok_or("is not a JSON object".to_string())?;
    default(object, "defaultProvider", json!(provider));
    default(object, "defaultModel", json!(model));
    Ok(pretty(&value))
}

/// The members of an array, or the values of an object.
fn each(value: &mut Value) -> Option<Vec<&mut Value>> {
    match value {
        Value::Array(items) => Some(items.iter_mut().collect()),
        Value::Object(items) => Some(items.values_mut().collect()),
        _ => None,
    }
}

fn models(text: &str) -> Result<String, String> {
    let mut value = parse(text)?;
    let providers = value
        .as_object_mut()
        .and_then(|o| o.get_mut("providers"))
        .and_then(each)
        .ok_or("has no providers to list".to_string())?;
    for provider in providers {
        let provider = provider
            .as_object_mut()
            .ok_or("has a provider that is not a JSON object".to_string())?;
        default(
            provider,
            "compat",
            json!({"supportsDeveloperRole": false, "supportsReasoningEffort": false}),
        );
        for model in provider
            .get_mut("models")
            .and_then(each)
            .unwrap_or_default()
        {
            let model = model
                .as_object_mut()
                .ok_or("has a model that is not a JSON object".to_string())?;
            let id = model.get("id").cloned().unwrap_or(Value::Null);
            default(model, "name", id);
            default(model, "input", json!(["text"]));
            default(model, "reasoning", json!(false));
            default(
                model,
                "cost",
                json!({"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}),
            );
        }
    }
    Ok(pretty(&value))
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;

    fn value(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn settings_get_the_session_model_they_leave_out() {
        let made = settings(r#"{"defaultProvider":"other","hide":false}"#, "p", "m").unwrap();
        assert_eq!(
            value(&made),
            json!({"defaultProvider": "other", "defaultModel": "m", "hide": false})
        );
        let made = settings(r#"{"defaultProvider":null,"defaultModel":false}"#, "p", "m").unwrap();
        assert_eq!(
            value(&made),
            json!({"defaultProvider": "p", "defaultModel": "m"})
        );
        assert!(made.ends_with("}\n"));
    }

    #[test]
    fn settings_that_are_not_an_object_are_refused() {
        assert_eq!(
            settings("[1]", "p", "m").unwrap_err(),
            "is not a JSON object"
        );
        assert_eq!(settings("{", "p", "m").unwrap_err(), "is not valid JSON");
    }

    #[test]
    fn a_model_list_gets_the_keys_pi_wants_and_keeps_its_own() {
        let made = models(
            r#"{"providers": {"lab": {"baseUrl": "http://m.test/v1", "apiKey": "unused",
              "models": [{"id": "a"}, {"id": "b", "name": "Named", "input": ["text", "image"], "reasoning": true}]},
              "own": {"compat": {"supportsDeveloperRole": true}, "models": {"x": {"id": "c", "contextWindow": 131072}}}}}"#,
        )
        .unwrap();
        let compat = json!({"supportsDeveloperRole": false, "supportsReasoningEffort": false});
        let cost = json!({"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0});
        assert_eq!(
            value(&made),
            json!({"providers": {
                "lab": {"baseUrl": "http://m.test/v1", "apiKey": "unused", "compat": compat, "models": [
                    {"id": "a", "name": "a", "input": ["text"], "reasoning": false, "cost": cost},
                    {"id": "b", "name": "Named", "input": ["text", "image"], "reasoning": true, "cost": cost}
                ]},
                "own": {"compat": {"supportsDeveloperRole": true}, "models": {"x":
                    {"id": "c", "contextWindow": 131072, "name": "c", "input": ["text"], "reasoning": false, "cost": cost}
                }}
            }})
        );
    }

    #[test]
    fn a_provider_with_no_models_gets_its_compat() {
        let made = models(r#"{"providers": [{"models": 5}, {}]}"#).unwrap();
        assert_eq!(value(&made)["providers"][0]["models"], json!(5));
        assert_eq!(
            value(&made)["providers"][1]["compat"]["supportsReasoningEffort"],
            json!(false)
        );
    }

    #[test]
    fn a_model_list_pi_cannot_read_is_refused() {
        assert_eq!(models("{").unwrap_err(), "is not valid JSON");
        assert_eq!(models("{}").unwrap_err(), "has no providers to list");
        assert_eq!(models("[]").unwrap_err(), "has no providers to list");
        assert_eq!(
            models(r#"{"providers": {"p": 1}}"#).unwrap_err(),
            "has a provider that is not a JSON object"
        );
        assert_eq!(
            models(r#"{"providers": {"p": {"models": [1]}}}"#).unwrap_err(),
            "has a model that is not a JSON object"
        );
    }

    #[test]
    fn the_verb_takes_the_instance_settings_over_the_generic_ones() {
        let d = dir("pi-verb");
        let generic = d.file("generic.json", r#"{"generic": true}"#);
        d.file(
            "inst/models.json",
            r#"{"providers": {"p": {"models": [{"id": "a"}]}}}"#,
        );
        let out = d.0.join("out");
        fs::create_dir_all(&out).unwrap();
        let args = |inst: &str| {
            [
                d.0.join(inst).display().to_string(),
                generic.display().to_string(),
                "p".into(),
                "m".into(),
                out.display().to_string(),
            ]
        };
        assert_eq!(verb(&args("inst")), 0);
        let made = value(&fs::read_to_string(out.join("settings.json")).unwrap());
        assert_eq!(made["generic"], json!(true));
        assert_eq!(made["defaultModel"], json!("m"));
        let made = value(&fs::read_to_string(out.join("models.json")).unwrap());
        assert_eq!(made["providers"]["p"]["models"][0]["name"], json!("a"));

        d.file("inst/settings.json", r#"{"own": 1}"#);
        assert_eq!(verb(&args("inst")), 0);
        let made = value(&fs::read_to_string(out.join("settings.json")).unwrap());
        assert_eq!(
            made,
            json!({"own": 1, "defaultProvider": "p", "defaultModel": "m"})
        );

        d.file("inst/settings.json", "[1]");
        assert_eq!(verb(&args("inst")), 78);
        d.file("inst/settings.json", "{}");
        d.file("inst/models.json", "nope");
        assert_eq!(verb(&args("inst")), 78);
        assert_eq!(verb(&args("missing")), 78);
        assert_eq!(verb(&args("inst")[..2]), 64);
    }
}
