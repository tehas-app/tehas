//! The arguments of one `tools/call`, read once where the request comes in.
//!
//! A tool asks for an argument by name and gets one of the few kinds a tool
//! takes; the client's JSON goes no further than this module.

use serde_json::Value;

/// What a client gave as the arguments of a call.
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    given: Value,
}

/// What the arguments are as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `null`: a call with none.
    Nothing,
    Object,
    /// Text, a number, a list: no arguments at all.
    Other,
}

/// One argument. A key that is absent and one given as `null` are both
/// `Absent`; `Args::has` tells them apart for the tool that must.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg<'a> {
    Absent,
    Text(&'a str),
    /// A whole number from 0 that fits a u64.
    Whole(u64),
    Flag(bool),
    List(Vec<Arg<'a>>),
    /// An object, a fraction, a number below 0 or over a u64.
    Other,
}

impl From<Value> for Args {
    fn from(given: Value) -> Self {
        Args { given }
    }
}

impl Args {
    pub fn shape(&self) -> Shape {
        match self.given {
            Value::Null => Shape::Nothing,
            Value::Object(_) => Shape::Object,
            _ => Shape::Other,
        }
    }

    /// The names given, in the object's order; none unless the arguments are
    /// an object.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.given
            .as_object()
            .into_iter()
            .flat_map(|m| m.keys().map(String::as_str))
    }

    /// Whether `key` is named at all, be it with `null`.
    pub fn has(&self, key: &str) -> bool {
        self.given.get(key).is_some()
    }

    pub fn get(&self, key: &str) -> Arg<'_> {
        self.given.get(key).map_or(Arg::Absent, Arg::read)
    }
}

impl<'a> Arg<'a> {
    fn read(v: &'a Value) -> Arg<'a> {
        match v {
            Value::Null => Arg::Absent,
            Value::String(s) => Arg::Text(s),
            Value::Bool(b) => Arg::Flag(*b),
            Value::Number(n) => n.as_u64().map_or(Arg::Other, Arg::Whole),
            Value::Array(a) => Arg::List(a.iter().map(Arg::read).collect()),
            Value::Object(_) => Arg::Other,
        }
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, Arg::Absent)
    }

    pub fn text(&self) -> Option<&'a str> {
        match self {
            Arg::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn whole(&self) -> Option<u64> {
        match self {
            Arg::Whole(n) => Some(*n),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_argument_is_read_as_one_of_the_kinds_a_tool_takes() {
        let args = Args::from(json!({
            "t": "x", "n": 7, "f": true, "l": ["a", 1, null], "none": null,
            "neg": -1, "frac": 1.5, "big": 1e30, "o": {"k": 1},
        }));
        assert_eq!(args.shape(), Shape::Object);
        assert_eq!(args.get("t"), Arg::Text("x"));
        assert_eq!(args.get("n"), Arg::Whole(7));
        assert_eq!(args.get("f"), Arg::Flag(true));
        assert_eq!(
            args.get("l"),
            Arg::List(vec![Arg::Text("a"), Arg::Whole(1), Arg::Absent])
        );
        for other in ["neg", "frac", "big", "o"] {
            assert_eq!(args.get(other), Arg::Other, "{other}");
        }
        assert_eq!(args.get("t").text(), Some("x"));
        assert_eq!(args.get("n").text(), None);
        assert_eq!(args.get("n").whole(), Some(7));
        assert_eq!(args.get("t").whole(), None);
    }

    #[test]
    fn null_and_a_missing_key_are_both_absent_and_has_tells_them_apart() {
        let args = Args::from(json!({"none": null}));
        assert!(args.get("none").is_absent());
        assert!(args.get("missing").is_absent());
        assert!(args.has("none"));
        assert!(!args.has("missing"));
        assert_eq!(args.keys().collect::<Vec<_>>(), ["none"]);
    }

    #[test]
    fn arguments_that_are_no_object_have_no_argument() {
        for (given, shape) in [
            (json!(null), Shape::Nothing),
            (json!("hit"), Shape::Other),
            (json!([{"issue": 1}]), Shape::Other),
            (json!(3), Shape::Other),
        ] {
            let args = Args::from(given);
            assert_eq!(args.shape(), shape);
            assert_eq!(args.keys().count(), 0);
            assert!(args.get("issue").is_absent());
            assert!(!args.has("issue"));
        }
    }
}
