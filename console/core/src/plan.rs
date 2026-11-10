//! A drafted plan made an issue's text: what a tick on Update this issue
//! prints through `console plan update`, and what the dispatcher writes for a
//! plan with no open points, which needs no tick.
//!
//! Neither writes text that says nothing of the issue. [`made`] asks
//! [`tehas_core::textcheck`] of the draft's title and spec and answers with a
//! [`Refusal`] naming the rule, so the writer that calls it can say the reason
//! to the person and change nothing. `tehas plan` cannot fail the turn for
//! a refused draft: the session stays open and the person replies to change
//! the plan, as with an open point.

use crate::textcheck::{check_body, check_title, Reason, Refusal};
use serde_json::{json, map::Map, Value};

/// A GitHub comment holds 65,536 characters; the plan is cut well before.
pub const CLIP: usize = 40_000;

/// How much of the runner's reason for a failed planning turn reaches the
/// issue: the turn cuts its own words to this length (`console/src/plan/turn.rs`)
/// and the dispatcher cuts them again where it prints them
/// (`console/dispatch/src/apply/plan.rs`), as the file between the two is state
/// a person can edit. One bound, so the two cannot drift apart.
pub const WHY_MAX: usize = 500;

/// What the comment says when a person still applies the label.
pub const APPLY_READY: &str = "This issue now holds the plan. Apply `tehas:ready` to build it.";

/// What a comment says when a drafted plan was refused: the line a writer
/// puts before the reason, for a person who reads the reason on the issue.
/// [`Refused::line`] is the only thing that reads it, so the sentence a
/// person is shown has one owner.
pub const REFUSED: &str = "Tehas did not make the drafted plan this issue's text";

pub fn clip(s: &str, what: &str) -> String {
    match s.char_indices().nth(CLIP) {
        Some((at, _)) => format!(
            "{}\n\n(cut: {what} is longer than a comment holds)",
            &s[..at]
        ),
        None => s.to_string(),
    }
}

/// A value as jq's `(. // "") | tostring` gives it.
pub fn text_of(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

/// The issue's new title and body, and the comment that ends the session.
#[derive(Debug, PartialEq, Eq)]
pub struct Made {
    pub title: String,
    pub body: String,
    pub text: String,
}

/// What [`made`] gives a person when the draft's text says nothing of the
/// issue: which rule the text failed, the line that says so, and what the
/// person can do next. The rule stays the closed enum [`Reason`] rather than
/// prose: a writer that answers differently for one of them matches on it and
/// the compiler says when a new rule is not answered.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    /// Which rule the text failed.
    pub reason: Reason,
    /// What the person reads: the rule in `textcheck`'s own words, with the
    /// refused title after it when there is room. One line.
    pub line: String,
}

impl Refused {
    /// The refusal of a `textcheck` rule: its reason, and the next step said
    /// of a plan that stands as shown. The only place that sentence lives.
    fn of(refused: Refusal) -> Refused {
        Refused {
            reason: refused.reason,
            line: refused.line,
        }
    }

    /// The refusal of a draft whose own text was never read: the rule is
    /// `textcheck`'s `Reason::EmptyTitle`, and [`Refused::line`] gives it the
    /// next step of a plan that holds nothing.
    fn no_text() -> Refused {
        Refused {
            reason: Reason::EmptyTitle,
            line: NO_TEXT.to_string(),
        }
    }

    /// The line for the comment: what was not done, the rule, and what to do
    /// next. One line, so the comment that says it holds one. Every refusal
    /// a person is told about is said with this, so the wording has one home:
    /// `textcheck`'s own words for the rule, then one of the two next steps,
    /// each named here.
    pub fn line(&self) -> String {
        let next = match self.reason {
            // The rule of a draft that holds no text at all, which is asked
            // before `textcheck` sees anything: its next step names what a
            // plan of nothing has to expect.
            Reason::EmptyTitle if self.line == NO_TEXT => NO_TEXT_NEXT,
            _ => STANDS_NEXT,
        };
        format!("{REFUSED}: {}. {}", self.line, next)
    }

    /// Whether this line is a refusal of a drafted plan: it opens with the
    /// sentence of [`REFUSED`], which only [`Refused::line`] puts there. The
    /// rule itself is not wanted, so a line read back from a document is
    /// given the variant of a title that says nothing.
    fn starts_with_rule(&self) -> bool {
        self.line
            .strip_prefix(REFUSED)
            .is_some_and(|rest| rest.starts_with(": "))
    }

    /// Whether a line of prose is the line of a refusal, as it is read back
    /// from a document a writer cannot ask the check about again.
    fn says_rule(why: &str) -> bool {
        !why.is_empty()
            && Refused {
                reason: Reason::OnlyPrefix,
                line: why.to_string(),
            }
            .starts_with_rule()
    }
}

/// What the line of a draft that holds no text says, and what it tells the
/// person to do. `textcheck`'s `Reason::EmptyTitle` words are left for the
/// rule of a title that is empty, which the check itself gives.
const NO_TEXT: &str = "the drafted plan has no text";
const NO_TEXT_NEXT: &str = "Reply to change the plan, and the next turn drafts \
                            it again. A plan of nothing has nothing to write.";
/// The next step of every refusal of a text the check read: the issue is
/// untouched, so the plan a person saw is the plan that stands.
const STANDS_NEXT: &str = "The plan stands as shown and nothing was written. \
                           Reply to change the title or the text of the plan, \
                           and the next turn drafts it again.";

/// Whether `draft` holds text a check can read: an entry that is no entry, or
/// one of nothing, has nothing for the check to say anything about.
fn has_text(draft: &Value) -> bool {
    draft
        .get("spec")
        .is_some_and(|v| !v.is_null() && !text_of(Some(v)).trim().is_empty())
}

/// Whether the check reads a draft's title: only the one the draft names, as
/// the title that would go on the issue. A draft that names none leaves the
/// issue the title it holds, which a person wrote and approved with the label
/// that started the session, so nothing written here refuses it.
fn names_title(draft: &Value) -> bool {
    draft
        .get("title")
        .is_some_and(|v| !v.is_null() && !text_of(Some(v)).trim().is_empty())
}

/// The check of #1210 over the draft's own title and text, with none of the
/// writing: what the console asks of a draft before it marks it `clear`, and
/// what the dispatcher asks again of the draft it kept for a poll. Both ask
/// this one function, so the two cannot answer differently for one draft.
/// A draft that names no title is asked only of its text, which is all it can
/// put on the issue.
pub fn checked(draft: &Value) -> Result<(), Refused> {
    if !has_text(draft) {
        return Err(Refused::no_text());
    }
    if names_title(draft) {
        check_title(&text_of(draft.get("title")), &text_of(draft.get("spec")))
            .map_err(Refused::of)?;
    }
    check_body(&text_of(draft.get("spec"))).map_err(Refused::of)
}

/// Why the text of `out` would make nothing of the issue, or nothing when it
/// may be written. `out` is the outcome kept for a poll, whose `update` holds
/// the title and body the poll would write, both made by [`made`] from
/// `draft`: the check reads the draft's own title, so a `was` is not wanted
/// and no title the issue already holds can refuse anything here.
///
/// `None` means this function has no answer, which is two different facts the
/// caller tells apart by what it holds: no `update` object at all (nothing of
/// the kind was ever asked for), or an `update` whose text the draft does not
/// hold (a document of a writer whose text this cannot see). The caller reads
/// `update` for itself; see `refused_by_draft`'s use in the dispatcher.
pub fn refused_by_draft(out: &Value, draft: &Value) -> Option<Refused> {
    // No answer for a document whose draft holds neither of its two strings:
    // its title would be checked against a draft that does not name it.
    if !draft_holds(out, draft, true) {
        return None;
    }
    checked(draft).err()
}

/// The two strings a drafted plan can put on the issue: the title and the body
/// of the `update` of an outcome, each read from the same key of the draft.
/// One pair for the two questions the dispatcher asks of a kept document, so
/// they cannot disagree about what a draft says.
fn strings(draft: &Value, update: &Map<String, Value>) -> (bool, bool) {
    let holds = |key: &str, from: &str| match (update.get(key), draft.get(from)) {
        (Some(v), Some(d)) => v.is_string() && !d.is_null() && text_of(Some(d)) == text_of(Some(v)),
        _ => false,
    };
    (holds("title", "title"), holds("body", "spec"))
}

/// Whether `out` carries an `update` the write could use: an object whose
/// title and body are both text, whatever draft it came from. A document that
/// does not hold it holds nothing for a poll to write, so no check of the
/// draft's text can stop it and the question of its turn is all it has to ask.
pub fn carries_update(out: &Value) -> bool {
    match out.get("update").and_then(Value::as_object) {
        Some(update) => {
            update.get("title").is_some_and(Value::is_string)
                && update.get("body").is_some_and(Value::is_string)
        }
        None => false,
    }
}

/// Whether the draft holds the text of `out`'s `update`. `all` asks for both
/// of its strings, which is what the check of #1210 needs to answer for a
/// document at all; without it one of the two is enough, which says the poll
/// is looking at the plan a turn kept and not some other document.
pub fn draft_holds(out: &Value, draft: &Value, all: bool) -> bool {
    let Some(update) = out.get("update").and_then(Value::as_object) else {
        return false;
    };
    let (title, body) = strings(draft, update);
    if all {
        title && body
    } else {
        title || body
    }
}

/// Whether `out` holds the refusal of a drafted plan: its text is the line
/// `Refused::line` builds, and there is no `update` for a poll to write. A
/// writer that has to say the rule again, or leave an `update` alone, asks
/// this instead of spelling the sentence.
pub fn is_refusal(out: &Value) -> bool {
    out.get("update").is_none_or(Value::is_null) && says_refusal(&text_of(out.get("text")))
}

/// Whether a line of prose is the line of a refusal: the sentence of
/// [`REFUSED`], which only [`Refused::line`] puts before a rule. A writer that
/// has a reason of `made` as text, and not the [`Refused`] itself, tells the
/// rule the draft failed apart from an operational reason with this.
pub fn says_refusal(why: &str) -> bool {
    Refused::says_rule(why)
}

/// The document of the outcome whose drafted plan the check refused: `why` is
/// the line the person reads, kept as the turn's text, with no `update` for a
/// poll to write. `said` is what the question asks: the console has no comment
/// of its own, so it says the line there too, while a poll that has already
/// said it in a comment of its own gives nothing, leaving the person to reply
/// rather than tick (the tick of Update this issue is the write the refusal
/// stopped). `session` is the console's own field, kept as it stands.
pub fn refusal_document(why: &str, said: &str, session: Option<&Value>) -> Value {
    json!({"session": session, "text": why, "question": {"question": said, "options": []}})
}

/// The info string of the block lean reads its spec from, as pi-rukas writes it
/// on the first line of its fence: `pi-spec v1`. tehas asks only that one such
/// block is there and holds valid JSON; the fields are pi-rukas's to judge.
pub const SPEC_INFO: &str = "pi-spec v1";

/// The `pi-spec v1` block of a body, fenced whole, or nothing when the body
/// holds none it can read. Three rules say what one is: the body holds exactly
/// one fence with that info string and no more (two are none, not the first),
/// that fence closes, and what sits inside it parses as one JSON document of
/// any shape - an object, an array or a scalar all read, because the fields
/// are pi-rukas's to judge and its `ready` step is the one that refuses a
/// document it cannot plan from. A line of dashes or stars right after the
/// info string is the table rule pi-rukas writes and its own reader drops.
///
/// The one function both sides ask, so what an update keeps and what a poll
/// refuses cannot disagree. The walk of the fences is tehas's own and follows
/// CommonMark, as pi-rukas's reader does: at most three spaces of indent, a run
/// of three or more backticks, and a closing run of the same character at least
/// as long as its opener and with no info string of its own. Those two rules
/// are what keep the quoted example honest - a four-backtick wrapper holds a
/// three-backtick `pi-spec v1` line as prose, so it names no block - and
/// pi-rukas renames a drafted `pi-spec v1` fence to `pi-spec-example v1` on the
/// way into an example, which is what makes the exact info string safe to ask
/// for. A fence line opens no block while one is already open. Tilde fences are
/// read as prose: pi-rukas writes a spec with backticks, and a `pi-spec v1`
/// line inside a `~~~` region names no block here or to its reader.
pub fn spec_block(body: &str) -> Option<String> {
    // The lines of the block seen so far, and whether a second one has been
    // seen (in which case there is no block and the walk ends there).
    let mut found: Option<(Vec<&str>, bool)> = None;
    // The fence now open: its character, the length of its run, its info
    // string, and the lines so far.
    let mut open: Option<(u8, usize, String, Vec<&str>)> = None;
    for line in body.lines() {
        match fence_line(line) {
            None => {
                if let Some((_, _, _, inner)) = open.as_mut() {
                    inner.push(line);
                }
            }
            // A run of the same character, at least as long, and with no info
            // string of its own: the line closes the open block.
            Some((ch, run, rest)) if rest.is_empty() => match open.take() {
                Some((ch0, len, info, inner)) if ch == ch0 && run >= len => {
                    if info != SPEC_INFO {
                        continue;
                    }
                    // A second block named for it is no block at all.
                    if found.is_some() {
                        return None;
                    }
                    found = Some((inner, false));
                }
                // A closing run that does not match the open fence is a line
                // of that block's text; one with none open is the tail of
                // something this does not read.
                Some(block) => open = Some(block),
                None => {}
            },
            // A named fence opens a block, and only when none is open: the
            // line of a fence inside one is prose.
            Some((ch, run, rest)) if open.is_none() => {
                open = Some((ch, run, rest, Vec::new()));
            }
            Some(_) => {}
        }
    }
    let (lines, _) = found?;
    let text = block_text(&lines);
    // Any one JSON document reads: the shape of it is lean's question, not
    // this one's.
    serde_json::from_str::<Value>(&text)
        .ok()
        .map(|_| format!("```{SPEC_INFO}\n{text}\n```"))
}

/// The heading pi-rukas reads as the operator's acceptance criteria in a
/// planning turn's context (`plan-directives.ts`): its bullets become the
/// draft's Acceptance criteria section as they are written.
const CRITERIA_DIRECTIVE: &str = "ACCEPTANCE CRITERIA:";

/// Whether a line is an Acceptance criteria heading as pi-rukas reads one:
/// any case, with markdown's `#`, `*` or `=` around it or not, and then a
/// colon, a parenthesis or nothing. A sentence that opens with the words is
/// prose.
fn criteria_heading(line: &str) -> bool {
    let marks = ['#', '*', '=', ' ', '\t'];
    let t = line.trim().trim_start_matches(marks);
    let (Some(head), Some(rest)) = (t.get(..19), t.get(19..)) else {
        return false;
    };
    let rest = rest.trim_start_matches(marks);
    head.eq_ignore_ascii_case("acceptance criteria")
        && (rest.is_empty() || rest.starts_with([':', '(']))
}

/// The `ACCEPTANCE CRITERIA` block a first planning turn adds to its context
/// for a request whose criteria pi-rukas does not read: the `criteria` of the
/// request's `pi-spec v1` block, one bullet each, `<id>: <text>` on one line.
/// Nothing when the request has no block with criteria, or has an Acceptance
/// criteria heading of its own, which pi-rukas reads already.
pub fn criteria_directive(request: &str) -> Option<String> {
    if request.lines().any(criteria_heading) {
        return None;
    }
    let block = spec_block(request)?;
    let lines: Vec<&str> = block.lines().collect();
    let inner = lines.get(1..lines.len().saturating_sub(1))?.join("\n");
    let spec: Value = serde_json::from_str(&inner).ok()?;
    let one_line = |v: Option<&Value>| text_of(v).split_whitespace().collect::<Vec<_>>().join(" ");
    let items: Vec<String> = spec
        .get("criteria")?
        .as_array()?
        .iter()
        .filter_map(|c| {
            let text = one_line(c.get("text"));
            let id = one_line(c.get("id"));
            match (id.is_empty(), text.is_empty()) {
                (_, true) => None,
                (true, false) => Some(format!("- {text}")),
                (false, false) => Some(format!("- {id}: {text}")),
            }
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(format!("{CRITERIA_DIRECTIVE}\n{}", items.join("\n")))
}

/// The fence line of a body: the character and length of its run and what
/// stands after it, or nothing when the line is not one. At most three spaces
/// of indent count as indent - a tab is not one, so a tab-indented line is
/// prose - and the run must be one character repeated.
fn fence_line(line: &str) -> Option<(u8, usize, String)> {
    let t = line
        .strip_prefix("   ")
        .or_else(|| line.strip_prefix("  "))
        .or_else(|| line.strip_prefix(' '))
        .unwrap_or(line);
    let bytes = t.as_bytes();
    let ch = *bytes.first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let run = bytes.iter().take_while(|b| **b == ch).count();
    if run < 3 {
        return None;
    }
    Some((ch, run, t[run..].trim().to_string()))
}

/// A block's lines with a table rule right after the info string taken off,
/// and the blank lines at its ends trimmed.
fn block_text(lines: &[&str]) -> String {
    let mut from = 0;
    if lines.first().is_some_and(|l| {
        let t = l.trim();
        t.len() >= 3 && t.chars().all(|c| c == '-' || c == '*')
    }) {
        from = 1;
    }
    let mut to = lines.len();
    while to > from && lines[to - 1].trim().is_empty() {
        to -= 1;
    }
    while from < to && lines[from].trim().is_empty() {
        from += 1;
    }
    lines[from..to].join("\n")
}

/// The note `clip` leaves at the end of a body it cut, without its leading
/// blank line: the length `keep_spec` has to leave room for on top of the
/// block it puts back.
const NOTE: &str = "(cut: the plan is longer than a comment holds)";

/// The body tehas writes over an issue: `new`, with the `pi-spec v1` block
/// `was` holds put back at the top when `new` brings none of its own, so a
/// plan whose draft carries no block keeps the spec lean reads. The console's
/// `update` passes the block pi-rukas assembled as `was` when it kept one.
///
/// This is the writer's own step, not part of [`made`]: the two callers of
/// `made` each apply it once, at the point where they put text on the issue, so
/// every check a caller makes of a body it is about to write is a check of the
/// same text GitHub will hold. The dispatcher's plan write asks the links and
/// the sum of the body before it calls this, and so approves only what the
/// person was shown; the console's `update` calls it here and writes the result
/// straight out.
///
/// The block is kept inside the length `clip` holds a body to, not on top of
/// it: `new` is cut to what is left under the block first, so what is written
/// never passes CLIP and the note of the cut keeps its place at the end.
pub fn keep_spec(new: &str, was: &str) -> String {
    let Some(block) = spec_block(was) else {
        return new.to_string();
    };
    if spec_block(new).is_some() {
        return new.to_string();
    }
    // What the block, the blank line under it, the line break before a cut's
    // note and the note itself cost, and the room the plan has left. A body
    // that reaches `clip` at the ceiling leaves it 46 over, its note added
    // after the cut, so the note is paid for twice: once from the ceiling and
    // once from what the ceiling may become. Nothing written here passes CLIP.
    let overhead = block.chars().count() + 3 + 2 * NOTE.len();
    // No room at all is no plan: the block stands on its own, without the
    // empty body and the note of a cut that would follow it.
    if CLIP <= overhead {
        return block;
    }
    let new = clip(new.trim_end_matches(['\n', '\r']), "the plan");
    let new = match new.char_indices().nth(CLIP - overhead) {
        Some((at, _)) => format!(
            "{}\n\n(cut: the plan is longer than a comment holds)",
            &new[..at]
        ),
        None => new,
    };
    if new.is_empty() {
        return block;
    }
    let candidate = format!("{block}\n\n{new}");
    // A body of two blocks is no body: the plan stands on its own.
    if spec_block(&candidate).is_none() {
        return new;
    }
    candidate
}

/// A plan of no open points the dispatcher can write without another tick:
/// its draft, the issue as `gh issue view` gives it, and the comment that
/// ends the session, or why it asks as any other draft does. `checked` is the
/// only reader of the draft's own text here, so the answer is the one
/// [`made`] gives for the same draft: the caller writes only when it is `Ok`.
pub fn clear_plan(draft: &Value, was: &Value, said: &str) -> Result<Made, Refused> {
    made(draft, was, said)
}

/// `said`, then the text the plan replaces, folded and quoted. `draft` is the
/// {title, spec} a turn kept, `was` the issue's {title, body}.
///
/// The draft's own title and spec are what the check reads, and the reason is
/// a [`Refused`], not a line of prose: a writer that only has to stop answers
/// with the reason in the comment it posts. A title the draft does not name is
/// the issue's own, which is the text a person wrote and approved with their
/// label, so nothing written here refuses it.
pub fn made(draft: &Value, was: &Value, said: &str) -> Result<Made, Refused> {
    // One check for every writer, so the dispatcher's pre-check and this write
    // cannot disagree about the same draft.
    checked(draft)?;
    let named = text_of(draft.get("title"));
    let title = if named.is_empty() {
        text_of(was.get("title"))
    } else {
        named
    };
    let old = format!(
        "# {}\n\n{}",
        text_of(was.get("title")),
        text_of(was.get("body"))
    );
    let quoted = clip(&old, "the text")
        .split('\n')
        .map(|l| format!("> {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    // The bot writes both onto the issue, and the planner wrote them, so a
    // marker in either is broken (text::disarm).
    let title = crate::text::disarm(&title).into_owned();
    Ok(Made {
        title,
        // The body as the draft wrote it. Each writer puts a person's spec
        // block back on top when it is ready to write: the checks a dispatcher
        // makes of this body are then the checks of the text the draft made,
        // not of what the issue already held. `keep_spec` clips as it goes.
        body: crate::text::disarm(&clip(&text_of(draft.get("spec")), "the plan")).into_owned(),
        text: format!(
            "{said}\n\n<details><summary>The text it replaced</summary>\n\n{quoted}\n\n</details>"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A body of the shape pi-rukas writes: the block at the top, its table
    /// rule after the info string.
    fn spec_body(rest: &str) -> String {
        format!("```pi-spec v1\n---\n{{\"intent\":\"faster search\"}}\n```\n\n{rest}")
    }

    #[test]
    fn a_spec_block_is_only_a_lone_fence_of_valid_json() {
        assert!(spec_block(&spec_body("## Outcome\n\nDo it.")).is_some());
        // Indentation, no rule, and a document that is no object all read.
        assert!(spec_block("```pi-spec v1\n[1,2]\n```").is_some());
        assert!(spec_block("  ```pi-spec v1\n\n{\"a\":1}\n\n  ```").is_some());
        // None at all, no JSON, and more than one block are all no block.
        assert!(spec_block("## Outcome\n\nDo it.").is_none());
        assert!(spec_block("").is_none());
        assert!(spec_block("```pi-spec v1\nnot json\n```").is_none());
        assert!(spec_block(&format!("{}\n\n{}", spec_body("a"), spec_body("b"))).is_none());
        // Another info string, a prefix of it, and an unterminated fence.
        assert!(spec_block("```json\n{\"a\":1}\n```").is_none());
        assert!(spec_block("```pi-spec\n{\"a\":1}\n```").is_none());
        assert!(spec_block("```pi-spec v1x\n{\"a\":1}\n```").is_none());
        assert!(spec_block("```pi-spec v1\n{\"a\":1}").is_none());
        // The fence of an indented example is not a fence: it closes the
        // example, so the `pi-spec v1` line inside it names no block and the
        // body has no block to read.
        assert!(spec_block("```markdown\n```pi-spec v1\n{\"a\":1}\n```\n```").is_none());
        // A rule alone is no JSON, and a block of nothing is no block.
        assert!(spec_block("```pi-spec v1\n---\n```").is_none());
    }

    /// The fence rules of CommonMark, which pi-rukas's `fencedBlocks` follows
    /// and a loose reader does not: the shapes where the two answer
    /// differently. Each row is the verdict the driver's reader gives too.
    #[test]
    fn a_fence_closes_only_on_a_run_at_least_as_long_as_its_opener() {
        // The shape pi-rukas writes for an example: a drafted spec fence
        // renamed `pi-spec-example v1` inside a longer wrapper, next to the
        // block of the body, which reads. The wrapper is closed by the longer
        // run, exactly as pi-rukas closes it, and the example's own fence
        // names nothing either way.
        assert!(spec_block(
            "````\n```pi-spec-example v1\nnope\n```\n````\n\n```pi-spec v1\n{\"a\":1}\n```"
        )
        .is_some());
        // A four-backtick opener is not closed by a three-backtick line, so
        // its JSON never stands in a closed block of its own.
        assert!(spec_block("````pi-spec v1\n{\"a\":1}\n```").is_none());
        // A longer run closes a shorter one, as CommonMark says: the dangling
        // run of the loose reading names nothing.
        assert!(spec_block("```pi-spec v1\n{\"a\":1}\n````").is_some());
        // Four spaces of indent is an indented code block, not a fence, so
        // neither line of this block is a fence line at all.
        assert!(spec_block("```pi-spec v1\n{\"a\":1}\n    ```").is_none());
        // Three spaces of indent are still a fence, opener and closer alike.
        assert!(spec_block("   ```pi-spec v1\n{\"a\":1}\n   ```").is_some());
        // A tab is not indent: nothing here is a fence.
        assert!(spec_block("\t```pi-spec v1\n{\"a\":1}\n\t```").is_none());
        // A `pi-spec v1` line quoted inside a `~~~` region: the inner run of
        // backticks opens and closes a block of its own there, as CommonMark
        // and pi-rukas both read it, and that block is the body's one. A
        // reader that held the region shut would name no block at all here.
        assert!(spec_block("~~~\n```pi-spec v1\n{\"a\":1}\n```\n~~~").is_some());
        // A `~~~` run cannot close a backtick block, so this one never ends.
        assert!(spec_block("```pi-spec v1\n{\"a\":1}\n~~~").is_none());
        // A closing run that carries an info string closes nothing: this is
        // one block of text, not JSON.
        assert!(spec_block("```pi-spec v1\n{\"a\":1}\n```json").is_none());
        // Scalars of any shape read: the type of the document is lean's
        // question, not this one's.
        assert!(spec_block("```pi-spec v1\nnull\n```").is_some());
        assert!(spec_block("```pi-spec v1\n42\n```").is_some());
    }

    #[test]
    fn the_criteria_of_a_requests_spec_block_become_a_directive() {
        let spec = r#"{"problem": "p", "criteria": [
            {"id": "AC1", "text": "A hit with the flag\n  writes `is_dps`.", "verify": "test"},
            {"id": "", "text": "No id is kept as text."},
            {"id": "AC3", "text": "  "},
            "no object"
        ]}"#;
        let request = format!("# chore: Give it\n\nWhy.\n\n```pi-spec v1\n{spec}\n```\n");
        assert_eq!(
            criteria_directive(&request).unwrap(),
            "ACCEPTANCE CRITERIA:\n- AC1: A hit with the flag writes `is_dps`.\n- No id is kept as text."
        );
        // A heading of the request's own is read by pi-rukas already.
        for heading in [
            "## Acceptance criteria",
            "**Acceptance Criteria:**",
            "ACCEPTANCE CRITERIA",
        ] {
            let own = format!("{request}\n{heading}\n- It works.\n");
            assert_eq!(criteria_directive(&own), None, "{heading}");
        }
        // No block, a block of no criteria, or criteria of no text say nothing.
        let prose = format!("{request}\nAcceptance criteria are in the block above.\n");
        assert!(criteria_directive(&prose).is_some());
        assert_eq!(criteria_directive("# T\n\nBody.\n"), None);
        assert_eq!(
            criteria_directive("```pi-spec v1\n{\"criteria\": []}\n```"),
            None
        );
        assert_eq!(
            criteria_directive("```pi-spec v1\n{\"problem\": \"p\"}\n```"),
            None
        );
        assert_eq!(
            criteria_directive("```pi-spec v1\n{\"criteria\": [{\"id\": \"AC1\"}]}\n```"),
            None
        );
    }

    #[test]
    fn a_body_written_over_an_issue_keeps_its_spec_block_at_the_top() {
        let was = spec_body("## Outcome\n\nWhat the person wrote.");
        let block = spec_block(&was).unwrap();
        let kept = keep_spec("## Plan\n\nDo it.", &was);
        assert_eq!(kept, format!("{block}\n\n## Plan\n\nDo it."));
        assert!(kept.starts_with("```pi-spec v1"));
        // A new body of its own is written as it stands, and one with no block
        // anywhere is written as today.
        let mine = spec_body("## Plan\n\nDo it.");
        assert_eq!(keep_spec(&mine, &was), mine);
        assert_eq!(keep_spec("## Plan", "no block here"), "## Plan");
        assert_eq!(keep_spec("## Plan", ""), "## Plan");
        // Nothing to put under the block: the block alone.
        assert_eq!(keep_spec("\n\n", &was), block);
    }

    /// What is written never passes the length a comment holds, and a body
    /// that would end up with two blocks keeps neither of the two it did not
    /// choose.
    #[test]
    fn a_kept_block_stays_inside_the_length_a_comment_holds() {
        let was = spec_body("What the person wrote.");
        let block = spec_block(&was).unwrap();
        let long = "x".repeat(CLIP);
        let kept = keep_spec(&long, &was);
        assert!(kept.starts_with(&block));
        // The note of the cut keeps its place at the end of the body: the
        // block plus the cut body, with the blank line between them, are all
        // inside what a comment holds, and so is the line break before the
        // note (the one character `clip` itself does not count).
        assert!(kept.ends_with(NOTE));
        assert!(kept.chars().count() <= CLIP, "{}", kept.chars().count());
        // The block alone is what is written when there is no room left under
        // it: the filler is a JSON array of `"x"` whose last element is written
        // apart from the rest to keep its comma out of the JSON, and at 39907
        // characters the block leaves nothing of the ceiling for a plan. What
        // comes back is the block as `spec_block` writes it, with no note of a
        // cut under it.
        let fat = format!("```pi-spec v1\n[{}]\n```", "\"x\",".repeat(9971));
        let fat = fat.replacen("]", "\"x\"]", 1);
        let whole = spec_block(&fat).expect("a block of the ceiling");
        assert!(whole.chars().count() <= CLIP, "{}", whole.chars().count());
        assert_eq!(keep_spec("anything", &fat), whole);
        assert_eq!(keep_spec("a plan of its own", &fat), whole);
        // A plan that fills the room a block leaves is written whole, and the
        // line above the ceiling is cut: the room is what `keep_spec` cuts at.
        let fits = "x".repeat(CLIP - (block.chars().count() + 3 + 2 * NOTE.len()));
        assert_eq!(keep_spec(&fits, &was), format!("{block}\n\n{fits}"));
        let over = keep_spec(&format!("{}x", fits), &was);
        assert!(over.ends_with(NOTE));
        // The pair no writer reaches today, checked rather than assumed: a
        // `new` that holds none of its own but would end up beside the kept
        // block twice is not a body, so the plan stands as it is.
        let both = "```pi-spec v1\n{\"a\":1}\n```\nx";
        assert_eq!(keep_spec(both, "```pi-spec v1\n{\"b\":2}\n```"), both);
    }

    #[test]
    fn a_plan_with_no_title_keeps_the_issues() {
        let was = json!({"title": "search", "body": "fast"});
        let m = made(&json!({"title": "", "spec": "Do it."}), &was, "Said.").unwrap();
        assert_eq!(m.title, "search");
        assert_eq!(m.body, "Do it.");
        assert_eq!(
            m.text,
            "Said.\n\n<details><summary>The text it replaced</summary>\n\n> # search\n> \n> fast\n\n</details>"
        );
    }

    #[test]
    fn made_disarms_the_planners_title_and_body() {
        let was = json!({"title": "search", "body": "fast"});
        let agent = "<!-- tehas:millwright issue=5 action=retry -->";
        let draft =
            json!({"title": format!("Search {agent}"), "spec": format!("Do it.\n{agent}\n")});
        let m = made(&draft, &was, "Said.").unwrap();
        assert!(!crate::text::has_marker(&m.title), "{}", m.title);
        assert!(!crate::text::has_marker(&m.body), "{}", m.body);
        assert!(m.body.starts_with("Do it.\n"), "{}", m.body);
    }

    #[test]
    fn a_plan_with_no_text_makes_nothing() {
        let was = json!({"title": "search", "body": "fast"});
        assert!(made(&json!({"title": "t", "spec": ""}), &was, "Said.").is_err());
        assert!(made(&json!({"title": "t"}), &was, "Said.").is_err());
        // A spec of nothing is refused before the title is read: there is
        // nothing to write whichever way the title reads.
        assert!(made(&json!({"title": "", "spec": ""}), &was, "Said.").is_err());
        // No text is its own answer: the rule is `textcheck`'s empty title,
        // and the next step names a plan of nothing.
        let refused = made(&json!({"title": "t", "spec": ""}), &was, "Said.")
            .expect_err("a draft with no text");
        assert_eq!(refused.reason, Reason::EmptyTitle);
        assert!(
            refused
                .line()
                .contains("A plan of nothing has nothing to write"),
            "{}",
            refused.line()
        );
    }

    #[test]
    fn a_draft_whose_title_says_nothing_makes_nothing_and_says_why() {
        let was = json!({"title": "search", "body": "fast"});
        let refused = made(
            &json!({"title": "EPIC: EPIC", "spec": "Do it."}),
            &was,
            "Said.",
        )
        .expect_err("a title of nothing but prefixes");
        assert_eq!(refused.reason, Reason::OnlyPrefix);
        // The words a person reads are `textcheck`'s own, with the refused
        // title quoted after them.
        assert_eq!(refused.line, "the title is nothing but prefixes: epic epic");
        assert!(
            refused.line().contains("nothing was written"),
            "{}",
            refused.line()
        );
        // The issue keeps the title it has, which the draft did not name, so
        // the draft cannot refuse it: it is what a person wrote.
        assert!(made(&json!({"title": "", "spec": "Do it."}), &was, "Said.").is_ok());
        assert!(made(
            &json!({"title": "faster search", "spec": "Do it."}),
            &was,
            "Said."
        )
        .is_ok());
    }

    #[test]
    fn a_draft_whose_text_is_the_brief_makes_nothing_and_says_why() {
        let was = json!({"title": "search", "body": "fast"});
        let refused = made(
            &json!({"title": "faster search",
                    "spec": "Descriptor: what the issue is\n\n## Prior context inventory\n\n- the file"}),
            &was,
            "Said.",
        )
        .expect_err("a body that is the brief");
        assert_eq!(refused.reason, Reason::Briefed);
        assert!(
            refused.line.starts_with("the body is the planner's brief"),
            "{}",
            refused.line
        );
        assert!(refused.line().contains("Reply"), "{}", refused.line());
    }

    /// The one check the console and the dispatcher both ask: no writing, and
    /// the same answer for the same draft, whatever the issue holds.
    #[test]
    fn the_check_of_a_draft_answers_alone_and_the_same_for_every_writer() {
        let passes = json!({"title": "feat: search is fast", "spec": "## Outcome\n\nThe p95 is under 100 ms."});
        assert!(checked(&passes).is_ok());
        assert_eq!(
            checked(&passes),
            made(&passes, &Value::Null, "Said.").map(|_| ())
        );
        let refuses =
            json!({"title": "work work", "spec": "## Outcome\n\nThe p95 is under 100 ms."});
        let alone = checked(&refuses).expect_err("a title of one word twice");
        let wrote = made(
            &refuses,
            &json!({"title": "search", "body": "fast"}),
            "Said.",
        )
        .expect_err("the same draft, of the write");
        assert_eq!(alone, wrote);
        // A draft that names no title is asked only of its text, so the
        // issue's own title cannot refuse anything here.
        assert!(checked(&json!({"title": "", "spec": "The console reads the key."})).is_ok());
    }

    /// The dispatcher's pre-check: it answers only for an `update` that holds
    /// the draft's own two strings, and never for one that does not, so a
    /// document of another writer is left to its writer.
    #[test]
    fn the_pre_check_answers_only_for_an_update_that_holds_the_draft() {
        let refused = json!({"title": "EPIC: EPIC", "spec": "Do it."});
        let out = json!({"update": {"title": "EPIC: EPIC", "body": "Do it."}});
        let why = refused_by_draft(&out, &refused).expect("an update of the refused draft");
        assert_eq!(why.reason, Reason::OnlyPrefix);
        // An update that is no object, one that holds no text to write, and a
        // draft that is not the draft the document was made from (it holds
        // neither of its two strings) each answer nothing.
        assert!(refused_by_draft(&json!({"update": 42}), &refused).is_none());
        assert!(
            refused_by_draft(&json!({"update": {"title": 42, "body": 42}}), &refused).is_none()
        );
        assert!(refused_by_draft(
            &json!({"update": {"title": "other", "body": "another"}}),
            &refused
        )
        .is_none());
        // A document with no `update` at all asks nothing of a draft, and its
        // own question is left as its writer made it: this is not a refusal.
        assert!(!carries_update(
            &json!({"question": {"question": "Q", "options": [1]}})
        ));
        assert!(carries_update(
            &json!({"update": {"title": "t", "body": "b"}})
        ));
        assert!(!carries_update(
            &json!({"update": {"title": 42, "body": "b"}})
        ));
        assert!(
            refused_by_draft(&out, &json!({"title": "faster search", "spec": "Do it."})).is_none()
        );
        assert!(refused_by_draft(&json!({"text": "t"}), &refused).is_none());
        // A non-string title or body is not answered either: it is no text the
        // write could put on the issue, and the writer that made it is asked.
        assert!(
            refused_by_draft(&json!({"update": {"title": 42, "body": 42}}), &refused).is_none()
        );
        // A draft that passes is no refusal.
        assert!(refused_by_draft(
            &json!({"update": {"title": "t", "body": "Do it."}}),
            &json!({"title": "t", "spec": "Do it."})
        )
        .is_none());
    }

    /// The refusal document: the same line for the text and the question, no
    /// `update` for a poll to write, and the console's session kept.
    #[test]
    fn the_refusal_document_asks_and_holds_nothing_to_write() {
        let refused = made(
            &json!({"title": "EPIC: EPIC", "spec": "Do it."}),
            &Value::Null,
            "Said.",
        )
        .expect_err("a refused draft");
        let out = refusal_document(&refused.line(), &refused.line(), Some(&json!("s-1")));
        assert_eq!(out["session"], "s-1");
        assert!(out.get("update").is_none());
        assert_eq!(out["text"], refused.line());
        assert_eq!(out["question"]["question"], refused.line());
        assert_eq!(out["question"]["options"], json!([]));
        assert_eq!(out["text"].as_str().unwrap().lines().count(), 1);
        // Read back as the refusal it is, and not one when the document holds
        // an `update` beside the line: such a document is not a refusal.
        assert!(is_refusal(&out));
        assert!(!is_refusal(&json!({"update": null})));
        assert!(!is_refusal(&json!({"text": "Drafted."})));
        assert!(is_refusal(&refusal_document(&refused.line(), "", None)));
        // A document that holds an `update` is no refusal whatever its text,
        // and an empty text is no refusal.
        assert!(!is_refusal(
            &json!({"text": refused.line(), "update": {"title": "x", "body": "y"}})
        ));
        assert!(!is_refusal(
            &json!({"text": "", "question": {"question": "", "options": []}})
        ));
    }
}
