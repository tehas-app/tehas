//! The adapter's one rule for a rate limit, on every path it reads GitHub
//! by: a GraphQL `RATE_LIMITED`, gh's 429 or its words of a primary or
//! secondary limit, and an anonymous answer's 429 or spent budget. A limit
//! is the pool spent until the time GitHub gave, or for [`FLOOR`] past now
//! when it gave none. Any other answer is the outcome its path reads.

use tehas_ports::{At, Outcome, Pool};

/// GraphQL's error type for a spent budget.
pub(crate) const RATE_LIMITED: &str = "RATE_LIMITED";

/// How long a limit with no reset time holds its budget out: "until now"
/// would hold nothing, so a limited host would ask again at once.
const FLOOR: i64 = 60;

/// `pool` spent at `now`, until `until` (a reset time, or now plus a
/// `retry-after`) or the floor past now with none.
pub(crate) fn spent(pool: Pool, now: i64, until: Option<i64>) -> Outcome {
    let until = until.unwrap_or(now.saturating_add(FLOOR));
    Outcome::Spent {
        pool,
        until: At::new(until),
    }
}

/// [`spent`] when the answer says `limited`, else `other`.
pub(crate) fn limited(
    limited: bool,
    pool: Pool,
    now: i64,
    until: Option<i64>,
    other: Outcome,
) -> Outcome {
    match limited {
        true => spent(pool, now, until),
        false => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_ports::Unread;

    const NOW: i64 = 1_800_000_000;

    fn until(o: Outcome) -> i64 {
        match o {
            Outcome::Spent { until, .. } => until.secs(),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn a_limit_with_no_reset_holds_for_a_minute() {
        assert_eq!(until(spent(Pool::Priced, NOW, None)), NOW + 60);
    }

    #[test]
    fn a_reset_the_server_gives_is_kept() {
        assert_eq!(until(spent(Pool::Calls, NOW, Some(NOW + 600))), NOW + 600);
        assert_eq!(until(spent(Pool::Calls, NOW, Some(NOW + 5))), NOW + 5);
    }

    #[test]
    fn an_answer_that_is_no_limit_is_its_other_outcome() {
        let other = Outcome::Unread(Unread::Inconsistent);
        assert_eq!(
            limited(false, Pool::Priced, NOW, None, other.clone()),
            other
        );
        assert_eq!(
            limited(true, Pool::Priced, NOW, None, other),
            Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(NOW + 60)
            }
        );
    }
}
