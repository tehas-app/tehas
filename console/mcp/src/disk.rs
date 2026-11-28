//! `factory_disk`: how full this host is, and what fills it. The answer of
//! `tehas disk --publish`, read from `live/disk.json` and copied with an
//! allowlist, so the server asks the host's disk nothing: no `df`, no
//! `docker`, no walk of a tree. The file is there when a person or a program
//! ran the command with `--publish` and says when it was written; it is
//! nothing's job to keep it fresh, and an old answer is served with `stale`.

use crate::snapshot::Scalar;
use crate::tools::{self, Dirs, ToolError};
use serde::Serialize;
use serde_json::{json, Map, Value};
use tehas_core::mask::mask;

/// The name of the published file, under the live directory.
pub const FILE: &str = "disk.json";

/// A reading older than this is stale: the cleaner publishes one every hour,
/// and its run may take up to half an hour of walking.
const STALE_SECS: i64 = 3 * 3600;

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
        "description": "How full this host is and what fills it, as tehas disk published it: one entry per file system the factory cares about - the instance's home, Docker's own directory, /tmp and the scratch disk when the instance has one, named by those roles and never by the host's paths - with its mount and its free and total bytes, and with full the biggest places under them and what docker system df said. A figure that could not be read is null and carries why, never 0. The file is written by tehas disk --publish, which the hourly cleaner runs, so time says how old the answer is and stale is true when it is more than three hours old or has no time that can be read; no file, or one that is not a whole reading, is a tool error.",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": int("The API version, 1."),
            "time": text("When the answer was made, UTC, as the run that wrote it kept it."),
            "stale": {"type": "boolean", "description": "The reading is more than three hours old, or its time cannot be read: the hourly publish has not run, and the figures may no longer be true."},
            "filesystems": {
                "type": "array",
                "description": "One entry per file system, its paths naming every watched path that stands on it.",
                "items": {"type": "object", "properties": {
                    "paths": {"type": "array", "items": {"type": "string", "enum": ["home", "docker", "tmp", "scratch"]}, "description": "What the watched paths that stand on this file system are: home (the instance's home), docker (Docker's own directory), tmp or scratch (TEHAS_SCRATCH_DIR). The host's own paths are not served."},
                    "mount": text("What df names the file system's own, such as overlay; left out when it did not say, or when it named a device path or another host."),
                    "freeBytes": int_("Free bytes. Left out, with why, when the figure could not be read: no figure is never 0 bytes free."),
                    "totalBytes": int_("Total bytes. Left out, with why, when the figure could not be read."),
                    "why": text("Why no figure is there, when none is: df answered nothing, its word was no number, or the call did not answer. A path in it is named by its role."),
                }},
            },
            "full": {
                "type": "object",
                "description": "What fills them, when the run was asked with --full; left out when it was not.",
                "properties": {
                    "places": {"type": "array", "description": "The biggest places first.", "items": {
                        "type": "object", "properties": {
                            "path": text("The place: its path under the instance's home, or its name alone when it is outside the home."),
                            "bytes": int_("Its bytes. Left out, with cut, when it could not be measured."),
                            "cut": text("The bound the walk ran out of: entries, depth, time or unreadable. Left out when the figure is the whole of the place."),
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

/// The answer: the published file, kept to what the allowlist of `spec`
/// names, so no field of the host's own reaches the answer. A file that is
/// not there is the tool error the read says, which is the shape of a host
/// where nobody has asked yet.
pub fn call(dirs: &Dirs) -> Result<Map<String, Value>, ToolError> {
    call_at(dirs, tools::now())
}

fn call_at(dirs: &Dirs, now: i64) -> Result<Map<String, Value>, ToolError> {
    let answer = tools::read(&dirs.live, FILE)?;
    let refuse = |why: &str| ToolError::File {
        file: FILE.to_string(),
        why: why.to_string(),
    };
    if !answer.is_object() {
        return Err(refuse("is not one object"));
    }
    if answer["api"] != 1 {
        return Err(refuse("is of another api"));
    }
    // The writer names at least the instance's home, so a reading without a
    // file system is half a write, not a host with nothing on it.
    if answer["filesystems"]
        .as_array()
        .is_none_or(|a| a.is_empty())
    {
        return Err(refuse("names no file system"));
    }
    let home = answer["filesystems"][0]["paths"][0].as_str().unwrap_or("");
    let docker = answer["dockerRoot"].as_str().unwrap_or("/var/lib/docker");
    let role = |p: &str| match p {
        _ if p == home => "home",
        _ if p == docker => "docker",
        "/tmp" => "tmp",
        _ => "scratch",
    };
    let scalar = |v: &Value| serde_json::from_value::<Scalar>(v.clone()).ok();
    let out = DiskOut {
        time: scalar(&answer["time"]),
        filesystems: answer["filesystems"].as_array().map(|a| {
            a.iter()
                .filter_map(|f| f.as_object())
                .map(|f| {
                    // The host's own paths, longest first, so a path is
                    // never half-replaced by its parent in the reason.
                    let mut longest: Vec<&str> = f
                        .get("paths")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    longest.sort_by_key(|p| std::cmp::Reverse(p.len()));
                    let why = match f.get("why").and_then(scalar) {
                        Some(Scalar::Text(t)) => {
                            let mut said = t;
                            for p in longest.iter().filter(|p| !p.is_empty()) {
                                said = said.replace(p, role(p));
                            }
                            Some(Scalar::Text(said))
                        }
                        other => other,
                    };
                    // df's first column is a device path, or host:/export on
                    // a network mount. Only a plain name such as overlay is
                    // served.
                    let local = |m: &str| !m.starts_with('/') && !m.contains(':');
                    let mount = match f.get("mount").and_then(scalar) {
                        Some(Scalar::Text(m)) if !local(&m) => None,
                        m => m,
                    };
                    FsOut {
                        paths: f.get("paths").and_then(Value::as_array).map(|a| {
                            a.iter()
                                .filter_map(scalar)
                                .map(|p| role(p.text().unwrap_or("")).to_string())
                                .collect()
                        }),
                        mount,
                        free_bytes: f.get("freeBytes").and_then(scalar),
                        total_bytes: f.get("totalBytes").and_then(scalar),
                        why,
                    }
                })
                .collect()
        }),
        full: answer
            .get("full")
            .and_then(Value::as_object)
            .map(|full| FullOut {
                places: full.get("places").and_then(Value::as_array).map(|a| {
                    a.iter()
                        .filter_map(|p| p.as_object())
                        .map(|p| PlaceOut {
                            path: p.get("path").and_then(scalar).map(|s| match s {
                                Scalar::Text(path) => {
                                    let under = std::path::Path::new(&path).strip_prefix(home).ok();
                                    Scalar::Text(match under {
                                        Some(rel)
                                            if !home.is_empty() && rel.as_os_str().is_empty() =>
                                        {
                                            ".".to_string()
                                        }
                                        Some(rel) if !home.is_empty() => {
                                            rel.to_string_lossy().into_owned()
                                        }
                                        _ => std::path::Path::new(&path)
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default(),
                                    })
                                }
                                other => other,
                            }),
                            bytes: p.get("bytes").and_then(scalar),
                            cut: p.get("cut").and_then(scalar),
                        })
                        .collect()
                }),
                docker: full
                    .get("docker")
                    .and_then(Value::as_object)
                    .map(|d| DockerOut {
                        state: d.get("state").and_then(scalar),
                        // Docker's own words are host text like any log line, so
                        // they pass the mask, which also hides private network
                        // addresses, before they are served. A line that is no
                        // text is not one the schema promises, and is left out.
                        lines: d
                            .get("lines")
                            .and_then(Value::as_array)
                            .map(|a| a.iter().filter_map(Value::as_str).map(mask).collect()),
                    }),
                stopped_containers: full.get("stoppedContainers").and_then(scalar),
            }),
        stale: answer["time"]
            .as_str()
            .and_then(tehas_core::time::from_iso)
            .is_none_or(|t| now - t > STALE_SECS),
    };
    match serde_json::to_value(out) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(refuse("could not be served")),
    }
}

/// One file system of the answer, named by the roles of its watched paths.
#[derive(Serialize)]
struct FsOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mount: Option<Scalar>,
    #[serde(rename = "freeBytes", skip_serializing_if = "Option::is_none")]
    free_bytes: Option<Scalar>,
    #[serde(rename = "totalBytes", skip_serializing_if = "Option::is_none")]
    total_bytes: Option<Scalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    why: Option<Scalar>,
}

/// One place of the answer, its path under the instance's home or its name
/// alone when it is outside it.
#[derive(Serialize)]
struct PlaceOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<Scalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<Scalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cut: Option<Scalar>,
}

/// What `docker system df` said, masked line by line.
#[derive(Serialize)]
struct DockerOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<Scalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lines: Option<Vec<String>>,
}

/// What fills the disks, when the run was asked with --full.
#[derive(Serialize)]
struct FullOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    places: Option<Vec<PlaceOut>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    docker: Option<DockerOut>,
    #[serde(rename = "stoppedContainers", skip_serializing_if = "Option::is_none")]
    stopped_containers: Option<Scalar>,
}

/// The answer: the published file, held to what the allowlist names, so no
/// field of the host's own reaches it.
#[derive(Serialize)]
struct DiskOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    time: Option<Scalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filesystems: Option<Vec<FsOut>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    full: Option<FullOut>,
    stale: bool,
}

/// The allowlist the answer is copied with: what the file holds that a person
/// may be shown. `true` is the mark `pick` keeps a field on, as every other
/// tool's spec uses it, and a field the file has no figure for is `null` there
/// and so is left out of the answer - which is how no figure is never served
/// as 0. The containers' own ids and commands are not here: what the daemon
/// holds back is a count, and naming a container is not what a disk question
/// asks.
///
/// `stale` is computed by `call`, which writes over any the file has.
pub(crate) fn spec() -> Value {
    json!({
        "time": true,
        "stale": true,
        "filesystems": [{"paths": [true], "mount": true, "freeBytes": true, "totalBytes": true, "why": true}],
        "full": {
            "places": [{"path": true, "bytes": true, "cut": true}],
            "docker": {"state": true, "lines": [true]},
            "stoppedContainers": true,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tehas_core::testdir::TestDir;

    fn dirs(live: &Path) -> Dirs {
        Dirs {
            live: live.to_path_buf(),
            sessions: live.join("sessions"),
            logs: live.join("logs"),
            links: crate::tools::no_links,
        }
    }

    fn published(text: &str) -> TestDir {
        let dir = TestDir::new("mcp-disk");
        fs::write(dir.join(FILE), text).unwrap();
        dir
    }

    // The whole answer a run of `tehas disk --publish` wrote, with the two
    // things the tool must not carry: a container's id, and any field of the
    // host the allowlist does not name. The second file system has no figure,
    // only its reason, and the second place is a tree that could not be read.
    const ANSWER: &str = r#"{"api":1,"time":"2026-10-31T23:09:32Z","dockerRoot":"/var/lib/docker",
      "filesystems":[{"paths":["/home/boss/tehas"],"mount":"overlay","freeBytes":223,"totalBytes":307},
                     {"paths":["/tmp"],"why":"df answered nothing"}],
      "full":{"places":[{"path":"/home/boss/tehas/logs","bytes":2048,"cut":"whole"},
                        {"path":"/tmp/scratch","bytes":null,"cut":"unreadable"}],
              "docker":{"state":"read","lines":["TYPE\tTOTAL","Images\t3 TOKEN",7]},
              "stoppedContainers":2,
              "containers":[{"id":"ab12","image":"x","command":"/bin/sh"}],
              "hostname":"a name no answer gives"},"other":"not asked for"}"#;

    #[test]
    fn the_answer_carries_the_published_figures_and_nothing_else() {
        let token = format!("ghp_{}", "a".repeat(30));
        let dir = published(&ANSWER.replace("TOKEN", &token));
        let out = call(&dirs(&dir)).unwrap();
        let out = Value::Object(out);
        assert_eq!(out["time"], "2026-10-31T23:09:32Z");
        assert_eq!(out["filesystems"][0]["paths"], json!(["home"]));
        assert_eq!(out["filesystems"][1]["paths"], json!(["tmp"]));
        assert_eq!(out["full"]["places"][0]["path"], "logs");
        assert_eq!(out["full"]["places"][1]["path"], "scratch");
        assert_eq!(out["filesystems"][0]["freeBytes"], 223);
        assert_eq!(out["filesystems"][1]["why"], "df answered nothing");
        // A figure that the host could not read is absent, never served as 0:
        // the reason travels in its place.
        assert!(
            out["filesystems"][1].get("freeBytes").is_none(),
            "{}",
            out["filesystems"][1]
        );
        assert!(
            out["full"]["places"][1].get("bytes").is_none(),
            "{}",
            out["full"]["places"][1]
        );
        assert_eq!(out["full"]["places"][1]["cut"], "unreadable");
        assert_eq!(out["full"]["stoppedContainers"], 2);
        // Docker's lines are served, masked, and a line that is no text is
        // left out.
        assert_eq!(out["full"]["docker"]["state"], "read");
        let lines = out["full"]["docker"]["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0], "TYPE\tTOTAL");
        assert!(lines[1].as_str().unwrap().starts_with("Images\t3 "));
        // What the allowlist does not name does not reach the answer.
        let said = out.to_string();
        for forbidden in [
            "ab12",
            "a name no answer gives",
            "not asked for",
            "hostname",
            "dockerRoot",
            "/home/boss",
            "/var/lib/docker",
            "/tmp",
            "ghp_aaaa",
        ] {
            assert!(
                !said.contains(forbidden),
                "{forbidden} reached the answer: {said}"
            );
        }
    }

    // The host's own directories stay on the host: a file system is named by
    // the role of the paths on it, a place by its path under the home, and a
    // place outside the home by its name alone.
    #[test]
    fn paths_are_roles_and_places_are_relative_to_the_home() {
        let dir = published(
            r#"{"api":1,"time":"2026-10-31T23:09:32Z","dockerRoot":"/srv/docker",
              "filesystems":[{"paths":["/home/boss/tehas","/srv/docker"],"freeBytes":1,"totalBytes":2},
                             {"paths":["/tmp","/mnt/scratch"],"freeBytes":1,"totalBytes":2}],
              "full":{"places":[{"path":"/home/boss/tehas/dev/alpha.state/target","bytes":5},
                                {"path":"/home/boss/.pi/agent/sessions","bytes":4},
                                {"path":"/home/boss/tehas","bytes":3}]}}"#,
        );
        let out = Value::Object(call(&dirs(&dir)).unwrap());
        assert_eq!(out["filesystems"][0]["paths"], json!(["home", "docker"]));
        assert_eq!(out["filesystems"][1]["paths"], json!(["tmp", "scratch"]));
        let places: Vec<&str> = out["full"]["places"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["path"].as_str().unwrap())
            .collect();
        assert_eq!(places, ["dev/alpha.state/target", "sessions", "."]);
        assert!(!out.to_string().contains("/srv"), "{out}");
        assert!(!out.to_string().contains("boss"), "{out}");
    }

    // df's own words can name the host: the reason names the path df was
    // asked about, and the first column of a network mount names another
    // host. The reason names the role instead, and a mount that is a path or
    // a remote is left out.
    #[test]
    fn why_and_mount_name_no_path_and_no_other_host() {
        let dir = published(
            r#"{"api":1,"time":"2026-10-31T23:09:32Z","dockerRoot":"/var/lib/docker",
              "filesystems":[{"paths":["/home/boss/tehas"],"mount":"fileserver:/export/home","freeBytes":1,"totalBytes":2},
                             {"paths":["/mnt/scratch"],"why":"/mnt/scratch: df gave no figure"},
                             {"paths":["/tmp"],"mount":"/dev/sda1","why":"/tmp: df's figure is not a number of bytes"},
                             {"paths":["/var/lib/docker"],"mount":"overlay","freeBytes":1,"totalBytes":2}],
              "full":{"places":[]}}"#,
        );
        let out = Value::Object(call(&dirs(&dir)).unwrap());
        assert_eq!(out["filesystems"][1]["why"], "scratch: df gave no figure");
        assert_eq!(
            out["filesystems"][2]["why"],
            "tmp: df's figure is not a number of bytes"
        );
        assert!(out["filesystems"][0].get("mount").is_none(), "{out}");
        assert!(out["filesystems"][2].get("mount").is_none(), "{out}");
        assert_eq!(out["filesystems"][3]["mount"], "overlay");
        let said = out.to_string();
        for forbidden in ["/mnt", "/tmp", "/dev", "fileserver", "/export", "boss"] {
            assert!(
                !said.contains(forbidden),
                "{forbidden} reached the answer: {said}"
            );
        }
    }

    #[test]
    fn no_published_file_is_the_error_that_says_so() {
        let dir = TestDir::new("mcp-disk-none");
        let why = match call(&dirs(&dir)) {
            Err(ToolError::File { why, .. }) => why,
            other => panic!("a file that is not there is an error, not an answer: {other:?}"),
        };
        assert_eq!(why, "not published");
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
        }
    }

    // An object that is not a whole reading is refused too: a write cut
    // after its first field, a file of another api, and one that names no
    // file system would each answer as a healthy host with nothing on it.
    #[test]
    fn an_object_that_is_not_a_whole_reading_is_refused() {
        let fs = r#""filesystems":[{"paths":["/h"],"freeBytes":1,"totalBytes":2}]"#;
        for (text, want) in [
            (r#"{"api":1}"#.to_string(), "names no file system"),
            (
                r#"{"api":1,"filesystems":[]}"#.to_string(),
                "names no file system",
            ),
            (
                r#"{"api":1,"filesystems":{}}"#.to_string(),
                "names no file system",
            ),
            (format!(r#"{{"api":2,{fs}}}"#), "is of another api"),
            (format!(r#"{{{fs}}}"#), "is of another api"),
        ] {
            let dir = published(&text);
            match call(&dirs(&dir)) {
                Err(ToolError::File { why, .. }) => assert_eq!(why, want, "{text}"),
                other => panic!("{text} is no answer: {other:?}"),
            }
        }
    }

    // time alone leaves the caller to compare it with the clock, so
    // the answer says itself when the reading is too old to trust.
    #[test]
    fn an_old_reading_or_one_without_a_time_is_stale() {
        let at = tehas_core::time::from_iso("2026-10-31T23:09:32Z").unwrap();
        let dir = published(ANSWER);
        let stale = |now| call_at(&dirs(&dir), now).unwrap()["stale"].clone();
        assert_eq!(stale(at + 60), false);
        assert_eq!(stale(at + STALE_SECS), false);
        assert_eq!(stale(at + STALE_SECS + 1), true);
        for text in [
            r#"{"api":1,"stale":false,"filesystems":[{"paths":["/h"]}]}"#,
            r#"{"api":1,"time":"yesterday","stale":false,"filesystems":[{"paths":["/h"]}]}"#,
        ] {
            let dir = published(text);
            let out = call_at(&dirs(&dir), at).unwrap();
            assert_eq!(out["stale"], true, "{text}");
        }
    }

    // The answer names no field the tool's own schema does not describe, and
    // the schema names none the allowlist would not copy: the two are read
    // side by side here because they are written twice on purpose, the second
    // being the door a host's file comes through.
    #[test]
    fn the_schema_and_the_allowlist_name_the_same_fields() {
        let schema_tool = tool();
        let schema = schema_tool["outputSchema"]["properties"]
            .as_object()
            .unwrap();
        let allowed = spec();
        let allow = allowed.as_object().unwrap();
        for named in ["time", "filesystems", "full"] {
            assert!(!schema.contains_key("dockerRoot") && !allow.contains_key("dockerRoot"));
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
