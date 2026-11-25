//! The accounts that may authorise work, by login and GitHub user id.
//!
//! A login can be renamed, and a freed login registered by someone else, so a
//! login alone does not say who acted. `tehas update` and install.sh resolve
//! each login of TEHAS_ALLOWED_LABELERS and TEHAS_MILLWRIGHT_LOGIN to its user
//! id and record the pairs in `state/account-ids`, one `<realm> <login> <id>`
//! a line, the realm `code-host` or `tickets` (console/ops/src/ids.rs). A
//! 1.x line, `<login> <id>`, is read as the code host's: the update rewrites
//! the file once in the new form, and every write is in it (ports spec,
//! "Identity across suppliers"; R9, expand then contract). Every answer of GitHub a program reads goes
//! through `vouch` first: a listed login whose object does not carry the
//! recorded id is renamed `<login>#<id>`, or `<login>#` when it carries none,
//! which no check of the list matches. A login of no list is left as it is.
//! While the file cannot be read with certainty no listed login counts.
//!
//! The key of a pair is the account's whole login, as `key` gives it: a
//! GitHub App's bot `<slug>[bot]`, a person or an organisation its login. An
//! organisation and the bot of an app with its name are two accounts with two
//! keys. GitHub logins are ASCII and one account in any case, so within a kind
//! the vouch compares them as `folded` gives them: every case variant of a
//! listed login carries its recorded id or does not count.
//!
//! A file written before bots were keyed whole keeps a bot under its bare
//! slug, which is also the key of an account of that name. Where a known bot
//! (the factory's, the millwright, a listed one) has no whole key and its slug
//! has a bare one, the file is not read (`settled`): nothing is guessed, no
//! listed login counts, the dispatcher stops with 78, and `console ids record`
//! settles the pair by the id GitHub gives the bot.

use crate::grant::{self, Actor};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;
use tehas_ports::{AccountKind, ChangeDigest};

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

/// A login as tehas compares it: ASCII case folded, nothing else. The one
/// fold of a login, for the vouch, the allowed list and the ports' adapter.
pub fn folded(login: &str) -> String {
    login.to_ascii_lowercase()
}

/// The key of `login` in the file: a bot's whole login `<slug>[bot]`, for
/// REST's `<slug>[bot]` and gh's `app/<slug>`, and any other login as it is.
/// A login that is only one of the two marks is kept whole and names nobody.
pub fn key(login: &str) -> String {
    match login {
        "" | "[bot]" | "app/" => login.to_string(),
        l => match l.strip_prefix("app/") {
            Some(slug) => bot_key(slug),
            None => l.to_string(),
        },
    }
}

/// The key of the bot whose login is `login`, with `[bot]` or without: the
/// caller knows it is a bot.
pub fn bot_key(login: &str) -> String {
    format!("{}[bot]", account(login))
}

/// Whether a key is a bot's.
fn is_bot(key: &str) -> bool {
    key.len() > "[bot]".len() && key.ends_with("[bot]")
}

/// The id `ids` records under `key`, compared as `folded` gives it.
pub fn id_of(ids: &BTreeMap<String, u64>, key: &str) -> Option<u64> {
    let key = folded(key);
    ids.iter()
        .find(|(l, _)| folded(l) == key)
        .map(|(_, id)| *id)
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
/// what is wrong and how it is repaired.
#[derive(Debug, PartialEq)]
pub struct Unreadable {
    pub line: Option<usize>,
    pub what: String,
    repair: Repair,
}

/// How an unreadable file is repaired.
#[derive(Debug, PartialEq)]
enum Repair {
    /// Fix or remove the line.
    Line,
    /// Make it a regular file of pairs, or remove it.
    File,
    /// Its pairs may be sound: give the factory's user access to it.
    Access,
    /// Read it again.
    Again,
    /// Record the ids again: `console ids record`.
    Record,
}

impl Unreadable {
    fn at(line: usize, what: impl Into<String>) -> Unreadable {
        Unreadable {
            line: Some(line),
            what: what.into(),
            repair: Repair::Line,
        }
    }

    fn whole(what: impl Into<String>, repair: Repair) -> Unreadable {
        Unreadable {
            line: None,
            what: what.into(),
            repair,
        }
    }

    /// The line that names `file`, what is wrong and how to repair it.
    pub fn says(&self, file: &str) -> String {
        let what = &self.what;
        match (self.line, &self.repair) {
            (Some(n), _) => format!(
                "tehas: {file} cannot be read: line {n} {what}. Fix or remove that line, then run tehas update"
            ),
            (None, Repair::Access) => format!(
                "tehas: {file} cannot be read: {what}. Give it back read and write for the factory's user, then run tehas update"
            ),
            (None, Repair::Again) => format!(
                "tehas: {file} cannot be read: {what}. Run tehas update again"
            ),
            (None, Repair::Record) => format!(
                "tehas: {file} cannot be read: {what}. Run console ids record (tehas update runs it)"
            ),
            (None, _) => format!(
                "tehas: {file} cannot be read: {what}. Make it a regular file of `<realm> <login> <id>` lines, or remove it, then run tehas update"
            ),
        }
    }
}

/// The pairs a file holds, the code host's and the tickets realm's, and
/// whether the file is in its written form: no line had its end trimmed and
/// none was in the 1.x form.
#[derive(Debug, PartialEq)]
pub struct Pairs {
    /// The code host's pairs: every check of 2.0 reads these.
    pub ids: BTreeMap<String, u64>,
    /// The tickets realm's pairs, kept as they are: a split setup's.
    pub tickets: BTreeMap<String, u64>,
    pub tidy: bool,
}

/// The realm of a line, as the file spells it.
pub const CODE_HOST: &str = "code-host";
/// The tickets realm, as the file spells it.
pub const TICKETS: &str = "tickets";

/// The pairs of a file's bytes. A line is `<realm> <login> <id>`, or a 1.x
/// line `<login> <id>`, read as the code host's. A line may end in spaces,
/// tabs or a carriage return; anything else is unreadable, and so is a login
/// given two ids in one realm, in one case or two.
pub fn parse_bytes(bytes: &[u8]) -> Result<Pairs, Unreadable> {
    let text = std::str::from_utf8(bytes).map_err(|e| {
        let n = bytes[..e.valid_up_to()]
            .iter()
            .filter(|b| **b == b'\n')
            .count();
        Unreadable::at(n + 1, "is not UTF-8")
    })?;
    let mut ids = BTreeMap::new();
    let mut tickets = BTreeMap::new();
    let mut seen: BTreeMap<(bool, String), (usize, u64)> = BTreeMap::new();
    let mut tidy = true;
    for (i, raw) in text.split('\n').enumerate() {
        let n = i + 1;
        let line = raw.trim_end_matches([' ', '\t', '\r']);
        tidy &= line.len() == raw.len();
        if line.is_empty() {
            continue;
        }
        let (realm, login, id) = entry(line).map_err(|what| Unreadable::at(n, what))?;
        tidy &= realm.is_some();
        let ticket_realm = realm == Some(TICKETS);
        let key = key(login);
        if !crate::settings::rules::is_login(account(&key)) {
            return Err(Unreadable::at(
                n,
                "is not `<realm> <login> <id>`: the login is not a GitHub login",
            ));
        }
        match seen.get(&(ticket_realm, folded(&key))) {
            Some(&(first, was)) if was != id => {
                return Err(Unreadable::at(
                    n,
                    format!("gives {key} another id than line {first}"),
                ))
            }
            Some(_) => {}
            None => {
                seen.insert((ticket_realm, folded(&key)), (n, id));
            }
        }
        if ticket_realm {
            tickets.insert(key, id);
        } else {
            ids.insert(key, id);
        }
    }
    Ok(Pairs { ids, tickets, tidy })
}

/// The realm, login and id of a line with its end trimmed, or what is
/// wrong. A line of one space is the 1.x form, `<login> <id>`, with no
/// realm: the code host's. A line of more is `<realm> <login> <id>`, its
/// realm `code-host` or `tickets`.
fn entry(line: &str) -> Result<(Option<&'static str>, &str, u64), &'static str> {
    if line.starts_with(' ') {
        return Err("is not `<realm> <login> <id>`: it starts with a space");
    }
    if line.contains("  ") {
        return Err("is not `<realm> <login> <id>`: more than one space");
    }
    if line.matches(' ').count() <= 1 {
        return pair(line).map(|(login, id)| (None, login, id));
    }
    let (word, rest) = line.split_once(' ').expect("the line holds a space");
    let realm = match word {
        CODE_HOST => CODE_HOST,
        TICKETS => TICKETS,
        _ => return Err("is not `<realm> <login> <id>`: the realm is not code-host or tickets"),
    };
    pair(rest).map(|(login, id)| (Some(realm), login, id))
}

/// The login and id of a line with its end trimmed, or what is wrong.
fn pair(line: &str) -> Result<(&str, u64), &'static str> {
    let Some((login, id)) = line.split_once(' ') else {
        return Err("is not `<realm> <login> <id>`: it has no space");
    };
    if login.is_empty() {
        return Err("is not `<realm> <login> <id>`: it starts with a space");
    }
    if id.starts_with(' ') {
        return Err("is not `<realm> <login> <id>`: more than one space");
    }
    if login.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("is not `<realm> <login> <id>`: the login holds a tab or control character");
    }
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("is not `<realm> <login> <id>`: the id is not a number");
    }
    id.parse::<u64>()
        .map(|id| (login, id))
        .map_err(|_| "is not `<realm> <login> <id>`: the id is too large")
}

/// The pairs of a file's text, as `parse_bytes` reads them.
pub fn parse(text: &str) -> Result<BTreeMap<String, u64>, Unreadable> {
    parse_bytes(text.as_bytes()).map(|p| p.ids)
}

/// The file's text for these pairs, the code host's then the tickets
/// realm's, each `<realm> <login> <id>`: the one form tehas writes.
pub fn text(ids: &BTreeMap<String, u64>, tickets: &BTreeMap<String, u64>) -> String {
    let lines = |realm: &str, pairs: &BTreeMap<String, u64>| -> String {
        pairs
            .iter()
            .map(|(l, i)| format!("{realm} {l} {i}\n"))
            .collect()
    };
    lines(CODE_HOST, ids) + &lines(TICKETS, tickets)
}

/// The pairs with `new` given the user id of `old`, for an account whose
/// login changed: a GitHub App's slug renamed its bot. The pair of `old`
/// stays. Both are bots, keyed whole. None when nothing is to be added: `old` has no recorded id, or
/// `new` already has one, the same or another, which only `tehas update
/// --accept-id` changes.
pub fn carried(ids: &BTreeMap<String, u64>, old: &str, new: &str) -> Option<BTreeMap<String, u64>> {
    let (old, new) = (bot_key(old), bot_key(new));
    let id = *ids.get(&old)?;
    if account(&new).is_empty() || ids.contains_key(&new) {
        return None;
    }
    let mut out = ids.clone();
    out.insert(new, id);
    Some(out)
}

/// The factory's account as the adapter knows it: the bot's login and the
/// user id `state/account-ids` records for it, the bot account's and not the
/// App's, keyed by the bot's whole login. An ids file that cannot be read,
/// or that is not `settled`, or no id for the login, gives no id: then no
/// account is the factory's own, and a write the site may have taken is
/// settled as unknown, never sent again.
pub fn bot(home: &Path) -> (String, Option<u64>) {
    let login = bot_login(home);
    let id = Some(&login)
        .filter(|l| !account(l).is_empty())
        .and_then(|l| id_of(&settled(home).ok()?.ids, &bot_key(l)));
    (login, id)
}

/// The bot's login as the home keeps it, trimmed; empty when it keeps none.
fn bot_login(home: &Path) -> String {
    use crate::home::{Home, State};
    std::fs::read_to_string(Home::new(home).state(State::BotLogin))
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The bots this process knows of `home`: the factory's, the millwright and
/// the listed logins spelled as bots.
fn known_bots(home: &Path) -> Vec<String> {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let factory = Some(bot_login(home))
        .filter(|l| !account(l).is_empty())
        .map(|l| bot_key(&l));
    let listed = format!(
        "{} {}",
        var("TEHAS_ALLOWED_LABELERS"),
        var("TEHAS_MILLWRIGHT_LOGIN")
    );
    factory
        .into_iter()
        .chain(listed.split_whitespace().map(key).filter(|k| is_bot(k)))
        .collect()
}

/// The first of `bots` whose pair `ids` keeps only under its bare slug: a
/// file written before bots were keyed whole. That key is also an account's
/// of the slug's name, so which account it records is not known.
pub fn ambiguous<'a>(ids: &BTreeMap<String, u64>, bots: &'a [String]) -> Option<&'a str> {
    bots.iter()
        .filter(|b| is_bot(&key(b)))
        .find(|b| id_of(ids, &key(b)).is_none() && id_of(ids, account(b)).is_some())
        .map(String::as_str)
}

/// The pairs of `home` as a reader takes them: `load`, and unreadable while a
/// known bot is recorded only under its bare slug, until `console ids record`
/// settles it.
pub fn settled(home: &Path) -> Result<Pairs, Unreadable> {
    let pairs = load(home)?;
    match ambiguous(&pairs.ids, &known_bots(home)) {
        Some(bot) => Err(Unreadable::whole(
            format!(
                "it keeps {} under the bare name, which may be the bot {bot} or another account of that name",
                account(bot)
            ),
            Repair::Record,
        )),
        None => Ok(pairs),
    }
}

/// The line that stops a reader of `home` whose file keeps a known bot only
/// under its bare slug, or None. Another fault of the file is the readers'
/// own to say.
pub fn unsettled(home: &Path) -> Option<String> {
    match (load(home), settled(home)) {
        (Ok(_), Err(u)) => Some(u.says(FILE)),
        _ => None,
    }
}

/// The pairs recorded in `home`. An absent file is no pairs, as before the
/// first record. A file that is there with no pairs is unreadable: it is
/// written whole or not at all, so it is never empty while it holds pairs.
pub fn load(home: &Path) -> Result<Pairs, Unreadable> {
    match crate::file::read_regular(&home.join(FILE), BOUND) {
        Ok((bytes, _)) => match parse_bytes(&bytes)? {
            p if p.ids.is_empty() => Err(Unreadable::whole("it holds no pairs", Repair::File)),
            p => Ok(p),
        },
        Err(e) => match unreadable(e) {
            Some(u) => Err(u),
            None => Ok(Pairs {
                ids: BTreeMap::new(),
                tickets: BTreeMap::new(),
                tidy: true,
            }),
        },
    }
}

/// What a failed read of the file says, or None when it is absent.
fn unreadable(e: crate::file::ReadError) -> Option<Unreadable> {
    use crate::file::ReadError;
    use std::io::ErrorKind;
    Some(match e {
        ReadError::Io(e) if e.kind() == ErrorKind::NotFound => return None,
        ReadError::Io(e) if e.kind() == ErrorKind::PermissionDenied => {
            Unreadable::whole("it cannot be read by this user", Repair::Access)
        }
        ReadError::Io(_) => Unreadable::whole("it could not be read", Repair::Access),
        ReadError::NotRegular => Unreadable::whole("it is not a regular file", Repair::File),
        ReadError::Replaced => {
            Unreadable::whole("it was replaced while it was read", Repair::Again)
        }
        ReadError::TooLarge => Unreadable::whole(
            format!("it is larger than {} KiB", BOUND / 1024),
            Repair::File,
        ),
    })
}

/// The line a reader says of a file it cannot read.
fn unread(u: &Unreadable) -> String {
    format!("{}. Until then no listed login counts", u.says(FILE))
}

/// The pairs recorded in `home`, for a reader that vouches. A file that
/// cannot be read is said on standard error and gives no pairs, so no listed
/// login counts until it is repaired.
pub fn read(home: &Path) -> BTreeMap<String, u64> {
    settled(home).map(|p| p.ids).unwrap_or_else(|u| {
        say_once(unread(&u));
        BTreeMap::new()
    })
}

/// The logins of the two settings, as `key` gives them.
pub fn listed(allowed: &str, millwright: &str) -> Vec<String> {
    let mut all: Vec<String> = allowed
        .split_whitespace()
        .chain(Some(millwright).filter(|m| !m.is_empty()))
        .map(key)
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

/// What an answer says of an account's kind: a bot, another kind, or
/// nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Bot,
    Other,
    Unknown,
}

impl Kind {
    /// The kind GitHub names, GraphQL's `__typename` or REST's `type`.
    fn named(name: Option<&str>) -> Kind {
        match name {
            Some("Bot") => Kind::Bot,
            Some(_) => Kind::Other,
            None => Kind::Unknown,
        }
    }
}

/// The listed logins and the pairs recorded for them, keyed as `key` and
/// `folded` give them.
#[derive(Debug, Default)]
pub struct Accounts {
    listed: Vec<String>,
    ids: BTreeMap<String, u64>,
    /// The file could not be read: what a doubt says instead of its own line.
    unread: Option<String>,
}

impl Accounts {
    /// Two pairs of one login in two cases that disagree record no id for
    /// it, so it does not count until `tehas update` settles it: it keeps
    /// one pair a login, and a changed id stops it (console/ops/src/ids.rs).
    pub fn new(listed: Vec<String>, ids: BTreeMap<String, u64>) -> Accounts {
        let listed = listed.iter().map(|l| folded(l)).collect();
        let mut folds: BTreeMap<String, Option<u64>> = BTreeMap::new();
        for (login, id) in ids {
            folds
                .entry(folded(&login))
                .and_modify(|had| *had = had.filter(|h| *h == id))
                .or_insert(Some(id));
        }
        let ids = folds
            .into_iter()
            .filter_map(|(l, id)| Some((l, id?)))
            .collect();
        Accounts {
            listed,
            ids,
            unread: None,
        }
    }

    /// The listed logins of a home whose file cannot be read: none counts.
    pub fn unreadable(listed: Vec<String>, u: &Unreadable) -> Accounts {
        Accounts {
            listed: listed.iter().map(|l| folded(l)).collect(),
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
        match settled(Path::new(&home)) {
            Ok(p) => Accounts::new(listed, p.ids),
            Err(u) => Accounts::unreadable(listed, &u),
        }
    }

    /// Why the recorded ids could not be read, when they could not: then no
    /// listed login counts.
    pub fn unread(&self) -> Option<&str> {
        self.unread.as_deref()
    }

    /// Whether `login` with `id` counts as the listed account it names, of
    /// a kind the answer does not say. A login of no list is no claim to
    /// doubt.
    pub fn doubt(&self, login: &str, id: Option<u64>) -> Option<Doubt> {
        self.doubt_as(login, Kind::Unknown, id)
    }

    /// `doubt` of an account of `kind`.
    pub fn doubt_as(&self, login: &str, kind: Kind, id: Option<u64>) -> Option<Doubt> {
        let a = self.actor(login, kind, id);
        grant::doubt(self.is_listed(login), a.recorded, a.carried)
    }

    /// Whether a change counts as the factory's own, the ports' form of
    /// [`own_pull`]: anyone can open a change from a fork with any branch name
    /// and any body, so no other one stands for an issue. `from_fork` is the
    /// site's word that the head is in another project, GitHub's
    /// `isCrossRepository`; the port gives no head project beside it, so it
    /// stands for both of own_pull's checks of the head's repository.
    ///
    /// The author is the factory's own (`AccountKind::Ours`, which the
    /// adapter answers only for a bot whose id is the bot's recorded one), or
    /// a login `allowed` takes, compared as `folded` gives it, whose id is the
    /// one recorded for it. No recorded id, no id on the account, or a file
    /// that could not be read: not own.
    pub fn own_change(&self, c: &ChangeDigest, allowed: impl Fn(&str) -> bool) -> bool {
        let Some(author) = c.author.as_ref().filter(|_| !c.from_fork) else {
            return false;
        };
        if self.unread.is_some() {
            return false;
        }
        if author.kind == AccountKind::Ours {
            return true;
        }
        let login = account(author.login.as_str());
        if login.is_empty() || !allowed(login) {
            return false;
        }
        let carried = author
            .id
            .as_ref()
            .and_then(|i| i.as_str().parse::<u64>().ok());
        let kind = match author.kind {
            AccountKind::Bot => Kind::Bot,
            AccountKind::Person => Kind::Other,
            _ => Kind::Unknown,
        };
        match (self.recorded(author.login.as_str(), kind), carried) {
            (Some(recorded), Some(id)) => recorded == id,
            _ => false,
        }
    }

    /// Whether a listed login has the bare name of `login`, of any kind: an
    /// answer may give a bot without `[bot]`, so each is checked.
    fn is_listed(&self, login: &str) -> bool {
        let bare = folded(account(login));
        self.listed.iter().any(|l| account(l) == bare)
    }

    /// The id recorded for `login` of `kind`, when a login of that kind is
    /// listed: an account of another kind with the listed name is not the
    /// listed one. A login spelled as a bot is one. Of a kind the answer does
    /// not say, the listed login of its name says which key, when only one
    /// kind is listed; a person and a bot of one name both listed give none.
    fn recorded(&self, login: &str, kind: Kind) -> Option<u64> {
        let bare = folded(account(login));
        let bot = format!("{bare}[bot]");
        let k = match kind {
            _ if is_bot(&key(login)) => bot,
            Kind::Bot => bot,
            Kind::Other => folded(login),
            Kind::Unknown => match (self.listed.contains(&bare), self.listed.contains(&bot)) {
                (true, false) => bare,
                (false, true) => bot,
                _ => return None,
            },
        };
        self.listed
            .contains(&k)
            .then(|| self.ids.get(&k).copied())
            .flatten()
    }

    fn actor<'a>(&self, login: &'a str, kind: Kind, id: Option<u64>) -> Actor<&'a str> {
        Actor {
            login,
            recorded: self.recorded(login, kind),
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
                    let kind = Kind::named(
                        o.get("__typename")
                            .or_else(|| o.get("type"))
                            .and_then(Value::as_str),
                    );
                    if let Some(doubt) = self.doubt_as(&login, kind, id) {
                        match &self.unread {
                            Some(line) => say_once(line.clone()),
                            None => say_once(says(&login, id, &doubt)),
                        }
                        let name = grant::counted(&self.actor(&login, kind, id), true)
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

    /// A login is one account in any case, so every case variant of a
    /// listed login is checked against the recorded id.
    #[test]
    fn a_case_variant_of_a_listed_login_counts_only_with_its_recorded_id() {
        let a = Accounts::new(listed("alice", ""), parse("alice 5\n").unwrap());
        assert_eq!(
            a.doubt("Alice", Some(99)),
            Some(Doubt::Other { recorded: 5 })
        );
        assert_eq!(a.doubt("ALICE", None), Some(Doubt::NoId));
        assert_eq!(a.doubt("Alice", Some(5)), None);
        assert_eq!(a.doubt("alıce", Some(99)), None, "no ASCII variant");
        // A list or a file in another case is the same account.
        let b = Accounts::new(listed("Alice", ""), parse("ALICE 5\n").unwrap());
        assert_eq!(
            b.doubt("alice", Some(99)),
            Some(Doubt::Other { recorded: 5 })
        );
        assert_eq!(b.doubt("alice", Some(5)), None);
        let mut v = json!([{"actor": {"login": "Alice", "id": 99}},
            {"actor": {"login": "ALICE"}}, {"actor": {"login": "Alice", "id": 5}}]);
        a.vouch(&mut v);
        let logins: Vec<&str> = (0..3)
            .map(|i| v[i]["actor"]["login"].as_str().unwrap())
            .collect();
        assert_eq!(logins, ["Alice#99", "ALICE#", "Alice"]);
    }

    /// Two entries of a list that differ only in case are one account. Two
    /// recorded ids for it that disagree vouch for neither.
    #[test]
    fn ids_recorded_for_one_login_in_two_cases_must_agree() {
        let same = Accounts::new(
            listed("alice Alice", ""),
            parse("alice 5\nAlice 5\n").unwrap(),
        );
        assert_eq!(same.doubt("ALICE", Some(5)), None);
        // The file reader refuses such a file; a map given otherwise still
        // vouches for neither.
        let split = Accounts::new(
            listed("alice Alice", ""),
            BTreeMap::from([("alice".into(), 5), ("Alice".into(), 99)]),
        );
        for id in [5, 99] {
            assert_eq!(
                split.doubt("alice", Some(id)),
                Some(Doubt::Unrecorded),
                "{id}"
            );
            assert_eq!(
                split.doubt("Alice", Some(id)),
                Some(Doubt::Unrecorded),
                "{id}"
            );
        }
    }

    #[test]
    fn the_fold_is_ascii_case_alone() {
        assert_eq!(folded("Alice"), "alice");
        assert_eq!(folded("ACME-Bot[bot]"), "acme-bot[bot]");
        assert_eq!(folded("alıce"), "alıce");
        assert_eq!(folded(""), "");
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
        let ids = parse("alice 1001\napp/mill 3001\n").unwrap();
        let none = BTreeMap::new();
        assert_eq!(
            text(&ids, &none),
            "code-host alice 1001\ncode-host mill[bot] 3001\n"
        );
        assert!(parse("alice\n").is_err());
        assert!(parse("alice x\n").is_err());
        assert!(parse(" 5\n").is_err());
        assert_eq!(parse(""), Ok(BTreeMap::new()));
        assert!(
            parse_bytes(b"code-host alice 1001\ncode-host mill 3001\n")
                .unwrap()
                .tidy
        );
        // Another order or spelling is no damage.
        assert!(
            parse_bytes(b"code-host mill[bot] 3001\ncode-host alice 1001")
                .unwrap()
                .tidy
        );
    }

    /// Expand then contract (R9): a 1.x line, `<login> <id>`, reads as the
    /// code host's pair, beside a line of the new form; the file is then not
    /// in its written form, so the update writes it once, every line with
    /// its realm. A tickets realm's pair is kept apart and written back.
    #[test]
    fn a_1x_line_reads_as_the_code_hosts_and_is_written_with_its_realm() {
        let old = parse_bytes(b"alice 1001\nmill 3001\n").unwrap();
        let new = parse_bytes(b"code-host alice 1001\ncode-host mill 3001\n").unwrap();
        assert_eq!(old.ids, new.ids);
        assert!(old.tickets.is_empty());
        assert!(!old.tidy && new.tidy);
        let mixed = parse_bytes(b"alice 1001\ncode-host bob 1002\ntickets alice 77\n").unwrap();
        assert!(!mixed.tidy);
        assert_eq!(mixed.ids.get("alice"), Some(&1001));
        assert_eq!(mixed.ids.get("bob"), Some(&1002));
        assert_eq!(mixed.tickets.get("alice"), Some(&77));
        let written = text(&mixed.ids, &mixed.tickets);
        assert_eq!(
            written,
            "code-host alice 1001\ncode-host bob 1002\ntickets alice 77\n"
        );
        let again = parse_bytes(written.as_bytes()).unwrap();
        assert!(again.tidy);
        assert_eq!((again.ids, again.tickets), (mixed.ids, mixed.tickets));
        // One login, one id a realm: in two realms it may differ, in one it
        // may not, whichever form names it.
        let u = parse_bytes(b"alice 1001\ncode-host Alice 7\n").unwrap_err();
        assert_eq!(u.line, Some(2));
        assert!(u.what.contains("another id than line 1"), "{}", u.what);
    }

    /// Any shape but the two forms is unreadable: no listed login counts
    /// until the line is fixed, and the line says how.
    #[test]
    fn a_line_of_another_shape_is_unreadable() {
        for (text, what) in [
            ("jira alice 1001\n", "the realm is not code-host or tickets"),
            (
                "Code-Host alice 1001\n",
                "the realm is not code-host or tickets",
            ),
            ("code-host alice 1001 extra\n", "not a number"),
            ("code-host alice\n", "not a number"),
            ("tickets alice x\n", "not a number"),
        ] {
            let u = parse_bytes(text.as_bytes()).unwrap_err();
            assert_eq!(u.line, Some(1), "{text:?}");
            assert!(u.what.contains(what), "{text:?}: {}", u.what);
            assert!(u
                .says(FILE)
                .contains("Fix or remove that line, then run tehas update"));
        }
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
        // The same pair twice says nothing new, in any case.
        assert!(parse("alice 1001\nalice 1001\n").is_ok());
        assert!(parse("alice 1001\nAlice 1001\n").is_ok());
        let said = Unreadable::at(2, "is not `<realm> <login> <id>`").says(FILE);
        assert!(said.contains(FILE) && said.contains("line 2"));
        assert!(said.contains("Fix or remove that line, then run tehas update"));
    }

    #[test]
    fn a_login_given_another_id_in_another_case_is_unreadable() {
        let u = parse_bytes(b"alice 5\nbob 7\nAlice 99\n").unwrap_err();
        assert_eq!(u.line, Some(3));
        assert!(
            u.what.contains("gives Alice another id than line 1"),
            "{}",
            u.what
        );
        assert!(u.says(FILE).contains("Fix or remove that line"));
        let u = parse_bytes(b"app/Acme 5\nacme[bot] 6\n").unwrap_err();
        assert_eq!(u.line, Some(2));
    }

    #[test]
    fn a_login_github_would_not_give_is_named() {
        for text in [
            "\u{feff}alice 1001\nbob 1002\n",
            "al\u{200b}ice 1001\nbob 1002\n",
            "[bot] 1001\n",
            "-alice 1001\n",
        ] {
            let u = parse_bytes(text.as_bytes()).unwrap_err();
            assert_eq!(u.line, Some(1), "{text:?}");
            assert!(
                u.what.contains("not a GitHub login"),
                "{text:?}: {}",
                u.what
            );
        }
        assert!(parse("app/acme 1\nacme-2[bot] 2\n").is_ok());
    }

    #[test]
    fn what_cannot_be_read_says_its_own_repair() {
        use crate::file::ReadError;
        use std::io::{Error, ErrorKind};
        assert!(unreadable(ReadError::Io(Error::from(ErrorKind::NotFound))).is_none());
        let denied = unreadable(ReadError::Io(Error::from(ErrorKind::PermissionDenied)))
            .unwrap()
            .says(FILE);
        assert!(denied.contains("cannot be read by this user"), "{denied}");
        assert!(
            denied.contains("read and write for the factory's user"),
            "{denied}"
        );
        assert!(
            !denied.contains("remove") && !denied.contains("os error"),
            "{denied}"
        );
        let other = unreadable(ReadError::Io(Error::other("disk")))
            .unwrap()
            .says(FILE);
        assert!(
            !other.contains("remove") && !other.contains("disk"),
            "{other}"
        );
        for e in [ReadError::NotRegular, ReadError::TooLarge] {
            let said = unreadable(e).unwrap().says(FILE);
            assert!(
                said.contains("or remove it, then run tehas update"),
                "{said}"
            );
        }
        let again = unreadable(ReadError::Replaced).unwrap().says(FILE);
        assert!(again.contains("Run tehas update again"), "{again}");
    }

    #[test]
    fn a_file_that_is_there_with_no_pairs_is_unreadable() {
        let h = home("empty");
        for text in ["", "  \n", "\n\r\n"] {
            std::fs::write(h.join(FILE), text).unwrap();
            let u = load(&h).unwrap_err();
            assert_eq!(u.line, None, "{text:?}");
            assert!(u.what.contains("holds no pairs"), "{text:?}");
            assert!(u.says(FILE).contains("remove it, then run tehas update"));
        }
        std::fs::remove_dir_all(&h).unwrap();
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
                tickets: BTreeMap::new(),
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
        let had = parse("alice 1001\nacme-old[bot] 4242\n").unwrap();
        let ids = carried(&had, "acme-old[bot]", "acme-new[bot]").unwrap();
        assert_eq!(
            text(&ids, &BTreeMap::new()),
            "code-host acme-new[bot] 4242\ncode-host acme-old[bot] 4242\ncode-host alice 1001\n"
        );
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
        let other = parse("acme-old[bot] 4242\nacme-new[bot] 7\n").unwrap();
        assert_eq!(carried(&other, "acme-old", "acme-new"), None);
        // A bare key is not known to be the bot's: nothing is carried.
        let old = parse("acme-old 4242\n").unwrap();
        assert_eq!(carried(&old, "acme-old[bot]", "acme-new[bot]"), None);
    }

    /// An organisation and the bot of an app with its name are two accounts:
    /// each is keyed by its whole login and resolves to its own id.
    #[test]
    fn an_organisation_and_the_bot_of_its_name_each_resolve_to_themselves() {
        let ids = parse("tehas-app 344514095\ntehas-app[bot] 347290281\n").unwrap();
        assert_eq!(id_of(&ids, "tehas-app"), Some(344514095));
        assert_eq!(id_of(&ids, &key("tehas-app[bot]")), Some(347290281));
        assert_eq!(id_of(&ids, &key("app/Tehas-App")), Some(347290281));
        let a = Accounts::new(listed("", "tehas-app[bot]"), ids.clone());
        let mut v = json!([
            {"actor": {"__typename": "Bot", "login": "tehas-app", "databaseId": 347290281}},
            {"actor": {"__typename": "Organization", "login": "tehas-app", "databaseId": 344514095}},
            {"actor": {"type": "Bot", "login": "tehas-app[bot]", "id": 344514095}},
            {"actor": {"login": "tehas-app", "databaseId": 347290281}},
        ]);
        a.vouch(&mut v);
        let logins: Vec<&str> = (0..4)
            .map(|i| v[i]["actor"]["login"].as_str().unwrap())
            .collect();
        assert_eq!(
            logins,
            [
                "tehas-app",
                "tehas-app#344514095",
                "tehas-app[bot]#344514095",
                "tehas-app"
            ]
        );
        assert_eq!(ambiguous(&ids, &["tehas-app[bot]".to_string()]), None);
        // Both listed, each counts with its own id.
        let both = Accounts::new(listed("tehas-app", "tehas-app[bot]"), ids);
        assert_eq!(
            both.doubt_as("tehas-app", Kind::Other, Some(344514095)),
            None
        );
        assert_eq!(both.doubt_as("tehas-app", Kind::Bot, Some(347290281)), None);
        assert_eq!(
            both.doubt_as("tehas-app", Kind::Bot, Some(344514095)),
            Some(Doubt::Other {
                recorded: 347290281
            })
        );
    }

    /// A person and a bot of one name: the fold holds within a kind, and an
    /// answer that names no kind resolves only while one kind is listed.
    #[test]
    fn a_person_and_a_bot_of_one_name_are_two_accounts() {
        let ids = parse("Jane 1001\njane[bot] 2002\n").unwrap();
        let both = Accounts::new(listed("jane", "jane[bot]"), ids.clone());
        assert_eq!(both.doubt_as("JANE", Kind::Other, Some(1001)), None);
        assert_eq!(both.doubt_as("jane", Kind::Bot, Some(2002)), None);
        assert_eq!(both.doubt_as("Jane[bot]", Kind::Unknown, Some(2002)), None);
        assert_eq!(
            both.doubt_as("jane", Kind::Bot, Some(1001)),
            Some(Doubt::Other { recorded: 2002 })
        );
        assert_eq!(
            both.doubt_as("jane", Kind::Other, Some(2002)),
            Some(Doubt::Other { recorded: 1001 })
        );
        assert_eq!(both.doubt("jane", Some(1001)), Some(Doubt::Unrecorded));
        let person = Accounts::new(listed("jane", ""), ids.clone());
        assert_eq!(person.doubt("jane", Some(1001)), None);
        assert_eq!(
            person.doubt("jane", Some(2002)),
            Some(Doubt::Other { recorded: 1001 })
        );
        let bot = Accounts::new(listed("", "jane[bot]"), ids);
        assert_eq!(bot.doubt("jane", Some(2002)), None);
        assert_eq!(
            bot.doubt("jane", Some(1001)),
            Some(Doubt::Other { recorded: 2002 })
        );
        // The person's pair is no bot's, and a person is not the listed bot.
        let only = parse("jane 1001\n").unwrap();
        let bot = Accounts::new(listed("", "jane[bot]"), only);
        assert_eq!(
            bot.doubt_as("jane", Kind::Bot, Some(1001)),
            Some(Doubt::Unrecorded)
        );
        assert_eq!(
            bot.doubt_as("jane", Kind::Other, Some(1001)),
            Some(Doubt::Unrecorded)
        );
    }

    /// A file of before bots were keyed whole keeps a bot under its bare
    /// slug: a known bot with no whole key whose slug has a bare one.
    #[test]
    fn a_bot_kept_only_under_its_bare_slug_is_ambiguous() {
        let bots = ["tehas-app[bot]".to_string()];
        let old = parse("alice 1\ntehas-app 347290281\n").unwrap();
        assert_eq!(ambiguous(&old, &bots), Some("tehas-app[bot]"));
        // A 2.0 line with its realm keeps the bare slug the same way.
        let realm = parse("code-host alice 1\ncode-host tehas-app 347290281\n").unwrap();
        assert_eq!(ambiguous(&realm, &bots), Some("tehas-app[bot]"));
        let cased = parse("Tehas-App 347290281\n").unwrap();
        assert_eq!(ambiguous(&cased, &bots), Some("tehas-app[bot]"));
        let settled = parse("tehas-app 344514095\ntehas-app[bot] 347290281\n").unwrap();
        assert_eq!(ambiguous(&settled, &bots), None);
        assert_eq!(ambiguous(&parse("alice 1\n").unwrap(), &bots), None);
        // A person of the name is no bot.
        assert_eq!(ambiguous(&old, &["tehas-app".to_string()]), None);
    }

    #[test]
    fn a_doubt_names_the_login_and_both_ids() {
        let line = says("alice", Some(66), &Doubt::Other { recorded: 1001 });
        assert!(line.contains("alice") && line.contains("66") && line.contains("1001"));
        assert!(line.contains("may have changed hands"));
        assert!(line.contains("If it is the same person, run tehas update --accept-id alice"));
    }

    /// A change of this project by `login` of `kind` carrying `id`.
    fn change(login: &str, kind: tehas_ports::AccountKind, id: Option<&str>) -> ChangeDigest {
        use tehas_ports::{
            Account, AccountId, At, Branch, ChangeNo, ChangeState, CommitId, Login, Realm, WebLink,
        };
        let mut c = ChangeDigest::new(
            ChangeNo::new(9).unwrap(),
            ChangeState::Open,
            Branch::new("feature/issue-7-a").unwrap(),
            CommitId::new("a".repeat(40)).unwrap(),
            Branch::new("main").unwrap(),
            CommitId::new("b".repeat(40)).unwrap(),
            At::new(1),
            At::new(1),
            WebLink::new("https://site.example/c/9").unwrap(),
        );
        let mut a = Account::new(Realm::CodeHost, Login::new(login), kind);
        a.id = id.map(AccountId::new);
        c.author = Some(a);
        c
    }

    fn with_ids(pairs: &[(&str, u64)]) -> Accounts {
        let ids = pairs.iter().map(|(l, i)| (l.to_string(), *i)).collect();
        Accounts::new(vec!["alice".into()], ids)
    }

    #[test]
    fn a_change_is_own_by_the_factorys_account_or_an_allowed_login_with_its_recorded_id() {
        use tehas_ports::AccountKind::{Bot, Ours, Person};
        let accounts = with_ids(&[("alice", 42), ("tehas-bot", 7)]);
        let allowed = |l: &str| folded(l) == "alice";
        let own = |c: &ChangeDigest| accounts.own_change(c, allowed);
        // The factory's own account, as the adapter vouches for it.
        assert!(own(&change("tehas-bot", Ours, Some("7"))));
        // An allowed login with its recorded id, in any case.
        assert!(own(&change("alice", Person, Some("42"))));
        assert!(own(&change("Alice", Person, Some("42"))));
        // A fork's change, whoever opened it: its head is in another project.
        let mut fork = change("alice", Person, Some("42"));
        fork.from_fork = true;
        assert!(!own(&fork));
        let mut ours_fork = change("tehas-bot", Ours, Some("7"));
        ours_fork.from_fork = true;
        assert!(!own(&ours_fork));
        // Another login, allowed or not.
        assert!(!own(&change("mallory", Person, Some("42"))));
        // An allowed login with another id, or with none.
        assert!(!own(&change("alice", Person, Some("43"))));
        assert!(!own(&change("alice", Person, None)));
        assert!(!own(&change("alice", Person, Some("not-a-number"))));
        // The bot's login without the adapter's word that it is the bot's
        // account: a bot of that name is not the factory.
        assert!(!own(&change("tehas-bot", Bot, Some("7"))));
        // No author.
        let mut nobody = change("alice", Person, Some("42"));
        nobody.author = None;
        assert!(!own(&nobody));
    }

    #[test]
    fn an_allowed_login_with_no_recorded_id_is_not_own() {
        let accounts = with_ids(&[("bob", 5)]);
        let c = change("alice", tehas_ports::AccountKind::Person, Some("42"));
        assert!(!accounts.own_change(&c, |l| folded(l) == "alice"));
    }

    #[test]
    fn while_the_ids_cannot_be_read_no_change_is_own() {
        let u = Unreadable::whole("is not a regular file", Repair::File);
        let accounts = Accounts::unreadable(vec!["alice".into()], &u);
        let allowed = |l: &str| folded(l) == "alice";
        for c in [
            change("alice", tehas_ports::AccountKind::Person, Some("42")),
            change("tehas-bot", tehas_ports::AccountKind::Ours, Some("7")),
        ] {
            assert!(!accounts.own_change(&c, allowed), "{c:?}");
        }
    }
}

#[cfg(test)]
mod bot_tests {
    use super::{bot, unsettled};
    use crate::home::{Home, State};
    use crate::testdir::TestDir;
    use std::fs;

    fn home_with(login: Option<&str>, ids: Option<&str>) -> TestDir {
        let d = TestDir::new("accounts-bot");
        let home = Home::new(d.path());
        if let Some(l) = login {
            let p = home.state(State::BotLogin);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, l).unwrap();
        }
        if let Some(i) = ids {
            let p = home.state(State::AccountIds);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, i).unwrap();
        }
        d
    }

    /// The adapter knows the factory's account by the bot account's id the
    /// factory recorded, its key the login GraphQL gives, any case.
    #[test]
    fn the_factorys_account_is_the_bots_login_and_its_recorded_id() {
        let d = home_with(
            Some("tehas-bot[bot]\n"),
            Some("alice 42\nTehas-Bot[bot] 7\n"),
        );
        assert_eq!(bot(d.path()), ("tehas-bot[bot]".to_string(), Some(7)));
        let side = home_with(
            Some("tehas-bot[bot]"),
            Some("tehas-bot 6\ntehas-bot[bot] 7\n"),
        );
        assert_eq!(bot(side.path()).1, Some(7));
        assert_eq!(unsettled(side.path()), None);
    }

    /// A file that keeps the bot only under its bare slug records no id the
    /// factory can stand on, and says to run `console ids record`.
    #[test]
    fn a_bot_kept_under_its_bare_slug_is_unrecorded_until_the_ids_are_recorded() {
        let d = home_with(Some("tehas-bot[bot]"), Some("alice 42\ntehas-bot 7\n"));
        assert_eq!(bot(d.path()), ("tehas-bot[bot]".to_string(), None));
        let line = unsettled(d.path()).unwrap();
        assert!(line.contains(super::FILE), "{line}");
        assert!(line.contains("tehas-bot[bot]"), "{line}");
        assert!(line.contains("Run console ids record"), "{line}");
        assert!(super::read(d.path()).is_empty());
        // A file that cannot be read is no bare key: the readers say it.
        let realm = home_with(Some("tehas-bot[bot]"), Some("code-host tehas-bot 7\n"));
        assert_eq!(bot(realm.path()).1, None);
        assert!(unsettled(realm.path()).is_some());
        let whole = home_with(Some("tehas-bot[bot]"), Some("code-host tehas-bot[bot] 7\n"));
        assert_eq!(bot(whole.path()).1, Some(7));
        let bad = home_with(Some("tehas-bot[bot]"), Some("tehas-bot x\n"));
        assert_eq!(unsettled(bad.path()), None);
        let none = home_with(Some("tehas-bot[bot]"), None);
        assert_eq!(unsettled(none.path()), None);
    }

    /// No id the factory can stand on: no account is its own, and a write
    /// the site may have taken settles as unknown.
    #[test]
    fn without_a_recorded_id_the_factory_has_no_account_of_its_own() {
        let none = home_with(Some("tehas-bot[bot]"), Some("alice 42\n"));
        assert_eq!(bot(none.path()).1, None);
        let missing = home_with(Some("tehas-bot[bot]"), None);
        assert_eq!(bot(missing.path()).1, None);
        let unreadable = home_with(Some("tehas-bot[bot]"), Some("tehas-bot not-a-number\n"));
        assert_eq!(bot(unreadable.path()).1, None);
        let no_login = home_with(None, Some("tehas-bot 7\n"));
        assert_eq!(bot(no_login.path()), (String::new(), None));
    }
}
