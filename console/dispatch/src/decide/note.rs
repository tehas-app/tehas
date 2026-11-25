//! The comment window a start reads the comments it is given from.

use std::cmp::Ordering;

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
