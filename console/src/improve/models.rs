//! The analyst's own models.json, on the endpoint picked now, and the tokens it
//! names: what lib/models.sh does for the other commands, for this one. Key
//! order and number text stay as the host's file has them.

use crate::jq::J;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::models::{auth, host_path};

const ROLES: [&str; 5] = [
    "DEVELOPER",
    "OPS",
    "EXPLORE",
    "ADVERSARIAL_DEVELOPER",
    "CODE_REVIEW_SPECIALIST",
];

fn fields(v: &J) -> &[(String, J)] {
    match v {
        J::Obj(o) => o,
        _ => &[],
    }
}

fn keys_of(provider: &J) -> impl Iterator<Item = &str> {
    [provider.get("apiKey"), provider.get("backupApiKey")]
        .into_iter()
        .filter_map(J::as_str)
}

/// `$TEHAS_MODEL_KEY_<NAME>` or `${TEHAS_MODEL_KEY_<NAME>}`, each brace alone
/// too, as the pattern of tehas_model_keys_ok.
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

/// tehas_model_key_names: the variables the file names, sorted, each once.
pub fn key_names(doc: &J) -> Vec<String> {
    let names: BTreeSet<String> = fields(doc.get("providers"))
        .iter()
        .flat_map(|(_, p)| keys_of(p))
        .filter_map(|k| k.strip_prefix('$'))
        .map(|k| k.strip_prefix('{').unwrap_or(k))
        .map(|k| k.strip_suffix('}').unwrap_or(k).to_string())
        .collect();
    names.into_iter().collect()
}

/// tehas_model_keys_ok: every key is a word or a set token. Why not is the
/// message for standard error.
fn keys_ok(doc: &J, file: &str, set: impl Fn(&str) -> bool) -> Result<(), String> {
    for (provider, p) in fields(doc.get("providers")) {
        if keys_of(p).any(|k| !names_a_token(k) && k.contains(['$', '!'])) {
            return Err(format!(
                "models.json: provider {provider} has an apiKey that is neither a word nor $TEHAS_MODEL_KEY_<NAME> (see {file})"
            ));
        }
    }
    for name in key_names(doc) {
        if !set(&name) {
            return Err(format!(
                "models.json names {name} and it is not set: put it in ~/.config/tehas/env"
            ));
        }
    }
    Ok(())
}

/// The models the agents use on `provider`, as tehas_models lists them.
fn models_of(provider: &str, env_of: &impl Fn(&str) -> String) -> Vec<String> {
    let mut pairs = vec![(
        env_of("PI_ENSEMBLE_SUBAGENT_PROVIDER"),
        env_of("PI_ENSEMBLE_SUBAGENT_MODEL"),
    )];
    for role in ROLES {
        let model = env_of(&format!("PI_ENSEMBLE_MODEL_{role}"));
        if !model.is_empty() {
            pairs.push((env_of(&format!("PI_ENSEMBLE_PROVIDER_{role}")), model));
        }
    }
    let mut models: Vec<String> = Vec::new();
    for (p, m) in pairs {
        if p == provider && !m.is_empty() && !models.contains(&m) {
            models.push(m);
        }
    }
    models
}

/// tehas_endpoint_ask: does `url` answer and offer every model? The token goes
/// to curl on standard input, never on a command line.
fn offers(url: &str, key: &str, models: &[String]) -> bool {
    let child = Command::new("curl")
        .args(["-fsS", "-K", "-", "-m", "10", &format!("{url}/models")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return false };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(auth(key).as_bytes());
    }
    let Ok(out) = child.wait_with_output() else {
        return false;
    };
    let list = J::parse(&String::from_utf8_lossy(&out.stdout));
    out.status.success() && list.is_some_and(|l| lists(&l, models))
}

fn lists(answer: &J, models: &[String]) -> bool {
    let data = answer.get("data").items();
    !data.is_empty()
        && models
            .iter()
            .all(|m| data.iter().any(|x| x.get("id").as_str() == Some(m)))
}

fn string<'a>(v: &'a J, key: &str) -> &'a str {
    v.get(key).as_str().unwrap_or("")
}

fn backup_key(p: &J) -> &str {
    let key = p.get("backupApiKey").or(p.get("apiKey"));
    key.as_str().unwrap_or("")
}

/// tehas_endpoint_pick: the backup only when main does not offer the provider's
/// models and it does. No call without a backup.
fn on_backup(p: &J, models: &[String], ask: &impl Fn(&str, &str, &[String]) -> bool) -> bool {
    let backup = string(p, "backupBaseUrl");
    !backup.is_empty()
        && !ask(string(p, "baseUrl"), string(p, "apiKey"), models)
        && ask(backup, backup_key(p), models)
}

fn set(obj: &mut J, key: &str, value: J) {
    if let J::Obj(fields) = obj {
        match fields.iter_mut().find(|(k, _)| k == key) {
            Some(field) => field.1 = value,
            None => fields.push((key.to_string(), value)),
        }
    }
}

/// tehas_models_resolve, as a document: each provider on its picked endpoint
/// and without the backup keys. None for what no provider list can be read in.
fn resolved(
    doc: &J,
    models_of: &impl Fn(&str) -> Vec<String>,
    ask: &impl Fn(&str, &str, &[String]) -> bool,
) -> Option<J> {
    if !matches!(doc, J::Obj(_) | J::Null) {
        return None;
    }
    let mut out = doc.clone();
    if let J::Obj(top) = &mut out {
        if let Some((_, J::Obj(providers))) = top.iter_mut().find(|(k, _)| k == "providers") {
            for (name, p) in providers {
                if !matches!(p, J::Obj(_) | J::Null) {
                    return None;
                }
                let given = doc.at(&["providers", name]);
                if let J::Obj(entry) = p {
                    entry.retain(|(k, _)| k != "backupBaseUrl" && k != "backupApiKey");
                }
                if !matches!(given.get("backupBaseUrl"), J::Str(_))
                    || !on_backup(given, &models_of(name), ask)
                {
                    continue;
                }
                set(p, "baseUrl", J::str(string(given, "backupBaseUrl")));
                let key = backup_key(given);
                if !key.is_empty() {
                    set(p, "apiKey", J::str(key));
                }
            }
        }
    }
    Some(out)
}

/// Write the analyst's models.json to `out`, and give the names of the tokens
/// its container is given. None when the host's file cannot be used; a bad key
/// says why on standard error.
pub fn write(out: &Path) -> Option<Vec<String>> {
    let file = host_path();
    let text = fs::read(&file).ok()?;
    let doc = J::parse(&String::from_utf8_lossy(&text))?;
    let value = |k: &str| env::var(k).unwrap_or_default();
    if let Err(why) = keys_ok(&doc, &file, |k| !value(k).is_empty()) {
        eprintln!("tehas: {why}");
        return None;
    }
    let done = resolved(&doc, &|p| models_of(p, &value), &offers)?;
    let new = out.with_extension("json.new");
    fs::write(&new, format!("{}\n", done.pretty())).ok()?;
    fs::rename(&new, out).ok()?;
    Some(key_names(&doc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO: &str = r#"{"providers":{
      "main":{"baseUrl":"http://m/v1","apiKey":"$TEHAS_MODEL_KEY_MAIN","backupBaseUrl":"http://b/v1","backupApiKey":"${TEHAS_MODEL_KEY_FLEET}","models":[{"id":"big","cost":0.10}]},
      "word":{"baseUrl":"http://w/v1","apiKey":"unused"}},"z":1}"#;

    fn doc(text: &str) -> J {
        J::parse(text).unwrap()
    }

    #[test]
    fn the_names_are_sorted_each_once_and_without_their_marks() {
        assert_eq!(
            key_names(&doc(TWO)),
            ["TEHAS_MODEL_KEY_FLEET", "TEHAS_MODEL_KEY_MAIN"]
        );
        assert!(key_names(&doc("{}")).is_empty());
    }

    #[test]
    fn a_key_is_a_word_or_a_set_token() {
        assert_eq!(keys_ok(&doc(TWO), "f", |_| true), Ok(()));
        assert_eq!(
            keys_ok(&doc(TWO), "f", |k| k != "TEHAS_MODEL_KEY_FLEET"),
            Err("models.json names TEHAS_MODEL_KEY_FLEET and it is not set: put it in ~/.config/tehas/env".to_string())
        );
        for bad in [
            "$OTHER",
            "pass!word",
            "a$b",
            "$TEHAS_MODEL_KEY_",
            "$TEHAS_MODEL_KEY_a",
        ] {
            let text = format!(r#"{{"providers":{{"odd":{{"apiKey":"{bad}"}}}}}}"#);
            assert_eq!(
                keys_ok(&doc(&text), "f", |_| true),
                Err("models.json: provider odd has an apiKey that is neither a word nor $TEHAS_MODEL_KEY_<NAME> (see f)".to_string()),
                "{bad}"
            );
        }
    }

    fn no_models(_: &str) -> Vec<String> {
        vec!["big".to_string()]
    }

    #[test]
    fn main_answers_and_the_backup_keys_go() {
        let ask = |url: &str, key: &str, _: &[String]| {
            assert_eq!((url, key), ("http://m/v1", "$TEHAS_MODEL_KEY_MAIN"));
            true
        };
        assert_eq!(
            resolved(&doc(TWO), &no_models, &ask).unwrap().dump(),
            r#"{"providers":{"main":{"baseUrl":"http://m/v1","apiKey":"$TEHAS_MODEL_KEY_MAIN","models":[{"id":"big","cost":0.10}]},"word":{"baseUrl":"http://w/v1","apiKey":"unused"}},"z":1}"#
        );
    }

    #[test]
    fn main_is_down_and_the_backup_takes_its_place() {
        let ask = |url: &str, _: &str, _: &[String]| url == "http://b/v1";
        assert_eq!(
            resolved(&doc(TWO), &no_models, &ask).unwrap().dump(),
            r#"{"providers":{"main":{"baseUrl":"http://b/v1","apiKey":"${TEHAS_MODEL_KEY_FLEET}","models":[{"id":"big","cost":0.10}]},"word":{"baseUrl":"http://w/v1","apiKey":"unused"}},"z":1}"#
        );
    }

    #[test]
    fn both_down_stays_on_main() {
        let ask = |_: &str, _: &str, _: &[String]| false;
        let got = resolved(&doc(TWO), &no_models, &ask).unwrap().dump();
        assert!(got.contains(r#""baseUrl":"http://m/v1""#), "{got}");
    }

    #[test]
    fn no_call_is_made_without_a_backup() {
        let ask = |_: &str, _: &str, _: &[String]| -> bool { panic!("no call") };
        let text = r#"{"providers":{"word":{"baseUrl":"http://w/v1","apiKey":"unused"}}}"#;
        assert_eq!(resolved(&doc(text), &no_models, &ask).unwrap().dump(), text);
    }

    #[test]
    fn what_no_provider_list_can_be_read_in_is_refused() {
        let ask = |_: &str, _: &str, _: &[String]| true;
        assert!(resolved(&doc("[]"), &no_models, &ask).is_none());
        assert!(resolved(&doc(r#"{"providers":{"a":7}}"#), &no_models, &ask).is_none());
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
        assert_eq!(models_of("p", &env_of), ["one", "two"]);
        assert_eq!(models_of("q", &env_of), ["three"]);
        assert!(models_of("r", &env_of).is_empty());
    }

    #[test]
    fn an_endpoint_lists_every_model_or_none_count() {
        let wanted = ["a".to_string()];
        assert!(lists(&doc(r#"{"data":[{"id":"a"},{"id":"b"}]}"#), &wanted));
        assert!(lists(&doc(r#"{"data":[{"id":"b"}]}"#), &[]));
        assert!(!lists(&doc(r#"{"data":[{"id":"b"}]}"#), &wanted));
        assert!(!lists(&doc(r#"{"data":[]}"#), &[]));
        assert!(!lists(&doc("{}"), &[]));
    }
}
