//! Who has access to the factory's repository, one check for the plugins'
//! gate and for a call of a tool with `--login`
//! (docs/specs/2026-11-21-plugins-web.md, "Who gets in"). What is asked of
//! GitHub is behind [`GitHub`], so the tests stub it: the person's token for
//! the web flow's code, who the token is, and the person's permission on the
//! repository with the installation token. [`check`] compares the answer
//! with `TEHAS_PLUGIN_ACCESS` and the person by their [`AccountId`], whole
//! and exact. Nothing of it is kept here: the gate keeps a session's last
//! check in its memory, which a restart empties.
//!
//! [`Curl`] asks through curl, as console/ops/src/github.rs does: every token,
//! secret and code goes on curl's standard input, never on a command line.
//! The installation token is what `tehasbot github-token` prints.

use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
pub use tehas_ports::AccountId;
use tehas_ports::{Got, Outcome, Passing, Sent, Unread};

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

    /// The most of GitHub's `permissions` booleans: `pull`, `triage`, `push`,
    /// `maintain`, `admin`. None when none is true.
    pub fn of_permissions(p: &Value) -> Option<Level> {
        [
            ("admin", Level::Admin),
            ("maintain", Level::Maintain),
            ("push", Level::Write),
            ("triage", Level::Triage),
            ("pull", Level::Read),
        ]
        .into_iter()
        .find(|(k, _)| p.get(*k) == Some(&Value::Bool(true)))
        .map(|(_, l)| l)
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

/// GitHub through curl.
pub struct Curl {
    pub api: String,
    pub web: String,
    pub repo: String,
    pub client_id: String,
    pub client_secret: String,
    /// The console, which `tehasbot github-token` runs as.
    pub console: PathBuf,
}

fn quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// curl with `config` on its standard input: the body and the HTTP status.
fn curl(config: &str) -> Got<(u16, Vec<u8>)> {
    let mut child = Command::new("curl")
        .args([
            "-sS",
            "--proto",
            "=https",
            "--max-time",
            "20",
            "--max-filesize",
            "1048576",
            "-w",
            "\n%{response_code}",
            "-K",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| unavailable(Passing::Unreachable, Sent::No))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child
        .wait_with_output()
        .map_err(|_| unavailable(Passing::Unreachable, Sent::Maybe))?;
    match out.status.code() {
        Some(0) => {}
        Some(28) => return Err(unavailable(Passing::Timeout, Sent::Maybe)),
        Some(63) => return Err(Outcome::Unread(Unread::TooLarge)),
        _ => return Err(unavailable(Passing::Unreachable, Sent::Maybe)),
    }
    let text = out.stdout;
    let at = text.iter().rposition(|&b| b == b'\n').unwrap_or(0);
    let status = String::from_utf8_lossy(&text[at..])
        .trim()
        .parse()
        .unwrap_or(0);
    Ok((status, text[..at].to_vec()))
}

fn api_config(url: &str, bearer: &str) -> String {
    format!(
        "url = \"{}\"\nheader = \"Accept: application/vnd.github+json\"\nheader = \"Authorization: Bearer {}\"\n",
        quoted(url),
        quoted(bearer)
    )
}

fn json(body: &[u8]) -> Got<Value> {
    serde_json::from_slice(body).map_err(|_| malformed())
}

fn unavailable(why: Passing, sent: Sent) -> Outcome {
    Outcome::Unavailable { why, sent }
}

fn malformed() -> Outcome {
    Outcome::Unread(Unread::Malformed)
}

/// A status other than the one asked for: 401 the credential, 403 and 404
/// the actor's, 429 and 5xx a passing failure, any other an answer that
/// does not read.
fn answered(status: u16) -> Outcome {
    match status {
        401 => Outcome::SignedOut,
        403 => Outcome::Forbidden,
        404 => Outcome::NotFound,
        429 | 500..=599 => unavailable(Passing::Server, Sent::Maybe),
        _ => malformed(),
    }
}

impl Curl {
    /// The installation token, or `SignedOut` when there is none.
    fn installation(&self) -> Got<String> {
        let out = Command::new(&self.console)
            .args(["tehasbot", "github-token"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|_| Outcome::SignedOut)?;
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() || token.is_empty() {
            return Err(Outcome::SignedOut);
        }
        Ok(token)
    }
}

impl GitHub for Curl {
    fn token(&self, code: &str) -> Got<String> {
        let config = format!(
            "url = \"{}/login/oauth/access_token\"\nheader = \"Accept: application/json\"\ndata-urlencode = \"client_id={}\"\ndata-urlencode = \"client_secret={}\"\ndata-urlencode = \"code={}\"\n",
            quoted(&self.web),
            quoted(&self.client_id),
            quoted(&self.client_secret),
            quoted(code)
        );
        let (status, body) = curl(&config)?;
        if status != 200 {
            return Err(answered(status));
        }
        // GitHub answers 200 with an `error` for a code it does not take.
        match json(&body)?["access_token"].as_str() {
            Some(t) if !t.is_empty() => Ok(t.to_string()),
            _ => Err(Outcome::Forbidden),
        }
    }

    fn user(&self, token: &str) -> Got<Person> {
        let (status, body) = curl(&api_config(&format!("{}/user", self.api), token))?;
        if status != 200 {
            return Err(answered(status));
        }
        let v = json(&body)?;
        match (v["id"].as_u64(), v["login"].as_str()) {
            (Some(id), Some(login)) if crate::settings::rules::is_login(login) => Ok(Person {
                id: id_of(id),
                login: login.to_string(),
            }),
            _ => Err(malformed()),
        }
    }

    fn permission(&self, login: &str) -> Got<Permission> {
        let token = self.installation()?;
        let url = format!(
            "{}/repos/{}/collaborators/{login}/permission",
            self.api, self.repo
        );
        let (status, body) = curl(&api_config(&url, &token))?;
        permission_of(status, &body)
    }

    fn public(&self) -> Got<bool> {
        let token = self.installation()?;
        let url = format!("{}/repos/{}", self.api, self.repo);
        let (status, body) = curl(&api_config(&url, &token))?;
        if status != 200 {
            return Err(answered(status));
        }
        match json(&body)?["private"].as_bool() {
            Some(private) => Ok(!private),
            None => Err(malformed()),
        }
    }
}

/// GitHub's answer for a permission: 404 is no access, any other status but
/// 200 an error.
pub fn permission_of(status: u16, body: &[u8]) -> Got<Permission> {
    match status {
        404 => return Ok(Permission::None),
        200 => {}
        s => return Err(answered(s)),
    }
    let v = json(body)?;
    let Some(id) = v["user"]["id"].as_u64() else {
        return Err(malformed());
    };
    let role = v["role_name"].as_str().unwrap_or("").to_string();
    Ok(Permission::Of(
        id_of(id),
        Level::of_permissions(&v["user"]["permissions"]),
        role,
    ))
}

/// GitHub's user id as the account's id: its number in decimal, as the
/// adapter writes it.
fn id_of(id: u64) -> AccountId {
    AccountId::new(id.to_string())
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
        let down = unavailable(Passing::Unreachable, Sent::Maybe);
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

    #[test]
    fn a_permission_names_its_user_by_the_decimal_of_its_number() {
        let body =
            br#"{"user": {"id": 583231, "permissions": {"push": true}}, "role_name": "write"}"#;
        assert_eq!(
            permission_of(200, body),
            Ok(Permission::Of(
                id("583231"),
                Some(Level::Write),
                "write".into()
            ))
        );
        assert_eq!(permission_of(404, b""), Ok(Permission::None));
        assert_eq!(
            permission_of(200, br#"{"user": {"id": "583231"}}"#),
            Err(malformed())
        );
    }

    #[test]
    fn a_status_not_asked_for_is_an_outcome() {
        assert_eq!(answered(401), Outcome::SignedOut);
        assert_eq!(answered(403), Outcome::Forbidden);
        assert_eq!(answered(404), Outcome::NotFound);
        for s in [429, 500, 502, 503] {
            assert_eq!(
                answered(s),
                unavailable(Passing::Server, Sent::Maybe),
                "{s}"
            );
        }
        for s in [0, 201, 301, 418] {
            assert_eq!(answered(s), malformed(), "{s}");
        }
        assert_eq!(permission_of(502, b""), Err(answered(502)));
    }
}
