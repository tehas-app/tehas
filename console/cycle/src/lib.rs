//! A cycle's supervisor in Rust (#256). So far its report: `report` takes
//! what an ended cycle left and the answers GitHub gives, and returns what
//! libexec/cycle does with them, and `apply` does it, where the instance asks
//! for it with TEHAS_CYCLE_REPORT. `shadow` decides beside the bash's report on
//! a host and says where the two differ. `pin` gives a cycle its own copy of the code.
//! `clock` measures a cycle on a clock that stops while the VM is saved.
//! `renew` keeps a cycle's GitHub token fresh and says what went wrong.
//! `finish` opens a pull request from what a cycle that stopped left.
//! `after` does what follows a cycle's record, and `improve` writes the
//! record the improvement loop reads.
//! `caps` reads what an image says its runner can do.
//! `index` builds the code index a cycle's agents search.
//! `sandbox` prints the docker options of a container for the commands that
//! keep their docker line in bash.
//! `start` holds a start to its approval before anything of it is written.
//! `prepare` makes what a container starts from.
//! `watch` starts a cycle's container and sees it to its end, and `progress`
//! summarises what the runner wrote.

pub mod after;
pub mod apply;
pub mod caps;
pub mod clock;
pub mod describe;
pub mod facts;
pub mod finish;
pub mod host;
pub mod improve;
pub mod index;
pub mod jq;
pub mod last_attempt;
pub mod pin;
pub mod prepare;
pub mod progress;
pub mod renew;
pub mod report;
pub mod runner;
pub mod sandbox;
pub mod shadow;
pub mod start;
pub mod watch;

pub use facts::Facts;
pub use report::{report, Report};
