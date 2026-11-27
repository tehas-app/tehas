//! The plugins' bridge (docs/specs/2026-11-22-events.md, "Plugins"): an
//! `on_any` listener of the cycle's events. On `change.offered` it writes a
//! request for each plugin granted `hook:after:change.offered` and ready, for the
//! change while it is open at its head, and starts its unit
//! (docs/specs/2026-11-21-plugins-hooks.md, "When it runs"). It waits for
//! nothing and holds no slot, lock or label: the unit runs the hook after
//! the cycle has ended. It never stops an event, so no plugin hides one
//! from another. The change is read on the code host port, by its number,
//! through the handle the cycle's wiring in src/main.rs makes.

use crate::after::log;
use crate::events::Get;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tehas_core::call::{run_full, GRACE};
use tehas_core::plugin::hook::{self, HookAsk};
use tehas_ports::events::{ChangeOffered, Envelope, Event, Heard};
use tehas_ports::{Bound, ChangeFilter, ChangeNo, ChangeStates, CodeHost, Got};

/// The bridge's id in the cycle's dispatcher.
pub const BRIDGE: &str = "plugin-hooks";

/// A code host handle on the project `TEHAS_REPO` names, acting as the
/// factory's account. Made only when a plugin is asked for a hook.
pub type CodeHostOf = Arc<dyn Fn(&str) -> Handle + Send + Sync>;

/// A code host handle, or why none is made.
type Handle = Got<Box<dyn CodeHost + Send>>;

/// `systemctl --user start --no-block` waits for no job, so it answers soon.
const START_BOUND: Duration = Duration::from_secs(30);

/// Each request written and its unit started by `start`. A failure is logged
/// and the next is asked all the same.
fn ask_each(
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

/// The bridge: on `change.offered`, the hooks of the plugins granted one.
/// `get` reads the settings the cycle started with.
pub fn bridge(
    home: PathBuf,
    get: Get,
    code_host: CodeHostOf,
) -> impl Fn(&Envelope) -> Heard + Send + Sync + 'static {
    move |e: &Envelope| {
        if let Event::ChangeOffered(offer) = &e.event {
            let names = hook::granted(&*get, &|name: &str| hook::read_state(&home, name));
            if !names.is_empty() {
                ask(
                    &home,
                    offer,
                    e,
                    &names,
                    &*get,
                    &*code_host,
                    &mut systemctl_start,
                );
            }
        }
        Heard::Go
    }
}

/// The one change of `no`, in any state: one question where the site
/// allows.
fn one_change(no: ChangeNo) -> ChangeFilter {
    ChangeFilter {
        states: ChangeStates::All,
        numbers: vec![no],
        ..ChangeFilter::default()
    }
}

/// The hooks of `names` on the change `offer` names, when it is open at its
/// head, each asked with the envelope `e` and its unit started by `start`.
fn ask(
    home: &Path,
    offer: &ChangeOffered,
    e: &Envelope,
    names: &[String],
    get: &dyn Fn(&str) -> String,
    code_host: &dyn Fn(&str) -> Handle,
    start: &mut dyn FnMut(&str) -> Result<(), String>,
) {
    let (issue, pr) = (offer.blueprint.get(), offer.change.get());
    let one = Bound::new(1).expect("one");
    let read =
        code_host(&get("TEHAS_REPO")).and_then(|mut h| h.changes(&one_change(offer.change), one));
    let Some(change) = read.ok().and_then(|w| w.into_items().into_iter().next()) else {
        log(&format!(
            "WARN: #{issue}: pull request #{pr} could not be read: no plugin's hook is asked"
        ));
        return;
    };
    let asked = hook::requests(names, &change, (pr, issue), e);
    ask_each(home, issue, &asked, start);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::after;
    use tehas_core::plugin::run::Trigger;
    use tehas_core::testdir::TestDir;
    use tehas_ports::fake::{ChangeSpec, CodeHostOp, FakeSite, Fault, Faulty, Op, World};
    use tehas_ports::{Actor, At, BlueprintNo, Caller, ChangeState, Scope, Site, ALL_MAX};

    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    const B: &str = "fedcba9876543210fedcba9876543210fedcba98";

    fn asked(name: &str) -> (String, HookAsk) {
        let ask = HookAsk {
            pr: 42,
            head: H.into(),
            base: B.into(),
            issue: Some(7),
            trigger: Trigger::Cycle,
            event: tehas_ports::events::samples()
                .into_iter()
                .find(|s| s.name == tehas_ports::events::Name::ChangeOffered)
                .map(|s| s.full),
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

    /// The offer of change 42 for issue 7.
    fn offered() -> Envelope {
        Envelope::new(ChangeOffered::new(
            ChangeNo::new(42).unwrap(),
            BlueprintNo::new(7).unwrap(),
            tehas_ports::events::CycleId::read("7-1").unwrap(),
            tehas_ports::events::End::Offered,
            At::new(1_763_820_000),
        ))
    }

    /// Change 42 of the factory's account from `feature/issue-7`, open,
    /// built further by `f`.
    fn world(f: impl FnOnce(ChangeSpec) -> ChangeSpec) -> World {
        World::new().change(42, "feature/issue-7", f)
    }

    /// The code host of `w` as the factory's account, on acme/widget.
    fn code_host(w: &World) -> impl Fn(&str) -> Handle + '_ {
        move |repo: &str| {
            assert_eq!(repo, "acme/widget");
            FakeSite::new(w).code_host(&Actor::Ours(Scope::Full), Caller::new("plugin-hooks"))
        }
    }

    fn repo(name: &str) -> String {
        if name == "TEHAS_REPO" {
            "acme/widget".into()
        } else {
            String::new()
        }
    }

    /// What `ask` logged and started for `names` on the offer of change 42,
    /// read through `host`.
    fn asked_through(
        home: &Path,
        names: &[String],
        host: &dyn Fn(&str) -> Handle,
    ) -> (Vec<String>, String) {
        let e = offered();
        let Event::ChangeOffered(offer) = &e.event else {
            unreachable!()
        };
        let mut started = Vec::new();
        let ((), logged) = after::holding(|| {
            ask(home, offer, &e, names, &repo, host, &mut |unit: &str| {
                started.push(unit.to_string());
                Ok(())
            })
        });
        (started, logged)
    }

    #[test]
    fn an_open_change_of_ours_is_read_on_the_code_host_and_asked_at_its_head() {
        let home = TestDir::new("cycle-hook-open");
        let w = world(|c| c);
        let names = ["floorplan".to_string(), "spec".to_string()];
        let (started, logged) = asked_through(&home, &names, &code_host(&w));
        assert_eq!(
            started,
            [
                "tehas-plugin-hook@floorplan-42.service",
                "tehas-plugin-hook@spec-42.service"
            ]
        );
        assert!(!logged.contains("WARN"), "{logged}");
        let c = FakeSite::new(&w)
            .code_host(&Actor::Operator, Caller::new("test"))
            .unwrap()
            .change(ChangeNo::new(42).unwrap(), ALL_MAX)
            .unwrap()
            .digest;
        for name in &names {
            let ask = hook::read_request(&home, name, 42).unwrap();
            assert_eq!(ask.head, c.head_commit.as_str());
            assert_eq!(ask.base, c.base_commit.as_str());
            assert_eq!((ask.issue, ask.trigger), (Some(7), Trigger::Cycle));
            assert_eq!(ask.event, Some(offered()));
        }
    }

    #[test]
    fn a_change_closed_merged_or_not_ours_asks_no_hook_and_says_nothing() {
        let closed = ChangeState::Closed { at: At::new(1) };
        let merged = ChangeState::Merged {
            at: At::new(1),
            by: None,
            commit: None,
        };
        for (what, w) in [
            ("closed", world(|c| c.state(closed))),
            ("merged", world(|c| c.state(merged))),
            ("someone's", world(|c| c.author("someone"))),
        ] {
            let home = TestDir::new("cycle-hook-not-open");
            let names = ["floorplan".to_string()];
            let (started, logged) = asked_through(&home, &names, &code_host(&w));
            assert!(started.is_empty(), "{what}");
            assert_eq!(logged, "", "{what}");
            assert!(!home.join("plugins").exists(), "{what}");
        }
    }

    #[test]
    fn a_change_that_cannot_be_read_asks_no_hook_and_says_so() {
        let warn = "WARN: #7: pull request #42 could not be read: no plugin's hook is asked";
        let names = ["floorplan".to_string()];
        let none = World::new();
        let w = world(|c| c);
        let down =
            Faulty::new(FakeSite::new(&w)).on(Op::CodeHost(CodeHostOp::Changes), Fault::Timeout);
        let down = |_: &str| down.code_host(&Actor::Ours(Scope::Full), Caller::new("plugin-hooks"));
        let unmade = |_: &str| -> Handle {
            Err(tehas_ports::Outcome::Refused(
                tehas_ports::Refusal::Unsupported,
            ))
        };
        let unread = |what: &str, host: &dyn Fn(&str) -> Handle| {
            let home = TestDir::new("cycle-hook-unread");
            let (started, logged) = asked_through(&home, &names, host);
            assert!(started.is_empty(), "{what}");
            assert!(logged.contains(warn), "{what}: {logged}");
            assert!(!home.join("plugins").exists(), "{what}");
        };
        unread("no such change", &code_host(&none));
        unread("down", &down);
        unread("no handle", &unmade);
    }

    #[test]
    fn an_offer_no_plugin_is_granted_asks_github_nothing() {
        let home = TestDir::new("cycle-hook-none");
        let offer = ChangeOffered::new(
            tehas_ports::ChangeNo::new(42).unwrap(),
            tehas_ports::BlueprintNo::new(7).unwrap(),
            tehas_ports::events::CycleId::read("7-1").unwrap(),
            tehas_ports::events::End::Offered,
            tehas_ports::At::new(1_763_820_000),
        );
        // No plugin is granted, and plugins are off: nothing is read or
        // written, and no code host handle is made.
        let listen = bridge(
            home.to_path_buf(),
            Arc::new(|_: &str| String::new()),
            Arc::new(|_: &str| -> Handle { panic!("no handle is made") }),
        );
        assert_eq!(listen(&Envelope::new(offer)), Heard::Go);
        assert!(!home.join("plugins").exists());
    }
}
