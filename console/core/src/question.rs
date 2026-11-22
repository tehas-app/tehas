//! The bot's question comments (q_clean, q_render): an agent's words made safe to post, and the body of a planning
//! turn's question or a park's. The dispatcher posts the one, a cycle the
//! other.

use regex::{Captures, Regex};
use serde_json::Value;
use std::sync::OnceLock;

/// The options a question lists and its record keeps.
pub const MAX_OPTIONS: usize = 20;

/// The two retry boxes of a park's question, without their box. The comment
/// and both polls read them through the labels the answer parser returns, so
/// `q_render` below and the readers in console/dispatch and the test helpers
/// share one string. `BOX_COMMENTS` is rendered only when the sandbox image
/// gives a cycle the comments (`comments` in the outcome).
pub const BOX_COMMENTS: &str = "Retry with the issue and all its comments";
pub const BOX_ALONE: &str = "Retry with the issue text only";

/// The labels of the two retries before #1492, which the readers still take on
/// a comment posted before it: the clean start is the comments retry, the
/// issue alone the text-only one. Remove them two releases after #1492.
pub const OLD_BOX_CLEAN: &str = "Retry from a clean start";
pub const OLD_BOX_ALONE: &str = "Retry from the issue alone";

/// The last box of a park's question, and of a planning question. A program
/// answers with them through `tehas answer` (#2782), so the poll finds the
/// box an answer stands for by the same text the comment shows.
pub const BOX_PARKED: &str = "Leave it parked";
pub const BOX_END: &str = "End this planning session";

/// A choice an answer of `tehas answer` gives (#2782). Its word is the one
/// the command, the store's verb, its CHECK (a test of the store holds the two
/// alike) and the dispatcher read; `option` carries the number of an option,
/// `text` a reply in words, and `update` is the planner's option
/// [`crate::plan::UPDATE`], kept by its number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Retry,
    RetryText,
    Park,
    Option,
    Update,
    End,
    Text,
}

/// Every choice, in the order a refusal lists their words.
pub const CHOICES: [Choice; 7] = [
    Choice::Retry,
    Choice::RetryText,
    Choice::Park,
    Choice::Option,
    Choice::Update,
    Choice::End,
    Choice::Text,
];

impl Choice {
    pub fn word(self) -> &'static str {
        match self {
            Choice::Retry => "retry",
            Choice::RetryText => "retry-text",
            Choice::Park => "park",
            Choice::Option => "option",
            Choice::Update => "update",
            Choice::End => "end",
            Choice::Text => "text",
        }
    }

    /// The kind of question it answers.
    pub fn kind(self) -> Kind {
        match self {
            Choice::Retry | Choice::RetryText | Choice::Park => Kind::Park,
            _ => Kind::Plan,
        }
    }

    /// The choice of a word, or None for no word of one.
    pub fn of(word: &str) -> Option<Choice> {
        CHOICES.into_iter().find(|c| c.word() == word)
    }
}

/// The choices a park's question offers, in the order it lists them: the
/// comments retry when the image gives a cycle the comments and a retry is
/// offered at all, the text-only retry when one is, and leaving it parked.
/// A develop timeout that repeats on the same text offers no retry.
pub fn park_offers(retry: bool, comments: bool) -> Vec<Choice> {
    let mut offers = Vec::new();
    if retry && comments {
        offers.push(Choice::Retry);
    }
    if retry {
        offers.push(Choice::RetryText);
    }
    offers.push(Choice::Park);
    offers
}

/// The words a person sees for a choice of a park's question; empty for a
/// choice of a planning question.
pub fn park_label(c: Choice) -> &'static str {
    match c {
        Choice::Retry => BOX_COMMENTS,
        Choice::RetryText => BOX_ALONE,
        Choice::Park => BOX_PARKED,
        _ => "",
    }
}

/// One choice a waiting question offers: the words a person sees, the
/// choice, and the number of a planning option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offered {
    pub label: String,
    pub choice: Choice,
    pub option: Option<u64>,
}

/// The choices of a planning question whose turn offered `options`, in the
/// order its comment lists them: each option up to [`MAX_OPTIONS`] (the
/// planner's [`crate::plan::UPDATE`] as `update`), then the end of the session.
pub fn plan_choices(options: &[String]) -> Vec<Offered> {
    let mut out: Vec<Offered> = options
        .iter()
        .take(MAX_OPTIONS)
        .enumerate()
        .map(|(i, label)| Offered {
            label: label.clone(),
            choice: if label == crate::plan::UPDATE {
                Choice::Update
            } else {
                Choice::Option
            },
            option: Some(i as u64 + 1),
        })
        .collect();
    out.push(Offered {
        label: BOX_END.to_string(),
        choice: Choice::End,
        option: None,
    });
    out
}

/// The choices of a park's question that offers `offers`, with their words.
pub fn park_choices(offers: &[Choice]) -> Vec<Offered> {
    offers
        .iter()
        .filter(|c| c.kind() == Kind::Park)
        .map(|c| Offered {
            label: park_label(*c).to_string(),
            choice: *c,
            option: None,
        })
        .collect()
}

/// The line that asks a planning question's choices, and a park's. The
/// choices follow it, one to a line, and [`listed`] reads them back from
/// after its last one: the host writes it after the agent's words.
pub const ASK_PLAN: &str =
    "Reply in a comment with the number of one of these, or answer in your own words:";
pub const ASK_PARK: &str = "Reply in a comment with the word of one of these:";

/// The line that ends the choices of every question.
pub const ONLY_ALLOWED: &str = "Only an answer from an account that may authorise work counts.";

/// What the end of a planning session leads to.
pub const LEADS_END: &str = "the session ends, and nothing more is planned";

/// What a park's choice leads to, said after its words.
pub fn leads_to(c: Choice) -> &'static str {
    match c {
        Choice::Retry => "A new cycle starts over in a new workspace, given what allowed accounts wrote on this issue and on its pull requests",
        Choice::RetryText => "A new cycle starts over in a new workspace, given the title and body and nothing else",
        Choice::Park => "Nothing more happens until someone answers",
        _ => "",
    }
}

/// The labels of the choices a question's comment lists, in its order: the
/// lines after the last of its asking lines, up to the line that ends them.
/// A planning option's label is its line's words before the first ": ", the
/// end of the session [`BOX_END`]; a park's choice is the label of its word.
/// None for a comment that lists none, as a comment of before 2.0 does.
pub fn listed(body: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = body.lines().collect();
    let at = lines
        .iter()
        .rposition(|l| *l == ASK_PLAN || *l == ASK_PARK)?;
    let plan = lines[at] == ASK_PLAN;
    let mut out = Vec::new();
    for l in lines[at + 1..].iter().copied() {
        if l == ONLY_ALLOWED {
            return Some(out);
        }
        if l.is_empty() {
            continue;
        }
        let label = if plan {
            let rest = l.split_once(". ").map(|(_, r)| r)?;
            if rest.starts_with(BOX_END) {
                BOX_END.to_string()
            } else {
                rest.split_once(": ").map_or(rest, |(l, _)| l).to_string()
            }
        } else {
            let word = l.strip_prefix("- ")?.split_once(':')?.0;
            park_label(Choice::of(word)?).to_string()
        };
        out.push(label);
    }
    None
}

/// Which retry a ticked label asks for: Some(true) the comments retry, Some(false)
/// the text-only one, None no retry. The labels of before #1492 count as theirs.
pub fn retry_of(label: &str) -> Option<bool> {
    match label {
        BOX_COMMENTS | OLD_BOX_CLEAN => Some(true),
        BOX_ALONE | OLD_BOX_ALONE => Some(false),
        _ => None,
    }
}
/// The reason on record of a park over a lean start tehas work refused: a
/// retry of it waits while the issue carries `tehas:lean`, or under
/// `TEHAS_DRIVER=lean` until it carries `tehas:classic`.
pub const LEAN_REFUSED: &str = "lean-refused";

/// The `sig` of a park marker: the hash of the park's fields as the state
/// store keeps them, its free text in the hash only. The cycle writes it, the
/// MCP server's `factory_park` gives it again from the fields the run
/// recorded, and the cycle's repeat check hashes the row `cycle-end` stored.
///
/// Each field is cleaned here and not at the callers, the way `cycle-end`
/// cleans it (`Cleaned::word` of the store): a park whose words hold a space,
/// an uppercase letter or a field over its bound has to say that it repeats
/// the one before it, so the marker of a run and the row of its history hash
/// the same bytes whatever a caller passed. A field already clean and within
/// its bound hashes as it did before the cleaning moved here.
///
/// A field is the characters of its set, in order, cut to its bound in bytes.
/// `step` keeps ASCII alphanumerics, `_`, `.` and `-`; `cap` keeps those and
/// `:`; `park_reason` and `kind` keep `a-z` and `-`; `cause` keeps the set of
/// `cap`.
pub fn park_sig(step: &str, cap: &str, park_reason: &str, kind: &str, cause: &str) -> String {
    use sha2::{Digest, Sha256};

    /// The characters of `keep`, then the first `max` bytes of them.
    fn word(keep: fn(u8) -> bool, max: usize, given: &str) -> String {
        let mut v: String = given
            .chars()
            .filter(|c| c.is_ascii() && keep(*c as u8))
            .collect();
        v.truncate(max);
        v
    }
    fn word_char(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b"_.-".contains(&b)
    }
    fn cap_char(b: u8) -> bool {
        word_char(b) || b == b':'
    }
    fn kebab_char(b: u8) -> bool {
        b.is_ascii_lowercase() || b == b'-'
    }
    let cleaned = [
        (word_char as fn(u8) -> bool, 40, step),
        (cap_char as fn(u8) -> bool, 80, cap),
        (kebab_char as fn(u8) -> bool, 40, park_reason),
        (kebab_char, 40, kind),
        (cap_char, 60, cause),
    ]
    .map(|(keep, max, given)| word(keep, max, given))
    .join("|");
    Sha256::digest(cleaned.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The line a comment of before 2.0 held for a box, ticked or not: what the
/// readers of an old comment and their test fixtures write. Nothing the
/// factory writes now holds one. Both polls take the text after the box as it stands, so the
/// label of the answer parser and this line are the same text.
pub fn box_line(label: &str, ticked: bool) -> String {
    format!("- [{}] {}", if ticked { "x" } else { " " }, label)
}

/// q_clean: an agent's words, safe to post on a public issue, cut at `max`
/// characters. The secrets, private addresses and names are what
/// `mask::mask` takes out, the one rule set the console uses too; the rules
/// here are a comment's own: a comment marker, a task box and a mention.
/// Line by line, as sed reads them.
pub fn q_clean(text: &str, max: usize) -> String {
    static BOX: OnceLock<Regex> = OnceLock::new();
    let boxed = BOX.get_or_init(|| {
        Regex::new(r"^([[:space:]>]*(?:(?:[-*+]|[0-9]+[.)])[[:space:]>]+)+)\[([ xX])\]").unwrap()
    });
    static AT: OnceLock<Regex> = OnceLock::new();
    let at = AT.get_or_init(|| Regex::new(r"@([A-Za-z0-9])").unwrap());
    // jq -r adds the line end the text is then read with.
    let whole = format!("{}\n", crate::text::shown(&crate::mask::mask(text))).replace('\r', "");
    let lines: Vec<String> = whole
        .split('\n')
        .map(|l| {
            let l = l.replace("<!--", r"<\!--");
            let l = boxed
                .replace(&l, |c: &Captures| {
                    let glyph = if &c[2] == " " { '☐' } else { '☑' };
                    format!("{}{glyph}", &c[1])
                })
                .into_owned();
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

impl Kind {
    /// The kind as the store and a question's marker name it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Plan => "plan",
            Kind::Park => "park",
        }
    }
}

/// A text of the outcome, a string or nothing, as `.k // ""` reads it.
fn s<'a>(v: &'a Value, path: &str) -> &'a str {
    v.pointer(path).and_then(Value::as_str).unwrap_or("")
}

/// The rule of pi-rukas's handoff marker: `line` is exactly the marker of
/// `issue`, and of no other issue. The cap is 1 to 80 characters of its set,
/// so a cap with a space or a missing cap is no marker, and neither is the
/// marker with anything else on its line. `q_render` prints the marker of a
/// park by this rule, and a cycle counts a handoff delivered by it too.
pub fn is_handoff_marker(line: &str, issue: u64) -> bool {
    static MARKER: OnceLock<Regex> = OnceLock::new();
    let marker = MARKER.get_or_init(|| {
        Regex::new(r"^<!-- pi-rukas:handoff issue=([0-9]+) cap=[A-Za-z0-9_.:-]{1,80} -->$").unwrap()
    });
    marker
        .captures(line)
        .is_some_and(|c| c[1] == *issue.to_string())
}

/// q_render: the body of a question comment. `out` holds the text, the
/// question, pi-rukas's handoff marker (`marker`) and, for a park, tehas's
/// park marker (`park`); `line` is the host's own, put between the question
/// and the boxes. A planning turn's text is left out: the caller says it in
/// a comment before this one, so nothing of a long plan can move a box.
pub fn q_render(kind: Kind, issue: u64, turn: u64, out: &Value, line: &str) -> String {
    let mut b = String::new();
    if kind == Kind::Park {
        let marker = s(out, "/marker");
        if is_handoff_marker(marker, issue) {
            b += &format!("{marker}\n");
        }
        b += &format!("{}\n\n", q_clean(s(out, "/text"), 40000));
    }
    if out.get("question").is_some_and(|q| !q.is_null()) {
        b += &format!("{}\n\n", q_clean(s(out, "/question/question"), 4000));
    }
    if !line.is_empty() {
        b += &format!("{line}\n\n");
    }
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
    let name = match kind {
        Kind::Plan => {
            b += &format!("{ASK_PLAN}\n\n");
            for (k, o) in options[..n].iter().enumerate() {
                let mut label = q_clean(&one_line(o.get("label")), 300);
                // An agent's option never reads as the end of the session.
                if label.to_lowercase().starts_with(&BOX_END.to_lowercase()) {
                    label = format!("Option {}, {label}", k + 1);
                }
                let desc = q_clean(&one_line(o.get("description")), 600);
                b += &if desc.is_empty() {
                    format!("{}. {label}\n", k + 1)
                } else {
                    format!("{}. {label}: {desc}\n", k + 1)
                };
            }
            b += &format!("{}. {BOX_END}: {LEADS_END}.\n\n", n + 1);
            b += &format!("{ONLY_ALLOWED}\n");
            "plan"
        }
        Kind::Park => {
            // A develop timeout that repeats on the same text offers no retry,
            // and no choice offers comments the image would not give the cycle.
            let retry = out.get("retry") != Some(&Value::Bool(false));
            let comments = out.get("comments") == Some(&Value::Bool(true));
            b += &format!("{ASK_PARK}\n\n");
            for c in park_offers(retry, comments) {
                b += &format!("- {}: {}. {}.\n", c.word(), park_label(c), leads_to(c));
            }
            b += &format!("\n{ONLY_ALLOWED}\n");
            "park"
        }
    };
    b += "\n";
    let park_marker = s(out, "/park");
    if is_park_marker(park_marker, issue) {
        b += &format!("{park_marker}\n");
    }
    b += &format!("{}\n", question_marker(name, issue, turn));
    // As $(q_render) has it: without its last line ends.
    b.trim_end_matches('\n').to_string()
}

/// Whether `line` is the host's park marker of `issue`.
pub fn is_park_marker(line: &str, issue: u64) -> bool {
    Regex::new(&format!(
        r"^<!-- tehas:park issue={issue} step=[a-z0-9-]{{0,40}} kind=[a-z0-9-]{{0,40}} provider=[01] sig=[0-9a-f]{{64}} -->$"
    ))
    .unwrap()
    .is_match(line)
}

/// The marker that ends a question of `kind` ("park" or "plan").
pub fn question_marker(kind: &str, issue: u64, turn: u64) -> String {
    format!("<!-- tehas:question kind={kind} issue={issue} turn={turn} -->")
}

/// A question asked before 2.0, asked again in 2.0's words (#2782): what the
/// poll posts in its place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reasked {
    /// The new comment, or None when the old one offered nothing to decide
    /// (a stop with no retry), which 2.0 does not ask.
    pub body: Option<String>,
    /// The words of a stop's choices, as `question-asked --offers` takes them.
    pub offers: Vec<Choice>,
}

/// The question of before 2.0 in `old`, a comment of boxes, rendered as 2.0
/// asks it: its words above the boxes, its markers, and its boxes as the
/// choices they were. None when `old` holds no box, which no question of
/// before 2.0 lacks.
pub fn reasked(kind: Kind, issue: u64, turn: u64, old: &str) -> Option<Reasked> {
    let lines: Vec<&str> = old.lines().collect();
    let is_box =
        |l: &str| l.starts_with("- [") && matches!(l.get(3..6), Some(" ] " | "x] " | "X] "));
    let first = lines.iter().position(|l| is_box(l))?;
    let boxes: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_box(l))
        .map(|(i, l)| (i, &l[6..]))
        .collect();
    let mut above: Vec<&str> = lines[..first].to_vec();
    let marker = match above.first() {
        Some(l) if is_handoff_marker(l, issue) => {
            let m = l.to_string();
            above.remove(0);
            m
        }
        _ => String::new(),
    };
    let words = above.join("\n").trim().to_string();
    match kind {
        Kind::Park => {
            let labels: Vec<&str> = boxes.iter().map(|(_, l)| *l).collect();
            let retry = labels.iter().any(|l| retry_of(l).is_some());
            let comments = labels.iter().any(|l| retry_of(l) == Some(true));
            if !retry {
                return Some(Reasked {
                    body: None,
                    offers: Vec::new(),
                });
            }
            // The words of before 2.0 that asked for a tick ask for a reply.
            let word = if comments { "retry" } else { "retry-text" };
            let words = words
                .replace(
                    "Tick a retry box on this comment",
                    &format!("Reply `{word}` to this comment"),
                )
                .replace(
                    "tick a retry box on this comment",
                    &format!("reply `{word}` to this comment"),
                )
                .replace("then tick a box.", "then reply with one of the choices.");
            let park = lines
                .iter()
                .find(|l| is_park_marker(l, issue))
                .copied()
                .unwrap_or("");
            let out = serde_json::json!({"text": words, "question": null, "marker": marker,
                "park": park, "retry": true, "comments": comments});
            Some(Reasked {
                body: Some(q_render(Kind::Park, issue, turn, &out, "")),
                offers: park_offers(true, comments),
            })
        }
        _ => {
            // An option is its box and the indented lines under it; the last
            // box, the end of the session, is the host's own.
            let mut options = Vec::new();
            for (k, (at, label)) in boxes.iter().enumerate() {
                if k + 1 == boxes.len() && label.starts_with(BOX_END) {
                    break;
                }
                let desc: Vec<&str> = lines[at + 1..]
                    .iter()
                    .take_while(|l| l.starts_with("  ") && !is_box(l))
                    .map(|l| l.trim())
                    .collect();
                options.push(serde_json::json!({"label": label, "description": desc.join(" ")}));
            }
            let out = serde_json::json!({"question": {"question": words, "options": options}});
            Some(Reasked {
                body: Some(q_render(Kind::Plan, issue, turn, &out, "")),
                offers: Vec::new(),
            })
        }
    }
}

/// The line added once to a question of before 2.0 that was asked again as
/// the comment `id` on the same issue, or closed when it offered nothing to
/// decide (None).
pub fn superseded(id: Option<u64>) -> String {
    match id {
        Some(id) => format!("This question is asked again in the words of tehas 2.0: answer [the new comment](#issuecomment-{id}). A box ticked here does nothing."),
        None => format!(
            "This question is closed: tehas 2.0 asks nothing of a stop that offers no retry: {}.",
            crate::hold::HELD_PARKED_EDIT
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The park marker of #7 that the tests below render.
    fn park_marker_7() -> String {
        let sig = "a".repeat(64);
        format!("<!-- tehas:park issue=7 step=review kind= provider=0 sig={sig} -->")
    }

    /// A question of before 2.0 is asked again in 2.0's words: a stop with
    /// its markers, its words and the retries its boxes offered; a planning
    /// question with its options and their descriptions; a stop that
    /// offered no retry is not asked.
    #[test]
    fn a_question_of_boxes_is_asked_again_in_words() {
        let park = park_marker_7();
        let old = format!("<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\n- [ ] {OLD_BOX_CLEAN}\n- [x] {BOX_PARKED}\n\nTick one box.\n\n{park}\n{}", question_marker("park", 7, 1));
        let r = reasked(Kind::Park, 7, 1, &old).unwrap();
        let body = r.body.unwrap();
        assert!(body.starts_with("<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\nReply in a comment"), "{body}");
        assert!(body.contains("- retry: Retry with the issue and all its comments."));
        assert!(body.contains(&park));
        assert!(!body.contains("- ["), "{body}");
        assert_eq!(
            r.offers,
            vec![Choice::Retry, Choice::RetryText, Choice::Park]
        );
        // The phrases of before 2.0 that asked for a tick ask for a reply of
        // a choice the comment offers.
        for (said, says, label) in [
            ("To run it on classic instead, take `tehas:lean` off this issue, then tick a box.", "To run it on classic instead, take `tehas:lean` off this issue, then reply with one of the choices.", OLD_BOX_CLEAN),
            ("Make the change yourself, then tick a retry box on this comment.", "Make the change yourself, then reply `retry` to this comment.", OLD_BOX_CLEAN),
            ("Tick a retry box on this comment once it is right.", "Reply `retry-text` to this comment once it is right.", BOX_ALONE),
        ] {
            let was = old.replace("Tehas cycle stopped.", &format!("Tehas cycle stopped.\n\n{said}")).replace(OLD_BOX_CLEAN, label);
            let body = reasked(Kind::Park, 7, 1, &was).unwrap().body.unwrap();
            assert!(body.contains(says), "{body}");
            assert!(!body.to_lowercase().contains("tick a"), "{body}");
        }
        let alone = old.replace(OLD_BOX_CLEAN, BOX_ALONE);
        assert_eq!(
            reasked(Kind::Park, 7, 1, &alone).unwrap().offers,
            vec![Choice::RetryText, Choice::Park]
        );
        let none = old.replace(&format!("- [ ] {OLD_BOX_CLEAN}\n"), "");
        assert_eq!(
            reasked(Kind::Park, 7, 1, &none).unwrap(),
            Reasked {
                body: None,
                offers: vec![]
            }
        );
        let plan = format!("Adopt?\n\n- [ ] Adopt all\n  Apply all five\n- [ ] Change some\n- [ ] End this planning session\n\nTick one box.\n\n{}", question_marker("plan", 7, 2));
        let body = reasked(Kind::Plan, 7, 2, &plan).unwrap().body.unwrap();
        assert_eq!(
            listed(&body),
            Some(vec![
                "Adopt all".to_string(),
                "Change some".to_string(),
                BOX_END.to_string()
            ])
        );
        assert!(
            body.contains(
                "1. Adopt all: Apply all five\n2. Change some\n3. End this planning session"
            ),
            "{body}"
        );
        assert_eq!(reasked(Kind::Plan, 7, 2, "no boxes"), None);
    }

    #[test]
    fn no_shape_of_task_box_survives_q_clean() {
        for (agent, clean) in [
            ("- [ ] #1 first", "- ☐ #1 first"),
            ("* [x] done", "* ☑ done"),
            ("  + [X] done", "  + ☑ done"),
            ("1. [ ] first", "1. ☐ first"),
            ("12) [x] done", "12) ☑ done"),
            ("> - [ ] quoted", "> - ☐ quoted"),
            ("- - [ ] nested", "- - ☐ nested"),
            // Words in brackets stay: they are no box.
            ("- [HIGH] finding", "- [HIGH] finding"),
            ("see [ ] here", "see [ ] here"),
        ] {
            assert_eq!(q_clean(agent, 100), clean, "{agent}");
        }
    }

    #[test]
    fn q_clean_drops_what_a_reader_does_not_see() {
        // A hidden character between the parts of a word does not keep it whole.
        assert_eq!(q_clean("a\u{202e}b\u{1b}[1mc\u{feff}", 100), "ab[1mc");
        assert_eq!(q_clean("@\u{2060}alice", 100), "@\u{200b}alice");
        assert_eq!(q_clean("<\u{200f}!--", 100), r"<\!--");
        assert_eq!(q_clean("a\tb", 100), "a\tb");
    }

    #[test]
    fn a_planning_question_holds_no_text_of_the_plan() {
        let out = json!({"text": "- [ ] #1 sub\n- [HIGH] gap",
            "question": {"question": "Which?", "options": [{"label": "Update this issue"}]}});
        assert_eq!(
            q_render(Kind::Plan, 7, 2, &out, ""),
            format!("Which?\n\n{ASK_PLAN}\n\n1. Update this issue\n2. End this planning session: the session ends, and nothing more is planned.\n\n{ONLY_ALLOWED}\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->")
        );
        // A turn with nothing but text: the choices open the comment.
        let out = json!({"text": "Which branch?", "question": null});
        assert!(q_render(Kind::Plan, 7, 1, &out, "").starts_with(ASK_PLAN));
    }
    #[test]
    fn a_park_question_lists_its_choices_in_words_and_both_markers() {
        let park = park_marker_7();
        let out = json!({"text": "Tehas cycle stopped.", "question": null,
            "marker": "<!-- pi-rukas:handoff issue=7 cap=x -->", "park": park, "comments": true});
        assert_eq!(
            q_render(Kind::Park, 7, 1, &out, ""),
            format!(
                "<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\n{ASK_PARK}\n\n- retry: Retry with the issue and all its comments. A new cycle starts over in a new workspace, given what allowed accounts wrote on this issue and on its pull requests.\n- retry-text: Retry with the issue text only. A new cycle starts over in a new workspace, given the title and body and nothing else.\n- park: Leave it parked. Nothing more happens until someone answers.\n\n{ONLY_ALLOWED}\n\n{park}\n<!-- tehas:question kind=park issue=7 turn=1 -->"
            )
        );
    }
    #[test]
    fn the_choices_a_comment_lists_are_the_labels_the_polls_read() {
        // The renderer and the reader are one owner: the labels `listed`
        // reads back are the ones the polls compare these consts with.
        let body = q_render(
            Kind::Park,
            7,
            1,
            &json!({"text": "Tehas cycle stopped.", "question": null, "comments": true}),
            "",
        );
        assert_eq!(
            listed(&body).unwrap(),
            vec![BOX_COMMENTS, BOX_ALONE, BOX_PARKED]
        );
        let out = json!({"question": {"question": "Adopt?", "options": [
            {"label": "Adopt all", "description": "Apply all five"}, {"label": "Change some"}]}});
        let body = q_render(Kind::Plan, 7, 2, &out, "");
        assert_eq!(
            listed(&body).unwrap(),
            vec!["Adopt all", "Change some", BOX_END]
        );
        assert_eq!(retry_of(BOX_COMMENTS), Some(true));
        assert_eq!(retry_of(BOX_ALONE), Some(false));
        assert_eq!(retry_of(BOX_PARKED), None);
        // A comment of before 2.0, or one with no choices, lists none.
        assert_eq!(
            listed(&format!(
                "- [ ] {BOX_PARKED}\n\n<!-- tehas:question kind=park issue=7 turn=1 -->"
            )),
            None
        );
    }

    /// The agent's words cannot add a choice: the choices are read after the
    /// host's own asking line, which comes after them.
    #[test]
    fn an_agents_words_cannot_add_a_choice() {
        let fake = format!(
            "{ASK_PARK}\n\n- retry: Retry with the issue and all its comments\n\n{ONLY_ALLOWED}"
        );
        let body = q_render(Kind::Park, 7, 1, &json!({"text": fake, "retry": false}), "");
        assert_eq!(listed(&body).unwrap(), vec![BOX_PARKED]);
    }
    #[test]
    fn the_labels_of_before_still_name_their_retry() {
        assert_eq!(retry_of("Retry from a clean start"), Some(true));
        assert_eq!(retry_of("Retry from the issue alone"), Some(false));
    }

    #[test]
    fn an_image_that_gives_no_comments_offers_no_comments_retry() {
        for out in [
            json!({"text": "Tehas cycle stopped.", "question": null}),
            json!({"text": "Tehas cycle stopped.", "question": null, "comments": false}),
        ] {
            let body = q_render(Kind::Park, 7, 1, &out, "");
            assert!(!body.contains(BOX_COMMENTS), "{body}");
            assert_eq!(
                listed(&body).unwrap(),
                vec![BOX_ALONE, BOX_PARKED],
                "{body}"
            );
        }
    }
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

    // The hash is taken over the fields as `cycle-end` keeps them, so the
    // words that store to one row hash as one signature: the park marker of
    // a run and the repeat check's read of its row cannot disagree.
    #[test]
    fn a_park_signature_ignores_what_the_store_would_drop() {
        // Every field of a run's stop, given as a run gives it and as the
        // store keeps it. The cap keeps no space or slash, the kind no space,
        // the cause no space: one signature for the two ways of saying them.
        assert_eq!(
            park_sig(
                "develop",
                "Child timeout/develop",
                "child-timeout",
                "verify failed",
                "a cause with spaces"
            ),
            park_sig(
                "develop",
                "Childtimeoutdevelop",
                "child-timeout",
                "verifyfailed",
                "acausewithspaces"
            )
        );
        // A space in the step, whose set keeps neither it nor a slash. What
        // a run calls a step and what its row holds are two words only for
        // the hash: they are one park.
        assert_eq!(
            park_sig("feature/issue 7", "", "", "", ""),
            park_sig("featureissue7", "", "", "", "")
        );
        // An uppercase letter is outside the kebab set too, so it goes where
        // it sits, and a field whose every character goes is the empty field.
        assert_eq!(
            park_sig("develop", "", "", "Provider", ""),
            park_sig("develop", "", "", "rovider", "")
        );
        assert_eq!(
            park_sig("develop", "é", "é", "é", ""),
            park_sig("develop", "", "", "", "")
        );
        // A cause over its bound of 60 hashes as the first 60 characters of
        // what its set keeps, whatever follows them.
        let long = "0123456789".repeat(7);
        assert_eq!(long.len(), 70);
        assert_eq!(
            park_sig("develop", "", "", "", &long),
            park_sig("develop", "", "", "", &long[..60])
        );
        // The kept run is cut at its bound, so what is left out of a field
        // over it is what comes after the bound, not what comes before: the
        // long cause above and one a character shorter say the same park,
        // and one that lost its first character does not.
        assert_eq!(
            park_sig("develop", "", "", "", &long),
            park_sig("develop", "", "", "", &long[..69])
        );
        assert_ne!(
            park_sig("develop", "", "", "", &long),
            park_sig("develop", "", "", "", &long[1..])
        );
        // `cap` keeps `:` and `step` does not; each field is cut to its own
        // bound, so the same characters are two fields of two lengths.
        assert_ne!(
            park_sig("develop", "verify-failed:develop", "", "verify", ""),
            park_sig("develop", "verify-faileddevelop", "", "verify", "")
        );
        let caps = "C".repeat(40) + ":" + &"c".repeat(41);
        assert_eq!(
            park_sig("", &caps, "", "", ""),
            park_sig("", &("C".repeat(40) + ":" + &"c".repeat(40)), "", "", "")
        );
        let steps = "s".repeat(41);
        assert_eq!(
            park_sig(&steps, "", "", "", ""),
            park_sig(&"s".repeat(40), "", "", "", "")
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

    fn park(extra: Value) -> Value {
        let mut out = json!({"text": "Tehas cycle stopped.", "question": null});
        for (k, v) in extra.as_object().unwrap() {
            out[k] = v.clone();
        }
        out
    }

    fn boxes(body: &str) -> Vec<String> {
        listed(body).unwrap()
    }

    #[test]
    fn a_planning_question_lists_the_options_then_the_way_out_then_the_marker() {
        let out = json!({"text": "Two ways.", "question": {"question": "Adopt?", "options": [
            {"label": "Adopt all", "description": "Apply all\nfive"}, {"label": "Change some"}]}});
        let body = q_render(Kind::Plan, 7, 2, &out, "");
        assert!(!body.contains("Retry with"));
        assert!(!body.contains("Two ways."));
        assert_eq!(
            body,
            format!("Adopt?\n\n{ASK_PLAN}\n\n1. Adopt all: Apply all five\n2. Change some\n3. End this planning session: the session ends, and nothing more is planned.\n\n{ONLY_ALLOWED}\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->")
        );
    }
    #[test]
    fn a_turn_without_the_question_tool_still_offers_the_way_out() {
        let out = json!({"text": "Which branch should this target?", "question": null});
        let body = q_render(Kind::Plan, 7, 1, &out, "");
        assert_eq!(boxes(&body).len(), 1);
        assert!(!body.contains("Which branch should this target?"));
    }

    #[test]
    fn a_parked_cycle_offers_the_two_retries_and_leave_in_that_order() {
        let body = q_render(Kind::Park, 7, 1, &park(json!({"comments": true})), "");
        assert_eq!(boxes(&body), [BOX_COMMENTS, BOX_ALONE, BOX_PARKED]);
        assert!(body.contains("<!-- tehas:question kind=park issue=7 turn=1 -->"));
    }

    #[test]
    fn a_park_that_offers_no_retry_has_only_the_box_to_leave_it() {
        let body = q_render(
            Kind::Park,
            7,
            1,
            &park(json!({"retry": false, "comments": true})),
            "",
        );
        assert_eq!(boxes(&body), [BOX_PARKED]);
        assert!(!body.contains("new workspace"));
        let body = q_render(
            Kind::Park,
            7,
            1,
            &park(json!({"retry": true, "comments": true})),
            "",
        );
        assert_eq!(boxes(&body), [BOX_COMMENTS, BOX_ALONE, BOX_PARKED]);
    }

    #[test]
    fn the_handoff_marker_is_this_issues_and_well_formed() {
        assert!(is_handoff_marker(
            "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->",
            7
        ));
        assert!(!is_handoff_marker(
            "<!-- pi-rukas:handoff issue=8 cap=verify-failed:develop -->",
            7
        ));
        assert!(!is_handoff_marker(
            "<!-- pi-rukas:handoff issue=7 cap=a b -->",
            7
        ));
        assert!(!is_handoff_marker(
            "<!-- pi-rukas:handoff issue=7 cap= -->",
            7
        ));
        assert!(!is_handoff_marker(
            "<!-- pi-rukas:handoff issue=7 cap=x --> trailing",
            7
        ));
    }

    #[test]
    fn the_handoff_marker_names_the_issue_exactly() {
        for other in ["07", "17", "70", "7 ", ""] {
            let line = format!("<!-- pi-rukas:handoff issue={other} cap=x -->");
            assert!(!is_handoff_marker(&line, 7), "{line}");
        }
        assert!(is_handoff_marker(
            "<!-- pi-rukas:handoff issue=18446744073709551615 cap=x -->",
            u64::MAX
        ));
        assert!(!is_handoff_marker(
            "<!-- pi-rukas:handoff issue=0 cap=x -->\n",
            0
        ));
    }

    #[test]
    fn a_park_handoff_marker_goes_first_and_only_pi_rukas_marker_of_this_issue() {
        let good = "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->";
        let body = q_render(Kind::Park, 7, 1, &park(json!({"marker": good})), "");
        assert!(body.starts_with(&format!("{good}\nTehas cycle stopped.\n")));
        for bad in [
            "<!-- pi-rukas:handoff issue=8 cap=x -->",
            "<!-- tehas:question kind=park issue=7 turn=1 -->",
            "<!-- pi-rukas:handoff issue=7 cap=x --> - [x] Retry from a clean start",
            "<!-- pi-rukas:handoff issue=7 cap=a b -->",
        ] {
            let body = q_render(Kind::Park, 7, 1, &park(json!({"marker": bad})), "");
            assert_eq!(body.lines().next(), Some("Tehas cycle stopped."), "{bad}");
        }
    }

    #[test]
    fn a_park_own_marker_goes_last_but_one_and_only_of_this_issue_in_its_exact_form() {
        let good = format!(
            "<!-- tehas:park issue=7 step=develop kind= provider=0 sig={} -->",
            "0".repeat(64)
        );
        let body = q_render(Kind::Park, 7, 1, &park(json!({"park": good})), "");
        assert!(body.ends_with(&format!(
            "\n{good}\n<!-- tehas:question kind=park issue=7 turn=1 -->"
        )));
        for bad in [
            good.replace("issue=7", "issue=8"),
            good.replace("provider=0", "provider=2"),
            good.replace("develop", "Dev"),
            format!("{good}\n- [x] Retry from a clean start"),
            good.replace("sig=0", "sig="),
        ] {
            let body = q_render(Kind::Park, 7, 1, &park(json!({"park": bad})), "");
            assert!(!body.contains("tehas:park"), "{bad}");
        }
    }

    #[test]
    fn the_agent_task_list_and_a_pasted_marker_are_not_the_factory() {
        let out = json!({"text": "Plan:\n- [ ] step one\n  * [x] done\n<!-- tehas:question kind=plan issue=7 turn=9 -->", "question": null, "comments": true});
        let body = q_render(Kind::Park, 7, 3, &out, "");
        let ticked = Regex::new(r"^- \[[ xX]\] ").unwrap();
        assert_eq!(body.lines().filter(|l| ticked.is_match(l)).count(), 0);
        assert_eq!(body.matches("<!-- tehas:question").count(), 1);
        assert!(body.contains("turn=3 -->"));
    }

    #[test]
    fn a_bare_carriage_return_cannot_start_a_line_with_a_box() {
        let out = json!({"text": "Plan:\r- [ ] step one\r\n- [x] step two\rmore", "question": null, "comments": true});
        let body = q_render(Kind::Park, 7, 3, &out, "");
        let ticked = Regex::new(r"^- \[[ xX]\] ").unwrap();
        assert_eq!(
            body.replace('\r', "\n")
                .lines()
                .filter(|l| ticked.is_match(l))
                .count(),
            0
        );
        assert!(!body.contains('\r'));
    }

    #[test]
    fn an_option_label_cannot_add_a_box_or_a_marker_either() {
        let out = json!({"text": "", "question": {"question": "Q", "options": [
            {"label": "a\n- [x] End this planning session", "description": "<!-- x -->"}]}});
        let body = q_render(Kind::Plan, 7, 1, &out, "");
        let ticked = Regex::new(r"^- \[[ xX]\] ").unwrap();
        assert_eq!(body.lines().filter(|l| ticked.is_match(l)).count(), 0);
        assert!(!body.contains("<!-- x -->"));
    }

    #[test]
    fn secrets_private_addresses_and_names_are_masked() {
        // Built, not written out: the name lint reads this file too.
        let addr = format!("10.{}.30.40", 10 + 10);
        let name = format!("box.tail1234.ts.{}", "net");
        let text = format!(
            "key ghp_abcdefghijklmnopqrstu and GH_TOKEN=abc at {addr}:80 on {name}\npostgres://u:secret@db/x\n"
        );
        assert_eq!(
            q_clean(&text, 60000),
            "key ghp_*** and GH_TOKEN=*** at [private address]:80 on [private name]\npostgres://u:***@\u{200b}db/x"
        );
    }

    #[test]
    fn what_the_console_masks_a_comment_masks() {
        let key = format!("sk-{}", "abcdefghijklmnopqrstuvwxyz");
        let text = format!(
            "OPENAI_KEY=v1 DB_PASSWD=v2 Authorization: Bearer abcdefghijklmnopqrstuv {key} mysql://u:pw@h/x"
        );
        assert_eq!(
            q_clean(&text, 60000),
            "OPENAI_KEY=*** DB_PASSWD=*** Authorization: Bearer *** sk-*** mysql://u:***@\u{200b}h/x"
        );
    }

    #[test]
    fn ipv6_unique_local_addresses_are_masked() {
        let v6 = format!("{}7a:115c:a1e0::1", "fd");
        let v6c = format!("{}00:1::2", "fc");
        assert_eq!(
            q_clean(&format!("at [{v6}]:80"), 60000),
            "at [[private address]]:80"
        );
        assert_eq!(q_clean(&v6.to_uppercase(), 60000), "[private address]");
        assert_eq!(
            q_clean(&format!("from {v6}, then {v6c}."), 60000),
            "from [private address], then [private address]."
        );
        assert_eq!(q_clean(&v6, 60000), "[private address]");
    }

    #[test]
    fn look_alikes_of_ipv6_unique_local_addresses_are_left_alone() {
        let v6 = format!("{}7a:115c:a1e0::1", "fd");
        for s in [
            format!("0{v6}"),
            format!("a:{v6}"),
            "fd12:30".to_string(),
            "2001:db8::1".to_string(),
            "fdisk and fdatasync".to_string(),
            "feed:beef:cafe::1".to_string(),
        ] {
            assert_eq!(q_clean(&s, 60000), s);
        }
    }

    #[test]
    fn a_public_address_is_left_alone() {
        assert_eq!(
            q_clean("see 192.0.2.7 and 8.8.8.8", 60000),
            "see 192.0.2.7 and 8.8.8.8"
        );
    }

    #[test]
    fn a_mention_notifies_nobody() {
        assert_eq!(
            q_clean("ask @alice, mail a@b.c", 60000),
            "ask @\u{200b}alice, mail a@\u{200b}b.c"
        );
    }

    #[test]
    fn a_long_message_is_cut_where_the_caller_asks() {
        let long = "a".repeat(70000);
        assert_eq!(q_clean(&long, 60000).chars().count(), 60000);
        assert_eq!(q_clean(&long, 300).chars().count(), 300);
    }

    fn big(options: usize) -> Value {
        let l = "l".repeat(1000);
        let d = "d".repeat(2000);
        let options: Vec<Value> = (0..options)
            .map(|i| json!({"label": format!("{i} {l}"), "description": d}))
            .collect();
        json!({"text": "m".repeat(70000),
            "question": {"question": "q".repeat(10000), "options": options}})
    }

    // GitHub refuses a comment over 65 536 characters.
    #[test]
    fn a_question_with_far_too_much_in_it_still_fits_in_a_comment() {
        let body = q_render(Kind::Plan, 7, 4, &big(30), "");
        assert!(body.len() < 65000);
        assert_eq!(boxes(&body).len(), 21);
        assert_eq!(
            body.lines()
                .filter(|l| *l == "Options left out: 10. A reply can name another choice.")
                .count(),
            1
        );
        assert_eq!(
            body.lines().last(),
            Some("<!-- tehas:question kind=plan issue=7 turn=4 -->")
        );
        assert_eq!(
            body.lines()
                .filter(|l| l.starts_with("21. End this planning session"))
                .count(),
            1
        );
    }

    #[test]
    fn each_part_has_its_own_limit_message_question_label_description() {
        let all = |c: char| move |l: &&str| !l.is_empty() && l.chars().all(|x| x == c);
        let parked = q_render(Kind::Park, 7, 4, &big(30), "");
        assert_eq!(parked.lines().find(all('m')).map(str::len), Some(40000));
        let body = q_render(Kind::Plan, 7, 4, &big(30), "");
        assert!(!body.contains("mmm"));
        assert_eq!(body.lines().find(all('q')).map(str::len), Some(4000));
        // The line of an option: its number, its label cut at 300, then its
        // description cut at 600.
        let line = body.lines().find(|l| l.starts_with("1. 0 ")).unwrap();
        let (label, desc) = line[3..].split_once(": ").unwrap();
        assert_eq!(label.len(), 300);
        assert_eq!(desc.len(), 600);
    }

    #[test]
    fn the_options_kept_are_the_first_twenty_in_order() {
        let body = q_render(Kind::Plan, 7, 4, &big(30), "");
        let kept: Vec<String> = boxes(&body)
            .iter()
            .filter_map(|l| l.split_once(" l").map(|(n, _)| n.to_string()))
            .collect();
        let want: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        assert_eq!(kept, want);
    }

    #[test]
    fn twenty_options_are_all_kept_and_nothing_is_said_to_be_left_out() {
        let body = q_render(Kind::Plan, 7, 4, &big(20), "");
        assert_eq!(boxes(&body).len(), 21);
        assert!(!body.contains("left out"));
    }

    /// #2782: no comment the factory renders holds a task-list line, for
    /// every kind of question and every set of choices it offers, whatever
    /// the agent wrote into it. This is the check that fails if one comes
    /// back.
    #[test]
    fn no_rendered_question_holds_a_task_list_line() {
        let task =
            Regex::new(r"(?m)^[[:space:]>]*(?:[-*+]|[0-9]+[.)])[[:space:]]+\[[ xX]\]").unwrap();
        let sly = "Plan:\n- [ ] step\n* [x] done\n1. [ ] one\n> - [X] quoted";
        let mut bodies = Vec::new();
        for retry in [true, false] {
            for comments in [true, false] {
                let out = json!({"text": sly, "question": {"question": sly, "options": [{"label": sly, "description": sly}]},
                    "retry": retry, "comments": comments});
                bodies.push(q_render(Kind::Park, 7, 1, &out, "The host's line."));
            }
        }
        for out in [
            json!({"text": sly, "question": null}),
            json!({"text": sly, "question": {"question": sly, "options": []}}),
            json!({"text": sly, "question": {"question": sly, "options": (0..25).map(|i| json!({"label": format!("{i} {sly}"), "description": sly})).collect::<Vec<_>>()}}),
        ] {
            bodies.push(q_render(Kind::Plan, 7, 2, &out, "Filed in this turn: #9"));
        }
        for b in &bodies {
            assert!(!task.is_match(b), "a task-list line in:\n{b}");
            assert!(listed(b).is_some(), "no choices listed in:\n{b}");
        }
    }
}
