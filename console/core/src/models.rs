//! The host's models.json: the endpoints of a provider and the key each is
//! asked with, as tehas_endpoint_url, tehas_endpoint_key and
//! _tehas_endpoint_auth in lib/models.sh read them, the models the agents use,
//! and how an endpoint is asked for them.

use serde_json::Value;

const ROLES: [&str; 5] = [
    "DEVELOPER",
    "OPS",
    "EXPLORE",
    "ADVERSARIAL_DEVELOPER",
    "CODE_REVIEW_SPECIALIST",
];

/// tehas_models: (provider, model), the subagent's first, each pair once.
pub fn used() -> Vec<(String, String)> {
    let env = |k: String| std::env::var(k).unwrap_or_default();
    let mut used: Vec<(String, String)> = Vec::new();
    let mut pairs = vec![(
        env("PI_ENSEMBLE_SUBAGENT_PROVIDER".into()),
        env("PI_ENSEMBLE_SUBAGENT_MODEL".into()),
    )];
    for r in ROLES {
        let m = env(format!("PI_ENSEMBLE_MODEL_{r}"));
        if !m.is_empty() {
            pairs.push((env(format!("PI_ENSEMBLE_PROVIDER_{r}")), m));
        }
    }
    for p in pairs {
        if !p.0.is_empty() && !p.1.is_empty() && !used.contains(&p) {
            used.push(p);
        }
    }
    used
}

/// tehas_endpoint_ask: Ok when the endpoint answers and offers every model,
/// else the words the hold says. `curl` runs curl with these arguments and the
/// text on its standard input, and gives what it printed when it succeeded.
pub fn ask(
    url: &str,
    key: &str,
    models: &[String],
    curl: &dyn Fn(&[String], &str) -> Option<String>,
) -> Result<(), String> {
    let args = ["-fsS", "-K", "-", "-m", "10", &format!("{url}/models")].map(String::from);
    let Some(out) = curl(&args, &auth(key)) else {
        return Err("does not answer".into());
    };
    let list: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    let Some(data) = list
        .get("data")
        .and_then(Value::as_array)
        .filter(|d| !d.is_empty())
    else {
        return Err("offers no models".into());
    };
    for m in models {
        if !data
            .iter()
            .any(|x| x.get("id").and_then(Value::as_str) == Some(m))
        {
            return Err(format!("does not offer {m}"));
        }
    }
    Ok(())
}

/// tehas_endpoint_pick: "main" when it offers the provider's models, else
/// "backup" when that does, else "main". No call without a backupBaseUrl.
pub fn pick(
    file: &ModelFile,
    provider: &str,
    used: &[(String, String)],
    curl: &dyn Fn(&[String], &str) -> Option<String>,
) -> &'static str {
    let backup = file.url(provider, "backup");
    if backup.is_empty() {
        return "main";
    }
    let models: Vec<String> = used
        .iter()
        .filter(|(p, _)| p == provider)
        .map(|(_, m)| m.clone())
        .collect();
    let main_ok = ask(
        &file.url(provider, "main"),
        &file.key(provider, "main"),
        &models,
        curl,
    );
    if main_ok.is_err() && ask(&backup, &file.key(provider, "backup"), &models, curl).is_ok() {
        "backup"
    } else {
        "main"
    }
}

pub struct ModelFile {
    /// The file, null when there is none or it cannot be read.
    pub json: Value,
}

/// tehas_model_key_names: the variables the file names, sorted, each once.
pub fn key_names(models: &Value) -> Vec<String> {
    let mut names: Vec<String> = keys(models)
        .filter_map(|(_, k)| k.strip_prefix('$'))
        .map(|k| k.strip_prefix('{').unwrap_or(k))
        .map(|k| k.strip_suffix('}').unwrap_or(k).to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// tehas_model_keys_ok: every key is a word or a set token. Why not is the
/// message for standard error.
pub fn keys_ok(models: &Value, file: &str, set: impl Fn(&str) -> bool) -> Result<(), String> {
    if let Some((provider, _)) =
        keys(models).find(|(_, k)| !names_a_token(k) && k.contains(['$', '!']))
    {
        return Err(format!(
            "models.json: provider {provider} has an apiKey that is neither a word nor $TEHAS_MODEL_KEY_<NAME> (see {file})"
        ));
    }
    match key_names(models).into_iter().find(|name| !set(name)) {
        Some(name) => Err(format!(
            "models.json names {name} and it is not set: put it in ~/.config/tehas/env"
        )),
        None => Ok(()),
    }
}

// Every provider's two keys, as (provider, key).
fn keys(models: &Value) -> impl Iterator<Item = (&str, &str)> {
    models
        .get("providers")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .flat_map(|(name, p)| {
            ["apiKey", "backupApiKey"]
                .into_iter()
                .filter_map(move |k| Some((name.as_str(), p.get(k)?.as_str()?)))
        })
}

// `$TEHAS_MODEL_KEY_<NAME>` or `${TEHAS_MODEL_KEY_<NAME>}`, each brace alone
// too, as the pattern of tehas_model_keys_ok.
fn names_a_token(key: &str) -> bool {
    let Some(rest) = key.strip_prefix('$') else {
        return false;
    };
    let rest = rest.strip_prefix('{').unwrap_or(rest);
    let rest = rest.strip_suffix('}').unwrap_or(rest);
    rest.strip_prefix("TEHAS_MODEL_KEY_").is_some_and(|name| {
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    })
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

    fn of(json: Value) -> ModelFile {
        ModelFile { json }
    }

    #[test]
    fn an_endpoint_is_asked_for_every_model() {
        let offers = |out: &'static str| move |_: &[String], _: &str| Some(out.to_string());
        let m = ["a".to_string()];
        assert_eq!(
            ask("u", "w", &m, &|_, _| None).unwrap_err(),
            "does not answer"
        );
        assert_eq!(
            ask("u", "w", &m, &offers("{}")).unwrap_err(),
            "offers no models"
        );
        assert_eq!(
            ask("u", "w", &m, &offers(r#"{"data":[{"id":"b"}]}"#)).unwrap_err(),
            "does not offer a"
        );
        assert!(ask("u", "w", &m, &offers(r#"{"data":[{"id":"a"}]}"#)).is_ok());
    }

    #[test]
    fn the_backup_is_picked_only_when_it_offers_what_the_main_does_not() {
        let f = of(serde_json::json!({"providers": {"p": {"baseUrl": "m", "backupBaseUrl": "b"}}}));
        let used = [
            ("p".to_string(), "a".to_string()),
            ("q".to_string(), "z".to_string()),
        ];
        let good = r#"{"data":[{"id":"a"}]}"#.to_string();
        let up = |host: &'static str| {
            let good = good.clone();
            move |args: &[String], _: &str| args[5].starts_with(host).then(|| good.clone())
        };
        assert_eq!(pick(&f, "p", &used, &up("m/")), "main");
        assert_eq!(pick(&f, "p", &used, &up("b/")), "backup");
        assert_eq!(pick(&f, "p", &used, &up("x/")), "main");
        let none = of(serde_json::json!({"providers": {"p": {"baseUrl": "m"}}}));
        assert_eq!(pick(&none, "p", &used, &|_, _| panic!("asked")), "main");
    }
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
