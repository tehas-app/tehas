//! The accounts that may authorise work, by login and GitHub user id.
//!
//! A login can be renamed, and a freed login registered by someone else, so a
//! login alone does not say who acted. `tehas update` and install.sh resolve
//! each login of TEHAS_ALLOWED_LABELERS and TEHAS_MILLWRIGHT_LOGIN to its user
//! id and record the pairs in `state/account-ids`, one `<login> <id>` a line
//! (console/src/accounts.rs). Every answer of GitHub a program reads goes
//! through `vouch` first: a listed login whose object does not carry the
//! recorded id is renamed `<login>#<id>`, or `<login>#` when it carries none,
//! which no check of the list matches. A login of no list is left as it is.
//!
//! The key of a pair is the login as GraphQL gives it: REST names a GitHub
//! App `<slug>[bot]` and GraphQL `<slug>`, one account.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;

/// Where the pairs are, under the instance's home.
pub const FILE: &str = "state/account-ids";

/// The bound of the file: a list of logins, nothing near it.
const BOUND: u64 = 64 * 1024;

/// A login as GraphQL gives it.
pub fn account(login: &str) -> &str {
    login.strip_suffix("[bot]").unwrap_or(login)
}

/// Whether `login` is the factory's own, the bot's. An empty name matches
/// nothing. Logins compare as `account` gives them, and gh's JSON
/// `app/<slug>` is the app's too.
pub fn factorys(login: &str, bot: &str) -> bool {
    let bare = |l: &str| account(l.strip_prefix("app/").unwrap_or(l)).to_string();
    let login = bare(login);
    !login.is_empty() && !bot.is_empty() && bare(bot) == login
}

/// Whether a pull request is the factory's own to count: its branch is in
/// `repo` and the bot or an account `allowed` takes opened it. Anyone can open
/// a pull request from a fork with any branch name and any body, so no other
/// one stands for an issue. `p` is GraphQL's node: `isCrossRepository`,
/// `headRepository { nameWithOwner }` and `author { login }`.
pub fn own_pull(p: &Value, repo: &str, bot: &str, allowed: impl Fn(&str) -> bool) -> bool {
    let s = |ptr: &str| p.pointer(ptr).and_then(Value::as_str).unwrap_or("");
    let author = s("/author/login");
    p.get("isCrossRepository").and_then(Value::as_bool) == Some(false)
        && !repo.is_empty()
        && s("/headRepository/nameWithOwner").eq_ignore_ascii_case(repo)
        && !author.is_empty()
        && (factorys(author, bot) || allowed(account(author)))
}

/// The pairs of a file's text. None when a line is not `<login> <id>`.
pub fn parse(text: &str) -> Option<BTreeMap<String, u64>> {
    let mut ids = BTreeMap::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (login, id) = line.split_once(' ')?;
        let id = id.parse::<u64>().ok().filter(|_| !login.is_empty())?;
        ids.insert(account(login).to_string(), id);
    }
    Some(ids)
}

/// The file's text for these pairs.
pub fn text(ids: &BTreeMap<String, u64>) -> String {
    ids.iter().map(|(l, i)| format!("{l} {i}\n")).collect()
}

/// The pairs with `new` given the user id of `old`, for an account whose
/// login changed: a GitHub App's slug renamed its bot. The pair of `old`
/// stays. None when nothing is to be added: `old` has no recorded id, or
/// `new` already has one, the same or another, which only `tehas update
/// --accept-id` changes.
pub fn carried(ids: &BTreeMap<String, u64>, old: &str, new: &str) -> Option<BTreeMap<String, u64>> {
    let (old, new) = (account(old), account(new));
    let id = *ids.get(old)?;
    if new.is_empty() || ids.contains_key(new) {
        return None;
    }
    let mut out = ids.clone();
    out.insert(new.to_string(), id);
    Some(out)
}

/// The pairs recorded in `home`. An absent or unreadable file is no pairs, so
/// every listed login fails to count until one is recorded.
pub fn read(home: &Path) -> BTreeMap<String, u64> {
    crate::file::read_regular(&home.join(FILE), BOUND)
        .ok()
        .and_then(|(b, _)| parse(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

/// The logins of the two settings, as `account` gives them.
pub fn listed(allowed: &str, millwright: &str) -> Vec<String> {
    let mut all: Vec<String> = allowed
        .split_whitespace()
        .chain(Some(millwright).filter(|m| !m.is_empty()))
        .map(|l| account(l).to_string())
        .collect();
    all.sort();
    all.dedup();
    all
}

/// Why an account that came with `id` does not count, or None when it counts
/// as its login says.
#[derive(Debug, PartialEq)]
pub enum Doubt {
    /// The object carried no id.
    NoId,
    /// Nothing is recorded for the login.
    Unrecorded,
    /// Another id than the recorded one.
    Other { recorded: u64 },
}

/// The listed logins and the pairs recorded for them.
#[derive(Debug, Default)]
pub struct Accounts {
    listed: Vec<String>,
    ids: BTreeMap<String, u64>,
}

impl Accounts {
    pub fn new(listed: Vec<String>, ids: BTreeMap<String, u64>) -> Accounts {
        Accounts { listed, ids }
    }

    /// The settings and the pairs this process was given: TEHAS_ALLOWED_LABELERS,
    /// TEHAS_MILLWRIGHT_LOGIN and the file under TEHAS_HOME.
    pub fn from_env() -> Accounts {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let listed = listed(
            &var("TEHAS_ALLOWED_LABELERS"),
            &var("TEHAS_MILLWRIGHT_LOGIN"),
        );
        if listed.is_empty() {
            return Accounts::default();
        }
        let home = var("TEHAS_HOME");
        let ids = if home.is_empty() {
            BTreeMap::new()
        } else {
            read(Path::new(&home))
        };
        Accounts { listed, ids }
    }

    /// Whether `login` with `id` counts as the listed account it names. A
    /// login of no list is no claim to doubt.
    pub fn doubt(&self, login: &str, id: Option<u64>) -> Option<Doubt> {
        let key = account(login);
        if !self.listed.iter().any(|l| l == key) {
            return None;
        }
        match (self.ids.get(key), id) {
            (_, None) => Some(Doubt::NoId),
            (None, _) => Some(Doubt::Unrecorded),
            (Some(r), Some(i)) if *r == i => None,
            (Some(r), Some(_)) => Some(Doubt::Other { recorded: *r }),
        }
    }

    /// Every object of `v` with a `login`: a listed login that does not come
    /// with its recorded id is renamed, and said once on standard error. The id
    /// is REST's `id` or GraphQL's `databaseId`; a GraphQL node `id` is text and
    /// no user id.
    pub fn vouch(&self, v: &mut Value) {
        if self.listed.is_empty() {
            return;
        }
        match v {
            Value::Array(a) => a.iter_mut().for_each(|x| self.vouch(x)),
            Value::Object(o) => {
                if let Some(login) = o.get("login").and_then(Value::as_str).map(str::to_string) {
                    let id = o
                        .get("databaseId")
                        .and_then(Value::as_u64)
                        .or_else(|| o.get("id").and_then(Value::as_u64));
                    if let Some(doubt) = self.doubt(&login, id) {
                        warn(&login, id, &doubt);
                        let mark = id.map(|i| i.to_string()).unwrap_or_default();
                        o.insert("login".into(), Value::String(format!("{login}#{mark}")));
                    }
                }
                o.values_mut().for_each(|x| self.vouch(x));
            }
            _ => {}
        }
    }
}

/// `Accounts::from_env().vouch(v)`.
pub fn vouch(v: &mut Value) {
    Accounts::from_env().vouch(v);
}

/// The line a doubt says, once a process for each login, id and reason.
pub fn says(login: &str, id: Option<u64>, doubt: &Doubt) -> String {
    match doubt {
        Doubt::NoId => format!("tehas: an action of {login} carries no user id, so it does not count"),
        Doubt::Unrecorded => format!(
            "tehas: no user id is recorded for {login} in {FILE}, so its actions do not count; run tehas update"
        ),
        Doubt::Other { recorded } => format!(
            "tehas: {login} acted with the user id {}, not the recorded {recorded}, so it does not count; if the account was renamed, run tehas update --accept-id {login}",
            id.unwrap_or_default()
        ),
    }
}

fn warn(login: &str, id: Option<u64>, doubt: &Doubt) {
    static SAID: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let line = says(login, id, doubt);
    if SAID
        .lock()
        .map(|mut s| s.insert(line.clone()))
        .unwrap_or(true)
    {
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_factorys_own_is_the_bot() {
        let bot = "tehas-app-bot[bot]";
        for who in ["tehas-app-bot", "tehas-app-bot[bot]", "app/tehas-app-bot"] {
            assert!(factorys(who, bot), "{who}");
        }
        assert!(!factorys("alice", bot));
        assert!(!factorys("tehas-cycle", bot));
        assert!(!factorys("", bot));
        assert!(!factorys("", ""));
        assert!(!factorys("alice", ""));
        assert!(factorys("tehas-bot", "tehas-bot"));
    }

    #[test]
    fn a_pull_request_counts_only_from_the_repository_and_its_accounts() {
        let own = json!({"isCrossRepository": false,
            "headRepository": {"nameWithOwner": "acme/widget"},
            "author": {"login": "alice"}});
        let alice = |a: &str| a == "alice";
        assert!(own_pull(&own, "acme/widget", "bot", alice));
        assert!(own_pull(&own, "Acme/Widget", "bot", alice));
        assert!(!own_pull(&own, "", "bot", alice));
        assert!(!own_pull(&own, "acme/other", "bot", alice));
        assert!(!own_pull(&own, "acme/widget", "bot", |_| false));
        let mut bot = own.clone();
        bot["author"] = json!({"login": "app/bot"});
        assert!(own_pull(&bot, "acme/widget", "bot", |_| false));
        let mut fork = own.clone();
        fork["isCrossRepository"] = json!(true);
        assert!(!own_pull(&fork, "acme/widget", "bot", alice));
        let mut unknown = own.clone();
        unknown.as_object_mut().unwrap().remove("isCrossRepository");
        assert!(!own_pull(&unknown, "acme/widget", "bot", alice));
        let mut nobody = own.clone();
        nobody["author"] = Value::Null;
        assert!(!own_pull(&nobody, "acme/widget", "", |_| true));
    }

    fn alice() -> Accounts {
        Accounts::new(
            listed("alice bob", "mill[bot]"),
            parse("alice 1001\nmill[bot] 3001\n").unwrap(),
        )
    }

    #[test]
    fn a_listed_login_counts_only_with_its_recorded_id() {
        let a = alice();
        assert_eq!(a.doubt("alice", Some(1001)), None);
        assert_eq!(
            a.doubt("alice", Some(7)),
            Some(Doubt::Other { recorded: 1001 })
        );
        assert_eq!(a.doubt("alice", None), Some(Doubt::NoId));
        assert_eq!(a.doubt("bob", Some(1002)), Some(Doubt::Unrecorded));
        assert_eq!(a.doubt("mill", Some(3001)), None);
        assert_eq!(a.doubt("mill[bot]", Some(3001)), None);
        assert_eq!(a.doubt("mallory", None), None);
    }

    #[test]
    fn vouch_renames_what_does_not_count_and_keeps_the_rest() {
        let mut v = json!([
            {"actor": {"login": "alice", "id": 1001}},
            {"actor": {"login": "alice", "id": 66}},
            {"author": {"login": "alice", "databaseId": 1001}},
            {"author": {"login": "alice", "id": "U_kgDO"}},
            {"user": {"login": "bob", "id": 1002}},
            {"user": {"login": "mallory"}},
            {"editor": {"login": "mill", "databaseId": 3001}},
        ]);
        alice().vouch(&mut v);
        let logins: Vec<&str> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                e.as_object().unwrap().values().next().unwrap()["login"]
                    .as_str()
                    .unwrap()
            })
            .collect();
        assert_eq!(
            logins,
            ["alice", "alice#66", "alice", "alice#", "bob#1002", "mallory", "mill"]
        );
        let mut again = v.clone();
        alice().vouch(&mut again);
        assert_eq!(again, v);
    }

    #[test]
    fn no_list_vouches_for_nothing() {
        let mut v = json!({"actor": {"login": "alice"}});
        Accounts::default().vouch(&mut v);
        assert_eq!(v["actor"]["login"], "alice");
    }

    #[test]
    fn the_file_is_one_pair_a_line() {
        let ids = parse("alice 1001\nmill[bot] 3001\n").unwrap();
        assert_eq!(text(&ids), "alice 1001\nmill 3001\n");
        assert_eq!(parse("alice\n"), None);
        assert_eq!(parse("alice x\n"), None);
        assert_eq!(parse(" 5\n"), None);
        assert_eq!(parse(""), Some(BTreeMap::new()));
    }

    #[test]
    fn a_renamed_bot_keeps_its_id_under_the_new_login() {
        let had = parse("alice 1001\nacme-old 4242\n").unwrap();
        let ids = carried(&had, "acme-old[bot]", "acme-new[bot]").unwrap();
        assert_eq!(text(&ids), "acme-new 4242\nacme-old 4242\nalice 1001\n");
        let bot = |ids| Accounts::new(listed("alice", "acme-new[bot]"), ids);
        let mut v = json!({"author": {"login": "acme-new", "databaseId": 4242}});
        bot(had.clone()).vouch(&mut v);
        assert_eq!(v["author"]["login"], "acme-new#4242");
        let mut v = json!({"author": {"login": "acme-new", "databaseId": 4242}});
        bot(ids.clone()).vouch(&mut v);
        assert_eq!(v["author"]["login"], "acme-new");
        // Nothing to carry, or a pair already there, adds nothing.
        assert_eq!(carried(&had, "gone[bot]", "acme-new[bot]"), None);
        assert_eq!(carried(&ids, "acme-old", "acme-new"), None);
        let other = parse("acme-old 4242\nacme-new 7\n").unwrap();
        assert_eq!(carried(&other, "acme-old", "acme-new"), None);
    }

    #[test]
    fn a_doubt_names_the_login_and_both_ids() {
        let line = says("alice", Some(66), &Doubt::Other { recorded: 1001 });
        assert!(line.contains("alice") && line.contains("66") && line.contains("1001"));
        assert!(line.contains("tehas update --accept-id alice"));
    }
}
