//! The code host's tags and releases: `tags`, `recent_tags`, `resolve_tag`,
//! `releases`, `recent_releases`, `latest_release` and `release`.
//!
//! Tags are GraphQL's `refs/tags/`, a page of 100 at a time. An annotated
//! tag is peeled to its commit, at most `PEEL_MAX` tags deep, and its time is
//! its tagger's date, never its commit's; a lightweight tag has none
//! (Decided 128). A tag that names no commit, such as a tree, cannot be a
//! `Tag` and fails the read `Unread(Malformed)`: a whole list never leaves
//! one out.
//!
//! Releases are REST's list, a page of 100 at a time, drafts among them where
//! the token may see them. Their notes are read without our markers.

use std::num::NonZeroU32;

use serde::Deserialize;

use crate::forge::uri;
use crate::{markers, GitHub};
use tehas_ports::{
    At, Bound, CommitId, FileName, Got, Outcome, Release, ReleaseFile, ReleaseId, Tag, TagName,
    Took, Unread, WebLink, Whole, ALL_MAX,
};

/// How deep a tag of a tag is peeled.
const PEEL_MAX: usize = 3;

/// The most pages of either list read before the read is `Unread(Cut)`.
const PAGES_MAX: u32 = 50;

/// Items a page.
const PER_PAGE: usize = 100;

/// What a tag's ref targets: a commit, or a tag object and what it names.
const TARGET: &str =
    "__typename oid ... on Tag { tagger { date } target { __typename oid ... on Tag { target { __typename oid ... on Tag { target { __typename oid } } } } } }";

/// A page of the tags.
const TAGS: &str = "query($owner: String!, $name: String!, $after: String) { repository(owner: $owner, name: $name) { refs(refPrefix: \"refs/tags/\", first: 100, after: $after) { totalCount pageInfo { hasNextPage endCursor } nodes { name target { {TARGET} } } } } }";

/// One tag, by its whole ref.
const ONE_TAG: &str = "query($owner: String!, $name: String!, $ref: String!) { repository(owner: $owner, name: $name) { ref(qualifiedName: $ref) { name target { {TARGET} } } } }";

/// An answer that does not parse.
const MALFORMED: Outcome = Outcome::Unread(Unread::Malformed);

impl GitHub {
    /// Every tag, whole up to `bound`: past it, `Unread(Cut)`, known from
    /// the first page's count. The pages must hold as many tags as the count,
    /// each once.
    pub(super) fn tags_of(&mut self, bound: Bound) -> Got<Whole<Tag>> {
        let query = format!("query={}", TAGS.replace("{TARGET}", TARGET));
        let (owner, name) = self.owner_name();
        let mut tags: Vec<Tag> = Vec::new();
        let mut after: Option<String> = None;
        let mut total = None;
        for _ in 0..PAGES_MAX {
            let from = after.as_ref().map(|c| format!("after={c}"));
            let mut args = vec!["api", "graphql", "-f", &owner, "-f", &name];
            if let Some(from) = &from {
                args.extend(["-f", from.as_str()]);
            }
            args.extend(["-f", &query]);
            let out = match self.call("the tags", &args) {
                Ok(out) => out,
                Err(e) => return Err(self.query_outcome(&e)),
            };
            let page = read_tags(&out)?;
            let count = *total.get_or_insert(page.total);
            if count != page.total {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            if count > u64::from(bound.get()) {
                return Err(Outcome::Unread(Unread::Cut));
            }
            for tag in page.tags {
                if tags.iter().any(|t| t.name == tag.name) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                tags.push(tag);
            }
            match page.next {
                Some(c) if Some(&c) != after.as_ref() => after = Some(c),
                Some(_) => return Err(Outcome::Unread(Unread::Inconsistent)),
                None if tags.len() as u64 == count => return Ok(Whole::new(tags)),
                None => return Err(Outcome::Unread(Unread::Inconsistent)),
            }
        }
        Err(Outcome::Unread(Unread::Cut))
    }

    /// The newest `first` tags: every tag read whole up to `ALL_MAX`, then
    /// newest made first; ties and tags with no time keep the site's order,
    /// the timeless after the rest. `more` when there are more than `first`.
    pub(super) fn recent_tags_of(&mut self, first: NonZeroU32) -> Got<Took<Tag>> {
        let mut tags = self.tags_of(ALL_MAX)?.into_items();
        tags.sort_by_key(|t| std::cmp::Reverse(t.at));
        Ok(took(tags, first))
    }

    /// The commit the tag names, peeled: one GraphQL question. `NotFound`
    /// when the repository holds no such tag.
    pub(super) fn tag_commit(&mut self, tag: &TagName) -> Got<CommitId> {
        let query = format!("query={}", ONE_TAG.replace("{TARGET}", TARGET));
        let (owner, name) = self.owner_name();
        let at = format!("ref=refs/tags/{}", tag.as_str());
        let args = [
            "api", "graphql", "-f", &owner, "-f", &name, "-f", &at, "-f", &query,
        ];
        let out = match self.call("the tag", &args) {
            Ok(out) => out,
            Err(e) => return Err(self.query_outcome(&e)),
        };
        let v: OneAnswer = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
        let node = v
            .data
            .and_then(|d| d.repository)
            .ok_or(MALFORMED)?
            .r#ref
            .ok_or(Outcome::NotFound)?;
        let read = node.tag()?;
        if read.name != *tag {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        Ok(read.commit)
    }

    /// Every release, whole up to `bound`, in the site's order (newest made
    /// first). Past the bound, `Unread(Cut)`.
    pub(super) fn releases_of(&mut self, bound: Bound) -> Got<Whole<Release>> {
        let all = self.releases_read(bound)?;
        Ok(Whole::new(all.into_iter().map(|r| r.release).collect()))
    }

    /// The newest `first` releases, drafts among them: every release read
    /// whole up to `ALL_MAX`, newest made first by GitHub's `created_at`.
    pub(super) fn recent_releases_of(&mut self, first: NonZeroU32) -> Got<Took<Release>> {
        let mut all = self.releases_read(ALL_MAX)?;
        all.sort_by_key(|r| std::cmp::Reverse(r.created));
        Ok(took(all.into_iter().map(|r| r.release).collect(), first))
    }

    /// GitHub's own latest release, REST's `releases/latest`, as the
    /// anonymous reader asks it: the release a person marked latest, else
    /// the one the site picks among the published that are neither a draft
    /// nor a prerelease. When it answers with none, the project is read: one
    /// that is there has no latest release (None); one that is not is that
    /// read's own outcome. A draft or a prerelease in the answer is
    /// `Unread(Inconsistent)`.
    pub(super) fn latest_release_of(&mut self) -> Got<Option<Release>> {
        let path = format!("repos/{}/releases/latest", self.repo);
        let out = match self.call("the latest release", &["api", &path]) {
            Ok(out) => out,
            Err(e) => {
                return match self.outcome(&e) {
                    Outcome::NotFound => {
                        let project = format!("repos/{}", self.repo);
                        match self.call("the project", &["api", &project]) {
                            Ok(_) => Ok(None),
                            Err(e) => Err(self.outcome(&e)),
                        }
                    }
                    other => Err(other),
                };
            }
        };
        let raw: Raw = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
        let release = raw.read()?.release;
        if release.draft || release.prerelease {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        Ok(Some(release))
    }

    /// The release of a tag, REST's `releases/tags/<tag>`. `NotFound` when
    /// the tag has none; an answer of another tag is `Unread(Inconsistent)`.
    pub(super) fn release_of(&mut self, tag: &TagName) -> Got<Release> {
        let path = format!("repos/{}/releases/tags/{}", self.repo, uri(tag.as_str()));
        let out = self
            .call("the release", &["api", &path])
            .map_err(|e| self.outcome(&e))?;
        let raw: Raw = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
        let read = raw.read()?;
        if read.release.tag != *tag {
            return Err(Outcome::Unread(Unread::Inconsistent));
        }
        Ok(read.release)
    }

    /// Every release with the time it was made, a page of 100 at a time
    /// until a page is short. Past `bound`, or `PAGES_MAX` pages,
    /// `Unread(Cut)`; a release met twice is `Unread(Inconsistent)`.
    fn releases_read(&mut self, bound: Bound) -> Got<Vec<Read>> {
        let mut all: Vec<Read> = Vec::new();
        for page in 1..=PAGES_MAX {
            let path = format!(
                "repos/{}/releases?per_page={PER_PAGE}&page={page}",
                self.repo
            );
            let out = self
                .call("the releases", &["api", &path])
                .map_err(|e| self.outcome(&e))?;
            let items: Vec<Raw> = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
            let full = items.len() == PER_PAGE;
            for raw in items {
                let read = raw.read()?;
                if all.iter().any(|r| r.release.id == read.release.id) {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                all.push(read);
                if all.len() > bound.get() as usize {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
            if !full {
                return Ok(all);
            }
        }
        Err(Outcome::Unread(Unread::Cut))
    }
}

/// The first `n` of `all`, and whether there were more.
fn took<T>(mut all: Vec<T>, n: NonZeroU32) -> Took<T> {
    let more = all.len() > n.get() as usize;
    all.truncate(n.get() as usize);
    let mut t = Took::default();
    t.items = all;
    t.more = more;
    t
}

/// A page of the tags read.
struct TagsPage {
    total: u64,
    next: Option<String>,
    tags: Vec<Tag>,
}

fn read_tags(out: &str) -> Got<TagsPage> {
    let v: TagsAnswer = serde_json::from_str(out.trim()).map_err(|_| MALFORMED)?;
    let refs = v
        .data
        .and_then(|d| d.repository)
        .and_then(|r| r.refs)
        .ok_or(MALFORMED)?;
    let tags = refs
        .nodes
        .into_iter()
        .map(|n| n.ok_or(MALFORMED).and_then(|n| n.tag()))
        .collect::<Got<Vec<Tag>>>()?;
    let next = match (refs.page_info.has_next_page, refs.page_info.end_cursor) {
        (true, Some(c)) => Some(c),
        (true, None) => return Err(MALFORMED),
        (false, _) => None,
    };
    Ok(TagsPage {
        total: refs.total_count,
        next,
        tags,
    })
}

#[derive(Deserialize)]
struct TagsAnswer {
    data: Option<TagsData>,
}

#[derive(Deserialize)]
struct TagsData {
    repository: Option<TagsRepo>,
}

#[derive(Deserialize)]
struct TagsRepo {
    refs: Option<Refs>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Refs {
    total_count: u64,
    page_info: PageInfo,
    nodes: Vec<Option<RefNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
struct OneAnswer {
    data: Option<OneData>,
}

#[derive(Deserialize)]
struct OneData {
    repository: Option<OneRepo>,
}

#[derive(Deserialize)]
struct OneRepo {
    r#ref: Option<RefNode>,
}

#[derive(Deserialize)]
struct RefNode {
    name: String,
    target: Option<Target>,
}

#[derive(Deserialize)]
struct Target {
    #[serde(rename = "__typename")]
    kind: String,
    oid: String,
    tagger: Option<Tagger>,
    target: Option<Box<Target>>,
}

#[derive(Deserialize)]
struct Tagger {
    date: Option<String>,
}

impl RefNode {
    /// The tag this ref is: a commit's name, or a tag object's, peeled.
    fn tag(self) -> Got<Tag> {
        let name = TagName::new(&self.name).map_err(|_| MALFORMED)?;
        let target = self.target.ok_or(MALFORMED)?;
        let annotated = target.kind == "Tag";
        let at = match &target.tagger {
            Some(Tagger { date: Some(d) }) if annotated => Some(time(d)?),
            _ => None,
        };
        let commit = peel(&target, 0)?;
        let mut tag = Tag::new(name, commit);
        tag.at = at;
        tag.annotated = annotated;
        Ok(tag)
    }
}

/// The commit a ref's target names, through at most `PEEL_MAX` tags.
fn peel(target: &Target, depth: usize) -> Got<CommitId> {
    match target.kind.as_str() {
        "Commit" => CommitId::new(&target.oid).map_err(|_| MALFORMED),
        "Tag" if depth < PEEL_MAX => peel(target.target.as_deref().ok_or(MALFORMED)?, depth + 1),
        _ => Err(MALFORMED),
    }
}

fn time(iso: &str) -> Got<At> {
    tehas_core::time::from_iso(iso)
        .map(At::new)
        .ok_or(MALFORMED)
}

/// A release as REST gives it, with the time it was made.
struct Read {
    release: Release,
    created: Option<At>,
}

#[derive(Deserialize)]
struct Raw {
    id: Option<u64>,
    tag_name: Option<String>,
    name: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
    created_at: Option<String>,
    published_at: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    assets: Option<Vec<Asset>>,
}

#[derive(Deserialize)]
struct Asset {
    name: Option<String>,
    size: Option<u64>,
}

impl Raw {
    fn read(self) -> Got<Read> {
        let id = self.id.filter(|i| *i > 0).ok_or(MALFORMED)?;
        let tag =
            TagName::new(self.tag_name.as_deref().ok_or(MALFORMED)?).map_err(|_| MALFORMED)?;
        let link =
            WebLink::new(self.html_url.as_deref().ok_or(MALFORMED)?).map_err(|_| MALFORMED)?;
        let mut release = Release::new(ReleaseId::new(id.to_string()), tag, link);
        release.title = self.name.unwrap_or_default();
        release.draft = self.draft.ok_or(MALFORMED)?;
        release.prerelease = self.prerelease.ok_or(MALFORMED)?;
        release.published = self.published_at.as_deref().map(time).transpose()?;
        if release.draft != release.published.is_none() {
            return Err(MALFORMED);
        }
        release.notes = markers::read(self.body.as_deref().unwrap_or(""), None).text;
        release.files = self
            .assets
            .ok_or(MALFORMED)?
            .into_iter()
            .map(|a| {
                let name =
                    FileName::new(a.name.as_deref().ok_or(MALFORMED)?).map_err(|_| MALFORMED)?;
                Ok(ReleaseFile::new(name, a.size.ok_or(MALFORMED)?))
            })
            .collect::<Got<Vec<_>>>()?;
        let created = self.created_at.as_deref().map(time).transpose()?;
        Ok(Read { release, created })
    }
}
