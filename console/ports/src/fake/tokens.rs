//! `MemoryTokens`: a `TokenStore` in memory that records what it was asked
//! to keep or forget.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::state::lock;
use super::world::Clock;
use crate::{Covers, Kept, Scope, SiteId, Token, TokenLock, TokenStore};

/// One thing a `MemoryTokens` was asked to change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Asked {
    /// Keep a token of this scope under what it covers.
    Keep(Covers, Scope),
    /// Forget what it covers, of one scope or of all.
    Forget(Covers, Option<Scope>),
}

/// A `TokenStore` in memory. It records each `Covers` and scope it was
/// asked to keep or forget. Made to fail, `keep` and `forget` answer false
/// and change nothing; `lock` answers None.
#[derive(Debug)]
pub struct MemoryTokens {
    inner: Mutex<Store>,
    clock: Clock,
}

#[derive(Debug, Default)]
struct Store {
    tokens: Vec<(Covers, Scope, Token)>,
    facts: Vec<(SiteId, Kept, String)>,
    asked: Vec<Asked>,
    failing: bool,
}

impl MemoryTokens {
    /// An empty store that works, reading the time from `clock`.
    pub fn new(clock: Clock) -> Self {
        Self {
            inner: Mutex::new(Store::default()),
            clock,
        }
    }

    /// Make `keep`, `forget` and `lock` fail, or work again.
    pub fn set_failing(&self, failing: bool) {
        lock(&self.inner).failing = failing;
    }

    /// Everything it was asked to keep or forget, in order.
    pub fn asked(&self) -> Vec<Asked> {
        lock(&self.inner).asked.clone()
    }

    /// Put a token in the store as if kept earlier, without recording it.
    pub fn seed(&self, covers: Covers, scope: Scope, token: Token) {
        lock(&self.inner).tokens.push((covers, scope, token));
    }
}

impl TokenStore for MemoryTokens {
    fn kept(&self, covers: &Covers, scope: Scope, left: u32) -> Option<Token> {
        let least = self.clock.now().secs() + i64::from(left);
        let store = lock(&self.inner);
        store
            .tokens
            .iter()
            .rev()
            .find(|(c, s, t)| c == covers && *s == scope && t.until.secs() >= least)
            .map(|(_, _, t)| t.clone())
    }

    fn keep(&self, covers: &Covers, scope: Scope, token: &Token) -> bool {
        let mut store = lock(&self.inner);
        store.asked.push(Asked::Keep(covers.clone(), scope));
        if store.failing {
            return false;
        }
        store
            .tokens
            .retain(|(c, s, _)| !(c == covers && *s == scope));
        store.tokens.push((covers.clone(), scope, token.clone()));
        true
    }

    fn forget(&self, covers: &Covers, scope: Option<Scope>) -> bool {
        let mut store = lock(&self.inner);
        store.asked.push(Asked::Forget(covers.clone(), scope));
        if store.failing {
            return false;
        }
        store
            .tokens
            .retain(|(c, s, _)| !(c == covers && scope.is_none_or(|x| x == *s)));
        if scope.is_none() {
            if let Covers::Site(site) = covers {
                store.facts.retain(|(s, _, _)| s != site);
            }
        }
        true
    }

    fn fact(&self, site: &SiteId, what: Kept) -> Option<String> {
        lock(&self.inner)
            .facts
            .iter()
            .find(|(s, k, _)| s == site && *k == what)
            .map(|(_, _, v)| v.clone())
    }

    fn keep_fact(&self, site: &SiteId, what: Kept, value: &str) -> bool {
        let mut store = lock(&self.inner);
        if store.failing {
            return false;
        }
        store.facts.retain(|(s, k, _)| !(s == site && *k == what));
        store.facts.push((site.clone(), what, value.to_string()));
        true
    }

    fn lock(&self, _covers: &Covers, _scope: Scope, _wait: u32) -> Option<TokenLock> {
        if lock(&self.inner).failing {
            None
        } else {
            Some(TokenLock(Box::new(())))
        }
    }
}
