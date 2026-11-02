//! tehas setup, after libexec/setup has found or built this console
//! (src/usage/setup.txt says what the command does as a whole).
//!
//!   console setup run <libexec dir> <euid> <bash> [options of tehas setup]...
//!
//! First the settings, the checks and the files. Reads what the options leave
//! out from the instance's files, then asks on a terminal. Checks every value
//! and refuses a setting that is there and differs, unless --force, before it
//! calls anything. Then checks GitHub and the endpoint, makes the control
//! issue, and writes instance.env, models.json, ~/.config/tehas/env and the
//! app's key. The app's token is made by src/github.rs. Nothing is written
//! before every check passed. What this half says goes to standard error.
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
use tehas_core::settings::load::{self, Vars};

const USAGE: &str = "usage: tehas setup [--repo owner/name] [--labellers \"a b\"] [--token-file f|- | --app-id id --app-installation id --app-key-file f] [--endpoint url] [--model id] [--model-key-file f|-] [--provider name] [--control-issue n] [--no-build] [--force]";

const CONTROL_BODY: &str = "The control issue of this repository's tehas factory. The label tehas:pause here stops it starting work. Keep this issue open; it is never worked on.";

/// The host git authenticates to; HELPER_KEY is its helper's config key.
const GIT_HOST: &str = "github.com";

/// tehas_require_instance_settings.
const INSTANCE_SETTINGS: [&str; 5] = [
    "TEHAS_REPO",
    "TEHAS_CONTROL_ISSUE",
    "TEHAS_ALLOWED_LABELERS",
    "PI_ENSEMBLE_SUBAGENT_MODEL",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER",
];

enum Fail {
    Usage,
    Bad(String),
    Die(String),
    /// The status, with what there was to say said.
    Stop(u8),
}
use Fail::{Bad, Die, Stop};

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

/// What the files leave for the steps. `bot` is the app's login and ID.
struct Written {
    build: bool,
    bot: Option<(String, String)>,
}

fn files(dir: &Path, args: &[String]) -> Result<Written, Fail> {
    let mut g = parse(args)?;
    let app = !(g.app_id.is_empty() && g.app_inst.is_empty() && g.app_key_file.is_empty());
    if app {
        if g.app_id.is_empty() || g.app_inst.is_empty() || g.app_key_file.is_empty() {
            say("--app-id, --app-installation and --app-key-file go together");
            return Err(Fail::Usage);
        }
        if !g.token_file.is_empty() {
            say("an app or --token-file, not both");
            return Err(Fail::Usage);
        }
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
    let stdin_used = [&g.token_file, &g.key_file]
        .iter()
        .filter(|f| f.as_str() == "-")
        .count();
    if stdin_used > 1 {
        return Err(Bad(
            "only one of --token-file and --model-key-file can read standard input".into(),
        ));
    }
    let tty = stdin_used == 0 && io::stdin().is_terminal();
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

    let mut token = String::new();
    if app {
        if !is_number(&g.app_id) {
            return Err(Bad(format!("the app ID is a number, not: {}", g.app_id)));
        }
        if !is_number(&g.app_inst) {
            return Err(Bad(format!(
                "the installation ID is a number, not: {}",
                g.app_inst
            )));
        }
        let readable = fs::File::open(&g.app_key_file).is_ok()
            && fs::metadata(&g.app_key_file).is_ok_and(|m| m.len() > 0);
        if !readable {
            return Err(Bad(format!(
                "cannot read the app's key from {}",
                g.app_key_file
            )));
        }
    } else {
        token = if g.token_file.is_empty() {
            value_in(&env_file, "GH_TOKEN")
        } else {
            read_secret(&g.token_file, "the token")?
        };
        if token.is_empty() && tty {
            token = ask("The bot's GitHub token", true);
        }
        if token.is_empty() {
            return Err(Bad("no token: give --token-file <file> or --token-file - and the token on standard input".into()));
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
    if app {
        conflicts.differs(&inst, "TEHAS_GITHUB_APP_ID", &g.app_id);
        conflicts.differs(&inst, "TEHAS_GITHUB_APP_INSTALLATION", &g.app_inst);
        if app_key.exists() && fs::read(&app_key).ok() != fs::read(&g.app_key_file).ok() {
            conflicts
                .list
                .push(format!("the app's key in {}", app_key.display()));
        }
    } else {
        conflicts.differs(&env_file, "GH_TOKEN", &token);
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

    // GitHub and the endpoint.
    let mut bot = None;
    if app {
        // Checked with the given key: nothing is installed yet. The minted
        // token is kept in no file.
        let given_app = github::App {
            id: g.app_id.clone(),
            installation: g.app_inst.clone(),
            key: PathBuf::from(&g.app_key_file),
        };
        let mint = given_app.mint().ok_or_else(|| {
            Die("the app's key could not make a token; nothing was written".into())
        })?;
        let mint: Value = serde_json::from_slice(&mint).unwrap_or_default();
        token = mint["token"].as_str().unwrap_or_default().to_string();
        if token.is_empty() {
            return Err(Die(
                "GitHub's answer for the app's token held no token; nothing was written".into(),
            ));
        }
        for p in ["contents", "issues", "pull_requests"] {
            if mint["permissions"][p] != "write" {
                return Err(Die(format!("the app's installation may not write {p} (it needs contents, issues and pull_requests: write); nothing was written")));
            }
        }
        let slug = given_app
            .slug()
            .filter(|s| is_slug_or_login(s))
            .ok_or_else(|| Die("GitHub did not say the app's name; nothing was written".into()))?;
        let login = format!("{slug}[bot]");
        let repos = gh(
            &token,
            &[
                "api",
                "--paginate",
                "installation/repositories",
                "--jq",
                ".repositories[].full_name",
            ],
        )
        .unwrap_or_default();
        if !repos.lines().any(|r| r.eq_ignore_ascii_case(&g.repo)) {
            return Err(Die(format!(
                "the app's installation {} does not list {}; nothing was written",
                g.app_inst, g.repo
            )));
        }
        say(&format!(
            "the app {slug} is installed on {} and may write to it",
            g.repo
        ));
        let id = gh(&token, &["api", &format!("users/{login}"), "--jq", ".id"])
            .filter(|id| is_number(id))
            .ok_or_else(|| {
                Die(format!(
                    "GitHub did not give the ID of {login}; nothing was written"
                ))
            })?;
        conflicts.differs(&inst, "TEHAS_BOT_LOGIN", &login);
        conflicts.refuse()?;
        bot = Some((login, id));
    } else {
        let push = gh(
            &token,
            &[
                "api",
                &format!("repos/{}", g.repo),
                "--jq",
                ".permissions.push",
            ],
        )
        .unwrap_or_default();
        if push != "true" {
            let push = if push.is_empty() { "unknown" } else { &push };
            return Err(Die(format!(
                "the token cannot push to {} (permissions.push is {push}); nothing was written",
                g.repo
            )));
        }
        say(&format!("the token can push to {}", g.repo));
    }

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
    let made = |d: &Path| {
        DirBuilder::new().recursive(true).mode(0o700).create(d)?;
        fs::set_permissions(d, fs::Permissions::from_mode(0o700))
    };
    made(dir)
        .and_then(|_| made(&config))
        .map_err(|_| Die(format!("could not make {}", dir.display())))?;
    if !inst.exists() {
        write_new(
            &inst,
            "# Written by tehas setup: the settings this instance must have. The\n# optional ones are in instances/example/optional.env of the release.\n\n",
        )?;
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
    if let Some((login, _)) = &bot {
        if fs::read(&app_key).ok() != fs::read(&g.app_key_file).ok() {
            install_key(Path::new(&g.app_key_file), &app_key).map_err(|_| {
                Die(format!(
                    "could not install the app's key at {}",
                    app_key.display()
                ))
            })?;
            // A kept token was made for the app before this one.
            let kept = tehas_home.join("state/github/token");
            let _ = fs::remove_file(&kept);
            let _ = fs::remove_file(kept.with_extension("expires"));
            say(&format!("the app's key installed at {}", app_key.display()));
        }
        let _ = fs::set_permissions(&app_key, fs::Permissions::from_mode(0o600));
        set(&inst, "TEHAS_GITHUB_APP_ID", &g.app_id, false)?;
        set(&inst, "TEHAS_GITHUB_APP_INSTALLATION", &g.app_inst, false)?;
        set(&inst, "TEHAS_BOT_LOGIN", login, false)?;
    } else {
        set(&env_file, "GH_TOKEN", &token, false)?;
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
        bot,
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

/// lib/github-app.sh, tehas_github_app_env: gh and git act as the app, through
/// the shim the install puts in place.
fn act_as_the_app(vars: &mut Vars) {
    if vars.text("TEHAS_GITHUB_APP_ID").is_empty() {
        return;
    }
    let shim = format!("{}/shim", vars.text("TEHAS_HOME"));
    let path = vars.text("PATH");
    vars.unset(b"GH_TOKEN");
    if !format!(":{path}:").contains(&format!(":{shim}:")) {
        vars.set("PATH", format!("{shim}:{path}"));
    }
    vars.set("GIT_CONFIG_COUNT", "2");
    vars.set("GIT_CONFIG_KEY_0", HELPER_KEY);
    vars.set("GIT_CONFIG_VALUE_0", "");
    vars.set("GIT_CONFIG_KEY_1", HELPER_KEY);
    vars.set(
        "GIT_CONFIG_VALUE_1",
        format!("!{shim}/gh auth git-credential"),
    );
}

/// The token just written. The file is shell, sourced as host/env.sh sources
/// it, and hands the token back on standard output, never on a command line.
fn written_token(vars: &Vars, bash: &str) -> Result<String, Fail> {
    let file = format!("{}/.config/tehas/env", vars.text("HOME"));
    let done = program(vars, bash)
        .args(["-c", r#". "$0" && printf '%s' "${GH_TOKEN:-}""#, &file])
        .env_remove("BASH_ENV")
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output();
    match done {
        Ok(done) if done.status.success() => Ok(String::from_utf8_lossy(&done.stdout)
            .trim_end_matches('\n')
            .to_string()),
        _ => Err(Die(format!("could not read {file}"))),
    }
}

/// How many entries `git config --get-all` listed, one line each.
fn helpers_listed(values: &str) -> usize {
    values.trim_end_matches('\n').split('\n').count()
}

/// Before the clone and before the identity: with no helper for github.com a
/// host cannot read a private product's remote with plain git, and the fetches
/// of tehas dispatch, update, status and preview stop at a prompt.
fn give_git_the_helper(vars: &Vars) -> Result<(), Fail> {
    // Neither stream is printed: the value is a command an operator wrote and
    // may hold a token, and git's words name the file and can quote a value.
    // git says nothing and exits 1 when the key is not set, so a word on
    // standard error is a file git refused. gh would write over it, so this is
    // a settings problem. The read covers what git reads from this directory;
    // the product's checkout is not made yet.
    let read = program(vars, "git")
        .args(["config", "--get-all", HELPER_KEY])
        .stdin(Stdio::inherit())
        .output();
    let (rc, values, words) = match read {
        Ok(done) => (status_of(Ok(done.status)), done.stdout, done.stderr),
        Err(_) => (127, Vec::new(), b"no git".to_vec()),
    };
    if !String::from_utf8_lossy(&words).trim().is_empty() {
        return Err(Bad(format!("could not read git's credential helper for {GIT_HOST} (git exit {rc}); fix git's config and run tehas setup again 78")));
    }
    // gitcredentials ends a list at an empty value, so a chain of nothing but
    // empty entries is no helper.
    let values = String::from_utf8_lossy(&values);
    if !values.trim().is_empty() {
        let n = helpers_listed(&values);
        tell(&format!(
            "git's helper for {GIT_HOST} is kept ({n} in git's config)"
        ));
        return Ok(());
    }
    // No terminal to prompt on: setup may be run from a unit.
    let rc = status_of(
        program(vars, "gh")
            .args(["auth", "setup-git", "--hostname", GIT_HOST])
            .env("GIT_TERMINAL_PROMPT", "0")
            .stderr(Stdio::null())
            .status(),
    );
    if rc != 0 {
        return Err(Die(format!(
            "gh auth setup-git failed for {GIT_HOST} (exit {rc}); it cannot give git a credential helper"
        )));
    }
    tell(&format!("git uses gh's credential helper for {GIT_HOST}"));
    Ok(())
}

/// The login and the ID in what `gh api user` said, as `read` splits a line.
fn who(said: &str) -> Option<(String, String)> {
    let line = said.lines().next()?.trim();
    let (login, id) = line.split_once([' ', '\t']).unwrap_or((line, ""));
    (!login.is_empty()).then(|| (login.to_string(), id.trim().to_string()))
}

/// An installation token cannot ask /user who it is: an app's bot comes from
/// the files.
fn the_bot(vars: &Vars, bot: &Option<(String, String)>) -> Result<(String, String), Fail> {
    if let Some(bot) = bot {
        return Ok(bot.clone());
    }
    who(&asked(
        program(vars, "gh")
            .args(["api", "user", "--jq", "[.login,.id]|@tsv"])
            .stderr(Stdio::null()),
    ))
    .ok_or_else(|| Die("gh api user did not say who the bot is".into()))
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

fn clone_the_product(vars: &Vars) -> Result<(), Fail> {
    let (repo, work) = (vars.text("TEHAS_REPO"), vars.text("TEHAS_WORK_REPO"));
    if Path::new(&work).join(".git").is_dir() {
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
    let check = libexec.join("check-product");
    if runnable(&check) {
        step(
            "tehas check-product --sandbox",
            program(vars, check).arg("--sandbox"),
        )?;
    } else {
        tell("check-product is not installed, so the product's gates were not run in the sandbox");
    }
    Ok(())
}

/// Steps 3 to 7 of the text. Each load is this process's, so install.sh still
/// builds and judges its own console.
fn steps(libexec: &Path, shell: &Shell, vars: &mut Vars, done: &Written) -> Result<(), Fail> {
    // From here gh acts as the app through the shim, as every later command on
    // the host does. Without one it acts with the token just written.
    if done.bot.is_some() {
        shell.load(vars)?;
        act_as_the_app(vars);
    } else {
        let token = written_token(vars, shell.bash)?;
        vars.set("GH_TOKEN", token);
    }
    give_git_the_helper(vars)?;
    let (login, id) = the_bot(vars, &done.bot)?;
    give_git_an_identity(vars, &login, &id)?;

    shell.load(vars)?;
    if let Some(line) = missing_setting(vars) {
        eprintln!("tehas: {line}");
        return Err(Stop(78));
    }
    let home = vars.text("TEHAS_HOME");
    let install = libexec.join("../install.sh");
    let has_install = runnable(&install);
    // The app's gh and helper are the install's; the install below reads the
    // clone.
    let no_shim = !runnable(&Path::new(&home).join("shim/gh"));
    if done.bot.is_some() && no_shim && has_install {
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

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("console-setup-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
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
    fn the_token_comes_out_of_the_file_on_no_command_line() {
        let d = scratch("token");
        fs::create_dir_all(d.join(".config/tehas")).unwrap();
        fs::write(d.join(".config/tehas/env"), "GH_TOKEN=ghp_x\n").unwrap();
        let mut vars = Vars::new([(
            b"PATH".to_vec(),
            std::env::var_os("PATH").unwrap().into_encoded_bytes(),
        )]);
        vars.set("HOME", d.display().to_string());
        assert_eq!(written_token(&vars, "bash").ok().unwrap(), "ghp_x");
        fs::write(d.join(".config/tehas/env"), "if\n").unwrap();
        let Err(Die(why)) = written_token(&vars, "bash") else {
            panic!("a file that is no shell was read");
        };
        assert_eq!(
            why,
            format!("could not read {}/.config/tehas/env", d.display())
        );
    }

    #[test]
    fn the_app_gets_its_shim_and_helper_and_loses_a_token() {
        let mut vars = Vars::new([(b"PATH".to_vec(), b"/bin".to_vec())]);
        vars.set("TEHAS_HOME", "/h");
        vars.set("GH_TOKEN", "old");
        act_as_the_app(&mut vars);
        assert_eq!(vars.text("GH_TOKEN"), "old", "no app, no change");
        assert!(!vars.is_set("GIT_CONFIG_COUNT"));

        vars.set("TEHAS_GITHUB_APP_ID", "7");
        act_as_the_app(&mut vars);
        assert!(!vars.is_set("GH_TOKEN"));
        assert_eq!(vars.text("PATH"), "/h/shim:/bin");
        assert_eq!(vars.text("GIT_CONFIG_COUNT"), "2");
        assert_eq!(vars.text("GIT_CONFIG_KEY_0"), HELPER_KEY);
        assert_eq!(vars.text("GIT_CONFIG_VALUE_0"), "");
        assert_eq!(vars.text("GIT_CONFIG_KEY_1"), HELPER_KEY);
        assert_eq!(
            vars.text("GIT_CONFIG_VALUE_1"),
            "!/h/shim/gh auth git-credential"
        );
        act_as_the_app(&mut vars);
        assert_eq!(vars.text("PATH"), "/h/shim:/bin", "once on PATH");
    }

    #[test]
    fn the_helper_key_names_the_host() {
        assert_eq!(HELPER_KEY, format!("credential.https://{GIT_HOST}.helper"));
    }

    #[test]
    fn an_empty_entry_counts_as_a_line_of_the_list() {
        assert_eq!(helpers_listed("\n!gh auth git-credential\n"), 2);
        assert_eq!(helpers_listed("!store\n"), 1);
    }

    #[test]
    fn the_bot_is_the_first_two_words_of_the_first_line() {
        let got = |s: &str| who(s);
        assert_eq!(
            got("tehas-bot\t4242\n"),
            Some(("tehas-bot".into(), "4242".into()))
        );
        assert_eq!(got("tehas-bot"), Some(("tehas-bot".into(), String::new())));
        assert_eq!(got(""), None);
        assert_eq!(got("\n"), None);
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
