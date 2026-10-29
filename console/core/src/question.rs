//! The bot's question comments, as lib/questions.sh writes them (q_clean,
//! q_render): an agent's words made safe to post, and the body of a planning
//! turn's question or a park's. The dispatcher posts the one, a cycle the
//! other.

use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

/// The options a question lists and its record keeps (Q_MAX_OPTIONS).
pub const MAX_OPTIONS: usize = 20;

/// The two retry boxes of a park's question, without their box. The comment
/// and both polls read them through the labels the answer parser returns, so
/// the wording of `q_render` below, of lib/questions.sh and of the readers in
/// libexec/dispatch, console/dispatch and the test helpers is one string that
/// cannot drift: test/questions_test.sh compares the two languages.
pub const BOX_CLEAN: &str = "Retry from a clean start";
pub const BOX_ALONE: &str = "Retry from the issue alone";

/// The `sig` of a park marker: the hash of the park's fields, its free text
/// in the hash only. The cycle writes it, the MCP server's `factory_park`
/// gives it again from the fields the run recorded.
pub fn park_sig(step: &str, cap: &str, park_reason: &str, kind: &str, cause: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(format!("{step}|{cap}|{park_reason}|{kind}|{cause}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The line `q_render` prints for a box, and the line a fixture writes for a
/// ticked one. Both polls take the text after the box as it stands, so the
/// label of the answer parser and this line are the same text.
pub fn box_line(label: &str, ticked: bool) -> String {
    format!("- [{}] {}", if ticked { "x" } else { " " }, label)
}

/// q_clean: an agent's words, safe to post on a public issue, cut at `max`
/// characters. Line by line, as sed reads them.
pub fn q_clean(text: &str, max: usize) -> String {
    static RULES: OnceLock<Vec<(Regex, String)>> = OnceLock::new();
    let rules = RULES.get_or_init(|| {
        [
            (r"(^|[^A-Za-z0-9_])(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}", "${1}${2}***"),
            (r"(postgres(ql)?://[^:[:space:]/]+:)[^@[:space:]]+@", "${1}***@"),
            (
                r"(^|[^A-Z0-9_])([A-Z0-9_]*(TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)[^[:space:]]+",
                "${1}${2}***",
            ),
            (
                r"(^|[^0-9.])(10\.[0-9]{1,3}|192\.168|172\.(1[6-9]|2[0-9]|3[01])|169\.254|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}",
                "${1}[private address]",
            ),
            (
                r"(^|[^0-9A-Fa-f:])[fF][cCdD][0-9A-Fa-f]{2}(:[0-9A-Fa-f]{0,4}){2,}",
                "${1}[private address]",
            ),
            (r"[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.ts\.net", "[private name]"),
            (r"<!--", r"<\!--"),
        ]
        .into_iter()
        .map(|(p, r)| (Regex::new(p).unwrap(), r.to_string()))
        .collect()
    });
    static BOX: OnceLock<Regex> = OnceLock::new();
    let boxed =
        BOX.get_or_init(|| Regex::new(r"^([[:space:]]*[-*+][[:space:]]+)\[([ xX])\]").unwrap());
    static AT: OnceLock<Regex> = OnceLock::new();
    let at = AT.get_or_init(|| Regex::new(r"@([A-Za-z0-9])").unwrap());
    // jq -r adds the line end the text is then read with.
    let whole = format!("{text}\n").replace('\r', "");
    let lines: Vec<String> = whole
        .split('\n')
        .map(|l| {
            let mut l = l.to_string();
            for (re, with) in rules {
                l = re.replace_all(&l, with.as_str()).into_owned();
            }
            l = boxed.replace(&l, r"${1}\[${2}]").into_owned();
            at.replace_all(&l, "@\u{200b}${1}").into_owned()
        })
        .collect();
    let cut: String = lines.join("\n").chars().take(max).collect();
    cut.trim_end_matches('\n').to_string()
}

/// Which question: a planning turn's or a park's. Each ends in its own boxes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Plan,
    Park,
}

/// A text of the outcome, a string or nothing, as `.k // ""` reads it.
fn s<'a>(v: &'a Value, path: &str) -> &'a str {
    v.pointer(path).and_then(Value::as_str).unwrap_or("")
}

/// q_render: the body of a question comment. `out` holds the text, the
/// question, pi-rukas's handoff marker (`marker`) and, for a park, tehas's
/// park marker (`park`); `line` is the host's own, put between the question
/// and the boxes.
pub fn q_render(kind: Kind, issue: u64, turn: u64, out: &Value, line: &str) -> String {
    let mut b = String::new();
    let handoff = Regex::new(&format!(
        r"^<!-- pi-rukas:handoff issue={issue} cap=[A-Za-z0-9_.:-]{{1,80}} -->$"
    ))
    .unwrap();
    let marker = s(out, "/marker");
    if handoff.is_match(marker) {
        b += &format!("{marker}\n");
    }
    b += &format!("{}\n", q_clean(s(out, "/text"), 40000));
    if out.get("question").is_some_and(|q| !q.is_null()) {
        b += &format!("\n{}\n", q_clean(s(out, "/question/question"), 4000));
    }
    if !line.is_empty() {
        b += &format!("\n{line}\n");
    }
    b += "\n";
    let options: Vec<Value> = out
        .pointer("/question/options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut n = options.len();
    if n > MAX_OPTIONS {
        b += &format!(
            "Options left out: {}. A reply can name another choice.\n\n",
            n - MAX_OPTIONS
        );
        n = MAX_OPTIONS;
    }
    let one_line = |v: Option<&Value>| {
        let t = v.and_then(Value::as_str).unwrap_or("");
        Regex::new(r"[\r\n]+")
            .unwrap()
            .replace_all(t, " ")
            .into_owned()
    };
    for o in &options[..n] {
        b += &format!("- [ ] {}\n", q_clean(&one_line(o.get("label")), 300));
        let desc = q_clean(&one_line(o.get("description")), 600);
        if !desc.is_empty() {
            b += &format!("  {desc}\n");
        }
    }
    let name = match kind {
        Kind::Plan => {
            b += "- [ ] End this planning session\n\n";
            b += "Tick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n";
            "plan"
        }
        Kind::Park => {
            b += &format!(
                "{}\n{}\n- [ ] Leave it parked\n\n",
                box_line(BOX_CLEAN, false),
                box_line(BOX_ALONE, false)
            );
            b += "Tick one box. Only a tick from an account that may authorise work counts.\n";
            "park"
        }
    };
    b += "\n";
    let park = Regex::new(&format!(
        r"^<!-- tehas:park issue={issue} step=[a-z0-9-]{{0,40}} kind=[a-z0-9-]{{0,40}} provider=[01] sig=[0-9a-f]{{64}} -->$"
    ))
    .unwrap();
    let park_marker = s(out, "/park");
    if park.is_match(park_marker) {
        b += &format!("{park_marker}\n");
    }
    b += &format!("<!-- tehas:question kind={name} issue={issue} turn={turn} -->\n");
    // As $(q_render) has it: without its last line ends.
    b.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_park_question_has_the_retry_boxes_and_both_markers() {
        let sig = "a".repeat(64);
        let park = format!("<!-- tehas:park issue=7 step=review kind= provider=0 sig={sig} -->");
        let out = json!({"text": "Tehas cycle stopped.", "question": null,
            "marker": "<!-- pi-rukas:handoff issue=7 cap=x -->", "park": park});
        assert_eq!(
            q_render(Kind::Park, 7, 1, &out, ""),
            format!(
                "<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\n- [ ] Retry from a clean start\n- [ ] Retry from the issue alone\n- [ ] Leave it parked\n\nTick one box. Only a tick from an account that may authorise work counts.\n\n{park}\n<!-- tehas:question kind=park issue=7 turn=1 -->"
            )
        );
    }

    #[test]
    fn the_retry_boxes_are_the_lines_the_polls_read() {
        // The renderer and the readers are one string: a ticked box of a
        // rendered body is what each poll reads, and the other stays unticked.
        for label in [BOX_CLEAN, BOX_ALONE] {
            assert_eq!(box_line(label, false), format!("- [ ] {label}"));
            assert_eq!(box_line(label, true), format!("- [x] {label}"));
            let body = q_render(
                Kind::Park,
                7,
                1,
                &json!({"text": "Tehas cycle stopped.", "question": null}),
                "",
            );
            // The label of the box as the answer parser reads it is the label
            // the polls compare these consts with, in order, one per box.
            let labels: Vec<String> = body
                .split('\n')
                .filter(|l| l.starts_with("- [ "))
                .map(|l| l[6..].to_string())
                .collect();
            assert_eq!(labels, vec![BOX_CLEAN, BOX_ALONE, "Leave it parked"]);
        }
    }

    // The signatures of real park markers, from the fields the run recorded.
    #[test]
    fn a_park_signature_is_the_hash_of_its_fields() {
        assert_eq!(
            park_sig("develop", "verify-failed:develop", "", "verify", ""),
            "a3e22363362fd7e9eaeee67a44164f72acc1065fd338f9ba93ed49e21dab554b"
        );
        assert_eq!(
            park_sig(
                "develop",
                "child-timeout:develop",
                "",
                "timeout",
                "self-killed:child-bound"
            ),
            "8812b904b6ac9626539ee62d3c9924899520b8721ebf2e0a859095d7c395ac78"
        );
        assert_eq!(
            park_sig("-", "", "", "", ""),
            "7e45afdeccfc3eb5505b1de691f6392e508199ca4275e92c42fdb17d51ad87ec"
        );
    }

    #[test]
    fn a_park_marker_of_another_issue_is_left_out() {
        let park = format!(
            "<!-- tehas:park issue=8 step= kind= provider=0 sig={} -->",
            "a".repeat(64)
        );
        let out = json!({"text": "x", "park": park});
        assert!(!q_render(Kind::Park, 7, 1, &out, "").contains("tehas:park"));
    }
}
