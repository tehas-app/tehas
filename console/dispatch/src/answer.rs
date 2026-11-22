//! An answer a program gave through `tehas answer` (#2782), read as the poll
//! reads a box: `overlay` writes it into what `questions::answer_of` read of
//! the question's comment, so every rule that acts on a ticked box acts on it the
//! same way, and nothing of what a retry or an answer does changes.
//!
//! The store keeps the answer bound to its question and gives it only while
//! that question is on record (`question-known`), so an answer counts once:
//! the start or the turn it leads to puts a new question on record, or none.
//! An answer the poll does not act on is taken away (`answer_drop`) where a
//! box would be unticked.

use crate::questions::{Asked, Reply, Status, Tick};
use crate::snapshot::Given;
use tehas_core::question::{
    BOX_ALONE, BOX_COMMENTS, BOX_END, BOX_PARKED, OLD_BOX_ALONE, OLD_BOX_CLEAN,
};

/// What was chosen, in the words of `tehas answer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// A park's: retry with the issue and its comments.
    Retry,
    /// A park's: retry with the issue text only.
    RetryText,
    /// A park's: leave it parked.
    Park,
    /// A planning question's option, numbered from 1. `update` is the option
    /// "Update this issue", kept by its number.
    Option(u64),
    /// A planning question's: end the session.
    End,
    /// A planning question's: a reply in words.
    Text(String),
}

/// One answer on record, of the question on record of its issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// The comment id of the question it answers.
    pub question: u64,
    /// park or plan.
    pub kind: String,
    pub choice: Choice,
    pub by: String,
    /// When it was given, as GitHub writes a time.
    pub at: String,
}

impl Answer {
    /// An answer of `question-known`, from its fields as the store writes
    /// them: `at` in Unix seconds, `option` the number of a planning option,
    /// `text` a reply in words. None for one that does not parse: every field
    /// its choice needs is required, and a choice is one of the words of its
    /// question's kind.
    pub fn new(
        question: u64,
        kind: &str,
        choice: &str,
        option: Option<u64>,
        text: Option<&str>,
        by: &str,
        at: i64,
    ) -> Option<Answer> {
        use tehas_core::question::Choice as Word;
        let word = Word::of(choice)?;
        if word.kind().name() != kind || by.is_empty() {
            return None;
        }
        let choice = match word {
            Word::Retry => Choice::Retry,
            Word::RetryText => Choice::RetryText,
            Word::Park => Choice::Park,
            Word::Option | Word::Update => Choice::Option(option.filter(|k| *k >= 1)?),
            Word::End => Choice::End,
            Word::Text => Choice::Text(text?.to_string()),
        };
        Some(Answer {
            question,
            kind: kind.to_string(),
            choice,
            by: by.to_string(),
            at: tehas_core::time::iso(at),
        })
    }

    /// The choice, as a person reads it in a line of the log or a note.
    pub fn said(&self) -> String {
        match &self.choice {
            Choice::Retry => "retry".into(),
            Choice::RetryText => "retry with the issue text only".into(),
            Choice::Park => "leave it parked".into(),
            Choice::Option(k) => format!("option {k}"),
            Choice::End => "end the planning session".into(),
            Choice::Text(_) => "a reply".into(),
        }
    }
}

/// What `overlay` made of an answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Overlay {
    /// No answer, or one of another question: the read is as it was.
    None,
    /// The answer is now the read's one tick, or its reply.
    Given,
    /// A person's tick or reply came after it, by the account named: that
    /// stands, and the caller takes the answer away.
    Later(String),
    /// The question offers no such choice, or the account may no longer
    /// answer: why. The caller takes the answer away.
    Refused(String),
}

/// The answer `a` written into the read `ans` of its question: a choice as
/// the one box ticked, by its account at its time, marked answered; a reply
/// in words as the reply. `allowed` is the account rule of a tick.
pub fn overlay(ans: &mut Asked, a: Option<&Answer>, allowed: impl Fn(&str) -> bool) -> Overlay {
    // The same question, of the same kind.
    let comment = match &ans.comment {
        Some(Given::Number(c)) => c.as_u64(),
        Some(Given::Text(s)) => s.parse().ok(),
        _ => None,
    };
    let of = |a: &&Answer| comment == Some(a.question) && ans.kind == a.kind;
    let Some(a) = a.filter(of) else {
        return Overlay::None;
    };
    if !allowed(&a.by) {
        return Overlay::Refused(format!(
            "the answer of {} is no answer: the account may not authorise work for this factory",
            a.by
        ));
    }
    // A person's choice or reply after the answer is the newer word.
    let after = |at: Option<&str>| at.is_some_and(|t| t > a.at.as_str());
    if ans.tick.status == Status::One && after(Some(&ans.tick.at)) {
        return Overlay::Later(ans.tick.by.clone());
    }
    if let Some(r) = ans.reply.as_ref() {
        if a.kind == "plan" && after(r.at.as_ref().and_then(Given::text)) {
            return Overlay::Later(r.by.clone());
        }
    }
    // The index of the first box whose label is one of `labels`.
    let box_of = |labels: &[&str]| ans.labels.iter().position(|l| labels.contains(&l.as_str()));
    let boxes = ans.boxes;
    let index = match &a.choice {
        Choice::Text(text) => {
            ans.reply = Some(Reply {
                by: a.by.clone(),
                text: Some(Given::Text(text.clone())),
                at: Some(Given::Text(a.at.clone())),
                id: None,
            });
            ans.tick = Tick::none();
            return Overlay::Given;
        }
        Choice::Retry => box_of(&[BOX_COMMENTS, OLD_BOX_CLEAN]),
        Choice::RetryText => box_of(&[BOX_ALONE, OLD_BOX_ALONE]),
        Choice::Park => box_of(&[BOX_PARKED]),
        // The options come first and the end of the session last.
        Choice::Option(k) => usize::try_from(k - 1).ok().filter(|i| i + 1 < boxes),
        Choice::End => box_of(&[BOX_END]).filter(|i| i + 1 == boxes),
    };
    let Some(index) = index else {
        return Overlay::Refused(format!(
            "the answer of {} asks to {}, which its question does not offer",
            a.by,
            a.said()
        ));
    };
    ans.tick = Tick {
        status: Status::One,
        by: a.by.clone(),
        index: Some(index),
        at: a.at.clone(),
        answered: true,
    };
    ans.reply = None;
    Overlay::Given
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tehas_core::question::{q_render, Kind};

    fn answer(kind: &str, choice: Choice) -> Answer {
        Answer {
            question: 4001,
            kind: kind.into(),
            choice,
            by: "alice".into(),
            at: "2026-01-01T02:00:00Z".into(),
        }
    }

    /// The read of a question rendered as the factory renders it, nothing
    /// ticked, with the reply given: what questions::answer gives of it.
    macro_rules! read {
        ($kind:expr, $out:expr, $reply:expr) => {{
            let kind = $kind;
            let body = q_render(kind, 7, 2, &$out, "");
            let labels: Vec<String> = tehas_core::question::listed(&body).unwrap();
            Asked {
                comment: Some(Given::Number(4001.into())),
                asked_at: String::new(),
                body,
                kind: if kind == Kind::Park { "park" } else { "plan" }.into(),
                issue: "7".into(),
                turn: "2".into(),
                boxes: labels.len(),
                labels,
                tick: Tick::none(),
                reply: $reply,
                fixer_reply: None,
            }
        }};
    }

    macro_rules! park {
        () => {
            read!(Kind::Park, json!({"text": "Parked.", "comments": true}), None)
        };
    }

    macro_rules! plan {
        ($reply:expr) => {
            read!(
                Kind::Plan,
                json!({"question": {"question": "Which?", "options": [{"label": "Adopt all"}, {"label": "Update this issue"}]}}),
                $reply
            )
        };
    }

    fn anyone(_: &str) -> bool {
        true
    }

    #[test]
    fn a_park_choice_is_the_box_of_its_label() {
        for (choice, label) in [
            (Choice::Retry, BOX_COMMENTS),
            (Choice::RetryText, BOX_ALONE),
            (Choice::Park, BOX_PARKED),
        ] {
            let mut ans = park!();
            assert_eq!(
                overlay(&mut ans, Some(&answer("park", choice)), anyone),
                Overlay::Given
            );
            let i = ans.tick.index.unwrap();
            assert_eq!(ans.labels[i], label);
            assert_eq!(ans.tick.status, Status::One);
            assert_eq!(ans.tick.by, "alice");
            assert_eq!(ans.tick.at, "2026-01-01T02:00:00Z");
            assert!(ans.tick.answered);
        }
    }

    #[test]
    fn a_retry_the_park_does_not_offer_is_refused() {
        let mut ans = read!(Kind::Park, json!({"text": "Parked.", "retry": false}), None);
        let got = overlay(&mut ans, Some(&answer("park", Choice::Retry)), anyone);
        assert!(
            matches!(got, Overlay::Refused(ref why) if why.contains("does not offer")),
            "{got:?}"
        );
        assert_eq!(ans.tick.status, Status::None);
        // The comments retry is not offered when the image gives none.
        let mut ans = read!(Kind::Park, json!({"text": "Parked."}), None);
        assert!(matches!(
            overlay(&mut ans, Some(&answer("park", Choice::Retry)), anyone),
            Overlay::Refused(_)
        ));
    }

    #[test]
    fn a_planning_choice_is_its_option_or_the_last_box() {
        let mut ans = plan!(None);
        overlay(&mut ans, Some(&answer("plan", Choice::Option(2))), anyone);
        assert_eq!(ans.tick.index, Some(1));
        let mut ans = plan!(None);
        overlay(&mut ans, Some(&answer("plan", Choice::End)), anyone);
        assert_eq!(ans.tick.index, Some(2));
        assert_eq!(ans.boxes, 3);
        let mut ans = plan!(None);
        let got = overlay(&mut ans, Some(&answer("plan", Choice::Option(3))), anyone);
        assert!(matches!(got, Overlay::Refused(_)), "{got:?}");
    }

    #[test]
    fn a_reply_in_words_is_the_reply_unless_the_persons_came_later() {
        let reply = |text: &str, at: &str, id: u64| Reply {
            by: "bob".into(),
            text: Some(Given::Text(text.into())),
            at: Some(Given::Text(at.into())),
            id: Some(Given::Number(id.into())),
        };
        let mut ans = plan!(Some(reply("older", "2026-01-01T01:00:00Z", 5)));
        let a = answer("plan", Choice::Text("Split it.".into()));
        assert_eq!(overlay(&mut ans, Some(&a), anyone), Overlay::Given);
        let given = ans.reply.as_ref().unwrap();
        assert_eq!(given.text, Some(Given::Text("Split it.".into())));
        assert_eq!(given.by, "alice");
        assert_eq!(given.id, None);
        let later = reply("newer", "2026-01-01T03:00:00Z", 6);
        let mut ans = plan!(Some(later.clone()));
        assert_eq!(
            overlay(&mut ans, Some(&a), anyone),
            Overlay::Later("bob".into())
        );
        assert_eq!(ans.reply, Some(later));
    }

    #[test]
    fn a_tick_after_the_answer_stands_and_one_before_it_does_not() {
        let a = answer("park", Choice::Retry);
        let mut ans = park!();
        ans.tick = Tick {
            status: Status::One,
            by: "bob".into(),
            index: Some(2),
            at: "2026-01-01T03:00:00Z".into(),
            answered: false,
        };
        let before = ans.clone();
        assert_eq!(
            overlay(&mut ans, Some(&a), anyone),
            Overlay::Later("bob".into())
        );
        assert_eq!(ans, before);
        ans.tick.at = "2026-01-01T01:00:00Z".into();
        assert_eq!(overlay(&mut ans, Some(&a), anyone), Overlay::Given);
        assert_eq!(ans.tick.by, "alice");
    }

    #[test]
    fn an_answer_of_another_question_or_account_changes_nothing() {
        let mut ans = park!();
        let before = ans.clone();
        let mut other = answer("park", Choice::Retry);
        other.question = 4002;
        assert_eq!(overlay(&mut ans, Some(&other), anyone), Overlay::None);
        assert_eq!(
            overlay(&mut ans, Some(&answer("plan", Choice::End)), anyone),
            Overlay::None
        );
        assert_eq!(overlay(&mut ans, None, anyone), Overlay::None);
        assert_eq!(ans, before);
        let got = overlay(&mut ans, Some(&answer("park", Choice::Retry)), |_| false);
        assert!(matches!(got, Overlay::Refused(_)), "{got:?}");
        assert_eq!(ans, before);
    }

    #[test]
    fn an_answer_is_made_only_whole() {
        assert_eq!(
            Answer::new(4001, "park", "retry", None, None, "alice", 1_767_232_800),
            Some(answer("park", Choice::Retry))
        );
        assert_eq!(
            Answer::new(4001, "plan", "update", Some(2), None, "a", 1).map(|a| a.choice),
            Some(Choice::Option(2))
        );
        for (kind, choice, option, text, by) in [
            ("plan", "retry", None, None, "a"),
            ("park", "option", Some(1), None, "a"),
            ("park", "tick", None, None, "a"),
            ("tick", "retry", None, None, "a"),
            ("plan", "option", Some(0), None, "a"),
            ("plan", "option", None, None, "a"),
            ("plan", "text", None, None, "a"),
            ("park", "retry", None, None, ""),
        ] {
            assert_eq!(
                Answer::new(4001, kind, choice, option, text, by, 1),
                None,
                "{kind} {choice}"
            );
        }
    }
}
