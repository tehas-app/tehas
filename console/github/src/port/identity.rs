//! The identity port, `Identity`: who the handle acts as (`me`), an
//! account's standing on the repository (`standing`), the account a login
//! names (`account_by_login`), whether a name is an organisation
//! (`is_organisation`), whether an account may be assigned (`assignable`),
//! and a person's sign-in (`sign_in_link`, `signed_in`, in `people.rs`).
//! The rest answers `Refused(Unsupported)` before any request: `account`
//! comes with the move that asks it, and tokens are the wiring's, made and
//! reviewed as security changes of their own.
//!
//! A bot is looked up by its whole login, `<slug>[bot]`, and known by its
//! id: the bare slug is another account's login, an organisation's (the
//! App `tehas-app` and the organisation `tehas-app` are two accounts) or a
//! person's.
//!
//! A login is folded as `tehas_core::accounts` keys it: REST names a GitHub
//! App's account `<slug>[bot]` and GraphQL `<slug>`, one account with one
//! id, so the `Login` here is the GraphQL form, ASCII case folded. Accounts
//! are compared by id; a login is a name to show and to vouch.

use serde_json::Value;

use super::unsupported;
use crate::forge::uri;
use crate::graphql::priced;
use crate::{rate, GitHub};
use tehas_core::accounts;
use tehas_ports::{
    Account, AccountId, AccountKind, Got, Identity, Login, Outcome, Pool, ProjectName, Realm,
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
        let ours = self.bot.as_ref().is_some_and(|b| b.is(&facts));
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
    /// App answers `None` too, through a handle the site made as much as
    /// one made with `GitHub::new`: whether it should answer otherwise is
    /// the tehas lead's to rule.
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

    /// GitHub's account of a numeric id: no program of tehas asks one,
    /// which asks by login asks [`Identity::account_by_login`]. The fake
    /// answers it and the laws drive it there; the adapter refuses it
    /// before any request, as the test holds. The port method stays:
    /// nothing leaves a port after the freeze of 2026-11-30.
    fn account(&mut self, _: &AccountId) -> Got<Account> {
        unsupported()
    }

    /// REST's account of the login as it is given: a bot by its whole
    /// `<slug>[bot]` (gh's `app/<slug>` is the same bot), never by the bare
    /// slug, which is another account's login, an organisation's or a
    /// person's, or nobody's. The answer is held to the login asked and read
    /// by its id. An organisation is no account here: `NotFound`, as for a
    /// login no account has. A login GitHub never gives is
    /// `Refused(Invalid)`, with no request.
    fn account_by_login(&mut self, login: &Login) -> Got<Account> {
        let (holder, said, id) = self.by_login(login)?;
        let kind = match holder {
            Holder::User(_) => AccountKind::Person,
            Holder::Bot => AccountKind::Bot,
            Holder::Organisation => return Err(Outcome::NotFound),
        };
        let facts = crate::gh::Facts {
            id: Some(id),
            kind: Some(
                if kind == AccountKind::Bot {
                    "Bot"
                } else {
                    "User"
                }
                .to_string(),
            ),
        };
        let ours = self.bot.as_ref().is_some_and(|b| b.is(&facts));
        let kind = if ours { AccountKind::Ours } else { kind };
        let login = Login::new(accounts::folded(accounts::account(&said)));
        let mut account = Account::new(Realm::CodeHost, login, kind);
        account.id = Some(AccountId::new(id.to_string()));
        Ok(account)
    }

    /// Whether REST's account of the name, read as `account_by_login`
    /// reads it, is an organisation. A bot or a person is not; `NotFound`
    /// when no account has the name.
    fn is_organisation(&mut self, name: &Login) -> Got<bool> {
        let (holder, _, _) = self.by_login(name)?;
        Ok(holder == Holder::Organisation)
    }

    /// The account read of the id, then GitHub's assignee check of the
    /// login it has now: `true` when GitHub answers it may be assigned on
    /// the repository. A bot and an organisation are never assignees, from
    /// the account read alone. A check that finds nothing is read once more
    /// under the login the id has then: the same login is `false`, another
    /// is asked once more, then `Unread(Inconsistent)`.
    fn assignable(&mut self, who: &AccountId) -> Got<bool> {
        let id = id_of(who)?;
        let mut last = None;
        for _ in 0..3 {
            let login = match self.login_of(id)? {
                Some(login) => login,
                None => return Ok(false),
            };
            if last
                .as_ref()
                .is_some_and(|l: &String| accounts::folded(l) == accounts::folded(&login))
            {
                return Ok(false);
            }
            let path = format!("repos/{}/assignees/{}", self.repo, uri(&login));
            match self.call("whether the account may be assigned", &["api", &path]) {
                Ok(_) => return Ok(true),
                Err(e) => match self.outcome(&e) {
                    Outcome::NotFound => last = Some(login),
                    o => return Err(o),
                },
            }
        }
        Err(Outcome::Unread(Unread::Inconsistent))
    }

    /// The minting of a token is the outsource, a building feature: the
    /// fake models a mint and the laws drive it there, while the adapter
    /// mints and keeps no token yet and refuses before any request, as
    /// the test holds. The port methods stay for it: nothing leaves a
    /// port after the freeze of 2026-11-30.
    fn token(&mut self, _: Scope) -> Got<Token> {
        unsupported()
    }

    /// See [`Identity::token`].
    fn token_over(&mut self, _: Scope, _: &[ProjectName]) -> Got<Token> {
        unsupported()
    }

    /// See [`super::people`]: GitHub's page for the App's client for
    /// people, `Refused(Unsupported)` from a site given none.
    fn sign_in_link(&self, state: &SignInState) -> Got<WebLink> {
        self.sign_in_page(state)
    }

    fn signed_in(&mut self, code: &SignInCode) -> Got<(Account, Secret)> {
        self.person_of(code)
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
            // An App holds no collaborator permission, the factory's own
            // included (see `standing`).
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

impl GitHub {
    /// REST's account of `login` as given: what it is, its login as GitHub
    /// answers it and its id. `app/<slug>` asks `<slug>[bot]`.
    fn by_login(&mut self, login: &Login) -> Got<(Holder, String, u64)> {
        let asked = rest_login(login.as_str())?;
        let path = format!("users/{}", uri(&asked));
        let out = self
            .call("the account of a login", &["api", &path])
            .map_err(|e| self.outcome(&e))?;
        read_login(&out, &asked)
    }

    /// The login the account of `id` has now; None for a bot or an
    /// organisation, which hold no assignment.
    pub(super) fn login_of(&mut self, id: u64) -> Got<Option<String>> {
        let user = format!("user/{id}");
        let out = self
            .call("the account", &["api", &user])
            .map_err(|e| self.outcome(&e))?;
        Ok(match read_user(&out, id)? {
            Holder::User(login) => Some(login),
            Holder::Bot | Holder::Organisation => None,
        })
    }
}

/// The login REST is asked for: a person's or an organisation's login, or a
/// bot's `<slug>[bot]`, whole; gh's `app/<slug>` is that bot. Anything else
/// is `Refused(Invalid)`.
fn rest_login(given: &str) -> Got<String> {
    let invalid = Outcome::Refused(Refusal::Invalid);
    let is_login = tehas_core::settings::rules::is_login;
    if let Some(slug) = given.strip_prefix("app/") {
        return (!slug.is_empty() && is_login(slug))
            .then(|| format!("{slug}[bot]"))
            .ok_or(invalid);
    }
    let bare = given.strip_suffix("[bot]").unwrap_or(given);
    (!bare.is_empty() && bare.len() <= 100 && is_login(bare))
        .then(|| given.to_string())
        .ok_or(invalid)
}

/// The account answer of a login: what it is, the login it gives and its
/// id. An answer of another login is `Unread(Inconsistent)`: the bare slug
/// and the bot are two accounts, and one must never stand for the other.
fn read_login(out: &str, asked: &str) -> Got<(Holder, String, u64)> {
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    let id = v
        .get("id")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or(MALFORMED)?;
    let said = v
        .get("login")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .ok_or(MALFORMED)?;
    if accounts::folded(said) != accounts::folded(asked) {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let holder = read_user(out, id)?;
    let bot = said.ends_with("[bot]");
    if (holder == Holder::Bot) != bot {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok((holder, said.to_string(), id))
}

/// GitHub's id in an `AccountId`, or `Refused(Invalid)` for one GitHub
/// never gives.
pub(super) fn id_of(who: &AccountId) -> Got<u64> {
    let s = who.as_str();
    match s.parse::<u64>() {
        Ok(id) if id > 0 && !s.starts_with(['0', '+']) => Ok(id),
        _ => Err(Outcome::Refused(Refusal::Invalid)),
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
                .any(|e| e.get("type").and_then(Value::as_str) == Some(rate::RATE_LIMITED))
        });
        let other = Outcome::Unread(Unread::Inconsistent);
        return Err(rate::limited(limited, Pool::Priced, now, None, other));
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
/// A base role is its own rank, held to what the legacy word allows: when
/// the two disagree the lower reading wins, and a word not of the four is
/// no answer (see `word_cap`). The legacy word
/// is one of GitHub's four, which folds maintain into `write` and triage
/// into `read`, so read alone it never answers `Maintain` or `Triage`. Any
/// other `role_name`, a custom role or an empty one, is never
/// above `Read`, since the dispatcher lets no such account write: it is
/// `Read` when the word shows any access, `None` when it says none, and no
/// answer without a word or with a word not of the four.
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
    let role = match role {
        "admin" => Standing::Admin,
        "maintain" => Standing::Maintain,
        "write" => Standing::Write,
        "triage" => Standing::Triage,
        "read" => Standing::Read,
        "none" => Standing::None,
        _ => {
            return match word {
                Some("admin" | "write" | "read") => Ok(Standing::Read),
                Some("none") => Ok(Standing::None),
                _ => Err(MALFORMED),
            }
        }
    };
    let cap = match v.get("permission") {
        None | Some(Value::Null) => Standing::Admin,
        Some(w) => word_cap(w.as_str()).ok_or(MALFORMED)?,
    };
    Ok(role.min(cap))
}

/// The highest base role a legacy word agrees with. GitHub's own answers
/// fold the role into the word, so a role above this cap disagrees with
/// its word and is read at the cap: the lower of the two readings. None
/// for any other word than the four: the role must not stand beside a word
/// not understood. A missing or null word caps nothing.
fn word_cap(word: Option<&str>) -> Option<Standing> {
    match word? {
        "admin" => Some(Standing::Admin),
        "write" => Some(Standing::Maintain),
        "read" => Some(Standing::Triage),
        "none" => Some(Standing::None),
        _ => None,
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
        use tehas_ports::{At, Pool};
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
