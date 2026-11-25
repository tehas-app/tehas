//! The release half of `tehas update`: the releases there are, a download and
//! its two checks, the unpack beside the versions already on the host, the
//! instance checked against the target's own rules, and the switch of
//! ~/tehas/current.
//!
//! The repository releases come from and the key a signature is checked with
//! are built into tehas (tehas_core::release), never read from a file an
//! earlier release left on the host or from the download. Nothing is unpacked before both checks pass, and current moves
//! only after the unpack and the target's own settings check, so whatever
//! fails leaves the host on the version it runs. An install.sh that says it
//! installed nothing after the switch has current and the image tag put back
//! (`undo`).
//!
//! host/release/install repeats these checks in POSIX sh, because it runs on a
//! host with nothing installed yet.

use super::{
    fault, first_line, host_or_refuse, output, quiet, say, tell, temp_dir, timed, Host, Mode, Plan,
    Undo,
};
use std::cmp::Ordering;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tehas_core::home::{Home, State};
use tehas_core::{release, version};
use tehas_ports::{
    Actor, CodeHost, FileName, Got, Outcome, Passing, Release, Sent, TagName, ALL_MAX,
};

/// Seconds for each call out: a remote that stops answering fails the update.
const API_LIMIT: u32 = 60;
const DOWNLOAD_LIMIT: u32 = 600;
/// The most bytes one release file may hold: a release's tarball is far
/// smaller, so a larger one is refused before it is fetched.
const DOWNLOAD_MAX: u64 = 1 << 30;
/// A code host handle on tehas's own release project, a site of its own
/// (Decided 23), acting as `Actor::Operator`, the operator's gh as it is
/// signed in, or as `Actor::Anonymous`, with no credential: the console's
/// wiring makes it from the actor, the project and the bound of each call,
/// so this module knows the port alone.
pub type Releases<'a> = &'a dyn Fn(&Actor, &str, Duration) -> Got<Box<dyn CodeHost + Send>>;
/// docker image inspect and docker tag.
const DOCKER_LIMIT: u32 = 180;
/// The target's own settings check, and how much of what it says is read.
const SETTINGS_LIMIT: u32 = 120;
const SETTINGS_SAID: usize = 64 * 1024;
/// The name of the image a version ran with.
const IMAGE_TAG: &str = "tehas/pi-rukas:release-";

/// The shape of a published version: vX.Y.Z, or a prerelease of one,
/// vX.Y.Z-alpha.N, -beta.N or -rc.N.
pub fn tag_ok(tag: &str) -> bool {
    version::is_tag(tag)
}

/// A prerelease of any stage: published as a GitHub prerelease, installed only
/// when named.
pub fn prerelease(tag: &str) -> bool {
    version::is_prerelease(tag)
}

fn order(c: u8) -> i32 {
    match c {
        b'0'..=b'9' => 0,
        b'A'..=b'Z' | b'a'..=b'z' => i32::from(c),
        b'~' => -1,
        _ => i32::from(c) + 256,
    }
}

/// The order of `sort -V`, without its handling of file name suffixes, except
/// that the prereleases of a version come before it, alpha before beta before
/// rc: v1.0.0-alpha.2 before v1.0.0-beta.1 before v1.0.0-rc.9 before
/// v1.0.0-rc.10 before v1.0.0.
fn version_order(a: &str, b: &str) -> Ordering {
    let parts =
        |s| version::prerelease(s).map_or((s, None), |(base, stage, n)| (base, Some((stage, n))));
    let ((x, m), (y, n)) = (parts(a), parts(b));
    sort_v(x.as_bytes(), y.as_bytes()).then_with(|| match (m, n) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some((s, m)), Some((t, n))) => s.cmp(&t).then_with(|| sort_v(m.as_bytes(), n.as_bytes())),
    })
}

fn sort_v(a: &[u8], b: &[u8]) -> Ordering {
    let at = |s: &[u8], i: usize| s.get(i).copied().unwrap_or(0);
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        while (i < a.len() && !at(a, i).is_ascii_digit())
            || (j < b.len() && !at(b, j).is_ascii_digit())
        {
            let x = if i < a.len() { order(a[i]) } else { 0 };
            let y = if j < b.len() { order(b[j]) } else { 0 };
            if x != y {
                return x.cmp(&y);
            }
            i += 1;
            j += 1;
        }
        while at(a, i) == b'0' {
            i += 1;
        }
        while at(b, j) == b'0' {
            j += 1;
        }
        let mut first = Ordering::Equal;
        while at(a, i).is_ascii_digit() && at(b, j).is_ascii_digit() {
            if first == Ordering::Equal {
                first = at(a, i).cmp(&at(b, j));
            }
            i += 1;
            j += 1;
        }
        if at(a, i).is_ascii_digit() {
            return Ordering::Greater;
        }
        if at(b, j).is_ascii_digit() {
            return Ordering::Less;
        }
        if first != Ordering::Equal {
            return first;
        }
    }
    Ordering::Equal
}

/// Whether version `a` is older than `b`.
pub fn older(a: &str, b: &str) -> bool {
    if a == b {
        return false;
    }
    let (x, y) = (
        a.strip_prefix('v').unwrap_or(a),
        b.strip_prefix('v').unwrap_or(b),
    );
    version_order(x, y).then_with(|| x.cmp(y)).is_le()
}

/// A kept newest version that cannot be said: no version, or older than the one
/// the host runs, which no published list of that repository holds.
pub fn stale(newest: &str, installed: &str) -> bool {
    !tag_ok(newest) || !tag_ok(installed) || older(newest, installed)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Published {
    pub tag: String,
    pub body: String,
}

/// Whether a release is listed: published, and a prerelease only when its tag
/// says it is one.
fn listed(tag: &str, draft: bool, pre: bool) -> bool {
    !draft && (!pre || prerelease(tag)) && tag_ok(tag)
}

/// The releases the code host answered, as `published` keeps them.
fn published_of(all: Vec<Release>) -> Vec<Published> {
    let mut found: Vec<Published> = all
        .into_iter()
        .filter(|r| listed(r.tag.as_str(), r.draft, r.prerelease))
        .map(|r| Published {
            tag: r.tag.as_str().to_string(),
            body: r.notes,
        })
        .collect();
    found.sort_by(|a, b| version_order(&a.tag, &b.tag));
    found.reverse();
    found
}

/// The newest release that is no prerelease.
pub(crate) fn newest(list: &[Published]) -> Option<&str> {
    list.iter()
        .map(|r| r.tag.as_str())
        .find(|tag| !prerelease(tag))
}

/// The words for a host that runs a version newer than the newest release, a
/// prerelease most often: what it is ahead of, and the newest prerelease of its
/// own version when that is newer. The older release is never offered. None
/// when the host is not ahead.
fn ahead(installed: &str, newest: &str, list: &[Published]) -> Option<Vec<String>> {
    if installed.is_empty() || newest.is_empty() || !older(newest, installed) {
        return None;
    }
    let keeps = "a plain tehas update keeps it until a newer release is out";
    let Some((base, _, _)) = version::prerelease(installed) else {
        return Some(vec![
            format!("release: installed {installed}, ahead of the newest release {newest}"),
            format!("release: {keeps}"),
        ]);
    };
    let first = format!(
        "release: installed {installed}, a prerelease ahead of the newest release {newest}"
    );
    let next = list.iter().map(|r| r.tag.as_str()).find(|tag| {
        version::prerelease(tag).is_some_and(|(b, _, _)| b == base) && older(installed, tag)
    });
    let second = match next {
        Some(tag) => format!(
            "release: the newest prerelease of {base} is {tag}; tehas update --to {tag} moves to it"
        ),
        None => format!("release: {installed} is the newest prerelease of {base}; {keeps}"),
    };
    Some(vec![first, second])
}

/// What an update moves to: the version named, when it is published, or else
/// the newest release. A prerelease only when it is named.
pub(crate) fn target<'a>(list: &'a [Published], to: Option<&'a str>) -> Option<&'a str> {
    match to {
        Some(to) => list.iter().any(|r| r.tag == to).then_some(to),
        None => newest(list),
    }
}

/// The sections after `from` up to and including `to`, oldest first.
pub(crate) fn between<'a>(
    from: &str,
    to: &str,
    list: &'a [Published],
) -> impl Iterator<Item = &'a Published> {
    let (from, to) = (from.to_string(), to.to_string());
    list.iter()
        .rev()
        .filter(move |r| !older(&to, &r.tag) && (from.is_empty() || older(&from, &r.tag)))
}

/// A body as a person reads it under its version. It goes to a terminal, so
/// no control or invisible character of it does.
fn indented(body: &str) -> String {
    let body: String = body
        .chars()
        .filter(|&c| c != '\r' && !tehas_core::text::hidden(c))
        .collect();
    let text = format!("{body}\n");
    text.split_inclusive('\n')
        .map(|line| format!("  {line}"))
        .collect()
}

/// The architecture in the name of an asset: the release workflow builds for
/// linux x86_64 only.
fn arch_of(machine: &str) -> Option<&'static str> {
    match machine {
        "x86_64" | "amd64" => Some("x86_64"),
        _ => None,
    }
}

fn machine() -> String {
    output(Command::new("uname").arg("-m").stderr(Stdio::null())).1
}

/// The machine's architecture, or the words of a machine with no release.
fn arch() -> Result<&'static str, String> {
    let machine = machine();
    arch_of(&machine).ok_or_else(|| {
        let said = if machine.is_empty() {
            "nothing"
        } else {
            &machine
        };
        format!("this machine has no tehas release to download: releases are published for x86_64 only, and uname -m says {said}")
    })
}

fn tarball_of(tag: &str, arch: &str) -> String {
    format!("tehas-{tag}-linux-{arch}.tar.gz")
}

/// A .sha256 names the tarball and nothing else, so the check is of this file.
fn sum_names(sums: &str, tarball: &str) -> bool {
    let names: Vec<&str> = sums
        .split('\n')
        .map(|line| line.split_whitespace().nth(1).unwrap_or(""))
        .collect();
    let named = names.join("\n");
    let named = named.trim_end_matches('\n');
    named == tarball || named.strip_prefix('*') == Some(tarball)
}

/// The members of `tar -tv` that a release does not hold: anything not a file
/// or a directory, anything outside tehas-<tag>/, anything that climbs out.
fn strangers(listing: &str, top: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let kind = fields.next().unwrap_or("").chars().next().unwrap_or(' ');
            let name = fields.nth(4).unwrap_or("");
            let inside = name.starts_with(top) && !name.split('/').any(|part| part == "..");
            (!matches!(kind, '-' | 'd') || !inside).then(|| name.to_string())
        })
        .collect()
}

/// The version the release of `current` says, or nothing.
pub(crate) fn installed(host: &Host) -> String {
    let link = host.current();
    let is_link = fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink());
    if !is_link || !link.join("VERSION").is_file() {
        return String::new();
    }
    first_line(&link.join("VERSION"))
}

/// git while the host updates from its platform checkout, release once it runs
/// from ~/tehas/current. The setting decides; without it the host's own layout
/// does, so a host installed before the setting existed keeps updating the way
/// it did. A host with neither is not guessed at.
pub(crate) fn source(host: &Host) -> Result<&'static str, u8> {
    match host.from.as_str() {
        "git" => return Ok("git"),
        "release" => return Ok("release"),
        _ => {}
    }
    if fs::symlink_metadata(host.current()).is_ok_and(|m| m.file_type().is_symlink()) {
        Ok("release")
    } else if host.checkout().join(".git").exists() {
        Ok("git")
    } else {
        let home = host.home.display();
        fault(&format!("no platform checkout at {home}/platform and no {home}/current: set TEHAS_UPDATE_FROM=release in {}/instance.env to install from a release, or clone the platform repository there", instance_dir()));
        Err(78)
    }
}

fn instance_dir() -> String {
    env::var("TEHAS_INSTANCE_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "?".to_string())
}

/// TEHAS_RELEASE_REPO, else the repository built into tehas. A versions.env on
/// the host is never read for it: a release writes that file.
fn repo() -> Result<String, u8> {
    if let Some(repo) = release::repo() {
        return Ok(repo);
    }
    fault(&format!(
        "TEHAS_RELEASE_REPO in {}/instance.env is not owner/name",
        instance_dir()
    ));
    Err(78)
}

/// Whether the operator's answer, to the handle's making or to its first
/// call, sends the update to the anonymous handle: a gh with no
/// credential or one no longer good (`SignedOut`), one that may not read
/// the project (`Forbidden`), or no gh to run, nothing sent
/// (`Unavailable { Unreachable, No }`). Never a failure of the site's or
/// one that may have been sent: those fail the update as they are
/// (tehas-68, 2026-11-25).
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

/// What stopped the first call: whether the anonymous handle answered it,
/// and its outcome.
struct Stopped {
    anonymous: bool,
    why: Outcome,
}

impl Stopped {
    /// The outcome as a person reads it. The anonymous handle refuses a
    /// project that is private or not there.
    fn line(&self) -> String {
        let line = self.why.line();
        if self.anonymous && matches!(self.why, Outcome::Refused(_) | Outcome::NotFound) {
            format!("{line} (a private repository needs gh signed in)")
        } else {
            line.to_string()
        }
    }
}

/// The handle that answered `ask`, and its answer: the operator's, or the
/// anonymous one when the operator's says so (`anonymous_instead`).
fn first<T>(
    site: Releases,
    repo: &str,
    limit: Duration,
    mut ask: impl FnMut(&mut dyn CodeHost) -> Got<T>,
) -> Result<(Box<dyn CodeHost + Send>, T), Stopped> {
    let mut on = |by: &Actor| {
        site(by, repo, limit).and_then(|mut h| {
            let got = ask(&mut *h)?;
            Ok((h, got))
        })
    };
    match on(&Actor::Operator) {
        Err(o) if anonymous_instead(&o) => on(&Actor::Anonymous).map_err(|why| Stopped {
            anonymous: true,
            why,
        }),
        got => got.map_err(|why| Stopped {
            anonymous: false,
            why,
        }),
    }
}

fn bound(seconds: u32) -> Duration {
    Duration::from_secs(seconds.into())
}

/// The published releases, newest first.
pub(crate) fn list(site: Releases) -> Result<Vec<Published>, u8> {
    let repo = repo()?;
    match first(site, &repo, bound(API_LIMIT), |h| h.releases(ALL_MAX)) {
        Ok((_, all)) => Ok(published_of(all.into_items())),
        Err(stopped) => {
            fault(&format!(
                "could not list the releases of {repo}: {}",
                stopped.line()
            ));
            Err(1)
        }
    }
}

/// The newest vX.Y.Z tag that is an ancestor of the checkout's HEAD: describe
/// reads only what the repository has, so this asks nothing of a remote.
fn checkout_tag(dir: &Path) -> String {
    if !dir.join(".git").exists() {
        return String::new();
    }
    let (rc, tag) = output(
        timed(API_LIMIT, "git")
            .arg("-C")
            .arg(dir)
            .args([
                "describe",
                "--tags",
                "--abbrev=0",
                "--match",
                "v[0-9]*",
                "HEAD",
            ])
            .stderr(Stdio::null()),
    );
    if rc == 0 && tag_ok(&tag) {
        tag
    } else {
        String::new()
    }
}

fn fetch(site: Releases, tag: &str, dir: &Path, tarball: &str) -> bool {
    let Ok(repo) = repo() else {
        return false;
    };
    let names = [
        tarball.to_string(),
        format!("{tarball}.sha256"),
        format!("{tarball}.minisig"),
    ];
    download(site, &repo, tag, dir, &names)
}

/// Each of `names` of the release `tag` into `dir` through the code host,
/// on the handle that answered the first (`first`); false, said, at the
/// first that is not downloaded whole.
fn download(site: Releases, repo: &str, tag: &str, dir: &Path, names: &[String]) -> bool {
    let Ok(at) = TagName::new(tag) else {
        fault(&format!("could not download {tag} of {repo}: not a tag"));
        return false;
    };
    let mut files = Vec::new();
    for name in names {
        let Ok(file) = FileName::new(name.as_str()) else {
            fault(&format!(
                "could not download {name} of {repo}: not a file name"
            ));
            return false;
        };
        files.push((name, file));
    }
    let Some(((name, file), rest)) = files.split_first() else {
        return true;
    };
    // Each attempt writes its file anew: false when it cannot be written.
    let one = |h: &mut dyn CodeHost, name: &str, file: &FileName| -> Got<bool> {
        let Ok(mut into) = fs::File::create(dir.join(name)) else {
            return Ok(false);
        };
        h.download(&at, file, DOWNLOAD_MAX, &mut into).map(|()| true)
    };
    let unwritten = |name: &str| {
        fault(&format!(
            "could not download {name} of {repo}: could not write it here"
        ));
        false
    };
    let mut handle = match first(site, repo, bound(DOWNLOAD_LIMIT), |h| one(h, name, file)) {
        Ok((_, false)) => return unwritten(name),
        Ok((handle, true)) => handle,
        Err(stopped) => {
            fault(&format!(
                "could not download {name} of {repo}: {}",
                stopped.line()
            ));
            return false;
        }
    };
    for (name, file) in rest {
        match one(&mut *handle, name, file) {
            Ok(true) => {}
            Ok(false) => return unwritten(name),
            Err(why) => {
                fault(&format!(
                    "could not download {name} of {repo}: {}",
                    why.line()
                ));
                return false;
            }
        }
    }
    true
}

/// Both checks of a download; false when either cannot say yes.
fn verify(dir: &Path, tarball: &str) -> bool {
    verify_with(dir, tarball, "minisign")
}

fn verify_with(dir: &Path, tarball: &str, minisign: &str) -> bool {
    let sums = format!("{tarball}.sha256");
    let signature = format!("{tarball}.minisig");
    for name in [tarball, &sums, &signature] {
        if !dir.join(name).is_file() {
            fault(&format!("{name} is missing from the download"));
            return false;
        }
    }
    let named = fs::read(dir.join(&sums)).unwrap_or_default();
    if !sum_names(&String::from_utf8_lossy(&named), tarball) {
        fault(&format!("{sums} does not name {tarball}"));
        return false;
    }
    let matches = Command::new("sha256sum")
        .args(["-c", "--status", &sums])
        .current_dir(dir)
        .status()
        .is_ok_and(|s| s.success());
    if !matches {
        fault(&format!("{tarball} does not match its .sha256"));
        return false;
    }
    // The key built into this program, never a file a release could have put
    // on the host.
    let key = dir.join("release.pub");
    if fs::write(&key, release::KEY).is_err() {
        fault(&format!(
            "could not write the release key to {}",
            key.display()
        ));
        return false;
    }
    let ran = Command::new(minisign)
        .args(["-q", "-V", "-p"])
        .arg(&key)
        .arg("-m")
        .arg(dir.join(tarball))
        .arg("-x")
        .arg(dir.join(&signature))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match ran {
        Err(_) => {
            fault("minisign is not installed, so the signature cannot be checked (sudo apt-get install minisign)");
            false
        }
        Ok(s) if !s.success() => {
            fault(&format!(
                "{tarball} is not signed by the release key built into tehas (host/release/minisign.pub)"
            ));
            false
        }
        Ok(_) => true,
    }
}

/// Every member under tehas-<tag>/, a file or a directory, no member that climbs
/// out: what is unpacked is the release and nothing beside it. It goes into a
/// directory beside the versions and is renamed into place, so a version that
/// is there is whole.
fn unpack(host: &Host, tag: &str, dir: &Path, tarball_name: &str) -> bool {
    let tarball = dir.join(tarball_name);
    let versions = host.home.join("versions");
    let tree = versions.join(tag);
    if first_line(&tree.join("VERSION")) == tag {
        return true;
    }
    let (rc, listing) = output(
        Command::new("tar")
            .arg("-tvzf")
            .arg(&tarball)
            .stderr(Stdio::null()),
    );
    let bad = if rc != 0 {
        vec!["(unreadable)".to_string()]
    } else {
        strangers(&listing, &format!("tehas-{tag}/"))
    };
    if !bad.is_empty() {
        let named: String = bad.iter().take(3).map(|n| format!("{n} ")).collect();
        fault(&format!(
            "{} holds what a release does not: {named}",
            tarball.display()
        ));
        return false;
    }
    if fs::create_dir_all(&versions).is_err() {
        return false;
    }
    let Ok(stage) = temp_dir(&versions, ".unpack-") else {
        return false;
    };
    let unpacked = Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&stage)
        .arg("--no-same-owner")
        .status()
        .is_ok_and(|s| s.success());
    if !unpacked {
        let _ = fs::remove_dir_all(&stage);
        fault(&format!("could not unpack {}", tarball.display()));
        return false;
    }
    let top = stage.join(format!("tehas-{tag}"));
    if first_line(&top.join("VERSION")) != tag {
        let _ = fs::remove_dir_all(&stage);
        fault(&format!(
            "{} does not say it is {tag} in its VERSION",
            tarball.display()
        ));
        return false;
    }
    let _ = fs::remove_dir_all(&tree);
    let moved = fs::rename(&top, &tree).is_ok();
    moved && fs::remove_dir_all(&stage).is_ok()
}

fn example_line(example: &str, name: &str) -> Option<String> {
    let assigned = format!("{name}=");
    example.lines().find_map(|line| {
        let rest = line
            .strip_prefix('#')
            .unwrap_or(line)
            .trim_start_matches(' ');
        rest.starts_with(&assigned).then(|| rest.to_string())
    })
}

fn not_set(said: &str) -> Vec<String> {
    said.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("tehas: ")?;
            let (name, _) = rest.split_once(" is not set")?;
            name.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                .then(|| name.to_string())
        })
        .collect()
}

#[derive(Debug, PartialEq)]
enum Held {
    Yes,
    Missing,
    /// The check gave no answer in time, or said more than is read: what it
    /// asks for is not known.
    Unanswered,
}

/// The rules of the version about to run, not of the one running now: its own
/// lib/config.sh, as a shell runs it. A setting it asks for that the instance
/// lacks is named with the value the release's example gives it.
fn settings_hold(tree: &Path, seconds: u32) -> Held {
    let (rc, said, cut) = crate::store::merged_up_to(
        timed(seconds, "bash")
            .args([
                "-c",
                ". \"$1/lib/config.sh\" && tehas_load && tehas_require_instance_settings",
                "_",
            ])
            .arg(tree),
        SETTINGS_SAID,
    );
    let said = said.trim_end_matches('\n');
    // A cut text may have lost the line that names a missing setting.
    if cut {
        tell(&format!(
            "settings: {}/lib/config.sh said more than {} KiB; its check is taken as failed",
            tree.display(),
            SETTINGS_SAID / 1024
        ));
        return Held::Unanswered;
    }
    if rc == 0 {
        return Held::Yes;
    }
    tell(said);
    if rc == 124 || rc == 137 {
        tell(&format!(
            "settings: {}/lib/config.sh gave no answer in {seconds}s",
            tree.display()
        ));
        return Held::Unanswered;
    }
    let example = fs::read(tree.join("instances/example/instance.env")).unwrap_or_default();
    let example = String::from_utf8_lossy(&example);
    for name in not_set(said) {
        if let Some(line) = example_line(&example, &name) {
            tell(&format!("  example: {line}"));
        }
    }
    Held::Missing
}

/// The TEHAS_ keys an instance.env assigns, sorted, each once.
fn keys_of(text: &str) -> Vec<String> {
    let mut keys: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let line = match line.strip_prefix("export") {
                Some(rest) if rest.starts_with([' ', '\t']) => rest.trim_start(),
                _ => line,
            };
            let (key, _) = line.split_once('=')?;
            let tail = key.strip_prefix("TEHAS_")?;
            (!tail.is_empty()
                && tail
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'))
            .then(|| key.to_string())
        })
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The TEHAS_ keys of instance.env that <tree> never names, and those it
/// could not be searched for: a grep that failed is not a key that is unknown.
fn unknown_keys(tree: &Path, env_file: &Path) -> (Vec<String>, Vec<String>) {
    let text = fs::read(env_file).unwrap_or_default();
    // A part a release does not have is not searched, so it is no error.
    let parts: Vec<PathBuf> = ["bin", "libexec", "lib", "host", "install.sh", "instances"]
        .iter()
        .map(|part| tree.join(part))
        .filter(|path| fs::symlink_metadata(path).is_ok())
        .collect();
    let (mut unknown, mut unread) = (Vec::new(), Vec::new());
    for key in keys_of(&String::from_utf8_lossy(&text)) {
        if parts.is_empty() {
            unread.push(key);
            continue;
        }
        let status = Command::new("grep")
            .args(["-rqsw", "--", &key])
            .args(&parts)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status.ok().and_then(|s| s.code()) {
            Some(0) => {}
            Some(1) => unknown.push(key),
            _ => unread.push(key),
        }
    }
    (unknown, unread)
}

/// A new link renamed over the old one, so current is never missing. The
/// history is what prune reads: the versions in the order they were switched to.
fn switch(host: &Host, tag: &str) -> bool {
    // The history goes first, so a failed append leaves `current` where it was.
    let state = Home::new(&host.home).state_dir();
    if fs::create_dir_all(&state).is_err() {
        return false;
    }
    use std::io::Write;
    if fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Home::new(&host.home).state(State::ReleaseHistory))
        .and_then(|mut f| writeln!(f, "{tag}"))
        .is_err()
    {
        return false;
    }
    let new = host.home.join("current.new");
    match fs::remove_file(&new) {
        Err(e) if e.kind() != ErrorKind::NotFound => return false,
        _ => {}
    }
    unix_fs::symlink(format!("versions/{tag}"), &new).is_ok()
        && fs::rename(&new, host.current()).is_ok()
}

/// The versions the last three switches named, newest first.
fn last_three(history: &str) -> Vec<&str> {
    let mut keep: Vec<&str> = Vec::new();
    for line in history.lines().rev() {
        if keep.len() == 3 {
            break;
        }
        if !keep.contains(&line) {
            keep.push(line);
        }
    }
    keep
}

fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

/// The version current names and the two switched to before it stay, with the
/// sandbox image each was tagged with. Anything older goes.
fn prune(host: &Host) {
    let history = fs::read(Home::new(&host.home).state(State::ReleaseHistory)).unwrap_or_default();
    let history = String::from_utf8_lossy(&history);
    let keep = last_three(&history);
    let versions = host.home.join("versions");
    let Ok(entries) = fs::read_dir(&versions) else {
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('v'))
        .collect();
    names.sort();
    for name in names {
        let dir = versions.join(&name);
        if !dir.is_dir() || keep.contains(&name.as_str()) || same_file(&dir, &host.current()) {
            continue;
        }
        // VERSION goes first: a tree it is left in is taken as whole, and a
        // removal that stops partway must not leave one that is not.
        match fs::remove_file(dir.join("VERSION")) {
            Err(e) if e.kind() != ErrorKind::NotFound => continue,
            _ => {}
        }
        if fs::remove_dir_all(&dir).is_ok() {
            say(&format!(
                "release: removed {name}, older than the last three"
            ));
        }
        quiet(Command::new("docker").args([
            "image".to_string(),
            "rm".to_string(),
            format!("{IMAGE_TAG}{name}"),
        ]));
    }
}

/// The image this version runs with keeps a tag of its own, so going back to it
/// later is a tag and not a build; prune drops the tags of versions it removes.
pub(crate) fn keep(host: &Host) {
    let version = installed(host);
    if !version.is_empty()
        && !quiet(timed(DOCKER_LIMIT, "docker").args([
            "tag",
            &host.image,
            &format!("{IMAGE_TAG}{version}"),
        ]))
    {
        say(&format!(
            "sandbox: {} could not be tagged for {version}; going back to it builds the image again",
            host.image
        ));
    }
    prune(host);
}

/// What `reuse_image` did.
#[derive(Debug, PartialEq)]
enum Reuse {
    Done,
    /// The image is there, built from another instance layer than the one
    /// the instance has now.
    OtherLayer,
    No,
}

/// What docker answers for one field of an image, or None.
fn inspect(image: &str, format: &str) -> Option<String> {
    let (rc, said) = output(
        timed(DOCKER_LIMIT, "docker")
            .args(["image", "inspect", "--format", format, image])
            .stderr(Stdio::null()),
    );
    (rc == 0).then_some(said)
}

fn image_id(image: &str) -> Option<String> {
    inspect(image, "{{.Id}}").filter(|id| !id.is_empty())
}

/// The sum of the instance's sandbox.Dockerfile, empty without one, None when
/// it cannot be read.
fn layer_now() -> Option<String> {
    let file = PathBuf::from(instance_dir()).join("sandbox.Dockerfile");
    if !file.exists() {
        return Some(String::new());
    }
    crate::sandbox::layer_sum(&file).ok()
}

/// Whether an image's tehas.layer label says it was built from the layer the
/// instance has now. An older docker prints `<no value>` for a missing label.
fn same_layer(label: &str, now: &str) -> bool {
    let label = if label == "<no value>" { "" } else { label };
    label == now
}

/// A version that ran here before has its sandbox image still tagged, so going
/// back to it is a tag and not a build, unless the instance's layer changed
/// since that image was built: then the build install.sh owes runs.
fn reuse_image(host: &Host, target: &str) -> Reuse {
    let tagged = format!("{IMAGE_TAG}{target}");
    let Some(label) = inspect(&tagged, "{{index .Config.Labels \"tehas.layer\"}}") else {
        return Reuse::No;
    };
    if !layer_now().is_some_and(|now| same_layer(&label, &now)) {
        return Reuse::OtherLayer;
    }
    if quiet(timed(DOCKER_LIMIT, "docker").args(["tag", &tagged, &host.image])) {
        Reuse::Done
    } else {
        Reuse::No
    }
}

/// The notes a person reads before moving: each release's own section of the
/// changelog, its Adopter action among them. Going back, the ones undone.
fn notes(target: &str, installed: &str, from_note: &str, list: &[Published]) {
    if target == installed {
        return;
    }
    let shown: Vec<&Published> = if !from_note.is_empty() && older(target, from_note) {
        say(&format!(
            "release: going back to {target} undoes these releases:"
        ));
        between(target, from_note, list).collect()
    } else {
        say(&format!("release: the notes up to {target}:"));
        between(from_note, target, list).collect()
    };
    for release in shown {
        say(&format!("\n== {}", release.tag));
        let text = indented(&release.body);
        say(text.trim_end_matches('\n'));
    }
    say("");
}

/// Download, check and unpack a version that is not on the host yet.
fn bring(site: Releases, host: &Host, target: &str, tree: &Path, arch: &str) -> u8 {
    let tmp = host.home.join("tmp");
    let _ = fs::create_dir_all(&tmp);
    let Ok(download) = temp_dir(&tmp, "release.") else {
        tell("release: cannot make a directory to download into; nothing was changed");
        return 1;
    };
    let tarball = tarball_of(target, arch);
    let brought = fetch(site, target, &download, &tarball)
        && verify(&download, &tarball)
        && unpack(host, target, &download, &tarball);
    let _ = fs::remove_dir_all(&download);
    if !brought {
        tell(&format!(
            "release: {target} was not installed; nothing was changed"
        ));
        return 1;
    }
    say(&format!(
        "release: {target} downloaded, checked against its .sha256 and its signature, unpacked in {}",
        tree.display()
    ));
    0
}

/// The version to run, unpacked beside the others and checked before current is
/// moved to it, so a download that fails a check, or a release that asks for a
/// setting this instance lacks, leaves the host on the version it runs.
pub(crate) fn go(site: Releases, host: &Host, to: Option<&str>, plan: &mut Plan) -> u8 {
    let check = plan.mode == Mode::Check;
    let installed = installed(host);
    if let Some(moved) = release::repo().and_then(|r| release::moved(&r, release::Holder::Setting))
    {
        say(&format!("release: {moved}"));
    }
    let list = match list(site) {
        Ok(list) => list,
        Err(rc) => return rc,
    };
    if let Some(to) = to {
        if !list.iter().any(|r| r.tag == to) {
            tell(&format!(
                "release: there is no published release {to}; nothing was changed"
            ));
            return 1;
        }
    }
    // A prerelease is never the newest: a host runs one only when it is named
    // with --to.
    let newest = newest(&list).unwrap_or_default().to_string();
    let mut target = match target(&list, to) {
        Some(target) => target.to_string(),
        None => {
            let named = repo().unwrap_or_default();
            tell(&format!(
                "release: {named} has no published release; nothing was changed"
            ));
            return 1;
        }
    };
    let newest_said = if newest.is_empty() { "none" } else { &newest };
    // Named here, before the download directory is made: nothing is written on a
    // machine that has no release to download.
    let arch = match arch() {
        Ok(arch) => arch,
        Err(words) => {
            fault(&words);
            return 1;
        }
    };
    // The notes start at the version the host came from: what is installed, or,
    // for a host that ran from its platform checkout, the tag of that checkout.
    // It has no installed version, so it still switches when the tag is the
    // newest. Without a tag the notes are every release.
    let mut from_note = installed.clone();
    let tag = if installed.is_empty() {
        checkout_tag(&host.checkout())
    } else {
        String::new()
    };
    if tag.is_empty() {
        let was = if installed.is_empty() {
            "none, this host ran from its platform checkout"
        } else {
            &installed
        };
        match ahead(&installed, &newest, &list) {
            Some(lines) if to.is_none() => lines.iter().for_each(|l| say(l)),
            _ => say(&format!("release: installed {was}, newest {newest_said}")),
        }
    } else {
        say(&format!(
            "release: installed none, this host ran from its platform checkout at {tag}, newest {newest_said}"
        ));
        from_note = tag;
    }
    // A checkout ahead of the newest release, on a prerelease, moves to the
    // release of its own tag: the newest would take it back.
    if to.is_none() && installed.is_empty() && !from_note.is_empty() && older(&newest, &from_note) {
        if !list.iter().any(|r| r.tag == from_note) {
            tell(&format!(
                "release: the checkout's {from_note} is newer than the newest release {newest} and is not published; tehas update --to names the release to move to; nothing was changed"
            ));
            return 1;
        }
        say(&format!(
            "release: the checkout's {from_note} is newer than the newest release; it moves to the release of its own tag"
        ));
        target = from_note.clone();
    }
    // A host ahead of the newest release stays where it is.
    if to.is_none() && !installed.is_empty() && older(&newest, &installed) {
        target = installed.clone();
    }
    // The prereleases of a version are read as that version's notes, so only
    // the one moved to or from is shown.
    let list: Vec<Published> = list
        .into_iter()
        .filter(|r| !prerelease(&r.tag) || r.tag == target || r.tag == from_note)
        .collect();
    notes(&target, &installed, &from_note, &list);

    let current = host.current();
    plan.tree = host.checkout().display().to_string();
    if check {
        if target != installed {
            say(&format!("release: would switch to {target}"));
            plan.found = true;
        }
        // What is installed is checked by the install.sh it came from.
        if fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
            plan.tree = current.display().to_string();
        } else if !host.checkout().join("install.sh").is_file() {
            // No install.sh to ask.
            plan.over = !plan.found;
            return if plan.found {
                tehas_core::exit::NO
            } else {
                tehas_core::exit::OK
            };
        }
        return 0;
    }
    if target == installed {
        say(&format!("release: {installed} is the version installed"));
        plan.tree = current.display().to_string();
        return 0;
    }
    let tree = host.home.join("versions").join(&target);
    if first_line(&tree.join("VERSION")) == target {
        say(&format!(
            "release: {target} is on this host from before; it is not downloaded again"
        ));
    } else {
        let rc = bring(site, host, &target, &tree, arch);
        if rc != 0 {
            return rc;
        }
    }
    match settings_hold(&tree, SETTINGS_LIMIT) {
        Held::Yes => {}
        Held::Missing => {
            tell(&format!(
                "settings: {target} asks for settings this instance does not have; nothing was switched"
            ));
            return 78;
        }
        Held::Unanswered => {
            tell(&format!(
                "settings: {target} did not say whether this instance has its settings; nothing was switched"
            ));
            return 1;
        }
    }
    let (unknown, unread) =
        unknown_keys(&tree, &PathBuf::from(instance_dir()).join("instance.env"));
    for key in unknown {
        tell(&format!(
            "WARN: {}/instance.env sets {key}, which {target} does not know",
            instance_dir()
        ));
    }
    if !unread.is_empty() {
        tell(&format!(
            "WARN: {} could not be searched for {}; whether {target} knows them is not checked",
            tree.display(),
            unread.join(", ")
        ));
    }
    if !switch(host, &target) {
        tell(&format!(
            "release: current could not be pointed at {target}; nothing was installed"
        ));
        return 1;
    }
    let was = if installed.is_empty() {
        "the platform checkout"
    } else {
        &installed
    };
    say(&format!("release: current is {target}, was {was}"));
    plan.tree = current.display().to_string();
    let mut undo = Undo {
        to: target.clone(),
        was: installed.clone(),
        image: None,
    };
    let before = image_id(&host.image);
    match reuse_image(host, &target) {
        Reuse::Done => {
            say(&format!(
                "sandbox: the image {target} ran with is in use again"
            ));
            plan.reused = true;
            undo.image = before;
        }
        Reuse::OtherLayer => say(&format!(
            "sandbox: the image {target} ran with was built from another sandbox.Dockerfile, so it is not put back"
        )),
        Reuse::No => {}
    }
    plan.undo = Some(undo);
    0
}

/// Current and the uid tag as they were before `go` moved them.
pub(crate) fn undo(host: &Host, undo: &Undo) {
    let Undo { to, was, image } = undo;
    let back = if was.is_empty() {
        !matches!(fs::remove_file(host.current()), Err(e) if e.kind() != ErrorKind::NotFound)
    } else {
        switch(host, was)
    };
    let named = if was.is_empty() {
        "the platform checkout"
    } else {
        was
    };
    if back {
        say(&format!(
            "release: install.sh did not finish {to}, so current is {named} again"
        ));
    } else {
        tell(&format!(
            "release: install.sh did not finish {to}, and current could not be put back to {named}; it still names {to}"
        ));
    }
    if let Some(id) = image {
        if !quiet(timed(DOCKER_LIMIT, "docker").args(["tag", id, &host.image])) {
            tell(&format!(
                "sandbox: {} could not be put back on {id}; it names the image {to} ran with",
                host.image
            ));
        }
    }
}

/// `console update release <question>`. The two that compare answer by status
/// alone: 0 yes, 1 no.
pub fn verb(given: &[String], site: Releases) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    match words[..] {
        ["older", a, b] => return u8::from(!older(a, b)),
        ["stale", newest, installed] => return u8::from(!stale(newest, installed)),
        _ => {}
    }
    let host = match host_or_refuse() {
        Ok(host) => host,
        Err(rc) => return rc,
    };
    match words[..] {
        ["source"] => match source(&host) {
            Ok(from) => {
                say(from);
                0
            }
            Err(rc) => rc,
        },
        ["installed"] => {
            let version = installed(&host);
            if !version.is_empty() {
                say(&version);
            }
            0
        }
        ["newest"] => match list(site) {
            Ok(list) => {
                if let Some(tag) = newest(&list) {
                    say(tag);
                }
                0
            }
            Err(rc) => rc,
        },
        ["keep"] => {
            keep(&host);
            0
        }
        _ => {
            tell("console update release: usage: source | installed | newest | keep | older <a> <b> | stale <newest> <installed>");
            64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;
    use serde_json::json;

    fn host(home: &Path) -> Host {
        Host {
            home: home.to_path_buf(),
            image: "tehas/pi-rukas:uid1000".to_string(),
            from: String::new(),
        }
    }

    /// A list as GitHub's answer gives it, read the way the code host's
    /// releases are kept: what `published_of` keeps of it.
    fn of(answer: &serde_json::Value) -> Vec<Published> {
        let all = answer.as_array().unwrap().iter().map(|r| {
            let tag = r["tag_name"].as_str().unwrap();
            let link = tehas_ports::WebLink::new(format!("https://example.test/{tag}")).unwrap();
            let mut got = Release::new(
                tehas_ports::ReleaseId::new(tag.to_string()),
                TagName::new(tag).unwrap(),
                link,
            );
            got.draft = r["draft"].as_bool().unwrap_or(false);
            got.prerelease = r["prerelease"].as_bool().unwrap_or(false);
            got.notes = r["body"].as_str().unwrap_or_default().to_string();
            got
        });
        published_of(all.collect())
    }

    fn rel(tag: &str) -> Published {
        Published {
            tag: tag.to_string(),
            body: format!("body of {tag}"),
        }
    }

    #[test]
    fn an_image_is_reused_only_when_built_from_the_layer_the_instance_has_now() {
        assert!(same_layer("", ""));
        assert!(same_layer("<no value>", ""));
        assert!(same_layer("sha256:a", "sha256:a"));
        assert!(!same_layer("sha256:a", "sha256:b"));
        assert!(!same_layer("", "sha256:b"));
        assert!(!same_layer("sha256:a", ""));
    }

    #[test]
    fn a_version_is_vxyz_or_a_prerelease_of_one_and_nothing_else() {
        for good in [
            "v1.2.3",
            "v0.0.0",
            "v10.20.30",
            "v0.22.0-rc.1",
            "v0.22.0-rc.10",
            "v1.0.0-alpha.1",
            "v1.0.0-beta.3",
        ] {
            assert!(tag_ok(good), "{good}");
        }
        for bad in [
            "",
            "1.2.3",
            "v1.2",
            "v1.2.3.4",
            "v1.2.x",
            "v1..3",
            "V1.2.3",
            "v1.2.3-rc1",
            "v1.2.3-rc.",
            "v1.2.3-rc.x",
            "v1.2.3-rc.1.2",
            "v1.2.3-rc.1-rc.2",
            "v1.2.3-RC.1",
            "v1.2-rc.1",
            "v1.2.3-gamma.1",
            "v1.2.3-alpha",
            "v1.2.3-Beta.1",
            "v1.2.3\n",
        ] {
            assert!(!tag_ok(bad), "{bad:?}");
        }
        assert!(prerelease("v0.22.0-rc.1"));
        assert!(prerelease("v1.0.0-alpha.1"));
        assert!(prerelease("v1.0.0-beta.1"));
        assert!(!prerelease("v0.22.0"));
        assert!(!prerelease("v0.22-rc.1"));
    }

    #[test]
    fn alpha_comes_before_beta_before_rc_before_the_release() {
        let order = [
            "v0.30.7",
            "v1.0.0-alpha.1",
            "v1.0.0-alpha.2",
            "v1.0.0-alpha.10",
            "v1.0.0-beta.1",
            "v1.0.0-beta.2",
            "v1.0.0-rc.1",
            "v1.0.0-rc.2",
            "v1.0.0",
            "v1.0.1-alpha.1",
            "v1.0.1",
        ];
        for (i, a) in order.iter().enumerate() {
            for (j, b) in order.iter().enumerate() {
                assert_eq!(older(a, b), i < j, "{a} {b}");
            }
        }
        // A host on a beta is behind its release, and ahead of the one before.
        assert!(!stale("v1.0.0", "v1.0.0-beta.1"));
        assert!(stale("v0.30.7", "v1.0.0-beta.1"));
    }

    #[test]
    fn a_candidate_comes_before_its_version_and_after_the_one_before() {
        assert!(older("v0.22.0-rc.1", "v0.22.0"));
        assert!(!older("v0.22.0", "v0.22.0-rc.1"));
        assert!(older("v0.22.0-rc.1", "v0.22.0-rc.2"));
        assert!(older("v0.22.0-rc.9", "v0.22.0-rc.10"));
        assert!(!older("v0.22.0-rc.10", "v0.22.0-rc.9"));
        assert!(older("v0.21.9", "v0.22.0-rc.1"));
        assert!(older("v0.22.0-rc.3", "v0.22.1-rc.1"));
        assert!(!older("v0.22.0-rc.1", "v0.22.0-rc.1"));
        // A host on a candidate is behind its version, and ahead of the one before.
        assert!(!stale("v0.22.0", "v0.22.0-rc.1"));
        assert!(stale("v0.21.0", "v0.22.0-rc.1"));
    }

    #[test]
    fn versions_order_by_number_not_by_text() {
        assert!(older("v1.9.0", "v1.10.0"));
        assert!(!older("v1.10.0", "v1.9.0"));
        assert!(older("v0.9.9", "v1.0.0"));
        assert!(older("v1.2.3", "v1.2.10"));
        // Digits of any length.
        assert!(older(
            "v0.0.99999999999999999999999999999999999999999",
            "v0.1.0"
        ));
        assert!(older(
            "v0.0.99999999999999999999999999999999999999999",
            "v0.0.100000000000000000000000000000000000000000"
        ));
        assert!(!older("v1.2.3", "v1.2.3"));
        // What sort -V does with a version that is not one.
        assert!(older("v1.2.3", "v1.2.3a"));
        assert!(older("v1.2.3~rc", "v1.2.3"));
        assert!(older("v1.02", "v1.2"));
    }

    #[test]
    fn a_kept_newest_goes_when_it_says_nothing() {
        assert!(!stale("v1.2.0", "v1.1.0"));
        assert!(!stale("v1.2.0", "v1.2.0"));
        assert!(stale("v1.0.0", "v1.1.0"));
        assert!(stale("garbage", "v1.1.0"));
        assert!(stale("v1.2.0", "dev"));
    }

    #[test]
    fn the_list_keeps_published_versions_newest_first() {
        let answer = json!([
            {"tag_name": "v1.9.0", "body": "nine"},
            {"tag_name": "v1.10.0", "body": null},
            {"tag_name": "v2.0.0", "draft": true},
            {"tag_name": "v3.0.0", "prerelease": true},
            {"tag_name": "nightly", "body": "no version"},
            {"tag_name": "v1.8.0"},
        ]);
        let list = of(&answer);
        let tags: Vec<&str> = list.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(tags, ["v1.10.0", "v1.9.0", "v1.8.0"]);
        assert_eq!(list[1].body, "nine");
        assert_eq!(list[0].body, "");
    }

    #[test]
    fn a_prerelease_of_any_stage_is_listed_but_never_the_newest() {
        let answer = json!([
            {"tag_name": "v0.30.7"},
            {"tag_name": "v1.0.0-alpha.1", "prerelease": true},
            {"tag_name": "v1.0.0-beta.1", "prerelease": true},
            {"tag_name": "v1.0.0-alpha.2", "prerelease": true},
            {"tag_name": "v1.0.0-rc.1", "prerelease": true},
        ]);
        let list = of(&answer);
        let tags: Vec<&str> = list.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(
            tags,
            [
                "v1.0.0-rc.1",
                "v1.0.0-beta.1",
                "v1.0.0-alpha.2",
                "v1.0.0-alpha.1",
                "v0.30.7"
            ]
        );
        assert_eq!(newest(&list), Some("v0.30.7"));
    }

    #[test]
    fn an_ordinary_update_skips_a_prerelease_and_a_named_one_is_taken() {
        let list = [
            rel("v1.0.0-beta.1"),
            rel("v1.0.0-alpha.1"),
            rel("v0.30.7"),
            rel("v0.30.6"),
        ];
        assert_eq!(target(&list, None), Some("v0.30.7"));
        assert_eq!(
            target(&list, Some("v1.0.0-alpha.1")),
            Some("v1.0.0-alpha.1")
        );
        assert_eq!(target(&list, Some("v0.30.6")), Some("v0.30.6"));
        assert_eq!(target(&list, Some("v1.0.0-alpha.2")), None);
        assert_eq!(target(&list[..2], None), None);
        assert_eq!(target(&[], None), None);
    }

    #[test]
    fn a_prerelease_ahead_of_the_newest_release_is_not_offered_the_older_one() {
        assert!(older("v0.32.0", "v1.0.0-rc.2"));
        assert!(older("v1.0.0-rc.3", "v1.0.0"));
        let list = [
            rel("v1.1.0-rc.1"),
            rel("v1.0.0-rc.3"),
            rel("v1.0.0-rc.2"),
            rel("v0.32.0"),
        ];
        assert_eq!(
            ahead("v1.0.0-rc.2", "v0.32.0", &list),
            Some(vec![
                "release: installed v1.0.0-rc.2, a prerelease ahead of the newest release v0.32.0"
                    .to_string(),
                "release: the newest prerelease of v1.0.0 is v1.0.0-rc.3; tehas update --to v1.0.0-rc.3 moves to it"
                    .to_string(),
            ])
        );
        assert_eq!(
            ahead("v1.0.0-rc.3", "v0.32.0", &list),
            Some(vec![
                "release: installed v1.0.0-rc.3, a prerelease ahead of the newest release v0.32.0"
                    .to_string(),
                "release: v1.0.0-rc.3 is the newest prerelease of v1.0.0; a plain tehas update keeps it until a newer release is out"
                    .to_string(),
            ])
        );
        assert_eq!(
            ahead("v1.5.0", "v0.32.0", &list),
            Some(vec![
                "release: installed v1.5.0, ahead of the newest release v0.32.0".to_string(),
                "release: a plain tehas update keeps it until a newer release is out".to_string(),
            ])
        );
        assert_eq!(ahead("v1.0.0-rc.2", "", &list[..3]), None);
        assert_eq!(ahead("v0.32.0", "v0.32.0", &list), None);
        assert_eq!(ahead("v0.31.0", "v0.32.0", &list), None);
        assert_eq!(ahead("v1.0.0-rc.2", "v1.0.0", &list), None);
        assert_eq!(ahead("", "v0.32.0", &list), None);
    }

    #[test]
    fn a_candidate_is_listed_but_never_the_newest() {
        let answer = json!([
            {"tag_name": "v0.21.0"},
            {"tag_name": "v0.22.0-rc.1", "prerelease": true, "body": "rc one"},
            {"tag_name": "v0.22.0-rc.2", "prerelease": true, "draft": true},
            {"tag_name": "v0.23.0", "prerelease": true},
            {"tag_name": "v0.22.0-rc.10", "prerelease": true},
        ]);
        let list = of(&answer);
        let tags: Vec<&str> = list.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(tags, ["v0.22.0-rc.10", "v0.22.0-rc.1", "v0.21.0"]);
        assert_eq!(newest(&list), Some("v0.21.0"));
        assert_eq!(newest(&list[..2]), None);
    }

    #[test]
    fn an_answer_that_is_no_list_is_an_error_and_no_answer_is_none() {
        use tehas_ports::fake::{CodeHostOp, FakeSite, Fault, Faulty, Op, World};
        use tehas_ports::{Caller, Site};
        assert_eq!(of(&json!([])), vec![]);
        // An answer the adapter cannot read whole is `Unread`, on either
        // handle: the list fails, never reads as empty.
        let site = Faulty::new(FakeSite::new(&World::new()))
            .on(Op::CodeHost(CodeHostOp::Releases), Fault::Malformed);
        let site = move |by: &Actor, _: &str, _: Duration| site.code_host(by, Caller::new("update"));
        assert_eq!(list(&site), Err(1));
        let none = FakeSite::new(&World::new());
        let none = move |by: &Actor, _: &str, _: Duration| none.code_host(by, Caller::new("update"));
        assert_eq!(list(&none), Ok(vec![]));
    }

    #[test]
    fn the_notes_between_run_in_both_directions() {
        let list = [rel("v1.3.0"), rel("v1.2.0"), rel("v1.1.0"), rel("v1.0.0")];
        let tags =
            |from, to| -> Vec<String> { between(from, to, &list).map(|r| r.tag.clone()).collect() };
        // Forward: after what is installed, up to the target, oldest first.
        assert_eq!(tags("v1.0.0", "v1.2.0"), ["v1.1.0", "v1.2.0"]);
        // Back: the caller asks from the target to where the host is.
        assert_eq!(tags("v1.1.0", "v1.3.0"), ["v1.2.0", "v1.3.0"]);
        // No version to start from: every release up to the target.
        assert_eq!(tags("", "v1.1.0"), ["v1.0.0", "v1.1.0"]);
        assert!(tags("v1.3.0", "v1.3.0").is_empty());
    }

    #[test]
    fn a_body_is_read_indented_with_its_blank_lines() {
        assert_eq!(indented("a\nb"), "  a\n  b\n");
        assert_eq!(indented("a\n"), "  a\n  \n");
        assert_eq!(indented(""), "  \n");
        assert_eq!(indented("a\r\nb"), "  a\n  b\n");
    }

    #[test]
    fn a_body_reaches_the_terminal_without_control_characters() {
        assert_eq!(
            indented("a\u{1b}]0;x\u{7}b\u{1b}[2J\u{9b}c\u{202e}d\te"),
            "  a]0;xb[2Jcd\te\n"
        );
    }

    #[test]
    fn the_asset_is_named_by_the_machine() {
        assert_eq!(arch_of("x86_64"), Some("x86_64"));
        assert_eq!(arch_of("amd64"), Some("x86_64"));
        assert_eq!(arch_of("aarch64"), None);
        assert_eq!(arch_of("arm64"), None);
        assert_eq!(arch_of("riscv64"), None);
        assert_eq!(arch_of(""), None);
        assert_eq!(
            tarball_of("v1.2.0", "x86_64"),
            "tehas-v1.2.0-linux-x86_64.tar.gz"
        );
    }

    #[test]
    fn a_sum_file_names_the_tarball_and_nothing_else() {
        let t = "tehas-v1.2.0-linux-x86_64.tar.gz";
        assert!(sum_names(&format!("abc  {t}\n"), t));
        assert!(sum_names(&format!("abc *{t}\n"), t));
        assert!(sum_names(&format!("abc  {t}\n\n"), t));
        assert!(!sum_names("abc  other.tar.gz\n", t));
        assert!(!sum_names(&format!("abc  {t}\nabc  other\n"), t));
        assert!(!sum_names(&format!("abc  other\nabc  {t}\n"), t));
        assert!(!sum_names(&format!("abc  **{t}\n"), t));
        assert!(!sum_names("", t));
    }

    #[test]
    fn a_tar_member_outside_the_release_is_refused() {
        let top = "tehas-v1.2.0/";
        let good = "drwxr-xr-x a/a 0 2026-01-01 00:00 tehas-v1.2.0/\n\
                    -rw-r--r-- a/a 7 2026-01-01 00:00 tehas-v1.2.0/VERSION\n";
        assert!(strangers(good, top).is_empty());
        let line = |kind: &str, name: &str| format!("{kind} a/a 7 2026-01-01 00:00 {name}\n");
        assert_eq!(strangers(&line("-rw-r--r--", "outside"), top), ["outside"]);
        assert_eq!(
            strangers(&line("-rw-r--r--", "tehas-v1.2.0/../x"), top),
            ["tehas-v1.2.0/../x"]
        );
        assert_eq!(
            strangers(&line("-rw-r--r--", "../tehas-v1.2.0/x"), top),
            ["../tehas-v1.2.0/x"]
        );
        assert_eq!(
            strangers(&line("-rw-r--r--", "/tehas-v1.2.0/x"), top),
            ["/tehas-v1.2.0/x"]
        );
        for kind in ["lrwxrwxrwx", "hrw-r--r--", "crw-r--r--", "prw-r--r--"] {
            assert_eq!(strangers(&line(kind, "tehas-v1.2.0/x"), top).len(), 1);
        }
        // A name that only starts like the top is not under it.
        assert_eq!(
            strangers(&line("-rw-r--r--", "tehas-v1.2.0x"), top).len(),
            1
        );
    }

    // A download as a release gives it, a key on the host that is not the
    // release key, and a minisign that answers `rc` when it is handed the key
    // built into tehas and 1 for any other.
    fn signed(d: &super::super::testing::Dir, rc: u8) -> (String, PathBuf) {
        let t = "tehas-v1.2.0-linux-x86_64.tar.gz".to_string();
        d.file(format!("dl/{t}"), "a release\n");
        let (_, sum) = output(
            Command::new("sha256sum")
                .arg(&t)
                .current_dir(d.0.join("dl")),
        );
        d.file(format!("dl/{t}.sha256"), &format!("{sum}\n"));
        d.file(format!("dl/{t}.minisig"), "signed\n");
        d.file("home/release/minisign.pub", "a key a release left\n");
        d.file("built-in.pub", release::KEY);
        let stub = d.0.join("minisign");
        let script = format!(
            "#!/bin/sh\nk=\nwhile [ $# -gt 0 ]; do [ \"$1\" != -p ] || k=$2; shift; done\ncmp -s \"$k\" '{}' || exit 1\nexit {rc}\n",
            d.0.join("built-in.pub").display()
        );
        tehas_core::testing::write_exe(&stub, script).unwrap();
        (t, stub)
    }

    #[test]
    fn a_download_passes_only_with_both_checks() {
        let d = dir("verify-good");
        let (t, stub) = signed(&d, 0);
        assert!(verify_with(&d.0.join("dl"), &t, stub.to_str().unwrap()));
    }

    #[test]
    fn no_minisign_fails_closed() {
        let d = dir("verify-none");
        let (t, _) = signed(&d, 0);
        assert!(!verify_with(&d.0.join("dl"), &t, "/no/such/minisign"));
    }

    #[test]
    fn a_signature_the_key_refuses_fails_closed() {
        let d = dir("verify-bad-sig");
        let (t, stub) = signed(&d, 1);
        assert!(!verify_with(&d.0.join("dl"), &t, stub.to_str().unwrap()));
    }

    #[test]
    fn a_sum_that_does_not_match_stops_before_the_signature() {
        let d = dir("verify-bad-sum");
        let (t, stub) = signed(&d, 0);
        d.file(format!("dl/{t}"), "tampered\n");
        assert!(!verify_with(&d.0.join("dl"), &t, stub.to_str().unwrap()));
    }

    #[test]
    fn a_missing_file_fails_closed() {
        for gone in ["tar.gz", "tar.gz.sha256", "tar.gz.minisig"] {
            let d = dir(&format!("verify-{gone}"));
            let (t, stub) = signed(&d, 0);
            let name = t.replace("tar.gz", gone);
            fs::remove_file(d.0.join("dl").join(name)).unwrap();
            assert!(!verify_with(&d.0.join("dl"), &t, stub.to_str().unwrap()));
        }
    }

    #[test]
    fn the_key_on_the_host_is_not_read() {
        let d = dir("verify-no-key");
        let (t, stub) = signed(&d, 0);
        fs::remove_file(d.0.join("home/release/minisign.pub")).unwrap();
        assert!(verify_with(&d.0.join("dl"), &t, stub.to_str().unwrap()));
    }

    #[test]
    fn prune_keeps_the_last_three_switched_to() {
        assert_eq!(last_three("v1\nv2\nv3\nv4\n"), ["v4", "v3", "v2"]);
        // A version switched to twice counts once, at its last place.
        assert_eq!(last_three("v1\nv2\nv3\nv2\nv2\n"), ["v2", "v3", "v1"]);
        assert!(last_three("").is_empty());
    }

    #[test]
    fn prune_removes_what_is_not_kept_and_never_current() {
        let d = dir("prune");
        for v in ["v1.0.0", "v1.1.0", "v1.2.0", "v1.3.0", "v1.4.0", "vault"] {
            d.file(format!("versions/{v}/VERSION"), v);
        }
        d.file("versions/.unpack-abc/x", "");
        d.file(
            Home::new(d.0.path()).state(State::ReleaseHistory),
            "v1.0.0\nv1.1.0\nv1.2.0\nv1.3.0\n",
        );
        unix_fs::symlink("versions/v1.0.0", d.0.join("current")).unwrap();
        prune(&host(&d.0));
        let mut left: Vec<String> = fs::read_dir(d.0.join("versions"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [".unpack-abc", "v1.0.0", "v1.1.0", "v1.2.0", "v1.3.0"]
        );
    }

    #[test]
    fn a_tree_prune_could_not_remove_whole_no_longer_says_its_version() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("prune-part");
        d.file("versions/v1.0.0/VERSION", "v1.0.0");
        d.file("versions/v1.0.0/stuck/file", "");
        d.file("versions/v1.1.0/VERSION", "v1.1.0");
        d.file(
            Home::new(d.0.path()).state(State::ReleaseHistory),
            "v1.1.0\n",
        );
        unix_fs::symlink("versions/v1.1.0", d.0.join("current")).unwrap();
        let stuck = d.0.join("versions/v1.0.0/stuck");
        fs::set_permissions(&stuck, fs::Permissions::from_mode(0o500)).unwrap();
        prune(&host(&d.0));
        fs::set_permissions(&stuck, fs::Permissions::from_mode(0o700)).ok();
        assert_eq!(first_line(&d.0.join("versions/v1.0.0/VERSION")), "");
        assert_eq!(first_line(&d.0.join("versions/v1.1.0/VERSION")), "v1.1.0");
    }

    #[test]
    fn the_installed_version_is_read_only_through_the_link() {
        let d = dir("installed");
        let h = host(&d.0);
        assert_eq!(installed(&h), "");
        d.file("versions/v1.0.0/VERSION", "v1.0.0\nmore\n");
        unix_fs::symlink("versions/v1.0.0", d.0.join("current")).unwrap();
        assert_eq!(installed(&h), "v1.0.0");
        fs::remove_file(d.0.join("current")).unwrap();
        fs::create_dir(d.0.join("current")).unwrap();
        d.file("current/VERSION", "v9.9.9\n");
        assert_eq!(installed(&h), "");
    }

    #[test]
    fn a_switch_moves_the_link_and_keeps_the_history() {
        let d = dir("switch");
        let h = host(&d.0);
        d.file("versions/v1.0.0/VERSION", "v1.0.0\n");
        d.file("versions/v1.1.0/VERSION", "v1.1.0\n");
        unix_fs::symlink("versions/v1.0.0", d.0.join("current")).unwrap();
        unix_fs::symlink("versions/stale", d.0.join("current.new")).unwrap();
        assert!(switch(&h, "v1.1.0"));
        assert_eq!(
            fs::read_link(d.0.join("current")).unwrap(),
            Path::new("versions/v1.1.0")
        );
        assert!(!d.0.join("current.new").exists());
        assert_eq!(
            fs::read_to_string(Home::new(d.0.path()).state(State::ReleaseHistory)).unwrap(),
            "v1.1.0\n"
        );
    }

    #[test]
    fn a_history_that_cannot_be_written_leaves_current_where_it_was() {
        let d = dir("switch-history");
        let h = host(&d.0);
        d.file("versions/v1.0.0/VERSION", "v1.0.0\n");
        d.file("versions/v1.1.0/VERSION", "v1.1.0\n");
        unix_fs::symlink("versions/v1.0.0", d.0.join("current")).unwrap();
        fs::create_dir_all(Home::new(d.0.path()).state(State::ReleaseHistory)).unwrap();
        assert!(!switch(&h, "v1.1.0"));
        assert_eq!(
            fs::read_link(d.0.join("current")).unwrap(),
            Path::new("versions/v1.0.0")
        );
    }

    #[test]
    fn a_missing_setting_is_named_with_its_example() {
        let said = "tehas: TEHAS_NEW_KNOB is not set (see /i/instance.env)\nsomething else";
        assert_eq!(not_set(said), ["TEHAS_NEW_KNOB"]);
        let example = "# TEHAS_OTHER=1\n#  TEHAS_NEW_KNOB=42\n";
        assert_eq!(
            example_line(example, "TEHAS_NEW_KNOB").as_deref(),
            Some("TEHAS_NEW_KNOB=42")
        );
        assert_eq!(
            example_line("TEHAS_NEW_KNOB=1\n", "TEHAS_NEW_KNOB").as_deref(),
            Some("TEHAS_NEW_KNOB=1")
        );
        assert_eq!(example_line("TEHAS_NEW_KNOBS=1\n", "TEHAS_NEW_KNOB"), None);
    }

    fn config(d: &super::super::testing::Dir, body: &str) {
        d.file(
            "lib/config.sh",
            &format!("tehas_load() {{ :; }}\ntehas_require_instance_settings() {{ {body}; }}\n"),
        );
    }

    #[test]
    fn the_targets_settings_check_holds_or_names_what_is_missing() {
        let d = dir("settings-hold");
        config(&d, "true");
        assert_eq!(settings_hold(&d.0, 30), Held::Yes);
        config(&d, "echo 'tehas: TEHAS_NEW_KNOB is not set' >&2; return 78");
        assert_eq!(settings_hold(&d.0, 30), Held::Missing);
    }

    #[test]
    fn a_settings_check_that_never_stops_talking_is_no_answer() {
        let d = dir("settings-loud");
        config(&d, "yes 'tehas: TEHAS_A is not set'");
        let started = std::time::Instant::now();
        assert_eq!(settings_hold(&d.0, 60), Held::Unanswered);
        assert!(started.elapsed().as_secs() < 30, "{:?}", started.elapsed());
        // Exiting 0 after too many words is no answer either.
        config(&d, "head -c 100000 /dev/zero | tr '\\0' x; return 0");
        assert_eq!(settings_hold(&d.0, 60), Held::Unanswered);
    }

    #[test]
    fn a_settings_check_that_does_not_answer_in_time_is_no_answer() {
        let d = dir("settings-slow");
        config(&d, "sleep 30");
        let started = std::time::Instant::now();
        assert_eq!(settings_hold(&d.0, 1), Held::Unanswered);
        assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
    }

    #[test]
    fn a_key_is_unknown_only_when_the_tree_could_be_searched() {
        let d = dir("unknown-keys");
        let env_file = d.file("instance.env", "TEHAS_KNOWN=1\nTEHAS_GONE=2\n");
        // Only lib/ is there: the parts a release lacks are not an error.
        d.file("tree/lib/config.sh", "TEHAS_KNOWN\n");
        let tree = d.0.join("tree");
        let found = unknown_keys(&tree, &env_file);
        assert_eq!(found, (vec!["TEHAS_GONE".to_string()], vec![]));
        // A tree with nothing to search is not read as every key unknown.
        let empty = d.0.join("empty");
        fs::create_dir_all(&empty).unwrap();
        let found = unknown_keys(&empty, &env_file);
        assert_eq!(
            found,
            (
                vec![],
                vec!["TEHAS_GONE".to_string(), "TEHAS_KNOWN".to_string()]
            )
        );
    }

    #[test]
    fn the_keys_of_an_instance_are_its_assignments() {
        let text = "TEHAS_B=1\n  export TEHAS_A=2\nexport\tTEHAS_C=3\n# TEHAS_D=4\nOTHER=5\nTEHAS_B=6\nTEHAS_lower=7\nexportTEHAS_E=8\n";
        assert_eq!(keys_of(text), ["TEHAS_A", "TEHAS_B", "TEHAS_C"]);
    }

    #[test]
    fn the_repository_is_the_setting_or_the_built_in_one() {
        // The host's versions.env, which a release writes, is not an input.
        assert_eq!(release::repo_of(""), release::pinned_repo());
        assert_eq!(release::repo_of("o/n"), "o/n");
    }

    mod on_the_code_host {
        //! The signed-in list and download, on the ports' fake.

        use super::*;
        use tehas_ports::fake::{CodeHostOp, FakeSite, Fault, Faulty, Op, World};
        use tehas_ports::{Actor, Caller, ReleaseId, Site, WebLink};

        fn release(tag: &str, draft: bool, pre: bool, notes: &str) -> Release {
            let link = WebLink::new(format!("https://example.test/{tag}")).unwrap();
            let mut r = Release::new(
                ReleaseId::new(tag.to_string()),
                TagName::new(tag).unwrap(),
                link,
            );
            (r.draft, r.prerelease, r.notes) = (draft, pre, notes.to_string());
            r
        }

        #[test]
        fn the_list_keeps_what_the_public_list_keeps_newest_by_version_first() {
            let all = vec![
                release("v1.0.0", false, false, "one"),
                release("v1.2.0", true, false, "a draft"),
                release("v1.1.0", false, false, ""),
                release("v1.1.1-rc.1", false, true, "rc"),
                release("v1.1.2", false, true, "a prerelease with a plain tag"),
                release("nightly", false, false, "no version"),
            ];
            let got = published_of(all);
            let tags: Vec<&str> = got.iter().map(|r| r.tag.as_str()).collect();
            assert_eq!(tags, ["v1.1.1-rc.1", "v1.1.0", "v1.0.0"]);
            assert_eq!(got[2].body, "one");
            assert_eq!(got[1].body, "");
        }

        const NAMES: [&str; 3] = ["t.tar.gz", "t.tar.gz.sha256", "t.tar.gz.minisig"];

        fn names() -> Vec<String> {
            NAMES.iter().map(|n| n.to_string()).collect()
        }

        /// A handle maker on `site` for the operator, that asserts the
        /// project asked and that the anonymous handle is never made.
        fn on(
            site: impl Site + 'static,
        ) -> impl Fn(&Actor, &str, Duration) -> Got<Box<dyn CodeHost + Send>> {
            move |by, repo, _| {
                assert_eq!((by, repo), (&Actor::Operator, "o/r"));
                site.code_host(by, Caller::new("update"))
            }
        }

        fn world() -> World {
            World::new().release(
                "v1.2.0",
                &[
                    (NAMES[0], b"tarball".as_slice()),
                    (NAMES[1], b"sum"),
                    (NAMES[2], b"signed"),
                ],
            )
        }

        #[test]
        fn each_file_of_the_release_is_written_whole_into_the_directory() {
            let d = dir("release-download");
            let site = on(FakeSite::new(&world()));
            assert!(download(&site, "o/r", "v1.2.0", &d.0, &names()));
            for (name, bytes) in [
                (NAMES[0], "tarball"),
                (NAMES[1], "sum"),
                (NAMES[2], "signed"),
            ] {
                assert_eq!(fs::read_to_string(d.0.join(name)).unwrap(), bytes);
            }
        }

        #[test]
        fn a_file_the_release_does_not_hold_stops_the_download() {
            let d = dir("release-download-missing");
            let world = World::new().release("v1.2.0", &[(NAMES[0], b"tarball".as_slice())]);
            let site = on(FakeSite::new(&world));
            assert!(!download(&site, "o/r", "v1.2.0", &d.0, &names()));
            assert!(!download(&site, "o/r", "v9.9.9", &d.0, &names()));
        }

        #[test]
        fn a_code_host_that_times_out_stops_the_list_and_the_download() {
            let d = dir("release-download-late");
            let late =
                |op| on(Faulty::new(FakeSite::new(&world())).on(Op::CodeHost(op), Fault::Timeout));
            let site = late(CodeHostOp::Download);
            assert!(!download(&site, "o/r", "v1.2.0", &d.0, &names()));
            let site = late(CodeHostOp::Releases);
            let read = site(&Actor::Operator, "o/r", Duration::ZERO)
                .and_then(|mut h| h.releases(ALL_MAX));
            assert!(read.is_err());
        }
    }

    mod the_fallback {
        //! The anonymous handle in the operator's place, on the ports' fake:
        //! when the operator's handle, made or asked first, is signed out,
        //! forbidden or has no gh to run, and never on a failure of the
        //! site's (tehas-68, 2026-11-25).

        use super::*;
        use std::cell::RefCell;
        use tehas_ports::fake::{CodeHostOp, FakeSite, Fault, Faulty, Op, World};
        use tehas_ports::{Caller, Site};

        const NAMES: [&str; 3] = ["t.tar.gz", "t.tar.gz.sha256", "t.tar.gz.minisig"];

        fn names() -> Vec<String> {
            NAMES.iter().map(|n| n.to_string()).collect()
        }

        fn world() -> World {
            World::new().release(
                "v1.2.0",
                &[
                    (NAMES[0], b"tarball".as_slice()),
                    (NAMES[1], b"sum"),
                    (NAMES[2], b"signed"),
                ],
            )
        }

        /// How the operator's handle answers.
        enum Operator {
            /// Its making fails with this.
            Made(Outcome),
            /// Its first call of `op` fails with `fault`, and so does each.
            Asked(CodeHostOp, Fault),
        }

        /// A handle maker: the operator's as `operator` says, the anonymous
        /// one on the whole fake; `asked` keeps each actor a handle was made
        /// for, in order.
        fn maker(
            operator: Operator,
            asked: &RefCell<Vec<Actor>>,
        ) -> impl Fn(&Actor, &str, Duration) -> Got<Box<dyn CodeHost + Send>> + '_ {
            move |by, repo, _| {
                assert_eq!(repo, "o/r");
                asked.borrow_mut().push(by.clone());
                let caller = Caller::new("update");
                match (by, &operator) {
                    (Actor::Operator, Operator::Made(o)) => Err(o.clone()),
                    (Actor::Operator, Operator::Asked(op, fault)) => {
                        Faulty::new(FakeSite::new(&world()))
                            .on(Op::CodeHost(*op), *fault)
                            .code_host(by, caller)
                    }
                    (Actor::Anonymous, _) => FakeSite::new(&world()).code_host(by, caller),
                    (other, _) => panic!("no handle is made as {other:?}"),
                }
            }
        }

        fn written(d: &Path) -> bool {
            [
                (NAMES[0], "tarball"),
                (NAMES[1], "sum"),
                (NAMES[2], "signed"),
            ]
            .iter()
            .all(|(name, bytes)| fs::read_to_string(d.join(name)).is_ok_and(|b| b == *bytes))
        }

        #[test]
        fn a_signed_out_operator_handle_reads_and_downloads_anonymously() {
            let d = dir("release-fallback-signed-out");
            let asked = RefCell::new(Vec::new());
            let site = maker(Operator::Made(Outcome::SignedOut), &asked);
            assert!(download(&site, "o/r", "v1.2.0", &d.0, &names()));
            assert!(written(&d.0));
            assert_eq!(*asked.borrow(), [Actor::Operator, Actor::Anonymous]);
            // The list too, when its first call answers signed out.
            let asked = RefCell::new(Vec::new());
            let site = maker(
                Operator::Asked(CodeHostOp::Releases, Fault::SignedOut),
                &asked,
            );
            let got = first(&site, "o/r", Duration::ZERO, |h| h.releases(ALL_MAX));
            assert!(got.is_ok());
            assert_eq!(*asked.borrow(), [Actor::Operator, Actor::Anonymous]);
        }

        #[test]
        fn a_forbidden_first_call_downloads_every_file_anonymously() {
            let d = dir("release-fallback-forbidden");
            let asked = RefCell::new(Vec::new());
            let site = maker(
                Operator::Asked(CodeHostOp::Download, Fault::Forbidden),
                &asked,
            );
            assert!(download(&site, "o/r", "v1.2.0", &d.0, &names()));
            assert!(written(&d.0));
            // One anonymous handle for all three files.
            assert_eq!(*asked.borrow(), [Actor::Operator, Actor::Anonymous]);
        }

        #[test]
        fn no_gh_to_run_sends_nothing_and_downloads_anonymously() {
            let d = dir("release-fallback-unreachable");
            let asked = RefCell::new(Vec::new());
            let no_gh = Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            };
            let site = maker(Operator::Made(no_gh), &asked);
            assert!(download(&site, "o/r", "v1.2.0", &d.0, &names()));
            assert!(written(&d.0));
            assert_eq!(*asked.borrow(), [Actor::Operator, Actor::Anonymous]);
            // Asked first, the same.
            let d = dir("release-fallback-unreachable-asked");
            let asked = RefCell::new(Vec::new());
            let site = maker(
                Operator::Asked(CodeHostOp::Download, Fault::Unreachable),
                &asked,
            );
            assert!(download(&site, "o/r", "v1.2.0", &d.0, &names()));
            assert_eq!(*asked.borrow(), [Actor::Operator, Actor::Anonymous]);
        }

        #[test]
        fn a_failure_of_the_site_or_one_that_may_have_been_sent_never_falls_back() {
            for (op, fault) in [
                (CodeHostOp::Download, Fault::Server { applied: false }),
                (CodeHostOp::Download, Fault::TimeoutMaybe { applied: false }),
                (CodeHostOp::Releases, Fault::Server { applied: false }),
            ] {
                let d = dir("release-fallback-server");
                let asked = RefCell::new(Vec::new());
                let site = maker(Operator::Asked(op, fault), &asked);
                let got = if op == CodeHostOp::Download {
                    download(&site, "o/r", "v1.2.0", &d.0, &names())
                } else {
                    first(&site, "o/r", Duration::ZERO, |h| h.releases(ALL_MAX)).is_ok()
                };
                assert!(!got, "{fault:?}");
                assert_eq!(*asked.borrow(), [Actor::Operator], "{fault:?}");
            }
            // Made with a server's failure, the same.
            let asked = RefCell::new(Vec::new());
            let server = Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            };
            let site = maker(Operator::Made(server), &asked);
            let d = dir("release-fallback-server-made");
            assert!(!download(&site, "o/r", "v1.2.0", &d.0, &names()));
            assert_eq!(*asked.borrow(), [Actor::Operator]);
        }

        #[test]
        fn a_refusal_after_the_first_file_does_not_change_the_handle() {
            // The first call decides: a later file's answer fails the update.
            let d = dir("release-fallback-later");
            let asked = RefCell::new(Vec::new());
            let world = World::new().release("v1.2.0", &[(NAMES[0], b"tarball".as_slice())]);
            let site = |by: &Actor, _: &str, _: Duration| {
                asked.borrow_mut().push(by.clone());
                FakeSite::new(&world).code_host(by, Caller::new("update"))
            };
            assert!(!download(&site, "o/r", "v1.2.0", &d.0, &names()));
            assert_eq!(*asked.borrow(), [Actor::Operator]);
        }

        #[test]
        fn the_anonymous_refusal_of_a_private_project_says_gh_is_needed() {
            let stopped = Stopped {
                anonymous: true,
                why: Outcome::Refused(tehas_ports::Refusal::Unsupported),
            };
            assert!(stopped.line().ends_with("(a private repository needs gh signed in)"));
            let signed = Stopped {
                anonymous: false,
                why: Outcome::Refused(tehas_ports::Refusal::Unsupported),
            };
            assert!(!signed.line().contains("gh signed in"));
        }
    }
}
