//! The feature registry: every optional feature, whether this release ships it
//! or is still building it, and the switch that turns it on. A building
//! feature cannot be on: the settings load refuses its switch set to on with
//! 78, and `tehas api --json` lists it in `flags` as false and not in
//! `features`. Promoting one to shipped only adds.
//!
//! A build that tests a building feature names it in [`TEST_VAR`], one of the
//! internal names of `KEPT` (settings/load.rs). No release note, label or
//! manual page tells an operator to set it.

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
}

pub const FEATURES: &[Feature] = &[Feature {
    name: "outsource",
    stage: Stage::Building,
    setting: "TEHAS_OUTSOURCE",
    since: "v1.0.0-alpha.2",
}];

/// The internal variable that lets a test turn building features on: their
/// names, one space apart.
pub const TEST_VAR: &str = "TEHAS_TEST_FEATURES";

fn find(name: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.name == name)
}

fn usable(feature: &Feature, get: &dyn Fn(&str) -> String) -> bool {
    feature.stage == Stage::Shipped
        || get(TEST_VAR)
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
    fn flags_name_every_feature_in_order() {
        let names: Vec<&str> = flags().iter().map(|(n, _)| *n).collect();
        let want: Vec<&str> = FEATURES.iter().map(|f| f.name).collect();
        assert_eq!(names, want);
    }
}
