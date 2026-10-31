//! From one line of a cycle's logs to the entries of the console.
//!
//! Two kinds of line are read: the progress the headless runner writes as
//! NDJSON, for driver notices, step boundaries, agents and caps, and an
//! agent's transcript, for what the agent said, the commands it ran and the
//! start of what came back. A line that is not JSON, or not one of these,
//! gives no entry. Of the progress only the kinds the runner declares stable
//! are read; a line it marks `unstable` gives no entry.

use serde_json::Value;
use tehas_core::mask::{is_format, mask};

/// One entry of the console: who speaks, what they said and, when the line
/// carried a time, the second of the UTC day it was said.
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub who: String,
    pub text: String,
    pub at: Option<u32>,
}

impl Entry {
    pub fn new(who: &str, text: impl Into<String>, at: Option<u32>) -> Self {
        Entry {
            who: who.to_string(),
            text: text.into(),
            at,
        }
    }
}

/// What is written of a text: masked, and of the control characters only
/// the line break and the tab. The file is also read in terminals, so no
/// escape sequence gets through, and in a browser, where a direction
/// override or a line separator would make the line read as another.
fn shown(s: &str) -> String {
    mask(s)
        .chars()
        .filter(|c| *c != '\r')
        .map(|c| match c {
            '\n' | '\t' => c,
            '\u{2028}' | '\u{2029}' => char::REPLACEMENT_CHARACTER,
            c if c.is_control() || is_format(c) => char::REPLACEMENT_CHARACTER,
            c => c,
        })
        .collect()
}

/// The entry as it is written: the time and the speaker on the first line,
/// further lines indented under the text.
pub fn format(entry: &Entry, now: u32) -> String {
    let t = entry.at.unwrap_or(now);
    let shown_text = shown(&entry.text);
    let mut lines = shown_text.split('\n');
    let first = lines.next().unwrap_or_default();
    let (h, m, s) = (t / 3600, t / 60 % 60, t % 60);
    let who = shown(&entry.who);
    let mut out = format!("{h:02}:{m:02}:{s:02} {who:<10} {first}\n");
    for line in lines {
        out.push_str(&format!("{:20}{line}\n", ""));
    }
    out
}

/// The first `lines` lines and `chars` characters of a text, marked when
/// something was left out.
fn clip(s: &str, chars: usize, lines: usize) -> String {
    let head = s.split('\n').take(lines).collect::<Vec<_>>().join("\n");
    let end = head
        .char_indices()
        .nth(chars)
        .map_or(head.len(), |(i, _)| i);
    let kept = &head[..end];
    if kept.len() < s.len() {
        format!("{}\n[...]", kept.trim_end())
    } else {
        kept.to_string()
    }
}

// The start of a text as an entry holds it. Masked before it is cut: a
// secret cut in two is too short for the mask to know, and its first half
// would be written.
fn start(s: &str, chars: usize, lines: usize) -> String {
    clip(&mask(s), chars, lines)
}

/// The second of the UTC day in a timestamp, in the two forms the logs hold:
/// milliseconds since the epoch, and a date and time that ends in `Z`.
/// Anything else is no time, and the entry is stamped with the time it was
/// read. Milliseconds count only between 2020 and 2100.
fn clock(at: &Value) -> Option<u32> {
    if let Some(ms) = at.as_f64() {
        let plausible = (1_577_836_800_000.0..4_102_444_800_000.0).contains(&ms);
        return plausible.then(|| ((ms / 1000.0).floor() % 86400.0) as u32);
    }
    let time = at.as_str()?.strip_suffix('Z')?.get(11..19)?;
    let mut parts = time.split(':').map(|part| match part.as_bytes() {
        [a @ b'0'..=b'9', b @ b'0'..=b'9'] => Some(u32::from(a - b'0') * 10 + u32::from(b - b'0')),
        _ => None,
    });
    let (h, m, s) = (parts.next()??, parts.next()??, parts.next()??);
    (h < 24 && m < 60 && s < 60).then_some(h * 3600 + m * 60 + s)
}

// A value as text: a string as it is, nothing for null, a number as written.
fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// A name from the lifecycle on one line, cut as other text is.
fn short(v: &Value) -> String {
    start(&text(v), 200, 1).replace('\n', " ")
}

// A week, in milliseconds: no dispatch takes longer.
const MAX_ELAPSED: u64 = 7 * 86400 * 1000;

/// The version of the runner's NDJSON this reads, and of its stable subset.
pub const NDJSON_VERSION: u64 = 1;

/// The idempotency key of a progress line: a resumed cycle writes its earlier
/// events again, and a line whose key was seen gives no second entry.
pub fn progress_key(line: &str) -> Option<String> {
    let r: Value = serde_json::from_str(line).ok()?;
    r["key"].as_str().map(str::to_string)
}

/// A line of the runner's NDJSON progress.
pub fn progress_line(line: &str) -> Option<Entry> {
    let r: Value = serde_json::from_str(line).ok()?;
    if r["v"].as_u64() != Some(NDJSON_VERSION) || r["unstable"] == true {
        return None;
    }
    let e = &r["event"];
    // When the event happened, else when the line was written.
    let at = clock(&e["at"]).or_else(|| clock(&r["at"]));
    let said = match r["kind"].as_str()? {
        "log" => start(&text(&r["message"]), 1200, 16),
        "step-started" => {
            let round = match &e["round"] {
                Value::Null => "1".to_string(),
                other => other
                    .as_u64()
                    .filter(|n| (1..=1000).contains(n))
                    .map_or("?".to_string(), |n| n.to_string()),
            };
            format!("--- {} started (round {round})", short(&e["step"]))
        }
        "dispatch-started" => format!("agent started: {}", short(&e["role"])),
        "dispatch-completed" => {
            let how = if e["ok"] == true { "done" } else { "failed" };
            match e["ms"].as_u64().filter(|ms| *ms <= MAX_ELAPSED) {
                Some(ms) => format!(
                    "agent {how}: {} in {}s",
                    short(&e["role"]),
                    (ms + 500) / 1000
                ),
                None => format!("agent {how}: {}", short(&e["role"])),
            }
        }
        "cap-hit" => match short(&e["nextStep"]) {
            next if next.is_empty() => format!("cap {}", short(&e["cap"])),
            next => format!("cap {}, next {next}", short(&e["cap"])),
        },
        "handoff-emitted" => match short(&e["commentUrl"]) {
            url if url.is_empty() => "handoff".to_string(),
            url => format!("handoff: {url}"),
        },
        "ci-status" => format!("ci {} {}", short(&e["status"]), short(&e["runUrl"]))
            .trim()
            .to_string(),
        "merged" => format!("merged: pull request {}", short(&e["prNumber"])),
        "interrupted" => format!("interrupted: {}", short(&e["reason"])),
        _ => return None,
    };
    Some(Entry::new("driver", said, at))
}

fn describe_call(name: &Value, arguments: &Value) -> String {
    if name == "bash" {
        return format!("$ {}", text(&arguments["command"]));
    }
    let target = ["path", "file_path", "pattern", "query"]
        .iter()
        .map(|key| &arguments[*key])
        .find(|v| !v.is_null())
        .and_then(Value::as_str)
        .unwrap_or("");
    format!("{} {target}", text(name)).trim().to_string()
}

/// A line of one agent's transcript. `role` names the agent.
pub fn transcript_line(role: &str, line: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let Ok(e) = serde_json::from_str::<Value>(line) else {
        return entries;
    };
    let m = &e["message"];
    if e["type"] != "message" || !m.is_object() {
        return entries;
    }
    let at = clock(&e["timestamp"]);
    let content = m["content"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if m["role"] == "assistant" {
        for c in content {
            if c["type"] == "text" {
                let said = c["text"].as_str().unwrap_or("").trim();
                if !said.is_empty() {
                    entries.push(Entry::new(role, start(said, 1200, 16), at));
                }
            } else if c["type"] == "toolCall" {
                let call = describe_call(&c["name"], &c["arguments"]);
                entries.push(Entry::new(role, start(&call, 600, 6), at));
            }
        }
        if let Some(error) = m["errorMessage"].as_str().filter(|e| !e.is_empty()) {
            entries.push(Entry::new(role, format!("! {}", start(error, 600, 6)), at));
        }
    } else if m["role"] == "toolResult" {
        let result = content
            .iter()
            .filter(|c| c["type"] == "text")
            .map(|c| text(&c["text"]))
            .collect::<Vec<_>>()
            .join("\n");
        let result = result.trim();
        if !result.is_empty() {
            let who = if m["isError"] == true {
                "  ! error"
            } else {
                "  result"
            };
            entries.push(Entry::new(who, start(result, 700, 10), at));
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOON: u32 = 12 * 3600;

    #[test]
    fn an_entry_is_one_line_with_its_time_and_speaker() {
        let e = Entry::new("driver", "hello", Some(3661));
        assert_eq!(format(&e, NOON), "01:01:01 driver     hello\n");
    }

    #[test]
    fn further_lines_are_indented_under_the_text_and_returns_are_dropped() {
        let e = Entry::new("  result", "one\r\ntwo", None);
        assert_eq!(
            format(&e, NOON),
            format!("12:00:00   result   one\n{}two\n", " ".repeat(20))
        );
    }

    #[test]
    fn what_is_written_is_masked() {
        let e = Entry::new("coder", "GH_TOKEN=abc123 set", None);
        assert_eq!(format(&e, 0), "00:00:00 coder      GH_TOKEN=*** set\n");
    }

    #[test]
    fn the_speaker_is_masked() {
        let token = format!("ghp_{}", "A".repeat(36));
        let out = format(&Entry::new(&token, "hello", None), 0);
        assert_eq!(out, "00:00:00 ghp_***    hello\n");
    }

    #[test]
    fn control_characters_do_not_reach_the_file() {
        let e = Entry::new("coder", "a\u{1b}[2Jb\u{7}c\td\u{9b}e\u{7f}", None);
        assert_eq!(
            format(&e, 0),
            "00:00:00 coder      a\u{fffd}[2Jb\u{fffd}c\td\u{fffd}e\u{fffd}\n"
        );
    }

    #[test]
    fn direction_overrides_and_line_separators_do_not_reach_the_file() {
        let e = Entry::new("coder", "a\u{202e}b\u{2028}c\u{200b}d\u{2029}e", None);
        let out = format(&e, 0);
        for c in ['\u{202e}', '\u{2028}', '\u{200b}', '\u{2029}'] {
            assert!(!out.contains(c), "{out:?}");
        }
        assert_eq!(out, "00:00:00 coder      ab\u{fffd}cd\u{fffd}e\n");
    }

    #[test]
    fn a_secret_where_a_text_is_cut_is_masked_whole() {
        let token = format!("ghp_{}", "A".repeat(36));
        let said = |m: Value| -> String {
            let entries = transcript_line("coder", &message(m));
            entries.into_iter().map(|e| e.text).collect()
        };
        for (role, before) in [("assistant", 1195), ("toolResult", 695)] {
            let text = format!("{} {token}", "x".repeat(before));
            let out = said(json!({"role": role, "content": [{"type": "text", "text": text}]}));
            assert!(
                out.contains(" ghp_") && !out.contains("ghp_A"),
                "{role}: {out}"
            );
        }
        let command = format!("{} {token}", "x".repeat(593));
        let call = json!({"type": "toolCall", "name": "bash", "arguments": {"command": command}});
        let out = said(json!({"role": "assistant", "content": [call]}));
        assert!(out.contains(" ghp_") && !out.contains("ghp_A"), "{out}");
    }

    #[test]
    fn a_long_notice_and_a_long_error_are_cut() {
        let long = "word ".repeat(1000);
        let notice = json!({"v": 1, "kind": "log", "message": long});
        let text = progress_line(&notice.to_string()).unwrap().text;
        assert!(
            text.chars().count() < 1300 && text.ends_with("[...]"),
            "{text}"
        );
        let error = message(json!({"role": "assistant", "content": [], "errorMessage": long}));
        let text = &transcript_line("coder", &error)[0].text;
        assert!(
            text.chars().count() < 700 && text.ends_with("[...]"),
            "{text}"
        );
    }

    #[test]
    fn a_short_text_is_kept_whole() {
        assert_eq!(clip("one\ntwo", 100, 5), "one\ntwo");
    }

    #[test]
    fn a_long_text_is_cut_by_lines_and_by_characters_and_marked() {
        assert_eq!(clip("one\ntwo\nthree", 100, 2), "one\ntwo\n[...]");
        assert_eq!(clip("abcdef", 3, 5), "abc\n[...]");
        assert_eq!(clip("ab   cdef", 4, 5), "ab\n[...]");
        // Characters are counted, not bytes.
        assert_eq!(clip("pä😀ivä", 3, 5), "pä😀\n[...]");
    }

    #[test]
    fn the_time_is_read_from_a_date_or_from_milliseconds() {
        assert_eq!(
            clock(&json!("2026-10-10T06:15:45.123Z")),
            Some(6 * 3600 + 15 * 60 + 45)
        );
        assert_eq!(clock(&json!("2026-10-10T23:59:59Z")), Some(86399));
        assert_eq!(
            clock(&json!(1791526545000u64)),
            Some(6 * 3600 + 15 * 60 + 45)
        );
    }

    #[test]
    fn a_time_that_cannot_be_read_is_no_time() {
        for at in [
            json!(null),
            json!({}),
            json!("soon"),
            json!("2026-10-10"),
            json!("2026-10-10T06:15Z"),
            json!("2026-10-10T06:15:45"),
            json!("2026-10-10T06:15:45+03:00"),
            json!("2026-10-10T24:00:00Z"),
            json!("2026-10-10T23:60:00Z"),
            json!("2026-10-10T23:59:60Z"),
            json!("2026-10-10T+1:+2:+3Z"),
            json!("päivää-päivää-päiväZ"),
            json!(-1),
            json!(1e300),
            json!(1_500_000_000_000u64),
            json!(5_000_000_000_000u64),
        ] {
            assert_eq!(clock(&at), None, "{at}");
        }
    }

    fn progress(kind: &str, event: Value) -> String {
        json!({"v": 1, "kind": kind, "at": "2026-10-10T00:00:07.120Z", "issue": 7,
               "key": "7:0:1", "event": event})
        .to_string()
    }

    #[test]
    fn a_driver_notice_is_an_entry() {
        let line =
            json!({"v": 1, "kind": "log", "at": "2026-10-10T00:00:07Z", "message": "parked"});
        assert_eq!(
            progress_line(&line.to_string()),
            Some(Entry::new("driver", "parked", Some(7)))
        );
        let empty = json!({"v": 1, "kind": "log", "at": "soon"});
        assert_eq!(
            progress_line(&empty.to_string()),
            Some(Entry::new("driver", "", None))
        );
    }

    #[test]
    fn the_stable_kinds_are_entries() {
        let said = |kind: &str, event: Value| {
            progress_line(&progress(kind, event)).map(|e| (e.text, e.at))
        };
        let at = Some(7);
        // The event's own time, in milliseconds, comes first.
        assert_eq!(
            said(
                "step-started",
                json!({"step": "plan", "at": 1_792_089_524_024u64})
            )
            .unwrap()
            .1,
            Some(18 * 3600 + 38 * 60 + 44)
        );
        assert_eq!(
            said("step-started", json!({"step": "develop", "at": 1})),
            Some(("--- develop started (round 1)".to_string(), at))
        );
        assert_eq!(
            said("step-started", json!({"step": "review", "round": 3})),
            Some(("--- review started (round 3)".to_string(), at))
        );
        assert_eq!(
            said(
                "dispatch-started",
                json!({"step": "develop", "role": "developer", "jobId": "j"})
            ),
            Some(("agent started: developer".to_string(), at))
        );
        assert_eq!(
            said(
                "dispatch-completed",
                json!({"role": "developer", "ok": true, "ms": 61500})
            ),
            Some(("agent done: developer in 62s".to_string(), at))
        );
        assert_eq!(
            said(
                "dispatch-completed",
                json!({"role": "ops", "ok": false, "ms": 1e300})
            ),
            Some(("agent failed: ops".to_string(), at))
        );
        assert_eq!(
            said(
                "cap-hit",
                json!({"cap": "verify-failed:develop", "nextStep": "handoff"})
            ),
            Some(("cap verify-failed:develop, next handoff".to_string(), at))
        );
        assert_eq!(
            said("cap-hit", json!({"cap": "round-cap"})),
            Some(("cap round-cap".to_string(), at))
        );
        assert_eq!(
            said(
                "handoff-emitted",
                json!({"commentUrl": "https://github.com/a/b/issues/7#issuecomment-1"})
            ),
            Some((
                "handoff: https://github.com/a/b/issues/7#issuecomment-1".to_string(),
                at
            ))
        );
        assert_eq!(
            said("handoff-emitted", json!({})),
            Some(("handoff".to_string(), at))
        );
        assert_eq!(
            said(
                "ci-status",
                json!({"status": "success", "runUrl": "https://ci/1"})
            ),
            Some(("ci success https://ci/1".to_string(), at))
        );
        assert_eq!(
            said("merged", json!({"prNumber": 99})),
            Some(("merged: pull request 99".to_string(), at))
        );
        assert_eq!(
            said("interrupted", json!({"reason": "SIGTERM"})),
            Some(("interrupted: SIGTERM".to_string(), at))
        );
    }

    #[test]
    fn names_are_cut_and_odd_numbers_left_out() {
        let said = |kind: &str, event: Value| progress_line(&progress(kind, event)).unwrap().text;
        let long = "l".repeat(10_000);
        let text = said("step-started", json!({"step": long}));
        assert!(
            text.chars().count() < 260 && text.contains("[...] started"),
            "{text}"
        );
        let text = said("dispatch-started", json!({"role": long}));
        assert!(
            text.chars().count() < 240 && text.ends_with("[...]"),
            "{text}"
        );
        assert_eq!(
            said(
                "step-started",
                json!({"step": "review", "round": {"a": [1, 2]}})
            ),
            "--- review started (round ?)"
        );
    }

    #[test]
    fn unstable_unknown_and_other_lines_give_no_entry() {
        let unstable = json!({"v": 1, "kind": "dispatch-failed", "unstable": true, "at": "2026-10-10T00:00:07Z",
                              "event": {"role": "ops"}});
        assert_eq!(progress_line(&unstable.to_string()), None);
        let marked =
            json!({"v": 1, "kind": "step-started", "unstable": true, "event": {"step": "x"}});
        assert_eq!(progress_line(&marked.to_string()), None);
        assert_eq!(progress_line(&progress("brand-new-kind", json!({}))), None);
        let later = json!({"v": 2, "kind": "log", "message": "x"});
        assert_eq!(progress_line(&later.to_string()), None);
        for line in [
            "",
            "not json",
            "[WARN  tini (7)] Tini is not running as PID 1",
            "null",
            "7",
            "[]",
            r#"{"type":"extension_ui_request","method":"notify","message":"x"}"#,
            r#"{"v":1}"#,
        ] {
            assert_eq!(progress_line(line), None, "{line}");
        }
    }

    #[test]
    fn the_key_of_a_line_is_read() {
        assert_eq!(
            progress_key(&progress("step-started", json!({}))).as_deref(),
            Some("7:0:1")
        );
        assert_eq!(progress_key(r#"{"v":1,"kind":"log"}"#), None);
        assert_eq!(progress_key("not json"), None);
    }

    fn message(m: Value) -> String {
        json!({"type": "message", "timestamp": "2026-10-10T00:00:09Z", "message": m}).to_string()
    }

    #[test]
    fn what_an_agent_says_and_the_commands_it_runs_are_entries() {
        let line = message(json!({"role": "assistant", "content": [
            {"type": "text", "text": "  I will run the tests.\n"},
            {"type": "thinking", "thinking": "hm"},
            {"type": "text", "text": "   "},
            {"type": "toolCall", "name": "bash", "arguments": {"command": "scripts/verify.sh"}},
            {"type": "toolCall", "name": "read", "arguments": {"path": "README.md"}},
            {"type": "toolCall", "name": "grep", "arguments": {"pattern": "tehas_load", "path": null}},
            {"type": "toolCall", "name": "edit", "arguments": {"path": 7, "pattern": "x"}},
            {"type": "toolCall", "name": "todo"},
        ]}));
        let said: Vec<String> = transcript_line("coder", &line)
            .into_iter()
            .map(|e| e.text)
            .collect();
        assert_eq!(
            said,
            [
                "I will run the tests.",
                "$ scripts/verify.sh",
                "read README.md",
                "grep tehas_load",
                "edit",
                "todo"
            ]
        );
        assert!(transcript_line("coder", &line)
            .iter()
            .all(|e| e.who == "coder" && e.at == Some(9)));
    }

    #[test]
    fn an_agent_error_is_an_entry() {
        let line = message(
            json!({"role": "assistant", "content": [], "errorMessage": "context overflow"}),
        );
        assert_eq!(
            transcript_line("coder", &line),
            [Entry::new("coder", "! context overflow", Some(9))]
        );
    }

    #[test]
    fn the_start_of_a_result_is_an_entry() {
        let ok = message(json!({"role": "toolResult", "content": [
            {"type": "text", "text": "ok    lint\n"}, {"type": "image", "data": "..."}, {"type": "text", "text": "ok    tests"},
        ]}));
        assert_eq!(
            transcript_line("coder", &ok),
            [Entry::new("  result", "ok    lint\n\nok    tests", Some(9))]
        );
        let failed = message(
            json!({"role": "toolResult", "isError": true, "content": [{"type": "text", "text": "exit 1"}]}),
        );
        assert_eq!(
            transcript_line("coder", &failed),
            [Entry::new("  ! error", "exit 1", Some(9))]
        );
        let long = message(
            json!({"role": "toolResult", "content": [{"type": "text", "text": "line\n".repeat(30)}]}),
        );
        assert_eq!(
            transcript_line("coder", &long)[0].text,
            format!("{}\n[...]", ["line"; 10].join("\n"))
        );
        let empty =
            message(json!({"role": "toolResult", "content": [{"type": "text", "text": " \n"}]}));
        assert_eq!(transcript_line("coder", &empty), []);
    }

    #[test]
    fn lines_that_are_no_message_give_no_entry() {
        for line in [
            "",
            "{",
            "null",
            "[1]",
            r#"{"type":"session"}"#,
            r#"{"type":"message"}"#,
            r#"{"type":"message","message":"text"}"#,
            r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
            r#"{"type":"message","message":{"role":"assistant","content":"hi"}}"#,
            r#"{"type":"message","message":{"role":"assistant","content":[null,7,{"type":"text","text":5}]}}"#,
        ] {
            assert_eq!(transcript_line("coder", line), [], "{line}");
        }
    }
}
