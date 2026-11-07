//! `tehas-cycle finish`: the factory opens a pull request from what a stopped
//! cycle left (#1131, part 1).
//!
//! A cycle that finished its change and then stopped on something outside it —
//! a flaky test in a file the change does not touch, a review lane that ran out
//! of disk, the gate's time bound — keeps its workspace, and nothing delivers
//! what the workspace holds. A person asks here, and the factory does the push,
//! so nobody outside the host needs write access to the code.
//!
//! The branch of the kept workspace is fetched into a repository of the run's
//! own, rebased there onto the default branch, pushed to a branch of its own,
//! `finish/issue-<n>`, and opened as a draft pull request that closes the issue.
//! No git runs in the workspace, because its git config is the container's to
//! write. The workspace is read and never written, and its branch never moves.
//! The run's repository and the body file go on every path, the refusals
//! included.
//!
//! Whether a failure is outside the change is the asker's judgement, and the
//! reason is theirs; it goes into the draft's body. Whether the gate passed is
//! not the asker's to say, and the draft says it did not.
//!
//! The command is given its words, reads no settings of its own, and asks nothing
//! of the state store. Part 2's poll step is its caller.
//!
//! The parts: [`cmd`] is the git and gh of one run and its scratch, [`plan`] is
//! what a run is given and the rules of those words, and [`run`] is the decision
//! — what is checked, in what order, and what is pushed and opened. The cases of
//! all three are in [`tests`], which drives a real repository and a stub `gh`.

pub mod cmd;
pub mod plan;
pub mod run;
#[cfg(test)]
mod tests;

pub use cmd::{Done, Host, Live};
pub use plan::{body, title, Finished, Plan};

/// What one ask for one issue did: the pull request it opened, or the one line
/// of why it refused and changed nothing.
pub fn finish(plan: &Plan, h: &mut dyn Host) -> Finished {
    if let Err(why) = plan.check() {
        return Finished::refused(why);
    }
    // A read-only instance (#608) pushes nothing: the guard says what this
    // would have done, and the run changes nothing.
    if tehas_core::read_only::refuse(&format!(
        "push finish/issue-{} and open its draft pull request",
        plan.issue
    )) {
        return Finished::refused(
            "TEHAS_READ_ONLY=1: nothing is pushed and no pull request is opened".to_string(),
        );
    }
    run::run(plan, h)
}

/// The prefix of every line this command says, so the poll's log of part 2 names
/// the step that refused without adding words of its own.
pub fn say(issue: u64, why: &str) {
    eprintln!("tehas-cycle: finish: #{issue}: {why}");
}
