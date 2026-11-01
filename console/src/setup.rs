//! The first half of tehas setup: the settings, the checks and the files
//! (libexec/setup says what the command does as a whole).
//!
//!   console setup files <instance dir> [options of tehas setup]...
//!
//! Reads what the options leave out from the instance's files, then asks on
//! a terminal. Checks every value and refuses a setting that is there and
//! differs, unless --force, before it calls anything. Then checks GitHub and
//! the endpoint, makes the control issue, and writes instance.env, models.json,
//! ~/.config/tehas/env and the app's key. The app's token is made by
//! src/github.rs. Nothing is written before every check passed.
//!
//! What it says goes to standard error. Standard output is one line for the
//! steps after it: "<build 0|1> token", or "<build 0|1> app <bot> <bot id>".
//!
//! Exit: 0 written; 1 a check failed; 64 usage; 78 a setting is missing,
//! malformed or differs from the one there without --force.

use crate::github;
use serde_json::{json, Value};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const USAGE: &str = "usage: tehas setup [--repo owner/name] [--labellers \"a b\"] [--token-file f|- | --app-id id --app-installation id --app-key-file f] [--endpoint url] [--model id] [--model-key-file f|-] [--provider name] [--control-issue n] [--no-build] [--force]";

const CONTROL_BODY: &str = "The control issue of this repository's tehas factory. The label tehas:pause here stops it starting work. Keep this issue open; it is never worked on.";

enum Fail {
    Usage,
    Bad(String),
    Die(String),
}
use Fail::{Bad, Die};

fn say(what: &str) {
    eprintln!("tehas setup: {what}");
}

pub fn main(given: &[String]) -> u8 {
    let [verb, dir, rest @ ..] = given else {
        eprintln!("{USAGE}");
        return 64;
    };
    if verb != "files" {
        eprintln!("{USAGE}");
        return 64;
    }
    match run(Path::new(dir), rest) {
        Ok(line) => {
            println!("{line}");
            0
        }
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
/// the file or the key is not there.
fn value_in(file: &Path, key: &str) -> String {
    let Ok(text) = fs::read_to_string(file) else {
        return String::new();
    };
    let prefix = format!("{key}=");
    let Some(v) = text
        .lines()
        .filter_map(|l| l.strip_prefix("export ").unwrap_or(l).strip_prefix(&prefix))
        .next_back()
    else {
        return String::new();
    };
    let v = unquote(v, '"');
    unquote(v, '\'').to_string()
}

fn unquote(v: &str, q: char) -> &str {
    match v.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
        Some(inner) if v.len() >= 2 => inner,
        _ => v,
    }
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

fn run(dir: &Path, args: &[String]) -> Result<String, Fail> {
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

    let build = u8::from(g.build);
    Ok(match bot {
        Some((login, id)) => format!("{build} app {login} {id}"),
        None => format!("{build} token"),
    })
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
    fn a_value_is_the_last_line_of_its_key_with_quotes_taken_off() {
        let d = scratch("value");
        let f = d.join("instance.env");
        fs::write(
            &f,
            "# TEHAS_REPO=no\nTEHAS_REPO=a/b\nexport TEHAS_REPO=\"c/d\"\nOTHER_TEHAS_REPO=e/f\nX='y'\nZ=\"\n",
        )
        .unwrap();
        assert_eq!(value_in(&f, "TEHAS_REPO"), "c/d");
        assert_eq!(value_in(&f, "X"), "y");
        assert_eq!(value_in(&f, "Z"), "\"");
        assert_eq!(value_in(&f, "MISSING"), "");
        assert_eq!(value_in(&d.join("none"), "TEHAS_REPO"), "");
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
