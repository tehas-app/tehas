//! tehas-dispatch: the dispatcher in Rust.
//!
//! Usage:
//!   tehas-dispatch poll [--dry-run]     one poll, as tehasbot dispatch runs it:
//!                                       it acts. 78 before it when models.json
//!                                       names a bad or unset key
//!   tehas-dispatch refresh              one run of tehas refresh: it
//!                                       writes the refresh requests
//!   tehas-dispatch -h|--help            print this text
//!
//! Reads the instance's settings from the environment, as lib/config.sh
//! leaves them (tehasbot loads them), $TEHAS_HOME, and gh, systemctl, curl,
//! docker and $TEHAS_HOME/libexec/store from the PATH and the home.
//!
//! Exit: poll ends as tehasbot dispatch does; 64 an argument it does not know;
//! 78 a setting tehas_load always leaves set is not set, or state/account-ids
//! keeps a bot only under its bare name (run console ids record).
//! refresh has its own text and statuses: tehas refresh --help.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use tehas_dispatch::Settings;

fn usage() -> String {
    include_str!("main.rs")
        .lines()
        .skip(2)
        .take_while(|l| l.starts_with("//!"))
        .map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn set_in(env: &Value, k: &str) -> bool {
    env.get(k)
        .and_then(Value::as_str)
        .is_some_and(|v| !v.is_empty())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let dry = match a.as_slice() {
        ["-h" | "--help"] => {
            tehas_core::outln!("{}", usage());
            return;
        }
        ["poll"] => false,
        ["poll", "--dry-run"] => true,
        ["refresh", rest @ ..] => std::process::exit(tehas_dispatch::refresh::main(rest)),
        _ => {
            eprintln!(
                "tehas-dispatch: unknown invocation: {}\n\n{}",
                args.join(" "),
                usage()
            );
            std::process::exit(64);
        }
    };
    let env: Map<String, Value> = std::env::vars().map(|(k, v)| (k, json!(v))).collect();
    let home = env
        .get("TEHAS_HOME")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // The settings an instance has to set itself, and the three a load always
    // leaves set that a poll cannot act by without: a run with neither has no
    // settings to decide by, and stops before any call (rule 2). tehas_core
    // names both lists, so the poll holds no copy of either.
    let set = |k: &str| {
        env.get(k)
            .and_then(Value::as_str)
            .is_some_and(|v| !v.is_empty())
    };
    let missing: Vec<&str> = tehas_core::settings::INSTANCE_SETTINGS
        .into_iter()
        .chain(tehas_core::settings::DISPATCHER_ALSO)
        .filter(|k| !set(k))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "tehas-dispatch: {} not set: load the instance's settings first (lib/config.sh, tehas_load)",
            missing.join(", ")
        );
        std::process::exit(78);
    }
    if let Err(why) = tehas_core::models::host_keys_ok() {
        eprintln!("tehas: {why}");
        std::process::exit(78);
    }
    let mut env = Value::Object(env);
    if let Some(why) = Settings::refused(&env) {
        eprintln!("tehas-dispatch: {why}");
        std::process::exit(78);
    }
    // A bot kept only under its bare name may be another account of that
    // name: no id is guessed, and the poll does not run until it is settled.
    if let Some(line) = tehas_core::accounts::unsettled(Path::new(&home)) {
        eprintln!("{line}");
        std::process::exit(78);
    }
    // tehas_branch: the setting, or the checkout's origin/HEAD. tehas_core
    // holds the rule, with the bound on the git read and the line lib/config.sh
    // and tehas work give.
    if !set_in(&env, "TEHAS_BRANCH") {
        match tehas_core::settings::branch() {
            Ok(b) => env["TEHAS_BRANCH"] = json!(b),
            Err(line) => {
                eprintln!("{line}");
                std::process::exit(78);
            }
        }
    }
    // The poll's own libexec, which the refine brief's path is named by.
    env["libexec"] = json!(tehas_dispatch::poll::here(Path::new(&home)));
    let settings = Settings::parse(&env);
    std::process::exit(tehas_dispatch::poll::run_poll(
        &PathBuf::from(&home),
        &settings,
        dry,
        tehas_dispatch::events::wiring(),
        tehas_dispatch::apply::Wiring {
            handles,
            mark_of,
            mark_name,
            assignee: assignee(&PathBuf::from(&home), &settings.repo),
        },
    ));
}

/// A label word of an action as a mark: one of tehas's words by the
/// adapter's table, None for any other, which is refused at the send.
/// Temporary: the T4.6 pull request that types decide's actions onto `Mark`
/// removes it.
fn mark_of(label: &str) -> Option<tehas_ports::Mark> {
    tehas_github::table::word(label)
}

/// A mark as the label word it is spelled by, for the queue's mapping of
/// marks back to the words the snapshot names them by.
fn mark_name(mark: &tehas_ports::Mark) -> String {
    tehas_github::table::mark_name(mark)
}

/// `TEHAS_ASSIGNEE`'s account through the site's identity, once a poll:
/// None with no such setting, Err with why when it cannot be resolved.
fn assignee(home: &Path, repo: &str) -> Option<Result<tehas_ports::AccountId, String>> {
    use std::sync::Arc;
    use tehas_ports::{Actor, Caller, Login, Scope, Site};
    let login = std::env::var("TEHAS_ASSIGNEE").unwrap_or_default();
    if login.trim().is_empty() {
        return None;
    }
    let (bot, id) = tehas_core::accounts::bot(home);
    let make: tehas_github::MakeRunner = Arc::new(move |_| {
        Ok(Box::new(tehas_core::forge::Bounded::new(
            "gh",
            std::time::Duration::from_secs(30),
        )) as Box<dyn tehas_core::forge::GhRunner>)
    });
    let site = tehas_github::GitHubSite::new(repo, &bot, id, make);
    let found = site
        .identity(&Actor::Ours(Scope::Full), Caller::new("poll"))
        .and_then(|mut i| i.account_by_login(&Login::new(login.trim())));
    Some(match found {
        Ok(account) => account
            .id
            .ok_or_else(|| "the site gives the account no id".to_string()),
        Err(o) => Err(o.line().to_string()),
    })
}

/// The handles a poll writes and settles through, from the GitHub adapter's
/// site acting as the factory's account. Its gh is the host's own, given the
/// poll's token so the shim asks for none, under the poll's bound: the site
/// is handed that runner and holds no token of its own.
fn handles(
    home: &Path,
    repo: &str,
    gh: &str,
    token: Option<&str>,
    bound: std::time::Duration,
    calls: std::sync::Arc<std::sync::atomic::AtomicU64>,
) -> tehas_ports::Got<tehas_dispatch::apply::Handles> {
    use std::sync::Arc;
    use tehas_ports::{Actor, Caller, Scope, Site};
    let (login, id) = tehas_core::accounts::bot(home);
    let (gh, token) = (gh.to_string(), token.map(str::to_string));
    let make: tehas_github::MakeRunner = Arc::new(move |_| {
        let mut b = tehas_core::forge::Bounded::new(gh.clone(), bound);
        b.token = token.clone();
        let counted = tehas_core::forge::Counted {
            inner: Box::new(b),
            count: calls.clone(),
        };
        Ok(Box::new(counted) as Box<dyn tehas_core::forge::GhRunner>)
    });
    let site = tehas_github::GitHubSite::new(repo, &login, id, make);
    let (by, c) = (Actor::Ours(Scope::Full), Caller::new("poll"));
    let writes: Box<dyn tehas_ports::Writes> = if tehas_core::read_only::on() {
        Box::new(tehas_ports::ReadOnly)
    } else {
        Box::new(tehas_ports::Writer {
            tickets: site.ticket_writes(&by, c)?,
            blueprints: site.blueprint_writes(&by, c)?,
            code_host: site.code_host_writes(&by, c)?,
        })
    };
    Ok(tehas_dispatch::apply::Handles {
        writes,
        tickets: site.tickets(&by, c)?,
        blueprints: site.blueprints(&by, c)?,
        code_host: site.code_host(&by, c)?,
        board: site.board(&by, c)?,
    })
}
