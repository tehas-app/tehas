//! Who has access to the factory's repository, one check for the plugins'
//! gate and for a call of a tool with `--login`
//! (docs/specs/2026-11-21-plugins-web.md, "Who gets in"). What is asked of
//! GitHub is behind [`GitHub`], so the tests stub it: the person's token for
//! the web flow's code, who the token is, and the person's permission on the
//! repository with the installation token. [`check`] compares the answer
//! with `TEHAS_PLUGIN_ACCESS` by user id
//! for 10 minutes in a file.
//!
//! [`Curl`] asks through curl, as console/ops/src/github.rs does: every token,
//! secret and code goes on curl's standard input, never on a command line.
//! The installation token is what `tehasbot github-token` prints.

use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
    pub id: u64,
    pub login: String,
}

/// A person's permission, as GitHub answered for their login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permission {
    /// 404: no access.
    None,
    /// The user the answer is about, their level and `role_name`, which is
    /// for logs only.
    Of(u64, Option<Level>, String),
}

pub trait GitHub: Send + Sync {
    /// The person's token for the web flow's `code`.
    fn token(&self, code: &str) -> Result<String, String>;
    /// Who `token` is.
    fn user(&self, token: &str) -> Result<Person, String>;
    /// `GET /repos/{repo}/collaborators/{login}/permission` with the
    /// installation token.
    fn permission(&self, login: &str) -> Result<Permission, String>;
    /// Whether the repository is public.
    fn public(&self) -> Result<bool, String>;
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
fn curl(config: &str) -> Result<(u16, Vec<u8>), String> {
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
        .map_err(|_| "curl cannot run".to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child
        .wait_with_output()
        .map_err(|_| "curl did not finish".to_string())?;
    if !out.status.success() {
        return Err("GitHub could not be reached".to_string());
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

fn json(body: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(body).map_err(|_| "GitHub's answer was not JSON".to_string())
}

impl Curl {
    fn installation(&self) -> Result<String, String> {
        let out = Command::new(&self.console)
            .args(["tehasbot", "github-token"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|_| "no installation token".to_string())?;
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() || token.is_empty() {
            return Err("no installation token".to_string());
        }
        Ok(token)
    }
}

impl GitHub for Curl {
    fn token(&self, code: &str) -> Result<String, String> {
        let config = format!(
            "url = \"{}/login/oauth/access_token\"\nheader = \"Accept: application/json\"\ndata-urlencode = \"client_id={}\"\ndata-urlencode = \"client_secret={}\"\ndata-urlencode = \"code={}\"\n",
            quoted(&self.web),
            quoted(&self.client_id),
            quoted(&self.client_secret),
            quoted(code)
        );
        let (status, body) = curl(&config)?;
        let v = json(&body)?;
        match v["access_token"].as_str() {
            Some(t) if status == 200 && !t.is_empty() => Ok(t.to_string()),
            _ => Err("GitHub did not take the sign-in's code".to_string()),
        }
    }

    fn user(&self, token: &str) -> Result<Person, String> {
        let (status, body) = curl(&api_config(&format!("{}/user", self.api), token))?;
        if status != 200 {
            return Err(format!("GitHub answered {status} for the person"));
        }
        let v = json(&body)?;
        match (v["id"].as_u64(), v["login"].as_str()) {
            (Some(id), Some(login)) if crate::settings::rules::is_login(login) => Ok(Person {
                id,
                login: login.to_string(),
            }),
            _ => Err("GitHub's answer named no person".to_string()),
        }
    }

    fn permission(&self, login: &str) -> Result<Permission, String> {
        let token = self.installation()?;
        let url = format!(
            "{}/repos/{}/collaborators/{login}/permission",
            self.api, self.repo
        );
        let (status, body) = curl(&api_config(&url, &token))?;
        permission_of(status, &body)
    }

    fn public(&self) -> Result<bool, String> {
        let token = self.installation()?;
        let url = format!("{}/repos/{}", self.api, self.repo);
        let (status, body) = curl(&api_config(&url, &token))?;
        if status != 200 {
            return Err(format!("GitHub answered {status} for the repository"));
        }
        match json(&body)?["private"].as_bool() {
            Some(private) => Ok(!private),
            None => Err("GitHub's answer did not say whether the repository is private".into()),
        }
    }
}

/// GitHub's answer for a permission: 404 is no access, any other status but
/// 200 an error.
pub fn permission_of(status: u16, body: &[u8]) -> Result<Permission, String> {
    match status {
        404 => return Ok(Permission::None),
        200 => {}
        s => return Err(format!("GitHub answered {s} for the permission")),
    }
    let v = json(body)?;
    let Some(id) = v["user"]["id"].as_u64() else {
        return Err("GitHub's answer named no user".to_string());
    };
    let role = v["role_name"].as_str().unwrap_or("").to_string();
    Ok(Permission::Of(
        id,
        Level::of_permissions(&v["user"]["permissions"]),
        role,
    ))
}

/// What [`check`] found: the user id it lets in, if any, and a line for a
/// log of why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub user: Option<u64>,
    pub said: String,
}

/// Whether `login` has `access` now: the user GitHub names for the login,
/// when it is `id` if one is given and its level is at least `access`. On a
/// public repository read and triage let no one in, since every account has
/// them. Err: GitHub could not say, which a caller takes as a refusal.
pub fn check(
    gh: &dyn GitHub,
    access: Level,
    login: &str,
    id: Option<u64>,
) -> Result<Verdict, String> {
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
            let ok = id.is_none_or(|i| i == user) && level.is_some_and(|l| l >= access);
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
        pub level: Result<Option<Level>, String>,
        pub public: bool,
        pub asked: Mutex<u32>,
    }

    impl GitHub for Stub {
        fn token(&self, _: &str) -> Result<String, String> {
            Err("no".into())
        }
        fn user(&self, _: &str) -> Result<Person, String> {
            Err("no".into())
        }
        fn permission(&self, _: &str) -> Result<Permission, String> {
            *self.asked.lock().unwrap() += 1;
            match self.level.clone()? {
                None => Ok(Permission::None),
                Some(l) => Ok(Permission::Of(7, Some(l), "write".into())),
            }
        }
        fn public(&self) -> Result<bool, String> {
            Ok(self.public)
        }
    }

    fn stub(level: Result<Option<Level>, String>) -> Stub {
        Stub {
            level,
            public: false,
            asked: Mutex::new(0),
        }
    }

    #[test]
    fn a_login_has_access_at_its_level_and_its_id_only() {
        let w = stub(Ok(Some(Level::Write)));
        assert_eq!(
            check(&w, Level::Write, "octocat", None).unwrap().user,
            Some(7)
        );
        assert_eq!(
            check(&w, Level::Write, "octocat", Some(7)).unwrap().user,
            Some(7)
        );
        assert_eq!(
            check(&w, Level::Write, "octocat", Some(8)).unwrap().user,
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
            Some(7)
        );
        assert!(check(&stub(Err("down".into())), Level::Write, "x", None).is_err());
    }
}
