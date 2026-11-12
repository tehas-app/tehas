//! `tehas host-message`: short messages an operator or a supervisor posts for
//! a factory, src/usage/host-message.txt.
//!
//! The list is state/host-messages.json, mode 600, and gone with no messages.
//! Every write is under state/host-message.lock, beside the file and renamed
//! into place. Reading makes no network call.

use crate::edge;
use crate::panel::{id_ok, url_host};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tehas_core::time::{from_iso, iso, now};

const USAGE: &str = "usage: tehas host-message [list] [--json] | set <id> --level info|warn|alert --text TEXT [--link URL] | clear <id>";
const MAX: usize = 20;
const TEXT_MAX: usize = 200;
const LEVELS: [&str; 3] = ["info", "warn", "alert"];

fn replaced(c: char) -> bool {
    matches!(c, '\u{1}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}')
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    pub level: String,
    pub text: String,
    pub link: Option<String>,
    pub at: String,
}

#[derive(Debug, PartialEq)]
pub enum Asked {
    List { json: bool },
    Set(Message),
    Clear(String),
}

/// The text as it is kept: a control character is a space, the spaces at the
/// ends go. None when what is left is empty or too long.
pub fn text_of(text: &str) -> Option<String> {
    let folded: String = text
        .chars()
        .map(|c| if replaced(c) { ' ' } else { c })
        .collect();
    let kept = folded.trim_matches(' ');
    (1..=TEXT_MAX)
        .contains(&kept.chars().count())
        .then(|| kept.to_string())
}

/// The arguments; `at` of a set is left empty for [`run`] to fill.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let bad = |why: &str| {
        eprintln!("tehas host-message: {why}\n{USAGE}");
        Err(64)
    };
    let as_str: Vec<&str> = given.iter().map(String::as_str).collect();
    match as_str[..] {
        [] | ["list"] => Ok(Asked::List { json: false }),
        ["--json"] | ["list", "--json"] => Ok(Asked::List { json: true }),
        ["clear", id] if id_ok(id) => Ok(Asked::Clear(id.to_string())),
        ["clear", _] => bad("an id is 1 to 32 characters of a-z, 0-9 and -"),
        ["set", id, ref rest @ ..] => {
            if !id_ok(id) {
                return bad("an id is 1 to 32 characters of a-z, 0-9 and -");
            }
            let (mut level, mut text, mut link) = (None, None, None);
            let mut i = 0;
            while i < rest.len() {
                let Some(value) = rest.get(i + 1) else {
                    return bad("an option without its value");
                };
                match rest[i] {
                    "--level" if level.is_none() => level = Some(*value),
                    "--text" if text.is_none() => text = Some(*value),
                    "--link" if link.is_none() => link = Some(*value),
                    _ => return bad("an option set does not know, or one given twice"),
                }
                i += 2;
            }
            let (Some(level), Some(text)) = (level, text) else {
                return bad("set wants both --level and --text");
            };
            if !LEVELS.contains(&level) {
                return bad("a level is info, warn or alert");
            }
            let Some(text) = text_of(text) else {
                return bad("a text is one line of 1 to 200 characters");
            };
            if let Some(Err(why)) = link.map(url_host) {
                return bad(&why.replace("the url", "the link"));
            }
            Ok(Asked::Set(Message {
                id: id.to_string(),
                level: level.to_string(),
                text,
                link: link.map(str::to_string),
                at: String::new(),
            }))
        }
        _ => bad("list, set or clear"),
    }
}

fn held(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

/// The object of state/host-messages.json and `list --json`.
pub fn json(messages: &[Message]) -> String {
    let list: Vec<String> = messages
        .iter()
        .map(|m| {
            format!(
                "{{\"id\":{},\"level\":{},\"text\":{},\"link\":{},\"at\":{}}}",
                held(&m.id),
                held(&m.level),
                held(&m.text),
                m.link.as_deref().map_or("null".to_string(), held),
                held(&m.at)
            )
        })
        .collect();
    format!("{{\"api\":1,\"messages\":[{}]}}", list.join(","))
}

/// The list a file holds, or an error when it is not a list this command
/// would have written.
pub fn parse(text: &str) -> Result<Vec<Message>, ()> {
    let doc: Value = serde_json::from_str(text).map_err(|_| ())?;
    let list = doc.get("messages").and_then(Value::as_array).ok_or(())?;
    let mut messages: Vec<Message> = Vec::new();
    for m in list {
        let field = |k: &str| m.get(k).and_then(Value::as_str).ok_or(());
        let (id, level, text, at) = (field("id")?, field("level")?, field("text")?, field("at")?);
        let link = match m.get("link") {
            Some(Value::Null) => None,
            Some(Value::String(l)) if url_host(l).is_ok() => Some(l.clone()),
            _ => return Err(()),
        };
        let good = id_ok(id)
            && LEVELS.contains(&level)
            && text_of(text).as_deref() == Some(text)
            && from_iso(at).is_some()
            && messages.iter().all(|q| q.id != id);
        if !good {
            return Err(());
        }
        messages.push(Message {
            id: id.to_string(),
            level: level.to_string(),
            text: text.to_string(),
            link,
            at: at.to_string(),
        });
    }
    if messages.len() > MAX {
        return Err(());
    }
    Ok(messages)
}

fn read(file: &Path) -> Result<Vec<Message>, String> {
    match fs::read_to_string(file) {
        Ok(text) => {
            parse(&text).map_err(|()| format!("{} holds no list of messages", file.display()))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{} could not be read: {e}", file.display())),
    }
}

/// The list after a set: an id that is there is replaced where it stands, a
/// new one goes last, and a 21st is refused.
pub fn with(mut messages: Vec<Message>, message: Message) -> Result<Vec<Message>, String> {
    if let Some(at) = messages.iter().position(|m| m.id == message.id) {
        messages[at] = message;
    } else if messages.len() >= MAX {
        return Err(format!(
            "at most {MAX} messages: clear one before setting '{}'",
            message.id
        ));
    } else {
        messages.push(message);
    }
    Ok(messages)
}

/// The list is kept, or the file goes when it is empty.
fn store(home: &Path, messages: &[Message]) -> Result<(), String> {
    let state = home.join("state");
    let file = state.join("host-messages.json");
    let said = |f: &Path, e: std::io::Error| format!("{} not written: {e}", f.display());
    if messages.is_empty() {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(said(&file, e)),
            _ => Ok(()),
        };
    }
    fs::create_dir_all(&state).map_err(|e| said(&state, e))?;
    tehas_core::file::write_atomic(
        &file,
        format!("{}\n", json(messages)).as_bytes(),
        0o600,
        &tehas_core::file::Temp::Predictable,
    )
    .map_err(|e| said(&file, e))
}

fn say(messages: &[Message], json_out: bool) {
    if json_out {
        tehas_core::outln!("{}", json(messages));
    } else if messages.is_empty() {
        tehas_core::outln!("no host messages");
    } else {
        for m in messages {
            let link = m.link.as_deref().unwrap_or("-");
            tehas_core::outln!("{}\t{}\t{}\t{}\t{}", m.id, m.level, m.at, m.text, link);
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
        eprintln!("tehas host-message: neither TEHAS_HOME nor HOME is set");
        return 78;
    };
    run(&home, asked, now())
}

pub fn run(home: &Path, asked: Asked, now: i64) -> u8 {
    let file = home.join("state/host-messages.json");
    if let Asked::List { json } = asked {
        return match read(&file) {
            Ok(messages) => {
                say(&messages, json);
                0
            }
            Err(why) => {
                eprintln!("tehas host-message: {why}");
                1
            }
        };
    }
    let Some(_lock) = edge::lock(home, "host-message", Some(Duration::from_secs(10))) else {
        eprintln!("tehas host-message: another tehas host-message is writing; try again");
        return 1;
    };
    let messages = match read(&file) {
        Ok(m) => m,
        Err(why) => {
            eprintln!("tehas host-message: {why}; nothing written");
            return 1;
        }
    };
    let messages = match asked {
        Asked::Set(mut m) => {
            m.at = iso(now);
            match with(messages, m) {
                Ok(m) => m,
                Err(why) => {
                    eprintln!("tehas host-message: {why}\n{USAGE}");
                    return 64;
                }
            }
        }
        Asked::Clear(id) => messages.into_iter().filter(|m| m.id != id).collect(),
        Asked::List { .. } => messages,
    };
    if let Err(why) = store(home, &messages) {
        eprintln!("tehas host-message: {why}");
        return 1;
    }
    say(&messages, false);
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

    fn message(id: &str, text: &str) -> Message {
        Message {
            id: id.into(),
            level: "info".into(),
            text: text.into(),
            link: None,
            at: String::new(),
        }
    }

    fn set(d: &Path, id: &str, text: &str) -> u8 {
        run(d, Asked::Set(message(id, text)), 1_789_000_000)
    }

    #[test]
    fn the_words_of_each_verb() {
        assert_eq!(w(&[]), Ok(Asked::List { json: false }));
        assert_eq!(w(&["list"]), Ok(Asked::List { json: false }));
        assert_eq!(w(&["--json"]), Ok(Asked::List { json: true }));
        assert_eq!(w(&["list", "--json"]), Ok(Asked::List { json: true }));
        assert_eq!(
            w(&["clear", "disk-low"]),
            Ok(Asked::Clear("disk-low".into()))
        );
        assert_eq!(
            w(&[
                "set",
                "disk-low",
                "--text",
                "Disk\t91% full ",
                "--level",
                "warn"
            ]),
            Ok(Asked::Set(Message {
                id: "disk-low".into(),
                level: "warn".into(),
                text: "Disk 91% full".into(),
                link: None,
                at: String::new(),
            }))
        );
        let Ok(Asked::Set(m)) = w(&[
            "set",
            "a",
            "--level",
            "alert",
            "--text",
            "x",
            "--link",
            "https://x.org/a",
        ]) else {
            panic!("set with a link");
        };
        assert_eq!(m.link.as_deref(), Some("https://x.org/a"));
        for bad in [
            &["show"][..],
            &["list", "x"],
            &["--json", "list"],
            &["clear"],
            &["clear", "a", "b"],
            &["set", "a"],
            &["set", "a", "--level", "info"],
            &["set", "a", "--text", "x"],
            &["set", "a", "--level", "info", "--text"],
            &[
                "set", "a", "--level", "info", "--text", "x", "--level", "warn",
            ],
            &["set", "a", "--level", "info", "--text", "x", "--x", "y"],
        ] {
            assert_eq!(w(bad), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn a_bad_id_level_text_or_link_is_refused() {
        let long = "x".repeat(201);
        for bad in [
            &["set", "Disk", "--level", "info", "--text", "x"][..],
            &["clear", "a_b"],
            &["set", "a", "--level", "loud", "--text", "x"],
            &["set", "a", "--level", "info", "--text", ""],
            &["set", "a", "--level", "info", "--text", " \n "],
            &["set", "a", "--level", "info", "--text", &long],
            &[
                "set",
                "a",
                "--level",
                "info",
                "--text",
                "x",
                "--link",
                "ftp://x.org/",
            ],
            &[
                "set", "a", "--level", "info", "--text", "x", "--link", "x.org",
            ],
        ] {
            assert_eq!(w(bad), Err(64), "{bad:?}");
        }
        assert!(text_of(&"ä".repeat(200)).is_some());
    }

    #[test]
    fn the_object_reads_back_and_odd_files_are_not_lists() {
        let mut a = message("a", "x");
        a.at = iso(0);
        let mut b = a.clone();
        b.id = "b".into();
        b.link = Some("https://x.org/".into());
        let list = vec![a.clone(), b];
        assert_eq!(parse(&json(&list)), Ok(list));
        assert_eq!(parse(&json(&[])), Ok(vec![]));
        assert_eq!(parse(&json(&[a.clone(), a])), Err(()));
        for bad in [
            "",
            "{}",
            r#"{"messages":[{"id":"a"}]}"#,
            r#"{"messages":[{"id":"a","level":"loud","text":"x","link":null,"at":"1970-01-01T00:00:00Z"}]}"#,
            r#"{"messages":[{"id":"a","level":"info","text":"x","at":"1970-01-01T00:00:00Z"}]}"#,
            r#"{"messages":[{"id":"a","level":"info","text":"x","link":null,"at":"now"}]}"#,
        ] {
            assert_eq!(parse(bad), Err(()), "{bad}");
        }
    }

    #[test]
    fn set_twice_keeps_one_message_with_the_new_text() {
        let d = TestDir::new("host-message");
        assert_eq!(set(&d, "a", "old"), 0);
        assert_eq!(set(&d, "b", "other"), 0);
        assert_eq!(run(&d, Asked::Set(message("a", "new")), 1_789_000_060), 0);
        let file = d.join("state/host-messages.json");
        let list = parse(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].id.as_str(), list[0].text.as_str()), ("a", "new"));
        assert_eq!(list[0].at, iso(1_789_000_060));
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn a_21st_id_is_refused_and_the_file_is_unchanged() {
        let d = TestDir::new("host-message");
        for i in 0..20 {
            assert_eq!(set(&d, &format!("m{i}"), "x"), 0);
        }
        let file = d.join("state/host-messages.json");
        let before = fs::read_to_string(&file).unwrap();
        assert_eq!(set(&d, "m20", "x"), 64);
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
        assert_eq!(set(&d, "m3", "y"), 0);
    }

    #[test]
    fn clear_of_the_last_removes_the_file() {
        let d = TestDir::new("host-message");
        let file = d.join("state/host-messages.json");
        assert_eq!(set(&d, "a", "x"), 0);
        assert_eq!(run(&d, Asked::Clear("zz".into()), 0), 0);
        assert!(file.exists());
        assert_eq!(run(&d, Asked::Clear("a".into()), 0), 0);
        assert!(!file.exists());
        assert_eq!(run(&d, Asked::Clear("a".into()), 0), 0);
    }

    #[test]
    fn a_file_that_is_no_list_is_kept_and_said() {
        let d = TestDir::new("host-message");
        fs::create_dir_all(d.join("state")).unwrap();
        fs::write(d.join("state/host-messages.json"), "nope").unwrap();
        assert_eq!(set(&d, "a", "x"), 1);
        assert_eq!(run(&d, Asked::List { json: true }, 0), 1);
        assert_eq!(
            fs::read_to_string(d.join("state/host-messages.json")).unwrap(),
            "nope"
        );
    }
}
