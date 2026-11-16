//! What a cycle ran with: the settings the store keeps for each attempt, and
//! the analytics read back with its run.

use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;
use tehas_core::jq::{obj, J};

/// Whether the value of a setting may be kept with a cycle's run, which the
/// analytics show. Only a value of a safe shape goes in: a number, a boolean, or a short
/// word. Everything else is withheld, and so is any value whose name says it may
/// be a secret or a host.
fn safe_value(name: &str, v: &str) -> String {
    const NAMES: [&str; 16] = [
        "KEY", "TOKEN", "SECRET", "PASS", "PWD", "COOKIE", "AUTH", "CRED", "BEARER", "PRIVATE",
        "HOST", "URL", "ADDR", "ENDPOINT", "DOMAIN", "PROXY",
    ];
    static SHAPES: OnceLock<[Regex; 5]> = OnceLock::new();
    let [word, address, long, jwt, dotted] = SHAPES.get_or_init(|| {
        [
            Regex::new(r"\A[A-Za-z0-9_.,:=-]{0,64}\z").unwrap(),
            Regex::new(r"\A[0-9]{1,3}(\.[0-9]{1,3}){3}(:[0-9]+)?\z|:[0-9]+\z").unwrap(),
            Regex::new(r"[A-Za-z0-9]{16,}").unwrap(),
            Regex::new(r"(\A|[^A-Za-z0-9])eyJ").unwrap(),
            Regex::new(r"[A-Za-z0-9-]\.[A-Za-z]").unwrap(),
        ]
    });
    let safe = !NAMES.iter().any(|n| name.contains(n))
        && word.is_match(v)
        && !address.is_match(v)
        && !long.is_match(v)
        && !jwt.is_match(v)
        && !dotted.is_match(v)
        && !tehas_core::secret::secret(v.as_bytes());
    if safe {
        v.to_string()
    } else {
        "[withheld]".to_string()
    }
}

/// What a cycle ran with: the PI_ENSEMBLE_* variables the -e options give the
/// container, the instance's TEHAS_* settings that shape a cycle, and how many
/// cycles were running. Not every setting: a host's addresses and the
/// analytics settings are none of a run's business.
pub fn settings(running: &str, options: &[String]) -> String {
    settings_with(
        running,
        options,
        &std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.to_string_lossy().into_owned())))
            .collect::<Vec<_>>(),
    )
}

/// The settings of a container from its -e options, and of a host from the
/// variables it was given, which a test holds fixed.
fn settings_with(running: &str, options: &[String], host: &[(String, String)]) -> String {
    let mut container: Vec<(String, J)> = vec![];
    let mut words = options.iter();
    while let Some(word) = words.next() {
        if word != "-e" {
            continue;
        }
        let Some(a) = words.next() else { break };
        let (name, value) = match a.split_once('=') {
            Some((n, v)) => (n, v.to_string()),
            None => (a.as_str(), std::env::var(a).unwrap_or_default()),
        };
        let shaped = name.strip_prefix("PI_ENSEMBLE_").is_some_and(|r| {
            !r.is_empty()
                && r.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        });
        if !shaped {
            continue;
        }
        let value = J::Str(safe_value(name, &value));
        match container.iter_mut().find(|(k, _)| k == name) {
            Some(slot) => slot.1 = value,
            None => container.push((name.to_string(), value)),
        }
    }
    let shape = host_shape(host);
    let count = running.len() < 6
        && !running.is_empty()
        && running.bytes().all(|b| b.is_ascii_digit())
        && (running == "0" || !running.starts_with('0'));
    obj(vec![
        ("container", J::Obj(container)),
        ("host", J::Obj(shape)),
        (
            "concurrentCycles",
            if count {
                J::Num(running.to_string())
            } else {
                J::Null
            },
        ),
    ])
    .pretty()
}

/// The TEHAS_* settings that shape a cycle, in their own order.
fn host_shape(given: &[(String, String)]) -> Vec<(String, J)> {
    static HOST: OnceLock<Regex> = OnceLock::new();
    let host = HOST.get_or_init(|| {
        Regex::new(
            r"\ATEHAS_(WORK_(TIMEOUT|STALL|CPUS|MEMORY|IMAGES|FILES|COMMENTS|DESCRIBE)|CONCURRENCY|TICKET_CONCURRENCY|SLICE_CONTAINERS|MAX_AWAITING|CYCLE_RESERVE|VERIFY_TIMEOUT|CHILD_TIMEOUT|PREFLIGHT_WAIT|STOP_WAIT|CONSOLE_WAIT|BRANCH)\z",
        )
        .unwrap()
    });
    let mut shape: Vec<(String, String)> = given
        .iter()
        .filter(|(k, _)| host.is_match(k))
        .cloned()
        .collect();
    shape.sort();
    shape
        .into_iter()
        .map(|(k, v)| {
            let value = J::Str(safe_value(&k, &v));
            (k, value)
        })
        .collect()
}

/// Container, host and concurrentCycles of a settings-view.
pub fn ran_with(view: &Value) -> (J, J, J) {
    let part = |k: &str| match view.get(k) {
        Some(v) => J::from(v),
        None => J::Null,
    };
    (part("container"), part("host"), part("concurrentCycles"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a cycle ran with, for fixed running, options and
    /// variables of the host: no setting of the shell this test runs in is
    /// read.
    #[test]
    fn the_settings_hold_the_container_the_host_and_the_count() {
        let options = [
            "-e".to_string(),
            "PI_ENSEMBLE_SUBAGENT_MODEL=m1".to_string(),
            "-e".to_string(),
            "PI_ENSEMBLE_HOME_URL=secret".to_string(),
            "-e".to_string(),
            "PI_ENSEMBLE_NOT_A_SETTING=x".to_string(),
            "--rm".to_string(),
        ];
        let host = [
            ("TEHAS_WORK_TIMEOUT".to_string(), "600".to_string()),
            ("TEHAS_WORK_STALL".to_string(), "0".to_string()),
            ("TEHAS_SLICE_CONTAINERS".to_string(), "4".to_string()),
            ("TEHAS_GITHUB_TOKEN".to_string(), "gh_secret".to_string()),
            ("PATH".to_string(), "/bin".to_string()),
        ];
        assert_eq!(
            settings_with("3", &options, &host),
            concat!(
                "{\n",
                "  \"container\": {\n",
                "    \"PI_ENSEMBLE_SUBAGENT_MODEL\": \"m1\",\n",
                "    \"PI_ENSEMBLE_HOME_URL\": \"[withheld]\",\n",
                "    \"PI_ENSEMBLE_NOT_A_SETTING\": \"x\"\n",
                "  },\n",
                "  \"host\": {\n",
                "    \"TEHAS_SLICE_CONTAINERS\": \"4\",\n",
                "    \"TEHAS_WORK_STALL\": \"0\",\n",
                "    \"TEHAS_WORK_TIMEOUT\": \"600\"\n",
                "  },\n",
                "  \"concurrentCycles\": 3\n",
                "}",
            )
        );
        // A count that is no number of cycles is null, and no -e option is no
        // container.
        assert_eq!(
            settings_with("03", &[], &[]),
            concat!(
                "{\n",
                "  \"container\": {},\n",
                "  \"host\": {},\n",
                "  \"concurrentCycles\": null\n",
                "}",
            )
        );
    }
}
