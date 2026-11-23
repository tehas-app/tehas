//! The identity port, `Identity`: who the handle acts as (`me`) and an
//! account's standing on the repository (`standing`). The rest answers
//! `Refused(Unsupported)` before any request: the account reads come with
//! the moves that ask them, and tokens and sign-in are the wiring's, made
//! and reviewed as security changes of their own.
//!
//! A login is folded as `tehas_core::accounts` keys it: REST names a GitHub
//! App's account `<slug>[bot]` and GraphQL `<slug>`, one account with one
//! id, so the `Login` here is the GraphQL form, ASCII case folded. Accounts
//! are compared by id; a login is a name to show and to vouch.

use serde_json::Value;

use super::unsupported;
use crate::forge::uri;
use crate::GitHub;
use tehas_core::accounts;
use tehas_ports::{
    Account, AccountId, AccountKind, At, Got, Identity, Login, Outcome, Pool, ProjectName, Realm,
    Refusal, Scope, Secret, SignInCode, SignInState, Standing, Token, Unread, WebLink,
};

/// The question of who the token is. GraphQL's `viewer` answers for a
/// person's token and for an App's installation token alike; REST's `user`
/// refuses the second.
const VIEWER: &str = "query { viewer { login databaseId } }";

impl Identity for GitHub {
    /// GraphQL's `viewer`, one priced call, then the account read of the id
    /// it names for the kind. GraphQL types every viewer a user and spells
    /// an App's login more than one way, so the kind is the account read's
    /// type: `Bot` or `Person`, `Unknown` for any other. A handle made by
    /// the site answers `Ours` where the account is the site's factory
    /// account.
    fn me(&mut self) -> Got<Account> {
        let query = format!("query={VIEWER}");
        let out = self
            .call("who the handle acts as", &["api", "graphql", "-f", &query])
            .map_err(|e| priced(self.outcome(&e)))?;
        let (login, id) = read_me(&out, (self.now)())?;
        let user = format!("user/{id}");
        let out = self
            .call("the account", &["api", &user])
            .map_err(|e| self.outcome(&e))?;
        let (kind, said) = match read_user(&out, id)? {
            Holder::User(_) => (AccountKind::Person, "User"),
            Holder::Bot => (AccountKind::Bot, "Bot"),
            Holder::Organisation => (AccountKind::Unknown, "Organization"),
        };
        let facts = crate::gh::Facts {
            id: Some(id),
            kind: Some(said.to_string()),
        };
        let ours = self
            .bot
            .as_ref()
            .is_some_and(|b| b.is(login.as_str(), &facts));
        let kind = if ours { AccountKind::Ours } else { kind };
        let mut account = Account::new(Realm::CodeHost, login, kind);
        account.id = Some(AccountId::new(id.to_string()));
        Ok(account)
    }

    /// GitHub's account by its id, then its permission on the repository by
    /// the login that id has now. `NotFound` when no account has the id, or
    /// when the permission read finds nothing twice under the same login (a
    /// repository the token cannot see); an account with no access answers
    /// `Standing::None` (on a public repository GitHub answers every account
    /// `Read`). The second answer must name the id asked. A login renamed
    /// between the two reads, to a free login or a taken one, is read once
    /// more, then `Unread(Inconsistent)`.
    ///
    /// An App's bot account answers `Standing::None` from the account read
    /// alone: an App is no collaborator, so it holds no collaborator
    /// permission, and its access is its installation's. The factory's own
    /// App cannot be told apart yet, since `GitHub::new` carries no actor, so
    /// until the handle factory gives the adapter its actor every bot
    /// answers `None`; the factory's own App will then answer from `Ours`.
    fn standing(&mut self, who: &AccountId) -> Got<Standing> {
        let id = id_of(who)?;
        match self.standing_once(id) {
            Pass::Answer(Err(Outcome::Unread(Unread::Inconsistent))) => {
                match self.standing_once(id) {
                    Pass::Answer(answer) => answer,
                    Pass::Unfound(_) => Err(Outcome::Unread(Unread::Inconsistent)),
                }
            }
            Pass::Answer(answer) => answer,
            Pass::Unfound(login) => match self.standing_once(id) {
                Pass::Answer(answer) => answer,
                Pass::Unfound(again) if accounts::folded(&again) == accounts::folded(&login) => {
                    Err(Outcome::NotFound)
                }
                Pass::Unfound(_) => Err(Outcome::Unread(Unread::Inconsistent)),
            },
        }
    }

    fn account(&mut self, _: &AccountId) -> Got<Account> {
        unsupported()
    }

    fn account_by_login(&mut self, _: &Login) -> Got<Account> {
        unsupported()
    }

    fn is_organisation(&mut self, _: &Login) -> Got<bool> {
        unsupported()
    }

    fn assignable(&mut self, _: &AccountId) -> Got<bool> {
        unsupported()
    }

    fn token(&mut self, _: Scope) -> Got<Token> {
        unsupported()
    }

    fn token_over(&mut self, _: Scope, _: &[ProjectName]) -> Got<Token> {
        unsupported()
    }

    /// GitHub's sign-in page, without `state`: the adapter holds no OAuth
    /// client, so no link it makes finishes a sign-in. No caller asks it.
    fn sign_in_link(&self, _: &SignInState) -> WebLink {
        WebLink::new("https://github.com/login").expect("a web link")
    }

    fn signed_in(&mut self, _: &SignInCode) -> Got<(Account, Secret)> {
        unsupported()
    }
}

/// What one pass of `standing` found.
enum Pass {
    Answer(Got<Standing>),
    /// The account was read, and the permission read under this login found
    /// nothing: the login may have been renamed between the two reads.
    Unfound(String),
}

impl GitHub {
    /// One pass of `standing`: the account read, then the permission read.
    fn standing_once(&mut self, id: u64) -> Pass {
        let user = format!("user/{id}");
        let out = match self.call("the account", &["api", &user]) {
            Ok(out) => out,
            Err(e) => return Pass::Answer(Err(self.outcome(&e))),
        };
        let login = match read_user(&out, id) {
            Ok(Holder::User(login)) => login,
            // An organisation holds no standing of its own on a repository.
            Ok(Holder::Organisation) => return Pass::Answer(Ok(Standing::None)),
            // Another App holds no collaborator permission. The factory's
            // own is not told apart until the adapter carries its actor.
            Ok(Holder::Bot) => return Pass::Answer(Ok(Standing::None)),
            Err(o) => return Pass::Answer(Err(o)),
        };
        let path = format!(
            "repos/{}/collaborators/{}/permission",
            self.repo,
            uri(&login)
        );
        match self.call("the account's permission", &["api", &path]) {
            Ok(out) => Pass::Answer(read_permission(&out, id)),
            Err(e) => match self.outcome(&e) {
                Outcome::NotFound => Pass::Unfound(login),
                o => Pass::Answer(Err(o)),
            },
        }
    }
}

/// GitHub's id in an `AccountId`, or `Refused(Invalid)` for one GitHub
/// never gives.
fn id_of(who: &AccountId) -> Got<u64> {
    let s = who.as_str();
    match s.parse::<u64>() {
        Ok(id) if id > 0 && !s.starts_with(['0', '+']) => Ok(id),
        _ => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

/// A spent budget of a GraphQL call is the priced pool's.
fn priced(o: Outcome) -> Outcome {
    match o {
        Outcome::Spent { until, .. } => Outcome::Spent {
            pool: Pool::Priced,
            until,
        },
        o => o,
    }
}

/// An answer that does not parse.
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

/// The vouched login and the id of the viewer answer. An answer that
/// carries errors is no whole viewer: a spent budget when GitHub says the
/// rate is limited, `Unread(Inconsistent)` for any other.
fn read_me(out: &str, now: i64) -> Got<(Login, u64)> {
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    if let Some(errors) = v.get("errors").filter(|e| !e.is_null()) {
        let limited = errors.as_array().is_some_and(|all| {
            all.iter()
                .any(|e| e.get("type").and_then(Value::as_str) == Some("RATE_LIMITED"))
        });
        return Err(if limited {
            Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(now),
            }
        } else {
            Outcome::Unread(Unread::Inconsistent)
        });
    }
    let viewer = v
        .pointer_mut("/data/viewer")
        .and_then(Value::as_object_mut)
        .ok_or(MALFORMED)?;
    let raw = viewer
        .get("login")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .ok_or(MALFORMED)?
        .to_string();
    let id = viewer
        .get("databaseId")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or(MALFORMED)?;
    // The vouch keys a login as `accounts::account` gives it; it is given
    // that form first, so a renamed answer keeps the same spelling.
    viewer.insert(
        "login".into(),
        Value::String(accounts::account(&raw).to_string()),
    );
    accounts::vouch(&mut v);
    let login = v
        .pointer("/data/viewer/login")
        .and_then(Value::as_str)
        .ok_or(MALFORMED)?;
    Ok((Login::new(accounts::folded(login)), id))
}

/// What an account read says of the account an id names.
#[derive(Debug, PartialEq)]
enum Holder {
    /// A user, by the login it has now.
    User(String),
    Organisation,
    Bot,
}

/// The account read of `id`. The login is a key for the next read alone and
/// is never compared, so it is not vouched: the permission answer is held
/// to the id instead.
fn read_user(out: &str, id: u64) -> Got<Holder> {
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    let said = v.get("id").and_then(Value::as_u64).ok_or(MALFORMED)?;
    if said != id {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let login = v
        .get("login")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .ok_or(MALFORMED)?;
    Ok(match v.get("type").and_then(Value::as_str) {
        Some("User") => Holder::User(login.to_string()),
        Some("Organization") => Holder::Organisation,
        Some("Bot") => Holder::Bot,
        _ => return Err(MALFORMED),
    })
}

/// The standing of the permission answer, held to `id`, by the rule of the
/// dispatcher's `may_write`: `role_name` first, the legacy `permission` word
/// only when `role_name` is missing or null.
///
/// A base role is its own rank. The legacy word is one of GitHub's four,
/// which folds maintain into `write` and triage into `read`, so read alone
/// it never answers `Maintain` or `Triage`. Any other `role_name`, a custom role or an empty one, is never
/// above `Read`, since the dispatcher lets no such account write: it is
/// `Read` when the word shows any access, `None` when it says none, and no
/// answer to read without a word.
fn read_permission(out: &str, id: u64) -> Got<Standing> {
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    let said = v
        .pointer("/user/id")
        .and_then(Value::as_u64)
        .ok_or(MALFORMED)?;
    if said != id {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let word = v.get("permission").and_then(Value::as_str);
    let role = match v.get("role_name") {
        None | Some(Value::Null) => {
            return match word {
                Some("admin") => Ok(Standing::Admin),
                Some("write") => Ok(Standing::Write),
                Some("read") => Ok(Standing::Read),
                Some("none") => Ok(Standing::None),
                _ => Err(MALFORMED),
            }
        }
        Some(Value::String(role)) => role.as_str(),
        Some(_) => return Err(MALFORMED),
    };
    match role {
        "admin" => Ok(Standing::Admin),
        "maintain" => Ok(Standing::Maintain),
        "write" => Ok(Standing::Write),
        "triage" => Ok(Standing::Triage),
        "read" => Ok(Standing::Read),
        "none" => Ok(Standing::None),
        _ => match word {
            Some("admin" | "maintain" | "write" | "triage" | "read") => Ok(Standing::Read),
            Some("none") => Ok(Standing::None),
            _ => Err(MALFORMED),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_githubs_whole_number_alone() {
        for bad in [
            "",
            "0",
            "007",
            "+7",
            "-7",
            "7a",
            "alice",
            "18446744073709551616",
        ] {
            assert_eq!(
                id_of(&AccountId::new(bad)),
                Err(Outcome::Refused(Refusal::Invalid)),
                "{bad}"
            );
        }
        assert_eq!(id_of(&AccountId::new("101")), Ok(101));
    }

    #[test]
    fn a_spent_graphql_call_is_the_priced_pools() {
        let at = At::new(5);
        assert_eq!(
            priced(Outcome::Spent {
                pool: Pool::Calls,
                until: at
            }),
            Outcome::Spent {
                pool: Pool::Priced,
                until: at
            }
        );
        assert_eq!(priced(Outcome::Forbidden), Outcome::Forbidden);
    }
}
