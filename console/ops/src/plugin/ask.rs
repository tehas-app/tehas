//! `tehas plugin run`, `hook` and `log` (docs/specs/2026-11-21-plugins.md,
//! "Commands"). The first two ask for a run and wait for nothing: the run is
//! its unit's, `tehas-plugin@<name>.service` or
//! `tehas-plugin-hook@<name>-<pr>.service`. `log` prints the last run's log as
//! the plugin wrote it.

use super::exec::Exec;
use super::prepare;
use super::runs::{self, Asked, HookAsk, DISK_REFUSAL};
use std::io::Write;
use std::path::Path;
use tehas_core::exit;
use tehas_core::plugin::hook::open_ours;
use tehas_core::plugin::run::Trigger;
use tehas_core::plugin::state::Word;
use tehas_core::plugin::{self, Cap, Grants, Pin};
use tehas_ports::{Board, Bound, ChangeAsk, ChangeFilter, ChangeNo, ChangeStates, Got};

/// A board handle on a project, acting as the factory's account, given by
/// its login and the id `state/account-ids` records for it: the console's
/// wiring makes it.
pub type BoardOf<'a> = &'a dyn Fn(&str, &str, u64) -> Got<Box<dyn Board + Send>>;

/// The one change of `no`, in any state, with nothing beside it: one
/// question where the site allows.
fn one_change(no: ChangeNo) -> ChangeAsk {
    ChangeAsk {
        filter: ChangeFilter {
            states: ChangeStates::All,
            numbers: vec![no],
            ..ChangeFilter::default()
        },
        files: None,
        closing: false,
        siblings: None,
        activity: None,
    }
}

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

pub fn hook(
    x: &mut dyn Exec,
    board: BoardOf,
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
    if !grants.caps.contains(&Cap::HOOK) {
        eprintln!("{what}: {name} is not granted hook:after:change.offered");
        return exit::NO;
    }
    if let Err(rc) = ready(home, &pin, what, false) {
        return rc;
    }
    let repo = get("TEHAS_REPO");
    if !crate::forge::repo_shaped(&repo) {
        eprintln!("{what}: TEHAS_REPO is needed to read the pull request");
        return exit::SETTINGS;
    }
    // The factory's pull requests are known by the bot's recorded id alone.
    let (bot, id) = tehas_core::accounts::bot(home);
    let Some(id) = id.filter(|_| !bot.is_empty()) else {
        let login = if bot.is_empty() {
            "the bot"
        } else {
            bot.as_str()
        };
        eprintln!(
            "{what}: the account id of {login} is not recorded in {}, so no pull request is known as the bot's; `console ids record` records it with TEHAS_MILLWRIGHT_LOGIN set to the bot's login",
            home.join(tehas_core::accounts::FILE).display()
        );
        return exit::SETTINGS;
    };
    if runs::held(&runs::hook_lock(home, name, pr)) {
        eprintln!("{what}: running: the hook of {name} on #{pr} runs");
        return exit::NO;
    }
    let Some(no) = ChangeNo::new(pr) else {
        eprintln!("{what}: #{pr} is no open pull request of the bot's in {repo}");
        return exit::NO;
    };
    let one = Bound::new(1).expect("one");
    let read = board(&repo, &bot, id).and_then(|mut b| b.open_changes(&one_change(no), one));
    let change = match read.map(|w| w.into_items().into_iter().next()) {
        Ok(Some(item)) => item.change,
        Ok(None) => {
            eprintln!("{what}: #{pr} is no pull request of {repo}");
            return exit::NO;
        }
        Err(why) => {
            eprintln!("{what}: GitHub did not answer for #{pr}: {}", why.line());
            return exit::FAILED;
        }
    };
    let Some((head, base)) = open_ours(&change) else {
        eprintln!("{what}: #{pr} is no open pull request of the bot's in {repo}");
        return exit::NO;
    };
    let ask = HookAsk {
        pr,
        head,
        base,
        issue: None,
        trigger: Trigger::Host,
        event: None,
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
    let _ = std::io::stdout().write_all(shown(&bytes).as_bytes());
    exit::OK
}

/// A container's output as text a terminal only shows: each control
/// character but newline and tab, and each byte that is not UTF-8, is
/// U+FFFD, so no escape sequence reaches the terminal.
fn shown(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::exec::fake::{ok, Fake};
    use super::*;
    use std::collections::BTreeMap;
    use tehas_core::plugin::state::State;
    use tehas_core::testdir::TestDir;
    use tehas_ports::fake::{BoardOp, ChangeSpec, FakeSite, Fault, Faulty, Op, World};
    use tehas_ports::{Actor, At, Caller, ChangeState, Scope, Site, ALL_MAX};

    const C: &str = "0123456789abcdef0123456789abcdef01234567";
    const URL: &str = "https://github.com/tehas-app/tehas-plugin-floorplan";

    fn vars(more: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        map.insert("TEHAS_PLUGINS".into(), format!("floorplan={URL}@{C}"));
        map.insert(
            "TEHAS_PLUGIN_GRANTS_FLOORPLAN".into(),
            format!("{C} product hook:after:change.offered"),
        );
        map.insert("TEHAS_REPO".into(), "acme/widget".into());
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

    /// Change 42 of the factory's account from `feature/issue-7`, open,
    /// built further by `f`.
    fn world(f: impl FnOnce(ChangeSpec) -> ChangeSpec) -> World {
        World::new().change(42, "feature/issue-7", f)
    }

    /// The board of `w`, as the factory's account, on acme/widget for the
    /// bot the home records.
    fn site(w: &World) -> impl Fn(&str, &str, u64) -> Got<Box<dyn Board + Send>> + '_ {
        move |repo: &str, bot: &str, id: u64| {
            assert_eq!((repo, bot, id), ("acme/widget", "acme-bot[bot]", 900));
            FakeSite::new(w).board(&Actor::Ours(Scope::Full), Caller::new("plugin-hook"))
        }
    }

    /// The bot's login, `acme-bot[bot]`, and the pairs `ids` recorded.
    fn bot_files(home: &Path, ids: &str) {
        use tehas_core::home::{Home, State};
        let login = Home::new(home).state(State::BotLogin);
        std::fs::create_dir_all(login.parent().unwrap()).unwrap();
        std::fs::write(login, "acme-bot[bot]\n").unwrap();
        std::fs::write(home.join(tehas_core::accounts::FILE), ids).unwrap();
    }

    /// The bot's login and its recorded id, as install and update leave
    /// them.
    fn record_bot(home: &Path) {
        bot_files(home, "acme-bot[bot] 900\n");
    }

    /// The head and base commits of change 42 in `w`.
    fn sides(w: &World) -> (String, String) {
        let c = FakeSite::new(w)
            .code_host(&Actor::Operator, Caller::new("test"))
            .unwrap()
            .change(ChangeNo::new(42).unwrap(), ALL_MAX)
            .unwrap();
        (
            c.digest.head_commit.as_str().to_string(),
            c.digest.base_commit.as_str().to_string(),
        )
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
        record_bot(&home);
        let mut x = Fake::new();
        x.on("systemctl", ok(""));
        let w = world(|c| c);
        assert_eq!(
            hook(&mut x, &site(&w), &home, &vars(&[]), "floorplan", 42),
            0
        );
        let asked = runs::read_hook(&home, "floorplan", 42).unwrap();
        assert_eq!((asked.head, asked.base), sides(&w));
        assert_eq!(asked.trigger, Trigger::Host);
        assert_eq!(
            x.ran,
            ["systemctl --user start --no-block tehas-plugin-hook@floorplan-42.service"]
        );
        let closed = ChangeState::Closed { at: At::new(1) };
        let merged = ChangeState::Merged {
            at: At::new(1),
            by: None,
            commit: None,
        };
        for (what, w) in [
            ("closed", world(|c| c.state(closed))),
            ("merged", world(|c| c.state(merged))),
            ("someone's", world(|c| c.author("someone"))),
            ("no such change", World::new()),
        ] {
            assert_eq!(
                hook(&mut x, &site(&w), &home, &vars(&[]), "floorplan", 42),
                3,
                "{what}"
            );
        }
        let down =
            Faulty::new(FakeSite::new(&w)).on(Op::Board(BoardOp::OpenChanges), Fault::Timeout);
        let down = move |_: &str, _: &str, _: u64| {
            down.board(&Actor::Ours(Scope::Full), Caller::new("plugin-hook"))
        };
        assert_eq!(hook(&mut x, &down, &home, &vars(&[]), "floorplan", 42), 1);
        let no_hook = vars(&[("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &format!("{C} product"))]);
        assert_eq!(hook(&mut x, &site(&w), &home, &no_hook, "floorplan", 42), 3);
        let no_repo = vars(&[("TEHAS_REPO", "")]);
        assert_eq!(
            hook(&mut x, &site(&w), &home, &no_repo, "floorplan", 42),
            78
        );
        std::fs::create_dir_all(runs::hooks(&home, "floorplan")).unwrap();
        let lock = std::fs::File::create(runs::hook_lock(&home, "floorplan", 42)).unwrap();
        lock.lock().unwrap();
        assert_eq!(
            hook(&mut x, &site(&w), &home, &vars(&[]), "floorplan", 42),
            3
        );
    }

    // With no id recorded for the bot no pull request is known as the
    // bot's: that is a setting to repair, said as one, and nothing is asked.
    #[test]
    fn a_bot_with_no_recorded_id_is_a_setting_and_asks_nothing() {
        let home = home_with("plugin-ask-hook-no-id", Some(State::new(Word::Ready, C)));
        let mut x = Fake::new();
        let unasked = |_: &str, _: &str, _: u64| -> Got<Box<dyn Board + Send>> {
            panic!("no handle is made")
        };
        assert_eq!(
            hook(&mut x, &unasked, &home, &vars(&[]), "floorplan", 42),
            78
        );
        bot_files(&home, "alice 101\n");
        assert_eq!(
            hook(&mut x, &unasked, &home, &vars(&[]), "floorplan", 42),
            78
        );
        assert!(x.ran.is_empty());
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

    #[test]
    fn a_log_shows_no_control_character_but_newline_and_tab() {
        let raw = b"a\tb\n\x1b]52;c;aGk=\x07\x1b]0;title\x07\r\x7f\xc2\x9b2J\xff\xc3\xa4\n";
        assert_eq!(
            shown(raw),
            "a\tb\n\u{FFFD}]52;c;aGk=\u{FFFD}\u{FFFD}]0;title\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}2J\u{FFFD}\u{e4}\n"
        );
    }
}
