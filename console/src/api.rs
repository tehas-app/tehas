//! `tehas api`: what this factory speaks, for a program to ask first, in
//! src/usage/api.txt. No network and no git, and it writes nothing. A store
//! that cannot answer is no poll on record: a report is not a check.

use crate::jq::{obj, J};
use crate::store;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// What this release can do. A feature is added here in the release that
/// brings it, and never taken out within api 1.
const FEATURES: [&str; 15] = [
    "status",
    "millwright-grant",
    "fixer-grant",
    "all-hands",
    "limit",
    "drain",
    "history",
    "export",
    "refresh-conflicts",
    "analytics",
    "state-move",
    "pr-title",
    "mcp-dashboard-keys",
    "clean",
    "disk",
];

/// The longest version text read, which `head -c` kept it to.
const VERSION_MAX: u64 = 200;

/// Whether the arguments ask for `--json`, or 64 with the usage.
pub fn words(given: &[String]) -> Result<bool, u8> {
    match given {
        [] => Ok(false),
        [flag] if flag == "--json" => Ok(true),
        _ => {
            eprintln!("usage: tehas api [--json]");
            Err(64)
        }
    }
}

/// The first line of what install.sh wrote, cut as `head -c` cuts it.
fn version_text(file: &Path) -> String {
    let mut bytes = Vec::new();
    if let Ok(f) = File::open(file) {
        let _ = f.take(VERSION_MAX).read_to_end(&mut bytes);
    }
    bytes.retain(|b| *b != 0);
    let text = String::from_utf8_lossy(&bytes);
    text.split('\n').next().unwrap_or_default().to_string()
}

fn is_tag(v: &str) -> bool {
    let Some(numbers) = v.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = numbers.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
}

/// The release and the commit of a version text: `v0.8.0+cb6912a` both, a
/// bare tag the release, anything else with no `+` the commit.
fn split_version(v: &str) -> (Option<String>, Option<String>) {
    if v.is_empty() {
        return (None, None);
    }
    let parts: Vec<&str> = v.split('+').collect();
    if parts.len() == 1 && is_tag(v) {
        return (Some(v.to_string()), None);
    }
    let tehas = (parts.len() == 2).then(|| parts[0].to_string());
    (tehas, parts.last().map(|p| p.to_string()))
}

fn or_null(v: &Option<String>) -> J {
    v.clone().map_or(J::Null, J::str)
}

/// `{last, hour}` of the store's answer, or null when no poll is on record.
fn poll_of(answer: &J) -> J {
    if answer.get("last").is_null() {
        return J::Null;
    }
    obj(vec![
        ("last", answer.get("last").clone()),
        ("hour", answer.get("hour").clone()),
    ])
}

fn object(version: &str, repo: &str, login: &str, poll: J) -> J {
    let (tehas, commit) = split_version(version);
    let millwright = if login.is_empty() {
        J::Null
    } else {
        J::str(login)
    };
    obj(vec![
        ("api", J::num(1.0)),
        ("tehas", or_null(&tehas)),
        ("commit", or_null(&commit)),
        ("repo", J::str(repo)),
        (
            "features",
            J::Arr(FEATURES.iter().map(|f| J::str(*f)).collect()),
        ),
        ("millwright", millwright.clone()),
        ("fixer", millwright),
        ("poll", poll),
    ])
}

/// The lines for a person. The poll line is the store's own words, with its
/// first word brought to the column of the others.
fn lines(version: &str, repo: &str, login: &str, poll_line: &str) -> Vec<String> {
    let (tehas, commit) = split_version(version);
    let release = tehas.unwrap_or_else(|| "unknown".to_string());
    let commit = commit.map_or(String::new(), |c| format!(" ({c})"));
    let mill = if login.is_empty() { "none" } else { login };
    let mut out = vec![
        "api         1".to_string(),
        format!("tehas       {release}{commit}"),
        format!("repo        {repo}"),
        format!("features    {}", FEATURES.join(" ")),
        format!("millwright  {mill}"),
    ];
    if !poll_line.is_empty() {
        out.push(match poll_line.strip_prefix("poll") {
            Some(rest) => format!("poll        {}", rest.trim_start_matches(' ')),
            None => poll_line.to_string(),
        });
    }
    out
}

pub fn main(given: &[String]) -> u8 {
    let json = match words(given) {
        Ok(json) => json,
        Err(rc) => return rc,
    };
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let login = env::var("TEHAS_MILLWRIGHT_LOGIN").unwrap_or_default();
    let version = version_text(&home.join("docs/version.txt"));
    if json {
        let now = tehas_core::time::now().to_string();
        let poll = store::line(&home, &["poll-show", "--api", "--now", &now])
            .ok()
            .and_then(|text| J::parse(&text))
            .map_or(J::Null, |answer| poll_of(&answer));
        println!("{}", object(&version, &repo, &login, poll).dump());
    } else {
        let poll = store::call(&home, &["poll-show"]).ok();
        let line = poll.as_ref().and_then(|v| v["line"].as_str()).unwrap_or("");
        for line in lines(&version, &repo, &login, line) {
            println!("{line}");
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(a: &str, b: &str) -> (Option<String>, Option<String>) {
        let some = |s: &str| (s != "-").then(|| s.to_string());
        (some(a), some(b))
    }

    #[test]
    fn a_version_text_is_split_as_install_wrote_it() {
        assert_eq!(split_version("v0.8.0+cb6912a"), pair("v0.8.0", "cb6912a"));
        assert_eq!(split_version("v0.10.2"), pair("v0.10.2", "-"));
        assert_eq!(split_version("cb6912a"), pair("-", "cb6912a"));
        assert_eq!(split_version(""), pair("-", "-"));
        assert_eq!(split_version("v1.2"), pair("-", "v1.2"));
        assert_eq!(split_version("a+b+c"), pair("-", "c"));
        assert_eq!(split_version("v1.2.3+"), pair("v1.2.3", ""));
        assert_eq!(split_version("+abc"), pair("", "abc"));
    }

    #[test]
    fn the_keys_stay_in_the_order_of_the_api() {
        let poll = J::parse(r#"{"z":1,"a":2}"#).unwrap();
        let printed = object("v1.2.3", "acme/widget", "mill", poll).dump();
        assert!(printed.starts_with(
            r#"{"api":1,"tehas":"v1.2.3","commit":null,"repo":"acme/widget","features":["status","#
        ));
        assert!(printed
            .ends_with(r#""disk"],"millwright":"mill","fixer":"mill","poll":{"z":1,"a":2}}"#));
    }

    #[test]
    fn a_poll_is_on_record_only_with_a_last() {
        assert!(poll_of(&J::Null).is_null());
        let none = J::parse(r#"{"api":1,"last":null,"hour":{"polls":0}}"#).unwrap();
        assert!(poll_of(&none).is_null());
        let one = J::parse(r#"{"ok":true,"last":{"z":1,"at":2},"hour":{"polls":1}}"#).unwrap();
        assert_eq!(
            poll_of(&one).dump(),
            r#"{"last":{"z":1,"at":2},"hour":{"polls":1}}"#
        );
    }

    #[test]
    fn the_poll_line_takes_the_column_of_the_others() {
        let got = lines("cb6912a", "a/b", "", "poll  last 2 s wall");
        assert_eq!(got[1], "tehas       unknown (cb6912a)");
        assert_eq!(got[4], "millwright  none");
        assert_eq!(got[5], "poll        last 2 s wall");
        assert_eq!(lines("", "a/b", "", "").len(), 5);
    }
}
