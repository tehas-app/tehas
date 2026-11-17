//! The feature registry: every optional feature, whether this release ships it
//! or is still building it, and the switch that turns it on. A building
//! feature cannot be on: the settings load refuses its switch set to on with
//! 78, and `tehas api --json` lists it in `flags` as false and not in
//! `features`. Promoting one to shipped only adds.
//!
//! A test turns a building feature on by naming it in [`TEST_VAR`], in the
//! environment it starts a program with. Only a build with debug assertions
//! reads it, as the store reads its tests' answers; a release never does, and
//! the load refuses the variable in instance.env and in ~/.config/tehas/env.

use crate::settings::rules::switch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Building,
    Shipped,
}

pub struct Feature {
    /// The word of `flags`, and of `features` once shipped.
    pub name: &'static str,
    pub stage: Stage,
    /// The instance setting that turns it on, a switch.
    pub setting: &'static str,
    /// The release that registered it.
    pub since: &'static str,
    /// The programs of the workspace install.sh builds and installs only while
    /// the feature is on, and a release packs beside the others.
    pub programs: &'static [&'static str],
    /// The units of host/systemd install.sh installs only while it is on.
    pub units: &'static [&'static str],
}

pub const FEATURES: &[Feature] = &[
    Feature {
        name: "outsource",
        stage: Stage::Building,
        setting: "TEHAS_OUTSOURCE",
        since: "v1.0.0-alpha.2",
        programs: &["tehas-proxy", "tehas-contractor"],
        units: &["tehas-outsource-proxy.service"],
    },
    Feature {
        name: "spec-pipe",
        stage: Stage::Building,
        setting: "TEHAS_SPEC_PIPE",
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
    Feature {
        name: "protolab-zip-url",
        stage: Stage::Building,
        setting: "TEHAS_PROTOLAB_ZIP_URL",
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
    Feature {
        name: "prototype-file",
        stage: Stage::Building,
        setting: "TEHAS_PROTOTYPE_FILE",
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
    Feature {
        name: "lean",
        stage: Stage::Building,
        setting: "TEHAS_LEAN",
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
];

/// The internal variable that lets a test turn building features on: their
/// names, one space apart.
pub const TEST_VAR: &str = "TEHAS_TEST_FEATURES";

fn find(name: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.name == name)
}

fn usable(feature: &Feature, get: &dyn Fn(&str) -> String) -> bool {
    usable_in(feature, get, cfg!(debug_assertions))
}

/// `tested` is whether this build reads [`TEST_VAR`].
fn usable_in(feature: &Feature, get: &dyn Fn(&str) -> String, tested: bool) -> bool {
    feature.stage == Stage::Shipped
        || tested
            && get(TEST_VAR)
                .split_whitespace()
                .any(|word| word == feature.name)
}

/// [`on`] over the variables `get` gives, for the load, which reads its own.
pub fn on_in(name: &str, get: &dyn Fn(&str) -> String) -> bool {
    find(name).is_some_and(|f| usable(f, get) && switch(&get(f.setting)) == Some(true))
}

/// Whether the feature is usable here and its setting says on. A name the
/// registry does not hold is off.
pub fn on(name: &str) -> bool {
    on_in(name, &env)
}

/// Every registered feature and whether it is on, in the registry's order.
pub fn flags() -> Vec<(&'static str, bool)> {
    FEATURES.iter().map(|f| (f.name, on(f.name))).collect()
}

/// The programs and the units of the features that are on, in the
/// registry's order: what install.sh installs besides its own list.
pub fn installs_in(get: &dyn Fn(&str) -> String) -> (Vec<&'static str>, Vec<&'static str>) {
    let on: Vec<&Feature> = FEATURES.iter().filter(|f| on_in(f.name, get)).collect();
    (
        on.iter().flat_map(|f| f.programs.iter().copied()).collect(),
        on.iter().flat_map(|f| f.units.iter().copied()).collect(),
    )
}

/// The units any feature installs, whether it is on or not.
pub fn units() -> impl Iterator<Item = &'static str> {
    FEATURES.iter().flat_map(|f| f.units.iter().copied())
}

/// The names of the shipped features: the words they add to `features`.
pub fn shipped() -> Vec<&'static str> {
    FEATURES
        .iter()
        .filter(|f| f.stage == Stage::Shipped)
        .map(|f| f.name)
        .collect()
}

/// A building feature's switch set to on, as the load refuses it. Err is the
/// reason, without the file.
pub fn refuse_building(get: &dyn Fn(&str) -> String) -> Result<(), String> {
    for f in FEATURES {
        if !usable(f, get) && switch(&get(f.setting)) == Some(true) {
            return Err(format!(
                "{} is on and {} is still being built in this release: set it to off or take the line out",
                f.setting, f.name
            ));
        }
    }
    Ok(())
}

/// Why `what` cannot run while the feature is not on, or Ok when it is. The
/// words every refusal of a part of a feature says, so the load, the poll and
/// a command name the feature and its switch the same way.
pub fn needs(name: &str, what: &str, get: &dyn Fn(&str) -> String) -> Result<(), String> {
    let Some(f) = find(name) else {
        return Err(format!(
            "{what} needs {name}, which this release does not have"
        ));
    };
    if on_in(name, get) {
        Ok(())
    } else if !usable(f, get) {
        Err(format!(
            "{what} needs the feature {name}, and {name} is still being built in this release"
        ))
    } else {
        Err(format!(
            "{what} needs the feature {name}: set {}=on",
            f.setting
        ))
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| map.get(name).cloned().unwrap_or_default()
    }

    #[test]
    fn the_contract_page_says_what_each_feature_leaves_visible_while_it_is_off() {
        let page = include_str!("../../../host/docs/contract.md");
        for f in FEATURES {
            let heading = format!("\n### {}\n", f.name);
            assert!(page.contains(&heading), "contract.md has no {heading:?}");
        }
    }

    #[test]
    fn every_feature_has_a_lower_case_word_and_a_tehas_switch() {
        let mut names: Vec<&str> = FEATURES.iter().map(|f| f.name).collect();
        for f in FEATURES {
            assert!(!f.name.is_empty());
            assert!(f.name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
            assert!(f.setting.starts_with("TEHAS_"), "{}", f.setting);
            assert!(crate::version::is_tag(f.since), "{}", f.since);
            assert!(
                crate::settings::rules::TABLE
                    .iter()
                    .any(|s| s.name == f.setting),
                "{} has no row in the settings table",
                f.setting
            );
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), FEATURES.len());
    }

    #[test]
    fn outsource_is_building() {
        assert_eq!(find("outsource").map(|f| f.stage), Some(Stage::Building));
        assert!(!shipped().contains(&"outsource"));
    }

    #[test]
    fn the_spec_pipe_is_building_and_follows_its_switch_only_in_a_test() {
        assert_eq!(find("spec-pipe").map(|f| f.stage), Some(Stage::Building));
        assert!(!shipped().contains(&"spec-pipe"));
        assert!(!on_in("spec-pipe", &vars(&[("TEHAS_SPEC_PIPE", "on")])));
        let tested = vars(&[("TEHAS_SPEC_PIPE", "on"), (TEST_VAR, "spec-pipe")]);
        assert!(on_in("spec-pipe", &tested));
        assert_eq!(
            refuse_building(&vars(&[("TEHAS_SPEC_PIPE", "1")])),
            Err("TEHAS_SPEC_PIPE is on and spec-pipe is still being built in this release: set it to off or take the line out".to_string())
        );
    }

    #[test]
    fn the_protolab_zip_url_is_building_and_follows_its_switch_only_in_a_test() {
        let name = "protolab-zip-url";
        assert_eq!(find(name).map(|f| f.stage), Some(Stage::Building));
        assert!(!shipped().contains(&name));
        assert!(!on_in(name, &vars(&[("TEHAS_PROTOLAB_ZIP_URL", "on")])));
        let tested = vars(&[("TEHAS_PROTOLAB_ZIP_URL", "on"), (TEST_VAR, name)]);
        assert!(on_in(name, &tested));
        assert_eq!(
            refuse_building(&vars(&[("TEHAS_PROTOLAB_ZIP_URL", "1")])),
            Err("TEHAS_PROTOLAB_ZIP_URL is on and protolab-zip-url is still being built in this release: set it to off or take the line out".to_string())
        );
    }

    #[test]
    fn the_prototype_file_is_building_and_follows_its_switch_only_in_a_test() {
        let name = "prototype-file";
        assert_eq!(find(name).map(|f| f.stage), Some(Stage::Building));
        assert!(!shipped().contains(&name));
        assert!(!on_in(name, &vars(&[("TEHAS_PROTOTYPE_FILE", "on")])));
        let tested = vars(&[("TEHAS_PROTOTYPE_FILE", "on"), (TEST_VAR, name)]);
        assert!(on_in(name, &tested));
        assert_eq!(
            refuse_building(&vars(&[("TEHAS_PROTOTYPE_FILE", "on")])),
            Err("TEHAS_PROTOTYPE_FILE is on and prototype-file is still being built in this release: set it to off or take the line out".to_string())
        );
    }

    #[test]
    fn lean_is_building_and_follows_its_switch_only_in_a_test() {
        let name = "lean";
        assert_eq!(find(name).map(|f| f.stage), Some(Stage::Building));
        assert!(!shipped().contains(&name));
        assert!(!on_in(name, &vars(&[("TEHAS_LEAN", "on")])));
        let tested = vars(&[("TEHAS_LEAN", "on"), (TEST_VAR, name)]);
        assert!(on_in(name, &tested));
        assert_eq!(
            refuse_building(&vars(&[("TEHAS_LEAN", "on")])),
            Err("TEHAS_LEAN is on and lean is still being built in this release: set it to off or take the line out".to_string())
        );
    }

    #[test]
    fn a_part_that_needs_a_feature_names_it_and_its_switch() {
        let what = "TEHAS_DRIVER=lean";
        assert_eq!(
            needs("lean", what, &vars(&[("TEHAS_LEAN", "on")])),
            Err("TEHAS_DRIVER=lean needs the feature lean, and lean is still being built in this release".to_string())
        );
        assert_eq!(
            needs(
                "lean",
                what,
                &vars(&[("TEHAS_LEAN", "off"), (TEST_VAR, "lean")])
            ),
            Err("TEHAS_DRIVER=lean needs the feature lean: set TEHAS_LEAN=on".to_string())
        );
        let tested = vars(&[("TEHAS_LEAN", "on"), (TEST_VAR, "lean")]);
        assert_eq!(needs("lean", what, &tested), Ok(()));
        assert_eq!(
            needs("nope", what, &tested),
            Err("TEHAS_DRIVER=lean needs nope, which this release does not have".to_string())
        );
    }

    #[test]
    fn a_building_feature_is_off_whatever_its_switch_says() {
        for value in ["on", "1", "off", "0", ""] {
            let get = vars(&[("TEHAS_OUTSOURCE", value)]);
            assert!(!on_in("outsource", &get), "{value}");
        }
    }

    #[test]
    fn the_test_variable_lets_a_building_feature_follow_its_switch() {
        for (value, want) in [("on", true), ("1", true), ("off", false), ("0", false)] {
            let get = vars(&[("TEHAS_OUTSOURCE", value), (TEST_VAR, "other outsource")]);
            assert_eq!(on_in("outsource", &get), want, "{value}");
        }
        let get = vars(&[("TEHAS_OUTSOURCE", "on"), (TEST_VAR, "outsourcer")]);
        assert!(!on_in("outsource", &get));
    }

    #[test]
    fn a_release_build_ignores_the_test_variable() {
        let outsource = find("outsource").unwrap();
        let get = vars(&[("TEHAS_OUTSOURCE", "on"), (TEST_VAR, "outsource")]);
        assert!(!usable_in(outsource, &get, false));
        assert!(usable_in(outsource, &get, true));
    }

    #[test]
    fn a_name_the_registry_does_not_hold_is_off() {
        let get = vars(&[("TEHAS_NOPE", "on"), (TEST_VAR, "nope")]);
        assert!(!on_in("nope", &get));
    }

    #[test]
    fn the_load_refuses_a_building_feature_set_on() {
        for value in ["on", "1"] {
            let got = refuse_building(&vars(&[("TEHAS_OUTSOURCE", value)]));
            assert_eq!(
                got,
                Err("TEHAS_OUTSOURCE is on and outsource is still being built in this release: set it to off or take the line out".to_string())
            );
        }
        for value in ["off", "0", ""] {
            assert_eq!(
                refuse_building(&vars(&[("TEHAS_OUTSOURCE", value)])),
                Ok(())
            );
        }
        let tested = vars(&[("TEHAS_OUTSOURCE", "on"), (TEST_VAR, "outsource")]);
        assert_eq!(refuse_building(&tested), Ok(()));
    }

    #[test]
    fn outsource_installs_its_proxy_its_unit_and_the_contractor_only_while_on() {
        let none: (Vec<&str>, Vec<&str>) = (vec![], vec![]);
        assert_eq!(installs_in(&vars(&[])), none);
        // A building feature set on without the test variable is off.
        assert_eq!(installs_in(&vars(&[("TEHAS_OUTSOURCE", "on")])), none);
        let off = vars(&[("TEHAS_OUTSOURCE", "off"), (TEST_VAR, "outsource")]);
        assert_eq!(installs_in(&off), none);
        let on = vars(&[("TEHAS_OUTSOURCE", "on"), (TEST_VAR, "outsource")]);
        assert_eq!(
            installs_in(&on),
            (
                vec!["tehas-proxy", "tehas-contractor"],
                vec!["tehas-outsource-proxy.service"]
            )
        );
        assert_eq!(
            units().collect::<Vec<_>>(),
            ["tehas-outsource-proxy.service"]
        );
    }

    #[test]
    fn every_unit_a_feature_installs_is_a_file_of_host_systemd() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../host/systemd");
        for unit in units() {
            assert!(dir.join(unit).is_file(), "host/systemd/{unit}");
        }
    }

    #[test]
    fn flags_name_every_feature_in_order() {
        let names: Vec<&str> = flags().iter().map(|(n, _)| *n).collect();
        let want: Vec<&str> = FEATURES.iter().map(|f| f.name).collect();
        assert_eq!(names, want);
    }
}
