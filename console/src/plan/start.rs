//! `tehas plan`: one turn of a planning session, from the settings to the end
//! of the turn. `console run` (src/run.rs) loads the settings first;
//! src/usage/plan.txt says what a person is told.
//!
//! The container is given what a cycle's is given, by the builder of
//! tehas_core::sandbox, and the goldens of test/sandbox_argv_test.sh pin it.

use super::turn::{self, Turn};
use super::{descriptor, update};
use crate::{attach, github, label, review};
use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::models;
use tehas_core::sandbox::{self, Identity, Kind, Paths};
use tehas_core::{settings, time};

const RUNNER: &str = "/opt/pi-rukas/bin/pi-rukas-headless";
const USAGE: &str = "usage: tehas plan <issue-number> --out <file> --keep <dir> (--brief <file> | --session <id> --answer <file> [--comment <id>]) [--kind refine]";
const UNREAD: &str = "the issue's label events could not be read for its images";

/// The words of one turn.
#[derive(Debug, Default, PartialEq)]
pub struct Words {
    pub issue: String,
    pub out: String,
    pub keep: String,
    pub brief: String,
    pub answer: String,
    pub session: String,
    pub comment: String,
    pub refine: bool,
}

fn readable(path: &str) -> bool {
    fs::File::open(path).is_ok()
}

fn digits(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}

/// The words, or None when they are no turn. A flag given twice keeps the
/// last, as the shell did.
fn parse(given: &[String]) -> Option<Words> {
    let (issue, mut rest) = given.split_first()?;
    if issue.is_empty() || !digits(issue) {
        return None;
    }
    let mut w = Words {
        issue: issue.clone(),
        ..Words::default()
    };
    while let [flag, value, more @ ..] = rest {
        let slot = match flag.as_str() {
            "--out" => &mut w.out,
            "--keep" => &mut w.keep,
            "--brief" => &mut w.brief,
            "--answer" => &mut w.answer,
            "--session" => &mut w.session,
            "--comment" => &mut w.comment,
            "--kind" if value == "refine" => {
                w.refine = true;
                rest = more;
                continue;
            }
            _ => return None,
        };
        slot.clone_from(value);
        rest = more;
    }
    if !rest.is_empty() || w.out.is_empty() || w.keep.is_empty() {
        return None;
    }
    let shaped = if w.brief.is_empty() {
        !w.session.is_empty() && !w.answer.is_empty() && readable(&w.answer)
    } else {
        w.session.is_empty() && w.answer.is_empty() && w.comment.is_empty() && readable(&w.brief)
    };
    // The id ends up on a command line: letters, digits and dashes only.
    let id = w
        .session
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    (shaped && id && digits(&w.comment)).then_some(w)
}

/// The words of `tehas plan`, as `console check` asks: 64 when they are no turn.
pub fn words(given: &[String]) -> Result<Words, u8> {
    parse(given).ok_or_else(|| {
        eprintln!("{USAGE}");
        64
    })
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

fn refuse(line: impl AsRef<str>) -> u8 {
    eprintln!("{}", line.as_ref());
    78
}

/// A comment on the issue, its output dropped. A failure is said and changes
/// nothing else.
fn comment(issue: &str, body: &str) {
    let posted = Command::new("gh")
        .args(["issue", "comment", issue, "--repo", &var("TEHAS_REPO")])
        .args(["--body", body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !posted.is_ok_and(|s| s.success()) {
        eprintln!("tehas plan: the comment on #{issue} failed");
    }
}

/// `$(cat file)`, as the shell compared it.
fn captured(path: &str) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.trim_end_matches('\n').to_string()
}

/// After a drafted plan, an answer that ticked "Update this issue" runs no
/// container.
fn chose_update(keep: &str, answer: &str) -> bool {
    let answer = fs::read(answer).unwrap_or_default();
    captured(&format!("{keep}/exit")) == "12"
        && answer
            .split(|b| *b == b'\n')
            .any(|line| line.ends_with(b" chose: Update this issue"))
}

/// The issue's {title, body}, as gh issue view gives them.
fn issue_text(issue: &str) -> Option<String> {
    let out = Command::new("gh")
        .args(["issue", "view", issue, "--repo", &var("TEHAS_REPO")])
        .args(["--json", "title,body"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn update_issue(w: &Words) -> u8 {
    let said = issue_text(&w.issue)
        .ok_or_else(String::new)
        .and_then(|was| update(&w.session, Path::new(&w.keep), &was));
    let new = format!("{}.new", w.out);
    let written = said.and_then(|said| {
        fs::write(&new, format!("{said}\n"))
            .and_then(|()| fs::rename(&new, &w.out))
            .map_err(|e| e.to_string())
    });
    match written {
        Ok(()) => {
            tehas_core::outln!(
                "tehas plan: issue={} session={} update=yes",
                w.issue,
                w.session
            );
            0
        }
        Err(why) => {
            let _ = fs::remove_file(&new);
            if !why.is_empty() {
                eprintln!("tehas plan: {why}");
            }
            eprintln!(
                "tehas plan: the drafted plan or the text of #{} could not be read",
                w.issue
            );
            2
        }
    }
}

/// tehas_github_env: an app's token goes in this environment, and from there by
/// name to the container. It is never an argument.
fn token() -> Result<(), u8> {
    if !var("TEHAS_GITHUB_APP_ID").is_empty() {
        let Some(token) = github::for_a_container() else {
            eprintln!("tehas plan: no GitHub token for the app");
            return Err(1);
        };
        env::set_var("GH_TOKEN", token);
    }
    if var("GH_TOKEN").is_empty() {
        eprintln!("tehas plan: GH_TOKEN is not set (source ~/tehas/env.sh)");
        return Err(64);
    }
    Ok(())
}

/// The images and the text files share one directory, given to the container
/// when the instance gives either kind. The second word names them.
fn kinds(images: bool, files: bool) -> (bool, &'static str) {
    match (images, files) {
        (true, true) => (true, "images and files"),
        (false, true) => (true, "files"),
        (true, false) => (true, "images"),
        (false, false) => (false, "images"),
    }
}

/// Whether an allowed account applied tehas:plan or tehas:refine: it approved
/// the text, links and all, as tehas:ready does a cycle's (rule 5). Err when
/// the label events could not be read.
fn approved(issue: &str) -> Result<&'static str, ()> {
    for name in ["tehas:plan", "tehas:refine"] {
        match label::allowed_issue(issue, name) {
            0 => return Ok("approved"),
            2 => return Err(()),
            _ => {}
        }
    }
    Ok("")
}

/// The issue's images, once per session, from the first turn. Err is the
/// status that ends the turn.
fn first_images(w: &Words, images: &str, kinds: &str) -> Result<(), u8> {
    let got = approved(&w.issue)
        .map_err(|()| UNREAD.to_string())
        .and_then(|word| attach::issue_images(&w.issue, images, word));
    if let Err(why) = got {
        let _ = fs::remove_dir_all(images);
        eprintln!("tehas plan: {why}, not starting a turn");
        comment(&w.issue, &format!("Tehas started no planning session: {why}. This instance gives a session the {kinds} an issue links, and a plan made without one of them may be the wrong plan. To retry, apply `tehas:plan`."));
        return Err(4);
    }
    if var("TEHAS_WORK_DESCRIBE") == "1"
        && tehas_cycle::describe::describe(Path::new(images)).is_err()
    {
        eprintln!("tehas plan: the descriptions of the images could not be written, the session goes on without them");
    }
    let reads = attach::reads_images(&var("PI_ENSEMBLE_IMAGE"));
    if let Some(said) = attach::said(images, "planning session", &reads) {
        comment(&w.issue, &said);
    }
    Ok(())
}

/// What a reply links, added to the session's images. Ok holds the new files.
/// A session that began without the directory goes on without it.
fn reply_images(w: &Words, images: &str, kinds: &str) -> Result<String, u8> {
    if w.comment.is_empty() || !Path::new(images).join("manifest.json").is_file() {
        return Ok(String::new());
    }
    match attach::answer_images(images, &w.answer, &w.comment) {
        Ok((new, left)) => {
            if !left.is_empty() {
                comment(&w.issue, &format!("Tehas left out of your reply:\n{left}"));
            }
            Ok(new)
        }
        Err(why) => {
            eprintln!("tehas plan: {why}, not starting a turn");
            comment(&w.issue, &format!("Tehas ran no planning turn: {why}. This instance gives a session the {kinds} a reply links, and a plan made without one of them may be the wrong plan. Reply again to run the turn."));
            Err(4)
        }
    }
}

fn status(rc: u8) -> Result<(), u8> {
    if rc == 0 {
        Ok(())
    } else {
        Err(rc)
    }
}

/// The words of the security options of "What a container may not do", the
/// size of /tmp aside.
const SECURITY: [&str; 9] = [
    "--cap-drop",
    "ALL",
    "--cap-add",
    "CHOWN",
    "--cap-add",
    "FOWNER",
    "--security-opt",
    "no-new-privileges:true",
    "--tmpfs",
];

/// The tmpfs of /tmp, of a size in megabytes that is no zero.
fn tmp_ok(word: &str) -> bool {
    word.strip_prefix("/tmp:rw,exec,nosuid,nodev,size=")
        .and_then(|rest| rest.strip_suffix(",mode=1777"))
        .and_then(|size| size.strip_suffix('m'))
        .is_some_and(|n| n.starts_with(|c: char| ('1'..='9').contains(&c)))
}

/// What `tehas-cycle sandbox limits` handed out, held to its shape: fourteen
/// words of limits under tehas.slice, then the ten security options. A list
/// that drifted starts nothing (AGENTS.md rule 4). Err is the line to say.
fn held(rc: i32, words: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let limits: Vec<String> = words.iter().take(14).cloned().collect();
    let security: Vec<String> = words.iter().skip(14).take(10).cloned().collect();
    if rc != 0 || limits.len() != 14 || limits[1] != "tehas.slice" || limits[12] != "--cpu-shares" {
        return Err("tehas plan: the container limits could not be made, not starting".into());
    }
    let shaped = security.len() == 10 && security[..9] == SECURITY && tmp_ok(&security[9]);
    if !shaped {
        return Err(format!(
            "tehas plan: tehas_sandbox_security gave {} words, wants 10: not starting a turn",
            security.len()
        ));
    }
    Ok((limits, security))
}

/// The container's limits and security options, from the tehas-cycle beside
/// this command. Err is the exit status, said.
fn limits(libexec: &str) -> Result<(Vec<String>, Vec<String>), u8> {
    let out = Command::new(format!("{libexec}/tehas-cycle"))
        .args(["sandbox", "limits", "tehas plan", "a turn"])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output();
    let (rc, bytes) = out.map_or((127, Vec::new()), |o| {
        (o.status.code().unwrap_or(1), o.stdout)
    });
    if rc == 78 {
        return Err(78);
    }
    let words: Vec<String> = bytes
        .split(|b| *b == 0)
        .filter(|w| !w.is_empty())
        .map(|w| String::from_utf8_lossy(w).into_owned())
        .collect();
    held(rc, &words).map_err(refuse)
}

/// The docker line of a turn, in order. `head` is the container's name, its
/// user, its working directory and the image; `options` the limits, the
/// security options, the mounts, the images' options, the variables and the
/// hosts.
fn docker_line(head: [&str; 4], options: [&Vec<String>; 6], run: &[String]) -> Vec<String> {
    let [name, user, repo, image] = head;
    let [limits, security, rest @ ..] = options;
    let mut line: Vec<String> = ["docker", "run", "--rm", "--init", "--name", name]
        .map(String::from)
        .to_vec();
    line.extend_from_slice(limits);
    line.extend_from_slice(security);
    line.extend(["--user", user, "-w", repo].map(String::from));
    for words in rest {
        line.extend_from_slice(words);
    }
    line.extend([image, RUNNER].map(String::from));
    line.extend_from_slice(run);
    line
}

/// This command's rows of the table of runs, read off the docker lines: the
/// image's probe and the turn.
pub fn table() -> String {
    let image = "PI_ENSEMBLE_IMAGE";
    let none = Vec::new();
    let runs = [
        sandbox::capabilities(image),
        docker_line(["", "", "", image], [&none; 6], &[]),
    ];
    let rows = runs.iter().map(|run| {
        let at = run.iter().position(|w| w == image);
        let entry = at.and_then(|i| run.get(i + 1)).map_or("", String::as_str);
        format!("plan\t{image}\t{entry}")
    });
    rows.collect::<Vec<_>>().join("\n")
}

/// `tehas plan`, once the settings are loaded and required.
pub fn start(libexec: &str, given: &[String]) -> u8 {
    match words(given).and_then(|w| turn_of(libexec, &w)) {
        Ok(rc) | Err(rc) => rc,
    }
}

fn turn_of(libexec: &str, w: &Words) -> Result<u8, u8> {
    sandbox::settings_ok().map_err(refuse)?;
    // Docker would create a missing mount source as a root-owned directory.
    settings::checkout().map_err(refuse)?;
    let branch = settings::branch().map_err(refuse)?;
    // No container starts outside the slice, and none with half its limits.
    let (limits, security) = limits(libexec)?;

    let home = var("TEHAS_HOME");
    let src = var("TEHAS_WORK_REPO");
    // The workspace inside the container, and the directory on the host behind it.
    let repo = var("TEHAS_WORK_DIR");
    let issue = w.issue.as_str();
    let tree = format!("{repo}-plan-{issue}");
    let image = var("PI_ENSEMBLE_IMAGE");
    let limit: u64 = var("TEHAS_PLAN_TIMEOUT")
        .parse()
        .map_err(|_| refuse("tehas plan: TEHAS_PLAN_TIMEOUT is no number of seconds"))?;
    let log = format!(
        "{home}/logs/plan-{issue}-{}.jsonl",
        time::stamp(time::now())
    );
    let name = format!("tehas-plan-{issue}");
    let runs = format!("{}/.pi/agent/ensemble-runs/plan-{issue}", var("HOME"));
    // The turn's own state on the host, beside the workspace and not in it.
    let container = format!("{tree}.state");
    let images = format!("{}/images", w.keep);
    // What a turn's container is given of them: a copy beside the workspace, so
    // no container sees the keep directory.
    let images_given = format!("{tree}.images");
    let images_inside = format!("{repo}.images");

    token()?;
    let _ = fs::create_dir_all(format!("{home}/logs"));

    if w.brief.is_empty() && chose_update(&w.keep, &w.answer) {
        return Ok(update_issue(w));
    }

    // The bot's identity is the tehas user's own.
    let (bot_name, bot_email) = (
        review::git_global("user.name"),
        review::git_global("user.email"),
    );
    if bot_name.is_empty() || bot_email.is_empty() {
        eprintln!(
            "tehas plan: git user.name and user.email are not set for {}",
            review::account()
        );
        return Err(64);
    }
    let (give, kinds) = kinds(
        var("TEHAS_WORK_IMAGES") == "1",
        var("TEHAS_WORK_FILES") == "1",
    );
    // Their paths are this command's to give, not the instance's: dropped
    // whether or not this turn is given attachments, so no container names a
    // directory tehas did not mount. So is the issue, and so is where the
    // session lives: in the clone, carried between turns.
    for name in [
        "PI_ENSEMBLE_EXTRA_IMAGE_DIRS",
        "PI_ENSEMBLE_PLAN_ISSUE",
        "PI_ENSEMBLE_PLAN_SESSION_DIR",
        "PI_ENSEMBLE_PLAN_CACHE_TTL_MS",
    ] {
        env::remove_var(name);
    }
    let who = Identity {
        tree: &repo,
        name: &bot_name,
        email: &bot_email,
    };
    let envs = sandbox::env_options(
        Kind::Turn,
        &who,
        &sandbox::ensemble(Kind::Turn),
        &sandbox::tokens(),
    );
    let hosts = sandbox::hosts(&var("PI_ENSEMBLE_HOST_ALIASES"));

    // An image whose runner does not take planning turns runs none, before
    // anything is fetched or cloned.
    let caps = review::capabilities(&image).unwrap_or_default();
    let lib = Path::new(libexec).join("../lib");
    if tehas_cycle::caps::mode(&caps, "planTurns", Some(&lib)).0 != "native" {
        return Err(refuse(format!("tehas plan: {image} runs no planning turns (its --capabilities has no features.planTurns), not starting a turn")));
    }

    // A fresh workspace for this turn, as a cycle gets one. Only a directory
    // tehas made is ever removed.
    let works = Path::new(&tree)
        .parent()
        .map_or("/".to_string(), |p| p.display().to_string());
    let rules = format!("{libexec}/../lib/issue-brief.md");
    let brief = (!w.brief.is_empty()).then_some((w.brief.as_str(), rules.as_str()));
    status(turn::workspace(&works, &w.keep, brief))?;

    let mut new = String::new();
    if give {
        if w.brief.is_empty() {
            new = reply_images(w, &images, kinds)?;
        } else {
            first_images(w, &images, kinds)?;
        }
    }
    status(turn::give(&works, &images, &images_given, give))?;
    let mut image_opts: Vec<String> = Vec::new();
    if Path::new(&images_given).is_dir() {
        image_opts = vec![
            "-v".into(),
            format!("{images_given}:{images_inside}:ro"),
            "-e".into(),
            format!("PI_ENSEMBLE_EXTRA_IMAGE_DIRS={images_inside}"),
            "-e".into(),
            format!("PI_ENSEMBLE_PLAN_ISSUE={issue}"),
        ];
    }

    let gone = |dir: &str| fs::remove_dir_all(dir).is_ok() || !Path::new(dir).exists();
    let made = fs::create_dir_all(&works).is_ok()
        && fs::write(Path::new(&works).join(".tehas-workspaces"), "").is_ok()
        && gone(&tree)
        && gone(&container)
        && sandbox::clone(&src, &tree, &branch);
    if !made {
        return Err(refuse(format!(
            "tehas plan: could not make the workspace {tree}, not starting a turn"
        )));
    }
    status(turn::place(&tree, &w.keep, &branch, brief.is_some()))?;

    let run = if brief.is_some() {
        let mut text = String::new();
        let read = fs::File::open(format!("{}/brief.md", w.keep))
            .and_then(|mut f| f.read_to_string(&mut text));
        if read.is_err() {
            return Err(refuse(format!(
                "tehas plan: the brief of #{issue} could not be read, not starting a turn"
            )));
        }
        let what = descriptor(&text, issue);
        let named = if tehas_cycle::caps::verb(&caps, "plan", "--issue") {
            issue
        } else {
            ""
        };
        turn::first(&w.keep, &what, named)
    } else {
        turn::later(&w.keep, &w.session, &w.answer, &tree, &repo, &new)?
    };

    // The turn's own copy of the host's code index, asked for only once this
    // turn will start: a turn that never began leaves no copy behind.
    if !sandbox::seed(&container, &format!("{home}/state/index")) {
        tehas_core::outln!("tehas plan: the code index could not be copied into {container}/cache, this turn searches without one");
    }
    // The mounts name directories that only exist now.
    let paths = Paths {
        tree: &repo,
        host: &tree,
        state: &container,
        runs: &runs,
    };
    let mounts = sandbox::prepared_mounts(Kind::Turn, &paths).map_err(|why| {
        if !why.is_empty() {
            eprintln!("{why}");
        }
    });
    let at = Path::new(&container);
    let models = models::write_for_container(&at.join("models.json"), &at.join("endpoints"));
    let (Ok(mounts), Some(())) = (mounts, models) else {
        return Err(refuse(
            "tehas plan: could not prepare the sandbox, not starting a turn",
        ));
    };

    let user = sandbox::user();
    let head = [name.as_str(), &user, &repo, &image];
    let options = [&limits, &security, &mounts, &image_opts, &envs, &hosts];
    let line = docker_line(head, options, &run);
    let argv: Vec<&str> = line.iter().map(String::as_str).collect();
    let turn = Turn {
        issue,
        session: &w.session,
        refine: w.refine,
        out: &w.out,
        keep: &w.keep,
        tree: &tree,
        state: &container,
        images: &images_given,
        log: &log,
    };
    Ok(turn::run(&turn, limit, &name, &argv))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(words: &str) -> Vec<String> {
        words.split(' ').map(String::from).collect()
    }

    #[test]
    fn a_first_and_a_later_turn_are_read_and_the_last_flag_counts() {
        let dir = tehas_core::testdir::TestDir::new("plan-start");
        let brief = dir.join("brief.md");
        fs::write(&brief, "# T\n").unwrap();
        let b = brief.display();
        let w = parse(&given(&format!(
            "7 --out o --keep k --brief {b} --kind refine --out p"
        )))
        .unwrap();
        assert_eq!(
            (w.out.as_str(), w.keep.as_str(), w.refine),
            ("p", "k", true)
        );
        let w = parse(&given(&format!(
            "7 --out o --keep k --session abc-1 --answer {b} --comment 12"
        )))
        .unwrap();
        assert_eq!(
            (w.session.as_str(), w.comment.as_str(), w.refine),
            ("abc-1", "12", false)
        );
    }

    #[test]
    fn words_that_are_no_turn_are_refused() {
        let dir = tehas_core::testdir::TestDir::new("plan-start-bad");
        let brief = dir.join("brief.md");
        fs::write(&brief, "# T\n").unwrap();
        let b = brief.display();
        for bad in [
            String::new(),
            "7".into(),
            format!("x --out o --keep k --brief {b}"),
            format!("07a --out o --keep k --brief {b}"),
            "7 --out o --keep k".into(),
            format!("7 --keep k --brief {b}"),
            format!("7 --out o --brief {b}"),
            format!("7 --out o --keep k --brief {b} --session abc"),
            format!("7 --out o --keep k --brief {b} --comment 3"),
            "7 --out o --keep k --session abc".into(),
            "7 --out o --keep k --brief /no/such/brief".into(),
            format!("7 --out o --keep k --session a;b --answer {b}"),
            format!("7 --out o --keep k --session abc --answer {b} --comment 1x"),
            format!("7 --out o --keep k --brief {b} --verbose"),
            format!("7 --out o --keep k --brief {b} --kind plan"),
            format!("7 --out o --keep k --brief {b} --out"),
        ] {
            assert_eq!(parse(&given(&bad)), None, "{bad}");
        }
    }

    #[test]
    fn only_fourteen_limits_under_the_slice_and_the_ten_security_words_pass() {
        let w = |line: &str| -> Vec<String> { line.split_whitespace().map(String::from).collect() };
        let fourteen = "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024";
        let ten = "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777";
        let good = w(&format!("{fourteen} {ten}"));
        assert_eq!(
            held(0, &good).map(|(l, s)| (l.len(), s.len())),
            Ok((14, 10))
        );
        assert!(held(1, &good).is_err());
        let limits = "tehas plan: the container limits could not be made, not starting";
        for bad in [
            String::new(),
            format!("--cgroup-parent tehas.slice --memory 2048m {ten}"),
            fourteen.replace("tehas.slice", "other.slice") + " " + ten,
        ] {
            assert_eq!(held(0, &w(&bad)), Err(limits.to_string()), "{bad}");
        }
        for (n, bad) in [
            (8, "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true"),
            (10, "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw --tmpfs /var/tmp:rw"),
            (10, "--cap-drop NET_RAW --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"),
            (10, "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=0m,mode=1777"),
        ] {
            assert_eq!(
                held(0, &w(&format!("{fourteen} {bad}"))),
                Err(format!("tehas plan: tehas_sandbox_security gave {n} words, wants 10: not starting a turn")),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_images_and_the_files_share_one_directory_named_by_what_it_holds() {
        assert_eq!(kinds(true, false), (true, "images"));
        assert_eq!(kinds(false, true), (true, "files"));
        assert_eq!(kinds(true, true), (true, "images and files"));
        assert_eq!(kinds(false, false), (false, "images"));
    }

    #[test]
    fn only_a_tick_of_update_after_a_drafted_plan_updates_the_issue() {
        let dir = tehas_core::testdir::TestDir::new("plan-start-update");
        let keep = dir.display().to_string();
        let answer = dir.join("answer.md").display().to_string();
        fs::write(&answer, "note\nalice chose: Update this issue\n").unwrap();
        fs::write(dir.join("exit"), "12\n").unwrap();
        assert!(chose_update(&keep, &answer));
        fs::write(dir.join("exit"), "11\n").unwrap();
        assert!(!chose_update(&keep, &answer));
        fs::write(dir.join("exit"), "12\n").unwrap();
        fs::write(&answer, "alice chose: Update this issue, but\n").unwrap();
        assert!(!chose_update(&keep, &answer));
    }
}
