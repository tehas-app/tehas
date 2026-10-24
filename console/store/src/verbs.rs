//! What the record verbs share: their arguments and the grammar of each value,
//! the cleaning of a value a container produced, the one line they answer
//! with, the store opened for a writer or a reader, the issues stub and the
//! mapping of an error to its exit.

use crate::{approval, cycle, db, history, live, park, plan};
use rusqlite::Connection;
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// The exit status and the line for standard error.
pub type Fail = (u8, String);

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Cycle(cycle::Call),
    Park(park::Call),
    Approval(approval::Call),
    Plan(plan::Call),
    History(history::Call),
    Live(live::Call),
}

/// None for a verb name this part does not know; else the call or the line of
/// a usage error.
pub fn parse(args: &[String]) -> Option<Result<Call, String>> {
    let (name, rest) = args.split_first()?;
    let name = name.as_str();
    if let Some(r) = cycle::parse(name, rest) {
        return Some(r.map(Call::Cycle));
    }
    if let Some(r) = park::parse(name, rest) {
        return Some(r.map(Call::Park));
    }
    if let Some(r) = approval::parse(name, rest) {
        return Some(r.map(Call::Approval));
    }
    if let Some(r) = plan::parse(name, rest) {
        return Some(r.map(Call::Plan));
    }
    if let Some(r) = live::parse(name, rest) {
        return Some(r.map(Call::Live));
    }
    history::parse(name, rest).map(|r| r.map(Call::History))
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Cycle(c) => cycle::run(home, c, now),
        Call::Park(c) => park::run(home, c, now),
        Call::Approval(c) => approval::run(home, c),
        Call::Plan(c) => plan::run(home, c, now),
        Call::History(c) => history::run(home, c),
        Call::Live(c) => live::run(home, c, now),
    }
}

// --- arguments -------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Switch,
    Value,
    Repeat,
}

/// A verb's arguments as given, before any grammar is applied.
pub struct Args {
    pub verb: &'static str,
    pub issue: Option<String>,
    given: Vec<(&'static str, Option<String>)>,
}

/// The word as given, safe for one line: Debug-escaped and cut to 64 bytes.
pub fn shown(word: &str) -> String {
    let mut s = format!("{word:?}");
    if s.len() > 64 {
        let mut end = 64;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

impl Args {
    /// `positional` says whether the verb takes `<issue>`. A value flag takes
    /// the next argument as it is, whatever its first character.
    pub fn parse(
        verb: &'static str,
        rest: &[String],
        flags: &[(&'static str, Kind)],
        positional: bool,
    ) -> Result<Args, String> {
        let mut a = Args {
            verb,
            issue: None,
            given: Vec::new(),
        };
        let mut json = false;
        let mut i = 0;
        while i < rest.len() {
            let arg = rest[i].as_str();
            i += 1;
            if arg == "--json" {
                if json {
                    return Err(a.usage("--json", "given twice"));
                }
                json = true;
                continue;
            }
            if arg.starts_with("--") {
                let Some(&(flag, kind)) = flags.iter().find(|(f, _)| *f == arg) else {
                    return Err(format!("usage {verb}: {} unknown flag", shown(arg)));
                };
                if kind != Kind::Repeat && a.has(flag) {
                    return Err(a.usage(flag, "given twice"));
                }
                let value = if kind == Kind::Switch {
                    None
                } else {
                    let Some(v) = rest.get(i) else {
                        return Err(a.usage(flag, "needs a value"));
                    };
                    i += 1;
                    Some(v.clone())
                };
                a.given.push((flag, value));
                continue;
            }
            if positional && a.issue.is_none() {
                a.issue = Some(arg.to_string());
                continue;
            }
            return Err(format!("usage {verb}: {} unexpected", shown(arg)));
        }
        Ok(a)
    }

    pub fn usage(&self, flag: &str, why: &str) -> String {
        format!("usage {}: {flag} {why}", self.verb)
    }

    pub fn has(&self, flag: &str) -> bool {
        self.given.iter().any(|(f, _)| *f == flag)
    }

    pub fn value(&self, flag: &str) -> Option<&str> {
        self.given
            .iter()
            .find(|(f, _)| *f == flag)
            .and_then(|(_, v)| v.as_deref())
    }

    pub fn values(&self, flag: &str) -> Vec<&str> {
        self.given
            .iter()
            .filter(|(f, _)| *f == flag)
            .filter_map(|(_, v)| v.as_deref())
            .collect()
    }

    pub fn need(&self, flag: &str) -> Result<&str, String> {
        self.value(flag)
            .ok_or_else(|| self.usage(flag, "is required"))
    }

    /// Refuses a flag a mode does not take.
    pub fn only(&self, allowed: &[&str], mode: &str) -> Result<(), String> {
        match self.given.iter().find(|(f, _)| !allowed.contains(f)) {
            Some((f, _)) => Err(self.usage(f, &format!("is not taken with {mode}"))),
            None => Ok(()),
        }
    }

    /// The `<issue>` the verb requires.
    pub fn issue(&self) -> Result<i64, String> {
        let Some(s) = self.issue.as_deref() else {
            return Err(self.usage("<issue>", "is required"));
        };
        number(s).ok_or_else(|| self.usage("<issue>", &format!("is not a number: {}", shown(s))))
    }

    /// `<issue>` or the one switch a show verb takes instead, never both.
    pub fn target(&self, switch: &str) -> Result<Option<i64>, String> {
        match (self.issue.is_some(), self.has(switch)) {
            (true, false) => self.issue().map(Some),
            (false, true) => Ok(None),
            (true, true) => Err(self.usage(switch, "is not taken with <issue>")),
            (false, false) => Err(self.usage("<issue>", &format!("or {switch} is required"))),
        }
    }

    /// A strict value: absent is None, outside its grammar is 64.
    pub fn strict<T>(
        &self,
        flag: &str,
        what: &str,
        grammar: impl Fn(&str) -> Option<T>,
    ) -> Result<Option<T>, String> {
        match self.value(flag) {
            None => Ok(None),
            Some(v) => grammar(v)
                .map(Some)
                .ok_or_else(|| self.usage(flag, &format!("is not {what}"))),
        }
    }

    /// A strict value that is required.
    pub fn strict_need<T>(
        &self,
        flag: &str,
        what: &str,
        grammar: impl Fn(&str) -> Option<T>,
    ) -> Result<T, String> {
        self.need(flag)?;
        self.strict(flag, what, grammar)
            .map(|v| v.expect("the flag was given"))
    }

    /// An enumerated value: the word given is named on refusal.
    pub fn word(&self, flag: &str, set: &[&'static str]) -> Result<Option<&'static str>, String> {
        match self.value(flag) {
            None => Ok(None),
            Some(v) => set
                .iter()
                .find(|w| **w == v)
                .copied()
                .map(Some)
                .ok_or_else(|| {
                    self.usage(
                        flag,
                        &format!("is not one of {}: {}", set.join(", "), shown(v)),
                    )
                }),
        }
    }

    /// An absolute path. Cleaning it is the file read's job.
    pub fn path(&self, flag: &str) -> Result<Option<PathBuf>, String> {
        self.strict(flag, "an absolute path", |v| {
            let p = PathBuf::from(v);
            p.is_absolute().then_some(p)
        })
    }
}

// --- grammars ----------------------------------------------------------------

fn digits(s: &str, max: usize, zero: bool) -> Option<i64> {
    if zero && s == "0" {
        return Some(0);
    }
    let b = s.as_bytes();
    let ok = !b.is_empty()
        && b.len() <= max
        && (b'1'..=b'9').contains(&b[0])
        && b.iter().all(u8::is_ascii_digit);
    if ok {
        s.parse().ok()
    } else {
        None
    }
}

/// An issue, pull request, turn or attempt.
pub fn number(s: &str) -> Option<i64> {
    digits(s, 10, false)
}

/// A count or an exit status.
pub fn count(s: &str) -> Option<i64> {
    digits(s, 9, true)
}

pub fn unix(s: &str) -> Option<i64> {
    digits(s, 11, false)
}

pub fn comment_id(s: &str) -> Option<i64> {
    digits(s, 16, false).filter(|n| *n < (1i64 << 53))
}

fn all(s: &str, min: usize, max: usize, keep: impl Fn(u8) -> bool) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(keep)
}

pub fn account(s: &str) -> Option<String> {
    all(s, 1, 39, |b| b.is_ascii_alphanumeric() || b == b'-').then(|| s.to_string())
}

fn lower_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

pub fn sha256(s: &str) -> Option<String> {
    all(s, 64, 64, lower_hex).then(|| s.to_string())
}

pub fn commit(s: &str) -> Option<String> {
    all(s, 7, 40, lower_hex).then(|| s.to_string())
}

pub fn head_sha(s: &str) -> Option<String> {
    all(s, 40, 40, lower_hex).then(|| s.to_string())
}

pub fn image(s: &str) -> Option<String> {
    s.strip_prefix("sha256:")
        .and_then(sha256)
        .map(|_| s.to_string())
}

// --- cleaned values ----------------------------------------------------------

/// The flags whose value cleaning changed or dropped.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cleaned(Vec<&'static str>);

pub fn word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_.-".contains(&b)
}

pub fn cap_char(b: u8) -> bool {
    word_char(b) || b == b':'
}

pub fn kebab_char(b: u8) -> bool {
    b.is_ascii_lowercase() || b == b'-'
}

impl Cleaned {
    pub fn mark(&mut self, flag: &'static str) {
        if !self.0.contains(&flag) {
            self.0.push(flag);
        }
    }

    /// The characters outside the set removed and the value cut to `max`
    /// bytes. Empty is None.
    pub fn word(
        &mut self,
        flag: &'static str,
        given: Option<&str>,
        keep: fn(u8) -> bool,
        max: usize,
    ) -> Option<String> {
        let given = given?;
        let mut v: String = given
            .chars()
            .filter(|c| c.is_ascii() && keep(*c as u8))
            .collect();
        v.truncate(max);
        if v != given {
            self.mark(flag);
        }
        (!v.is_empty()).then_some(v)
    }

    /// Kept whole when it matches, else dropped.
    pub fn whole(
        &mut self,
        flag: &'static str,
        given: Option<&str>,
        ok: impl Fn(&str) -> bool,
    ) -> Option<String> {
        let given = given?;
        if ok(given) {
            Some(given.to_string())
        } else {
            self.mark(flag);
            None
        }
    }

    pub fn json(&self) -> String {
        j(self.0.clone())
    }
}

// --- output ------------------------------------------------------------------

pub fn j(v: impl Into<Value>) -> String {
    v.into().to_string()
}

/// An object from rendered values, in the order given.
pub fn obj(pairs: &[(&str, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}:{v}", j(*k)))
        .collect();
    format!("{{{}}}", body.join(","))
}

pub fn list(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

/// A row's columns in the order selected, under the names given. A name that
/// ends in `?` is a 0/1 column rendered as a boolean.
pub fn row_json(
    r: &rusqlite::Row,
    names: &[&'static str],
) -> rusqlite::Result<Vec<(&'static str, String)>> {
    use rusqlite::types::ValueRef;
    let mut out = Vec::with_capacity(names.len());
    for (i, &name) in names.iter().enumerate() {
        let v = r.get_ref(i)?;
        out.push(match name.strip_suffix('?') {
            Some(key) => (key, j(matches!(v, ValueRef::Integer(n) if n != 0))),
            None => (
                name,
                match v {
                    ValueRef::Integer(n) => n.to_string(),
                    ValueRef::Real(f) => j(f),
                    ValueRef::Text(t) => j(String::from_utf8_lossy(t).as_ref()),
                    ValueRef::Null | ValueRef::Blob(_) => "null".to_string(),
                },
            ),
        });
    }
    Ok(out)
}

pub fn line(verb: &str, pairs: &[(&str, String)]) -> String {
    let mut all = vec![
        ("api", "1".to_string()),
        ("verb", j(verb)),
        ("ok", "true".to_string()),
    ];
    all.extend(pairs.iter().cloned());
    obj(&all)
}

// --- the store -----------------------------------------------------------------

/// A constraint failure is a row that disagreed: exit 1, `conflict`.
pub fn sql(e: rusqlite::Error) -> Fail {
    if e.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
        return (1, format!("conflict: {e}"));
    }
    crate::line(db::classify(e))
}

pub trait Q<T> {
    fn q(self) -> Result<T, Fail>;
}

impl<T> Q<T> for rusqlite::Result<T> {
    fn q(self) -> Result<T, Fail> {
        self.map_err(sql)
    }
}

/// A precondition the verb states, not met.
pub fn refuse(word: &str, issue: i64, why: &str) -> Fail {
    (1, format!("{word} #{issue}: {why}"))
}

/// A store below the version this binary writes. 0 holds no schema at all;
/// a lower version is one `migrate` has not brought forward yet.
fn no_schema(held: i64) -> Fail {
    let why = if held == 0 {
        "the store holds no schema; run store migrate".to_string()
    } else {
        format!(
            "the store is at schema {held}, this store writes {}; run store migrate",
            db::SCHEMA_VERSION
        )
    };
    crate::line(db::DbError::Unreadable(why))
}

fn schema(conn: &Connection) -> Result<(), Fail> {
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .q()?;
    if v == db::SCHEMA_VERSION {
        Ok(())
    } else {
        Err(no_schema(v))
    }
}

/// A file of no bytes is a store nobody migrated, refused before SQLite gives
/// it a header.
fn not_empty(file: &Path) -> Result<(), Fail> {
    match std::fs::symlink_metadata(file) {
        Ok(m) if m.is_file() && m.len() == 0 => Err(no_schema(0)),
        _ => Ok(()),
    }
}

/// One transaction under the write lock: committed when `f` succeeds, rolled
/// back otherwise.
pub fn write<T>(home: &Path, f: impl FnOnce(&Connection) -> Result<T, Fail>) -> Result<T, Fail> {
    let file = db::path(home);
    not_empty(&file)?;
    let conn = db::lock(&file).map_err(crate::line)?;
    let done = schema(&conn).and_then(|()| f(&conn));
    match done {
        Ok(v) => {
            conn.execute_batch("COMMIT").q()?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// A read with a bounded wait and no write, in one transaction so that every
/// query of the verb sees the same snapshot.
pub fn look<T>(home: &Path, f: impl FnOnce(&Connection) -> Result<T, Fail>) -> Result<T, Fail> {
    let file = db::path(home);
    not_empty(&file)?;
    let conn = db::open_look(&file).map_err(crate::line)?;
    conn.execute_batch("BEGIN").q()?;
    let done = schema(&conn).and_then(|()| f(&conn));
    match done {
        Ok(v) => {
            conn.execute_batch("COMMIT").q()?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// The row every record table references, made when the issue has none. An
/// existing row is never touched.
pub fn stub(conn: &Connection, issue: i64) -> Result<(), Fail> {
    conn.execute(
        "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (?1, 'unreadable', 0, 0)",
        [issue],
    )
    .q()
    .map(|_| ())
}

// --- files ---------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum FileErr {
    /// Not there, a link, not a regular file, or replaced while opened.
    Refused,
    TooLarge,
}

/// A regular file read whole, at most `bound` bytes, with its mtime in Unix
/// seconds.
pub fn read_file(path: &Path, bound: u64) -> Result<(Vec<u8>, i64), FileErr> {
    let before = std::fs::symlink_metadata(path).map_err(|_| FileErr::Refused)?;
    if !before.file_type().is_file() {
        return Err(FileErr::Refused);
    }
    let f = File::open(path).map_err(|_| FileErr::Refused)?;
    let m = f.metadata().map_err(|_| FileErr::Refused)?;
    if m.dev() != before.dev() || m.ino() != before.ino() || !m.is_file() {
        return Err(FileErr::Refused);
    }
    if m.len() > bound {
        return Err(FileErr::TooLarge);
    }
    let mut bytes = Vec::new();
    f.take(bound + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FileErr::Refused)?;
    if bytes.len() as u64 > bound {
        return Err(FileErr::TooLarge);
    }
    Ok((bytes, m.mtime()))
}

/// A text file argument, read before the store is opened. Exit 1 with `word`
/// when it is not a regular file, is over `bound` (`said` names the bound) or
/// is not UTF-8.
pub fn read_text(
    path: &Path,
    bound: u64,
    said: &str,
    issue: i64,
    word: &str,
    what: &str,
) -> Result<String, Fail> {
    match read_file(path, bound) {
        Ok((bytes, _)) => String::from_utf8(bytes)
            .map_err(|_| refuse(word, issue, &format!("{what} is not UTF-8"))),
        Err(FileErr::TooLarge) => Err(refuse(word, issue, &format!("{what} is over {said}"))),
        Err(FileErr::Refused) => Err(refuse(
            word,
            issue,
            &format!("{what} is not a regular file"),
        )),
    }
}
