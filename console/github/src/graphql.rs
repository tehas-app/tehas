//! GitHub's GraphQL answers: the one rule for an answer that carries
//! `errors`. GraphQL answers 200 with `errors` beside a `data` that may be
//! partial, so a read that takes `data` without this rule can take a part
//! for the whole. An answer with errors is no whole answer: a spent priced
//! budget when GitHub says the rate is limited, `Unread(Inconsistent)` for
//! any other. `items.rs`, which reads aliased questions alias by alias,
//! applies its own form of it, an error naming its alias.

use serde::de::{DeserializeOwned, IgnoredAny};
use serde::Deserialize;
use serde_json::Value;
use tehas_ports::{At, Got, Outcome, Pool, Unread};

/// The `errors` of an answer, and nothing else of it.
#[derive(Deserialize)]
struct Head {
    #[serde(default)]
    errors: Option<Errors>,
}

/// GitHub's list of errors, or anything else in its place, which is errors
/// too.
#[derive(Deserialize)]
#[serde(untagged)]
enum Errors {
    List(Vec<Error>),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
struct Error {
    #[serde(rename = "type", default)]
    kind: Option<String>,
}

/// The outcome of the answer `out` when its `errors` are there and not
/// null, at `now`; None for one without errors, or one that is no JSON
/// (which its reader finds malformed).
pub(crate) fn refusal(out: &str, now: i64) -> Option<Outcome> {
    let errors = serde_json::from_str::<Head>(out.trim()).ok()?.errors?;
    let limited = match errors {
        Errors::List(all) => all
            .iter()
            .any(|e| e.kind.as_deref() == Some("RATE_LIMITED")),
        Errors::Other(_) => false,
    };
    Some(if limited {
        Outcome::Spent {
            pool: Pool::Priced,
            until: At::new(now),
        }
    } else {
        Outcome::Unread(Unread::Inconsistent)
    })
}

/// The part of the answer `out` at `at` (a JSON pointer), its logins
/// vouched: [`refusal`] first, then `Unread(Malformed)` for an answer that
/// does not read or a part that is not there or not a `T`.
pub(crate) fn part<T: DeserializeOwned>(out: &str, at: &str, now: i64) -> Got<T> {
    let malformed = Outcome::Unread(Unread::Malformed);
    if let Some(refused) = refusal(out, now) {
        return Err(refused);
    }
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
    tehas_core::accounts::vouch(&mut v);
    let part = v
        .pointer_mut(at)
        .map(std::mem::take)
        .ok_or(malformed.clone())?;
    serde_json::from_value(part).map_err(|_| malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn errors_beside_data_are_no_whole_answer() {
        let with =
            r#"{"errors":[{"message":"Something went wrong"}],"data":{"repository":{"n":1}}}"#;
        assert_eq!(
            part::<Value>(with, "/data/repository", NOW),
            Err(Outcome::Unread(Unread::Inconsistent))
        );
        let limited = r#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded"}],"data":null}"#;
        assert_eq!(
            part::<Value>(limited, "/data", NOW),
            Err(Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(NOW)
            })
        );
        // An empty list of errors still carries errors: the rule is their
        // presence, not their count.
        let empty = r#"{"errors":[],"data":{"repository":{"n":1}}}"#;
        assert_eq!(
            part::<Value>(empty, "/data/repository", NOW),
            Err(Outcome::Unread(Unread::Inconsistent))
        );
    }

    #[test]
    fn an_answer_without_errors_gives_its_part() {
        let fine = r#"{"errors":null,"data":{"repository":{"n":1}}}"#;
        assert_eq!(
            part::<Value>(fine, "/data/repository/n", NOW),
            Ok(Value::from(1))
        );
        assert_eq!(
            part::<Value>(r#"{"data":{}}"#, "/data/repository", NOW),
            Err(Outcome::Unread(Unread::Malformed))
        );
        assert_eq!(
            part::<Value>("not json", "/data", NOW),
            Err(Outcome::Unread(Unread::Malformed))
        );
    }
}
