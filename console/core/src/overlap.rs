//! The paths an issue's body names that a branch changes.
//!
//! An issue declares the files it will touch in `[paths: ...]` lists, one per
//! deliverable: `[paths: `lib/a.sh`, `console/core/src/b.rs` (new)]`. Before
//! it starts a cycle, the dispatcher asks which of those paths a running
//! cycle's branch already changes (#725); this module is the answer, and the
//! store's `paths-overlap` verb is the way the shell reaches it.
//!
//! A list starts at `[paths:` and ends at the first `]` outside backquotes. Its
//! items are split at the commas outside backquotes. An item is trimmed of the
//! white space around it, of the ` (new)` marker a new file carries, and of
//! its backquotes; what is left is the named path. A named path matches a
//! changed file when the two are equal, and a named path ending in `/` covers
//! every changed file under it. The answer is the named paths that match, each
//! once, in the order the body names them. A body with no paths list names
//! nothing, so it matches nothing. The rule is plain text: no pattern, no glob,
//! no regex.

/// What opens a list of the paths a deliverable touches.
const OPEN: &str = "[paths:";

/// The marker a list puts after a path the deliverable creates.
const NEW: &str = "(new)";

/// Every path the `[paths: ...]` lists of a body name, each once, in the order
/// the body names them.
pub fn named(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find(OPEN) {
        rest = &rest[at + OPEN.len()..];
        let (items, end) = list(rest);
        for item in items {
            let path = clean(item);
            if !path.is_empty() && !out.iter().any(|p| p == path) {
                out.push(path.to_string());
            }
        }
        rest = &rest[end..];
    }
    out
}

/// The named paths of `body` that a file of `changed` equals or, for a named
/// directory, lies under: each once, in the order the body names them. The
/// lines of `changed` are trimmed, and an empty one is no file.
pub fn matches<S: AsRef<str>>(body: &str, changed: &[S]) -> Vec<String> {
    let changed: Vec<&str> = changed
        .iter()
        .map(|c| c.as_ref().trim())
        .filter(|c| !c.is_empty())
        .collect();
    named(body)
        .into_iter()
        .filter(|n| changed.iter().any(|c| covers(n, c)))
        .collect()
}

/// A named file is the one file equal to it; a named directory, ending in `/`,
/// is every file under it.
fn covers(named: &str, changed: &str) -> bool {
    if named.ends_with('/') {
        changed.starts_with(named)
    } else {
        changed == named
    }
}

/// The items of one list, read from just after its `[paths:`, and the offset
/// just past its closing `]` (the end of the text when it never closes). A
/// comma or a `]` between backquotes is part of a path, not the list's.
fn list(text: &str) -> (Vec<&str>, usize) {
    let mut items = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        match ch {
            '`' => quoted = !quoted,
            ',' if !quoted => {
                items.push(&text[start..i]);
                start = i + 1;
            }
            ']' if !quoted => {
                items.push(&text[start..i]);
                return (items, i + 1);
            }
            _ => {}
        }
    }
    items.push(&text[start..]);
    (items, text.len())
}

/// A list item as the path it names: without the white space around it, the
/// ` (new)` marker, or its backquotes, whether the marker is inside the
/// backquotes or after them.
fn clean(item: &str) -> &str {
    let mut s = item.trim();
    loop {
        let before = s;
        s = s.trim();
        if let Some(t) = s.strip_suffix(NEW) {
            s = t;
        }
        if let Some(t) = s.strip_prefix('`').and_then(|t| t.strip_suffix('`')) {
            s = t;
        }
        if s == before {
            return s;
        }
    }
}
