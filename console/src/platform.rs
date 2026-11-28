//! What platform this host runs, in live/platform.json, so a host that needs
//! tehas update says so. tehasbot status writes it at every run, and
//! tehasbot health reads it.
//!
//! A host that runs a release ($TEHAS_HOME/current): the installed version and
//! the newest published one, {release, newest, behind: 0, upstream:
//! "releases"}. A host with the platform checkout ($TEHAS_HOME/platform): the
//! commit it is at and how many commits it is behind origin's default branch.
//! A host with neither has no file.
//!
//! Each half reads the outside world at most once an hour, under a time limit,
//! and state/platform-fetch says when it last tried and last succeeded, so a
//! read that hangs or fails is tried again an hour later and changes nothing
//! else. The last list of releases answered is kept in state/platform-newest.

use crate::jq::{obj, J};
use crate::update::release::{older, prerelease, stale, tag_ok};
use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tehas_core::file::{write_atomic, Temp};
use tehas_core::home::{Home, State};
use tehas_core::time;
use tehas_ports::{Actor, CodeHost, Got, Outcome, Passing, Release, Scope, Sent};

const FETCH_EVERY: i64 = 3600;
const FETCH_LIMIT: u64 = 60;
/// How many of the newest releases the read asks for, as its old
/// `per_page=100` did.
const RECENT: u32 = 100;

/// The handle maker of the release reads: the same contract as
/// `wiring::releases` for `tehas update`, so a test can answer with the
/// ports' fake.
type Releases<'a> = &'a dyn Fn(&Actor, &str, Duration) -> Got<Box<dyn CodeHost + Send>>;

/// Write live/platform.json, or take it away on a host with no platform.
pub fn write(home: &Path, live: &Path) {
    match installed(home) {
        Some(version) if !version.is_empty() => {
            release(home, live, &version, &crate::wiring::platform_releases)
        }
        Some(_) => checkout(home, live),
        None => {}
    }
}

/// The version current points at: the first line of its VERSION, empty when
/// current is no link to a version, and None when that file cannot be read.
fn installed(home: &Path) -> Option<String> {
    let link = home.join("current");
    let version = link.join("VERSION");
    let is_link = fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink());
    if !is_link || !version.is_file() {
        return Some(String::new());
    }
    let text = fs::read(&version).ok()?;
    let text = String::from_utf8_lossy(&text);
    Some(text.lines().next().unwrap_or_default().to_string())
}

/// The newest of these releases by version, not by date: published
/// `vX.Y.Z` tags only, no draft and no prerelease. None when the list holds
/// none, which ends the read as a list that failed.
fn newest_of(list: &[Release]) -> Option<String> {
    let mut newest: Option<&str> = None;
    for release in list {
        if release.draft || release.prerelease {
            continue;
        }
        let tag = release.tag.as_str();
        if !tag_ok(tag) || prerelease(tag) {
            continue;
        }
        if newest.is_none_or(|n| !older(tag, n)) {
            newest = Some(tag);
        }
    }
    newest.map(str::to_string)
}

/// Whether the factory's answer, to the handle's making or to its call,
/// sends the read to the anonymous handle: a gh with no credential or one
/// no longer good, one that may not read the project, or no gh to run,
/// nothing sent. The rule `tehas update`'s release reads use
/// (console/ops/src/update/release.rs), here for the host's own read.
fn anonymous_instead(o: &Outcome) -> bool {
    matches!(
        o,
        Outcome::SignedOut
            | Outcome::Forbidden
            | Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            }
    )
}

/// The newest published release, or nothing when GitHub does not answer or
/// names none. The factory's handle at read scope asks first, which reaches
/// a private repository with the bot's token; the anonymous one answers
/// when that says so, through curl with no credential.
fn newest_read(site: Releases<'_>) -> Option<String> {
    let repo = tehas_core::release::repo()?;
    let first = NonZeroU32::new(RECENT)?;
    let ask = |by: Actor| -> Got<Vec<Release>> {
        let mut host = site(&by, &repo, Duration::from_secs(FETCH_LIMIT))?;
        host.recent_releases(first).map(|t| t.items)
    };
    match ask(Actor::Ours(Scope::Read)) {
        Ok(list) => newest_of(&list),
        Err(o) if anonymous_instead(&o) => {
            ask(Actor::Anonymous).ok().and_then(|list| newest_of(&list))
        }
        Err(_) => None,
    }
}

/// state/platform-fetch: when a read was last tried and when one last
/// worked, each as seconds. 0 and nothing when the file says neither.
fn stamp_read(stamp: &Path) -> (i64, String) {
    let text = fs::read_to_string(stamp).unwrap_or_default();
    let line = text.lines().next().unwrap_or_default().trim();
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let digits = |w: &str| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit());
    let last = if digits(first) {
        first.parse().unwrap_or(0)
    } else {
        0
    };
    let rest = rest.trim();
    (last, if digits(rest) { rest } else { "" }.to_string())
}

fn stamp_write(stamp: &Path, now: i64, fetched: &str) {
    if let Some(dir) = stamp.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(stamp, format!("{now} {fetched}\n"));
}

fn time_of(seconds: &str) -> J {
    match seconds.parse() {
        Ok(t) => J::str(time::iso(t)),
        Err(_) => J::Null,
    }
}

fn put(live: &Path, file: &J) {
    let text = format!("{}\n", file.pretty());
    let _ = write_atomic(
        &live.join("platform.json"),
        text.as_bytes(),
        0o644,
        &Temp::Pid,
    );
}

/// A host that runs a release: the version current points at, the newest
/// there is, and nothing of a checkout it may still hold. A cached newest
/// that is no version, or older than the installed one, or beside an installed
/// version that is none, goes, and the next read asks again. A prerelease is
/// ahead of the newest release, so an older newest stays beside one. The
/// commit is the one the release was built from, from its COMMIT file, and is
/// left out when the release has none.
fn release(home: &Path, live: &Path, installed: &str, site: Releases<'_>) {
    let at = Home::new(home);
    let stamp = at.state(State::PlatformFetch);
    let cache = at.state(State::PlatformNewest);
    let now = time::now();
    let (last, mut fetched) = stamp_read(&stamp);
    let cached = fs::read_to_string(&cache).unwrap_or_default();
    let mut newest = cached.lines().next().unwrap_or_default().to_string();
    if !newest.is_empty()
        && stale(&newest, installed)
        && !(prerelease(installed) && tag_ok(&newest))
    {
        let _ = fs::remove_file(&cache);
        newest.clear();
    }
    if now - last >= FETCH_EVERY {
        // The attempt is remembered first, so a read that hangs or fails is
        // not tried at every poll.
        stamp_write(&stamp, now, &fetched);
        if let Some(got) = newest_read(site) {
            newest = got;
            fetched = now.to_string();
            let _ = fs::write(&cache, format!("{newest}\n"));
            stamp_write(&stamp, now, &fetched);
        }
    }
    let mut fields = vec![
        ("generatedAt", J::str(time::iso(time::now()))),
        ("release", J::str(installed)),
    ];
    if let Some(commit) = built_from(home) {
        fields.push(("commit", J::str(commit)));
    }
    fields.extend([
        (
            "newest",
            if tag_ok(&newest) {
                J::str(newest)
            } else {
                J::Null
            },
        ),
        ("behind", J::num(0.0)),
        ("upstream", J::str("releases")),
        ("fetchedAt", time_of(&fetched)),
    ]);
    put(live, &obj(fields));
}

/// The commit the installed release was built from: the first line of
/// current/COMMIT, when it is 40 hex digits.
fn built_from(home: &Path) -> Option<String> {
    let text = fs::read_to_string(home.join("current/COMMIT")).ok()?;
    let line = text.lines().next()?.trim();
    tehas_core::grammar::commit_id(line).then(|| line.to_string())
}

fn git(co: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(co)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
}

fn fetched_from_origin(co: &Path) -> bool {
    Command::new("timeout")
        .arg(FETCH_LIMIT.to_string())
        .args(["git", "-C"])
        .arg(co)
        .args(["fetch", "-q", "origin"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A host on the platform checkout: its commit, and how far it is behind.
fn checkout(home: &Path, live: &Path) {
    let co = home.join("platform");
    if !co.join(".git").exists() {
        let _ = fs::remove_file(live.join("platform.json"));
        return;
    }
    let stamp = Home::new(home).state(State::PlatformFetch);
    let now = time::now();
    let (last, mut fetched) = stamp_read(&stamp);
    if now - last >= FETCH_EVERY {
        stamp_write(&stamp, now, &fetched);
        if fetched_from_origin(&co) {
            fetched = now.to_string();
            stamp_write(&stamp, now, &fetched);
        }
    }
    let branch = |head: &str| git(&co, &["symbolic-ref", "--quiet", "--short", head]);
    let upstream = branch("refs/remotes/origin/HEAD")
        .unwrap_or_else(|| format!("origin/{}", branch("HEAD").unwrap_or_default()));
    let range = format!("HEAD..{upstream}");
    let head = git(&co, &["rev-parse", "--quiet", "--verify", "HEAD"]).unwrap_or_default();
    let behind = git(&co, &["rev-list", "--count", &range]).unwrap_or_default();
    // When the oldest commit main has and this checkout lacks landed there.
    let since = git(
        &co,
        &["log", "--first-parent", "--reverse", "--format=%ct", &range],
    )
    .unwrap_or_default();
    let since = since.lines().next().unwrap_or_default();
    put(
        live,
        &obj(vec![
            ("generatedAt", J::str(time::iso(time::now()))),
            (
                "commit",
                if head.is_empty() {
                    J::Null
                } else {
                    J::str(head)
                },
            ),
            ("upstream", J::str(upstream)),
            (
                "behind",
                behind.parse::<u64>().map_or(J::Null, |n| J::num(n as f64)),
            ),
            ("behindSince", time_of(since)),
            ("fetchedAt", time_of(&fetched)),
        ]),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_ports::fake::{FakeSite, World};
    use tehas_ports::{Caller, ReleaseId, Site, TagName, WebLink};

    fn release_of(tag: &str, draft: bool, prerelease: bool) -> Release {
        let mut r = Release::new(
            ReleaseId::new(tag),
            TagName::new(tag).unwrap(),
            WebLink::new("https://example.test/r").unwrap(),
        );
        r.draft = draft;
        r.prerelease = prerelease;
        r
    }

    #[test]
    fn the_newest_is_by_version_and_leaves_out_what_is_not_published() {
        let list = vec![
            release_of("v0.9.0", false, false),
            release_of("v0.10.0", false, false),
            release_of("v0.11.0", true, false),
            release_of("v0.12.0", false, true),
            release_of("nightly", false, false),
            release_of("v0.11.0-rc.1", false, true),
            // A prerelease is never the newest, flagged or not.
            release_of("v0.11.0-rc.2", false, false),
        ];
        let pre = vec![
            release_of("v0.10.0", false, false),
            release_of("v0.11.0-alpha.1", false, false),
            release_of("v0.11.0-beta.1", false, true),
        ];
        assert_eq!(newest_of(&pre).as_deref(), Some("v0.10.0"));
        assert_eq!(newest_of(&list).as_deref(), Some("v0.10.0"));
        assert_eq!(newest_of(&[]), None);
        // A draft is passed over before its tag is weighed.
        assert_eq!(newest_of(&[release_of("v9.9.9", true, false)]), None);
    }

    #[test]
    fn the_stamp_holds_two_numbers_or_says_nothing() {
        let dir = tehas_core::testdir::TestDir::new("stamp");
        let f = dir.join("stamp");
        let read = |text: &str| {
            fs::write(&f, text).unwrap();
            stamp_read(&f)
        };
        assert_eq!(read("12 7\n"), (12, "7".to_string()));
        assert_eq!(read("12 \n"), (12, String::new()));
        assert_eq!(read("12"), (12, String::new()));
        assert_eq!(read("soon 7\n"), (0, "7".to_string()));
        assert_eq!(read("12 later\n"), (12, String::new()));
        assert_eq!(read("12 7 3\n"), (12, String::new()));
        drop(dir);
        assert_eq!(stamp_read(&f), (0, String::new()));
    }

    #[test]
    fn a_candidate_keeps_an_older_cached_newest() {
        let home = tehas_core::testdir::TestDir::new("rc");
        let at = Home::new(home.to_path_buf());
        let live = at.live_dir();
        fs::create_dir_all(at.state_dir()).unwrap();
        fs::create_dir_all(&live).unwrap();
        let now = time::now();
        fs::write(at.state(State::PlatformFetch), format!("{now} {now}\n")).unwrap();
        fs::write(at.state(State::PlatformNewest), "v0.22.1\n").unwrap();
        release(&home, &live, "v0.23.0-rc.2", &no_site);
        let file = J::parse(&fs::read_to_string(live.join("platform.json")).unwrap()).unwrap();
        assert_eq!(file.get("newest").as_str(), Some("v0.22.1"));
        release(&home, &live, "v0.23.0", &no_site);
        let file = J::parse(&fs::read_to_string(live.join("platform.json")).unwrap()).unwrap();
        assert!(file.get("newest").is_null());
    }

    #[test]
    fn a_release_names_the_commit_it_was_built_from() {
        let home = tehas_core::testdir::TestDir::new("commit");
        let at = Home::new(home.to_path_buf());
        let live = at.live_dir();
        fs::create_dir_all(at.state_dir()).unwrap();
        fs::create_dir_all(home.join("versions/v1.0.0-alpha.1")).unwrap();
        fs::create_dir_all(&live).unwrap();
        std::os::unix::fs::symlink("versions/v1.0.0-alpha.1", home.join("current")).unwrap();
        let now = time::now();
        fs::write(at.state(State::PlatformFetch), format!("{now} {now}\n")).unwrap();
        let commit = |text: &str| {
            fs::write(home.join("current/COMMIT"), text).unwrap();
            release(&home, &live, "v1.0.0-alpha.1", &no_site);
            let file = J::parse(&fs::read_to_string(live.join("platform.json")).unwrap()).unwrap();
            assert_eq!(file.get("release").as_str(), Some("v1.0.0-alpha.1"));
            file.get("commit").as_str().map(str::to_string)
        };
        let sha = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(commit(&format!("{sha}\n")).as_deref(), Some(sha));
        assert_eq!(commit("not a commit\n"), None);
        assert_eq!(commit(&sha[..39]), None);
        fs::remove_file(home.join("current/COMMIT")).unwrap();
        release(&home, &live, "v1.0.0-alpha.1", &no_site);
        let file = J::parse(&fs::read_to_string(live.join("platform.json")).unwrap()).unwrap();
        assert!(matches!(file.get("commit"), J::Null));
    }

    #[test]
    fn the_time_of_a_stamp() {
        assert_eq!(time_of("1"), J::str("1970-01-01T00:00:01Z"));
        assert_eq!(time_of(""), J::Null);
    }

    /// A handle maker no read may answer with: the tests that use it keep
    /// the fetch inside the hour, so the release is never asked for.
    fn no_site(_: &Actor, _: &str, _: Duration) -> Got<Box<dyn CodeHost + Send>> {
        Err(Outcome::Refused(tehas_ports::Refusal::Unsupported))
    }

    /// The newest published release the fake answers with, the drafts and
    /// prereleases of the list passed over. The read is signed in, its
    /// answer cached, and the stamp says when it worked.
    #[test]
    fn the_read_asks_the_factory_handle_and_keeps_the_newest_published() {
        let world = World::new()
            .release("v0.9.0", &[])
            .release("v0.11.0-rc.2", &[])
            .release("nightly", &[])
            .release("v0.10.0", &[]);
        let site = |by: &Actor, _: &str, _: Duration| {
            assert_eq!(*by, Actor::Ours(Scope::Read));
            FakeSite::new(&world)
                .code_host(by, Caller::new("platform"))
                .map_err(|_| Outcome::Unread(tehas_ports::Unread::Malformed))
        };
        assert_eq!(newest_read(&site).as_deref(), Some("v0.10.0"));
    }

    /// A gh with no credential sends the read to the anonymous handle; a
    /// failure of the site itself does not.
    #[test]
    fn a_signed_out_gh_sends_the_read_to_the_anonymous_handle() {
        let world = World::new().release("v0.10.0", &[]);
        let site = |by: &Actor, _: &str, _: Duration| {
            if *by == Actor::Anonymous {
                FakeSite::new(&world)
                    .code_host(by, Caller::new("platform"))
                    .map_err(|_| Outcome::Unread(tehas_ports::Unread::Malformed))
            } else {
                Err(Outcome::SignedOut)
            }
        };
        assert_eq!(newest_read(&site).as_deref(), Some("v0.10.0"));
        let site = |by: &Actor, _: &str, _: Duration| {
            if *by == Actor::Anonymous {
                panic!("a failure of the site does not send the read to the anonymous handle")
            }
            Err(Outcome::Unavailable {
                why: Passing::Server,
                sent: tehas_ports::Sent::Maybe,
            })
        };
        assert_eq!(newest_read(&site), None);
    }

    /// A fresh read writes the newest into live/platform.json, caches it in
    /// state/platform-newest, and says when it worked.
    #[test]
    fn a_fresh_read_fills_the_file_the_cache_and_the_stamp() {
        let home = tehas_core::testdir::TestDir::new("read");
        let at = Home::new(home.to_path_buf());
        let live = at.live_dir();
        fs::create_dir_all(at.state_dir()).unwrap();
        fs::create_dir_all(&live).unwrap();
        let world = World::new().release("v0.10.0", &[]).release("v0.9.0", &[]);
        let site = |by: &Actor, _: &str, _: Duration| {
            FakeSite::new(&world)
                .code_host(by, Caller::new("platform"))
                .map_err(|_| Outcome::Unread(tehas_ports::Unread::Malformed))
        };
        release(&home, &live, "v0.9.0", &site);
        let file = J::parse(&fs::read_to_string(live.join("platform.json")).unwrap()).unwrap();
        assert_eq!(file.get("newest").as_str(), Some("v0.10.0"));
        assert!(!file.get("fetchedAt").is_null());
        assert_eq!(
            fs::read_to_string(at.state(State::PlatformNewest)).unwrap(),
            "v0.10.0\n"
        );
        let (last, fetched) = stamp_read(&at.state(State::PlatformFetch));
        assert!(last > 0);
        assert_eq!(fetched, last.to_string());
    }
}
