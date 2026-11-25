//! The words the site holds: the repository's labels, each with its node
//! id, as `Tickets::marks` answers them, and the owner's issue types, as
//! `Tickets::kinds` answers them. Each is a GraphQL question paged here to
//! its end, or one past the bound. The labels are read with their colours and
//! texts too, one or all, for `TicketWrites::prepare_one` and `prepare`.

use serde::Deserialize;

use crate::ticket::{Page, Pages};
use crate::GitHub;
use tehas_ports::{Bound, Got, Kind, MarkHeld, MarkId, MarkSpec, Outcome, Unread, Whole};

const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);
const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);
const CUT: Outcome = Outcome::Unread(Unread::Cut);

/// Every label of the repository with its node id, the id `state/labels`
/// pins, a page of them in name order.
const LABELS: &str = "query($owner: String!, $name: String!, $cursor: String) { repository(owner: $owner, name: $name) { labels(first: 100, after: $cursor, orderBy: {field: NAME, direction: ASC}) { pageInfo { hasNextPage endCursor } nodes { id name } } } }";

/// One label of the repository by its name, with its colour and text.
const LABEL: &str = "query($owner: String!, $name: String!, $label: String!) { repository(owner: $owner, name: $name) { label(name: $label) { name color description } } }";

/// Every label of the repository with its colour and text, a page of them.
const LABELS_HELD: &str = "query($owner: String!, $name: String!, $cursor: String) { repository(owner: $owner, name: $name) { labels(first: 100, after: $cursor, orderBy: {field: NAME, direction: ASC}) { pageInfo { hasNextPage endCursor } nodes { name color description } } } }";

/// The issue types of the repository's owner, a page of them.
const ISSUE_TYPES: &str = "query($owner: String!, $name: String!, $cursor: String) { repository(owner: $owner, name: $name) { issueTypes(first: 100, after: $cursor) { pageInfo { hasNextPage endCursor } nodes { name } } } }";

#[derive(Deserialize)]
struct Label {
    id: String,
    name: String,
}

/// A label as the repository holds it.
#[derive(Deserialize)]
pub(crate) struct LabelHeld {
    name: String,
    color: String,
    description: Option<String>,
}

impl LabelHeld {
    /// The label's name as GitHub holds it.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Whether the label is what `spec` makes: its name exactly, and the
    /// colour and the text the spec gives. A colour is read in any case.
    pub(crate) fn is(&self, spec: &MarkSpec) -> bool {
        self.name == spec.name.as_str()
            && spec
                .colour
                .as_ref()
                .is_none_or(|c| c.as_str().eq_ignore_ascii_case(&self.color))
            && spec
                .about
                .as_ref()
                .is_none_or(|a| self.description.as_deref().unwrap_or_default() == a)
    }
}

#[derive(Deserialize)]
struct LabelRepo {
    #[serde(deserialize_with = "crate::ticket::present")]
    label: Option<LabelHeld>,
}

#[derive(Deserialize)]
struct LabelsRepo {
    labels: Page<LabelHeld>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct RepoAnswer<T> {
    #[serde(deserialize_with = "crate::ticket::present")]
    data: Option<RepoData<T>>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct RepoData<T> {
    #[serde(deserialize_with = "crate::ticket::present")]
    repository: Option<T>,
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
            let page: Page<Label> =
                crate::graphql::part(&out, "/data/repository/labels", (self.now)())?;
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

    /// The label `name` as the repository holds it, None where it holds
    /// none. GitHub finds a label by its name in any case, so the name it
    /// answers is kept for [`LabelHeld::is`]. A failure is read as gh's
    /// answer beside it says (`question_outcome`); a repository that is
    /// gone is `NotFound`, and an answer with errors in it is
    /// `Unread(Inconsistent)`.
    pub(crate) fn label_held(&mut self, name: &str) -> Got<Option<LabelHeld>> {
        let label = format!("label={name}");
        let repository: LabelRepo = self.ask_repository("the label", LABEL, &[&label])?;
        let held = repository.label;
        if held
            .as_ref()
            .is_some_and(|h| !h.name.eq_ignore_ascii_case(name))
        {
            return Err(INCONSISTENT);
        }
        Ok(held)
    }

    /// Every label of the repository with its colour and text, in name
    /// order, as [`GitHub::label_held`] reads one. More than `ALL_MAX` is
    /// `Unread(Cut)`.
    pub(crate) fn labels_held(&mut self) -> Got<Vec<LabelHeld>> {
        let mut all = Vec::new();
        let mut pages = Pages::default();
        let mut cursor: Option<String> = None;
        loop {
            let from = cursor.as_ref().map(|c| format!("cursor={c}"));
            let fields: Vec<&str> = from.iter().map(String::as_str).collect();
            let repository: LabelsRepo = self.ask_repository("the labels", LABELS_HELD, &fields)?;
            for l in repository.labels.nodes {
                all.push(l.ok_or(INCONSISTENT)?);
            }
            if all.len() > tehas_ports::ALL_MAX.get() as usize {
                return Err(CUT);
            }
            match pages.next(repository.labels.page_info)? {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(all)
    }

    /// The repository's part of the answer to `question`, asked with the
    /// `-f` fields given. A failure is read as gh's answer beside it says
    /// (`question_outcome`); a repository that is gone is `NotFound`, and
    /// an answer with errors in it is `Unread(Inconsistent)`.
    fn ask_repository<T: serde::de::DeserializeOwned>(
        &mut self,
        call: &str,
        question: &str,
        fields: &[&str],
    ) -> Got<T> {
        let (owner, repo) = self.owner_name();
        let query = format!("query={question}");
        let mut args = vec!["api", "graphql", "-F", &owner, "-F", &repo];
        for f in fields {
            args.extend(["-f", f]);
        }
        args.extend(["-f", &query]);
        let out = match self.call_answer(call, &args) {
            Ok(out) => out,
            Err((e, said)) => {
                let path = format!("repos/{}", self.repo);
                return Err(self.question_outcome(&e, said.as_deref(), &path));
            }
        };
        if let Some(refused) = crate::graphql::refusal(&out, (self.now)()) {
            return Err(refused);
        }
        let answer: RepoAnswer<T> = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
        answer
            .data
            .ok_or(MALFORMED)?
            .repository
            .ok_or(Outcome::NotFound)
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
            let types: Types = crate::graphql::part(&out, "/data/repository", (self.now)())?;
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
