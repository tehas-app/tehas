//! The job channel: everything a contractor asks of the factory besides the
//! model, on the proxy's own listener and address.
//!
//! | Path | Who calls | What it does |
//! | --- | --- | --- |
//! | `join` | a host with a one-time code | registers its name and key |
//! | `hello` | a contractor, by its key | "still here", between jobs |
//! | `job` | a contractor, by its key | takes the offer in its hand, if any |
//! | `check-in` | a job, by its token | "still here", while it runs |
//! | `token` | a job, by its token | a fresh cycle token for the repository |
//! | `relay` | a job, by its token | one issue write, checked and made here |
//! | `result` | a job, by its token | the cycle's result, kept for its report |
//! | `stopped` | a job, by its token | the job handed back or given up |
//!
//! Each path is `/outsource/v1/<name>`, POST, a JSON body of at most
//! [`BODY_MAX`] bytes and a JSON answer. A contractor's key is sent as a bearer
//! with its name in `x-tehas-contractor`; the store holds the key's sha256 and
//! compares it. A job's token is the one the proxy issued when the job was
//! taken, which the model paths take too.
//!
//! What each call requires is the spec's "The job channel": check-in, token
//! and relay need a running job of the issue's current attempt on a contractor
//! that is not revoked; result and stopped also take a job that is stopping or
//! was lost to a silence, while its attempt is current. Only a revoked
//! contractor is answered `revoked`, which is the one answer that tells a
//! contractor to push nothing. Every other refusal names the state.
//!
//! The App's key never leaves: a token renewal answers the narrowed cycle
//! token a local cycle gets, and a relayed write is made here with the
//! factory's own token, after the checks of the local relay.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tehas_cycle::relay::Scope;

use crate::jobs::Issuer;
use crate::request::Live;

/// The paths of the channel start with this.
pub const PREFIX: &str = "/outsource/v1/";

/// The largest body the channel reads: a result of a cycle, or one relayed
/// write with its body (the local relay takes a body of 1 MiB).
pub const BODY_MAX: usize = 4 << 20;

/// The header that names a contractor on the calls made with its key.
pub const NAME_HEADER: &str = "x-tehas-contractor";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    Join,
    Hello,
    Job,
    CheckIn,
    Token,
    Relay,
    Result,
    Stopped,
}

impl Call {
    /// The call a path names, or None for a path that is not the channel's.
    pub fn of(path: &str) -> Option<Call> {
        let path = path.split_once('?').map_or(path, |(p, _)| p);
        Some(match path.strip_prefix(PREFIX)? {
            "join" => Call::Join,
            "hello" => Call::Hello,
            "job" => Call::Job,
            "check-in" => Call::CheckIn,
            "token" => Call::Token,
            "relay" => Call::Relay,
            "result" => Call::Result,
            "stopped" => Call::Stopped,
            _ => return None,
        })
    }

    pub fn word(self) -> &'static str {
        match self {
            Call::Join => "join",
            Call::Hello => "hello",
            Call::Job => "job",
            Call::CheckIn => "check-in",
            Call::Token => "token",
            Call::Relay => "relay",
            Call::Result => "result",
            Call::Stopped => "stopped",
        }
    }
}

/// What the channel needs of the factory. The host's is [`crate::host`]'s; a
/// test gives its own.
pub trait Factory {
    /// One verb of the state store and its answer, None when it gave none.
    fn store(&self, args: &[&str]) -> Option<Value>;
    /// A fresh narrowed cycle token for the repository and the second it runs
    /// out, as `tehasbot github-token --cycle` keeps it.
    fn cycle_token(&self) -> Option<(String, i64)>;
    /// One relayed issue write of a job's cycle, planned and held to `scope`
    /// here as the local relay does: gh's output, error and status, or why it
    /// was refused.
    fn relay(
        &self,
        args: &[String],
        input: Option<&str>,
        scope: &mut Scope,
    ) -> Result<(String, String, i32), String>;
    /// Keep the result of an issue's cycle where its report reads it.
    fn keep(&self, issue: i64, result: &[u8], exit: i64) -> Result<(), String>;
    /// What a job carries besides its numbers and token: the settings that
    /// may leave the factory, and the models.json of the proxy.
    fn carried(&self) -> Value;
    /// The seconds a job may go without a check-in.
    fn timeout(&self) -> i64;
    /// A new job token.
    fn random(&self) -> Option<String>;
}

/// One answer of the channel: a status and a JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

impl Answer {
    fn ok(body: Value) -> Answer {
        Answer { status: 200, body }
    }

    /// A refusal: 401 for a key or token the factory does not know, 403 for
    /// one it knows and refuses, 409 for an attempt replaced by a newer one.
    fn no(status: u16, word: &str) -> Answer {
        Answer {
            status,
            body: json!({ "refused": word }),
        }
    }

    pub fn refused(&self) -> Option<&str> {
        self.body["refused"].as_str()
    }
}

/// The sha256 of a contractor's key, as the store holds it.
pub fn key_hash(key: &str) -> String {
    hex(&Sha256::digest(key.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The channel of one proxy: the tokens it issued and the relay scopes of the
/// jobs it answers for.
pub struct Channel<'a> {
    pub issuer: &'a Issuer,
    pub factory: &'a (dyn Factory + Sync),
    scopes: Mutex<HashMap<i64, Scope>>,
}

/// One request of the channel, as the listener read it.
pub struct Asked<'a> {
    pub call: Call,
    pub bearer: Option<&'a str>,
    pub name: Option<&'a str>,
    pub body: &'a [u8],
}

impl<'a> Channel<'a> {
    pub fn new(issuer: &'a Issuer, factory: &'a (dyn Factory + Sync)) -> Channel<'a> {
        Channel {
            issuer,
            factory,
            scopes: Mutex::new(HashMap::new()),
        }
    }

    pub fn answer(&self, asked: &Asked) -> Answer {
        let body: Value = if asked.body.is_empty() {
            json!({})
        } else {
            match serde_json::from_slice(asked.body) {
                Ok(v @ Value::Object(_)) => v,
                _ => return Answer::no(400, "a body that is no JSON object"),
            }
        };
        match asked.call {
            Call::Join => self.join(&body),
            Call::Hello | Call::Job => match self.contractor(asked) {
                Ok((name, draining)) if asked.call == Call::Hello => {
                    let _ = self.factory.store(&["contractor-seen", "--name", &name]);
                    Answer::ok(json!({ "hello": name, "draining": draining }))
                }
                Ok((name, draining)) => self.job(&name, draining),
                Err(no) => no,
            },
            call => {
                let Some(live) = asked.bearer.and_then(|t| self.issuer.look(t)) else {
                    return Answer::no(401, "no job");
                };
                match self.held(&live, call) {
                    Ok(issue) => self.of_job(call, &live, issue, &body),
                    Err(no) => no,
                }
            }
        }
    }

    fn join(&self, body: &Value) -> Answer {
        let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
        let (code, name, key) = (text("code"), text("name"), text("key"));
        if code.is_empty() || name.is_empty() || key.is_empty() {
            return Answer::no(400, "a join takes a code, a name and a key");
        }
        let said = self.factory.store(&[
            "invite-use",
            "--code",
            &key_hash(&code),
            "--name",
            &name,
            "--key",
            &key,
        ]);
        match said.as_ref().and_then(|v| v["verb"].as_str()) {
            Some("joined") => Answer::ok(json!({ "joined": name })),
            Some(word @ ("expired" | "unknown" | "used")) => Answer::no(403, word),
            _ => Answer::no(403, "refused"),
        }
    }

    /// The contractor a call made with a key names, and whether it drains.
    fn contractor(&self, asked: &Asked) -> Result<(String, bool), Answer> {
        let (Some(name), Some(key)) = (asked.name, asked.bearer) else {
            return Err(Answer::no(401, "no contractor"));
        };
        let said = self
            .factory
            .store(&["contractor-auth", "--name", name, "--key", &key_hash(key)])
            .unwrap_or_default();
        match (said["verb"].as_str(), said["said"].as_str()) {
            (Some("allowed"), _) => Ok((name.to_string(), said["draining"] == true)),
            (_, Some("revoked")) => Err(Answer::no(403, "revoked")),
            _ => Err(Answer::no(401, "no contractor")),
        }
    }

    /// The offer in a contractor's hand, taken: its token is issued and the
    /// job moves to running. A draining contractor is given nothing.
    fn job(&self, name: &str, draining: bool) -> Answer {
        let none = Answer {
            status: 200,
            body: json!({ "job": null }),
        };
        if draining {
            return none;
        }
        let Some(offer) = self
            .factory
            .store(&["job-offered", "--contractor", name])
            .filter(|v| v["verb"] == "job")
        else {
            return none;
        };
        let (Some(job), Some(issue), Some(attempt)) = (
            offer["job"].as_i64(),
            offer["issue"].as_i64(),
            offer["attempt"].as_i64(),
        ) else {
            return none;
        };
        let Some(token) = self
            .issuer
            .issue(job, attempt, name, &mut || self.factory.random())
        else {
            return Answer::no(503, "no token could be made");
        };
        let taken = self
            .factory
            .store(&["job-check-in", "--job", &job.to_string()])
            .is_some_and(|v| v["verb"] == "checked-in");
        if !taken {
            self.issuer.forget_jobs(&[job]);
            return none;
        }
        let carried = self.factory.carried();
        Answer::ok(json!({
            "job": job,
            "issue": issue,
            "attempt": attempt,
            "timeout": self.factory.timeout(),
            "token": token,
            "settings": carried["settings"],
            "models": carried["models"],
        }))
    }

    /// The issue of a job a call made with its token may act on, or the
    /// refusal. The job's state and attempt are the store's, read now.
    fn held(&self, live: &Live, call: Call) -> Result<i64, Answer> {
        let revoked = self
            .factory
            .store(&["contractor-list"])
            .and_then(|v| v["rows"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|r| match r {
                Value::String(t) => serde_json::from_str::<Value>(t).ok(),
                v => Some(v.clone()),
            })
            .find(|r| r["name"] == live.contractor.as_str())
            .is_none_or(|r| r["revoked"] != false);
        if revoked {
            self.forget(live.job);
            return Err(Answer::no(403, "revoked"));
        }
        let job = live.job.to_string();
        let Some(record) = self
            .factory
            .store(&["job-show", "--job", &job])
            .filter(|v| v["verb"] == "job")
        else {
            return Err(Answer::no(403, "no job"));
        };
        if record["attempt"].as_i64() != Some(live.attempt)
            || record["contractor"] != live.contractor.as_str()
        {
            return Err(Answer::no(409, "stale"));
        }
        let state = record["state"].as_str().unwrap_or_default();
        let lost_silent = state == "lost" && record["lost_reason"] != "revoked";
        let ok = match call {
            Call::Result | Call::Stopped => matches!(state, "running" | "stopping") || lost_silent,
            _ => state == "running",
        };
        let issue = record["issue"].as_i64().unwrap_or_default();
        if !ok {
            return Err(Answer::no(
                403,
                if state.is_empty() { "no job" } else { state },
            ));
        }
        // The issue's current attempt: a newer offer of it replaces this one.
        let current = self
            .factory
            .store(&["job-attempts", "--issue", &issue.to_string()])
            .and_then(|v| v["given"].as_i64());
        if current.is_some_and(|given| given > live.attempt) {
            return Err(Answer::no(409, "stale"));
        }
        Ok(issue)
    }

    fn of_job(&self, call: Call, live: &Live, issue: i64, body: &Value) -> Answer {
        let job = live.job.to_string();
        match call {
            Call::CheckIn => {
                let said = self.factory.store(&["job-check-in", "--job", &job]);
                let _ = self
                    .factory
                    .store(&["contractor-seen", "--name", &live.contractor]);
                match said.as_ref().and_then(|v| v["verb"].as_str()) {
                    Some("checked-in") => Answer::ok(json!({ "checked-in": live.job })),
                    Some("stale") => Answer::no(409, "stale"),
                    _ => Answer::no(403, "refused"),
                }
            }
            Call::Token => match self.factory.cycle_token() {
                Some((token, expires)) => Answer::ok(json!({ "token": token, "expires": expires })),
                None => Answer::no(502, "no token from the factory"),
            },
            Call::Relay => {
                let Some(argv) = body["argv"].as_array().and_then(|a| {
                    a.iter()
                        .map(|w| w.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                }) else {
                    return Answer::no(400, "a relay takes argv");
                };
                let input = body["input"].as_str();
                let mut scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
                let scope = scopes
                    .entry(live.job)
                    .or_insert_with(|| Scope::new(u64::try_from(issue).unwrap_or_default()));
                let (out, err, rc) = self
                    .factory
                    .relay(&argv, input, scope)
                    .unwrap_or_else(|why| (String::new(), why, 1));
                Answer::ok(json!({ "out": out, "err": err, "rc": rc }))
            }
            Call::Result => {
                let result = body["result"].to_string();
                let exit = body["exit"].as_i64().unwrap_or(1);
                if body["result"].is_null() {
                    return Answer::no(400, "a result takes the cycle's result");
                }
                if let Err(why) = self.factory.keep(issue, result.as_bytes(), exit) {
                    return Answer::no(503, &why);
                }
                let sha = hex(&Sha256::digest(result.as_bytes()));
                let said = self
                    .factory
                    .store(&["job-finish", "--job", &job, "--sha256", &sha]);
                match said.as_ref().and_then(|v| v["verb"].as_str()) {
                    Some("finished") => {
                        self.forget(live.job);
                        Answer::ok(json!({ "finished": live.job }))
                    }
                    Some("stale") => Answer::no(409, "stale"),
                    _ => Answer::no(403, "refused"),
                }
            }
            Call::Stopped => {
                // A job handed back while it is open is released, which costs
                // nobody anything; one already lost stays lost.
                let _ = self.factory.store(&["job-release", "--job", &job]);
                self.forget(live.job);
                Answer::ok(json!({ "stopped": live.job }))
            }
            Call::Join | Call::Hello | Call::Job => Answer::no(400, "refused"),
        }
    }

    fn forget(&self, job: i64) {
        self.issuer.forget_jobs(&[job]);
        self.scopes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&job);
    }
}

/// The settings a job carries: the ones a cycle is shaped by, never a host's
/// address, path or secret. The model's endpoint and key are not here: the
/// models.json a job carries puts every provider behind this proxy.
pub const CARRIED: &[&str] = &[
    "TEHAS_REPO",
    // Required by every load, so the contractor's `tehas work` loads: a
    // number, public logins and the App's public id, none of them a secret.
    "TEHAS_CONTROL_ISSUE",
    "TEHAS_ALLOWED_LABELERS",
    "TEHAS_GITHUB_APP_ID",
    "TEHAS_BRANCH",
    "TEHAS_DRIVER",
    "TEHAS_READ_ONLY",
    "TEHAS_WORK_TIMEOUT",
    "TEHAS_WORK_STALL",
    "TEHAS_CHILD_TIMEOUT",
    "TEHAS_WORKSTREAMS",
    "TEHAS_VERIFY_TIMEOUT",
    "TEHAS_BUILD_TIMEOUT",
    "TEHAS_RUKAS_WORKAROUNDS",
    "TEHAS_OUTSOURCE",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER",
    "PI_ENSEMBLE_SUBAGENT_MODEL",
];

/// Whether a setting may leave the factory with a job: one of [`CARRIED`], or
/// a role's own model or provider.
pub fn carried(name: &str) -> bool {
    CARRIED.contains(&name)
        || tehas_core::settings::rules::ROLES.iter().any(|role| {
            name == format!("PI_ENSEMBLE_MODEL_{role}")
                || name == format!("PI_ENSEMBLE_PROVIDER_{role}")
        })
}

/// The settings of a job from the variables `get` gives: every carried name
/// that is set.
pub fn settings_of(names: &[String], get: &dyn Fn(&str) -> String) -> Value {
    let mut out = serde_json::Map::new();
    for name in names.iter().filter(|n| carried(n)) {
        let value = get(name);
        if !value.is_empty() {
            out.insert(name.clone(), Value::String(value));
        }
    }
    Value::Object(out)
}
