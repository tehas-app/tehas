//! The factory's GitHub App: an installation token, made from the app's key
//! and kept fresh in one file (docs/specs/2026-10-24-github-app.md).
//!
//!   console tehasbot github-token [--check]
//!
//! Prints a token with at least 10 minutes left, making one when the kept one
//! has less. --check says when the token runs out and whether it reaches
//! TEHAS_REPO, and never prints the token. `libexec/github-token` is the path
//! the gh shim, the poll and `tehas-cycle renew` ask: it loads the settings
//! and comes here.
//!
//! The token is $TEHAS_HOME/state/github/token (mode 600), its expiry
//! token.expires (epoch seconds) beside it. The directory, not the file, is
//! what a cycle's container is given read-only: a new token is moved over the
//! old one, and a mount of the file would keep showing the old. One mint at a
//! time: state/github/lock, waited for up to 30 seconds. GitHub is asked at
//! TEHAS_GITHUB_API [https://api.github.com, from the environment; the tests
//! point it away]. The key is TEHAS_GITHUB_APP_KEY, which tehas setup uses for
//! a key it has not installed yet, else ~/.config/tehas/github-app.pem.
//!
//! A JWT or a token goes to curl on standard input, never on a command line.
//! A token is only ever printed to standard output, never logged.
//!
//! Exit: 0 a token, or a check that passed; 1 no token could be had, or the
//! token does not reach TEHAS_REPO; 64 an argument it does not know; 78 the
//! settings name no GitHub App.

use crate::flags;
use serde_json::Value;
use std::env;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::{settings, time};

const USAGE: &str = "usage: tehasbot github-token [--check]";
const ACCEPT: &str = "Accept: application/vnd.github+json";
const LEFT: i64 = 600;
const LOCK_WAIT: Duration = Duration::from_secs(30);

pub struct App {
    pub id: String,
    pub installation: String,
    pub key: PathBuf,
}

fn api() -> String {
    env::var("TEHAS_GITHUB_API")
        .ok()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "https://api.github.com".to_string())
}

fn b64url(data: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let at = |i: usize| chunk.get(i).copied().unwrap_or(0) as u32;
        let n = at(0) << 16 | at(1) << 8 | at(2);
        for i in 0..=chunk.len() {
            out.push(LETTERS[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// The signed part of the app's JSON Web Token, valid for 9 minutes and
/// back-dated one, as GitHub allows for a clock a little ahead of its own.
fn claims(id: &str, now: i64) -> String {
    let body = format!(r#"{{"iat":{},"exp":{},"iss":"{id}"}}"#, now - 60, now + 540);
    format!(
        "{}.{}",
        b64url(br#"{"alg":"RS256","typ":"JWT"}"#),
        b64url(body.as_bytes())
    )
}

/// A value inside the double quotes of a curl config: a quote or a line of
/// its own there would end the value and start an option.
fn quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// curl's answer, when it exits 0. The bearer goes on standard input.
fn curl(bearer: &str, flags: &[&str], url: &str) -> Option<Vec<u8>> {
    let mut child = Command::new("curl")
        .args(["-fsS", "--max-time", "30", "-K", "-"])
        .args(flags)
        .args(["-H", ACCEPT, url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let config = format!("header = \"Authorization: Bearer {}\"\n", quoted(bearer));
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    out.status.success().then_some(out.stdout)
}

/// The app's slug in GitHub's answer for GET /app.
fn slug_of(answer: &[u8]) -> Option<String> {
    let answer: Value = serde_json::from_slice(answer).ok()?;
    let slug = answer["slug"].as_str()?;
    let mut letters = slug.bytes();
    let good = letters.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && letters.all(|c| c.is_ascii_alphanumeric() || c == b'-');
    good.then(|| slug.to_string())
}

impl App {
    /// The app the loaded settings name. The caller has required the names.
    fn loaded() -> App {
        let var = |name| env::var(name).unwrap_or_default();
        let key = env::var_os("TEHAS_GITHUB_APP_KEY")
            .filter(|k| !k.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env::var_os("HOME").unwrap_or_default())
                    .join(".config/tehas/github-app.pem")
            });
        App {
            id: var("TEHAS_GITHUB_APP_ID"),
            installation: var("TEHAS_GITHUB_APP_INSTALLATION"),
            key,
        }
    }

    fn readable(&self) -> bool {
        let readable = File::open(&self.key).is_ok();
        if !readable {
            eprintln!(
                "tehas: the GitHub App's key is not readable at {}",
                self.key.display()
            );
        }
        readable
    }

    fn sign(&self, message: &str) -> Option<Vec<u8>> {
        let mut child = Command::new("openssl")
            .args(["dgst", "-sha256", "-sign"])
            .arg(&self.key)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(message.as_bytes());
        }
        let out = child.wait_with_output().ok()?;
        (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
    }

    fn jwt(&self) -> Option<String> {
        if !self.readable() {
            return None;
        }
        let signed = claims(&self.id, time::now());
        let Some(signature) = self.sign(&signed) else {
            eprintln!(
                "tehas: the GitHub App's token could not be signed with {}",
                self.key.display()
            );
            return None;
        };
        Some(format!("{signed}.{}", b64url(&signature)))
    }

    /// GitHub's whole answer for a new installation token: the token, its
    /// expiry and its permissions. Keeps nothing.
    pub fn mint(&self) -> Option<Vec<u8>> {
        let jwt = self.jwt()?;
        let url = format!(
            "{}/app/installations/{}/access_tokens",
            api(),
            self.installation
        );
        let answer = curl(&jwt, &["-X", "POST"], &url);
        if answer.is_none() {
            eprintln!(
                "tehas: GitHub did not give the app a token for installation {}",
                self.installation
            );
        }
        answer
    }

    /// The app's slug, from GET /app with its JWT.
    pub fn slug(&self) -> Option<String> {
        let jwt = self.jwt()?;
        let Some(answer) = curl(&jwt, &[], &format!("{}/app", api())) else {
            eprintln!("tehas: GitHub did not answer GET /app for app {}", self.id);
            return None;
        };
        let slug = slug_of(&answer);
        if slug.is_none() {
            eprintln!("tehas: GitHub's answer for app {} held no slug", self.id);
        }
        slug
    }
}

/// A file's text, without the newlines that end it.
fn text(file: &Path) -> Option<String> {
    let read = fs::read_to_string(file).ok()?;
    Some(read.trim_end_matches('\n').to_string())
}

/// The kept token, when it lasts until `by`.
fn kept(dir: &Path, by: i64) -> Option<String> {
    let until = text(&dir.join("token.expires"))?;
    if until.is_empty() || until.len() > 12 || !until.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if until.parse::<i64>().ok()? < by {
        return None;
    }
    text(&dir.join("token")).filter(|token| !token.is_empty())
}

/// The token and expiry of GitHub's answer, when the expiry is still ahead.
fn exchanged(answer: &[u8], now: i64) -> Option<(String, i64)> {
    let answer: Value = serde_json::from_slice(answer).ok()?;
    let token = answer["token"].as_str().filter(|t| !t.is_empty())?;
    let until = answer["expires_at"]
        .as_str()
        .and_then(flags::epoch)
        .filter(|until| *until > now)?;
    Some((token.to_string(), until))
}

fn write_new(file: &Path, line: &str) -> bool {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)
        .and_then(|mut f| writeln!(f, "{line}"))
        .is_ok()
}

/// Written beside the old one and moved over it: a reader sees the old token
/// or the new one, never half of one.
fn keep(dir: &Path, token: &str, until: i64) -> bool {
    let token_new = dir.join("token.new");
    let until_new = dir.join("token.expires.new");
    let kept = write_new(&token_new, token)
        && write_new(&until_new, &until.to_string())
        && fs::rename(&token_new, dir.join("token")).is_ok()
        && fs::rename(&until_new, dir.join("token.expires")).is_ok();
    if !kept {
        let _ = fs::remove_file(&token_new);
        let _ = fs::remove_file(&until_new);
    }
    kept
}

fn wait_for(lock: &File, wait: Duration) -> bool {
    let started = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return true,
            Err(TryLockError::WouldBlock) if started.elapsed() < wait => {
                sleep(Duration::from_millis(50));
            }
            Err(_) => return false,
        }
    }
}

/// A token with at least 10 minutes left, or None with one line on standard
/// error: no key, the exchange failed or answered no token.
fn token(app: &App, dir: &Path) -> Option<String> {
    if let Some(token) = kept(dir, time::now() + LEFT) {
        return Some(token);
    }
    if !app.readable() {
        return None;
    }
    let lock = fs::create_dir_all(dir).and_then(|()| {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(dir.join("lock"))
    });
    let lock = match lock {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!(
                "tehas: the app's token cannot be kept in {}: {e}",
                dir.display()
            );
            return None;
        }
    };
    if !wait_for(&lock, LOCK_WAIT) {
        eprintln!("tehas: another command is making the GitHub token");
        return None;
    }
    // Another command may have made one while this one waited.
    if let Some(token) = kept(dir, time::now() + LEFT) {
        return Some(token);
    }
    let answer = app.mint()?;
    let Some((token, until)) = exchanged(&answer, time::now()) else {
        eprintln!("tehas: GitHub's answer for the app's token held no token and expiry");
        return None;
    };
    if !keep(dir, &token, until) {
        eprintln!(
            "tehas: the app's token could not be kept in {}",
            dir.display()
        );
        return None;
    }
    Some(token)
}

fn say(line: &str) {
    let _ = writeln!(io::stdout(), "{line}");
}

fn reaches(answer: &[u8], repo: &str) -> bool {
    let answer: Value = serde_json::from_slice(answer).unwrap_or_default();
    answer["repositories"]
        .as_array()
        .is_some_and(|all| all.iter().any(|r| r["full_name"] == repo))
}

fn check(app: &App, dir: &Path, token: &str) -> u8 {
    if let Err(rc) = settings::require(&["TEHAS_REPO"]) {
        return rc;
    }
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let login = env::var("TEHAS_BOT_LOGIN").unwrap_or_default();
    say(&format!(
        "app {}, installation {}, acting as {login}",
        app.id, app.installation
    ));
    let until = text(&dir.join("token.expires"))
        .and_then(|until| until.parse().ok())
        .map_or_else(|| "an expiry that cannot be read".to_string(), time::iso);
    say(&format!("token good until {until}"));
    let url = format!("{}/installation/repositories?per_page=100", api());
    let Some(answer) = curl(token, &[], &url) else {
        eprintln!("tehas github-token: the token could not list its repositories");
        return 1;
    };
    if !reaches(&answer, &repo) {
        eprintln!(
            "tehas github-token: the app's installation does not reach {repo}; install the app on it"
        );
        return 1;
    }
    say(&format!("it reaches {repo}"));
    0
}

/// Whether the arguments ask for --check. What is said of them before the
/// settings are loaded.
pub fn words(given: &[String]) -> Result<bool, u8> {
    match given {
        [] => Ok(false),
        [word] if word.is_empty() => Ok(false),
        [word] if word == "--check" => Ok(true),
        _ => {
            eprintln!("{USAGE}");
            Err(64)
        }
    }
}

const NEEDS: [&str; 4] = [
    "TEHAS_HOME",
    "TEHAS_GITHUB_APP_ID",
    "TEHAS_GITHUB_APP_INSTALLATION",
    "TEHAS_BOT_LOGIN",
];

fn kept_in() -> PathBuf {
    PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default()).join("state/github")
}

/// tehas_github_env, for a command that hands the token on to a container: a
/// token with at least 10 minutes left. None when none could be had, said on
/// standard error.
pub fn for_a_container() -> Option<String> {
    settings::require(&NEEDS).ok()?;
    token(&App::loaded(), &kept_in())
}

pub fn main(given: &[String]) -> u8 {
    let checking = match words(given) {
        Ok(checking) => checking,
        Err(rc) => return rc,
    };
    if let Err(rc) = settings::require(&NEEDS) {
        return rc;
    }
    let dir = kept_in();
    let app = App::loaded();
    let Some(token) = token(&app, &dir) else {
        return 1;
    };
    if checking {
        return check(&app, &dir, &token);
    }
    say(&token);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let d = env::temp_dir().join(format!("tehas-github-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    fn put(d: &Dir, token: &str, until: &str) {
        fs::write(d.0.join("token"), token).unwrap();
        fs::write(d.0.join("token.expires"), until).unwrap();
    }

    #[test]
    fn b64url_has_no_padding_and_the_url_letters() {
        for (data, want) in [
            (&b""[..], ""),
            (b"f", "Zg"),
            (b"fo", "Zm8"),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg"),
            (&[0xfb, 0xff, 0xfe], "-__-"),
        ] {
            assert_eq!(b64url(data), want);
        }
    }

    #[test]
    fn claims_name_the_app_and_last_ten_minutes() {
        let signed = claims("5452597", 1000);
        assert_eq!(
            signed,
            format!(
                "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.{}",
                b64url(br#"{"iat":940,"exp":1540,"iss":"5452597"}"#)
            )
        );
    }

    #[test]
    fn a_config_value_cannot_end_its_line() {
        assert_eq!(quoted("ghs_abc.def-_"), "ghs_abc.def-_");
        assert_eq!(quoted("a\"b\\c\nd\re"), "a\\\"b\\\\c\\nd\\re");
    }

    #[test]
    fn the_slug_is_a_word_of_letters_digits_and_dashes() {
        assert_eq!(
            slug_of(br#"{"slug":"tehas-app-bot"}"#).as_deref(),
            Some("tehas-app-bot")
        );
        for bad in [
            &br#"{}"#[..],
            br#"{"slug":""}"#,
            br#"{"slug":null}"#,
            br#"{"slug":7}"#,
            br#"{"slug":"-x"}"#,
            br#"{"slug":"a b"}"#,
            b"not json",
        ] {
            assert_eq!(slug_of(bad), None, "{}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn an_answer_needs_a_token_and_an_expiry_ahead() {
        let good = br#"{"token":"ghs_x","expires_at":"2026-10-20T08:46:07Z"}"#;
        assert_eq!(
            exchanged(good, 1_792_485_966),
            Some(("ghs_x".to_string(), 1_792_485_967))
        );
        assert_eq!(exchanged(good, 1_792_485_967), None);
        for bad in [
            &br#"{"message":"Bad credentials"}"#[..],
            br#"{"token":"ghs_x"}"#,
            br#"{"token":"ghs_x","expires_at":"soon"}"#,
            br#"{"token":"","expires_at":"2099-01-01T00:00:00Z"}"#,
            br#"{"token":7,"expires_at":"2099-01-01T00:00:00Z"}"#,
            br#"{"expires_at":"2099-01-01T00:00:00Z"}"#,
            b"not json",
        ] {
            assert_eq!(exchanged(bad, 0), None, "{}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn a_kept_token_lasts_until_the_time_asked_for() {
        let d = dir("kept");
        assert_eq!(kept(&d.0, 100), None);
        put(&d, "ghs_kept\n", "100\n");
        assert_eq!(kept(&d.0, 100).as_deref(), Some("ghs_kept"));
        assert_eq!(kept(&d.0, 101), None);
    }

    #[test]
    fn a_kept_token_with_a_bad_expiry_or_no_text_is_none() {
        let d = dir("bad");
        for (token, until) in [
            ("ghs_kept\n", ""),
            ("ghs_kept\n", "soon\n"),
            ("ghs_kept\n", "-5\n"),
            ("ghs_kept\n", "1234567890123\n"),
            ("\n", "100\n"),
        ] {
            put(&d, token, until);
            assert_eq!(kept(&d.0, 0), None, "{token:?} {until:?}");
        }
        fs::remove_file(d.0.join("token")).unwrap();
        fs::write(d.0.join("token.expires"), "100\n").unwrap();
        assert_eq!(kept(&d.0, 0), None);
    }

    #[test]
    fn a_new_token_is_moved_over_the_old_with_mode_600() {
        let d = dir("keep");
        put(&d, "ghs_old\n", "1\n");
        let old = fs::metadata(d.0.join("token")).unwrap().ino();
        assert!(keep(&d.0, "ghs_new", 2_000_000_000));
        assert_eq!(fs::read_to_string(d.0.join("token")).unwrap(), "ghs_new\n");
        assert_eq!(
            fs::read_to_string(d.0.join("token.expires")).unwrap(),
            "2000000000\n"
        );
        assert_ne!(fs::metadata(d.0.join("token")).unwrap().ino(), old);
        for name in ["token", "token.expires"] {
            let mode = fs::metadata(d.0.join(name)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
        }
        assert!(!d.0.join("token.new").exists());
        assert!(!d.0.join("token.expires.new").exists());
    }

    #[test]
    fn a_token_that_cannot_be_moved_leaves_nothing_beside_it() {
        let d = dir("nomove");
        fs::create_dir(d.0.join("token")).unwrap();
        assert!(!keep(&d.0, "ghs_new", 2_000_000_000));
        assert!(!d.0.join("token.new").exists());
        assert!(!d.0.join("token.expires.new").exists());
        assert!(!d.0.join("token.expires").exists());
    }

    #[test]
    fn a_held_lock_is_waited_for_and_given_up_on() {
        let d = dir("lock");
        let open = || {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(d.0.join("lock"))
                .unwrap()
        };
        let (held, waiting) = (open(), open());
        assert!(wait_for(&held, Duration::ZERO));
        assert!(!wait_for(&waiting, Duration::from_millis(100)));
        held.unlock().unwrap();
        assert!(wait_for(&waiting, Duration::ZERO));
    }

    #[test]
    fn the_repository_is_found_by_its_whole_name() {
        let answer = br#"{"repositories":[{"full_name":"acme/widget"}]}"#;
        assert!(reaches(answer, "acme/widget"));
        assert!(!reaches(answer, "acme/other"));
        assert!(!reaches(br#"{"repositories":[]}"#, "acme/widget"));
        assert!(!reaches(b"not json", "acme/widget"));
    }

    #[test]
    fn check_is_the_one_argument_and_an_empty_word_is_none() {
        let words_of =
            |given: &[&str]| words(&given.iter().map(|w| w.to_string()).collect::<Vec<_>>());
        assert_eq!(words_of(&[]), Ok(false));
        assert_eq!(words_of(&[""]), Ok(false));
        assert_eq!(words_of(&["--check"]), Ok(true));
        assert_eq!(words_of(&["--print"]), Err(64));
        assert_eq!(words_of(&["--check", "extra"]), Err(64));
        assert_eq!(words_of(&["", "x"]), Err(64));
    }
}
