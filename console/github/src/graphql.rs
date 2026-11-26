//! GitHub's GraphQL answers: the one rule for an answer that carries
//! `errors`. GraphQL answers 200 with `errors` beside a `data` that may be
//! partial, so a read that takes `data` without this rule can take a part
//! for the whole. An answer with errors is no whole answer: a spent priced
//! budget when GitHub says the rate is limited, `Unread(Inconsistent)` for
//! any other. `items.rs`, which reads aliased questions alias by alias,
//! applies its own form of it, an error naming its alias.

use crate::rate;
use serde::de::{DeserializeOwned, Deserializer, IgnoredAny};
use serde::Deserialize;
use serde_json::Value;
use tehas_ports::{Got, Outcome, Pool, Unread};

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

/// One of GraphQL's `errors`: its type, and where in the answer it lies.
/// The aliased readers of `items.rs` and `port/board/work.rs` name an
/// error's item by its path; the rule above reads only its type.
#[derive(Deserialize)]
pub(crate) struct Error {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default, deserialize_with = "steps")]
    pub(crate) path: Vec<Step>,
}

/// A step of an error's path: a field or an alias, or a place in a list.
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum Step {
    At(u64),
    Name(String),
    Other(IgnoredAny),
}

impl Error {
    /// Whether GitHub gave this error the type `kind`.
    pub(crate) fn is(&self, kind: &str) -> bool {
        self.kind.as_deref() == Some(kind)
    }
}

/// A path that is no list, null among them, is none.
fn steps<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Step>, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(d)?).unwrap_or_default())
}

/// A spent budget of a GraphQL call is the priced pool's.
pub(crate) fn priced(o: Outcome) -> Outcome {
    match o {
        Outcome::Spent { until, .. } => Outcome::Spent {
            pool: Pool::Priced,
            until,
        },
        o => o,
    }
}

/// The outcome of the answer `out` when it carries errors, at `now`; None
/// for one without, or one that is no JSON object (which its reader finds
/// malformed). An object whose `errors` the head cannot read, a key given
/// twice among them, carries errors too: no reading of it may say none.
pub(crate) fn refusal(out: &str, now: i64) -> Option<Outcome> {
    let text = out.trim();
    let v: Value = serde_json::from_str(text).ok()?;
    if !v.is_object() {
        return None;
    }
    let Ok(head) = serde_json::from_str::<Head>(text) else {
        return Some(Outcome::Unread(Unread::Inconsistent));
    };
    if head.errors.is_none() && v.get("errors").is_none_or(Value::is_null) {
        return None;
    }
    let limited = match head.errors {
        Some(Errors::List(all)) => all
            .iter()
            .any(|e| e.kind.as_deref() == Some(rate::RATE_LIMITED)),
        _ => false,
    };
    let other = Outcome::Unread(Unread::Inconsistent);
    Some(rate::limited(limited, Pool::Priced, now, None, other))
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
    use tehas_ports::At;

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
                until: At::new(NOW + 60)
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
    fn errors_given_twice_are_errors_whichever_comes_last() {
        for twice in [
            r#"{"errors":[{"message":"no"}],"errors":null,"data":{"repository":{"n":1}}}"#,
            r#"{"errors":null,"errors":[{"message":"no"}],"data":{"repository":{"n":1}}}"#,
            r#"{"errors":null,"errors":null,"data":{"repository":{"n":1}}}"#,
        ] {
            assert_eq!(
                part::<Value>(twice, "/data/repository", NOW),
                Err(Outcome::Unread(Unread::Inconsistent)),
                "{twice}"
            );
        }
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
