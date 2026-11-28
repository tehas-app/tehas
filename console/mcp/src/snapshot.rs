//! The sessions snapshot the host writes to ~/tehas/mcp/sessions.jsonl: a
//! header line, then one session per line, newest first. Never published.
//!
//! A line is read into a [`Session`]: its scalars by key and its structured
//! parts, so the readers of the snapshot pass typed rows and not documents.
//! A key of a line that holds an object, an array or null is held as nothing,
//! which is what every reader of a line has always done with it; a scalar is
//! held as the file wrote it, even where its type is not the one the answer's
//! schema names for that key, because the answer check is what refuses such a
//! row and a reader must not answer as if the key were not there.

use crate::tools::{ToolError, STALE_SECS};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tehas_core::file::READ_BOUND;
use tehas_core::snapshot::{API, SESSIONS_SHAPE};
use tehas_core::time::from_iso;

pub const FILE: &str = "sessions.jsonl";

/// A snapshot's scalar: a word, a count, a large count, a fraction or a
/// flag, kept as the file wrote it. Only scalars reach an answer: a key that
/// holds an object, an array or null is held as nothing, and a scalar of
/// another type than the schema of the answer names for its key travels as
/// it is, because it is the row the answer's check refuses.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Text(String),
    Count(i64),
    Big(u64),
    /// A number that is not a whole one, with what it would read as a signed
    /// count: `Value::as_i64` reads a whole-valued fraction as one, so a
    /// reader that once asked for a signed count must still see it.
    Decimal(f64, Option<i64>),
    Flag(bool),
}

impl Serialize for Scalar {
    /// A number is a number, however it is held: the fraction serialises as
    /// the figure the file wrote, not as its parts.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Scalar::Text(t) => s.serialize_str(t),
            Scalar::Count(n) => s.serialize_i64(*n),
            Scalar::Big(n) => s.serialize_u64(*n),
            Scalar::Decimal(f, _) => s.serialize_f64(*f),
            Scalar::Flag(b) => s.serialize_bool(*b),
        }
    }
}

impl<'de> Deserialize<'de> for Scalar {
    /// Exactly the scalars a JSON document holds, each as its own kind: a
    /// number that is not a whole number is a fraction even when it stands
    /// for a whole value, and null, an object or an array is no scalar.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(match v {
            Value::String(s) => Scalar::Text(s),
            Value::Bool(b) => Scalar::Flag(b),
            Value::Number(n) => {
                // `is_f64` says what the document wrote: `as_i64` would read
                // a whole-valued fraction as a count, and the answer would
                // serve it as an integer the schema now passes.
                if n.is_f64() {
                    Scalar::Decimal(n.as_f64().unwrap_or_default(), n.as_i64())
                } else if let Some(u) = n.as_u64() {
                    Scalar::Big(u)
                } else if let Some(i) = n.as_i64() {
                    Scalar::Count(i)
                } else {
                    Scalar::Decimal(n.as_f64().unwrap_or_default(), n.as_i64())
                }
            }
            _ => return Err(serde::de::Error::custom("not a scalar")),
        })
    }
}

impl Scalar {
    /// The word, when it is one.
    pub fn text(&self) -> Option<&str> {
        match self {
            Scalar::Text(s) => Some(s),
            _ => None,
        }
    }

    /// The count as a whole number from 0: a negative count, a fraction, a
    /// word and a flag are no count.
    pub fn whole(&self) -> Option<u64> {
        match self {
            Scalar::Big(n) => Some(*n),
            _ => None,
        }
    }

    /// The count as a signed number, as `Value::as_i64` reads it: what is
    /// written as a whole number, and a fraction that stands for one, but not
    /// a whole number that does not fit, a word or a flag.
    pub fn count(&self) -> Option<i64> {
        match self {
            Scalar::Count(n) => Some(*n),
            Scalar::Big(n) => i64::try_from(*n).ok(),
            Scalar::Decimal(_, c) => *c,
            _ => None,
        }
    }
}

/// A field that is a scalar or nothing: the reading of a typed field that
/// holds no value where the file holds null, an object or an array. The
/// reading never fails: a value that is no scalar is no value, not a bad file.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Fact(pub Option<Scalar>);

impl<'de> Deserialize<'de> for Fact {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Fact(serde_json::from_value(Value::deserialize(d)?).ok()))
    }
}

impl Fact {
    pub(crate) fn whole(&self) -> Option<u64> {
        self.0.as_ref().and_then(Scalar::whole)
    }

    pub(crate) fn count(&self) -> Option<i64> {
        self.0.as_ref().and_then(Scalar::count)
    }
}

/// A typed field that reads as nothing where the JSON holds no value, null
/// or one of another type: a word of another type is no word, a count of
/// another type is no count.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Loose<T>(pub Option<T>);

impl<T> Loose<T> {
    pub fn get(&self) -> Option<&T> {
        self.0.as_ref()
    }

    pub fn into_inner(self) -> Option<T> {
        self.0
    }
}

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for Loose<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(Loose(serde_json::from_value(v).ok()))
    }
}

/// The members of an array, each read as a `T`: what is no array, and a
/// member that is no `T`, holds nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum Many<T> {
    #[default]
    Absent,
    List(Vec<T>),
}

impl<T> Many<T> {
    pub fn list(&self) -> &[T] {
        match self {
            Many::Absent => &[],
            Many::List(l) => l,
        }
    }
}

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for Many<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let Some(a) = v.as_array() else {
            return Ok(Many::Absent);
        };
        Ok(Many::List(
            a.iter()
                .filter_map(|e| serde_json::from_value(e.clone()).ok())
                .collect(),
        ))
    }
}

/// One log a session's line lists: the word `factory_log` takes, the one
/// file name the line holds for it, and its size in bytes. A word that is no
/// word, a name that is no name and a size that is no whole number read as
/// nothing, as every reader of them has always read them.
#[derive(Debug, Clone, Default)]
pub struct LogRow {
    word: Loose<String>,
    name: Loose<String>,
    bytes: Loose<u64>,
}

impl LogRow {
    /// A log found on disk by its stamps, as a running cycle has them: the
    /// word, the name the stamp rule built, and the size.
    pub(crate) fn found(word: String, name: String, bytes: u64) -> LogRow {
        LogRow {
            word: Loose(Some(word)),
            name: Loose(Some(name)),
            bytes: Loose(Some(bytes)),
        }
    }

    pub fn word(&self) -> Option<&str> {
        self.word.get().map(String::as_str)
    }

    pub fn name(&self) -> Option<&str> {
        self.name.get().map(String::as_str)
    }

    pub fn bytes(&self) -> Option<u64> {
        Some(*self.bytes.get()?)
    }
}

/// The place of a stopped search, as the snapshot's line carries it under
/// `searchResume`: the word of the log it stopped in and the offset of the
/// rest, in masked bytes.
#[derive(Debug, Clone, Default)]
pub struct SearchResume {
    word: Loose<String>,
    offset: Loose<u64>,
}

impl SearchResume {
    pub fn word(&self) -> Option<&str> {
        self.word.get().map(String::as_str)
    }

    pub fn offset(&self) -> Option<u64> {
        Some(*self.offset.get()?)
    }
}

/// The scalars of one object of a snapshot line, by key: what `pick` with
/// `true` copies of it, and nothing else of it.
pub type Fields = BTreeMap<String, Scalar>;

/// One session of the snapshot: its scalars by key, the logs it lists, the
/// steps, roles and filed issues it holds when the line holds their arrays,
/// and the place of a search it stopped in. A snapshot line is one session,
/// so no part of it is a document on its way elsewhere: the tools answer
/// from these fields.
#[derive(Debug, Clone, Default)]
pub struct Session {
    scalars: BTreeMap<String, Scalar>,
    logs: Vec<LogRow>,
    steps: Option<Vec<Fields>>,
    roles: Option<Vec<Fields>>,
    filed: Option<Vec<Fields>>,
    resume: Option<SearchResume>,
}

impl Session {
    /// One line of the snapshot, or nothing for a line that is no object.
    pub fn from_line(line: &[u8]) -> Option<Session> {
        let doc: Value = serde_json::from_slice(line).ok()?;
        let obj = doc.as_object()?;
        // The scalar a value holds, or nothing for null, an object or an
        // array: what `pick` with `true` leaves out, every reader of a line
        // has always read as absent.
        let scalar = |v: &Value| serde_json::from_value::<Scalar>(v.clone()).ok();
        let fields = |v: &Value| -> Option<Vec<Fields>> {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(|e| {
                        e.as_object().map(|o| {
                            o.iter()
                                .filter_map(|(k, x)| scalar(x).map(|f| (k.clone(), f)))
                                .collect()
                        })
                    })
                    .collect()
            })
        };
        let mut s = Session::default();
        for (k, v) in obj {
            match k.as_str() {
                "logs" => {
                    s.logs = v
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|l| l.is_object())
                        .map(|l| LogRow {
                            word: Loose(l["word"].as_str().map(str::to_string)),
                            name: Loose(l["name"].as_str().map(str::to_string)),
                            bytes: Loose(l["bytes"].as_u64()),
                        })
                        .collect();
                }
                "steps" => s.steps = fields(v),
                "roles" => s.roles = fields(v),
                "filed" => s.filed = fields(v),
                "searchResume" => {
                    s.resume = v.as_object().map(|o| SearchResume {
                        word: Loose(o.get("word").and_then(Value::as_str).map(str::to_string)),
                        offset: Loose(o.get("offset").and_then(Value::as_u64)),
                    });
                }
                _ => {
                    if let Some(f) = scalar(v) {
                        s.scalars.insert(k.clone(), f);
                    }
                }
            }
        }
        Some(s)
    }

    /// The session's word at `key`, or nothing when the line holds none or
    /// holds one of another type.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.scalars.get(key)?.text()
    }

    /// The session's whole count at `key`, as `whole` gives it.
    pub fn whole(&self, key: &str) -> Option<u64> {
        self.scalars.get(key)?.whole()
    }

    /// The session's signed count at `key`, as `count` gives it.
    pub fn count(&self, key: &str) -> Option<i64> {
        self.scalars.get(key)?.count()
    }

    /// The session's scalar at `key`, as the line wrote it, or nothing.
    pub fn field(&self, key: &str) -> Option<&Scalar> {
        self.scalars.get(key)
    }

    /// The logs the line lists, in its order.
    pub fn logs(&self) -> &[LogRow] {
        &self.logs
    }

    pub fn steps(&self) -> Option<&[Fields]> {
        self.steps.as_deref()
    }

    pub fn roles(&self) -> Option<&[Fields]> {
        self.roles.as_deref()
    }

    pub fn filed(&self) -> Option<&[Fields]> {
        self.filed.as_deref()
    }

    /// The place of a search the caller stopped in this session's log.
    pub fn resume(&self) -> Option<&SearchResume> {
        self.resume.as_ref()
    }

    /// The running cycle of an issue, as `live/running.json` names it and the
    /// stamps of its log names find them: an id built from the store's start
    /// that names the running cycle alone, and the logs found by their stamps.
    pub(crate) fn running_cycle(issue: u64, started: i64, logs: Vec<LogRow>) -> Session {
        let scalars = [
            (
                "id".to_string(),
                Scalar::Text(format!("work-{issue}-{started}")),
            ),
            ("kind".to_string(), Scalar::Text("work".to_string())),
            ("issue".to_string(), Scalar::Big(issue)),
        ]
        .into_iter()
        .collect();
        Session {
            scalars,
            logs,
            steps: None,
            roles: None,
            filed: None,
            resume: None,
        }
    }
}

pub struct Snapshot {
    pub days: i64,
    pub generated_at: String,
    /// The host has not written it for ten minutes.
    pub stale: bool,
    path: PathBuf,
    now: i64,
}

fn bad(why: &str) -> ToolError {
    ToolError::File {
        file: FILE.into(),
        why: why.into(),
    }
}

impl Snapshot {
    /// Reads and checks the header only.
    pub fn open(dir: &Path, now: i64) -> Result<Snapshot, ToolError> {
        let path = dir.join(FILE);
        let f = File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                bad("not written")
            } else {
                bad("could not be read")
            }
        })?;
        let mut first = String::new();
        // The header is read under the crate's bound: a first line of more
        // than it is refused, never buffered, and the answer is the fixed one
        // that names no path and no content.
        BufReader::new(f.take(READ_BOUND + 1))
            .read_line(&mut first)
            .map_err(|_| bad("could not be read"))?;
        if first.len() as u64 > READ_BOUND {
            return Err(bad("could not be read"));
        }
        let head: Value = serde_json::from_str(&first).map_err(|_| bad("has no header"))?;
        if head["api"] != API {
            return Err(bad("has a header of another api"));
        }
        // The shape number says which release wrote the lines: 1.4.x named
        // the pull request `pr` and 2.x `change`, so a stale file of the old
        // shape is refused by name instead of answered without its links.
        if head["sessions"] != SESSIONS_SHAPE {
            return Err(bad("has a header of another shape: run tehas update, or analytics sessions, to rewrite it"));
        }
        let days = head["days"]
            .as_i64()
            .filter(|d| *d >= 1)
            .ok_or_else(|| bad("has a header without days"))?;
        let generated_at = head["generatedAt"].as_str().unwrap_or("").to_string();
        let stale = from_iso(&generated_at).is_none_or(|t| now - t > STALE_SECS);
        Ok(Snapshot {
            days,
            generated_at,
            stale,
            path,
            now,
        })
    }

    /// The sessions within the window, one line read at a time. A line that is
    /// not a JSON object, or has no time in the window, is skipped. A read
    /// error ends the sessions.
    pub fn sessions(&self) -> impl Iterator<Item = Session> {
        let from = self.now - self.days * 86400;
        File::open(&self.path)
            .ok()
            .into_iter()
            // Bytes, not lines of text: a line that is not UTF-8 is skipped
            // like any other bad line instead of ending the reading.
            .flat_map(|f| BufReader::new(f).split(b'\n').skip(1))
            .map_while(Result::ok)
            .filter_map(|l| Session::from_line(&l))
            .filter(move |s| {
                s.text("started")
                    .and_then(from_iso)
                    .is_some_and(|t| t >= from)
            })
    }
}
