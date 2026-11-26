//! The [`Authorised`] curl of the factory's access questions and of a
//! person's sign-in: a login's permission on the project and whether the
//! project is public, with the installation token, for
//! `tehas_core::access::GitHub`; and the trade of a sign-in's code for a
//! person's token and who that token is, for the adapter's own
//! [`Trade`], which `port/people.rs` consumes. The person's token, the
//! App's client secret and the installation token go only in curl's config
//! on its standard input. The wiring gives the client and the installation
//! token's source; the adapter reads no file and no variable. Without them,
//! the questions that need them are `Refused(Unsupported)`, with no
//! request.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tehas_core::access::{AccountId, GitHub, Level, Permission};
use tehas_core::settings::rules;
use tehas_ports::{Got, Outcome, Passing, Refusal, Secret, Sent, Unread};

use crate::curl::{escape, Answer, Authorised};
use crate::port::{Person, Trade};

/// The most time one question takes.
const LIMIT: Duration = Duration::from_secs(20);

/// The access questions of one project.
#[derive(Clone)]
pub struct Access {
    curl: Authorised,
    /// `owner` and `name`, each escaped for a path.
    project: (String, String),
    client: Option<(String, Secret)>,
    installation: Option<Arc<dyn Fn() -> Got<Secret> + Send + Sync>>,
}

impl Access {
    /// The questions of `project` (`owner/name`) through `curl`, with no
    /// client and no installation token yet. None for a project that is
    /// not `owner/name`.
    pub fn new(curl: Authorised, project: &str) -> Option<Access> {
        if !rules::is_repo(project) {
            return None;
        }
        let (owner, name) = project.split_once('/')?;
        if [owner, name].iter().any(|p| matches!(*p, "." | "..")) {
            return None;
        }
        Some(Access {
            curl,
            project: (escape(owner), escape(name)),
            client: None,
            installation: None,
        })
    }

    /// The same with the App's client for people, which trades a code.
    pub fn with_client(self, id: &str, secret: Secret) -> Access {
        Access {
            client: Some((id.to_string(), secret)),
            ..self
        }
    }

    /// The same with the installation token's source, the wiring's, asked
    /// once for each permission and for the project's visibility.
    pub fn with_installation(
        self,
        installation: impl Fn() -> Got<Secret> + Send + Sync + 'static,
    ) -> Access {
        Access {
            installation: Some(Arc::new(installation)),
            ..self
        }
    }

    fn installation(&self) -> Got<Secret> {
        let source = self
            .installation
            .as_ref()
            .ok_or(Outcome::Refused(Refusal::Unsupported))?;
        source()
    }

    fn repo_path(&self) -> String {
        format!("repos/{}/{}", self.project.0, self.project.1)
    }
}

/// A status other than the one asked for: 401 the credential, 403 and 404
/// the actor's, 429 and 5xx a passing failure, any other an answer that
/// does not read.
fn answered(status: u16) -> Outcome {
    match status {
        401 => Outcome::SignedOut,
        403 => Outcome::Forbidden,
        404 => Outcome::NotFound,
        429 | 500..=599 => Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        },
        _ => malformed(),
    }
}

fn malformed() -> Outcome {
    Outcome::Unread(Unread::Malformed)
}

/// The body of a 200 as `T`; any other status as [`answered`].
fn read<T: for<'de> Deserialize<'de>>(a: &Answer) -> Got<T> {
    if a.status != 200 {
        return Err(answered(a.status));
    }
    serde_json::from_slice(&a.body).map_err(|_| malformed())
}

#[derive(Deserialize)]
struct Traded {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct User {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct Granted {
    user: GrantedUser,
    #[serde(default)]
    role_name: Option<String>,
}

#[derive(Deserialize)]
struct GrantedUser {
    id: u64,
    #[serde(default)]
    permissions: Booleans,
}

/// GitHub's `permissions` booleans; one not given is false.
#[derive(Deserialize, Default)]
struct Booleans {
    #[serde(default)]
    admin: bool,
    #[serde(default)]
    maintain: bool,
    #[serde(default)]
    push: bool,
    #[serde(default)]
    triage: bool,
    #[serde(default)]
    pull: bool,
}

impl Booleans {
    /// The most that is true. None when none is.
    fn level(&self) -> Option<Level> {
        [
            (self.admin, Level::Admin),
            (self.maintain, Level::Maintain),
            (self.push, Level::Write),
            (self.triage, Level::Triage),
            (self.pull, Level::Read),
        ]
        .into_iter()
        .find(|(on, _)| *on)
        .map(|(_, l)| l)
    }
}

#[derive(Deserialize)]
struct Repository {
    private: bool,
}

/// GitHub's user id as the account's id: its number in decimal.
fn id_of(id: u64) -> AccountId {
    AccountId::new(id.to_string())
}

impl Trade for Access {
    /// `Forbidden`: GitHub answered and did not take the code, which it
    /// says with a 200 and no token.
    fn token(&self, code: &str) -> Got<String> {
        let (id, secret) = self
            .client
            .as_ref()
            .ok_or(Outcome::Refused(Refusal::Unsupported))?;
        let answer = self.curl.trade(
            &[
                ("client_id", id),
                ("client_secret", secret.expose()),
                ("code", code),
            ],
            LIMIT,
        )?;
        match read::<Traded>(&answer)?.access_token {
            Some(t) if !t.is_empty() => Ok(t),
            _ => Err(Outcome::Forbidden),
        }
    }

    fn user(&self, token: &str) -> Got<Person> {
        let u: User = read(&self.curl.get("user", token, LIMIT)?)?;
        if !rules::is_login(&u.login) {
            return Err(malformed());
        }
        Ok(Person {
            id: id_of(u.id),
            login: u.login,
        })
    }
}

impl GitHub for Access {
    /// 404 is no access. A login GitHub never gives is `Refused(Invalid)`
    /// with no request.
    fn permission(&self, login: &str) -> Got<Permission> {
        if !rules::is_login(login) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let token = self.installation()?;
        let path = format!(
            "{}/collaborators/{}/permission",
            self.repo_path(),
            escape(login)
        );
        let answer = self.curl.get(&path, token.expose(), LIMIT)?;
        if answer.status == 404 {
            return Ok(Permission::None);
        }
        let g: Granted = read(&answer)?;
        Ok(Permission::Of(
            id_of(g.user.id),
            g.user.permissions.level(),
            g.role_name.unwrap_or_default(),
        ))
    }

    fn public(&self) -> Got<bool> {
        let token = self.installation()?;
        let r: Repository = read(&self.curl.get(&self.repo_path(), token.expose(), LIMIT)?)?;
        Ok(!r.private)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_not_asked_for_is_an_outcome() {
        assert_eq!(answered(401), Outcome::SignedOut);
        assert_eq!(answered(403), Outcome::Forbidden);
        assert_eq!(answered(404), Outcome::NotFound);
        for s in [429, 500, 503] {
            assert!(matches!(answered(s), Outcome::Unavailable { .. }), "{s}");
        }
        for s in [0, 201, 301, 418] {
            assert_eq!(answered(s), malformed(), "{s}");
        }
    }

    #[test]
    fn the_most_true_boolean_is_the_level() {
        let b = |s: &str| serde_json::from_str::<Booleans>(s).unwrap().level();
        assert_eq!(
            b(r#"{"pull":true,"triage":false,"push":false}"#),
            Some(Level::Read)
        );
        assert_eq!(
            b(r#"{"pull":true,"triage":true,"push":true,"maintain":false}"#),
            Some(Level::Write)
        );
        assert_eq!(b(r#"{"admin":true}"#), Some(Level::Admin));
        assert_eq!(b(r#"{}"#), None);
    }

    #[test]
    fn a_project_that_is_not_owner_and_name_has_no_questions() {
        for bad in [
            "",
            "acme",
            "acme/",
            "/widget",
            "acme/widget/x",
            "../x",
            "acme/..",
            "./x",
        ] {
            assert!(Access::new(Authorised::default(), bad).is_none(), "{bad:?}");
        }
        assert!(Access::new(Authorised::default(), "acme/widget").is_some());
    }
}
