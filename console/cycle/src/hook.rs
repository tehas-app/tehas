//! The seam of the hook after work (docs/specs/2026-11-21-plugins-hooks.md,
//! "When it runs"): once the report is done, a request for each plugin
//! granted `hook:after:work` and ready, and its unit started. It waits for
//! nothing and holds no slot, lock or label: the unit runs the hook after
//! the cycle has ended.

use crate::after::log;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use tehas_core::call::{run_full, GH_BOUND, GRACE};
use tehas_core::plugin::hook::{self, Ending, HookAsk};

/// `systemctl --user start --no-block` waits for no job, so it answers soon.
const START_BOUND: Duration = Duration::from_secs(30);

/// Each request written and its unit started by `start`. A failure is logged
/// and the next is asked all the same.
pub fn ask_each(
    home: &Path,
    issue: u64,
    asked: &[(String, HookAsk)],
    start: &mut dyn FnMut(&str) -> Result<(), String>,
) {
    for (name, ask) in asked {
        let unit = hook::unit(name, ask.pr);
        match hook::write_request(home, name, ask).and_then(|()| start(&unit)) {
            Ok(()) => log(&format!(
                "#{issue}: asked {unit} on {} of #{}",
                ask.head, ask.pr
            )),
            Err(why) => log(&format!(
                "WARN: #{issue}: the hook of {name} on #{} was not asked: {why}",
                ask.pr
            )),
        }
    }
}

fn systemctl_start(unit: &str) -> Result<(), String> {
    let args: Vec<String> = ["--user", "start", "--no-block", unit]
        .iter()
        .map(|a| a.to_string())
        .collect();
    let r = run_full("systemctl", &args, None, START_BOUND, GRACE, &[]);
    if r.ok() {
        Ok(())
    } else {
        Err(format!(
            "systemctl could not start {unit}: {}",
            r.err.trim()
        ))
    }
}

/// The hooks of the cycle of `issue` that ended as `e`. `get` reads the
/// settings the cycle started with.
pub fn ask(home: &Path, issue: u64, e: &Ending, get: &dyn Fn(&str) -> String) {
    let names = hook::after_work(e, get, &|name: &str| hook::read_state(home, name));
    let Some(pr) = e.pr.filter(|_| !names.is_empty()) else {
        return;
    };
    let repo = get("TEHAS_REPO");
    let args = vec!["api".to_string(), format!("repos/{repo}/pulls/{pr}")];
    let read = run_full("gh", &args, None, GH_BOUND, GRACE, &[]);
    let pull = read
        .ok()
        .then(|| serde_json::from_str::<Value>(&read.out).ok())
        .flatten();
    let Some(pull) = pull else {
        log(&format!(
            "WARN: #{issue}: pull request #{pr} could not be read: no plugin's hook is asked"
        ));
        return;
    };
    let asked = hook::requests(&names, &pull, &repo, &get("TEHAS_BOT_LOGIN"), pr, issue);
    ask_each(home, issue, &asked, &mut systemctl_start);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::plugin::run::Trigger;
    use tehas_core::testdir::TestDir;

    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210fedcba98";

    fn asked(name: &str) -> (String, HookAsk) {
        let ask = HookAsk {
            pr: 42,
            head: H.into(),
            base: B.into(),
            issue: Some(7),
            trigger: Trigger::Cycle,
        };
        (name.to_string(), ask)
    }

    #[test]
    fn each_request_is_written_and_its_unit_started_without_waiting() {
        let home = TestDir::new("cycle-hook-ask");
        let mut started = Vec::new();
        let all = [asked("floorplan"), asked("spec")];
        ask_each(&home, 7, &all, &mut |unit: &str| {
            started.push(unit.to_string());
            if unit.contains("spec") {
                Err("refused".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(
            started,
            [
                "tehas-plugin-hook@floorplan-42.service",
                "tehas-plugin-hook@spec-42.service"
            ]
        );
        for (name, ask) in &all {
            assert_eq!(hook::read_request(&home, name, 42).as_ref(), Some(ask));
        }
    }

    #[test]
    fn a_cycle_that_asks_no_plugin_asks_github_nothing() {
        let home = TestDir::new("cycle-hook-none");
        let e = Ending {
            signalled: true,
            refused: false,
            pr: Some(42),
        };
        // No plugin is granted, and plugins are off: nothing is read or
        // written, so no gh runs.
        ask(&home, 7, &e, &|_| String::new());
        assert!(!home.join("plugins").exists());
    }
}
