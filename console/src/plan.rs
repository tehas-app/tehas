//! What a planning turn says, for libexec/plan: the runner's answer shaped
//! as the dispatcher posts it, and a drafted plan the operator chose to make
//! the issue's own text. The host's side of a turn is src/plan/turn.rs.
//!
//!   console plan update <session> <keep>
//!       the issue's {title, body} on standard input, as gh issue view gives
//!       them, and the draft in <keep>/draft.json.
//!   console plan descriptor <brief> <issue>
//!       the one line the first turn of libexec/plan plans, read from <brief>
//!       (what plan_brief writes: `# <title>`, a blank line, then the body)
//!       and the issue's number. See descriptor for how the line is made.
//!
//! update prints the --out document of libexec/plan, descriptor its one line.
//! Exit 0 printed, 1 nothing to print (why on standard error), 2 a usage
//! error.
//!
//! `outcome` shapes the runner's answer. A drafted plan (exit 12) is kept in
//! <keep>/draft.json for update. Only a refine session offers to file it as a
//! new issue. A plan session's draft with no open points is marked `clear`:
//! the dispatcher decides whether it needs the tick its question asks for.

use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use tehas_core::plan::{clip, made, text_of, APPLY_READY, CLIP};

mod turn;

/// pi-rukas keeps a plan's title to 72 characters in all, its prefix included,
/// and its longest prefix, `research: `, is 10 of them.
const TITLE_BUDGET: usize = 62;
/// Below this many words pi-rukas asks questions instead of planning, unless
/// the descriptor carries a code name, which tehas cannot know it has.
const MIN_WORDS: usize = 6;
/// The longest suffix tehas adds: ` (issue #n)` with a five-digit number.
const MAX_SUFFIX: usize = 13;
/// What a turn is asked to plan when the brief holds no title at all.
const NO_TITLE: &str = "the work in .tehas/brief.md";
const DRAFTED: &str = "Tick Update this issue to make the plan, proposals included, this issue's text, or reply to change it.";
const DRAFTED_REFINE: &str = "Tick Update this issue to make the plan, proposals included, this issue's text, File as a new issue to file it as its own issue, or reply to change it.";
const DUPLICATE: &str = "pi-rukas thinks this plan duplicates open work. Reply to say why it does not, or end the session.";

pub fn main(given: &[String]) -> u8 {
    if let Some(code) = turn::verb(given) {
        return code;
    }
    let mut input = String::new();
    let done = match given {
        [verb, session, keep] if verb == "update" => {
            let _ = std::io::stdin().read_to_string(&mut input);
            update(session, Path::new(keep), &input)
        }
        [verb, brief, issue] if verb == "descriptor" => {
            if issue.is_empty() || !issue.bytes().all(|b| b.is_ascii_digit()) {
                return usage();
            }
            match std::fs::read_to_string(brief) {
                Ok(text) => Ok(descriptor(&text, issue)),
                Err(_) => return usage(),
            }
        }
        _ => return usage(),
    };
    match done {
        Ok(said) => {
            println!("{said}");
            0
        }
        Err(why) => {
            eprintln!("console plan: {why}");
            1
        }
    }
}

fn usage() -> u8 {
    eprintln!(
        "console plan: usage: update <session> <keep> | descriptor <brief> <issue> | workspace|give|place|turn|run ..."
    );
    2
}

fn filed_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let name = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    };
    match rest.split('/').collect::<Vec<_>>()[..] {
        [owner, repo, "issues", n] => {
            name(owner) && name(repo) && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
        }
        _ => false,
    }
}

/// The --out document, its keys in the order libexec/plan always wrote them.
fn document(session: &str, text: &str, question: &Value, filed: &[String], more: &str) -> String {
    let mut out = format!(
        "{{\"session\":{},\"text\":{},\"question\":{}",
        json!(session),
        json!(text),
        question
    );
    if !filed.is_empty() {
        out += &format!(",\"filed\":{}", json!(filed));
    }
    out + more + "}"
}

/// A drafted plan needs no tick only when the planner lists no open points in
/// so many words and the issue can hold the plan whole. No list is not an
/// empty list, and an entry that is no question is still left open.
fn clear(d: &Value, spec: &str) -> bool {
    d.get("questions")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
        && !spec.trim().is_empty()
        && spec.chars().count() <= CLIP
}

pub fn outcome(
    rc: i64,
    session: &str,
    keep: &Path,
    doc: &str,
    refine: bool,
) -> Result<String, String> {
    let d: Value = serde_json::from_str(doc)
        .map_err(|_| "the runner's answer could not be read".to_string())?;
    if !d.is_object() {
        return Err("the runner's answer could not be read".into());
    }
    let qs: Vec<&Value> = d
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|q| q.is_object())
        .collect();
    let asked: Vec<String> = qs
        .iter()
        .map(|q| {
            let resolution = text_of(q.get("resolution"));
            let mut s = text_of(q.get("question"));
            if !resolution.is_empty() {
                s += &format!("\n   Proposed: {resolution}");
            }
            s
        })
        .collect();
    let numbered = asked
        .iter()
        .enumerate()
        .map(|(i, a)| format!("{}. {a}", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    let said = text_of(d.get("text"));
    let ask = |q: String| json!({"question": q, "options": []});
    let mut filed = Vec::new();
    let mut more = "";
    let (text, question) = match rc {
        0 => {
            let url = d.get("url").filter(|u| !u.is_null()).or(d.get("issueUrl"));
            if let Some(u) = url.and_then(Value::as_str).filter(|u| filed_ok(u)) {
                filed.push(u.to_string());
            }
            let title = text_of(d.get("title"));
            let named = if title.is_empty() {
                String::new()
            } else {
                format!(": {title}")
            };
            (format!("Filed {}{named}.", text_of(url)), Value::Null)
        }
        12 => {
            let title = match d.get("title") {
                None | Some(Value::Null) | Some(Value::Bool(false)) => "The plan".to_string(),
                t => text_of(t),
            };
            let spec = text_of(d.get("spec"));
            let draft = json!({"title": text_of(d.get("title")), "spec": spec});
            std::fs::write(keep.join("draft.json"), format!("{draft}\n"))
                .map_err(|_| "the drafted plan could not be kept".to_string())?;
            let open = if qs.is_empty() {
                String::new()
            } else {
                format!("Open points, with the planner's proposals:\n\n{numbered}\n\n")
            };
            let mut options = vec![
                json!({"label": "Update this issue", "description": "make the plan above the text of this issue"}),
            ];
            let tick = if refine {
                options.push(json!({"label": "File as a new issue", "description": "file the plan above as one new issue"}));
                DRAFTED_REFINE
            } else {
                if clear(&d, &spec) {
                    more = ",\"clear\":true";
                }
                DRAFTED
            };
            (
                format!("{said}\n\n### {title}\n\n{}", clip(&spec, "the plan")),
                json!({"question": open + tick, "options": options}),
            )
        }
        11 if qs.len() == 1 => (said, ask(asked[0].clone())),
        11 if qs.len() > 1 => (
            said,
            ask(format!(
                "The planner asked {} questions. Answer each by its number in a reply.\n\n{numbered}",
                qs.len()
            )),
        ),
        10 => (said, ask(DUPLICATE.into())),
        _ => (said, Value::Null),
    };
    if text.is_empty() && question.is_null() && filed.is_empty() {
        return Err("the turn said and asked nothing".into());
    }
    Ok(document(session, &text, &question, &filed, more))
}

/// The first line of the brief, which is its `# ` heading, without the marker.
fn heading(brief: &str) -> Option<&str> {
    let line = brief.lines().next()?.trim_start_matches('\u{feff}');
    let rest = line.strip_prefix('#')?;
    Some(if rest.starts_with([' ', '\t']) {
        rest.trim_start_matches([' ', '\t'])
    } else {
        rest
    })
}

/// The text of the first line when it names the issue outright, as a drafted
/// plan does: `Title: <text>`. A `Title:` later in the body is the plan's own
/// text and names nothing.
fn named(brief: &str) -> Option<&str> {
    let line = brief.lines().next()?.trim_start_matches('\u{feff}').trim();
    let rest = line.strip_prefix("Title:")?;
    Some(if rest.starts_with([' ', '\t']) {
        rest.trim_start_matches([' ', '\t'])
    } else {
        rest
    })
}

/// Words as a shell counts them: runs of characters between spaces.
fn words(s: &str) -> usize {
    s.split_whitespace().count()
}

/// What the first turn of libexec/plan plans, on one line: the issue's title,
/// or the drafted plan's own `Title:` line when the brief opens with one, so a
/// half of a split is named for its own work and not its parent's. The issue's
/// number goes on only when the name is too short for pi-rukas to plan from
/// (MIN_WORDS) and still fits what pi-rukas keeps of a title (TITLE_BUDGET).
pub fn descriptor(brief: &str, issue: &str) -> String {
    let named = named(brief).unwrap_or("").trim();
    let heading = heading(brief).unwrap_or("").trim();
    let said = if !named.is_empty() {
        named
    } else if !heading.is_empty() {
        heading
    } else {
        NO_TITLE
    };
    let suffix = format!(" (issue #{issue})");
    // Sized off the longest suffix and not this one: a short number here must
    // not let a long one through when the same brief names a later issue.
    let room = said.chars().count() + MAX_SUFFIX <= TITLE_BUDGET;
    if words(said) < MIN_WORDS && room {
        format!("{said}{suffix}")
    } else {
        said.to_string()
    }
}

/// The drafted plan as the issue's new text, and the comment that ends the
/// session with the text it replaces, folded and quoted.
pub fn update(session: &str, keep: &Path, was: &str) -> Result<String, String> {
    let was: Value = serde_json::from_str(was)
        .map_err(|_| "the text of the issue could not be read".to_string())?;
    let draft: Value = std::fs::read_to_string(keep.join("draft.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or("no drafted plan is kept")?;
    let plan = made(&draft, &was, APPLY_READY)?;
    let more = format!(
        ",\"update\":{}",
        json!({"title": plan.title, "body": plan.body})
    );
    Ok(document(session, &plan.text, &Value::Null, &[], &more))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keep() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "tehas-plan-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn parsed(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    const DRAFT: &str = r#"{"text":"Drafted.","title":"Fast","spec":"Do it.","questions":[{"question":"Both?","resolution":"Yes"}]}"#;

    #[test]
    fn a_draft_offers_to_update_the_issue_and_is_kept() {
        let k = keep();
        let out = outcome(12, "abc", &k, DRAFT, false).unwrap();
        let v = parsed(&out);
        assert_eq!(v["text"], "Drafted.\n\n### Fast\n\nDo it.");
        assert_eq!(
            v["question"]["question"],
            format!("Open points, with the planner's proposals:\n\n1. Both?\n   Proposed: Yes\n\n{DRAFTED}")
        );
        assert_eq!(
            v["question"]["options"],
            json!([{"label": "Update this issue", "description": "make the plan above the text of this issue"}])
        );
        assert_eq!(
            std::fs::read_to_string(k.join("draft.json")).unwrap(),
            "{\"spec\":\"Do it.\",\"title\":\"Fast\"}\n"
        );
        assert!(out.starts_with("{\"session\":\"abc\",\"text\":"));
    }

    const CLEAR: &str = r#"{"text":"Drafted.","title":"Fast","spec":"Do it.","questions":[]}"#;

    #[test]
    fn a_draft_with_no_open_points_is_clear_and_still_carries_its_question() {
        let k = keep();
        let v = parsed(&outcome(12, "abc", &k, CLEAR, false).unwrap());
        assert_eq!(v["clear"], true);
        // What the dispatcher posts when the start is not allowed: today's.
        assert_eq!(v["question"]["question"], DRAFTED);
        assert_eq!(
            std::fs::read_to_string(k.join("draft.json")).unwrap(),
            "{\"spec\":\"Do it.\",\"title\":\"Fast\"}\n"
        );
    }

    #[test]
    fn only_a_plan_session_that_says_it_has_no_open_points_is_clear() {
        let k = keep();
        let not = |doc: &str, refine: bool| {
            assert!(
                parsed(&outcome(12, "abc", &k, doc, refine).unwrap())
                    .get("clear")
                    .is_none(),
                "{doc}"
            )
        };
        not(DRAFT, false);
        not(CLEAR, true);
        // No list is not an empty list, and an entry that is no question is
        // still something the planner left open.
        not(r#"{"title":"Fast","spec":"Do it."}"#, false);
        not(
            r#"{"title":"Fast","spec":"Do it.","questions":null}"#,
            false,
        );
        not(r#"{"title":"Fast","spec":"Do it.","questions":[7]}"#, false);
        // Nothing to write, and a plan the issue could not hold whole.
        not(
            r#"{"text":"t","title":"Fast","spec":" \n","questions":[]}"#,
            false,
        );
        let big = json!({"title": "Big", "spec": "x".repeat(40_001), "questions": []});
        not(&big.to_string(), false);
    }

    #[test]
    fn only_a_drafted_plan_is_clear() {
        let k = keep();
        for rc in [0, 10, 11] {
            let v = parsed(&outcome(rc, "abc", &k, CLEAR, false).unwrap());
            assert!(v.get("clear").is_none(), "{rc}");
        }
    }

    #[test]
    fn a_refine_draft_also_offers_to_file_a_new_issue() {
        let k = keep();
        let v = parsed(&outcome(12, "abc", &k, DRAFT, true).unwrap());
        assert!(v["question"]["question"]
            .as_str()
            .unwrap()
            .ends_with(DRAFTED_REFINE));
        let labels: Vec<&str> = v["question"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, ["Update this issue", "File as a new issue"]);
    }

    #[test]
    fn a_filed_issue_is_named_and_a_bad_address_is_not_filed() {
        let k = keep();
        let out = outcome(
            0,
            "abc",
            &k,
            r#"{"title":"Fast","url":"https://github.com/a/b/issues/4"}"#,
            false,
        )
        .unwrap();
        assert_eq!(
            out,
            r#"{"session":"abc","text":"Filed https://github.com/a/b/issues/4: Fast.","question":null,"filed":["https://github.com/a/b/issues/4"]}"#
        );
        for bad in [
            "https://github.com/a\nb/issues/4",
            "https://github.com/a/b/issues/4\n",
            "https://example.com/a/b/issues/4",
        ] {
            let doc = json!({"url": bad}).to_string();
            assert!(!outcome(0, "abc", &k, &doc, false)
                .unwrap()
                .contains("\"filed\""));
        }
    }

    #[test]
    fn questions_are_asked_one_as_it_is_and_more_numbered() {
        let k = keep();
        let one = parsed(
            &outcome(
                11,
                "s",
                &k,
                r#"{"text":"Q.","questions":[{"question":"Which?","resolution":"A"}]}"#,
                false,
            )
            .unwrap(),
        );
        assert_eq!(
            one["question"],
            json!({"question": "Which?\n   Proposed: A", "options": []})
        );
        let two = parsed(
            &outcome(
                11,
                "s",
                &k,
                r#"{"questions":[{"question":"A?"},{"question":"B?"}]}"#,
                false,
            )
            .unwrap(),
        );
        assert_eq!(two["question"]["question"], "The planner asked 2 questions. Answer each by its number in a reply.\n\n1. A?\n\n2. B?");
        let dup = parsed(&outcome(10, "s", &k, r#"{"text":"Like #3."}"#, false).unwrap());
        assert_eq!(dup["question"]["question"], DUPLICATE);
    }

    #[test]
    fn a_turn_that_says_nothing_or_cannot_be_read_prints_nothing() {
        let k = keep();
        assert_eq!(
            outcome(11, "s", &k, "{}", false),
            Err("the turn said and asked nothing".into())
        );
        assert_eq!(
            outcome(11, "s", &k, "not json", false),
            Err("the runner's answer could not be read".into())
        );
    }

    #[test]
    fn a_long_plan_is_cut_and_says_so() {
        let k = keep();
        let doc = json!({"title": "Big", "spec": "x".repeat(50_000)}).to_string();
        let v = parsed(&outcome(12, "s", &k, &doc, false).unwrap());
        let t = v["text"].as_str().unwrap();
        assert!(t.ends_with("(cut: the plan is longer than a comment holds)"));
        assert!(t.chars().count() < 41_000);
    }

    #[test]
    fn a_descriptor_is_the_title_itself_when_it_is_long_enough() {
        // 58 characters, 10 words: the title, with nothing added to it.
        let title = "Implement a Rust renderer for the review issue comment bod";
        assert_eq!(title.chars().count(), 58);
        assert_eq!(words(title), 10);
        assert_eq!(descriptor(&format!("# {title}\n\nBody.\n"), "883"), title);
    }

    #[test]
    fn a_short_descriptor_names_the_issue_and_a_title_later_says_nothing() {
        assert_eq!(
            descriptor("# Faster search\n\nMake it fast.\n", "7"),
            "Faster search (issue #7)"
        );
        // A Title: line that is not the first line is the plan's own text.
        assert_eq!(
            descriptor("# Faster search\n\nTitle: Something else\n", "7"),
            "Faster search (issue #7)"
        );
        // Five words still asks for the number, and the room is measured off
        // the longest suffix, so a small number here is not a licence for a
        // large one: 21 + 13 is 34, well inside the budget.
        assert_eq!(
            descriptor("# Make it fast now then\n", "12345"),
            "Make it fast now then (issue #12345)"
        );
    }

    #[test]
    fn a_title_line_of_a_draft_names_the_issue_it_is_filed_for() {
        assert_eq!(
            descriptor(
                "Title: A Rust renderer of the review issue\n\n# The parent\n\nBody.\n",
                "883"
            ),
            "A Rust renderer of the review issue"
        );
        // Named, yet short: the number still goes with it.
        assert_eq!(
            descriptor("Title: Faster search\n", "7"),
            "Faster search (issue #7)"
        );
    }

    #[test]
    fn a_descriptor_with_no_title_at_all_says_where_the_work_is() {
        // The fallback has 4 words, so the number goes with it: 27 + the
        // longest suffix, 13, is 40, inside the budget of 62.
        assert_eq!(NO_TITLE.chars().count() + MAX_SUFFIX, 40);
        assert_eq!(words(NO_TITLE), 4);
        let named = "the work in .tehas/brief.md (issue #7)";
        assert_eq!(descriptor("\nMake it fast.\n", "7"), named);
        assert_eq!(descriptor("#\n\nBody.\n", "7"), named);
        assert_eq!(descriptor("# \t\n\nBody.\n", "7"), named);
        assert_eq!(descriptor("", "7"), named);
        assert_eq!(
            descriptor("#\n", "12345"),
            "the work in .tehas/brief.md (issue #12345)"
        );
    }

    #[test]
    fn a_short_title_with_no_room_for_the_suffix_is_left_as_it_is() {
        // Ten words, 54 characters: the longest suffix would make 67, and
        // this number's own 65, both over what pi-rukas keeps. The rule reads
        // the longest, so the number is left off although the title is too
        // short to plan from.
        let wide = "the renderer for the review issue comment body no more";
        assert_eq!(words(wide), 10);
        assert_eq!(wide.chars().count(), 54);
        assert_eq!(descriptor(&format!("# {wide}\n"), "7"), wide);
        assert_eq!(descriptor(&format!("# {wide}\n"), "12345"), wide);
        // The suffix is never invented for a title of its own right: a Title:
        // line too short to plan from and too long for the number keeps its
        // text, and the heading under it names nothing.
        let named = "Title: the renderer for the review issue comment body no more";
        assert_eq!(words(named) - 1, 10);
        assert_eq!(descriptor(&format!("{named}\n\n# The parent\n"), "7"), wide);
        // Five words with room for the longest suffix: 28 + 13 is 41, so the
        // number goes with them.
        let few = "the renderer for this review";
        assert_eq!(words(few), 5);
        // pi-rukas asks below six words, and five is below six.
        assert!(words(few) < MIN_WORDS);
        assert_eq!(few.chars().count(), 28);
        assert_eq!(
            descriptor(&format!("# {few}\n"), "7"),
            "the renderer for this review (issue #7)"
        );
        // Seven words is enough to plan from on its own: the same title, with
        // two words more, is handed over as it is.
        let said = "the renderer for this review today now and then";
        assert_eq!(words(said), 9);
        assert_eq!(descriptor(&format!("# {said}\n"), "7"), said);
    }

    #[test]
    fn an_update_carries_the_draft_and_the_text_it_replaces() {
        let k = keep();
        std::fs::write(
            k.join("draft.json"),
            r###"{"title":"feat: fast","spec":"## Outcome"}"###,
        )
        .unwrap();
        let out = update(
            "s",
            &k,
            r#"{"title":"search","body":"make it fast\n\nreally"}"#,
        )
        .unwrap();
        let v = parsed(&out);
        assert_eq!(
            v["update"],
            json!({"title": "feat: fast", "body": "## Outcome"})
        );
        assert_eq!(v["question"], Value::Null);
        assert_eq!(v["text"], "This issue now holds the plan. Apply `tehas:ready` to build it.\n\n<details><summary>The text it replaced</summary>\n\n> # search\n> \n> make it fast\n> \n> really\n\n</details>");
        std::fs::write(k.join("draft.json"), r#"{"title":"","spec":"x"}"#).unwrap();
        assert_eq!(
            parsed(&update("s", &k, r#"{"title":"search","body":null}"#).unwrap())["update"]
                ["title"],
            "search"
        );
    }

    #[test]
    fn an_update_without_a_draft_or_the_issue_prints_nothing() {
        let k = keep();
        assert!(update("s", &k, r#"{"title":"t","body":""}"#).is_err());
        std::fs::write(k.join("draft.json"), r#"{"title":"t","spec":""}"#).unwrap();
        assert!(update("s", &k, r#"{"title":"t","body":""}"#).is_err());
        std::fs::write(k.join("draft.json"), r#"{"title":"t","spec":"x"}"#).unwrap();
        assert!(update("s", &k, "").is_err());
    }
}
