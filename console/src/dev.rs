//! `tehas dev`: an interactive Pi session on the factory host, in the sandbox
//! image, in a clone of the product that is kept between sessions.
//! src/usage/dev.txt says what a person is told.
//!
//!   console tehas dev <arguments>            one session; exits as that says
//!   console tehas dev-login "Name <email>" [-- <pi arguments>]
//!                                            the same for another account, as
//!                                            the tehas user: libexec/dev-login
//!   console dev table                        its row of the table of runs
//!
//! Every refusal that needs neither GitHub nor a write comes first. libexec/dev
//! and libexec/dev-login are the paths of this, and each execs it, so the pid,
//! the terminal and the signal dispositions are the caller's.
//!
//! The session is spawned, not exec'd: the terminal is put back after it. The
//! docker client shares this program's stdio, so it has the real terminal. The
//! clone's lock is held here for the session's life, on a file that closes on
//! exec, so the docker client does not hold it past this program.

use crate::{attach, clean, github, label};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tehas_core::lock;
use tehas_core::models;
use tehas_core::sandbox::{self, Identity, Kind, Paths, Taken};
use tehas_core::settings;
use tehas_core::slice::{self, Host, Load, Reading};

pub const USAGE: &str = include_str!("usage/dev.txt");

/// What a session cannot start without.
const NEEDS: [&str; 3] = [
    "TEHAS_REPO",
    "PI_ENSEMBLE_SUBAGENT_MODEL",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER",
];

/// What the words of the command ask for.
#[derive(Debug, Default, PartialEq)]
pub struct Words {
    pub name: String,
    pub author: String,
    pub issue: String,
    pub pi: Vec<String>,
}

fn control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

fn word_char(c: char) -> bool {
    c != '<' && c != '>' && !c.is_ascii_whitespace()
}

// Name <email>, on one line.
fn author_ok(author: &str) -> bool {
    let Some((name, mail)) = author.rsplit_once(" <") else {
        return false;
    };
    let Some((user, host)) = mail.strip_suffix('>').and_then(|mail| mail.split_once('@')) else {
        return false;
    };
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| word_char(c) && c != '@');
    !control(author)
        && !name.contains(['<', '>'])
        && name.chars().next().is_some_and(word_char)
        && name.chars().next_back().is_some_and(word_char)
        && plain(user)
        && plain(host)
}

// One plain word: it becomes a directory, a file name and a container name.
fn name_ok(name: &str) -> bool {
    !name.starts_with(['_', '-'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The words, or what to refuse them with.
fn parse(given: &[String]) -> Result<Option<Words>, String> {
    let mut words = Words {
        name: "main".to_string(),
        ..Words::default()
    };
    let mut named = false;
    let mut at = 0;
    while at < given.len() {
        let word = given[at].as_str();
        let next = given.get(at + 1).map(String::as_str);
        match word {
            "-h" | "--help" => return Ok(None),
            "--author" => {
                let Some(author) = next else {
                    return Err("--author needs a value: \"Name <email>\"".to_string());
                };
                if !author_ok(author) {
                    return Err(format!(
                        "not an author, expected \"Name <email>\": {author}"
                    ));
                }
                words.author = author.to_string();
                at += 2;
            }
            "--issue" => {
                let Some(issue) = next else {
                    return Err("--issue needs an issue number".to_string());
                };
                let digits = !issue.is_empty() && issue.bytes().all(|b| b.is_ascii_digit());
                if !digits || issue.starts_with('0') {
                    return Err(format!("not an issue number: {issue}"));
                }
                words.issue = issue.to_string();
                at += 2;
            }
            "--" => {
                words.pi = given[at + 1..].to_vec();
                break;
            }
            "" => return Err(format!("unknown argument: {word}")),
            w if w.starts_with('-') || named => return Err(format!("unknown argument: {w}")),
            w => {
                words.name = w.to_string();
                named = true;
                at += 1;
            }
        }
    }
    if !name_ok(&words.name) {
        return Err(format!(
            "not a name for a clone (letters, digits, _ and -): {}",
            words.name
        ));
    }
    Ok(Some(words))
}

// The docker line of a session, in order.
struct Session<'a> {
    name: &'a str,
    limits: &'a [String],
    security: &'a [String],
    user: &'a str,
    tree: &'a str,
    mounts: &'a [String],
    images: Option<&'a str>,
    envs: &'a [String],
    hosts: &'a [String],
    image: &'a str,
    pi: &'a [String],
}

impl Session<'_> {
    fn words(&self) -> Vec<String> {
        let mut w: Vec<String> = ["run", "--rm", "-it", "--init", "--name"]
            .map(String::from)
            .to_vec();
        w.push(format!("tehas-dev-{}", self.name));
        w.extend_from_slice(self.limits);
        w.extend_from_slice(self.security);
        w.extend(["--user", self.user, "-w", self.tree].map(String::from));
        w.extend_from_slice(self.mounts);
        if let Some(images) = self.images {
            w.extend([
                "-v".to_string(),
                format!("{images}:{images}:ro"),
                "-e".to_string(),
                format!("PI_ENSEMBLE_EXTRA_IMAGE_DIRS={images}"),
            ]);
        }
        w.extend_from_slice(self.envs);
        // By name: Pi draws a terminal interface, and docker's own default
        // describes a plainer terminal than most people sit at.
        w.extend(["-e", "TERM", "-e", "COLORTERM"].map(String::from));
        w.extend_from_slice(self.hosts);
        w.extend([self.image, "pi"].map(String::from));
        w.extend_from_slice(self.pi);
        w
    }
}

fn table() -> String {
    let image = "PI_ENSEMBLE_IMAGE";
    let session = Session {
        name: "",
        limits: &[],
        security: &[],
        user: "",
        tree: "",
        mounts: &[],
        images: None,
        envs: &[],
        hosts: &[],
        image,
        pi: &[],
    };
    let words = session.words();
    let at = words.iter().position(|w| w == image);
    let entry = at.map_or("", |i| &words[i + 1]);
    format!("dev\t{image}\t{entry}")
}

fn git_global(key: &str) -> String {
    Command::new("git")
        .args(["config", "--global", key])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim_end_matches('\n')
                .to_string()
        })
        .unwrap_or_default()
}

fn account() -> String {
    Command::new("id")
        .arg("-un")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
        .unwrap_or_default()
}

fn split_author(author: &str) -> (String, String) {
    let name = author.rsplit_once(" <").map_or(author, |(n, _)| n);
    let mail = author.rsplit_once('<').map_or(author, |(_, m)| m);
    (
        name.to_string(),
        mail.strip_suffix('>').unwrap_or(mail).to_string(),
    )
}

// Anything with files in it that has no .git directory is somebody's, not ours.
fn not_a_clone(tree: &Path) -> bool {
    tree.exists() && (!tree.is_dir() || fs::read_dir(tree).map_or(true, |mut d| d.next().is_some()))
}

// Made beside its place and moved there whole. A clone that stopped halfway has a
// .git directory too, and the next run would take it for a finished one. The name
// starts with a dot, which no clone's name can.
fn make_clone(devs: &str, name: &str, tree: &str, src: &str, branch: &str) -> bool {
    let making = format!("{devs}/.{name}.making");
    let _ = fs::remove_dir_all(&making);
    let made = sandbox::clone(src, &making, branch)
        && (!Path::new(tree).is_dir() || fs::remove_dir(tree).is_ok())
        && fs::rename(&making, tree).is_ok();
    if !made {
        let _ = fs::remove_dir_all(&making);
    }
    made
}

// The running cycles, from the view or from the one object an earlier version
// wrote.
fn running_said(view: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(view).ok()?;
    let one = [doc.clone()];
    let cycles: &[Value] = match doc.get("cycles") {
        Some(Value::Array(list)) => list,
        Some(_) => return None,
        None if doc.get("running") == Some(&Value::Bool(true)) => &one,
        None => &[],
    };
    let issues: Vec<String> = cycles
        .iter()
        .filter_map(|c| c.get("issue"))
        .map(|i| i.as_str().map_or_else(|| i.to_string(), str::to_string))
        .collect();
    match issues.as_slice() {
        [] => None,
        [one] => Some(format!(
            "a cycle is running on issue {one}; the session shares the model endpoint with it"
        )),
        many => Some(format!(
            "cycles are running on issues {}; the session shares the model endpoint with them",
            many.join(", ")
        )),
    }
}

// What the room looks like now, with the cycles that hold a lock. The session's
// own memory comes off it once its container runs.
fn room_said(home: &str, usable: &slice::Usable, load: &Load) -> String {
    let issues = lock::held_in(&Path::new(home).join("state/cycles"));
    let cids: Vec<String> = issues
        .iter()
        .map(|n| format!("{home}/state/cycles/{n}.cid"))
        .collect();
    let reading = Reading::of_room(slice::root(), &cids);
    let room = slice::room(&reading, usable, &load.reserve, issues.len() as u64);
    let why = match room.why.as_str() {
        "pressure" => " (it is under memory pressure)".to_string(),
        "full" => " (it holds as many containers as their ceilings fit)".to_string(),
        "unreadable" if room.note.is_empty() => " (its use could not be read)".to_string(),
        "unreadable" => format!(" (its use could not be read: {})", room.note),
        _ => String::new(),
    };
    format!(
        "tehas dev: tehas.slice has room for {} more cycles{why}",
        room.room
    )
}

// Pi leaves the terminal on its alternate screen with the cursor hidden when its
// container is killed. These do nothing to a terminal that is in order.
fn terminal_back() {
    if !std::io::stdout().is_terminal() {
        return;
    }
    let run = |program: &str, word: &str| {
        let _ = Command::new(program)
            .arg(word)
            .stderr(Stdio::null())
            .status();
    };
    run("stty", "sane");
    run("tput", "rmcup");
    run("tput", "cnorm");
}

fn refuse(line: String) -> u8 {
    eprintln!("{line}");
    78
}

/// Whose name the commits carry.
struct Committer {
    /// The author to store; empty for the bot.
    author: String,
    name: String,
    email: String,
    said: String,
}

// The author given now, else the stored one, else the bot's, which is the tehas
// user's own git identity. Err is the exit status.
fn committer(given: &Words, stored: &str) -> Result<Committer, u8> {
    let mut author = given.author.clone();
    if author.is_empty() && Path::new(stored).exists() {
        let text = fs::read_to_string(stored).unwrap_or_default();
        author = text.lines().next().unwrap_or("").to_string();
        if !author_ok(&author) {
            eprintln!("tehas dev: {stored} does not hold \"Name <email>\"; give --author again");
            return Err(64);
        }
    }
    if !author.is_empty() {
        let (name, email) = split_author(&author);
        let said = author.clone();
        return Ok(Committer {
            author,
            name,
            email,
            said,
        });
    }
    let (name, email) = (git_global("user.name"), git_global("user.email"));
    if name.is_empty() || email.is_empty() {
        eprintln!(
            "tehas dev: git user.name and user.email are not set for {}, and {} has no --author",
            account(),
            given.name
        );
        return Err(64);
    }
    let said =
        format!("the bot, {name} <{email}>. Give --author \"Name <email>\" once to use your own");
    Ok(Committer {
        author,
        name,
        email,
        said,
    })
}

// Whether the clone is still to make. An existing clone is known by its .git
// directory and by nothing git would have to be run for. Err is the exit status.
fn fresh_clone(tree: &str) -> Result<bool, u8> {
    let fresh = !Path::new(tree).join(".git").is_dir();
    if fresh && not_a_clone(Path::new(tree)) {
        return Err(refuse(format!(
            "tehas dev: {tree} exists and is not a clone, not touching it"
        )));
    }
    Ok(fresh)
}

// Every refusal that needs neither GitHub nor a write, in the order a session
// meets them. Nothing here writes. Err is the exit status.
fn ready(given: &Words) -> Result<Taken, u8> {
    // No container starts outside the slice, and none with half its limits.
    let taken = sandbox::take(&Host::here(), &Load::from_env(), "tehas dev", "a session");
    let taken = taken.map_err(refuse)?;
    let on = |k: &str| env::var(k).is_ok_and(|v| v == "1");
    if !given.issue.is_empty() && !on("TEHAS_WORK_IMAGES") && !on("TEHAS_WORK_FILES") {
        return Err(refuse(
            "tehas dev: this instance gives no images or files (TEHAS_WORK_IMAGES, TEHAS_WORK_FILES), so --issue has nothing to give"
                .to_string(),
        ));
    }
    let devs = format!("{}/dev", env::var("TEHAS_HOME").unwrap_or_default());
    let name = &given.name;
    committer(given, &format!("{devs}/{name}.author"))?;
    fresh_clone(&format!("{devs}/{name}"))?;
    Ok(taken)
}

// tehas_github_env: an app's token goes in this environment, and from there by
// name to what is started. It is never an argument.
fn token() -> Result<(), u8> {
    if !env::var("TEHAS_GITHUB_APP_ID").is_ok_and(|id| !id.is_empty()) {
        return Ok(());
    }
    let Some(token) = github::for_a_container() else {
        eprintln!("tehas dev: no GitHub token for the app");
        return Err(1);
    };
    env::set_var("GH_TOKEN", token);
    Ok(())
}

// The issue's images and files, by the rules a cycle is given them. Who applied
// the label decides (rule 5). Err is the exit status.
fn images(given: &Words) -> Result<(), u8> {
    if given.issue.is_empty() {
        return Ok(());
    }
    let devs = format!("{}/dev", env::var("TEHAS_HOME").unwrap_or_default());
    fs::create_dir_all(&devs).map_err(|_| 78u8)?;
    let dir = format!("{devs}/{}.images", given.name);
    let approved = match label::allowed_issue(&given.issue, "tehas:ready") {
        0 => Ok("approved"),
        2 => Err("the issue's label events could not be read for its images".to_string()),
        _ => Ok(""),
    };
    let got = approved.and_then(|word| attach::issue_images(&given.issue, &dir, word));
    if let Err(why) = got {
        let _ = fs::remove_dir_all(&dir);
        eprintln!("tehas dev: {why}, not starting a session");
        return Err(4);
    }
    let reads = attach::reads_images(&env::var("PI_ENSEMBLE_IMAGE").unwrap_or_default());
    for line in attach::said(&dir, "session", &reads)
        .iter()
        .flat_map(|t| t.split('\n'))
    {
        println!("tehas dev: {line}");
    }
    Ok(())
}

// Up to the point the container's words can be made. Err is the exit status.
fn prepared(given: &Words) -> Result<Taken, u8> {
    crate::settings::loaded(&NEEDS)?;
    sandbox::settings_ok().map_err(refuse)?;
    settings::checkout().map_err(refuse)?;
    settings::branch().map_err(refuse)?;
    let taken = ready(given)?;
    token()?;
    if env::var("GH_TOKEN").unwrap_or_default().is_empty() {
        eprintln!("tehas dev: GH_TOKEN is not set (source ~/tehas/env.sh)");
        return Err(64);
    }
    images(given)?;
    Ok(taken)
}

/// `tehas dev`.
pub fn start(given: &[String]) -> u8 {
    let words = match parse(given) {
        Ok(Some(words)) => words,
        Ok(None) => {
            print!("{USAGE}");
            return 0;
        }
        Err(why) => {
            eprintln!("tehas dev: {why}");
            eprint!("{USAGE}");
            return 64;
        }
    };
    match prepared(&words) {
        Ok(taken) => session(&words, &taken),
        Err(rc) => rc,
    }
}

// What another account may ask for: its author and pi's arguments, nothing more.
fn login_words<'a>(account: &str, given: &'a [String]) -> Result<(&'a str, &'a [String]), String> {
    if account.is_empty() {
        return Err("this is run by sudo for another account; type: tehas dev".to_string());
    }
    if !name_ok(account) {
        return Err(format!(
            "the account name {account} cannot name a clone (letters, digits, _ and -)"
        ));
    }
    let Some((author, rest)) = given.split_first() else {
        return Err("no author was given; type: tehas dev".to_string());
    };
    match rest.split_first() {
        None => Ok((author, rest)),
        Some((word, pi)) if word == "--" => Ok((author, pi)),
        Some((word, _)) => Err(format!("unknown argument: {word}")),
    }
}

// sudo cleared the environment, so the tehas user's own is loaded, as the timers
// do. It is a shell file, and it exports what a command should have and keeps the
// rest to itself. The shell ends in this program again.
const ENTER: &str = r#"if [ -r "$0" ]; then . "$0"; fi; exec "$@""#;

/// `tehas dev` for another account, as the tehas user. The clone is the account
/// sudo was asked by (SUDO_USER, which the caller cannot set); the author is a
/// label, checked for its shape. Exit: that of the session; 64 not started by
/// sudo, an account whose name cannot name a clone, or other arguments.
pub fn login(given: &[String]) -> u8 {
    let account = env::var_os("SUDO_USER").unwrap_or_default();
    let account = account.to_string_lossy();
    let (author, pi) = match login_words(&account, given) {
        Ok(words) => words,
        Err(why) => {
            eprintln!("tehas dev: {why}");
            return 64;
        }
    };
    let Some(program) = env::current_exe().ok() else {
        return refuse("tehas dev: this program cannot find itself".to_string());
    };
    let Some(root) = program.parent().and_then(Path::parent) else {
        return refuse(format!("tehas dev: no home above {}", program.display()));
    };
    // The caller's directory is usually one the tehas user cannot read.
    let home = env::var_os("HOME").unwrap_or_default();
    if env::set_current_dir(home).is_err() {
        let _ = env::set_current_dir("/");
    }
    let failed = Command::new("bash")
        .args(["-c", ENTER])
        .arg(root.join("env.sh"))
        .arg(&program)
        .args(["tehas", "dev", &account, "--author", author, "--"])
        .args(pi)
        .exec();
    refuse(format!("tehas dev: cannot run bash: {failed}"))
}

fn session(given: &Words, taken: &Taken) -> u8 {
    let var = |k: &str| env::var(k).unwrap_or_default();
    let name = given.name.as_str();
    let load = Load::from_env();
    let branch = match settings::branch() {
        Ok(branch) => branch,
        Err(line) => return refuse(line),
    };
    let home = var("TEHAS_HOME");
    let devs = format!("{home}/dev");
    let tree = format!("{devs}/{name}");
    let stored = format!("{devs}/{name}.author");
    // The clone's own state on the host: what this session writes and nothing a
    // cycle wrote. Kept with the clone.
    let state = format!("{devs}/{name}.state");
    let runs = format!("{state}/runs");

    let who = match committer(given, &stored) {
        Ok(who) => who,
        Err(rc) => return rc,
    };
    let fresh = match fresh_clone(&tree) {
        Ok(fresh) => fresh,
        Err(rc) => return rc,
    };
    if fs::create_dir_all(&devs).is_err() {
        return 78;
    }
    if !who.author.is_empty() && fs::write(&stored, format!("{}\n", who.author)).is_err() {
        return refuse(format!("tehas dev: cannot write {stored}"));
    }

    // The clone's lock, held for the session's life, so no session meets a
    // target half removed.
    let wait = env::var("TEHAS_DEV_LOCK_WAIT")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(300);
    let target = format!("{state}/target");
    let Ok(_lock) = clean::open_session(&home, name, wait) else {
        return refuse(format!("tehas dev: not starting a session of {name}"));
    };
    // The cleaner may have taken it during the wait, and docker would make it
    // root's.
    if !Path::new(&target).is_dir() && fs::create_dir(&target).is_err() {
        return refuse(format!(
            "tehas dev: cannot make {target}, not starting a session"
        ));
    }

    if fresh {
        if !make_clone(&devs, name, &tree, &var("TEHAS_WORK_REPO"), &branch) {
            // Only empty ones: a state a person kept from an earlier clone stays.
            let _ = fs::remove_dir(&target);
            let _ = fs::remove_dir(&state);
            return refuse(format!(
                "tehas dev: could not make the clone {tree}, not starting a session"
            ));
        }
        println!("tehas dev: made {tree} at the tip of {branch} the product checkout last fetched");
    }

    let who_git = Identity {
        tree: &tree,
        name: &who.name,
        email: &who.email,
    };
    let envs = sandbox::env_options(
        Kind::Session,
        &who_git,
        &sandbox::ensemble(Kind::Session),
        &sandbox::tokens(),
    );
    let paths = Paths {
        tree: &tree,
        host: &tree,
        state: &state,
        runs: &runs,
    };
    let mounts = sandbox::prepared_mounts(Kind::Session, &paths).map_err(|why| {
        if !why.is_empty() {
            eprintln!("{why}");
        }
    });
    let at = Path::new(&state);
    let models = models::write_for_container(&at.join("models.json"), &at.join("endpoints"));
    let (Ok(mounts), Some(())) = (mounts, models) else {
        return refuse("tehas dev: could not prepare the sandbox, not starting a session".into());
    };
    let hosts = sandbox::hosts(&var("PI_ENSEMBLE_HOST_ALIASES"));

    println!("tehas dev: {tree}, commits as {}", who.said);
    let view = fs::read_to_string(format!("{home}/live/running.json")).unwrap_or_default();
    if let Some(said) = running_said(&view) {
        println!("tehas dev: {said}");
    }
    println!("{}", room_said(&home, &taken.slice, &load));

    // Ctrl-C belongs to the session: it must not take the terminal's reset away.
    let interrupted = Arc::new(AtomicBool::new(false));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, interrupted);
    let images = (!given.issue.is_empty()).then(|| format!("{devs}/{name}.images"));
    let user = sandbox::user();
    let image = var("PI_ENSEMBLE_IMAGE");
    let line = Session {
        name,
        limits: &taken.limits,
        security: &taken.security,
        user: &user,
        tree: &tree,
        mounts: &mounts,
        images: images.as_deref(),
        envs: &envs,
        hosts: &hosts,
        image: &image,
        pi: &given.pi,
    };
    let status = Command::new("docker").args(line.words()).status();
    let rc = status.map_or(127, |s| {
        s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0))
    });
    terminal_back();
    rc as u8
}

pub fn main(given: &[String]) -> u8 {
    match given {
        [verb] if verb == "table" => {
            println!("{}", table());
            0
        }
        _ => {
            eprintln!("console dev: usage: table");
            64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(w: &[&str]) -> Vec<String> {
        w.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_author_is_a_name_and_an_address_on_one_line() {
        for good in [
            "Ada Lovelace <ada@example.com>",
            "A <a@b>",
            "A B-C D <x.y@z>",
        ] {
            assert!(author_ok(good), "{good}");
        }
        for bad in [
            "",
            "Ada",
            "<a@b>",
            " Ada <a@b>",
            "Ada  <a@b> x",
            "Ada <a b@c>",
            "Ada <ab>",
            "Ada <a@b@c>",
            "A<da <a@b>",
            "Ada\n <a@b>",
        ] {
            assert!(!author_ok(bad), "{bad}");
        }
    }

    #[test]
    fn the_words_of_a_session() {
        let w = parse(&given(&[
            "zed", "--author", "A <a@b>", "--issue", "7", "--", "-r", "x",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(w.name, "zed");
        assert_eq!(w.author, "A <a@b>");
        assert_eq!(w.issue, "7");
        assert_eq!(w.pi, ["-r", "x"]);
        assert_eq!(parse(&given(&[])).unwrap().unwrap().name, "main");
        assert_eq!(parse(&given(&["--help"])).unwrap(), None);
    }

    #[test]
    fn words_that_cannot_be_right_are_refused() {
        for bad in [
            vec!["--resume"],
            vec!["main", "other"],
            vec![""],
            vec!["a/b"],
            vec!["_x"],
            vec!["--author"],
            vec!["--author", "nobody"],
            vec!["--issue"],
            vec!["--issue", "x"],
            vec!["--issue", "07"],
            vec!["--issue", ""],
        ] {
            assert!(parse(&given(&bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_account_gets_a_clone_of_its_own_name_and_gives_only_an_author() {
        let alone = given(&["A <a@b>"]);
        let (author, pi) = login_words("alice", &alone).unwrap();
        assert_eq!((author, pi.len()), ("A <a@b>", 0));
        let words = given(&["A <a@b>", "--", "-r", "--author", "x"]);
        let (_, pi) = login_words("alice", &words).unwrap();
        assert_eq!(pi, given(&["-r", "--author", "x"]));
        let said = |account: &str, w: &[&str]| login_words(account, &given(w)).unwrap_err();
        assert_eq!(
            said("", &["A <a@b>"]),
            "this is run by sudo for another account; type: tehas dev"
        );
        for bad in ["../main", "a.b", "-x", "_x", "a b"] {
            assert_eq!(
                said(bad, &["A <a@b>"]),
                format!("the account name {bad} cannot name a clone (letters, digits, _ and -)")
            );
        }
        assert_eq!(said("alice", &[]), "no author was given; type: tehas dev");
        assert_eq!(said("alice", &["A <a@b>", "bob"]), "unknown argument: bob");
    }

    #[test]
    fn a_session_line_has_the_terminal_the_images_and_the_arguments_of_pi() {
        let w = Session {
            name: "zed",
            limits: &given(&["--memory", "2048m"]),
            security: &given(&["--cap-drop", "ALL"]),
            user: "1:2",
            tree: "/t",
            mounts: &given(&["-v", "/a:/b"]),
            images: Some("/i"),
            envs: &given(&["-e", "GH_TOKEN"]),
            hosts: &given(&["--add-host=m:1.2.3.4"]),
            image: "img",
            pi: &given(&["-r"]),
        }
        .words();
        assert_eq!(
            w.join(" "),
            "run --rm -it --init --name tehas-dev-zed --memory 2048m --cap-drop ALL \
             --user 1:2 -w /t -v /a:/b -v /i:/i:ro -e PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/i \
             -e GH_TOKEN -e TERM -e COLORTERM --add-host=m:1.2.3.4 img pi -r"
        );
        assert_eq!(table(), "dev\tPI_ENSEMBLE_IMAGE\tpi");
    }

    #[test]
    fn the_running_cycles_are_said_once() {
        assert_eq!(running_said(""), None);
        assert_eq!(running_said(r#"{"cycles":[]}"#), None);
        let one = running_said(r#"{"cycles":[{"issue":7}]}"#).unwrap();
        assert!(one.starts_with("a cycle is running on issue 7;"));
        let two = running_said(r#"{"cycles":[{"issue":7},{"issue":8}]}"#).unwrap();
        assert!(two.starts_with("cycles are running on issues 7, 8;"));
        let old = running_said(r#"{"running":true,"issue":5}"#).unwrap();
        assert!(old.starts_with("a cycle is running on issue 5;"));
        assert_eq!(running_said(r#"{"running":false,"issue":5}"#), None);
    }

    #[test]
    fn an_author_splits_at_its_last_address() {
        assert_eq!(
            split_author("Ada L <ada@example.com>"),
            ("Ada L".to_string(), "ada@example.com".to_string())
        );
    }
}
