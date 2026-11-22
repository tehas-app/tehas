//! The issue feed: which open issues carry a mark (a label, or an issue type
//! when TEHAS_ISSUE_TYPES maps the kind) given by an allowed account, and
//! which links of their texts such an account wrote. Authorisation is the
//! account, not the mark (AGENTS.md rule 5).
//!
//!   which issues   Open issues whose most recent labelling is by an account
//!                  of the allowed list. One applied by anyone else, the bot
//!                  included, counts as no mark at all. A list that names the
//!                  bot is refused: an account in a cycle labels as the bot.
//!   which label    The label that carries the name must be the one
//!                  `tehas labels` pinned: a label renamed to it would bring
//!                  its old applications along.
//!   which title    A title changed since the approval by an account off the
//!                  list holds the issue.
//!   which links    Only links of the link rule, in the issue text and its
//!                  comments, written by an allowed account and last edited by
//!                  none outside the list, on a line that is no quote. With
//!                  zips on, a `Zip: <link> sha256:<hex>` line names a zip.
//!
//! Every call to GitHub goes through [`crate::forge`]. Nothing here writes.

use crate::events::{first_unallowed_rename, label_state, standing_actor, Event, LabelState};
use crate::forge::{Forge, GhRunner};
use serde_json::Value;
use std::path::Path;

/// Whole logins, split by spaces, exact case. An empty name is nobody.
pub fn allowed(who: &str, list: &str) -> bool {
    crate::events::allowed_in(list, who)
}

/// The refusal of an allowed list that names the bot, by its login or its
/// account. None when it does not.
pub fn bot_refusal(bot: &str, list: &str) -> Option<String> {
    (allowed(bot, list) || allowed(crate::accounts::account(bot), list)).then(|| {
        format!(
            "ERROR: the bot account {bot} is in TEHAS_ALLOWED_LABELERS; the bot never authorises work"
        )
    })
}

/// Why the type `t` that TEHAS_ISSUE_TYPES maps `kind` to stops a pass, with
/// the exit status: 78 the owner has no such type, 0 the types could not be
/// read. None when the owner has it.
pub fn type_fault(forge: &mut dyn Forge, t: &str, kind: &str, repo: &str) -> Option<(u8, String)> {
    match forge.issue_types() {
        Ok(types) if types.iter().any(|have| have == t) => None,
        Ok(_) => Some((
            78,
            format!(
                "ERROR: TEHAS_ISSUE_TYPES maps {kind} to {t}, which is no issue type of {repo}'s owner"
            ),
        )),
        Err(e) => Some((0, format!("{}, nothing built or removed", e.line()))),
    }
}

/// The line of a pass whose list of open tickets could not be read. `why` is
/// gh's reason from the runner's fixed list, or empty for an answer that was
/// not a list.
pub fn unlisted(why: &str) -> String {
    let what = if why.is_empty() {
        "GitHub's answer was not a list of issues".to_string()
    } else {
        format!("gh: {}", why.trim_start_matches(": "))
    };
    format!("the open tickets could not be read ({what}), nothing built or removed")
}

pub fn unread(n: u64, typed: bool) -> String {
    let what = if typed { "type" } else { "label" };
    format!("#{n}: its {what} events could not be read, nothing built or removed")
}

/// The node id of GitHub's answer for one label. None when it names none.
pub fn node_id(answer: &str) -> Option<String> {
    let v: Value = serde_json::from_str(answer).ok()?;
    let id = v["node_id"].as_str()?;
    (!id.is_empty()).then(|| id.to_string())
}

/// Why no application of `label` counts, or None when the label that carries
/// the name is the pinned one. `stage` is what the mark puts an issue at.
pub fn refusal(label: &str, stage: &str, pin: Option<&str>, id: &str) -> Option<String> {
    let file = crate::label_pins::FILE;
    match pin {
        Some(p) if p == id => None,
        Some(p) => Some(format!(
            "refused: the label {label} is not the one pinned in {file} (pinned {p}, now {id}): no ticket is at {stage}"
        )),
        None => Some(format!(
            "refused: the label {label} ({id}) has no pin in {file}: no ticket is at {stage} until a person pins it (see tehas labels)"
        )),
    }
}

/// The label's refusal as [`refusal`] says it, read from GitHub and the pins
/// of `home`. Err with the line when the label could not be read.
pub fn pin_refusal(
    gh: &mut dyn GhRunner,
    home: &Path,
    repo: &str,
    label: &str,
    stage: &str,
) -> Result<Option<String>, String> {
    let path = format!("repos/{repo}/labels/{label}");
    let Some(id) = gh.run(&["api", &path]).ok().and_then(|a| node_id(&a)) else {
        return Err(format!(
            "the label {label} could not be read, nothing built or removed"
        ));
    };
    let pins = crate::label_pins::load(home);
    Ok(refusal(
        label,
        stage,
        pins.get(label).map(String::as_str),
        &id,
    ))
}

pub fn denied(label: &str, n: u64, who: &str, kind: Option<&str>) -> String {
    let who = if who.is_empty() { "unknown" } else { who };
    match kind {
        Some(t) => format!("#{n}: the issue type {t} was set by '{who}', not an allowed account"),
        None => format!("#{n}: {label} was applied by '{who}', not an allowed account"),
    }
}

/// The tickets of GitHub's list, newest first. None when it is no list. A pull
/// request is no ticket, nor is a row without a number or a title.
pub fn tickets(answer: &str) -> Option<Vec<Value>> {
    let Value::Array(rows) = serde_json::from_str(answer).ok()? else {
        return None;
    };
    let mut found: Vec<Value> = rows
        .iter()
        .filter(|row| row["pull_request"].is_null())
        .filter_map(|row| {
            let title: String = row["title"].as_str()?.chars().take(180).collect();
            Some(serde_json::json!({
                "number": row["number"].as_u64()?,
                "title": title,
                "url": row["html_url"],
                "author": row["user"]["login"],
                "updatedAt": row["updated_at"],
            }))
        })
        .collect();
    found.sort_by_key(|row| std::cmp::Reverse(row["number"].as_i64().unwrap_or(0)));
    Some(found)
}

/// Who gave an issue its mark, as the events read it.
pub enum Standing {
    /// The label was taken off since it was applied: the line says by whom.
    TakenOff(String),
    /// The events could not be read: nothing is built and nothing removed.
    Unread(String),
    /// Given by an account off the list.
    Denied(String),
    Allowed(Approved),
}

/// The allowed account that gave the mark, and when.
pub struct Approved {
    pub who: String,
    pub since: String,
    events: Option<Vec<Event>>,
}

/// Who gave issue `n` its mark: `label`, or the type `kind` when it is Some.
/// Read before anything of the issue is published: an agent in a cycle holds
/// the bot's token, and the bot can label.
pub fn standing(
    forge: &mut dyn Forge,
    n: u64,
    label: &str,
    kind: Option<&str>,
    list: &str,
) -> Standing {
    let read = match kind {
        Some(t) => forge
            .typed_by(n, t)
            .map(|a| (a.map(|a| (a.actor, a.at)).unwrap_or_default(), None)),
        None => forge
            .label_events(n)
            .map(|ev| (standing_actor(&ev, label), Some(ev))),
    };
    if let Ok((_, Some(ev))) = &read {
        if let LabelState::Unlabeled { actor, .. } = label_state(ev, label) {
            let by = if actor.is_empty() { "unknown" } else { &actor };
            return Standing::TakenOff(format!(
                "#{n}: {label} was taken off by '{by}' since it was applied"
            ));
        }
    }
    let ((who, since), events) = match read {
        Ok(read) => read,
        Err(_) => return Standing::Unread(unread(n, kind.is_some())),
    };
    if !allowed(&who, list) {
        return Standing::Denied(denied(label, n, &who, kind));
    }
    Standing::Allowed(Approved { who, since, events })
}

/// The line that holds issue `n` when its title was changed since the
/// approval by an account off the list, or its events could not be read. None
/// when it may be published.
pub fn title_hold(forge: &mut dyn Forge, n: u64, approved: Approved, list: &str) -> Option<String> {
    let events = match approved.events.map_or_else(|| forge.label_events(n), Ok) {
        Ok(events) => events,
        Err(_) => return Some(unread(n, false)),
    };
    let who = &approved.who;
    first_unallowed_rename(&events, &approved.since, who, &|w| allowed(w, list)).map(|by| {
        format!("#{n}: its title was changed by '{by}' since {who} approved it, left as it was")
    })
}

/// An account as the texts read asks for it: its login and its user id, so the
/// login can be vouched for.
pub const WHO: &str = "login ...on User { databaseId } ...on Bot { databaseId }";

/// The issue's body and its first hundred comments, each with its author and
/// its last editor, the logins vouched. None when GitHub does not answer with
/// the issue.
pub fn texts(repo: &str, gh: &mut dyn GhRunner, n: u64) -> Option<Value> {
    let (owner, name) = repo.split_once('/')?;
    let query = format!(
        "query($owner: String!, $name: String!, $number: Int!) {{ repository(owner: $owner, name: $name) {{ issue(number: $number) {{ body author {{ {WHO} }} editor {{ {WHO} }} comments(first: 100) {{ nodes {{ body author {{ {WHO} }} editor {{ {WHO} }} }} }} }} }} }}"
    );
    let answer = gh
        .run(&[
            "api",
            "graphql",
            "-F",
            &format!("owner={owner}"),
            "-F",
            &format!("name={name}"),
            "-F",
            &format!("number={n}"),
            "-f",
            &format!("query={query}"),
        ])
        .ok()?;
    let mut v: Value = serde_json::from_str(&answer).ok()?;
    crate::accounts::vouch(&mut v);
    v.pointer("/data/repository/issue")
        .filter(|i| i.is_object())
        .cloned()
}

/// The links of an issue to fetch, the zips it names by address, and how many
/// came from outside.
pub struct Refs {
    pub fetch: Vec<String>,
    pub zips: Vec<ZipMark>,
    pub outside: i64,
}

/// What a `Zip:` line says: the zip's link and sha256, or, for a line of
/// another form, its first word.
pub type ZipMark = Result<Zip, String>;

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Zip {
    pub link: String,
    pub sha256: String,
}

/// The word that opens a line naming a zip by address.
pub const ZIP_MARK: &str = "Zip:";

/// The zip a line names: `Zip: <link> sha256:<64 lower-case hex>`. None when
/// the line does not start with [`ZIP_MARK`].
pub fn zip_mark(line: &str) -> Option<ZipMark> {
    let rest = line.trim().strip_prefix(ZIP_MARK)?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    let hex = |h: &str| {
        h.len() == 64
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    Some(match words[..] {
        [link, sum] if sum.strip_prefix("sha256:").is_some_and(hex) => Ok(Zip {
            link: link.to_string(),
            sha256: sum["sha256:".len()..].to_string(),
        }),
        _ => Err(words.first().map_or_else(String::new, |w| w.to_string())),
    })
}

impl Refs {
    /// What a page is built from besides the files themselves: the title, the
    /// links taken and the count of the rest. The same key needs no fetch.
    pub fn key(&self, title: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&(title, &self.fetch, self.outside), &mut hasher);
        if !self.zips.is_empty() {
            std::hash::Hash::hash(&self.zips, &mut hasher);
        }
        format!("{:016x}", std::hash::Hasher::finish(&hasher))
    }
}

/// The links of one issue, from the issue as [`texts`] answers it: its body
/// and its comments, each with its author and its last editor, the logins
/// vouched. A link is fetched when an allowed account wrote the text, nobody
/// off the list edited it last, and the line is no quote: a quote reply
/// carries someone else's words. Any other link is only counted. With `zips`,
/// a `Zip:` line is read by the same rule and its link is no attachment link.
/// `links` is the link rule, `attach::link::links`.
pub fn refs(
    issue: &Value,
    allowed: &dyn Fn(&str) -> bool,
    zips: bool,
    links: &dyn Fn(&str) -> Vec<String>,
) -> Refs {
    let mut texts = vec![issue];
    if let Some(nodes) = issue.pointer("/comments/nodes").and_then(Value::as_array) {
        texts.extend(nodes);
    }
    let mut mine = Vec::new();
    let mut named: Vec<ZipMark> = Vec::new();
    let mut theirs = Vec::new();
    for text in texts {
        let ours = allowed(&login_of(text, "author"))
            && match text.get("editor") {
                None | Some(Value::Null) => true,
                Some(_) => allowed(&login_of(text, "editor")),
            };
        let body = text.get("body").and_then(Value::as_str).unwrap_or("");
        for line in body.lines() {
            let quoted = line.trim_start().starts_with('>');
            let unquoted = line.trim_start_matches(|c: char| c == '>' || c.is_whitespace());
            if let Some(mark) = zip_mark(unquoted).filter(|_| zips) {
                if ours && !quoted {
                    if !named.contains(&mark) {
                        named.push(mark);
                    }
                } else {
                    let link = mark.map_or_else(|word| word, |zip| zip.link);
                    if !theirs.contains(&link) {
                        theirs.push(link);
                    }
                }
                continue;
            }
            for link in links(line) {
                let keep = if ours && !quoted {
                    &mut mine
                } else {
                    &mut theirs
                };
                if !keep.contains(&link) {
                    keep.push(link);
                }
            }
        }
    }
    Refs {
        fetch: mine,
        zips: named,
        outside: theirs.len() as i64,
    }
}

/// The login of an account a text names, empty when it names none.
fn login_of(text: &Value, key: &str) -> String {
    match text.get(key).and_then(|who| who.get("login")) {
        Some(Value::String(login)) => login.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::Kind;
    use attach::link::links;

    const LABEL: &str = Kind::Prototype.label();
    const STAGE: &str = "the prototype stage";

    fn alice(who: &str) -> bool {
        who == "alice"
    }

    /// An issue as the texts read answers it: its body and its comments, each
    /// with its author and, when edited, its last editor.
    fn issue(body: &str, author: &str, editor: Option<&str>, comments: Value) -> Value {
        serde_json::json!({
            "body": body,
            "author": {"login": author},
            "editor": editor.map(|e| serde_json::json!({"login": e})),
            "comments": {"nodes": comments},
        })
    }

    fn comment(body: &str, author: &str, editor: Option<&str>) -> Value {
        serde_json::json!({
            "body": body,
            "author": {"login": author},
            "editor": editor.map(|e| serde_json::json!({"login": e})),
        })
    }

    const SHOT: &str =
        "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555";

    #[test]
    fn only_allowed_accounts_give_links_to_fetch() {
        let body = format!(
            "shot [x]({SHOT}) and [again]({SHOT}) and {}",
            "https://github.com/acme/widget/files/21/proto.html"
        );
        let comments = serde_json::json!([
            comment(
                "https://github.com/user-attachments/files/31/vid.mp4",
                "alice",
                None
            ),
            comment(
                "[c](https://github.com/user-attachments/files/32/collab.html)",
                "carol",
                None
            ),
            comment(
                "[b](https://github.com/user-attachments/files/33/bot.html)",
                "tehas-bot[bot]",
                None
            ),
        ]);
        let got = refs(
            &issue(&body, "alice", None, comments),
            &alice,
            false,
            &links,
        );
        assert_eq!(
            got.fetch,
            [
                SHOT,
                "https://github.com/acme/widget/files/21/proto.html",
                "https://github.com/user-attachments/files/31/vid.mp4",
            ]
        );
        // A collaborator off the list and the bot are both outside.
        assert_eq!(got.outside, 2);
    }

    #[test]
    fn a_text_someone_else_edited_last_gives_nothing_to_fetch() {
        let link = "https://github.com/user-attachments/files/40/theirs.html";
        let comments = serde_json::json!([
            comment(&format!("one {link}"), "alice", Some("tehas-bot[bot]")),
            comment(
                "https://github.com/user-attachments/files/9/mine.html",
                "alice",
                Some("alice")
            ),
        ]);
        let got = refs(
            &issue(SHOT, "alice", Some("mallory"), comments),
            &alice,
            false,
            &links,
        );
        assert_eq!(
            got.fetch,
            ["https://github.com/user-attachments/files/9/mine.html"]
        );
        assert_eq!(got.outside, 2);
        // An editor with no login is nobody on the list.
        let ghost = serde_json::json!({"body": SHOT, "author": {"login": "alice"}, "editor": {}});
        assert!(refs(&ghost, &alice, false, &links).fetch.is_empty());
    }

    #[test]
    fn a_link_on_a_quoted_line_is_someone_elses() {
        let theirs = "https://github.com/user-attachments/files/41/theirs.html";
        let body = format!("> {theirs}\n  >> [x]({theirs})\nmine: {SHOT}");
        let got = refs(
            &issue(&body, "alice", None, serde_json::json!([])),
            &alice,
            false,
            &links,
        );
        assert_eq!(got.fetch, [SHOT]);
        assert_eq!(got.outside, 1);
    }

    #[test]
    fn a_text_with_no_author_or_no_comments_is_read_as_far_as_it_goes() {
        let got = refs(
            &serde_json::json!({"body": SHOT, "author": null}),
            &alice,
            false,
            &links,
        );
        assert!(got.fetch.is_empty());
        assert_eq!(got.outside, 1);
        let got = refs(
            &serde_json::json!({"body": SHOT, "author": {"login": "alice"}}),
            &alice,
            false,
            &links,
        );
        assert_eq!(got.fetch, [SHOT]);
    }

    #[test]
    fn the_key_of_the_refs_follows_the_title_the_links_and_the_rest() {
        let a = Refs {
            fetch: vec![SHOT.to_string()],
            zips: Vec::new(),
            outside: 0,
        };
        let b = Refs {
            fetch: vec![SHOT.to_string()],
            zips: Vec::new(),
            outside: 1,
        };
        assert_eq!(a.key("Map"), a.key("Map"));
        assert_ne!(a.key("Map"), a.key("Maps"));
        assert_ne!(a.key("Map"), b.key("Map"));
        assert!(!a.key("Map").contains(' '));
        let zipped = |sha256: &str| Refs {
            fetch: vec![SHOT.to_string()],
            zips: vec![Ok(Zip {
                link: ZIP.to_string(),
                sha256: sha256.to_string(),
            })],
            outside: 0,
        };
        assert_ne!(zipped(&SUM).key("Map"), a.key("Map"));
        assert_ne!(zipped(&SUM).key("Map"), zipped(&"b".repeat(64)).key("Map"));
    }

    const ZIP: &str = "https://files.example.org/p/proto.zip";
    static SUM: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| "a".repeat(64));

    #[test]
    fn a_zip_line_names_its_link_and_sha256() {
        let zip = Zip {
            link: ZIP.to_string(),
            sha256: SUM.clone(),
        };
        assert_eq!(
            zip_mark(&format!("Zip: {ZIP} sha256:{}", *SUM)),
            Some(Ok(zip.clone()))
        );
        assert_eq!(
            zip_mark(&format!("  Zip:\t{ZIP}   sha256:{}  \r", *SUM)),
            Some(Ok(zip))
        );
        for line in [
            "",
            "zip: x sha256:y",
            "A Zip: mid-line",
            "- Zip: a list item",
        ] {
            assert_eq!(zip_mark(line), None, "{line:?}");
        }
        for (line, word) in [
            ("Zip:".to_string(), ""),
            (format!("Zip: {ZIP}"), ZIP),
            (format!("Zip: {ZIP} {}", *SUM), ZIP),
            (format!("Zip: {ZIP} sha256:{}", SUM.to_uppercase()), ZIP),
            (format!("Zip: {ZIP} sha256:{}", &SUM[1..]), ZIP),
            (format!("Zip: {ZIP} x sha256:{}", *SUM), ZIP),
        ] {
            assert_eq!(zip_mark(&line), Some(Err(word.to_string())), "{line:?}");
        }
    }

    #[test]
    fn a_zip_line_is_read_only_with_zips_on_and_by_the_rule_of_links() {
        let line = format!("Zip: {ZIP} sha256:{}", *SUM);
        let github = format!(
            "Zip: https://github.com/user-attachments/files/5/p.zip sha256:{}",
            *SUM
        );
        let body = format!("{line}\n{line}\n> {line}\n{github}\nZip: nonsense");
        let comments = serde_json::json!([comment(
            &format!("Zip: https://files.example.org/theirs.zip sha256:{}", *SUM),
            "mallory",
            None
        )]);
        let ticket = issue(&body, "alice", None, comments);
        let off = refs(&ticket, &alice, false, &links);
        assert!(off.zips.is_empty());
        // Off, a Zip: line is text as before: its GitHub link is an attachment.
        assert_eq!(
            off.fetch,
            ["https://github.com/user-attachments/files/5/p.zip"]
        );
        assert_eq!(off.outside, 0);
        let on = refs(&ticket, &alice, true, &links);
        assert!(on.fetch.is_empty());
        assert_eq!(
            on.zips,
            [
                Ok(Zip {
                    link: ZIP.to_string(),
                    sha256: SUM.clone()
                }),
                Ok(Zip {
                    link: "https://github.com/user-attachments/files/5/p.zip".to_string(),
                    sha256: SUM.clone()
                }),
                Err("nonsense".to_string()),
            ]
        );
        // The quoted line and mallory's line.
        assert_eq!(on.outside, 2);
    }

    #[test]
    fn the_allowed_list_is_whole_logins_in_exact_case() {
        assert!(allowed("alice", "alice bob"));
        assert!(allowed("bob", "alice bob"));
        assert!(!allowed("ali", "alice bob"));
        assert!(!allowed("Alice", "alice bob"));
    }

    #[test]
    fn only_the_pinned_label_puts_a_ticket_at_the_stage() {
        assert_eq!(refusal(LABEL, STAGE, Some("LA_p"), "LA_p"), None);
        assert_eq!(
            refusal(LABEL, STAGE, Some("LA_p"), "LA_bug").unwrap(),
            format!("refused: the label {LABEL} is not the one pinned in state/labels (pinned LA_p, now LA_bug): no ticket is at the prototype stage")
        );
        assert!(refusal(LABEL, STAGE, None, "LA_p")
            .unwrap()
            .starts_with(&format!(
                "refused: the label {LABEL} (LA_p) has no pin in state/labels"
            )));
    }

    #[test]
    fn a_label_answer_names_its_node_id_or_none() {
        assert_eq!(
            node_id(&format!(r#"{{"name":"{LABEL}","node_id":"LA_p"}}"#)),
            Some("LA_p".to_string())
        );
        for bad in [
            r#"{"node_id":""}"#,
            r#"{"message":"Not Found"}"#,
            "[]",
            "no json",
        ] {
            assert_eq!(node_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_denied_line_names_the_account_or_says_unknown() {
        assert_eq!(
            denied(LABEL, 7, "mallory", None),
            format!("#7: {LABEL} was applied by 'mallory', not an allowed account")
        );
        assert_eq!(
            denied(LABEL, 7, "", None),
            format!("#7: {LABEL} was applied by 'unknown', not an allowed account")
        );
        assert_eq!(
            denied(LABEL, 7, "mallory", Some("Prototype")),
            "#7: the issue type Prototype was set by 'mallory', not an allowed account"
        );
    }

    #[test]
    fn the_list_is_issues_newest_first_and_nothing_else() {
        assert!(tickets("{\"message\":\"Bad credentials\"}").is_none());
        assert!(tickets("nope").is_none());
        let got = tickets(
            r#"[{"number":3,"title":"c","body":null,"user":{"login":"a"}},
                {"number":11,"title":"a"},
                {"number":4,"title":"pull","pull_request":{}},
                {"number":5,"title":null},
                {"title":"no number"}]"#,
        )
        .unwrap();
        let numbers: Vec<i64> = got
            .iter()
            .map(|row| row["number"].as_i64().unwrap_or(0))
            .collect();
        assert_eq!(numbers, [11, 3]);
        assert_eq!(got[1]["author"], "a");
        let long = format!("[{{\"number\":1,\"title\":\"{}\"}}]", "ä".repeat(200));
        let cut = tickets(&long).unwrap();
        assert_eq!(cut[0]["title"].as_str().unwrap().chars().count(), 180);
    }

    #[test]
    fn a_list_that_could_not_be_read_says_why() {
        assert_eq!(
            unlisted(": HTTP 401"),
            "the open tickets could not be read (gh: HTTP 401), nothing built or removed"
        );
        assert!(unlisted("").contains("not a list of issues"));
    }
}
