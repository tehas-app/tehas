//! The runs of plugins, rows of `plugin_runs`
//! (docs/specs/2026-11-21-plugins.md, "Runs"):
//!
//!   plugin-run-begin --plugin <name> --commit <sha> --kind refresh|hook|call
//!                    --how timer|host|action|cycle|chat|mcp --by <who> --head <sha>
//!   plugin-run-end --run N --result done|failed|timeout|lost [--tokens N]
//!                  [--size N]
//!   plugin-runs --plugin <name> [--limit N] [--no-calls]
//!
//! `begin` answers the run's id, `end` the row it ended, and `plugin-runs`
//! the plugin's last runs, newest first. A run ends once. A call
//! (docs/specs/2026-11-21-plugins-v2-chats.md, "A call") is started by a
//! chat, the MCP server or the host, and keeps no login: its `by` is `chat`,
//! the MCP key's 8 hex id, or `host`. Its `size` is the bytes of its answer.

use crate::verbs::{self, line, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
use tehas_core::grammar::commit_id;
use tehas_core::plugin::is_name;

const KINDS: [&str; 3] = ["refresh", "hook", "call"];
const HOWS: [&str; 6] = ["timer", "host", "action", "cycle", "chat", "mcp"];
const RESULTS: [&str; 4] = ["done", "failed", "timeout", "lost"];
/// The most rows `plugin-runs` answers, and its default.
const LIMIT: i64 = 50;

#[derive(Debug, PartialEq, Eq)]
pub struct Begin {
    pub plugin: String,
    pub commit: String,
    pub kind: &'static str,
    pub how: &'static str,
    pub by: String,
    pub head: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Begin(Begin),
    End {
        run: i64,
        result: &'static str,
        tokens: i64,
        size: Option<i64>,
    },
    Runs {
        plugin: String,
        limit: i64,
        /// Leave out the calls of tools, so a refresh or a hook is found
        /// however many calls came after it.
        no_calls: bool,
    },
}

fn one_of(a: &Args, flag: &str, words: &[&'static str]) -> Result<&'static str, String> {
    let given = a.need(flag)?;
    words
        .iter()
        .find(|w| **w == given)
        .copied()
        .ok_or_else(|| a.usage(flag, &format!("is one of {}", words.join(", "))))
}

fn plugin(a: &Args) -> Result<String, String> {
    let name = a.need("--plugin")?;
    is_name(name)
        .then(|| name.to_string())
        .ok_or_else(|| a.usage("--plugin", "is a plugin's name"))
}

fn commit(a: &Args, flag: &str) -> Result<String, String> {
    let c = a.need(flag)?;
    commit_id(c)
        .then(|| c.to_string())
        .ok_or_else(|| a.usage(flag, "is a whole commit"))
}

/// Who started a run: a GitHub login, `timer` or `host`.
fn by(a: &Args) -> Result<String, String> {
    let b = a.need("--by")?;
    verbs::account(b).ok_or_else(|| a.usage("--by", "is a login, timer or host"))
}

/// A call keeps who started it only as `chat`, a key's id or `host`, and
/// nothing else is started by a chat or the MCP server.
fn held_to_kind(a: &Args, b: Begin) -> Result<Begin, String> {
    let fits = match (b.kind, b.how) {
        ("call", "chat") => b.by == "chat",
        ("call", "mcp") => {
            b.by.len() == 8 && b.by.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
        }
        ("call", "host") => b.by == "host",
        ("call", _) => return Err(a.usage("--how", "of a call is chat, mcp or host")),
        (_, "chat" | "mcp") => {
            return Err(a.usage(
                "--how",
                "of a refresh or a hook is timer, host, action or cycle",
            ))
        }
        _ => true,
    };
    if fits {
        Ok(b)
    } else {
        Err(a.usage("--by", "of a call is chat, the key's 8 hex id, or host"))
    }
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "plugin-run-begin" => Args::parse(
            "plugin-run-begin",
            rest,
            &[
                ("--plugin", Kind::Value),
                ("--commit", Kind::Value),
                ("--kind", Kind::Value),
                ("--how", Kind::Value),
                ("--by", Kind::Value),
                ("--head", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let b = Begin {
                plugin: plugin(&a)?,
                commit: commit(&a, "--commit")?,
                kind: one_of(&a, "--kind", &KINDS)?,
                how: one_of(&a, "--how", &HOWS)?,
                by: by(&a)?,
                head: commit(&a, "--head")?,
            };
            held_to_kind(&a, b).map(Call::Begin)
        }),
        "plugin-run-end" => Args::parse(
            "plugin-run-end",
            rest,
            &[
                ("--run", Kind::Value),
                ("--result", Kind::Value),
                ("--tokens", Kind::Value),
                ("--size", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let run = verbs::number(a.need("--run")?)
                .filter(|n| *n > 0)
                .ok_or_else(|| a.usage("--run", "is a run's id"))?;
            let tokens = match a.value("--tokens") {
                None => 0,
                Some(t) => verbs::count(t).ok_or_else(|| a.usage("--tokens", "is a count"))?,
            };
            let size = a
                .value("--size")
                .map(|n| verbs::count(n).ok_or_else(|| a.usage("--size", "is a count")))
                .transpose()?;
            Ok(Call::End {
                run,
                result: one_of(&a, "--result", &RESULTS)?,
                tokens,
                size,
            })
        }),
        "plugin-runs" => Args::parse(
            "plugin-runs",
            rest,
            &[
                ("--plugin", Kind::Value),
                ("--limit", Kind::Value),
                ("--no-calls", Kind::Switch),
            ],
            false,
        )
        .and_then(|a| {
            let limit = match a.value("--limit") {
                None => LIMIT,
                Some(n) => verbs::number(n)
                    .filter(|n| (1..=LIMIT).contains(n))
                    .ok_or_else(|| a.usage("--limit", &format!("is 1 to {LIMIT}")))?,
            };
            Ok(Call::Runs {
                plugin: plugin(&a)?,
                limit,
                no_calls: a.has("--no-calls"),
            })
        }),
        _ => return None,
    })
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Begin(b) => {
            let id = verbs::write(home, |c| {
                c.execute(
                    "INSERT INTO plugin_runs (plugin, plugin_commit, kind, how, started_by, head, started_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![b.plugin, b.commit, b.kind, b.how, b.by, b.head, now],
                )
                .q()?;
                Ok(c.last_insert_rowid())
            })?;
            Ok(line("plugin-run-begin", &[("run", id.to_string())]))
        }
        Call::End {
            run,
            result,
            tokens,
            size,
        } => {
            let row = verbs::write(home, |c| {
                let n = c
                    .execute(
                        "UPDATE plugin_runs SET ended_at = ?2, result = ?3, tokens = ?4, size = ?5
                         WHERE id = ?1 AND ended_at IS NULL",
                        params![run, now, result, tokens, size],
                    )
                    .q()?;
                if n == 0 {
                    let known: Option<i64> = c
                        .query_row("SELECT 1 FROM plugin_runs WHERE id = ?1", [run], |r| {
                            r.get(0)
                        })
                        .optional()
                        .q()?;
                    return Err(if known.is_some() {
                        (1, format!("conflict: run {run} has ended"))
                    } else {
                        (1, format!("conflict: no run {run}"))
                    });
                }
                one(c, *run)
            })?;
            Ok(line("plugin-run-end", &[("run", row.to_string())]))
        }
        Call::Runs {
            plugin,
            limit,
            no_calls,
        } => {
            let rows = verbs::look(home, |c| runs(c, plugin, *limit, *no_calls))?;
            Ok(line(
                "plugin-runs",
                &[("runs", Value::Array(rows).to_string())],
            ))
        }
    }
}

const COLUMNS: &str =
    "id, plugin, plugin_commit, kind, how, started_by, head, started_at, ended_at, result, tokens, size";

fn json_of(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?,
        "plugin": r.get::<_, String>(1)?,
        "commit": r.get::<_, String>(2)?,
        "kind": r.get::<_, String>(3)?,
        "trigger": r.get::<_, String>(4)?,
        "by": r.get::<_, String>(5)?,
        "head": r.get::<_, String>(6)?,
        "start": r.get::<_, i64>(7)?,
        "end": r.get::<_, Option<i64>>(8)?,
        "result": r.get::<_, Option<String>>(9)?,
        "tokens": r.get::<_, i64>(10)?,
        "size": r.get::<_, Option<i64>>(11)?,
    }))
}

fn one(c: &Connection, id: i64) -> Result<Value, Fail> {
    c.query_row(
        &format!("SELECT {COLUMNS} FROM plugin_runs WHERE id = ?1"),
        [id],
        json_of,
    )
    .q()
}

/// The last runs of `plugin`, newest first, with or without its calls.
pub fn runs(c: &Connection, plugin: &str, limit: i64, no_calls: bool) -> Result<Vec<Value>, Fail> {
    let mut q = c
        .prepare(&format!(
            "SELECT {COLUMNS} FROM plugin_runs WHERE plugin = ?1 AND NOT (?3 AND kind = 'call')
             ORDER BY started_at DESC, id DESC LIMIT ?2"
        ))
        .q()?;
    let rows = q.query_map(params![plugin, limit, no_calls], json_of).q()?;
    rows.collect::<rusqlite::Result<Vec<_>>>().q()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs_tests::{call_at, home, ok_at};

    const C: &str = "0123456789abcdef0123456789abcdef01234567";
    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn begin(how: &str, by: &str) -> Vec<String> {
        w(&[
            "--plugin",
            "floorplan",
            "--commit",
            C,
            "--kind",
            "refresh",
            "--how",
            how,
            "--by",
            by,
            "--head",
            H,
        ])
    }

    #[test]
    fn the_verbs_read_their_words_and_refuse_what_no_row_may_hold() {
        assert_eq!(
            parse("plugin-run-begin", &begin("timer", "timer")),
            Some(Ok(Call::Begin(Begin {
                plugin: "floorplan".into(),
                commit: C.into(),
                kind: "refresh",
                how: "timer",
                by: "timer".into(),
                head: H.into(),
            })))
        );
        assert!(matches!(
            parse("plugin-run-begin", &begin("tick", "timer")),
            Some(Err(_))
        ));
        let mut bad = begin("timer", "timer");
        bad[1] = "Floor-plan".into();
        assert!(matches!(parse("plugin-run-begin", &bad), Some(Err(_))));
        let mut short = begin("timer", "timer");
        short[3] = "0123abc".into();
        assert!(matches!(parse("plugin-run-begin", &short), Some(Err(_))));
        assert_eq!(
            parse("plugin-run-end", &w(&["--run", "3", "--result", "done"])),
            Some(Ok(Call::End {
                run: 3,
                result: "done",
                tokens: 0,
                size: None
            }))
        );
        for bad in [
            &["--run", "0", "--result", "done"][..],
            &["--run", "1", "--result", "ok"],
            &["--run", "1", "--result", "done", "--tokens", "-1"],
            &["--run", "1", "--result", "done", "--size", "x"],
            &["--result", "done"],
        ] {
            assert!(
                matches!(parse("plugin-run-end", &w(bad)), Some(Err(_))),
                "{bad:?}"
            );
        }
        assert_eq!(
            parse("plugin-runs", &w(&["--plugin", "floorplan"])),
            Some(Ok(Call::Runs {
                plugin: "floorplan".into(),
                limit: LIMIT,
                no_calls: false
            }))
        );
        assert!(matches!(
            parse(
                "plugin-runs",
                &w(&["--plugin", "floorplan", "--limit", "51"])
            ),
            Some(Err(_))
        ));
        assert_eq!(parse("plugin-run", &[]), None);
    }

    fn args(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_run_is_a_row_from_its_start_to_its_end_and_ends_once() {
        let home = home("plugin-runs");
        let mut first = vec!["plugin-run-begin".to_string()];
        first.extend(begin("timer", "timer"));
        let a = ok_at(&home, 100, &args(&first))["run"].as_i64().unwrap();
        let mut second = vec!["plugin-run-begin".to_string()];
        second.extend(begin("action", "octocat"));
        let b = ok_at(&home, 200, &args(&second))["run"].as_i64().unwrap();
        assert_ne!(a, b);

        let ended = ok_at(
            &home,
            160,
            &[
                "plugin-run-end",
                "--run",
                &a.to_string(),
                "--result",
                "done",
                "--tokens",
                "1234",
            ],
        );
        assert_eq!(ended["run"]["end"], 160);
        assert_eq!(ended["run"]["tokens"], 1234);
        let again = call_at(
            &home,
            170,
            &[
                "plugin-run-end",
                "--run",
                &a.to_string(),
                "--result",
                "failed",
            ],
        );
        assert_eq!(again.unwrap_err().0, 1);
        assert_eq!(
            call_at(
                &home,
                170,
                &["plugin-run-end", "--run", "99", "--result", "done"]
            )
            .unwrap_err()
            .0,
            1
        );

        let runs = ok_at(&home, 300, &["plugin-runs", "--plugin", "floorplan"]);
        let runs = runs["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(
            runs[0],
            json!({
                "id": b, "plugin": "floorplan", "commit": C, "kind": "refresh",
                "trigger": "action", "by": "octocat", "head": H, "start": 200,
                "end": null, "result": null, "tokens": 0, "size": null
            })
        );
        assert_eq!(runs[1]["result"], "done");
        let none = ok_at(&home, 300, &["plugin-runs", "--plugin", "other"]);
        assert_eq!(none["runs"], json!([]));
    }

    fn call(how: &str, by: &str) -> Vec<String> {
        let mut b = begin(how, by);
        b[5] = "call".into();
        b
    }

    #[test]
    fn a_call_keeps_no_login_and_its_answers_size() {
        for (how, by) in [("chat", "chat"), ("mcp", "0a1b2c3d"), ("host", "host")] {
            assert!(
                matches!(parse("plugin-run-begin", &call(how, by)), Some(Ok(_))),
                "{how} {by}"
            );
        }
        for (how, by) in [
            ("chat", "octocat"),
            ("mcp", "octocat"),
            ("mcp", "0A1B2C3D"),
            ("host", "timer"),
            ("timer", "timer"),
            ("action", "octocat"),
        ] {
            assert!(
                matches!(parse("plugin-run-begin", &call(how, by)), Some(Err(_))),
                "{how} {by}"
            );
        }
        for how in ["chat", "mcp"] {
            assert!(matches!(
                parse("plugin-run-begin", &begin(how, "chat")),
                Some(Err(_))
            ));
        }

        let home = home("plugin-calls");
        let mut first = vec!["plugin-run-begin".to_string()];
        first.extend(call("mcp", "0a1b2c3d"));
        let a = ok_at(&home, 100, &args(&first))["run"].as_i64().unwrap();
        let ended = ok_at(
            &home,
            101,
            &[
                "plugin-run-end",
                "--run",
                &a.to_string(),
                "--result",
                "done",
                "--size",
                "17",
            ],
        );
        assert_eq!(
            ended["run"],
            json!({
                "id": a, "plugin": "floorplan", "commit": C, "kind": "call",
                "trigger": "mcp", "by": "0a1b2c3d", "head": H, "start": 100,
                "end": 101, "result": "done", "tokens": 0, "size": 17
            })
        );
    }

    #[test]
    fn the_runs_without_calls_find_a_refresh_past_any_number_of_calls() {
        let home = home("plugin-runs-calls");
        let mut first = vec!["plugin-run-begin".to_string()];
        first.extend(begin("timer", "timer"));
        let refresh = ok_at(&home, 100, &args(&first))["run"].clone();
        let mut one = vec!["plugin-run-begin".to_string()];
        one.extend(call("chat", "chat"));
        for t in 0..LIMIT + 5 {
            ok_at(&home, 200 + t, &args(&one));
        }
        let all = ok_at(&home, 400, &["plugin-runs", "--plugin", "floorplan"]);
        let all = all["runs"].as_array().unwrap();
        assert_eq!(all.len() as i64, LIMIT);
        assert!(all.iter().all(|r| r["kind"] == "call"));
        let no = ok_at(
            &home,
            400,
            &["plugin-runs", "--plugin", "floorplan", "--no-calls"],
        );
        assert_eq!(no["runs"].as_array().unwrap().len(), 1);
        assert_eq!(no["runs"][0]["id"], refresh);
    }
}
