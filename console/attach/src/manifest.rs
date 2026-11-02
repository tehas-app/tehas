//! The manifest.json of an issue's directory, keys in the order the readers of
//! it have always seen them.

use crate::given::{cut, Ordered, CUT_LINK, MOST_LEFT};
use serde_json::Value;

/// One file given to a cycle. A file of an archive also says the archive and
/// where in it the file stood; a text file says the name it had.
pub struct Given<'a> {
    pub file: &'a str,
    pub link: &'a str,
    pub name: Option<&'a str>,
    pub kind: &'a str,
    pub bytes: i64,
    pub sha256: &'a str,
    pub source: &'a str,
    pub path: Option<&'a str>,
}

pub fn given(g: &Given) -> Ordered {
    let mut e = Ordered::map([
        ("file", Ordered::text(g.file)),
        ("link", Ordered::text(g.link)),
    ]);
    if let Some(name) = g.name {
        e.set("name", Ordered::text(name));
    }
    e.set("type", Ordered::text(g.kind));
    e.set("bytes", Ordered::Plain(g.bytes.into()));
    e.set("sha256", Ordered::text(g.sha256));
    e.set("source", Ordered::text(g.source));
    if let Some(path) = g.path {
        e.set("zip", Ordered::text(g.link));
        e.set("path", Ordered::text(path));
    }
    e
}

/// One link or entry left out, and why.
pub fn left(link: &str, reason: &str, source: &str, path: Option<&str>) -> Ordered {
    let mut e = Ordered::map([
        ("link", Ordered::text(link)),
        ("reason", Ordered::text(reason)),
        ("source", Ordered::text(source)),
    ]);
    if let Some(path) = path {
        e.set("path", Ordered::text(path));
    }
    e
}

/// At most 100 left-out entries, each link cut at 512 characters, and how many
/// more there were: an issue dense with attachments leaves a record the
/// improvement loop can copy rather than none.
fn capped(mut left: Vec<Ordered>) -> (Vec<Ordered>, usize) {
    let more = left.len().saturating_sub(MOST_LEFT);
    left.truncate(MOST_LEFT);
    for e in &mut left {
        if let Some(Ordered::Plain(Value::String(link))) = e.get("link") {
            let link = cut(link, CUT_LINK);
            e.set("link", Ordered::text(&link));
        }
    }
    (left, more)
}

/// What one start of an issue was given. `files` is absent where the instance
/// gives a cycle none.
pub fn of_issue(
    images: Vec<Ordered>,
    files: Option<Vec<Ordered>>,
    left: Vec<Ordered>,
    issue: Value,
    approval: Ordered,
    context: Option<&str>,
) -> Ordered {
    let (left, more) = capped(left);
    let mut m = Ordered::map([("images", Ordered::List(images))]);
    if let Some(files) = files {
        m.set("files", Ordered::List(files));
    }
    m.set("leftOut", Ordered::List(left));
    m.set("issue", Ordered::Plain(issue));
    m.set("approval", approval);
    if let Some(context) = context {
        m.set("retryContext", Ordered::text(context));
    }
    m.set("leftOutMore", Ordered::Plain(more.into()));
    m
}
