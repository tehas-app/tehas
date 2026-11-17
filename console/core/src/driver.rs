//! The driver a cycle runs. One type for `tehas work --driver`, `TEHAS_DRIVER`,
//! the `driver` of an approval record, the labels that choose a driver for one
//! issue and the runner's verb, so these cannot drift apart.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Driver {
    Classic,
    Lean,
}

impl Driver {
    pub const ALL: [Driver; 2] = [Driver::Classic, Driver::Lean];

    /// The word of `--driver`, `TEHAS_DRIVER` and a record's `driver`.
    pub const fn word(self) -> &'static str {
        match self {
            Driver::Classic => "classic",
            Driver::Lean => "lean",
        }
    }

    /// The driver a word names, exactly as [`Driver::word`] spells it.
    pub fn named(word: &str) -> Option<Driver> {
        Driver::ALL.into_iter().find(|d| d.word() == word)
    }

    /// The runner's verb.
    pub const fn verb(self) -> &'static str {
        match self {
            Driver::Classic => "work",
            Driver::Lean => "work-lean",
        }
    }

    /// The label that chooses this driver for one issue.
    pub const fn label(self) -> &'static str {
        match self {
            Driver::Classic => "tehas:classic",
            Driver::Lean => "tehas:lean",
        }
    }

    /// The flags of a run that this driver's verb never takes, in any release
    /// of the runner. Tehas passing one is a fault of tehas, not of the image.
    pub const fn never_takes(self) -> &'static [&'static str] {
        match self {
            Driver::Classic => &[],
            Driver::Lean => &["--restart"],
        }
    }

    pub const fn other(self) -> Driver {
        match self {
            Driver::Classic => Driver::Lean,
            Driver::Lean => Driver::Classic,
        }
    }

    /// The instance's default driver, of the value of `TEHAS_DRIVER`: lean
    /// only when it says lean. The load refuses any other word.
    pub fn default_of(setting: &str) -> Driver {
        if setting == Driver::Lean.word() {
            Driver::Lean
        } else {
            Driver::Classic
        }
    }

    /// The label that chooses against `default`: the one whose applier counts.
    /// The other agrees with the default and chooses nothing.
    pub fn opt_out(default: Driver) -> &'static str {
        default.other().label()
    }

    /// The driver a record binds: none is classic. None for one this tehas
    /// does not know, which never runs as classic.
    pub fn of_record(record: &Value) -> Option<Driver> {
        match record.get("driver") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => Some(Driver::Classic),
            Some(Value::String(s)) if s.is_empty() => Some(Driver::Classic),
            Some(Value::String(s)) => Driver::named(s),
            Some(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_driver_is_named_by_its_word_only() {
        for d in Driver::ALL {
            assert_eq!(Driver::named(d.word()), Some(d));
        }
        assert_eq!(Driver::named("lean"), Some(Driver::Lean));
        assert_eq!(Driver::named("classic"), Some(Driver::Classic));
        for bad in ["Lean", "", " lean", "work-lean", "both"] {
            assert_eq!(Driver::named(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_driver_is_a_verb_of_the_runner_and_a_label() {
        assert_eq!(Driver::Classic.verb(), "work");
        assert_eq!(Driver::Lean.verb(), "work-lean");
        assert_eq!(Driver::Classic.label(), "tehas:classic");
        assert_eq!(Driver::Lean.label(), "tehas:lean");
    }

    #[test]
    fn lean_never_takes_a_restart_and_classic_takes_every_flag() {
        assert_eq!(Driver::Lean.never_takes(), &["--restart"]);
        assert!(Driver::Classic.never_takes().is_empty());
    }

    #[test]
    fn the_default_is_lean_only_when_the_setting_says_lean() {
        assert_eq!(Driver::default_of("lean"), Driver::Lean);
        for other in ["", "classic", "Lean"] {
            assert_eq!(Driver::default_of(other), Driver::Classic, "{other}");
        }
    }

    #[test]
    fn the_label_that_chooses_is_the_one_against_the_default() {
        assert_eq!(Driver::opt_out(Driver::Classic), "tehas:lean");
        assert_eq!(Driver::opt_out(Driver::Lean), "tehas:classic");
    }

    #[test]
    fn a_record_with_no_driver_is_classic_and_an_unknown_one_is_none() {
        assert_eq!(
            Driver::of_record(&json!({"driver": "lean"})),
            Some(Driver::Lean)
        );
        assert_eq!(
            Driver::of_record(&json!({"driver": "classic"})),
            Some(Driver::Classic)
        );
        for no in [
            json!({}),
            json!({"driver": null}),
            json!({"driver": false}),
            json!({"driver": ""}),
        ] {
            assert_eq!(Driver::of_record(&no), Some(Driver::Classic), "{no}");
        }
        for odd in [
            json!({"driver": "other"}),
            json!({"driver": "Lean"}),
            json!({"driver": 1}),
            json!({"driver": true}),
        ] {
            assert_eq!(Driver::of_record(&odd), None, "{odd}");
        }
    }
}
