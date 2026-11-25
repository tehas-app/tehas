//! How a check holds an answer's links to this project's name.

/// `text` ends with `tail`, ASCII case folded: GitHub names a project in
/// any case.
pub(crate) fn ends_with_folded(text: &str, tail: &str) -> bool {
    text.len() >= tail.len()
        && text.is_char_boundary(text.len() - tail.len())
        && text[text.len() - tail.len()..].eq_ignore_ascii_case(tail)
}
