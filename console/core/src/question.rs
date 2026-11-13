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

/// What the retries are given, said under the boxes.
pub const SAY_COMMENTS: &str = "Both retries start over in a new workspace. \"All its comments\" means what\nallowed accounts wrote on this issue and on its pull requests up to your\ntick. \"Issue text only\" means the title and body, nothing else.";
pub const SAY_ALONE: &str = "The retry starts over in a new workspace. \"Issue text only\" means the\ntitle and body, nothing else.";

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
/// retry of it waits while the issue carries `tehas:lean`.
pub const LEAN_REFUSED: &str = "lean-refused";

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
    let whole = format!("{}
", crate::text::shown(&crate::mask::mask(text))).replace('', "");
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

/// A text of the outcome, a string or nothing, as `.k // ""` reads it.
fn s<'a>(v: &'a Value, path: &str) -> &'a str {
    v.pointer(path).and_then(Value::as_str).unwrap_or("")
}

/// q_render: the body of a question comment. `out` holds the text, the
/// question, pi-rukas's handoff marker (`marker`) and, for a park, tehas's
/// park marker (`park`); `line` is the host's own, put between the question
/// and the boxes. A planning turn's text is left out: the caller says it in
/// a comment before this one, so nothing of a long plan can move a box.
pub fn q_render(kind: Kind, issue: u64, turn: u64, out: &Value, line: &str) -> String {
    let mut b = String::new();
    let handoff = Regex::new(&format!(
        r"^<!-- pi-rukas:handoff issue={issue} cap=[A-Za-z0-9_.:-]{{1,80}} -->$"
    ))
    .unwrap();
    if kind == Kind::Park {
        let marker = s(out, "/marker");
        if handoff.is_match(marker) {
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
            // A develop timeout that repeats on the same text offers no retry,
            // and no box offers comments the image would not give the cycle.
            let retry = out.get("retry") != Some(&Value::Bool(false));
            let comments = out.get("comments") == Some(&Value::Bool(true));
            if retry && comments {
                b += &format!("{}\n", box_line(BOX_COMMENTS, false));
            }
            if retry {
                b += &format!("{}\n", box_line(BOX_ALONE, false));
            }
            b += "- [ ] Leave it parked\n\n";
            match (retry, comments) {
                (true, true) => b += &format!("{SAY_COMMENTS}\n"),
                (true, false) => b += &format!("{SAY_ALONE}\n"),
                _ => {}
            }
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
            "Which?\n\n- [ ] Update this issue\n- [ ] End this planning session\n\nTick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->"
        );
        // A turn with nothing but text: the boxes open the comment.
        let out = json!({"text": "Which branch?", "question": null});
        assert!(q_render(Kind::Plan, 7, 1, &out, "").starts_with("- [ ] End this planning session"));
    }

    #[test]
    fn a_park_question_has_the_retry_boxes_and_both_markers() {
        let sig = "a".repeat(64);
        let park = format!("<!-- tehas:park issue=7 step=review kind= provider=0 sig={sig} -->");
        let out = json!({"text": "Tehas cycle stopped.", "question": null,
            "marker": "<!-- pi-rukas:handoff issue=7 cap=x -->", "park": park, "comments": true});
        assert_eq!(
            q_render(Kind::Park, 7, 1, &out, ""),
            format!(
                "<!-- pi-rukas:handoff issue=7 cap=x -->\nTehas cycle stopped.\n\n- [ ] Retry with the issue and all its comments\n- [ ] Retry with the issue text only\n- [ ] Leave it parked\n\nBoth retries start over in a new workspace. \"All its comments\" means what\nallowed accounts wrote on this issue and on its pull requests up to your\ntick. \"Issue text only\" means the title and body, nothing else.\nTick one box. Only a tick from an account that may authorise work counts.\n\n{park}\n<!-- tehas:question kind=park issue=7 turn=1 -->"
            )
        );
    }

    #[test]
    fn the_retry_boxes_are_the_lines_the_polls_read() {
        // The renderer and the readers are one string: a ticked box of a
        // rendered body is what each poll reads, and the other stays unticked.
        for label in [BOX_COMMENTS, BOX_ALONE] {
            assert_eq!(box_line(label, false), format!("- [ ] {label}"));
            assert_eq!(box_line(label, true), format!("- [x] {label}"));
        }
        let body = q_render(
            Kind::Park,
            7,
            1,
            &json!({"text": "Tehas cycle stopped.", "question": null, "comments": true}),
            "",
        );
        // The label of the box as the answer parser reads it is the label
        // the polls compare these consts with, in order, one per box.
        let labels: Vec<String> = body
            .split('\n')
            .filter(|l| l.starts_with("- [ "))
            .map(|l| l[6..].to_string())
            .collect();
        assert_eq!(labels, vec![BOX_COMMENTS, BOX_ALONE, "Leave it parked"]);
        assert_eq!(retry_of(BOX_COMMENTS), Some(true));
        assert_eq!(retry_of(BOX_ALONE), Some(false));
        assert_eq!(retry_of("Leave it parked"), None);
    }

    #[test]
    fn the_labels_of_before_still_name_their_retry() {
        assert_eq!(retry_of("Retry from a clean start"), Some(true));
        assert_eq!(retry_of("Retry from the issue alone"), Some(false));
    }

    #[test]
    fn an_image_that_gives_no_comments_gets_no_comments_box() {
        for out in [
            json!({"text": "Tehas cycle stopped.", "question": null}),
            json!({"text": "Tehas cycle stopped.", "question": null, "comments": false}),
        ] {
            let body = q_render(Kind::Park, 7, 1, &out, "");
            assert!(!body.contains(BOX_COMMENTS), "{body}");
            assert!(!body.contains("All its comments"), "{body}");
            assert!(
                body.contains(&format!(
                    "- [ ] {BOX_ALONE}\n- [ ] Leave it parked\n\n{SAY_ALONE}\nTick one box."
                )),
                "{body}"
            );
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

    fn park(extra: Value) -> Value {
        let mut out = json!({"text": "Tehas cycle stopped.", "question": null});
        for (k, v) in extra.as_object().unwrap() {
            out[k] = v.clone();
        }
        out
    }

    fn boxes(body: &str) -> Vec<&str> {
        body.lines().filter(|l| l.starts_with("- [ ] ")).collect()
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
            "Adopt?\n\n- [ ] Adopt all\n  Apply all five\n- [ ] Change some\n- [ ] End this planning session\n\nTick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->"
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
        assert_eq!(
            boxes(&body),
            [
                "- [ ] Retry with the issue and all its comments",
                "- [ ] Retry with the issue text only",
                "- [ ] Leave it parked"
            ]
        );
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
        assert_eq!(boxes(&body), ["- [ ] Leave it parked"]);
        assert!(!body.contains("new workspace"));
        let body = q_render(
            Kind::Park,
            7,
            1,
            &park(json!({"retry": true, "comments": true})),
            "",
        );
        assert!(body.contains("- [ ] Retry with the issue and all its comments"));
        assert!(body.contains("- [ ] Retry with the issue text only"));
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
        assert_eq!(body.lines().filter(|l| ticked.is_match(l)).count(), 3);
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
            3
        );
        assert!(!body.contains('\r'));
    }

    #[test]
    fn an_option_label_cannot_add_a_box_or_a_marker_either() {
        let out = json!({"text": "", "question": {"question": "Q", "options": [
            {"label": "a\n- [x] End this planning session", "description": "<!-- x -->"}]}});
        let body = q_render(Kind::Plan, 7, 1, &out, "");
        let ticked = Regex::new(r"^- \[[ xX]\] ").unwrap();
        assert_eq!(body.lines().filter(|l| ticked.is_match(l)).count(), 2);
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
                .filter(|l| *l == "- [ ] End this planning session")
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
        let label = body.lines().find(|l| l.starts_with("- [ ] 0 ")).unwrap();
        assert_eq!(label.len(), 306);
        let desc = body
            .lines()
            .find(|l| l.len() > 2 && l.starts_with("  ") && l[2..].chars().all(|x| x == 'd'))
            .unwrap();
        assert_eq!(desc.len(), 602);
    }

    #[test]
    fn the_options_kept_are_the_first_twenty_in_order() {
        let body = q_render(Kind::Plan, 7, 4, &big(30), "");
        let kept: Vec<String> = boxes(&body)
            .iter()
            .filter_map(|l| l[6..].split_once(" l").map(|(n, _)| n.to_string()))
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
}
