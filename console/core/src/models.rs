//! The host's models.json: the endpoints of a provider and the key each is
//! asked with, as tehas_endpoint_url, tehas_endpoint_key and
//! _tehas_endpoint_auth in lib/models.sh read them.

use serde_json::Value;

pub struct ModelFile {
    /// The file, null when there is none or it cannot be read.
    pub json: Value,
}

/// tehas_models_file: the host's file, under HOME.
pub fn host_path() -> String {
    format!(
        "{}/.pi/agent/models.json",
        std::env::var("HOME").unwrap_or_default()
    )
}

impl ModelFile {
    pub fn read(path: &str) -> ModelFile {
        let json = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        ModelFile { json }
    }

    /// The url of a provider's main or backup endpoint, empty when it has none.
    pub fn url(&self, provider: &str, end: &str) -> String {
        let k = if end == "main" {
            "baseUrl"
        } else {
            "backupBaseUrl"
        };
        self.json
            .get("providers")
            .and_then(|x| x.get(provider))
            .and_then(|x| x.get(k))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    /// The key of an endpoint as the file writes it. A backup with none has
    /// the main endpoint's.
    pub fn key(&self, provider: &str, end: &str) -> String {
        let pr = self.json.get("providers").and_then(|x| x.get(provider));
        let k = |n: &str| pr.and_then(|x| x.get(n)).and_then(Value::as_str);
        let v = if end == "main" {
            k("apiKey")
        } else {
            k("backupApiKey").or_else(|| k("apiKey"))
        };
        v.unwrap_or("").to_string()
    }
}

/// curl's config line for a key named by a variable, empty for a plain word
/// or a variable that is not set. It goes to curl on standard input with
/// `-K -`, never on a command line.
pub fn auth(key: &str) -> String {
    auth_from(key, |name| std::env::var(name).unwrap_or_default())
}

fn auth_from(key: &str, value: impl Fn(&str) -> String) -> String {
    let name = key.strip_prefix('$').unwrap_or(key);
    let name = name.strip_prefix('{').unwrap_or(name);
    let name = name.strip_suffix('}').unwrap_or(name);
    if name == key {
        return String::new();
    }
    let v = value(name);
    if v.is_empty() {
        String::new()
    } else {
        format!("header = \"Authorization: Bearer {v}\"\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file() -> ModelFile {
        ModelFile {
            json: json!({"providers": {
                "two": {"baseUrl": "https://main.example/v1", "apiKey": "$TEHAS_MODEL_KEY_MAIN",
                        "backupBaseUrl": "https://backup.example/v1", "backupApiKey": "$TEHAS_MODEL_KEY_FLEET"},
                "one": {"baseUrl": "https://only.example/v1", "apiKey": "unused"},
                "odd": {"baseUrl": 7}
            }}),
        }
    }

    #[test]
    fn a_provider_names_its_two_endpoints() {
        let f = file();
        assert_eq!(f.url("two", "main"), "https://main.example/v1");
        assert_eq!(f.url("two", "backup"), "https://backup.example/v1");
        assert_eq!(f.key("two", "main"), "$TEHAS_MODEL_KEY_MAIN");
        assert_eq!(f.key("two", "backup"), "$TEHAS_MODEL_KEY_FLEET");
    }

    #[test]
    fn a_backup_with_no_key_has_the_main_key() {
        assert_eq!(file().key("one", "backup"), "unused");
    }

    #[test]
    fn what_is_not_there_is_empty() {
        let f = file();
        assert_eq!(f.url("one", "backup"), "");
        assert_eq!(f.url("odd", "main"), "");
        assert_eq!(f.url("none", "main"), "");
        assert_eq!(f.key("none", "main"), "");
        let none = ModelFile::read("/nonexistent/models.json");
        assert!(none.json.is_null());
        assert_eq!(none.url("two", "main"), "");
    }

    #[test]
    fn a_key_named_by_a_variable_is_a_header() {
        let set = |_: &str| "s3cret".to_string();
        let want = "header = \"Authorization: Bearer s3cret\"\n";
        assert_eq!(auth_from("$TEHAS_MODEL_KEY_MAIN", set), want);
        assert_eq!(auth_from("${TEHAS_MODEL_KEY_MAIN}", set), want);
    }

    #[test]
    fn a_word_or_an_unset_variable_sends_nothing() {
        assert_eq!(auth_from("unused", |_| "s3cret".into()), "");
        assert_eq!(auth_from("$TEHAS_MODEL_KEY_MAIN", |_| String::new()), "");
    }
}
