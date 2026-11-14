//! The issue lists of `live/status.json`: the keys `tehasbot status` writes
//! them under. The health page, the analytics sampler and the MCP server
//! read the lists by these constants, so a list is renamed in one place.

/// `running`: the issues a cycle is working on.
pub const RUNNING: &str = "running";
/// `queued`: the ready issues in line for a cycle.
pub const QUEUED: &str = "queued";
/// `awaiting`: the issues whose pull request waits for review.
pub const AWAITING: &str = "awaiting";
/// `stopped`: the issues that need a person.
pub const STOPPED: &str = "stopped";
/// `planned`: the issues in planning or with a plan.
pub const PLANNED: &str = "planned";
/// `completed`: the issues shipped lately, newest first.
pub const COMPLETED: &str = "completed";

/// Every list, in the order the file writes them.
pub const LISTS: [&str; 6] = [RUNNING, QUEUED, AWAITING, STOPPED, PLANNED, COMPLETED];

#[cfg(test)]
mod tests {
    use super::*;

    // The names are the file's wire format: the dashboard and every host's
    // older readers spell them so. A rename here must be deliberate.
    #[test]
    fn the_list_names_are_the_wire_names() {
        assert_eq!(
            LISTS,
            [
                "running",
                "queued",
                "awaiting",
                "stopped",
                "planned",
                "completed"
            ]
        );
    }
}
