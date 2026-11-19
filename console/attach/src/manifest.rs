//! The manifest.json of an issue's directory, keys in the order the readers of
//! it have always seen them, after the version of its format.

use crate::given::{cut, Ordered, CUT_LINK, MOST_LEFT};
use serde_json::Value;
use tehas_core::grammar::SPEC_BLOCK_VERSION;

/// One file given to a cycle. A file of an archive also says the archive and
/// where in it the file stood; a text file says the name it had.
pub struct Given<'a> {
    pub file: &'a str,
    pub link: &'a str,
    pub name: Option<&'a str>,
    pub kind: &'a str,
    pub bytes: i64,
    pub sha256: &'a str,
    pub source: &'a str,
    pub path: Option<&'a str>,
}

pub fn given(g: &Given) -> Ordered {
    let mut e = Ordered::map([
        ("file", Ordered::text(g.file)),
        ("link", Ordered::text(g.link)),
    ]);
    if let Some(name) = g.name {
        e.set("name", Ordered::text(name));
    }
    e.set("type", Ordered::text(g.kind));
    e.set("bytes", Ordered::Plain(g.bytes.into()));
    e.set("sha256", Ordered::text(g.sha256));
    e.set("source", Ordered::text(g.source));
    if let Some(path) = g.path {
        e.set("zip", Ordered::text(g.link));
        e.set("path", Ordered::text(path));
    }
    e
}

/// One link or entry left out, and why.
pub fn left(link: &str, reason: &str, source: &str, path: Option<&str>) -> Ordered {
    let mut e = Ordered::map([
        ("link", Ordered::text(link)),
        ("reason", Ordered::text(reason)),
        ("source", Ordered::text(source)),
    ]);
    if let Some(path) = path {
        e.set("path", Ordered::text(path));
    }
    e
}

/// At most 100 left-out entries, each link cut at 512 characters, and how many
/// more there were: an issue dense with attachments still leaves a manifest of
/// a bounded size.
fn capped(mut left: Vec<Ordered>) -> (Vec<Ordered>, usize) {
    let more = left.len().saturating_sub(MOST_LEFT);
    left.truncate(MOST_LEFT);
    for e in &mut left {
        if let Some(Ordered::Plain(Value::String(link))) = e.get("link") {
            let link = cut(link, CUT_LINK);
            e.set("link", Ordered::text(&link));
        }
    }
    (left, more)
}

/// The format of the manifest, the one `PI_ENSEMBLE_ISSUE_IMAGES` names. A
/// field is only added within it.
pub const VERSION: i64 = 1;

/// What `linksUnavailable` says of a text with no link to fetch: an empty
/// list is then no image to give, not an image the cycle could not see.
pub const NO_LINKS: &str = "The text links no file of GitHub's storage.";

/// What one start of an issue was given. `files` is absent where the instance
/// gives a cycle none. `unavailable`, where given, is `linksUnavailable`: why
/// the text gave no link to fetch, or null when every link is listed.
pub fn of_issue(
    images: Vec<Ordered>,
    files: Option<Vec<Ordered>>,
    left: Vec<Ordered>,
    unavailable: Option<Ordered>,
    issue: Value,
    approval: Ordered,
    comments: Option<Ordered>,
) -> Ordered {
    let (left, more) = capped(left);
    let mut m = Ordered::map([
        ("version", Ordered::Plain(VERSION.into())),
        ("images", Ordered::List(images)),
    ]);
    if let Some(files) = files {
        m.set("files", Ordered::List(files));
    }
    m.set("leftOut", Ordered::List(left));
    if let Some(why) = unavailable {
        m.set("linksUnavailable", why);
    }
    m.set("issue", Ordered::Plain(issue));
    m.set("approval", approval);
    if let Some(comments) = comments {
        m.set("comments", comments);
    }
    m.set("leftOutMore", Ordered::Plain(more.into()));
    m
}

/// The manifest of a directory given nothing, from a text that links nothing.
/// `issue` and `approval` are JSON; `files` is whether the instance gives a
/// cycle files, so the list is there.
pub fn nothing(issue: &str, approval: &str, files: bool) -> String {
    let files = if files { "  \"files\": [],\n" } else { "" };
    format!(
        "{{\n  \"version\": {VERSION},\n  \"images\": [],\n{files}  \"leftOut\": [],\n  \"linksUnavailable\": {},\n  \"issue\": {issue},\n  \"approval\": {approval},\n  \"leftOutMore\": 0\n}}\n",
        Value::from(NO_LINKS)
    )
}

/// The `spec` entry: the file of the spec an approval pinned, as the cycle
/// is given it, with its link, the name the link has, and the bytes and
/// sha256 that equal the approval's pin, and the version of the pull request
/// spec block the cycle writes.
pub fn spec(file: &str, link: &str, bytes: i64, sha256: &str) -> Ordered {
    Ordered::map([
        ("file", Ordered::text(file)),
        ("link", Ordered::text(link)),
        (
            "name",
            Ordered::text(link.rsplit('/').next().unwrap_or(link)),
        ),
        ("bytes", Ordered::Plain(bytes.into())),
        ("sha256", Ordered::text(sha256)),
        ("block", Ordered::Plain(SPEC_BLOCK_VERSION.into())),
    ])
}

/// The `approvedText` entry: the file that holds the title and body the
/// approval bound, as the start read and compared them.
pub fn approved_text(file: &str, bytes: i64, sha256: &str) -> Ordered {
    Ordered::map([
        ("file", Ordered::text(file)),
        ("bytes", Ordered::Plain(bytes.into())),
        ("sha256", Ordered::text(sha256)),
    ])
}

/// A manifest as written here with `entries` added at its end, in their
/// order. None when `text` is not a manifest of this format, or already has
/// one of the keys. The keys before keep their order and their text.
pub fn handed(text: &str, entries: &[(&str, Ordered)]) -> Option<String> {
    let doc: Value = serde_json::from_str(text).ok()?;
    let m = doc.as_object()?;
    if m.get("version").and_then(Value::as_i64) != Some(VERSION)
        || entries.iter().any(|(k, _)| m.contains_key(*k))
    {
        return None;
    }
    let head = text.strip_suffix("\n}\n")?;
    let mut out = head.to_string();
    for (key, value) in entries {
        let one = Ordered::Map(vec![(key.to_string(), value.clone())]).pretty();
        // `{\n  "key": value\n}\n` without its braces: one entry at the depth
        // of the manifest's own.
        let inner = one.strip_prefix("{\n")?.strip_suffix("\n}\n")?;
        out.push_str(",\n");
        out.push_str(inner);
    }
    out.push_str("\n}\n");
    serde_json::from_str::<Value>(&out).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_given_nothing_says_its_version_first() {
        let text = nothing("7", "null", false);
        assert!(text.starts_with("{\n  \"version\": 1,\n"), "{text}");
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            doc,
            serde_json::json!({"version": 1, "images": [], "leftOut": [], "linksUnavailable": NO_LINKS, "issue": 7, "approval": null, "leftOutMore": 0})
        );
        let doc: Value = serde_json::from_str(&nothing("7", r#"{"at":"a"}"#, true)).unwrap();
        assert_eq!(doc["files"], serde_json::json!([]));
        assert_eq!(doc["approval"]["at"], "a");
    }

    #[test]
    fn a_manifest_of_an_issue_says_its_version_first() {
        let m = of_issue(
            Vec::new(),
            None,
            Vec::new(),
            None,
            Value::from(7),
            Ordered::Plain(Value::Null),
            None,
        );
        assert!(m.pretty().starts_with("{\n  \"version\": 1,\n"));
    }

    #[test]
    fn a_handed_manifest_keeps_its_keys_and_adds_the_spec_and_the_text_last() {
        let text = nothing("7", r#"{"at":"a"}"#, false);
        let spec_entry = spec(
            "spec.md",
            "https://github.com/user-attachments/files/1/search.md",
            12,
            &"a".repeat(64),
        );
        let text_entry = approved_text("approved.json", 30, &"b".repeat(64));
        let out = handed(&text, &[("spec", spec_entry), ("approvedText", text_entry)]).unwrap();
        assert!(
            out.starts_with(text.strip_suffix("\n}\n").unwrap()),
            "{out}"
        );
        assert!(out.ends_with("\n  }\n}\n"), "{out}");
        let doc: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            doc["spec"],
            serde_json::json!({"file": "spec.md", "link": "https://github.com/user-attachments/files/1/search.md", "name": "search.md", "bytes": 12, "sha256": "a".repeat(64), "block": 1})
        );
        assert_eq!(
            doc["approvedText"],
            serde_json::json!({"file": "approved.json", "bytes": 30, "sha256": "b".repeat(64)})
        );
        let keys: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("  \""))
            .map(|l| l.trim_start().split('"').nth(1).unwrap())
            .collect();
        assert_eq!(
            keys,
            [
                "version",
                "images",
                "leftOut",
                "linksUnavailable",
                "issue",
                "approval",
                "leftOutMore",
                "spec",
                "approvedText"
            ]
        );
    }

    #[test]
    fn only_a_manifest_of_this_format_without_the_keys_is_handed_more() {
        let entry = approved_text("approved.json", 1, &"b".repeat(64));
        let one = [("approvedText", entry)];
        let text = nothing("7", "null", false);
        let once = handed(&text, &one).unwrap();
        assert_eq!(handed(&once, &one), None);
        assert_eq!(handed("{}\n", &one), None);
        assert_eq!(handed("[]\n", &one), None);
        assert_eq!(handed("", &one), None);
        let other = text.replace("\"version\": 1", "\"version\": 2");
        assert_eq!(handed(&other, &one), None);
        assert_eq!(handed(text.trim_end(), &one), None);
        let pretty = of_issue(
            Vec::new(),
            Some(Vec::new()),
            Vec::new(),
            None,
            Value::from(7),
            Ordered::Plain(Value::Null),
            None,
        )
        .pretty();
        assert!(handed(&pretty, &one).is_some());
    }
}
