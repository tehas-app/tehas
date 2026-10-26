//! The bot's questions on an issue and the answers it takes, as
//! lib/questions.sh reads them (q_answer, q_untick).

use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

fn login(v: &Value, path: &str) -> String {
    v.pointer(path)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// q_answer: the newest question of the bot among the comments (the one
/// with id `id` when it is given), what was ticked in it and the replies
/// after it. None when the comments cannot be read.
pub fn answer(
    comments: &Value,
    bot: &str,
    allowed: &[String],
    id: &str,
    fixer: &str,
) -> Option<Value> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"<!-- tehas:question kind=(?<kind>[a-z]+) issue=(?<issue>[0-9]+) turn=(?<turn>[0-9]+) -->").unwrap()
    });
    let bot = bot.strip_suffix("[bot]").unwrap_or(bot);
    let c = match comments.pointer("/data/repository/issue/comments/nodes") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.clone(),
        Some(_) => return None,
    };
    let listed = |l: &str| allowed.iter().any(|a| a == l);
    let mut q = None;
    for (k, x) in c.iter().enumerate() {
        if login(x, "/author/login") != bot {
            continue;
        }
        // A bot comment with no text is an answer jq cannot read.
        let body = x.get("body").and_then(Value::as_str)?;
        let did = match x.get("databaseId") {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => "null".into(),
        };
        if re.is_match(body) && (id.is_empty() || did == id) {
            q = Some(k);
        }
    }
    let Some(k) = q else {
        return Some(json!({"asked": false}));
    };
    let qv = &c[k];
    let body = qv["body"].as_str().unwrap_or("");
    let m = re.captures(body)?;
    let boxes: Vec<bool> = body
        .split('\n')
        .filter(|l| l.starts_with("- [") && matches!(l.get(3..6), Some(" ] " | "x] " | "X] ")))
        .map(|l| matches!(l.get(3..4), Some("x" | "X")))
        .collect();
    let ticks: Vec<usize> = boxes
        .iter()
        .enumerate()
        .filter(|(_, t)| **t)
        .map(|(i, _)| i)
        .collect();
    let ed = login(qv, "/editor/login");
    let edited_at = qv.get("lastEditedAt").and_then(Value::as_str).unwrap_or("");
    let after = &c[k + 1..];
    let reply = after.iter().rfind(|x| {
        let a = login(x, "/author/login");
        let e = login(x, "/editor/login");
        listed(&a) && (e.is_empty() || listed(&e))
    });
    let tick = if ticks.is_empty() {
        json!({"status": "none"})
    } else if !fixer.is_empty() && ed == fixer && ticks.len() == 1 {
        let h = qv.get("userContentEdits").cloned().unwrap_or(json!({}));
        let nodes = h
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let total = h.get("totalCount").and_then(Value::as_f64).unwrap_or(0.0);
        if total > nodes.len() as f64
            || nodes.iter().any(|x| {
                let e = login(x, "/editor/login");
                e != fixer && e != bot
            })
        {
            json!({"status": "held", "by": ed})
        } else {
            json!({"status": "fixer", "by": ed, "index": ticks[0], "at": edited_at})
        }
    } else if !listed(&ed) && (fixer.is_empty() || ed != fixer) {
        json!({"status": "stranger", "by": ed})
    } else if ticks.len() > 1 {
        json!({"status": "many", "by": ed})
    } else {
        json!({"status": "one", "by": ed, "index": ticks[0], "at": edited_at})
    };
    // jq's tonumber: digits too many for an integer are a float, never an
    // error.
    let num = |d: &str| -> Value {
        d.parse::<u64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!(d.parse::<f64>().unwrap_or(f64::MAX)))
    };
    let issue = &m["issue"];
    let mut out = json!({
        "asked": true, "comment": qv.get("databaseId").cloned().unwrap_or(Value::Null),
        "asked_at": qv.get("createdAt").and_then(Value::as_str).unwrap_or(""),
        "body": body, "kind": &m["kind"], "issue": num(issue),
        "turn": num(&m["turn"]),
        "boxes": boxes.len(), "tick": tick,
        "reply": reply.map(|r| json!({"by": login(r, "/author/login"), "text": r.get("body"),
            "at": r.get("createdAt"), "id": r.get("databaseId")})),
    });
    if !fixer.is_empty() {
        let f = after.iter().rfind(|x| {
            let e = login(x, "/editor/login");
            login(x, "/author/login") == fixer && (e.is_empty() || e == fixer)
        });
        let mark = format!("<!-- tehas:millwright issue={issue} action=answer -->");
        out["fixer_reply"] = match f {
            None => Value::Null,
            Some(f) => {
                let b = f.get("body").and_then(Value::as_str).unwrap_or("");
                let first = b.split('\n').next().unwrap_or("");
                let first = first.strip_suffix('\r').unwrap_or(first);
                let marked = first == mark;
                let text = if marked {
                    b.split_once('\n').map_or("", |(_, rest)| rest)
                } else {
                    b
                };
                if marked && text.trim().is_empty() {
                    Value::Null
                } else {
                    json!({"by": login(f, "/author/login"), "text": text, "at": f.get("createdAt"),
                        "id": f.get("databaseId"), "marked": marked})
                }
            }
        };
    }
    Some(out)
}

/// q_untick: a body with every box empty.
/// The body is read as `$(jq -r .body)` reads it: with no final newlines.
pub fn untick(body: &str) -> String {
    body.trim_end_matches('\n')
        .split('\n')
        .map(|l| match l.get(..6) {
            Some("- [x] " | "- [X] ") => format!("- [ ] {}", &l[6..]),
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(list: Value) -> Value {
        json!({"data": {"repository": {"issue": {"comments": {"nodes": list}}}}})
    }

    #[test]
    fn a_tick_of_an_allowed_account_is_one_answer() {
        let q = "<!-- tehas:question kind=park issue=7 turn=1 -->\nStuck.\n\n- [x] Retry from a clean start\n- [ ] Leave it";
        let c = nodes(json!([
            {"databaseId": 4001, "author": {"login": "tehas-bot"}, "body": q, "createdAt": "2026-10-01T00:00:00Z",
             "lastEditedAt": "2026-10-01T01:00:00Z", "editor": {"login": "alice"}},
        ]));
        let a = answer(&c, "tehas-bot", &["alice".into()], "", "").unwrap();
        assert_eq!(a["kind"], "park");
        assert_eq!(a["boxes"], 2);
        assert_eq!(
            a["tick"],
            json!({"status": "one", "by": "alice", "index": 0, "at": "2026-10-01T01:00:00Z"})
        );
        let b = answer(&c, "tehas-bot", &["bob".into()], "", "").unwrap();
        assert_eq!(b["tick"]["status"], "stranger");
        assert_eq!(
            answer(&c, "tehas-bot", &[], "99", "").unwrap(),
            json!({"asked": false})
        );
    }

    #[test]
    fn untick_empties_every_box() {
        assert_eq!(
            untick("a\n- [x] One\n- [X] Two\n- [ ] Three"),
            "a\n- [ ] One\n- [ ] Two\n- [ ] Three"
        );
    }
}
