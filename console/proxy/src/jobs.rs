//! The job tokens: issued here, held here, and nowhere else.
//!
//! The token of a job is the only thing a contractor carries to the model
//! proxy, and it dies with the job and with revocation. It is never written to
//! the store, never in a command line and never in a log. This process is the
//! only holder: [`Issuer`] mints a token when a job is given out, keeps it
//! while the job runs, and forgets it the moment the job ends or its
//! contractor is revoked.
//!
//! A token is 32 bytes of `/dev/urandom` in the URL-safe alphabet tehas uses
//! for its MCP keys, so it survives a JSON body and a header alike. It is
//! asked for by name and never by value, as every token of a cycle's container
//! is today.

use crate::request::Live;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The bytes of a token, and the characters it is written in: the URL-safe
/// alphabet of [`tehas_core::grammar`]'s MCP keys, which no header or JSON
/// string needs escaping.
pub const BYTES: usize = 32;
const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_";

/// What one token names, and the number of requests that carried it. The
/// counts go to the store per contractor and per job, so rule 5 outlives the
/// token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub live: Live,
    pub requests: u64,
}

/// The tokens this process issued. One lock, read on every request and written
/// when a job starts or ends: the list is a few dozen live jobs, never the
/// factory's history.
/// Cloning an issuer is another handle on the same tokens, which is what the
/// listener's threads, the control line and a test's own case all share.
#[derive(Default, Clone)]
pub struct Issuer(Arc<Mutex<HashMap<String, Entry>>>);

impl Issuer {
    pub fn new() -> Issuer {
        Issuer::default()
    }

    /// The token a contractor is given for this job, kept until [`forget`]
    /// says the job is over. Two jobs of one issue take turns, so the attempt
    /// number is part of what is remembered: a late request of the attempt
    /// before carries no token that is still on the list.
    pub fn issue(
        &self,
        job: i64,
        attempt: i64,
        contractor: &str,
        random: &mut dyn FnMut() -> String,
    ) -> String {
        let token = random();
        debug_assert!(
            token.len() >= 8,
            "a token of {} characters is too short",
            token.len()
        );
        let _ = self.lock().insert(
            token.clone(),
            Entry {
                live: Live {
                    job,
                    attempt,
                    contractor: contractor.to_string(),
                },
                requests: 0,
            },
        );
        token
    }

    /// The job and contractor a token names, and the count of requests that
    /// carried it going up by one: the count of rule 5 for a request whose
    /// token names a job, since a request of a token never issued counts for
    /// nobody. [`Issuer::asked`] is the count of the requests that name none.
    pub fn look(&self, token: &str) -> Option<Live> {
        let mut held = self.lock();
        let entry = held.get_mut(token)?;
        entry.requests += 1;
        Some(entry.live.clone())
    }

    /// Put one token in the table by hand. A test of the rules of one request
    /// uses it; a token of a job on a host is issued, never put, since the
    /// proxy is the one holder of a token and mints it of `/dev/urandom`.
    #[doc(hidden)]
    pub fn put(&self, token: &str, job: i64, attempt: i64, contractor: &str) {
        self.put_counted(token, job, attempt, contractor, 0)
    }

    /// [`Self::put`] of a token that already carried a number of requests:
    /// what a test hands the records of a proxy whose table holds the token and
    /// its count both.
    #[doc(hidden)]
    pub fn put_counted(
        &self,
        token: &str,
        job: i64,
        attempt: i64,
        contractor: &str,
        requests: u64,
    ) {
        let _ = self.lock().insert(
            token.to_string(),
            Entry {
                live: Live {
                    job,
                    attempt,
                    contractor: contractor.to_string(),
                },
                requests,
            },
        );
    }

    /// One request of a token carried, counted for its job and nothing else:
    /// what a request whose token names no job counts, once, since
    /// [`Self::look`] counted nothing of it. A token never issued counts for
    /// nobody.
    pub fn asked(&self, token: &str) {
        if let Some(entry) = self.lock().get_mut(token) {
            entry.requests += 1;
        }
    }

    /// Whether this token is one of a job still going: the check a caller does
    /// without taking the count, which is what a test of a refusal asks for.
    pub fn holds(&self, token: &str) -> bool {
        self.lock().contains_key(token)
    }

    /// The job is over, or its contractor is revoked: the token dies now, and
    /// a request that carries it from here on is the one of a job that ended.
    pub fn forget(&self, token: &str) -> bool {
        self.lock().remove(token).is_some()
    }

    /// Every job that ended or whose contractor was revoked, said by
    /// [`Self::forget`] one token at a time.
    pub fn forget_jobs(&self, jobs: &[i64]) -> usize {
        let mut held = self.lock();
        let dead: Vec<String> = held
            .iter()
            .filter(|(_, e)| jobs.contains(&e.live.job))
            .map(|(token, _)| token.clone())
            .collect();
        let gone = dead.len();
        for token in dead {
            held.remove(&token);
        }
        gone
    }

    /// The tokens of one contractor: what `tehas outsource revoke` takes back
    /// at once, along with its check-ins and its results.
    pub fn forget_contractor(&self, contractor: &str) -> usize {
        let mut held = self.lock();
        let dead: Vec<String> = held
            .iter()
            .filter(|(_, e)| e.live.contractor == contractor)
            .map(|(token, _)| token.clone())
            .collect();
        let gone = dead.len();
        for token in dead {
            held.remove(&token);
        }
        gone
    }

    /// What to write to the store: the requests each live token carried, by
    /// contractor and by job, and the counts cleared. A call that is refused
    /// before its token is read carries no token, so it counts for nobody.
    pub fn take_counts(&self) -> Vec<(String, i64, u64)> {
        let mut held = self.lock();
        let mut out: Vec<(String, i64, u64)> = held
            .values()
            .filter(|e| e.requests > 0)
            .map(|e| (e.live.contractor.clone(), e.live.job, e.requests))
            .collect();
        for entry in held.values_mut() {
            entry.requests = 0;
        }
        out.sort();
        out
    }

    /// The jobs this process holds a token of, each once, in order: the jobs
    /// whose records the proxy reads.
    pub fn jobs(&self) -> Vec<i64> {
        let mut jobs: Vec<i64> = self.lock().values().map(|e| e.live.job).collect();
        jobs.sort_unstable();
        jobs.dedup();
        jobs
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Another handle on the same tokens, which is what the listener's request
    /// threads and its control line share.
    pub fn shared(&self) -> Issuer {
        Issuer(Arc::clone(&self.0))
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The random of a host: 32 bytes of `/dev/urandom`, written in the alphabet.
/// A reader that cannot be opened or cannot fill is None, and the caller who
/// mints a token refuses the job rather than mint one of a weak shape.
pub fn random() -> Option<String> {
    let mut bytes = [0u8; BYTES];
    File::open("/dev/urandom")
        .ok()
        .and_then(|mut f| f.read_exact(&mut bytes).ok())?;
    Some(token_of(&bytes))
}

/// The token the bytes of a job's key are written as: the alphabet above, one
/// character a byte. Split out so a test holds the shape without a reader.
pub fn token_of(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer_with(jobs: &[(i64, i64, &str)]) -> (Issuer, Vec<String>) {
        let issuer = Issuer::new();
        let mut tokens = Vec::new();
        for (job, attempt, contractor) in jobs {
            let n = tokens.len();
            tokens.push(issuer.issue(*job, *attempt, contractor, &mut || {
                format!("token-{n}-for-job-{job}")
            }));
        }
        (issuer, tokens)
    }

    #[test]
    fn a_token_names_its_job_and_the_contractor_it_was_issued_to() {
        let (issuer, tokens) = issuer_with(&[(7, 2, "gpu1"), (8, 1, "gpu2")]);
        assert_eq!(
            issuer.look(&tokens[0]),
            Some(Live {
                job: 7,
                attempt: 2,
                contractor: "gpu1".to_string()
            })
        );
        assert_eq!(
            issuer.look(&tokens[1]),
            Some(Live {
                job: 8,
                attempt: 1,
                contractor: "gpu2".to_string()
            })
        );
        assert_eq!(issuer.look("tok-9"), None);
        assert_eq!(issuer.look(""), None);
    }

    #[test]
    fn a_forgotten_token_names_nothing_at_all() {
        let (issuer, tokens) = issuer_with(&[(7, 1, "gpu1")]);
        assert!(issuer.holds(&tokens[0]));
        assert!(issuer.forget(&tokens[0]));
        assert!(!issuer.holds(&tokens[0]));
        assert_eq!(issuer.look(&tokens[0]), None);
        assert!(!issuer.forget(&tokens[0]), "a second forget is no forget");
        assert!(issuer.is_empty());
    }

    #[test]
    fn revoking_a_contractor_takes_every_token_it_holds_and_no_other() {
        let (issuer, tokens) = issuer_with(&[(1, 1, "gpu1"), (2, 1, "gpu1"), (3, 1, "gpu2")]);
        assert_eq!(issuer.forget_contractor("gpu1"), 2);
        assert!(!issuer.holds(&tokens[0]));
        assert!(!issuer.holds(&tokens[1]));
        assert!(issuer.holds(&tokens[2]), "another contractor lost its job");
        assert_eq!(issuer.forget_contractor("nobody"), 0);
    }

    #[test]
    fn a_job_that_ended_takes_its_token_back() {
        let (issuer, tokens) = issuer_with(&[(1, 1, "gpu1"), (2, 3, "gpu2")]);
        assert_eq!(issuer.forget_jobs(&[1]), 1);
        assert!(!issuer.holds(&tokens[0]));
        assert!(issuer.holds(&tokens[1]));
    }

    #[test]
    fn the_jobs_held_are_named_once_each() {
        let (issuer, _) = issuer_with(&[(3, 1, "gpu1"), (1, 1, "gpu2"), (3, 2, "gpu1")]);
        assert_eq!(issuer.jobs(), vec![1, 3]);
        assert!(Issuer::new().jobs().is_empty());
    }

    #[test]
    fn the_made_token_is_long_and_of_the_alphabet() {
        let made = random().expect("/dev/urandom cannot be read");
        assert_eq!(made.len(), BYTES);
        assert!(
            made.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
            "{made}"
        );
        let again = random().unwrap();
        assert_ne!(made, again, "two tokens are the same token");
        // A token is never a word that could be read as one of the shapes the
        // mask and the store's secret check refuse: it is written by us, in
        // the alphabet, and carries no prefix of any vendor's key.
        assert!(!tehas_core::secret::secret(made.as_bytes()));
    }

    #[test]
    fn the_alphabet_carries_no_character_a_header_or_json_needs_escaped() {
        let made = token_of(&(0..=255u8).collect::<Vec<u8>>());
        assert_eq!(made.len(), 256);
        assert!(made
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert!(!made.contains(['+', '/', '=', ' ', '\r', '\n', '"']));
    }
}
