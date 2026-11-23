//! `tehas question`: the question an issue waits on and the choices it
//! offers, in the words `tehas answer` takes (#2782), or the questions of the
//! whole store: every one that waits, or every one after a number
//! (docs/specs/2026-11-22-events.md, T11.6). Read only.
//! console/src/usage/question.txt is the text a person reads.
//!
//!   tehas question <issue> [--json]
//!   tehas question [--after <id>] [--json]
//!
//! It reads the store (store question-show, question-list) and nothing else.
//! A planning question offers its options and the end of the session, a
//! park's question the choices it was asked with. A park's question asked
//! before the store kept its choices lists all three; the poll refuses an
//! answer its comment does not offer. The store keeps a planning question's
//! options only while the issue waits on it, so a list leaves the choices of
//! an answered or withdrawn planning question out.
//!
//! Exit: 0 a question waits, or the list is printed, an empty one too; 1 the
//! store refused or could not be read; 3 no question waits on the issue; 64 a
//! bad argument.

use crate::store;
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;
use tehas_core::grammar::digits;
use tehas_core::question::{park_choices, plan_choices, Choice, Offered, CHOICES};
use tehas_core::{exit, time};

const USAGE: &str =
    "usage: tehas question <issue> [--json] | tehas question [--after <id>] [--json]";

/// Which questions are asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum Which {
    /// The one the issue waits on.
    Issue(u64),
    /// The one each issue waits on.
    Waiting,
    /// Every one numbered above this, the answered and the withdrawn too.
    After(u64),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    which: Which,
    json: bool,
}

fn bad(why: &str) -> u8 {
    eprintln!("tehas question: {why}");
    eprintln!("{USAGE}");
    exit::USAGE
}

/// The arguments, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let (mut issue, mut after, mut json) = (None, None, false);
    let number = |w: &str, max, zero| digits(w, max, zero).and_then(|n| u64::try_from(n).ok());
    let mut rest = given.iter();
    while let Some(w) = rest.next() {
        match w.strip_prefix("--") {
            Some("json") if !json => json = true,
            Some("after") if after.is_none() => {
                let n = rest.next().and_then(|v| number(v, 18, true));
                let Some(n) = n else {
                    return Err(bad("--after takes a question's number, or 0"));
                };
                after = Some(n);
            }
            _ if w.starts_with('-') => return Err(bad(&format!("unknown argument: {w}"))),
            _ if issue.is_some() => return Err(bad(&format!("one issue number at most: {w}"))),
            _ => {
                let Some(n) = number(w, 10, false) else {
                    return Err(bad(&format!("not an issue number: {w}")));
                };
                issue = Some(n);
            }
        }
    }
    let which = match (issue, after) {
        (Some(_), Some(_)) => return Err(bad("--after is not taken with an issue")),
        (Some(issue), None) => Which::Issue(issue),
        (None, Some(id)) => Which::After(id),
        (None, None) => Which::Waiting,
    };
    Ok(Asked { which, json })
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

/// A question as the store gives it (store/src/answer.rs).
#[derive(Debug, PartialEq, Eq)]
struct Row {
    id: u64,
    issue: u64,
    kind: String,
    attempt: u64,
    asked: i64,
    answered: Option<i64>,
    withdrawn: Option<i64>,
    offers: Option<Vec<String>>,
    options: Option<Vec<String>>,
}

impl Row {
    /// The choices, where the store still knows them: every park's, and a
    /// planning question's while its options are on record.
    fn choices(&self) -> Option<Vec<Offered>> {
        let known = self.kind != "plan" || self.options.is_some();
        known.then(|| {
            choices(
                &self.kind,
                self.offers.clone(),
                self.options.clone().unwrap_or_default(),
            )
        })
    }

    /// The id of the cycle that parked, `<issue>-<attempt>`: a park's alone.
    fn cycle(&self) -> Option<String> {
        (self.kind == "park").then(|| format!("{}-{}", self.issue, self.attempt))
    }
}

/// The questions of the store's answer to question-show or question-list,
/// none for a `question` of null. `None` when the line is not that answer:
/// every field is required, and a kind is one of the two.
fn rows(line: &str) -> Option<Vec<Row>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let texts = |v: &Value| -> Option<Option<Vec<String>>> {
        if v.is_null() {
            return Some(None);
        }
        let all = v.as_array()?.iter().map(|s| s.as_str().map(str::to_string));
        all.collect::<Option<Vec<_>>>().map(Some)
    };
    let time = |v: &Value| match v {
        Value::Null => Some(None),
        v => v.as_i64().map(Some),
    };
    let row = |q: &Value| -> Option<Row> {
        Some(Row {
            id: q["id"].as_u64()?,
            issue: q["issue"].as_u64()?,
            kind: Some(q["kind"].as_str()?)
                .filter(|k| ["park", "plan"].contains(k))?
                .to_string(),
            attempt: q["attempt"].as_u64()?,
            asked: q["posted_at"].as_i64()?,
            answered: time(&q["answered_at"])?,
            withdrawn: time(&q["withdrawn_at"])?,
            offers: texts(&q["offers"])?,
            options: texts(&q["options"])?,
        })
    };
    match (&v["questions"], &v["question"]) {
        (Value::Array(all), _) => all.iter().map(row).collect(),
        (_, Value::Null) => Some(Vec::new()),
        (_, one) => Some(vec![row(one)?]),
    }
}

/// What `--json` prints: the one question of an issue, or the list.
fn object(which: &Which, rows: &[Row]) -> String {
    let said = |offered: Vec<Offered>| -> Vec<Value> {
        let one = |o: &Offered| json!({"label": o.label, "choice": answer_word(o)});
        offered.iter().map(one).collect()
    };
    if let (Which::Issue(issue), Some(q)) = (which, rows.first()) {
        let listed = said(choices(
            &q.kind,
            q.offers.clone(),
            q.options.clone().unwrap_or_default(),
        ));
        return json!({"api": 1, "issue": issue, "question": {"id": q.id, "kind": q.kind,
            "askedAt": time::iso(q.asked), "choices": listed}})
        .to_string();
    }
    let listed: Vec<Value> = rows
        .iter()
        .map(|q| {
            let mut o = json!({"id": q.id, "issue": q.issue, "kind": q.kind,
                "askedAt": time::iso(q.asked)});
            let times = [("answeredAt", q.answered), ("withdrawnAt", q.withdrawn)];
            for (key, at) in times {
                if let Some(at) = at {
                    o[key] = json!(time::iso(at));
                }
            }
            if let Some(cycle) = q.cycle() {
                o["cycle"] = json!(cycle);
            }
            if let Some(offered) = q.choices() {
                o["choices"] = json!(said(offered));
            }
            o
        })
        .collect();
    json!({"api": 1, "questions": listed}).to_string()
}

/// A line of the list a person reads.
fn list_line(q: &Row) -> String {
    let mut line = format!(
        "question {}: #{} {}, asked at {}",
        q.id,
        q.issue,
        q.kind,
        time::iso(q.asked)
    );
    for (word, at) in [("answered", q.answered), ("withdrawn", q.withdrawn)] {
        if let Some(at) = at {
            line.push_str(&format!(", {word} at {}", time::iso(at)));
        }
    }
    line
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
    let call: Vec<String> = match a.which {
        Which::Issue(issue) => vec!["question-show".into(), issue.to_string()],
        Which::Waiting => vec!["question-list".into()],
        Which::After(id) => vec!["question-list".into(), "--after".into(), id.to_string()],
    };
    let call: Vec<&str> = call.iter().map(String::as_str).collect();
    let line = match store::line(&here, &call) {
        Ok(line) => line,
        Err(r) => {
            eprintln!("tehas question: the store said: {}", r.line);
            return exit::FAILED;
        }
    };
    let Some(rows) = rows(&line) else {
        eprintln!(
            "tehas question: the store's answer to {} could not be read",
            call[0]
        );
        return exit::FAILED;
    };
    if let (Which::Issue(n), None) = (&a.which, rows.first()) {
        eprintln!("tehas question: no question waits on #{n}");
        return exit::NO;
    }
    if a.json {
        tehas_core::outln!("{}", object(&a.which, &rows));
        return exit::OK;
    }
    match (&a.which, rows.first()) {
        (Which::Issue(n), Some(q)) => {
            tehas_core::outln!(
                "#{n} waits on its {} question, asked at {}:",
                q.kind,
                time::iso(q.asked)
            );
            let options = q.options.clone().unwrap_or_default();
            for o in &choices(&q.kind, q.offers.clone(), options) {
                tehas_core::outln!("  {:<12} {}", answer_word(o), o.label);
            }
        }
        (Which::After(id), None) => tehas_core::outln!("no question after {id}"),
        (_, None) => tehas_core::outln!("no question waits"),
        _ => rows
            .iter()
            .for_each(|q| tehas_core::outln!("{}", list_line(q))),
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
    fn the_words_are_an_issue_or_a_list_and_a_switch() {
        let which = |a: &[&str]| words(&w(a)).map(|a| (a.which, a.json));
        assert_eq!(which(&["12"]), Ok((Which::Issue(12), false)));
        assert_eq!(which(&[]), Ok((Which::Waiting, false)));
        assert_eq!(which(&["--after", "0"]), Ok((Which::After(0), false)));
        assert_eq!(which(&["--after", "7"]), Ok((Which::After(7), false)));
        for bad in [
            &["0"][..],
            &["12", "13"],
            &["12", "--soon"],
            &["x"],
            &["--after"],
            &["--after", "x"],
            &["--after", "07"],
            &["--after", "1", "--after", "2"],
            &["12", "--after", "1"],
        ] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    /// The store's answers are read whole or not at all.
    #[test]
    fn the_stores_answer_is_read_whole_or_refused() {
        let one = r#"{"id":2,"issue":12,"kind":"park","attempt":3,"posted_at":5,"answered_at":null,"withdrawn_at":9,"offers":["park"],"options":null}"#;
        let read = rows(&format!(r#"{{"questions":[{one}]}}"#)).unwrap();
        assert_eq!(read, rows(&format!(r#"{{"question":{one}}}"#)).unwrap());
        assert_eq!(read[0].cycle().as_deref(), Some("12-3"));
        assert_eq!((read[0].answered, read[0].withdrawn), (None, Some(9)));
        assert_eq!(rows(r#"{"question":null}"#), Some(vec![]));
        assert_eq!(rows(r#"{"questions":[]}"#), Some(vec![]));
        for (from, to) in [
            (r#""id":2,"#, ""),
            (r#""kind":"park""#, r#""kind":"other""#),
            (r#""withdrawn_at":9"#, r#""withdrawn_at":"9""#),
            (r#"["park"]"#, "[1]"),
        ] {
            let line = format!(r#"{{"questions":[{}]}}"#, one.replace(from, to));
            assert_eq!(rows(&line), None, "{line}");
        }
        assert_eq!(rows("no"), None);
    }

    /// A planning question the issue no longer waits on has no cycle and no
    /// choices: the store kept its options with the turn.
    #[test]
    fn a_list_leaves_out_what_a_question_has_not() {
        let line = r#"{"questions":[{"id":1,"issue":12,"kind":"plan","attempt":1,"posted_at":0,"answered_at":60,"withdrawn_at":null,"offers":null,"options":null}]}"#;
        let printed = object(&Which::After(0), &rows(line).unwrap());
        assert_eq!(
            printed,
            r#"{"api":1,"questions":[{"answeredAt":"1970-01-01T00:01:00Z","askedAt":"1970-01-01T00:00:00Z","id":1,"issue":12,"kind":"plan"}]}"#
        );
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
