//! The name a check holds an answer's links to: this project's under one
//! name, or any name, the probe [`crate::GitHub::as_here`] asks first.

/// The repository a link must name.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Named<'a> {
    /// This project, under this `owner/name`.
    As(&'a str),
    /// Any repository: a check that still fails did not fail by the name.
    Any,
}

impl Named<'_> {
    /// Whether `text` ends with `{before}/{repo}{rest}`, ASCII case
    /// folded, `repo` this name; for `Any`, with `rest`.
    pub(crate) fn ends(self, text: &str, before: &str, rest: &str) -> bool {
        match self {
            Named::As(repo) => ends_with_folded(text, &format!("{before}/{repo}{rest}")),
            Named::Any => ends_with_folded(text, rest),
        }
    }

    /// Whether `name` is this name, ASCII case folded; any for `Any`.
    pub(crate) fn is(self, name: &str) -> bool {
        match self {
            Named::As(repo) => repo.eq_ignore_ascii_case(name),
            Named::Any => true,
        }
    }
}

/// `text` ends with `tail`, ASCII case folded: GitHub names a project in
/// any case.
pub(crate) fn ends_with_folded(text: &str, tail: &str) -> bool {
    text.len() >= tail.len()
        && text.is_char_boundary(text.len() - tail.len())
        && text[text.len() - tail.len()..].eq_ignore_ascii_case(tail)
}
