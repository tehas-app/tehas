//! The code host handle of `Actor::Anonymous` (Decided 76): a public
//! project's releases and tags, read through [`Curl`] with no credential
//! and never through gh. It answers `releases`, `latest_release`, `tags`,
//! `resolve_tag` and `download`; every other read is
//! `Refused(Unsupported)` before any request, and a write handle is never
//! made (`site.rs`).
//!
//! GitHub answers 404 alike for a project that is not there and for one
//! that is private, so either is `Refused(Unsupported)`, never `NotFound`.
//! Where a 404 can also mean "none" (no release yet, no such tag), the
//! project is asked once more: if it answers, the 404 was of the project's
//! own content.
//!
//! Its calls spend the anonymous budget, one holder for every anonymous
//! caller, `Holder::Anonymous`, in the `Calls` pool: a spent budget is kept
//! in the ledger, and a call while it is spent answers `Spent` with no
//! request. The factory's budget is never asked.
//!
//! `download` fetches one public release file: the release of the tag is
//! read first, and a file it does not list is `NotFound`, one it lists as
//! larger than the limit `Unread(TooLarge)`, both before any fetch. The
//! file is asked of the API by its id, and at most [`REDIRECTS`] redirects
//! are followed, each to one of `curl::HOSTS` exactly over `https`; any
//! other is `Refused(Unsupported)` with nothing written. The bytes go to a
//! file in a directory of the adapter's own, held to the limit on the
//! stream (no more than `limit` bytes ever reach the temporary directory)
//! and to the listed size after, then streamed into the caller's writer, so
//! `limit` bounds the disk a download takes, not its memory;
//! the directory is removed whatever happens. The whole call is bound by
//! `curl::DOWNLOAD_LIMIT` and the deadline. `archive` stays
//! `Refused(Unsupported)`.
//!
//! Only the API's answers touch the ledger: a redirect's or a file's
//! answer from another host neither spends nor heals the anonymous
//! budget. A 2xx from the API that ignored `Accept` and sent the asset's
//! JSON in place of its bytes fails on its size as `Unread(Inconsistent)`,
//! on purpose: it is never parsed.

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::code_host::download::Scratch;
use super::{site::count, unsupported};
use crate::curl::{self, Answer, Curl, API, DOWNLOAD_LIMIT};
use crate::markers;
use tehas_ports::{
    Access, At, Bound, Branch, Caller, Change, ChangeDelta, ChangeDigest, ChangeEvent,
    ChangeFilter, ChangeNo, ChangeToken, ChangedFile, Checks, Cite, Cites, CodeHost, CommitId,
    Compared, Cursor, Did, Facts, Failure, FileName, Got, HealthState, Holder, HolderId, HostId,
    Ledger, MeterRow, Outcome, Passing, Pool, PortName, Refusal, Release, ReleaseFile, ReleaseId,
    Remote, RepoPath, Report, Reports, Rev, Sent, SiteId, Tag, TagName, Took, Unread, WebLink,
    Whole, Whose, Window,
};

/// The host the ledger keeps GitHub's health and budgets under.
const HOST: &str = "github.com";

/// How many seconds of failures the ledger is asked to keep.
const FAILURES_KEPT: u32 = 600;

/// The most tags a tag object is peeled through.
const PEELS: usize = 4;

/// The most redirects a download follows.
pub(crate) const REDIRECTS: usize = 2;

/// What a site needs to make anonymous handles: curl and the ledger.
#[derive(Clone)]
pub(super) struct Public {
    pub(super) curl: Curl,
    pub(super) ledger: Arc<dyn Ledger>,
}

/// One anonymous code host handle of a repository.
pub(super) struct Anonymous {
    repo: String,
    curl: Curl,
    ledger: Arc<dyn Ledger>,
    meter: Arc<Mutex<Vec<MeterRow>>>,
    caller: Caller,
    deadline: Option<At>,
    now: fn() -> i64,
}

impl Anonymous {
    pub(super) fn new(
        repo: &str,
        public: &Public,
        meter: Arc<Mutex<Vec<MeterRow>>>,
        caller: Caller,
        deadline: Option<At>,
        now: fn() -> i64,
    ) -> Anonymous {
        Anonymous {
            repo: repo.to_string(),
            curl: public.curl.clone(),
            ledger: Arc::clone(&public.ledger),
            meter,
            caller,
            deadline,
            now,
        }
    }

    fn holder() -> Holder {
        Holder::Anonymous(HolderId::new("anonymous"))
    }

    /// One GET of `path`, the answer None for a 404. At or past the
    /// deadline, or while the ledger holds the budget spent, no request is
    /// made. A redirect within the API's host is followed once.
    fn get(&mut self, path: &str) -> Got<Option<Answer>> {
        let mut answer = self.once(path)?;
        if let Some(to) = answer
            .as_ref()
            .filter(|a| matches!(a.status, 301 | 302 | 307 | 308))
            .and_then(|a| a.header("location"))
        {
            let to = curl::api_path(to).ok_or(Outcome::Unread(Unread::Malformed))?;
            answer = self.once(to)?;
        }
        Ok(answer)
    }

    fn once(&mut self, path: &str) -> Got<Option<Answer>> {
        let curl = self.curl.clone();
        self.ask(Duration::MAX, true, |left| curl.get(path, left))
    }

    /// One request by `run`, given the time left of `bound` and the
    /// deadline. At or past the deadline `run` is not called. The call is
    /// metered. An `api` call, one to the API's host, draws on the anonymous
    /// budget: while the ledger holds it spent `run` is not called, and its
    /// answer is given to the ledger. Any other host's answer (a download's
    /// redirect or file) never touches the ledger: its rate headers and
    /// refusals are not the API's, so they neither spend nor heal its
    /// budget, and are read by [`read_other`].
    fn ask(
        &mut self,
        bound: Duration,
        api: bool,
        run: impl FnOnce(Duration) -> Result<Answer, Outcome>,
    ) -> Got<Option<Answer>> {
        let now = (self.now)();
        let left = match self.deadline {
            Some(d) if now >= d.secs() => {
                return Err(Outcome::Unavailable {
                    why: Passing::Timeout,
                    sent: Sent::No,
                })
            }
            Some(d) => Duration::from_secs((d.secs() - now) as u64),
            None => Duration::MAX,
        };
        if !api {
            count(&self.meter, self.caller, PortName::CodeHost, Pool::Calls);
            return run(left.min(bound)).and_then(read_other);
        }
        if let Some(spent) = self.spent(now) {
            return Err(spent);
        }
        count(&self.meter, self.caller, PortName::CodeHost, Pool::Calls);
        let answer = run(left.min(bound)).and_then(|a| read_status(a, now));
        self.observe(now, &answer);
        answer
    }

    /// `Spent` when the ledger holds the anonymous budget out at `now`.
    fn spent(&self, now: i64) -> Option<Outcome> {
        let host = HostId::new(HOST);
        let whose = Whose::Pool(Self::holder(), Pool::Calls);
        match self.ledger.health(&host, &whose) {
            Ok(HealthState::Spent {
                pool: Pool::Calls,
                until,
            }) if now < until.secs() => Some(Outcome::Spent {
                pool: Pool::Calls,
                until,
            }),
            _ => None,
        }
    }

    /// Give an answer to the ledger: its budget, each state `tehas_ports::keys`
    /// names, and a failure of the supplier. A ledger that fails is passed
    /// by: the answer still goes to the caller.
    fn observe(&self, now: i64, answer: &Got<Option<Answer>>) {
        let host = HostId::new(HOST);
        let holder = Self::holder();
        let at = At::new(now);
        if let Ok(Some(a)) = answer {
            if let (Some(left), Some(resets)) = (
                a.header("x-ratelimit-remaining")
                    .and_then(|v| v.parse().ok()),
                a.header("x-ratelimit-reset").and_then(|v| v.parse().ok()),
            ) {
                let budget = tehas_ports::Budget::new(left, At::new(resets));
                let _ = self.ledger.put_budget(&host, &holder, Pool::Calls, budget);
            }
        }
        let seen = answer.as_ref().map(drop).map_err(Clone::clone);
        for whose in tehas_ports::keys(&holder, &seen, &[Pool::Calls]) {
            let Ok(was) = self.ledger.health(&host, &whose) else {
                continue;
            };
            let realm = tehas_ports::Realm::CodeHost;
            let (new, _) = tehas_ports::next(&host, &whose, realm, at, &was, &seen);
            if new != was {
                let _ = self.ledger.swap_health(&host, &whose, &was, &new);
            }
        }
        if let Err(o) = &seen {
            let kept = match o {
                Outcome::Unavailable { why, .. } => *why != Passing::Broken,
                Outcome::Spent { .. } => true,
                _ => false,
            };
            if kept {
                let f = Failure::new(at, SiteId::new(self.repo.clone()), o.clone());
                let _ = self.ledger.failed(&host, f, FAILURES_KEPT);
            }
        }
    }

    /// Every page of a list, from `first` on by each page's next link, or
    /// `Unread(Cut)` past `bound`. A 404 is the project's.
    fn list<T: DeserializeOwned>(&mut self, first: String, bound: Bound) -> Got<Vec<T>> {
        let mut path = first;
        let mut all = Vec::new();
        loop {
            let Some(page) = self.get(&path)? else {
                return refused();
            };
            let items: Vec<T> = json(&page)?;
            let empty = items.is_empty();
            all.extend(items);
            if all.len() > bound.get() as usize {
                return Err(Outcome::Unread(Unread::Cut));
            }
            match next_page(&page)? {
                Some(next) if !empty => path = next,
                Some(_) => return Err(Outcome::Unread(Unread::Inconsistent)),
                None => return Ok(all),
            }
        }
    }

    /// After a 404 that may be of the project's content: the content's
    /// `missing` if the project answers, else the project's refusal.
    fn missing<T>(&mut self, missing: Outcome) -> Got<T> {
        match self.get(&format!("repos/{}", self.repo))? {
            Some(_) => Err(missing),
            None => refused(),
        }
    }

    fn repo_path(&self) -> Got<&str> {
        super::code_host::parts(&self.repo)
            .map(|_| self.repo.as_str())
            .ok_or(Outcome::Refused(Refusal::Invalid))
    }
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 307 | 308)
}

/// A tag's name as a path, each segment escaped.
fn segments(tag: &TagName) -> String {
    tag.as_str()
        .split('/')
        .map(curl::escape)
        .collect::<Vec<_>>()
        .join("/")
}

/// The fetched file, `size` bytes, streamed into `into`: it is never read
/// whole, so the memory a download takes does not grow with `limit`. Its
/// size was held to the listed one before, so a short file here is
/// `Unread(Inconsistent)`; one that cannot be read, or a writer that fails,
/// `Unread(Malformed)`, as the ports' fake answers a writer that fails.
fn copy(path: &std::path::Path, size: u64, into: &mut dyn std::io::Write) -> Did {
    use std::io::Read;
    let malformed = Outcome::Unread(Unread::Malformed);
    let f = std::fs::File::open(path).map_err(|_| malformed.clone())?;
    if f.metadata().map_err(|_| malformed.clone())?.len() != size {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    let copied = std::io::copy(&mut f.take(size), into).map_err(|_| malformed)?;
    if copied != size {
        return Err(Outcome::Unread(Unread::Inconsistent));
    }
    Ok(())
}

/// A private or missing project, alike.
fn refused<T>() -> Got<T> {
    Err(Outcome::Refused(Refusal::Unsupported))
}

/// An answer by its status: a body to read, None for a 404, or the outcome.
fn read_status(a: Answer, now: i64) -> Got<Option<Answer>> {
    match a.status {
        200..=299 | 301 | 302 | 307 | 308 => Ok(Some(a)),
        404 => Ok(None),
        403 | 429 => {
            let reset = a
                .header("x-ratelimit-reset")
                .and_then(|v| v.parse::<i64>().ok());
            let after = a.header("retry-after").and_then(|v| v.parse::<i64>().ok());
            let out = a.header("x-ratelimit-remaining") == Some("0");
            match (out, reset, after) {
                (true, Some(reset), _) => Err(spent(reset)),
                (_, _, Some(after)) => Err(spent(now + after)),
                _ if a.status == 429 => Err(spent(now + 60)),
                _ => Err(Outcome::Forbidden),
            }
        }
        401 => Err(Outcome::Forbidden),
        500..=599 => Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        }),
        _ => Err(Outcome::Unread(Unread::Malformed)),
    }
}

/// An answer of a host other than the API's, by its status: a body or a
/// redirect to read, None for a 404, else the outcome. Its rate headers are
/// not the API's budget: a 429 or a `retry-after` is the site's failure for
/// this call alone, and a 401 or 403 (a storage refusal, an expired
/// signature) is `Forbidden`.
fn read_other(a: Answer) -> Got<Option<Answer>> {
    match a.status {
        200..=299 | 301 | 302 | 307 | 308 => Ok(Some(a)),
        404 => Ok(None),
        401 | 403 if a.header("retry-after").is_none() => Err(Outcome::Forbidden),
        401 | 403 | 429 | 500..=599 => Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        }),
        _ => Err(Outcome::Unread(Unread::Malformed)),
    }
}

fn spent(until: i64) -> Outcome {
    Outcome::Spent {
        pool: Pool::Calls,
        until: At::new(until),
    }
}

fn json<T: DeserializeOwned>(a: &Answer) -> Got<T> {
    serde_json::from_slice(&a.body).map_err(|_| Outcome::Unread(Unread::Malformed))
}

/// The path of the page after this one, from its `link` header.
fn next_page(a: &Answer) -> Got<Option<String>> {
    let Some(link) = a.header("link") else {
        return Ok(None);
    };
    let Some(next) = link.split(',').find(|p| p.contains("rel=\"next\"")) else {
        return Ok(None);
    };
    let address = next
        .split_once('<')
        .and_then(|(_, rest)| rest.split_once('>'))
        .map(|(address, _)| address)
        .ok_or(Outcome::Unread(Unread::Malformed))?;
    curl::api_path(address)
        .map(|p| Some(p.to_string()))
        .ok_or(Outcome::Unread(Unread::Malformed))
}

/// A release as GitHub's REST API gives it.
#[derive(Deserialize)]
struct RestRelease {
    id: u64,
    tag_name: String,
    name: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
    body: Option<String>,
    html_url: String,
    assets: Vec<RestAsset>,
}

#[derive(Deserialize)]
struct RestAsset {
    id: u64,
    name: String,
    size: u64,
}

impl RestRelease {
    fn release(self) -> Got<Release> {
        let malformed = |_| Outcome::Unread(Unread::Malformed);
        let tag = TagName::new(self.tag_name).map_err(malformed)?;
        let link = WebLink::new(self.html_url).map_err(malformed)?;
        let mut r = Release::new(ReleaseId::new(self.id.to_string()), tag, link);
        r.title = self.name.unwrap_or_default();
        r.draft = self.draft;
        r.prerelease = self.prerelease;
        r.published = match self.published_at {
            Some(t) => Some(At::new(
                tehas_core::time::from_iso(&t).ok_or(Outcome::Unread(Unread::Malformed))?,
            )),
            None => None,
        };
        r.notes = markers::read(&self.body.unwrap_or_default(), None).text;
        r.files = self
            .assets
            .into_iter()
            .map(|a| {
                Ok(ReleaseFile::new(
                    FileName::new(a.name).map_err(malformed)?,
                    a.size,
                ))
            })
            .collect::<Got<_>>()?;
        Ok(r)
    }
}

/// A tag of the REST list: its commit is peeled.
#[derive(Deserialize)]
struct RestTag {
    name: String,
    commit: RestObject,
}

/// A ref, or what a tag object points at.
#[derive(Deserialize)]
struct RestRef {
    #[serde(default)]
    r#ref: String,
    object: RestObject,
}

#[derive(Deserialize)]
struct RestObject {
    sha: String,
    #[serde(default, rename = "type")]
    kind: String,
}

fn commit(sha: String) -> Got<CommitId> {
    CommitId::new(sha).map_err(|_| Outcome::Unread(Unread::Malformed))
}

impl CodeHost for Anonymous {
    fn facts(&mut self) -> Got<Facts> {
        unsupported()
    }

    fn branch(&mut self, _: &Branch) -> Got<Option<CommitId>> {
        unsupported()
    }

    fn compare(&mut self, _: &CommitId, _: &CommitId) -> Got<Compared> {
        unsupported()
    }

    fn file(&mut self, _: &RepoPath, _: &Rev, _: u64) -> Got<Vec<u8>> {
        unsupported()
    }

    fn changes(&mut self, _: &ChangeFilter, _: Bound) -> Got<Whole<ChangeDigest>> {
        unsupported()
    }

    fn change(&mut self, _: ChangeNo, _: Bound) -> Got<Change> {
        unsupported()
    }

    fn changed_files(&mut self, _: ChangeNo, _: Bound) -> Got<Whole<ChangedFile>> {
        unsupported()
    }

    fn activity(&mut self, _: ChangeNo, _: Window) -> Got<Took<ChangeEvent>> {
        unsupported()
    }

    fn checks(&mut self, _: &CommitId, _: Option<&str>) -> Got<Checks> {
        unsupported()
    }

    /// The tags list for each tag's peeled commit, and the tag refs for
    /// which are annotated: two lists. The list gives no tag's time.
    fn tags(&mut self, bound: Bound) -> Got<Whole<Tag>> {
        let repo = self.repo_path()?.to_string();
        let listed: Vec<RestTag> = self.list(format!("repos/{repo}/tags?per_page=100"), bound)?;
        let refs: Vec<RestRef> = self.list(
            format!("repos/{repo}/git/matching-refs/tags/?per_page=100"),
            bound,
        )?;
        let tags = listed
            .into_iter()
            .map(|t| {
                let held = format!("refs/tags/{}", t.name);
                let r = refs
                    .iter()
                    .find(|r| r.r#ref == held)
                    .ok_or(Outcome::Unread(Unread::Inconsistent))?;
                let name = TagName::new(t.name).map_err(|_| Outcome::Unread(Unread::Malformed))?;
                let mut tag = Tag::new(name, commit(t.commit.sha)?);
                tag.annotated = r.object.kind == "tag";
                Ok(tag)
            })
            .collect::<Got<_>>()?;
        Ok(Whole::new(tags))
    }

    fn recent_tags(&mut self, _: NonZeroU32) -> Got<Took<Tag>> {
        unsupported()
    }

    /// The tag's ref, and an annotated tag's object peeled to its commit.
    fn resolve_tag(&mut self, name: &TagName) -> Got<CommitId> {
        let repo = self.repo_path()?.to_string();
        let path = format!("repos/{repo}/git/ref/tags/{}", segments(name));
        let Some(found) = self.get(&path)? else {
            return self.missing(Outcome::NotFound);
        };
        let mut object = json::<RestRef>(&found)?.object;
        for _ in 0..PEELS {
            match object.kind.as_str() {
                "commit" => return commit(object.sha),
                "tag" => {
                    let sha = commit(object.sha)?;
                    let path = format!("repos/{repo}/git/tags/{}", sha.as_str());
                    let peeled = self
                        .get(&path)?
                        .ok_or(Outcome::Unread(Unread::Inconsistent))?;
                    object = json::<RestRef>(&peeled)?.object;
                }
                _ => return Err(Outcome::Unread(Unread::Malformed)),
            }
        }
        Err(Outcome::Unread(Unread::Malformed))
    }

    /// Every page of the releases. An anonymous reader is shown no draft.
    fn releases(&mut self, bound: Bound) -> Got<Whole<Release>> {
        let repo = self.repo_path()?.to_string();
        let listed: Vec<RestRelease> =
            self.list(format!("repos/{repo}/releases?per_page=100"), bound)?;
        let releases = listed
            .into_iter()
            .map(RestRelease::release)
            .collect::<Got<_>>()?;
        Ok(Whole::new(releases))
    }

    fn recent_releases(&mut self, _: NonZeroU32) -> Got<Took<Release>> {
        unsupported()
    }

    /// GitHub's latest release; a project that answers with none has none.
    fn latest_release(&mut self) -> Got<Option<Release>> {
        let repo = self.repo_path()?.to_string();
        match self.get(&format!("repos/{repo}/releases/latest"))? {
            Some(a) => json::<RestRelease>(&a)?.release().map(Some),
            None => match self.get(&format!("repos/{repo}"))? {
                Some(_) => Ok(None),
                None => refused(),
            },
        }
    }

    fn release(&mut self, _: &TagName) -> Got<Release> {
        unsupported()
    }

    /// See the module: the release, then the file by its id.
    fn download(
        &mut self,
        tag: &TagName,
        file: &FileName,
        limit: u64,
        into: &mut dyn std::io::Write,
    ) -> Did {
        let started = Instant::now();
        let repo = self.repo_path()?.to_string();
        let Some(found) = self.get(&format!("repos/{repo}/releases/tags/{}", segments(tag)))?
        else {
            return self.missing(Outcome::NotFound);
        };
        let release: RestRelease = json(&found)?;
        let asset = release
            .assets
            .into_iter()
            .find(|a| a.name == file.as_str())
            .ok_or(Outcome::NotFound)?;
        if asset.size > limit {
            return Err(Outcome::Unread(Unread::TooLarge));
        }
        let scratch = Scratch::new().ok_or(Outcome::Unread(Unread::Unknown))?;
        let path = scratch.file();
        let mut address = format!("{API}repos/{repo}/releases/assets/{}", asset.id);
        for hop in 0..=REDIRECTS {
            let bound = DOWNLOAD_LIMIT.saturating_sub(started.elapsed());
            if bound.is_zero() {
                return Err(Outcome::Unavailable {
                    why: Passing::Timeout,
                    sent: Sent::Maybe,
                });
            }
            let curl = self.curl.clone();
            let api = curl::parse_address(&address).is_some_and(|p| p.host == curl::API_HOST);
            let answer = self
                .ask(bound, api, |left| curl.fetch(&address, left, limit, &path))?
                .ok_or(Outcome::NotFound)?;
            // A 2xx is the file's bytes. One from the API that ignored
            // `Accept` and sent the asset's JSON is held to the listed size
            // and fails as `Inconsistent`, on purpose: it is never parsed.
            if (200..300).contains(&answer.status) {
                if answer.written != asset.size {
                    return Err(Outcome::Unread(Unread::Inconsistent));
                }
                return copy(&path, asset.size, into);
            }
            if !is_redirect(answer.status) {
                return Err(Outcome::Unread(Unread::Malformed));
            }
            // A redirect with no place to go is not GitHub's answer.
            let to = answer
                .header("location")
                .filter(|to| !to.is_empty())
                .ok_or(Outcome::Unread(Unread::Malformed))?;
            // Every redirect is held to the hosts and https again, and
            // only two are followed.
            if hop == REDIRECTS || !curl::address_ok(to) {
                return Err(Outcome::Refused(Refusal::Unsupported));
            }
            address = to.to_string();
        }
        Err(Outcome::Refused(Refusal::Unsupported))
    }

    fn archive(&mut self, _: &CommitId, _: u64, _: &mut dyn std::io::Write) -> Did {
        unsupported()
    }

    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        unsupported()
    }

    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<ChangeDelta> {
        unsupported()
    }

    fn remote(&mut self, _: Access) -> Got<Remote> {
        unsupported()
    }
}

impl Reports for Anonymous {
    fn take_reports(&mut self) -> Vec<Report> {
        Vec::new()
    }
}

impl Cites for Anonymous {
    fn cite(&self, what: &Cite) -> String {
        super::cite(what)
    }

    fn link(&self, what: &Cite) -> Option<WebLink> {
        super::link(&self.repo, what)
    }
}
