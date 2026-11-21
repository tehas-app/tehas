//! `tehasbot plugin tick`, the step of `tehas-plugin.timer`
//! (docs/specs/2026-11-21-plugins.md, "Runs"): a refresh of each ready
//! plugin whose last refresh began at another head of the default branch.
//! The head is the one the poll fetched into the product's mirror: the tick
//! asks GitHub nothing. Nothing starts while the factory is paused, drained or
//! read-only, or while `tehas update` runs, and the log says why. Each
//! refresh is its own unit, asked for and not waited for. The tick also
//! removes what runs that died left: their containers, volumes and
//! directories, and ends their rows as lost.

use crate::host::Host;
use crate::run::{clean, default_head, Setup};
use std::fs;
use std::path::Path;
use tehas_core::lock::{probe, Probe};
use tehas_core::plugin::state::Word;
use tehas_core::plugin::{self, is_name};
use tehas_core::{features, pause};
use tehas_ops::plugin::prepare;
use tehas_ops::plugin::runs::{self, Asked};

fn say(line: &str) {
    tehas_core::outln!("tick: {line}");
}

/// What holds every refresh back, if anything.
pub fn held(home: &Path, get: &dyn Fn(&str) -> String) -> Option<String> {
    if get(tehas_core::read_only::NAME) == "1" {
        return Some("the factory is read-only (TEHAS_READ_ONLY)".into());
    }
    let flag = pause::read(&home.join("state/pause.json"));
    if flag.state != "none" {
        return Some("the factory is paused (state/pause.json)".into());
    }
    if tehas_ops::holds::home_drained(home) {
        return Some("the factory is drained (tehas drain)".into());
    }
    if matches!(
        probe(&home.join("state/update.lock")),
        Probe::Held | Probe::Unknown(_)
    ) {
        return Some("tehas update runs".into());
    }
    None
}

/// `tehas-plugin-<name>-<run>` back to its name and run.
fn run_of(container: &str) -> Option<(String, u64)> {
    let rest = container.strip_prefix("tehas-plugin-")?;
    let (name, id) = rest.rsplit_once('-')?;
    let ok = is_name(name) && !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| Some((name.to_string(), id.parse().ok()?)))
        .flatten()
}

/// The containers and directories of runs whose plugin holds no lock: each
/// removed, and its row ended as lost.
pub fn sweep(h: &mut dyn Host, s: &Setup) {
    let mut dead: Vec<(String, u64)> = Vec::new();
    let listed = h.exec().run(&tehas_ops::plugin::exec::Cmd::new(
        "docker",
        [
            "ps",
            "-a",
            "--filter",
            "name=^tehas-plugin-",
            "--format",
            "{{.Names}}",
        ]
        .iter()
        .map(|a| a.to_string())
        .collect(),
    ));
    if listed.ok {
        dead.extend(listed.text().lines().filter_map(run_of));
    } else {
        say(&format!("docker ps failed: {}", listed.err));
    }
    if let Ok(dirs) = fs::read_dir(prepare::root(s.home)) {
        for d in dirs.flatten() {
            let name = d.file_name().to_string_lossy().into_owned();
            if !is_name(&name) {
                continue;
            }
            for r in fs::read_dir(d.path()).into_iter().flatten().flatten() {
                let f = r.file_name().to_string_lossy().into_owned();
                if let Some(id) = f.strip_prefix("run-").and_then(|i| i.parse().ok()) {
                    dead.push((name.clone(), id));
                }
            }
        }
    }
    dead.sort();
    dead.dedup();
    for (name, id) in dead {
        // A call's run is not the runner's, and is not held by its lock.
        if runs::running(s.home, &name) || runs::calling(s.home, &name) {
            continue;
        }
        clean(h.exec(), &name, &format!("tehas-plugin-{name}-{id}"), id);
        let _ = fs::remove_dir_all(prepare::dir(s.home, &name).join(format!("run-{id}")));
        let id_word = id.to_string();
        let ended = h.store(&["plugin-run-end", "--run", &id_word, "--result", "lost"]);
        let row = match ended {
            Ok(_) => "ended as lost".to_string(),
            Err(why) => format!("its row was not ended: {why}"),
        };
        say(&format!(
            "plugin {name}: run {id} died: its container and files are removed, {row}"
        ));
    }
}

/// The tick: always 0, and one line for each thing it did or did not do.
pub fn tick(h: &mut dyn Host, s: &Setup) -> u8 {
    let get = s.get;
    if !features::on_in("plugins", get) {
        say("plugins are off: nothing to do");
        return 0;
    }
    if let Some(why) = held(s.home, get) {
        say(&format!("no refresh starts: {why}"));
        return 0;
    }
    sweep(h, s);
    let head = match default_head(h.exec(), get) {
        Ok(c) => c,
        Err(why) => {
            say(&format!(
                "no refresh starts: the default branch's head: {why}"
            ));
            return 0;
        }
    };
    for pin in plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default() {
        let name = &pin.name;
        let state = prepare::read_state(s.home, name).filter(|st| st.commit == pin.commit);
        match state {
            Some(st) if st.state == Word::Ready && st.image.is_some() => {}
            Some(st) => {
                say(&format!(
                    "plugin {name}: {}, not refreshed",
                    st.state.word()
                ));
                continue;
            }
            None => {
                say(&format!("plugin {name}: unbuilt at its pin, not refreshed"));
                continue;
            }
        }
        let tried = fs::read_to_string(runs::tried(s.home, name)).unwrap_or_default();
        if tried.trim() == head {
            continue;
        }
        if runs::running(s.home, name) {
            say(&format!("plugin {name}: running"));
            continue;
        }
        let asked = Asked {
            trigger: plugin::run::Trigger::Timer,
            by: "timer".into(),
        };
        match runs::ask_refresh(h.exec(), s.home, name, &asked) {
            Ok(()) => say(&format!("plugin {name}: refresh asked at {head}")),
            Err(why) => say(&format!("plugin {name}: {why}")),
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::tests::{world, Fake, World, H};
    use std::time::Duration;
    use tehas_ops::plugin::exec::fake::ok;

    fn go(h: &mut Fake, w: &World) -> u8 {
        let get = w.get();
        let s = Setup {
            home: &w.home,
            get: &get,
            models: &w.models,
            poll: Duration::from_millis(10),
        };
        tick(h, &s)
    }

    const START: &str = "systemctl --user start --no-block tehas-plugin@demo.service";

    #[test]
    fn a_new_head_asks_for_a_refresh_and_the_same_head_does_not() {
        let mut h = Fake::new();
        let w = world("plugin-tick-head", &mut h, "", "product");
        assert_eq!(go(&mut h, &w), 0);
        assert!(h.x.ran_any(START));
        assert_eq!(
            runs::read_request(&w.home, "demo"),
            Some(Asked {
                trigger: plugin::run::Trigger::Timer,
                by: "timer".into()
            })
        );
        fs::write(runs::tried(&w.home, "demo"), format!("{H}\n")).unwrap();
        h.x.ran.clear();
        assert_eq!(go(&mut h, &w), 0);
        assert!(!h.x.ran_any("systemctl"));
    }

    #[test]
    fn a_refresh_that_runs_is_not_asked_again_and_its_container_stays() {
        let mut h = Fake::new();
        let w = world("plugin-tick-running", &mut h, "", "");
        h.x.on("docker ps", ok("tehas-plugin-demo-3\n"));
        let lock = std::fs::File::create(runs::refresh_lock(&w.home, "demo")).unwrap();
        lock.lock().unwrap();
        assert_eq!(go(&mut h, &w), 0);
        assert!(!h.x.ran_any("systemctl"));
        assert!(!h.x.ran_any("docker rm"));
        assert!(h.calls.is_empty());
    }

    #[test]
    fn what_a_dead_run_left_is_removed_and_its_row_ended_as_lost() {
        let mut h = Fake::new();
        let w = world("plugin-tick-sweep", &mut h, "", "");
        h.x.on("docker ps", ok("tehas-plugin-demo-3\nother\n"));
        let left = prepare::dir(&w.home, "demo").join("run-5");
        fs::create_dir_all(left.join("product")).unwrap();
        assert_eq!(go(&mut h, &w), 0);
        assert!(h.x.ran_any("docker rm -f tehas-plugin-demo-3"));
        assert!(h
            .x
            .ran_any("docker volume rm -f tehas-plugin-demo-work-5 tehas-plugin-demo-out-5"));
        assert!(!left.exists());
        assert_eq!(
            h.calls,
            [
                "plugin-run-end --run 3 --result lost",
                "plugin-run-end --run 5 --result lost"
            ]
        );
    }

    #[test]
    fn a_call_that_runs_is_left_to_end_itself() {
        let mut h = Fake::new();
        let w = world("plugin-tick-call", &mut h, "", "");
        h.x.on("docker ps", ok("tehas-plugin-demo-3\n"));
        let calls = runs::calls(&w.home, "demo");
        fs::create_dir_all(&calls).unwrap();
        let slot = tehas_core::lock::try_lock(&calls.join("slot-1.lock"))
            .unwrap()
            .unwrap();
        go(&mut h, &w);
        assert!(!h.x.ran_any("docker rm"));
        assert!(h.calls.is_empty());
        drop(slot);
        go(&mut h, &w);
        assert!(h.x.ran_any("docker rm -f tehas-plugin-demo-3"));
    }

    #[test]
    fn nothing_starts_while_the_factory_is_held_back() {
        let mut h = Fake::new();
        let mut w = world("plugin-tick-held", &mut h, "", "");
        fs::create_dir_all(w.home.join("state")).unwrap();
        fs::write(w.home.join("state/pause.json"), "not a flag").unwrap();
        assert_eq!(go(&mut h, &w), 0);
        assert!(h
            .x
            .ran
            .iter()
            .all(|l| !l.starts_with("docker") && !l.starts_with("systemctl")));
        fs::remove_file(w.home.join("state/pause.json")).unwrap();
        let lock = std::fs::File::create(w.home.join("state/update.lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(
            held(&w.home, &w.get()).as_deref(),
            Some("tehas update runs")
        );
        drop(lock);
        w.set("TEHAS_READ_ONLY", "1");
        assert!(held(&w.home, &w.get()).unwrap().contains("read-only"));
        w.set("TEHAS_READ_ONLY", "");
        fs::write(
            w.home.join("state/limit.json"),
            r#"{"v":1,"cycles":0,"plan":false,"until":null,"reason":"move"}"#,
        )
        .unwrap();
        assert!(held(&w.home, &w.get()).unwrap().contains("drained"));
        fs::remove_file(w.home.join("state/limit.json")).unwrap();
        assert_eq!(held(&w.home, &w.get()), None);
        w.set("TEHAS_PLUGIN", "off");
        h.x.ran.clear();
        assert_eq!(go(&mut h, &w), 0);
        assert!(h.x.ran.is_empty());
    }

    #[test]
    fn a_container_name_gives_its_plugin_and_run() {
        assert_eq!(
            run_of("tehas-plugin-floorplan-12"),
            Some(("floorplan".into(), 12))
        );
        for bad in [
            "tehas-plugin-floorplan",
            "tehas-plugin-floor-plan-x",
            "tehas-plugin--3",
            "tehas-plugin-Floor-3",
            "other-floorplan-3",
        ] {
            assert_eq!(run_of(bad), None, "{bad}");
        }
    }
}
