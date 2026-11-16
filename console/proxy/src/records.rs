//! Who may work, and which job is going: the proxy's reading of the factory's
//! records, and the record of a factory that has no outsource tables yet.
//!
//! The proxy decides nothing from what a contractor sends (rule 4). It asks
//! these four questions of the factory on every request, so a job that ended or
//! a contractor revoked mid-stream is refused by the next request rather than
//! remembered wrongly from the last one.
//!
//! The rows come from #2272's tables. Until they are there, a factory reads no
//! job and no contractor, and this answers nothing at all: every request is
//! refused as a job whose record says absent, and nothing is ever forwarded to
//! the model endpoint. Failing closed is the point of a proxy that holds the
//! factory's model key.

use crate::jobs::Issuer;
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

/// One job's record, as far as the proxy asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub job: i64,
    pub attempt: i64,
    pub contractor: String,
    /// The state of the job's record, in the words the table holds.
    pub state: String,
    /// The token of this attempt, as this process issued it. The proxy has the
    /// tokens of live jobs in memory and nowhere else, so the ask names the
    /// job and the table answers with the token rather than the other way
    /// round: nothing that comes from a contractor is ever looked up.
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

/// The records of a factory with no outsource tables: the state before #2272
/// lands, and the state of any instance whose tables cannot be read. Nothing
/// is allowed and no job is going, so the proxy answers only refusals.
pub struct NoneAtAll;

/// The records of one proxy, read as a request comes: the factory's two lists,
/// which #2272 writes, and the tokens this process minted. They answer the four
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

impl Records for OfProxy<'_> {
    /// The job and contractor of a token this process issued. A token it
    /// never issued, or one it took back when the job ended or its contractor
    /// was revoked, names no job.
    fn live(&self, token: &str) -> Option<Live> {
        self.issuer.look(token)
    }

    fn state_of(&self, job: i64) -> Option<State> {
        self.factory
            .rows()
            .jobs
            .iter()
            .find(|j| j.job == job)
            .and_then(|j| State::of(&j.state))
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

/// A read of the factory's rows, kept for what it says: the lists are small,
/// and a proxy that reads them once per request needs no cache and no
/// invalidation. An empty list is a factory where nothing may work, which is
/// the answer of a read that failed.
pub struct Of {
    rows: Rows,
}

impl Of {
    /// The rows a proxy is given to read, taken from a reader: what a request
    /// makes of one read of the factory's two lists, and what a test hands the
    /// proxy to say what its records hold.
    pub fn once(reader: &dyn Reader) -> Of {
        Of {
            rows: reader.rows(),
        }
    }

    /// The rows themselves, for a caller that read them already.
    pub fn of(rows: Rows) -> Of {
        Of { rows }
    }
}

impl Records for Of {
    fn live(&self, token: &str) -> Option<Live> {
        self.rows
            .jobs
            .iter()
            .find(|j| !j.token.is_empty() && j.token == token)
            .map(|j| Live {
                job: j.job,
                attempt: j.attempt,
                contractor: j.contractor.clone(),
            })
    }

    fn state_of(&self, job: i64) -> Option<State> {
        self.rows
            .jobs
            .iter()
            .find(|j| j.job == job)
            .and_then(|j| State::of(&j.state))
    }

    fn allowed(&self, contractor: &str) -> bool {
        self.rows.contractors.iter().any(|c| c == contractor)
    }

    /// A row read of the factory's own list was asked for the model, which is
    /// what the count of rule 5 is a count of.
    fn asked(&self, _token: &str) {}
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
        // The state before #2272, and the state of a read that failed: no
        // token is known, no job is going, nobody is allowed. A proxy told
        // nothing answers only refusals, and never the model.
        let none = NoneAtAll;
        assert_eq!(none.live("token-of-7"), None);
        assert_eq!(none.state_of(7), None);
        assert!(!none.allowed("gpu1"));
        assert!(none.rows().contractors.is_empty());
        assert!(none.rows().jobs.is_empty());
    }

    #[test]
    fn the_records_of_a_read_answer_the_four_questions() {
        let records = Of::of(rows());
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
    }

    #[test]
    fn an_empty_token_names_no_job_even_against_a_row_that_holds_one() {
        // A contractor that sends no bearer word asks the question with an
        // empty string; a row whose token was never issued is no answer.
        let mut rows = rows();
        rows.jobs[0].token = String::new();
        let records = Of::of(rows);
        assert_eq!(records.live(""), None);
        assert_eq!(records.live("token-of-7"), None);
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
        let records = Of::once(&Empty);
        assert_eq!(records.live("token-of-7"), None);
        assert!(!records.allowed("gpu1"));
    }
}
