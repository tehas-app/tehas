//! Accounts and actors: `Realm`, `Login`, `AccountId`, `Account`, `Actor`,
//! `Scope`, `Secret`, `Token`, `Keyring`, `DeployCredential`, `TokenStore`,
//! `TokenLock`, `SiteId`, `GrantId`, `Covers`, `Kept`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::opaque;
use crate::{At, NotValid, ProjectName};

/// Which supplier an account belongs to. Two at most in an allowed setup.
/// Where tickets come from the code host, every account is `CodeHost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Realm {
    /// The code host's accounts.
    CodeHost,
    /// The tickets supplier's accounts.
    Tickets,
}

opaque!(
    /// An account name, folded by the adapter: every spelling a site has for
    /// one account becomes one `Login`, so no caller compares strings to find
    /// its own bot.
    Login
);
opaque!(
    /// The supplier's stable id of an account, which a rename does not
    /// change.
    AccountId
);

/// What kind of account it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountKind {
    /// The factory's own account.
    Ours,
    /// Another program's account.
    Bot,
    /// A person.
    Person,
    /// The site does not say.
    Unknown,
}

/// An account, with its realm and the supplier's stable id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Account {
    /// The supplier it belongs to.
    pub realm: Realm,
    /// Its name, folded.
    pub login: Login,
    /// None only where the site gives none, as for a deleted account.
    /// Left out when None, never null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<AccountId>,
    /// What kind of account it is.
    pub kind: AccountKind,
}

/// Who a handle acts as. Minting, keeping and renewing tokens is the
/// adapter's; a caller names the kind of actor, never a token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Actor {
    /// The factory's own account, narrowed.
    Ours(Scope),
    /// A signed-in person, by a key the caller holds; the adapter asks the
    /// caller's keyring for the credential when it makes the handle.
    Person(PersonKey),
    /// The operator's own credential on this machine, as their command-line
    /// tool for the site is signed in. For an operator's tools alone, never
    /// a service; a handle made from it names that account in history.
    Operator,
    /// A credential bound to one project, not to an account: a deploy key or
    /// deploy token, by a key the caller holds in its keyring. Reads files
    /// and git; no ticket, blueprint or change write. A key gives an `ssh`
    /// remote, a token an `https` one.
    Deploy(PersonKey),
    /// A short-lived token the caller hands in, by a key into its keyring:
    /// the factory's account, narrowed and minted elsewhere. Used as given
    /// and never minted, renewed or widened. Once past its `until`, or
    /// refused by the site, every call answers `SignedOut`, past `until`
    /// with no request.
    Given(PersonKey),
    /// No credential: public reads alone, on the site's anonymous budget. A
    /// write handle asked as `Anonymous` is `Forbidden`.
    Anonymous,
}

opaque!(
    /// A key into the caller's keyring. Names a person's session or a deploy
    /// credential; never the credential itself.
    PersonKey
);

/// What the factory's account is narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Scope {
    /// Everything the factory writes.
    Full,
    /// A cycle: what the agents in a cycle's container make, with its own
    /// token or through its relay. Read the project; push branches;
    /// propose, open, edit and comment on changes; file, note and edit
    /// tickets and open, comment on and edit blueprints, with text alone.
    /// No mark, close, merge, tag or release: those are the factory's.
    /// Without the product's merge grant, a cycle's token is this one.
    Cycle,
    /// A cycle's token with the product's merge grant: what `Cycle` may,
    /// and merge its own change. Still no tag or release. tehas mints it in
    /// place of `Cycle` only when the product opts in to a cycle merging
    /// its own change; the grant is off by default.
    CycleMerge,
    /// A planning turn or a dev session: a cycle's reads and change writes,
    /// and the ticket and blueprint reads planning asks for. No ticket or
    /// blueprint write.
    Session,
    /// Read tickets, blueprints and files.
    Read,
    /// Read files only (release reads, a settings pull).
    Files,
    /// An agent's token: write tickets, blueprints and changes, read
    /// contents. No file write and no push, so an agent never gets `Full`.
    Agent,
    /// A scope this copy does not know.
    #[serde(other)]
    Unknown,
}

/// A credential. No `Serialize`, no `Display`; `Debug` prints `Secret(..)`.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wraps a credential.
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The credential, for an adapter and git's askpass only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// A token and when it stops being good.
#[derive(Debug, Clone)]
pub struct Token {
    /// The credential.
    pub secret: Secret,
    /// When it stops being good.
    pub until: At,
}

/// The caller's store of person and deploy credentials, in memory. The
/// adapter asks it per handle and keeps the answer no longer than the
/// handle.
pub trait Keyring: Send + Sync {
    /// A person's credential.
    fn secret(&self, key: &PersonKey) -> Option<Secret>;
    /// A deploy credential, and which kind it is.
    fn deploy(&self, key: &PersonKey) -> Option<DeployCredential>;
    /// A token handed in for `Actor::Given`, with its `until`.
    fn given(&self, key: &PersonKey) -> Option<Token>;
    /// The site said the credential is no longer good: end that person's
    /// sessions.
    fn signed_out(&self, key: &PersonKey);
}

/// A deploy key is a private key for git over `ssh`; a deploy token is a
/// secret for `https`.
#[derive(Debug, Clone)]
pub enum DeployCredential {
    /// A private key, for `ssh`.
    Key(Secret),
    /// A token, for `https`.
    Token(Secret),
}

opaque!(
    /// One project on one supplier, as the adapter names it: the key of its
    /// tokens. Health and the budget are not keyed by it.
    SiteId
);
opaque!(
    /// One grant the factory's tokens are minted from, as the adapter names
    /// it.
    GrantId
);

/// What a kept token covers: one project, or a set of projects that one
/// grant covers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Covers {
    /// One project.
    Site(SiteId),
    /// A set of projects of one grant.
    Projects {
        /// The grant.
        grant: GrantId,
        /// Its projects.
        projects: ProjectSet,
    },
}

impl Covers {
    /// A set of projects of one grant, sorted and without repeats. An empty
    /// set is refused.
    pub fn projects(grant: GrantId, projects: Vec<ProjectName>) -> Result<Self, NotValid> {
        Ok(Self::Projects {
            grant,
            projects: ProjectSet::new(projects)?,
        })
    }
}

/// A set of projects: at least one, sorted and without repeats, so two
/// equal sets are one key. Checked when made, and again when read back.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<ProjectName>", into = "Vec<ProjectName>")]
pub struct ProjectSet(Vec<ProjectName>);

impl ProjectSet {
    /// The set, sorted and without repeats; refused when empty.
    pub fn new(mut projects: Vec<ProjectName>) -> Result<Self, NotValid> {
        if projects.is_empty() {
            return Err(NotValid {
                what: "set of projects",
            });
        }
        projects.sort();
        projects.dedup();
        Ok(Self(projects))
    }

    /// The projects, sorted.
    pub fn as_slice(&self) -> &[ProjectName] {
        &self.0
    }
}

impl TryFrom<Vec<ProjectName>> for ProjectSet {
    type Error = NotValid;
    fn try_from(projects: Vec<ProjectName>) -> Result<Self, Self::Error> {
        Self::new(projects)
    }
}

impl From<ProjectSet> for Vec<ProjectName> {
    fn from(set: ProjectSet) -> Self {
        set.0
    }
}

/// Where the caller keeps the factory's minted tokens, by what they cover
/// and their scope, so several processes share one and a cycle's container
/// can be given it. Keyed by what a token covers because one program mints
/// for several projects, and one project's token, or one set's, must never
/// overwrite another's. The adapter mints and renews; the caller decides
/// where a token lives and who reads it.
pub trait TokenStore: Send + Sync {
    /// A kept token with at least `left` seconds to run, if any.
    fn kept(&self, covers: &Covers, scope: Scope, left: u32) -> Option<Token>;
    /// Keep a new token. False if it could not be kept; the caller then gets
    /// it once, kept nowhere.
    fn keep(&self, covers: &Covers, scope: Scope, token: &Token) -> bool;
    /// The site refused a kept token: drop it, so no other process reads
    /// it again. `None` drops every scope of what it covers and, for a
    /// site, its facts. False if anything could not be dropped: the adapter
    /// raises `Health::TokenNotDropped` once per handle.
    fn forget(&self, covers: &Covers, scope: Option<Scope>) -> bool;
    /// A small fact the adapter keeps beside its tokens, so a mint need not
    /// look it up each time.
    fn fact(&self, site: &SiteId, what: Kept) -> Option<String>;
    /// Keep a fact. False if it could not be kept: the adapter uses the fact
    /// for this handle alone and looks it up again on the next mint. Not
    /// reported: a lost fact costs one call, never a wrong answer.
    fn keep_fact(&self, site: &SiteId, what: Kept, value: &str) -> bool;
    /// Hold the minting of `scope` across processes, at most `wait` seconds.
    /// `None` after the wait: the adapter answers `Unavailable { why:
    /// Timeout, sent: No }` and mints nothing.
    fn lock(&self, covers: &Covers, scope: Scope, wait: u32) -> Option<TokenLock>;
}

/// Held until dropped; what it holds is the caller's.
pub struct TokenLock(pub Box<dyn Send>);

impl fmt::Debug for TokenLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenLock(..)")
    }
}

/// The facts kept beside the tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kept {
    /// The factory's own account, as the site names it.
    Me,
    /// The grant the factory's tokens are minted from on this project.
    Grant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), "Secret(..)");
        let t = Token {
            secret: s.clone(),
            until: At::new(1),
        };
        assert!(!format!("{t:?}").contains("hunter2"));
        assert!(!format!("{:?}", DeployCredential::Token(s.clone())).contains("hunter2"));
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn a_set_of_projects_is_sorted_without_repeats() {
        let p = |s: &str| ProjectName::new(s);
        let a = Covers::projects(GrantId::new("g"), vec![p("b"), p("a"), p("b")]).unwrap();
        let b = Covers::projects(GrantId::new("g"), vec![p("a"), p("b")]).unwrap();
        assert_eq!(a, b);
        match a {
            Covers::Projects { projects, .. } => {
                assert_eq!(projects.as_slice(), [p("a"), p("b")]);
            }
            Covers::Site(_) => panic!("a set"),
        }
        let set = ProjectSet::new(vec![p("c"), p("a"), p("b")]).unwrap();
        assert_eq!(set.as_slice(), [p("a"), p("b"), p("c")]);
    }

    #[test]
    fn an_empty_set_of_projects_is_refused() {
        let refused = NotValid {
            what: "set of projects",
        };
        assert_eq!(ProjectSet::new(vec![]), Err(refused));
        assert_eq!(Covers::projects(GrantId::new("g"), vec![]), Err(refused));
    }
}
