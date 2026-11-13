//! `console update run <console> <arguments>`: the whole of `tehas update`, which
//! libexec/update execs. `<console>` is the path the script ran, so the console
//! install.sh leaves there, or the `.prev` it kept, can finish the run.
//!
//! The steps are the verbs an older libexec/update runs one by one, in this
//! process: the arguments, the wait for the lock (read before the settings,
//! whose load drops it), the settings, `gate`, the lock, `prepare`, install.sh
//! with its output kept apart, then `finish` by the console that speaks this
//! one's `api`. The finisher is a child and not an exec: this process keeps the
//! lock while it runs, and the finisher's own children cannot be handed it.
//!
//! install.sh renames over the console while it runs, which this process does
//! not notice: it runs from the file it was started from.

use super::finish::INSTALL_LIMIT;
use super::lock::Held;
use super::{code, gate, output, parse, prepare, tell, temp_dir, timed, wait};
use super::{host_or_refuse, Mode, Plan, ANSWERED, API, NO_LOCK};
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::settings::INSTANCE_SETTINGS;

/// A directory removed when the run ends, however it ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch() -> Option<Scratch> {
    let parent = env::var_os("TMPDIR")
        .filter(|t| !t.is_empty())
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    let parent = if fs::create_dir_all(&parent).is_ok() {
        parent
    } else {
        PathBuf::from("/tmp")
    };
    temp_dir(&parent, "tehas-update.").ok().map(Scratch)
}

/// Whether this console can finish what this one began.
fn speaks(console: &str) -> bool {
    let mut api = Command::new(console);
    api.args(["update", "api"]).stderr(Stdio::null());
    output(&mut api).1 == API.to_string()
}

/// install.sh, with its two outputs in their files. The status as a shell's
/// `$?` gives it.
fn install(plan: &Plan, lock: Option<&Held>, out: &Path, err: &Path) -> io::Result<i32> {
    let (out, err) = (File::create(out)?, File::create(err)?);
    if plan.tree.is_empty() {
        return Ok(0);
    }
    let mut run = timed(INSTALL_LIMIT, Path::new(&plan.tree).join("install.sh"));
    if plan.mode == Mode::Check {
        run.arg("--check");
    }
    run.stdout(out).stderr(err);
    if let Some(lock) = lock {
        lock.inherit(true)?;
    }
    let ran = run.status();
    if let Some(lock) = lock {
        lock.inherit(false)?;
    }
    Ok(ran.map_or(127, code))
}

fn copy(file: &Path, to: &mut dyn Write) {
    if let Ok(text) = fs::read(file) {
        let _ = to.write_all(&text);
    }
}

pub(super) fn main(given: &[String]) -> u8 {
    let Some((console, rest)) = given.split_first() else {
        tell("console update run: usage: run <console> <arguments>");
        return 64;
    };
    match parse(rest) {
        Ok(_) => {}
        Err(ANSWERED) => return 0,
        Err(rc) => return rc,
    }
    let wait = wait::update();
    if let Err(rc) = crate::settings::loaded(&INSTANCE_SETTINGS) {
        return rc;
    }
    let lock = match gate(rest) {
        0 => {
            let host = match host_or_refuse() {
                Ok(host) => host,
                Err(rc) => return rc,
            };
            match Held::take(&host.home, wait) {
                Ok(held) => {
                    env::set_var("TEHAS_UPDATE_LOCK_FD", held.fd().to_string());
                    Some(held)
                }
                Err(rc) => return rc,
            }
        }
        NO_LOCK => None,
        rc => return rc,
    };
    let Some(dir) = scratch() else {
        tell("tehas update: could not make a scratch file to keep install.sh's diagnostics apart");
        return 1;
    };
    let (plan, out, err) = (dir.0.join("plan"), dir.0.join("out"), dir.0.join("err"));
    let words: Vec<String> = ["--plan".to_string(), plan.display().to_string()]
        .into_iter()
        .chain(rest.iter().cloned())
        .collect();
    match prepare(&words) {
        0 => {}
        ANSWERED => return 0,
        rc => return rc,
    }
    let Some(read) = Plan::read(&plan) else {
        tell(&format!(
            "tehas update: could not read {}; run tehas update again to finish",
            plan.display()
        ));
        return 1;
    };
    let rc = match install(&read, lock.as_ref(), &out, &err) {
        Ok(rc) => rc,
        Err(e) => {
            tell(&format!("tehas update: could not run install.sh: {e}"));
            return 1;
        }
    };
    let prev = format!("{console}.prev");
    let Some(finisher) = [console.as_str(), &prev].into_iter().find(|c| speaks(c)) else {
        copy(&out, &mut io::stdout());
        copy(&err, &mut io::stderr());
        tell(&format!("tehas update: neither {console} nor {prev} can finish this update; what install.sh copied is installed, and what it recorded is still owed"));
        return 1;
    };
    let finished = Command::new(finisher)
        .args(["update", "finish", "--plan"])
        .arg(&plan)
        .arg("--install-out")
        .arg(&out)
        .arg("--install-err")
        .arg(&err)
        .args(["--install-rc", &rc.to_string()])
        .status();
    match finished {
        Ok(status) => code(status) as u8,
        Err(e) => {
            tell(&format!("tehas update: {finisher} cannot run: {e}"));
            1
        }
    }
}
