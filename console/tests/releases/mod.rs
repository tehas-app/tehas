//! What the release tests of `tehas update` share: a local directory of
//! releases that gh and curl answer from, homes installed from them, and the
//! homes older versions left. minisign is a stub that takes a signature file
//! saying "signed".

#![allow(dead_code)] // each test file uses a part of this module

use crate::common::{root, s, Host};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use tehas_core::home::{Home, State};
use tehas_core::testing::write_exe;

/// gh answering from `$T/releases` as GitHub does the code host's release
/// reads: the list, one release with its files and their sizes as they are
/// now, and a file downloaded to `--output`. With GH_AUTH set to other than
/// 0 it is a gh with no credential for the release project: each of those
/// reads exits 4 and asks nothing, as gh does. The update's other reads of
/// gh (an account's id) answer as before.
pub const GH: &str = r#"case "$1 $2" in
  "api repos/"*/releases* | "release download")
    if [ "${GH_AUTH:-0}" != 0 ]; then
      echo "Alternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token." >&2
      exit 4
    fi ;;
esac
files_of() {
  for f in "$T/releases/$1"/tehas-*; do
    [ -f "$f" ] && jq -n --arg n "${f##*/}" --argjson s "$(stat -c %s "$f")" '{name: $n, size: $s}'
  done | jq -s .
}
one() { jq --argjson a "$(files_of "$1")" '. + {assets: $a}' "$T/releases/$1/release.json"; }
case "$1 $2" in
  "api repos/"*/releases/tags/*)
    tag=${2##*/}
    [ -f "$T/releases/$tag/release.json" ] || { echo "release not found" >&2; exit 1; }
    one "$tag" ;;
  "api repos/"*"&page=1")
    for d in "$T/releases"/*/; do
      d=${d%/}; [ ! -f "$d/release.json" ] || one "${d##*/}"
    done | jq -s . ;;
  "api repos/"*) echo "[]" ;;
  "release download")
    out="" name="" tag=""
    for a in "$@"; do
      case $a in --output=*) out=${a#--output=} ;; --pattern=*) name=${a#--pattern=} ;; esac
      tag=$a
    done
    cp "$T/releases/$tag/$name" "$out" ;;
esac"#;

/// GitHub's API, as the anonymous code host asks it through curl.
const API: &str = "https://api.github.com/";

/// curl answering from `$T/releases` as GitHub's API does the anonymous
/// code host's reads: the list, one release by its tag with its files, each
/// file's id its inode, and a file asked by its id. The adapter runs curl
/// with an environment of its own, so the stub names `$T` itself. Each
/// call's arguments are kept in `$T/calls/curl`.
pub fn curl_stub(h: &Host) {
    let t = h.join("");
    let api = API;
    let body = format!(
        r#"#!/usr/bin/env bash
T='{t}'
printf '%s\n' "$*" >>"$T/calls/curl"
url=""; for a in "$@"; do url=$a; done
path=${{url#{api}}}
[ "$path" != "$url" ] || path=/
files_of() {{
  for f in "$T/releases/$1"/tehas-*; do
    [ -f "$f" ] && jq -n --arg n "${{f##*/}}" --argjson s "$(stat -c %s "$f")" \
      --argjson i "$(stat -c %i "$f")" '{{id: $i, name: $n, size: $s}}'
  done | jq -s .
}}
one() {{ jq --argjson a "$(files_of "$1")" '. + {{assets: $a}}' "$T/releases/$1/release.json"; }}
found() {{ printf 'HTTP/2 200 \r\ncontent-type: application/json\r\n\r\n'; }}
none() {{ printf 'HTTP/2 404 \r\n\r\n{{"message":"Not Found"}}'; }}
case "$path" in
  repos/*/releases\?per_page=100*)
    found
    for d in "$T/releases"/*/; do
      d=${{d%/}}; [ ! -f "$d/release.json" ] || one "${{d##*/}}"
    done | jq -s . ;;
  repos/*/releases/tags/*)
    tag=${{url##*/}}
    if [ -f "$T/releases/$tag/release.json" ]; then found; one "$tag"; else none; fi ;;
  repos/*/releases/assets/*)
    f=$(find "$T/releases" -type f -inum "${{url##*/}}" | head -n 1)
    if [ -n "$f" ]; then printf 'HTTP/2 200 \r\n\r\n'; cat "$f"; else none; fi ;;
  *) none ;;
esac
"#,
        t = t.display().to_string().trim_end_matches('/'),
    );
    write_exe(h.join("bin/curl"), body).unwrap();
}

/// The release files asked of the anonymous handle, by curl, each as
/// `<project> <tag> <file>`.
pub fn anonymous_downloads(h: &Host) -> Vec<String> {
    let calls = h.calls("curl");
    let asked: Vec<&str> = calls
        .lines()
        .filter_map(|l| l.rsplit(' ').next())
        .collect();
    let mut got = Vec::new();
    for (i, url) in asked.iter().enumerate() {
        let Some(rest) = url.strip_prefix(API).and_then(|u| u.strip_prefix("repos/")) else {
            continue;
        };
        let Some((project, id)) = rest.split_once("/releases/assets/") else {
            continue;
        };
        // The release read by its tag just before names the tag.
        let tag = asked[..i]
            .iter()
            .rev()
            .find_map(|u| u.rsplit_once("/releases/tags/").map(|(_, tag)| tag))
            .unwrap_or_default();
        let file = inode_name(h, id).unwrap_or_default();
        got.push(format!("{project} {tag} {file}"));
    }
    got
}

/// The name of the release file whose inode is `id`.
fn inode_name(h: &Host, id: &str) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let id: u64 = id.parse().ok()?;
    for tag in fs::read_dir(rel_dir(h)).ok()? {
        let tag = tag.ok()?.path();
        for f in fs::read_dir(&tag).into_iter().flatten().flatten() {
            if f.metadata().is_ok_and(|m| m.ino() == id) {
                return Some(f.file_name().to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// The program the release stub answers as. Its calls are read here alone.
const STUB: &str = "gh";

/// The projects whose releases were listed through the stub, one a read.
pub fn listed(h: &Host) -> Vec<String> {
    h.calls(STUB)
        .lines()
        .filter_map(|l| l.strip_prefix("api repos/"))
        .filter_map(|l| l.split_once("/releases?").map(|(repo, _)| repo.to_string()))
        .collect()
}

/// The files downloaded through the stub, each as `<project> <tag> <file>`.
pub fn downloads(h: &Host) -> Vec<String> {
    h.calls(STUB)
        .lines()
        .filter(|l| l.starts_with("release download "))
        .map(|l| {
            let words: Vec<&str> = l.split(' ').collect();
            let given = |flag: &str| {
                words
                    .iter()
                    .find_map(|w| w.strip_prefix(flag))
                    .unwrap_or_default()
            };
            let tag = words.last().copied().unwrap_or_default();
            format!("{} {tag} {}", given("--repo="), given("--pattern="))
        })
        .collect()
}

/// Whether `tag`'s tarball of this machine was downloaded from `project`.
pub fn downloaded(h: &Host, project: &str, tag: &str) -> bool {
    downloads(h).contains(&format!("{project} {tag} {}", asset_of(tag)))
}

/// minisign that keeps the key it was handed and takes "signed". It exits
/// itself: `Host::stub` ends a body with `:`.
pub const MINISIGN: &str = r#"x=""; p=""; prev=""
for a in "$@"; do [ "$prev" != -x ] || x=$a; [ "$prev" != -p ] || p=$a; prev=$a; done
cat "$p" >"$T/calls/minisign-key"
[ "$(cat "$x")" = signed ]
exit"#;

/// One release as pack-release.sh makes it, small.
pub struct Rel<'a> {
    pub tag: &'a str,
    /// The notes; the default for None.
    pub body: Option<&'a str>,
    pub pre: bool,
    /// The machine of the asset's name.
    pub arch: &'a str,
    /// The tree carries host/release/minisign.pub.
    pub key: bool,
    /// install.sh after its shebang.
    pub install: String,
}

/// The asset name of a release for `machine`: x86_64 only.
pub fn arch_of(machine: &str) -> Option<&'static str> {
    matches!(machine, "x86_64" | "amd64").then_some("x86_64")
}

/// What `uname -m` says on this host.
pub fn machine() -> String {
    let out = Command::new("uname").arg("-m").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The asset of `tag` for this host's machine.
pub fn asset_of(tag: &str) -> String {
    let arch = arch_of(&machine()).expect("no release is packed for this machine");
    format!("tehas-{tag}-linux-{arch}.tar.gz")
}

pub fn rel_dir(h: &Host) -> PathBuf {
    h.join("releases")
}

fn sh_in(dir: &Path, cmd: &str) {
    let out = Command::new("bash")
        .args(["-c", cmd])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{cmd}: {out:?}");
}

/// The tarball of `$T/src/tehas-<tag>` and its .sha256 in the release dir.
pub fn repack(h: &Host, tag: &str, name: &str) {
    let out = rel_dir(h).join(tag);
    sh_in(
        &h.join("src"),
        &format!("tar -czf '{}/{name}' 'tehas-{tag}'", s(&out)),
    );
    sh_in(&out, &format!("sha256sum '{name}' >'{name}.sha256'"));
}

/// A release of this tree: its lib/ and example instance, a VERSION and the
/// install.sh asked for, its .sha256 and a signature minisign takes.
pub fn pack(h: &Host, r: &Rel) {
    let name = format!("tehas-{}-linux-{}.tar.gz", r.tag, r.arch);
    let src = h.join(format!("src/tehas-{}", r.tag));
    let out = rel_dir(h).join(r.tag);
    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(src.join("instances")).unwrap();
    fs::create_dir_all(&out).unwrap();
    sh_in(
        &root(),
        &format!(
            "cp -R lib '{s}/lib' && cp -R instances/example '{s}/instances/'",
            s = s(&src)
        ),
    );
    if r.key {
        fs::create_dir_all(src.join("host/release")).unwrap();
        fs::copy(
            root().join("host/release/minisign.pub"),
            src.join("host/release/minisign.pub"),
        )
        .unwrap();
    }
    fs::write(src.join("VERSION"), format!("{}\n", r.tag)).unwrap();
    write_exe(
        src.join("install.sh"),
        format!("#!/usr/bin/env bash\n{}", r.install),
    )
    .unwrap();
    repack(h, r.tag, &name);
    fs::write(out.join(format!("{name}.minisig")), "signed\n").unwrap();
    let default = format!("### Changes\n- {}\n### Adopter action\nNone.", r.tag);
    let id = r
        .tag
        .bytes()
        .fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(b.into()));
    let json = serde_json::json!({
        "id": id % 1_000_000_007 + 1,
        "tag_name": r.tag,
        "name": r.tag,
        "body": r.body.unwrap_or(&default),
        "draft": false,
        "prerelease": r.pre,
        "created_at": "2026-01-01T00:00:00Z",
        "published_at": "2026-01-01T00:00:00Z",
        "html_url": format!("https://example.test/r/{}", r.tag),
    });
    fs::write(out.join("release.json"), format!("{json}\n")).unwrap();
    relist(h);
}

/// list.json: every release.json, as `jq -s .` reads them in name order.
pub fn relist(h: &Host) {
    let dir = rel_dir(h);
    let mut tags: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("release.json").is_file())
        .collect();
    tags.sort();
    let list: Vec<serde_json::Value> = tags
        .iter()
        .map(|p| {
            serde_json::from_str(&fs::read_to_string(p.join("release.json")).unwrap()).unwrap()
        })
        .collect();
    fs::write(
        dir.join("list.json"),
        format!("{}\n", serde_json::Value::Array(list)),
    )
    .unwrap();
}

/// A release is gone from the directory and the list.
pub fn unpublish(h: &Host, tag: &str) {
    fs::remove_dir_all(rel_dir(h).join(tag)).unwrap();
    relist(h);
}

/// The tokens and login of the app, as t_instance makes them (t_app_world).
pub fn app_world(h: &Host) {
    let state = Home::new(&h.home).state_dir();
    let until = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 86400;
    for (dir, token) in [
        ("github", "app"),
        ("github-cycle", "cycle"),
        ("github-session", "session"),
    ] {
        fs::create_dir_all(state.join(dir)).unwrap();
        fs::write(state.join(dir).join("token"), format!("ghs_{token}\n")).unwrap();
        fs::write(state.join(dir).join("token.expires"), format!("{until}\n")).unwrap();
    }
    fs::write(state.join("github/login"), "tehas-bot[bot]\n").unwrap();
}

/// instance.env as given, and the app world unless the host has a login
/// (t_instance).
pub fn instance(h: &mut Host, lines: &[&str]) {
    h.instance_with(lines);
    if !Home::new(&h.home).state(State::BotLogin).exists() {
        app_world(h);
    }
}

/// The home an older version left, running `version` from current:
/// `fixture` holds its versions.env, its key and its VERSION.
pub fn old_home(h: &Host, fixture: &Path, version: &str) {
    let th = &h.home;
    let _ = fs::remove_dir_all(th);
    h.clear_calls();
    for d in [
        format!("versions/{version}"),
        "release".into(),
        "state".into(),
    ] {
        fs::create_dir_all(th.join(d)).unwrap();
    }
    fs::write(th.join(".tehas-home"), "").unwrap();
    fs::copy(fixture.join("versions.env"), th.join("versions.env")).unwrap();
    fs::copy(
        fixture.join("release/minisign.pub"),
        th.join("release/minisign.pub"),
    )
    .unwrap();
    fs::copy(
        fixture.join("VERSION"),
        th.join(format!("versions/{version}/VERSION")),
    )
    .unwrap();
    symlink(format!("versions/{version}"), th.join("current")).unwrap();
    fs::write(
        Home::new(th).state(State::ReleaseHistory),
        format!("{version}\n"),
    )
    .unwrap();
}

/// Where current points.
pub fn current(h: &Host) -> String {
    fs::read_link(h.home.join("current"))
        .map(|p| s(&p))
        .unwrap_or_default()
}

/// The repository host/versions.env names for releases.
pub fn pinned() -> String {
    setting(&root().join("host/versions.env"), "TEHAS_RELEASE_REPO")
}

/// The last value of `key=` in a file.
pub fn setting(file: &Path, key: &str) -> String {
    fs::read_to_string(file)
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix(&format!("{key}=")).map(str::to_string))
        .next_back()
        .unwrap_or_default()
}

/// The words of a run of the console's update, `prefix` being assignments
/// that go before it.
pub fn update_with(h: &Host, prefix: &str, args: &str) -> crate::common::Run {
    h.sh(&format!(
        "{prefix} \"$TEHAS_CONSOLE\" update run \"$TEHAS_CONSOLE\" {args}"
    ))
}

/// The settings check an older updater asks of a release tree before it
/// switches current.
pub fn settings_hold(h: &Host, tree: &Path) -> crate::common::Run {
    h.sh(&format!(
        ". '{}/lib/config.sh' && tehas_load && tehas_require_instance_settings",
        s(tree)
    ))
}

/// The lines of a file.
pub fn lines(p: &Path) -> Vec<String> {
    fs::read_to_string(p)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}
