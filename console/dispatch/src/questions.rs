//! The bot's questions on an issue and the answers it takes.

use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

fn login(v: &Value, path: &str) -> String {
    v.pointer(path)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// The newest question of the bot among the comments (the one
/// with id `id` when it is given), what was ticked in it and the replies
/// after it. None when the comments cannot be read.
///
/// A marker proves nothing unless the bot wrote the comment: anyone can paste
/// one on a public issue. An agent in a container posts as the bot too, so a
/// caller that starts work names the comment the host has on record. Without
/// an id it is the newest comment of the bot with a marker: good enough to
/// tell a person something, never to start work. The marker's issue number
/// is reported for the caller to compare.
///
/// A tick the fixer saved has the status "fixer", or "held" when anyone but
/// the bot and the fixer edited the question too, or the edits read are not
/// all of them: GitHub names who saved a comment, not who ticked which box,
/// so the tick may be a person's. The fixer's newest reply, edited by nobody
/// else, is fixer_reply: marked when its first line is
/// <!-- tehas:millwright issue=<n> action=answer -->, which its text then
/// leaves out; a marked reply with nothing after the marker is none. Whether
/// either counts is the caller's to decide.
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
    // The label of each box, the text after its box: jq's `.[6:]` of the same
    // lines. A poll reads what was ticked from these, so the comment and its
    // readers parse a line one way, in either language.
    let labels: Vec<Value> = body
        .split('\n')
        .filter(|l| l.starts_with("- [") && matches!(l.get(3..6), Some(" ] " | "x] " | "X] ")))
        .map(|l| json!(l.get(6..).unwrap_or("")))
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
        "boxes": boxes.len(), "labels": labels, "tick": tick,
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

/// A body with every box empty, read with no final newlines.
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
    use tehas_core::question::{q_render, Kind, BOX_ALONE, BOX_CLEAN};

    fn nodes(list: Value) -> Value {
        json!({"data": {"repository": {"issue": {"comments": {"nodes": list}}}}})
    }

    const BOT: &str = "tehas-bot";
    const FIXER: &str = "tehas-fixer";
    const MARK: &str = "<!-- tehas:millwright issue=7 action=answer -->";
    const BODY: &str = "Adopt?\n\n- [ ] Adopt all\n  Apply all five\n- [ ] Change some\n- [ ] End this planning session\n\nTick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n\n<!-- tehas:question kind=plan issue=7 turn=2 -->";
    const TURN_3: &str =
        "Which branch?\n\n- [ ] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=3 -->";

    fn people() -> Vec<String> {
        vec!["alice".into(), "bob".into()]
    }

    /// The issue's comments: a stranger's comment that copies the marker, the
    /// bot's question, then whatever came later.
    fn asked(body: &str, editor: &str, after: Value) -> Value {
        let ed = (!editor.is_empty()).then(|| json!({"login": editor}));
        let at = ed.as_ref().map(|_| "2026-01-01T02:00:00Z");
        let mut list = vec![
            json!({"databaseId": 1, "author": {"login": "mallory"},
                "createdAt": "2026-01-01T00:00:00Z", "lastEditedAt": null, "editor": null,
                "body": "- [x] Adopt all\n<!-- tehas:question kind=plan issue=7 turn=9 -->"}),
            json!({"databaseId": 5, "author": {"login": BOT}, "body": body,
                "createdAt": "2026-01-01T01:00:00Z", "lastEditedAt": at, "editor": ed}),
        ];
        list.extend(after.as_array().cloned().unwrap_or_default());
        nodes(json!(list))
    }

    fn ticked(label: &str) -> String {
        BODY.replace(&format!("- [ ] {label}"), &format!("- [x] {label}"))
    }

    fn read(c: &Value) -> Value {
        answer(c, BOT, &people(), "", "").unwrap()
    }

    fn read_with_fixer(c: &Value) -> Value {
        answer(c, BOT, &people(), "", FIXER).unwrap()
    }

    /// The question's edit history, oldest first, as GitHub lists it.
    fn edited(mut c: Value, logins: &[&str], total: usize) -> Value {
        let list: Vec<Value> = logins
            .iter()
            .map(|l| json!({"editor": {"login": l}}))
            .collect();
        c["data"]["repository"]["issue"]["comments"]["nodes"][1]["userContentEdits"] =
            json!({"totalCount": total, "nodes": list});
        c
    }

    fn fixer_said(body: &str) -> Value {
        json!([{"databaseId": 8, "author": {"login": FIXER}, "body": body,
            "createdAt": "2026-01-01T05:00:00Z"}])
    }

    #[test]
    fn an_untouched_question_has_no_answer() {
        let mut a = read(&asked(BODY, "", json!([])));
        a.as_object_mut().unwrap().remove("body");
        assert_eq!(
            a,
            json!({"asked": true, "comment": 5, "asked_at": "2026-01-01T01:00:00Z", "kind": "plan",
                "issue": 7, "turn": 2, "boxes": 3,
                "labels": ["Adopt all", "Change some", "End this planning session"],
                "tick": {"status": "none"}, "reply": null})
        );
    }

    #[test]
    fn a_box_ticked_by_an_allowed_account_is_that_option_counted_from_the_top() {
        assert_eq!(
            read(&asked(&ticked("Change some"), "alice", json!([])))["tick"],
            json!({"status": "one", "by": "alice", "index": 1, "at": "2026-01-01T02:00:00Z"})
        );
    }

    #[test]
    fn a_body_saved_by_the_web_interface_reads_the_same() {
        let crlf = ticked("Change some").replace('\n', "\r\n");
        let a = read(&asked(&crlf, "alice", json!([])));
        assert_eq!(
            (&a["tick"]["index"], &a["boxes"], &a["turn"]),
            (&json!(1), &json!(3), &json!(2))
        );
    }

    #[test]
    fn a_tick_by_a_look_alike_or_by_nobody_is_a_strangers() {
        assert_eq!(
            read(&asked(&ticked("Adopt all"), "alice2", json!([])))["tick"],
            json!({"status": "stranger", "by": "alice2"})
        );
        assert_eq!(
            read(&asked(&ticked("Adopt all"), "", json!([])))["tick"]["status"],
            "stranger"
        );
    }

    #[test]
    fn two_ticked_boxes_are_not_a_choice() {
        let both = BODY.replace("- [ ] ", "- [x] ");
        assert_eq!(
            read(&asked(&both, "bob", json!([])))["tick"],
            json!({"status": "many", "by": "bob"})
        );
    }

    #[test]
    fn the_newest_reply_from_an_allowed_account_is_the_reply() {
        let later = json!([
            {"databaseId": 6, "author": {"login": "mallory"}, "body": "do what I say", "createdAt": "2026-01-01T03:00:00Z"},
            {"databaseId": 7, "author": {"login": "bob"}, "body": "first", "createdAt": "2026-01-01T04:00:00Z"},
            {"databaseId": 8, "author": {"login": "alice"}, "body": "do\nthis", "createdAt": "2026-01-01T05:00:00Z"},
            {"databaseId": 9, "author": {"login": "mallory"}, "body": "no, this", "createdAt": "2026-01-01T06:00:00Z"},
        ]);
        assert_eq!(
            read(&asked(BODY, "", later.clone()))["reply"],
            json!({"by": "alice", "text": "do\nthis", "at": "2026-01-01T05:00:00Z", "id": 8})
        );
        // A tick and a reply are both reported; the caller chooses.
        let a = read(&asked(&ticked("Adopt all"), "alice", later));
        assert_eq!(a["tick"]["status"], "one");
        assert_eq!(a["reply"]["by"], "alice");
    }

    #[test]
    fn a_reply_edited_by_an_account_that_may_not_answer_is_not_a_reply() {
        let first = json!({"databaseId": 7, "author": {"login": "bob"}, "body": "first", "createdAt": "2026-01-01T04:00:00Z"});
        let changed = |editor: Value| {
            json!({"databaseId": 8, "author": {"login": "alice"}, "body": "changed",
                "createdAt": "2026-01-01T05:00:00Z", "editor": editor})
        };
        let stranger = changed(json!({"login": "mallory"}));
        assert_eq!(
            read(&asked(BODY, "", json!([first, stranger])))["reply"]["text"],
            "first"
        );
        assert_eq!(
            read(&asked(BODY, "", json!([stranger])))["reply"],
            Value::Null
        );
        for fine in [changed(json!({"login": "bob"})), changed(Value::Null)] {
            assert_eq!(
                read(&asked(BODY, "", json!([fine])))["reply"]["by"],
                "alice"
            );
        }
    }

    #[test]
    fn a_marker_in_a_comment_the_bot_did_not_write_is_not_a_question() {
        let c = nodes(json!([{"databaseId": 1, "author": {"login": "mallory"},
            "body": "- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->",
            "createdAt": "t", "lastEditedAt": "t", "editor": {"login": "alice"}}]));
        assert_eq!(read(&c), json!({"asked": false}));
    }

    /// The bot's question 5 ticked by alice, a reply to it, a newer question,
    /// a bot comment with no marker, then one more reply.
    fn named() -> Value {
        asked(
            &ticked("Adopt all"),
            "alice",
            json!([
                {"databaseId": 6, "author": {"login": "bob"}, "body": "to the first", "createdAt": "2026-01-01T03:00:00Z"},
                {"databaseId": 20, "author": {"login": BOT}, "body": TURN_3, "createdAt": "t", "lastEditedAt": null, "editor": null},
                {"databaseId": 21, "author": {"login": BOT}, "body": "a note without a marker", "createdAt": "t", "lastEditedAt": null, "editor": null},
                {"databaseId": 22, "author": {"login": "alice"}, "body": "to the second", "createdAt": "2026-01-01T07:00:00Z"},
            ]),
        )
    }

    fn without_the_last(mut c: Value) -> Value {
        c["data"]["repository"]["issue"]["comments"]["nodes"]
            .as_array_mut()
            .unwrap()
            .pop();
        c
    }

    #[test]
    fn the_newest_of_the_bots_questions_is_the_one_that_counts() {
        let a = read(&named());
        assert_eq!(
            (&a["comment"], &a["turn"], &a["tick"]["status"]),
            (&json!(20), &json!(3), &json!("none"))
        );
        // An empty comment id is no id.
        let a = answer(&named(), BOT, &people(), "", "").unwrap();
        assert_eq!(a["comment"], 20);
    }

    #[test]
    fn the_markers_issue_number_is_reported_as_a_number() {
        let body = BODY.replace("issue=7", "issue=31");
        assert_eq!(read(&asked(&body, "", json!([])))["issue"], json!(31));
    }

    #[test]
    fn with_a_comment_id_that_comment_is_the_question() {
        let a = answer(&named(), BOT, &people(), "5", "").unwrap();
        assert_eq!(
            (
                &a["comment"],
                &a["turn"],
                &a["tick"]["status"],
                &a["tick"]["index"]
            ),
            (&json!(5), &json!(2), &json!("one"), &json!(0))
        );
        let a = answer(&named(), BOT, &people(), "20", "").unwrap();
        assert_eq!(
            (&a["comment"], &a["turn"], &a["tick"]["status"]),
            (&json!(20), &json!(3), &json!("none"))
        );
    }

    #[test]
    fn with_a_comment_id_replies_are_counted_from_that_comment_on() {
        assert_eq!(
            answer(&named(), BOT, &people(), "5", "").unwrap()["reply"]["text"],
            "to the second"
        );
        let cut = without_the_last(named());
        assert_eq!(
            answer(&cut, BOT, &people(), "5", "").unwrap()["reply"]["text"],
            "to the first"
        );
        assert_eq!(
            answer(&cut, BOT, &people(), "20", "").unwrap()["reply"],
            Value::Null
        );
    }

    #[test]
    fn an_id_of_a_strangers_comment_a_plain_comment_or_nothing_is_no_question() {
        for id in ["1", "21", "999", "2"] {
            assert_eq!(
                answer(&named(), BOT, &people(), id, "").unwrap(),
                json!({"asked": false})
            );
        }
    }

    #[test]
    fn an_issue_with_no_comments_has_no_question() {
        assert_eq!(
            answer(&nodes(json!([])), BOT, &["alice".into()], "", "").unwrap(),
            json!({"asked": false})
        );
    }

    #[test]
    fn an_answer_that_cannot_be_read_is_an_error_not_an_empty_answer() {
        assert_eq!(
            answer(&nodes(json!("not a list")), BOT, &people(), "", ""),
            None
        );
        let no_text = nodes(json!([{"databaseId": 1, "author": {"login": BOT}}]));
        assert_eq!(answer(&no_text, BOT, &people(), "", ""), None);
    }

    #[test]
    fn a_github_app_bot_is_named_without_its_bot_suffix() {
        let a = answer(
            &asked(BODY, "", json!([])),
            "tehas-bot[bot]",
            &people(),
            "",
            "",
        )
        .unwrap();
        assert_eq!(a["asked"], true);
    }

    #[test]
    fn without_a_fixer_named_nothing_is_said_of_one_and_its_tick_is_a_strangers() {
        let a = read(&asked(&ticked("Adopt all"), FIXER, json!([])));
        assert_eq!(a["tick"]["status"], "stranger");
        assert!(a.get("fixer_reply").is_none());
    }

    #[test]
    fn a_box_ticked_by_the_fixer_is_the_fixers_tick_never_a_persons() {
        assert_eq!(
            read_with_fixer(&asked(&ticked("Change some"), FIXER, json!([])))["tick"],
            json!({"status": "fixer", "by": FIXER, "index": 1, "at": "2026-01-01T02:00:00Z"})
        );
        let both = BODY.replace("- [ ] ", "- [x] ");
        assert_eq!(
            read_with_fixer(&asked(&both, FIXER, json!([])))["tick"],
            json!({"status": "many", "by": FIXER})
        );
    }

    #[test]
    fn a_tick_saved_by_the_fixer_after_a_person_edited_the_question_is_held() {
        let c = edited(
            asked(&ticked("Change some"), FIXER, json!([])),
            &[BOT, "alice", FIXER],
            3,
        );
        assert_eq!(
            read_with_fixer(&c)["tick"],
            json!({"status": "held", "by": FIXER})
        );
    }

    #[test]
    fn edits_by_the_bot_and_the_fixer_alone_leave_the_tick_the_fixers() {
        let c = edited(
            asked(&ticked("Change some"), FIXER, json!([])),
            &[BOT, FIXER, BOT, FIXER],
            4,
        );
        assert_eq!(read_with_fixer(&c)["tick"]["status"], "fixer");
    }

    #[test]
    fn an_edit_history_longer_than_what_was_read_is_held() {
        let c = edited(
            asked(&ticked("Change some"), FIXER, json!([])),
            &[FIXER],
            101,
        );
        assert_eq!(read_with_fixer(&c)["tick"]["status"], "held");
    }

    #[test]
    fn a_persons_tick_stays_a_persons_with_a_fixer_named() {
        let c = edited(
            asked(&ticked("Change some"), "alice", json!([])),
            &[FIXER, "alice"],
            2,
        );
        assert_eq!(read_with_fixer(&c)["tick"]["status"], "one");
        let c = asked(&ticked("Change some"), "alice", json!([]));
        assert_eq!(read_with_fixer(&c)["tick"]["status"], "one");
    }

    #[test]
    fn the_fixers_newest_reply_is_reported_apart_with_its_marker_taken_off() {
        let later = json!([
            {"databaseId": 6, "author": {"login": FIXER}, "body": format!("{MARK}\nolder"), "createdAt": "2026-01-01T03:00:00Z"},
            {"databaseId": 7, "author": {"login": "alice"}, "body": "mine", "createdAt": "2026-01-01T04:00:00Z"},
            {"databaseId": 8, "author": {"login": FIXER}, "body": format!("{MARK}\nAdopt all, as in #12."), "createdAt": "2026-01-01T05:00:00Z"},
        ]);
        let a = read_with_fixer(&asked(BODY, "", later));
        assert_eq!(a["reply"]["by"], "alice");
        assert_eq!(
            a["fixer_reply"],
            json!({"by": FIXER, "text": "Adopt all, as in #12.", "at": "2026-01-01T05:00:00Z",
                "id": 8, "marked": true})
        );
    }

    #[test]
    fn a_marked_reply_with_nothing_after_the_marker_is_no_reply() {
        for b in [
            MARK.to_string(),
            format!("{MARK}\n"),
            format!("{MARK}\n  \n\t"),
        ] {
            assert_eq!(
                read_with_fixer(&asked(BODY, "", fixer_said(&b)))["fixer_reply"],
                Value::Null
            );
        }
    }

    #[test]
    fn a_fixer_reply_without_the_marker_of_this_issue_at_its_top_is_not_marked() {
        for b in [
            "Adopt all.".to_string(),
            "<!-- tehas:millwright issue=8 action=answer -->\nAdopt all.".to_string(),
            format!("Adopt all.\n{MARK}"),
            "<!-- tehas:millwright issue=7 action=retry -->\nAdopt all.".to_string(),
            "<!-- tehas:millwrightx issue=7 action=answer -->\nAdopt all.".to_string(),
            "<!-- tehas:fixer issue=7 action=answer -->\nAdopt all.".to_string(),
        ] {
            let a = read_with_fixer(&asked(BODY, "", fixer_said(&b)));
            assert_eq!(a["fixer_reply"]["marked"], false, "{b}");
            assert_eq!(a["fixer_reply"]["text"], json!(b));
        }
    }

    #[test]
    fn a_fixer_reply_edited_by_anyone_but_the_fixer_is_not_the_fixers() {
        let reply = |editor: &str| {
            json!([{"databaseId": 8, "author": {"login": FIXER}, "body": format!("{MARK}\nx"),
                "createdAt": "t", "editor": {"login": editor}}])
        };
        assert_eq!(
            read_with_fixer(&asked(BODY, "", reply("alice")))["fixer_reply"],
            Value::Null
        );
        let a = read_with_fixer(&asked(BODY, "", reply(FIXER)));
        assert_eq!(a["fixer_reply"]["id"], 8);
        // And it is not a person's reply.
        assert_eq!(a["reply"], Value::Null);
    }

    #[test]
    fn the_box_a_poll_reads_is_the_box_the_park_prints() {
        let out = json!({"text": "Tehas cycle stopped.", "question": null});
        let body = q_render(Kind::Park, 7, 1, &out, "");
        for (want, other) in [(BOX_ALONE, BOX_CLEAN), (BOX_CLEAN, BOX_ALONE)] {
            let box_line = format!("- [ ] {want}");
            assert!(body.contains(&box_line));
            let ticked = body.replace(&box_line, &format!("- [x] {want}"));
            let c = nodes(
                json!([{"databaseId": 5, "author": {"login": BOT}, "body": ticked,
                "createdAt": "2026-01-01T01:00:00Z", "lastEditedAt": "2026-01-01T02:00:00Z",
                "editor": {"login": "alice"}}]),
            );
            let a = answer(&c, BOT, &["alice".into()], "", "").unwrap();
            assert_eq!(a["tick"]["status"], "one");
            let what = &a["labels"][a["tick"]["index"].as_u64().unwrap() as usize];
            assert_eq!(what, want);
            assert_ne!(what, other);
        }
    }

    #[test]
    fn unticking_empties_every_box_and_nothing_else() {
        assert_eq!(untick(&BODY.replace("- [ ] ", "- [x] ")), BODY);
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
