//! Three things install.sh asks of the console about the host it installs on.
//!
//!   console update keeps <home> <work dir>
//!   console update units <env file>
//!   console update login <root> <login command> <marker>
//!
//! `keeps` moves each planning session's kept directory, an older tehas's
//! `<home>/state/plan-<n>.json.keep`, to beside the workspaces, where the
//! planning turn now keeps it: `<work dir>-plan-<n>.keep`. Only under the
//! issue's lock, `state/plan-<n>.json.lock`, and never over a path that is
//! there. A lock held by a running turn leaves the directory where it is, on a
//! `later:` line. Exit 0, 1 a directory could not be moved (the ones before it
//! were), 64 usage.
//!
//! `units` prints, on one line, the units a first install enables: the dispatch
//! timer and path, and each optional unit whose setting is on. It reads the
//! settings from its environment. A first install runs before any shell has
//! read env.sh, which exports TEHAS_ENERGY from the host's private file, so
//! without it in the environment the last `TEHAS_ENERGY=` line of `<env file>`
//! counts. Exit 0, 64 usage.
//!
//! `login` compares the command root installed for the other accounts,
//! `<login command>` (/usr/local/bin/tehas), with `<root>/host/login/tehas`.
//! Only where `bootstrap.sh dev` left its marker, `<marker>`
//! (/etc/tehas/manager), which names the tehas user. When the two differ it
//! prints the command that installs the checkout's copy, else nothing. Exit 0,
//! 1 a file of the checkout is missing, 64 usage.

use super::{fault, say, tell};
use std::env;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn keeps_verb(given: &[String]) -> u8 {
    let [home, work] = given else {
        tell("console update keeps: usage: <home> <work dir>");
        return 64;
    };
    let mut lines = Vec::new();
    let rc = keeps(Path::new(home), work, &mut lines);
    for line in lines {
        match line {
            Line::Out(text) => say(&text),
            Line::Err(text) => tell(&text),
        }
    }
    rc
}

#[derive(Debug, PartialEq)]
enum Line {
    Out(String),
    Err(String),
}

/// The issue numbers of the kept directories under `state`, in order. A
/// symbolic link is not one.
fn kept(state: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(state) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let n = name.strip_prefix("plan-")?.strip_suffix(".json.keep")?;
            let digits = !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
            digits.then(|| n.to_string())
        })
        .collect();
    found.sort();
    found
}

fn keeps(home: &Path, work: &str, lines: &mut Vec<Line>) -> u8 {
    let state = home.join("state");
    for n in kept(&state) {
        let keep = state.join(format!("plan-{n}.json.keep"));
        let to = PathBuf::from(format!("{work}-plan-{n}.keep"));
        let lock = state.join(format!("plan-{n}.json.lock"));
        // Held to the end of the move, so no turn starts on the issue meanwhile.
        let held = match lock.exists() {
            true => match File::open(&lock) {
                Ok(f) if f.try_lock().is_ok() => Some(f),
                _ => {
                    lines.push(Line::Out(format!(
                        "later: ./install.sh again when the planning turn of #{n} has ended   ({} stays where it is)",
                        keep.display()
                    )));
                    continue;
                }
            },
            false => None,
        };
        if to.symlink_metadata().is_ok() {
            lines.push(Line::Err(format!(
                "note: {} stays where it is: {} is there already",
                keep.display(),
                to.display()
            )));
            continue;
        }
        if move_dir(&keep, &to).is_err() {
            lines.push(Line::Err(format!(
                "tehas: could not move {} to {}",
                keep.display(),
                to.display()
            )));
            return 1;
        }
        drop(held);
        lines.push(Line::Out(format!(
            "moved: {} to {}",
            keep.display(),
            to.display()
        )));
    }
    0
}

/// A rename, or mv where the two are on different file systems.
fn move_dir(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            let ran = Command::new("mv")
                .arg("-T")
                .arg("--")
                .arg(from)
                .arg(to)
                .status()?;
            match ran.success() {
                true => Ok(()),
                false => Err(io::Error::other("mv failed")),
            }
        }
        other => other,
    }
}

pub fn units_verb(given: &[String]) -> u8 {
    let [env_file] = given else {
        tell("console update units: usage: <env file>");
        return 64;
    };
    let file = fs::read(env_file).ok();
    let file = file.as_deref().map(String::from_utf8_lossy);
    say(&first_units(|name| env::var(name).ok(), file.as_deref()).join(" "));
    0
}

/// The units a first install enables, from the settings `get` reads and the
/// host's private env file.
fn first_units(get: impl Fn(&str) -> Option<String>, env_file: Option<&str>) -> Vec<&'static str> {
    let set = |name: &str| get(name).filter(|v| !v.is_empty());
    let mut units = vec!["tehas-dispatch.timer", "tehas-dispatch.path"];
    if set("TEHAS_IMPROVE_REPO").is_some() {
        units.push("tehas-improve.timer");
    }
    if set("TEHAS_DOMAIN").is_some() {
        units.push("tehas-protolab.timer");
    }
    if get("TEHAS_PREVIEWS").as_deref() == Some("1") {
        units.extend(["tehas-preview.timer", "tehas-preview-sweep.timer"]);
    }
    let energy = set("TEHAS_ENERGY").or_else(|| {
        env_file?
            .lines()
            .filter_map(|l| l.strip_prefix("TEHAS_ENERGY="))
            .next_back()
            .map(str::to_string)
    });
    if energy.is_some_and(|e| !e.is_empty() && e != "off") {
        units.push("tehas-energy.service");
    }
    units
}

pub fn login_verb(given: &[String]) -> u8 {
    let [root, bin, marker] = given else {
        tell("console update login: usage: <root> <login command> <marker>");
        return 64;
    };
    match login_fix(Path::new(root), Path::new(bin), Path::new(marker)) {
        Ok(Some(fix)) => {
            say(&fix);
            0
        }
        Ok(None) => 0,
        Err(missing) => {
            fault(&format!(
                "{} is missing from this checkout",
                missing.display()
            ));
            1
        }
    }
}

/// The command that brings the login command to this checkout's copy, or
/// nothing when it is that copy or this host never ran `bootstrap.sh dev`.
/// The error is a file of the checkout that is not there.
fn login_fix(root: &Path, bin: &Path, marker: &Path) -> Result<Option<String>, PathBuf> {
    let Ok(text) = fs::read(marker) else {
        return Ok(None);
    };
    // Said as a missing checkout: a missing source is not a difference this
    // host can fix.
    let ours = root.join("host/login/tehas");
    for need in [&ours, &root.join("bootstrap.sh")] {
        if !need.is_file() {
            return Err(need.to_path_buf());
        }
    }
    let first = text.split(|b| *b == b'\n').next().unwrap_or_default();
    let user = std::str::from_utf8(first)
        .ok()
        .filter(|u| login_name(u))
        .unwrap_or("<tehas user>");
    let same = match (fs::read(&ours), fs::read(bin)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    Ok((!same).then(|| format!("sudo {}/bootstrap.sh dev {user}", root.display())))
}

/// The shape of an account name bootstrap.sh dev accepts. Anything else is left
/// for the person to fill in, since the name goes into a printed command.
fn login_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;
    use std::collections::HashMap;
    use std::os::unix::fs::symlink;

    #[test]
    fn a_kept_directory_moves_beside_the_workspaces() {
        let d = dir("keeps-move");
        d.file("home/state/plan-8.json.keep/plan-sessions/one", "kept");
        d.file("home/state/plan-8.json.lock", "");
        let home = d.0.join("home");
        let work = format!("{}/home/work/widget", d.0.display());
        let mut lines = Vec::new();
        assert_eq!(keeps(&home, &work, &mut lines), 0);
        let to = format!("{work}-plan-8.keep");
        assert_eq!(
            fs::read_to_string(format!("{to}/plan-sessions/one")).unwrap(),
            "kept"
        );
        assert!(!home.join("state/plan-8.json.keep").exists());
        assert_eq!(
            lines,
            [Line::Out(format!(
                "moved: {}/state/plan-8.json.keep to {to}",
                home.display()
            ))]
        );
    }

    #[test]
    fn a_held_lock_or_a_path_that_is_there_leaves_it_where_it_is() {
        let d = dir("keeps-stay");
        d.file("home/state/plan-3.json.keep/a", "");
        d.file("home/state/plan-4.json.keep/a", "");
        let lock = d.file("home/state/plan-3.json.lock", "");
        d.file("home/work/w-plan-4.keep/b", "");
        let home = d.0.join("home");
        let work = format!("{}/home/work/w", d.0.display());
        let holder = File::open(&lock).unwrap();
        holder.lock().unwrap();
        let mut lines = Vec::new();
        assert_eq!(keeps(&home, &work, &mut lines), 0);
        let state = home.join("state");
        assert_eq!(
            lines,
            [
                Line::Out(format!(
                    "later: ./install.sh again when the planning turn of #3 has ended   ({}/plan-3.json.keep stays where it is)",
                    state.display()
                )),
                Line::Err(format!(
                    "note: {}/plan-4.json.keep stays where it is: {work}-plan-4.keep is there already",
                    state.display()
                )),
            ]
        );
        assert!(state.join("plan-3.json.keep/a").exists());
        assert!(state.join("plan-4.json.keep/a").exists());
    }

    #[test]
    fn only_a_directory_named_for_an_issue_is_moved() {
        let d = dir("keeps-names");
        d.file("home/state/plan-x1.json.keep/a", "");
        d.file("home/state/plan-.json.keep/a", "");
        d.file("home/state/plan-5.json.keep", "a file");
        d.file("elsewhere/a", "");
        let state = d.0.join("home/state");
        symlink(d.0.join("elsewhere"), state.join("plan-6.json.keep")).unwrap();
        d.file("home/state/plan-10.json.keep/a", "");
        d.file("home/state/plan-9.json.keep/a", "");
        assert_eq!(kept(&state), ["10", "9"]);
        assert!(kept(&d.0.join("none")).is_empty());
    }

    #[test]
    fn a_move_that_fails_stops_with_1() {
        let d = dir("keeps-fail");
        d.file("home/state/plan-2.json.keep/a", "");
        // The workspace's parent is a file, so it cannot be made.
        d.file("home/work", "not a directory");
        let home = d.0.join("home");
        let work = format!("{}/home/work/w", d.0.display());
        let mut lines = Vec::new();
        assert_eq!(keeps(&home, &work, &mut lines), 1);
        assert!(matches!(&lines[..], [Line::Err(l)] if l.starts_with("tehas: could not move ")));
        assert!(home.join("state/plan-2.json.keep/a").exists());
    }

    fn units(set: &[(&str, &str)], env_file: Option<&str>) -> String {
        let set: HashMap<String, String> = set
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        first_units(|name| set.get(name).cloned(), env_file).join(" ")
    }

    #[test]
    fn a_first_install_enables_the_dispatcher_and_the_units_whose_setting_is_on() {
        assert_eq!(units(&[], None), "tehas-dispatch.timer tehas-dispatch.path");
        assert_eq!(
            units(
                &[
                    ("TEHAS_IMPROVE_REPO", ""),
                    ("TEHAS_DOMAIN", ""),
                    ("TEHAS_PREVIEWS", "0")
                ],
                None
            ),
            "tehas-dispatch.timer tehas-dispatch.path"
        );
        assert_eq!(
            units(
                &[
                    ("TEHAS_IMPROVE_REPO", "acme/tehas"),
                    ("TEHAS_DOMAIN", "example.org"),
                    ("TEHAS_PREVIEWS", "1"),
                    ("TEHAS_ENERGY", "viiwork"),
                ],
                None
            ),
            "tehas-dispatch.timer tehas-dispatch.path tehas-improve.timer tehas-protolab.timer \
             tehas-preview.timer tehas-preview-sweep.timer tehas-energy.service"
        );
    }

    #[test]
    fn energy_comes_from_the_environment_else_the_last_line_of_the_env_file() {
        let base = "tehas-dispatch.timer tehas-dispatch.path";
        let energy = format!("{base} tehas-energy.service");
        assert_eq!(units(&[("TEHAS_ENERGY", "off")], None), base);
        assert_eq!(
            units(&[], Some("TEHAS_ENERGY=on\nTEHAS_ENERGY=off\n")),
            base
        );
        assert_eq!(
            units(&[], Some("TEHAS_ENERGY=off\nTEHAS_ENERGY=on\n")),
            energy
        );
        assert_eq!(units(&[], Some("TEHAS_ENERGY=\n")), base);
        assert_eq!(units(&[], Some("# TEHAS_ENERGY=on\nOTHER=1\n")), base);
        // The environment wins over the file.
        assert_eq!(
            units(&[("TEHAS_ENERGY", "off")], Some("TEHAS_ENERGY=on\n")),
            base
        );
        // An empty setting is no setting.
        assert_eq!(
            units(&[("TEHAS_ENERGY", "")], Some("TEHAS_ENERGY=on\n")),
            energy
        );
    }

    #[test]
    fn the_login_command_is_compared_only_where_bootstrap_dev_left_its_marker() {
        let d = dir("login");
        let root = d.0.join("root");
        d.file("root/host/login/tehas", "#!/bin/sh\nexec tehas\n");
        d.file("root/bootstrap.sh", "");
        let bin = d.0.join("bin/tehas");
        let marker = d.0.join("etc/tehas/manager");
        assert_eq!(login_fix(&root, &bin, &marker), Ok(None));
        d.file("etc/tehas/manager", "boss\n");
        let fix = format!("sudo {}/bootstrap.sh dev boss", root.display());
        assert_eq!(login_fix(&root, &bin, &marker), Ok(Some(fix.clone())));
        d.file("bin/tehas", "#!/bin/sh\nexec tehas\n");
        assert_eq!(login_fix(&root, &bin, &marker), Ok(None));
        d.file("bin/tehas", "#!/bin/sh\nexec tehas\n# older\n");
        assert_eq!(login_fix(&root, &bin, &marker), Ok(Some(fix)));
    }

    #[test]
    fn a_name_of_another_shape_is_left_for_the_person() {
        let d = dir("login-name");
        let root = d.0.join("root");
        d.file("root/host/login/tehas", "x");
        d.file("root/bootstrap.sh", "");
        let bin = d.0.join("bin/tehas");
        let marker = d.0.join("manager");
        let fix = |user: &str| format!("sudo {}/bootstrap.sh dev {user}", root.display());
        for (text, user) in [
            ("someone\nsecond line\n", "someone"),
            ("_svc-1", "_svc-1"),
            ("", "<tehas user>"),
            ("Boss\n", "<tehas user>"),
            ("boss; rm -rf /\n", "<tehas user>"),
            ("1boss\n", "<tehas user>"),
        ] {
            d.file("manager", text);
            assert_eq!(
                login_fix(&root, &bin, &marker),
                Ok(Some(fix(user))),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_missing_file_of_the_checkout_is_said_as_one() {
        let d = dir("login-missing");
        let root = d.0.join("root");
        let marker = d.file("manager", "boss\n");
        let bin = d.0.join("bin/tehas");
        assert_eq!(
            login_fix(&root, &bin, &marker),
            Err(root.join("host/login/tehas"))
        );
        d.file("root/host/login/tehas", "x");
        assert_eq!(
            login_fix(&root, &bin, &marker),
            Err(root.join("bootstrap.sh"))
        );
    }
}
