//! `tehas panel`: the pages of the project the dashboard links, src/usage/panel.txt.
//!
//! The list is state/panels.json, mode 600, and the same object is published
//! to live/panels.json, which the dashboard reads (host/fun/panels.js). With no
//! panels both files are gone. Every write is under state/panel.lock, beside
//! the file and renamed into place. Reading makes no network call.

use crate::edge;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

const USAGE: &str = "usage: tehas panel [--json] | set <id> --label TEXT --url URL | clear <id>";
const MAX: usize = 4;
const ID_MAX: usize = 32;
const LABEL_MAX: usize = 24;
const URL_MAX: usize = 512;

/// The characters `tehas pause` makes a space of in a reason.
fn replaced(c: char) -> bool {
    matches!(c, '\u{1}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}')
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: String,
    pub label: String,
    pub url: String,
}

#[derive(Debug, PartialEq)]
pub enum Asked {
    Show { json: bool },
    Set(Panel),
    Clear(String),
}

pub fn id_ok(id: &str) -> bool {
    (1..=ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The label as it is kept: a control character is a space, the spaces at the
/// ends go. None when what is left is empty or too long.
pub fn label_of(text: &str) -> Option<String> {
    let folded: String = text
        .chars()
        .map(|c| if replaced(c) { ' ' } else { c })
        .collect();
    let kept = folded.trim_matches(' ');
    (1..=LABEL_MAX)
        .contains(&kept.chars().count())
        .then(|| kept.to_string())
}

/// The host of an `http://` or `https://` URL with no user or password, in
/// lower case and with no trailing dot, or why the URL is refused.
pub fn url_host(url: &str) -> Result<String, &'static str> {
    if url.len() > URL_MAX {
        return Err("the url is longer than 512 characters");
    }
    if !url.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("the url holds a space, a control character or a character outside ASCII");
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or("the url starts with neither https:// nor http://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err("the url holds a user or a password");
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.contains(']') => (h, Some(p)),
        _ => (authority, None),
    };
    if port.is_some_and(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return Err("the url's port is not a number");
    }
    let named = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && host.split('.').any(|l| !l.is_empty());
    let literal = host.len() > 2
        && host.starts_with('[')
        && host.ends_with(']')
        && host[1..host.len() - 1]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.');
    if !named && !literal {
        return Err("the url names no host");
    }
    Ok(host.trim_end_matches('.').to_ascii_lowercase())
}

/// Whether a host is the factory's own domain, which the dashboard is served
/// from: a sibling name is another origin, so it is allowed.
pub fn is_own(host: &str, domain: &str) -> bool {
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    !domain.is_empty() && host == domain
}

pub fn words(given: &[String]) -> Result<Asked, u8> {
    let bad = |why: &str| {
        eprintln!("tehas panel: {why}\n{USAGE}");
        Err(64)
    };
    let as_str: Vec<&str> = given.iter().map(String::as_str).collect();
    match as_str[..] {
        [] => Ok(Asked::Show { json: false }),
        ["--json"] => Ok(Asked::Show { json: true }),
        ["clear", id] if id_ok(id) => Ok(Asked::Clear(id.to_string())),
        ["clear", _] => bad("an id is 1 to 32 characters of a-z, 0-9 and -"),
        ["set", id, ref rest @ ..] => {
            if !id_ok(id) {
                return bad("an id is 1 to 32 characters of a-z, 0-9 and -");
            }
            let (mut label, mut url) = (None, None);
            let mut i = 0;
            while i < rest.len() {
                let Some(value) = rest.get(i + 1) else {
                    return bad("an option without its value");
                };
                match rest[i] {
                    "--label" if label.is_none() => label = Some(*value),
                    "--url" if url.is_none() => url = Some(*value),
                    _ => return bad("an option set does not know, or one given twice"),
                }
                i += 2;
            }
            let (Some(label), Some(url)) = (label, url) else {
                return bad("set wants both --label and --url");
            };
            let Some(label) = label_of(label) else {
                return bad("a label is one line of 1 to 24 characters");
            };
            if let Err(why) = url_host(url) {
                return bad(why);
            }
            Ok(Asked::Set(Panel {
                id: id.to_string(),
                label,
                url: url.to_string(),
            }))
        }
        _ => bad("show, set or clear"),
    }
}

fn held(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

/// The object of state/panels.json, live/panels.json and `--json`.
pub fn json(panels: &[Panel]) -> String {
    let list: Vec<String> = panels
        .iter()
        .map(|p| {
            format!(
                "{{\"id\":{},\"label\":{},\"url\":{}}}",
                held(&p.id),
                held(&p.label),
                held(&p.url)
            )
        })
        .collect();
    format!("{{\"api\":1,\"panels\":[{}]}}", list.join(","))
}

/// The list a file holds: none when there is no file, an error when it is not
/// a list of panels this command would have written.
pub fn parse(text: &str) -> Result<Vec<Panel>, ()> {
    let doc: Value = serde_json::from_str(text).map_err(|_| ())?;
    let list = doc.get("panels").and_then(Value::as_array).ok_or(())?;
    let mut panels: Vec<Panel> = Vec::new();
    for p in list {
        let field = |k: &str| p.get(k).and_then(Value::as_str).ok_or(());
        let (id, label, url) = (field("id")?, field("label")?, field("url")?);
        let good = id_ok(id)
            && label_of(label).as_deref() == Some(label)
            && url_host(url).is_ok()
            && panels.iter().all(|q| q.id != id);
        if !good {
            return Err(());
        }
        panels.push(Panel {
            id: id.to_string(),
            label: label.to_string(),
            url: url.to_string(),
        });
    }
    if panels.len() > MAX {
        return Err(());
    }
    Ok(panels)
}

fn read(file: &Path) -> Result<Vec<Panel>, String> {
    match fs::read_to_string(file) {
        Ok(text) => {
            parse(&text).map_err(|()| format!("{} holds no list of panels", file.display()))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{} could not be read: {e}", file.display())),
    }
}

/// The list after a set: an id that is there is replaced where it stands, a
/// new one goes last, and a fifth is refused.
pub fn with(mut panels: Vec<Panel>, panel: Panel) -> Result<Vec<Panel>, String> {
    if let Some(at) = panels.iter().position(|p| p.id == panel.id) {
        panels[at] = panel;
    } else if panels.len() >= MAX {
        return Err(format!(
            "at most {MAX} panels: clear one before setting '{}'",
            panel.id
        ));
    } else {
        panels.push(panel);
    }
    Ok(panels)
}

fn write(file: &Path, text: &str, mode: u32) -> std::io::Result<()> {
    tehas_core::file::write_atomic(
        file,
        text.as_bytes(),
        mode,
        &tehas_core::file::Temp::Predictable,
    )
}

fn gone(file: &Path) -> std::io::Result<()> {
    match fs::remove_file(file) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The list is kept and published, or both files go when it is empty.
fn store(home: &Path, panels: &[Panel]) -> Result<(), String> {
    let (state, live) = (home.join("state"), home.join("live"));
    let kept = state.join("panels.json");
    let shown = live.join("panels.json");
    let said = |f: &Path, e: std::io::Error| format!("{} not written: {e}", f.display());
    if panels.is_empty() {
        gone(&shown).map_err(|e| said(&shown, e))?;
        return gone(&kept).map_err(|e| said(&kept, e));
    }
    let text = format!("{}\n", json(panels));
    fs::create_dir_all(&state).map_err(|e| said(&state, e))?;
    write(&kept, &text, 0o600).map_err(|e| said(&kept, e))?;
    fs::create_dir_all(&live).map_err(|e| said(&live, e))?;
    write(&shown, &text, 0o644).map_err(|e| said(&shown, e))
}

fn say(panels: &[Panel], json_out: bool) {
    if json_out {
        tehas_core::outln!("{}", json(panels));
    } else if panels.is_empty() {
        tehas_core::outln!("no panels: the dashboard shows none");
    } else {
        for p in panels {
            tehas_core::outln!("{}\t{}\t{}", p.id, p.label, p.url);
        }
    }
}

fn home() -> Option<PathBuf> {
    let from = |name: &str| match env::var_os(name) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    };
    from("TEHAS_HOME").or_else(|| from("HOME"))
}

pub fn main(given: &[String]) -> u8 {
    let asked = match words(given) {
        Ok(a) => a,
        Err(rc) => return rc,
    };
    let Some(home) = home() else {
        eprintln!("tehas panel: neither TEHAS_HOME nor HOME is set");
        return 78;
    };
    run(&home, &env::var("TEHAS_DOMAIN").unwrap_or_default(), asked)
}

pub fn run(home: &Path, domain: &str, asked: Asked) -> u8 {
    let file = home.join("state/panels.json");
    if let Asked::Show { json } = asked {
        return match read(&file) {
            Ok(panels) => {
                say(&panels, json);
                0
            }
            Err(why) => {
                eprintln!("tehas panel: {why}");
                1
            }
        };
    }
    if let Asked::Set(p) = &asked {
        let host = url_host(&p.url).unwrap_or_default();
        if is_own(&host, domain) {
            eprintln!(
                "tehas panel: the url is on {host}, the factory's own domain: the frame would share the dashboard's origin. Use another name, such as a sibling of it\n{USAGE}"
            );
            return 64;
        }
    }
    let Some(_lock) = edge::lock(home, "panel", Some(Duration::from_secs(10))) else {
        eprintln!("tehas panel: another tehas panel is writing; try again");
        return 1;
    };
    let panels = match read(&file) {
        Ok(p) => p,
        Err(why) => {
            eprintln!("tehas panel: {why}; nothing written");
            return 1;
        }
    };
    let panels = match asked {
        Asked::Set(p) => match with(panels, p) {
            Ok(p) => p,
            Err(why) => {
                eprintln!("tehas panel: {why}\n{USAGE}");
                return 64;
            }
        },
        Asked::Clear(id) => panels.into_iter().filter(|p| p.id != id).collect(),
        Asked::Show { .. } => panels,
    };
    if let Err(why) = store(home, &panels) {
        eprintln!("tehas panel: {why}");
        return 1;
    }
    say(&panels, false);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tehas_core::testdir::TestDir;

    fn w(given: &[&str]) -> Result<Asked, u8> {
        words(&given.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn panel(id: &str) -> Panel {
        Panel {
            id: id.into(),
            label: format!("L {id}"),
            url: format!("https://{id}.example.org/"),
        }
    }

    #[test]
    fn an_id_is_lower_case_digits_and_hyphens() {
        for good in ["a", "support", "a-1", &"x".repeat(32)] {
            assert!(id_ok(good), "{good}");
        }
        for bad in ["", "A", "a_b", "a b", "é", &"x".repeat(33)] {
            assert!(!id_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_label_is_one_line_of_up_to_24_characters() {
        assert_eq!(label_of("Ask us").as_deref(), Some("Ask us"));
        assert_eq!(label_of(" a\nb\t").as_deref(), Some("a b"));
        assert_eq!(label_of(&"ä".repeat(24)).as_deref(), Some(&*"ä".repeat(24)));
        for bad in ["", "  ", "\n", &"x".repeat(25)] {
            assert_eq!(label_of(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_url_is_http_or_https_with_a_host_and_no_user() {
        assert_eq!(
            url_host("https://Support.Example.org./f/acme"),
            Ok("support.example.org".into())
        );
        assert_eq!(
            url_host("http://example.org:8080?x=1"),
            Ok("example.org".into())
        );
        assert_eq!(url_host("https://[::1]:443/"), Ok("[::1]".into()));
        for bad in [
            "",
            "ftp://example.org/",
            "javascript:alert(1)",
            "HTTPS://example.org/",
            "https://",
            "https:///x",
            "https://user@example.org/",
            "https://u:p@example.org/",
            "https://example.org:x/",
            "https://exa mple.org/",
            "https://evil.org\\@example.org/",
            "https://example.org/\n",
            "https://ä.org/",
        ] {
            assert!(url_host(bad).is_err(), "{bad:?}");
        }
        let long = format!("https://example.org/{}", "x".repeat(493));
        assert_eq!(long.len(), 513);
        assert!(url_host(&long).is_err());
        assert!(url_host(&long[..512]).is_ok());
    }

    #[test]
    fn the_own_domain_is_refused_and_a_sibling_is_not() {
        assert!(is_own("example.org", "example.org"));
        assert!(is_own("example.org", "Example.org."));
        assert!(!is_own("support.example.org", "example.org"));
        assert!(!is_own("example.org", ""));
    }

    #[test]
    fn the_words_of_each_verb() {
        assert_eq!(w(&[]), Ok(Asked::Show { json: false }));
        assert_eq!(w(&["--json"]), Ok(Asked::Show { json: true }));
        assert_eq!(w(&["clear", "a"]), Ok(Asked::Clear("a".into())));
        assert_eq!(
            w(&["set", "a", "--url", "https://x.org", "--label", "A"]),
            Ok(Asked::Set(Panel {
                id: "a".into(),
                label: "A".into(),
                url: "https://x.org".into()
            }))
        );
        for bad in [
            &["show"][..],
            &["--json", "x"],
            &["clear"],
            &["clear", "A"],
            &["clear", "a", "b"],
            &["set", "a"],
            &["set", "a", "--label", "A"],
            &["set", "a", "--label", "A", "--url"],
            &[
                "set",
                "a",
                "--label",
                "A",
                "--label",
                "B",
                "--url",
                "https://x.org",
            ],
            &["set", "a", "--label", "", "--url", "https://x.org"],
            &["set", "a", "--label", "A", "--url", "x.org"],
            &[
                "set",
                "a",
                "--label",
                "A",
                "--url",
                "https://x.org",
                "--x",
                "y",
            ],
        ] {
            assert_eq!(w(bad), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn the_object_reads_back_and_odd_files_are_not_lists() {
        let list = vec![panel("a"), panel("b")];
        assert_eq!(parse(&json(&list)), Ok(list));
        assert_eq!(parse(&json(&[])), Ok(vec![]));
        for bad in [
            "",
            "[]",
            "{}",
            r#"{"panels":[{"id":"a"}]}"#,
            r#"{"panels":[{"id":"A","label":"x","url":"https://x.org"}]}"#,
            r#"{"panels":[{"id":"a","label":"x","url":"javascript:1"}]}"#,
        ] {
            assert_eq!(parse(bad), Err(()), "{bad}");
        }
        let twice = json(&[panel("a"), panel("a")]);
        assert_eq!(parse(&twice), Err(()));
        let five: Vec<Panel> = ["a", "b", "c", "d", "e"].iter().map(|i| panel(i)).collect();
        assert_eq!(parse(&json(&five)), Err(()));
    }

    #[test]
    fn set_replaces_in_place_and_a_fifth_is_refused() {
        let mut list = vec![];
        for id in ["a", "b", "c", "d"] {
            list = with(list, panel(id)).unwrap();
        }
        let mut new_b = panel("b");
        new_b.label = "B".into();
        let list = with(list, new_b.clone()).unwrap();
        assert_eq!(list[1], new_b);
        assert_eq!(list.len(), 4);
        assert!(with(list, panel("e")).is_err());
    }

    #[test]
    fn the_files_follow_the_list_and_go_with_the_last() {
        let d = TestDir::new("panel");
        let set = |id: &str| run(&d, "example.org", Asked::Set(panel(id)));
        assert_eq!(set("a"), 0);
        let kept = d.join("state/panels.json");
        let shown = d.join("live/panels.json");
        assert_eq!(
            fs::read_to_string(&kept).unwrap(),
            fs::read_to_string(&shown).unwrap()
        );
        assert_eq!(
            fs::metadata(&kept).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&shown).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(run(&d, "", Asked::Clear("zz".into())), 0);
        assert_eq!(run(&d, "", Asked::Clear("a".into())), 0);
        assert!(!kept.exists() && !shown.exists());
        assert_eq!(run(&d, "", Asked::Clear("a".into())), 0);
    }

    #[test]
    fn the_own_domain_writes_nothing() {
        let d = TestDir::new("panel");
        let mut p = panel("a");
        p.url = "https://example.org/x".into();
        assert_eq!(run(&d, "example.org", Asked::Set(p)), 64);
        assert!(!d.join("state/panels.json").exists());
    }

    #[test]
    fn a_file_that_is_no_list_is_kept_and_said() {
        let d = TestDir::new("panel");
        fs::create_dir_all(d.join("state")).unwrap();
        fs::write(d.join("state/panels.json"), "nope").unwrap();
        assert_eq!(run(&d, "", Asked::Set(panel("a"))), 1);
        assert_eq!(run(&d, "", Asked::Show { json: true }), 1);
        assert_eq!(
            fs::read_to_string(d.join("state/panels.json")).unwrap(),
            "nope"
        );
    }
}
