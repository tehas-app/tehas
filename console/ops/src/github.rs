//! The factory's GitHub App: an installation token, made from the app's key
//! and kept fresh in one file (docs/specs/2026-10-24-github-app.md).
//!
//!   console tehasbot github-token [--cycle | --session] [--check] [--login]
//!
//! Prints a token with at least 10 minutes left, making one when the kept one
//! has less. --check says when the token runs out and whether it reaches
//! TEHAS_REPO, and never prints the token. --cycle is the cycle's token
//! (#1814): made from the same app, narrowed to TEHAS_REPO alone with
//! Contents and Pull requests write and Issues read, kept in state/github-cycle.
//! A planning turn and a dev session are given the same body in
//! state/github-session (--session), which a thread of the command keeps
//! fresh while the container runs, as `tehas-cycle renew` keeps a cycle's
//! (`renew_session`). A container is given one of those, never the app's own. `tehasbot github-token` is what
//! the gh shim, the poll and `tehas-cycle renew` ask: `console run` loads the
//! settings and comes here.
//!
//! --login prints the app's login, `<slug>[bot]`. The installation of the app
//! on TEHAS_REPO and the login are looked up at GitHub with the app's JWT and
//! kept in state/github/installation (`<owner> <id>`, looked up again for
//! another owner) and state/github/login; the cycle's token uses the same two
//! files, so a cycle's own directory holds a token and nothing else. A new
//! token asks for the login again and rewrites the file when the app was
//! renamed (`App::login_now`).
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
use std::sync::atomic::AtomicBool;
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::{settings, time};
use tehas_cycle::renew;

const USAGE: &str = "usage: tehasbot github-token [--cycle | --session] [--check] [--login]";
const ACCEPT: &str = "Accept: application/vnd.github+json";
const LEFT: i64 = renew::LEFT as i64;
const LOCK_WAIT: Duration = Duration::from_secs(30);

pub struct App {
    pub id: String,
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

/// curl's answer, when it exits 0, else why not: `HTTP <status>` or curl's own
/// words. The bearer goes on standard input.
fn curl_why(bearer: &str, flags: &[&str], url: &str) -> Result<Vec<u8>, String> {
    let config = format!("header = \"Authorization: Bearer {}\"\n", quoted(bearer));
    curl_config(&config, flags, Some(url))
}

/// curl with `config` on its standard input; the URL there too when `url`
/// is None.
fn curl_config(config: &str, flags: &[&str], url: Option<&str>) -> Result<Vec<u8>, String> {
    let mut child = Command::new("curl")
        .args(["-fsS", "--max-time", "30", "-K", "-"])
        .args(flags)
        .args(["-H", ACCEPT])
        .args(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl cannot run: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("curl did not finish: {e}"))?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let said = said.trim();
    Err(match said.rsplit_once("returned error: ") {
        Some((_, status)) => format!("HTTP {}", status.trim()),
        None if said.is_empty() => format!("curl failed ({})", out.status),
        None => said.replace('\n', " "),
    })
}

/// GitHub's answer to POST /app-manifests/{code}/conversions, which needs no
/// authentication and works once per code. The code goes to curl on standard
/// input, inside the URL.
pub fn convert_manifest(code: &str) -> Result<Vec<u8>, String> {
    let config = format!(
        "url = \"{}\"\nrequest = \"POST\"\n",
        quoted(&format!("{}/app-manifests/{code}/conversions", api()))
    );
    curl_config(&config, &[], None)
}

/// Whether a GitHub account is an organisation, from GET /users/{login}.
pub fn is_org(login: &str) -> Result<bool, String> {
    let config = format!(
        "url = \"{}\"\n",
        quoted(&format!("{}/users/{login}", api()))
    );
    let answer: Value = serde_json::from_slice(&curl_config(&config, &[], None)?)
        .map_err(|_| "its answer was not JSON".to_string())?;
    match answer["type"].as_str() {
        Some(kind) => Ok(kind == "Organization"),
        None => Err("its answer named no account type".to_string()),
    }
}

fn curl(bearer: &str, flags: &[&str], url: &str) -> Option<Vec<u8>> {
    curl_why(bearer, flags, url).ok()
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
    /// expiry and its permissions. Keeps nothing. `narrow` is the JSON body
    /// that limits the token's repositories and permissions; None asks for
    /// all the installation has.
    pub fn mint(&self, installation: &str, narrow: Option<&str>) -> Option<Vec<u8>> {
        let jwt = self.jwt()?;
        let url = format!("{}/app/installations/{installation}/access_tokens", api());
        let mut flags = vec!["-X", "POST"];
        if let Some(body) = narrow {
            flags.extend(["-H", "Content-Type: application/json", "--data", body]);
        }
        let answer = curl(&jwt, &flags, &url);
        if answer.is_none() {
            eprintln!("tehas: GitHub did not give the app a token for installation {installation}");
        }
        answer
    }

    /// The id of the app's installation on `repo`, asked of GitHub when the
    /// one kept in `dir/installation` (`<owner> <id>`) is for another owner.
    /// None, with the repo and the link to install the app said on standard
    /// error, when the app is not installed there.
    pub fn installation(&self, repo: &str, dir: &Path) -> Option<String> {
        let (owner, _) = repo.split_once('/')?;
        let file = dir.join("installation");
        if let Some(id) = text(&file)
            .as_deref()
            .and_then(|t| t.split_once(' '))
            .and_then(|(kept_owner, id)| {
                (kept_owner == owner && digits(id)).then(|| id.to_string())
            })
        {
            return Some(id);
        }
        let jwt = self.jwt()?;
        let found = curl_why(&jwt, &[], &format!("{}/repos/{repo}/installation", api())).and_then(
            |answer| {
                serde_json::from_slice::<Value>(&answer)
                    .ok()
                    .and_then(|answer| answer["id"].as_u64())
                    .ok_or_else(|| "its answer held no installation id".to_string())
            },
        );
        let id = match found {
            Ok(id) => id,
            Err(why) => {
                let slug = (why == "HTTP 404")
                    .then(|| self.login(dir))
                    .flatten()
                    .map(|l| l.trim_end_matches("[bot]").to_string());
                eprintln!("{}", not_found(repo, slug.as_deref(), &why));
                return None;
            }
        };
        if !keep_line(dir, "installation", &format!("{owner} {id}")) {
            eprintln!(
                "tehas: the app's installation could not be kept in {}",
                dir.display()
            );
        }
        Some(id.to_string())
    }

    /// The login of the app, `<slug>[bot]`, kept in `dir/login` for the
    /// settings to read. Asked of GitHub when the file is absent or malformed.
    pub fn login(&self, dir: &Path) -> Option<String> {
        if let Some(kept) = fs::read(dir.join("login"))
            .ok()
            .and_then(|t| settings::login_of(&t))
        {
            return Some(kept);
        }
        let login = format!("{}[bot]", self.slug()?);
        if !keep_line(dir, "login", &login) {
            eprintln!(
                "tehas: the app's login could not be kept in {}",
                dir.display()
            );
        }
        Some(login)
    }

    /// The login of the app as GitHub says it now, kept in `dir/login` when it
    /// is not the kept one. An app can be renamed, and a login kept from
    /// before then names no account: a cycle that lists its pull requests by
    /// it finds none. The kept login when GitHub does not answer. Asked when a
    /// token is made, so about once an hour.
    pub fn login_now(&self, dir: &Path) -> Option<String> {
        let kept = fs::read(dir.join("login"))
            .ok()
            .and_then(|t| settings::login_of(&t));
        let Some(slug) = self.slug() else {
            return kept;
        };
        let login = format!("{slug}[bot]");
        if kept.as_deref() == Some(login.as_str()) {
            return Some(login);
        }
        if let Some(old) = &kept {
            eprintln!("tehas: the GitHub App's login is now {login}, not {old}");
            renamed(old, &login);
            if !crate::ids::carry_beside(dir, old, &login) {
                return Some(login);
            }
        }
        if !keep_line(dir, "login", &login) {
            eprintln!(
                "tehas: the app's login could not be kept in {}",
                dir.display()
            );
        }
        Some(login)
    }

    /// How many installations the app has, from GET /app/installations, or
    /// why GitHub did not say (`HTTP <status>` for a refusal).
    pub fn installations(&self) -> Result<usize, String> {
        let jwt = self.jwt().ok_or("the app's token could not be signed")?;
        let answer = curl_why(&jwt, &[], &format!("{}/app/installations", api()))?;
        serde_json::from_slice::<Value>(&answer)
            .ok()
            .and_then(|a| a.as_array().map(Vec::len))
            .ok_or_else(|| "its answer held no list of installations".to_string())
    }

    /// Whether the app is installed on `repo`, from GET
    /// /repos/{repo}/installation; why not when it is not.
    pub fn reaches(&self, repo: &str) -> Result<(), String> {
        let jwt = self.jwt().ok_or("the app's token could not be signed")?;
        let answer = curl_why(&jwt, &[], &format!("{}/repos/{repo}/installation", api()))?;
        serde_json::from_slice::<Value>(&answer)
            .ok()
            .and_then(|a| a["id"].as_u64())
            .map(|_| ())
            .ok_or_else(|| "its answer held no installation id".to_string())
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

fn digits(word: &str) -> bool {
    !word.is_empty() && word.len() <= 20 && word.bytes().all(|c| c.is_ascii_digit())
}

/// What is said when the installation on `repo` was not found: that the app
/// is not installed there for a 404 alone, and for any other failure that it
/// could not be looked up, and why.
fn not_found(repo: &str, slug: Option<&str>, why: &str) -> String {
    if why == "HTTP 404" {
        not_installed(repo, slug)
    } else {
        format!("tehas: could not find the GitHub App's installation on {repo}: {why}")
    }
}

/// What is said when the app has no installation on `repo`.
fn not_installed(repo: &str, slug: Option<&str>) -> String {
    match slug {
        Some(slug) => format!(
            "tehas: the GitHub App is not installed on {repo}; install it at https://github.com/apps/{slug}/installations/new"
        ),
        None => format!("tehas: the GitHub App is not installed on {repo}; install the app on it"),
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

/// One line kept as `dir/name`, mode 600, moved into place whole.
fn keep_line(dir: &Path, name: &str, line: &str) -> bool {
    let new = dir.join(format!("{name}.new"));
    let kept = fs::create_dir_all(dir).is_ok()
        && write_new(&new, line)
        && fs::rename(&new, dir.join(name)).is_ok();
    if !kept {
        let _ = fs::remove_file(&new);
    }
    kept
}

/// Keeps `login` as the app's login in `dir`, as a lookup would.
pub fn keep_login(dir: &Path, login: &str) -> bool {
    keep_line(dir, "login", login)
}

/// The first poll after a fresh install should know the bot before the
/// settings are loaded again.
fn adopt(login: &str) {
    for name in ["TEHAS_BOT_LOGIN", "TEHAS_MILLWRIGHT_LOGIN"] {
        if env::var_os(name).is_none_or(|v| v.is_empty()) {
            env::set_var(name, login);
        }
    }
}

/// The rest of this process acts under the app's new login where it held the
/// old one; a login set to another account stays.
fn renamed(old: &str, new: &str) {
    for name in ["TEHAS_BOT_LOGIN", "TEHAS_MILLWRIGHT_LOGIN"] {
        if env::var(name).is_ok_and(|v| v == old) {
            env::set_var(name, new);
        }
    }
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

/// The body that narrows a cycle's token: the product repository alone, and
/// what a cycle writes. It reads issues and never writes them or their labels. None when TEHAS_REPO is not `<owner>/<name>`.
fn narrowed(repo: &str) -> Option<String> {
    let (owner, name) = repo.split_once('/')?;
    let word = |w: &str| {
        !w.is_empty()
            && w.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    };
    (word(owner) && word(name)).then(|| {
        format!(
            r#"{{"repositories":["{name}"],"permissions":{{"contents":"write","pull_requests":"write","issues":"read"}}}}"#
        )
    })
}

/// A token with at least 10 minutes left, or None with one line on standard
/// error: no key, the exchange failed or answered no token.
///
/// `lookup` is where the app's installation and login are kept; the cycle's
/// token shares the app's, state/github.
fn token(app: &App, dir: &Path, lookup: &Path, repo: &str, narrow: Option<&str>) -> Option<String> {
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
    let had_one = lookup.join("installation").exists();
    let installation = app.installation(repo, lookup)?;
    let answer = match app.mint(&installation, narrow) {
        Some(answer) => answer,
        None if had_one => {
            // The kept installation may be an old one: look it up once more.
            let _ = fs::remove_file(lookup.join("installation"));
            let installation = app.installation(repo, lookup)?;
            app.mint(&installation, narrow)?
        }
        None => return None,
    };
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
    if let Some(login) = app.login_now(lookup) {
        adopt(&login);
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

fn check(app: &App, dir: &Path, lookup: &Path, token: &str) -> u8 {
    if let Err(rc) = settings::require(&["TEHAS_REPO"]) {
        return rc;
    }
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let login = env::var("TEHAS_BOT_LOGIN").unwrap_or_default();
    let installation = text(&lookup.join("installation"))
        .and_then(|t| t.split_once(' ').map(|(_, id)| id.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    say(&format!(
        "app {}, installation {installation}, acting as {login}",
        app.id
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

/// What the arguments ask for. Said before the settings are loaded.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ask {
    pub check: bool,
    pub cycle: bool,
    pub session: bool,
    pub login: bool,
}

pub fn words(given: &[String]) -> Result<Ask, u8> {
    let mut ask = Ask::default();
    let given = match given {
        [word] if word.is_empty() => &[][..],
        _ => given,
    };
    for word in given {
        let seen = match word.as_str() {
            "--check" => std::mem::replace(&mut ask.check, true),
            "--cycle" => std::mem::replace(&mut ask.cycle, true) || ask.session,
            "--session" => std::mem::replace(&mut ask.session, true) || ask.cycle,
            "--login" => std::mem::replace(&mut ask.login, true),
            _ => true,
        };
        if seen {
            eprintln!("{USAGE}");
            return Err(64);
        }
    }
    Ok(ask)
}

const NEEDS: [&str; 3] = ["TEHAS_HOME", "TEHAS_GITHUB_APP_ID", "TEHAS_REPO"];

/// Where a planning turn's or a dev session's narrowed token is kept.
const SESSION_DIR: &str = "state/github-session";

fn kept_in(cycle: bool) -> PathBuf {
    let dir = if cycle {
        "state/github-cycle"
    } else {
        "state/github"
    };
    home_dir(dir)
}

fn home_dir(dir: &str) -> PathBuf {
    PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default()).join(dir)
}

/// The one token path of a host command: a token with at least 10 minutes
/// left, for the command to put in its environment as GH_TOKEN. The error is
/// the exit status, the reason already said on standard error.
pub fn token_env() -> Result<String, u8> {
    settings::require(&NEEDS)?;
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    token(
        &App::loaded(),
        &kept_in(false),
        &kept_in(false),
        &repo,
        None,
    )
    .ok_or(1)
}

/// The narrowed token of a planning turn's or a dev session's container, kept
/// in state/github-session: the product repository, Contents and Pull
/// requests write, Issues read. The command keeps the app's own token (see
/// `token_env`) for what it writes to the tracker itself.
pub fn session_token_env() -> Result<String, u8> {
    settings::require(&NEEDS)?;
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let Some(body) = narrowed(&repo) else {
        eprintln!("tehas: TEHAS_REPO ({repo}) is not <owner>/<name>");
        return Err(78);
    };
    token(
        &App::loaded(),
        &home_dir(SESSION_DIR),
        &kept_in(false),
        &repo,
        Some(&body),
    )
    .ok_or(1)
}

/// Keeps state/github-session fresh for the rest of this process, as
/// `tehas-cycle renew` keeps a cycle's token: a thread asks `tehasbot
/// github-token --session` every TEHAS_GITHUB_RENEW_EVERY seconds, as
/// `renew::every` reads it, the first one period after the start, and
/// `renew::RETRY` seconds after a failed ask. A planning turn and a dev session
/// run their container in this process, so the thread ends with them.
pub fn renew_session() {
    static NEVER: AtomicBool = AtomicBool::new(false);
    let every = renew::every(&env::var("TEHAS_GITHUB_RENEW_EVERY").unwrap_or_default());
    let cmd = home_dir("bin/tehasbot");
    session_renewer(cmd, every, renew::RETRY, &NEVER);
}

fn session_renewer(
    cmd: PathBuf,
    every: u64,
    retry: u64,
    stop: &'static AtomicBool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // The command has just made the token: the first ask is one period on.
        sleep(Duration::from_secs(every));
        let args = ["github-token".to_string(), "--session".to_string()];
        renew::renew(&cmd, &args, every, retry, stop, renew::epoch_seconds);
    })
}

pub fn main(given: &[String]) -> u8 {
    let ask = match words(given) {
        Ok(ask) => ask,
        Err(rc) => return rc,
    };
    if let Err(rc) = settings::require(&NEEDS) {
        return rc;
    }
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let narrow = if ask.cycle || ask.session {
        let Some(body) = narrowed(&repo) else {
            eprintln!("tehas github-token: TEHAS_REPO ({repo}) is not <owner>/<name>");
            return 78;
        };
        Some(body)
    } else {
        None
    };
    let dir = if ask.session {
        home_dir(SESSION_DIR)
    } else {
        kept_in(ask.cycle)
    };
    let lookup = kept_in(false);
    let app = App::loaded();
    if ask.login {
        let Some(login) = app.login(&lookup) else {
            return 1;
        };
        say(&login);
        return 0;
    }
    let Some(token) = token(&app, &dir, &lookup, &repo, narrow.as_deref()) else {
        return 1;
    };
    if ask.check {
        return check(&app, &dir, &lookup, &token);
    }
    say(&token);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use tehas_core::testdir::TestDir;

    fn dir(name: &str) -> TestDir {
        TestDir::new(&format!("github-{name}"))
    }

    fn put(d: &TestDir, token: &str, until: &str) {
        fs::write(d.join("token"), token).unwrap();
        fs::write(d.join("token.expires"), until).unwrap();
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
        assert_eq!(kept(d.path(), 100), None);
        put(&d, "ghs_kept\n", "100\n");
        assert_eq!(kept(d.path(), 100).as_deref(), Some("ghs_kept"));
        assert_eq!(kept(d.path(), 101), None);
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
            assert_eq!(kept(d.path(), 0), None, "{token:?} {until:?}");
        }
        fs::remove_file(d.join("token")).unwrap();
        fs::write(d.join("token.expires"), "100\n").unwrap();
        assert_eq!(kept(d.path(), 0), None);
    }

    #[test]
    fn a_new_token_is_moved_over_the_old_with_mode_600() {
        let d = dir("keep");
        put(&d, "ghs_old\n", "1\n");
        let old = fs::metadata(d.join("token")).unwrap().ino();
        assert!(keep(d.path(), "ghs_new", 2_000_000_000));
        assert_eq!(fs::read_to_string(d.join("token")).unwrap(), "ghs_new\n");
        assert_eq!(
            fs::read_to_string(d.join("token.expires")).unwrap(),
            "2000000000\n"
        );
        assert_ne!(fs::metadata(d.join("token")).unwrap().ino(), old);
        for name in ["token", "token.expires"] {
            let mode = fs::metadata(d.join(name)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
        }
        assert!(!d.join("token.new").exists());
        assert!(!d.join("token.expires.new").exists());
    }

    #[test]
    fn a_token_that_cannot_be_moved_leaves_nothing_beside_it() {
        let d = dir("nomove");
        fs::create_dir(d.join("token")).unwrap();
        assert!(!keep(d.path(), "ghs_new", 2_000_000_000));
        assert!(!d.join("token.new").exists());
        assert!(!d.join("token.expires.new").exists());
        assert!(!d.join("token.expires").exists());
    }

    #[test]
    fn a_held_lock_is_waited_for_and_given_up_on() {
        let d = dir("lock");
        let open = || {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(d.join("lock"))
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
    fn check_and_cycle_are_the_arguments_and_an_empty_word_is_none() {
        let words_of =
            |given: &[&str]| words(&given.iter().map(|w| w.to_string()).collect::<Vec<_>>());
        let ask = |check, cycle| {
            Ok(Ask {
                check,
                cycle,
                session: false,
                login: false,
            })
        };
        assert_eq!(words_of(&[]), ask(false, false));
        assert_eq!(words_of(&[""]), ask(false, false));
        assert_eq!(words_of(&["--check"]), ask(true, false));
        assert_eq!(words_of(&["--cycle"]), ask(false, true));
        assert_eq!(words_of(&["--cycle", "--check"]), ask(true, true));
        assert_eq!(words_of(&["--check", "--cycle"]), ask(true, true));
        assert_eq!(
            words_of(&["--login"]),
            Ok(Ask {
                check: false,
                cycle: false,
                session: false,
                login: true
            })
        );
        assert_eq!(
            words_of(&["--session", "--check"]),
            Ok(Ask {
                check: true,
                cycle: false,
                session: true,
                login: false
            })
        );
        assert_eq!(words_of(&["--session", "--cycle"]), Err(64));
        assert_eq!(words_of(&["--cycle", "--session"]), Err(64));
        assert_eq!(words_of(&["--login", "--login"]), Err(64));
        assert_eq!(words_of(&["--print"]), Err(64));
        assert_eq!(words_of(&["--check", "extra"]), Err(64));
        assert_eq!(words_of(&["--cycle", "--cycle"]), Err(64));
        assert_eq!(words_of(&["", "x"]), Err(64));
    }

    #[test]
    fn a_cycles_token_is_narrowed_to_the_product_and_what_a_cycle_writes() {
        let body: Value = serde_json::from_str(&narrowed("acme/widget").unwrap()).unwrap();
        assert_eq!(body["repositories"], serde_json::json!(["widget"]));
        assert_eq!(
            body["permissions"],
            serde_json::json!({"contents": "write", "pull_requests": "write", "issues": "read"})
        );
        for bad in [
            "widget",
            "",
            "/widget",
            "acme/",
            "acme/wid\"get",
            "acme/a b",
        ] {
            assert_eq!(narrowed(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_cycles_token_leaves_issues_and_their_labels_alone() {
        let body = narrowed("acme/widget").unwrap();
        assert_eq!(
            body,
            r#"{"repositories":["widget"],"permissions":{"contents":"write","pull_requests":"write","issues":"read"}}"#
        );
    }

    #[test]
    fn a_cycles_token_is_kept_apart_from_the_apps() {
        assert!(kept_in(true).ends_with("state/github-cycle"));
        assert!(kept_in(false).ends_with("state/github"));
        assert!(home_dir(SESSION_DIR).ends_with("state/github-session"));
    }

    // A turn or a session that runs past one renewal period reads a token
    // made after it started: the stand-in for tehasbot writes a new one each
    // time it is asked, as github-token --session does once the kept one is
    // near its end.
    #[test]
    fn a_session_past_one_renewal_period_reads_a_fresh_token() {
        static STOP: AtomicBool = AtomicBool::new(false);
        let d = dir("session-renew");
        let kept = d.path().join("state/github-session");
        fs::create_dir_all(&kept).unwrap();
        fs::write(kept.join("token"), "ghs_first\n").unwrap();
        let cmd = d.path().join("tehasbot");
        fs::write(
            &cmd,
            format!(
                "#!/bin/sh\n[ \"$*\" = \"github-token --session\" ] || exit 64\necho \"$*\" >>'{0}/asked'\nprintf 'ghs_%s\\n' \"$(wc -l <'{0}/asked')\" >'{1}/token.new' && mv '{1}/token.new' '{1}/token'\n",
                d.path().display(),
                kept.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&cmd, fs::Permissions::from_mode(0o755)).unwrap();
        let thread = session_renewer(cmd, 1, 1, &STOP);
        let until = Instant::now() + Duration::from_secs(10);
        let asked = || fs::read_to_string(d.path().join("asked")).unwrap_or_default();
        while asked().lines().count() < 2 && Instant::now() < until {
            sleep(Duration::from_millis(50));
        }
        STOP.store(true, std::sync::atomic::Ordering::SeqCst);
        thread.join().unwrap();
        assert!(asked().lines().count() >= 2, "{}", asked());
        let token = fs::read_to_string(kept.join("token")).unwrap();
        assert_ne!(token, "ghs_first\n");
        assert_ne!(token, "ghs_1\n");
    }

    // A fake GitHub on a local port: each route is (METHOD path, status,
    // body); every request line it gets is kept.
    struct Fake {
        url: String,
        hits: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    fn fake(routes: &[(&str, u16, &str)]) -> Fake {
        use std::io::Read;
        use std::sync::{Arc, Mutex};
        let routes: Vec<(String, u16, String)> = routes
            .iter()
            .map(|(r, s, b)| (r.to_string(), *s, b.to_string()))
            .collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).to_string();
                let mut line = head.lines().next().unwrap_or("").split(' ');
                let route = format!(
                    "{} {}",
                    line.next().unwrap_or(""),
                    line.next().unwrap_or("")
                );
                seen.lock().unwrap().push(route.clone());
                let (status, body) = routes
                    .iter()
                    .find(|(r, _, _)| *r == route)
                    .map_or((404, "{}".to_string()), |(_, s, b)| (*s, b.clone()));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Fake { url, hits }
    }

    impl Fake {
        fn count(&self, route: &str) -> usize {
            self.hits
                .lock()
                .unwrap()
                .iter()
                .filter(|h| *h == route)
                .count()
        }
    }

    // TEHAS_GITHUB_API is the process's: one test at a time holds it.
    static API: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn app_of(d: &TestDir) -> App {
        let key = d.join("app.pem");
        let made = Command::new("openssl")
            .args(["genrsa", "-out"])
            .arg(&key)
            .arg("2048")
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(made.success());
        App {
            id: "5".to_string(),
            key,
        }
    }

    fn against<T>(fake: &Fake, run: impl FnOnce() -> T) -> T {
        let _held = API.lock().unwrap_or_else(|e| e.into_inner());
        env::set_var("TEHAS_GITHUB_API", &fake.url);
        let out = run();
        env::remove_var("TEHAS_GITHUB_API");
        out
    }

    const INSTALLED: (&str, u16, &str) = (
        "GET /repos/acme/widget/installation",
        200,
        r#"{"id":77,"account":{"login":"acme"}}"#,
    );
    const SLUG: (&str, u16, &str) = ("GET /app", 200, r#"{"slug":"acme-factory"}"#);

    #[test]
    fn installation_is_looked_up_once_per_owner() {
        let d = dir("inst-once");
        let app = app_of(&d);
        let f = fake(&[INSTALLED]);
        let (a, b) = against(&f, || {
            (
                app.installation("acme/widget", d.path()),
                app.installation("acme/gadget", d.path()),
            )
        });
        assert_eq!(a.as_deref(), Some("77"));
        assert_eq!(b.as_deref(), Some("77"));
        assert_eq!(f.count("GET /repos/acme/widget/installation"), 1);
        assert_eq!(f.hits.lock().unwrap().len(), 1);
        assert_eq!(
            fs::read_to_string(d.join("installation")).unwrap(),
            "acme 77\n"
        );
        let mode = fs::metadata(d.join("installation"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn another_owner_looks_up_again() {
        let d = dir("inst-owner");
        let app = app_of(&d);
        let f = fake(&[
            INSTALLED,
            ("GET /repos/other/widget/installation", 200, r#"{"id":88}"#),
        ]);
        let (a, b) = against(&f, || {
            (
                app.installation("acme/widget", d.path()),
                app.installation("other/widget", d.path()),
            )
        });
        assert_eq!((a.as_deref(), b.as_deref()), (Some("77"), Some("88")));
        assert_eq!(f.hits.lock().unwrap().len(), 2);
        assert_eq!(
            fs::read_to_string(d.join("installation")).unwrap(),
            "other 88\n"
        );
    }

    #[test]
    fn not_installed_says_the_repo_and_the_link() {
        let d = dir("inst-404");
        let app = app_of(&d);
        let f = fake(&[SLUG]);
        let got = against(&f, || app.installation("acme/widget", d.path()));
        assert_eq!(got, None);
        assert!(!d.join("installation").exists());
        let said = not_installed("acme/widget", Some("acme-factory"));
        assert!(said.contains("acme/widget"), "{said}");
        assert!(
            said.contains("https://github.com/apps/acme-factory/installations/new"),
            "{said}"
        );
        assert!(not_installed("acme/widget", None).contains("acme/widget"));
    }

    #[test]
    fn only_a_404_says_the_app_is_not_installed() {
        let said = not_found("acme/widget", Some("acme-factory"), "HTTP 404");
        assert!(said.contains("is not installed on acme/widget"), "{said}");
        assert!(
            said.contains("/apps/acme-factory/installations/new"),
            "{said}"
        );
        for why in ["HTTP 500", "HTTP 401", "Could not resolve host: x"] {
            let said = not_found("acme/widget", None, why);
            assert!(!said.contains("not installed"), "{said}");
            assert!(said.contains("could not find"), "{said}");
            assert!(said.contains("acme/widget") && said.contains(why), "{said}");
        }
    }

    #[test]
    fn a_manifest_code_is_converted_without_auth() {
        let f = fake(&[(
            "POST /app-manifests/abc123/conversions",
            201,
            r#"{"id":9,"slug":"acme-factory"}"#,
        )]);
        let (made, again) = against(&f, || {
            (convert_manifest("abc123"), convert_manifest("other"))
        });
        assert_eq!(made, Ok(br#"{"id":9,"slug":"acme-factory"}"#.to_vec()));
        assert_eq!(again, Err("HTTP 404".to_string()));
        assert_eq!(f.count("POST /app-manifests/abc123/conversions"), 1);
    }

    #[test]
    fn an_account_is_an_organisation_or_a_user() {
        let f = fake(&[
            ("GET /users/acme", 200, r#"{"type":"Organization"}"#),
            ("GET /users/alice", 200, r#"{"type":"User"}"#),
        ]);
        let got = against(&f, || (is_org("acme"), is_org("alice"), is_org("ghost")));
        assert_eq!(got, (Ok(true), Ok(false), Err("HTTP 404".to_string())));
    }

    #[test]
    fn reach_and_installations_ask_with_the_apps_jwt() {
        let d = dir("reach");
        let app = app_of(&d);
        let f = fake(&[INSTALLED, ("GET /app/installations", 200, r#"[{"id":77}]"#)]);
        let got = against(&f, || {
            (
                app.reaches("acme/widget"),
                app.reaches("acme/other"),
                app.installations(),
            )
        });
        assert_eq!(got, (Ok(()), Err("HTTP 404".to_string()), Ok(1)));
    }

    // tehas setup stops at a refusal rather than wait ten minutes on it.
    #[test]
    fn a_refused_list_of_installations_says_why() {
        let d = dir("installations-refused");
        let app = app_of(&d);
        let f = fake(&[("GET /app/installations", 401, "{}")]);
        assert_eq!(
            against(&f, || app.installations()),
            Err("HTTP 401".to_string())
        );
    }

    #[test]
    fn curl_says_the_status_of_a_refusal() {
        let f = fake(&[("GET /ok", 200, "{}"), ("GET /down", 500, "{}")]);
        let (ok, down, gone) = against(&f, || {
            let url = |p: &str| format!("{}{p}", api());
            (
                curl_why("t", &[], &url("/ok")),
                curl_why("t", &[], &url("/down")),
                curl_why("t", &[], &url("/missing")),
            )
        });
        assert_eq!(ok, Ok(b"{}".to_vec()));
        assert_eq!(down, Err("HTTP 500".to_string()));
        assert_eq!(gone, Err("HTTP 404".to_string()));
        let dead = curl_why("t", &[], "http://127.0.0.1:1/x").unwrap_err();
        assert!(!dead.starts_with("HTTP"), "{dead}");
    }

    #[test]
    fn a_failed_mint_with_a_kept_installation_looks_it_up_once_more() {
        let d = dir("mint-stale");
        let app = app_of(&d);
        fs::write(d.join("installation"), "acme 11\n").unwrap();
        fs::write(d.join("login"), "acme-factory[bot]\n").unwrap();
        let f = fake(&[
            INSTALLED,
            (
                "POST /app/installations/77/access_tokens",
                201,
                r#"{"token":"ghs_new","expires_at":"2099-01-01T00:00:00Z"}"#,
            ),
        ]);
        let tokens = d.join("tokens");
        let got = against(&f, || token(&app, &tokens, d.path(), "acme/widget", None));
        assert_eq!(got.as_deref(), Some("ghs_new"));
        assert_eq!(f.count("POST /app/installations/11/access_tokens"), 1);
        assert_eq!(f.count("GET /repos/acme/widget/installation"), 1);
        assert_eq!(f.count("POST /app/installations/77/access_tokens"), 1);
        assert_eq!(
            fs::read_to_string(d.join("installation")).unwrap(),
            "acme 77\n"
        );
    }

    #[test]
    fn a_failed_mint_with_a_fresh_lookup_is_not_tried_again() {
        let d = dir("mint-fresh");
        let app = app_of(&d);
        fs::write(d.join("login"), "acme-factory[bot]\n").unwrap();
        let f = fake(&[INSTALLED]);
        let tokens = d.join("tokens");
        let got = against(&f, || token(&app, &tokens, d.path(), "acme/widget", None));
        assert_eq!(got, None);
        assert_eq!(f.count("GET /repos/acme/widget/installation"), 1);
        assert_eq!(f.count("POST /app/installations/77/access_tokens"), 1);
    }

    #[test]
    fn login_is_the_slug_with_bot() {
        let d = dir("login");
        let app = app_of(&d);
        let f = fake(&[SLUG]);
        let (a, b) = against(&f, || (app.login(d.path()), app.login(d.path())));
        assert_eq!(a.as_deref(), Some("acme-factory[bot]"));
        assert_eq!(b.as_deref(), Some("acme-factory[bot]"));
        assert_eq!(f.count("GET /app"), 1);
        assert_eq!(
            fs::read_to_string(d.join("login")).unwrap(),
            "acme-factory[bot]\n"
        );
        let mode = fs::metadata(d.join("login")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_first_token_looks_up_the_installation_and_the_login() {
        let d = dir("first");
        let app = app_of(&d);
        let f = fake(&[
            INSTALLED,
            SLUG,
            (
                "POST /app/installations/77/access_tokens",
                201,
                r#"{"token":"ghs_new","expires_at":"2099-01-01T00:00:00Z"}"#,
            ),
        ]);
        let _held = API.lock().unwrap_or_else(|e| e.into_inner());
        env::remove_var("TEHAS_BOT_LOGIN");
        env::set_var("TEHAS_MILLWRIGHT_LOGIN", "someone");
        env::set_var("TEHAS_GITHUB_API", &f.url);
        let tokens = d.join("tokens");
        let got = token(&app, &tokens, d.path(), "acme/widget", None);
        env::remove_var("TEHAS_GITHUB_API");
        let (bot, wright) = (
            env::var("TEHAS_BOT_LOGIN"),
            env::var("TEHAS_MILLWRIGHT_LOGIN"),
        );
        env::remove_var("TEHAS_BOT_LOGIN");
        env::remove_var("TEHAS_MILLWRIGHT_LOGIN");
        assert_eq!(got.as_deref(), Some("ghs_new"));
        assert_eq!(
            fs::read_to_string(d.join("installation")).unwrap(),
            "acme 77\n"
        );
        assert_eq!(
            fs::read_to_string(d.join("login")).unwrap(),
            "acme-factory[bot]\n"
        );
        assert_eq!(bot.as_deref(), Ok("acme-factory[bot]"));
        assert_eq!(wright.as_deref(), Ok("someone"));
    }

    const MINTED: (&str, u16, &str) = (
        "POST /app/installations/77/access_tokens",
        201,
        r#"{"token":"ghs_new","expires_at":"2099-01-01T00:00:00Z"}"#,
    );

    #[test]
    fn a_new_token_follows_a_renamed_app() {
        let d = dir("renamed");
        let app = app_of(&d);
        fs::write(d.join("installation"), "acme 77\n").unwrap();
        fs::write(d.join("login"), "acme-old[bot]\n").unwrap();
        let f = fake(&[SLUG, MINTED]);
        let _held = API.lock().unwrap_or_else(|e| e.into_inner());
        env::set_var("TEHAS_BOT_LOGIN", "acme-old[bot]");
        env::set_var("TEHAS_MILLWRIGHT_LOGIN", "someone");
        env::set_var("TEHAS_GITHUB_API", &f.url);
        let got = token(&app, &d.join("tokens"), d.path(), "acme/widget", None);
        env::remove_var("TEHAS_GITHUB_API");
        let (bot, wright) = (
            env::var("TEHAS_BOT_LOGIN"),
            env::var("TEHAS_MILLWRIGHT_LOGIN"),
        );
        env::remove_var("TEHAS_BOT_LOGIN");
        env::remove_var("TEHAS_MILLWRIGHT_LOGIN");
        assert_eq!(got.as_deref(), Some("ghs_new"));
        assert_eq!(
            fs::read_to_string(d.join("login")).unwrap(),
            "acme-factory[bot]\n"
        );
        assert_eq!(bot.as_deref(), Ok("acme-factory[bot]"));
        assert_eq!(wright.as_deref(), Ok("someone"));
    }

    #[test]
    fn a_renamed_app_keeps_its_user_id_and_tries_again_until_it_is_written() {
        let d = dir("renamed-id");
        let app = app_of(&d);
        let lookup = d.join("state/github");
        fs::create_dir_all(&lookup).unwrap();
        fs::write(lookup.join("login"), "acme-old[bot]\n").unwrap();
        let ids = d.join(tehas_core::accounts::FILE);
        fs::write(&ids, "acme-old 4242\n").unwrap();
        // A pair that cannot be written leaves the old login kept: here a
        // directory stands where the new file is written first.
        let tmp = ids.with_extension(format!("tmp{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let f = fake(&[SLUG]);
        let got = against(&f, || app.login_now(&lookup));
        assert_eq!(got.as_deref(), Some("acme-factory[bot]"));
        assert_eq!(fs::read_to_string(&ids).unwrap(), "acme-old 4242\n");
        assert_eq!(
            fs::read_to_string(lookup.join("login")).unwrap(),
            "acme-old[bot]\n"
        );
        // The next token sees the rename again and records the id first.
        fs::remove_dir_all(&tmp).unwrap();
        let got = against(&f, || app.login_now(&lookup));
        assert_eq!(got.as_deref(), Some("acme-factory[bot]"));
        assert_eq!(
            fs::read_to_string(&ids).unwrap(),
            "acme-factory 4242\nacme-old 4242\n"
        );
        assert_eq!(
            fs::read_to_string(lookup.join("login")).unwrap(),
            "acme-factory[bot]\n"
        );
    }

    #[test]
    fn a_login_github_still_says_is_not_written_again() {
        let d = dir("login-same");
        let app = app_of(&d);
        fs::write(d.join("login"), "acme-factory[bot]\n").unwrap();
        let before = fs::metadata(d.join("login")).unwrap().ino();
        let f = fake(&[SLUG]);
        let got = against(&f, || app.login_now(d.path()));
        assert_eq!(got.as_deref(), Some("acme-factory[bot]"));
        assert_eq!(f.count("GET /app"), 1);
        assert_eq!(fs::metadata(d.join("login")).unwrap().ino(), before);
    }

    #[test]
    fn the_kept_login_stands_when_github_does_not_answer() {
        let d = dir("login-down");
        let app = app_of(&d);
        fs::write(d.join("login"), "acme-old[bot]\n").unwrap();
        let f = fake(&[("GET /app", 502, "")]);
        let got = against(&f, || app.login_now(d.path()));
        assert_eq!(got.as_deref(), Some("acme-old[bot]"));
        assert_eq!(
            fs::read_to_string(d.join("login")).unwrap(),
            "acme-old[bot]\n"
        );
    }

    #[test]
    fn a_login_in_the_wrong_shape_is_looked_up_again() {
        let d = dir("login-bad");
        let app = app_of(&d);
        fs::write(d.join("login"), "acme factory\n").unwrap();
        let f = fake(&[SLUG]);
        let got = against(&f, || app.login(d.path()));
        assert_eq!(got.as_deref(), Some("acme-factory[bot]"));
        assert_eq!(f.count("GET /app"), 1);
    }
}
