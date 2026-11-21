//! The gate's memory of who is signed in, and of the sign-ins under way
//! (docs/specs/2026-11-21-plugins-web.md, "The sign-in" and "The session").
//! Nothing here is written to disk: a restart signs everyone out. No token is
//! kept.
//!
//! A sign-in crosses three tables. A plugin's name keeps an [`Ask`] under the
//! sha256 of a nonce it set in the browser. `plugins.<domain>` keeps a
//! [`Flow`] for GitHub's web flow, under its state, with a nonce of its own.
//! When that name has a session it makes a [`Code`] for the ask: one use, 60
//! seconds, for that plugin's name and the nonce's hash only.

use super::github::Person;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;

pub const SESSION: &str = "__Host-tehas-plugin";
/// The nonce of an ask, on a plugin's name.
pub const ASK: &str = "__Host-tehas-plugin-ask";
/// The nonce of GitHub's web flow, on `plugins.<domain>`.
pub const SIGNIN: &str = "__Host-tehas-plugin-signin";

pub const IDLE: u64 = 2 * 3600;
pub const TOTAL: u64 = 12 * 3600;
pub const CHECK: u64 = 10 * 60;
/// How long a sign-in may take, from the ask to GitHub's answer.
pub const FLOW: u64 = 10 * 60;
pub const CODE: u64 = 60;
/// The most of each table. A full table drops its oldest.
pub const MOST: usize = 1024;
/// The most sessions at once.
pub const SESSIONS_MOST: usize = 8192;

/// Where a session or an ask belongs: a plugin's name, by the plugin, or
/// `plugins.<domain>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Site {
    Plugin(String),
    SignIn,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub site: Site,
    pub person: Person,
    pub started: u64,
    pub used: u64,
    pub checked: u64,
}

#[derive(Debug, Clone)]
pub struct Ask {
    pub plugin: String,
    pub at: u64,
}

#[derive(Debug, Clone)]
pub struct Flow {
    pub nonce: String,
    /// The ask it signs in for, and the path to go back to.
    pub ask: String,
    pub back: String,
    pub at: u64,
}

#[derive(Debug, Clone)]
pub struct Code {
    pub plugin: String,
    pub ask: String,
    pub back: String,
    pub person: Person,
    pub checked: u64,
    pub at: u64,
}

#[derive(Default)]
pub struct Memory {
    pub sessions: HashMap<String, Session>,
    pub asks: HashMap<String, Ask>,
    pub flows: HashMap<String, Flow>,
    pub codes: HashMap<String, Code>,
    /// The time of each plugin's last press.
    pub presses: HashMap<String, u64>,
}

/// 32 random bytes as hex.
pub fn random() -> Option<String> {
    let mut b = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .ok()?
        .read_exact(&mut b)
        .ok()?;
    Some(hex(&b))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn sha256(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

/// Whether `s` is one of [`random`]'s or [`sha256`]'s values.
pub fn is_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The value of the cookie `name` in the request's Cookie headers. None
/// when it is not there, is there twice, or is not one of the gate's ids.
pub fn cookie(headers: &[(String, String)], name: &str) -> Option<String> {
    let mut found = headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("Cookie"))
        .flat_map(|(_, v)| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .filter(|(k, _)| *k == name)
        .map(|(_, v)| v.trim());
    let v = found.next()?;
    (found.next().is_none() && is_id(v)).then(|| v.to_string())
}

/// A `Set-Cookie` value of the gate: host only, https only, out of reach of
/// scripts, sent on a top-level navigation from another site.
pub fn set_cookie(name: &str, value: &str, max_age: Option<u64>) -> String {
    let age = max_age.map_or(String::new(), |a| format!("; Max-Age={a}"));
    format!("{name}={value}; Path=/; Secure; HttpOnly; SameSite=Lax{age}")
}

pub fn clear_cookie(name: &str) -> String {
    set_cookie(name, "", Some(0))
}

fn keep<V>(map: &mut HashMap<String, V>, most: usize, at: impl Fn(&V) -> u64) {
    while map.len() >= most {
        let Some(oldest) = map
            .iter()
            .min_by_key(|(_, v)| at(v))
            .map(|(k, _)| k.clone())
        else {
            return;
        };
        map.remove(&oldest);
    }
}

impl Memory {
    /// The session `id` on `site`, if it is live, marked used at `now`.
    pub fn session(&mut self, id: &str, site: &Site, now: u64) -> Option<Session> {
        let s = self.sessions.get_mut(id)?;
        if &s.site != site {
            return None;
        }
        if now.saturating_sub(s.used) > IDLE || now.saturating_sub(s.started) > TOTAL {
            self.sessions.remove(id);
            return None;
        }
        s.used = now;
        Some(s.clone())
    }

    pub fn open(&mut self, site: Site, person: Person, checked: u64, now: u64) -> Option<String> {
        self.sessions.retain(|_, s| {
            now.saturating_sub(s.used) <= IDLE && now.saturating_sub(s.started) <= TOTAL
        });
        keep(&mut self.sessions, SESSIONS_MOST, |s| s.used);
        let id = random()?;
        self.sessions.insert(
            id.clone(),
            Session {
                site,
                person,
                started: now,
                used: now,
                checked,
            },
        );
        Some(id)
    }

    /// Marks every session of the user checked at `now`.
    pub fn checked(&mut self, user: u64, now: u64) {
        for s in self.sessions.values_mut().filter(|s| s.person.id == user) {
            s.checked = now;
        }
    }

    /// Ends every session of the user, on every name.
    pub fn end(&mut self, user: u64) {
        self.sessions.retain(|_, s| s.person.id != user);
        self.codes.retain(|_, c| c.person.id != user);
    }

    /// A new ask of `plugin`: the nonce for its cookie. The ask is kept
    /// under the nonce's hash, the state sent on.
    pub fn ask(&mut self, plugin: &str, now: u64) -> Option<String> {
        self.asks.retain(|_, a| now.saturating_sub(a.at) <= FLOW);
        keep(&mut self.asks, MOST, |a| a.at);
        let nonce = random()?;
        self.asks.insert(
            sha256(&nonce),
            Ask {
                plugin: plugin.to_string(),
                at: now,
            },
        );
        Some(nonce)
    }

    /// The live ask of `state`.
    pub fn asked(&self, state: &str, now: u64) -> Option<Ask> {
        self.asks
            .get(state)
            .filter(|a| now.saturating_sub(a.at) <= FLOW)
            .cloned()
    }

    /// A new flow for the ask `state`: GitHub's state and the nonce for the
    /// cookie.
    pub fn flow(&mut self, ask: &str, back: &str, now: u64) -> Option<(String, String)> {
        self.flows.retain(|_, f| now.saturating_sub(f.at) <= FLOW);
        keep(&mut self.flows, MOST, |f| f.at);
        let state = random()?;
        let nonce = random()?;
        self.flows.insert(
            state.clone(),
            Flow {
                nonce: nonce.clone(),
                ask: ask.to_string(),
                back: back.to_string(),
                at: now,
            },
        );
        Some((state, nonce))
    }

    /// Takes the flow of `state` if the browser holds its nonce and it is
    /// in time. Used up either way.
    pub fn finish(&mut self, state: &str, nonce: Option<&str>, now: u64) -> Option<Flow> {
        let f = self.flows.remove(state)?;
        (Some(f.nonce.as_str()) == nonce && now.saturating_sub(f.at) <= FLOW).then_some(f)
    }

    /// The code for the ask `state`, which is used up. None when the ask is
    /// not live.
    pub fn code(
        &mut self,
        state: &str,
        back: &str,
        s: &Session,
        now: u64,
    ) -> Option<(String, String)> {
        let ask = self.asked(state, now)?;
        self.asks.remove(state);
        self.codes.retain(|_, c| now.saturating_sub(c.at) <= CODE);
        keep(&mut self.codes, MOST, |c| c.at);
        let code = random()?;
        self.codes.insert(
            code.clone(),
            Code {
                plugin: ask.plugin.clone(),
                ask: state.to_string(),
                back: back.to_string(),
                person: s.person.clone(),
                checked: s.checked,
                at: now,
            },
        );
        Some((code, ask.plugin))
    }

    /// Takes `code` on `plugin`'s name, in the browser whose ask nonce is
    /// `nonce`. Used up either way.
    pub fn take(
        &mut self,
        code: &str,
        plugin: &str,
        nonce: Option<&str>,
        now: u64,
    ) -> Option<Code> {
        let c = self.codes.remove(code)?;
        let ok = c.plugin == plugin
            && nonce.map(sha256).as_deref() == Some(c.ask.as_str())
            && now.saturating_sub(c.at) <= CODE;
        ok.then_some(c)
    }
}
