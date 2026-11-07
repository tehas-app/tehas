//! The user ids of the accounts that may authorise work (tehas_core::accounts).
//!
//!   console ids record [--accept-id <login>]...
//!     asks GitHub for the user id of each login of TEHAS_ALLOWED_LABELERS and
//!     TEHAS_MILLWRIGHT_LOGIN (`GET /users/<login>`) and records the pairs in
//!     $TEHAS_HOME/state/account-ids. The pair of a login no longer listed
//!     stays, so a login listed again later must still be the same account.
//!     install.sh runs it, and `tehas update` with the --accept-id it was given.
//!
//! A login that does not resolve, or that GitHub could not be asked about,
//! stops with 78 and names it. So does a login whose id is not the recorded
//! one: the login may have changed hands, and only a person who knows it did
//! not confirms the new id with --accept-id. Nothing is written then.
//!
//! With an app named and no GH_TOKEN, gh is given the app's token, made from
//! its key as `tehasbot github-token` makes it (console/src/github.rs). A first
//! install runs this before the shim is in place, so gh has no other token. A
//! token that cannot be had stops with 78.
//!
//! Exit: 0 recorded, 64 bad words or an --accept-id of no listed login, 78 as
//! above or TEHAS_HOME unset or the file cannot be written.

use crate::forge::CALL_BOUND;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use tehas_core::accounts::{self, account};
use tehas_core::forge::{Bounded, GhRunner};

const USAGE: &str = "usage: console ids record [--accept-id <login>]...";

/// What GitHub said of a login.
#[derive(Debug, PartialEq)]
pub enum Asked {
    Id(u64),
    /// No such account.
    Missing,
    /// The call failed for another reason, said in the words.
    Failed(String),
}

/// The pairs to record and the lines a run says, or the lines that stop it.
pub type Recorded = Result<(BTreeMap<String, u64>, Vec<String>), Vec<String>>;

/// The pairs to record, or the lines that stop the run. `logins` are the
/// settings' logins as written, `had` the recorded pairs, `accept` the logins
/// whose new id a person confirmed.
pub fn record(
    logins: &[String],
    had: &BTreeMap<String, u64>,
    accept: &[String],
    ask: &mut dyn FnMut(&str) -> Asked,
) -> Recorded {
    let mut ids = had.clone();
    let mut asked = Vec::new();
    let mut faults = Vec::new();
    let mut said = Vec::new();
    for login in logins {
        let key = account(login).to_string();
        if asked.contains(&key) {
            continue;
        }
        asked.push(key.clone());
        let accepted = accept.iter().any(|a| account(a) == key);
        match (ask(login), had.get(&key)) {
            (Asked::Missing, _) => faults.push(format!(
                "tehas: the login {login} does not resolve to a GitHub account; take it out of the settings or correct it"
            )),
            (Asked::Failed(why), _) => faults.push(format!(
                "tehas: GitHub could not be asked for the user id of {login}{why}"
            )),
            (Asked::Id(id), Some(was)) if *was != id && !accepted => faults.push(format!(
                "tehas: the login {login} now has the user id {id}, not the recorded {was}: the login may have changed hands. If it is the same person, run tehas update --accept-id {login}"
            )),
            (Asked::Id(id), was) => {
                if let Some(w) = was.filter(|w| **w != id) {
                    said.push(format!(
                        "accounts: {login} is recorded with the user id {id} (it was {w})"
                    ));
                }
                ids.insert(key, id);
            }
        }
    }
    if faults.is_empty() {
        Ok((ids, said))
    } else {
        Err(faults)
    }
}

/// The settings' logins as written, the allowed ones first.
fn logins() -> Vec<String> {
    let var = |k: &str| env::var(k).unwrap_or_default();
    var("TEHAS_ALLOWED_LABELERS")
        .split_whitespace()
        .map(str::to_string)
        .chain(Some(var("TEHAS_MILLWRIGHT_LOGIN")).filter(|m| !m.is_empty()))
        .collect()
}

/// A login of the shape GitHub allows, with `[bot]` for an app: nothing that
/// could reach the path as anything but one name.
fn login_shaped(login: &str) -> bool {
    let bare = account(login);
    !bare.is_empty()
        && !bare.starts_with('-')
        && bare.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn ask_github(gh: &mut dyn GhRunner, login: &str) -> Asked {
    if !login_shaped(login) {
        return Asked::Missing;
    }
    match gh.run(&["api", &format!("users/{login}"), "--jq", ".id"]) {
        Ok(out) => match out.trim().parse::<u64>() {
            Ok(id) => Asked::Id(id),
            Err(_) => Asked::Failed(": its answer held no id".into()),
        },
        Err(why) if why.contains("404") => Asked::Missing,
        Err(why) => Asked::Failed(why),
    }
}

fn write(home: &Path, ids: &BTreeMap<String, u64>) -> std::io::Result<()> {
    let file = home.join(accounts::FILE);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, accounts::text(ids))?;
    fs::rename(&tmp, &file)
}

/// `console ids record`, for `tehas update`'s gate.
pub fn record_here(accept: &[String]) -> u8 {
    let Some(home) = env::var("TEHAS_HOME").ok().filter(|h| !h.is_empty()) else {
        eprintln!("console ids: TEHAS_HOME is not set");
        return 78;
    };
    let logins = logins();
    if let Some(a) = accept
        .iter()
        .find(|a| !logins.iter().any(|l| account(l) == account(a)))
    {
        eprintln!("tehas: --accept-id {a} names no login of TEHAS_ALLOWED_LABELERS or TEHAS_MILLWRIGHT_LOGIN");
        return 64;
    }
    let app = !env::var("TEHAS_GITHUB_APP_ID").unwrap_or_default().is_empty();
    if app && env::var("GH_TOKEN").unwrap_or_default().is_empty() {
        let Some(token) = crate::github::for_a_container() else {
            eprintln!("tehas: ids record: no GitHub token for the app, so no user id was recorded");
            return 78;
        };
        env::set_var("GH_TOKEN", &token);
        env::set_var("TEHAS_GH_TOKEN", &token);
    }
    // gh by PATH, as `tehas labels` runs it: a login shell has the app's shim
    // first, and an install from a checkout has the person's own.
    let mut gh = Bounded::new("gh", CALL_BOUND);
    let home = Path::new(&home);
    let had = accounts::read(home);
    match record(&logins, &had, accept, &mut |l| ask_github(&mut gh, l)) {
        Ok((ids, said)) => {
            if ids != had {
                if let Err(e) = write(home, &ids) {
                    eprintln!(
                        "tehas: could not write {}: {e}",
                        home.join(accounts::FILE).display()
                    );
                    return 78;
                }
            }
            said.iter().for_each(|l| tehas_core::outln!("{l}"));
            0
        }
        Err(faults) => {
            faults.iter().for_each(|l| eprintln!("{l}"));
            eprintln!("tehas: no user id was recorded");
            78
        }
    }
}

pub fn main(given: &[String]) -> u8 {
    let mut words = given.iter();
    if words.next().map(String::as_str) != Some("record") {
        eprintln!("{USAGE}");
        return 64;
    }
    let mut accept = Vec::new();
    while let Some(w) = words.next() {
        match (w.as_str(), words.next()) {
            ("--accept-id", Some(l)) => accept.push(l.clone()),
            _ => {
                eprintln!("{USAGE}");
                return 64;
            }
        }
    }
    record_here(&accept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logins(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    fn github(login: &str) -> Asked {
        match login {
            "alice" => Asked::Id(1001),
            "mill[bot]" => Asked::Id(3001),
            "down" => Asked::Failed(": HTTP 502".into()),
            _ => Asked::Missing,
        }
    }

    #[test]
    fn each_login_is_recorded_by_the_name_graphql_gives() {
        let (ids, said) = record(
            &logins(&["alice", "mill[bot]"]),
            &BTreeMap::new(),
            &[],
            &mut github,
        )
        .unwrap();
        assert_eq!(accounts::text(&ids), "alice 1001\nmill 3001\n");
        assert!(said.is_empty());
    }

    #[test]
    fn a_login_that_does_not_resolve_stops_and_is_named() {
        let faults = record(
            &logins(&["alice", "gone"]),
            &BTreeMap::new(),
            &[],
            &mut github,
        )
        .unwrap_err();
        assert_eq!(faults.len(), 1);
        assert!(faults[0].contains("gone") && faults[0].contains("does not resolve"));
        let faults = record(&logins(&["down"]), &BTreeMap::new(), &[], &mut github).unwrap_err();
        assert!(faults[0].contains("down") && faults[0].contains("HTTP 502"));
    }

    #[test]
    fn an_id_that_changed_stops_until_it_is_accepted() {
        let had = accounts::parse("alice 999\n").unwrap();
        let faults = record(&logins(&["alice"]), &had, &[], &mut github).unwrap_err();
        assert!(faults[0].contains("1001") && faults[0].contains("999"));
        assert!(faults[0].contains("tehas update --accept-id alice"));
        let (ids, said) =
            record(&logins(&["alice"]), &had, &logins(&["alice"]), &mut github).unwrap();
        assert_eq!(ids.get("alice"), Some(&1001));
        assert!(said[0].contains("1001") && said[0].contains("999"));
    }

    #[test]
    fn a_login_no_longer_listed_keeps_its_pair() {
        let had = accounts::parse("alice 1001\nbob 1002\n").unwrap();
        let (ids, _) = record(&logins(&["alice"]), &had, &[], &mut github).unwrap();
        assert_eq!(ids, had);
    }

    #[test]
    fn only_a_login_shaped_name_reaches_the_path() {
        assert!(login_shaped("alice-2"));
        assert!(login_shaped("mill[bot]"));
        assert!(!login_shaped("../x"));
        assert!(!login_shaped("a?b"));
        assert!(!login_shaped("-x"));
        assert!(!login_shaped(""));
    }
}
