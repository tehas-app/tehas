//! The exit statuses of the public commands, as host/docs/contract.md lists
//! them. One meaning per number in every command; a status only one command
//! uses is in that command's module below.

/// Done.
pub const OK: u8 = 0;
/// A call, a read or a write did not succeed.
pub const FAILED: u8 = 1;
/// The command worked and the answer is no, or not yet.
pub const NO: u8 = 3;
/// A bad argument.
pub const USAGE: u8 = 64;
/// Settings missing or malformed, or a feature that is off or still being
/// built.
pub const SETTINGS: u8 = 78;
/// Standard output closed before the command finished printing.
pub const CLOSED: u8 = 141;

/// Every status above with its line in the contract page's common table.
pub const COMMON: &[(u8, &str)] = &[
    (OK, "done"),
    (FAILED, "failed: a call, a read or a write did not succeed"),
    (NO, "the command worked and the answer is no, or not yet"),
    (USAGE, "a bad argument"),
    (
        SETTINGS,
        "settings missing or malformed, or a feature that is off",
    ),
    (
        CLOSED,
        "standard output closed before the command finished printing",
    ),
];

/// `tehas work`: tehas's own statuses beside the runner's.
pub mod work {
    /// The cycle was stopped as stalled or at its time limit, or the runner
    /// ended without a result.
    pub const STOPPED: u8 = 2;
    /// The sandbox cannot push to the repository; no cycle was started.
    pub const NO_PUSH: u8 = 3;
    /// No cycle was started: the approval, the issue's text or an attachment
    /// did not hold.
    pub const NOT_STARTED: u8 = 4;
    /// A cycle of the issue is running already.
    pub const BUSY: u8 = 5;
    /// The container was killed while the cycle ran.
    pub const KILLED: u8 = 137;
}

/// `tehas plan`.
pub mod plan {
    /// The runner failed, ran out of time, refused the turn or said nothing.
    pub const FAILED: u8 = 2;
    /// An image or file the issue links could not be fetched; nothing ran.
    pub const NOT_FETCHED: u8 = 4;
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT: &str = include_str!("../../../host/docs/contract.md");

    #[test]
    fn the_contract_page_lists_every_common_status_with_its_meaning() {
        for (code, meaning) in COMMON {
            let row = format!("| {code} | {meaning} |");
            assert!(CONTRACT.contains(&row), "contract.md has no row {row}");
        }
    }

    #[test]
    fn a_common_status_has_one_meaning() {
        let mut codes: Vec<u8> = COMMON.iter().map(|(c, _)| *c).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), COMMON.len());
    }
}
