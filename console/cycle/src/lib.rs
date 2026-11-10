//! A cycle's supervisor in Rust (#256). `report` takes what an ended cycle
//! left and the answers GitHub gives, and returns what to do with them, and
//! `apply` does it. `pin` gives a cycle its own copy of the code.
//! `clock` measures a cycle on a clock that stops while the VM is saved,
//! and `freeze` says what the watch does for a frozen cycle.
//! `renew` keeps a cycle's GitHub token fresh and says what went wrong.
//! `finish` opens a pull request from what a cycle that stopped left.
//! `after` does what follows a cycle's record, and `improve` writes the
//! record the improvement loop reads.
//! `caps` reads what an image says its runner can do.
//! `index` builds the code index a cycle's agents search.
//! `sandbox` prints the docker options of a container for the commands that
//! keep their docker line in bash.
//! `work` is tehas work: `start` holds a start to its approval before anything
//! of it is written, `gives` fetches what the issue links, `prepare` makes what
//! a container starts from, `conflict` reads whether a refresh's pull request
//! conflicts before one starts, and `watch` starts a cycle's container and sees it
//! to its end. `progress` summarises what the runner wrote.
//! `supervise` starts a cycle, runs tehas work and sees to its report.
//! `lock` takes the locks of a cycle, tehas work and the index, and hands them
//! down to what they start.

pub mod after;
pub mod apply;
pub mod caps;
pub mod clock;
pub mod conflict;
pub mod describe;
pub mod end_failed;
pub mod facts;
pub mod finish;
pub mod freeze;
pub mod gives;
pub mod host;
pub mod improve;
pub mod index;
pub mod last_attempt;
pub mod lock;
pub mod pin;
pub mod prepare;
pub mod progress;
pub mod renew;
pub mod report;
pub mod runner;
pub mod sandbox;
pub mod scratch;
pub mod start;
pub mod supervise;
pub mod watch;
pub mod work;

pub use facts::Facts;
pub use report::{report, Report};
