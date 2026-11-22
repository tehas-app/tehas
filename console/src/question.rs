//! `tehas question <issue>`: the question an issue waits on, and the choices
//! it offers, in the words `tehas answer` takes (#2782). Read only.
//! console/src/usage/question.txt is the text a person reads.
//!
//!   tehas question <issue> [--json]
//!
//! It reads the store (store question-show) and nothing else. A planning
//! question offers its options and the end of the session, a park's question
//! the choices it was asked with. A park's question asked before the store
//! kept its choices lists all three; the poll refuses an answer its comment
//! does not offer.
//!
//! Exit: 0 a question waits; 1 the store refused or could not be read; 3 no
//! question waits on the issue; 64 a bad argument.

use crate::store;
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;
use tehas_core::question::{park_choices, plan_choices, Choice, Offered, CHOICES};
use tehas_core::{exit, time};

const USAGE: &str = "usage: tehas question <issue> [--json]";

#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    issue: u64,
    json: bool,
}

fn bad(why: &str) -> u8 {
    eprintln!("tehas question: {why}");
    eprintln!("{USAGE}");
    exit::USAGE
}

/// The arguments, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let (mut issue, mut json) = (None, false);
    for w in given {
        match w.strip_prefix("--") {
            Some("json") if !json => json = true,
            _ if w.starts_with('-') => return Err(bad(&format!("unknown argument: {w}"))),
            _ if issue.is_some() => return Err(bad(&format!("one issue number at most: {w}"))),
            _ => {
                let b = w.as_bytes();
                let shaped = (1..=10).contains(&b.len())
                    && (b'1'..=b'9').contains(&b[0])
                    && b.iter().all(u8::is_ascii_digit);
                if !shaped {
                    return Err(bad(&format!("not an issue number: {w}")));
                }
                issue = w.parse().ok();
            }
        }
    }
    let Some(issue) = issue else {
        return Err(bad("it needs an issue number"));
    };
    Ok(Asked { issue, json })
}

/// The word `tehas answer` takes for a choice: `option:<k>` for an option.
pub fn answer_word(o: &Offered) -> String {
    match (o.choice, o.option) {
        (Choice::Option, Some(k)) => format!("option:{k}"),
        (c, _) => c.word().to_string(),
    }
}

/// The choices of the store's question: a planning question's from its
/// options, a park's from the choices it was asked with, every park choice
/// when those were not kept.
fn choices(kind: &str, offers: Option<Vec<String>>, options: Vec<String>) -> Vec<Offered> {
    if kind == "plan" {
        return plan_choices(&options);
    }
    let offered: Vec<Choice> = match offers {
        Some(words) => words.iter().filter_map(|w| Choice::of(w)).collect(),
        None => CHOICES.to_vec(),
    };
    park_choices(&offered)
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
    let Some(here) = home() else {
        eprintln!("tehas question: neither TEHAS_HOME nor HOME is set");
        return exit::USAGE;
    };
    if !store::ready(&here) {
        eprintln!(
            "tehas question: no state store in {}: run install.sh",
            here.display()
        );
        return exit::FAILED;
    }
    let n = a.issue.to_string();
    let v = match store::call(&here, &["question-show", &n]) {
        Ok(v) => v,
        Err(r) => {
            eprintln!("tehas question: the store said: {}", r.line);
            return exit::FAILED;
        }
    };
    let q = &v["question"];
    if q.is_null() {
        eprintln!("tehas question: no question waits on #{n}");
        return exit::NO;
    }
    let strings = |k: &str| -> Option<Vec<String>> {
        q[k].as_array().map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
    };
    let kind = q["kind"].as_str().unwrap_or("");
    let offered = choices(
        kind,
        strings("offers"),
        strings("options").unwrap_or_default(),
    );
    let asked_at = q["posted_at"].as_i64().map(time::iso);
    if a.json {
        let listed: Vec<Value> = offered
            .iter()
            .map(|o| json!({"label": o.label, "choice": answer_word(o)}))
            .collect();
        tehas_core::outln!(
            "{}",
            json!({"api": 1, "issue": a.issue, "question": {"id": q["id"], "kind": kind,
                "asked_at": asked_at, "choices": listed}})
        );
    } else {
        tehas_core::outln!(
            "#{n} waits on its {kind} question, asked at {}:",
            asked_at.as_deref().unwrap_or("?")
        );
        for o in &offered {
            tehas_core::outln!("  {:<12} {}", answer_word(o), o.label);
        }
    }
    exit::OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::plan::UPDATE;
    use tehas_core::question::{BOX_ALONE, BOX_COMMENTS, BOX_END, BOX_PARKED};

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_words_are_one_issue_and_a_switch() {
        assert_eq!(
            words(&w(&["12"])),
            Ok(Asked {
                issue: 12,
                json: false
            })
        );
        for bad in [&[][..], &["0"], &["12", "13"], &["12", "--soon"], &["x"]] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    /// Each choice in the words `tehas answer` takes, with what a person sees.
    fn listed(o: &[Offered]) -> Vec<(String, String)> {
        o.iter()
            .map(|o| (answer_word(o), o.label.clone()))
            .collect()
    }

    #[test]
    fn a_planning_question_offers_its_options_then_the_end() {
        let options = vec!["Adopt all".to_string(), UPDATE.to_string()];
        assert_eq!(
            listed(&choices("plan", None, options)),
            [
                ("option:1".into(), "Adopt all".into()),
                ("update".into(), UPDATE.into()),
                ("end".into(), BOX_END.into())
            ]
        );
    }

    #[test]
    fn a_park_offers_what_it_was_asked_with_or_every_choice_when_unknown() {
        let asked = Some(vec!["retry-text".to_string(), "park".to_string()]);
        assert_eq!(
            listed(&choices("park", asked, vec![])),
            [
                ("retry-text".into(), BOX_ALONE.into()),
                ("park".into(), BOX_PARKED.into())
            ]
        );
        assert_eq!(
            listed(&choices("park", None, vec![])),
            [
                ("retry".into(), BOX_COMMENTS.into()),
                ("retry-text".into(), BOX_ALONE.into()),
                ("park".into(), BOX_PARKED.into())
            ]
        );
    }

    /// Every choice listed is a word `tehas answer` takes.
    #[test]
    fn every_choice_listed_is_one_tehas_answer_takes() {
        let options: Vec<String> = (1..=25).map(|i| format!("Option {i}")).collect();
        let all = [
            choices("plan", None, options),
            choices("park", None, vec![]),
        ]
        .concat();
        assert_eq!(all.len(), 20 + 1 + 3);
        for o in &all {
            let word = answer_word(o);
            let args = w(&["12", &word, "--by", "alice"]);
            assert!(crate::answer::words(&args).is_ok(), "{word}");
        }
    }
}
