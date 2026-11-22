//! The millwright's guidance comment a start is given, and the comment
//! window it is read from. decide reads it to lift a park's hold; apply
//! writes it on the start's record.

use super::iso;
use crate::settings::account;
use crate::snapshot::Comment;
use std::cmp::Ordering;

/// A comment as a cycle's context and the approval record keep it. Its keys
/// and their kinds, all text, are a contract of the store's record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextNote {
    /// `<source>:<id>`.
    pub source: String,
    pub by: String,
    pub at: String,
    /// The body without carriage returns, cut at 4000 characters.
    pub text: String,
}

impl ContextNote {
    /// The note of a comment `id`, the id as JSON writes it.
    pub(crate) fn new(source: &str, id: &str, by: &str, at: &str, body: &str) -> Self {
        ContextNote {
            source: format!("{source}:{id}"),
            by: by.to_string(),
            at: at.to_string(),
            text: body.replace('\r', "").chars().take(4000).collect(),
        }
    }

    /// Each key of the record and its text.
    pub(crate) fn pairs(&self) -> [(&'static str, &str); 4] {
        [
            ("source", &self.source),
            ("by", &self.by),
            ("at", &self.at),
            ("text", &self.text),
        ]
    }
}

/// Whether a comment is in the comment window: written by an account `who`
/// takes, at or after `since` and at or before `at`, and saved by `at`.
pub(crate) fn in_window(
    login: &str,
    created: &str,
    updated: &str,
    since: &str,
    at: &str,
    who: &dyn Fn(&str) -> bool,
) -> bool {
    !login.is_empty()
        && who(login)
        && !at.is_empty()
        && created >= since
        && created <= at
        && updated <= at
}

/// The window's order: by the time written, then by id.
pub(crate) fn window_order(a: (&str, f64), b: (&str, f64)) -> Ordering {
    a.0.cmp(b.0).then(a.1.total_cmp(&b.1))
}

/// The comment that lifts a park's hold off the millwright's `tehas:ready` at
/// `at`: the note the fixer's start puts on the record, when it carries the
/// `action=change` marker within the text a cycle is given and no other
/// account saved it. `action=retry` asks for the same run again, so a newer
/// one of those takes the lift away (#1130). None when `at` is no time.
pub(crate) fn change_note_of(
    nodes: &[Comment],
    n: u64,
    fixer: &str,
    at: &str,
) -> Option<Option<ContextNote>> {
    let since = iso(u64::try_from(tehas_core::time::from_iso(at)? - 600).ok()?);
    let marker = regex::Regex::new(&format!(
        "<!-- tehas:millwright issue={n} action=(retry|change) -->"
    ))
    .ok()?;
    let who = |l: &str| !fixer.is_empty() && account(l) == fixer;
    let mut w: Vec<_> = nodes
        .iter()
        .filter_map(|x| x.note.as_ref())
        .filter(|x| in_window(&x.login, &x.created, &x.updated, &since, at, &who))
        .collect();
    let key = |x: &crate::snapshot::Note| (x.created.clone(), x.id.as_f64().unwrap_or(0.0));
    w.sort_by(|a, b| {
        let (x, y) = (key(a), key(b));
        window_order((&x.0, x.1), (&y.0, y.1))
    });
    let Some(newest) = w.into_iter().rfind(|x| marker.is_match(&x.body)) else {
        return Some(None);
    };
    let note = ContextNote::new(
        "millwright",
        &newest.id.to_string(),
        &newest.login,
        &newest.created,
        &newest.body,
    );
    let change = format!("<!-- tehas:millwright issue={n} action=change -->");
    let lifts = account(&newest.editor) == account(&newest.login) && note.text.contains(&change);
    Some(lifts.then_some(note))
}
