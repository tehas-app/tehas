//! `tehas plugin`: the plugins of this factory, console/src/usage/plugin.txt.
//! Plugins are a feature still being built: with it off the command says so
//! and exits 78. On, it reads the settings and nothing else. Until the fetch
//! and the build are there, a plugin whose grants are for its pin is
//! `unbuilt`.

use std::env;
use tehas_core::jq::{obj, J};
use tehas_core::plugin::{self, Pin};
use tehas_core::{exit, features, outln};

const USAGE: &str = "usage: tehas plugin [--json]";

/// Whether the arguments ask for `--json`, or 64 with the usage.
pub fn words(given: &[String]) -> Result<bool, u8> {
    match given {
        [] => Ok(false),
        [flag] if flag == "--json" => Ok(true),
        _ => {
            eprintln!("{USAGE}");
            Err(exit::USAGE)
        }
    }
}

/// One plugin as the command shows it.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub pin: Pin,
    /// What its grants line grants at the pin: nothing when the line is for
    /// another commit or there is none.
    pub grants: Vec<String>,
    pub state: &'static str,
    pub refusal: Option<String>,
}

/// The plugins of the settings `get` gives, which the load has checked.
pub fn rows(get: &dyn Fn(&str) -> String) -> Vec<Row> {
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    pins.into_iter()
        .map(|pin| {
            let line = get(&plugin::grants_setting(&pin.name));
            let grants = plugin::grants(&line).ok();
            match grants {
                Some(g) if g.commit != pin.commit => Row {
                    pin,
                    grants: Vec::new(),
                    state: "refused",
                    refusal: Some("the grants are for another commit".to_string()),
                },
                g => Row {
                    pin,
                    grants: g.map(|g| g.words).unwrap_or_default(),
                    state: "unbuilt",
                    refusal: None,
                },
            }
        })
        .collect()
}

/// The object of `--json`, api/plugin.schema.json.
pub fn json(rows: &[Row]) -> J {
    let plugins = rows
        .iter()
        .map(|r| {
            obj(vec![
                ("name", J::str(&r.pin.name)),
                ("url", J::str(&r.pin.url)),
                ("commit", J::str(&r.pin.commit)),
                (
                    "grants",
                    J::Arr(r.grants.iter().map(|g| J::str(g.as_str())).collect()),
                ),
                ("state", J::str(r.state)),
                ("refusal", r.refusal.as_deref().map_or(J::Null, J::str)),
            ])
        })
        .collect();
    obj(vec![("api", J::int(1)), ("plugins", J::Arr(plugins))])
}

/// The lines a person reads.
pub fn text(rows: &[Row]) -> Vec<String> {
    if rows.is_empty() {
        return vec!["no plugins: TEHAS_PLUGINS names none".to_string()];
    }
    rows.iter()
        .map(|r| {
            let grants = if r.grants.is_empty() {
                "none".to_string()
            } else {
                r.grants.join(" ")
            };
            let state = match &r.refusal {
                Some(why) => format!("{}: {why}", r.state),
                None => r.state.to_string(),
            };
            format!(
                "{}\t{}@{}\t{state}\tgrants: {grants}",
                r.pin.name, r.pin.url, r.pin.commit
            )
        })
        .collect()
}

pub fn main(given: &[String]) -> u8 {
    let json_out = match words(given) {
        Ok(j) => j,
        Err(rc) => return rc,
    };
    let get = |name: &str| env::var(name).unwrap_or_default();
    if let Err(why) = features::needs("plugins", "tehas plugin", &get) {
        eprintln!("tehas plugin: {why}");
        return exit::SETTINGS;
    }
    let rows = rows(&get);
    if json_out {
        outln!("{}", json(&rows).dump());
    } else {
        for line in text(&rows) {
            outln!("{line}");
        }
    }
    exit::OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const PIN: &str = "0123456789abcdef0123456789abcdef01234567";
    const URL: &str = "https://github.com/tehas-app/tehas-plugin-floorplan";

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| map.get(name).cloned().unwrap_or_default()
    }

    fn sample(name: &str) -> J {
        let file = format!(
            "{}/../../api/plugin.{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        J::parse(&std::fs::read_to_string(&file).unwrap()).unwrap()
    }

    fn floorplan() -> String {
        format!("floorplan={URL}@{PIN}")
    }

    #[test]
    fn the_words_are_none_or_json() {
        let w = |words: &[&str]| words_of(words);
        assert_eq!(w(&[]), Ok(false));
        assert_eq!(w(&["--json"]), Ok(true));
        assert_eq!(w(&["review"]), Err(64));
        assert_eq!(w(&["--json", "--json"]), Err(64));
    }

    fn words_of(given: &[&str]) -> Result<bool, u8> {
        words(&given.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn no_plugin_is_the_empty_sample() {
        let rows = rows(&vars(&[]));
        assert_eq!(json(&rows), sample("empty"));
        assert_eq!(text(&rows), ["no plugins: TEHAS_PLUGINS names none"]);
    }

    #[test]
    fn a_plugin_granted_at_its_pin_is_unbuilt_with_its_grants() {
        let line = format!("{PIN} view product model:2000000 action:refresh hook:after:work");
        let get = vars(&[
            ("TEHAS_PLUGINS", &floorplan()),
            ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &line),
        ]);
        let rows = rows(&get);
        assert_eq!(json(&rows), sample("sample"));
        assert_eq!(
            text(&rows),
            [format!("floorplan\t{URL}@{PIN}\tunbuilt\tgrants: view product model:2000000 action:refresh hook:after:work")]
        );
    }

    #[test]
    fn grants_for_another_commit_refuse_the_plugin() {
        let other = format!("{} view", "f".repeat(40));
        let get = vars(&[
            ("TEHAS_PLUGINS", &floorplan()),
            ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &other),
        ]);
        let rows = rows(&get);
        assert_eq!(rows[0].state, "refused");
        assert!(rows[0].grants.is_empty());
        assert_eq!(json(&rows), sample("refused"));
        assert_eq!(
            text(&rows),
            [format!(
                "floorplan\t{URL}@{PIN}\trefused: the grants are for another commit\tgrants: none"
            )]
        );
    }

    #[test]
    fn a_plugin_with_no_grants_line_grants_nothing() {
        let rows = rows(&vars(&[("TEHAS_PLUGINS", &floorplan())]));
        assert_eq!(rows[0].state, "unbuilt");
        assert!(rows[0].grants.is_empty());
    }
}
