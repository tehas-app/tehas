//! What a snapshot asks of the state store, through its read verbs, and the
//! shapes lib/store.sh gives the answers before the poll reads them.

use super::run::Calls;
use crate::decide::iso;
use serde_json::{json, Value};

fn get(v: &Value, k: &str) -> Value {
    v.get(k).cloned().unwrap_or(Value::Null)
}

/// plan-show --open: the open sessions, or the store's failure.
pub fn sessions(calls: &mut Calls) -> Value {
    match calls.store(&["plan-show", "--open"]) {
        Ok(v) => Value::Array(
            v.get("sessions")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter(|s| s.get("issue").is_some_and(Value::is_number))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Err(e) => json!({"word": e["word"], "rc": e["rc"]}),
    }
}

/// cycle-show --running: each running cycle's record as running_starts
/// reads it, Err with the store's failure.
pub fn cycles(calls: &mut Calls) -> Result<Vec<Value>, Value> {
    let v = calls.store(&["cycle-show", "--running"])?;
    Ok(v.get("cycles")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let n = r.get("issue")?.as_u64()?;
                    let c = get(r, "cycle");
                    let num = |x: &Value, k: &str| x.get(k).and_then(Value::as_u64).unwrap_or(0);
                    Some(if c.is_object() && c.get("ended_at").is_none_or(Value::is_null) {
                        json!({"issue": n, "how": c.get("how").and_then(Value::as_str).unwrap_or(""),
                            "took": num(&c, "started_at"), "heard": num(&c, "heard_at"),
                            "said_for": c.pointer("/quiet/said_for").map(|s| match s { Value::String(s) => s.clone(), v => v.to_string() }).unwrap_or("-".into()),
                            "quiet_at": c.pointer("/quiet/at").and_then(Value::as_u64).unwrap_or(0),
                            "asked": num(&c, "stop_asked_at"), "times": num(&c, "stop_said_times")})
                    } else {
                        json!({"issue": n, "how": "", "took": 0, "heard": if c.is_object() { num(&c, "heard_at") } else { 0 }})
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// park-show <n>'s park, or the failure.
pub fn park(calls: &mut Calls, n: u64) -> Value {
    match calls.store(&["park-show", &n.to_string()]) {
        Ok(v) => get(&v, "park"),
        Err(e) => e,
    }
}

/// The parks owed a retry (--automatic) or a refine (--refine).
pub fn owed(calls: &mut Calls, which: &str) -> Value {
    match calls.store(&["park-show", which]) {
        Ok(v) => v.get("parks").cloned().unwrap_or_else(|| json!([])),
        Err(e) => e,
    }
}

/// tehas_approval_shape: the approval of the issue's latest start as the
/// readers of tehas_approval_view have it. Null for none.
fn approval_shape(v: &Value) -> Option<Value> {
    let c = v.get("current")?;
    if c.is_null() {
        return Some(Value::Null);
    }
    if !c.is_object() {
        return None;
    }
    let person = |p: &Value| -> Option<Value> {
        if p.is_null() {
            return Some(Value::Null);
        }
        Some(
            json!({"at": iso(p.get("at")?.as_u64()?), "by": get(p, "by"), "how": get(p, "how"),
            "body_sha256": get(p, "body_sha256")}),
        )
    };
    let starts = v
        .get("millwright_starts")
        .filter(|x| !x.is_null())
        .or_else(|| v.get("fixer_starts").filter(|x| !x.is_null()))
        .cloned()
        .unwrap_or(json!(0));
    let input = c.get("input").cloned().unwrap_or(json!({}));
    let base = person(&get(v, "rests_on"))?;
    let how = match c.get("how").and_then(Value::as_str) {
        Some("millwright") => "fixer".to_string(),
        Some(h) => h.to_string(),
        None => String::new(),
    };
    let mut out = json!({"at": iso(c.get("at")?.as_u64()?), "by": get(c, "by"), "how": how,
        "body_sha256": get(c, "body_sha256"), "links": input.get("links").cloned().unwrap_or(json!([]))});
    if let Some(t) = c.get("title_sha256").filter(|t| !t.is_null()) {
        out["title_sha256"] = t.clone();
    }
    if let Some(l) = input.get("comment_links") {
        out["comment_links"] = l.clone();
    }
    if input
        .get("retry_context")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty())
    {
        out["retry_context"] = input["retry_context"].clone();
    }
    if how == "fixer" {
        out["rests_on"] = base.clone();
        out["fixer_starts"] = starts.clone();
    }
    if how == "refresh" {
        out["rests_on"] = base.clone();
        out["fixer_starts"] = starts;
        out["refreshes"] = get(v, "refreshes");
        out["refresh_of"] = get(&input, "refresh_of");
        if !base.is_null() {
            out["at"] = base["at"].clone();
            out["by"] = base["by"].clone();
        }
    }
    Some(out)
}

/// approval-current <n>: the shaped approval (null for none), its raw
/// current (for a refresh's owner), or the failure for both.
pub fn approval(calls: &mut Calls, n: u64) -> (Value, Value) {
    match calls.store(&["approval-current", &n.to_string()]) {
        Ok(v) => (
            approval_shape(&v).unwrap_or_else(|| json!({"failed": true})),
            get(&v, "current"),
        ),
        Err(e) => (json!({"failed": true}), e),
    }
}

/// history-show <n> --kind cycle, as tehas_cycle_history gives its lines.
pub fn history(calls: &mut Calls, n: u64) -> Value {
    match calls.store(&["history-show", &n.to_string(), "--kind", "cycle"]) {
        Ok(v) => Value::Array(
            v.get("rows")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("cycle"))
                        .map(|r| json!({"how": get(r, "how"), "parked": get(r, "parked"), "started": get(r, "started_at")}))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Err(_) => json!({"failed": true}),
    }
}

/// The issues with a refresh in their history.
pub fn refreshed(calls: &mut Calls) -> Value {
    match calls.store(&["history-show", "--kind", "cycle", "--how", "refresh"]) {
        Ok(v) => {
            let mut n: Vec<u64> = v
                .get("rows")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|r| r.get("issue").and_then(Value::as_u64))
                        .collect()
                })
                .unwrap_or_default();
            n.sort_unstable();
            n.dedup();
            json!(n)
        }
        Err(e) => e,
    }
}

/// The question on record of an asking issue: {plan, id, kind}, as the bash
/// reads it from plan-show, park-show and question-known; the failure of the
/// last of them that failed.
pub fn question(calls: &mut Calls, n: u64) -> Value {
    let num = n.to_string();
    let plan = match calls.store(&["plan-show", &num]) {
        Ok(v) => match v.get("planning") {
            Some(p) if p.is_object() && p.get("ended_at").is_none_or(Value::is_null) => p.clone(),
            _ => Value::Null,
        },
        Err(e) => return e,
    };
    let digits = |v: Option<&Value>| match v {
        Some(Value::Number(x)) => x.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    let ok = |s: &str| {
        !s.is_empty()
            && s.len() <= 16
            && !s.starts_with('0')
            && s.bytes().all(|b| b.is_ascii_digit())
    };
    let mut qid = digits(plan.get("comment_id"));
    if !ok(&qid) {
        qid.clear();
        match calls.store(&["park-show", &num]) {
            Ok(v) => {
                qid = digits(v.pointer("/park/question"));
                if !ok(&qid) {
                    qid.clear();
                }
            }
            Err(e) => return e,
        }
    }
    let mut kind = String::new();
    if !qid.is_empty() {
        match calls.store(&["question-known", &num, "--id", &qid]) {
            Ok(v) if v.get("known") == Some(&json!(true)) => {
                kind = v
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
            Ok(_) => qid.clear(),
            Err(e) => return e,
        }
    }
    json!({"plan": plan, "id": qid, "kind": kind})
}
