//! The feature registry: every optional feature, whether this release ships it
//! or is still building it, and the switch that turns it on. A building
//! feature cannot be on: the settings load refuses its switch set to on with
//! 78, and `tehas api --json` lists it in `flags` as false and not in
//! `features`. Promoting one to shipped only adds.
//!
//! A test turns a building feature on by naming it in [`TEST_VAR`], in the
//! environment it starts a program with. A build with debug assertions, the
//! tests', reads it, and so does a lab build, which `install.sh --lab-build`
//! makes with [`LAB_VAR`] on and debug assertions off: it reads none of the
//! tests' other hooks. A release never reads it, and the load refuses the
//! variable in instance.env and in ~/.config/tehas/env.

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
    /// The feature's own settings beside its switch. While it is building the
    /// load refuses a line that sets one.
    pub settings: &'static [&'static str],
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
        settings: &[
            "TEHAS_OUTSOURCE_TIMEOUT",
            "TEHAS_OUTSOURCE_GRACE",
            "TEHAS_OUTSOURCE_RETRIES",
            "TEHAS_OUTSOURCE_REST",
            "TEHAS_OUTSOURCE_REPOS",
            "TEHAS_OUTSOURCE_CONTRACTORS",
            "TEHAS_OUTSOURCE_JOIN_TTL",
            "TEHAS_OUTSOURCE_PROXY_ADDR",
            "TEHAS_OUTSOURCE_PROXY_IDLE",
        ],
        since: "v1.0.0-alpha.2",
        programs: &["tehas-proxy", "tehas-contractor"],
        units: &["tehas-outsource-proxy.service"],
    },
    Feature {
        name: "spec-pipe",
        stage: Stage::Building,
        setting: "TEHAS_SPEC_PIPE",
        settings: &[],
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
    Feature {
        name: "protolab-zip-url",
        stage: Stage::Building,
        setting: "TEHAS_PROTOLAB_ZIP_URL",
        settings: &["TEHAS_PROTOLAB_ZIP_HOSTS"],
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
    Feature {
        name: "prototype-file",
        stage: Stage::Building,
        setting: "TEHAS_PROTOTYPE_FILE",
        settings: &[],
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
    Feature {
        name: "lean",
        stage: Stage::Building,
        setting: "TEHAS_LEAN",
        settings: &[],
        since: "v1.0.0-alpha.3",
        programs: &[],
        units: &[],
    },
    // Shipped as a setting before the registry, never seen on a host: its
    // names stay on api/surface-1.0.txt (console/tests/surface.rs).
    Feature {
        name: "review",
        stage: Stage::Building,
        setting: "TEHAS_REVIEW",
        settings: &["TEHAS_REVIEW_REPO"],
        since: "v1.0.0-rc.1",
        programs: &[],
        units: &["tehas-review.service", "tehas-review.timer"],
    },
    // docs/specs/2026-11-21-plugins.md. The gate's program and unit come with
    // the gate.
    Feature {
        name: "plugins",
        stage: Stage::Building,
        setting: "TEHAS_PLUGIN",
        settings: &[
            "TEHAS_PLUGINS",
            "TEHAS_PLUGIN_GRANTS_<NAME>",
            "TEHAS_PLUGIN_ACCESS",
            "TEHAS_PLUGIN_LIMITS",
            "TEHAS_PLUGIN_MODEL_DAY",
            "TEHAS_PLUGIN_GATE_ADDR",
        ],
        since: "v1.2.0",
        programs: &["tehas-plugin"],
        units: &[
            "tehas-plugin@.service",
            "tehas-plugin-hook@.service",
            "tehas-plugin.service",
            "tehas-plugin.timer",
        ],
    },
    // docs/specs/2026-11-21-wiki.md.
    Feature {
        name: "wiki",
        stage: Stage::Building,
        setting: "TEHAS_WIKI",
        settings: &[],
        since: "v1.3.0",
        programs: &[],
        units: &[
            "tehas-wiki-status.service",
            "tehas-wiki-status.timer",
            "tehas-wiki.service",
            "tehas-wiki.timer",
            "tehas-wiki.path",
        ],
    },
];

/// The internal variable that lets a test turn building features on: their
/// names, one space apart.
pub const TEST_VAR: &str = "TEHAS_TEST_FEATURES";

/// The variable `install.sh --lab-build` sets to `on` while cargo builds, and
/// every other run of it to `off`. Read when this crate is compiled, never
/// while a program runs; rustc names it in the build's dependencies, so cargo
/// builds again when it changes.
pub const LAB_VAR: &str = "TEHAS_LAB_BUILD";

/// Whether `install.sh --lab-build` made this program.
pub fn lab_build() -> bool {
    lab_marker(option_env!("TEHAS_LAB_BUILD"))
}

fn lab_marker(value: Option<&str>) -> bool {
    value == Some("on")
}

/// Whether this build reads [`TEST_VAR`]: the tests' builds and a lab build.
fn reads_test_var() -> bool {
    reads(cfg!(debug_assertions), option_env!("TEHAS_LAB_BUILD"))
}

/// `debug` is whether the build has debug assertions, `marker` [`LAB_VAR`] as
/// it was when it was compiled.
fn reads(debug: bool, marker: Option<&str>) -> bool {
    debug || lab_marker(marker)
}

fn find(name: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.name == name)
}

fn usable(feature: &Feature, get: &dyn Fn(&str) -> String) -> bool {
    usable_in(feature, get, reads_test_var())
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

/// Whether `unit` is a service a timer of `units` starts: it is installed with
/// the timer, but neither enabled nor restarted on its own, which would run
/// its job off schedule.
pub fn timed(unit: &str, units: &[&str]) -> bool {
    unit.strip_suffix(".service")
        .is_some_and(|stem| units.contains(&format!("{stem}.timer").as_str()))
}

/// Whether `unit` is a template, `<name>@.<type>`: it runs only as an
/// instance something starts, so it is installed but never enabled or
/// restarted by name.
pub fn template(unit: &str) -> bool {
    unit.split_once('.')
        .is_some_and(|(stem, _)| stem.ends_with('@'))
}

/// The names of the shipped features: the words they add to `features`.
pub fn shipped() -> Vec<&'static str> {
    FEATURES
        .iter()
        .filter(|f| f.stage == Stage::Shipped)
        .map(|f| f.name)
        .collect()
}

/// Whether the setting is the switch or one of the own settings of a feature
/// this release is still building: a row api/settings-rules.json marks
/// `building`, which is not contract.
pub fn building_setting(name: &str) -> bool {
    FEATURES
        .iter()
        .filter(|f| f.stage == Stage::Building)
        .any(|f| f.setting == name || f.settings.contains(&name))
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

/// A line that sets one of a building feature's own settings, as the load
/// refuses it: the feature leaves no setting in use while it is building.
/// `set` is whether the instance gave the name a value; for a family's row
/// (`rules::FAMILY`), whether it gave any name of the family one. Err is the
/// reason, without the file.
pub fn refuse_building_settings(
    set: &dyn Fn(&str) -> bool,
    get: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    for f in FEATURES.iter().filter(|f| !usable(f, get)) {
        if let Some(name) = f.settings.iter().find(|name| set(name)) {
            return Err(format!(
                "{name} is set and {} is still being built in this release: take the line out",
                f.name
            ));
        }
    }
    Ok(())
}

/// Every line of `text`, a file named `file`, that the load refuses for a
/// feature still being built: its switch set on, or one of its own settings
/// given a value. One line each, as the load words it, so an update names them
/// all at once.
pub fn found_in(text: &[u8], file: &str) -> Vec<String> {
    found_in_with(text, file, &env)
}

fn found_in_with(text: &[u8], file: &str, get: &dyn Fn(&str) -> String) -> Vec<String> {
    let value = |name: &str| crate::settings::renamed::value_in(text, name).unwrap_or_default();
    // A family's row stands for each name of it the file sets.
    let names = |row: &str| -> Vec<String> {
        let Some(front) = row.strip_suffix(crate::settings::rules::FAMILY) else {
            return vec![row.to_string()];
        };
        let mut names: Vec<String> = text
            .split(|b| *b == b'\n')
            .filter_map(|line| {
                let line = crate::settings::load::bare_line(line);
                let name = line.split(|b| *b == b'=').next()?;
                let name = String::from_utf8_lossy(name).into_owned();
                (name.len() > front.len() && name.starts_with(front)).then_some(name)
            })
            .collect();
        names.sort();
        names.dedup();
        names
    };
    let mut found = Vec::new();
    for f in FEATURES.iter().filter(|f| !usable(f, get)) {
        if switch(&value(f.setting)) == Some(true) {
            found.push(format!(
                "{} is on and {} is still being built in this release: set it to off or take the line out (see {file})",
                f.setting, f.name
            ));
        }
        let set = f.settings.iter().flat_map(|row| names(row));
        for name in set.filter(|n| !value(n).is_empty()) {
            found.push(format!(
                "{name} is set and {} is still being built in this release: take the line out (see {file})",
                f.name
            ));
        }
    }
    found
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

/// What [`TEST_VAR`] comes to, for health and `tehas status`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tested {
    /// Whether this build reads it.
    pub read: bool,
    /// Whether this is a lab build ([`lab_build`]).
    pub lab: bool,
    /// Whether it is set and not blank.
    pub set: bool,
    /// The building features it names, in the registry's order.
    pub features: Vec<&'static str>,
}

/// [`Tested`] in this environment: None while the variable is unset or blank
/// and this is no lab build, so the tests' own builds say nothing of it. A
/// release reads it never; a build with debug assertions or a lab build,
/// always.
pub fn test_features() -> Option<Tested> {
    test_features_in(&env, reads_test_var(), lab_build())
}

/// [`test_features`] over the variables `get` gives; `read` is whether this
/// build reads [`TEST_VAR`] and `lab` whether it is a lab build.
pub fn test_features_in(get: &dyn Fn(&str) -> String, read: bool, lab: bool) -> Option<Tested> {
    let value = get(TEST_VAR);
    let set = !value.trim().is_empty();
    if !set && !lab {
        return None;
    }
    let features = FEATURES
        .iter()
        .filter(|f| f.stage == Stage::Building && value.split_whitespace().any(|w| w == f.name))
        .map(|f| f.name)
        .collect();
    Some(Tested {
        read,
        lab,
        set,
        features,
    })
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

    fn tested(read: bool, lab: bool, set: bool, features: &[&'static str]) -> Option<Tested> {
        Some(Tested {
            read,
            lab,
            set,
            features: features.to_vec(),
        })
    }

    #[test]
    fn the_test_variable_is_reported_only_when_set_and_names_only_building_features() {
        assert_eq!(test_features_in(&vars(&[]), true, false), None);
        assert_eq!(
            test_features_in(&vars(&[(TEST_VAR, " ")]), true, false),
            None
        );
        assert_eq!(
            test_features_in(&vars(&[(TEST_VAR, "lean nothing outsource")]), true, false),
            tested(true, false, true, &["outsource", "lean"])
        );
        // A release does not read it, and says what it names all the same.
        assert_eq!(
            test_features_in(&vars(&[(TEST_VAR, "spec-pipe")]), false, false),
            tested(false, false, true, &["spec-pipe"])
        );
        assert_eq!(
            test_features_in(&vars(&[(TEST_VAR, "nothing")]), true, false),
            tested(true, false, true, &[])
        );
    }

    #[test]
    fn a_lab_build_is_reported_with_the_test_variable_unset() {
        assert_eq!(
            test_features_in(&vars(&[]), true, true),
            tested(true, true, false, &[])
        );
        assert_eq!(
            test_features_in(&vars(&[(TEST_VAR, "outsource")]), true, true),
            tested(true, true, true, &["outsource"])
        );
    }

    #[test]
    fn only_the_word_on_marks_a_lab_build() {
        assert!(lab_marker(Some("on")));
        for other in [None, Some(""), Some("off"), Some("true"), Some(" on")] {
            assert!(!lab_marker(other), "{other:?}");
        }
    }

    #[test]
    fn install_and_the_release_name_the_lab_variable_this_crate_reads() {
        let this = include_str!("features.rs");
        assert!(this.contains(&format!("option_env!(\"{LAB_VAR}\")")));
        // Every run of install.sh says on or off, so the caller's environment
        // never decides; the release builds with it unset.
        let install = include_str!("../../../install.sh");
        assert!(install.contains(&format!("{LAB_VAR}=\"$lab\"")));
        let release = include_str!("../../../.github/workflows/release.yml");
        let unset = release.find(&format!("unset {LAB_VAR}"));
        let build = release.find("cargo build --release");
        assert!(
            matches!((unset, build), (Some(u), Some(b)) if u < b),
            "release.yml unsets {LAB_VAR} before it builds"
        );
    }

    #[test]
    fn a_lab_build_reads_the_test_variable_and_a_release_does_not() {
        let get = vars(&[(TEST_VAR, "outsource")]);
        let outsource = find("outsource").unwrap();
        assert!(usable_in(outsource, &get, reads(false, Some("on"))));
        assert!(usable_in(outsource, &get, reads(true, None)));
        assert!(!usable_in(outsource, &get, reads(false, Some("off"))));
        assert!(!usable_in(outsource, &get, reads(false, None)));
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
    fn a_building_setting_is_a_building_features_switch_or_own_setting() {
        for name in [
            "TEHAS_OUTSOURCE",
            "TEHAS_OUTSOURCE_REPOS",
            "TEHAS_PROTOLAB_ZIP_HOSTS",
            "TEHAS_LEAN",
            "TEHAS_REVIEW",
            "TEHAS_REVIEW_REPO",
        ] {
            assert!(building_setting(name), "{name}");
        }
        for name in ["TEHAS_REPO", "TEHAS_DRIVER", "TEHAS_NOPE"] {
            assert!(!building_setting(name), "{name}");
        }
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
    fn review_is_building_and_follows_its_switch_only_in_a_test() {
        let name = "review";
        assert_eq!(find(name).map(|f| f.stage), Some(Stage::Building));
        assert!(!shipped().contains(&name));
        assert!(!on_in(name, &vars(&[("TEHAS_REVIEW", "on")])));
        assert!(flags().contains(&(name, false)));
        let tested = vars(&[("TEHAS_REVIEW", "1"), (TEST_VAR, name)]);
        assert!(on_in(name, &tested));
        assert_eq!(
            refuse_building(&vars(&[("TEHAS_REVIEW", "on")])),
            Err("TEHAS_REVIEW is on and review is still being built in this release: set it to off or take the line out".to_string())
        );
        let set = |n: &str| n == "TEHAS_REVIEW_REPO";
        assert_eq!(
            refuse_building_settings(&set, &vars(&[])),
            Err("TEHAS_REVIEW_REPO is set and review is still being built in this release: take the line out".to_string())
        );
    }

    #[test]
    fn review_installs_its_timer_and_service_only_while_on() {
        let none: (Vec<&str>, Vec<&str>) = (vec![], vec![]);
        assert_eq!(installs_in(&vars(&[("TEHAS_REVIEW", "on")])), none);
        let on = vars(&[("TEHAS_REVIEW", "on"), (TEST_VAR, "review")]);
        assert_eq!(
            installs_in(&on),
            (vec![], vec!["tehas-review.service", "tehas-review.timer"])
        );
    }

    #[test]
    fn plugins_is_building_and_follows_its_switch_only_in_a_test() {
        let name = "plugins";
        assert_eq!(find(name).map(|f| f.stage), Some(Stage::Building));
        assert!(!shipped().contains(&name));
        assert!(!on_in(name, &vars(&[("TEHAS_PLUGIN", "on")])));
        assert!(flags().contains(&(name, false)));
        let tested = vars(&[("TEHAS_PLUGIN", "on"), (TEST_VAR, name)]);
        assert!(on_in(name, &tested));
        assert_eq!(installs_in(&tested).0, ["tehas-plugin"]);
        assert_eq!(
            refuse_building(&vars(&[("TEHAS_PLUGIN", "on")])),
            Err("TEHAS_PLUGIN is on and plugins is still being built in this release: set it to off or take the line out".to_string())
        );
        for setting in [
            "TEHAS_PLUGINS",
            "TEHAS_PLUGIN_ACCESS",
            "TEHAS_PLUGIN_LIMITS",
            "TEHAS_PLUGIN_MODEL_DAY",
        ] {
            assert!(building_setting(setting), "{setting}");
            let set = |n: &str| n == setting;
            assert_eq!(
                refuse_building_settings(&set, &vars(&[])),
                Err(format!("{setting} is set and plugins is still being built in this release: take the line out"))
            );
        }
    }

    #[test]
    fn a_grants_line_of_any_plugin_is_refused_while_plugins_is_building() {
        // The load answers for a family's row whether any name of it is set.
        let set = |n: &str| n == "TEHAS_PLUGIN_GRANTS_<NAME>";
        assert_eq!(
            refuse_building_settings(&set, &vars(&[])),
            Err("TEHAS_PLUGIN_GRANTS_<NAME> is set and plugins is still being built in this release: take the line out".to_string())
        );
        let text = b"TEHAS_REPO=acme/widget\nexport TEHAS_PLUGIN_GRANTS_FLOORPLAN='0123 view'\nTEHAS_PLUGIN_GRANTS_B=\nTEHAS_PLUGIN_GRANTS_=x\n# TEHAS_PLUGIN_GRANTS_C=x\n";
        assert_eq!(
            found_in_with(text, "/i/instance.env", &vars(&[])),
            ["TEHAS_PLUGIN_GRANTS_FLOORPLAN is set and plugins is still being built in this release: take the line out (see /i/instance.env)"]
        );
        assert!(found_in_with(text, "/i/instance.env", &vars(&[(TEST_VAR, "plugins")])).is_empty());
    }

    #[test]
    fn a_service_its_timer_starts_is_timed() {
        let units = ["tehas-review.service", "tehas-review.timer"];
        assert!(timed("tehas-review.service", &units));
        assert!(!timed("tehas-review.timer", &units));
        assert!(!timed("tehas-review.service", &["tehas-review.service"]));
        assert!(!timed(
            "tehas-outsource-proxy.service",
            &["tehas-outsource-proxy.service"]
        ));
    }

    #[test]
    fn every_line_of_a_building_feature_in_a_file_is_found() {
        let text = b"TEHAS_REPO=acme/widget\nexport TEHAS_REVIEW=on\nTEHAS_REVIEW_REPO='acme/backlog'\nTEHAS_LEAN=off\nTEHAS_SPEC_PIPE=\n";
        assert_eq!(
            found_in_with(text, "/i/instance.env", &vars(&[])),
            [
                "TEHAS_REVIEW is on and review is still being built in this release: set it to off or take the line out (see /i/instance.env)",
                "TEHAS_REVIEW_REPO is set and review is still being built in this release: take the line out (see /i/instance.env)",
            ]
        );
        assert!(found_in_with(text, "/i/instance.env", &vars(&[(TEST_VAR, "review")])).is_empty());
        assert!(found_in_with(b"TEHAS_REVIEW=0\n", "f", &vars(&[])).is_empty());
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
    fn the_load_refuses_a_building_features_own_setting() {
        let set = |names: &'static [&'static str]| move |name: &str| names.contains(&name);
        assert_eq!(
            refuse_building_settings(&set(&["TEHAS_OUTSOURCE_REPOS"]), &vars(&[])),
            Err("TEHAS_OUTSOURCE_REPOS is set and outsource is still being built in this release: take the line out".to_string())
        );
        assert_eq!(
            refuse_building_settings(&set(&["TEHAS_PROTOLAB_ZIP_HOSTS"]), &vars(&[])),
            Err("TEHAS_PROTOLAB_ZIP_HOSTS is set and protolab-zip-url is still being built in this release: take the line out".to_string())
        );
        assert_eq!(refuse_building_settings(&set(&[]), &vars(&[])), Ok(()));
        // A test that can turn the feature on may set its settings with it off.
        let tested = vars(&[(TEST_VAR, "outsource protolab-zip-url")]);
        let both = set(&["TEHAS_OUTSOURCE_TIMEOUT", "TEHAS_PROTOLAB_ZIP_HOSTS"]);
        assert_eq!(refuse_building_settings(&both, &tested), Ok(()));
    }

    #[test]
    fn every_setting_of_a_feature_is_ruled() {
        for f in FEATURES {
            for name in f.settings {
                assert!(
                    crate::settings::rules::TABLE
                        .iter()
                        .any(|s| s.name == *name),
                    "{name} has no row in the settings table"
                );
            }
        }
        // Every ruled name under a feature's switch is one of its settings.
        for s in crate::settings::rules::TABLE {
            if let Some(f) = FEATURES
                .iter()
                .find(|f| s.name.starts_with(&format!("{}_", f.setting)))
            {
                assert!(f.settings.contains(&s.name), "{} is not listed", s.name);
            }
        }
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
            [
                "tehas-outsource-proxy.service",
                "tehas-review.service",
                "tehas-review.timer",
                "tehas-plugin@.service",
                "tehas-plugin-hook@.service",
                "tehas-plugin.service",
                "tehas-plugin.timer",
                "tehas-wiki-status.service",
                "tehas-wiki-status.timer",
                "tehas-wiki.service",
                "tehas-wiki.timer",
                "tehas-wiki.path"
            ]
        );
    }

    #[test]
    fn plugins_install_their_program_runs_and_timer_only_while_on() {
        let on = vars(&[("TEHAS_PLUGIN", "on"), (TEST_VAR, "plugins")]);
        let (programs, units) = installs_in(&on);
        assert_eq!(programs, ["tehas-plugin"]);
        assert_eq!(units.len(), 4);
        assert_eq!(installs_in(&vars(&[("TEHAS_PLUGIN", "on")])).0.len(), 0);
        // The timer's service is the timer's; the runs are templates.
        assert!(timed("tehas-plugin.service", &units));
        for u in ["tehas-plugin@.service", "tehas-plugin-hook@.service"] {
            assert!(template(u), "{u}");
            assert!(!timed(u, &units), "{u}");
        }
        for u in ["tehas-plugin.timer", "tehas-cycle.service", "a@b.service"] {
            assert!(!template(u), "{u}");
        }
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

    fn building() -> impl Iterator<Item = &'static Feature> {
        FEATURES.iter().filter(|f| f.stage == Stage::Building)
    }

    /// The switch and the own settings of every building feature.
    fn building_names() -> Vec<&'static str> {
        building()
            .flat_map(|f| std::iter::once(f.setting).chain(f.settings.iter().copied()))
            .collect()
    }

    /// Labels older than the building feature that now reads them. Each is in
    /// the feature's section of contract.md.
    const KEPT_LABELS: [(&str, &str); 1] = [("lean", "tehas:lean")];

    #[test]
    fn no_building_feature_adds_a_label() {
        let table = include_str!("../../../host/labels.tsv");
        let mut labels: Vec<&str> = table
            .lines()
            .filter_map(|l| l.split('\t').next())
            .filter(|n| !n.is_empty())
            .collect();
        labels.extend(crate::label_pins::PINNED);
        labels.extend(crate::label_pins::GRANTING);
        labels.extend([
            crate::kinds::Kind::Improvement.label(),
            crate::kinds::Kind::Prototype.label(),
        ]);
        let page = include_str!("../../../host/docs/contract.md");
        for f in building() {
            let own = |label: &str| {
                label.strip_prefix("tehas:").is_some_and(|word| {
                    word == f.name
                        || word.starts_with(&format!("{}-", f.name))
                        || word.starts_with(&format!("{}:", f.name))
                })
            };
            for label in labels.iter().filter(|l| own(l)) {
                assert!(
                    KEPT_LABELS.contains(&(f.name, label)),
                    "{label} is a label of {}, which is still being built",
                    f.name
                );
            }
        }
        for (name, label) in KEPT_LABELS {
            assert!(labels.contains(&label), "{label} is no label");
            let start = page
                .find(&format!("\n### {name}\n"))
                .unwrap_or_else(|| panic!("contract.md has no section {name}"));
            let section = &page[start + 1..];
            let section = &section[..section.find("\n#").unwrap_or(section.len())];
            assert!(
                section.contains(&format!("`{label}`")),
                "contract.md does not say {name} keeps {label}"
            );
        }
    }

    #[test]
    fn a_building_features_switch_defaults_to_off() {
        for f in building() {
            let row = crate::settings::rules::TABLE
                .iter()
                .find(|s| s.name == f.setting)
                .unwrap();
            assert_eq!(row.default.and_then(switch), Some(false), "{}", f.setting);
        }
    }

    #[test]
    fn no_rename_an_update_names_moves_a_value_onto_a_building_feature() {
        let names = building_names();
        let table = include_str!("../../../api/renames-1.0.tsv");
        for line in table.lines().skip(1) {
            let fields: Vec<&str> = line.split('\t').collect();
            for field in fields.iter().skip(1).take(2) {
                let key = field.split('=').next().unwrap_or_default();
                assert!(!names.contains(&key), "api/renames-1.0.tsv: {line}");
            }
        }
    }

    /// The update and the manual point at these files for an example line.
    #[test]
    fn the_example_instance_switches_no_building_feature_on() {
        let names = building_names();
        let switches: Vec<&str> = building().map(|f| f.setting).collect();
        for text in [
            include_str!("../../../instances/example/instance.env"),
            include_str!("../../../instances/example/optional.env"),
        ] {
            for line in text.lines() {
                let commented = line.starts_with('#');
                let rest = line.trim_start_matches('#').trim_start();
                let rest = rest.strip_prefix("export ").unwrap_or(rest);
                let Some((key, value)) = rest.split_once('=') else {
                    continue;
                };
                if !names.contains(&key) {
                    continue;
                }
                assert!(commented, "the example sets {key}: {line}");
                if switches.contains(&key) {
                    assert_eq!(switch(value), Some(false), "{line}");
                }
            }
        }
    }
}
