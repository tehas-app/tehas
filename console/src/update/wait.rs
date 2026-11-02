//! The seconds tehas update and install.sh wait for a lock before they give up.
//! Both read them here, from the environment and before tehas_load, which drops
//! every TEHAS_* it does not know: these are knobs of a run, not settings of an
//! instance.
//!
//!   update     TEHAS_UPDATE_LOCK_WAIT, for state/update.lock: 0 to 300, else 10.
//!              The default is small so a refusal stays instant for the tests
//!              that time it; the ceiling keeps a mistyped number from hanging
//!              an operator.
//!   dispatch   TEHAS_DISPATCH_LOCK_WAIT, for state/dispatch.lock, which a poll
//!              holds for its whole run: 0 to 3600, else 900. A poll's own bounds
//!              add up to minutes, so the update lock's 10 would refuse an install
//!              whenever the timer's poll is running.
//!
//! 0 refuses at once. A leading zero, a sign, a fraction or anything past the
//! ceiling is the default.

use super::{say, tell};
use std::env;

struct Wait {
    name: &'static str,
    default: u64,
    max: u64,
}

const UPDATE: Wait = Wait {
    name: "TEHAS_UPDATE_LOCK_WAIT",
    default: 10,
    max: 300,
};

const DISPATCH: Wait = Wait {
    name: "TEHAS_DISPATCH_LOCK_WAIT",
    default: 900,
    max: 3600,
};

fn seconds(wait: &Wait, value: Option<&str>) -> u64 {
    let Some(v) = value else {
        return wait.default;
    };
    let digits = !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit());
    if !digits || (v.len() > 1 && v.starts_with('0')) {
        return wait.default;
    }
    v.parse()
        .ok()
        .filter(|n| *n <= wait.max)
        .unwrap_or(wait.default)
}

/// `console update wait update|dispatch`: the seconds, one line.
pub fn verb(given: &[String]) -> u8 {
    let wait = match given {
        [w] if w == "update" => UPDATE,
        [w] if w == "dispatch" => DISPATCH,
        _ => {
            tell("console update wait: usage: update | dispatch");
            return 64;
        }
    };
    let value = env::var(wait.name).ok();
    say(&seconds(&wait, value.as_deref()).to_string());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(wait: &Wait, value: &str) -> u64 {
        seconds(wait, Some(value))
    }

    #[test]
    fn the_update_wait_takes_0_through_300_and_is_10_otherwise() {
        assert_eq!(seconds(&UPDATE, None), 10);
        for (v, want) in [
            ("0", 0),
            ("1", 1),
            ("45", 45),
            ("300", 300),
            ("301", 10),
            ("9999", 10),
            ("999999999999999999999", 10),
            ("00", 10),
            ("05", 10),
            ("0000000009", 10),
            ("-1", 10),
            ("+5", 10),
            ("1.5", 10),
            ("1e3", 10),
            ("soon", 10),
            (" 5", 10),
            ("", 10),
        ] {
            assert_eq!(asked(&UPDATE, v), want, "{v:?}");
        }
    }

    #[test]
    fn the_dispatch_wait_takes_0_through_3600_and_is_900_otherwise() {
        assert_eq!(seconds(&DISPATCH, None), 900);
        for (v, want) in [
            ("0", 0),
            ("60", 60),
            ("3600", 3600),
            ("3601", 900),
            ("012", 900),
            ("x", 900),
            ("", 900),
        ] {
            assert_eq!(asked(&DISPATCH, v), want, "{v:?}");
        }
    }

    #[test]
    fn a_wait_it_does_not_know_is_a_usage_error() {
        assert_eq!(verb(&["poll".to_string()]), 64);
        assert_eq!(verb(&[]), 64);
    }
}
