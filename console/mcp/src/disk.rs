//! `factory_disk`: how full this host is, and what fills it. The answer of
//! `tehas disk --publish`, read from `live/disk.json` and copied with an
//! allowlist, so the server asks the host's disk nothing: no `df`, no
//! `docker`, no walk of a tree. The file is there when a person or a program
//! ran the command with `--publish` and says when it was written; it is
//! nothing's job to keep it fresh, and an old answer is read as one.

use crate::tools::{self, Dirs, ToolError};
use serde_json::{json, Map, Value};

/// The name of the published file, under the live directory.
pub const FILE: &str = "disk.json";

pub fn tool() -> Value {
    let int = |d: &str| json!({"type": "integer", "description": d});
    // A figure the host could not read is left out of the answer rather than
    // published as 0, so these fields are typed by what they hold when they
    // are there, as every other tool's optional field is, and `required`
    // names none of them.
    let int_ = |d: &str| json!({"type": "integer", "description": d});
    let text = |d: &str| json!({"type": "string", "description": d});
    json!({
        "name": "factory_disk",
        "description": "How full this host is and what fills it, as tehas disk published it: one entry per file system the factory cares about - the instance's home, Docker's own directory, /tmp and the scratch disk when the instance has one - with its mount and its free and total bytes, and with full the biggest places under them and what docker system df said. A figure that could not be read is null and carries why, never 0. The file is written by tehas disk --publish and nothing refreshes it, so generatedAt says how old the answer is; no file is a tool error.",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": int("The API version, 1."),
            "generatedAt": text("When the answer was made, UTC, as the run that wrote it kept it."),
            "dockerRoot": int_("The directory Docker keeps its images in, as Docker named it; left out when it did not say."),
            "filesystems": {
                "type": "array",
                "description": "One entry per file system, its paths naming every watched path that stands on it.",
                "items": {"type": "object", "properties": {
                    "paths": {"type": "array", "items": {"type": "string"}, "description": "The watched paths that stand on this file system."},
                    "mount": text("What df names the file system's own, such as overlay; left out when it did not say."),
                    "freeBytes": int_("Free bytes. Left out, with why, when the figure could not be read: no figure is never 0 bytes free."),
                    "totalBytes": int_("Total bytes. Left out, with why, when the figure could not be read."),
                    "why": text("Why no figure is there, when none is: df answered nothing, its word was no number, or the call did not answer."),
                }},
            },
            "full": {
                "type": "object",
                "description": "What fills them, when the run was asked with --full; left out when it was not.",
                "properties": {
                    "places": {"type": "array", "description": "The biggest places first.", "items": {
                        "type": "object", "properties": {
                            "path": text("The place."),
                            "bytes": int_("Its bytes. Left out, with cut, when it could not be measured."),
                            "cut": text("The bound the walk ran out of: whole, entries, depth, time or unreadable."),
                        },
                    }},
                    "docker": {"type": "object", "description": "docker system df, as the daemon printed it.", "properties": {
                        "state": {"type": "string", "enum": ["read", "none", "unreadable"], "description": "read answered lines, none answered nothing, unreadable did not answer."},
                        "lines": {"type": "array", "items": {"type": "string"}, "description": "Its lines, at most 12, each cut."},
                    }},
                    "stoppedContainers": int_("Containers the daemon still holds back. Left out when it did not answer."),
                },
            },
        }, "required": ["api"]},
    })
}

/// The answer: the published file, copied with `spec_of` so no field of the
/// host's own reaches the answer. A file that is not there is the tool error
/// the read says, which is the shape of a host where nobody has asked yet.
pub fn call(dirs: &Dirs) -> Result<Map<String, Value>, ToolError> {
    let answer = tools::read(&dirs.live, FILE)?;
    if !answer.is_object() {
        return Err(ToolError::File {
            file: FILE.to_string(),
            why: "is not one object".to_string(),
        });
    }
    let mut out = Map::new();
    tools::merge(&mut out, tools::pick(&answer, &spec()));
    Ok(out)
}

/// The allowlist the answer is copied with: what the file holds that a person
/// may be shown. `true` is the mark `pick` keeps a field on, as every other
/// tool's spec uses it, and a field the file has no figure for is `null` there
/// and so is left out of the answer - which is how no figure is never served
/// as 0. The containers' own ids and commands are not here: what the daemon
/// holds back is a count, and naming a container is not what a disk question
/// asks.
pub(crate) fn spec() -> Value {
    json!({
        "generatedAt": true,
        "dockerRoot": true,
        "filesystems": [{"paths": [], "mount": true, "freeBytes": true, "totalBytes": true, "why": true}],
        "full": {
            "places": [{"path": true, "bytes": true, "cut": true}],
            "docker": {"state": true, "lines": []},
            "stoppedContainers": true,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static ATOMIC: AtomicU64 = AtomicU64::new(0);

    fn dirs(live: &Path) -> Dirs {
        Dirs {
            live: live.to_path_buf(),
            sessions: live.join("sessions"),
            logs: live.join("logs"),
        }
    }

    fn published(text: &str) -> PathBuf {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "mcp-disk-{}-{}",
            std::process::id(),
            // One directory per case: the tests of this file run side by side
            // and would otherwise read and remove one another's file.
            ATOMIC.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE), text).unwrap();
        dir
    }

    // The whole answer a run of `tehas disk --publish` wrote, with the two
    // things the tool must not carry: a container's id, and any field of the
    // host the allowlist does not name. The second file system has no figure,
    // only its reason, and the second place is a tree that could not be read.
    const ANSWER: &str = r#"{"api":1,"generatedAt":"2026-10-31T23:09:32Z","dockerRoot":"/var/lib/docker",
      "filesystems":[{"paths":["/home/boss/tehas"],"mount":"overlay","freeBytes":223,"totalBytes":307},
                     {"paths":["/tmp"],"why":"df answered nothing"}],
      "full":{"places":[{"path":"/home/boss/tehas/logs","bytes":2048,"cut":"whole"},
                        {"path":"/tmp/scratch","bytes":null,"cut":"unreadable"}],
              "docker":{"state":"read","lines":["TYPE\tTOTAL"]},
              "stoppedContainers":2,
              "containers":[{"id":"ab12","image":"x","command":"/bin/sh"}],
              "hostname":"a name no answer gives"},"other":"not asked for"}"#;

    #[test]
    fn the_answer_carries_the_published_figures_and_nothing_else() {
        let dir = published(ANSWER);
        let out = call(&dirs(&dir)).unwrap();
        let out = Value::Object(out);
        assert_eq!(out["generatedAt"], "2026-10-31T23:09:32Z");
        assert_eq!(out["dockerRoot"], "/var/lib/docker");
        assert_eq!(out["filesystems"][0]["freeBytes"], 223);
        assert_eq!(out["filesystems"][1]["why"], "df answered nothing");
        // A figure that the host could not read is absent, never served as 0:
        // the reason travels in its place.
        assert!(out["filesystems"][1].get("freeBytes").is_none(), "{}", out["filesystems"][1]);
        assert!(out["full"]["places"][1].get("bytes").is_none(), "{}", out["full"]["places"][1]);
        assert_eq!(out["full"]["places"][1]["cut"], "unreadable");
        assert_eq!(out["full"]["stoppedContainers"], 2);
        // What the allowlist does not name does not reach the answer.
        let said = out.to_string();
        for forbidden in ["ab12", "a name no answer gives", "not asked for", "hostname"] {
            assert!(!said.contains(forbidden), "{forbidden} reached the answer: {said}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_published_file_is_the_error_that_says_so() {
        let dir = std::env::temp_dir().join(format!("mcp-disk-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let why = match call(&dirs(&dir)) {
            Err(ToolError::File { why, .. }) => why,
            other => panic!("a file that is not there is an error, not an answer: {other:?}"),
        };
        assert_eq!(why, "not published");
        let _ = fs::remove_dir_all(&dir);
    }

    // A file that holds something which is not one object - half a write, or
    // a file somebody else put under that name - is refused, not answered
    // with an empty object that would read as a host with nothing on it.
    #[test]
    fn a_file_that_is_not_one_object_is_refused() {
        for text in ["[]", "\"a word\"", "7", ""] {
            let dir = published(text);
            let said = match call(&dirs(&dir)) {
                Err(ToolError::File { why, .. }) => why,
                other => panic!("{text:?} is no answer: {other:?}"),
            };
            assert!(
                said == "is not complete JSON" || said == "is not one object",
                "{text:?} refused with {said:?}"
            );
            let _ = fs::remove_dir_all(&dir);
        }
    }

    // The answer names no field the tool's own schema does not describe, and
    // the schema names none the allowlist would not copy: the two are read
    // side by side here because they are written twice on purpose, the second
    // being the door a host's file comes through.
    #[test]
    fn the_schema_and_the_allowlist_name_the_same_fields() {
        let schema_tool = tool();
        let schema = schema_tool["outputSchema"]["properties"].as_object().unwrap();
        let allowed = spec();
        let allow = allowed.as_object().unwrap();
        for named in ["generatedAt", "dockerRoot", "filesystems", "full"] {
            assert!(schema.contains_key(named), "{named} has no schema");
            assert!(allow.contains_key(named), "{named} is not allowed through");
        }
        // The same fields, named once in each of the two literals this file
        // writes: the allowlist a call is copied with, and the schema a client
        // is told. They are written twice on purpose, so the pair is checked
        // here field by field rather than by reaching into one with the other.
        let full = allow["full"].as_object().unwrap();
        // The same fields, named once in each of the two literals this file
        // writes: the allowlist a call is copied with, and the schema a client
        // is told. They are written twice on purpose, so the pair is checked
        // here field by field rather than by reaching into one with the other.
        let full_schema = ["places", "docker", "stoppedContainers"];
        for named in full_schema {
            assert!(full.contains_key(named), "{named} is not allowed through");
            assert!(
                tool()["outputSchema"]["properties"]["full"]["properties"][named].is_object(),
                "{named} has no schema: {}",
                tool()["outputSchema"]["properties"]["full"]
            );
        }
        // The containers' own names are allowed nowhere.
        assert!(!full.contains_key("containers"));
    }
}
