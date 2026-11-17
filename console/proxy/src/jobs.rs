//! The job tokens: issued here, held here, and nowhere else.
//!
//! The token of a job is the only thing a contractor carries to the model
//! proxy, and it dies with the job and with revocation. It is never written to
//! the store, never in a command line and never in a log. This process is the
//! only holder: [`Issuer`] mints a token when a job is given out, keeps it
//! while the job runs, and forgets it the moment the job ends or its
//! contractor is revoked.
//!
//! A job has two tokens, one per [`Scope`], minted together when the job is
//! given out and ended together. The model token goes into the container's
//! models.json and reaches only the model paths; the job token goes to the
//! contractor program and reaches only the job channel. A token of one scope
//! is refused on the other's paths.
//!
//! A proxy on a host keeps the sha256 of each token it mints in the job's row
//! of the store ([`Issuer::backed`]), and a token it does not hold is checked
//! against those hashes: a proxy that restarts takes the token of a job still
//! going, and the store never holds a token.
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

/// What a token may reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The container's token, in its models.json: the model paths, and the
    /// egress tunnel route when it is built (`/outsource/v1/egress`).
    Model,
    /// The contractor program's token: the job channel, and the job's
    /// downloads under `/outsource/v1/` when they are built. Never given to
    /// the container.
    Job,
}

impl Scope {
    /// The word the store keeps the scope by.
    pub fn word(self) -> &'static str {
        match self {
            Scope::Model => "model",
            Scope::Job => "job",
        }
    }

    pub fn of(word: &str) -> Option<Scope> {
        match word {
            "model" => Some(Scope::Model),
            "job" => Some(Scope::Job),
            _ => None,
        }
    }
}

/// The two tokens of one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    /// For the container's models.json.
    pub model: String,
    /// For the contractor program.
    pub job: String,
}

/// Why a token was not taken for a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miss {
    /// It names no job this process holds or the store keeps.
    Unknown,
    /// It names a live job, in the other scope.
    Scope,
}

/// What one token names, its scope, and the number of requests that carried
/// it. The counts go to the store per contractor and per job, so rule 5
/// outlives the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub live: Live,
    pub scope: Scope,
    pub requests: u64,
}

/// The tokens this process issued. One lock, read on every request and written
/// when a job starts or ends: the list is a few dozen live jobs, never the
/// factory's history.
/// Cloning an issuer is another handle on the same tokens, which is what the
/// listener's threads, the control line and a test's own case all share.
#[derive(Default, Clone)]
pub struct Issuer {
    held: Arc<Mutex<HashMap<String, Entry>>>,
    store: Option<Arc<dyn Backing>>,
}

/// Where the hashes of the tokens are kept: the job rows of the store.
pub trait Backing: Send + Sync {
    /// Keep the hash of a job's new token of one scope. False when it was not
    /// kept.
    fn keep(&self, job: i64, scope: Scope, sha256: &str) -> bool;
    /// The job and scope a hash names while a call of its token may be
    /// answered.
    fn recall(&self, sha256: &str) -> Option<(Live, Scope)>;
}

/// The sha256 of a token, as the store keeps it.
pub fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether a word could be a token this proxy minted: the length and the
/// alphabet. Nothing else is looked up.
fn token_shaped(token: &str) -> bool {
    token.len() == BYTES && token.bytes().all(|b| ALPHABET.contains(&b))
}

impl Issuer {
    pub fn new() -> Issuer {
        Issuer::default()
    }

    /// An issuer whose tokens' hashes are kept by `store`: a token is minted
    /// only once its hash is kept, and a token-shaped word this process does
    /// not hold is looked up by its hash.
    pub fn backed(store: Arc<dyn Backing>) -> Issuer {
        Issuer {
            held: Arc::default(),
            store: Some(store),
        }
    }

    /// The job and scope a token this process does not hold names, from the
    /// kept hashes, put in the table with `requests` counted when the scope
    /// is `counted`.
    fn recall(&self, token: &str, counted: Option<Scope>) -> Option<(Live, Scope)> {
        let store = self.store.as_ref()?;
        if !token_shaped(token) {
            return None;
        }
        let (live, scope) = store.recall(&token_hash(token))?;
        let mut held = self.lock();
        let entry = held.entry(token.to_string()).or_insert(Entry {
            live,
            scope,
            requests: 0,
        });
        if counted == Some(entry.scope) {
            entry.requests += 1;
        }
        Some((entry.live.clone(), entry.scope))
    }

    /// The two tokens of this job: the model token for the container and the
    /// job token for the contractor, kept until [`forget`] says the job is
    /// over. Two jobs of one issue take turns, so the attempt number is part
    /// of what is remembered: a late request of the attempt before carries no
    /// token that is still on the list. Both or neither: a job is never given
    /// one scope without the other.
    pub fn issue(
        &self,
        job: i64,
        attempt: i64,
        contractor: &str,
        random: &mut dyn FnMut() -> Option<String>,
    ) -> Option<Pair> {
        let model = minted(random)?;
        let job_token = minted(random).filter(|t| *t != model)?;
        // A token whose hash the store did not keep would die with this
        // process, so none is given.
        if let Some(store) = &self.store {
            if !store.keep(job, Scope::Model, &token_hash(&model))
                || !store.keep(job, Scope::Job, &token_hash(&job_token))
            {
                return None;
            }
        }
        let live = Live {
            job,
            attempt,
            contractor: contractor.to_string(),
        };
        let mut held = self.lock();
        for (token, scope) in [(&model, Scope::Model), (&job_token, Scope::Job)] {
            held.insert(
                token.clone(),
                Entry {
                    live: live.clone(),
                    scope,
                    requests: 0,
                },
            );
        }
        drop(held);
        Some(Pair {
            model,
            job: job_token,
        })
    }

    /// A model token for the control line, which writes it where a
    /// container's models.json is made from and may name a job the store does
    /// not hold: the hash is kept when the store takes it, and the token is
    /// held either way, so a token the store did not keep ends with this
    /// process. The control line never makes a job token.
    pub fn issue_held(
        &self,
        job: i64,
        attempt: i64,
        contractor: &str,
        random: &mut dyn FnMut() -> Option<String>,
    ) -> Option<String> {
        let token = minted(random)?;
        if let Some(store) = &self.store {
            let _ = store.keep(job, Scope::Model, &token_hash(&token));
        }
        self.put_scoped(&token, Scope::Model, job, attempt, contractor, 0);
        Some(token)
    }

    /// The job and contractor a token of `scope` names, and the count of
    /// requests that carried it going up by one: the count of rule 5 for a
    /// request whose token names a job, since a request of a token never
    /// issued counts for nobody. [`Issuer::asked`] is the count of the
    /// requests that name none. A token of the other scope is
    /// [`Miss::Scope`] and counts for nobody.
    pub fn look(&self, token: &str, scope: Scope) -> Result<Live, Miss> {
        {
            let mut held = self.lock();
            if let Some(entry) = held.get_mut(token) {
                if entry.scope != scope {
                    return Err(Miss::Scope);
                }
                entry.requests += 1;
                return Ok(entry.live.clone());
            }
        }
        match self.recall(token, Some(scope)) {
            Some((live, s)) if s == scope => Ok(live),
            Some(_) => Err(Miss::Scope),
            None => Err(Miss::Unknown),
        }
    }

    /// The scope of a token that names a live job, without counting.
    pub fn scope_of(&self, token: &str) -> Option<Scope> {
        let held = self.lock().get(token).map(|e| e.scope);
        held.or_else(|| self.recall(token, None).map(|(_, s)| s))
    }

    /// The job and contractor a token of either scope names, without
    /// counting a request.
    pub fn peek(&self, token: &str) -> Option<Live> {
        let held = self.lock().get(token).map(|e| e.live.clone());
        held.or_else(|| self.recall(token, None).map(|(l, _)| l))
    }

    /// Put one model token in the table by hand. A test of the rules of one
    /// request uses it; a token of a job on a host is issued, never put, since
    /// the proxy is the one holder of a token and mints it of `/dev/urandom`.
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
        self.put_scoped(token, Scope::Model, job, attempt, contractor, requests)
    }

    /// [`Self::put`] of a token of either scope.
    #[doc(hidden)]
    pub fn put_scoped(
        &self,
        token: &str,
        scope: Scope,
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
                scope,
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

    /// The job is over, or its contractor is revoked: the token dies now with
    /// the job's token of the other scope, and a request that carries either
    /// from here on is the one of a job that ended.
    pub fn forget(&self, token: &str) -> bool {
        let job = self.lock().get(token).map(|e| e.live.job);
        job.is_some_and(|job| self.forget_jobs(&[job]) > 0)
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

    /// What to write to the store: the requests each live job's tokens
    /// carried, both scopes together, by contractor and by job, and the counts
    /// cleared. A call that is refused before its token is read carries no
    /// token, so it counts for nobody.
    pub fn take_counts(&self) -> Vec<(String, i64, u64)> {
        let mut held = self.lock();
        let mut sums: std::collections::BTreeMap<(String, i64), u64> = Default::default();
        for e in held.values().filter(|e| e.requests > 0) {
            *sums
                .entry((e.live.contractor.clone(), e.live.job))
                .or_default() += e.requests;
        }
        for entry in held.values_mut() {
            entry.requests = 0;
        }
        sums.into_iter().map(|((c, j), n)| (c, j, n)).collect()
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
        self.clone()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One token of `random`. No randomness, or too little of it, is no token:
/// nothing is kept and the caller is told, so a job never gets a token that
/// cannot work.
fn minted(random: &mut dyn FnMut() -> Option<String>) -> Option<String> {
    random().filter(|t| t.len() >= 8)
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

    /// An issuer that gave out these jobs, and the model token of each.
    fn issuer_with(jobs: &[(i64, i64, &str)]) -> (Issuer, Vec<String>) {
        let (issuer, pairs) = issued(jobs);
        (issuer, pairs.into_iter().map(|p| p.model).collect())
    }

    fn issued(jobs: &[(i64, i64, &str)]) -> (Issuer, Vec<Pair>) {
        let issuer = Issuer::new();
        let mut pairs = Vec::new();
        let mut n = 0;
        for (job, attempt, contractor) in jobs {
            pairs.push(
                issuer
                    .issue(*job, *attempt, contractor, &mut || {
                        n += 1;
                        Some(format!("token-{n}-for-job-{job}"))
                    })
                    .unwrap(),
            );
        }
        (issuer, pairs)
    }

    fn look(issuer: &Issuer, token: &str) -> Option<Live> {
        issuer.look(token, Scope::Model).ok()
    }

    #[test]
    fn a_token_names_its_job_and_the_contractor_it_was_issued_to() {
        let (issuer, tokens) = issuer_with(&[(7, 2, "gpu1"), (8, 1, "gpu2")]);
        assert_eq!(
            look(&issuer, &tokens[0]),
            Some(Live {
                job: 7,
                attempt: 2,
                contractor: "gpu1".to_string()
            })
        );
        assert_eq!(
            look(&issuer, &tokens[1]),
            Some(Live {
                job: 8,
                attempt: 1,
                contractor: "gpu2".to_string()
            })
        );
        assert_eq!(look(&issuer, "tok-9"), None);
        assert_eq!(look(&issuer, ""), None);
    }

    #[test]
    fn a_job_has_a_token_of_each_scope_and_each_is_refused_on_the_other_s_paths() {
        let (issuer, pairs) = issued(&[(7, 2, "gpu1")]);
        let pair = &pairs[0];
        assert_ne!(pair.model, pair.job);
        assert_eq!(issuer.len(), 2);
        assert_eq!(issuer.look(&pair.model, Scope::Model).map(|l| l.job), Ok(7));
        assert_eq!(issuer.look(&pair.job, Scope::Job).map(|l| l.job), Ok(7));
        assert_eq!(issuer.look(&pair.model, Scope::Job), Err(Miss::Scope));
        assert_eq!(issuer.look(&pair.job, Scope::Model), Err(Miss::Scope));
        assert_eq!(issuer.look("token-of-none", Scope::Job), Err(Miss::Unknown));
        assert_eq!(issuer.scope_of(&pair.job), Some(Scope::Job));
        // A refused scope counts for nobody; the two taken count for the job once each.
        assert_eq!(issuer.take_counts(), vec![("gpu1".to_string(), 7, 2)]);
        // One lifetime: the end of the job takes both.
        assert_eq!(issuer.forget_jobs(&[7]), 2);
        assert_eq!(issuer.look(&pair.job, Scope::Job), Err(Miss::Unknown));
        assert_eq!(issuer.look(&pair.model, Scope::Model), Err(Miss::Unknown));
    }

    #[test]
    fn two_equal_tokens_are_no_pair() {
        let issuer = Issuer::new();
        let same = &mut || Some("the-same-token-twice".to_string());
        assert_eq!(issuer.issue(7, 1, "gpu1", same), None);
        assert!(issuer.is_empty());
    }

    #[test]
    fn a_forgotten_token_names_nothing_at_all() {
        let (issuer, pairs) = issued(&[(7, 1, "gpu1")]);
        let tokens = [pairs[0].model.clone()];
        assert!(issuer.holds(&tokens[0]));
        assert!(issuer.forget(&tokens[0]));
        assert!(!issuer.holds(&tokens[0]));
        assert!(!issuer.holds(&pairs[0].job), "one lifetime for both");
        assert_eq!(look(&issuer, &tokens[0]), None);
        assert!(!issuer.forget(&tokens[0]), "a second forget is no forget");
        assert!(issuer.is_empty());
    }

    #[test]
    fn revoking_a_contractor_takes_every_token_it_holds_and_no_other() {
        let (issuer, pairs) = issued(&[(1, 1, "gpu1"), (2, 1, "gpu1"), (3, 1, "gpu2")]);
        assert_eq!(issuer.forget_contractor("gpu1"), 4);
        for p in &pairs[..2] {
            assert!(!issuer.holds(&p.model) && !issuer.holds(&p.job));
        }
        assert!(
            issuer.holds(&pairs[2].model),
            "another contractor lost its job"
        );
        assert!(
            issuer.holds(&pairs[2].job),
            "another contractor lost its job"
        );
        assert_eq!(issuer.forget_contractor("nobody"), 0);
    }

    #[test]
    fn a_job_that_ended_takes_its_token_back() {
        let (issuer, tokens) = issuer_with(&[(1, 1, "gpu1"), (2, 3, "gpu2")]);
        assert_eq!(issuer.forget_jobs(&[1]), 2);
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

    /// A store of hashes: what a host's job rows keep, and whether it takes a
    /// hash at all.
    #[derive(Default)]
    struct Kept {
        rows: Mutex<HashMap<String, (Live, Scope)>>,
        down: std::sync::atomic::AtomicBool,
    }

    impl Backing for Kept {
        fn keep(&self, job: i64, scope: Scope, sha256: &str) -> bool {
            if self.down.load(std::sync::atomic::Ordering::SeqCst) {
                return false;
            }
            let live = Live {
                job,
                attempt: 1,
                contractor: "gpu1".to_string(),
            };
            self.rows
                .lock()
                .unwrap()
                .insert(sha256.to_string(), (live, scope));
            true
        }
        fn recall(&self, sha256: &str) -> Option<(Live, Scope)> {
            self.rows.lock().unwrap().get(sha256).cloned()
        }
    }

    #[test]
    fn a_backed_token_is_kept_as_its_hash_and_named_again_after_a_restart() {
        let kept = Arc::new(Kept::default());
        let before = Issuer::backed(kept.clone());
        let pair = before.issue(7, 1, "gpu1", &mut random).unwrap();
        let token = pair.model.clone();
        let rows = kept.rows.lock().unwrap().clone();
        let mut keys: Vec<&String> = rows.keys().collect();
        keys.sort();
        let mut want = [token_hash(&pair.model), token_hash(&pair.job)];
        want.sort();
        assert_eq!(keys, want.iter().collect::<Vec<_>>());
        assert!(!rows.keys().any(|k| k == &pair.model || k == &pair.job));
        // The process is gone; a new one holds nothing until a token comes,
        // and each comes back in its own scope only.
        let after = Issuer::backed(kept.clone());
        assert!(after.is_empty());
        assert_eq!(after.look(&pair.job, Scope::Model), Err(Miss::Scope));
        assert_eq!(after.look(&pair.model, Scope::Job), Err(Miss::Scope));
        assert_eq!(look(&after, &token).map(|l| l.job), Some(7));
        assert_eq!(after.look(&pair.job, Scope::Job).map(|l| l.job), Ok(7));
        assert_eq!(after.jobs(), vec![7]);
        assert_eq!(after.take_counts(), vec![("gpu1".to_string(), 7, 2)]);
        // A word of another shape is never looked up.
        assert_eq!(after.peek("token-of-7"), None);
        // An issuer with no store names nothing it did not mint.
        assert_eq!(look(&Issuer::new(), &token), None);
    }

    #[test]
    fn a_token_whose_hash_is_not_kept_is_not_given() {
        let kept = Arc::new(Kept::default());
        kept.down.store(true, std::sync::atomic::Ordering::SeqCst);
        let issuer = Issuer::backed(kept);
        assert_eq!(issuer.issue(7, 1, "gpu1", &mut random), None);
        assert!(issuer.is_empty());
        // The control line's token is held all the same, in this process only,
        // and it is a model token.
        let held = issuer.issue_held(7, 1, "gpu1", &mut random).unwrap();
        assert!(issuer.holds(&held));
        assert_eq!(issuer.scope_of(&held), Some(Scope::Model));
    }

    #[test]
    fn no_randomness_is_no_token_and_nothing_kept() {
        let issuer = Issuer::new();
        assert_eq!(issuer.issue(7, 1, "gpu1", &mut || None), None);
        assert_eq!(
            issuer.issue(7, 1, "gpu1", &mut || Some(String::new())),
            None
        );
        assert_eq!(
            issuer.issue(7, 1, "gpu1", &mut || Some("short".into())),
            None
        );
        assert!(issuer.is_empty());
    }
}
