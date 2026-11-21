//! `tehas plugin`: the plugins of this factory, console/src/usage/plugin.txt.
//! Plugins are a feature still being built: with it off the command says so
//! and exits 78. `tehas plugin` reads the settings and what `tehas update`
//! left in `~/tehas/plugins/<name>/state.json`; `tehas plugin review`
//! fetches a commit and reads it for a person to grant.

mod exec;
mod prepare;
mod review;
mod skills;
pub mod update;

use std::env;
use std::path::{Path, PathBuf};
use tehas_core::jq::{obj, J};
use tehas_core::plugin::state::{State, Word};
use tehas_core::plugin::{self, Pin};
use tehas_core::{exit, features, outln};

const USAGE: &str = "usage: tehas plugin [--json] | tehas plugin review <name> [<commit>] [--json] | tehas plugin skills [--chat obeya|wizard|assistant] [--json]";

/// What the arguments ask for.
#[derive(Debug, PartialEq, Eq)]
pub enum Words {
    List {
        json: bool,
    },
    Review {
        name: String,
        commit: Option<String>,
        json: bool,
    },
    Skills {
        chat: Option<String>,
        json: bool,
    },
}

/// What the arguments ask for, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Words, u8> {
    let usage = || {
        eprintln!("{USAGE}");
        exit::USAGE
    };
    let (json, rest): (Vec<&String>, Vec<&String>) = given.iter().partition(|w| *w == "--json");
    if json.len() > 1 {
        return Err(usage());
    }
    let json = json.len() == 1;
    match rest.as_slice() {
        [] => Ok(Words::List { json }),
        [verb, name] if *verb == "review" && plugin::is_name(name) => Ok(Words::Review {
            name: name.to_string(),
            commit: None,
            json,
        }),
        [verb, name, commit]
            if *verb == "review"
                && plugin::is_name(name)
                && tehas_core::grammar::commit_id(commit) =>
        {
            Ok(Words::Review {
                name: name.to_string(),
                commit: Some(commit.to_string()),
                json,
            })
        }
        [verb] if *verb == "skills" => Ok(Words::Skills { chat: None, json }),
        [verb, flag, chat] if *verb == "skills" && *flag == "--chat" && skills::is_chat(chat) => {
            Ok(Words::Skills {
                chat: Some(chat.to_string()),
                json,
            })
        }
        _ => Err(usage()),
    }
}

/// One plugin as the command shows it.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub pin: Pin,
    /// What its grants line grants at the pin: nothing when the line is for
    /// another commit or there is none.
    pub grants: Vec<String>,
    /// What `tehas update` left for the pin's commit.
    pub state: State,
}

/// The plugins of the settings `get` gives, which the load has checked, and
/// the state `state` reads of each.
pub fn rows(get: &dyn Fn(&str) -> String, state: &dyn Fn(&str) -> Option<State>) -> Vec<Row> {
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    pins.into_iter()
        .map(|pin| {
            let line = get(&plugin::grants_setting(&pin.name));
            let grants = plugin::grants(&line).ok();
            match grants {
                Some(g) if g.commit != pin.commit => Row {
                    state: State::refused(&pin.commit, "the grants are for another commit"),
                    pin,
                    grants: Vec::new(),
                },
                g => Row {
                    state: state(&pin.name)
                        .filter(|s| s.commit == pin.commit)
                        .unwrap_or_else(|| State::new(Word::Unbuilt, &pin.commit)),
                    pin,
                    grants: g.map(|g| g.words).unwrap_or_default(),
                },
            }
        })
        .collect()
}

fn text_or_null(s: &Option<String>) -> J {
    s.as_deref().map_or(J::Null, J::str)
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
                ("state", J::str(r.state.state.word())),
                ("refusal", text_or_null(&r.state.refusal)),
                ("image", text_or_null(&r.state.image)),
                ("manifest_sha256", text_or_null(&r.state.manifest_sha256)),
                (
                    "bases",
                    J::Arr(r.state.bases.iter().map(|b| J::str(b.as_str())).collect()),
                ),
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
            let state = match &r.state.refusal {
                Some(why) => format!("{}: {why}", r.state.state.word()),
                None => r.state.state.word().to_string(),
            };
            format!(
                "{}\t{}@{}\t{state}\tgrants: {grants}",
                r.pin.name, r.pin.url, r.pin.commit
            )
        })
        .collect()
}

fn home() -> Option<PathBuf> {
    env::var_os("TEHAS_HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The host's own names, for whether the model's endpoint is on it.
fn host_names() -> Vec<String> {
    let mut out = Vec::new();
    for file in ["/proc/sys/kernel/hostname", "/etc/hostname"] {
        if let Ok(n) = std::fs::read_to_string(file) {
            let n = n.trim().to_ascii_lowercase();
            if !n.is_empty() && !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

fn review_main(
    home: &Path,
    get: &dyn Fn(&str) -> String,
    name: &str,
    commit: Option<&str>,
    json_out: bool,
) -> u8 {
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    let Some(pin) = pins.into_iter().find(|p| p.name == name) else {
        eprintln!("tehas plugin review: TEHAS_PLUGINS names no plugin {name}");
        return exit::USAGE;
    };
    let commit = commit.unwrap_or(&pin.commit).to_string();
    let models = tehas_core::models::ModelFile::read(&tehas_core::models::host_path());
    let names = host_names();
    let given = review::Given {
        home,
        get,
        models: &models,
        names: &names,
    };
    let r = match review::review(&mut exec::Host, &given, &pin, &commit) {
        Ok(r) => r,
        Err(why) => {
            eprintln!("tehas plugin review: {why}");
            return exit::FAILED;
        }
    };
    if json_out {
        outln!("{}", r.json());
    } else {
        for line in r.text() {
            outln!("{line}");
        }
    }
    if r.refusals().is_empty() {
        exit::OK
    } else {
        exit::NO
    }
}

pub fn main(given: &[String]) -> u8 {
    let asked = match words(given) {
        Ok(w) => w,
        Err(rc) => return rc,
    };
    let get = |name: &str| env::var(name).unwrap_or_default();
    let what = match asked {
        Words::List { .. } => "tehas plugin",
        Words::Review { .. } => "tehas plugin review",
        Words::Skills { .. } => "tehas plugin skills",
    };
    if let Err(why) = features::needs("plugins", what, &get) {
        eprintln!("{what}: {why}");
        return exit::SETTINGS;
    }
    let Some(home) = home() else {
        eprintln!("{what}: TEHAS_HOME is not set");
        return exit::SETTINGS;
    };
    match asked {
        Words::List { json: json_out } => {
            let rows = rows(&get, &|name| prepare::read_state(&home, name));
            if json_out {
                outln!("{}", json(&rows).dump());
            } else {
                for line in text(&rows) {
                    outln!("{line}");
                }
            }
            exit::OK
        }
        Words::Review { name, commit, json } => {
            review_main(&home, &get, &name, commit.as_deref(), json)
        }
        Words::Skills { chat, json } => {
            let l = skills::list(&mut exec::Host, &home, &get, chat.as_deref());
            if json {
                outln!("{}", l.json().dump());
                for (name, why) in &l.left_out {
                    eprintln!("tehas plugin skills: left out {name}: {why}");
                }
            } else {
                for line in l.text() {
                    outln!("{line}");
                }
            }
            exit::OK
        }
    }
}

/// The plugins' step of `tehas update`, on the host.
pub fn update_step(home: &Path) -> update::Said {
    let get = |name: &str| env::var(name).unwrap_or_default();
    update::step(&mut exec::Host, home, &get)
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

    fn none(_: &str) -> Option<State> {
        None
    }

    fn words_of(given: &[&str]) -> Result<Words, u8> {
        words(&given.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn the_words_are_a_list_or_a_review() {
        assert_eq!(words_of(&[]), Ok(Words::List { json: false }));
        assert_eq!(words_of(&["--json"]), Ok(Words::List { json: true }));
        assert_eq!(words_of(&["--json", "--json"]), Err(64));
        assert_eq!(
            words_of(&["review", "floorplan"]),
            Ok(Words::Review {
                name: "floorplan".into(),
                commit: None,
                json: false
            })
        );
        assert_eq!(
            words_of(&["review", "floorplan", PIN, "--json"]),
            Ok(Words::Review {
                name: "floorplan".into(),
                commit: Some(PIN.into()),
                json: true
            })
        );
        assert_eq!(words_of(&["review"]), Err(64));
        assert_eq!(words_of(&["review", "Floor"]), Err(64));
        assert_eq!(words_of(&["review", "floorplan", "0123abc"]), Err(64));
        assert_eq!(words_of(&["review", "floorplan", PIN, "x"]), Err(64));
        assert_eq!(words_of(&["run", "floorplan"]), Err(64));
    }

    #[test]
    fn the_skills_words_take_one_chat_or_none() {
        assert_eq!(
            words_of(&["skills"]),
            Ok(Words::Skills {
                chat: None,
                json: false
            })
        );
        assert_eq!(
            words_of(&["skills", "--chat", "wizard", "--json"]),
            Ok(Words::Skills {
                chat: Some("wizard".into()),
                json: true
            })
        );
        assert_eq!(words_of(&["skills", "--chat", "slack"]), Err(64));
        assert_eq!(words_of(&["skills", "--chat"]), Err(64));
        assert_eq!(words_of(&["skills", "floorplan"]), Err(64));
    }

    #[test]
    fn no_plugin_is_the_empty_sample() {
        let rows = rows(&vars(&[]), &none);
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
        let rows = rows(&get, &none);
        assert_eq!(json(&rows), sample("sample"));
        assert_eq!(
            text(&rows),
            [format!("floorplan\t{URL}@{PIN}\tunbuilt\tgrants: view product model:2000000 action:refresh hook:after:work")]
        );
    }

    #[test]
    fn the_state_update_left_for_the_pin_is_shown() {
        let line = format!("{PIN} view product model:2000000 action:refresh hook:after:work");
        let get = vars(&[
            ("TEHAS_PLUGINS", &floorplan()),
            ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &line),
        ]);
        let ready = |_: &str| {
            let mut s = State::new(Word::Ready, PIN);
            s.image = Some(format!("sha256:{}", "b".repeat(64)));
            s.manifest_sha256 = Some("c".repeat(64));
            s.bases = vec![format!("node@sha256:{}", "a".repeat(64))];
            Some(s)
        };
        assert_eq!(json(&rows(&get, &ready)), sample("ready"));
        let refused = |_: &str| Some(State::refused(PIN, "view is asked and not granted"));
        assert_eq!(
            text(&rows(&get, &refused))[0],
            format!("floorplan\t{URL}@{PIN}\trefused: view is asked and not granted\tgrants: view product model:2000000 action:refresh hook:after:work")
        );
        let old = |_: &str| Some(State::new(Word::Ready, &"f".repeat(40)));
        assert_eq!(rows(&get, &old)[0].state.state, Word::Unbuilt);
    }

    #[test]
    fn grants_for_another_commit_refuse_the_plugin() {
        let other = format!("{} view", "f".repeat(40));
        let get = vars(&[
            ("TEHAS_PLUGINS", &floorplan()),
            ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &other),
        ]);
        let ready = |_: &str| Some(State::new(Word::Ready, PIN));
        let rows = rows(&get, &ready);
        assert_eq!(rows[0].state.state, Word::Refused);
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
        let rows = rows(&vars(&[("TEHAS_PLUGINS", &floorplan())]), &none);
        assert_eq!(rows[0].state.state, Word::Unbuilt);
        assert!(rows[0].grants.is_empty());
    }
}
