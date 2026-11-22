//! The product wiki's rules (docs/specs/2026-11-21-wiki.md), kept from core
//! when the wiki left it in 2.0 for the wiki plugin of 2.1 to vendor: the
//! check of what the model wrote, its render to HTML, and how builds are
//! named, put in place and kept.

use serde_json::Value;

pub mod check;
pub mod publish;
pub mod render;

/// The commit a build reads and what it says of itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Head {
    pub commit: String,
    pub date: String,
    pub describe: String,
}

/// A build's id: its date and the first eight hex of its commit,
/// `2026-11-21-68ffab1d`. None when either is not of its form.
pub fn id(date: &str, commit: &str) -> Option<String> {
    let d = date.as_bytes();
    let date_ok = d.len() == 10
        && d.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
    let hex = commit.get(..8)?;
    let hex_ok = commit.len() >= 8
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (date_ok && hex_ok).then(|| format!("{date}-{hex}"))
}

/// Whether a name is a build id, and so a directory a build may be in.
pub fn is_id(name: &str) -> bool {
    name.len() == 19
        && id(&name[..10], &name[11..]).as_deref() == Some(name)
        && &name[10..11] == "-"
}

/// A build's build.json, when it is an object whose id is a build id and
/// whose commit is full hex.
pub fn build_of(text: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(text).ok()?;
    let id = v.get("id")?.as_str()?;
    let commit = v.get("commit")?.as_str()?;
    let full = commit.len() == 40
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (is_id(id) && full && id[11..] == commit[..8]).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_id_is_the_date_and_eight_hex() {
        let hash = "68ffab1d0123456789abcdef0123456789abcdef";
        assert_eq!(
            id("2026-11-21", hash).as_deref(),
            Some("2026-11-21-68ffab1d")
        );
        assert_eq!(
            id("2026-11-21", "68ffab1d").as_deref(),
            Some("2026-11-21-68ffab1d")
        );
        for (date, commit) in [
            ("2026-1-21", hash),
            ("2026/11/21", hash),
            ("2026-11-21", "68FFAB1D"),
            ("2026-11-21", "68ffab1"),
            ("2026-11-21", "../../etc"),
        ] {
            assert_eq!(id(date, commit), None, "{date} {commit}");
        }
        assert!(is_id("2026-11-21-68ffab1d"));
        for bad in [
            "2026-11-21-68ffab1",
            "2026-11-21_68ffab1d",
            "current",
            "2026-11-21-68ffab1d0",
            "",
        ] {
            assert!(!is_id(bad), "{bad}");
        }
    }

    const A: &str = "68ffab1d0123456789abcdef0123456789abcdef";

    fn build(id: &str, commit: &str) -> String {
        json!({"id": id, "commit": commit, "describe": "v1.2.0-5-g68ffab1d", "builtAt": "2026-11-21T03:04:10Z"}).to_string()
    }

    #[test]
    fn a_build_json_must_name_itself() {
        assert!(build_of(&build("2026-11-21-68ffab1d", A)).is_some());
        assert!(build_of(&build("2026-11-21-00000000", A)).is_none());
        assert!(build_of(&build("2026-11-21-68ffab1d", "68ffab1d")).is_none());
        assert!(build_of(&build("../x", A)).is_none());
        assert!(build_of("[]").is_none());
        assert!(build_of("not json").is_none());
    }
}
