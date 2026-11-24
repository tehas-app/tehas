//! The hook after work (docs/specs/2026-11-21-plugins-hooks.md): its request,
//! which a cycle and `tehas plugin hook` write and the runner reads, and which
//! plugins are asked for one. Written once here, since the cycle's
//! supervisor and the console's operations both write the request. Which
//! ends ask for a hook is the event's rule: a cycle asks on `change.offered`
//! (docs/specs/2026-11-22-events.md, "Plugins").

use super::run::Trigger;
use super::state::{State, Word};
use super::Cap;
use crate::accounts::factorys;
use crate::file::{write_atomic, Temp};
use crate::grammar::commit_id;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tehas_ports::events::{Envelope, Name, When};

/// `~/tehas/plugins/<name>/hooks`.
pub fn dir(home: &Path, name: &str) -> PathBuf {
    home.join("plugins").join(name).join("hooks")
}

/// `hooks/<pr>.json`: the request of a hook.
pub fn request(home: &Path, name: &str, pr: u64) -> PathBuf {
    dir(home, name).join(format!("{pr}.json"))
}

/// The unit of the hook of `name` on `pr`.
pub fn unit(name: &str, pr: u64) -> String {
    format!("tehas-plugin-hook@{name}-{pr}.service")
}

/// The request of a hook: the pull request, its head as GitHub gave it, its
/// base, the issue, what asked, and the envelope of the event that asked
/// (api/event.schema.json): `change.offered`, the one event a hook is asked
/// on. A person's run has no event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookAsk {
    pub pr: u64,
    pub head: String,
    pub base: String,
    pub issue: Option<u64>,
    pub trigger: Trigger,
    pub event: Option<Envelope>,
}

/// The request file as it is written.
#[derive(serde::Serialize)]
struct AskFile<'a> {
    api: u8,
    pr: u64,
    head: &'a str,
    base: &'a str,
    issue: Option<u64>,
    trigger: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<&'a Envelope>,
}

impl HookAsk {
    /// The request file's text.
    pub fn json(&self) -> String {
        let file = AskFile {
            api: 1,
            pr: self.pr,
            head: &self.head,
            base: &self.base,
            issue: self.issue,
            trigger: self.trigger.word(),
            event: self.event.as_ref(),
        };
        serde_json::to_string(&file).unwrap_or_default()
    }

    pub fn read(text: &[u8]) -> Option<HookAsk> {
        let v: Value = serde_json::from_slice(text).ok()?;
        if v.get("api")? != 1 {
            return None;
        }
        let commit = |k: &str| {
            v.get(k)?
                .as_str()
                .filter(|c| commit_id(c))
                .map(str::to_string)
        };
        let issue = match v.get("issue")? {
            Value::Null => None,
            n => Some(n.as_u64().filter(|n| *n > 0)?),
        };
        // Only a cycle and a person ask for a hook.
        let trigger = match v.get("trigger")?.as_str()? {
            "cycle" => Trigger::Cycle,
            "host" => Trigger::Host,
            _ => return None,
        };
        // An event is read as its envelope, and only the one a hook is
        // asked on: anything else refuses the request.
        let event = match v.get("event") {
            None => None,
            Some(e) => Some(
                serde_json::from_value::<Envelope>(e.clone())
                    .ok()
                    .filter(asks)?,
            ),
        };
        Some(HookAsk {
            pr: v.get("pr")?.as_u64().filter(|n| *n > 0)?,
            head: commit("head")?,
            base: commit("base")?,
            issue,
            trigger,
            event,
        })
    }
}

/// Whether `e` is an event a hook is asked on: `change.offered`, after the
/// fact.
pub fn asks(e: &Envelope) -> bool {
    e.name() == Name::ChangeOffered && e.when == When::After
}

pub fn read_request(home: &Path, name: &str, pr: u64) -> Option<HookAsk> {
    HookAsk::read(&std::fs::read(request(home, name, pr)).ok()?).filter(|h| h.pr == pr)
}

/// Writes the request, replacing one there. The caller starts the unit.
pub fn write_request(home: &Path, name: &str, ask: &HookAsk) -> Result<(), String> {
    let d = dir(home, name);
    std::fs::create_dir_all(&d).map_err(|e| format!("cannot make {}: {e}", d.display()))?;
    write_atomic(
        &request(home, name, ask.pr),
        format!("{}\n", ask.json()).as_bytes(),
        0o600,
        &Temp::Pid,
    )
    .map_err(|e| format!("cannot write the request: {e}"))
}

/// What `tehas update` left in `plugins/<name>/state.json`.
pub fn read_state(home: &Path, name: &str) -> Option<State> {
    let file = home.join("plugins").join(name).join("state.json");
    State::read(&std::fs::read_to_string(file).ok()?)
}

/// The pull request as GitHub gives it, when it is open, the bot's and of
/// the repository itself: its head and its base.
pub fn open_bot_pull(pull: &Value, repo: &str, bot: &str) -> Option<(String, String)> {
    let s = |ptr: &str| pull.pointer(ptr).and_then(Value::as_str).unwrap_or("");
    let ok = s("/state") == "open"
        && pull.get("merged").and_then(Value::as_bool) != Some(true)
        && factorys(s("/user/login"), bot)
        && s("/head/repo/full_name").eq_ignore_ascii_case(repo)
        && commit_id(s("/head/sha"))
        && commit_id(s("/base/sha"));
    ok.then(|| (s("/head/sha").to_string(), s("/base/sha").to_string()))
}

/// The plugins asked for a hook, in the order of TEHAS_PLUGINS: none while
/// plugins are off. A plugin is asked when it is granted `hook:after:change.offered`
/// at its pin and ready there. `state` reads a plugin's state.json.
pub fn granted(get: &dyn Fn(&str) -> String, state: &dyn Fn(&str) -> Option<State>) -> Vec<String> {
    if !crate::features::on_in("plugins", get) {
        return Vec::new();
    }
    let pins = super::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    pins.into_iter()
        .filter(|pin| {
            let granted = super::grants(&get(&super::grants_setting(&pin.name)))
                .is_ok_and(|g| g.commit == pin.commit && g.caps.contains(&Cap::HOOK));
            let ready =
                state(&pin.name).is_some_and(|s| s.commit == pin.commit && s.state == Word::Ready);
            granted && ready
        })
        .map(|pin| pin.name)
        .collect()
}

/// The request of each plugin of `names` for the pull request GitHub gave
/// as `pull`, asked by the envelope `event`: none when it is merged, closed,
/// or no pull request of the bot's in `repo`.
pub fn requests(
    names: &[String],
    pull: &Value,
    repo: &str,
    bot: &str,
    (pr, issue): (u64, u64),
    event: &Envelope,
) -> Vec<(String, HookAsk)> {
    let Some((head, base)) = open_bot_pull(pull, repo, bot) else {
        return Vec::new();
    };
    names
        .iter()
        .map(|name| {
            let ask = HookAsk {
                pr,
                head: head.clone(),
                base: base.clone(),
                issue: Some(issue),
                trigger: Trigger::Cycle,
                event: Some(event.clone()),
            };
            (name.clone(), ask)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;
    use serde_json::json;
    use std::collections::BTreeMap;

    const C: &str = "0123456789abcdef0123456789abcdef01234567";
    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210fedcba98";

    fn vars(more: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for (k, v) in [
            ("TEHAS_PLUGIN", "on".to_string()),
            ("TEHAS_TEST_FEATURES", "plugins".to_string()),
            (
                "TEHAS_PLUGINS",
                format!("floorplan=https://example.test/floorplan@{C} spec=https://example.test/spec@{C}"),
            ),
            (
                "TEHAS_PLUGIN_GRANTS_FLOORPLAN",
                format!("{C} view hook:after:change.offered"),
            ),
            ("TEHAS_PLUGIN_GRANTS_SPEC", format!("{C} hook:after:change.offered")),
        ] {
            map.insert(k.to_string(), v);
        }
        for (k, v) in more {
            map.insert(k.to_string(), v.to_string());
        }
        move |name: &str| map.get(name).cloned().unwrap_or_default()
    }

    fn ready(_: &str) -> Option<State> {
        Some(State::new(Word::Ready, C))
    }

    #[test]
    fn each_ready_granted_plugin_is_asked_while_plugins_are_on() {
        assert_eq!(granted(&vars(&[]), &ready), ["floorplan", "spec"]);
        assert!(granted(&vars(&[("TEHAS_PLUGIN", "off")]), &ready).is_empty());
    }

    #[test]
    fn a_plugin_refused_unbuilt_or_not_granted_the_hook_is_not_asked() {
        let ungranted = vars(&[("TEHAS_PLUGIN_GRANTS_SPEC", &format!("{C} view"))]);
        assert_eq!(granted(&ungranted, &ready), ["floorplan"]);
        let other = "f".repeat(40);
        let elsewhere = vars(&[(
            "TEHAS_PLUGIN_GRANTS_SPEC",
            &format!("{other} hook:after:change.offered"),
        )]);
        assert_eq!(granted(&elsewhere, &ready), ["floorplan"]);
        let refused = |name: &str| {
            Some(if name == "spec" {
                State::refused(C, "no build")
            } else {
                State::new(Word::Ready, C)
            })
        };
        assert_eq!(granted(&vars(&[]), &refused), ["floorplan"]);
        let old = |_: &str| Some(State::new(Word::Ready, &"f".repeat(40)));
        assert!(granted(&vars(&[]), &old).is_empty());
        assert!(granted(&vars(&[]), &|_: &str| None).is_empty());
    }

    /// The sample of `name` with every field.
    fn sample(name: Name) -> Envelope {
        tehas_ports::events::samples()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap()
            .full
    }

    fn pull() -> Value {
        json!({
            "state": "open", "merged": false,
            "user": {"login": "acme-bot[bot]"},
            "head": {"sha": H, "repo": {"full_name": "acme/widget"}},
            "base": {"sha": B},
        })
    }

    #[test]
    fn an_open_pull_request_of_the_bot_is_asked_for_at_its_head() {
        let names = vec!["floorplan".to_string()];
        let asked = requests(
            &names,
            &pull(),
            "acme/widget",
            "acme-bot[bot]",
            (42, 7),
            &sample(Name::ChangeOffered),
        );
        assert_eq!(
            asked,
            [(
                "floorplan".to_string(),
                HookAsk {
                    pr: 42,
                    head: H.into(),
                    base: B.into(),
                    issue: Some(7),
                    trigger: Trigger::Cycle,
                    event: Some(sample(Name::ChangeOffered)),
                }
            )]
        );
        for (ptr, v) in [
            ("/state", json!("closed")),
            ("/merged", json!(true)),
            ("/user/login", json!("someone")),
            ("/head/repo/full_name", json!("fork/widget")),
            ("/head/sha", json!("89abcdef")),
        ] {
            let mut p = pull();
            *p.pointer_mut(ptr).unwrap() = v;
            assert!(
                requests(
                    &names,
                    &p,
                    "acme/widget",
                    "acme-bot[bot]",
                    (42, 7),
                    &sample(Name::ChangeOffered)
                )
                .is_empty(),
                "{ptr}"
            );
        }
    }

    #[test]
    fn a_request_is_written_and_read_back() {
        let home = TestDir::new("plugin-hook-request");
        let ask = HookAsk {
            pr: 42,
            head: H.into(),
            base: B.into(),
            issue: Some(7),
            trigger: Trigger::Cycle,
            event: Some(sample(Name::ChangeOffered)),
        };
        write_request(&home, "demo", &ask).unwrap();
        assert_eq!(read_request(&home, "demo", 42), Some(ask.clone()));
        let by_hand = HookAsk {
            event: None,
            trigger: Trigger::Host,
            ..ask.clone()
        };
        assert!(!by_hand.json().contains("\"event\""));
        // An event other than the offer, or no envelope, refuses the request.
        for event in [
            serde_json::to_value(sample(Name::CycleEnded)).unwrap(),
            json!({"api": 1, "key": "7-1"}),
        ] {
            let mut other: Value = serde_json::from_str(&ask.json()).unwrap();
            other["event"] = event;
            assert_eq!(HookAsk::read(other.to_string().as_bytes()), None);
        }
        assert_eq!(HookAsk::read(by_hand.json().as_bytes()), Some(by_hand));
        assert_eq!(read_request(&home, "demo", 43), None);
        assert_eq!(unit("demo", 42), "tehas-plugin-hook@demo-42.service");
        let mut timer: Value = serde_json::from_str(&ask.json()).unwrap();
        timer["trigger"] = json!("timer");
        assert_eq!(HookAsk::read(timer.to_string().as_bytes()), None);
    }
}
