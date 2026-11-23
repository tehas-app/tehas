//! Accounts and actors: `Realm`, `Login`, `AccountId`, `Account`, `Actor`,
//! `Scope`, `Secret`, `Token`, `DeployCredential`, `SiteId`, `GrantId`,
//! `Covers`, `Kept`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::opaque;
use crate::{At, ProjectName};

// T4.1b: `Keyring`, the trait of the caller's store of person and deploy
// credentials, and `TokenStore`, the trait of where the caller keeps the
// factory's minted tokens, with `TokenLock`, the hold `TokenStore::lock`
// answers.

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
    /// A cycle: read the project and its blueprints, push branches, open
    /// changes; no ticket or blueprint write.
    Cycle,
    /// A planning turn or a dev session: what a cycle has, and the ticket
    /// and blueprint reads planning asks for.
    Session,
    /// Read tickets, blueprints and files.
    Read,
    /// Read files only (release reads, a settings pull).
    Files,
    /// An agent's token: write tickets and changes, read files. No file
    /// write and no push, so an agent never gets `Full`.
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
/// grant covers. The set is kept sorted and without repeats, so two equal
/// sets are one key: make it with `Covers::projects`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "CoversRead")]
pub enum Covers {
    /// One project.
    Site(SiteId),
    /// A set of projects of one grant.
    Projects {
        /// The grant.
        grant: GrantId,
        /// Its projects, sorted, without repeats.
        projects: Vec<ProjectName>,
    },
}

impl Covers {
    /// A set of projects of one grant, sorted and without repeats.
    pub fn projects(grant: GrantId, mut projects: Vec<ProjectName>) -> Self {
        projects.sort();
        projects.dedup();
        Self::Projects { grant, projects }
    }
}

/// `Covers` as written, sorted when read.
#[derive(Deserialize)]
enum CoversRead {
    Site(SiteId),
    Projects {
        grant: GrantId,
        projects: Vec<ProjectName>,
    },
}

impl From<CoversRead> for Covers {
    fn from(read: CoversRead) -> Self {
        match read {
            CoversRead::Site(site) => Covers::Site(site),
            CoversRead::Projects { grant, projects } => Covers::projects(grant, projects),
        }
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
        let a = Covers::projects(GrantId::new("g"), vec![p("b"), p("a"), p("b")]);
        let b = Covers::projects(GrantId::new("g"), vec![p("a"), p("b")]);
        assert_eq!(a, b);
        match a {
            Covers::Projects { projects, .. } => assert_eq!(projects, vec![p("a"), p("b")]),
            Covers::Site(_) => panic!("a set"),
        }
    }
}
