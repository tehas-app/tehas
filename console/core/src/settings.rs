//! The settings a program needs of the instance, read from the environment
//! `tehas_load` leaves. The load has checked each value it holds, so all that
//! is left of `tehas_require` is whether the name is set: a command that needs
//! a setting stops with 78 before it does anything else.

use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

pub mod load;
pub mod rules;

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

/// tehas_checkout: the product checkout is a clone of TEHAS_REPO. Err is the
/// line for standard error.
pub fn checkout() -> Result<(), String> {
    let var = |k: &str| env::var(k).unwrap_or_default();
    let (dir, repo) = (var("TEHAS_WORK_REPO"), var("TEHAS_REPO"));
    if dir.is_empty() || !Path::new(&dir).join(".git").is_dir() {
        let shown = if dir.is_empty() { "(unset)" } else { &dir };
        let instance = var("TEHAS_INSTANCE_DIR");
        let instance = if instance.is_empty() { "?" } else { &instance };
        return Err(format!(
            "tehas: no product checkout at {shown}: clone {repo} there, or set TEHAS_WORK_REPO in {instance}/instance.env"
        ));
    }
    let url = git_line(&dir, &["config", "--get", "remote.origin.url"]).unwrap_or_default();
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
    let var = |k: &str| env::var(k).unwrap_or_default();
    let set = var("TEHAS_BRANCH");
    if !set.is_empty() {
        return Ok(set);
    }
    let dir = var("TEHAS_WORK_REPO");
    let head = ["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"];
    let from = if dir.is_empty() { "/nonexistent" } else { &dir };
    let name = git_line(from, &head).unwrap_or_default();
    let name = name.strip_prefix("origin/").unwrap_or(&name);
    if name.is_empty() {
        let shown = if dir.is_empty() {
            "the product checkout"
        } else {
            &dir
        };
        let instance = var("TEHAS_INSTANCE_DIR");
        let instance = if instance.is_empty() { "?" } else { &instance };
        return Err(format!(
            "tehas: the default branch of {shown} is unknown: run install.sh, or set TEHAS_BRANCH in {instance}/instance.env"
        ));
    }
    Ok(name.to_string())
}

fn git_line(dir: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
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
}
