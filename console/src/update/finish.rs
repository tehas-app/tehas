//! What is left of `tehas update` once install.sh has run: its answer read, the
//! follow-ups it asked for run in order, the pi-rukas commit followed, and the
//! tag of the image a release ran with.
//!
//! Whether a service runs is a question put to Docker or to systemd, and only
//! their answer means anything, never their exit status on its own. Only silence
//! next to exit 0 says systemd reloaded its unit files. When a question cannot
//! be asked this stops with exit 1 rather than report the service as absent.
//! Each step comes off the record only after it has run.

use super::release;
use super::steps::{self, read_record, record_file, sorted, write_record};
use super::{host_or_refuse, output, say, tell, timed, Host, Mode, Plan};
use crate::store::merged;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// What libexec/update gives install.sh. It runs it; this says so.
const INSTALL_LIMIT: u32 = 1800;
/// docker ps and docker compose up.
const COMPOSE_LIMIT: u32 = 180;
/// systemctl daemon-reload, is-active and restart.
const SYSTEMD_LIMIT: u32 = 60;
/// The sandbox image build.
const BUILD_LIMIT: u32 = 1800;
/// tehas labels: one gh label create per label of this version.
const LABELS_LIMIT: u32 = 120;

/// The state words of `systemctl is-active` that mean this unit runs.
const ACTIVE: &[&str] = &[
    "active",
    "activating",
    "reloading",
    "refreshing",
    "maintenance",
    "deactivating",
];
/// The ones that leave it alone. `unknown` means systemd has no such unit here,
/// which is an answer too.
const NOT_ACTIVE: &[&str] = &["inactive", "failed", "unknown"];

fn trimmed(said: &str) -> &str {
    said.trim_end_matches('\n')
}

/// What a command said, to standard output, as `printf '%s\n'` of a `$(...)`.
fn echo(said: &str) {
    if !said.is_empty() {
        say(said);
    }
}

/// The same indented, to standard error.
fn echo_indented(said: &str) {
    if !said.is_empty() {
        for line in said.split('\n') {
            tell(&format!("  {line}"));
        }
    }
}

fn executable(file: &Path) -> bool {
    fs::metadata(file).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

/// What a line of install.sh --check says: `<label>:` and spaces, then a word.
fn says(line: &str, label: &str, words: &[&str]) -> bool {
    let Some(rest) = line.strip_prefix(label) else {
        return false;
    };
    let value = rest.trim_start_matches(' ');
    value.len() < rest.len() && words.iter().any(|w| value.starts_with(w))
}

/// install.sh --check answers 1 when it found something. Files that differ and
/// steps owed are what an update fixes; the host's tools, the slice, root's copy
/// of the command and the tests are not.
fn found_to_update(out: &str) -> bool {
    out.lines()
        .any(|l| says(l, "Installed:", &["DRIFT"]) || says(l, "Follow-ups:", &["NOT DONE"]))
}

fn found_not_to_fix(out: &str) -> bool {
    out.lines().any(|l| {
        says(l, "Dependencies:", &["NOT OK"])
            || says(l, "Slice:", &["MISSING", "DIFFERS"])
            || says(l, "Login:", &["NOT CURRENT"])
            || says(l, "Tests:", &["FAILED"])
    })
}

/// The step lines install.sh printed.
fn next_lines(out: &str) -> impl Iterator<Item = &str> {
    out.split('\n').filter_map(|l| l.strip_prefix("next: "))
}

/// Everything install.sh printed but a follow-up line: this command runs the
/// ones it knows and names the ones it does not.
fn report(out: &[u8]) -> Vec<u8> {
    let mut end = out.len();
    while end > 0 && out[end - 1] == b'\n' {
        end -= 1;
    }
    if end == 0 {
        return Vec::new();
    }
    let mut shown = Vec::new();
    for line in out[..end].split(|b| *b == b'\n') {
        if !line.starts_with(b"next:") {
            shown.extend_from_slice(line);
            shown.push(b'\n');
        }
    }
    shown
}

/// Whether install.sh already printed the follow-up about the clone.
fn asked_for_bootstrap(out: &str) -> bool {
    out.find("later: ")
        .is_some_and(|at| out[at..].contains("bootstrap.sh user"))
}

struct Run<'a> {
    host: &'a Host,
    file: String,
    /// A build this run made is already on the commit followed.
    built: bool,
}

impl Run<'_> {
    fn warn_unknown(&self, unknown: &[String]) {
        if unknown.is_empty() {
            return;
        }
        tell(&format!(
            "WARN: {} step line(s) name no step this command knows; they were not run and stay recorded in {}",
            unknown.len(),
            self.file
        ));
        for line in unknown {
            tell(&format!("  next: {line}"));
        }
    }

    /// A compose server is renewed only where it already runs: an update keeps a
    /// service the host is using current, it does not add one to the host. The
    /// question is one plain ps on the project name the compose file pins.
    fn compose_up(&self, name: &str, project: &str, file: &Path, args: &[&str]) -> bool {
        if !file.is_file() {
            tell(&format!(
                "{name}: {} is not installed, left alone",
                file.display()
            ));
            return false;
        }
        let (rc, running) = merged(timed(COMPOSE_LIMIT, "docker").args([
            "ps".to_string(),
            "--quiet".to_string(),
            "--filter".to_string(),
            format!("label=com.docker.compose.project={project}"),
        ]));
        let running = trimmed(&running);
        if rc != 0 {
            tell(&format!("{name}: docker ps failed; the step is not done"));
            if rc == 124 {
                tell(&format!("  docker gave no answer in {COMPOSE_LIMIT}s"));
            }
            echo_indented(running);
            return false;
        }
        if running.is_empty() {
            say(&format!("{name}: not running on this host, left alone"));
            return true;
        }
        say(&format!("{name}: updating the container"));
        let (rc, said) = merged(
            timed(COMPOSE_LIMIT, "docker")
                .args(["compose", "-f"])
                .arg(file)
                .args(["up", "-d"])
                .args(args),
        );
        echo(trimmed(&said));
        match rc {
            0 => true,
            124 => {
                tell(&format!(
                    "{name}: docker compose gave no answer in {COMPOSE_LIMIT}s; the step is not done"
                ));
                false
            }
            _ => {
                tell(&format!("{name}: docker compose exited {rc}"));
                false
            }
        }
    }

    /// This never enables a unit: it restarts the ones that were active and
    /// leaves the rest as the operator left them. The reload comes first, because
    /// a unit file copied onto the host is not in effect until systemd read it.
    fn restart_units(&self, units: &[&str]) -> bool {
        let (rc, said) =
            merged(timed(SYSTEMD_LIMIT, "systemctl").args(["--user", "daemon-reload"]));
        let said = trimmed(&said);
        if rc == 124 {
            tell(&format!("systemd: systemctl --user daemon-reload gave no answer in {SYSTEMD_LIMIT}s; no unit was restarted, and the step is not done"));
            return false;
        }
        if rc != 0 || !said.is_empty() {
            let reason = if rc != 0 {
                "did not run"
            } else {
                "answered with text"
            };
            tell(&format!("systemd: systemctl --user daemon-reload {reason}; the changed unit files are not in effect, no unit was restarted, and the step is not done"));
            echo_indented(said);
            return false;
        }
        let mut active = Vec::new();
        for unit in units {
            // Asked without --quiet so there is an answer to read, and only
            // systemd's own state words are read: a wrapper that prints an error
            // and exits 0 says nothing about this unit.
            let (rc, answer) =
                merged(timed(SYSTEMD_LIMIT, "systemctl").args(["--user", "is-active", unit]));
            let answer = trimmed(&answer);
            if rc == 124 {
                tell(&format!("systemd: systemctl is-active gave no answer for {unit} in {SYSTEMD_LIMIT}s; no unit was restarted"));
                return false;
            }
            if ACTIVE.contains(&answer) {
                active.push(*unit);
            } else if !NOT_ACTIVE.contains(&answer) {
                tell(&format!(
                    "systemd: systemctl is-active gave no answer for {unit}; no unit was restarted"
                ));
                echo_indented(answer);
                return false;
            }
        }
        if active.is_empty() {
            say("systemd: none of them runs on this host, left alone");
            return true;
        }
        say(&format!("systemd: restarted {}", active.join(" ")));
        let (rc, said) = merged(
            timed(SYSTEMD_LIMIT, "systemctl")
                .args(["--user", "restart"])
                .args(&active),
        );
        match rc {
            0 => true,
            124 => {
                tell(&format!(
                    "systemd: systemctl gave no answer in {SYSTEMD_LIMIT}s; the step is not done"
                ));
                false
            }
            _ => {
                tell(&format!("systemd: systemctl exited {rc}"));
                echo_indented(trimmed(&said));
                false
            }
        }
    }

    /// The sandbox image, when its Dockerfile or the versions it pins changed.
    fn build_sandbox(&mut self) -> bool {
        let build = self.host.home.join("sandbox/build.sh");
        if !executable(&build) {
            tell(&format!("sandbox: {} is not installed", build.display()));
            return false;
        }
        say("sandbox: building the image");
        self.built = true;
        let (rc, said) = merged(&mut timed(BUILD_LIMIT, &build));
        echo(trimmed(&said));
        match rc {
            0 => true,
            124 => {
                tell(&format!(
                    "sandbox: the build gave no answer in {BUILD_LIMIT}s; the step is not done"
                ));
                false
            }
            _ => {
                tell(&format!("sandbox: the build exited {rc}"));
                false
            }
        }
    }

    /// `tehas labels` by the path the host's units name, rather than by whatever
    /// `tehas` the calling account has on its PATH.
    fn run_labels(&self) -> bool {
        let tehas = self.host.home.join("bin/tehas");
        if !executable(&tehas) {
            tell(&format!("labels: {} is not installed", tehas.display()));
            return false;
        }
        say("labels: making the tehas labels");
        let (rc, said) = merged(timed(LABELS_LIMIT, &tehas).arg("labels"));
        echo(trimmed(&said));
        match rc {
            0 => true,
            124 => {
                tell(&format!(
                    "labels: tehas labels gave no answer in {LABELS_LIMIT}s; the step is not done"
                ));
                false
            }
            _ => {
                tell(&format!(
                    "labels: tehas labels exited {rc}; the step is not done"
                ));
                false
            }
        }
    }

    /// One step: the id says which, and the arguments on the line, checked by
    /// steps::valid, say how.
    fn step(&mut self, line: &str) -> bool {
        let argv = steps::words(line);
        let Some((first, rest)) = argv.split_first() else {
            return false;
        };
        let edge = self.host.home.join("edge");
        match steps::step_id(first) {
            "units" => self.restart_units(rest),
            "sandbox" => self.build_sandbox(),
            "docs" => self.compose_up("docs", "tehas-docs", &edge.join("docs.compose.yaml"), rest),
            "edge" => self.compose_up(
                "edge",
                "tehas-edge",
                &edge.join("router.compose.yaml"),
                rest,
            ),
            "labels" => self.run_labels(),
            _ => {
                tell(&format!("WARN: no step this command knows for: {line}"));
                false
            }
        }
    }

    /// The steps still owed, then the lines no step here knows, as they came.
    fn write_pending(&self, steps: &[String], unknown: &[String]) -> bool {
        let owed: Vec<String> = steps.iter().chain(unknown).cloned().collect();
        write_record(&self.host.home, &owed).is_ok()
    }

    /// The installed versions.env names what the image follows, so an update
    /// looks upstream even when no file of tehas changed.
    fn follows_rukas(&self) -> bool {
        let versions = fs::read(self.host.home.join("versions.env")).unwrap_or_default();
        String::from_utf8_lossy(&versions)
            .lines()
            .any(|l| l.starts_with("PI_RUKAS_FOLLOW="))
            && executable(&self.host.home.join("sandbox/build.sh"))
    }

    fn rukas_step(&self, flag: &str) -> bool {
        let build = self.host.home.join("sandbox/build.sh");
        let (rc, said) = merged(timed(BUILD_LIMIT, &build).arg(flag));
        echo(trimmed(&said));
        if rc == 124 {
            tell(&format!(
                "pi-rukas: the build gave no answer in {BUILD_LIMIT}s"
            ));
        }
        rc == 0
    }

    /// The host's clone of pi-rukas lags the image whenever the followed commit
    /// moved, and only bootstrap.sh user moves it. Not a step: it fetches from
    /// the network. Nothing is decided on a missing answer: no image, no label,
    /// no clone is no evidence the clone is behind.
    fn rukas_later(&self, tree: &str) {
        let (rc, image) = output(
            timed(COMPOSE_LIMIT, "docker")
                .args([
                    "image",
                    "inspect",
                    "--format",
                    "{{index .Config.Labels \"tehas.rukas\"}}",
                    &self.host.image,
                ])
                .stderr(Stdio::null()),
        );
        if rc != 0 || image.is_empty() || image.contains("No such image") {
            return;
        }
        let (rc, clone) = output(
            Command::new("git")
                .arg("-C")
                .arg(self.host.home.join("repos/pi-rukas"))
                .args(["rev-parse", "HEAD"])
                .stderr(Stdio::null()),
        );
        if rc != 0 || clone.is_empty() || clone == image {
            return;
        }
        let short: String = image.chars().take(7).collect();
        say(&format!("later: {tree}/bootstrap.sh user   (pi-rukas moved to {short}; until it runs, --check says Dependencies: NOT OK)"));
    }
}

/// The status of a run install.sh did not finish. In check mode nothing was
/// meant to run: its answer is its report.
fn install_failed(run: &Run, plan: &Plan, rc: i32, out: &str, unknown: &[String]) -> u8 {
    let check = plan.mode == Mode::Check;
    run.warn_unknown(unknown);
    if rc == 124 {
        if check {
            tell(&format!("install: install.sh --check gave no answer in {INSTALL_LIMIT}s and was stopped; it changes nothing, so nothing was installed."));
        } else {
            tell(&format!("install: install.sh gave no answer in {INSTALL_LIMIT}s and was stopped; what it had copied is installed, and any follow-up it recorded in {} is still owed.", run.file));
        }
        return 1;
    }
    if check {
        let mut found = false;
        if found_to_update(out) {
            tell("install: install.sh --check found something to update");
            found = true;
        }
        if found_not_to_fix(out) {
            tell("install: install.sh --check found something tehas update does not fix; its lines above say what");
            found = true;
        }
        if !found {
            tell(&format!("install: install.sh --check exited {rc}"));
        }
    } else {
        tell(&format!(
            "install: install.sh exited {rc}; no follow-up was run"
        ));
    }
    // The statuses install.sh documents. Anything else is a step that failed:
    // bash answers 126 or 127 when the file lost its exec bit.
    if rc == 78 {
        78
    } else {
        1
    }
}

/// The follow-ups of a run: the lines install.sh printed and the ones its record
/// holds, which are not the same list, since a run that died partway records
/// what it copied and prints no report.
fn follow_ups(run: &mut Run, plan: &Plan, out: &str) -> u8 {
    let mut given: Vec<String> = Vec::new();
    let mut add = |line: &str| {
        if !line.is_empty() && !given.iter().any(|have| have == line) {
            given.push(line.to_string());
        }
    };
    next_lines(out).for_each(&mut add);
    let Ok(record) = read_record(&run.host.home) else {
        tell("update: the follow-up record is there but cannot be read; no follow-up was run, and what install.sh copied stays installed");
        return 1;
    };
    record.iter().for_each(|line| add(line));

    let (asked, unknown): (Vec<String>, Vec<String>) =
        given.into_iter().partition(|line| steps::valid(line));
    run.warn_unknown(&unknown);

    if plan.mode == Mode::Check {
        // Nothing was merged, installed, built or started. What is left to do is
        // the checkout moving, and any step an earlier run never finished.
        let mut found = plan.found || !plan.leftover.is_empty() || !unknown.is_empty();
        if run.follows_rukas() && !run.rukas_step("--check") {
            found = true;
        }
        return u8::from(found);
    }

    let mut steps = sorted(&asked);
    if plan.reused {
        steps.retain(|line| steps::step_id(line.split(' ').next().unwrap_or("")) != "sandbox");
        run.built = true;
    }
    let mut status = u8::from(!unknown.is_empty());
    if !run.write_pending(&steps, &unknown) {
        return 1;
    }
    let mut ran = 0;
    for step in &steps {
        if !run.step(step) {
            status = 1;
            break;
        }
        ran += 1;
        if !run.write_pending(&steps[ran..], &unknown) {
            status = 1;
            break;
        }
    }
    if ran < steps.len() {
        tell(&format!(
            "update: {} of {} follow-up(s) are not done, recorded in {}; tehas update runs them again",
            steps.len() - ran,
            steps.len(),
            run.file
        ));
    }
    // --pending asked no remote anything, so a head it did not ask about is not
    // something it can be said to have missed.
    if plan.mode != Mode::Pending
        && status == 0
        && !run.built
        && run.follows_rukas()
        && !run.rukas_step("--if-moved")
    {
        tell("pi-rukas: the image was not moved to the commit versions.env follows; tehas update tries again");
        status = 1;
    }
    if status == 0 && plan.mode == Mode::Update && run.follows_rukas() && !asked_for_bootstrap(out)
    {
        run.rukas_later(&plan.tree);
    }
    if status == 0 && plan.from == "release" {
        release::keep(run.host);
    }
    status
}

/// `console update finish --plan <file> --install-out <file> --install-err
/// <file> --install-rc <n>`, by the console install.sh left. --pending runs no
/// install.sh, so of those it gives only the plan. Exit: the status of the run.
pub fn main(given: &[String]) -> u8 {
    let (mut file, mut out, mut err, mut rc) = (None, None, None, None);
    let mut words = given.iter();
    while let Some(word) = words.next() {
        let slot = match word.as_str() {
            "--plan" => &mut file,
            "--install-out" => &mut out,
            "--install-err" => &mut err,
            "--install-rc" => &mut rc,
            _ => {
                tell(&format!("console update finish: unknown argument: {word}"));
                return 64;
            }
        };
        *slot = words.next().map(String::as_str);
    }
    let Some(plan) = file.and_then(|f| Plan::read(Path::new(f))) else {
        tell("console update finish: --plan names no plan");
        return 64;
    };
    let host = match host_or_refuse() {
        Ok(host) => host,
        Err(rc) => return rc,
    };
    let mut run = Run {
        host: &host,
        file: record_file(&host.home).display().to_string(),
        built: false,
    };
    if plan.mode == Mode::Pending {
        let lines: String = plan
            .leftover
            .iter()
            .map(|l| format!("next: {l}\n"))
            .collect();
        return follow_ups(&mut run, &plan, trimmed(&lines));
    }
    let rc: i32 = rc.and_then(|n| n.parse().ok()).unwrap_or(1);
    let raw = out.and_then(|f| fs::read(f).ok()).unwrap_or_default();
    let _ = io::stdout().write_all(&report(&raw));
    if let Some(words) = err.and_then(|f| fs::read(f).ok()) {
        let _ = io::stderr().write_all(&words);
    }
    let text = String::from_utf8_lossy(&raw);
    let text = trimmed(&text);
    if rc != 0 {
        let unknown = plan.unknown.clone();
        return install_failed(&run, &plan, rc, text, &unknown);
    }
    follow_ups(&mut run, &plan, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_and_this_console_agree() {
        let script = include_str!("../../../libexec/update");
        assert!(script.contains(&format!("timeout {INSTALL_LIMIT} \"${{run[@]}}\"")));
        assert!(script.contains(&format!("7>&-)\" != {} ]", super::super::API)));
    }

    #[test]
    fn a_check_line_is_read_by_its_label_and_its_word() {
        assert!(says(
            "Installed:    DRIFT (1 files)",
            "Installed:",
            &["DRIFT"]
        ));
        assert!(!says("Installed:DRIFT", "Installed:", &["DRIFT"]));
        assert!(!says("  Installed:    DRIFT", "Installed:", &["DRIFT"]));
        assert!(found_to_update("x\nFollow-ups:   NOT DONE (1)\n"));
        assert!(!found_to_update("Installed:    OK\n"));
        for line in [
            "Dependencies: NOT OK",
            "Slice:        MISSING (sudo)",
            "Slice:        DIFFERS (sudo)",
            "Login:        NOT CURRENT",
            "Tests:        FAILED",
        ] {
            assert!(found_not_to_fix(line), "{line}");
        }
        assert!(!found_not_to_fix("Dependencies: OK\nSlice:        OK\n"));
    }

    #[test]
    fn what_install_sh_printed_is_shown_but_a_follow_up_line() {
        assert_eq!(
            report(b"installed: a\nnext: sandbox\n\nlater: b\nnext:x\n\n"),
            b"installed: a\n\nlater: b\n"
        );
        assert_eq!(report(b"\n\n"), b"");
        assert_eq!(report(b""), b"");
    }

    #[test]
    fn only_a_line_of_next_and_a_space_is_a_follow_up() {
        let out = "next: sandbox\nnext:x\n next: y\nnext: docs --force-recreate";
        assert_eq!(
            next_lines(out).collect::<Vec<_>>(),
            ["sandbox", "docs --force-recreate"]
        );
    }

    #[test]
    fn the_later_line_is_recognised_whatever_it_names() {
        assert!(asked_for_bootstrap(
            "later: /x/bootstrap.sh user   (versions.env changed)"
        ));
        assert!(!asked_for_bootstrap("bootstrap.sh user\nlater: y"));
        assert!(!asked_for_bootstrap("installed: x"));
    }

    #[test]
    fn the_word_lists_keep_the_family_of_active_apart_from_the_rest() {
        for word in [
            "active",
            "activating",
            "reloading",
            "refreshing",
            "maintenance",
            "deactivating",
        ] {
            assert!(ACTIVE.contains(&word));
        }
        for word in ["inactive", "failed", "unknown"] {
            assert!(NOT_ACTIVE.contains(&word));
        }
        assert!(!ACTIVE.contains(&"systemd is not running"));
        assert!(!NOT_ACTIVE.contains(&""));
    }
}
