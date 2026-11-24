//! A GraphQL question of the repository that asks several numbers at
//! once, an alias a number, read item by item. GitHub answers a number it
//! does not know with a null node and a `NOT_FOUND` error at that alias,
//! and gh then exits 1, so the question is read from gh's answer beside
//! its failure (Decided 121). `items.rs` asks what each number names and
//! `port/code_host/numbers.rs` the changes of the numbers a filter names.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::{ForgeError, GitHub};
use tehas_ports::{Got, Outcome, Unread};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);

/// The alias number `n` is asked under.
pub(crate) fn alias(n: u64) -> String {
    format!("n{n}")
}

/// gh's answer, the nodes of the repository each of type `T`.
#[derive(Deserialize)]
#[serde(bound = "T: DeserializeOwned")]
struct Answer<T> {
    data: Option<Data<T>>,
    #[serde(default)]
    errors: Vec<Error>,
}

#[derive(Deserialize)]
#[serde(bound = "T: DeserializeOwned")]
struct Data<T> {
    /// Each alias asked, null where GitHub has no node for it, and any
    /// other field asked of the repository.
    repository: Option<BTreeMap<String, Option<T>>>,
    /// The account asking, when the question asks it.
    #[serde(default)]
    viewer: Option<Value>,
}

#[derive(Deserialize)]
struct Error {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    path: Vec<Value>,
}

impl Error {
    /// The alias this error is about, None for one about the question.
    fn alias(&self) -> Option<&str> {
        match self.path.as_slice() {
            [Value::String(r), Value::String(a), ..] if r == "repository" => Some(a),
            _ => None,
        }
    }

    /// GitHub's word that the alias names nothing: the alias itself is not
    /// found, not something inside it (Decided 121).
    fn not_found(&self) -> bool {
        self.kind.as_deref() == Some("NOT_FOUND") && self.path.len() == 2
    }
}

/// What a question of aliases answered.
pub(crate) struct Aliased<T> {
    /// Each alias's answer, in the order asked: its node; `NotFound` for a
    /// null node with exactly one not-found error at it; `Unread(Malformed)`
    /// for an alias missing from the answer; `Unread(Inconsistent)` for any
    /// other error at it, or a null node with none.
    pub(crate) nodes: Vec<Got<T>>,
    /// The repository's other fields, as asked beside the aliases.
    pub(crate) repository: BTreeMap<String, Option<T>>,
    /// The account asking, when asked.
    pub(crate) viewer: Option<Value>,
}

impl GitHub {
    /// The question `args` of `aliases`, read beside gh's failure. Err is
    /// the question's own outcome, for every alias: the call's, when gh
    /// failed with no answer, an answer that is no JSON of this shape, no
    /// repository, or data with no error to read it by; `Unread(Malformed)`
    /// for such an answer of a call that did not fail; and
    /// `Unread(Inconsistent)` for an error that names no alias.
    pub(crate) fn ask_aliased<T: DeserializeOwned>(
        &mut self,
        call: &str,
        args: &[&str],
        aliases: &[String],
    ) -> Result<Aliased<T>, Outcome> {
        let (out, failed) = match self.call_answer(call, args) {
            Ok(out) => (out, None),
            Err((e, Some(out))) => (out, Some(e)),
            Err((e, None)) => return Err(self.failed(&e, aliases.len())),
        };
        let whole = |gh: &Self, failed: Option<ForgeError>| {
            failed.map_or(MALFORMED, |e| gh.failed(&e, aliases.len()))
        };
        let parsed = serde_json::from_str::<Value>(out.trim()).and_then(|mut v| {
            tehas_core::accounts::vouch(&mut v);
            serde_json::from_value::<Answer<T>>(v)
        });
        let Ok(answer) = parsed else {
            return Err(whole(self, failed));
        };
        let (repository, viewer) = match answer.data {
            Some(d) => (d.repository, d.viewer),
            None => (None, None),
        };
        let repository = match (repository, failed.as_ref()) {
            (Some(r), None) => r,
            (Some(r), Some(_)) if !answer.errors.is_empty() => r,
            // No node, or gh failed with no error to read the nodes by.
            _ => return Err(whole(self, failed)),
        };
        if answer.errors.iter().any(|e| e.alias().is_none()) {
            return Err(INCONSISTENT);
        }
        let mut repository = repository;
        let nodes = aliases
            .iter()
            .map(|a| {
                let errors: Vec<&Error> = answer
                    .errors
                    .iter()
                    .filter(|e| e.alias() == Some(a))
                    .collect();
                match (repository.remove(a), errors.as_slice()) {
                    (None, _) => Err(MALFORMED),
                    (Some(None), [e]) if e.not_found() => Err(Outcome::NotFound),
                    (Some(Some(node)), []) => Ok(node),
                    _ => Err(INCONSISTENT),
                }
            })
            .collect();
        Ok(Aliased {
            nodes,
            repository,
            viewer,
        })
    }

    /// The outcome of a question gh failed with no answer to read. A
    /// number GitHub does not know fails the whole question, so where
    /// several were asked a `NotFound` names none of them.
    fn failed(&self, e: &ForgeError, asked: usize) -> Outcome {
        match self.outcome(e) {
            Outcome::NotFound if asked > 1 => INCONSISTENT,
            o => o,
        }
    }
}
