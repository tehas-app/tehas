//! The edit histories of bodies and comments (Decided 119): GraphQL's
//! `userContentEdits`, asked for up to `CHUNK` items in one question, as
//! the poll's `fill_comment_edits` asks them today. Each item is answered
//! alone, by the adapter's one reader of a question of several
//! (`items.rs`): one GitHub cannot find is `NotFound` and the others stand,
//! and the bound holds for each. A failed call fails the items of its
//! question.
//!
//! GitHub lists a text's versions newest first, the oldest its text as
//! created, stamped with the item's creation; the edits are the others.
//! A history that does not add up is `Unread(Inconsistent)`: a count the
//! nodes do not match, a null node, times out of order, an oldest version
//! not stamped with the creation, a newest not stamped with the item's last
//! edit, or a version whose text GitHub deleted.
//!
//! A comment is asked by its node id. A read of comments on this handle
//! keeps the node id of each comment it found edited; any other comment is
//! read once by REST for it first.

use std::collections::HashMap;

use super::{editor, is_node_id, time, Actor};
use crate::port::blueprint_writes::comment_issue;
use crate::tickets::Bot;
use crate::{markers, GitHub};
use serde::Deserialize;
use serde_json::Value;
use tehas_ports::{At, BlueprintNo, Bound, CommentId, Edit, Got, Outcome, Unread, Whole};

/// The most items one question asks of, as `COMMENT_EDIT_CHUNK` today.
pub(super) const CHUNK: usize = 20;

/// The most versions GitHub gives of one connection, the text as created
/// among them. A text with more, edited 100 times or more, is
/// `Unread(Cut)` whatever the bound: no read pages a history.
const VERSIONS_MAX: u32 = 100;

/// The fields of a text's history, its versions newest first.
fn history_fields(first: u32) -> String {
    format!(
        "createdAt lastEditedAt userContentEdits(first: {first}) {{ totalCount nodes {{ editedAt deletedAt diff editor {{ __typename login ...on User {{ databaseId }} ...on Bot {{ databaseId }} }} }} }}"
    )
}

/// The versions one question asks of each item: the bound's edits and the
/// text as created, to check its stamp.
fn versions(bound: Bound) -> u32 {
    bound.get().saturating_add(1).min(VERSIONS_MAX)
}

/// The question of the body histories of `nos`, `i<n>` answering issue n.
fn bodies_question(nos: &[BlueprintNo], bound: Bound) -> String {
    let fields = history_fields(versions(bound));
    let mut q = String::from(
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {",
    );
    for no in nos {
        let n = no.get();
        q.push_str(&format!(" i{n}: issue(number: {n}) {{ number {fields} }}"));
    }
    q.push_str(" } }");
    q
}

/// The question of the comment histories of `nodes`, by node id. It names
/// the repository because every question of the adapter's gives it.
fn comments_question(nodes: &[String], bound: Bound) -> String {
    let ids: Vec<String> = nodes.iter().map(|n| format!("\"{n}\"")).collect();
    format!(
        "query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ id }} nodes(ids: [{}]) {{ __typename ...on IssueComment {{ id databaseId url {} }} }} }}",
        ids.join(", "),
        history_fields(versions(bound))
    )
}

/// Each body's history, and the first failure of a whole question.
type BodiesRead = (Vec<(BlueprintNo, Got<Whole<Edit>>)>, Option<Outcome>);

/// A text's history as GraphQL answers it.
#[derive(Deserialize)]
struct Text {
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "lastEditedAt")]
    last_edited_at: Option<String>,
    #[serde(rename = "userContentEdits")]
    history: Option<History>,
}

#[derive(Deserialize)]
struct History {
    #[serde(rename = "totalCount")]
    total: Option<u32>,
    nodes: Option<Vec<Option<Version>>>,
}

/// One version of a text.
#[derive(Deserialize)]
struct Version {
    #[serde(rename = "editedAt")]
    edited_at: Option<String>,
    #[serde(rename = "deletedAt")]
    deleted_at: Option<String>,
    diff: Option<String>,
    editor: Option<Actor>,
}

/// An issue of the bodies' question.
#[derive(Deserialize)]
struct IssueNode {
    number: Option<u64>,
    #[serde(flatten)]
    text: Text,
}

/// A node of the comments' question.
#[derive(Deserialize)]
struct CommentNode {
    #[serde(rename = "__typename")]
    kind: Option<String>,
    id: Option<String>,
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
    url: Option<String>,
    #[serde(flatten)]
    text: Text,
}

/// A comment's node id and its issue.
type Kept = (String, BlueprintNo);

/// A comment's REST read, for its node id.
#[derive(Deserialize)]
struct RestComment {
    id: Option<u64>,
    node_id: Option<String>,
    html_url: Option<String>,
}

impl Text {
    /// The edits, oldest first, of a text of blueprint `no`, its markers
    /// read as that item's.
    fn edits(self, no: BlueprintNo, bound: Bound, bot: Option<&Bot>) -> Got<Whole<Edit>> {
        let malformed = Outcome::Unread(Unread::Malformed);
        let inconsistent = Outcome::Unread(Unread::Inconsistent);
        let created = time(self.created_at.as_deref())?;
        let history = self.history.ok_or(malformed.clone())?;
        let total = history.total.ok_or(malformed.clone())?;
        let nodes = history.nodes.ok_or(malformed)?;
        let last = self
            .last_edited_at
            .as_deref()
            .map(|t| time(Some(t)))
            .transpose()?;
        if total == 0 {
            return match (last, nodes.is_empty()) {
                (None, true) => Ok(Whole::default()),
                _ => Err(inconsistent),
            };
        }
        let edits = total - 1;
        if edits > bound.get() || total > VERSIONS_MAX {
            return Err(Outcome::Unread(Unread::Cut));
        }
        if nodes.len() != total as usize {
            return Err(inconsistent);
        }
        let nodes: Vec<Version> = nodes
            .into_iter()
            .collect::<Option<_>>()
            .ok_or(inconsistent.clone())?;
        let stamps: Vec<At> = nodes
            .iter()
            .map(|v| time(v.edited_at.as_deref()))
            .collect::<Got<_>>()?;
        if last != stamps.first().copied()
            || stamps.last() != Some(&created)
            || stamps.windows(2).any(|w| w[0] < w[1])
        {
            return Err(inconsistent);
        }
        let mut out = Vec::with_capacity(edits as usize);
        for (v, at) in nodes.into_iter().zip(stamps).take(edits as usize) {
            let text = match (v.deleted_at, v.diff) {
                (None, Some(text)) => text,
                _ => return Err(inconsistent),
            };
            let read = markers::read(&text, Some(no));
            let mut edit = Edit::new(at);
            edit.by = Some(editor(v.editor, bot)?);
            edit.after = read.text;
            edit.markers = read.markers;
            out.push(edit);
        }
        out.reverse();
        Ok(Whole::new(out))
    }
}

impl GitHub {
    /// The body histories of `nos`, in the order asked, and the first
    /// failure of a whole question among them: its items each fail with it.
    pub(super) fn bodies_edits(&mut self, nos: &[BlueprintNo], bound: Bound) -> BodiesRead {
        let asked = once(nos.iter().copied());
        let mut got: HashMap<BlueprintNo, Got<Whole<Edit>>> = HashMap::new();
        let mut whole = None;
        for chunk in asked.chunks(CHUNK) {
            let aliases: Vec<String> = chunk.iter().map(|no| format!("i{}", no.get())).collect();
            let answer = self.ask_aliased_beside::<IssueNode>(
                "the issues' edits",
                &bodies_question(chunk, bound),
                &aliases,
            );
            if let Some(o) = answer.whole {
                whole.get_or_insert(o);
            }
            for (&no, part) in chunk.iter().zip(answer.nodes) {
                let read = part.and_then(|v| self.body_edits_from(v, no, bound));
                got.insert(no, read);
            }
        }
        let each = nos
            .iter()
            .map(|no| {
                (
                    *no,
                    got.get(no)
                        .cloned()
                        .unwrap_or(Err(Outcome::Unread(Unread::Inconsistent))),
                )
            })
            .collect();
        (each, whole)
    }

    fn body_edits_from(&self, issue: IssueNode, no: BlueprintNo, bound: Bound) -> Got<Whole<Edit>> {
        if issue.number != Some(no.get()) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        issue.text.edits(no, bound, self.bot.as_ref())
    }

    /// The comment histories of `ids`, in the order asked. An id that is no
    /// REST id of GitHub's is `NotFound`, with no request.
    pub(super) fn comments_edits(
        &mut self,
        ids: &[CommentId],
        bound: Bound,
    ) -> Vec<(CommentId, Got<Whole<Edit>>)> {
        let mut got: HashMap<CommentId, Got<Whole<Edit>>> = HashMap::new();
        let mut asked: Vec<(CommentId, u64, Kept)> = Vec::new();
        for id in once(ids.iter().cloned()) {
            match self.comment_node(&id) {
                Ok((n, kept)) => asked.push((id, n, kept)),
                Err(o) => {
                    got.insert(id, Err(o));
                }
            }
        }
        for chunk in asked.chunks(CHUNK) {
            let nodes: Vec<String> = chunk.iter().map(|(_, _, (node, _))| node.clone()).collect();
            let parts = self.ask_nodes::<CommentNode>(
                "the comments' edits",
                &comments_question(&nodes, bound),
                chunk.len(),
            );
            for ((id, n, kept), part) in chunk.iter().zip(parts) {
                let read = part.and_then(|v| self.comment_edits_from(v, *n, kept, bound));
                got.insert(id.clone(), read);
            }
        }
        ids.iter()
            .map(|id| {
                let read = got.get(id).cloned();
                (
                    id.clone(),
                    read.unwrap_or(Err(Outcome::Unread(Unread::Inconsistent))),
                )
            })
            .collect()
    }

    /// The REST id of comment `id`, its node id and its issue: kept from a
    /// read of comments, or read by REST. A comment that is not on an issue
    /// of this project is `NotFound`. REST's 404 is `Unread(Inconsistent)`:
    /// a repository that is not there, or a token that cannot see it, gives
    /// it too, and a caller must not drop a history on it.
    fn comment_node(&mut self, id: &CommentId) -> Got<(u64, Kept)> {
        let text = id.as_str();
        let n: u64 = match text.bytes().all(|b| b.is_ascii_digit()) {
            true => text.parse().map_err(|_| Outcome::NotFound)?,
            false => return Err(Outcome::NotFound),
        };
        if let Some(kept) = self.comment_nodes.get(&n) {
            return Ok((n, kept.clone()));
        }
        let path = format!("repos/{}/issues/comments/{n}", self.repo);
        let out =
            self.call("the comment", &["api", &path])
                .map_err(|e| match self.outcome(&e) {
                    Outcome::NotFound => Outcome::Unread(Unread::Inconsistent),
                    o => o,
                })?;
        let malformed = Outcome::Unread(Unread::Malformed);
        let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
        tehas_core::accounts::vouch(&mut v);
        let c: RestComment = serde_json::from_value(v).map_err(|_| malformed.clone())?;
        if c.id != Some(n) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let url = c.html_url.as_deref().ok_or(malformed.clone())?;
        let no = self.as_here(|repo| comment_issue(repo, url, n))?;
        let node = c.node_id.filter(|n| is_node_id(n)).ok_or(malformed)?;
        self.comment_nodes.insert(n, (node.clone(), no));
        Ok((n, (node, no)))
    }

    /// The history of comment `n` from its node, which must be the comment
    /// kept, on the issue kept.
    fn comment_edits_from(
        &mut self,
        c: CommentNode,
        n: u64,
        (node, no): &Kept,
        bound: Bound,
    ) -> Got<Whole<Edit>> {
        if c.kind.as_deref() != Some("IssueComment")
            || c.id.as_deref() != Some(node.as_str())
            || c.database_id != Some(n)
        {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let url = c.url.as_deref().ok_or(Outcome::Unread(Unread::Malformed))?;
        if self.as_here(|repo| comment_issue(repo, url, n))? != *no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        c.text.edits(*no, bound, self.bot.as_ref())
    }
}

/// The items in the order first asked, each once.
fn once<T: Eq + std::hash::Hash + Clone>(items: impl Iterator<Item = T>) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items.filter(|i| seen.insert(i.clone())).collect()
}
