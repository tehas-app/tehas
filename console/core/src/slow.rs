//! The driver's slow notices of one cycle, as the record keeps them.
//!
//! pi-rukas writes a `dispatch-slow` line on a cycle's stream when one of its
//! children crosses the bound it watches, and, once it recognises a repeated
//! tool call, the command that repeated, how many times it ran and how many
//! identical results came back. That line is the only place a child going over
//! the same ground is told apart from one doing slow work, and the console it
//! also appears on is written inside the container and swept with the
//! workspace. So the cycle's record keeps these lines.
//!
//! The line is selected by `kind` alone, and kept whole: tehas reads pi-rukas
//! only through the contract it publishes, so a runner that marks the line
//! stable or unstable, or adds a field, needs no change here. Of what is left
//! the last `LIMIT` lines are kept and the file under `BOUND` bytes, the bounds
//! the driver's diagnostics copied into err.txt keep. A line is kept whole or
//! left out, never cut, and the newest notice is the one a stuck cycle is read
//! by, so when the bound bites it is the oldest of them that goes.

use serde_json::Value;

/// The most slow lines one record holds.
pub const LIMIT: usize = 20;

/// The most bytes one record holds of them, so a stream line of any size
/// cannot make a record of the size of the stream.
pub const BOUND: usize = 65_536;

/// The kind of a driver event, which a progress line names `kind` and an older
/// line names `type`. Only a line that parses as an object with one of those
/// two keys holding the string is a slow line: a line that mentions the word as
/// some other value, or is no JSON at all, is not.
fn slow_line(line: &str) -> bool {
    match serde_json::from_str::<Value>(line) {
        Ok(v) => v
            .get("kind")
            .or_else(|| v.get("type"))
            .and_then(Value::as_str)
            == Some("dispatch-slow"),
        Err(_) => false,
    }
}

/// The `dispatch-slow` lines of a stream, oldest first: at most `LIMIT` of
/// them, in at most `BOUND` bytes, each one complete. Consecutive copies of one
/// line count as one, as they do in the record's stream.
pub fn lines(stream: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut previous: Option<&str> = None;
    for line in stream.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || Some(line) == previous {
            continue;
        }
        previous = Some(line);
        if slow_line(line) {
            kept.push(line);
            if kept.len() > LIMIT {
                kept.remove(0);
            }
        }
    }
    // From the newest back, so the bound leaves the notices a stuck cycle is
    // read by rather than the ones it had no use for.
    let mut bytes = 0;
    let mut start = kept.len();
    for i in (0..kept.len()).rev() {
        let need = kept[i].len() + 1;
        if bytes + need > BOUND {
            break;
        }
        bytes += need;
        start = i;
    }
    let mut out = String::with_capacity(bytes);
    for line in &kept[start..] {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slow(step: &str, rest: &str) -> String {
        format!("{{\"v\":1,\"kind\":\"dispatch-slow\",\"step\":\"{step}\"{rest}}}")
    }

    fn kept(stream: &str) -> Vec<String> {
        lines(stream).lines().map(str::to_owned).collect()
    }

    #[test]
    fn nothing_is_no_lines() {
        assert_eq!(lines(""), "");
        assert_eq!(lines("\n\n   \n"), "");
    }

    #[test]
    fn a_line_of_another_kind_is_not_kept() {
        let stream = [
            "{\"type\":\"message_update\",\"delta\":\"x\"}",
            "{\"kind\":\"dispatch-completed\",\"command\":\"the probe\",\"count\":24}",
            "{\"kind\":\"slow-notice\"}",
            "not json at all",
            "dispatch-slow in a line that is no JSON",
            "{\"v\":1,\"kind\":\"cap-hit\",\"cap\":\"deadline\"}",
            // The word as some other value, and as a key of its own: neither is
            // the kind of the line.
            "{\"kind\":\"note\",\"says\":\"dispatch-slow\"}",
            "{\"dispatch-slow\":true}",
            "[\"dispatch-slow\"]",
        ]
        .join("\n");
        assert_eq!(lines(&stream), "");
    }

    #[test]
    fn the_slow_lines_are_kept_in_the_order_they_came() {
        let a = slow("lens-review", "");
        let b = slow("develop", "");
        assert_eq!(kept(&[a.clone(), b.clone()].join("\n")), vec![a, b]);
    }

    #[test]
    fn the_kind_may_be_named_type_and_the_line_may_be_marked_unstable() {
        let typed = "{\"v\":1,\"type\":\"dispatch-slow\",\"role\":\"developer\"}";
        assert_eq!(lines(typed), format!("{typed}\n"));
        let marked = "{\"v\":1,\"unstable\":true,\"kind\":\"dispatch-slow\",\"turns\":300}";
        assert_eq!(lines(marked), format!("{marked}\n"));
    }

    #[test]
    fn the_repeat_fields_come_through_whole() {
        let line = slow(
            "lens-review",
            ",\"role\":\"code-review-specialist-p\",\"command\":\"bash -c probe\",\"count\":24,\"identicalResultCount\":24",
        );
        // The whole line is what is kept: the fields of a runner's line are not
        // tehas's to name one by one, so a runner that adds one needs no change
        // here.
        assert_eq!(lines(&line), format!("{line}\n"));
        assert!(lines(&line).contains("\"identicalResultCount\":24"));
    }

    #[test]
    fn a_repeated_line_counts_once() {
        let line = slow("lens-review", "");
        assert_eq!(
            lines(&[line.clone(), line.clone(), line.clone()].join("\n")),
            format!("{line}\n")
        );
    }

    #[test]
    fn only_the_last_limit_lines_are_kept() {
        let stream: Vec<String> = (1..=30).map(|i| slow(&format!("s{i}"), "")).collect();
        let kept = kept(&stream.join("\n"));
        assert_eq!(kept.len(), LIMIT);
        assert!(kept[0].contains("s11"));
        assert!(kept[LIMIT - 1].contains("s30"));
    }

    #[test]
    fn the_file_stays_under_the_bound_and_no_line_is_cut() {
        let big = "x".repeat(4_000);
        let stream: Vec<String> = (1..=30)
            .map(|i| slow(&format!("s{i}"), &format!(",\"tail\":\"{big}\"")))
            .collect();
        let out = lines(&stream.join("\n"));
        assert!(out.len() < BOUND, "{} bytes", out.len());
        let kept = kept(&out);
        assert!(!kept.is_empty());
        assert!(kept.len() < LIMIT, "the bound should bite first: {}", kept.len());
        assert!(kept.iter().all(|l| l.ends_with('}')));
        assert!(kept.last().unwrap().contains("s30"));
    }

    #[test]
    fn a_line_over_the_bound_leaves_the_others_of_their_size_alone() {
        let huge = slow(
            "lens-review",
            &format!(",\"tail\":\"{}\"", "y".repeat(BOUND)),
        );
        let small = slow("handoff", "");
        assert_eq!(lines(&[huge, small.clone()].join("\n")), format!("{small}\n"));
    }

    #[test]
    fn a_line_of_the_bound_exactly_is_kept_and_one_longer_is_not() {
        // The newline is counted with the line, so a line of BOUND-1 bytes
        // fits and one of BOUND bytes does not.
        let fits = format!(
            "{{\"kind\":\"dispatch-slow\",\"p\":\"{}\"}}",
            "z".repeat(BOUND - 32)
        );
        assert_eq!(fits.len(), BOUND - 1);
        assert_eq!(lines(&fits), format!("{fits}\n"));
        assert_eq!(lines(&format!("{fits}x")), "");
    }
}
