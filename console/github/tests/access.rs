//! The plugins' access questions through the adapter's authorised curl
//! (`tehas_github::Access`): each credential only on curl's standard input,
//! never in its arguments or environment; the API's host and GitHub's
//! trade address alone; GitHub's answers read into the access check's
//! terms, and every failure an outcome. curl is a stub that keeps the
//! arguments, standard input and environment of each call and answers
//! from the files a test wrote.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use tehas_core::access::{AccountId, GitHub, Level, Permission};
use tehas_core::testdir::TestDir;
use tehas_github::{Access, AuthorisedCurl, CURL_ENV};
use tehas_ports::{Outcome, Passing, Refusal, Secret, Sent, Unread};

const PERSON_TOKEN: &str = "gho_person0123456789";
const INSTALLATION: &str = "ghs_installation0123456789";
const CLIENT_SECRET: &str = "0123456789abcdef0123456789abcdef01234567";

/// One test at a time: a script just written cannot be started while
/// another test's fork still holds it open for writing.
static SERIAL: Mutex<()> = Mutex::new(());

struct Stub {
    dir: TestDir,
    _serial: MutexGuard<'static, ()>,
}

impl Stub {
    fn new(name: &str) -> Stub {
        let serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let dir = TestDir::new(name);
        let script = format!(
            r#"#!/bin/sh
d='{dir}'
for a in "$@"; do printf '%s\n' "$a"; done >> "$d/argv"
printf -- '--\n' >> "$d/argv"
case " $* " in *" --config - "*) cat >> "$d/stdin";; esac
printf -- '--\n' >> "$d/stdin"
tr '\0' '\n' < /proc/$$/environ >> "$d/env"
printf -- '--\n' >> "$d/env"
for a in "$@"; do url=$a; done
f=$(printf '%s' "$url" | tr -c 'A-Za-z0-9' '_')
if [ -f "$d/answers/$f" ]; then cat "$d/answers/$f"; else printf 'HTTP/2 404 \r\n\r\n{{"message":"Not Found"}}'; fi
"#,
            dir = dir.path().display(),
        );
        std::fs::create_dir_all(dir.join("answers")).unwrap();
        let program = dir.join("curl");
        std::fs::write(&program, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Stub {
            dir,
            _serial: serial,
        }
    }

    fn program(&self) -> PathBuf {
        self.dir.join("curl")
    }

    /// `url` answers `status` with `body`.
    fn answer(&self, url: &str, status: u16, body: &str) {
        let f: String = url
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        std::fs::write(
            self.dir.join("answers").join(f),
            format!("HTTP/2 {status} \r\ncontent-type: application/json\r\n\r\n{body}"),
        )
        .unwrap();
    }

    fn calls(&self, file: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.join(file))
            .unwrap_or_default()
            .split_terminator("--\n")
            .map(String::from)
            .collect()
    }

    fn access(&self) -> Access {
        Access::new(AuthorisedCurl::at(self.program()), "acme/widget")
            .unwrap()
            .with_client("Iv1.client", Secret::new(CLIENT_SECRET))
            .with_installation(|| Ok(Secret::new(INSTALLATION)))
    }

    /// No credential in any call's arguments or environment.
    fn kept_no_secret_outside_stdin(&self) {
        for file in ["argv", "env"] {
            let text = std::fs::read_to_string(self.dir.join(file)).unwrap_or_default();
            for secret in [PERSON_TOKEN, INSTALLATION, CLIENT_SECRET, "c0de"] {
                assert!(!text.contains(secret), "{secret} in {file}");
            }
        }
    }
}

const API: &str = "https://api.github.com/";

#[test]
fn a_code_is_traded_with_the_client_on_stdin_only() {
    let s = Stub::new("access-trade");
    s.answer(
        "https://github.com/login/oauth/access_token",
        200,
        &format!(r#"{{"access_token":"{PERSON_TOKEN}","token_type":"bearer"}}"#),
    );
    assert_eq!(s.access().token("c0de").as_deref(), Ok(PERSON_TOKEN));
    assert_eq!(
        s.calls("stdin"),
        [format!(
            "data-urlencode = \"client_id=Iv1.client\"\ndata-urlencode = \"client_secret={CLIENT_SECRET}\"\ndata-urlencode = \"code=c0de\"\n"
        )]
    );
    // The anonymous GET's arguments but its headers, then the config from
    // standard input and the one address.
    assert_eq!(
        s.calls("argv")[0].lines().collect::<Vec<_>>(),
        [
            "-q",
            "--silent",
            "--include",
            "--proto",
            "=https",
            "--max-redirs",
            "0",
            "--connect-timeout",
            "10",
            "--max-time",
            "20",
            "--max-filesize",
            "16777216",
            "--header",
            "Accept: application/json",
            "--user-agent",
            "tehas",
            "--config",
            "-",
            "https://github.com/login/oauth/access_token",
        ]
    );
    s.kept_no_secret_outside_stdin();
}

#[test]
fn a_code_github_does_not_take_is_forbidden() {
    let s = Stub::new("access-trade-refused");
    s.answer(
        "https://github.com/login/oauth/access_token",
        200,
        r#"{"error":"bad_verification_code","error_description":"The code passed is incorrect or expired."}"#,
    );
    assert_eq!(s.access().token("c0de"), Err(Outcome::Forbidden));
}

#[test]
fn who_a_token_is_is_asked_with_it_on_stdin() {
    let s = Stub::new("access-user");
    s.answer(
        &format!("{API}user"),
        200,
        r#"{"id":583231,"login":"Carol","type":"User"}"#,
    );
    let p = s.access().user(PERSON_TOKEN).unwrap();
    assert_eq!(
        (p.id, p.login.as_str()),
        (AccountId::new("583231"), "Carol")
    );
    assert_eq!(
        s.calls("stdin"),
        [format!(
            "header = \"Authorization: Bearer {PERSON_TOKEN}\"\n"
        )]
    );
    s.kept_no_secret_outside_stdin();
    assert_eq!(
        s.calls("argv")[0].lines().collect::<Vec<_>>(),
        [
            "-q",
            "--silent",
            "--include",
            "--proto",
            "=https",
            "--max-redirs",
            "0",
            "--connect-timeout",
            "10",
            "--max-time",
            "20",
            "--max-filesize",
            "16777216",
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--user-agent",
            "tehas",
            "--config",
            "-",
            "https://api.github.com/user",
        ]
    );
    s.answer(
        &format!("{API}user"),
        200,
        r#"{"id":1,"login":"not a login"}"#,
    );
    assert_eq!(
        s.access().user(PERSON_TOKEN).err(),
        Some(Outcome::Unread(Unread::Malformed))
    );
}

#[test]
fn a_permission_is_read_into_a_level_by_the_account_id() {
    let s = Stub::new("access-permission");
    let url = format!("{API}repos/acme/widget/collaborators/carol/permission");
    s.answer(
        &url,
        200,
        r#"{"permission":"write","role_name":"write","user":{"id":583231,"login":"carol","permissions":{"admin":false,"maintain":false,"push":true,"triage":true,"pull":true}}}"#,
    );
    assert_eq!(
        s.access().permission("carol"),
        Ok(Permission::Of(
            AccountId::new("583231"),
            Some(Level::Write),
            "write".into()
        ))
    );
    assert_eq!(
        s.calls("stdin"),
        [format!(
            "header = \"Authorization: Bearer {INSTALLATION}\"\n"
        )]
    );
    s.kept_no_secret_outside_stdin();
    s.answer(&url, 404, r#"{"message":"Not Found"}"#);
    assert_eq!(s.access().permission("carol"), Ok(Permission::None));
    s.answer(&url, 200, r#"{"user":{"id":"583231"}}"#);
    assert_eq!(
        s.access().permission("carol"),
        Err(Outcome::Unread(Unread::Malformed))
    );
}

#[test]
fn a_login_github_never_gives_asks_nothing() {
    let s = Stub::new("access-login");
    for bad in ["", "../x", "a/b", "a b", "a\"b"] {
        assert_eq!(
            s.access().permission(bad),
            Err(Outcome::Refused(Refusal::Invalid)),
            "{bad:?}"
        );
    }
    assert!(s.calls("argv").is_empty());
}

#[test]
fn whether_the_project_is_public() {
    let s = Stub::new("access-public");
    let url = format!("{API}repos/acme/widget");
    s.answer(&url, 200, r#"{"id":1,"private":false}"#);
    assert_eq!(s.access().public(), Ok(true));
    s.answer(&url, 200, r#"{"id":1,"private":true}"#);
    assert_eq!(s.access().public(), Ok(false));
    s.answer(&url, 200, r#"{"id":1}"#);
    assert_eq!(s.access().public(), Err(Outcome::Unread(Unread::Malformed)));
    s.kept_no_secret_outside_stdin();
}

#[test]
fn a_status_not_asked_for_is_an_outcome() {
    let s = Stub::new("access-status");
    let url = format!("{API}repos/acme/widget");
    for (status, outcome) in [
        (401, Outcome::SignedOut),
        (403, Outcome::Forbidden),
        (404, Outcome::NotFound),
        (
            502,
            Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            },
        ),
        (302, Outcome::Unread(Unread::Malformed)),
    ] {
        s.answer(&url, status, "{}");
        assert_eq!(s.access().public(), Err(outcome), "{status}");
    }
}

#[test]
fn without_the_client_or_the_installation_nothing_is_asked() {
    let s = Stub::new("access-unsupported");
    let bare = Access::new(AuthorisedCurl::at(s.program()), "acme/widget").unwrap();
    let unsupported = Err(Outcome::Refused(Refusal::Unsupported));
    assert_eq!(
        bare.token("c0de"),
        unsupported.clone().map(|_: ()| String::new())
    );
    assert_eq!(bare.public(), unsupported.clone().map(|_: ()| false));
    assert_eq!(
        bare.permission("carol"),
        unsupported.map(|_: ()| Permission::None)
    );
    assert!(s.calls("argv").is_empty());
}

#[test]
fn curl_runs_with_its_own_environment_alone() {
    let s = Stub::new("access-env");
    s.answer(
        &format!("{API}repos/acme/widget"),
        200,
        r#"{"private":false}"#,
    );
    assert_eq!(s.access().public(), Ok(true));
    let env = s.calls("env");
    let mut got: Vec<&str> = env[0].lines().filter(|l| !l.starts_with("PWD=")).collect();
    got.sort();
    let mut want: Vec<String> = CURL_ENV.iter().map(|(k, v)| format!("{k}={v}")).collect();
    want.sort();
    assert_eq!(got, want);
}
