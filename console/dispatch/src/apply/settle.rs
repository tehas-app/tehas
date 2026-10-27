//! The journal settled at the start of a poll (section 5 of the state store's
//! spec): an action a poll intended and did not settle, because it died
//! between two writes, is read again and settled from what GitHub says now.
//! Never replayed: the poll that settles it decides afresh whether anything
//! is still owed. A write never attempted made no call, and is abandoned.

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

fn names(v: &Value) -> Vec<String> {
    v.get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
        .collect()
}

impl Writer {
    /// The actions left open, each settled or said.
    pub fn settle(&mut self, now: i64, log: &mut Vec<String>) {
        let open = match self.calls.store(&["journal-open"]) {
            Ok(v) => v
                .get("actions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            Err(f) => {
                log.push(format!(
                    "WARN: the journal's open actions could not be read ({}); the next poll tries again",
                    word(&f)
                ));
                return;
            }
        };
        for a in open {
            let id = a.get("id").and_then(Value::as_i64).unwrap_or(0).to_string();
            let kind = text(&a, "kind").to_string();
            let n = a.get("issue").and_then(Value::as_u64).unwrap_or(0);
            let age = now - a.get("intended_at").and_then(Value::as_i64).unwrap_or(now);
            let what = format!("action {id} ({kind} on #{n}, {age}s old)");
            let found = if text(&a, "state") == "intended" {
                Found::Abandoned("it was never attempted")
            } else {
                self.found(&a, &kind, n)
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
    fn found(&mut self, a: &Value, kind: &str, n: u64) -> Found {
        let repo = self.repo.clone();
        let gh: Vec<Value> = a
            .pointer("/args/gh")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let attempted = a.get("attempted_at").and_then(Value::as_i64).unwrap_or(0);
        let since = crate::decide::iso(attempted.max(0) as u64);
        // A minute of slack for the clocks of the host and GitHub: a post is
        // also found by its sum.
        let posted_since = crate::decide::iso((attempted - 60).max(0) as u64);
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
