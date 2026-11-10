//! tehas setup, after libexec/setup has found or built this console
//! (src/usage/setup.txt says what the command does as a whole).
//!
//!   console setup run <libexec dir> <euid> <bash> [options of tehas setup]...
//!
//! First the settings, the checks and the files. Reads what the options leave
//! out from the instance's files, then asks on a terminal. Checks every value
//! and refuses a setting that is there and differs, unless --force, before it
//! calls anything. Then checks the endpoint and the GitHub App, makes the
//! control issue, and writes instance.env, models.json, ~/.config/tehas/env,
//! the app's key and its login. The app's token is made by src/github.rs.
//! Nothing is written before every check passed, except what --new-app
//! writes as soon as GitHub made the app (setup/manifest.rs): its key and
//! TEHAS_GITHUB_APP_ID. What this half says goes to standard error.
//!
//! Then the steps: git's helper and identity, the clone, the install, the
//! labels, the sandbox, the units. What they say goes to standard output, a
//! failure to standard error. `<libexec dir>` is where the command stands and
//! `<euid>` and `<bash>` are its shell's, as for `console settings load`.
//!
//! Exit: 0 done; 1 a check or a step failed (a step's own status when it has
//! one); 64 usage; 78 a setting is missing, malformed or differs from the one
//! there without --force, or git's config could not be read.

use crate::forge::runnable;
use crate::github;
use crate::settings;
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use tehas_core::sandbox::HELPER_KEY;
use tehas_core::settings::{
    load::{self, Vars},
    INSTANCE_SETTINGS,
};

const USAGE: &str = "usage: tehas setup [--repo owner/name] [--labellers \"a b\"] [--new-app [--app-owner login] [--app-name name] | --app-id id --app-key-file f] [--endpoint url] [--model id] [--model-key-file f|-] [--provider name] [--control-issue n] [--no-build] [--force]";

const CONTROL_BODY: &str = "The control issue of this repository's tehas factory. The label tehas:pause here stops it starting work. Keep this issue open; it is never worked on.";

/// The host git authenticates to; HELPER_KEY is its helper's config key.
const GIT_HOST: &str = "github.com";

enum Fail {
    Usage,
    Bad(String),
    Die(String),
    /// The status, with what there was to say said.
    Stop(u8),
}
use Fail::{Bad, Die, Stop};

mod manifest;
mod product;

fn say(what: &str) {
    eprintln!("tehas setup: {what}");
}

/// For a person reading standard output, which may be gone.
fn tell(what: &str) {
    let _ = writeln!(io::stdout(), "tehas setup: {what}");
}

pub fn main(given: &[String]) -> u8 {
    let [verb, libexec, uid, bash, rest @ ..] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    if verb != "run" {
        eprintln!("{USAGE}");
        return 64;
    }
    let shell = Shell { uid, bash };
    let mut vars = settings::this_environment();
    let dir = settings::instance_dir(&vars);
    let done = files(Path::new(&dir), rest)
        .and_then(|written| steps(Path::new(libexec), &shell, &mut vars, &written));
    match done {
        Ok(()) => 0,
        Err(Stop(status)) => status,
        Err(Fail::Usage) => {
            eprintln!("{USAGE}");
            64
        }
        Err(Bad(m)) => {
            say(&m);
            78
        }
        Err(Die(m)) => {
            say(&m);
            1
        }
    }
}

#[derive(Default)]
struct Given {
    repo: String,
    labellers: String,
    token_file: String,
    app_id: String,
    app_inst: String,
    app_key_file: String,
    new_app: bool,
    app_owner: String,
    app_name: String,
    endpoint: String,
    model: String,
    key_file: String,
    provider: String,
    control: String,
    build: bool,
    force: bool,
}

fn parse(args: &[String]) -> Result<Given, Fail> {
    let mut g = Given {
        build: true,
        ..Given::default()
    };
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let slot = match flag.as_str() {
            "--no-build" => {
                g.build = false;
                continue;
            }
            "--force" => {
                g.force = true;
                continue;
            }
            "--new-app" => {
                g.new_app = true;
                continue;
            }
            "--app-owner" => &mut g.app_owner,
            "--app-name" => &mut g.app_name,
            "--repo" => &mut g.repo,
            "--labellers" => &mut g.labellers,
            "--token-file" => &mut g.token_file,
            "--app-id" => &mut g.app_id,
            "--app-installation" => &mut g.app_inst,
            "--app-key-file" => &mut g.app_key_file,
            "--endpoint" => &mut g.endpoint,
            "--model" => &mut g.model,
            "--model-key-file" => &mut g.key_file,
            "--provider" => &mut g.provider,
            "--control-issue" => &mut g.control,
            _ => return Err(Fail::Usage),
        };
        *slot = args.next().ok_or(Fail::Usage)?.clone();
    }
    Ok(g)
}

/// The value a KEY=value file has for a key, quotes taken off; empty when
/// the file or the key is not there. The reading itself is the core's, so
/// `tehas disk` and `tehas setup` take the same value of one file.
fn value_in(file: &Path, key: &str) -> String {
    let Ok(text) = fs::read_to_string(file) else {
        return String::new();
    };
    tehas_core::env_file::value(&text, key).unwrap_or_default()
}

fn all(s: &str, ok: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.chars().all(ok)
}
fn is_number(s: &str) -> bool {
    all(s, |c| c.is_ascii_digit()) && !s.starts_with('0')
}
fn is_repo(s: &str) -> bool {
    let part = |p: &str| all(p, |c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    matches!(s.split_once('/'), Some((o, n)) if part(o) && part(n))
}
fn is_logins(s: &str) -> bool {
    s.split(' ')
        .all(|l| all(l, |c| c.is_ascii_alphanumeric() || c == '-'))
}
fn is_endpoint(s: &str) -> bool {
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"));
    matches!(rest, Some(r) if all(r, |c| !c.is_whitespace()))
}
fn is_provider(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && all(s, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
        })
}
fn is_model(s: &str) -> bool {
    all(s, |c| c.is_ascii_alphanumeric() || "._:/-".contains(c))
}
fn is_slug_or_login(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphanumeric())
        && all(s, |c| c.is_ascii_alphanumeric() || c == '-')
}

fn key_name(provider: &str) -> String {
    format!(
        "TEHAS_MODEL_KEY_{}",
        provider.to_ascii_uppercase().replace('-', "_")
    )
}

fn ask(prompt: &str, secret: bool) -> String {
    eprint!("{prompt}: ");
    let quiet = secret && stty("-echo");
    let mut answer = String::new();
    let _ = io::stdin().lock().read_line(&mut answer);
    if quiet {
        stty("echo");
    }
    if secret {
        eprintln!();
    }
    answer.trim_end_matches(['\n', '\r']).to_string()
}

fn stty(mode: &str) -> bool {
    Command::new("stty")
        .arg(mode)
        .stdin(Stdio::inherit())
        .status()
        .is_ok_and(|s| s.success())
}

/// The first line of a file, or of standard input for "-".
fn read_secret(from: &str, what: &str) -> Result<String, Fail> {
    let mut line = String::new();
    if from == "-" {
        let _ = io::stdin().lock().read_line(&mut line);
    } else {
        let file =
            fs::File::open(from).map_err(|_| Bad(format!("cannot read {what} from {from}")))?;
        let _ = io::BufReader::new(file).read_line(&mut line);
    }
    let line = line.trim_end_matches('\n').to_string();
    if line.is_empty() {
        let source = if from == "-" { "standard input" } else { from };
        return Err(Bad(format!("{what} in {source} is empty")));
    }
    Ok(line)
}

fn read_json(file: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(file).ok()?).ok()
}

fn base_url(models: &Value, provider: &str) -> String {
    models["providers"][provider]["baseUrl"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// gh with the token in its environment; its standard output, trimmed, when
/// it exits 0.
fn gh(token: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("gh")
        .args(args)
        .env("GH_TOKEN", token)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The ids of the models the endpoint lists. The key goes to curl on
/// standard input, never on its command line.
fn model_ids(endpoint: &str, key: &str) -> Vec<String> {
    let mut config = "silent\nshow-error\nfail\nmax-time = 30\n".to_string();
    if !key.is_empty() {
        config.push_str(&format!("header = \"Authorization: Bearer {key}\"\n"));
    }
    let Ok(mut child) = Command::new("curl")
        .args(["-K", "-", &format!("{endpoint}/models")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    else {
        return Vec::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let Ok(out) = child.wait_with_output() else {
        return Vec::new();
    };
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    answer["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().map(str::to_string))
        .collect()
}

struct Conflicts {
    list: Vec<String>,
    force: bool,
}

impl Conflicts {
    fn differs(&mut self, file: &Path, key: &str, new: &str) {
        let old = value_in(file, key);
        if !old.is_empty() && old != new {
            self.list.push(format!("{key} in {}", file.display()));
        }
    }
    fn refuse(&self) -> Result<(), Fail> {
        if self.list.is_empty() || self.force {
            return Ok(());
        }
        for c in &self.list {
            say(&format!("differs from what is there: {c}"));
        }
        Err(Bad(
            "nothing was changed; run again with --force to replace it".into(),
        ))
    }
}

/// What the files leave for the steps: the app's login and its user's ID.
struct Written {
    build: bool,
    bot: (String, String),
}

/// The words of a refused way to give the factory an account, as the
/// settings say them.
const NO_LONGER_USED: &str = "is no longer used: the factory acts as a GitHub App. Run tehas setup --new-app or give --app-id and --app-key-file.";

/// Which app the factory acts as.
enum Way {
    /// --app-id and --app-key-file: the key is installed when the checks pass.
    Given { id: String, key: PathBuf },
    /// The app instance.env names, its key installed.
    Configured { id: String },
    /// --new-app: made in the owner's browser.
    New { owner: String, name: String },
}

/// GitHub allows 34 characters in an app's name.
fn is_app_name(s: &str) -> bool {
    s.len() <= 34
        && s.starts_with(|c: char| c.is_ascii_alphanumeric())
        && all(s, |c| c.is_ascii_alphanumeric() || " ._-".contains(c))
}

/// A directory of this run for the app's lookups before its settings are
/// written, removed when the run is done with it.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch, Fail> {
        let word = manifest::new_state().map_err(|e| Die(format!("no random source: {e}")))?;
        let dir = std::env::temp_dir().join(format!("tehas-setup.{word}"));
        DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|e| Die(format!("could not make {}: {e}", dir.display())))?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn made_dir(d: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(d)?;
    fs::set_permissions(d, fs::Permissions::from_mode(0o700))
}

const INSTANCE_HEADER: &str = "# Written by tehas setup: the settings this instance must have. The\n# optional ones are in instances/example/optional.env of the release.\n\n";

/// The app's settings page, where a new key is made.
fn app_settings(owner: &str, is_org: bool, slug: &str) -> String {
    if is_org {
        format!("https://github.com/organizations/{owner}/settings/apps/{slug}")
    } else {
        format!("https://github.com/settings/apps/{slug}")
    }
}

/// Steps 1 to 6 of "The wizard": the manifest goes to GitHub in the owner's
/// browser, the code comes back pasted, and GitHub's conversion gives the
/// app's ID and key. The key and TEHAS_GITHUB_APP_ID are written together,
/// as soon as the app is made: a code works once, and a second run then finds
/// the app and only checks it. Then waits for the app to be installed.
fn make_app(
    owner: &str,
    name: &str,
    repo: &str,
    paths: (&Path, &Path, &Path),
) -> Result<github::App, Fail> {
    let (dir, config, app_key) = paths;
    let is_org = github::is_org(owner).map_err(|why| {
        Die(format!(
            "could not ask GitHub whether {owner} is an organisation ({why}); nothing was written"
        ))
    })?;
    let owner_url = format!("https://github.com/{owner}");
    let manifest = manifest::json(name, &owner_url, &owner_url);
    let action = manifest::form_url(owner, is_org);
    let state = manifest::new_state().map_err(|e| Die(format!("no random source: {e}")))?;
    made_dir(dir)
        .and_then(|_| made_dir(config))
        .map_err(|_| Die(format!("could not make {}", config.display())))?;
    manifest::key_can_go(app_key).map_err(|e| {
        Bad(format!(
            "the new app's key cannot go to {} ({e}); no app was made",
            app_key.display()
        ))
    })?;
    let page_file = config.join("new-app.html");
    let page = manifest::page(&manifest, &action, &state);
    let kept_page = manifest::write_page(&page_file, &page);
    if let Err(e) = &kept_page {
        say(&format!("could not write {}: {e}", page_file.display()));
    }
    let account = if is_org {
        "organisation"
    } else {
        "user account"
    };
    eprintln!();
    eprintln!("To make the GitHub App {name} for the {account} {owner}, paste this link into your browser's address bar:");
    eprintln!();
    eprintln!("{}", manifest::data_link(&manifest, &action, &state));
    eprintln!();
    if kept_page.is_ok() {
        eprintln!(
            "If the browser will not open it: {} on this host is the same page. Copy it to the machine your browser runs on and open it there.",
            page_file.display()
        );
    }
    eprintln!("GitHub shows the app's permissions. Create it, and GitHub opens {owner_url}?code=...: copy that address.");
    let pasted = ask("The address GitHub opened, or the code in it", false);
    let code = manifest::checked_code(&pasted, &state).map_err(|why| {
        Bad(format!(
            "{why}; no app was made, run tehas setup --new-app again"
        ))
    })?;
    let answer = github::convert_manifest(&code).map_err(|why| {
        Die(format!(
            "GitHub did not make the app from the code ({why}); a code lasts one hour and works once, run tehas setup --new-app again"
        ))
    })?;
    let (id, slug) = keep_new_app(&answer, dir, app_key, |slug| {
        app_settings(owner, is_org, slug)
    })?;
    if kept_page.is_ok() {
        let _ = fs::remove_file(&page_file);
    }
    say(&format!(
        "made the GitHub App {slug}: TEHAS_GITHUB_APP_ID={id} written to {}, its key to {}",
        dir.join("instance.env").display(),
        app_key.display()
    ));
    let install = format!("https://github.com/apps/{slug}/installations/new");
    eprintln!();
    eprintln!("Install the app on {owner}: {install}");
    eprintln!("All repositories, or pick {repo} and the other repositories the factory works in.");
    eprintln!("Waiting up to 10 minutes for the installation.");
    let app = github::App {
        id: id.clone(),
        key: app_key.to_path_buf(),
    };
    let started = std::time::Instant::now();
    loop {
        if app.installations().is_some_and(|n| n > 0) {
            break;
        }
        if started.elapsed() >= INSTALL_WAIT {
            return Err(Die(format!(
                "the app {slug} is not installed after 10 minutes. Install it at {install} and run tehas setup again: it finds app {id} and checks it"
            )));
        }
        std::thread::sleep(INSTALL_POLL);
    }
    say(&format!("the app {slug} is installed"));
    Ok(app)
}

/// GitHub's answer to the conversion, kept: the key at `app_key` (a new file,
/// mode 600) and TEHAS_GITHUB_APP_ID in the instance, in one step. The key is
/// never printed. The app's ID and slug.
fn keep_new_app(
    answer: &[u8],
    dir: &Path,
    app_key: &Path,
    settings_page: impl Fn(&str) -> String,
) -> Result<(String, String), Fail> {
    let made = manifest::made(answer).ok_or_else(|| {
        Die("GitHub's answer for the new app held no ID, name and key; find the app among your GitHub Apps, make a private key for it and run tehas setup --app-id <id> --app-key-file <key>".to_string())
    })?;
    let manifest::Made { id, slug, pem } = made;
    manifest::write_key(app_key, &pem).map_err(|e| {
        Die(format!(
            "the app {slug} (ID {id}) was made, but its key could not be written to {}: {e}. Make a new private key at {} and run tehas setup --app-id {id} --app-key-file <key>",
            app_key.display(),
            settings_page(&slug)
        ))
    })?;
    drop(pem);
    let inst = dir.join("instance.env");
    if !inst.exists() {
        write_new(&inst, INSTANCE_HEADER)?;
    }
    put(&inst, "TEHAS_GITHUB_APP_ID", &id, false).map_err(|_| {
        Die(format!(
            "could not write {}; the app's ID is {id}",
            inst.display()
        ))
    })?;
    let _ = fs::set_permissions(&inst, fs::Permissions::from_mode(0o600));
    Ok((id, slug))
}

const INSTALL_WAIT: std::time::Duration = std::time::Duration::from_secs(600);
const INSTALL_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// The settings besides TEHAS_REPO that name a repository the factory works in.
const OTHER_REPOS: [&str; 4] = [
    "TEHAS_IMPROVE_REPO",
    "TEHAS_IMPROVE_RUKAS_REPO",
    "TEHAS_REVIEW_REPO",
    "TEHAS_RELEASE_REPO",
];

/// The app is installed on the repository and may write what the factory
/// writes; the other repositories the instance names are reached or named.
/// Lookups are kept in `scratch`, not in the factory's state. The bot's login,
/// its user's ID and a token of the installation, which is kept in no file.
fn check_app(
    app: &github::App,
    repo: &str,
    inst: &Path,
    scratch: &Path,
    after: &str,
) -> Result<(String, String, String), Fail> {
    let installation = app
        .installation(repo, scratch)
        .ok_or_else(|| Die(format!("the app cannot act on {repo}; {after}")))?;
    let mint = app
        .mint(&installation, None)
        .ok_or_else(|| Die(format!("the app's key could not make a token; {after}")))?;
    let mint: Value = serde_json::from_slice(&mint).unwrap_or_default();
    let token = mint["token"].as_str().unwrap_or_default().to_string();
    if token.is_empty() {
        return Err(Die(format!(
            "GitHub's answer for the app's token held no token; {after}"
        )));
    }
    for p in ["contents", "issues", "pull_requests"] {
        if mint["permissions"][p] != "write" {
            return Err(Die(format!("the app's installation may not write {p} (it needs contents, issues and pull_requests: write); {after}")));
        }
    }
    let login = app
        .login(scratch)
        .filter(|l| is_slug_or_login(l.trim_end_matches("[bot]")))
        .ok_or_else(|| Die(format!("GitHub did not say the app's name; {after}")))?;
    let slug = login.trim_end_matches("[bot]");
    say(&format!(
        "the app {slug} is installed on {repo} and may write to it"
    ));
    for name in OTHER_REPOS {
        let other = value_in(inst, name);
        if !is_repo(&other) || other.eq_ignore_ascii_case(repo) {
            continue;
        }
        match app.reaches(&other) {
            Ok(()) => say(&format!("the app reaches {other} ({name})")),
            Err(why) => say(&format!(
                "the app does not reach {other} ({name}: {why}); install it there at https://github.com/apps/{slug}/installations/new"
            )),
        }
    }
    let id = gh(&token, &["api", &format!("users/{login}"), "--jq", ".id"])
        .filter(|id| is_number(id))
        .ok_or_else(|| Die(format!("GitHub did not give the ID of {login}; {after}")))?;
    Ok((login, id, token))
}

fn files(dir: &Path, args: &[String]) -> Result<Written, Fail> {
    let mut g = parse(args)?;
    if !g.token_file.is_empty() {
        return Err(Bad(format!("--token-file {NO_LONGER_USED}")));
    }
    if !g.app_inst.is_empty() {
        say("--app-installation is no longer used: the app's installation on the repository is looked up; the value is ignored");
    }
    let given_app = !(g.app_id.is_empty() && g.app_key_file.is_empty());
    if given_app && (g.app_id.is_empty() || g.app_key_file.is_empty()) {
        say("--app-id and --app-key-file go together");
        return Err(Fail::Usage);
    }
    if given_app && g.new_app {
        say("--new-app makes an app; --app-id and --app-key-file name one. Not both");
        return Err(Fail::Usage);
    }
    if !g.new_app && !(g.app_owner.is_empty() && g.app_name.is_empty()) {
        say("--app-owner and --app-name go with --new-app");
        return Err(Fail::Usage);
    }

    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let config = home.join(".config/tehas");
    let env_file = config.join("env");
    let app_key = config.join("github-app.pem");
    let tehas_home = std::env::var_os("TEHAS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("tehas"));
    let inst = dir.join("instance.env");
    let models = dir.join("models.json");

    // What is asked for: the options, then the files, then the terminal.
    let fill = |v: &mut String, key: &str| {
        if v.is_empty() {
            *v = value_in(&inst, key);
        }
    };
    fill(&mut g.repo, "TEHAS_REPO");
    fill(&mut g.labellers, "TEHAS_ALLOWED_LABELERS");
    fill(&mut g.control, "TEHAS_CONTROL_ISSUE");
    fill(&mut g.provider, "PI_ENSEMBLE_SUBAGENT_PROVIDER");
    if g.provider.is_empty() {
        g.provider = "main".into();
    }
    fill(&mut g.model, "PI_ENSEMBLE_SUBAGENT_MODEL");
    if g.endpoint.is_empty() {
        if let Some(m) = read_json(&models) {
            g.endpoint = base_url(&m, &g.provider);
        }
    }
    let kept_id = value_in(&inst, "TEHAS_GITHUB_APP_ID");
    let configured = !kept_id.is_empty() && app_key.exists();
    if g.key_file == "-" && g.new_app {
        return Err(Bad(
            "--new-app reads GitHub's code on standard input: give --model-key-file a file".into(),
        ));
    }
    let tty = g.key_file != "-" && io::stdin().is_terminal();
    if tty {
        if g.repo.is_empty() {
            g.repo = ask("Product repository (owner/name)", false);
        }
        if g.labellers.is_empty() {
            g.labellers = ask("Logins whose labels count, space separated", false);
        }
        if g.endpoint.is_empty() {
            g.endpoint = ask("Model endpoint, with its /v1", false);
        }
        if !given_app && !g.new_app && !configured {
            let answer = ask(
                "The factory's GitHub App: new to make one, or the ID of one you have",
                false,
            );
            if answer == "new" {
                g.new_app = true;
            } else {
                g.app_id = answer;
                g.app_key_file = ask("The path of that app's private key", false);
            }
        }
    }
    if g.repo.is_empty() {
        return Err(Bad("no repository: give --repo owner/name".into()));
    }
    if g.labellers.is_empty() {
        return Err(Bad("no labellers: give --labellers \"<login> ...\"".into()));
    }
    if g.endpoint.is_empty() {
        return Err(Bad("no model endpoint: give --endpoint <url>".into()));
    }

    if !is_repo(&g.repo) {
        return Err(Bad(format!(
            "the repository is owner/name, not: {}",
            g.repo
        )));
    }
    if !is_logins(&g.labellers) {
        return Err(Bad(format!(
            "the labellers are GitHub logins separated by spaces, not: {}",
            g.labellers
        )));
    }
    if !is_endpoint(&g.endpoint) {
        return Err(Bad(format!(
            "the endpoint is an http or https address, not: {}",
            g.endpoint
        )));
    }
    if g.endpoint.ends_with('/') {
        g.endpoint.pop();
    }
    if !is_provider(&g.provider) {
        return Err(Bad(format!(
            "the provider is a lowercase name, not: {}",
            g.provider
        )));
    }
    if !g.control.is_empty() && !is_number(&g.control) {
        return Err(Bad(format!(
            "the control issue is a number, not: {}",
            g.control
        )));
    }
    if !g.model.is_empty() && !is_model(&g.model) {
        return Err(Bad(format!("not a model id: {}", g.model)));
    }

    // Which app. A second app is never made over one the instance has.
    let way = if !g.app_id.is_empty() {
        Way::Given {
            id: g.app_id.clone(),
            key: PathBuf::from(&g.app_key_file),
        }
    } else if configured {
        if g.new_app {
            say(&format!(
                "instance.env names the app {kept_id} and its key is at {}: it is checked, no new app is made",
                app_key.display()
            ));
        }
        Way::Configured {
            id: kept_id.clone(),
        }
    } else if !kept_id.is_empty() {
        return Err(Bad(format!(
            "instance.env names the app {kept_id}, and its key is not at {}: give --app-id {kept_id} --app-key-file <key>",
            app_key.display()
        )));
    } else if app_key.exists() {
        return Err(Bad(format!(
            "a key is at {} and instance.env names no app: give --app-id <id> --app-key-file {}; --new-app does not make a second app",
            app_key.display(),
            app_key.display()
        )));
    } else if g.new_app {
        let (repo_owner, repo_name) = g.repo.split_once('/').unwrap_or_default();
        let owner = if g.app_owner.is_empty() {
            repo_owner.to_string()
        } else {
            g.app_owner.clone()
        };
        let name = if g.app_name.is_empty() {
            format!("{repo_name}-factory")
        } else {
            g.app_name.clone()
        };
        if !is_slug_or_login(&owner) {
            return Err(Bad(format!(
                "the app's owner is a GitHub login, not: {owner}"
            )));
        }
        if !is_app_name(&name) {
            return Err(Bad(format!(
                "the app's name is up to 34 letters, digits, spaces, dots, dashes and underscores, not: {name}; give --app-name"
            )));
        }
        Way::New { owner, name }
    } else {
        return Err(Bad(
            "no GitHub App: give --new-app to make one, or --app-id <id> --app-key-file <key>"
                .into(),
        ));
    };
    if let Way::Given { id, key } = &way {
        if !is_number(id) {
            return Err(Bad(format!("the app ID is a number, not: {id}")));
        }
        let readable = fs::File::open(key).is_ok() && fs::metadata(key).is_ok_and(|m| m.len() > 0);
        if !readable {
            return Err(Bad(format!(
                "cannot read the app's key from {}",
                key.display()
            )));
        }
    }

    let key_name = key_name(&g.provider);
    let key = if g.key_file.is_empty() {
        value_in(&env_file, &key_name)
    } else {
        read_secret(&g.key_file, "the model key")?
    };

    // Nothing that is there is replaced without --force.
    let mut conflicts = Conflicts {
        list: Vec::new(),
        force: g.force,
    };
    conflicts.differs(&inst, "TEHAS_REPO", &g.repo);
    conflicts.differs(&inst, "TEHAS_ALLOWED_LABELERS", &g.labellers);
    if !g.control.is_empty() {
        conflicts.differs(&inst, "TEHAS_CONTROL_ISSUE", &g.control);
    }
    conflicts.differs(&inst, "PI_ENSEMBLE_SUBAGENT_PROVIDER", &g.provider);
    if !g.model.is_empty() {
        conflicts.differs(&inst, "PI_ENSEMBLE_SUBAGENT_MODEL", &g.model);
    }
    if let Way::Given { id, key } = &way {
        conflicts.differs(&inst, "TEHAS_GITHUB_APP_ID", id);
        if app_key.exists() && fs::read(&app_key).ok() != fs::read(key).ok() {
            conflicts
                .list
                .push(format!("the app's key in {}", app_key.display()));
        }
    }
    if !key.is_empty() {
        conflicts.differs(&env_file, &key_name, &key);
    }
    let old_models = if fs::File::open(&models).is_ok() {
        let m =
            read_json(&models).ok_or_else(|| Bad(format!("{} is not JSON", models.display())))?;
        let old = base_url(&m, &g.provider);
        if !old.is_empty() && old != g.endpoint {
            conflicts.list.push(format!(
                "the baseUrl of {} in {}",
                g.provider,
                models.display()
            ));
        }
        Some(m)
    } else {
        None
    };
    conflicts.refuse()?;

    // The endpoint, before an app is made for a factory that has no model.
    let ids = model_ids(&g.endpoint, &key);
    if ids.is_empty() {
        return Err(Die(format!(
            "{}/models answered with no models; nothing was written",
            g.endpoint
        )));
    }
    if g.model.is_empty() {
        g.model = if ids.len() == 1 {
            ids[0].clone()
        } else if tty {
            eprintln!("The endpoint offers:");
            for (n, id) in ids.iter().enumerate() {
                eprintln!("{:>2}. {id}", n + 1);
            }
            let pick = ask("Which one (number)", false);
            if !is_number(&pick) {
                return Err(Bad(format!("not a number from the list: {pick}")));
            }
            let n: usize = pick.parse().unwrap_or(usize::MAX);
            ids.get(n - 1)
                .cloned()
                .ok_or_else(|| Bad("no model picked".into()))?
        } else {
            return Err(Bad(format!(
                "the endpoint offers several models, give --model with one of: {}",
                ids.join(" ")
            )));
        };
    }
    if !ids.contains(&g.model) {
        return Err(Die(format!(
            "{} does not offer {}; it offers: {}",
            g.endpoint,
            g.model,
            ids.join(" ")
        )));
    }
    say(&format!("the endpoint offers {}", g.model));

    // GitHub. A given key is checked where it is: nothing is installed yet.
    let scratch = Scratch::new()?;
    let (app, after) = match &way {
        Way::Given { id, key } => (
            github::App {
                id: id.clone(),
                key: key.clone(),
            },
            "nothing was written".to_string(),
        ),
        Way::Configured { id } => (
            github::App {
                id: id.clone(),
                key: app_key.clone(),
            },
            "nothing was written".to_string(),
        ),
        Way::New { owner, name } => (
            make_app(owner, name, &g.repo, (dir, &config, &app_key))?,
            "the app's ID and key are written; fix that and run tehas setup again".to_string(),
        ),
    };
    let (login, bot_id, token) = check_app(&app, &g.repo, &inst, &scratch.0, &after)?;
    drop(scratch);

    if g.control.is_empty() {
        let url = gh(
            &token,
            &[
                "issue",
                "create",
                "--repo",
                &g.repo,
                "--title",
                "tehas control",
                "--body",
                CONTROL_BODY,
            ],
        )
        .ok_or_else(|| {
            Die(format!(
                "could not make the control issue in {}; nothing was written",
                g.repo
            ))
        })?;
        let n = url.rsplit('/').next().unwrap_or_default();
        if !is_number(n) {
            return Err(Die(format!(
                "gh answered {url} for the new control issue; nothing was written"
            )));
        }
        g.control = n.to_string();
        say(&format!("made the control issue #{}", g.control));
    }

    // The files.
    made_dir(dir)
        .and_then(|_| made_dir(&config))
        .map_err(|_| Die(format!("could not make {}", dir.display())))?;
    if !inst.exists() {
        write_new(&inst, INSTANCE_HEADER)?;
    }
    let set = |file: &Path, k: &str, v: &str, quoted: bool| -> Result<(), Fail> {
        if put(file, k, v, quoted)
            .map_err(|_| Die(format!("could not write {}", file.display())))?
        {
            // A secret is named, never printed.
            if file == env_file {
                say(&format!("{k} written to {}", file.display()));
            } else {
                say(&format!(
                    "{} written to {}",
                    line(k, v, quoted),
                    file.display()
                ));
            }
        }
        Ok(())
    };
    set(&inst, "TEHAS_REPO", &g.repo, false)?;
    set(&inst, "TEHAS_CONTROL_ISSUE", &g.control, false)?;
    set(&inst, "TEHAS_ALLOWED_LABELERS", &g.labellers, true)?;
    set(&inst, "PI_ENSEMBLE_SUBAGENT_PROVIDER", &g.provider, false)?;
    set(&inst, "PI_ENSEMBLE_SUBAGENT_MODEL", &g.model, false)?;
    let _ = fs::set_permissions(&inst, fs::Permissions::from_mode(0o600));
    let lookups = tehas_home.join("state/github");
    let mut stale: &[&str] = if kept_id == app.id {
        &[]
    } else {
        &["token", "token.expires", "installation", "login"]
    };
    if let Way::Given { key, .. } = &way {
        if fs::read(&app_key).ok() != fs::read(key).ok() {
            install_key(key, &app_key).map_err(|_| {
                Die(format!(
                    "could not install the app's key at {}",
                    app_key.display()
                ))
            })?;
            say(&format!("the app's key installed at {}", app_key.display()));
            if stale.is_empty() {
                stale = &["token", "token.expires"];
            }
        }
    }
    let _ = fs::set_permissions(&app_key, fs::Permissions::from_mode(0o600));
    // What was kept for another app, or made with the key before this one.
    for name in stale {
        let _ = fs::remove_file(lookups.join(name));
    }
    set(&inst, "TEHAS_GITHUB_APP_ID", &app.id, false)?;
    // The first poll knows the bot from this file.
    let installed = github::App {
        id: app.id.clone(),
        key: app_key.clone(),
    };
    let kept_login = fs::read(lookups.join("login")).unwrap_or_default();
    if String::from_utf8_lossy(&kept_login).trim_end() != login {
        let _ = fs::remove_file(lookups.join("login"));
    }
    if installed.login(&lookups).as_deref() != Some(login.as_str()) {
        return Err(Die(format!(
            "could not keep the app's login in {}",
            lookups.display()
        )));
    }
    if !key.is_empty() {
        set(&env_file, &key_name, &key, false)?;
    }
    if env_file.exists() {
        let _ = fs::set_permissions(&env_file, fs::Permissions::from_mode(0o600));
    }

    let api_key = if key.is_empty() {
        "unused".to_string()
    } else {
        format!("${key_name}")
    };
    let new = with_provider(
        old_models.clone().unwrap_or_else(|| json!({})),
        &g.provider,
        &g.endpoint,
        &api_key,
        &g.model,
    );
    if old_models.as_ref() != Some(&new) {
        let text = serde_json::to_string_pretty(&new).unwrap_or_default() + "\n";
        write_file(&models, &text)
            .map_err(|_| Die(format!("could not write {}", models.display())))?;
        say(&format!(
            "{} names {} at {} with {}",
            models.display(),
            g.provider,
            g.endpoint,
            g.model
        ));
    }

    Ok(Written {
        build: g.build,
        bot: (login, bot_id),
    })
}

/// The shell of libexec/setup, which a load asks for.
struct Shell<'a> {
    uid: &'a str,
    bash: &'a str,
}

impl Shell<'_> {
    /// tehas_load || exit 78.
    fn load(&self, vars: &mut Vars) -> Result<(), Fail> {
        settings::load_into(vars, self.uid, self.bash).map_err(|line| {
            eprintln!("tehas: {line}");
            Stop(78)
        })
    }
}

/// A program under `vars` and nothing else. A name without a slash is looked
/// for on the PATH of `vars`.
fn program(vars: &Vars, name: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(name);
    command.env_clear().envs(
        vars.iter()
            .map(|(k, v)| (OsStr::from_bytes(k), OsStr::from_bytes(v))),
    );
    command
}

/// The status a shell gives what it ran.
fn status_of(done: io::Result<ExitStatus>) -> u8 {
    match done {
        Ok(status) => status
            .code()
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
        Err(e) if e.kind() == io::ErrorKind::NotFound => 127,
        Err(_) => 126,
    }
}

fn step(name: &str, command: &mut Command) -> Result<(), Fail> {
    match status_of(command.status()) {
        0 => Ok(()),
        rc => {
            say(&failed(name, rc));
            Err(Stop(rc))
        }
    }
}

/// The status closes the line, as it always has.
fn failed(name: &str, rc: u8) -> String {
    format!("{name} failed (exit {rc}); fix it and run tehas setup again {rc}")
}

/// The command's standard output, whatever its status; standard input and
/// standard error stay the caller's to give.
fn asked(command: &mut Command) -> String {
    let said = command.stdin(Stdio::inherit()).output();
    let said = said.map_or(Vec::new(), |done| done.stdout);
    String::from_utf8_lossy(&said)
        .trim_end_matches('\n')
        .to_string()
}

fn git_global(vars: &Vars, key: &str) -> String {
    asked(
        program(vars, "git")
            .args(["config", "--global", key])
            .stderr(Stdio::inherit()),
    )
}

/// lib/github-app.sh, tehas_github_app_env, run by bash on `vars`: gh and git
/// act as the app, through the shim the install puts in place. Only what it
/// changes comes back.
fn act_as_the_app(vars: &mut Vars, bash: &str, lib: &Path) -> Result<(), Fail> {
    const SCRIPT: &str = r#". "$0" && tehas_github_app_env && env -0"#;
    let done = program(vars, bash)
        .args(["-c", SCRIPT])
        .arg(lib)
        .env_remove("BASH_ENV")
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output();
    let done = match done {
        Ok(done) if done.status.success() => done,
        _ => return Err(Die(format!("could not run {}", lib.display()))),
    };
    let after = Vars::new(settings::pairs(&done.stdout));
    vars.unset(b"GH_TOKEN");
    for name in [
        "PATH",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_KEY_0",
        "GIT_CONFIG_VALUE_0",
        "GIT_CONFIG_KEY_1",
        "GIT_CONFIG_VALUE_1",
    ] {
        if after.is_set(name) {
            vars.set(name, after.text(name));
        }
    }
    Ok(())
}

/// Before the clone and before the identity. git's helper for github.com is
/// the app's shim, given as environment-level config by act_as_the_app: it
/// outranks every file, so nothing is written to git's files. What git reads
/// is still read once, so a file git refuses stops the run here.
fn give_git_the_helper(vars: &Vars) -> Result<(), Fail> {
    // Neither stream is printed: a helper an operator wrote in git's files may
    // hold a token, and git's words name the file and can quote a value.
    let read = program(vars, "git")
        .args(["config", "--get-all", HELPER_KEY])
        .stdin(Stdio::inherit())
        .output();
    let (rc, words) = match read {
        Ok(done) => (status_of(Ok(done.status)), done.stderr),
        Err(_) => (127, b"no git".to_vec()),
    };
    if !String::from_utf8_lossy(&words).trim().is_empty() {
        return Err(Bad(format!("could not read git's credential helper for {GIT_HOST} (git exit {rc}); fix git's config and run tehas setup again 78")));
    }
    tell(&format!(
        "git asks the app's gh for {GIT_HOST} credentials ({})",
        vars.text("GIT_CONFIG_VALUE_1").trim_start_matches('!')
    ));
    Ok(())
}

fn give_git_an_identity(vars: &Vars, login: &str, id: &str) -> Result<(), Fail> {
    let name = git_global(vars, "user.name");
    let email = git_global(vars, "user.email");
    if !name.is_empty() || !email.is_empty() {
        tell(&format!("git's identity is kept: {name} <{email}>"));
        return Ok(());
    }
    let noreply = format!("{id}+{login}@users.noreply.github.com");
    for (key, value) in [("user.name", login), ("user.email", &noreply)] {
        let set = program(vars, "git")
            .args(["config", "--global", key, value])
            .status();
        if status_of(set) != 0 {
            return Err(Die("could not set git's identity".into()));
        }
    }
    tell(&format!("git commits as {login} <{noreply}>"));
    Ok(())
}

/// tehas_require_instance_settings, on what the load left: the line for
/// standard error, without its `tehas: `, or nothing when all are good.
fn missing_setting(vars: &Vars) -> Option<String> {
    let dir = vars.text("TEHAS_INSTANCE_DIR");
    let dir = if dir.is_empty() { "?" } else { &dir };
    let bad = load::bad(vars);
    INSTANCE_SETTINGS.iter().find_map(|name| {
        if vars.text(name).trim_matches(' ').is_empty() {
            return Some(format!("{name} is not set (see {dir}/instance.env)"));
        }
        let (_, why) = bad.iter().find(|(n, _)| n == name)?;
        Some(format!(
            "{name} is not valid: {why} (see {dir}/instance.env)"
        ))
    })
}

/// TEHAS_OWNER, when set, is a login GitHub assigns on the repository:
/// GET repos/{repo}/assignees/{login} answers 204 for one, 404 for others.
fn unassignable_owner(vars: &Vars) -> Option<String> {
    let owner = vars.text("TEHAS_OWNER");
    if owner.is_empty() {
        return None;
    }
    let repo = vars.text("TEHAS_REPO");
    let asked = program(vars, "gh")
        .args([
            "api",
            &format!("repos/{repo}/assignees/{owner}"),
            "--silent",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let dir = vars.text("TEHAS_INSTANCE_DIR");
    (status_of(asked) != 0).then(|| {
        format!(
            "TEHAS_OWNER is not valid: GitHub does not say {owner} can be assigned issues in {repo} (see {dir}/instance.env)"
        )
    })
}

fn clone_the_product(vars: &Vars) -> Result<(), Fail> {
    let (repo, work) = (vars.text("TEHAS_REPO"), vars.text("TEHAS_WORK_REPO"));
    if Path::new(&work).join(".git").is_dir() {
        tell(&product::fast_forward(vars, &work));
        return Ok(());
    }
    if let Some(parent) = Path::new(&work).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let clone = program(vars, "gh")
        .args(["repo", "clone", &repo, &work, "--", "-q"])
        .status();
    if status_of(clone) != 0 {
        return Err(Die(format!("could not clone {repo} to {work}")));
    }
    tell(&format!("cloned {repo} to {work}"));
    Ok(())
}

fn the_sandbox(vars: &Vars, libexec: &Path, home: &str) -> Result<(), Fail> {
    let image = vars.text("PI_ENSEMBLE_IMAGE");
    let there = program(vars, "docker")
        .args(["image", "inspect", &image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if status_of(there) == 0 {
        tell(&format!("the sandbox image is there ({image})"));
    } else {
        step(
            "the sandbox build",
            &mut program(vars, format!("{home}/sandbox/build.sh")),
        )?;
    }
    let check = libexec.join("../bin/tehas");
    if runnable(&check) {
        step(
            "tehas check-product --sandbox",
            program(vars, check).args(["check-product", "--sandbox"]),
        )?;
    } else {
        tell("bin/tehas is not installed, so the product's gates were not run in the sandbox");
    }
    Ok(())
}

/// Steps 3 to 7 of the text. Each load is this process's, so install.sh still
/// builds and judges its own console.
fn steps(libexec: &Path, shell: &Shell, vars: &mut Vars, done: &Written) -> Result<(), Fail> {
    // From here gh acts as the app through the shim, as every later command on
    // the host does.
    shell.load(vars)?;
    act_as_the_app(vars, shell.bash, &libexec.join("../lib/github-app.sh"))?;
    give_git_the_helper(vars)?;
    let (login, id) = &done.bot;
    give_git_an_identity(vars, login, id)?;

    shell.load(vars)?;
    if let Some(line) = missing_setting(vars).or_else(|| unassignable_owner(vars)) {
        eprintln!("tehas: {line}");
        return Err(Stop(78));
    }
    let home = vars.text("TEHAS_HOME");
    let install = libexec.join("../install.sh");
    let has_install = runnable(&install);
    // The app's gh and helper are the install's; the install below reads the
    // clone.
    let no_shim = !runnable(&Path::new(&home).join("shim/gh"));
    if no_shim && has_install {
        step("install.sh", &mut program(vars, &install))?;
    }
    clone_the_product(vars)?;
    if has_install {
        step("install.sh", &mut program(vars, &install))?;
    } else {
        tell("no install.sh beside this command; run the platform's install.sh yourself");
    }
    step(
        "tehas labels",
        program(vars, format!("{home}/bin/tehas")).arg("labels"),
    )?;
    if done.build {
        the_sandbox(vars, libexec, &home)?;
    }

    step(
        "systemctl --user daemon-reload",
        program(vars, "systemctl").args(["--user", "daemon-reload"]),
    )?;
    step(
        "enabling the dispatcher",
        program(vars, "systemctl").args([
            "--user",
            "enable",
            "--now",
            "tehas-dispatch.timer",
            "tehas-dispatch.path",
        ]),
    )?;
    let docs = format!("{home}/edge/docs.compose.yaml");
    if fs::File::open(&docs).is_ok() {
        step(
            "the docs server",
            program(vars, "docker").args(["compose", "-f", &docs, "up", "-d"]),
        )?;
    }
    // One mint, the login and the reach, as every later command sees them.
    let tehasbot = Path::new(&home).join("bin/tehasbot");
    if runnable(&tehasbot) {
        step(
            "tehasbot github-token --check",
            program(vars, tehasbot).args(["github-token", "--check"]),
        )?;
    } else {
        tell("bin/tehasbot is not installed, so the app's token was not checked");
    }
    let addr = vars.text("TEHAS_HOST_ADDR");
    let addr = if addr.is_empty() { "127.0.0.1" } else { &addr };
    tell(&format!("done. The dashboard is at http://{addr}:8080/"));
    tell(&format!(
        "label a small issue in {} with tehas:ready to start the first cycle",
        vars.text("TEHAS_REPO")
    ));
    Ok(())
}

fn line(k: &str, v: &str, quoted: bool) -> String {
    if quoted {
        format!("{k}=\"{v}\"")
    } else {
        format!("{k}={v}")
    }
}

/// Set KEY=value in a file: a new key is added, one with that value is left,
/// another value is replaced, and a second line of the key goes. True when
/// the file changed.
fn put(file: &Path, k: &str, v: &str, quoted: bool) -> io::Result<bool> {
    if value_in(file, k) == v {
        return Ok(false);
    }
    let new = line(k, v, quoted);
    let text = fs::read_to_string(file).unwrap_or_default();
    let prefix = format!("{k}=");
    let is_key = |l: &str| l.strip_prefix("export ").unwrap_or(l).starts_with(&prefix);
    if text.lines().any(is_key) {
        let mut out = String::new();
        let mut done = false;
        for l in text.lines() {
            if !is_key(l) {
                out.push_str(l);
                out.push('\n');
            } else if !done {
                out.push_str(&new);
                out.push('\n');
                done = true;
            }
        }
        let tmp = file.with_extension("new");
        write_file(&tmp, &out)?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        fs::rename(&tmp, file)?;
    } else {
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(file)?;
        writeln!(f, "{new}")?;
    }
    Ok(true)
}

fn write_new(file: &Path, text: &str) -> Result<(), Fail> {
    write_file(file, text).map_err(|_| Die(format!("could not write {}", file.display())))
}

fn write_file(file: &Path, text: &str) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)?
        .write_all(text.as_bytes())
}

fn install_key(from: &Path, to: &Path) -> io::Result<()> {
    let tmp = to.with_extension("pem.new");
    let done = fs::copy(from, &tmp)
        .and_then(|_| fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)))
        .and_then(|_| fs::rename(&tmp, to));
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    done
}

/// models.json with the provider's four keys install.sh needs; it fills the
/// rest. An apiKey that is there is kept unless a model key was given.
fn with_provider(mut models: Value, p: &str, url: &str, api_key: &str, model: &str) -> Value {
    if !models.is_object() {
        models = json!({});
    }
    if !models["providers"].is_object() {
        models["providers"] = json!({});
    }
    let entry = &mut models["providers"][p];
    if !entry.is_object() {
        *entry = json!({"api": "openai-completions"});
    }
    entry["baseUrl"] = json!(url);
    let missing = entry["apiKey"].is_null() || entry["apiKey"] == false;
    if missing || api_key != "unused" {
        entry["apiKey"] = json!(api_key);
    }
    if !entry["models"].is_array() {
        entry["models"] = json!([]);
    }
    let list = entry["models"].as_array_mut().expect("an array");
    if !list.iter().any(|m| m["id"] == model) {
        list.push(json!({ "id": model }));
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new(&format!("setup-{name}"))
    }

    #[test]
    fn conversion_writes_key_600_and_id() {
        let d = scratch("conversion");
        let key = d.join("github-app.pem");
        let answer = br#"{"id":9,"slug":"acme-factory","pem":"-----BEGIN RSA PRIVATE KEY-----\nk\n-----END RSA PRIVATE KEY-----\n","client_secret":"cs"}"#;
        let (id, slug) = keep_new_app(answer, d.path(), &key, |s| s.to_string())
            .ok()
            .unwrap();
        assert_eq!((id.as_str(), slug.as_str()), ("9", "acme-factory"));
        assert!(fs::read_to_string(&key)
            .unwrap()
            .starts_with("-----BEGIN RSA"));
        assert_eq!(
            fs::metadata(&key).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let inst = d.join("instance.env");
        assert_eq!(value_in(&inst, "TEHAS_GITHUB_APP_ID"), "9");
        assert_eq!(
            fs::metadata(&inst).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!fs::read_to_string(&inst).unwrap().contains("BEGIN"));
        // A key that is there is never written over, and no ID is written.
        let other = scratch("conversion-again");
        let there = other.join("github-app.pem");
        fs::write(&there, "old key").unwrap();
        let again = keep_new_app(answer, other.path(), &there, |s| s.to_string());
        assert!(matches!(again, Err(Die(m)) if !m.contains("BEGIN")));
        assert_eq!(fs::read_to_string(&there).unwrap(), "old key");
        assert!(!other.join("instance.env").exists());
        assert!(matches!(
            keep_new_app(br#"{"id":9}"#, other.path(), &d.join("x.pem"), |s| s
                .to_string()),
            Err(Die(_))
        ));
    }

    #[test]
    fn an_app_name_is_what_github_takes() {
        assert!(is_app_name("widget-factory") && is_app_name("My factory 2"));
        assert!(!is_app_name("") && !is_app_name("-x") && !is_app_name(&"a".repeat(35)));
        assert!(!is_app_name("a<b") && !is_app_name("a\nb"));
    }

    #[test]
    fn put_adds_keeps_and_replaces_one_line() {
        let d = scratch("put");
        let f = d.join("env");
        assert!(put(&f, "A", "1", false).unwrap());
        assert!(!put(&f, "A", "1", false).unwrap());
        assert!(put(&f, "B", "x y", true).unwrap());
        fs::write(&f, "A=1\n# note\nexport A=2\nB=\"x y\"\n").unwrap();
        assert!(put(&f, "A", "3", false).unwrap());
        assert_eq!(fs::read_to_string(&f).unwrap(), "A=3\n# note\nB=\"x y\"\n");
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn the_checks_take_what_the_settings_take() {
        assert!(is_repo("acme/widget") && is_repo("a.b_c/d-e"));
        assert!(!is_repo("acme widget") && !is_repo("acme/") && !is_repo("a/b/c"));
        assert!(is_logins("alice bob") && !is_logins("alice  bob") && !is_logins(" alice"));
        assert!(is_endpoint("http://gb1:8086/v1") && !is_endpoint("gb1:8086"));
        assert!(!is_endpoint("http://") && !is_endpoint("https://a b"));
        assert!(is_provider("main") && is_provider("m-2_x") && !is_provider("Main"));
        assert!(is_number("7") && !is_number("07") && !is_number("") && !is_number("x"));
        assert!(is_model("org/m:1.5") && !is_model("a b"));
        assert_eq!(key_name("my-gpu"), "TEHAS_MODEL_KEY_MY_GPU");
    }

    #[test]
    fn options_take_a_value_and_an_unknown_one_is_usage() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let g = parse(&a(&["--repo", "a/b", "--no-build", "--force"]))
            .ok()
            .unwrap();
        assert_eq!(g.repo, "a/b");
        assert!(!g.build && g.force);
        assert!(matches!(parse(&a(&["--repo"])), Err(Fail::Usage)));
        assert!(matches!(parse(&a(&["--bogus"])), Err(Fail::Usage)));
    }

    #[test]
    fn a_shell_reads_the_status_of_what_it_ran() {
        let run = |line: &str| status_of(Command::new("sh").args(["-c", line]).status());
        assert_eq!(run("exit 3"), 3);
        assert_eq!(run("kill -TERM $$"), 143);
        assert_eq!(status_of(Command::new("/no/such/program").status()), 127);
        assert_eq!(status_of(Command::new("/").status()), 126);
    }

    #[test]
    fn a_step_that_fails_stops_with_its_status_and_says_it_twice() {
        assert_eq!(
            failed("install.sh", 78),
            "install.sh failed (exit 78); fix it and run tehas setup again 78"
        );
        let mut command = Command::new("sh");
        command.args(["-c", "exit 5"]);
        assert!(matches!(step("x", &mut command), Err(Stop(5))));
        assert!(step("x", &mut Command::new("true")).is_ok());
    }

    #[test]
    fn a_child_has_the_environment_of_the_load_and_no_other() {
        let path = std::env::var_os("PATH").unwrap().into_encoded_bytes();
        let mut vars = Vars::new([(b"PATH".to_vec(), path)]);
        vars.set("A_NAME", "a value");
        let said = asked(
            program(&vars, "sh")
                .args(["-c", "echo \"$A_NAME/${HOME:-none}\""])
                .stderr(Stdio::inherit()),
        );
        assert_eq!(said, "a value/none");
    }

    #[test]
    fn the_helper_key_names_the_host() {
        assert_eq!(HELPER_KEY, format!("credential.https://{GIT_HOST}.helper"));
    }

    #[test]
    fn the_app_gets_its_shim_and_helper_and_loses_a_token() {
        let lib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/github-app.sh");
        let mut vars = Vars::new([(b"PATH".to_vec(), b"/usr/bin:/bin".to_vec())]);
        vars.set("TEHAS_HOME", "/h");
        vars.set("GH_TOKEN", "old");
        assert!(act_as_the_app(&mut vars, "bash", &lib).is_ok());
        assert!(!vars.is_set("GH_TOKEN"));
        assert_eq!(vars.text("PATH"), "/h/shim:/usr/bin:/bin");
        assert_eq!(vars.text("GIT_CONFIG_COUNT"), "2");
        assert_eq!(vars.text("GIT_CONFIG_KEY_0"), HELPER_KEY);
        assert_eq!(vars.text("GIT_CONFIG_VALUE_0"), "");
        assert_eq!(vars.text("GIT_CONFIG_KEY_1"), HELPER_KEY);
        assert_eq!(
            vars.text("GIT_CONFIG_VALUE_1"),
            "!/h/shim/gh auth git-credential"
        );
        assert!(act_as_the_app(&mut vars, "bash", &lib).is_ok());
        assert_eq!(vars.text("PATH"), "/h/shim:/usr/bin:/bin", "once on PATH");
    }

    #[test]
    fn an_instance_setting_is_named_when_it_is_unset_or_invalid() {
        let mut vars = Vars::new([]);
        assert_eq!(
            missing_setting(&vars).unwrap(),
            "TEHAS_REPO is not set (see ?/instance.env)"
        );
        vars.set("TEHAS_INSTANCE_DIR", "/i");
        vars.set("TEHAS_REPO", "   ");
        assert_eq!(
            missing_setting(&vars).unwrap(),
            "TEHAS_REPO is not set (see /i/instance.env)"
        );
        vars.set("TEHAS_REPO", "not a repo");
        assert_eq!(
            missing_setting(&vars).unwrap(),
            "TEHAS_REPO is not valid: expected owner/name (see /i/instance.env)"
        );
        vars.set("TEHAS_REPO", "acme/widget");
        vars.set("TEHAS_CONTROL_ISSUE", "1");
        vars.set("TEHAS_ALLOWED_LABELERS", "alice");
        vars.set("PI_ENSEMBLE_SUBAGENT_MODEL", "m");
        vars.set("PI_ENSEMBLE_SUBAGENT_PROVIDER", "p");
        assert_eq!(missing_setting(&vars), None);
    }

    #[test]
    fn a_provider_is_added_with_its_four_keys() {
        let m = with_provider(json!({}), "main", "http://e/v1", "unused", "m1");
        assert_eq!(
            m,
            json!({"providers": {"main": {"api": "openai-completions", "baseUrl": "http://e/v1", "apiKey": "unused", "models": [{"id": "m1"}]}}})
        );
        assert_eq!(
            with_provider(m.clone(), "main", "http://e/v1", "unused", "m1"),
            m
        );
    }

    #[test]
    fn a_provider_that_is_there_keeps_its_key_and_models_and_others() {
        let old = json!({"providers": {
            "main": {"api": "x", "apiKey": "$OLD", "models": [{"id": "m0", "name": "zero"}], "extra": 1},
            "other": {"baseUrl": "http://o"}
        }});
        let m = with_provider(old.clone(), "main", "http://e/v1", "unused", "m1");
        assert_eq!(m["providers"]["main"]["apiKey"], "$OLD");
        assert_eq!(m["providers"]["main"]["api"], "x");
        assert_eq!(m["providers"]["main"]["extra"], 1);
        assert_eq!(
            m["providers"]["main"]["models"],
            json!([{"id": "m0", "name": "zero"}, {"id": "m1"}])
        );
        assert_eq!(m["providers"]["other"], old["providers"]["other"]);
        let k = with_provider(old, "main", "http://e/v1", "$TEHAS_MODEL_KEY_MAIN", "m0");
        assert_eq!(k["providers"]["main"]["apiKey"], "$TEHAS_MODEL_KEY_MAIN");
        assert_eq!(
            k["providers"]["main"]["models"].as_array().unwrap().len(),
            1
        );
    }
}
