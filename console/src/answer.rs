//! `tehas answer <issue> <choice>`: answer the question a stopped ticket or a
//! planning session waits on, for a person, from a program on the host
//! (#2782). console/src/usage/answer.txt is the text a person reads.
//!
//!   tehas answer <issue> <choice> --by <login> [--json]
//!   tehas answer <issue> --text <file> --by <login> [--json]
//!
//! The answer is a row of the state store, bound to the question on record of
//! the issue (store answer-record). The next poll acts on it as on the box of
//! the same choice ticked by `--by`, under the rules every answer passes, and
//! nothing starts here. `--by` is trusted as the host's caller, as `tehas
//! start` trusts its caller, and it must be an account that may authorise
//! work (TEHAS_ALLOWED_LABELERS).
//!
//! Exit: 0 recorded; 1 the store refused or could not be read; 3 no question
//! of that kind waits on the issue, or it does not offer that choice; 64 a
//! bad argument, which writes nothing; 78 `--by` may not authorise work,
//! which writes nothing.

use crate::store;
use serde_json::json;
use std::env;
use std::path::PathBuf;
use tehas_core::question::{Choice as Word, MAX_OPTIONS};
use tehas_core::{exit, time};

const USAGE: &str = "usage: tehas answer <issue> <choice> | --text <file> --by <login> [--json]";

/// The choices, as `tehas answer` takes them and the store keeps them.
#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    /// A stop's: retry with the issue and its comments, with the issue text
    /// only, or leave it parked.
    Retry,
    RetryText,
    Park,
    /// A planning question's option, numbered from 1 as the comment lists it.
    Option(u64),
    /// A planning question's: the option "Update this issue", and the end of
    /// the session.
    Update,
    End,
    /// A planning question's: a reply in words, from this file.
    Text(PathBuf),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    issue: u64,
    choice: Choice,
    by: String,
    json: bool,
}

/// A positive number of at most ten digits, with no leading zero.
fn is_issue(word: &str) -> bool {
    let b = word.as_bytes();
    (1..=10).contains(&b.len()) && (b'1'..=b'9').contains(&b[0]) && b.iter().all(u8::is_ascii_digit)
}

/// A GitHub login: 1 to 39 letters, digits and hyphens.
fn is_login(word: &str) -> bool {
    (1..=39).contains(&word.len()) && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn bad(why: &str) -> u8 {
    eprintln!("tehas answer: {why}");
    eprintln!("{USAGE}");
    exit::USAGE
}

/// The choice a word names, or None: a word of `tehas_core::question::Choice`
/// but `option` and `text`, which are `option:<k>` and `--text` here. `<k>` is
/// 1 to the options a question lists, with no leading zero.
fn choice(word: &str) -> Option<Choice> {
    if let Some(k) = word.strip_prefix("option:") {
        if k.starts_with('0') || !k.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let k: u64 = k.parse().ok()?;
        return (1..=MAX_OPTIONS as u64)
            .contains(&k)
            .then_some(Choice::Option(k));
    }
    Some(match Word::of(word)? {
        Word::Retry => Choice::Retry,
        Word::RetryText => Choice::RetryText,
        Word::Park => Choice::Park,
        Word::Update => Choice::Update,
        Word::End => Choice::End,
        Word::Option | Word::Text => return None,
    })
}

/// The arguments, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let (mut issue, mut chose, mut text, mut by, mut json) = (None, None, None, None, false);
    let mut it = given.iter();
    while let Some(w) = it.next() {
        // A flag by its name, the two dashes off.
        match w.strip_prefix("--") {
            Some("json") if !json => json = true,
            Some("by") if by.is_none() => match it.next() {
                Some(l) if is_login(l) => by = Some(l.clone()),
                Some(l) => return Err(bad(&format!("--by is not a GitHub login: {l}"))),
                None => return Err(bad("--by needs a login")),
            },
            Some("text") if text.is_none() => match it.next() {
                Some(f) if !f.is_empty() => text = Some(PathBuf::from(f)),
                _ => return Err(bad("--text needs a file")),
            },
            _ if w.starts_with('-') => return Err(bad(&format!("unknown argument: {w}"))),
            _ if issue.is_none() => {
                if !is_issue(w) {
                    return Err(bad(&format!("not an issue number: {w}")));
                }
                issue = w.parse().ok();
            }
            _ if chose.is_none() => match choice(w) {
                Some(c) => chose = Some(c),
                None => return Err(bad(&format!("not a choice: {w}"))),
            },
            _ => return Err(bad(&format!("one choice at most: {w}"))),
        }
    }
    let Some(issue) = issue else {
        return Err(bad("it needs an issue number"));
    };
    let choice = match (chose, text) {
        (Some(c), None) => c,
        (None, Some(f)) => Choice::Text(f),
        (Some(_), Some(_)) => return Err(bad("a choice or --text, not both")),
        (None, None) => return Err(bad("it needs a choice or --text")),
    };
    let Some(by) = by else {
        return Err(bad("it needs --by, the account the answer is given for"));
    };
    Ok(Asked {
        issue,
        choice,
        by,
        json,
    })
}

fn home() -> Option<PathBuf> {
    let from = |name: &str| match env::var_os(name) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    };
    from("TEHAS_HOME").or_else(|| from("HOME"))
}

pub fn main(given: &[String]) -> u8 {
    let a = match words(given) {
        Ok(a) => a,
        Err(rc) => return rc,
    };
    let allowed = env::var("TEHAS_ALLOWED_LABELERS").unwrap_or_default();
    if !tehas_core::events::allowed_in(&allowed, &a.by) {
        eprintln!(
            "tehas answer: {} may not authorise work for this factory (TEHAS_ALLOWED_LABELERS); nothing was recorded",
            a.by
        );
        return exit::SETTINGS;
    }
    let Some(here) = home() else {
        eprintln!("tehas answer: neither TEHAS_HOME nor HOME is set");
        return exit::USAGE;
    };
    if !store::ready(&here) {
        eprintln!(
            "tehas answer: no state store in {}: run install.sh",
            here.display()
        );
        return exit::FAILED;
    }
    let n = a.issue.to_string();
    let (word, option, file) = match &a.choice {
        Choice::Retry => (Word::Retry, None, None),
        Choice::RetryText => (Word::RetryText, None, None),
        Choice::Park => (Word::Park, None, None),
        Choice::Option(k) => (Word::Option, Some(k.to_string()), None),
        Choice::Update => (Word::Update, None, None),
        Choice::End => (Word::End, None, None),
        Choice::Text(f) => {
            // The store reads an absolute path; a relative one is the caller's.
            let f = match std::path::absolute(f) {
                Ok(f) => f.to_string_lossy().into_owned(),
                Err(e) => return bad(&format!("--text {}: {e}", f.display())),
            };
            (Word::Text, None, Some(f))
        }
    };
    let word = word.word();
    let mut args = vec!["answer-record", &n, "--by", &a.by, "--choice", word];
    if let Some(k) = &option {
        args.extend(["--option", k]);
    }
    if let Some(f) = &file {
        args.extend(["--text-file", f]);
    }
    let v = match store::call(&here, &args) {
        Ok(v) => v,
        Err(r) => {
            let rc = match r.word.as_str() {
                "unasked" | "unoffered" => exit::NO,
                _ => exit::FAILED,
            };
            eprintln!(
                "tehas answer: the store said: {}; nothing was recorded",
                r.line
            );
            return rc;
        }
    };
    let r = &v["answer"];
    let at = r["at"].as_i64().map(time::iso);
    if a.json {
        tehas_core::outln!(
            "{}",
            json!({
                "api": 1, "issue": a.issue,
                "question": {"id": r["question"], "kind": r["kind"]},
                "answer": {"choice": r["choice"], "option": r["option"], "by": r["by"],
                    "how": "answer", "at": at},
                "replaced": v["replaced"],
            })
        );
    } else {
        let what = match a.choice {
            Choice::Option(k) => format!("option {k}"),
            _ => word.to_string(),
        };
        let again = if v["replaced"] == json!(true) {
            ", in place of the answer given before"
        } else {
            ""
        };
        tehas_core::outln!(
            "answered the {} question of #{} for {} at {}: {what}{again}; the next poll acts on it",
            r["kind"].as_str().unwrap_or("?"),
            a.issue,
            a.by,
            at.as_deref().unwrap_or("?")
        );
    }
    exit::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_words_are_an_issue_a_choice_or_a_text_and_the_account() {
        assert_eq!(
            words(&w(&["12", "retry", "--by", "alice"])),
            Ok(Asked {
                issue: 12,
                choice: Choice::Retry,
                by: "alice".into(),
                json: false
            })
        );
        assert_eq!(
            words(&w(&["--by", "bob", "7", "option:3"])).map(|a| a.choice),
            Ok(Choice::Option(3))
        );
        assert_eq!(
            words(&w(&["7", "--text", "reply.md", "--by", "bob"])).map(|a| a.choice),
            Ok(Choice::Text("reply.md".into()))
        );
        for (word, c) in [
            ("retry-text", Choice::RetryText),
            ("park", Choice::Park),
            ("update", Choice::Update),
            ("end", Choice::End),
            ("option:20", Choice::Option(20)),
        ] {
            assert_eq!(
                words(&w(&["7", word, "--by", "a"])).map(|a| a.choice),
                Ok(c)
            );
        }
        for bad in [
            &[][..],
            &["12"],
            &["12", "retry"],
            &["12", "--by", "alice"],
            &["0", "retry", "--by", "alice"],
            &["12", "tick", "--by", "alice"],
            &["12", "retry", "end", "--by", "alice"],
            &["12", "retry", "--text", "f", "--by", "alice"],
            &["12", "retry", "--by", "al ice"],
            &["12", "retry", "--by"],
            &["12", "retry", "--by", "a", "--by", "b"],
            &["12", "option:0", "--by", "a"],
            &["12", "option:01", "--by", "a"],
            &["12", "option:21", "--by", "a"],
            &["12", "option:", "--by", "a"],
            &["12", "--text", "--by", "a"],
            &["12", "retry", "--by", "a", "--now"],
        ] {
            assert_eq!(words(&w(bad)).map(|_| ()), Err(64), "{bad:?}");
        }
    }
}
