//! The seconds tehas update and install.sh wait for a lock before they give up.
//! Both read them here, from the environment and before tehas_load, which drops
//! every TEHAS_* it does not know.
//!
//!   update     TEHAS_UPDATE_LOCK_WAIT, for state/update.lock: a setting of the
//!              host (rules::HOST), 0 to 300, 10 when unset or empty. Any other
//!              value stops the run with 78. The default is small so a refusal
//!              stays instant for the tests that time it; the ceiling keeps a
//!              mistyped number from hanging an operator.
//!   dispatch   TEHAS_DISPATCH_LOCK_WAIT, for state/dispatch.lock, which a poll
//!              holds for its whole run: 0 to 3600, else 900. A poll's own bounds
//!              add up to minutes, so the update lock's 10 would refuse an install
//!              whenever the timer's poll is running. It is internal: a leading
//!              zero, a sign, a fraction or anything past the ceiling is the
//!              default.
//!
//! 0 refuses at once.

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

/// The update wait as its rule reads it, or the line that refuses it.
fn update_seconds(value: Option<&str>) -> Result<u64, String> {
    let v = value.unwrap_or_default();
    if v.is_empty() {
        return Ok(UPDATE.default);
    }
    tehas_core::settings::rules::check(UPDATE.name, v, &Default::default())
        .map_err(|why| format!("{} is not valid: {why}", UPDATE.name))?;
    Ok(v.parse().unwrap_or(UPDATE.default))
}

/// The seconds `console update run` waits for state/update.lock, or the line
/// that refuses the value.
pub(super) fn update() -> Result<u64, String> {
    update_seconds(env::var(UPDATE.name).ok().as_deref())
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
    if wait.name != UPDATE.name {
        say(&seconds(&wait, value.as_deref()).to_string());
        return 0;
    }
    match update_seconds(value.as_deref()) {
        Ok(n) => {
            say(&n.to_string());
            0
        }
        Err(line) => {
            super::fault(&line);
            78
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(wait: &Wait, value: &str) -> u64 {
        seconds(wait, Some(value))
    }

    #[test]
    fn the_update_wait_takes_0_through_300_and_refuses_anything_else() {
        assert_eq!(update_seconds(None), Ok(10));
        assert_eq!(update_seconds(Some("")), Ok(10));
        for (v, want) in [("0", 0), ("1", 1), ("45", 45), ("300", 300)] {
            assert_eq!(update_seconds(Some(v)), Ok(want), "{v:?}");
        }
        for v in [
            "301",
            "9999",
            "999999999999999999999",
            "00",
            "05",
            "0000000009",
            "-1",
            "+5",
            "1.5",
            "1e3",
            "soon",
            " 5",
        ] {
            assert_eq!(
                update_seconds(Some(v)),
                Err("TEHAS_UPDATE_LOCK_WAIT is not valid: expected a whole number of seconds from 0 to 300".into()),
                "{v:?}"
            );
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
