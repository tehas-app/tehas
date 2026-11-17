//! The model proxy of outsource: the one way a contractor's cycle reaches
//! the factory's model, so the factory's real key never leaves the factory.
//!
//! A contractor calls this factory with a job's token. The proxy checks the
//! token against its own list and the job and the contractor against the
//! factory's records, forwards the request to the endpoint the instance has
//! configured, and carries the answer back as it arrives. It runs apart from
//! the dispatcher, in a program of its own, so a stream that stalls holds up
//! no poll.
//!
//! Seven rules, each with a test beside the code that keeps it:
//!
//! 1. A streamed reply is passed through as it arrives, never buffered.
//! 2. Its idle bound is a setting, and the load refuses one that is not
//!    longer than the check-in timeout (`TEHAS_OUTSOURCE_PROXY_IDLE`).
//! 3. Only the model paths a cycle needs are forwarded ([`PATHS`]); the rest
//!    are refused before an upstream call is made.
//! 4. A job that has ended, a contractor that is revoked and a path outside
//!    the list each end the request with one line in the log and no upstream
//!    call, judged by what the factory's records say and never by a field the
//!    contractor sent.
//! 5. Requests are counted per contractor and per job. The endpoint's token
//!    figures are read of each answer, and are kept from #2277 on.
//! 6. A body is never logged: it holds source code.
//! 7. A busy upstream's answer goes back as it is, status and body, so the
//!    agents' retry of a busy model (#215) works unchanged.
//!
//! A job's tokens are secrets, one per scope: the model token, which the
//! container holds and the model paths take, and the job token, which the
//! contractor program holds and the job channel takes. They are never in a
//! command line, never in a log and never in the store, which keeps only
//! their hashes: [`jobs::Issuer`] holds the tokens of live jobs in this
//! process and hands each pair out once, to the contractor that took the job.

pub mod channel;
pub mod count;
pub mod forward;
pub mod host;
pub mod jobs;
pub mod log;
pub mod records;
pub mod request;
pub mod serve;
