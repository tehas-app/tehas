//! The shadow's comparison: what the Rust decision said a poll would log,
//! beside what the bash poll logged. One JSON line of logs/dispatch-shadow.log.

use serde_json::{json, Value};

/// A line of the poll without the time log() puts in front.
fn bare(line: &str) -> &str {
    let b = line.as_bytes();
    let stamped = b.len() > 21
        && b[4] == b'-'
        && b[10] == b'T'
        && b[19] == b'Z'
        && b[20] == b' '
        && b[..4].iter().all(u8::is_ascii_digit);
    if stamped {
        &line[21..]
    } else {
        line
    }
}

/// The lines a decision says the poll logs: its own, then the line each
/// action logs once it has worked, in order.
pub fn expected(decision: &Value) -> Vec<String> {
    let mut out: Vec<String> = decision
        .get("log")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    out.extend(
        decision
            .get("actions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|a| a.get("log").and_then(Value::as_str))
            .map(str::to_string),
    );
    out
}

/// The record of one poll: whether the two agree, the lines only one of
/// them has (each line matched once), and what the shadow counted.
pub fn compare(decision: &Value, poll: &str, poll_exit: Option<i64>, time: &str) -> Value {
    let mut rust = expected(decision);
    let mut bash_only = Vec::new();
    for line in poll.lines().map(bare).filter(|l| !l.is_empty()) {
        match rust.iter().position(|r| r == line) {
            Some(i) => {
                rust.remove(i);
            }
            None => bash_only.push(line.to_string()),
        }
    }
    let exit = decision.get("exit").and_then(Value::as_i64);
    let exit_same = poll_exit.is_none() || poll_exit == exit;
    json!({
        "time": time,
        "same": bash_only.is_empty() && rust.is_empty() && exit_same,
        "bash_only": bash_only,
        "rust_only": rust,
        "exit": {"bash": poll_exit, "rust": exit},
        "needs": decision.get("needs").cloned().unwrap_or(json!([])),
        "picked": decision.get("picked").cloned().unwrap_or(json!([])),
        "gh_requests": decision.get("gh_requests").cloned().unwrap_or(Value::Null),
        "seconds": decision.get("seconds").cloned().unwrap_or(Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_match_once_each_whatever_their_time() {
        let d = json!({"exit": 0, "log": ["holding: 1 cycles are running (limit 1)"],
            "actions": [{"issue": 9, "log": "clear #9: it is closed"}]});
        let poll = "2026-10-26T10:00:00Z holding: 1 cycles are running (limit 1)\n2026-10-26T10:00:01Z clear #9: it is closed\n";
        let r = compare(&d, poll, Some(0), "t");
        assert_eq!(r["same"], json!(true));
        let r = compare(&d, "2026-10-26T10:00:00Z start #7 (labelled by alice)\n", Some(0), "t");
        assert_eq!(r["same"], json!(false));
        assert_eq!(r["bash_only"], json!(["start #7 (labelled by alice)"]));
        assert_eq!(r["rust_only"], json!(["holding: 1 cycles are running (limit 1)", "clear #9: it is closed"]));
        assert_eq!(compare(&d, poll, Some(1), "t")["same"], json!(false));
    }
}
