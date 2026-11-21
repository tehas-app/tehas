//! The plugins' part of `tehas update`: with the feature on, each plugin of
//! TEHAS_PLUGINS fetched, checked and built when its image of the pin is not
//! there, its repository pruned to the pin's commit, and what is left of a
//! plugin taken out of the settings removed
//! (docs/specs/2026-11-21-plugins.md, guarantees 4 and 9). A plugin that is
//! refused is skipped, never the update; the caller exits 1 at the end.
//! Then the dock: a panel `p-<name>` for each plugin ready with `view`
//! granted and a view in its manifest, and none for any other
//! (docs/specs/2026-11-21-plugins-web.md, "The panel"). With the feature off
//! it runs nothing, and only takes away a plugin's panel left from before.

use super::exec::Exec;
use super::prepare::{self, prepare_view, Shown};
use crate::panel::{self, Panel};
use std::fs;
use std::path::Path;
use tehas_core::plugin::state::Word;
use tehas_core::plugin::{self, is_name, needs, Cap, Pin};
use tehas_core::settings::rules::switch;

/// What the step said, and whether a plugin was refused.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Said {
    pub lines: Vec<String>,
    pub refused: bool,
}

/// A plugin as `needs::resolve` reads it: the needs of its manifest at the
/// pin, when the pin is fetched, and what its grants give.
fn weighed(
    x: &mut dyn Exec,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    pin: &Pin,
    state: needs::State,
) -> needs::Plugin {
    let grants = plugin::grants(&get(&plugin::grants_setting(&pin.name)))
        .ok()
        .filter(|g| g.commit == pin.commit);
    let caps = grants.map(|g| g.caps).unwrap_or_default();
    let needs = prepare::read(x, home, &pin.name, &pin.commit)
        .ok()
        .and_then(|r| r.manifest)
        .map(|m| m.needs)
        .unwrap_or_default();
    needs::Plugin {
        name: pin.name.clone(),
        state,
        needs,
        granted: caps
            .iter()
            .filter_map(|c| match c {
                Cap::Needs(n) => Some(n.clone()),
                _ => None,
            })
            .collect(),
        offers: caps
            .iter()
            .filter_map(|c| match c {
                Cap::Skill(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
    }
}

pub fn step(x: &mut dyn Exec, home: &Path, get: &dyn Fn(&str) -> String) -> Said {
    let mut said = Said::default();
    if !tehas_core::features::on_in("plugins", get) {
        docked(home, &[], &mut said);
        menued(home, &[], &mut said);
        return said;
    }
    let domain = get("TEHAS_DOMAIN");
    let one_level = switch(&get("TEHAS_EDGE_ONE_LEVEL")) == Some(true);
    let mut views = Vec::new();
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    // Dependencies first, by the needs of each manifest a fetch already
    // brought; then the needs weighed again on what the builds came to.
    let first: Vec<needs::Plugin> = pins
        .iter()
        .map(|p| weighed(x, home, get, p, needs::State::Ready))
        .collect();
    let mut states = Vec::new();
    for (name, _) in needs::resolve(&first) {
        let Some(pin) = pins.iter().find(|p| p.name == name) else {
            continue;
        };
        let grants = plugin::grants(&get(&plugin::grants_setting(&pin.name))).ok();
        let (s, view) = prepare_view(x, home, pin, grants.as_ref());
        if let Err(why) = prepare::prune(x, home, pin) {
            said.lines.push(format!("plugin {}: {why}", pin.name));
        }
        let caps = grants.map(|g| g.caps).unwrap_or_default();
        states.push((pin, s, view, caps));
    }
    let built: Vec<needs::Plugin> = states
        .iter()
        .map(|(pin, s, _, _)| {
            let state = if s.state == Word::Ready {
                needs::State::Ready
            } else {
                needs::State::Refused
            };
            weighed(x, home, get, pin, state)
        })
        .collect();
    let whys = needs::resolve(&built);
    let mut menu = Vec::new();
    for (pin, mut s, shown, caps) in states {
        let why = whys
            .iter()
            .find(|(n, _)| *n == pin.name)
            .and_then(|(_, w)| w.clone());
        if let (Word::Ready, Some(why)) = (s.state, why) {
            s.state = Word::Refused;
            s.refusal = Some(why);
            if let Err(e) = prepare::write_state(home, &pin.name, &s) {
                said.lines
                    .push(format!("plugin {}: state.json: {e}", pin.name));
            }
        }
        // A view docks only for a plugin still ready once its needs are
        // weighed.
        if let (Word::Ready, Some(shown), false) = (s.state, shown, domain.is_empty()) {
            let base = plugin::view_url(&pin.name, &domain, one_level);
            if caps.contains(&Cap::Menu) {
                if let Some(entry) = listed(&pin.name, &shown, &caps, &base) {
                    menu.push(entry);
                }
            } else if let (Some(label), true) = (shown.panel, caps.contains(&Cap::View)) {
                views.push(Panel {
                    id: format!("{}{}", plugin::PANEL_PREFIX, pin.name),
                    label,
                    url: base,
                });
            }
        }
        let line = match (s.state, &s.image, &s.refusal) {
            (Word::Ready, Some(id), _) => format!("plugin {}: ready, image {id}", pin.name),
            (Word::Ready, None, _) => format!("plugin {}: ready, no image", pin.name),
            (_, _, why) => {
                said.refused = true;
                format!(
                    "plugin {}: refused and left off: {}",
                    pin.name,
                    why.as_deref().unwrap_or("no reason was kept")
                )
            }
        };
        said.lines.push(line);
    }
    docked(home, &views, &mut said);
    menued(home, &menu, &mut said);
    let Ok(dirs) = fs::read_dir(prepare::root(home)) else {
        return said;
    };
    let mut gone: Vec<String> = dirs
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| is_name(n) && !pins.iter().any(|p| &p.name == n))
        .collect();
    gone.sort();
    for name in gone {
        said.lines.push(match prepare::remove(x, home, &name) {
            Ok(()) => format!("plugin {name}: removed, as TEHAS_PLUGINS no longer names it"),
            Err(why) => {
                said.refused = true;
                format!("plugin {name}: not in TEHAS_PLUGINS and not all removed: {why}")
            }
        });
    }
    said
}

/// The menu's entry of a plugin granted `menu`: its name and label, and each
/// view granted, v1's `view` as the view `view` at its folder `/`. None when
/// no view is granted.
fn listed(name: &str, shown: &Shown, caps: &[Cap], base: &str) -> Option<serde_json::Value> {
    use tehas_core::plugin::manifest::Open;
    let mut views = Vec::new();
    if let (Some(label), true) = (&shown.panel, caps.contains(&Cap::View)) {
        views.push(serde_json::json!({"id": "view", "label": label, "url": base, "open": "dock"}));
    }
    for v in &shown.views {
        if caps.contains(&Cap::ViewOf(v.id.clone())) {
            let open = match v.open {
                Open::Dock => "dock",
                Open::Page => "page",
            };
            let url = format!("{}{}", base.trim_end_matches('/'), v.path);
            views.push(serde_json::json!({"id": v.id, "label": v.label, "url": url, "open": open}));
        }
    }
    (!views.is_empty())
        .then(|| serde_json::json!({"name": name, "label": shown.label, "views": views}))
}

/// `live/plugins.json`: each plugin granted `menu` and its granted views, or
/// no file when there are none. Written whole through a new file and a
/// rename, so the front pages never read half of it.
fn menued(home: &Path, menu: &[serde_json::Value], said: &mut Said) {
    let file = home.join("live/plugins.json");
    let written = if menu.is_empty() {
        match fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        let text = format!("{}\n", serde_json::json!({"api": 1, "plugins": menu}));
        let new = home.join("live/plugins.json.new");
        fs::create_dir_all(home.join("live"))
            .and_then(|()| fs::write(&new, text))
            .and_then(|()| {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&new, fs::Permissions::from_mode(0o644))
            })
            .and_then(|()| fs::rename(&new, &file))
    };
    if let Err(e) = written {
        said.lines
            .push(format!("plugins: live/plugins.json was not written: {e}"));
    }
}

/// The plugins' panels set to `views`. A view left off is said, and the
/// update goes on: it refuses the view, not the plugin.
fn docked(home: &Path, views: &[Panel], said: &mut Said) {
    match panel::set_views(home, views) {
        Ok(left) => {
            for (id, why) in left {
                let name = id.strip_prefix(plugin::PANEL_PREFIX).unwrap_or(id.as_str());
                said.lines
                    .push(format!("plugin {name}: its view is not in the dock: {why}"));
            }
        }
        Err(why) => said
            .lines
            .push(format!("plugins: the views' panels were not set: {why}")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::exec::fake::{ok, Fake};
    use super::super::prepare::tests::{dockerfile, good, manifest, C, ID, URL};
    use super::*;
    use std::collections::BTreeMap;
    use tehas_core::testdir::TestDir;

    fn vars(pairs: &[(&str, String)]) -> impl Fn(&str) -> String {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        move |name: &str| map.get(name).cloned().unwrap_or_default()
    }

    fn on(more: &[(&str, String)]) -> impl Fn(&str) -> String {
        let mut all = vec![
            ("TEHAS_PLUGIN", "on".to_string()),
            (tehas_core::features::TEST_VAR, "plugins".to_string()),
        ];
        all.extend(more.iter().cloned());
        vars(&all)
    }

    #[test]
    fn with_the_feature_off_nothing_is_run() {
        let home = TestDir::new("plugin-update-off");
        let mut x = Fake::new();
        let get = vars(&[("TEHAS_PLUGINS", format!("floorplan={URL}@{C}"))]);
        assert_eq!(step(&mut x, &home, &get), Said::default());
        assert!(x.ran.is_empty());
    }

    #[test]
    fn a_plugin_whose_needs_are_not_met_is_refused_after_its_build() {
        let m = r#"{"plugin": 1, "kind": "batch", "name": "floorplan", "label": "Floorplan", "build": "Dockerfile", "asks": ["product", "needs:specs"], "timeout": 3600, "needs": [{"name": "specs", "skills": ["grill"]}]}"#;
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-update-needs",
            &[("tehas-plugin.json", m), ("Dockerfile", &d)],
        );
        x.on("{{.Id}}", ok(&format!("{ID}\n")));
        x.on("for-each-ref", ok(""));
        x.on("gc --prune=now", ok(""));
        let get = on(&[
            ("TEHAS_PLUGINS", format!("floorplan={URL}@{C}")),
            (
                "TEHAS_PLUGIN_GRANTS_FLOORPLAN",
                format!("{C} product needs:specs"),
            ),
        ]);
        let said = step(&mut x, &home, &get);
        assert!(said.refused);
        assert_eq!(
            said.lines,
            ["plugin floorplan: refused and left off: needs `specs`, which is not in TEHAS_PLUGINS"]
        );
        let s = prepare::read_state(&home, "floorplan").unwrap();
        assert_eq!(s.state, Word::Refused);
        assert_eq!(s.image.as_deref(), Some(ID));
    }

    #[test]
    fn a_ready_plugin_is_said_and_a_refused_one_skipped_with_why() {
        let m = manifest(r#""product""#);
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-update",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.on("{{.Id}}", ok(&format!("{ID}\n")));
        let get = on(&[
            ("TEHAS_PLUGINS", format!("floorplan={URL}@{C}")),
            ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", format!("{C} product")),
        ]);
        x.on(
            "for-each-ref",
            ok(&format!("refs/tehas/{C}\nrefs/tehas/{}\n", "f".repeat(40))),
        );
        x.on("update-ref -d", ok(""));
        x.on("gc --prune=now", ok(""));
        assert_eq!(
            step(&mut x, &home, &get),
            Said {
                lines: vec![format!("plugin floorplan: ready, image {ID}")],
                refused: false
            }
        );
        assert!(x.ran_any(&format!("update-ref -d refs/tehas/{}", "f".repeat(40))));
        assert!(x.ran_any("gc --prune=now --quiet"));
        let get = on(&[("TEHAS_PLUGINS", format!("floorplan={URL}@{C}"))]);
        let said = step(&mut x, &home, &get);
        assert!(said.refused);
        assert_eq!(
            said.lines,
            ["plugin floorplan: refused and left off: product is asked and not granted"]
        );
    }

    #[test]
    fn a_plugin_taken_out_is_removed_and_a_stray_directory_name_left() {
        let (home, mut x) = good("plugin-update-gone", &[]);
        fs::create_dir_all(prepare::root(&home).join("not-a-name")).unwrap();
        x.on("docker image ls", ok(""));
        x.on("docker volume inspect", ok("[]"));
        x.on("docker volume rm", ok(""));
        let get = on(&[]);
        let said = step(&mut x, &home, &get);
        assert_eq!(
            said.lines,
            ["plugin floorplan: removed, as TEHAS_PLUGINS no longer names it"]
        );
        assert!(!said.refused);
        assert!(!prepare::dir(&home, "floorplan").exists());
        assert!(prepare::root(&home).join("not-a-name").exists());
    }

    fn panels(home: &Path) -> Vec<Panel> {
        match fs::read_to_string(home.join("state/panels.json")) {
            Ok(text) => panel::parse(&text).unwrap(),
            Err(_) => Vec::new(),
        }
    }

    fn menu_file(home: &Path) -> Option<serde_json::Value> {
        fs::read_to_string(home.join("live/plugins.json"))
            .ok()
            .map(|t| serde_json::from_str(&t).unwrap())
    }

    #[test]
    fn a_plugin_granted_menu_lists_its_granted_views_and_takes_no_panel() {
        let views = r#""views": [
            {"id": "map", "label": "Map", "path": "/", "open": "dock"},
            {"id": "health", "label": "Health", "path": "/health/", "open": "page"},
            {"id": "debts", "label": "Debts", "path": "/debts/", "open": "page"}]"#;
        let m = manifest(r#""menu", "view:map", "view:health", "view:debts""#).replace(
            r#""timeout": 3600}"#,
            &format!(r#""timeout": 3600, {views}}}"#),
        );
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-update-menu",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.on("{{.Id}}", ok(&format!("{ID}\n")));
        x.on("for-each-ref", ok(&format!("refs/tehas/{C}\n")));
        x.on("gc --prune=now", ok(""));
        let set = |grants: &str| {
            on(&[
                ("TEHAS_PLUGINS", format!("floorplan={URL}@{C}")),
                ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", format!("{C} {grants}")),
                ("TEHAS_DOMAIN", "demo.example.org".to_string()),
                ("TEHAS_EDGE_ONE_LEVEL", "1".to_string()),
            ])
        };
        // The manifest asks for debts too; a grant without it refuses the plugin.
        let said = step(&mut x, &home, &set("menu view:map view:health view:debts"));
        assert_eq!(said.lines, [format!("plugin floorplan: ready, image {ID}")]);
        assert!(
            panels(&home).is_empty(),
            "no panel for a plugin in the menu"
        );
        let base = "https://plugin-floorplan-demo.example.org";
        assert_eq!(
            menu_file(&home).unwrap(),
            serde_json::json!({"api": 1, "plugins": [{"name": "floorplan", "label": "Floorplan", "views": [
                {"id": "map", "label": "Map", "url": format!("{base}/"), "open": "dock"},
                {"id": "health", "label": "Health", "url": format!("{base}/health/"), "open": "page"},
                {"id": "debts", "label": "Debts", "url": format!("{base}/debts/"), "open": "page"}]}]})
        );
        let mode = fs::metadata(home.join("live/plugins.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o644
        );
        // Refused at the pin, or the feature off: the file goes.
        let said = step(&mut x, &home, &set("menu view:map"));
        assert!(said.refused);
        assert_eq!(menu_file(&home), None);
        step(&mut x, &home, &set("menu view:map view:health view:debts"));
        assert!(menu_file(&home).is_some());
        let off = on(&[("TEHAS_PLUGIN", "off".to_string())]);
        step(&mut x, &home, &off);
        assert_eq!(menu_file(&home), None);
    }

    #[test]
    fn a_ready_view_granted_is_a_panel_and_goes_when_it_is_refused() {
        let m = manifest(r#""view""#).replace(
            r#""timeout": 3600}"#,
            r#""timeout": 3600, "view": {"label": "Map"}}"#,
        );
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-update-view",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.on("{{.Id}}", ok(&format!("{ID}\n")));
        x.on("for-each-ref", ok(&format!("refs/tehas/{C}\n")));
        x.on("gc --prune=now", ok(""));
        let domain = ("TEHAS_DOMAIN", "demo.example.org".to_string());
        let get = on(&[
            ("TEHAS_PLUGINS", format!("floorplan={URL}@{C}")),
            ("TEHAS_PLUGIN_GRANTS_FLOORPLAN", format!("{C} view")),
            domain.clone(),
            ("TEHAS_EDGE_ONE_LEVEL", "1".to_string()),
        ]);
        let said = step(&mut x, &home, &get);
        assert_eq!(said.lines, [format!("plugin floorplan: ready, image {ID}")]);
        assert_eq!(
            panels(&home),
            [Panel {
                id: "p-floorplan".into(),
                label: "Map".into(),
                url: "https://plugin-floorplan-demo.example.org/".into(),
            }]
        );
        // Not granted at the pin: refused, and its panel is taken away.
        let get = on(&[
            ("TEHAS_PLUGINS", format!("floorplan={URL}@{C}")),
            domain.clone(),
        ]);
        assert!(step(&mut x, &home, &get).refused);
        assert!(panels(&home).is_empty());
    }

    #[test]
    fn with_the_feature_off_a_views_panel_goes_and_a_persons_stays() {
        let home = TestDir::new("plugin-update-off-panels");
        let views = [
            Panel {
                id: "support".into(),
                label: "Ask us".into(),
                url: "https://support.example.org/".into(),
            },
            Panel {
                id: "p-floorplan".into(),
                label: "Map".into(),
                url: "https://plugin-floorplan.example.org/".into(),
            },
        ];
        assert_eq!(panel::set_views(&home, &views[1..]), Ok(vec![]));
        assert_eq!(
            panel::run(&home, "example.org", panel::Asked::Set(views[0].clone())),
            0
        );
        let mut x = Fake::new();
        assert_eq!(step(&mut x, &home, &vars(&[])), Said::default());
        assert!(x.ran.is_empty());
        assert_eq!(panels(&home), views[..1]);
    }
}
