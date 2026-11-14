//! The reading of an instance's `state/pause.json`, the flag `tehas pause`
//! writes. Every reader of the file asks here, so the console, the dashboard,
//! the health page and the dispatcher give the same answer for the same name.
//!
//! `state` is `none` when nothing is at the name, `ok` for an object whose
//! `at` and `reason` are strings, and `unreadable` for anything else: a
//! dangling link, a directory, a damaged file, an object which names one key
//! twice. Unreadable is never none, so a reader holds every start rather than
//! finding no flag where something is.

use crate::limit::{object, oneline, quoted};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// A reading of the flag. The fields are empty unless the state is `ok`; the
/// reason is one line of at most 200 characters, and `by` is empty when the
/// file's is not a string.
#[derive(Debug, PartialEq, Clone)]
pub struct Pause {
    pub state: &'static str,
    pub by: String,
    pub at: String,
    pub reason: String,
}

impl Pause {
    fn of(state: &'static str) -> Pause {
        Pause {
            state,
            by: String::new(),
            at: String::new(),
            reason: String::new(),
        }
    }

    /// The reading as the dispatcher's snapshot holds it: null for none,
    /// `"unreadable"`, or the flag as {by, at, reason}.
    pub fn value(&self) -> Value {
        match self.state {
            "none" => Value::Null,
            "ok" => json!({"by": self.by, "at": self.at, "reason": self.reason}),
            _ => json!("unreadable"),
        }
    }
}

/// The flag at <file>. A name the file system does not know is no flag; a name
/// it knows but cannot read through is unreadable.
pub fn read(file: &Path) -> Pause {
    match fs::symlink_metadata(file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Pause::of("none"),
        Err(_) => return Pause::of("unreadable"),
        Ok(_) => {}
    }
    match fs::read(file) {
        Ok(bytes) => parse(&bytes),
        Err(_) => Pause::of("unreadable"),
    }
}

/// The reading of the bytes of a flag.
pub fn parse(bytes: &[u8]) -> Pause {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Pause::of("unreadable");
    };
    let Some(keys) = object(bytes) else {
        return Pause::of("unreadable");
    };
    // serde keeps the last of a key named twice and this scan the first, so
    // such a file is no flag to anyone.
    if keys
        .iter()
        .enumerate()
        .any(|(i, (name, _))| keys[..i].iter().any(|(seen, _)| seen == name))
    {
        return Pause::of("unreadable");
    }
    let held = |want: &str| {
        keys.iter()
            .find(|(name, _)| name == want)
            .map(|(_, at)| &text[at.0..at.1])
    };
    let (Some(at), Some(reason)) = (held("at").and_then(quoted), held("reason").and_then(quoted))
    else {
        return Pause::of("unreadable");
    };
    Pause {
        state: "ok",
        by: held("by").and_then(quoted).unwrap_or_default(),
        at,
        reason: oneline(&reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;
    use std::os::unix::fs::symlink;

    #[test]
    fn a_flag_reads_as_its_fields() {
        let p = parse(br#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"a\tb"}"#);
        assert_eq!(p.state, "ok");
        assert_eq!(
            p.value(),
            json!({"by":"boss","at":"2026-10-15T10:00:00Z","reason":"a b"})
        );
        // by may be anything and reads as nobody.
        let p = parse(br#"{"by":7,"at":"t","reason":""}"#);
        assert_eq!((p.state, p.by.as_str()), ("ok", ""));
    }

    #[test]
    fn what_is_no_flag_is_unreadable() {
        for bad in [
            "",
            "{",
            "[]",
            "null",
            r#"{"at":"t"}"#,
            r#"{"at":"t","reason":7}"#,
            r#"{"at":"t","reason":"r","at":"u"}"#,
        ] {
            assert_eq!(parse(bad.as_bytes()).value(), json!("unreadable"), "{bad}");
        }
    }

    #[test]
    fn only_a_name_nothing_holds_is_no_flag() {
        let d = TestDir::new("pause-read");
        let file = d.join("pause.json");
        assert_eq!(read(&file).value(), Value::Null);
        // A link whose target is gone is still something at the name.
        symlink(d.join("gone.json"), &file).unwrap();
        assert_eq!(read(&file).value(), json!("unreadable"));
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert_eq!(read(&file).value(), json!("unreadable"));
    }
}
