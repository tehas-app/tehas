//! The bot's questions on an issue and the answers it takes.

use crate::snapshot::{Account, Comment, Comments, Given};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

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
/// so the tick may be a person's. It is "unread" when the question's edit
/// history could not be read this poll: the rules wait for the next poll
/// without acting. The fixer's newest reply, edited by nobody
/// else, is fixer_reply, and only a comment whose first line is
/// <!-- tehas:millwright issue=<n> action=answer --> is a reply: the fixer is
/// the bot, so every note the host writes is the fixer's comment too, and none
/// starts with that line. Its text leaves the marker out; a reply with nothing
/// after the marker is none. Whether either counts is the caller's to decide.
///
/// The bot is the account with `bot_id`, its user id on record, when one is:
/// an id survives a rename of the app and the vouch of an unrecorded login,
/// a login neither. With no id on record it is the login `bot`. When the
/// comment `id` is there but another account wrote it, the answer is
/// `{"asked": false, "written_by": <login>}`.
pub fn answer(
    comments: &Value,
    bot: &str,
    bot_id: Option<u64>,
    allowed: &[String],
    id: &str,
    fixer: &str,
) -> Option<Value> {
    let c = Comments::parse(comments);
    let a = answer_of(c.or_none()?, bot, bot_id, allowed, id, fixer)?;
    let q = match a {
        Answer::NotAsked {
            written_by: Some(by),
        } => return Some(json!({"asked": false, "written_by": by})),
        Answer::NotAsked { written_by: None } => return Some(json!({"asked": false})),
        Answer::Asked(q) => q,
    };
    // jq's tonumber: digits too many for an integer are a float, never an
    // error.
    let num = |d: &str| -> Value {
        d.parse::<u64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!(d.parse::<f64>().unwrap_or(f64::MAX)))
    };
    let t = &q.tick;
    let tick = match t.status {
        Status::None => json!({"status": "none"}),
        Status::Fixer | Status::One => {
            json!({"status": t.status.word(), "by": t.by, "index": t.index, "at": t.at})
        }
        _ => json!({"status": t.status.word(), "by": t.by}),
    };
    let mut out = json!({
        "asked": true, "comment": Given::json(q.comment.as_ref()),
        "asked_at": q.asked_at,
        "body": q.body, "kind": q.kind, "issue": num(&q.issue),
        "turn": num(&q.turn),
        "boxes": q.boxes, "labels": q.labels, "tick": tick,
        "reply": q.reply.map(|r| json!({"by": r.by, "text": Given::json(r.text.as_ref()),
            "at": Given::json(r.at.as_ref()), "id": Given::json(r.id.as_ref())})),
    });
    if !fixer.is_empty() {
        out["fixer_reply"] = match q.fixer_reply {
            None => Value::Null,
            Some(f) => json!({"by": f.by, "text": f.text, "at": Given::json(f.at.as_ref()),
                "id": Given::json(f.id.as_ref()), "marked": true}),
        };
    }
    Some(out)
}

/// What `answer` reads of the comments.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    /// No question of the bot's is there; `written_by` names who wrote the
    /// comment with the id asked for, when another account did.
    NotAsked {
        written_by: Option<String>,
    },
    Asked(Box<Asked>),
}

/// The bot's question and what was answered to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    /// The question's databaseId as GitHub gave it.
    pub comment: Option<Given>,
    /// createdAt, empty when it is not text.
    pub asked_at: String,
    pub body: String,
    pub kind: String,
    /// The marker's issue and turn, as digits.
    pub issue: String,
    pub turn: String,
    pub boxes: usize,
    /// The label of each box.
    pub labels: Vec<String>,
    pub tick: Tick,
    pub reply: Option<Reply>,
    /// None also when no fixer was named.
    pub fixer_reply: Option<FixerReply>,
}

impl Asked {
    /// The marker's issue as a number, None when it is too big for one.
    pub fn issue_number(&self) -> Option<u64> {
        self.issue.parse().ok()
    }

    /// The label of the ticked box, when one was ticked.
    pub fn ticked_label(&self) -> Option<&str> {
        self.labels.get(self.tick.index?).map(String::as_str)
    }
}

/// What was ticked in the question.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub status: Status,
    /// Who saved the question, empty for no tick.
    pub by: String,
    /// The box ticked, counted from the top: for One and Fixer.
    pub index: Option<usize>,
    /// When the question was saved, for One and Fixer; empty otherwise.
    pub at: String,
    /// The tick is an answer of `tehas answer`, written in by
    /// `answer::overlay`.
    pub answered: bool,
}

impl Tick {
    /// No box ticked.
    pub fn none() -> Tick {
        Tick {
            status: Status::None,
            by: String::new(),
            index: None,
            at: String::new(),
            answered: false,
        }
    }
}

/// The kinds of tick `answer` tells apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    None,
    Unread,
    Held,
    Fixer,
    Stranger,
    Many,
    One,
}

impl Status {
    /// The status as the answer writes it.
    pub fn word(self) -> &'static str {
        match self {
            Status::None => "none",
            Status::Unread => "unread",
            Status::Held => "held",
            Status::Fixer => "fixer",
            Status::Stranger => "stranger",
            Status::Many => "many",
            Status::One => "one",
        }
    }
}

/// The newest reply of an allowed account, its values as GitHub gave them.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub by: String,
    pub text: Option<Given>,
    pub at: Option<Given>,
    pub id: Option<Given>,
}

/// The fixer's marked reply: its text leaves the marker out.
#[derive(Clone, Debug, PartialEq)]
pub struct FixerReply {
    pub by: String,
    pub text: String,
    pub at: Option<Given>,
    pub id: Option<Given>,
}

/// `answer` of comments already read.
pub fn answer_of(
    c: &[Comment],
    bot: &str,
    bot_id: Option<u64>,
    allowed: &[String],
    id: &str,
    fixer: &str,
) -> Option<Answer> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"<!-- tehas:question kind=(?<kind>[a-z]+) issue=(?<issue>[0-9]+) turn=(?<turn>[0-9]+) -->").unwrap()
    });
    let bot = bot.strip_suffix("[bot]").unwrap_or(bot);
    let listed = |l: &str| allowed.iter().any(|a| a == l);
    let bots = |a: &Account| match bot_id {
        Some(i) => a.id == Some(i),
        None => a.login == bot,
    };
    let mut q = None;
    let mut stranger = None;
    for (k, x) in c.iter().enumerate() {
        if !bots(&x.author) {
            if !id.is_empty()
                && x.id.as_ref().and_then(Given::number).map(|d| d.to_string())
                    == Some(id.to_string())
            {
                stranger = Some(x.author.login.clone());
            }
            continue;
        }
        // A bot comment with no text is an answer jq cannot read.
        let body = x.body_text()?;
        let did = Given::said(x.id.as_ref());
        if re.is_match(body) && (id.is_empty() || did == id) {
            q = Some(k);
        }
    }
    let Some(k) = q else {
        return Some(Answer::NotAsked {
            written_by: stranger,
        });
    };
    let qv = &c[k];
    let body = qv.body_text().unwrap_or("");
    let m = re.captures(body)?;
    let boxes: Vec<bool> = body
        .split('\n')
        .filter(|l| l.starts_with("- [") && matches!(l.get(3..6), Some(" ] " | "x] " | "X] ")))
        .map(|l| matches!(l.get(3..4), Some("x" | "X")))
        .collect();
    // The label of each box, the text after its box: jq's `.[6:]` of the same
    // lines. A poll reads what was ticked from these, so the comment and its
    // readers parse a line one way, in either language.
    let labels: Vec<String> = body
        .split('\n')
        .filter(|l| l.starts_with("- [") && matches!(l.get(3..6), Some(" ] " | "x] " | "X] ")))
        .map(|l| l.get(6..).unwrap_or("").to_string())
        .collect();
    let ticks: Vec<usize> = boxes
        .iter()
        .enumerate()
        .filter(|(_, t)| **t)
        .map(|(i, _)| i)
        .collect();
    let ed = qv.editor_login().to_string();
    let edited_at = qv.last_edited_at.as_str();
    let after = &c[k + 1..];
    let reply = after.iter().rfind(|x| {
        let e = x.editor_login();
        listed(&x.author.login) && (e.is_empty() || listed(e))
    });
    let saved = |status: Status| Tick {
        status,
        by: ed.clone(),
        index: None,
        at: String::new(),
        answered: false,
    };
    let chose = |status: Status| Tick {
        status,
        by: ed.clone(),
        index: Some(ticks[0]),
        at: edited_at.to_string(),
        answered: false,
    };
    let tick = if ticks.is_empty() {
        Tick::none()
    } else if !fixer.is_empty() && ed == fixer && ticks.len() == 1 {
        let h = &qv.history;
        // A history whose read failed says nothing of who saved the question:
        // the tick is "unread", and the rules wait for the next poll without
        // acting. The poll's read
        // gives every comment a history: read, none for an unedited one, or
        // failed.
        if h.failed {
            saved(Status::Unread)
        } else if h.total > h.editors.len() as f64
            || h.editors.iter().any(|e| e.login != fixer && !bots(e))
        {
            saved(Status::Held)
        } else {
            chose(Status::Fixer)
        }
    } else if !listed(&ed) && (fixer.is_empty() || ed != fixer) {
        saved(Status::Stranger)
    } else if ticks.len() > 1 {
        saved(Status::Many)
    } else {
        chose(Status::One)
    };
    let issue = &m["issue"];
    let fixer_reply = if fixer.is_empty() {
        None
    } else {
        let mark = format!("<!-- tehas:millwright issue={issue} action=answer -->");
        let marked = |b: &str| {
            let first = b.split('\n').next().unwrap_or("");
            first.strip_suffix('\r').unwrap_or(first) == mark
        };
        after
            .iter()
            .rfind(|x| {
                let e = x.editor_login();
                x.author.login == fixer
                    && (e.is_empty() || e == fixer)
                    && marked(x.body_text().unwrap_or(""))
            })
            .and_then(|f| {
                let b = f.body_text().unwrap_or("");
                let text = b.split_once('\n').map_or("", |(_, rest)| rest);
                (!text.trim().is_empty()).then(|| FixerReply {
                    by: f.author.login.clone(),
                    text: text.to_string(),
                    at: f.created_at.clone(),
                    id: f.id.clone(),
                })
            })
    };
    Some(Answer::Asked(Box::new(Asked {
        comment: qv.id.clone(),
        asked_at: qv.created_text().unwrap_or("").to_string(),
        body: body.to_string(),
        kind: m["kind"].to_string(),
        issue: issue.to_string(),
        turn: m["turn"].to_string(),
        boxes: boxes.len(),
        labels,
        tick,
        reply: reply.map(|r| Reply {
            by: r.author.login.clone(),
            text: r.body.clone(),
            at: r.created_at.clone(),
            id: r.id.clone(),
        }),
        fixer_reply,
    })))
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
    use tehas_core::question::{q_render, Kind, BOX_ALONE, BOX_COMMENTS};

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
        answer(c, BOT, None, &people(), "", "").unwrap()
    }

    fn read_with_fixer(c: &Value) -> Value {
        answer(c, BOT, None, &people(), "", FIXER).unwrap()
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

    /// A comment by `login` with its body, its time and its id as given.
    macro_rules! said {
        ($login:expr, $body:expr, $at:expr, $id:expr) => {
            json!({"databaseId": $id, "author": {"login": $login}, "body": $body, "createdAt": $at})
        };
    }

    fn fixer_said(body: &str) -> Value {
        json!([said!(FIXER, body, "2026-01-01T05:00:00Z", 8)])
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
        let a = answer(&named(), BOT, None, &people(), "", "").unwrap();
        assert_eq!(a["comment"], 20);
    }

    #[test]
    fn the_markers_issue_number_is_reported_as_a_number() {
        let body = BODY.replace("issue=7", "issue=31");
        assert_eq!(read(&asked(&body, "", json!([])))["issue"], json!(31));
    }

    #[test]
    fn with_a_comment_id_that_comment_is_the_question() {
        let a = answer(&named(), BOT, None, &people(), "5", "").unwrap();
        assert_eq!(
            (
                &a["comment"],
                &a["turn"],
                &a["tick"]["status"],
                &a["tick"]["index"]
            ),
            (&json!(5), &json!(2), &json!("one"), &json!(0))
        );
        let a = answer(&named(), BOT, None, &people(), "20", "").unwrap();
        assert_eq!(
            (&a["comment"], &a["turn"], &a["tick"]["status"]),
            (&json!(20), &json!(3), &json!("none"))
        );
    }

    #[test]
    fn with_a_comment_id_replies_are_counted_from_that_comment_on() {
        assert_eq!(
            answer(&named(), BOT, None, &people(), "5", "").unwrap()["reply"]["text"],
            "to the second"
        );
        let cut = without_the_last(named());
        assert_eq!(
            answer(&cut, BOT, None, &people(), "5", "").unwrap()["reply"]["text"],
            "to the first"
        );
        assert_eq!(
            answer(&cut, BOT, None, &people(), "20", "").unwrap()["reply"],
            Value::Null
        );
    }

    #[test]
    fn an_id_of_a_strangers_comment_a_plain_comment_or_nothing_is_no_question() {
        for id in ["21", "999", "2"] {
            assert_eq!(
                answer(&named(), BOT, None, &people(), id, "").unwrap(),
                json!({"asked": false})
            );
        }
        assert_eq!(
            answer(&named(), BOT, None, &people(), "1", "").unwrap(),
            json!({"asked": false, "written_by": "mallory"})
        );
    }

    /// The bot's question 5 after the app was renamed: GraphQL names its
    /// author by the new slug, and the settings still by the old.
    fn renamed(author: &str, id: u64) -> Value {
        let mut c = asked(&ticked("Adopt all"), "alice", json!([]));
        c["data"]["repository"]["issue"]["comments"]["nodes"][1]["author"] =
            json!({"login": author, "databaseId": id});
        c
    }

    #[test]
    fn with_the_bots_id_on_record_its_question_is_found_under_a_new_login() {
        for author in ["acme-new", "acme-new#4242"] {
            let c = renamed(author, 4242);
            let a = answer(&c, "acme-old[bot]", Some(4242), &people(), "5", "").unwrap();
            assert_eq!(
                (&a["asked"], &a["comment"], &a["tick"]["status"]),
                (&json!(true), &json!(5), &json!("one")),
                "{author}"
            );
            assert_eq!(
                answer(&c, "acme-old[bot]", None, &people(), "5", "").unwrap(),
                json!({"asked": false, "written_by": author})
            );
        }
    }

    #[test]
    fn with_the_bots_id_on_record_its_login_alone_is_not_the_bot() {
        let c = renamed(BOT, 7);
        assert_eq!(
            answer(&c, BOT, Some(4242), &people(), "5", "").unwrap(),
            json!({"asked": false, "written_by": BOT})
        );
        // The stranger's comment with a marker is no question either way.
        assert_eq!(
            answer(&c, BOT, Some(4242), &people(), "", "").unwrap(),
            json!({"asked": false})
        );
    }

    #[test]
    fn edits_by_the_bot_under_a_new_login_leave_the_tick_the_fixers() {
        let mut c = edited(asked(&ticked("Change some"), FIXER, json!([])), &[FIXER], 2);
        let edits = &mut c["data"]["repository"]["issue"]["comments"]["nodes"][1]
            ["userContentEdits"]["nodes"];
        edits.as_array_mut().unwrap().insert(
            0,
            json!({"editor": {"login": "acme-new", "databaseId": 4242}}),
        );
        c["data"]["repository"]["issue"]["comments"]["nodes"][1]["author"] =
            json!({"login": "acme-new", "databaseId": 4242});
        let a = answer(&c, "acme-old", Some(4242), &people(), "", FIXER).unwrap();
        assert_eq!(a["tick"]["status"], "fixer");
    }

    #[test]
    fn an_issue_with_no_comments_has_no_question() {
        assert_eq!(
            answer(&nodes(json!([])), BOT, None, &["alice".into()], "", "").unwrap(),
            json!({"asked": false})
        );
    }

    #[test]
    fn an_answer_that_cannot_be_read_is_an_error_not_an_empty_answer() {
        assert_eq!(
            answer(&nodes(json!("not a list")), BOT, None, &people(), "", ""),
            None
        );
        let no_text = nodes(json!([{"databaseId": 1, "author": {"login": BOT}}]));
        assert_eq!(answer(&no_text, BOT, None, &people(), "", ""), None);
    }

    #[test]
    fn a_github_app_bot_is_named_without_its_bot_suffix() {
        let a = answer(
            &asked(BODY, "", json!([])),
            "tehas-bot[bot]",
            None,
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
    fn a_tick_the_fixer_saved_whose_history_read_failed_is_unread() {
        // A history whose read failed says neither that only the fixer saved
        // the question nor that someone else did: the tick waits, unread.
        let mut c = asked(&ticked("Change some"), FIXER, json!([]));
        c["data"]["repository"]["issue"]["comments"]["nodes"][1]["userContentEdits"] =
            json!({"failed": true, "line": "ERROR: gh api graphql failed"});
        assert_eq!(
            read_with_fixer(&c)["tick"],
            json!({"status": "unread", "by": FIXER})
        );
        c["data"]["repository"]["issue"]["comments"]["nodes"][1]["userContentEdits"] =
            json!({"failed": true});
        assert_eq!(read_with_fixer(&c)["tick"]["status"], "unread");
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
    fn a_fixer_comment_without_the_marker_of_this_issue_at_its_top_is_no_reply() {
        for b in [
            "Adopt all.".to_string(),
            "<!-- tehas:millwright issue=8 action=answer -->\nAdopt all.".to_string(),
            format!("Adopt all.\n{MARK}"),
            "<!-- tehas:millwright issue=7 action=retry -->\nAdopt all.".to_string(),
            "<!-- tehas:millwrightx issue=7 action=answer -->\nAdopt all.".to_string(),
            "<!-- tehas:fixer issue=7 action=answer -->\nAdopt all.".to_string(),
        ] {
            let a = read_with_fixer(&asked(BODY, "", fixer_said(&b)));
            assert_eq!(a["fixer_reply"], Value::Null, "{b}");
        }
    }

    #[test]
    fn a_host_note_after_the_fixers_reply_leaves_the_reply_as_it_was() {
        let later = json!([
            {"databaseId": 8, "author": {"login": FIXER}, "body": format!("{MARK}\nAdopt all."), "createdAt": "2026-01-01T05:00:00Z"},
            {"databaseId": 9, "author": {"login": FIXER}, "body": "Tehas did not take the fixer's reply as an answer.", "createdAt": "2026-01-01T06:00:00Z"},
        ]);
        let a = read_with_fixer(&asked(BODY, "", later));
        assert_eq!(a["fixer_reply"]["id"], 8);
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
        let out = json!({"text": "Tehas cycle stopped.", "question": null, "comments": true});
        let body = q_render(Kind::Park, 7, 1, &out, "");
        for (want, other) in [(BOX_ALONE, BOX_COMMENTS), (BOX_COMMENTS, BOX_ALONE)] {
            let box_line = format!("- [ ] {want}");
            assert!(body.contains(&box_line));
            let ticked = body.replace(&box_line, &format!("- [x] {want}"));
            let c = nodes(
                json!([{"databaseId": 5, "author": {"login": BOT}, "body": ticked,
                "createdAt": "2026-01-01T01:00:00Z", "lastEditedAt": "2026-01-01T02:00:00Z",
                "editor": {"login": "alice"}}]),
            );
            let a = answer(&c, BOT, None, &["alice".into()], "", "").unwrap();
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
        let a = answer(&c, "tehas-bot", None, &["alice".into()], "", "").unwrap();
        assert_eq!(a["kind"], "park");
        assert_eq!(a["boxes"], 2);
        assert_eq!(
            a["tick"],
            json!({"status": "one", "by": "alice", "index": 0, "at": "2026-10-01T01:00:00Z"})
        );
        let b = answer(&c, "tehas-bot", None, &["bob".into()], "", "").unwrap();
        assert_eq!(b["tick"]["status"], "stranger");
        assert_eq!(
            answer(&c, "tehas-bot", None, &[], "99", "").unwrap(),
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

    #[test]
    fn ids_texts_and_stamps_of_other_types_pass_through_as_given() {
        let c = nodes(json!([
            {"author": {"login": BOT}, "databaseId": true, "body": TURN_3},
            said!("alice", 7, 12345, json!({"x": 1})),
            said!(FIXER, format!("{MARK}\nyes"), [1], false),
        ]));
        let a = answer(&c, BOT, None, &people(), "", FIXER).unwrap();
        assert_eq!(a["comment"], json!(true));
        assert_eq!(
            a["reply"],
            json!({"by": "alice", "text": 7, "at": 12345, "id": {"x": 1}})
        );
        assert_eq!(a["fixer_reply"]["at"], json!([1]));
        assert_eq!(a["fixer_reply"]["id"], json!(false));
    }

    #[test]
    fn a_question_id_of_another_type_is_matched_by_its_json() {
        let c = nodes(json!([{"author": {"login": BOT}, "databaseId": true, "body": TURN_3}]));
        let read = |id| answer(&c, BOT, None, &people(), id, "").unwrap()["asked"].clone();
        assert_eq!(read("true"), json!(true));
        assert_eq!(read("null"), json!(false));
    }
}
