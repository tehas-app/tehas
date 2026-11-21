//! The runs of plugins, rows of `plugin_runs`
//! (docs/specs/2026-11-21-plugins.md, "Runs"):
//!
//!   plugin-run-begin --plugin <name> --commit <sha> --kind refresh|hook
//!                    --how timer|host|action|cycle --by <who> --head <sha>
//!   plugin-run-end --run N --result done|failed|timeout|lost [--tokens N]
//!   plugin-runs --plugin <name> [--limit N]
//!
//! `begin` answers the run's id, `end` the row it ended, and `plugin-runs`
//! the plugin's last runs, newest first. A run ends once.

use crate::verbs::{self, line, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
use tehas_core::grammar::commit_id;
use tehas_core::plugin::is_name;

const KINDS: [&str; 2] = ["refresh", "hook"];
const HOWS: [&str; 4] = ["timer", "host", "action", "cycle"];
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
    },
    Runs {
        plugin: String,
        limit: i64,
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
            Ok(Call::Begin(Begin {
                plugin: plugin(&a)?,
                commit: commit(&a, "--commit")?,
                kind: one_of(&a, "--kind", &KINDS)?,
                how: one_of(&a, "--how", &HOWS)?,
                by: by(&a)?,
                head: commit(&a, "--head")?,
            }))
        }),
        "plugin-run-end" => Args::parse(
            "plugin-run-end",
            rest,
            &[
                ("--run", Kind::Value),
                ("--result", Kind::Value),
                ("--tokens", Kind::Value),
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
            Ok(Call::End {
                run,
                result: one_of(&a, "--result", &RESULTS)?,
                tokens,
            })
        }),
        "plugin-runs" => Args::parse(
            "plugin-runs",
            rest,
            &[("--plugin", Kind::Value), ("--limit", Kind::Value)],
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
        } => {
            let row = verbs::write(home, |c| {
                let n = c
                    .execute(
                        "UPDATE plugin_runs SET ended_at = ?2, result = ?3, tokens = ?4
                         WHERE id = ?1 AND ended_at IS NULL",
                        params![run, now, result, tokens],
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
        Call::Runs { plugin, limit } => {
            let rows = verbs::look(home, |c| runs(c, plugin, *limit))?;
            Ok(line(
                "plugin-runs",
                &[("runs", Value::Array(rows).to_string())],
            ))
        }
    }
}

const COLUMNS: &str =
    "id, plugin, plugin_commit, kind, how, started_by, head, started_at, ended_at, result, tokens";

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

/// The last runs of `plugin`, newest first.
pub fn runs(c: &Connection, plugin: &str, limit: i64) -> Result<Vec<Value>, Fail> {
    let mut q = c
        .prepare(&format!(
            "SELECT {COLUMNS} FROM plugin_runs WHERE plugin = ?1 ORDER BY started_at DESC, id DESC LIMIT ?2"
        ))
        .q()?;
    let rows = q.query_map(params![plugin, limit], json_of).q()?;
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
                tokens: 0
            }))
        );
        for bad in [
            &["--run", "0", "--result", "done"][..],
            &["--run", "1", "--result", "ok"],
            &["--run", "1", "--result", "done", "--tokens", "-1"],
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
                limit: LIMIT
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
                "end": null, "result": null, "tokens": 0
            })
        );
        assert_eq!(runs[1]["result"], "done");
        let none = ok_at(&home, 300, &["plugin-runs", "--plugin", "other"]);
        assert_eq!(none["runs"], json!([]));
    }
}
