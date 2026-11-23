//! Identity, of every realm: who a handle acts as, an account by id or by
//! login, its standing, whether it can be assigned, tokens, and a person's
//! sign-in. One trait, `Identity`, and no writes.

use serde::{Deserialize, Serialize};

use crate::ids::opaque;
use crate::{Account, AccountId, Got, Login, ProjectName, Reports, Scope, Secret, Token, WebLink};

/// What an account may do on a project, least first, so a caller compares
/// levels: `None < Read < Triage < Write < Maintain < Admin`. A site
/// without the triage or the maintain level folds triage into `Read` and
/// maintain into `Write`, so a level never reads higher than the site
/// grants. On the wire each is its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Standing {
    /// Nothing.
    None,
    /// Read only.
    Read,
    /// Read, and sort tickets and changes: marks, assignees, closing. No
    /// write to the code.
    Triage,
    /// Write.
    Write,
    /// Write, and manage the project short of its settings that matter
    /// most.
    Maintain,
    /// Everything, settings included.
    Admin,
}

opaque!(
    /// What a sign-in link carries, for the caller to match when the person
    /// comes back.
    SignInState
);
opaque!(
    /// What a finished sign-in hands back, for `Identity::signed_in`.
    SignInCode
);

/// Identity, for the realm of the site that made the handle.
pub trait Identity: Reports {
    /// Who this handle acts as.
    fn me(&mut self) -> Got<Account>;
    /// What an account of this realm may do on this project. The rule that
    /// reads it is the caller's; the port never says whether an act counts.
    fn standing(&mut self, who: &AccountId) -> Got<Standing>;
    /// The account behind an id, as the site names it now.
    fn account(&mut self, who: &AccountId) -> Got<Account>;
    /// The account a login names now, with its stable id: how a setting
    /// that gives logins becomes the ids authorisation reads. `NotFound`
    /// when no account has that login.
    fn account_by_login(&mut self, login: &Login) -> Got<Account>;
    /// Whether a name is an organisation (a group, on another site), not a
    /// person: setup and an anonymous gate ask it.
    fn is_organisation(&mut self, name: &Login) -> Got<bool>;
    /// Whether an account can be assigned tickets on this project.
    fn assignable(&mut self, who: &AccountId) -> Got<bool>;
    /// A token of the factory's account, narrowed. Minted and renewed by
    /// the adapter, kept by the caller's `TokenStore`; at least ten minutes
    /// left.
    fn token(&mut self, scope: Scope) -> Got<Token>;
    /// One token over a set of projects that one grant covers, this
    /// handle's project among them, kept under `Covers::Projects`. It
    /// covers no project outside the set. A project the grant does not
    /// cover is `Forbidden` before any mint, never a wider set or scope.
    fn token_over(&mut self, scope: Scope, projects: &[ProjectName]) -> Got<Token>;
    /// The link a person opens to sign in, carrying `state`.
    fn sign_in_link(&self, state: &SignInState) -> WebLink;
    /// The person behind a finished sign-in, and their credential, for the
    /// caller's keyring.
    fn signed_in(&mut self, code: &SignInCode) -> Got<(Account, Secret)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_standing_is_ordered_least_first() {
        let all = [
            Standing::None,
            Standing::Read,
            Standing::Triage,
            Standing::Write,
            Standing::Maintain,
            Standing::Admin,
        ];
        assert!(all.windows(2).all(|w| w[0] < w[1]));
        assert!(Standing::Triage < Standing::Write);
        assert!(Standing::Maintain >= Standing::Write);
    }
}
