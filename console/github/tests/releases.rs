//! The code host's tags and releases as the adapter reads them, answered
//! from recordings of the fixture project under `tests/recorded/releases/`,
//! one directory a case, replayed through a runner that writes down every
//! call; then the ports' law of the newest first over the same answers.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::{
    At, Bound, CodeHost, CommitId, FileName, Got, Outcome, Passing, Release, Sent, Tag, TagName,
    Unread,
};

const PROJECT: &str = "acme/widget";

type Asked = Arc<Mutex<Vec<String>>>;

/// Answers from `tests/recorded/releases/<case>/`: the first page of the
/// tags from `tags`, a later one from `tags-after-<cursor>`, one tag from
/// `ref-<tag>`, a page of releases from `releases-<page>`, one release from
/// `release-<tag>`; `.json` is gh's answer, `.fail` the runner's reason. A
/// call with no recording fails the test.
struct Replay {
    case: String,
    asked: Asked,
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.lock().unwrap().push(args.join(" "));
        let field = |name: &str| {
            args.iter()
                .find_map(|a| a.strip_prefix(&format!("{name}=")))
                .map(str::to_string)
        };
        let name = match args {
            ["api", "graphql", ..] => {
                assert_eq!(field("owner").as_deref(), Some("acme"));
                assert_eq!(field("name").as_deref(), Some("widget"));
                match (field("ref"), field("after")) {
                    (Some(r), _) => format!(
                        "ref-{}",
                        r.strip_prefix("refs/tags/").expect("a tag's whole ref")
                    ),
                    (None, Some(after)) => format!("tags-after-{after}"),
                    (None, None) => "tags".to_string(),
                }
            }
            ["api", path] => {
                let rest = path
                    .strip_prefix(&format!("repos/{PROJECT}/releases"))
                    .unwrap_or_else(|| panic!("no recording of {path}"));
                if let Some(tag) = rest.strip_prefix("/tags/") {
                    format!("release-{tag}")
                } else {
                    let page = rest
                        .strip_prefix("?per_page=100&page=")
                        .unwrap_or_else(|| panic!("no recording of {path}"));
                    format!("releases-{page}")
                }
            }
            _ => panic!("no recording of {args:?}"),
        };
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/recorded/releases")
            .join(&self.case);
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{name}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => panic!("no recording {}/{name}", self.case),
        }
    }
}

fn adapter(case: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        case: case.to_string(),
        asked: Arc::clone(&asked),
    };
    (GitHub::new(PROJECT, Box::new(replay)), asked)
}

fn bound(n: u32) -> Bound {
    Bound::new(n).unwrap()
}

fn nz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

fn tag(name: &str) -> TagName {
    TagName::new(name).unwrap()
}

fn commit(n: u64) -> CommitId {
    CommitId::new(format!("{n:040x}")).unwrap()
}

fn at(iso: &str) -> Option<At> {
    tehas_core::time::from_iso(iso).map(At::new)
}

fn names(tags: &[Tag]) -> Vec<&str> {
    tags.iter().map(|t| t.name.as_str()).collect()
}

#[test]
fn tags_are_peeled_and_timed_by_the_tag_itself() {
    let (mut gh, asked) = adapter("fixture");
    let tags = gh.tags(bound(10)).unwrap().into_items();
    assert_eq!(names(&tags), ["v1.0.0", "v1.1.0", "v1.2.0", "v0.9.0"]);
    let of = |name: &str| tags.iter().find(|t| t.name.as_str() == name).unwrap();
    // A lightweight tag keeps no time; an annotated one its tagger's date,
    // never its commit's (Decided 128).
    assert_eq!((of("v1.0.0").annotated, of("v1.0.0").at), (false, None));
    assert_eq!(of("v1.0.0").commit, commit(1));
    assert!(of("v1.1.0").annotated);
    assert_eq!(of("v1.1.0").at, at("2026-01-02T10:00:00Z"));
    assert_eq!(of("v1.1.0").commit, commit(2));
    // A tag of a tag is peeled to its commit.
    assert_eq!(of("v0.9.0").commit, commit(4));
    assert_eq!(of("v0.9.0").at, at("2025-12-01T10:00:00Z"));
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2, "two pages");
    assert!(asked[1].contains("-f after=Y3Vyc29yOjM="), "{}", asked[1]);
}

#[test]
fn the_newest_tags_are_newest_made_first_and_the_timeless_last() {
    let (mut gh, _) = adapter("fixture");
    let recent = gh.recent_tags(nz(2)).unwrap();
    assert_eq!(names(&recent.items), ["v1.2.0", "v1.1.0"]);
    assert!(recent.more);
    let all = gh.recent_tags(nz(10)).unwrap();
    assert_eq!(names(&all.items), ["v1.2.0", "v1.1.0", "v0.9.0", "v1.0.0"]);
    assert!(!all.more);
}

#[test]
fn a_tag_list_past_its_bound_or_not_whole_is_never_given() {
    let (mut gh, asked) = adapter("fixture");
    assert_eq!(gh.tags(bound(3)), Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(asked.lock().unwrap().len(), 1, "known from the first count");
    let (mut gh, _) = adapter("short");
    assert_eq!(
        gh.tags(bound(10)),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
    let (mut gh, _) = adapter("tree");
    assert_eq!(gh.tags(bound(10)), Err(Outcome::Unread(Unread::Malformed)));
}

#[test]
fn a_tag_resolves_to_its_commit_peeled() {
    let (mut gh, asked) = adapter("fixture");
    assert_eq!(gh.resolve_tag(&tag("v1.1.0")), Ok(commit(2)));
    assert!(asked.lock().unwrap()[0].contains("-f ref=refs/tags/v1.1.0"));
    assert_eq!(gh.resolve_tag(&tag("v1.0.0")), Ok(commit(1)));
    assert_eq!(gh.resolve_tag(&tag("v0.9.0")), Ok(commit(4)));
    assert_eq!(gh.resolve_tag(&tag("v8")), Err(Outcome::NotFound));
    assert_eq!(
        gh.resolve_tag(&tag("v7")),
        Err(Outcome::Unread(Unread::Inconsistent)),
        "an answer of another tag"
    );
}

fn tags_of(releases: &[Release]) -> Vec<&str> {
    releases.iter().map(|r| r.tag.as_str()).collect()
}

#[test]
fn releases_are_read_whole_drafts_among_them() {
    let (mut gh, asked) = adapter("fixture");
    let all = gh.releases(bound(10)).unwrap().into_items();
    assert_eq!(tags_of(&all), ["v1.1.0", "v1.3.0", "v1.2.0"]);
    assert_eq!(
        *asked.lock().unwrap(),
        ["api repos/acme/widget/releases?per_page=100&page=1"]
    );
    let v11 = &all[0];
    assert_eq!(v11.id.as_str(), "11");
    assert_eq!(v11.title, "v1.1.0");
    assert_eq!(v11.notes, "What changed.");
    assert_eq!(v11.published, at("2026-01-02T10:10:00Z"));
    assert!(!v11.draft && !v11.prerelease);
    assert_eq!(
        v11.files
            .iter()
            .map(|f| (f.name.as_str(), f.size))
            .collect::<Vec<_>>(),
        [
            ("tehas-v1.1.0-linux-x86_64.tar.gz", 1_048_576),
            ("tehas-v1.1.0-linux-x86_64.tar.gz.sha256", 98)
        ]
    );
    assert_eq!(
        v11.link.as_str(),
        "https://github.com/acme/widget/releases/tag/v1.1.0"
    );
    let draft = &all[1];
    assert!(draft.draft);
    assert_eq!(draft.published, None);
    assert_eq!(all[2].notes, "", "no body reads as no notes");
    assert!(all[2].prerelease);
}

#[test]
fn the_newest_releases_are_newest_made_and_the_latest_is_published() {
    let (mut gh, _) = adapter("fixture");
    let recent = gh.recent_releases(nz(2)).unwrap();
    assert_eq!(tags_of(&recent.items), ["v1.3.0", "v1.2.0"]);
    assert!(recent.more);
    // Neither the draft nor the prerelease is the latest.
    let latest = gh.latest_release().unwrap().unwrap();
    assert_eq!(latest.tag.as_str(), "v1.1.0");
}

#[test]
fn a_release_is_read_by_its_tag() {
    let (mut gh, asked) = adapter("fixture");
    let v11 = gh.release(&tag("v1.1.0")).unwrap();
    assert_eq!(v11.id.as_str(), "11");
    assert_eq!(
        asked.lock().unwrap()[0],
        "api repos/acme/widget/releases/tags/v1.1.0"
    );
    assert_eq!(gh.release(&tag("v9")), Err(Outcome::NotFound));
    assert_eq!(
        gh.release(&tag("v6")),
        Err(Outcome::Unread(Unread::Inconsistent)),
        "an answer of another tag"
    );
}

#[test]
fn releases_are_paged_and_bounded() {
    let (mut gh, asked) = adapter("many");
    assert_eq!(gh.releases(bound(200)).unwrap().items().len(), 101);
    assert_eq!(asked.lock().unwrap().len(), 2);
    let (mut gh, _) = adapter("many");
    assert_eq!(gh.releases(bound(100)), Err(Outcome::Unread(Unread::Cut)));
    let (mut gh, _) = adapter("bad");
    assert_eq!(
        gh.releases(bound(10)),
        Err(Outcome::Unread(Unread::Malformed)),
        "a draft that says it was published"
    );
    let (mut gh, _) = adapter("down");
    assert_eq!(
        gh.latest_release(),
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe
        })
    );
}

#[test]
fn download_and_archive_wait_for_their_ruling() {
    let (mut gh, asked) = adapter("fixture");
    let no: Got<()> = Err(Outcome::Refused(tehas_ports::Refusal::Unsupported));
    let file = FileName::new("tehas-v1.1.0-linux-x86_64.tar.gz").unwrap();
    assert_eq!(
        gh.download(&tag("v1.1.0"), &file, 1 << 20, &mut Vec::new()),
        no
    );
    assert_eq!(gh.archive(&commit(1), 1 << 20, &mut Vec::new()), no);
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn the_newest_first_law_holds() {
    let (mut gh, _) = adapter("fixture");
    tehas_ports::laws::code_host::recent_tags_and_releases_are_newest_first(
        &mut gh,
        nz(2),
        bound(10),
        &tag("v1.3.0"),
    )
    .unwrap();
}
