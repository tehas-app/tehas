//! The store of an item's files: the links an issue's allowed accounts wrote
//! ([`tehas_core::issue_feed::refs`]), fetched into a directory of the work
//! area by `attach::fetch`, with a zip unpacked by libexec/attach. The
//! directory holds `files/`, `.meta.json`, `index.html` and `.stamp`; the
//! caller puts it in its place.
//!
//!   which types     png jpg gif webp svg avif, mp4 webm mov, html htm, and
//!                   css js mjs json txt. A zip is unpacked into a folder of
//!                   its own by libexec/attach, which takes the same types and
//!                   md; its files count against the same limits.
//!   how much        10 megabytes an item, counted in the order the files were
//!                   attached, and at most 40 files. What does not fit is left
//!                   out and the page says so.
//!   a zip by address  A file attached on GitHub, or an https address on a
//!                   host of the store's `zip_hosts`, followed only to
//!                   addresses on those hosts. Bytes of another sha256 are
//!                   refused.
//!
//! A name is cut to the characters a file name may hold, so nothing a link
//! says can leave the item's directory.

use attach::fetch::{self, Fetched, Full, Pin, Unpinned};
use attach::link::{is_external, is_link};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::issue_feed::{Refs, Zip};
use tehas_core::jq::{obj, J};

pub const LIMIT: i64 = 10 * 1024 * 1024;
const MAX_FILES: i64 = 40;
const OVER: &str = "Over the 10 MB limit of a ticket.";
const NOT_HOSTED: &str = "Not a type the host serves.";
const RETRY: &str = "Could not be fetched. It is tried again.";
const REFUSED: &str = "GitHub did not hand it out.";
const REFUSED_NO_TOKEN: &str =
    "GitHub did not hand it out. The host holds no GitHub credential, so it asked as nobody.";
const NOT_A_MARK: &str =
    "Not a zip and its sha256: the line is Zip:, the address and sha256: with 64 lower-case hex digits.";
const NOT_ALLOWED: &str = "Not from a host the instance allows.";

/// Where and how the files of an item are taken.
pub struct Store {
    /// The home, whose libexec/attach unpacks a zip.
    pub home: PathBuf,
    /// Where an item is made before it takes its place.
    pub work: PathBuf,
    /// The credential a link to GitHub's storage is fetched with, or empty.
    pub token: String,
    /// The hosts a zip named by address may come from, one space apart.
    pub zip_hosts: String,
}

/// What one item holds so far.
#[derive(Default)]
struct Held {
    used: i64,
    count: i64,
    unnamed: i64,
    retry: bool,
    files: Vec<J>,
    skipped: Vec<J>,
}

impl Held {
    fn skip(&mut self, name: &str, reason: &str) {
        self.skipped.push(obj(vec![
            ("name", J::str(name)),
            ("reason", J::str(reason)),
        ]));
    }

    fn file(&mut self, name: &str, kind: J, bytes: i64) {
        self.files.push(obj(vec![
            ("name", J::str(name)),
            ("kind", kind),
            ("bytes", J::Num(bytes.to_string())),
        ]));
        self.used += bytes;
        self.count += 1;
    }
}

/// The page of an item as the caller draws it: from its meta, its files, the
/// files left out and the count of links from outside.
pub type Page<'a> = &'a dyn Fn(&Value, &[Value], &[Value], i64) -> String;

/// Make item `n`'s directory in the work area: its files, its meta, its page
/// and, unless a file is to be tried again, its stamp. The directory and the
/// line for the log, or None when it could not be made; what is in place is
/// then kept.
pub fn build(
    store: &Store,
    n: u64,
    item: &Value,
    found: &Refs,
    stamp: &str,
    page: Page,
) -> Option<(PathBuf, String)> {
    let dir = store.work.join(n.to_string());
    fs::create_dir_all(dir.join("files")).ok()?;

    let mut held = Held::default();
    for link in &found.fetch {
        take(store, &dir, link, &mut held)?;
    }
    for mark in &found.zips {
        match mark {
            Ok(zip) => take_zip(store, &dir, zip, &mut held),
            Err(word) => held.skip(
                &(if word.is_empty() {
                    "Zip:".to_string()
                } else {
                    clean_name(word)
                }),
                NOT_A_MARK,
            ),
        }
    }

    let (bytes, files, skipped) = (held.used, held.files.len(), held.skipped.len());
    let of = |key: &str| J::parse(&item[key].to_string()).unwrap_or(J::Null);
    let meta = obj(vec![
        ("number", of("number")),
        ("title", of("title")),
        ("url", of("url")),
        ("author", of("author")),
        ("updatedAt", of("updatedAt")),
        ("files", J::Arr(held.files)),
        ("skipped", J::Arr(held.skipped)),
        ("outside", J::Num(found.outside.to_string())),
        ("bytes", J::Num(bytes.to_string())),
    ])
    .pretty();
    fs::write(dir.join(".meta.json"), format!("{meta}\n")).ok()?;
    let row: Value = serde_json::from_str(&meta).ok()?;
    let html = page(
        &row,
        row["files"].as_array()?,
        row["skipped"].as_array()?,
        found.outside,
    );
    fs::write(dir.join("index.html"), html).ok()?;
    let _ = fs::remove_file(dir.join(".part"));
    // Without a stamp the next run builds the item again.
    if !held.retry {
        let _ = fs::write(dir.join(".stamp"), format!("{stamp}\n"));
    }
    Some((
        dir,
        format!("#{n}: {files} files, {bytes} bytes, {skipped} left out"),
    ))
}

/// What every stamp starts with: the program that writes the pages, so a page
/// whose words changed is built again, and so is one drawn in another design.
/// Read through /proc, which still works while an update has replaced the
/// file. With zips by address on, the hosts they may come from are part of it
/// too, so a host added or taken off, or the switch turned, builds every page
/// again.
pub fn generator(domain: &str, design: &str, zips: Option<&str>) -> String {
    let bytes = fs::read("/proc/self/exe").unwrap_or_default();
    let mut hash = DefaultHasher::new();
    bytes.hash(&mut hash);
    stamp_of(hash.finish(), bytes.len(), domain, design, zips)
}

fn stamp_of(hash: u64, len: usize, domain: &str, design: &str, zips: Option<&str>) -> String {
    let domain = if domain.is_empty() { "path" } else { domain };
    let design = if design == "pixel" {
        "pixel"
    } else {
        "beesknees26"
    };
    let mut stamp = format!("{hash:016x}-{len}-{domain}-{design}");
    if let Some(hosts) = zips {
        let mut hosts: Vec<&str> = hosts.split(' ').filter(|h| !h.is_empty()).collect();
        hosts.sort_unstable();
        hosts.dedup();
        let mut hash = DefaultHasher::new();
        hosts.hash(&mut hash);
        stamp.push_str(&format!("-zips-{:016x}", hash.finish()));
    }
    stamp
}

/// A value as `jq -r` prints it.
pub fn raw(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// One link of the item, fetched into the directory or said to be left out.
/// None when the item cannot be built at all.
fn take(store: &Store, dir: &Path, link: &str, held: &mut Held) -> Option<()> {
    let mut name = clean_name(link);
    let named = is_named(link);
    let mut kind = "";
    if named {
        let ext = extension(&name);
        kind = if ext == "zip" {
            "zip"
        } else if let Some(kind) = kind_of(&ext) {
            kind
        } else {
            held.skip(&name, NOT_HOSTED);
            return Some(());
        };
    }
    let Some(left) = room(&name, held) else {
        return Some(());
    };

    let part = dir.join(".part");
    let _ = fs::remove_file(&part);
    let Fetched {
        code, content_type, ..
    } = fetch::fetch(link, &part, left, &store.token);
    match code {
        0 => {}
        fetch::TOO_BIG => {
            held.skip(&name, OVER);
            return Some(());
        }
        fetch::REFUSED => {
            held.skip(&name, refused(link, &store.token));
            return Some(());
        }
        _ => {
            held.skip(&name, RETRY);
            held.retry = true;
            return Some(());
        }
    }
    let size = fs::metadata(&part).map_or(0, |m| m.len() as i64);
    if size == 0 {
        held.skip(&name, "Empty.");
        return Some(());
    }
    if size > left {
        held.skip(&name, OVER);
        return Some(());
    }
    if !named {
        let Some(ext) = extension_of_type(&content_type) else {
            held.skip(&name, NOT_HOSTED);
            return Some(());
        };
        kind = kind_of(ext).unwrap_or_default();
        held.unnamed += 1;
        name = format!("{kind}-{}.{ext}", held.unnamed);
    }
    if kind == "zip" {
        unpack(store, dir, &name, left, held);
        return Some(());
    }
    let files = dir.join("files");
    // Two attachments may carry one name.
    let name = free_name(&files, name);
    fs::rename(&part, files.join(&name)).ok()?;
    let _ = fs::set_permissions(files.join(&name), fs::Permissions::from_mode(0o644));
    held.file(&name, J::str(kind), size);
    Some(())
}

/// Why GitHub refused a file: a link that would have carried the credential,
/// fetched without one, says so, so it does not read as a file deleted.
fn refused(link: &str, token: &str) -> &'static str {
    if token.is_empty() && attach::link::sends_token(link, "-") {
        REFUSED_NO_TOKEN
    } else {
        REFUSED
    }
}

/// The bytes left for one more file, or None when it is left out, and said.
fn room(name: &str, held: &mut Held) -> Option<i64> {
    match fetch::room(held.count, MAX_FILES, held.used, LIMIT) {
        Ok(left) => Some(left),
        Err(Full::Count) => {
            held.skip(name, &more_than());
            None
        }
        Err(Full::Bytes) => {
            held.skip(name, OVER);
            None
        }
    }
}

/// A zip the item names by address and sha256: a file attached on GitHub,
/// fetched as one, or an address on a host of the store's `zip_hosts`, which
/// is followed only to addresses on those hosts. Bytes of another sha256 are
/// refused. It is unpacked as an attached zip, under the same limits.
fn take_zip(store: &Store, dir: &Path, zip: &Zip, held: &mut Held) {
    let name = clean_name(&zip.link);
    let within = |a: &str| is_external(a, &store.zip_hosts);
    if !is_link(&zip.link) && !within(&zip.link) {
        held.skip(&name, NOT_ALLOWED);
        return;
    }
    let Some(left) = room(&name, held) else {
        return;
    };
    let part = dir.join(".part");
    let pin = Pin {
        link: &zip.link,
        sha256: &zip.sha256,
        most: left,
    };
    let got = if is_link(&zip.link) {
        fetch::fetch_pinned(&pin, &part, &store.token)
    } else {
        fetch::fetch_pinned_within(&pin, &part, &store.token, &within)
    };
    let failed = match got {
        Ok(_) => None,
        Err(Unpinned::Fetch(Fetched { code, .. })) => match code {
            fetch::TOO_BIG => Some(OVER),
            fetch::REFUSED => Some("The host did not hand it out."),
            fetch::MOVED => Some("Sent on to an address the instance does not allow."),
            _ => {
                held.retry = true;
                Some(RETRY)
            }
        },
        Err(Unpinned::Other(_)) => Some("Not the bytes its sha256 names."),
        Err(Unpinned::Unreadable(_)) => {
            held.retry = true;
            Some(RETRY)
        }
    };
    let size = fs::metadata(&part).map_or(0, |m| m.len() as i64);
    let failed = failed.or(if size == 0 {
        Some("Empty.")
    } else if size > left {
        Some(OVER)
    } else {
        None
    });
    if let Some(reason) = failed {
        let _ = fs::remove_file(&part);
        held.skip(&name, reason);
        return;
    }
    unpack(store, dir, &name, left, held);
}

fn more_than() -> String {
    format!("More than {MAX_FILES} files.")
}

/// The last part of the address, kept to the characters a file name may hold,
/// so it cannot leave the item's directory.
fn clean_name(link: &str) -> String {
    let last = link.rsplit('/').next().unwrap_or("");
    let last = last.split('?').next().unwrap_or("");
    let safe: String = last
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
                b as char
            } else {
                '-'
            }
        })
        .collect();
    let name: String = safe
        .trim_start_matches(['.', '-'])
        .chars()
        .take(80)
        .collect();
    if name.is_empty() {
        "file".to_string()
    } else {
        name
    }
}

/// An image or a video pasted into an issue has no name of its own, and its
/// type is the one GitHub's storage answers with.
fn is_named(link: &str) -> bool {
    !link
        .strip_prefix("https://github.com/")
        .is_some_and(|tail| tail.contains("/assets/"))
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// The same table as `kind` in console/attach/src/unpack.rs, less md.
fn kind_of(ext: &str) -> Option<&'static str> {
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" => Some("image"),
        "mp4" | "webm" | "mov" => Some("video"),
        "html" | "htm" => Some("app"),
        "css" | "js" | "mjs" | "json" | "txt" => Some("part"),
        _ => None,
    }
}

/// An image or video pasted into an issue has no name, so its type is the one
/// GitHub's storage answers with.
fn extension_of_type(content_type: &str) -> Option<&'static str> {
    match content_type.split(';').next().unwrap_or("") {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/avif" => Some("avif"),
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/quicktime" => Some("mov"),
        _ => None,
    }
}

/// The name, or `<i>-<name>` for the first i from 2 that nothing holds. One
/// digit and a hyphen at the front of the name give way to the number.
fn free_name(dir: &Path, name: String) -> String {
    let mut name = name;
    let mut i = 2;
    while dir.join(&name).exists() {
        name = format!("{i}-{}", without_number(&name));
        i += 1;
    }
    name
}

fn without_number(name: &str) -> &str {
    let b = name.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_digit() && b[1] == b'-' {
        &name[2..]
    } else {
        name
    }
}

fn status_of(status: std::process::ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

/// What libexec/attach made of a zip.
enum Unpacked {
    Entries(Vec<Value>),
    /// The input's fault, or the answer has no entries.
    NoZip,
    /// The host's.
    NoHost,
}

/// The zip in .part, into a folder of its own under files/. Its files and what
/// it leaves out are listed under that folder.
fn unpack(store: &Store, dir: &Path, name: &str, left: i64, held: &mut Held) {
    let base = name.rsplit_once('.').map_or(name, |(base, _)| base);
    let base = if base.is_empty() { "zip" } else { base };
    let files = dir.join("files");
    let base = free_name(&files, base.to_string());
    let folder = files.join(&base);
    let part = dir.join(".part");
    let answer = attach(
        &store.home.join("libexec/attach"),
        &part,
        &folder,
        left,
        MAX_FILES - held.count,
    );
    let _ = fs::remove_file(&part);
    let entries = match answer {
        Unpacked::Entries(entries) => entries,
        failed => {
            let _ = fs::remove_dir_all(&folder);
            let reason = match failed {
                Unpacked::NoHost => "This host cannot unpack a zip.",
                _ => "Not a zip the host can read.",
            };
            held.skip(name, reason);
            return;
        }
    };
    let _ = fs::remove_dir(&folder);
    for entry in &entries {
        if !entry["skip"].is_null() {
            let reason = raw(&entry["reason"]);
            let reason = if reason.starts_with("More than ") && reason.ends_with(" files.") {
                more_than()
            } else {
                reason
            };
            held.skip(&format!("{base}/{}", raw(&entry["skip"])), &reason);
        } else if !entry["name"].is_null() {
            let kind = J::parse(&entry["kind"].to_string()).unwrap_or(J::Null);
            let bytes = entry["bytes"].as_i64().unwrap_or(0);
            held.file(&format!("{base}/{}", raw(&entry["name"])), kind, bytes);
        }
    }
}

/// Run libexec/attach, in the bound of 60 seconds tehas_attach gives it.
fn attach(bin: &Path, zip: &Path, folder: &Path, most: i64, files: i64) -> Unpacked {
    if !tehas_core::file::runnable(bin) {
        eprintln!(
            "tehas: attach: absent: {} is not an executable file",
            bin.display()
        );
        return Unpacked::NoHost;
    }
    let done = Command::new("timeout")
        .args(["-k", "2", "60"])
        .arg(bin)
        .arg("unpack")
        .arg(zip)
        .arg(folder)
        .arg("--max-bytes")
        .arg(most.to_string())
        .arg("--max-files")
        .arg(files.to_string())
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output();
    let (code, out) = match done {
        Ok(done) => {
            let mut out = String::from_utf8_lossy(&done.stdout).into_owned();
            out.push_str(&String::from_utf8_lossy(&done.stderr));
            (status_of(done.status), out)
        }
        Err(_) => (127, String::new()),
    };
    read_attach(code, &out)
}

/// The answer of `attach unpack`.
fn read_attach(code: i32, out: &str) -> Unpacked {
    let out = out.trim_end_matches('\n');
    if code == 0 && !out.contains('\n') {
        if let Ok(said) = serde_json::from_str::<Value>(out) {
            if said["api"] == 1 && said["ok"] == true && said["verb"] == "unpack" {
                return match said["entries"].as_array() {
                    Some(entries) => Unpacked::Entries(entries.clone()),
                    None => Unpacked::NoZip,
                };
            }
        }
    }
    let line = out.split('\n').next().unwrap_or("");
    let unreadable = code == 1 && line.starts_with("attach: unreadable:");
    if unreadable || matches!(code, 124 | 137 | 101) {
        return Unpacked::NoZip;
    }
    eprintln!("{}", attach_line(code, line));
    Unpacked::NoHost
}

/// The log line of an unpack that failed. Its text comes from a zip on an
/// issue, so it is masked before it is cut to one short line.
fn attach_line(code: i32, line: &str) -> String {
    let line: String = tehas_core::mask::mask(line)
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(300)
        .collect();
    format!("tehas: attach: unpack exited {code}: {line}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use attach::link::sends_token;

    fn scratch(name: &str) -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new(&format!("items-{name}"))
    }

    #[test]
    fn the_token_goes_only_to_a_whole_attachment_link() {
        for link in [
            "https://github.com/o/r/files/1/a.json",
            "https://github.com/user-attachments/files/1/a.json",
            "https://github.com/user-attachments/assets/1",
        ] {
            assert!(sends_token(link, "tok"), "{link}");
            assert!(!sends_token(link, ""), "{link}");
        }
        for link in [
            "https://github.com/o/r/files/1/../../raw/main/a.json",
            "https://github.com/o/r/files/1/%2e%2e/%2E%2E/raw/main/a.json",
            "https://github.com/o/r/files/1/.%2e/raw/a.json",
            "https://github.com/user-attachments/files/../../o/r/raw/a.json",
            "https://github.com/o/r/raw/main/a.json",
            "https://github.com/o/r/files/1/a.json#x",
            "https://user-images.githubusercontent.com/1/a.png",
            "https://github.com.example/o/r/files/1/a.json",
        ] {
            assert!(!sends_token(link, "tok"), "{link}");
        }
    }

    #[test]
    fn a_name_stays_inside_the_directory() {
        assert_eq!(
            clean_name("https://github.com/user-attachments/files/71/..%2F..%2Fevil.html"),
            "2F..-2Fevil.html"
        );
        assert_eq!(
            clean_name("https://github.com/user-attachments/files/1/a b.html?x=1"),
            "a-b.html"
        );
        assert_eq!(
            clean_name("https://github.com/user-attachments/files/1/..."),
            "file"
        );
        assert_eq!(
            clean_name(&format!("https://x/{}", "a".repeat(90))).len(),
            80
        );
    }

    #[test]
    fn a_link_under_assets_has_no_name_of_its_own() {
        assert!(!is_named("https://github.com/user-attachments/assets/1-2"));
        assert!(!is_named("https://github.com/acme/widget/assets/1/x.png"));
        assert!(is_named(
            "https://github.com/user-attachments/files/1/x.png"
        ));
        assert!(is_named(
            "https://user-images.githubusercontent.com/1/x.png"
        ));
    }

    #[test]
    fn the_extension_is_lower_case_and_may_be_absent() {
        assert_eq!(extension("A.B.PNG"), "png");
        assert_eq!(extension("plain"), "");
    }

    #[test]
    fn the_kind_follows_the_extension_and_an_unknown_one_has_none() {
        for (ext, kind) in [
            ("png", "image"),
            ("jpg", "image"),
            ("jpeg", "image"),
            ("gif", "image"),
            ("webp", "image"),
            ("svg", "image"),
            ("avif", "image"),
            ("mp4", "video"),
            ("webm", "video"),
            ("mov", "video"),
            ("html", "app"),
            ("htm", "app"),
            ("css", "part"),
            ("js", "part"),
            ("mjs", "part"),
            ("json", "part"),
            ("txt", "part"),
        ] {
            assert_eq!(kind_of(ext), Some(kind), "{ext}");
        }
        assert_eq!(kind_of(""), None);
    }

    #[test]
    fn a_content_type_gives_an_extension_for_an_image_or_a_video_only() {
        for (content_type, ext) in [
            ("image/png", "png"),
            ("image/jpeg", "jpg"),
            ("image/gif", "gif"),
            ("image/webp", "webp"),
            ("image/svg+xml", "svg"),
            ("image/avif", "avif"),
            ("video/mp4", "mp4"),
            ("video/webm", "webm"),
            ("video/quicktime", "mov"),
        ] {
            assert_eq!(extension_of_type(content_type), Some(ext));
            assert_eq!(
                extension_of_type(&format!("{content_type}; charset=x")),
                Some(ext)
            );
        }
        assert_eq!(kind_of("zip"), None);
        assert_eq!(extension_of_type("text/html"), None);
    }

    #[test]
    fn a_taken_name_gets_a_number_of_its_own() {
        let dir = scratch("names");
        assert_eq!(free_name(&dir, "same.html".to_string()), "same.html");
        fs::write(dir.join("same.html"), "").unwrap();
        fs::write(dir.join("2-same.html"), "").unwrap();
        assert_eq!(free_name(&dir, "same.html".to_string()), "3-same.html");
        fs::write(dir.join("3-a"), "").unwrap();
        assert_eq!(free_name(&dir, "3-a".to_string()), "2-a");
    }

    #[test]
    fn the_stamp_says_the_program_the_domain_and_the_design() {
        assert_eq!(
            stamp_of(255, 7, "", "", None),
            "00000000000000ff-7-path-beesknees26"
        );
        assert_eq!(
            stamp_of(255, 7, "a.org", "pixel", None),
            "00000000000000ff-7-a.org-pixel"
        );
    }

    #[test]
    fn with_zips_on_the_stamp_says_the_hosts_in_any_order() {
        let off = stamp_of(255, 7, "", "", None);
        let none = stamp_of(255, 7, "", "", Some(""));
        let one = stamp_of(255, 7, "", "", Some("a.org"));
        let two = stamp_of(255, 7, "", "", Some("a.org b.org"));
        assert!(none.starts_with(&format!("{off}-zips-")));
        assert_ne!(none, one);
        assert_ne!(one, two);
        assert_eq!(two, stamp_of(255, 7, "", "", Some("b.org a.org")));
        assert!(!two.contains(' '));
    }

    #[test]
    fn the_line_of_a_failed_unpack_is_masked_and_cut() {
        let token = format!("ghp_{}", "a".repeat(36));
        let line = attach_line(1, &format!("attach: unwritable: {token}\tthere"));
        assert!(!line.contains(&token), "{line}");
        assert!(line.starts_with("tehas: attach: unpack exited 1: attach: unwritable: "));
        assert!(!line.contains('\t'));
        let long = attach_line(2, &"x".repeat(400));
        assert_eq!(long.len(), "tehas: attach: unpack exited 2: ".len() + 300);
    }

    #[test]
    fn a_refusal_without_a_credential_says_so() {
        let link = "https://github.com/user-attachments/files/1/a.json";
        assert_eq!(refused(link, ""), REFUSED_NO_TOKEN);
        assert_eq!(refused(link, "tok"), REFUSED);
        assert_eq!(refused("https://example.com/a.json", ""), REFUSED);
    }

    #[test]
    fn the_answer_of_attach_is_held_to_its_shape() {
        let good = r#"{"api":1,"ok":true,"verb":"unpack","entries":[{"name":"a.txt"}]}"#;
        assert!(matches!(read_attach(0, good), Unpacked::Entries(e) if e.len() == 1));
        assert!(matches!(
            read_attach(0, &format!("{good}\nmore")),
            Unpacked::NoHost
        ));
        assert!(matches!(
            read_attach(0, r#"{"api":1,"ok":true,"verb":"unpack"}"#),
            Unpacked::NoZip
        ));
        assert!(matches!(
            read_attach(1, "attach: unreadable: bad zip"),
            Unpacked::NoZip
        ));
        assert!(matches!(read_attach(124, ""), Unpacked::NoZip));
        assert!(matches!(read_attach(137, ""), Unpacked::NoZip));
        assert!(matches!(read_attach(101, "panic"), Unpacked::NoZip));
        // Its own refusal to write is the host's.
        assert!(matches!(
            read_attach(1, "attach: unwritable: no room"),
            Unpacked::NoHost
        ));
        assert!(matches!(read_attach(2, "usage"), Unpacked::NoHost));
    }
}
