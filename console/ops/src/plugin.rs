//! `tehas plugin`: the plugins of this factory, console/src/usage/plugin.txt.
//! Plugins are a feature still being built: with it off the command says so
//! and exits 78. `tehas plugin` reads the settings and what `tehas update`
//! left in `~/tehas/plugins/<name>/state.json`; `tehas plugin review`
//! fetches a commit and reads it for a person to grant. `run` and `hook` ask
//! for a run, which a unit makes (runs.rs); `log` prints the last one's log.
//! `call` runs one call of a tool, attached (call.rs).

mod ask;
pub mod call;
mod check;
pub mod exec;
pub mod prepare;
mod ready;
mod review;
pub mod runs;
mod skills;
pub mod update;

use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use tehas_core::jq::{obj, J};
use tehas_core::plugin::state::{State, Word};
use tehas_core::plugin::{self, Pin};
use tehas_core::{exit, features, outln};

const USAGE: &str = "usage: tehas plugin [--json] | tehas plugin review <name> [<commit>] [--json] | tehas plugin skills [--chat obeya|wizard|assistant] [--json] | tehas plugin run <name> | tehas plugin hook <name> <pr> | tehas plugin log <name> [--hook <pr>] | tehas plugin check <dir> [--json] | tehas plugin call <name> <tool> --args <json> [--login <login>] [--json]";

/// What the arguments ask for.
#[derive(Debug, PartialEq)]
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
    Run {
        name: String,
    },
    Hook {
        name: String,
        pr: u64,
    },
    Log {
        name: String,
        hook: Option<u64>,
    },
    Check {
        dir: String,
        json: bool,
    },
    Call {
        name: String,
        tool: String,
        args: Value,
        login: Option<String>,
        json: bool,
    },
}

fn pr_number(s: &str) -> Option<u64> {
    (s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit()) && !s.starts_with('0'))
        .then(|| s.parse().ok())
        .flatten()
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
        [verb, name] if !json && *verb == "run" && plugin::is_name(name) => Ok(Words::Run {
            name: name.to_string(),
        }),
        [verb, name, pr] if !json && *verb == "hook" && plugin::is_name(name) => {
            match pr_number(pr) {
                Some(pr) => Ok(Words::Hook {
                    name: name.to_string(),
                    pr,
                }),
                None => Err(usage()),
            }
        }
        [verb, name] if !json && *verb == "log" && plugin::is_name(name) => Ok(Words::Log {
            name: name.to_string(),
            hook: None,
        }),
        [verb, name, tool, flag, args, more @ ..]
            if *verb == "call" && *flag == "--args" && plugin::is_name(name) =>
        {
            let login = match more {
                [] => None,
                [f, l] if *f == "--login" && tehas_core::settings::rules::is_login(l) => {
                    Some(l.to_string())
                }
                _ => return Err(usage()),
            };
            match serde_json::from_str(args) {
                Ok(args) => Ok(Words::Call {
                    name: name.to_string(),
                    tool: tool.to_string(),
                    args,
                    login,
                    json,
                }),
                Err(_) => Err(usage()),
            }
        }
        [verb, name, flag, pr]
            if !json && *verb == "log" && *flag == "--hook" && plugin::is_name(name) =>
        {
            match pr_number(pr) {
                Some(pr) => Ok(Words::Log {
                    name: name.to_string(),
                    hook: Some(pr),
                }),
                None => Err(usage()),
            }
        }
        [verb, dir] if *verb == "check" && !dir.is_empty() => Ok(Words::Check {
            dir: dir.to_string(),
            json,
        }),
        _ => Err(usage()),
    }
}

/// `tehas plugin check <dir>`: no settings, no home, no feature switch, so a
/// plugin's author runs it anywhere tehas unpacks. 0 kept; 3 refused, a
/// folder with no manifest that can be read among them.
fn check_main(dir: &str, json_out: bool) -> u8 {
    let c = match check::check(std::path::Path::new(dir)) {
        Ok(c) => c,
        Err(why) => {
            eprintln!("tehas plugin check: {why}");
            return exit::NO;
        }
    };
    if json_out {
        outln!("{}", c.json());
    } else {
        for line in c.text() {
            outln!("{line}");
        }
    }
    if c.refusals().is_empty() {
        exit::OK
    } else {
        exit::NO
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
    /// The newest run in the state store, as `runs::last_and_today` gives it.
    pub last: Option<Value>,
    /// The model tokens of its runs that started today, UTC.
    pub tokens_today: u64,
    /// Why the last hook's commit status could not be written; None when it
    /// was, or none was tried.
    pub status_unwritten: Option<String>,
}

/// What the store's runs of a plugin come to: its last run and today's tokens.
pub type Runs<'a> = &'a dyn Fn(&str) -> (Option<Value>, u64);

/// The plugins of the settings `get` gives, which the load has checked, the
/// state `state` reads of each and what `runs` reads of its runs.
pub fn rows(
    get: &dyn Fn(&str) -> String,
    state: &dyn Fn(&str) -> Option<State>,
    runs: Runs,
) -> Vec<Row> {
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    pins.into_iter()
        .map(|pin| {
            let line = get(&plugin::grants_setting(&pin.name));
            let grants = plugin::grants(&line).ok();
            let (last, tokens_today) = runs(&pin.name);
            match grants {
                Some(g) if g.commit != pin.commit => Row {
                    state: State::refused(&pin.commit, "the grants are for another commit"),
                    pin,
                    grants: Vec::new(),
                    last,
                    tokens_today,
                    status_unwritten: None,
                },
                g => Row {
                    state: state(&pin.name)
                        .filter(|s| s.commit == pin.commit)
                        .unwrap_or_else(|| State::new(Word::Unbuilt, &pin.commit)),
                    pin,
                    grants: g.map(|g| g.words).unwrap_or_default(),
                    last,
                    tokens_today,
                    status_unwritten: None,
                },
            }
        })
        .collect()
}

/// The runs of a plugin in the home's store; none when it cannot be read.
fn store_runs(home: &Path, name: &str) -> (Option<Value>, u64) {
    let since = tehas_core::time::now().div_euclid(86_400) * 86_400;
    match crate::store::call(home, &["plugin-runs", "--plugin", name]) {
        Ok(v) => {
            let runs = v["runs"].as_array().cloned().unwrap_or_default();
            runs::last_and_today(&runs, since)
        }
        Err(_) => (None, 0),
    }
}

fn text_or_null(s: &Option<String>) -> J {
    s.as_deref().map_or(J::Null, J::str)
}

fn last_json(l: &Value) -> J {
    let n = |k: &str| l[k].as_i64().map_or(J::Null, J::int);
    let s = |k: &str| l[k].as_str().map_or(J::Null, J::str);
    obj(vec![
        ("kind", s("kind")),
        ("trigger", s("trigger")),
        ("start", n("start")),
        ("end", n("end")),
        ("result", s("result")),
        ("tokens", n("tokens")),
    ])
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
                ("last_run", r.last.as_ref().map_or(J::Null, last_json)),
                ("tokens_today", J::int(r.tokens_today as i64)),
                ("status_unwritten", text_or_null(&r.status_unwritten)),
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
            let last = match &r.last {
                None => "last run: none".to_string(),
                Some(l) => format!(
                    "last run: {} {}, {}",
                    l["kind"].as_str().unwrap_or("?"),
                    l["trigger"].as_str().unwrap_or("?"),
                    l["result"].as_str().unwrap_or("running")
                ),
            };
            let unwritten = match &r.status_unwritten {
                Some(why) => format!("\tthe status could not be written: {why}"),
                None => String::new(),
            };
            format!(
                "{}\t{}@{}\t{state}\tgrants: {grants}\t{last}\ttokens today: {}{unwritten}",
                r.pin.name, r.pin.url, r.pin.commit, r.tokens_today
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
    // No settings and no feature switch: an author's folder on any machine.
    let asked = match asked {
        Words::Check { dir, json } => return check_main(&dir, json),
        other => other,
    };
    let get = |name: &str| env::var(name).unwrap_or_default();
    let what = match asked {
        Words::List { .. } => "tehas plugin",
        Words::Review { .. } => "tehas plugin review",
        Words::Skills { .. } => "tehas plugin skills",
        Words::Run { .. } => "tehas plugin run",
        Words::Hook { .. } => "tehas plugin hook",
        Words::Log { .. } => "tehas plugin log",
        Words::Check { .. } => return exit::USAGE,
        Words::Call { .. } => "tehas plugin call",
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
            let mut rows = rows(&get, &|name| prepare::read_state(&home, name), &|name| {
                store_runs(&home, name)
            });
            for r in &mut rows {
                r.status_unwritten = runs::status_unwritten(&home, &r.pin.name);
            }
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
        // Answered above, before the settings.
        Words::Check { dir, json } => check_main(&dir, json),
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
        Words::Run { name } => ask::run(&mut exec::Host, &home, &get, &name),
        Words::Hook { name, pr } => {
            let program = crate::forge::gh_program(Some(&home.display().to_string()), &get("PATH"));
            let Some(program) = program else {
                eprintln!("tehas plugin hook: no gh to ask GitHub with");
                return exit::FAILED;
            };
            let mut gh = tehas_core::forge::Bounded::new(program, crate::forge::CALL_BOUND);
            ask::hook(&mut exec::Host, &mut gh, &home, &get, &name, pr)
        }
        Words::Log { name, hook } => ask::log(&home, &get, &name, hook),
        Words::Call {
            name,
            tool,
            args,
            login,
            json,
        } => call_main(&home, &get, &name, &tool, &args, login, json),
    }
}

fn call_main(
    home: &Path,
    get: &dyn Fn(&str) -> String,
    name: &str,
    tool: &str,
    args: &Value,
    login: Option<String>,
    json_out: bool,
) -> u8 {
    let what = "tehas plugin call";
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    if !pins.iter().any(|p| p.name == name) {
        eprintln!("{what}: TEHAS_PLUGINS names no plugin {name}");
        return exit::USAGE;
    }
    let caller = login.map_or(call::Caller::Host, call::Caller::Login);
    match call::call(&mut exec::Host, home, get, name, tool, args, &caller) {
        Ok(answer) => {
            if json_out {
                outln!(r#"{{"api":1,"answer":{answer}}}"#);
            } else {
                outln!("{answer}");
            }
            exit::OK
        }
        Err(call::Failed::Tool(why)) => {
            if json_out {
                outln!(r#"{{"api":1,"error":{}}}"#, Value::String(why));
            } else {
                eprintln!("{what}: {name}/{tool}: {why}");
            }
            exit::FAILED
        }
        Err(call::Failed::Refused(why)) => {
            eprintln!("{what}: {name}/{tool}: refused: {why}");
            exit::NO
        }
        Err(call::Failed::Args(why)) => {
            eprintln!("{what}: {name}/{tool}: the arguments: {why}");
            exit::USAGE
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

    fn never(_: &str) -> (Option<Value>, u64) {
        (None, 0)
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
        assert_eq!(words_of(&["run"]), Err(64));
        assert_eq!(words_of(&["run", "floorplan", "--json"]), Err(64));
    }

    #[test]
    fn the_words_of_run_hook_and_log() {
        let name = || "floorplan".to_string();
        assert_eq!(
            words_of(&["run", "floorplan"]),
            Ok(Words::Run { name: name() })
        );
        assert_eq!(
            words_of(&["hook", "floorplan", "42"]),
            Ok(Words::Hook {
                name: name(),
                pr: 42
            })
        );
        assert_eq!(
            words_of(&["log", "floorplan"]),
            Ok(Words::Log {
                name: name(),
                hook: None
            })
        );
        assert_eq!(
            words_of(&["log", "floorplan", "--hook", "42"]),
            Ok(Words::Log {
                name: name(),
                hook: Some(42)
            })
        );
        for bad in [
            &["hook", "floorplan"][..],
            &["hook", "floorplan", "042"],
            &["hook", "floorplan", "x"],
            &["hook", "floorplan", "0"],
            &["log", "floorplan", "--hook"],
            &["log", "floorplan", "42"],
            &["log", "floorplan", "--json"],
        ] {
            assert_eq!(words_of(bad), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn the_check_words_take_one_folder() {
        assert_eq!(
            words_of(&["check", "plugin-dir"]),
            Ok(Words::Check {
                dir: "plugin-dir".into(),
                json: false
            })
        );
        assert_eq!(
            words_of(&["check", ".", "--json"]),
            Ok(Words::Check {
                dir: ".".into(),
                json: true
            })
        );
        assert_eq!(words_of(&["check"]), Err(64));
        assert_eq!(words_of(&["check", "a", "b"]), Err(64));
    }

    #[test]
    fn the_words_of_a_call_take_its_arguments_as_json_and_a_login() {
        let args = r#"{"q": "install"}"#;
        assert_eq!(
            words_of(&["call", "floorplan", "find-doc", "--args", args, "--json"]),
            Ok(Words::Call {
                name: "floorplan".into(),
                tool: "find-doc".into(),
                args: serde_json::json!({"q": "install"}),
                login: None,
                json: true
            })
        );
        assert_eq!(
            words_of(&[
                "call",
                "floorplan",
                "find-doc",
                "--args",
                "{}",
                "--login",
                "octocat"
            ]),
            Ok(Words::Call {
                name: "floorplan".into(),
                tool: "find-doc".into(),
                args: serde_json::json!({}),
                login: Some("octocat".into()),
                json: false
            })
        );
        for bad in [
            &["call", "floorplan", "find-doc"][..],
            &["call", "floorplan", "find-doc", "--args"],
            &["call", "floorplan", "find-doc", "--args", "{"],
            &["call", "floorplan", "find-doc", "--args", "{} {}"],
            &["call", "Floor", "find-doc", "--args", "{}"],
            &["call", "floorplan", "find-doc", "--args", "{}", "--login"],
            &[
                "call",
                "floorplan",
                "find-doc",
                "--args",
                "{}",
                "--login",
                "-x",
            ],
            &["call", "floorplan", "find-doc", "--args", "{}", "--by", "x"],
        ] {
            assert_eq!(words_of(bad), Err(64), "{bad:?}");
        }
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
        let rows = rows(&vars(&[]), &none, &never);
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
        let rows = rows(&get, &none, &never);
        assert_eq!(json(&rows), sample("sample"));
        assert_eq!(
            text(&rows),
            [format!("floorplan\t{URL}@{PIN}\tunbuilt\tgrants: view product model:2000000 action:refresh hook:after:work\tlast run: none\ttokens today: 0")]
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
        let ran = |_: &str| {
            let last = serde_json::json!({"kind": "refresh", "trigger": "timer", "start": 1763700000, "end": 1763700600, "result": "done", "tokens": 120000});
            (Some(last), 120000)
        };
        assert_eq!(json(&rows(&get, &ready, &ran)), sample("ready"));
        assert!(text(&rows(&get, &ready, &ran))[0]
            .ends_with("\tlast run: refresh timer, done\ttokens today: 120000"));
        let refused = |_: &str| Some(State::refused(PIN, "view is asked and not granted"));
        assert_eq!(
            text(&rows(&get, &refused, &never))[0],
            format!("floorplan\t{URL}@{PIN}\trefused: view is asked and not granted\tgrants: view product model:2000000 action:refresh hook:after:work\tlast run: none\ttokens today: 0")
        );
        let old = |_: &str| Some(State::new(Word::Ready, &"f".repeat(40)));
        assert_eq!(rows(&get, &old, &never)[0].state.state, Word::Unbuilt);
    }

    #[test]
    fn grants_for_another_commit_refuse_the_plugin() {
        let other = format!("{} view", "f".repeat(40));
        let get = vars(&[
            ("TEHAS_PLUGINS", &floorplan()),
            ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &other),
        ]);
        let ready = |_: &str| Some(State::new(Word::Ready, PIN));
        let rows = rows(&get, &ready, &never);
        assert_eq!(rows[0].state.state, Word::Refused);
        assert!(rows[0].grants.is_empty());
        assert_eq!(json(&rows), sample("refused"));
        assert_eq!(
            text(&rows),
            [format!(
                "floorplan\t{URL}@{PIN}\trefused: the grants are for another commit\tgrants: none\tlast run: none\ttokens today: 0"
            )]
        );
    }

    #[test]
    fn a_plugin_with_no_grants_line_grants_nothing() {
        let rows = rows(&vars(&[("TEHAS_PLUGINS", &floorplan())]), &none, &never);
        assert_eq!(rows[0].state.state, Word::Unbuilt);
        assert!(rows[0].grants.is_empty());
    }

    #[test]
    fn a_status_the_hook_could_not_write_is_said() {
        let mut rows = rows(&vars(&[("TEHAS_PLUGINS", &floorplan())]), &none, &never);
        assert_eq!(rows[0].status_unwritten, None);
        rows[0].status_unwritten = Some("GitHub refused it: HTTP 403".into());
        let v: Value = serde_json::from_str(&json(&rows).dump()).unwrap();
        assert_eq!(
            v["plugins"][0]["status_unwritten"],
            "GitHub refused it: HTTP 403"
        );
        assert!(text(&rows)[0]
            .ends_with("\tthe status could not be written: GitHub refused it: HTTP 403"));
    }
}
