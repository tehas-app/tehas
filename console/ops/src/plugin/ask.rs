//! `tehas plugin run`, `hook` and `log` (docs/specs/2026-11-21-plugins.md,
//! "Commands"). The first two ask for a run and wait for nothing: the run is
//! its unit's, `tehas-plugin@<name>.service` or
//! `tehas-plugin-hook@<name>-<pr>.service`. `log` prints the last run's log as
//! the plugin wrote it.

use super::exec::Exec;
use super::prepare;
use super::runs::{self, Asked, HookAsk, DISK_REFUSAL};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use tehas_core::accounts::factorys;
use tehas_core::exit;
use tehas_core::forge::GhRunner;
use tehas_core::grammar::commit_id;
use tehas_core::plugin::run::Trigger;
use tehas_core::plugin::state::Word;
use tehas_core::plugin::{self, Cap, Grants, Pin};

/// The pin and the grants at it, or the exit that says why not.
fn plugin_of(get: &dyn Fn(&str) -> String, what: &str, name: &str) -> Result<(Pin, Grants), u8> {
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    let Some(pin) = pins.into_iter().find(|p| p.name == name) else {
        eprintln!("{what}: TEHAS_PLUGINS names no plugin {name}");
        return Err(exit::USAGE);
    };
    let grants = plugin::grants(&get(&plugin::grants_setting(name)))
        .ok()
        .filter(|g| g.commit == pin.commit);
    let Some(grants) = grants else {
        eprintln!("{what}: {name} is granted nothing at its pin");
        return Err(exit::NO);
    };
    Ok((pin, grants))
}

/// Whether the plugin may run: ready at its pin, or refused only for the
/// disk, which a person's run clears.
fn ready(home: &Path, pin: &Pin, what: &str, person: bool) -> Result<(), u8> {
    let s = prepare::read_state(home, &pin.name).filter(|s| s.commit == pin.commit);
    match s {
        Some(s) if s.state == Word::Ready => Ok(()),
        Some(s) if person && s.refusal.as_deref() == Some(DISK_REFUSAL) => Ok(()),
        Some(s) => {
            let why = s.refusal.map(|r| format!(": {r}")).unwrap_or_default();
            eprintln!("{what}: {} is {}{why}", pin.name, s.state.word());
            Err(exit::NO)
        }
        None => {
            eprintln!(
                "{what}: {} is unbuilt at its pin: run tehas update",
                pin.name
            );
            Err(exit::NO)
        }
    }
}

pub fn run(x: &mut dyn Exec, home: &Path, get: &dyn Fn(&str) -> String, name: &str) -> u8 {
    let what = "tehas plugin run";
    let pin = match plugin_of(get, what, name) {
        Ok((pin, _)) => pin,
        Err(rc) => return rc,
    };
    if let Err(rc) = ready(home, &pin, what, true) {
        return rc;
    }
    if runs::running(home, name) {
        eprintln!("{what}: running: a run of {name} holds its lock");
        return exit::NO;
    }
    let asked = Asked {
        trigger: Trigger::Host,
        by: "host".to_string(),
    };
    match runs::ask_refresh(x, home, name, &asked) {
        Ok(()) => {
            tehas_core::outln!(
                "asked: {}; tehas plugin log {name} shows its log when it ends",
                runs::unit(name, None)
            );
            exit::OK
        }
        Err(why) => {
            eprintln!("{what}: {why}");
            exit::FAILED
        }
    }
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

pub fn hook(
    x: &mut dyn Exec,
    gh: &mut dyn GhRunner,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    name: &str,
    pr: u64,
) -> u8 {
    let what = "tehas plugin hook";
    let (pin, grants) = match plugin_of(get, what, name) {
        Ok(p) => p,
        Err(rc) => return rc,
    };
    if !grants.caps.contains(&Cap::HookAfterWork) {
        eprintln!("{what}: {name} is not granted hook:after:work");
        return exit::NO;
    }
    if let Err(rc) = ready(home, &pin, what, false) {
        return rc;
    }
    let (repo, bot) = (get("TEHAS_REPO"), get("TEHAS_BOT_LOGIN"));
    if !crate::forge::repo_shaped(&repo) || bot.is_empty() {
        eprintln!("{what}: TEHAS_REPO and TEHAS_BOT_LOGIN are needed to read the pull request");
        return exit::SETTINGS;
    }
    if runs::held(&runs::hook_lock(home, name, pr)) {
        eprintln!("{what}: running: the hook of {name} on #{pr} runs");
        return exit::NO;
    }
    let path = format!("repos/{repo}/pulls/{pr}");
    let pull = match gh.run(&["api", &path]) {
        Ok(out) => serde_json::from_str::<Value>(&out).unwrap_or(Value::Null),
        Err(why) => {
            eprintln!("{what}: GitHub did not answer for #{pr}: {why}");
            return exit::FAILED;
        }
    };
    let Some((head, base)) = open_bot_pull(&pull, &repo, &bot) else {
        eprintln!("{what}: #{pr} is no open pull request of the bot's in {repo}");
        return exit::NO;
    };
    let ask = HookAsk {
        pr,
        head,
        base,
        issue: None,
        trigger: Trigger::Host,
    };
    match runs::ask_hook(x, home, name, &ask) {
        Ok(()) => {
            tehas_core::outln!(
                "asked: {}; tehas plugin log {name} --hook {pr} shows its log when it ends",
                runs::unit(name, Some(pr))
            );
            exit::OK
        }
        Err(why) => {
            eprintln!("{what}: {why}");
            exit::FAILED
        }
    }
}

pub fn log(home: &Path, get: &dyn Fn(&str) -> String, name: &str, pr: Option<u64>) -> u8 {
    let what = "tehas plugin log";
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    if !pins.iter().any(|p| p.name == name) {
        eprintln!("{what}: TEHAS_PLUGINS names no plugin {name}");
        return exit::USAGE;
    }
    let file = runs::log(home, name, pr);
    let Ok((bytes, _)) = tehas_core::file::read_regular(&file, runs::LOG_MAX) else {
        eprintln!("{what}: no log: {} has not run", runs::unit(name, pr));
        return exit::NO;
    };
    // A reader that has gone is no failure of this command.
    let _ = std::io::stdout().write_all(&bytes);
    exit::OK
}

#[cfg(test)]
mod tests {
    use super::super::exec::fake::{ok, Fake};
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use tehas_core::plugin::state::State;
    use tehas_core::testdir::TestDir;

    const C: &str = "0123456789abcdef0123456789abcdef01234567";
    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210fedcba98";
    const URL: &str = "https://github.com/tehas-app/tehas-plugin-floorplan";

    fn vars(more: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        map.insert("TEHAS_PLUGINS".into(), format!("floorplan={URL}@{C}"));
        map.insert(
            "TEHAS_PLUGIN_GRANTS_FLOORPLAN".into(),
            format!("{C} product hook:after:work"),
        );
        map.insert("TEHAS_REPO".into(), "acme/widget".into());
        map.insert("TEHAS_BOT_LOGIN".into(), "acme-bot[bot]".into());
        for (k, v) in more {
            map.insert(k.to_string(), v.to_string());
        }
        move |name: &str| map.get(name).cloned().unwrap_or_default()
    }

    fn home_with(name: &str, s: Option<State>) -> TestDir {
        let home = TestDir::new(name);
        std::fs::create_dir_all(prepare::dir(&home, "floorplan")).unwrap();
        if let Some(s) = s {
            prepare::write_state(&home, "floorplan", &s).unwrap();
        }
        home
    }

    struct Gh(Result<String, String>);
    impl GhRunner for Gh {
        fn run(&mut self, args: &[&str]) -> Result<String, String> {
            assert_eq!(args, ["api", "repos/acme/widget/pulls/42"]);
            self.0.clone()
        }
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
    fn a_run_is_asked_of_a_ready_plugin_that_is_not_running() {
        let home = home_with("plugin-ask-run", Some(State::new(Word::Ready, C)));
        let mut x = Fake::new();
        x.on("systemctl", ok(""));
        assert_eq!(run(&mut x, &home, &vars(&[]), "floorplan"), 0);
        assert_eq!(
            x.ran,
            ["systemctl --user start --no-block tehas-plugin@floorplan.service"]
        );
        let lock = std::fs::File::create(runs::refresh_lock(&home, "floorplan")).unwrap();
        lock.lock().unwrap();
        assert_eq!(run(&mut x, &home, &vars(&[]), "floorplan"), 3);
        assert_eq!(run(&mut x, &home, &vars(&[]), "other"), 64);
    }

    #[test]
    fn a_run_of_a_refused_plugin_is_refused_unless_the_disk_refused_it() {
        let home = home_with("plugin-ask-refused", Some(State::refused(C, "no build")));
        let mut x = Fake::new();
        x.on("systemctl", ok(""));
        assert_eq!(run(&mut x, &home, &vars(&[]), "floorplan"), 3);
        assert!(x.ran.is_empty());
        prepare::write_state(&home, "floorplan", &State::refused(C, DISK_REFUSAL)).unwrap();
        assert_eq!(run(&mut x, &home, &vars(&[]), "floorplan"), 0);
        let home = home_with("plugin-ask-unbuilt", None);
        assert_eq!(run(&mut x, &home, &vars(&[]), "floorplan"), 3);
    }

    #[test]
    fn a_hook_needs_an_open_pull_request_of_the_bot_and_its_grant() {
        let home = home_with("plugin-ask-hook", Some(State::new(Word::Ready, C)));
        let mut x = Fake::new();
        x.on("systemctl", ok(""));
        let mut gh = Gh(Ok(pull().to_string()));
        assert_eq!(hook(&mut x, &mut gh, &home, &vars(&[]), "floorplan", 42), 0);
        let asked = runs::read_hook(&home, "floorplan", 42).unwrap();
        assert_eq!((asked.head.as_str(), asked.base.as_str()), (H, B));
        assert_eq!(asked.trigger, Trigger::Host);
        assert_eq!(
            x.ran,
            ["systemctl --user start --no-block tehas-plugin-hook@floorplan-42.service"]
        );
        for (ptr, v) in [
            ("/state", json!("closed")),
            ("/merged", json!(true)),
            ("/user/login", json!("someone")),
            ("/head/repo/full_name", json!("fork/widget")),
        ] {
            let mut p = pull();
            *p.pointer_mut(ptr).unwrap() = v;
            let mut gh = Gh(Ok(p.to_string()));
            assert_eq!(
                hook(&mut x, &mut gh, &home, &vars(&[]), "floorplan", 42),
                3,
                "{ptr}"
            );
        }
        let mut down = Gh(Err("timeout".into()));
        assert_eq!(
            hook(&mut x, &mut down, &home, &vars(&[]), "floorplan", 42),
            1
        );
        let no_hook = vars(&[("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &format!("{C} product"))]);
        assert_eq!(hook(&mut x, &mut gh, &home, &no_hook, "floorplan", 42), 3);
        let no_repo = vars(&[("TEHAS_REPO", "")]);
        assert_eq!(hook(&mut x, &mut gh, &home, &no_repo, "floorplan", 42), 78);
        std::fs::create_dir_all(runs::hooks(&home, "floorplan")).unwrap();
        let lock = std::fs::File::create(runs::hook_lock(&home, "floorplan", 42)).unwrap();
        lock.lock().unwrap();
        let mut gh = Gh(Ok(pull().to_string()));
        assert_eq!(hook(&mut x, &mut gh, &home, &vars(&[]), "floorplan", 42), 3);
    }

    #[test]
    fn a_log_is_printed_or_there_is_none() {
        let home = home_with("plugin-ask-log", None);
        assert_eq!(log(&home, &vars(&[]), "floorplan", None), 3);
        std::fs::write(runs::log(&home, "floorplan", None), "hello\n").unwrap();
        assert_eq!(log(&home, &vars(&[]), "floorplan", None), 0);
        assert_eq!(log(&home, &vars(&[]), "floorplan", Some(42)), 3);
        assert_eq!(log(&home, &vars(&[]), "other", None), 64);
    }
}
