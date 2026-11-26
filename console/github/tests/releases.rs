//! The code host's tags and releases as the adapter reads them, answered
//! from recordings of the scratch projects under `tests/recorded/releases/`
//! (faults under `tests/synthetic/releases/`), one directory a case, replayed through a runner that writes down every
//! call; then the ports' law of the newest first over the same answers, and
//! the law of the read scope over a site that replays them.

mod answers;

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::{GitHub, GitHubSite, MakeRunner};
use tehas_ports::{
    Actor, At, Bound, Caller, CodeHost, CommitId, FileName, Got, Outcome, Passing, Release, Sent,
    Tag, TagName, Unread,
};

const PROJECT: &str = "tehas-app/tehas-rec-releases";

/// The project each case was recorded from.
fn project_of(case: &str) -> &'static str {
    match case {
        "many" => "tehas-app/tehas-rec-releases-many",
        "none" => "tehas-app/tehas-rec-empty",
        "gone" => "tehas-app/tehas-rec-gone",
        _ => PROJECT,
    }
}

type Asked = Arc<Mutex<Vec<String>>>;

/// Answers from `answers::at("releases", <case>, ..)`: the first page of the
/// tags from `tags`, a later one from `tags-after-<cursor>`, one tag from
/// `ref-<tag>`, a page of releases from `releases-<page>`, one release from
/// `release-<tag>`, the latest release from `latest` and the project from
/// `project`; `.json` is gh's answer, `.fail` the runner's reason. A
/// release file asked with `release download` is written, as gh writes it,
/// to the `--output` path from `download-<name>.bin`, or fails with
/// `download-<name>.fail`. A call with no recording fails the test.
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
        let at = |file: String| answers::at("releases", &self.case, &file);
        let project = project_of(&self.case);
        let (owner, repo) = project.split_once('/').unwrap();
        if let ["release", "download", ..] = args {
            let name = field("--pattern").expect("a pattern");
            let output = field("--output").expect("an output");
            let recorded = format!("download-{name}");
            if let Ok(bytes) = std::fs::read(at(format!("{recorded}.bin"))) {
                std::fs::write(&output, bytes).unwrap();
                return Ok(String::new());
            }
            return match std::fs::read_to_string(at(format!("{recorded}.fail"))) {
                Ok(why) => Err(why.trim_end().to_string()),
                Err(_) => panic!("no recording {}/{recorded}", self.case),
            };
        }
        let name = match args {
            ["api", "graphql", ..] => {
                assert_eq!(field("owner").as_deref(), Some(owner));
                assert_eq!(field("name").as_deref(), Some(repo));
                match (field("ref"), field("after")) {
                    (Some(r), _) => format!(
                        "ref-{}",
                        r.strip_prefix("refs/tags/").expect("a tag's whole ref")
                    ),
                    (None, Some(after)) => format!("tags-after-{after}"),
                    (None, None) => "tags".to_string(),
                }
            }
            ["api", path] if *path == format!("repos/{project}") => "project".to_string(),
            ["api", path] => {
                let rest = path
                    .strip_prefix(&format!("repos/{project}/releases"))
                    .unwrap_or_else(|| panic!("no recording of {path}"));
                if rest == "/latest" {
                    "latest".to_string()
                } else if let Some(tag) = rest.strip_prefix("/tags/") {
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
        if let Ok(out) = std::fs::read_to_string(at(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at(format!("{name}.fail"))) {
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
    (GitHub::new(project_of(case), Box::new(replay)), asked)
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

fn commit(sha: &str) -> CommitId {
    CommitId::new(sha).unwrap()
}

// The commits the tags of tehas-app/tehas-rec-releases name.
const V090: &str = "8291811765fd8c6bbcf77bca477ad3976046e86a";
const V100: &str = "3b2bc922e522fe40d66ccf796c60593ace42d282";
const V110: &str = "f703bfa5b38d4088f9235eed9fde20a16370e0ee";
const V120: &str = "e359d23e1d64c6b30b5d400efe3976b49df99a69";

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
    assert_eq!(names(&tags), ["v0.9.0", "v1.0.0", "v1.1.0", "v1.2.0"]);
    let of = |name: &str| tags.iter().find(|t| t.name.as_str() == name).unwrap();
    // A lightweight tag keeps no time; an annotated one its tagger's date,
    // never its commit's (Decided 128).
    assert_eq!((of("v1.0.0").annotated, of("v1.0.0").at), (false, None));
    assert_eq!(of("v1.0.0").commit, commit(V100));
    assert!(of("v1.1.0").annotated);
    assert_eq!(of("v1.1.0").at, at("2026-11-26T14:09:48Z"));
    assert_eq!(of("v1.1.0").commit, commit(V110));
    assert_eq!(of("v1.2.0").commit, commit(V120));
    // A tag of a tag is peeled to its commit.
    assert_eq!(of("v0.9.0").commit, commit(V090));
    assert_eq!(of("v0.9.0").at, at("2026-11-26T14:09:43Z"));
    assert_eq!(asked.lock().unwrap().len(), 1, "one page");
}

#[test]
fn tags_are_paged_and_bounded() {
    let (mut gh, asked) = adapter("many");
    let tags = gh.tags(bound(200)).unwrap().into_items();
    assert_eq!(tags.len(), 101);
    assert_eq!(tags[0].name.as_str(), "v0.001.0");
    assert_eq!(tags[100].name.as_str(), "v0.101.0");
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2, "two pages");
    assert!(asked[1].contains("-f after=MTAw"), "{}", asked[1]);
    let (mut gh, asked) = adapter("many");
    assert_eq!(gh.tags(bound(100)), Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(asked.lock().unwrap().len(), 1, "known from the first count");
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
    assert_eq!(gh.resolve_tag(&tag("v1.1.0")), Ok(commit(V110)));
    assert!(asked.lock().unwrap()[0].contains("-f ref=refs/tags/v1.1.0"));
    assert_eq!(gh.resolve_tag(&tag("v1.0.0")), Ok(commit(V100)));
    assert_eq!(gh.resolve_tag(&tag("v0.9.0")), Ok(commit(V090)));
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
    assert_eq!(tags_of(&all), ["v1.3.0", "v1.2.0", "v1.1.0"]);
    assert_eq!(
        *asked.lock().unwrap(),
        [format!("api repos/{PROJECT}/releases?per_page=100&page=1")]
    );
    let v11 = &all[2];
    assert_eq!(v11.id.as_str(), "459501096");
    assert_eq!(v11.title, "v1.1.0");
    assert_eq!(v11.notes, "What changed.");
    assert_eq!(v11.published, at("2026-11-26T14:09:53Z"));
    assert!(!v11.draft && !v11.prerelease);
    assert_eq!(
        v11.files
            .iter()
            .map(|f| (f.name.as_str(), f.size))
            .collect::<Vec<_>>(),
        [
            ("tehas-v1.1.0-linux-x86_64.tar.gz", 232),
            ("tehas-v1.1.0-linux-x86_64.tar.gz.sha256", 99)
        ]
    );
    assert_eq!(
        v11.link.as_str(),
        "https://github.com/tehas-app/tehas-rec-releases/releases/tag/v1.1.0"
    );
    let draft = &all[0];
    assert!(draft.draft);
    assert_eq!(draft.published, None);
    assert_eq!(all[1].notes, "", "no body reads as no notes");
    assert!(all[1].prerelease);
}

#[test]
fn the_newest_releases_are_newest_made_and_the_latest_is_published() {
    let (mut gh, _) = adapter("fixture");
    let recent = gh.recent_releases(nz(2)).unwrap();
    assert_eq!(tags_of(&recent.items), ["v1.3.0", "v1.2.0"]);
    assert!(recent.more);
    // The latest is GitHub's own, one question: here neither the draft nor
    // the prerelease.
    let (mut gh, asked) = adapter("fixture");
    let latest = gh.latest_release().unwrap().unwrap();
    assert_eq!(latest.tag.as_str(), "v1.1.0");
    assert_eq!(
        *asked.lock().unwrap(),
        [format!("api repos/{PROJECT}/releases/latest")]
    );
}

#[test]
fn the_latest_release_is_the_one_github_names_latest() {
    // v1.1.0 marked latest by hand is the latest, though v1.4.0 is
    // published after it: the token's reader answers as the anonymous one
    // does.
    let (mut gh, _) = adapter("marked");
    tehas_ports::laws::code_host::the_marked_release_is_the_latest(&mut gh, &tag("v1.1.0"))
        .unwrap();
    // None answered, the project is there: it has no latest release.
    let (mut gh, asked) = adapter("none");
    assert_eq!(gh.latest_release(), Ok(None));
    assert_eq!(
        *asked.lock().unwrap(),
        [
            "api repos/tehas-app/tehas-rec-empty/releases/latest",
            "api repos/tehas-app/tehas-rec-empty"
        ]
    );
    // Nor the project: the read's own outcome.
    let (mut gh, _) = adapter("gone");
    assert_eq!(gh.latest_release(), Err(Outcome::NotFound));
    // A prerelease named latest is no latest.
    let (mut gh, _) = adapter("bad");
    assert_eq!(
        gh.latest_release(),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

#[test]
fn a_release_is_read_by_its_tag() {
    let (mut gh, asked) = adapter("fixture");
    let v11 = gh.release(&tag("v1.1.0")).unwrap();
    assert_eq!(v11.id.as_str(), "459501096");
    assert_eq!(
        asked.lock().unwrap()[0],
        format!("api repos/{PROJECT}/releases/tags/v1.1.0")
    );
    let v12 = gh.release(&tag("v1.2.0")).unwrap();
    assert_eq!(v12.id.as_str(), "459501205");
    assert!(v12.prerelease);
    assert_eq!(gh.release(&tag("v9")), Err(Outcome::NotFound));
    assert_eq!(
        gh.release(&tag("v1.3.0")),
        Err(Outcome::NotFound),
        "a draft is not found by its tag"
    );
    assert_eq!(
        gh.release(&tag("v6")),
        Err(Outcome::Unread(Unread::Inconsistent)),
        "an answer of another tag"
    );
}

#[test]
fn releases_are_paged_and_bounded() {
    let (mut gh, asked) = adapter("many");
    let all = gh.releases(bound(200)).unwrap().into_items();
    assert_eq!(all.len(), 101);
    assert_eq!(tags_of(&all[..1]), ["v0.101.0"]);
    assert_eq!(tags_of(&all[100..]), ["v0.001.0"]);
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

const SUMS: &str = "tehas-v1.1.0-linux-x86_64.tar.gz.sha256";

fn file(name: &str) -> FileName {
    FileName::new(name).unwrap()
}

#[test]
fn a_release_file_is_downloaded_whole_and_its_directory_goes() {
    let (mut gh, asked) = adapter("fixture");
    let mut into = Vec::new();
    assert_eq!(
        gh.download(&tag("v1.1.0"), &file(SUMS), 1 << 20, &mut into),
        Ok(())
    );
    assert_eq!(into.len(), 99);
    assert!(into.starts_with(b"4d4f5f9b"));
    assert!(into.ends_with(b"  tehas-v1.1.0-linux-x86_64.tar.gz\n"));
    let asked = asked.lock().unwrap();
    assert_eq!(
        asked[0],
        format!("api repos/{PROJECT}/releases/tags/v1.1.0")
    );
    let words: Vec<&str> = asked[1].split(' ').collect();
    assert_eq!(
        &words[..4],
        [
            "release",
            "download",
            &format!("--repo={PROJECT}"),
            &format!("--pattern={SUMS}")
        ]
    );
    assert_eq!(&words[words.len() - 2..], ["--", "v1.1.0"]);
    let output = words[4].strip_prefix("--output=").unwrap();
    assert!(
        !std::path::Path::new(output).parent().unwrap().exists(),
        "the directory goes"
    );
    assert!(!asked[1].contains("token"), "{}", asked[1]);
}

#[test]
fn a_file_past_the_limit_or_not_listed_is_never_downloaded() {
    let (mut gh, asked) = adapter("fixture");
    let mut into = Vec::new();
    assert_eq!(
        gh.download(&tag("v1.1.0"), &file(SUMS), 98, &mut into),
        Err(Outcome::Unread(Unread::TooLarge)),
        "listed at 99 bytes"
    );
    assert_eq!(
        gh.download(&tag("v1.1.0"), &file("other.tar.gz"), 1 << 20, &mut into),
        Err(Outcome::NotFound)
    );
    assert_eq!(
        gh.download(&tag("v9"), &file(SUMS), 1 << 20, &mut into),
        Err(Outcome::NotFound)
    );
    assert!(into.is_empty());
    assert!(asked.lock().unwrap().iter().all(|a| a.starts_with("api ")));
}

#[test]
fn a_file_of_another_size_or_a_refused_download_is_an_outcome() {
    let mut into = Vec::new();
    let (mut gh, _) = adapter("lying");
    assert_eq!(
        gh.download(&tag("v1.1.0"), &file(SUMS), 1 << 20, &mut into),
        Err(Outcome::Unread(Unread::Inconsistent)),
        "gh wrote 98 bytes of a file listed at 99"
    );
    let (mut gh, _) = adapter("refused");
    assert_eq!(
        gh.download(&tag("v1.1.0"), &file(SUMS), 1 << 20, &mut into),
        Err(Outcome::Forbidden)
    );
    assert!(into.is_empty());
}

#[test]
fn archive_is_a_floor() {
    let (mut gh, asked) = adapter("fixture");
    let no: Got<()> = Err(Outcome::Refused(tehas_ports::Refusal::Unsupported));
    assert_eq!(gh.archive(&commit(V100), 1 << 20, &mut Vec::new()), no);
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

#[test]
fn the_read_scope_law_holds_on_the_site() {
    let asked = Asked::default();
    let seen = Arc::clone(&asked);
    let make: MakeRunner = Arc::new(move |_: &Actor| {
        Ok(Box::new(Replay {
            case: "fixture".to_string(),
            asked: Arc::clone(&seen),
        }) as Box<dyn GhRunner>)
    });
    let site = GitHubSite::new(PROJECT, "", None, make);
    tehas_ports::laws::scopes::read_scope_fetches_and_writes_nothing(
        &site,
        Caller::new("test.releases"),
    )
    .unwrap();
    assert_eq!(
        *asked.lock().unwrap(),
        [format!("api repos/{PROJECT}/releases?per_page=100&page=1")]
    );
}
