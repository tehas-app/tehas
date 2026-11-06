//! The kinds of an issue that are not states: a proposal of the improvement
//! loop, and a ticket at the prototype stage. Each is a label unless
//! `TEHAS_ISSUE_TYPES` maps it to one of the owner's issue types, written as
//! `improvement=Improvement prototype=Prototype`. A kind left out of the
//! setting stays its label.

/// The setting that maps a kind to an issue type.
pub const SETTING: &str = "TEHAS_ISSUE_TYPES";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Improvement,
    Prototype,
}

impl Kind {
    const ALL: [Kind; 2] = [Kind::Improvement, Kind::Prototype];

    /// The word the setting names the kind by.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Improvement => "improvement",
            Kind::Prototype => "prototype",
        }
    }

    /// The label that is the kind when the setting maps no type to it.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Improvement => "tehas:improvement",
            Kind::Prototype => "tehas:prototype",
        }
    }
}

/// The characters of a type name here. GitHub allows more, a space among
/// them, but a name goes into an argv and a query, and the owner chooses it.
fn is_type(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 50
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The pairs of the setting, or why it is no such value.
pub fn parse(value: &str) -> Result<Vec<(Kind, String)>, String> {
    let mut out: Vec<(Kind, String)> = Vec::new();
    for pair in value.split(' ').filter(|p| !p.is_empty()) {
        let Some((word, name)) = pair.split_once('=') else {
            return Err(format!("expected kind=Type pairs, such as improvement=Improvement prototype=Prototype, not {pair}"));
        };
        let Some(kind) = Kind::ALL.into_iter().find(|k| k.word() == word) else {
            return Err(format!(
                "{word} is no kind: expected improvement or prototype"
            ));
        };
        if !is_type(name) {
            return Err(format!(
                "{name:?} is no issue type name: one word of letters, digits, - and _"
            ));
        }
        if out.iter().any(|(k, _)| *k == kind) {
            return Err(format!("{word} is mapped twice"));
        }
        out.push((kind, name.to_string()));
    }
    Ok(out)
}

/// The issue type `value` maps `kind` to, or None: the kind is its label. A
/// value the load would refuse maps nothing; the load stops before a command
/// reads it.
pub fn type_of(value: &str, kind: Kind) -> Option<String> {
    parse(value)
        .ok()?
        .into_iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, t)| t)
}

/// [`type_of`] as the environment holds the setting.
pub fn type_from_env(kind: Kind) -> Option<String> {
    type_of(&std::env::var(SETTING).unwrap_or_default(), kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_maps_to_its_type_and_one_left_out_stays_a_label() {
        let v = "improvement=Improvement prototype=Proto_type-2";
        assert_eq!(
            type_of(v, Kind::Improvement).as_deref(),
            Some("Improvement")
        );
        assert_eq!(type_of(v, Kind::Prototype).as_deref(), Some("Proto_type-2"));
        assert_eq!(type_of("prototype=Prototype", Kind::Improvement), None);
        assert_eq!(type_of("", Kind::Prototype), None);
        assert_eq!(parse("").unwrap(), vec![]);
    }

    #[test]
    fn a_value_of_no_such_shape_is_refused_and_maps_nothing() {
        for bad in [
            "Improvement",
            "improvement=",
            "bug=Bug",
            "improvement=Two words",
            "improvement=A,B",
            "improvement=A improvement=B",
            "=Improvement",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
            assert_eq!(type_of(bad, Kind::Improvement), None, "{bad}");
        }
    }
}
