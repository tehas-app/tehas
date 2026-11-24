//! The files a text links to on GitHub's own store, as the ports'
//! `Attachment`s. The links are found by tehas's one link rule,
//! `attach::link::links`, which only takes GitHub's storage over https,
//! with no part of a path `.`, `..` or an escaped `.`, `/` or `\`. This
//! module names what each link is: its id, and its name where the link
//! carries one, and refuses a link the rule cut short. Pure: no request.
//!
//! A link in a code span, a fenced block or a quote counts as an
//! attachment, read as the text gives it, as attach, the dispatcher and the
//! issue feed read the raw text today; the marker reader's dead regions are
//! for markers, not files.

use tehas_ports::{Attachment, AttachmentId, WebLink};

const GITHUB: &str = "https://github.com/";
const IMAGES: &str = "https://user-images.githubusercontent.com/";

/// The attachments `text` links to, each once, in the order it links them.
/// The id is the link itself. A file uploaded to an issue
/// (`user-attachments/files/<n>/<name>`, or a project's
/// `<owner>/<name>/files/<n>/<name>`) has its name, exactly one part of
/// the path. A pasted image (`user-attachments/assets/<id>`, a project's
/// `<owner>/<name>/assets/<n>/<id>`, or `user-images.githubusercontent.com`)
/// has none. A link of another shape, a file link with no name or with
/// more parts after it, or a link longer than a `WebLink` holds, is no
/// attachment.
pub(crate) fn read(text: &str) -> Vec<Attachment> {
    attach::link::links(text)
        .into_iter()
        .filter(|link| whole_somewhere(text, link))
        .filter_map(|link| {
            let name = name_of(&link)?;
            let web = WebLink::new(&link).ok()?;
            let mut a = Attachment::new(AttachmentId::new(link), web);
            a.name = name;
            Some(a)
        })
        .collect()
}

/// The link rule ends a link before a part it refuses (`.`, `..`, an
/// escaped `.`, `/` or `\`), so what it found may be the start of a
/// longer address, cut. Such a link is no attachment: it is taken only
/// where the text holds it whole, with no character of an address
/// straight after it (one the rule takes, `%`, `/` or `\`). An occurrence
/// that runs on is a longer link, or this one cut.
fn whole_somewhere(text: &str, link: &str) -> bool {
    text.match_indices(link).any(|(at, _)| {
        !text[at + link.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || "._~?&=+-%/\\".contains(c))
    })
}

/// The name a link of GitHub's store carries: `Some(None)` for a pasted
/// image, `Some(Some(name))` for a file, None for a link of no shape the
/// store gives an attachment. A query after the name is no part of it.
fn name_of(link: &str) -> Option<Option<String>> {
    if let Some(rest) = link.strip_prefix(IMAGES) {
        return (!rest.is_empty()).then_some(None);
    }
    let parts: Vec<&str> = link.strip_prefix(GITHUB)?.split('/').collect();
    let file = |name: &str| {
        let name = name.split('?').next().unwrap_or_default();
        is_name(name).then(|| Some(name.to_string()))
    };
    match parts.as_slice() {
        ["user-attachments", "assets", id] if !id.is_empty() => Some(None),
        ["user-attachments", "files", n, name] if numbered(n) => file(name),
        [owner, _, "assets", n, id] if *owner != "user-attachments" && numbered(n) => {
            (!id.is_empty()).then_some(None)
        }
        [owner, _, "files", n, name] if *owner != "user-attachments" && numbered(n) => file(name),
        _ => None,
    }
}

/// An upload's number: digits alone.
fn numbered(n: &str) -> bool {
    !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
}

/// One part of a path that names a file: not empty, not `.` or `..`, no
/// escaped slash. The link rule keeps the rest out already; this says it
/// where the name is taken.
fn is_name(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && !part.to_ascii_lowercase().contains("%2f")
        && !part.contains('/')
}
