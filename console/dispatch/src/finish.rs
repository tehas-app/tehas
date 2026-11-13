//! The poll's finish step (#1665, part 2 of #1131): a person or the millwright
//! asks on the issue for the factory to open the draft pull request of a
//! stopped cycle, and the poll runs `tehas-cycle finish` for it.
//!
//! The ask is a comment that starts with
//! `<!-- tehas:millwright issue=<n> action=finish -->`, the reason after it,
//! and then the label `tehas:finish`, applied by the comment's author within
//! ten minutes of it. An allowed labeller may ask. The millwright may ask
//! within its grant: a person approved a cycle of the issue, the text is the
//! text they approved, nobody outside the list changed it since, and the
//! millwright's starts on it are within `TEHAS_MILLWRIGHT_MAX_RETRIES`. An ask
//! from anyone else is ignored and the label stays.
//!
//! The step takes no slot and starts no agent and no cycle. Whatever the
//! answer, the label comes off first, so a finish is run once for one ask, and
//! the issue is told: the draft's URL, or the one line of why not. The line the
//! poll logs is the record: `finish #<n>: ...` in `dispatch.log`, which
//! `factory_decisions` answers.

use crate::decide::{base_counts, fixer_base, fixer_starts};
use crate::read::host::epoch;
use crate::settings::account;
use crate::text::body_sha256;
use crate::Settings;
use serde_json::Value;
use tehas_core::call::Ran;
use tehas_core::events::{changed_after, first_unallowed_rename, label_actor, Event, Events};
use tehas_core::text::disarm;

/// The label of the ask.
pub const LABEL: &str = "tehas:finish";
/// How long before the label its comment may be written.
const WINDOW: i64 = 600;
/// The most asks one poll runs. The rest wait for the next poll.
pub const PER_POLL: usize = 3;
/// The reason is cut to this many characters before it is passed on.
const REASON_MAX: usize = 2000;
/// A reason in the poll's log line is cut to this many characters.
const LINE_MAX: usize = 200;

/// The comment's first line.
pub fn marker(n: u64) -> String {
    format!("<!-- tehas:millwright issue={n} action=finish -->")
}

/// The issues of the snapshot that carry the label, by number, the control
/// issue left out.
pub fn asked(snap: &Value, control: u64) -> Vec<u64> {
    let mut out: Vec<u64> = snap
        .get("issues")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(_, i)| {
            i.pointer("/work/labels")
                .and_then(Value::as_array)
                .is_some_and(|l| l.iter().any(|x| x["name"] == LABEL))
        })
        .filter_map(|(n, _)| n.parse().ok())
        .filter(|n| *n != control)
        .collect();
    out.sort_unstable();
    out
}

/// Who applied the label and how they may ask, or the line that ignores the
/// ask: the label of an account outside the list that is not the millwright.
fn asker(c: &Settings, n: u64, ev: &[Event]) -> Result<(String, String, &'static str), String> {
    let (who, at) = label_actor(ev, LABEL);
    if c.allowed(&who) {
        Ok((who, at, "label"))
    } else if c.is_millwright(&who) {
        Ok((who, at, "millwright"))
    } else {
        let who = if who.is_empty() { "unknown" } else { &who };
        Err(format!(
            "ignore #{n}: {LABEL} was applied by '{who}', not an allowed account"
        ))
    }
}

/// The asks the step judges, in order, and the lines of those it ignores.
/// An ask is ignored on its label events before the poll's cap, so the label
/// of an account not allowed takes no turn of an allowed one. An ask whose
/// events cannot be read is kept: it waits.
pub fn counted(c: &Settings, snap: &Value, control: u64) -> (Vec<u64>, Vec<String>) {
    let mut asks = Vec::new();
    let mut ignored = Vec::new();
    for n in asked(snap, control) {
        if let Events::Read(ev) = Events::parse(&snap["issues"][n.to_string()]["events"]) {
            if let Err(line) = asker(c, n, &ev) {
                ignored.push(line);
                continue;
            }
        }
        asks.push(n);
    }
    (asks, ignored)
}

/// What the step knows of one asked issue.
pub struct Ask {
    pub n: u64,
    pub events: Events,
    /// The poll's GraphQL read of the issue's comments, `{"failed": true}`
    /// when it failed.
    pub comments: Value,
    /// The edit record of the snapshot: title, body, lastEditedAt, editor.
    pub edit: Value,
    /// approval-current shaped as the rules read it; Err when the store
    /// could not answer.
    pub approval: Result<Option<Value>, ()>,
    /// A cycle of it holds its lock, carries `tehas:running`, or starts in
    /// this poll.
    pub running: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing changes: the line the poll logs.
    Ignore(String),
    Wait(String),
    /// The label comes off and the issue is told why.
    Refuse {
        by: String,
        why: String,
    },
    /// The finish runs.
    Finish {
        by: String,
        how: &'static str,
        reason: String,
    },
}

fn s<'a>(v: &'a Value, p: &str) -> &'a str {
    v.pointer(p).and_then(Value::as_str).unwrap_or("")
}

/// The reason of the asker's newest marked comment in the window before the
/// label at `at`, or why there is none.
fn reason_of(a: &Ask, who: &str, at: &str) -> Result<String, String> {
    let Some(nodes) = a
        .comments
        .pointer("/data/repository/issue/comments/nodes")
        .and_then(Value::as_array)
    else {
        return Err(String::new());
    };
    let mark = marker(a.n);
    let since = epoch(at).map(|t| t - WINDOW).unwrap_or(i64::MAX);
    let newest = nodes
        .iter()
        .filter(|c| account(s(c, "/author/login")) == account(who))
        .filter(|c| {
            c["body"]
                .as_str()
                .is_some_and(|b| b.trim_start().starts_with(&mark))
        })
        .filter(|c| {
            let created = s(c, "/createdAt");
            created <= at && epoch(created).is_some_and(|t| t >= since)
        })
        .max_by(|x, y| s(x, "/createdAt").cmp(s(y, "/createdAt")));
    let Some(c) = newest else {
        return Err(format!(
            "no comment of {who} that starts with `{mark}` was written in the ten minutes before the label"
        ));
    };
    let edited = s(c, "/lastEditedAt");
    let editor = c.pointer("/editor/login").and_then(Value::as_str);
    if !edited.is_empty() && (edited > at || editor.is_some_and(|e| account(e) != account(who))) {
        return Err("its comment was edited after the label or by another account".into());
    }
    let body = c["body"].as_str().unwrap_or("").trim_start();
    let reason = body[mark.len()..].trim();
    if reason.is_empty() {
        return Err("its comment gives no reason after the marker".into());
    }
    Ok(reason.chars().take(REASON_MAX).collect())
}

/// The millwright's grant, as its `tehas:ready` has it, with one difference:
/// a finish starts no cycle, so the last start the cap allowed may still be
/// finished. None when the grant holds.
fn grant(c: &Settings, a: &Ask) -> Result<Option<String>, String> {
    let record = match &a.approval {
        Err(()) => return Err("its approvals on the state store could not be read".into()),
        Ok(r) => r,
    };
    let none = || Some("no person approved a cycle of it".to_string());
    let Some(record) = record else {
        return Ok(none());
    };
    let Some(base) = fixer_base(record).filter(|b| base_counts(c, b)) else {
        return Ok(none());
    };
    let by = base["by"].as_str().unwrap_or("");
    let since = base["at"].as_str().unwrap_or("");
    if body_sha256(&a.edit) != base["body_sha256"].as_str().unwrap_or("") {
        return Ok(Some(format!("its text is not the text {by} approved")));
    }
    let Some(had) = fixer_starts(record) else {
        return Ok(Some(
            "its approval record does not say how many cycles the millwright started".into(),
        ));
    };
    if had as i64 > c.fixer_max_retries {
        return Ok(Some(format!(
            "the millwright has started {had} cycles on it since {by} approved it, more than TEHAS_MILLWRIGHT_MAX_RETRIES"
        )));
    }
    let edited = s(&a.edit, "/lastEditedAt");
    let editor = s(&a.edit, "/editor/login");
    if !edited.is_empty() && changed_after(edited, editor, since, by) && !c.allowed(editor) {
        return Ok(Some(format!(
            "its title or text was changed since {by} approved it"
        )));
    }
    if let Events::Read(ev) = &a.events {
        if first_unallowed_rename(ev, since, by, &|w| c.allowed(w)).is_some() {
            return Ok(Some(format!(
                "its title or text was changed since {by} approved it"
            )));
        }
    }
    Ok(None)
}

/// What the step does with one ask.
pub fn judge(c: &Settings, a: &Ask) -> Verdict {
    let n = a.n;
    let ev: &[Event] = match &a.events {
        Events::Read(v) => v,
        _ => {
            return Verdict::Wait(format!(
                "wait #{n}: {LABEL} waits: its label events could not be read"
            ))
        }
    };
    let (who, at, how) = match asker(c, n, ev) {
        Ok(a) => a,
        Err(line) => return Verdict::Ignore(line),
    };
    let refuse = |why: String| Verdict::Refuse {
        by: who.clone(),
        why,
    };
    if a.running {
        return refuse("a cycle of it is running: ask again once it has stopped".into());
    }
    let reason = match reason_of(a, &who, &at) {
        Ok(r) => r,
        Err(why) if why.is_empty() => {
            return Verdict::Wait(format!(
                "wait #{n}: {LABEL} waits: its comments could not be read"
            ))
        }
        Err(why) => return refuse(why),
    };
    if how == "millwright" {
        match grant(c, a) {
            Err(why) => return Verdict::Wait(format!("wait #{n}: {LABEL} waits: {why}")),
            Ok(Some(why)) => return refuse(format!("the millwright may not ask it: {why}")),
            Ok(None) => {}
        }
    }
    Verdict::Finish {
        by: who,
        how,
        reason,
    }
}

/// What the step needs of the host: the label off, the finish run, the issue
/// told and the poll's log.
pub trait Hands {
    /// `tehas:finish` off the issue; false when it did not come off.
    fn unlabel(&mut self, n: u64) -> bool;
    /// The draft's URL, or the one line of why not.
    fn finish(&mut self, n: u64, reason: &str) -> Result<String, String>;
    fn say(&mut self, n: u64, body: &str);
    fn log(&mut self, line: String);
}

fn one_line(t: &str) -> String {
    let flat: String = t
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = flat.chars().take(LINE_MAX).collect();
    if flat.chars().count() > LINE_MAX {
        out.push_str("...");
    }
    out
}

/// The reply of a finish that opened its pull request.
pub fn opened(n: u64, by: &str, url: &str) -> String {
    format!(
        "<!-- tehas:finish issue={n} result=opened -->\nTehas took `{LABEL}` off this issue and opened a draft pull request from what its stopped cycle left, as {} asked: {}\n\nIts gate did not pass. The checks and the review decide on it as on any other pull request.",
        disarm(by),
        disarm(url)
    )
}

/// The reply of a finish refused.
pub fn refused(n: u64, why: &str) -> String {
    format!(
        "<!-- tehas:finish issue={n} result=refused -->\nTehas took `{LABEL}` off this issue and finished nothing: {}. To ask again, write a new comment that starts with `{}` and apply `{LABEL}`.",
        disarm(why),
        marker(n)
    )
}

/// The step over the asks, in order.
pub fn step(c: &Settings, asks: &[Ask], hands: &mut dyn Hands) {
    for a in asks {
        let n = a.n;
        match judge(c, a) {
            Verdict::Ignore(line) | Verdict::Wait(line) => hands.log(line),
            Verdict::Refuse { by, why } => {
                if hands.unlabel(n) {
                    hands.log(format!(
                        "finish #{n}: refused, asked by {by}: {}",
                        one_line(&why)
                    ));
                    hands.say(n, &refused(n, &why));
                }
            }
            Verdict::Finish { by, how, reason } => {
                if !hands.unlabel(n) {
                    continue;
                }
                match hands.finish(n, &reason) {
                    Ok(url) => {
                        hands.log(format!(
                            "finish #{n}: opened {url}, asked by {by} ({how}): {}",
                            one_line(&reason)
                        ));
                        hands.say(n, &opened(n, &by, &url));
                    }
                    Err(why) => {
                        hands.log(format!(
                            "finish #{n}: refused, asked by {by} ({how}): {}",
                            one_line(&why)
                        ));
                        hands.say(n, &refused(n, &why));
                    }
                }
            }
        }
    }
}

/// What the command that refused said, from the line `tehas-cycle finish`
/// prints before its refusal, as a line for the poll's log. The issue is told
/// only the refusal: this line can quote what the cycle wrote.
pub fn said(n: u64, ran: &Ran) -> Option<String> {
    let lead = format!("tehas-cycle: finish: #{n}: the command said: ");
    ran.err
        .lines()
        .find_map(|l| l.trim().strip_prefix(lead.as_str()))
        .filter(|w| !w.trim().is_empty())
        .map(|w| {
            format!(
                "the finish of #{n} was refused, the command said: {}",
                one_line(w)
            )
        })
}

/// What `tehas-cycle finish` answered: the URL it printed, or its line of why
/// not, without the command's own prefix.
pub fn outcome(n: u64, ran: &Ran, bound: u64) -> Result<String, String> {
    let last = |t: &str| {
        t.lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .unwrap_or("")
            .to_string()
    };
    match ran.status {
        Some(0) => {
            let url = last(&ran.out);
            if url.starts_with("https://") && !url.contains(char::is_whitespace) {
                Ok(url)
            } else {
                Err("tehas-cycle finish printed no pull request".into())
            }
        }
        Some(1) => {
            let line = last(&ran.err);
            let prefix = format!("tehas-cycle: finish: #{n}: ");
            match line.strip_prefix(&prefix) {
                Some(why) if !why.is_empty() => Err(why.to_string()),
                _ if !line.is_empty() => Err(one_line(&line)),
                _ => Err("tehas-cycle finish refused and said nothing".into()),
            }
        }
        Some(rc) => Err(format!("tehas-cycle finish could not run (exit {rc})")),
        // A program that could not be started says so in `err`, with no
        // output; one the bound ended said nothing of its own.
        None if ran.err.ends_with("could not be run") => {
            Err("tehas-cycle finish could not be run".into())
        }
        None => Err(format!(
            "tehas-cycle finish did not end within {bound} seconds"
        )),
    }
}

#[cfg(test)]
mod tests;
