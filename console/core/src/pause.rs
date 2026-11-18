//! The reading of an instance's `state/pause.json`, the flag `tehas pause`
//! writes. Every reader of the file asks here, so the console, the dashboard,
//! the health page and the dispatcher give the same answer for the same name.
//!
//! `state` is `none` when nothing is at the name, `ok` for an object whose
//! `at` and `reason` are strings, and `unreadable` for anything else: a
//! dangling link, a directory, a damaged file, an object which names one key
//! twice. Unreadable is never none, so a reader holds every start rather than
//! finding no flag where something is.

use crate::file::{ReadError, READ_BOUND};
use crate::limit::{object, oneline, quoted};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
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
    match bytes(file) {
        Ok(None) => Pause::of("none"),
        Ok(Some(b)) => parse(&b),
        Err(_) => Pause::of("unreadable"),
    }
}

/// The bytes at <file>, or None when nothing is at the name. A link is read
/// through to the file it points at, as an operator may keep the flag
/// elsewhere. A dangling link, a file that is not regular, one over
/// `READ_BOUND` or one that cannot be read is an error, which a reader takes
/// as unreadable.
pub fn bytes(file: &Path) -> Result<Option<Vec<u8>>, ReadError> {
    match fs::symlink_metadata(file) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ReadError::Io(e)),
        Ok(_) => {}
    }
    let f = File::open(file).map_err(ReadError::Io)?;
    let m = f.metadata().map_err(ReadError::Io)?;
    if !m.is_file() {
        return Err(ReadError::NotRegular);
    }
    if m.len() > READ_BOUND {
        return Err(ReadError::TooLarge);
    }
    let mut b = Vec::new();
    f.take(READ_BOUND + 1)
        .read_to_end(&mut b)
        .map_err(ReadError::Io)?;
    if b.len() as u64 > READ_BOUND {
        return Err(ReadError::TooLarge);
    }
    Ok(Some(b))
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

    #[test]
    fn a_flag_over_the_bound_is_unreadable_and_a_link_is_read_through() {
        let d = TestDir::new("pause-bound");
        let file = d.join("pause.json");
        let pad = " ".repeat(READ_BOUND as usize);
        fs::write(&file, format!(r#"{{"at":"t","reason":"r"}}{pad}"#)).unwrap();
        assert_eq!(read(&file).value(), json!("unreadable"));
        let real = d.join("real.json");
        fs::write(&real, r#"{"at":"t","reason":"r"}"#).unwrap();
        assert_eq!(read(&real).state, "ok");
        fs::remove_file(&file).unwrap();
        symlink(&real, &file).unwrap();
        assert_eq!(read(&file).state, "ok");
        fs::remove_file(&real).unwrap();
        assert_eq!(read(&file).value(), json!("unreadable"));
    }

    #[test]
    fn a_long_reason_within_the_bound_is_cut_to_200() {
        let d = TestDir::new("pause-long");
        let file = d.join("pause.json");
        let long = "x".repeat(10_000);
        fs::write(&file, format!(r#"{{"at":"t","reason":"{long}"}}"#)).unwrap();
        let p = read(&file);
        assert_eq!((p.state, p.reason.chars().count()), ("ok", 200));
    }
}
