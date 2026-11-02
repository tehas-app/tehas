//! What the record verbs share: their arguments and the grammar of each value,
//! the cleaning of a value a container produced, the one line they answer
//! with, the store opened for a writer or a reader, the issues stub and the
//! mapping of an error to its exit.

use crate::{approval, cycle, db, history, journal, live, park, plan, poll, upkeep};
use rusqlite::Connection;
use serde_json::Value;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tehas_core::file::{read_regular, ReadError};
pub use tehas_core::grammar::count;
use tehas_core::grammar::{digits, is_hex, is_image};

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
    Upkeep(upkeep::Call),
    Poll(poll::Call),
    Journal(journal::Call),
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
    if let Some(r) = upkeep::parse(name, rest) {
        return Some(r.map(Call::Upkeep));
    }
    if let Some(r) = poll::parse(name, rest) {
        return Some(r.map(Call::Poll));
    }
    if let Some(r) = journal::parse(name, rest) {
        return Some(r.map(Call::Journal));
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
        Call::Upkeep(c) => upkeep::run(home, c, now),
        Call::Poll(c) => poll::run(home, c, now),
        Call::Journal(c) => journal::run(home, c, now),
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

    /// `words` are the pairs of one word: the spelling of today, which is what
    /// comes back and the only one a refusal prints, and the old spelling of
    /// #686 taken beside it. The pair is one value, so an alias cannot stand
    /// beside the wrong word, and no row ever carries the old spelling. #686:
    /// part 5 drops each alias and leaves the current word alone.
    pub fn word(
        &self,
        flag: &str,
        words: &[(&'static str, &'static str)],
    ) -> Result<Option<&'static str>, String> {
        let Some(given) = self.value(flag) else {
            return Ok(None);
        };
        if let Some((current, _)) = words.iter().find(|(w, a)| *w == given || *a == given) {
            return Ok(Some(current));
        }
        let current: Vec<&str> = words.iter().map(|(w, _)| *w).collect();
        Err(self.usage(
            flag,
            &format!("is not one of {}: {}", current.join(", "), shown(given)),
        ))
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

/// An issue, pull request, turn or attempt.
pub fn number(s: &str) -> Option<i64> {
    digits(s, 10, false)
}

/// The number of a start: as the shell took it, never 0 and at most 9 digits.
pub fn start_number(s: &str) -> Option<i64> {
    digits(s, 9, false)
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

pub fn sha256(s: &str) -> Option<String> {
    is_hex(s, 64, 64).then(|| s.to_string())
}

pub fn commit(s: &str) -> Option<String> {
    is_hex(s, 7, 40).then(|| s.to_string())
}

pub fn head_sha(s: &str) -> Option<String> {
    is_hex(s, 40, 40).then(|| s.to_string())
}

pub fn image(s: &str) -> Option<String> {
    is_image(s).then(|| s.to_string())
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

/// The value, unless it is null or false: what a reader takes as none on record.
pub fn given(v: &Value) -> Option<&Value> {
    Some(v).filter(|v| !v.is_null() && *v != &Value::Bool(false))
}

/// A view's refusal of what the store itself answered: it is damage.
pub fn damaged(what: &str) -> Fail {
    crate::line(crate::db::DbError::Corrupt(what.to_string()))
}

/// One of this store's own answers, as a view reads it.
pub fn parsed(answer: &str) -> Result<Value, Fail> {
    serde_json::from_str(answer).map_err(|_| damaged("an answer of the store is not JSON"))
}

/// A time on record, as the record files wrote it.
pub fn view_time(v: &Value) -> Result<String, Fail> {
    v.as_i64()
        .map(tehas_core::time::iso)
        .ok_or_else(|| damaged("a time on record is not a time"))
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
    // Inside a batch the store is open already, in the transaction that gives
    // every call of the batch one snapshot; the batch's reads run their queries
    // on it and ask for no transaction of their own.
    if let Some(conn) = reading() {
        return schema(conn).and_then(|()| f(conn));
    }
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

/// Several reads of `store batch`, read from one opening of the store and one
/// transaction, so all of them see the same snapshot (src/batch.rs).
///
/// The calls arrive parsed by `parse_read`, which answers only `READS`, so
/// nothing this function runs can be a write: it holds no `Call` of the
/// argument grammar at all. Each read is run by the same verb the single call
/// runs, with one difference: the `look` of that verb finds this thread inside
/// a read and answers from this connection instead of opening the file again.
/// A call that fails is answered with its own exit and its own line and the
/// batch goes on, because one issue whose row is damaged is no reason for a
/// poll to lose the other reads.
pub fn look_many(home: &Path, calls: &[ReadCall]) -> Result<Vec<(i64, String)>, Fail> {
    let file = db::path(home);
    not_empty(&file)?;
    let conn = db::open_look(&file).map_err(crate::line)?;
    conn.execute_batch("BEGIN").q()?;
    // The schema first, as `look` reads it: one answer of a store nobody
    // migrated, not one per call. The window in which `look` answers from this
    // connection is the scope of the `Reading` made below: whatever the reads
    // do, and however they leave it, the thread is outside a read again once
    // that value is gone.
    let started = schema(&conn).map(|()| {
        let reading = Reading::new(&conn);
        let answers = calls
            .iter()
            .map(|call| match call {
                ReadCall::PlanShow(n) => plan::show(home, Some(*n)),
                ReadCall::ParkShow(n) => park::show(home, Some(*n), false),
                ReadCall::QuestionKnown(n, id) => park::known(home, *n, *id),
                ReadCall::ApprovalCurrent(n) => approval::current(home, *n),
            })
            .map(|r: Result<String, Fail>| match r {
                Ok(line) => (0i64, line),
                Err((rc, why)) => (i64::from(rc), why),
            })
            .collect::<Vec<_>>();
        // Given up before the transaction closes: no verb is left holding a
        // connection that has been committed.
        drop(reading);
        answers
    });
    let done = started.and_then(|answers| conn.execute_batch("COMMIT").q().map(|()| answers));
    match done {
        Ok(answers) => Ok(answers),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// The four reads `store batch` answers, named by the verbs of `batch::READS`:
/// the four arms are the whole of the type, so an element of a request reaches a
/// verb through this type or not at all. A write is not expressible as a
/// `ReadCall`, so `look_many` cannot be handed one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadCall {
    PlanShow(i64),
    ParkShow(i64),
    QuestionKnown(i64, i64),
    ApprovalCurrent(i64),
}

/// The read grammar of `store batch`: the four verbs of `batch::READS` and
/// nothing else, taken from the same `READS` the batch checks. A name outside
/// the four is refused by its first argument alone -- the line of the verb it
/// is, or `usage <verb>: unknown verb` for a name the store does not have --
/// before its other arguments are asked anything, so a write can never be run
/// from a batch. A name inside them is parsed by that verb's own grammar, so
/// the 64 usage line of a call that stops at its arguments is the line the
/// single call prints, and only the four arms above can reach `look_many`.
pub fn parse_read(args: &[String]) -> Result<ReadCall, (u8, String)> {
    let name = args
        .first()
        .map(String::as_str)
        .ok_or((64u8, "batch: a call holds no verb".to_string()))?;
    // The name decides first: outside the four, nothing else of the call is
    // read, and the refusal is the line the single command gives for it.
    if !crate::batch::is_read(name) {
        return Err(refusal_of(name, args));
    }
    let rest = &args[1..];
    // Each verb's own grammar answers for it; `said` gives the unknown name the
    // line the single command gives it, and `usage_rc` the 64 or the 1.
    match name {
        "plan-show" => said(plan::read_plan_show(rest), name).map(ReadCall::PlanShow),
        "park-show" => said(park::read_park_show(rest), name).map(ReadCall::ParkShow),
        "question-known" => said(park::read_question_known(rest), name)
            .map(|(n, id)| ReadCall::QuestionKnown(n, id)),
        "approval-current" => {
            said(approval::read_approval_current(rest), name).map(ReadCall::ApprovalCurrent)
        }
        // `is_read` holds four names and the four are matched above.
        _ => Err((64u8, format!("usage {name}: unknown verb"))),
    }
}

/// The answer of a verb's own grammar to a call of the batch: its value, the
/// line of a call that stops at its arguments, or the line of a name that verb
/// does not have.
fn said<T>(given: Option<Result<T, String>>, name: &str) -> Result<T, (u8, String)> {
    given
        .ok_or_else(|| (64u8, format!("usage {name}: unknown verb")))
        .and_then(|r| r.map_err(usage_rc))
}

/// The refusal of a call whose verb is none of the four reads: the exit of a
/// read of the store and the line naming the verb and the issue it was asked
/// about, which is the shape a caller that logs failures sorts on.
fn refusal_of(name: &str, args: &[String]) -> (u8, String) {
    // A verb outside the four is refused by its name alone, without its other
    // arguments being asked anything of the store: the line names the verb and
    // the issue it was asked about, and the exit is that of a read of the store
    // -- 1 -- for a verb of the store, and the 64 of a name the store does not
    // have at all. Whether a name belongs to the store is answered by the one
    // grammar of `parse`, so a call is judged by the same rules here as alone.
    match parse(args) {
        Some(_) if !crate::batch::is_read(name) => (
            1u8,
            format!("{name} #{}: not a read of a batch", args_issue(args)),
        ),
        None => (64u8, format!("usage {name}: unknown verb")),
        // One of the four, reached by a path the parser above does not take.
        Some(Ok(_)) => (
            1u8,
            format!("{name} #{}: not a read of a batch", args_issue(args)),
        ),
        // The name is one of the four and the arguments do not parse, so the
        // call stops at them: the usage line and the 64 of the single command.
        Some(Err(why)) => usage_rc(why),
    }
}

/// The exit of a line the store's own grammar refused: 64 for a call that stops
/// at its arguments, and 1 for the verb's own refusal of the row it was asked
/// about, which is the exit of the read made alone. A caller that reads the word
/// of a failure reads the same word, and the same exit, either way.
fn usage_rc(why: String) -> (u8, String) {
    if why.starts_with("usage ") {
        (64u8, why)
    } else {
        (1u8, why)
    }
}

/// The issue a call names for the line of a refusal: the digits of its second
/// argument, or 0 for a call that names none, as the store's own refusals of a
/// call without an issue read.
fn args_issue(args: &[String]) -> i64 {
    args.get(1)
        .filter(|a| a.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|a| a.parse().ok())
        .unwrap_or(0)
}

// The connection of the read this thread is inside, held by the `Reading` of
// the batch that set it. A thread with no `Reading` reads the store the way it
// always did, by opening it.
thread_local! {
    static READING: std::cell::Cell<Option<*const Connection>> = const {
        std::cell::Cell::new(None)
    };
}

/// The window in which `look` answers from a connection that is open already.
/// Setting is done by `new` and clearing by `Drop`, so the window is the scope
/// of the value and a read that leaves it early -- by a `?` or a panic -- still
/// leaves the thread outside a read.
struct Reading<'a> {
    _bound: std::marker::PhantomData<&'a Connection>,
}

impl<'a> Reading<'a> {
    fn new(conn: &'a Connection) -> Reading<'a> {
        // The borrow checker keeps `conn` alive for as long as the value does,
        // which is the whole of the window; the cell holds the address rather
        // than the reference because a thread-local cannot own one.
        READING.with(|cell| cell.set(Some(conn as *const Connection)));
        Reading {
            _bound: std::marker::PhantomData,
        }
    }
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        READING.with(|cell| cell.set(None));
    }
}

/// Whether this thread is inside a batch's read, and so whether `look` must
/// open the store again or has it already.
fn reading() -> Option<&'static Connection> {
    READING.with(|cell| cell.get().map(|p| unsafe { &*p }))
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
    match read_regular(path, bound) {
        Ok((bytes, m)) => Ok((bytes, m.mtime())),
        Err(ReadError::TooLarge) => Err(FileErr::TooLarge),
        Err(_) => Err(FileErr::Refused),
    }
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
