//! What a planning turn says: the runner's answer shaped as the dispatcher
//! posts it, and a drafted plan the operator chose to make the issue's own
//! text. `tehas plan` is src/plan/start.rs, the host's side of a turn
//! src/plan/turn.rs.
//!
//!   console plan update <session> <keep>
//!       the issue's {title, body} on standard input, as gh issue view gives
//!       them, and the draft in <keep>/draft.json.
//!   console plan descriptor <brief> <issue>
//!       the one line the first turn plans, read from <brief>
//!       (what plan_brief writes: `# <title>`, a blank line, then the body)
//!       and the issue's number. See descriptor for how the line is made.
//!
//!   console plan table
//!       this command's rows of the table of runs (test/sandbox_test.sh).
//!
//! update prints the --out document of a turn, descriptor its one line.
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
use tehas_core::plan::{
    checked, clip, keep_spec, made, refusal_document, text_of, APPLY_READY, CLIP, SPEC_INFO,
};

mod start;
mod turn;

pub use start::start;
use start::table;
pub use start::words as turn_words;

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
    let mut input = String::new();
    let done = match given {
        [verb, session, keep] if verb == "update" => {
            let _ = std::io::stdin().read_to_string(&mut input);
            update(session, Path::new(keep), &input)
        }
        [verb] if verb == "table" => Ok(table()),
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
            tehas_core::outln!("{said}");
            0
        }
        Err(why) => {
            eprintln!("console plan: {why}");
            1
        }
    }
}

fn usage() -> u8 {
    eprintln!("console plan: usage: update <session> <keep> | descriptor <brief> <issue> | table");
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

/// The --out document, its keys in the order a turn always wrote them.
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
///
/// The caller joins this with [`tehas_core::plan::checked`] in this order: the
/// two answer different questions (whether the issue can hold the plan whole,
/// and whether the draft's own text says anything of the issue), and a draft
/// too long for the issue or left open by the planner is asked about whatever
/// its text reads. `checked` is the one function the dispatcher asks later on
/// the kept draft, so the two cannot answer differently for one draft.
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
            let (block, spec) = runner_block(&text_of(d.get("spec")));
            let mut draft = json!({"title": text_of(d.get("title")), "spec": spec});
            if let Some(block) = block {
                draft["block"] = json!(block);
            }
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
                // `clear` tells the dispatcher to write the draft onto the
                // issue without another tick, so it is said only of a draft
                // the checks of #1210 let through: one that says nothing of
                // the issue is asked about, like any other draft.
                if clear(&d, &spec) && checked(&draft).is_ok() {
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

/// A drafted plan the person chose to make the issue's text. One whose title
/// or text says nothing of the issue is refused in the same document: the text
/// names the rule, the question asks for the change, and there is no `update`
/// key, so the dispatcher writes nothing and the session stays open.
pub fn update(session: &str, keep: &Path, was: &str) -> Result<String, String> {
    let was: Value = serde_json::from_str(was)
        .map_err(|_| "the text of the issue could not be read".to_string())?;
    let draft: Value = read_draft(keep).ok_or("no drafted plan is kept")?;
    let plan = match made(&draft, &was, APPLY_READY) {
        Ok(plan) => plan,
        // Refused: the text of the issue stays as it is, and the person is
        // asked for the change. The document of a refusal holds no `update`,
        // so nothing is written.
        Err(refused) => {
            let why = refused.line();
            return Ok(refused_document_of(session, &why).to_string());
        }
    };
    // The block pi-rukas assembled, kept at exit 12, is the one the issue
    // holds. A draft from a runner that placed none keeps the person's.
    let kept_from = match draft.get("block").and_then(Value::as_str) {
        Some(block) => block.to_string(),
        None => text_of(was.get("body")),
    };
    let more = format!(
        ",\"update\":{}",
        json!({
            "title": plan.title,
            // The runner's block, or else the person's, goes on top here and
            // nowhere earlier: the document the tick writes holds it, and
            // nothing else reads this body.
            "body": keep_spec(&plan.body, &kept_from)
        })
    );
    Ok(document(session, &plan.text, &Value::Null, &[], &more))
}

/// The document of a turn whose drafted plan is refused by the check of #1210.
/// The console has no comment of its own, so the line of the rule is both what
/// the person reads and what the question asks them to answer; the document
/// holds no `update`, so the dispatcher writes nothing and the session stays
/// open for the reply that changes the plan. It is
/// [`tehas_core::plan::refusal_document`] with the line asked twice over, once
/// as the text and once as the question, so the three keys of every outcome
/// are still made by one function.
fn refused_document_of(session: &str, why: &str) -> Value {
    refusal_document(why, why, Some(&json!(session)))
}

/// The heading pi-rukas places its assembled `pi-spec v1` block under, at
/// the top of the body it shows at exit 12 (`plan-pi-spec.ts`).
const SPEC_HEADING: &str = "## Spec (pi-spec v1)";

/// The `pi-spec v1` block pi-rukas assembled into a drafted plan, and the
/// spec to keep. The block is the runner's only when it opens the body: its
/// heading is the first line, or the second after the provenance line, and
/// the fence right under it reads as a block. Every other `pi-spec v1` fence
/// is the draft's prose and is renamed `pi-spec-example v1`, as pi-rukas
/// renames one, so the issue holds one block. A spec with no such section
/// comes from a runner that places none and is kept as it came.
fn runner_block(spec: &str) -> (Option<String>, String) {
    let as_is = (None, spec.to_string());
    let lines: Vec<&str> = spec.split('\n').collect();
    let filled = |from: usize| (from..lines.len()).find(|&i| !lines[i].trim().is_empty());
    let Some(first) = filled(0) else {
        return as_is;
    };
    let heading = if lines[first].trim() == SPEC_HEADING {
        first
    } else {
        match filled(first + 1) {
            Some(i) if lines[i].trim() == SPEC_HEADING => i,
            _ => return as_is,
        }
    };
    let Some(open) = filled(heading + 1) else {
        return as_is;
    };
    let Some(run) = opens_spec(lines[open]) else {
        return as_is;
    };
    let Some(close) = (open + 1..lines.len()).find(|&i| closes(lines[i], run)) else {
        return as_is;
    };
    let Some(block) = tehas_core::plan::spec_block(&lines[open..=close].join("\n")) else {
        return as_is;
    };
    let kept = lines
        .iter()
        .enumerate()
        .map(|(i, line)| match opens_spec(line) {
            Some(_) if i != open => line.replacen(SPEC_INFO, "pi-spec-example v1", 1),
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    (Some(block), kept)
}

/// A line's text after at most three spaces of indent.
fn unindented(line: &str) -> &str {
    let spaces = line.len() - line.trim_start_matches(' ').len();
    &line[spaces.min(3)..]
}

/// The length of the backtick run of a line that opens a `pi-spec v1` fence.
fn opens_spec(line: &str) -> Option<usize> {
    let t = unindented(line);
    let run = t.bytes().take_while(|b| *b == b'`').count();
    (run >= 3 && t[run..].trim() == SPEC_INFO).then_some(run)
}

/// Whether a line closes a backtick fence opened with a run of `run`.
fn closes(line: &str, run: usize) -> bool {
    let t = unindented(line).trim_end();
    t.len() >= run && t.bytes().all(|b| b == b'`')
}

/// The draft a turn kept beside its session, or nothing when it is gone or no
/// document.
fn read_draft(keep: &Path) -> Option<Value> {
    std::fs::read_to_string(keep.join("draft.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::plan::REFUSED;

    fn keep() -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new("plan")
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

    /// A draft the text check refuses is never marked `clear`, so the
    /// dispatcher cannot write it onto the issue or credit a start for it
    /// without a person's tick: it is asked about as any other draft is.
    #[test]
    fn a_draft_the_check_refuses_is_never_clear() {
        let k = keep();
        let good = "## Outcome\n\nThe p95 is under 100 ms.";
        // A title of one word written twice, long enough to be no summary of
        // anything: the rule is what the person reads, so its name stays in
        // the assertion below even though the core test pins it exactly.
        let long = "work ".repeat(9) + "work"; // 39 characters, all one word
        for (title, spec) in [
            ("EPIC: EPIC", good),
            (&long, good),
            (
                "feat: faster search",
                "Descriptor: what the issue is\n\n## Prior context inventory\n\n- the file",
            ),
            ("feat: faster search", "[context param]\n\n[context param]"),
        ] {
            let doc = json!({"text": "Drafted.", "title": title, "spec": spec, "questions": []})
                .to_string();
            let v = parsed(&outcome(12, "abc", &k, &doc, false).unwrap());
            assert!(v.get("clear").is_none(), "{doc}: {v}");
            // Still shown with its box, so the person can tick it after they
            // have read what the check said of it.
            assert_eq!(v["question"]["options"][0]["label"], "Update this issue");
        }
        // The same document with a title and a text that hold says `clear`.
        let v = parsed(&outcome(12, "abc", &k, CLEAR, false).unwrap());
        assert_eq!(v["clear"], true);
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
        // A plan of one line holds nothing a person wrote, so the check
        // refuses it; a drafted plan is written in whole sentences.
        std::fs::write(
            k.join("draft.json"),
            r###"{"title":"feat: fast","spec":"## Outcome\n\nThe p95 is under 100 ms."}"###,
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
            json!({"title": "feat: fast", "body": "## Outcome\n\nThe p95 is under 100 ms."})
        );
        assert_eq!(v["question"], Value::Null);
        assert_eq!(v["text"], "This issue now holds the plan. Apply `tehas:ready` to build it.\n\n<details><summary>The text it replaced</summary>\n\n> # search\n> \n> make it fast\n> \n> really\n\n</details>");
        // No title of its own: the issue keeps the one it has, which the draft
        // cannot refuse.
        std::fs::write(
            k.join("draft.json"),
            r#"{"title":"","spec":"The console reads the new key."}"#,
        )
        .unwrap();
        assert_eq!(
            parsed(&update("s", &k, r#"{"title":"search","body":null}"#).unwrap())["update"]
                ["title"],
            "search"
        );
    }

    /// A person's `pi-spec v1` block survives the write of a plan: the lean
    /// driver reads only that block, and the draft of a plan holds none of its
    /// own, so the issue's block goes back on top of the text (`keep_spec`).
    #[test]
    fn an_update_keeps_the_issue_spec_block_at_the_top() {
        let k = keep();
        // A plan the check lets through, written over a person's issue that
        // holds the block lean reads.
        let draft = json!({"title": "feat: fast",
                           "spec": "## Outcome\n\nThe p95 is under 100 ms."});
        let plan = json!("## Outcome\n\nThe p95 is under 100 ms.");
        std::fs::write(k.join("draft.json"), draft.to_string()).unwrap();
        let was = json!({
            "title": "search",
            "body": "```pi-spec v1\n---\n{\"intent\":\"faster search\"}\n```\n\nWhat the person wrote."
        });
        let v = parsed(&update("s", &k, &was.to_string()).unwrap());
        // The block is put back fenced whole, its table rule off: JSON is
        // what lean reads, and the rule is what pi-rukas drops of its own.
        assert_eq!(
            v["update"]["body"],
            json!("```pi-spec v1\n{\"intent\":\"faster search\"}\n```\n\n## Outcome\n\nThe p95 is under 100 ms.")
        );
        // A body that brings a block of its own keeps that one and loses the
        // issue's, and one with no block anywhere is written as today.
        let mine = json!({"title": "feat: fast",
                          "spec": "```pi-spec v1\n{\"intent\":\"new\"}\n```\n\n## Outcome\n\nThe p95 is under 100 ms."});
        std::fs::write(k.join("draft.json"), mine.to_string()).unwrap();
        let v = parsed(&update("s", &k, &was.to_string()).unwrap());
        assert_eq!(v["update"]["body"], mine["spec"]);
        std::fs::write(k.join("draft.json"), draft.to_string()).unwrap();
        let v = parsed(
            &update(
                "s",
                &k,
                &json!({"title": "t", "body": "no block here"}).to_string(),
            )
            .unwrap(),
        );
        assert_eq!(v["update"]["body"], plan);
        // Two blocks, or one that is no JSON, are no block to keep.
        for was in [
            json!({"title": "t", "body": "```pi-spec v1\n{\"a\":1}\n```\n\n```pi-spec v1\n{\"b\":2}\n```"}),
            json!({"title": "t", "body": "```pi-spec v1\nnot json\n```"}),
        ] {
            let v = parsed(&update("s", &k, &was.to_string()).unwrap());
            assert_eq!(
                v["update"]["body"],
                json!("## Outcome\n\nThe p95 is under 100 ms."),
                "{was}"
            );
        }
    }

    /// A draft that fails the text check: the document says why, asks for the
    /// change, and holds no `update`, so nothing is written and the session
    /// stays open.
    #[test]
    fn a_draft_that_fails_the_check_changes_nothing_and_says_why() {
        let k = keep();
        let refused = |draft: &str| {
            std::fs::write(k.join("draft.json"), draft).unwrap();
            parsed(&update("s", &k, r#"{"title":"search","body":"fast"}"#).unwrap())
        };
        let good = "## Outcome\n\nThe p95 is under 100 ms.";
        let long = "work ".repeat(9) + "work"; // 39 characters, all one word
                                               // [draft, the rule it fails, what the person is told to do]
        for (title, spec, why) in [
            ("EPIC: EPIC", good, "nothing but prefixes"),
            (&long, good, "one word repeated"),
            (
                "feat: faster search",
                "Descriptor: what the issue is\n\n## Prior context inventory\n\n- the file",
                "the planner's brief",
            ),
            (
                "feat: faster search",
                "[context param]\n\n[context param]",
                "markers",
            ),
            ("feat: faster search", "", "no text"),
        ] {
            let draft = json!({"title": title, "spec": spec}).to_string();
            let v = refused(&draft);
            assert!(v.get("update").is_none(), "{draft}: {v}");
            assert_eq!(v["question"]["options"], json!([]));
            // The structural contract of a refusal: what the person reads is
            // the question they answer, it is one line, it opens with the
            // sentence of the refusal and tells them to reply. Which rule the
            // text failed is said in `textcheck`'s own words, which the core
            // test pins exactly: the name of the rule is asked here only so a
            // draft that failed some other rule cannot pass this case.
            let asked = v["question"]["question"].as_str().unwrap();
            assert_eq!(asked, v["text"].as_str().unwrap());
            assert!(asked.starts_with(REFUSED), "{draft}: {asked}");
            assert!(asked.contains(why), "{draft}: {asked}");
            assert!(asked.contains("Reply"), "{draft}: {asked}");
            // One line, so the comment that says it holds one line of its own.
            assert_eq!(asked.lines().count(), 1, "{draft}: {asked}");
            assert_eq!(v["session"], "s");
        }
        // A draft that passes is written as before, with its `update`.
        let v = refused(&json!({"title": "feat: faster search", "spec": good}).to_string());
        assert_eq!(
            v["update"],
            json!({"title": "feat: faster search", "body": good})
        );
        assert!(v["question"].is_null());
    }

    #[test]
    fn an_update_without_a_draft_or_the_issue_prints_nothing() {
        // A keep directory of its own: the tests of this file run side by side,
        // and a draft one of them kept would answer for this one.
        let k = tehas_core::testdir::TestDir::new("plan-none");
        let good = r#"{"title":"t","spec":"The console reads the new key."}"#;
        // Nothing to print and the turn fails: no draft kept, a kept draft
        // that is no document, and no issue to read. A draft the check
        // refuses, of which one holding no text is one, is an answer: it
        // prints a document that says why.
        assert!(update("s", &k, r#"{"title":"t","body":""}"#).is_err());
        std::fs::write(k.join("draft.json"), "not a document").unwrap();
        assert!(update("s", &k, r#"{"title":"t","body":""}"#).is_err());
        std::fs::write(k.join("draft.json"), good).unwrap();
        assert!(update("s", &k, "").is_err());
        let _ = std::fs::remove_dir_all(&k);
    }

    /// The body pi-rukas shows at exit 12: its provenance line, the block it
    /// assembled under its heading, then the drafted prose.
    fn assembled(prose: &str) -> String {
        format!("_Planned by pi-rukas._\n\n## Spec (pi-spec v1)\n\n```pi-spec v1\n{{\n  \"problem\": \"fast\"\n}}\n```\n\n{prose}")
    }

    fn shown(spec: &str) -> String {
        json!({"text": "Drafted.", "title": "feat: fast", "spec": spec, "questions": []})
            .to_string()
    }

    const RUNNER_BLOCK: &str = "```pi-spec v1\n{\n  \"problem\": \"fast\"\n}\n```";

    /// The block the runner assembled is kept with the draft, and the update
    /// writes it at the top of the issue in place of the person's own.
    #[test]
    fn a_draft_with_a_block_from_the_runner_writes_it_at_the_top() {
        let k = keep();
        let spec = assembled("## Outcome\n\nThe p95 is under 100 ms.");
        outcome(12, "abc", &k, &shown(&spec), false).unwrap();
        let kept = parsed(&std::fs::read_to_string(k.join("draft.json")).unwrap());
        assert_eq!(kept["block"], RUNNER_BLOCK);
        assert_eq!(kept["spec"], spec.as_str());
        let was = json!({"title": "search",
                         "body": "```pi-spec v1\n{\"intent\":\"old\"}\n```\n\nWhat the person wrote."});
        let v = parsed(&update("s", &k, &was.to_string()).unwrap());
        let body = v["update"]["body"].as_str().unwrap();
        assert_eq!(body, spec);
        assert_eq!(tehas_core::plan::spec_block(body).unwrap(), RUNNER_BLOCK);
    }

    /// A `pi-spec v1` fence in the drafted prose below the runner's block is
    /// renamed as pi-rukas renames one, so the issue holds one block.
    #[test]
    fn a_drafted_prose_fence_named_pi_spec_is_not_written_as_a_second_block() {
        let k = keep();
        let prose = "## Outcome\n\nAn example:\n\n```pi-spec v1\n{\"problem\":\"quoted\"}\n```";
        outcome(12, "abc", &k, &shown(&assembled(prose)), false).unwrap();
        let kept = parsed(&std::fs::read_to_string(k.join("draft.json")).unwrap());
        assert_eq!(kept["block"], RUNNER_BLOCK);
        let renamed = assembled(
            "## Outcome\n\nAn example:\n\n```pi-spec-example v1\n{\"problem\":\"quoted\"}\n```",
        );
        assert_eq!(kept["spec"], renamed.as_str());
        let was = json!({"title": "search", "body": "What the person wrote."});
        let v = parsed(&update("s", &k, &was.to_string()).unwrap());
        let body = v["update"]["body"].as_str().unwrap();
        assert_eq!(body, renamed);
        assert_eq!(tehas_core::plan::spec_block(body).unwrap(), RUNNER_BLOCK);
        assert_eq!(body.matches("```pi-spec v1\n").count(), 1);
    }

    /// A runner that places no block: no block is kept, the spec is kept as
    /// it came, and the update keeps the person's block as before (#1458).
    #[test]
    fn a_runner_without_the_block_leaves_the_body_as_today() {
        let k = keep();
        let prose = "## Outcome\n\nThe p95 is under 100 ms.";
        // A block somewhere below the top is the draft's prose, not a section
        // the runner placed: nothing is renamed and nothing is kept for it.
        let low = format!("{prose}\n\n## Spec (pi-spec v1)\n\n{RUNNER_BLOCK}");
        for spec in [prose.to_string(), low] {
            outcome(12, "abc", &k, &shown(&spec), false).unwrap();
            let kept = parsed(&std::fs::read_to_string(k.join("draft.json")).unwrap());
            assert!(kept.get("block").is_none(), "{kept}");
            assert_eq!(kept["spec"], spec.as_str());
        }
        outcome(12, "abc", &k, &shown(prose), false).unwrap();
        let was = json!({"title": "search",
                         "body": "```pi-spec v1\n{\"intent\":\"old\"}\n```\n\nWhat the person wrote."});
        let v = parsed(&update("s", &k, &was.to_string()).unwrap());
        assert_eq!(
            v["update"]["body"],
            json!(format!(
                "```pi-spec v1\n{{\"intent\":\"old\"}}\n```\n\n{prose}"
            ))
        );
        // A heading with no block under it, or one that is no JSON, is no
        // block the runner placed.
        for top in [
            "## Spec (pi-spec v1)\n\nNo block.".to_string(),
            "## Spec (pi-spec v1)\n\n```pi-spec v1\nnot json\n```".to_string(),
        ] {
            outcome(12, "abc", &k, &shown(&top), false).unwrap();
            let kept = parsed(&std::fs::read_to_string(k.join("draft.json")).unwrap());
            assert!(kept.get("block").is_none(), "{kept}");
            assert_eq!(kept["spec"], top.as_str());
        }
    }
}
