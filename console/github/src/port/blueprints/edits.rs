//! The edit histories of bodies and comments (Decided 119): GraphQL's
//! `userContentEdits`, asked for up to `CHUNK` items in one question, as
//! the poll's `fill_comment_edits` asks them today. Each item is answered
//! alone: one GitHub cannot find is `NotFound` and the others stand, and
//! the bound holds for each. A failed call fails the items of its question.
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

use super::{editor, ends_with_folded, is_node_id, priced, time, Actor};
use crate::tickets::Bot;
use crate::{markers, ForgeError, GitHub};
use serde::Deserialize;
use serde_json::Value;
use tehas_ports::{At, BlueprintNo, Bound, CommentId, Edit, Got, Outcome, Pool, Unread, Whole};

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

/// What a question of several items answered: each item's part by its
/// place, or the outcome GitHub gave that item alone.
struct Several {
    parts: Vec<Got<Value>>,
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
    /// The body histories of `nos`, in the order asked.
    pub(super) fn bodies_edits(
        &mut self,
        nos: &[BlueprintNo],
        bound: Bound,
    ) -> Vec<(BlueprintNo, Got<Whole<Edit>>)> {
        let asked = once(nos.iter().copied());
        let mut got: HashMap<BlueprintNo, Got<Whole<Edit>>> = HashMap::new();
        for chunk in asked.chunks(CHUNK) {
            let paths: Vec<Vec<String>> = chunk
                .iter()
                .map(|no| vec!["repository".to_string(), format!("i{}", no.get())])
                .collect();
            let answer = self.ask_several(
                "the issues' edits",
                &bodies_question(chunk, bound),
                &paths,
                |v| {
                    v.pointer("/data/repository")
                        .map(|r| r.is_object())
                        .unwrap_or(false)
                },
            );
            match answer {
                Err(o) => got.extend(chunk.iter().map(|&no| (no, Err(o.clone())))),
                Ok(several) => {
                    for (&no, part) in chunk.iter().zip(several.parts) {
                        let read = part.and_then(|v| self.body_edits_from(v, no, bound));
                        got.insert(no, read);
                    }
                }
            }
        }
        nos.iter()
            .map(|no| {
                (
                    *no,
                    got.get(no)
                        .cloned()
                        .unwrap_or(Err(Outcome::Unread(Unread::Inconsistent))),
                )
            })
            .collect()
    }

    fn body_edits_from(&self, v: Value, no: BlueprintNo, bound: Bound) -> Got<Whole<Edit>> {
        let issue: IssueNode =
            serde_json::from_value(v).map_err(|_| Outcome::Unread(Unread::Malformed))?;
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
            let paths: Vec<Vec<String>> = (0..chunk.len())
                .map(|i| vec!["nodes".to_string(), i.to_string()])
                .collect();
            let answer = self.ask_several(
                "the comments' edits",
                &comments_question(&nodes, bound),
                &paths,
                |v| {
                    v.pointer("/data/nodes")
                        .map(Value::is_array)
                        .unwrap_or(false)
                },
            );
            match answer {
                Err(o) => got.extend(chunk.iter().map(|(id, _, _)| (id.clone(), Err(o.clone())))),
                Ok(several) => {
                    for ((id, n, kept), part) in chunk.iter().zip(several.parts) {
                        let read = part.and_then(|v| self.comment_edits_from(v, *n, kept, bound));
                        got.insert(id.clone(), read);
                    }
                }
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
    /// of this project is `NotFound`.
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
        let out = self
            .call("the comment", &["api", &path])
            .map_err(|e| self.outcome(&e))?;
        let malformed = Outcome::Unread(Unread::Malformed);
        let mut v: Value = serde_json::from_str(out.trim()).map_err(|_| malformed.clone())?;
        tehas_core::accounts::vouch(&mut v);
        let c: RestComment = serde_json::from_value(v).map_err(|_| malformed.clone())?;
        if c.id != Some(n) {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let no = issue_of(
            &self.repo,
            c.html_url.as_deref().ok_or(malformed.clone())?,
            n,
        )?;
        let node = c.node_id.filter(|n| is_node_id(n)).ok_or(malformed)?;
        self.comment_nodes.insert(n, (node.clone(), no));
        Ok((n, (node, no)))
    }

    /// The history of comment `n` from its node, which must be the comment
    /// kept, on the issue kept.
    fn comment_edits_from(
        &self,
        v: Value,
        n: u64,
        (node, no): &Kept,
        bound: Bound,
    ) -> Got<Whole<Edit>> {
        let c: CommentNode =
            serde_json::from_value(v).map_err(|_| Outcome::Unread(Unread::Malformed))?;
        if c.kind.as_deref() != Some("IssueComment")
            || c.id.as_deref() != Some(node.as_str())
            || c.database_id != Some(n)
        {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        let url = c.url.as_deref().ok_or(Outcome::Unread(Unread::Malformed))?;
        if issue_of(&self.repo, url, n)? != *no {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        c.text.edits(*no, bound, self.bot.as_ref())
    }

    /// One question of several items, `paths` the place GitHub's errors
    /// name each by, `whole` whether the data holds the items' list. A
    /// failed call, an answer that does not parse or lacks the list, and an
    /// error that names no item fail every item: a spent budget when GitHub
    /// says the rate is limited, else `Unread(Inconsistent)`. An error that
    /// names an item fails that item alone: `NotFound` when GitHub cannot
    /// find it and its part is null, else `Unread(Inconsistent)`. An item
    /// with no error and no part is `Unread(Inconsistent)`.
    fn ask_several(
        &mut self,
        call: &str,
        question: &str,
        paths: &[Vec<String>],
        whole: impl Fn(&Value) -> bool,
    ) -> Got<Several> {
        let (owner, name) = self.owner_name();
        let query = format!("query={question}");
        let args = ["api", "graphql", "-F", &owner, "-F", &name, "-f", &query];
        let out = match self.call_answer(&args) {
            Ok(out) => out,
            Err(f) => match f.answer.filter(|a| answered(a)) {
                Some(out) => out,
                // Not one item's: GitHub names which in its answer.
                None => {
                    return Err(match self.outcome(&ForgeError::new(call, f.why)) {
                        Outcome::NotFound => Outcome::Unread(Unread::Inconsistent),
                        o => priced(o),
                    })
                }
            },
        };
        let mut v: Value =
            serde_json::from_str(out.trim()).map_err(|_| Outcome::Unread(Unread::Malformed))?;
        let errors: Vec<GraphError> = match v.get("errors") {
            None | Some(Value::Null) => Vec::new(),
            Some(e) => {
                serde_json::from_value(e.clone()).map_err(|_| Outcome::Unread(Unread::Malformed))?
            }
        };
        let mut failed: Vec<Option<Outcome>> = vec![None; paths.len()];
        for e in errors {
            let at = e
                .path
                .as_ref()
                .and_then(|p| paths.iter().position(|q| path_is(p, q)));
            match at {
                Some(i) if e.kind.as_deref() == Some("NOT_FOUND") => {
                    failed[i] = Some(Outcome::NotFound);
                }
                Some(i) => failed[i] = Some(Outcome::Unread(Unread::Inconsistent)),
                None if e.kind.as_deref() == Some("RATE_LIMITED") => {
                    return Err(Outcome::Spent {
                        pool: Pool::Priced,
                        until: At::new((self.now)()),
                    })
                }
                None => return Err(Outcome::Unread(Unread::Inconsistent)),
            }
        }
        if !whole(&v) {
            return Err(Outcome::Unread(Unread::Malformed));
        }
        tehas_core::accounts::vouch(&mut v);
        let parts = paths
            .iter()
            .zip(failed)
            .map(|(p, failed)| {
                let mut at = String::from("/data");
                for step in p {
                    at.push('/');
                    at.push_str(step);
                }
                let part = v
                    .pointer_mut(&at)
                    .map(std::mem::take)
                    .unwrap_or(Value::Null);
                match (failed, part) {
                    (Some(Outcome::NotFound), Value::Null) => Err(Outcome::NotFound),
                    (Some(_), _) | (None, Value::Null) => {
                        Err(Outcome::Unread(Unread::Inconsistent))
                    }
                    (None, part) => Ok(part),
                }
            })
            .collect();
        Ok(Several { parts })
    }
}

/// One of GraphQL's `errors`.
#[derive(Deserialize)]
struct GraphError {
    #[serde(rename = "type")]
    kind: Option<String>,
    path: Option<Vec<Value>>,
}

/// GitHub's path of an error is in the item at `item`.
fn path_is(path: &[Value], item: &[String]) -> bool {
    path.len() >= item.len()
        && path.iter().zip(item).all(|(p, q)| match p {
            Value::String(s) => s == q,
            Value::Number(n) => n.to_string() == *q,
            _ => false,
        })
}

/// gh's output on a failed call is a GraphQL answer with data.
fn answered(out: &str) -> bool {
    serde_json::from_str::<Value>(out.trim())
        .ok()
        .is_some_and(|v| v.get("data").is_some_and(Value::is_object))
}

/// The issue of this project a comment's link `url` names, the comment
/// `n`: `NotFound` for a comment on a pull request, `Unread(Inconsistent)`
/// for one of another project or another comment.
fn issue_of(repo: &str, url: &str, n: u64) -> Got<BlueprintNo> {
    let inconsistent = Outcome::Unread(Unread::Inconsistent);
    let head = url
        .strip_suffix(&format!("#issuecomment-{n}"))
        .ok_or(inconsistent.clone())?;
    let (at, no) = head.rsplit_once('/').ok_or(inconsistent.clone())?;
    let no = no
        .parse()
        .ok()
        .and_then(BlueprintNo::new)
        .ok_or(inconsistent.clone())?;
    if ends_with_folded(at, &format!("/{repo}/pull")) {
        return Err(Outcome::NotFound);
    }
    if !ends_with_folded(at, &format!("/{repo}/issues")) {
        return Err(inconsistent);
    }
    Ok(no)
}

/// The items in the order first asked, each once.
fn once<T: Eq + std::hash::Hash + Clone>(items: impl Iterator<Item = T>) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items.filter(|i| seen.insert(i.clone())).collect()
}
