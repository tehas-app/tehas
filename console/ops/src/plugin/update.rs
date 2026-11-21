//! The plugins' part of `tehas update`: with the feature on, each plugin of
//! TEHAS_PLUGINS fetched, checked and built when its image of the pin is not
//! there, and what is left of a plugin taken out of the settings removed
//! (docs/specs/2026-11-21-plugins.md, guarantees 4 and 9). A plugin that is
//! refused is skipped, never the update; the caller exits 1 at the end.
//! With the feature off it does nothing.

use super::exec::Exec;
use super::prepare::{self, prepare};
use std::fs;
use std::path::Path;
use tehas_core::plugin::state::Word;
use tehas_core::plugin::{self, is_name};

/// What the step said, and whether a plugin was refused.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Said {
    pub lines: Vec<String>,
    pub refused: bool,
}

pub fn step(x: &mut dyn Exec, home: &Path, get: &dyn Fn(&str) -> String) -> Said {
    let mut said = Said::default();
    if !tehas_core::features::on_in("plugins", get) {
        return said;
    }
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    // TODO(#2673): walk plugin::needs::resolve's order, dependencies first,
    // and skip a plugin it gives a why.
    for pin in &pins {
        let grants = plugin::grants(&get(&plugin::grants_setting(&pin.name))).ok();
        let s = prepare(x, home, pin, grants.as_ref());
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
        assert_eq!(
            step(&mut x, &home, &get),
            Said {
                lines: vec![format!("plugin floorplan: ready, image {ID}")],
                refused: false
            }
        );
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
}
