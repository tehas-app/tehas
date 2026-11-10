//! The settings a program needs of the instance, read from the environment
//! `tehas_load` leaves. The load has checked each value it holds, so all that
//! is left of `tehas_require` is whether the name is set: a command that needs
//! a setting stops with 78 before it does anything else.

use std::env;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

pub mod load;
pub mod rules;

/// The settings an instance has to set itself, because nothing here can give
/// them a default. Every command that checks the settings before it calls
/// GitHub, Docker or git or writes a file checks this one list: `tehas_load`
/// hands it to a shell as `_tehas_required`, so lib/config.sh, install.sh,
/// `console run` and tehas setup hold no copy of it.
pub const INSTANCE_SETTINGS: [&str; 5] = [
    "TEHAS_REPO",
    "TEHAS_CONTROL_ISSUE",
    "TEHAS_ALLOWED_LABELERS",
    "PI_ENSEMBLE_SUBAGENT_MODEL",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER",
];

/// What tehas-dispatch checks on top of [`INSTANCE_SETTINGS`]. A poll reads its
/// settings from the environment with no shell before it that loaded them, so
/// these three are named here rather than in the poll: without TEHAS_HOME it
/// has no libexec/store and no state directory to read, TEHAS_INSTANCE_DIR is
/// the file every refusal names, and TEHAS_TICKET_CONCURRENCY is a load's
/// derivation from the host, not a count a poll may guess.
pub const DISPATCHER_ALSO: [&str; 3] = [
    "TEHAS_HOME",
    "TEHAS_INSTANCE_DIR",
    "TEHAS_TICKET_CONCURRENCY",
];

/// A name set to nothing, or to spaces only, as `tehas_require` counts it.
fn first_unset<'a>(names: &[&'a str], get: impl Fn(&str) -> String) -> Option<&'a str> {
    names
        .iter()
        .copied()
        .find(|name| get(name).trim_matches(' ').is_empty())
}

/// Every name set, or the status to exit with, 78, the name said on standard
/// error in the words of `tehas_require`.
pub fn require(names: &[&str]) -> Result<(), u8> {
    let get = |name: &str| env::var_os(name).map_or(String::new(), |v| v.to_string_lossy().into());
    match first_unset(names, get) {
        Some(name) => {
            let dir = get("TEHAS_INSTANCE_DIR");
            let dir = if dir.is_empty() { "?" } else { &dir };
            eprintln!("tehas: {name} is not set (see {dir}/instance.env)");
            Err(78)
        }
        None => Ok(()),
    }
}

/// The login in the text of state/github/login: one line, `<slug>[bot]`.
pub fn login_of(text: &[u8]) -> Option<String> {
    let line = String::from_utf8_lossy(text);
    let line = line.lines().next()?.trim();
    (line.ends_with("[bot]") && line.len() > "[bot]".len() && !line.contains(char::is_whitespace))
        .then(|| line.to_string())
}

/// The login of the app the factory acts as, which the lookup of the app's
/// installation wrote under `home`; None before it has.
pub fn bot_login(home: &Path) -> Option<String> {
    login_of(&std::fs::read(home.join("state/github/login")).ok()?)
}

/// tehas_checkout: the product checkout is a clone of TEHAS_REPO. Err is the
/// line for standard error.
pub fn checkout() -> Result<(), String> {
    let (dir, repo, instance) = (
        var("TEHAS_WORK_REPO"),
        var("TEHAS_REPO"),
        var("TEHAS_INSTANCE_DIR"),
    );
    checkout_at(&dir, &repo, &instance)
}

/// checkout with the three names it reads given, so a case needs no
/// environment of its own.
fn checkout_at(dir: &str, repo: &str, instance: &str) -> Result<(), String> {
    if dir.is_empty() || !Path::new(dir).join(".git").is_dir() {
        let shown = if dir.is_empty() { "(unset)" } else { dir };
        let instance = if instance.is_empty() { "?" } else { instance };
        return Err(format!(
            "tehas: no product checkout at {shown}: clone {repo} there, or set TEHAS_WORK_REPO in {instance}/instance.env"
        ));
    }
    let url = git_line(dir, &["config", "--get", "remote.origin.url"]).unwrap_or_default();
    let url = url.strip_suffix('/').unwrap_or(&url);
    let url = url.strip_suffix(".git").unwrap_or(url);
    if url.ends_with(&format!("/{repo}")) || url.ends_with(&format!(":{repo}")) {
        return Ok(());
    }
    let url = if url.is_empty() { "nothing" } else { url };
    Err(format!(
        "tehas: {dir} is a checkout of {url}, not of {repo}"
    ))
}

/// tehas_branch: the branch work lands on. Read locally: the checkout's
/// origin/HEAD is set by install.sh, never looked up here. Err is the line for
/// standard error.
pub fn branch() -> Result<String, String> {
    branch_at(
        &var("TEHAS_BRANCH"),
        &var("TEHAS_WORK_REPO"),
        &var("TEHAS_INSTANCE_DIR"),
    )
}

/// branch with the three names it reads given.
fn branch_at(set: &str, dir: &str, instance: &str) -> Result<String, String> {
    if !set.is_empty() {
        return Ok(set.to_string());
    }
    let head = ["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"];
    let from = if dir.is_empty() { "/nonexistent" } else { dir };
    let name = git_line(from, &head).unwrap_or_default();
    let name = name.strip_prefix("origin/").unwrap_or(&name);
    if name.is_empty() {
        let shown = if dir.is_empty() {
            "the product checkout"
        } else {
            dir
        };
        let instance = if instance.is_empty() { "?" } else { instance };
        return Err(format!(
            "tehas: the default branch of {shown} is unknown: run install.sh, or set TEHAS_BRANCH in {instance}/instance.env"
        ));
    }
    Ok(name.to_string())
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

/// The bound of one read of the checkout's own git configuration. A poll and a
/// cycle both ask it before they act, and a git that hangs must not hang
/// either: tehas-dispatch bounded its own read at 30 seconds, and that bound
/// is here now.
pub const GIT_WAIT: Duration = Duration::from_secs(30);

/// What one call of git printed, under `timeout -k 5` of its own bound: None
/// when it failed, printed nothing or ran out of time. As `timeout` does it,
/// because a read of a checkout is never worth waiting for.
fn git_line(dir: &str, args: &[&str]) -> Option<String> {
    let mut all = vec!["-C", dir];
    all.extend_from_slice(args);
    line("git", &all, GIT_WAIT)
}

/// What `program` printed under `timeout -k 5 <bound>`, None when it failed or
/// outlived the bound.
fn line(program: &str, args: &[&str], bound: Duration) -> Option<String> {
    let out = Command::new("timeout")
        .arg("-k")
        .arg("5")
        .arg(bound.as_secs().to_string())
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    fn given(name: &str) -> String {
        match name {
            "TEHAS_REPO" => "acme/widget".to_string(),
            "SPACES" => "   ".to_string(),
            _ => String::new(),
        }
    }

    #[test]
    fn every_name_set_is_no_refusal() {
        assert_eq!(first_unset(&["TEHAS_REPO"], given), None);
        assert_eq!(first_unset(&[], given), None);
    }

    #[test]
    fn the_first_name_empty_or_only_spaces_is_the_one_said() {
        assert_eq!(
            first_unset(&["TEHAS_REPO", "EMPTY", "SPACES"], given),
            Some("EMPTY")
        );
        assert_eq!(
            first_unset(&["SPACES", "TEHAS_REPO"], given),
            Some("SPACES")
        );
    }

    #[test]
    fn a_name_not_set_is_78() {
        assert_eq!(
            require(&["TEHAS_SETTINGS_TEST_NO_SUCH_NAME"]),
            Err(78),
            "exit 78, no fallback"
        );
    }

    /// A directory git holds as a clone of `url`, with nothing fetched.
    fn checkout_of(url: &str) -> crate::testdir::TestDir {
        let at = crate::testdir::TestDir::new("checkout");
        let git = |args: &[&str]| {
            let done = Command::new("git")
                .arg("-C")
                .arg(at.path())
                .args(args)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .unwrap();
            assert!(
                done.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&done.stderr)
            );
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["remote", "add", "origin", url]);
        at
    }

    fn said(dir: &str) -> String {
        dir.to_string()
    }

    #[test]
    fn a_checkout_that_is_not_there_is_refused_by_name() {
        let at = crate::testdir::TestDir::new("no-checkout");
        let gone = at.join("absent").display().to_string();
        assert_eq!(
            checkout_at(&gone, "acme/widget", "/i"),
            Err(format!(
                "tehas: no product checkout at {gone}: clone acme/widget there, or set TEHAS_WORK_REPO in /i/instance.env"
            ))
        );
        assert_eq!(
            checkout_at("", "acme/widget", ""),
            Err("tehas: no product checkout at (unset): clone acme/widget there, or set TEHAS_WORK_REPO in ?/instance.env".to_string())
        );
    }

    #[test]
    fn the_checkout_is_accepted_only_for_the_repository_the_instance_names() {
        for (url, want) in [
            ("https://github.com/acme/widget.git", Ok::<(), String>(())),
            ("https://github.com/acme/widget", Ok::<(), String>(())),
            ("git@github.com:acme/widget.git", Ok::<(), String>(())),
            ("ssh://git@github.com/acme/widget/", Ok::<(), String>(())),
        ] {
            let at = checkout_of(url);
            let dir = said(&at.path().display().to_string());
            assert_eq!(checkout_at(&dir, "acme/widget", "/i"), want, "{url}");
        }
        for (url, seen) in [
            (
                "https://github.com/acme/other.git",
                "https://github.com/acme/other",
            ),
            (
                "git@github.com:notacme/widget.git",
                "git@github.com:notacme/widget",
            ),
        ] {
            let at = checkout_of(url);
            let dir = said(&at.path().display().to_string());
            assert_eq!(
                checkout_at(&dir, "acme/widget", "/i"),
                Err(format!(
                    "tehas: {dir} is a checkout of {seen}, not of acme/widget"
                )),
                "{url}"
            );
        }
        let at = crate::testdir::TestDir::new("bare");
        fs::create_dir(at.join(".git")).unwrap();
        let dir = said(&at.path().display().to_string());
        assert_eq!(
            checkout_at(&dir, "acme/widget", "/i"),
            Err(format!(
                "tehas: {dir} is a checkout of nothing, not of acme/widget"
            ))
        );
    }

    #[test]
    fn the_branch_is_the_setting_or_the_checkouts_origin_head() {
        assert_eq!(branch_at("release", "", ""), Ok("release".to_string()));
        let upstream = crate::testdir::TestDir::new("upstream");
        let at = crate::testdir::TestDir::new("branch");
        let done = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "init",
                "-q",
                "-b",
                "main",
            ])
            .arg(upstream.path())
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .unwrap();
        assert!(done.status.success());
        // The identity is named for the commit too: a runner with no git
        // config of its own has nothing to commit with otherwise.
        let done = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first",
            ])
            .current_dir(upstream.path())
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .unwrap();
        assert!(done.status.success());
        let done = Command::new("git")
            .args(["clone", "-q"])
            .arg(upstream.path())
            .arg(at.path())
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .unwrap();
        assert!(done.status.success());
        let dir = said(&at.path().display().to_string());
        assert_eq!(branch_at("", &dir, "/i"), Ok("main".to_string()));
        assert_eq!(branch_at("release", &dir, "/i"), Ok("release".to_string()));
    }

    #[test]
    fn a_default_branch_that_is_unknown_names_the_checkout_and_the_file() {
        assert_eq!(
            branch_at("", "/nonexistent", "/i"),
            Err(
                "tehas: the default branch of /nonexistent is unknown: run install.sh, or set TEHAS_BRANCH in /i/instance.env".to_string()
            )
        );
        assert_eq!(
            branch_at("", "", ""),
            Err(
                "tehas: the default branch of the product checkout is unknown: run install.sh, or set TEHAS_BRANCH in ?/instance.env".to_string()
            )
        );
    }

    #[test]
    fn a_read_that_outlives_its_bound_answers_nothing_and_does_not_wait_for_it() {
        let began = Instant::now();
        assert_eq!(line("sleep", &["9"], Duration::from_secs(1)), None);
        assert!(began.elapsed() < Duration::from_secs(5));
        assert_eq!(GIT_WAIT, Duration::from_secs(30));
    }
}
