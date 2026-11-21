//! The endpoint a plugin's model socket sends to: a provider's main endpoint
//! of the instance's models.json, read as `tehas_core::models` reads it.

use serde_json::Value;
use std::path::Path;
use tehas_core::models::{keys_ok, ModelFile};

/// Where a request goes and as what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    /// The base URL, `http://` or `https://`, without a trailing `/`.
    pub url: String,
    /// The key as the file writes it: a word, or `$TEHAS_MODEL_KEY_<NAME>`,
    /// whose value is read from the environment when a request is sent.
    pub key: String,
    /// The endpoint's own id of the model.
    pub model: String,
}

/// The upstream of `provider`'s `model` in the file at `path`.
pub fn read(path: &Path, provider: &str, model: &str) -> Result<Upstream, String> {
    let file = ModelFile::read(&path.to_string_lossy());
    of(&file.json, provider, model).map_err(|why| format!("{why} (see {})", path.display()))
}

/// The upstream of `provider`'s `model`. Refused: a file whose keys
/// `keys_ok` refuses, a provider that is not there or does not speak chat
/// completions, a base URL that is not plain `http://` or `https://`, and a
/// model the provider does not list.
pub fn of(models: &Value, provider: &str, model: &str) -> Result<Upstream, String> {
    keys_ok(models, "models.json", |_| true)?;
    let Some(p) = models.get("providers").and_then(|p| p.get(provider)) else {
        return Err(format!("models.json has no provider {provider}"));
    };
    if p.get("api").and_then(Value::as_str) != Some("openai-completions") {
        return Err(format!(
            "models.json: provider {provider} does not speak openai-completions"
        ));
    }
    let file = ModelFile {
        json: models.clone(),
    };
    let url = file.url(provider, "main");
    let plain = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .is_some_and(|rest| {
            !rest.is_empty() && !rest.chars().any(|c| c.is_control() || c.is_whitespace())
        });
    if !plain {
        return Err(format!(
            "models.json: provider {provider} has a baseUrl that is not http:// or https://"
        ));
    }
    let listed = p.get("models").and_then(Value::as_array).is_some_and(|ms| {
        ms.iter()
            .any(|m| m.get("id").and_then(Value::as_str) == Some(model))
    });
    if !listed {
        return Err(format!(
            "models.json: provider {provider} lists no model {model}"
        ));
    }
    Ok(Upstream {
        url: url.trim_end_matches('/').to_string(),
        key: file.key(provider, "main"),
        model: model.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tehas_core::testdir::TestDir;

    fn file(provider: Value) -> Value {
        json!({ "providers": { "local": provider } })
    }

    fn local() -> Value {
        json!({
            "api": "openai-completions",
            "baseUrl": "https://models.example:8080/v1/",
            "apiKey": "$TEHAS_MODEL_KEY_MAIN",
            "backupBaseUrl": "https://fleet.example/v1",
            "backupApiKey": "$TEHAS_MODEL_KEY_FLEET",
            "models": [{ "id": "example-model" }, { "id": "other" }]
        })
    }

    fn with(field: &str, value: Value) -> Value {
        let mut p = local();
        p[field] = value;
        file(p)
    }

    #[test]
    fn the_example_instance_s_file_names_its_endpoint_key_and_model() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../instances/example/models.json");
        assert_eq!(
            read(&path, "local", "example-model"),
            Ok(Upstream {
                url: "http://models.example:8080/v1".into(),
                key: "unused".into(),
                model: "example-model".into(),
            })
        );
    }

    #[test]
    fn the_main_endpoint_and_its_key_are_taken_never_the_backup() {
        let up = of(&file(local()), "local", "other").unwrap();
        assert_eq!(up.url, "https://models.example:8080/v1");
        assert_eq!(up.key, "$TEHAS_MODEL_KEY_MAIN");
        assert_eq!(up.model, "other");
    }

    #[test]
    fn a_provider_or_model_that_is_not_there_is_refused() {
        assert!(of(&file(local()), "remote", "example-model")
            .unwrap_err()
            .contains("no provider remote"));
        assert!(of(&file(local()), "local", "missing")
            .unwrap_err()
            .contains("no model missing"));
        assert!(of(&json!({}), "local", "example-model").is_err());
    }

    #[test]
    fn only_chat_completions_over_plain_http_or_https_is_taken() {
        for api in [json!("anthropic-messages"), Value::Null] {
            assert!(of(&with("api", api), "local", "example-model")
                .unwrap_err()
                .contains("openai-completions"));
        }
        for url in [
            "file:///etc/passwd",
            "ftp://h/v1",
            "https://",
            "http://h /v1",
            "",
            "h/v1",
        ] {
            assert!(
                of(&with("baseUrl", json!(url)), "local", "example-model")
                    .unwrap_err()
                    .contains("baseUrl"),
                "{url}"
            );
        }
    }

    #[test]
    fn a_key_that_is_neither_a_word_nor_a_model_key_is_refused() {
        assert!(
            of(&with("apiKey", json!("$PATH")), "local", "example-model")
                .unwrap_err()
                .contains("apiKey")
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_names_itself() {
        let dir = TestDir::new("upstream");
        let path = dir.join("models.json");
        std::fs::write(&path, "{").unwrap();
        let why = read(&path, "local", "example-model").unwrap_err();
        assert!(why.contains("no provider local"), "{why}");
        assert!(why.contains(&path.display().to_string()), "{why}");
    }
}
