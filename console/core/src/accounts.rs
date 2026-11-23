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
//! While the file cannot be read with certainty no listed login counts.
//!
//! The key of a pair is the login as GraphQL gives it: REST names a GitHub
//! App `<slug>[bot]` and GraphQL `<slug>`, one account.

use crate::grant::{self, Actor};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;

/// Where the pairs are, under the instance's home.
pub const FILE: &str = crate::home::State::AccountIds.relative();

/// The bound of the file: a list of logins, nothing near it.
const BOUND: u64 = 64 * 1024;

/// A login as GraphQL gives it: one account is gh's `app/<slug>`, REST's
/// `<slug>[bot]` and GraphQL's `<slug>`. A login that is only one of the two
/// marks - `""`, `"[bot]"` or `"app/"` - names nobody and is kept whole, so it
/// matches no account of any list.
pub fn account(login: &str) -> &str {
    match login {
        "" | "[bot]" | "app/" => login,
        login => {
            let login = login.strip_prefix("app/").unwrap_or(login);
            login.strip_suffix("[bot]").unwrap_or(login)
        }
    }
}

/// Whether `login` is the factory's own, the bot's. An empty name matches
/// nothing. Logins compare as `account` gives them, so all three spellings of
/// the app's name are the app's.
pub fn factorys(login: &str, bot: &str) -> bool {
    let (login, bot) = (account(login), account(bot));
    !login.is_empty() && !bot.is_empty() && bot == login
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

/// Why the file cannot be read with certainty: the line it is on, if one,
/// and what is wrong.
#[derive(Debug, PartialEq)]
pub struct Unreadable {
    pub line: Option<usize>,
    pub what: String,
}

impl Unreadable {
    fn at(line: usize, what: impl Into<String>) -> Unreadable {
        Unreadable {
            line: Some(line),
            what: what.into(),
        }
    }

    fn whole(what: impl Into<String>) -> Unreadable {
        Unreadable {
            line: None,
            what: what.into(),
        }
    }

    /// The line that names `file`, what is wrong and how to repair it.
    pub fn says(&self, file: &str) -> String {
        match self.line {
            Some(n) => format!(
                "tehas: {file} cannot be read: line {n} {}. Fix or remove that line, then run tehas update",
                self.what
            ),
            None => format!(
                "tehas: {file} cannot be read: {}. Make it a regular file of `<login> <id>` lines, or remove it, then run tehas update",
                self.what
            ),
        }
    }
}

/// The pairs a file holds, and whether no line of it had its end trimmed.
#[derive(Debug, PartialEq)]
pub struct Pairs {
    pub ids: BTreeMap<String, u64>,
    pub tidy: bool,
}

/// The pairs of a file's bytes. A line may end in spaces, tabs or a carriage
/// return; anything else that is not `<login> <id>` is unreadable, and so is
/// a login given two ids.
pub fn parse_bytes(bytes: &[u8]) -> Result<Pairs, Unreadable> {
    let text = std::str::from_utf8(bytes).map_err(|e| {
        let n = bytes[..e.valid_up_to()]
            .iter()
            .filter(|b| **b == b'\n')
            .count();
        Unreadable::at(n + 1, "is not UTF-8")
    })?;
    let mut ids = BTreeMap::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut tidy = true;
    for (i, raw) in text.split('\n').enumerate() {
        let n = i + 1;
        let line = raw.trim_end_matches([' ', '\t', '\r']);
        tidy &= line.len() == raw.len();
        if line.is_empty() {
            continue;
        }
        let (login, id) = pair(line).map_err(|what| Unreadable::at(n, what))?;
        let key = account(login).to_string();
        match ids.insert(key.clone(), id) {
            Some(was) if was != id => {
                return Err(Unreadable::at(
                    n,
                    format!("gives {key} another id than line {}", seen[&key]),
                ))
            }
            _ => {
                seen.entry(key).or_insert(n);
            }
        }
    }
    Ok(Pairs { ids, tidy })
}

/// The login and id of a line with its end trimmed, or what is wrong.
fn pair(line: &str) -> Result<(&str, u64), &'static str> {
    let Some((login, id)) = line.split_once(' ') else {
        return Err("is not `<login> <id>`: it has no space");
    };
    if login.is_empty() {
        return Err("is not `<login> <id>`: it starts with a space");
    }
    if id.starts_with(' ') {
        return Err("is not `<login> <id>`: more than one space");
    }
    if login.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("is not `<login> <id>`: the login holds a tab or control character");
    }
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("is not `<login> <id>`: the id is not a number");
    }
    id.parse::<u64>()
        .map(|id| (login, id))
        .map_err(|_| "is not `<login> <id>`: the id is too large")
}

/// The pairs of a file's text, as `parse_bytes` reads them.
pub fn parse(text: &str) -> Result<BTreeMap<String, u64>, Unreadable> {
    parse_bytes(text.as_bytes()).map(|p| p.ids)
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

/// The pairs recorded in `home`. An absent file is no pairs, as before the
/// first record.
pub fn load(home: &Path) -> Result<Pairs, Unreadable> {
    use crate::file::ReadError;
    match crate::file::read_regular(&home.join(FILE), BOUND) {
        Ok((bytes, _)) => parse_bytes(&bytes),
        Err(ReadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(Pairs {
            ids: BTreeMap::new(),
            tidy: true,
        }),
        Err(ReadError::Io(e)) => Err(Unreadable::whole(format!("it could not be opened: {e}"))),
        Err(ReadError::NotRegular) => Err(Unreadable::whole("it is not a regular file")),
        Err(ReadError::Replaced) => Err(Unreadable::whole("it was replaced while it was read")),
        Err(ReadError::TooLarge) => Err(Unreadable::whole(format!(
            "it is larger than {} KiB",
            BOUND / 1024
        ))),
    }
}

/// The line a reader says of a file it cannot read.
fn unread(u: &Unreadable) -> String {
    format!("{}. Until then no listed login counts", u.says(FILE))
}

/// The pairs recorded in `home`, for a reader that vouches. A file that
/// cannot be read is said on standard error and gives no pairs, so no listed
/// login counts until it is repaired.
pub fn read(home: &Path) -> BTreeMap<String, u64> {
    load(home).map(|p| p.ids).unwrap_or_else(|u| {
        say_once(unread(&u));
        BTreeMap::new()
    })
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
    /// The file could not be read: what a doubt says instead of its own line.
    unread: Option<String>,
}

impl Accounts {
    pub fn new(listed: Vec<String>, ids: BTreeMap<String, u64>) -> Accounts {
        Accounts {
            listed,
            ids,
            unread: None,
        }
    }

    /// The listed logins of a home whose file cannot be read: none counts.
    pub fn unreadable(listed: Vec<String>, u: &Unreadable) -> Accounts {
        Accounts {
            listed,
            ids: BTreeMap::new(),
            unread: Some(unread(u)),
        }
    }

    /// The settings and the pairs this process was given: TEHAS_ALLOWED_LABELERS,
    /// TEHAS_MILLWRIGHT_LOGIN and the file under TEHAS_HOME. A file that
    /// cannot be read counts no listed login and says so.
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
        if home.is_empty() {
            return Accounts::new(listed, BTreeMap::new());
        }
        match load(Path::new(&home)) {
            Ok(p) => Accounts::new(listed, p.ids),
            Err(u) => Accounts::unreadable(listed, &u),
        }
    }

    /// Whether `login` with `id` counts as the listed account it names. A
    /// login of no list is no claim to doubt.
    pub fn doubt(&self, login: &str, id: Option<u64>) -> Option<Doubt> {
        let a = self.actor(login, id);
        grant::doubt(self.is_listed(login), a.recorded, a.carried)
    }

    fn is_listed(&self, login: &str) -> bool {
        let key = account(login);
        self.listed.iter().any(|l| l == key)
    }

    fn actor<'a>(&self, login: &'a str, id: Option<u64>) -> Actor<&'a str> {
        Actor {
            login,
            recorded: self.ids.get(account(login)).copied(),
            carried: id,
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
                        match &self.unread {
                            Some(line) => say_once(line.clone()),
                            None => say_once(says(&login, id, &doubt)),
                        }
                        let name = grant::counted(&self.actor(&login, id), true)
                            .text()
                            .into_owned();
                        o.insert("login".into(), Value::String(name));
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
            "tehas: {login} acted with the user id {}, not the recorded {recorded}, so it does not count: the login may have changed hands. If it is the same person, run tehas update --accept-id {login}",
            id.unwrap_or_default()
        ),
    }
}

/// Says `line` on standard error, once a process.
fn say_once(line: String) {
    static SAID: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
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
    fn the_three_spellings_of_an_app_are_one_account() {
        for (spelling, bare) in [
            ("app/acme-bot", "acme-bot"),
            ("acme-bot[bot]", "acme-bot"),
            ("acme-bot", "acme-bot"),
            ("app/acme-bot[bot]", "acme-bot"),
            ("alice", "alice"),
            // What names nobody stays as it is: it is no other account's name.
            ("", ""),
            ("[bot]", "[bot]"),
            ("app/", "app/"),
        ] {
            assert_eq!(account(spelling), bare, "{spelling}");
        }
        let one = account("app/acme-bot");
        assert_eq!(one, account("acme-bot[bot]"));
        assert_eq!(one, account("acme-bot"));
        assert_eq!(one, "acme-bot");
    }

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
        assert!(parse("alice\n").is_err());
        assert!(parse("alice x\n").is_err());
        assert!(parse(" 5\n").is_err());
        assert_eq!(parse(""), Ok(BTreeMap::new()));
        assert!(parse_bytes(b"alice 1001\nmill 3001\n").unwrap().tidy);
        // Another order or spelling is no damage.
        assert!(parse_bytes(b"mill[bot] 3001\nalice 1001").unwrap().tidy);
    }

    #[test]
    fn trailing_spaces_and_crlf_read_as_the_same_pairs_not_tidy() {
        let clean = parse("alice 1001\nbob 1002\n").unwrap();
        for text in [
            "alice 1001\r\nbob 1002\r\n",
            "alice 1001  \nbob 1002\t\n",
            "alice 1001 \r\n\r\nbob 1002",
            "alice 1001\n  \nbob 1002\n",
        ] {
            let p = parse_bytes(text.as_bytes()).unwrap();
            assert_eq!(p.ids, clean, "{text:?}");
            assert!(!p.tidy, "{text:?}");
        }
    }

    #[test]
    fn a_line_that_cannot_be_read_with_certainty_is_named() {
        for (text, line, what) in [
            ("alice 1001\nbob\n", 2, "no space"),
            ("alice 1001\nbob  1002\n", 2, "more than one space"),
            ("alice 1001\nbob x1\n", 2, "not a number"),
            ("alice 1001\nbob +1\n", 2, "not a number"),
            ("alice 1001\n\n bob 1\n", 3, "starts with a space"),
            ("alice 1001\nb\tob 1\n", 2, "tab or control"),
            (
                "alice 1001\nbob 1002\nalice 7\n",
                3,
                "another id than line 1",
            ),
        ] {
            let u = parse_bytes(text.as_bytes()).unwrap_err();
            assert_eq!(u.line, Some(line), "{text:?}");
            assert!(u.what.contains(what), "{text:?}: {}", u.what);
        }
        let u = parse_bytes(b"alice 1001\nbob 10\xff2\n").unwrap_err();
        assert_eq!(u.line, Some(2));
        assert!(u.what.contains("UTF-8"));
        // The same pair twice says nothing new.
        assert!(parse("alice 1001\nalice 1001\n").is_ok());
        let said = Unreadable::at(2, "is not `<login> <id>`").says(FILE);
        assert!(said.contains(FILE) && said.contains("line 2"));
        assert!(said.contains("Fix or remove that line, then run tehas update"));
    }

    fn home(name: &str) -> std::path::PathBuf {
        let h = std::env::temp_dir().join(format!("tehas-accounts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(h.join(FILE).parent().unwrap()).unwrap();
        h
    }

    #[test]
    fn an_absent_file_is_no_pairs_and_a_damaged_one_is_unreadable() {
        let h = home("load");
        assert_eq!(
            load(&h),
            Ok(Pairs {
                ids: BTreeMap::new(),
                tidy: true
            })
        );
        let file = h.join(FILE);
        std::fs::write(&file, vec![b'a'; BOUND as usize + 1]).unwrap();
        assert!(load(&h).unwrap_err().what.contains("larger than"));
        std::fs::remove_file(&file).unwrap();
        std::fs::write(h.join("other"), "alice 1001\n").unwrap();
        std::os::unix::fs::symlink(h.join("other"), &file).unwrap();
        let u = load(&h).unwrap_err();
        assert_eq!(u.line, None);
        assert!(u.what.contains("not a regular file"));
        assert!(u.says(FILE).contains("or remove it, then run tehas update"));
        std::fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn nothing_counts_while_the_file_cannot_be_read() {
        let h = home("vouch");
        std::fs::write(h.join(FILE), "alice 1001\nbob  1002\n").unwrap();
        let u = load(&h).unwrap_err();
        assert!(read(&h).is_empty());
        let a = Accounts::unreadable(listed("alice bob", ""), &u);
        assert_eq!(a.doubt("alice", Some(1001)), Some(Doubt::Unrecorded));
        let mut v = json!({"actor": {"login": "alice", "id": 1001}});
        a.vouch(&mut v);
        assert_eq!(v["actor"]["login"], "alice#1001");
        assert!(unread(&u).contains("no listed login counts"));
        // A file with only trailing damage vouches as its pairs say.
        std::fs::write(h.join(FILE), "alice 1001 \r\n").unwrap();
        assert_eq!(read(&h).get("alice"), Some(&1001));
        std::fs::remove_dir_all(&h).unwrap();
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
        assert!(line.contains("may have changed hands"));
        assert!(line.contains("If it is the same person, run tehas update --accept-id alice"));
    }
}
