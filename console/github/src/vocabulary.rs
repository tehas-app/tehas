//! The words the site holds: the repository's labels, each with its node
//! id, as `Tickets::marks` answers them, and the owner's issue types, as
//! `Tickets::kinds` answers them. Each is a GraphQL question paged here to
//! its end, or one past the bound.

use serde::Deserialize;

use crate::ticket::{part, Page, Pages};
use crate::GitHub;
use tehas_ports::{Bound, Got, Kind, MarkHeld, MarkId, Outcome, Unread, Whole};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// Every label of the repository with its node id, the id `state/labels`
/// pins, a page of them in name order.
const LABELS: &str = "query($owner: String!, $name: String!, $cursor: String) { repository(owner: $owner, name: $name) { labels(first: 100, after: $cursor, orderBy: {field: NAME, direction: ASC}) { pageInfo { hasNextPage endCursor } nodes { id name } } } }";

/// The issue types of the repository's owner, a page of them.
const ISSUE_TYPES: &str = "query($owner: String!, $name: String!, $cursor: String) { repository(owner: $owner, name: $name) { issueTypes(first: 100, after: $cursor) { pageInfo { hasNextPage endCursor } nodes { name } } } }";

#[derive(Deserialize)]
struct Label {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Type {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Types {
    /// None where the owner has no issue types: a personal account's.
    #[serde(deserialize_with = "crate::ticket::present")]
    issue_types: Option<Page<Type>>,
}

impl GitHub {
    /// The repository's labels as marks, each once with its node id, in
    /// name order. A name no mark can have is left out, and kept for
    /// [`GitHub::take_left_out`]. More marks than `bound` is
    /// `Unread(Cut)`.
    pub(crate) fn marks_held(&mut self, bound: Bound) -> Got<Whole<MarkHeld>> {
        let mut all: Vec<MarkHeld> = Vec::new();
        let mut pages = Pages::default();
        let mut cursor: Option<String> = None;
        loop {
            let out = self
                .ask("the labels", LABELS, None, cursor.as_deref())
                .map_err(|e| self.outcome(&e))?;
            let page: Page<Label> = part(&out, "/data/repository/labels")?;
            let labels = page
                .nodes
                .into_iter()
                .map(|l| l.ok_or(INCONSISTENT))
                .collect::<Got<Vec<Label>>>()?;
            let names: Vec<String> = labels.iter().map(|l| l.name.clone()).collect();
            for (label, mark) in labels.into_iter().zip(self.read_marks(&names)) {
                if label.id.is_empty() {
                    return Err(MALFORMED);
                }
                let Some(mark) = mark else {
                    continue;
                };
                let id = MarkId::new(label.id);
                if all.iter().any(|h| h.mark == mark || h.id == id) {
                    return Err(INCONSISTENT);
                }
                all.push(MarkHeld::new(mark, id));
            }
            if all.len() > bound.get() as usize {
                return Err(CUT);
            }
            match pages.next(page.page_info)? {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(Whole::new(all))
    }

    /// The owner's issue types as kinds, each once, in the site's order:
    /// none for an owner that has none. A name no kind can have
    /// (`Kind::Unknown`) is left out, and kept for
    /// [`GitHub::take_left_out`], so a setting no type can match is not
    /// matched by it.
    pub(crate) fn kinds_held(&mut self) -> Got<Whole<Kind>> {
        let mut all: Vec<Kind> = Vec::new();
        let mut pages = Pages::default();
        let mut cursor: Option<String> = None;
        loop {
            let out = self
                .ask("the issue types", ISSUE_TYPES, None, cursor.as_deref())
                .map_err(|e| self.outcome(&e))?;
            let types: Types = part(&out, "/data/repository")?;
            let Some(page) = types.issue_types else {
                return match cursor {
                    None => Ok(Whole::default()),
                    Some(_) => Err(INCONSISTENT),
                };
            };
            for t in page.nodes {
                let name = t.ok_or(INCONSISTENT)?.name;
                let kind = Kind::named(&name);
                if kind == Kind::Unknown {
                    if !self.left_out.contains(&name) {
                        self.left_out.push(name);
                    }
                    continue;
                }
                if all.contains(&kind) {
                    return Err(INCONSISTENT);
                }
                all.push(kind);
            }
            match pages.next(page.page_info)? {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(Whole::new(all))
    }
}
