//! A frozen cycle (#1745). `tehas freeze <issue>` writes
//! `state/cycles/<issue>.freeze` and `tehas thaw <issue>` removes it. The
//! watch of the cycle (src/watch.rs) does the rest, because it is the one
//! process that owns the container: it pauses the container while the request
//! is there and unpauses it once the request is gone.
//!
//! While the container is paused, neither the stall nor the time limit is
//! checked. On the thaw the start's mark moves forward by the time the cycle
//! was frozen, and the stall counts from the thaw, so the cycle has the time
//! it had left. Once the container is paused the watch writes
//! `state/cycles/<issue>.frozen` with the clock reading it was paused at, and
//! it removes that file on the thaw.

/// What the watch does on one look.
#[derive(Debug, PartialEq, Eq)]
pub enum Move {
    Pause,
    Unpause,
    Stay,
}

/// The move for a look that finds the request `asked` and the container
/// `paused`.
pub fn next(asked: bool, paused: bool) -> Move {
    match (asked, paused) {
        (true, false) => Move::Pause,
        (false, true) => Move::Unpause,
        _ => Move::Stay,
    }
}

/// The start's mark after a freeze from `frozen_at` to `now`, all on the
/// cycle clock. Nothing for a clock that ran backwards: the caller then
/// leaves the mark alone rather than guess.
pub fn shifted(start: u64, frozen_at: u64, now: u64) -> Option<u64> {
    now.checked_sub(frozen_at)?.checked_add(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_pauses_and_its_removal_unpauses() {
        assert_eq!(next(true, false), Move::Pause);
        assert_eq!(next(false, true), Move::Unpause);
    }

    #[test]
    fn a_look_that_finds_things_as_they_are_does_nothing() {
        assert_eq!(next(true, true), Move::Stay);
        assert_eq!(next(false, false), Move::Stay);
    }

    #[test]
    fn the_start_moves_forward_by_the_time_frozen() {
        // Started at 1000, frozen at 1600 for an hour.
        assert_eq!(shifted(1000, 1600, 5200), Some(4600));
        assert_eq!(shifted(1000, 1600, 1600), Some(1000));
    }

    #[test]
    fn a_clock_that_ran_backwards_moves_nothing() {
        assert_eq!(shifted(1000, 1600, 1500), None);
        assert_eq!(shifted(u64::MAX, 0, 1), None);
    }
}
