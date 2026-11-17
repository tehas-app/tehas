//! The spec pipe's handoff: what a start gives its cycle of what was approved,
//! beside the files of its issue. The spec an approval pinned, fetched again
//! and held to that pin, and the title and body the start compared with the
//! approval, each a file of the issue's directory with an entry in its
//! manifest.json. A reason here may be put on the issue, so it names no host
//! path.

use crate::fetch::{self, Pin, Unpinned};
use crate::given::Ordered;
use crate::manifest;
use crate::take::{link_ext, text_type};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// The most bytes a spec is fetched with: one file of an issue's.
pub const SPEC_MOST: i64 = 1024 * 1024;

/// The file of the approved title and body.
pub const APPROVED_FILE: &str = "approved.json";

/// The name the spec's file has in the directory: `spec`, with the extension
/// of its link when that is a text file's.
pub fn spec_file(link: &str) -> String {
    match link_ext(link).filter(|e| text_type(e).is_some()) {
        Some(ext) => format!("spec.{ext}"),
        None => "spec".to_string(),
    }
}

/// The title and body of the issue's answer, as the file the cycle reads:
/// the number, the title and the body as GitHub gave them. None when the
/// answer has no title or a body that is not text.
pub fn approved_json(number: u64, answer: &Value) -> Option<String> {
    let title = answer.get("title")?.as_str()?;
    let body = match answer.get("body") {
        None | Some(Value::Null) => "",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return None,
    };
    let doc = Ordered::map([
        ("number", Ordered::Plain(number.into())),
        ("title", Ordered::text(title)),
        ("body", Ordered::text(body)),
    ]);
    Some(doc.pretty())
}

/// What one handoff gives.
pub struct Handoff<'a> {
    /// The issue's directory, with its manifest.json already written.
    pub dir: &'a Path,
    pub number: u64,
    /// The issue as the start read it, its text held to the approval.
    pub answer: &'a Value,
    /// The approval's pin of a spec, if it has one: link and sha256.
    pub spec: Option<(&'a str, &'a str)>,
    /// The token a fetch from github.com sends.
    pub token: &'a str,
}

/// Why nothing may start: a spec of other bytes than the approval's, one that
/// could not be fetched, or a file that could not be written.
pub fn hand_over(h: &Handoff) -> Result<(), String> {
    let unwritten = || "what was approved could not be given to the cycle".to_string();
    let manifest = h.dir.join("manifest.json");
    let text = fs::read_to_string(&manifest).map_err(|_| unwritten())?;
    let mut entries = Vec::new();
    if let Some((link, sha256)) = h.spec {
        let name = spec_file(link);
        let file = h.dir.join(&name);
        let pin = Pin {
            link,
            sha256,
            most: SPEC_MOST,
        };
        fetch::fetch_pinned(&pin, &file, h.token).map_err(|u| unpinned(&u))?;
        let bytes = fs::metadata(&file).map_err(|_| unwritten())?.len();
        readable(&file).map_err(|_| unwritten())?;
        entries.push((
            "spec",
            manifest::spec(&name, link, bytes as i64, &sha256.to_ascii_lowercase()),
        ));
    }
    let doc = approved_json(h.number, h.answer).ok_or_else(|| {
        "the issue's title and body could not be read to give the cycle".to_string()
    })?;
    let file = h.dir.join(APPROVED_FILE);
    fs::write(&file, &doc)
        .and_then(|()| readable(&file))
        .map_err(|_| unwritten())?;
    let sum = fetch::sha256_of(&file).map_err(|_| unwritten())?;
    entries.push((
        "approvedText",
        manifest::approved_text(APPROVED_FILE, doc.len() as i64, &sum),
    ));
    let out = manifest::handed(&text, &entries).ok_or_else(unwritten)?;
    fs::write(&manifest, out).map_err(|_| unwritten())
}

fn readable(file: &Path) -> std::io::Result<()> {
    fs::set_permissions(file, fs::Permissions::from_mode(0o644))
}

/// Why a pinned spec gave the cycle no file.
pub fn unpinned(u: &Unpinned) -> String {
    match u {
        Unpinned::Other(got) => {
            format!("the bytes of its spec are not the ones approved: they have sha256 {got}")
        }
        Unpinned::Fetch(f) if f.code == fetch::TOO_BIG => format!(
            "its spec is larger than {} KiB, the most tehas fetches for one",
            SPEC_MOST / 1024
        ),
        Unpinned::Fetch(f) if f.http > 0 => format!(
            "its spec could not be fetched (curl exited {}, HTTP {})",
            f.code, f.http
        ),
        Unpinned::Fetch(f) => format!("its spec could not be fetched (curl exited {})", f.code),
        Unpinned::Unreadable(_) => "its spec could not be read back once fetched".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Fetched;
    use serde_json::json;
    use tehas_core::testdir::TestDir;

    #[test]
    fn the_spec_keeps_the_extension_of_a_text_file_only() {
        let f = "https://github.com/user-attachments/files/1";
        assert_eq!(spec_file(&format!("{f}/search.md")), "spec.md");
        assert_eq!(spec_file(&format!("{f}/Search.TXT")), "spec.txt");
        assert_eq!(spec_file(&format!("{f}/search.pdf")), "spec");
        assert_eq!(spec_file(&format!("{f}/search")), "spec");
        assert_eq!(spec_file(&format!("{f}/a.sh")), "spec");
    }

    #[test]
    fn the_approved_text_is_the_title_and_the_body_as_given() {
        let answer = json!({"title": "Faster search", "body": "Do it.\r\n\r\nSpec: x", "user": {}});
        let doc = approved_json(7, &answer).unwrap();
        assert_eq!(
            doc,
            "{\n  \"number\": 7,\n  \"title\": \"Faster search\",\n  \"body\": \"Do it.\\r\\n\\r\\nSpec: x\"\n}\n"
        );
        let none = approved_json(7, &json!({"title": "t", "body": null})).unwrap();
        assert!(none.contains("\"body\": \"\""), "{none}");
        assert_eq!(approved_json(7, &json!({"body": "b"})), None);
        assert_eq!(approved_json(7, &json!({"title": "t", "body": 3})), None);
    }

    #[test]
    fn a_handoff_with_no_spec_gives_the_approved_text_alone() {
        let dir = TestDir::new("attach-handoff-text");
        let text = manifest::nothing("7", "null", false);
        fs::write(dir.path().join("manifest.json"), &text).unwrap();
        let answer = json!({"title": "T", "body": "B"});
        let h = Handoff {
            dir: dir.path(),
            number: 7,
            answer: &answer,
            spec: None,
            token: "",
        };
        hand_over(&h).unwrap();
        let written = fs::read_to_string(dir.path().join(APPROVED_FILE)).unwrap();
        let m: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(m.get("spec"), None);
        assert_eq!(m["approvedText"]["file"], APPROVED_FILE);
        assert_eq!(m["approvedText"]["bytes"], written.len());
        assert_eq!(
            m["approvedText"]["sha256"],
            fetch::sha256_of(&dir.path().join(APPROVED_FILE)).unwrap()
        );
        assert_eq!(m["issue"], 7);
        // A second handoff into the same manifest is refused, not doubled.
        assert_eq!(
            hand_over(&h),
            Err("what was approved could not be given to the cycle".to_string())
        );
    }

    #[test]
    fn a_handoff_into_no_manifest_or_of_no_title_gives_nothing() {
        let dir = TestDir::new("attach-handoff-none");
        let answer = json!({"title": "T", "body": "B"});
        let mut h = Handoff {
            dir: dir.path(),
            number: 7,
            answer: &answer,
            spec: None,
            token: "",
        };
        assert!(hand_over(&h).is_err());
        fs::write(
            dir.path().join("manifest.json"),
            manifest::nothing("7", "null", false),
        )
        .unwrap();
        let untitled = json!({"body": "B"});
        h.answer = &untitled;
        assert_eq!(
            hand_over(&h),
            Err("the issue's title and body could not be read to give the cycle".to_string())
        );
    }

    #[test]
    fn a_spec_that_is_not_the_one_approved_is_said_by_why() {
        let other = Unpinned::Other("c".repeat(64));
        assert_eq!(
            unpinned(&other),
            format!(
                "the bytes of its spec are not the ones approved: they have sha256 {}",
                "c".repeat(64)
            )
        );
        let f = |code, http| {
            Unpinned::Fetch(Fetched {
                code,
                content_type: String::new(),
                http,
            })
        };
        assert_eq!(
            unpinned(&f(fetch::TOO_BIG, 0)),
            "its spec is larger than 1024 KiB, the most tehas fetches for one"
        );
        assert_eq!(
            unpinned(&f(22, 404)),
            "its spec could not be fetched (curl exited 22, HTTP 404)"
        );
        assert_eq!(
            unpinned(&f(6, 0)),
            "its spec could not be fetched (curl exited 6)"
        );
    }
}
