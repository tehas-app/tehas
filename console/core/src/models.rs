//! The host's models.json. A provider may name a second endpoint that serves
//! the same models, and the token for each:
//!
//!   "baseUrl": "https://llm.example/v1",          "apiKey": "$TEHAS_MODEL_KEY_MAIN",
//!   "backupBaseUrl": "https://fleet.example/v1",  "backupApiKey": "$TEHAS_MODEL_KEY_FLEET"
//!
//! An apiKey is a word without "$" or "!" (such as "unused"), or
//! "$TEHAS_MODEL_KEY_<NAME>": a variable set in ~/.config/tehas/env, never in
//! the instance directory, which is in a repository. backupApiKey defaults to
//! apiKey. Pi reads the same "$NAME" form from its environment.
//!
//! Only the backup is new to Pi. A container is given a models.json of its own
//! with every provider on the endpoint picked at its start, and without the
//! backup keys. A cycle stays on its endpoint: one that fails mid-cycle is
//! retried by Pi, not switched.
//!
//! install.sh holds the one check of the keys that runs before any program of
//! this workspace exists: its model_keys_ok.

use crate::jq::J;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const ROLES: [&str; 5] = [
    "DEVELOPER",
    "OPS",
    "EXPLORE",
    "ADVERSARIAL_DEVELOPER",
    "CODE_REVIEW_SPECIALIST",
];

/// tehas_models: (provider, model), the subagent's first, each pair once.
pub fn used() -> Vec<(String, String)> {
    used_from(|k| std::env::var(k).unwrap_or_default())
}

fn used_from(env: impl Fn(&str) -> String) -> Vec<(String, String)> {
    let mut used: Vec<(String, String)> = Vec::new();
    let mut pairs = vec![(
        env("PI_ENSEMBLE_SUBAGENT_PROVIDER"),
        env("PI_ENSEMBLE_SUBAGENT_MODEL"),
    )];
    for r in ROLES {
        let m = env(&format!("PI_ENSEMBLE_MODEL_{r}"));
        if !m.is_empty() {
            pairs.push((env(&format!("PI_ENSEMBLE_PROVIDER_{r}")), m));
        }
    }
    for p in pairs {
        if !p.0.is_empty() && !p.1.is_empty() && !used.contains(&p) {
            used.push(p);
        }
    }
    used
}

/// Ok when the endpoint answers and offers every model, else the words the
/// hold says. `curl` runs curl with these arguments and the
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

/// "main" when it offers the provider's models, else
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

/// The variables the file names, sorted, each once.
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

/// Every key is a word or a set token. Why not is the message for standard
/// error.
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
// too.
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

/// The host's file, under HOME.
pub fn host_path() -> String {
    format!(
        "{}/.pi/agent/models.json",
        std::env::var("HOME").unwrap_or_default()
    )
}

/// [`keys_ok`] of the host's file, against the environment. A file that is
/// not there or cannot be read names no key.
pub fn host_keys_ok() -> Result<(), String> {
    let file = host_path();
    let set = |name: &str| std::env::var(name).is_ok_and(|v| !v.is_empty());
    keys_ok(&ModelFile::read(&file).json, &file, set)
}

/// curl with these arguments and the text on its standard input: what it
/// printed, when it succeeded.
pub fn curl(args: &[String], input: &str) -> Option<String> {
    let mut child = Command::new("curl")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn set(obj: &mut J, key: &str, value: J) {
    if let J::Obj(fields) = obj {
        match fields.iter_mut().find(|(k, _)| k == key) {
            Some(field) => field.1 = value,
            None => fields.push((key.to_string(), value)),
        }
    }
}

// A provider without its backup keys. None for what can hold no keys.
fn strip(provider: &mut J) -> Option<()> {
    match provider {
        J::Obj(entry) => entry.retain(|(k, _)| k != "backupBaseUrl" && k != "backupApiKey"),
        J::Null => {}
        _ => return None,
    }
    Some(())
}

/// The file as a container is given it: each provider on its picked endpoint
/// and without the backup keys, and the providers moved to their backup, in
/// the file's order. Key order and number text stay as the file has them.
/// None for what no provider list can be read in.
pub fn resolved(
    text: &str,
    used: &[(String, String)],
    curl: &dyn Fn(&[String], &str) -> Option<String>,
) -> Option<(J, Vec<String>)> {
    let mut doc = J::parse(text)?;
    if !matches!(doc, J::Obj(_) | J::Null) {
        return None;
    }
    let file = ModelFile {
        json: serde_json::from_str(text).ok()?,
    };
    let mut moved = Vec::new();
    let providers = match &mut doc {
        J::Obj(top) => top.iter_mut().find(|(k, _)| k == "providers"),
        _ => None,
    };
    match providers.map(|(_, list)| list) {
        Some(J::Obj(list)) => {
            for (name, p) in list {
                strip(p)?;
                if pick(&file, name, used, curl) != "backup" {
                    continue;
                }
                set(p, "baseUrl", J::str(file.url(name, "backup")));
                let key = file.key(name, "backup");
                if !key.is_empty() {
                    set(p, "apiKey", J::str(&key));
                }
                moved.push(name.clone());
            }
        }
        Some(J::Arr(list)) => {
            for p in list {
                strip(p)?;
            }
        }
        _ => {}
    }
    Some((doc, moved))
}

/// What [`write`] did.
pub struct Written {
    /// The variables the host's file names, for a container's environment.
    pub keys: Vec<String>,
    /// The providers on their backup.
    pub moved: Vec<String>,
}

/// Write a container's models.json to `out`, on the endpoints picked now. None
/// when the host's file cannot be used; a bad key says why on standard error.
pub fn write(out: &Path) -> Option<Written> {
    let file = host_path();
    let text = String::from_utf8_lossy(&fs::read(&file).ok()?).into_owned();
    if let Err(why) = host_keys_ok() {
        eprintln!("tehas: {why}");
        return None;
    }
    let (done, moved) = resolved(&text, &used(), &curl)?;
    let new = out.with_extension("json.new");
    fs::write(&new, format!("{}\n", done.pretty())).ok()?;
    fs::rename(&new, out).ok()?;
    let keys = key_names(&serde_json::from_str(&text).ok()?);
    Some(Written { keys, moved })
}

/// [`write`] for a container with a state directory: <out>, and in `endpoints`
/// one line "<provider> backup" for each provider on its backup.
pub fn write_for_container(out: &Path, endpoints: &Path) -> Option<()> {
    let lines: String = write(out)?
        .moved
        .iter()
        .map(|name| format!("{name} backup\n"))
        .collect();
    fs::write(endpoints, lines).ok()
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

    const TWO: &str = r#"{"providers":{
      "main":{"baseUrl":"http://m/v1","apiKey":"$TEHAS_MODEL_KEY_MAIN","backupBaseUrl":"http://b/v1","backupApiKey":"${TEHAS_MODEL_KEY_FLEET}","models":[{"id":"big","cost":0.10}]},
      "word":{"baseUrl":"http://w/v1","apiKey":"unused"}},"z":1}"#;

    fn two_used() -> Vec<(String, String)> {
        vec![("main".to_string(), "big".to_string())]
    }

    // An endpoint that offers the model when its url starts `host`.
    fn only(host: &'static str) -> impl Fn(&[String], &str) -> Option<String> {
        move |args, _| {
            args[5]
                .starts_with(host)
                .then(|| r#"{"data":[{"id":"big"}]}"#.to_string())
        }
    }

    #[test]
    fn the_names_are_sorted_each_once_and_without_their_marks() {
        assert_eq!(
            key_names(&serde_json::from_str(TWO).unwrap()),
            ["TEHAS_MODEL_KEY_FLEET", "TEHAS_MODEL_KEY_MAIN"]
        );
        assert!(key_names(&json!({})).is_empty());
    }

    #[test]
    fn a_key_is_a_word_or_a_set_token() {
        let two: Value = serde_json::from_str(TWO).unwrap();
        assert_eq!(keys_ok(&two, "f", |_| true), Ok(()));
        assert_eq!(
            keys_ok(&two, "f", |k| k != "TEHAS_MODEL_KEY_FLEET"),
            Err("models.json names TEHAS_MODEL_KEY_FLEET and it is not set: put it in ~/.config/tehas/env".to_string())
        );
        for bad in [
            "$OTHER",
            "pass!word",
            "a$b",
            "$TEHAS_MODEL_KEY_",
            "$TEHAS_MODEL_KEY_a",
        ] {
            assert_eq!(
                keys_ok(&json!({"providers": {"odd": {"apiKey": bad}}}), "f", |_| true),
                Err("models.json: provider odd has an apiKey that is neither a word nor $TEHAS_MODEL_KEY_<NAME> (see f)".to_string()),
                "{bad}"
            );
        }
    }

    #[test]
    fn main_answers_and_the_backup_keys_go() {
        let (done, moved) = resolved(TWO, &two_used(), &only("http://m/")).unwrap();
        assert_eq!(
            done.dump(),
            r#"{"providers":{"main":{"baseUrl":"http://m/v1","apiKey":"$TEHAS_MODEL_KEY_MAIN","models":[{"id":"big","cost":0.10}]},"word":{"baseUrl":"http://w/v1","apiKey":"unused"}},"z":1}"#
        );
        assert!(moved.is_empty());
    }

    #[test]
    fn main_is_down_and_the_backup_takes_its_place() {
        let (done, moved) = resolved(TWO, &two_used(), &only("http://b/")).unwrap();
        assert_eq!(
            done.dump(),
            r#"{"providers":{"main":{"baseUrl":"http://b/v1","apiKey":"${TEHAS_MODEL_KEY_FLEET}","models":[{"id":"big","cost":0.10}]},"word":{"baseUrl":"http://w/v1","apiKey":"unused"}},"z":1}"#
        );
        assert_eq!(moved, ["main"]);
    }

    #[test]
    fn the_providers_moved_keep_the_order_of_the_file() {
        let text = r#"{"providers":{
          "z":{"baseUrl":"http://m/v1","backupBaseUrl":"http://b/v1"},
          "a":{"baseUrl":"http://m/v1","backupBaseUrl":"http://b/v1"}}}"#;
        let used = [
            ("a".to_string(), "big".to_string()),
            ("z".to_string(), "big".to_string()),
        ];
        let (_, moved) = resolved(text, &used, &only("http://b/")).unwrap();
        assert_eq!(moved, ["z", "a"]);
    }

    #[test]
    fn both_down_stays_on_main() {
        let (done, moved) = resolved(TWO, &two_used(), &only("x/")).unwrap();
        assert!(done.dump().contains(r#""baseUrl":"http://m/v1""#));
        assert!(moved.is_empty());
    }

    #[test]
    fn no_call_is_made_without_a_backup() {
        let text = r#"{"providers":{"word":{"baseUrl":"http://w/v1","apiKey":"unused"}}}"#;
        let (done, _) = resolved(text, &two_used(), &|_, _| panic!("no call")).unwrap();
        assert_eq!(done.dump(), text);
    }

    #[test]
    fn what_no_provider_list_can_be_read_in_is_refused() {
        let no = |_: &[String], _: &str| None;
        assert!(resolved("[]", &[], &no).is_none());
        assert!(resolved(r#"{"providers":{"a":7}}"#, &[], &no).is_none());
        assert!(resolved(r#"{"providers":[7]}"#, &[], &no).is_none());
        assert!(resolved("not json", &[], &no).is_none());
    }

    #[test]
    fn a_list_of_providers_loses_its_backup_keys_and_moves_nothing() {
        let text = r#"{"providers":[{"baseUrl":"m","backupBaseUrl":"b"}]}"#;
        let (done, moved) = resolved(text, &[], &|_, _| panic!("no call")).unwrap();
        assert_eq!(done.dump(), r#"{"providers":[{"baseUrl":"m"}]}"#);
        assert!(moved.is_empty());
    }

    #[test]
    fn the_models_of_a_provider_are_the_agents_each_once() {
        let env_of = |k: &str| {
            match k {
                "PI_ENSEMBLE_SUBAGENT_PROVIDER"
                | "PI_ENSEMBLE_PROVIDER_OPS"
                | "PI_ENSEMBLE_PROVIDER_EXPLORE" => "p",
                "PI_ENSEMBLE_SUBAGENT_MODEL" | "PI_ENSEMBLE_MODEL_EXPLORE" => "one",
                "PI_ENSEMBLE_MODEL_OPS" => "two",
                "PI_ENSEMBLE_MODEL_DEVELOPER" => "three",
                "PI_ENSEMBLE_PROVIDER_DEVELOPER" => "q",
                _ => "",
            }
            .to_string()
        };
        let pair = |p: &str, m: &str| (p.to_string(), m.to_string());
        assert_eq!(
            used_from(env_of),
            [pair("p", "one"), pair("q", "three"), pair("p", "two")]
        );
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
