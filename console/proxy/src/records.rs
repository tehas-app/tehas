//! Who may work, and which job is going: the proxy's reading of the factory's
//! records in the state store.
//!
//! The proxy decides nothing from what a contractor sends (rule 4). It asks
//! these four questions of the factory on every request, so a job that ended or
//! a contractor revoked mid-stream is refused by the next request rather than
//! remembered wrongly from the last one.
//!
//! The rows are the store's outsource tables, read with `contractor-list` and
//! `job-show`. A store that cannot be read, or one with the feature off, which
//! answers those verbs `refused`, reads as no job and no contractor: every
//! request is refused and nothing is forwarded to the model endpoint. Failing
//! closed is the point of a proxy that holds the factory's model key.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jobs::{Backing, Issuer, Scope};
use crate::request::{Live, Records, State};

/// The rows the proxy reads: who was invited and not revoked, and which jobs
/// are going with whose token. One call of the factory's own, of no token and
/// no path. A proxy that holds the rows of one request answers from them and
/// reads no further, so a test says what the factory's records hold by handing
/// these over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rows {
    /// Contractors that may work: invited, not revoked, not drained.
    pub contractors: Vec<String>,
    /// The jobs that are going, with the token their attempt was issued.
    pub jobs: Vec<Job>,
}

impl Rows {
    /// The attempt and the contractor of a job's record.
    pub fn holder(&self, job: i64) -> Option<(i64, String)> {
        self.jobs
            .iter()
            .find(|j| j.job == job)
            .map(|j| (j.attempt, j.contractor.clone()))
    }
}

/// One job's record, as far as the proxy asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub job: i64,
    pub attempt: i64,
    pub contractor: String,
    /// The state of the job's record, in the words the table holds.
    pub state: String,
    /// The token a test's rows were minted with. The store holds no token, so
    /// its rows leave this empty: a token is answered from the proxy's own
    /// table only.
    pub token: String,
}

/// What the proxy asks of the factory. A read that fails is empty, which is a
/// factory where nothing may work.
///
/// A reader is shared, not copied: the proxy reads it for every request, so a
/// process told that a job ended or a contractor revoked answers the next one
/// from what the rows say then.
pub trait Reader {
    /// The whole of both lists, in one read.
    fn rows(&self) -> Rows;
}

/// The records of a factory whose tables cannot be read. Nothing is allowed
/// and no job is going, so the proxy answers only refusals.
pub struct NoneAtAll;

/// The records of one proxy, read as a request comes: the factory's two lists
/// and the tokens this process minted. They answer the four
/// questions together because the process is the one holder of both: it read
/// the job when the job was placed and it minted the token then, so a request
/// that carries a token it no longer holds is the request of a job that ended,
/// whatever the rows still say about it.
pub struct OfProxy<'a> {
    factory: &'a (dyn Reader + Sync),
    issuer: &'a Issuer,
}

impl<'a> OfProxy<'a> {
    /// The records of one proxy: its two readers, named for what they are, so a
    /// test can hand the proxy its own.
    pub fn of(factory: &'a (dyn Reader + Sync), issuer: &'a Issuer) -> OfProxy<'a> {
        OfProxy { factory, issuer }
    }
}

/// A request is counted once: a request of a token of a job was counted by
/// [`Issuer::look`], so only the ask of a token of no job is said here.
impl Records for OfProxy<'_> {
    /// The job and contractor of a token this process issued. A token it
    /// never issued, or one it took back when the job ended or its contractor
    /// was revoked, names no job.
    fn live(&self, token: &str) -> Option<Live> {
        self.issuer.look(token, Scope::Model).ok()
    }

    fn wrong_scope(&self, token: &str) -> bool {
        self.issuer.scope_of(token) == Some(Scope::Job)
    }

    fn state_of(&self, job: i64) -> Option<State> {
        self.factory
            .rows()
            .jobs
            .iter()
            .find(|j| j.job == job)
            .and_then(|j| State::of(&j.state))
    }

    fn holder(&self, job: i64) -> Option<(i64, String)> {
        self.factory.rows().holder(job)
    }

    fn allowed(&self, contractor: &str) -> bool {
        self.factory
            .rows()
            .contractors
            .iter()
            .any(|c| c == contractor)
    }

    /// One request carried this token, counted for its job. A refusal of a
    /// live job's token counts too: the factory was asked.
    fn asked(&self, token: &str) {
        self.issuer.asked(token);
    }
}

impl Records for NoneAtAll {
    fn live(&self, _token: &str) -> Option<Live> {
        None
    }
    fn state_of(&self, _job: i64) -> Option<State> {
        None
    }
    fn holder(&self, _job: i64) -> Option<(i64, String)> {
        None
    }
    fn allowed(&self, _contractor: &str) -> bool {
        false
    }
    fn asked(&self, _token: &str) {}
}

impl Reader for NoneAtAll {
    fn rows(&self) -> Rows {
        Rows {
            contractors: Vec::new(),
            jobs: Vec::new(),
        }
    }
}

/// The factory's records as the state store of `home` holds them: the
/// contractors that are not revoked, and the jobs this proxy holds a token of.
/// A draining contractor still finishes the job it has, so it stays allowed.
/// Only the jobs of held tokens are read: a request whose token the proxy does
/// not hold is refused before a job is asked for.
pub struct OfStore {
    home: PathBuf,
    issuer: Issuer,
}

impl OfStore {
    pub fn at(home: &Path, issuer: &Issuer) -> OfStore {
        OfStore {
            home: home.to_path_buf(),
            issuer: issuer.shared(),
        }
    }

    /// One read verb of the store, when it answered with `verb`.
    fn ask(&self, args: &[&str], verb: &str) -> Option<Value> {
        let text = store::call(&self.home, args, &mut std::io::empty()).ok()?;
        let said: Value = serde_json::from_str(&text).ok()?;
        (said["verb"] == verb).then_some(said)
    }
}

/// One row of a store list. The store writes each row of `contractor-list` as
/// a string of JSON today; an object is taken as it is too.
fn row(v: &Value) -> Option<Value> {
    match v {
        Value::String(text) => serde_json::from_str(text).ok(),
        Value::Object(_) => Some(v.clone()),
        _ => None,
    }
}

impl Reader for OfStore {
    fn rows(&self) -> Rows {
        let contractors = self
            .ask(&["contractor-list"], "contractors")
            .and_then(|v| v["rows"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(row)
            .filter(|c| c["revoked"] == Value::Bool(false))
            .filter_map(|c| c["name"].as_str().map(str::to_string))
            .collect();
        let jobs = self
            .issuer
            .jobs()
            .into_iter()
            .filter_map(|id| {
                let v = self.ask(&["job-show", "--job", &id.to_string()], "job")?;
                Some(Job {
                    job: v["job"].as_i64()?,
                    attempt: v["attempt"].as_i64()?,
                    contractor: v["contractor"].as_str()?.to_string(),
                    state: v["state"].as_str()?.to_string(),
                    token: String::new(),
                })
            })
            .collect();
        Rows { contractors, jobs }
    }
}

/// The hashes of the job tokens as the state store of `home` keeps them, in the
/// rows of the jobs, one column per scope: `job-token` keeps one,
/// `job-by-token` names the job and scope of one. A store that gives no answer
/// keeps nothing and names nothing, so a token is then neither given nor
/// taken.
pub struct KeptIn {
    home: PathBuf,
}

impl KeptIn {
    pub fn at(home: &Path) -> KeptIn {
        KeptIn {
            home: home.to_path_buf(),
        }
    }

    fn ask(&self, args: &[&str]) -> Option<Value> {
        let text = store::call(&self.home, args, &mut std::io::empty()).ok()?;
        serde_json::from_str(&text).ok()
    }
}

impl Backing for KeptIn {
    fn keep(&self, job: i64, scope: Scope, sha256: &str) -> bool {
        self.ask(&[
            "job-token",
            "--job",
            &job.to_string(),
            "--scope",
            scope.word(),
            "--sha256",
            sha256,
        ])
        .is_some_and(|v| v["verb"] == "kept")
    }

    fn recall(&self, sha256: &str) -> Option<(Live, Scope)> {
        let v = self
            .ask(&["job-by-token", "--sha256", sha256])
            .filter(|v| v["verb"] == "job")?;
        let live = Live {
            job: v["job"].as_i64()?,
            attempt: v["attempt"].as_i64()?,
            contractor: v["contractor"].as_str()?.to_string(),
        };
        Some((live, Scope::of(v["scope"].as_str()?)?))
    }
}

/// A read of the factory's rows with the tokens of the proxy that answers
/// them: the shape of [`OfProxy`] with both halves kept by the reader, which is
/// what a test of one request hands the proxy. The lists are small, and a proxy
/// that reads them once per request needs no cache and no invalidation. An
/// empty list is a factory where nothing may work, which is the answer of a
/// read that failed.
///
/// A token is answered from the proxy's own table and never from a row of the
/// factory: a row whose token the proxy does not hold is the row of a job that
/// ended or of a contractor revoked, and is refused.
pub struct Of {
    rows: Rows,
    issuer: Issuer,
}

impl Of {
    /// The rows of a reader and the tokens of the proxy that answers them:
    /// what a request makes of one read of the factory's two lists and the
    /// process's own table, and what a test hands the proxy to say what its
    /// records hold. Rows with no token to go with them are `of` nothing.
    pub fn once(reader: &dyn Reader, issuer: &Issuer) -> Of {
        Of {
            rows: reader.rows(),
            issuer: issuer.shared(),
        }
    }

    /// The rows of a factory and the tokens of a proxy that answered them: the
    /// shape of [`OfProxy::of`] with both halves kept, which is what a test of
    /// one request hands the proxy.
    pub fn read(rows: Rows, issuer: &Issuer) -> Of {
        Of {
            rows,
            issuer: issuer.shared(),
        }
    }
}

impl Records for Of {
    /// The job and contractor of a token this proxy holds. The factory's rows
    /// name a token, and a name is not what decides: the table of the proxy
    /// that issued the token is, since that is the table a revocation and the
    /// end of a job empty.
    fn live(&self, token: &str) -> Option<Live> {
        self.issuer.look(token, Scope::Model).ok()
    }

    fn wrong_scope(&self, token: &str) -> bool {
        self.issuer.scope_of(token) == Some(Scope::Job)
    }

    fn state_of(&self, job: i64) -> Option<State> {
        self.rows
            .jobs
            .iter()
            .find(|j| j.job == job)
            .and_then(|j| State::of(&j.state))
    }

    fn holder(&self, job: i64) -> Option<(i64, String)> {
        self.rows.holder(job)
    }

    fn allowed(&self, contractor: &str) -> bool {
        self.rows.contractors.iter().any(|c| c == contractor)
    }

    /// A request whose token names no job of this proxy, counted for the token
    /// it carried. A request of a token of a job was counted by [`Self::live`]
    /// and is never counted twice.
    fn asked(&self, token: &str) {
        self.issuer.asked(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Rows {
        Rows {
            contractors: vec!["gpu1".to_string()],
            jobs: vec![Job {
                job: 7,
                attempt: 1,
                contractor: "gpu1".to_string(),
                state: "running".to_string(),
                token: "token-of-7".to_string(),
            }],
        }
    }

    #[test]
    fn a_factory_of_no_records_answers_nothing_at_all() {
        // The state of a read that failed: no
        // token is known, no job is going, nobody is allowed. A proxy told
        // nothing answers only refusals, and never the model.
        let none = NoneAtAll;
        assert_eq!(none.live("token-of-7"), None);
        assert_eq!(none.state_of(7), None);
        assert_eq!(none.holder(7), None);
        assert!(!none.allowed("gpu1"));
        assert!(none.rows().contractors.is_empty());
        assert!(none.rows().jobs.is_empty());
    }

    /// The rows of a case and the tokens of the proxy that answers them, from
    /// one table: the records a test hands the proxy of one request.
    fn read_of(issuer: &Issuer) -> Of {
        Of::once(&TableOf(rows()), issuer)
    }

    struct TableOf(Rows);
    impl Reader for TableOf {
        fn rows(&self) -> Rows {
            self.0.clone()
        }
    }

    #[test]
    fn the_records_of_a_read_answer_the_four_questions() {
        let issuer = Issuer::new();
        issuer.put("token-of-7", 7, 1, "gpu1");
        let records = read_of(&issuer);
        assert_eq!(
            records.live("token-of-7"),
            Some(Live {
                job: 7,
                attempt: 1,
                contractor: "gpu1".to_string()
            })
        );
        assert_eq!(records.state_of(7), Some(State::Running));
        assert!(records.allowed("gpu1"));
        // A token of no job, a job of no record, a contractor of no record:
        // each is answered as the nothing it is.
        assert_eq!(records.live("token-of-8"), None);
        assert_eq!(records.state_of(8), None);
        assert!(!records.allowed("gpu2"));
        assert_eq!(records.holder(7), Some((1, "gpu1".to_string())));
        assert_eq!(records.holder(8), None);
    }

    #[test]
    fn a_token_is_answered_from_the_proxy_s_table_and_never_from_a_row() {
        // The row of the factory still names the token of the job; the proxy
        // no longer holds it, which is the job that ended or the contractor
        // revoked. A row is the factory's word about who works what, and a
        // token is never looked up from what a contractor sends.
        let issuer = Issuer::new();
        let records = read_of(&issuer);
        assert_eq!(
            records.live("token-of-7"),
            None,
            "a row of the factory answered for a token the proxy does not hold"
        );
        // An empty token is no token, whatever the rows say.
        assert_eq!(records.live(""), None);
        // The token comes back when the proxy holds it, and its requests are
        // counted for its job once, by the reading that named the job.
        let issuer = Issuer::new();
        issuer.put_counted("token-of-7", 7, 1, "gpu1", 3);
        let records = read_of(&issuer);
        assert!(records.live("token-of-7").is_some());
        records.asked("a-token-of-no-job");
        assert_eq!(
            issuer.take_counts(),
            vec![("gpu1".to_string(), 7, 4)],
            "the ask of a token of no job, or the count of twice"
        );
    }

    #[test]
    fn a_read_of_nobody_s_rows_is_a_factory_where_nothing_may_work() {
        struct Empty;
        impl Reader for Empty {
            fn rows(&self) -> Rows {
                Rows {
                    contractors: Vec::new(),
                    jobs: Vec::new(),
                }
            }
        }
        let issuer = Issuer::new();
        issuer.put("token-of-7", 7, 1, "gpu1");
        let records = Of::once(&Empty, &issuer);
        // A token the proxy holds names a job the factory does not know, which
        // the rules refuse as a job that is over; nothing of the read itself
        // may work.
        assert!(records.live("token-of-7").is_some());
        assert_eq!(records.state_of(7), None);
        assert!(!records.allowed("gpu1"));
    }
}
