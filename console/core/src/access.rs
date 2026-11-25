//! Who has access to the factory's repository, one check for the plugins'
//! gate and for a call of a tool with `--login`
//! (docs/specs/2026-11-21-plugins-web.md, "Who gets in"). What is asked of
//! the code host is behind [`GitHub`], which the adapter answers
//! (`tehas_github::Access`) and the tests stub: the person's token for the
//! web flow's code, who the token is, and the person's permission on the
//! repository with the installation token. [`check`] compares the answer
//! with `TEHAS_PLUGIN_ACCESS` and the person by their [`AccountId`], whole
//! and exact. Nothing of it is kept here: the gate keeps a session's last
//! check in its memory, which a restart empties.

use std::path::Path;
use std::process::{Command, Stdio};
pub use tehas_ports::AccountId;
use tehas_ports::{Got, Outcome, Secret};

/// A permission on a repository, least first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Read,
    Triage,
    Write,
    Maintain,
    Admin,
}

impl Level {
    /// The word of TEHAS_PLUGIN_ACCESS.
    pub fn of_setting(word: &str) -> Option<Level> {
        Some(match word {
            "read" => Level::Read,
            "triage" => Level::Triage,
            "write" => Level::Write,
            "maintain" => Level::Maintain,
            "admin" => Level::Admin,
            _ => return None,
        })
    }
}

/// A person, as GitHub's `GET /user` names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: AccountId,
    pub login: String,
}

/// A person's permission, as GitHub answered for their login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permission {
    /// 404: no access.
    None,
    /// The user the answer is about, their level and `role_name`, which is
    /// for logs only.
    Of(AccountId, Option<Level>, String),
}

/// What is asked of GitHub, each failure an [`Outcome`].
pub trait GitHub: Send + Sync {
    /// The person's token for the web flow's `code`. `Forbidden`: GitHub
    /// did not take the code.
    fn token(&self, code: &str) -> Got<String>;
    /// Who `token` is.
    fn user(&self, token: &str) -> Got<Person>;
    /// `GET /repos/{repo}/collaborators/{login}/permission` with the
    /// installation token.
    fn permission(&self, login: &str) -> Got<Permission>;
    /// Whether the repository is public.
    fn public(&self) -> Got<bool>;
}

/// The installation token, as `tehasbot github-token` of `console` prints
/// it. `SignedOut` when there is none.
pub fn installation_token(console: &Path) -> Got<Secret> {
    let out = Command::new(console)
        .args(["tehasbot", "github-token"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| Outcome::SignedOut)?;
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || token.is_empty() {
        return Err(Outcome::SignedOut);
    }
    Ok(Secret::new(token))
}

/// What [`check`] found: the account it lets in, if any, and a line for a
/// log of why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub user: Option<AccountId>,
    pub said: String,
}

/// Whether `login` has `access` now: the account GitHub names for the
/// login, when it is `id` if one is given, compared whole, and its level is
/// at least `access`. On a
/// public repository read and triage let no one in, since every account has
/// them. Err: GitHub could not say, which a caller takes as a refusal.
pub fn check(gh: &dyn GitHub, access: Level, login: &str, id: Option<&AccountId>) -> Got<Verdict> {
    if access <= Level::Triage && gh.public()? {
        return Ok(Verdict {
            user: None,
            said: "the repository is public, so read and triage let no one in".into(),
        });
    }
    Ok(match gh.permission(login)? {
        Permission::None => Verdict {
            user: None,
            said: format!("{login} has no access"),
        },
        Permission::Of(user, level, role) => {
            let role: String = role
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || " _-".contains(*c))
                .take(64)
                .collect();
            let ok = id.is_none_or(|i| *i == user) && level.is_some_and(|l| l >= access);
            Verdict {
                user: ok.then_some(user),
                said: format!("{login} has the role {role:?}"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// GitHub answering `level` for user 7, or an error, and counting asks.
    pub struct Stub {
        pub level: Got<Option<Level>>,
        pub public: bool,
        pub asked: Mutex<u32>,
    }

    impl GitHub for Stub {
        fn token(&self, _: &str) -> Got<String> {
            Err(Outcome::Forbidden)
        }
        fn user(&self, _: &str) -> Got<Person> {
            Err(Outcome::Forbidden)
        }
        fn permission(&self, _: &str) -> Got<Permission> {
            *self.asked.lock().unwrap() += 1;
            match self.level.clone()? {
                None => Ok(Permission::None),
                Some(l) => Ok(Permission::Of(AccountId::new("7"), Some(l), "write".into())),
            }
        }
        fn public(&self) -> Got<bool> {
            Ok(self.public)
        }
    }

    fn stub(level: Got<Option<Level>>) -> Stub {
        Stub {
            level,
            public: false,
            asked: Mutex::new(0),
        }
    }

    fn id(s: &str) -> AccountId {
        AccountId::new(s)
    }

    #[test]
    fn a_login_has_access_at_its_level_and_its_id_only() {
        let w = stub(Ok(Some(Level::Write)));
        assert_eq!(
            check(&w, Level::Write, "octocat", None).unwrap().user,
            Some(id("7"))
        );
        assert_eq!(
            check(&w, Level::Write, "octocat", Some(&id("7")))
                .unwrap()
                .user,
            Some(id("7"))
        );
        assert_eq!(
            check(&w, Level::Write, "octocat", Some(&id("8")))
                .unwrap()
                .user,
            None
        );
        assert_eq!(check(&w, Level::Admin, "octocat", None).unwrap().user, None);
        assert_eq!(
            check(&stub(Ok(None)), Level::Read, "x", None).unwrap().user,
            None
        );
        let mut public = stub(Ok(Some(Level::Admin)));
        public.public = true;
        assert_eq!(check(&public, Level::Triage, "x", None).unwrap().user, None);
        assert_eq!(
            check(&public, Level::Write, "x", None).unwrap().user,
            Some(id("7"))
        );
        let down = Outcome::Unavailable {
            why: tehas_ports::Passing::Unreachable,
            sent: tehas_ports::Sent::Maybe,
        };
        assert_eq!(
            check(&stub(Err(down.clone())), Level::Write, "x", None),
            Err(down)
        );
    }

    /// The id is compared whole: no spelling of another id, nor of the same
    /// number, passes for it.
    #[test]
    fn only_the_same_id_passes() {
        let w = stub(Ok(Some(Level::Admin)));
        for other in ["07", "7 ", " 7", "", "70", "+7", "id-7", "7\n"] {
            assert_eq!(
                check(&w, Level::Read, "octocat", Some(&id(other)))
                    .unwrap()
                    .user,
                None,
                "{other:?}"
            );
        }
    }
}
