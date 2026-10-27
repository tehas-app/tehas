//! The JSON Schema of `factory_outcomes`, and the keys its answer copies.
//!
//! Kept apart from the tool that reads the counts file: the shape of the
//! answer is one thing and reading the file is another. The analytics program
//! writes the file against the same names, so `spec` and that writer are the
//! two ends of one contract.

use crate::outcomes::BOUND;
use crate::outcomes::BYS;
use crate::tools::WORD;
use serde_json::{json, Map, Value};

fn count(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

fn text(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}

fn counts(desc: &str) -> Value {
    json!({"type": "object", "description": desc,
           "additionalProperties": {"type": "integer"}})
}

/// What every entry holds, whatever its grouping: the count of its runs, and
/// the three maps that name a reason. One list, so the schema a client reads
/// and the keys a call copies cannot part.
const ENTRY: [(&str, &str); 4] = [
    ("runs", "Runs in this group."),
    ("outcomes", "Runs per outcome: pr, merged, handoff, parked, failed, refused and stopped, and unknown for a run that recorded none."),
    ("failureKinds", "Runs per failure kind. A run that failed with no kind recorded is counted in runs and outcomes, not here."),
    ("parkReasons", "Runs per park reason. A run parked with no reason recorded is counted in runs and outcomes, not here."),
];

/// The schema's description of one entry's keys, from `ENTRY`.
fn count_entry_key(key: &str, desc: &str) -> Value {
    if key == "runs" {
        count(desc)
    } else {
        counts(desc)
    }
}

/// A count of the file's own words: a map of keys no spec can name in advance,
/// served only when every value of it is a whole count, as a map holding a
/// value of another shape is no count of anything.
fn count_map() -> Value {
    json!({})
}

fn output(props: Vec<(&str, Value)>) -> Value {
    let props: Map<String, Value> = props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    json!({"type": "object", "properties": props, "required": ["api"]})
}

fn entry_schema(named: Vec<(&str, Value)>) -> Value {
    let mut props: Map<String, Value> = named.into_iter().map(|(k, v)| (k.into(), v)).collect();
    for (key, desc) in ENTRY {
        props.insert(key.into(), count_entry_key(key, desc));
    }
    json!({"type": "object", "properties": props})
}

/// The tool, for `tools::list`.
pub fn tool() -> Value {
    let word =
        |desc: &str| json!({"type": "string", "pattern": WORD.as_str(), "description": desc});
    let when =
        |desc: &str| json!({"type": "string", "pattern": BOUND.as_str(), "description": desc});
    let properties = Value::Object(
        vec![
            (
                "tehasVersion",
                word("Only runs that ran this tehas version, matched exactly."),
            ),
            (
                "rukasVersion",
                word("Only runs that ran this pi-rukas version or commit, matched exactly."),
            ),
            (
                "since",
                when(
                    "The first day counted, YYYY-MM-DD, or a full UTC time. The whole retention without it.",
                ),
            ),
            (
                "until",
                when(
                    "The last day counted, YYYY-MM-DD, or a full UTC time. The whole day is counted. Now without it.",
                ),
            ),
            (
                "by",
                json!({
                    "type": "string",
                    "enum": BYS,
                    "description": "How to group the counts: per version pair, or per UTC week. version without it.",
                }),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<Map<String, Value>>(),
    );
    json!({
        "name": "factory_outcomes",
        "description": "How the factory has done further back than 30 days: the runs per tehas and pi-rukas version pair, or per UTC week, as far back as the factory's analytics holds. Counts only, so it answers whether a release changed the park or failure rates and how often a failure kind comes back, and says nothing of any issue. Arguments all optional: tehasVersion, rukasVersion, since and until (a day as YYYY-MM-DD, or a full UTC time), and by (version or week).",
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "additionalProperties": false,
        },
        "outputSchema": output(vec![
            ("api", count("The API version, 1.")),
            (
                "by",
                json!({"type": "string", "enum": BYS, "description": "How the entries are grouped, which is how the host writes its file."}),
            ),
            (
                "days",
                count("How far back the file reaches: the retention of the factory's analytics."),
            ),
            ("generatedAt", text("When the host wrote the file, UTC.")),
            (
                "stale",
                json!({"type": "boolean", "description": "True when the host has not written the file for a day, so it holds none of that day's runs."}),
            ),
            (
                "entries",
                json!({"type": "array", "description": "One entry per version pair, the most runs first, or per week, newest first.", "items": entry_schema(vec![
                    ("tehasVersion", text("The tehas version. Left out when the run recorded none.")),
                    ("rukasVersion", text("The pi-rukas version, or its commit. Left out when the run recorded neither.")),
                ])}),
            ),
        ]),
    })
}

/// The keys of one entry the tool serves, by the grouping the file gives: the
/// keys of `ENTRY`, and those of the grouping that names it. The answer is
/// copied with these, so the schema test compares the schema with what a call
/// is actually built from.
pub fn spec(by: &str) -> Value {
    let mut s = Map::new();
    match by {
        "week" => {
            s.insert("week".into(), json!(true));
        }
        _ => {
            s.insert("tehasVersion".into(), json!(true));
            s.insert("rukasVersion".into(), json!(true));
        }
    }
    for (key, _) in ENTRY {
        // `runs` is a scalar the spec names; a map of counts holds the file's
        // own words, so its spec names no keys of its own: `tools::pick`
        // copies it whole when every value of it is a count, and drops it
        // when the file were to hold a value of another shape there.
        s.insert(
            key.into(),
            if key == "runs" {
                json!(true)
            } else {
                count_map()
            },
        );
    }
    Value::Object(s)
}
