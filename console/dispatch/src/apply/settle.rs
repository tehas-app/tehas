//! The journal settled at the start of a poll (section 5 of the state store's
//! spec): an action a poll intended and did not settle, because it died
//! between two writes, is read again and settled from what GitHub says now.
//! Never replayed: the poll that settles it decides afresh whether anything
//! is still owed. A write never attempted made no call, and is abandoned.
//! A row that does not describe itself is left open with one WARN.

use super::{text, word, Writer};
use serde_json::{json, Value};

/// How a fresh read settles one action.
enum Found {
    Done(Value),
    Abandoned(&'static str),
    /// The read failed: the action stays for the next poll.
    Unread(String),
}

/// The words of a gh call that follow a flag, each time it is given.
fn after<'a>(args: &'a [Value], flag: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter(|w| w[0].as_str() == Some(flag))
        .filter_map(|w| w[1].as_str())
        .collect()
}

/// The field of an open action as a positive integer, or None when it is
/// missing, not a number, or not above zero.
fn positive(a: &Value, k: &str) -> Option<u64> {
    a.get(k).and_then(Value::as_u64).filter(|n| *n > 0)
}

/// A word of a journal row for one log line: control characters become '?'
/// and it is cut at 300, as `Calls::store` cuts a store's answer. `kind` and
/// `state` have no CHECK of their own, so a hand-edited row could otherwise
/// put arbitrary bytes into the log.
fn word_of(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(300)
        .collect()
}

/// One open action, read once: its number, its issue, and the time it was
/// attempted (0 for an intended row, which has no time yet).
struct Row {
    id: u64,
    issue: u64,
    attempted_at: u64,
}

impl Row {
    /// The numbers of an open action, or the field that fails to be one. The
    /// store asks for these on the write side, so a row that fails here is a
    /// corrupt one: substituting 0 would settle an unknown row, read issue 0
    /// for ever, or take the epoch as the start of the window and call an old
    /// comment the evidence. So the row is left open instead, and the caller
    /// says so once. `attempted_at` is 0 for an intended row, which is never
    /// read, and above 0 for an attempted one, so the window it opens cannot
    /// be the epoch.
    fn of(a: &Value) -> Result<Row, &'static str> {
        let id = positive(a, "id").ok_or("id")?;
        let issue = positive(a, "issue").ok_or("issue")?;
        let attempted_at = if intended(a) {
            0
        } else {
            positive(a, "attempted_at").ok_or("attempted_at")?
        };
        Ok(Row {
            id,
            issue,
            attempted_at,
        })
    }
}

/// The state the store gives a row whose write has not been answered: it has
/// no time and no read to make. The other open state is 'attempted'.
fn intended(a: &Value) -> bool {
    text(a, "state") == "intended"
}

fn names(v: &Value) -> Vec<String> {
    v.get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
        .collect()
}

impl Writer {
    /// The actions left open, each settled or said: the read, then the list.
    /// The poll takes the two steps itself, so that a journal it could not
    /// read is not mistaken for an empty one; this is the whole of them, for
    /// the tests.
    #[cfg(test)]
    pub(super) fn settle(&mut self, now: i64, log: &mut Vec<String>) {
        let Some(open) = self.journal_open(log) else {
            return;
        };
        self.settle_actions(now, open, log);
    }

    /// The actions the journal holds open, or None when it could not be read
    /// at all: a caller that skips work on an empty journal must not skip the
    /// work of a poll whose journal it never saw.
    pub(crate) fn journal_open(&mut self, log: &mut Vec<String>) -> Option<Vec<Value>> {
        match self.calls.store(&["journal-open"]) {
            Ok(v) => Some(
                v.get("actions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            ),
            Err(f) => {
                log.push(format!(
                    "WARN: the journal's open actions could not be read ({}); the next poll tries again",
                    word(&f)
                ));
                None
            }
        }
    }

    /// The read list of actions, each settled or said.
    pub(crate) fn settle_actions(&mut self, now: i64, open: Vec<Value>, log: &mut Vec<String>) {
        for a in open {
            let state = word_of(text(&a, "state"));
            let kind = word_of(text(&a, "kind"));
            let row = match Row::of(&a) {
                Ok(row) => row,
                Err(field) => {
                    // Named by the numbers it does have: any of them may be
                    // the missing one. It stays in the journal for a person.
                    let known = match (positive(&a, "id"), positive(&a, "issue")) {
                        (Some(id), Some(issue)) => format!("id {id}, #{issue}"),
                        (Some(id), None) => format!("id {id}"),
                        (None, Some(issue)) => format!("#{issue}"),
                        (None, None) => "no number".into(),
                    };
                    log.push(format!(
                        "WARN: the journal's {state} action {kind} is malformed ({field} is not a positive integer; {known}); it is left open for a person to settle with store journal-settle --action N"
                    ));
                    continue;
                }
            };
            let id = row.id.to_string();
            let n = row.issue;
            let age = now - a.get("intended_at").and_then(Value::as_i64).unwrap_or(now);
            let what = format!("action {id} ({kind} on #{n}, {age}s old)");
            let found = if intended(&a) {
                Found::Abandoned("it was never attempted")
            } else {
                self.found(&a, &kind, n, row.attempted_at)
            };
            let settled = match &found {
                Found::Done(evidence) => {
                    let e = evidence.to_string();
                    self.calls.store(&[
                        "journal-settle",
                        "--action",
                        &id,
                        "--result",
                        "ok",
                        "--evidence",
                        &e,
                    ])
                }
                Found::Abandoned(_) => {
                    self.calls
                        .store(&["journal-settle", "--action", &id, "--abandoned"])
                }
                Found::Unread(why) => {
                    log.push(format!("WARN: the journal's {what} could not be settled: {why}; the next poll tries again"));
                    continue;
                }
            };
            match (found, settled) {
                (_, Err(f)) => log.push(format!(
                    "WARN: the journal's {what} could not be settled ({}); the next poll tries again",
                    word(&f)
                )),
                (Found::Done(_), Ok(_)) => log.push(format!("journal: {what} was done")),
                (Found::Abandoned(why), Ok(_)) => log.push(format!(
                    "journal: {what} did not happen ({why}); this poll decides it again"
                )),
                (Found::Unread(_), Ok(_)) => {}
            }
        }
    }

    /// What GitHub says now of one attempted write.
    fn found(&mut self, a: &Value, kind: &str, n: u64, attempted: u64) -> Found {
        let repo = self.repo.clone();
        let gh: Vec<Value> = a
            .pointer("/args/gh")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        // A minute of slack for the clocks of the host and GitHub: a post is
        // also found by its sum. The time is above zero and the store's
        // writers bound it to eleven digits, so neither window reaches the
        // epoch: the guard proved the first, and the column's width the rest.
        let since = tehas_core::time::iso(attempted as i64);
        let posted_since = tehas_core::time::iso(attempted.saturating_sub(60) as i64);
        let read = |w: &mut Writer, path: String, page: bool| {
            let mut args = vec!["api".to_string(), path.clone()];
            if page {
                args.push("--paginate".into());
            }
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            w.calls
                .gh(&refs)
                .map_err(|_| format!("gh api {path} failed"))
        };
        match kind {
            "label-add" | "label-remove" | "label-remove-only" => {
                let v = match read(self, format!("repos/{repo}/issues/{n}"), false) {
                    Ok(v) => v,
                    Err(e) => return Found::Unread(e),
                };
                let have = names(&v);
                let on = after(&gh, "--add-label")
                    .iter()
                    .all(|l| have.iter().any(|h| h == l));
                let off = after(&gh, "--remove-label")
                    .iter()
                    .all(|l| !have.iter().any(|h| h == l));
                if on && off {
                    Found::Done(json!({"labels": have}))
                } else {
                    Found::Abandoned("the labels are not as it wanted them")
                }
            }
            "comment-post" => {
                let sum = a
                    .pointer("/args/input/sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let v = match read(
                    self,
                    format!("repos/{repo}/issues/{n}/comments?per_page=100"),
                    true,
                ) {
                    Ok(v) => v,
                    Err(e) => return Found::Unread(e),
                };
                // The bot's login, when the settings name it: the settling
                // comes before the poll has read it.
                let bot = self.host.bot.trim_end_matches("[bot]").to_string();
                let posted = v.as_array().into_iter().flatten().find(|c| {
                    let login = c
                        .pointer("/user/login")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let body = c.get("body").and_then(Value::as_str).unwrap_or("");
                    use sha2::{Digest, Sha256};
                    let got: String = Sha256::digest(body.as_bytes())
                        .iter()
                        .map(|x| format!("{x:02x}"))
                        .collect();
                    (bot.is_empty() || login.trim_end_matches("[bot]") == bot)
                        && c.get("created_at")
                            .and_then(Value::as_str)
                            .is_some_and(|t| t >= posted_since.as_str())
                        && got == sum
                });
                match posted {
                    Some(c) => Found::Done(json!({"comment": c.get("id")})),
                    // Only its sum is kept, so it is never posted again: this
                    // poll decides whether anything is still to be said.
                    None => Found::Abandoned("no comment of the bot with its text is there"),
                }
            }
            "comment-edit" => {
                let path = gh
                    .iter()
                    .filter_map(Value::as_str)
                    .find(|p| p.contains("/issues/comments/"))
                    .unwrap_or("");
                let v = match read(self, path.to_string(), false) {
                    Ok(v) => v,
                    Err(e) => return Found::Unread(e),
                };
                if v.get("updated_at")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t >= since.as_str())
                {
                    Found::Done(json!({"updated_at": v.get("updated_at")}))
                } else {
                    Found::Abandoned("the comment was not edited since")
                }
            }
            "refresh-begin" | "refresh-end" => {
                let v = match read(self, format!("repos/{repo}/issues/{n}"), false) {
                    Ok(v) => v,
                    Err(e) => return Found::Unread(e),
                };
                let marked = names(&v).iter().any(|l| l == "tehas:refreshing");
                if marked == (kind == "refresh-begin") {
                    Found::Done(json!({"labels": names(&v)}))
                } else {
                    Found::Abandoned("the pull request's mark is not as it wanted it")
                }
            }
            "issue-edit" => {
                let v = match read(self, format!("repos/{repo}/issues/{n}"), false) {
                    Ok(v) => v,
                    Err(e) => return Found::Unread(e),
                };
                let body = v
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .replace('\r', "");
                let title = v.get("title").and_then(Value::as_str);
                if body.starts_with("Replaces #") {
                    Found::Done(json!({"replaces": true}))
                } else if after(&gh, "--title")
                    .first()
                    .is_some_and(|t| title == Some(*t))
                {
                    // A plan written onto its issue names its title.
                    Found::Done(json!({"title": title}))
                } else {
                    Found::Abandoned("its body does not say what it replaces")
                }
            }
            "sub-issue-add" => {
                let want: Option<u64> = after(&gh, "-F")
                    .iter()
                    .find_map(|f| f.strip_prefix("sub_issue_id="))
                    .and_then(|x| x.parse().ok());
                let v = match read(
                    self,
                    format!("repos/{repo}/issues/{n}/sub_issues?per_page=100"),
                    true,
                ) {
                    Ok(v) => v,
                    Err(e) => return Found::Unread(e),
                };
                let there = v
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|s| s.get("id").and_then(Value::as_u64) == want);
                if there {
                    Found::Done(json!({"sub_issue": want}))
                } else {
                    Found::Abandoned("the sub-issue is not linked")
                }
            }
            "issue-close" => {
                let v = match read(self, format!("repos/{repo}/issues/{n}"), false) {
                    Ok(v) => v,
                    Err(e) => return Found::Unread(e),
                };
                if v.get("state").and_then(Value::as_str) == Some("closed") {
                    Found::Done(json!({"state": "closed"}))
                } else {
                    Found::Abandoned("the issue is open")
                }
            }
            _ => Found::Unread(format!("no read settles a {kind}")),
        }
    }
}
