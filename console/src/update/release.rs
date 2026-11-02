//! The release half of `tehas update`: the releases there are, a download and
//! its two checks, the unpack beside the versions already on the host, the
//! instance checked against the target's own rules, and the switch of
//! ~/tehas/current.
//!
//! The key a signature is checked with is the one install.sh put on the host,
//! $TEHAS_HOME/release/minisign.pub, never one that arrived in the download it
//! is to check. Nothing is unpacked before both checks pass, and current moves
//! only after the unpack and the target's own settings check, so whatever
//! fails leaves the host on the version it runs.
//!
//! host/release/install repeats these checks in POSIX sh, because it runs on a
//! host with nothing installed yet.

use super::{
    fault, first_line, host_or_refuse, output, quiet, say, tell, temp_dir, timed, Host, Mode, Plan,
    ANSWERED,
};
use serde_json::Value;
use std::cmp::Ordering;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Seconds for each call out: a remote that stops answering fails the update.
const API_LIMIT: u32 = 60;
const DOWNLOAD_LIMIT: u32 = 600;
/// docker image inspect and docker tag.
const DOCKER_LIMIT: u32 = 180;
/// The name of the image a version ran with.
const IMAGE_TAG: &str = "tehas/pi-rukas:release-";

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// The shape of a published version: vX.Y.Z.
pub(crate) fn tag_ok(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| digits(p))
}

fn order(c: u8) -> i32 {
    match c {
        b'0'..=b'9' => 0,
        b'A'..=b'Z' | b'a'..=b'z' => i32::from(c),
        b'~' => -1,
        _ => i32::from(c) + 256,
    }
}

/// The order of `sort -V`, without its handling of file name suffixes.
fn version_order(a: &[u8], b: &[u8]) -> Ordering {
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
    version_order(x.as_bytes(), y.as_bytes())
        .then_with(|| x.cmp(y))
        .is_le()
}

/// A kept newest version that cannot be said: no version, or older than the one
/// the host runs, which no published list of that repository holds.
pub(crate) fn stale(newest: &str, installed: &str) -> bool {
    !tag_ok(newest) || !tag_ok(installed) || older(newest, installed)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Published {
    pub tag: String,
    pub body: String,
}

fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Null | Value::Bool(false))
}

/// Published vX.Y.Z releases only, newest first by version, not by date. An
/// answer that is not a list of releases is an error, and no answer is none.
fn published(text: &str) -> Result<Vec<Published>, ()> {
    let mut found = Vec::new();
    for answer in serde_json::Deserializer::from_str(text).into_iter::<Value>() {
        let answer = answer.map_err(|_| ())?;
        let items: Vec<&Value> = match &answer {
            Value::Array(items) => items.iter().collect(),
            Value::Object(items) => items.values().collect(),
            _ => return Err(()),
        };
        for item in items {
            let release = item.as_object().ok_or(())?;
            let held = |name: &str| release.get(name).is_some_and(truthy);
            if held("draft") || held("prerelease") {
                continue;
            }
            let tag = release.get("tag_name").and_then(Value::as_str).ok_or(())?;
            if !tag_ok(tag) {
                continue;
            }
            let body = match release.get("body") {
                Some(Value::String(body)) => body.clone(),
                Some(v) if truthy(v) => v.to_string(),
                _ => String::new(),
            };
            found.push(Published {
                tag: tag.to_string(),
                body,
            });
        }
    }
    found.sort_by(|a, b| version_order(a.tag.as_bytes(), b.tag.as_bytes()));
    found.reverse();
    Ok(found)
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

/// A body as a person reads it under its version.
fn indented(body: &str) -> String {
    let text = format!("{body}\n");
    text.split_inclusive('\n')
        .map(|line| format!("  {line}"))
        .collect()
}

/// The architecture in the name of an asset: releases are published for linux
/// x86_64 and aarch64 only. arm64 is what some ports answer for aarch64.
fn arch_of(machine: &str) -> Option<&'static str> {
    match machine {
        "x86_64" | "amd64" => Some("x86_64"),
        "aarch64" | "arm64" => Some("aarch64"),
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
        format!("this machine has no tehas release to download: releases are published for x86_64 and aarch64, and uname -m says {said}")
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
    match env::var("TEHAS_UPDATE_FROM").as_deref() {
        Ok("git") => return Ok("git"),
        Ok("release") => return Ok("release"),
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

fn repo_ok(repo: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    };
    repo.split_once('/')
        .is_some_and(|(a, b)| part(a) && part(b))
}

/// The instance's setting, else the line of the installed versions.env, which a
/// release carries. Read as text: versions.env is not run.
fn repo_named<'a>(setting: &'a str, versions: &'a str) -> &'a str {
    if !setting.is_empty() {
        return setting;
    }
    versions
        .lines()
        .filter_map(|l| l.strip_prefix("TEHAS_RELEASE_REPO="))
        .next_back()
        .unwrap_or("")
}

fn repo(host: &Host) -> Result<String, u8> {
    let setting = env::var("TEHAS_RELEASE_REPO").unwrap_or_default();
    let versions = fs::read(host.home.join("versions.env")).unwrap_or_default();
    let versions = String::from_utf8_lossy(&versions);
    let repo = repo_named(&setting, &versions);
    if repo_ok(repo) {
        return Ok(repo.to_string());
    }
    fault(&format!(
        "no release repository: TEHAS_RELEASE_REPO is in neither {}/instance.env nor {}/versions.env",
        instance_dir(),
        host.home.display()
    ));
    Err(78)
}

/// gh when it is signed in, which reaches a private repository with the bot's
/// token; the public address otherwise.
fn signed_in() -> bool {
    quiet(timed(API_LIMIT, "gh").args(["auth", "status"]))
}

/// The published releases, newest first.
pub(crate) fn list(host: &Host) -> Result<Vec<Published>, u8> {
    let repo = repo(host)?;
    let answer = if signed_in() {
        let (rc, out) = output(timed(API_LIMIT, "gh").args([
            "api".to_string(),
            format!("repos/{repo}/releases?per_page=100"),
        ]));
        if rc != 0 {
            fault(&format!("gh could not list the releases of {repo}"));
            return Err(1);
        }
        out
    } else {
        let (rc, out) = output(timed(API_LIMIT, "curl").args([
            "-fsSL".to_string(),
            "-H".to_string(),
            "Accept: application/vnd.github+json".to_string(),
            format!("https://api.github.com/repos/{repo}/releases?per_page=100"),
        ]));
        if rc != 0 {
            fault(&format!(
                "could not list the releases of {repo} (a private repository needs gh signed in)"
            ));
            return Err(1);
        }
        out
    };
    published(&answer).map_err(|()| {
        fault(&format!(
            "the release list of {repo} is not what GitHub answers"
        ));
        1
    })
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

fn fetch(host: &Host, tag: &str, dir: &Path, tarball: &str) -> bool {
    let Ok(repo) = repo(host) else {
        return false;
    };
    let names = [
        tarball.to_string(),
        format!("{tarball}.sha256"),
        format!("{tarball}.minisig"),
    ];
    if signed_in() {
        let mut cmd = timed(DOWNLOAD_LIMIT, "gh");
        cmd.args(["release", "download", tag, "--repo", &repo, "--dir"])
            .arg(dir);
        for name in &names {
            cmd.args(["--pattern", name]);
        }
        let done = cmd
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !done {
            fault(&format!("gh could not download {tag} of {repo}"));
        }
        return done;
    }
    for name in &names {
        let done = timed(DOWNLOAD_LIMIT, "curl")
            .args(["-fsSL", "-o"])
            .arg(dir.join(name))
            .arg(format!(
                "https://github.com/{repo}/releases/download/{tag}/{name}"
            ))
            .status()
            .is_ok_and(|s| s.success());
        if !done {
            fault(&format!("could not download {name} of {repo}"));
            return false;
        }
    }
    true
}

/// Both checks of a download; false when either cannot say yes.
fn verify(host: &Host, dir: &Path, tarball: &str) -> bool {
    verify_with(host, dir, tarball, "minisign")
}

fn verify_with(host: &Host, dir: &Path, tarball: &str, minisign: &str) -> bool {
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
    let key = host.home.join("release/minisign.pub");
    if fs::File::open(&key).is_err() {
        fault(&format!(
            "no key to check a signature with at {} (install.sh puts it there)",
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
                "{tarball} is not signed by the key in {}",
                key.display()
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

/// The rules of the version about to run, not of the one running now: its own
/// lib/config.sh, as a shell runs it. A setting it asks for that the instance
/// lacks is named with the value the release's example gives it.
fn settings_hold(tree: &Path) -> bool {
    let (rc, said) = crate::store::merged(
        Command::new("bash")
            .args([
                "-c",
                ". \"$1/lib/config.sh\" && tehas_load && tehas_require_instance_settings",
                "_",
            ])
            .arg(tree),
    );
    if rc == 0 {
        return true;
    }
    let said = said.trim_end_matches('\n');
    tell(said);
    let example = fs::read(tree.join("instances/example/instance.env")).unwrap_or_default();
    let example = String::from_utf8_lossy(&example);
    for name in not_set(said) {
        if let Some(line) = example_line(&example, &name) {
            tell(&format!("  example: {line}"));
        }
    }
    false
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

/// The TEHAS_ keys of instance.env that <tree> never names.
fn unknown_keys(tree: &Path) -> Vec<String> {
    let file = PathBuf::from(instance_dir()).join("instance.env");
    let text = fs::read(file).unwrap_or_default();
    keys_of(&String::from_utf8_lossy(&text))
        .into_iter()
        .filter(|key| {
            let mut cmd = Command::new("grep");
            cmd.args(["-rqsw", "--", key]);
            for part in ["bin", "libexec", "lib", "host", "install.sh", "instances"] {
                cmd.arg(tree.join(part));
            }
            !cmd.status().is_ok_and(|s| s.success())
        })
        .collect()
}

/// A new link renamed over the old one, so current is never missing. The
/// history is what prune reads: the versions in the order they were switched to.
fn switch(host: &Host, tag: &str) -> bool {
    let new = host.home.join("current.new");
    match fs::remove_file(&new) {
        Err(e) if e.kind() != ErrorKind::NotFound => return false,
        _ => {}
    }
    if unix_fs::symlink(format!("versions/{tag}"), &new).is_err()
        || fs::rename(&new, host.current()).is_err()
    {
        return false;
    }
    let state = host.home.join("state");
    if fs::create_dir_all(&state).is_err() {
        return false;
    }
    use std::io::Write;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.join("release-history"))
        .and_then(|mut f| writeln!(f, "{tag}"))
        .is_ok()
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
    let history = fs::read(host.home.join("state/release-history")).unwrap_or_default();
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

/// A version that ran here before has its sandbox image still tagged, so going
/// back to it is a tag and not a build.
fn reuse_image(host: &Host, target: &str) -> bool {
    let tagged = format!("{IMAGE_TAG}{target}");
    quiet(timed(DOCKER_LIMIT, "docker").args(["image", "inspect", &tagged]))
        && quiet(timed(DOCKER_LIMIT, "docker").args(["tag", &tagged, &host.image]))
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
fn bring(host: &Host, target: &str, tree: &Path, arch: &str) -> u8 {
    let tmp = host.home.join("tmp");
    let _ = fs::create_dir_all(&tmp);
    let Ok(download) = temp_dir(&tmp, "release.") else {
        tell("release: cannot make a directory to download into; nothing was changed");
        return 1;
    };
    let tarball = tarball_of(target, arch);
    let brought = fetch(host, target, &download, &tarball)
        && verify(host, &download, &tarball)
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
pub(crate) fn go(host: &Host, to: Option<&str>, plan: &mut Plan) -> u8 {
    let check = plan.mode == Mode::Check;
    let installed = installed(host);
    let list = match list(host) {
        Ok(list) => list,
        Err(rc) => return rc,
    };
    let Some(newest) = list.first().map(|r| r.tag.clone()) else {
        let named = repo(host).unwrap_or_default();
        tell(&format!(
            "release: {named} has no published release; nothing was changed"
        ));
        return 1;
    };
    if let Some(to) = to {
        if !list.iter().any(|r| r.tag == to) {
            tell(&format!(
                "release: there is no published release {to}; nothing was changed"
            ));
            return 1;
        }
    }
    let mut target = to.unwrap_or(&newest).to_string();
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
        say(&format!("release: installed {was}, newest {newest}"));
    } else {
        say(&format!(
            "release: installed none, this host ran from its platform checkout at {tag}, newest {newest}"
        ));
        from_note = tag;
    }
    if to.is_none() && !installed.is_empty() && older(&newest, &installed) {
        say(&format!(
            "release: {installed} is newer than the newest release; tehas update --to moves it back"
        ));
        target = installed.clone();
    }
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
            return if plan.found { 1 } else { ANSWERED };
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
        let rc = bring(host, &target, &tree, arch);
        if rc != 0 {
            return rc;
        }
    }
    if !settings_hold(&tree) {
        tell(&format!(
            "settings: {target} asks for settings this instance does not have; nothing was switched"
        ));
        return 78;
    }
    for key in unknown_keys(&tree) {
        tell(&format!(
            "WARN: {}/instance.env sets {key}, which {target} does not know",
            instance_dir()
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
    if reuse_image(host, &target) {
        say(&format!(
            "sandbox: the image {target} ran with is in use again"
        ));
        plan.reused = true;
    }
    0
}

/// `console update release <question>`. The two that compare answer by status
/// alone: 0 yes, 1 no.
pub fn verb(given: &[String]) -> u8 {
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
        ["newest"] => match list(&host) {
            Ok(list) => {
                if let Some(first) = list.first() {
                    say(&first.tag);
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
        }
    }

    fn rel(tag: &str) -> Published {
        Published {
            tag: tag.to_string(),
            body: format!("body of {tag}"),
        }
    }

    #[test]
    fn a_version_is_vxyz_and_nothing_else() {
        for good in ["v1.2.3", "v0.0.0", "v10.20.30"] {
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
            "v1.2.3\n",
        ] {
            assert!(!tag_ok(bad), "{bad:?}");
        }
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
        let list = published(&answer.to_string()).unwrap();
        let tags: Vec<&str> = list.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(tags, ["v1.10.0", "v1.9.0", "v1.8.0"]);
        assert_eq!(list[1].body, "nine");
        assert_eq!(list[0].body, "");
    }

    #[test]
    fn an_answer_that_is_no_list_is_an_error_and_no_answer_is_none() {
        assert_eq!(published("").unwrap(), vec![]);
        assert_eq!(published("[]").unwrap(), vec![]);
        assert!(published("not json").is_err());
        assert!(published("7").is_err());
        assert!(published(r#"{"message": "Not Found"}"#).is_err());
        assert!(published(r#"[{"tag_name": 7}]"#).is_err());
        assert!(published(r#"[{"body": "no tag"}]"#).is_err());
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
        assert_eq!(indented("a\r\nb"), "  a\r\n  b\n");
    }

    #[test]
    fn the_asset_is_named_by_the_machine() {
        assert_eq!(arch_of("x86_64"), Some("x86_64"));
        assert_eq!(arch_of("amd64"), Some("x86_64"));
        assert_eq!(arch_of("aarch64"), Some("aarch64"));
        assert_eq!(arch_of("arm64"), Some("aarch64"));
        assert_eq!(arch_of("riscv64"), None);
        assert_eq!(arch_of(""), None);
        assert_eq!(
            tarball_of("v1.2.0", "aarch64"),
            "tehas-v1.2.0-linux-aarch64.tar.gz"
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

    // A download as a release gives it, and a minisign that answers `rc`.
    fn signed(d: &super::super::testing::Dir, rc: u8) -> (String, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let t = "tehas-v1.2.0-linux-x86_64.tar.gz".to_string();
        d.file(&format!("dl/{t}"), "a release\n");
        let (_, sum) = output(
            Command::new("sha256sum")
                .arg(&t)
                .current_dir(d.0.join("dl")),
        );
        d.file(&format!("dl/{t}.sha256"), &format!("{sum}\n"));
        d.file(&format!("dl/{t}.minisig"), "signed\n");
        d.file("home/release/minisign.pub", "key\n");
        let stub = d.file("minisign", &format!("#!/bin/sh\nexit {rc}\n"));
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
        (t, stub)
    }

    #[test]
    fn a_download_passes_only_with_both_checks() {
        let d = dir("verify-good");
        let (t, stub) = signed(&d, 0);
        assert!(verify_with(
            &host(&d.0.join("home")),
            &d.0.join("dl"),
            &t,
            stub.to_str().unwrap()
        ));
    }

    #[test]
    fn no_minisign_fails_closed() {
        let d = dir("verify-none");
        let (t, _) = signed(&d, 0);
        assert!(!verify_with(
            &host(&d.0.join("home")),
            &d.0.join("dl"),
            &t,
            "/no/such/minisign"
        ));
    }

    #[test]
    fn a_signature_the_key_refuses_fails_closed() {
        let d = dir("verify-bad-sig");
        let (t, stub) = signed(&d, 1);
        assert!(!verify_with(
            &host(&d.0.join("home")),
            &d.0.join("dl"),
            &t,
            stub.to_str().unwrap()
        ));
    }

    #[test]
    fn a_sum_that_does_not_match_stops_before_the_signature() {
        let d = dir("verify-bad-sum");
        let (t, stub) = signed(&d, 0);
        d.file(&format!("dl/{t}"), "tampered\n");
        assert!(!verify_with(
            &host(&d.0.join("home")),
            &d.0.join("dl"),
            &t,
            stub.to_str().unwrap()
        ));
    }

    #[test]
    fn a_missing_file_or_key_fails_closed() {
        for gone in ["tar.gz", "tar.gz.sha256", "tar.gz.minisig"] {
            let d = dir(&format!("verify-{gone}"));
            let (t, stub) = signed(&d, 0);
            let name = t.replace("tar.gz", gone);
            fs::remove_file(d.0.join("dl").join(name)).unwrap();
            assert!(!verify_with(
                &host(&d.0.join("home")),
                &d.0.join("dl"),
                &t,
                stub.to_str().unwrap()
            ));
        }
        let d = dir("verify-no-key");
        let (t, stub) = signed(&d, 0);
        fs::remove_file(d.0.join("home/release/minisign.pub")).unwrap();
        assert!(!verify_with(
            &host(&d.0.join("home")),
            &d.0.join("dl"),
            &t,
            stub.to_str().unwrap()
        ));
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
            d.file(&format!("versions/{v}/VERSION"), v);
        }
        d.file("versions/.unpack-abc/x", "");
        d.file("state/release-history", "v1.0.0\nv1.1.0\nv1.2.0\nv1.3.0\n");
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
            fs::read_to_string(d.0.join("state/release-history")).unwrap(),
            "v1.1.0\n"
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

    #[test]
    fn the_keys_of_an_instance_are_its_assignments() {
        let text = "TEHAS_B=1\n  export TEHAS_A=2\nexport\tTEHAS_C=3\n# TEHAS_D=4\nOTHER=5\nTEHAS_B=6\nTEHAS_lower=7\nexportTEHAS_E=8\n";
        assert_eq!(keys_of(text), ["TEHAS_A", "TEHAS_B", "TEHAS_C"]);
    }

    #[test]
    fn a_repository_is_owner_and_name() {
        assert!(repo_ok("tehas-app/tehas"));
        assert!(repo_ok("a_b/c.d"));
        for bad in ["", "no-slash", "a/b/c", "/b", "a/", "a b/c"] {
            assert!(!repo_ok(bad), "{bad}");
        }
    }

    #[test]
    fn the_repository_comes_from_the_setting_then_the_last_versions_line() {
        let versions = "TEHAS_RELEASE_REPO=a/first\nOTHER=1\nTEHAS_RELEASE_REPO=a/last\n";
        assert_eq!(repo_named("", versions), "a/last");
        assert_eq!(repo_named("o/n", versions), "o/n");
        assert_eq!(repo_named("", "OTHER=1\n"), "");
    }
}
