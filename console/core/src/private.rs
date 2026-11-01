//! The names the host knows that an agent can print, for a mask to hide: the
//! host aliases, the host of every endpoint of every provider the agents use
//! and the bot's email. They are names, not secrets, so they may go on a
//! command line; a token never does. A name under 4 characters would mask
//! ordinary text.
//!
//! This is `tehas_private_names` of lib/private.sh, which libexec/work and
//! libexec/review still source. test/mcp_test.sh compares the two.

use crate::models::ModelFile;
use std::collections::HashSet;
use std::env;

const MIN_CHARS: usize = 4;

// _tehas_roles of lib/config.sh.
const ROLES: [&str; 5] = [
    "DEVELOPER",
    "OPS",
    "EXPLORE",
    "ADVERSARIAL_DEVELOPER",
    "CODE_REVIEW_SPECIALIST",
];

/// The provider of each pair `tehas_models` lists: a provider with two models
/// comes twice, and so do its names.
fn providers(get: impl Fn(&str) -> String) -> Vec<String> {
    let subagent = format!(
        "{} {}",
        get("PI_ENSEMBLE_SUBAGENT_PROVIDER"),
        get("PI_ENSEMBLE_SUBAGENT_MODEL")
    );
    let roles = ROLES.iter().filter_map(|role| {
        let model = get(&format!("PI_ENSEMBLE_MODEL_{role}"));
        let provider = get(&format!("PI_ENSEMBLE_PROVIDER_{role}"));
        (!model.is_empty()).then(|| format!("{provider} {model}"))
    });
    let mut seen = HashSet::new();
    std::iter::once(subagent)
        .chain(roles)
        .filter_map(|pair| {
            let mut words = pair.split_ascii_whitespace();
            let provider = words.next()?.to_string();
            words.next()?;
            seen.insert(pair.clone()).then_some(provider)
        })
        .collect()
}

/// The host of an endpoint's url: no scheme, path or user, and no port. An
/// IPv6 address loses its brackets.
fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split('/').next().unwrap_or_default();
    let rest = rest.rsplit('@').next().unwrap_or_default();
    match rest.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => rest.split(':').next().unwrap_or_default(),
    }
}

fn names_from(models: &ModelFile, email: &str, get: impl Fn(&str) -> String) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let aliases = get("PI_ENSEMBLE_HOST_ALIASES");
    for pair in aliases.split('\n').next().unwrap_or_default().split(',') {
        match pair.split_once(':') {
            Some((alias, address)) => names.extend([alias.to_string(), address.to_string()]),
            None => names.push(pair.to_string()),
        }
    }
    for provider in providers(&get) {
        for end in ["main", "backup"] {
            let url = models.url(&provider, end);
            for line in url.split('\n') {
                names.push(host(line.trim_matches([' ', '\t'])).to_string());
            }
        }
    }
    names.push(email.to_string());
    names.retain(|name| name.chars().count() >= MIN_CHARS);
    names
}

/// The names in the order the mask is given them, from the settings in the
/// environment, the host's models file and the bot's email.
pub fn names(models: &ModelFile, email: &str) -> Vec<String> {
    names_from(models, email, |name| {
        env::var_os(name).map_or(String::new(), |v| v.to_string_lossy().into_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn settings(given: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> String {
        |name| {
            given
                .iter()
                .find(|(k, _)| *k == name)
                .map_or(String::new(), |(_, v)| v.to_string())
        }
    }

    fn file(json: Value) -> ModelFile {
        ModelFile { json }
    }

    #[test]
    fn a_host_is_what_is_left_of_a_url() {
        assert_eq!(host("http://models.lan:8086/v1"), "models.lan");
        assert_eq!(host("https://user:pw@gpu.lan/v1"), "gpu.lan");
        assert_eq!(host("https://a@b@gpu.lan:1/"), "gpu.lan");
        assert_eq!(host("http://[2001:db8::1]:8086/v1"), "2001:db8::1");
        assert_eq!(host("bare.lan"), "bare.lan");
        assert_eq!(host(""), "");
    }

    #[test]
    fn a_pair_needs_both_words_and_counts_once() {
        let got = providers(settings(&[
            ("PI_ENSEMBLE_SUBAGENT_PROVIDER", "one"),
            ("PI_ENSEMBLE_SUBAGENT_MODEL", "small"),
            ("PI_ENSEMBLE_MODEL_DEVELOPER", "big"),
            ("PI_ENSEMBLE_PROVIDER_DEVELOPER", "one"),
            ("PI_ENSEMBLE_MODEL_OPS", "small"),
            ("PI_ENSEMBLE_PROVIDER_OPS", "one"),
            ("PI_ENSEMBLE_MODEL_EXPLORE", "alone"),
            ("PI_ENSEMBLE_PROVIDER_EXPLORE", ""),
            ("PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST", "unused"),
        ]));
        assert_eq!(got, ["one", "one"]);
    }

    #[test]
    fn names_come_in_order_and_short_ones_go() {
        let models = file(json!({"providers": {
            "one": {"baseUrl": "http://main.lan/v1", "backupBaseUrl": "http://back.lan:9/v1"},
        }}));
        let got = names_from(
            &models,
            "bot@example.com",
            settings(&[
                (
                    "PI_ENSEMBLE_HOST_ALIASES",
                    "gb:192.0.2.10,,v6host:2001:db8::1",
                ),
                ("PI_ENSEMBLE_SUBAGENT_PROVIDER", "one"),
                ("PI_ENSEMBLE_SUBAGENT_MODEL", "small"),
            ]),
        );
        assert_eq!(
            got,
            [
                "192.0.2.10",
                "v6host",
                "2001:db8::1",
                "main.lan",
                "back.lan",
                "bot@example.com"
            ]
        );
    }

    #[test]
    fn no_settings_and_no_file_is_no_names() {
        assert!(names_from(&file(Value::Null), "", settings(&[])).is_empty());
    }
}
