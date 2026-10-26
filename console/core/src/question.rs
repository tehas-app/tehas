//! The bot's question comments, as lib/questions.sh writes them (q_clean,
//! q_render): an agent's words made safe to post, and the body of a planning
//! turn's question or a park's. The dispatcher posts the one, a cycle the
//! other.

use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

/// The options a question lists and its record keeps (Q_MAX_OPTIONS).
pub const MAX_OPTIONS: usize = 20;

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
            b += "- [ ] Retry from a clean start\n- [ ] Leave it parked\n\n";
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
                "<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\n- [ ] Retry from a clean start\n- [ ] Leave it parked\n\nTick one box. Only a tick from an account that may authorise work counts.\n\n{park}\n<!-- tehas:question kind=park issue=7 turn=1 -->"
            )
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
