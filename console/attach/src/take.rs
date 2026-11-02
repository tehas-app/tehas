//! The files an issue or a reply links: who may attach, how much an issue is
//! given, what each file is, and what a cycle is given of an archive. Every
//! link, name and byte here is the input of anyone who can write on the issue.
//!
//! This process holds GH_TOKEN, so it reads no hostile bytes. curl and gh are
//! child processes. What was fetched is read by this program run again as a
//! child: no token and no other environment, the 60 seconds of a read, so a
//! panic or a hang is the archive's fault and leaves it out.
//!
//! Every name a file is given here is made here, from a count and an extension
//! of a closed list. Nothing of a link names a place on the disk.

use crate::given::{self, Linked, Ordered};
use crate::manifest::{self, Given};
use crate::unpack;
use serde_json::Value;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};

const LIMIT: i64 = 10 * 1024 * 1024;
const MOST_IMAGES: i64 = 8;
const FLIMIT: i64 = 4 * 1024 * 1024;
const MOST_FILES: i64 = 16;
const ONE_FILE: i64 = 1024 * 1024;
const MOST_ZIPS: i64 = 2;
/// The most an archive costs to read at all, which is also the most one
/// download of one is given.
const ZLIMIT: i64 = FLIMIT;

const OVER: &str = "Over the 10 MB limit of an issue.";
const FOVER: &str = "Over the 4 MB limit of an issue's files.";
const FBIG: &str = "Over the 1 MB limit of a file.";
const NOTEXT: &str = "Not valid UTF-8 text, or it holds a NUL byte.";
const TOOBIG: &str = "Over the 10 MB limit of a ticket.";
const NOT_FILE: &str = "Not a text file a cycle is given, and not an image.";
const OUTSIDE: &str = "A path outside the archive.";
const NOT_WRITTEN: &str = "the issue's images could not be written";

const NOT_OPENED: &str = "The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account.";
const NOT_REPLY: &str = "The reply was not written by an allowed account.";
const NOT_COMMENT: &str =
    "The comment was not written by an allowed account, or the start was not approved by one.";

/// What the instance gives a cycle. An archive holds anything, so it is taken
/// only where images or files are.
#[derive(Clone, Copy)]
pub struct Wants {
    pub images: bool,
    pub files: bool,
    pub zips: bool,
}

impl Wants {
    pub fn of(images: &str, files: &str, zips: &str) -> Wants {
        let (images, files) = (images != "0", files == "1");
        Wants {
            images,
            files,
            zips: zips == "1" && (files || images),
        }
    }

    fn other(self) -> &'static str {
        if !self.images {
            "Not a md, txt, csv, json, yaml, log, html, css or js file. This instance gives a cycle no images."
        } else if self.files {
            "Not a png, jpg, webp or gif image, nor a md, txt, csv, json, yaml, log, html, css or js file."
        } else {
            "Not a png, jpg, webp or gif image."
        }
    }

    fn notzip(self) -> &'static str {
        if !self.images && self.files {
            "Not a zip archive, nor a png, jpg, webp or gif image, nor a md, txt, csv, json, yaml, log, html, css or js file."
        } else {
            "Not a zip archive."
        }
    }
}

/// The type of a text file a cycle may be given. Not svg: as an image it is
/// code, and no step wants it as text yet.
pub fn text_type(ext: &str) -> Option<&'static str> {
    match ext {
        "md" => Some("text/markdown"),
        "txt" | "log" => Some("text/plain"),
        "csv" => Some("text/csv"),
        "json" => Some("application/json"),
        "yaml" | "yml" => Some("application/yaml"),
        "html" => Some("text/html"),
        "css" => Some("text/css"),
        "js" => Some("text/javascript"),
        _ => None,
    }
}

fn is_image_ext(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "gif")
}

/// The extension a link's name says, or None for a pasted file, which has no
/// name and is only ever an image.
pub fn link_ext(link: &str) -> Option<String> {
    let pasted = link
        .strip_prefix("https://github.com/")
        .is_some_and(|rest| rest.contains("/assets/"));
    if pasted {
        return None;
    }
    let name = link.rsplit('/').next().unwrap_or(link);
    let name = name.split('?').next().unwrap_or(name);
    let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
    Some(ext.to_ascii_lowercase())
}

fn last_part(link: &str) -> &str {
    link.rsplit('/').next().unwrap_or(link)
}

/// `$1` in a list of logins parted by single spaces, whole logins only.
pub fn allowed_login(allowed: &str, login: &str) -> bool {
    !login.is_empty() && format!(" {allowed} ").contains(&format!(" {login} "))
}

/// Why a file attach wrote is still not given: an svg, a nested archive, a
/// binary, or bytes that do not answer the extension of an image or an app name.
/// The last two say the kinds a cycle is given, because a name of either is a
/// name the instance can hand out as neither.
fn left_reason(path: &str, other: &'static str) -> &'static str {
    let ext = path.rsplit_once('.').map_or("", |(_, ext)| ext);
    match unpack::kind(&ext.to_ascii_lowercase()) {
        Some("image" | "app") => other,
        _ => NOT_FILE,
    }
}

/// A name as it stood in the tab-parted lines this was read from: an entry of
/// the archive is named in a manifest so, dots and all. A NUL never got through.
fn as_listed(name: &str) -> String {
    let listed = name
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\0', "");
    if listed.is_empty() {
        "-".to_string()
    } else {
        listed
    }
}

/// A path attach says it wrote: the characters of a cleaned name, under the
/// directory. It wrote them, so this only holds a program that went wrong.
fn inside(path: &str) -> bool {
    path.split('/').all(|part| {
        !part.is_empty()
            && !part.starts_with('.')
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    })
}

fn rm_f(path: &str) {
    let _ = fs::remove_file(path);
}

fn rm_rf(path: &str) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn chmod(path: &str, mode: u32) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

/// Why a child run of this program gave no answer. The word is its refusal
/// (unreadable, unwritable), timeout, crashed or absent, or empty.
#[derive(Debug, PartialEq)]
pub struct Failure {
    pub word: &'static str,
    pub why: String,
}

/// What this module asks of the world.
pub trait Net {
    /// Download a link into a file of at most `most` bytes: curl's exit status.
    fn fetch(&self, link: &str, file: &str, most: i64) -> i32;
    /// This program run as a child, for its answer.
    fn attach(&self, verb: &str, args: &[String]) -> Result<Value, Failure>;
}

/// curl's arguments, and whether it is given the token on standard input. The
/// token goes to github.com only and never on the command line; curl does not
/// pass it on when GitHub redirects to its storage.
pub fn curl_args(link: &str, file: &str, most: i64, token: &str) -> (Vec<String>, bool) {
    let mut args: Vec<String> = [
        "-sSfL",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        "3",
        "--max-time",
        "120",
        "--max-filesize",
    ]
    .map(String::from)
    .to_vec();
    args.push(most.to_string());
    args.extend(["-o".to_string(), file.to_string()]);
    args.extend(["-w".to_string(), "%{content_type}".to_string()]);
    let with_token = link.starts_with("https://github.com/") && !token.is_empty();
    if with_token {
        args.extend(["-H".to_string(), "@-".to_string()]);
    }
    args.push(link.to_string());
    (args, with_token)
}

/// curl and this program as children.
pub struct Children<'a> {
    pub token: &'a str,
    pub program: &'a str,
}

impl Net for Children<'_> {
    fn fetch(&self, link: &str, file: &str, most: i64) -> i32 {
        let (args, with_token) = curl_args(link, file, most, self.token);
        let mut curl = Command::new("curl");
        curl.args(args).stdout(Stdio::null());
        curl.stdin(if with_token {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        let Ok(mut child) = curl.spawn() else {
            return 127;
        };
        if let Some(mut stdin) = child.stdin.take() {
            // A curl that went without reading it fails on its own.
            let _ = writeln!(stdin, "Authorization: Bearer {}", self.token);
        }
        match child.wait() {
            Ok(status) => status.code().unwrap_or(1),
            Err(_) => 1,
        }
    }

    fn attach(&self, verb: &str, args: &[String]) -> Result<Value, Failure> {
        run_attach(self.program, verb, args)
    }
}

/// `tehas_attach` of lib/attachments.sh, for a child that holds nothing: only
/// PATH, to find timeout and the program.
fn run_attach(program: &str, verb: &str, args: &[String]) -> Result<Value, Failure> {
    let can_run =
        fs::metadata(program).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if !can_run {
        return Err(Failure {
            word: "absent",
            why: format!("attach: absent: {program} is not an executable file"),
        });
    }
    let mut child = Command::new("timeout");
    child
        .args(["-k", "2", "60", program, verb])
        .args(args)
        .env_clear()
        .stdin(Stdio::null());
    if let Ok(path) = std::env::var("PATH") {
        child.env("PATH", path);
    }
    let (rc, out) = match child.output() {
        Ok(o) => {
            let rc = o
                .status
                .code()
                .unwrap_or(128 + o.status.signal().unwrap_or(0));
            let mut text = o.stdout;
            text.extend(o.stderr);
            (rc, text)
        }
        Err(e) => (127, format!("timeout: {e}").into_bytes()),
    };
    let text = String::from_utf8_lossy(&out);
    let text = text.trim_end_matches('\n');
    if rc == 0 && !text.contains('\n') {
        let answer: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        if answer["api"] == 1 && answer["ok"] == true && answer["verb"] == verb {
            return Ok(answer);
        }
    }
    Err(failed(verb, rc, &out))
}

/// The line a log is told, as the shell cut it: the first line, in bytes,
/// control characters made '?', and which failure it was.
fn failed(verb: &str, rc: i32, out: &[u8]) -> Failure {
    let line = out.split(|b| *b == b'\n').next().unwrap_or(out);
    let word = match rc {
        1 if line.starts_with(b"attach: unreadable:") => "unreadable",
        1 if line.starts_with(b"attach: unwritable:") => "unwritable",
        124 | 137 => "timeout",
        101 => "crashed",
        _ => "",
    };
    let clean: Vec<u8> = line
        .iter()
        .take(300)
        .map(|b| if *b < 32 || *b == 127 { b'?' } else { *b })
        .collect();
    Failure {
        word,
        why: format!(
            "attach: {verb} exited {rc}: {}",
            String::from_utf8_lossy(&clean)
        ),
    }
}

#[cfg(test)]
pub fn failed_for_test(rc: i32, out: &[u8]) -> Failure {
    failed("sniff", rc, out)
}

#[cfg(test)]
pub fn run_attach_for_test(program: &str) -> Result<Value, Failure> {
    run_attach(program, "sniff", &[])
}

struct Sniffed {
    bytes: i64,
    sha256: String,
    image: Option<String>,
    zip: bool,
    text: bool,
}

/// How much of each kind a directory holds so far.
#[derive(Default)]
pub struct Counts {
    pub count: i64,
    pub used: i64,
    pub fcount: i64,
    pub fused: i64,
    pub zcount: i64,
}

/// One run of the rules over the links of an issue or of a reply.
pub struct Take<'a> {
    pub net: &'a dyn Net,
    pub dir: &'a str,
    pub wants: Wants,
    pub approved: bool,
    pub who: &'a str,
    pub allowed: &'a str,
    pub n: Counts,
    pub images: Vec<Ordered>,
    pub files: Vec<Ordered>,
    pub left: Vec<Ordered>,
}

impl Take<'_> {
    fn part(&self) -> String {
        format!("{}/.part", self.dir)
    }

    fn leave(&mut self, l: &Linked, reason: &str, path: Option<&str>) {
        self.left
            .push(manifest::left(&l.link, reason, &l.source, path));
    }

    /// Why a link is not taken on its author's word and the approval's. A reply
    /// was taken as the answer only from an allowed account, and the caller says
    /// so. A comment's author must be allowed now: the record says who they
    /// were, not that they still may.
    fn refused(&self, l: &Linked) -> Option<&'static str> {
        if l.source == "body" {
            let member = matches!(self.who, "OWNER" | "MEMBER" | "COLLABORATOR");
            (!self.approved && !member).then_some(NOT_OPENED)
        } else if l.source.starts_with("answer:") {
            (!self.approved).then_some(NOT_REPLY)
        } else {
            (!self.approved || !allowed_login(self.allowed, &l.by)).then_some(NOT_COMMENT)
        }
    }

    /// Fetch the links into the directory, in order. Err, with the reason, when
    /// a link could not be fetched or written and nothing may start.
    pub fn all(&mut self, links: &[Linked]) -> Result<(), String> {
        for l in links {
            self.one(l)?;
        }
        Ok(())
    }

    fn one(&mut self, l: &Linked) -> Result<(), String> {
        if let Some(why) = self.refused(l) {
            self.leave(l, why, None);
            return Ok(());
        }
        let other = self.wants.other();
        let ext = link_ext(&l.link).filter(|ext| !is_image_ext(ext));
        match ext {
            None if !self.wants.images => self.leave(l, other, None),
            None => return self.image(l),
            Some(ext) if ext == "zip" && self.wants.zips => return self.archive(l),
            Some(ext) => match text_type(&ext).filter(|_| self.wants.files) {
                Some(kind) => return self.text(l, &ext, kind),
                None => self.leave(l, other, None),
            },
        }
        Ok(())
    }

    fn fetch(&self, link: &str, most: i64) -> i32 {
        let part = self.part();
        rm_f(&part);
        self.net.fetch(link, &part, most)
    }

    fn sniff(&self, link: &str) -> Result<Sniffed, String> {
        let why = |what: &str| format!("{link} could not be checked ({what})");
        let v = self
            .net
            .attach("sniff", &[self.part()])
            .map_err(|f| why(&f.why))?;
        let sha256 = v["sha256"].as_str().unwrap_or_default().to_string();
        let hex = sha256.len() == 64
            && sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        match v["bytes"].as_i64() {
            Some(bytes) if bytes >= 0 && hex => Ok(Sniffed {
                bytes,
                sha256,
                image: v["image"].as_str().map(String::from),
                zip: v["zip"] == true,
                text: v["text"] == true,
            }),
            _ => Err(why("attach: sniff answered without its fields")),
        }
    }

    /// Move a checked file into the directory under the name made for it.
    fn place(&self, from: &str, name: &str) -> Result<(), String> {
        let to = format!("{}/{name}", self.dir);
        fs::rename(from, &to)
            .and_then(|()| chmod(&to, 0o644))
            .map_err(|_| NOT_WRITTEN.to_string())
    }

    fn image(&mut self, l: &Linked) -> Result<(), String> {
        let other = self.wants.other();
        if self.n.count >= MOST_IMAGES {
            self.leave(l, &format!("More than {MOST_IMAGES} images."), None);
            return Ok(());
        }
        let left = LIMIT - self.n.used;
        if left <= 0 {
            self.leave(l, OVER, None);
            return Ok(());
        }
        match self.fetch(&l.link, left) {
            0 => {}
            63 => {
                self.leave(l, OVER, None);
                return Ok(());
            }
            _ => return Err(format!("{} could not be fetched", l.link)),
        }
        let s = self.sniff(&l.link)?;
        if s.bytes > left {
            self.leave(l, OVER, None);
            return Ok(());
        }
        // A named file too: the bytes say what it is, never the name.
        let Some(ext) = s.image else {
            self.leave(l, other, None);
            return Ok(());
        };
        self.n.count += 1;
        self.n.used += s.bytes;
        let name = format!("image-{}.{ext}", self.n.count);
        self.place(&self.part(), &name)?;
        self.images.push(manifest::given(&Given {
            file: &name,
            link: &l.link,
            name: None,
            kind: &format!("image/{}", ext.replacen("jpg", "jpeg", 1)),
            bytes: s.bytes,
            sha256: &s.sha256,
            source: &l.source,
            path: None,
        }));
        Ok(())
    }

    fn text(&mut self, l: &Linked, ext: &str, kind: &str) -> Result<(), String> {
        if self.n.fcount >= MOST_FILES {
            self.leave(l, &format!("More than {MOST_FILES} files."), None);
            return Ok(());
        }
        let room = FLIMIT - self.n.fused;
        if room <= 0 {
            self.leave(l, FOVER, None);
            return Ok(());
        }
        let left = room.min(ONE_FILE);
        let over = if left == ONE_FILE { FBIG } else { FOVER };
        match self.fetch(&l.link, left) {
            0 => {}
            63 => {
                self.leave(l, over, None);
                return Ok(());
            }
            _ => return Err(format!("{} could not be fetched", l.link)),
        }
        let s = self.sniff(&l.link)?;
        if s.bytes > left {
            self.leave(l, over, None);
            return Ok(());
        }
        if !s.text {
            self.leave(l, NOTEXT, None);
            return Ok(());
        }
        self.n.fcount += 1;
        self.n.fused += s.bytes;
        let name = format!("file-{}.{ext}", self.n.fcount);
        self.place(&self.part(), &name)?;
        self.files.push(manifest::given(&Given {
            file: &name,
            link: &l.link,
            // The name as the link has it: the link's characters only.
            name: Some(last_part(&l.link)),
            kind,
            bytes: s.bytes,
            sha256: &s.sha256,
            source: &l.source,
            path: None,
        }));
        Ok(())
    }

    /// A link named .zip is an archive only if its first bytes say so. What is
    /// downloaded is paid for as the files it holds are: no more than the room
    /// left of the 4 MB of an issue's files, and no more than the 4 MB of one
    /// download. What is unpacked from it is paid for file by file.
    fn archive(&mut self, l: &Linked) -> Result<(), String> {
        if self.n.zcount >= MOST_ZIPS {
            self.leave(l, &format!("More than {MOST_ZIPS} archives."), None);
            return Ok(());
        }
        let room = FLIMIT - self.n.fused;
        if room <= 0 {
            self.leave(l, FOVER, None);
            return Ok(());
        }
        match self.fetch(&l.link, room.min(ZLIMIT)) {
            0 => {}
            63 => {
                let mb = ZLIMIT / 1024 / 1024;
                let why = format!(
                    "A zip of more than {mb} MB, or of more than is left of the 4 MB of an issue's files."
                );
                self.leave(l, &why, None);
                return Ok(());
            }
            _ => return Err(format!("{} could not be fetched", l.link)),
        }
        let s = self.sniff(&l.link)?;
        if !s.zip {
            rm_f(&self.part());
            self.leave(l, self.wants.notzip(), None);
            return Ok(());
        }
        self.n.zcount += 1;
        self.unpack(l)
    }

    /// The archive in .part, into a scratch directory beside it, then one file
    /// at a time through the checks of a fetched one. The archive is never kept
    /// and the scratch directory is removed whatever it held.
    fn unpack(&mut self, l: &Linked) -> Result<(), String> {
        let (part, zdir) = (
            self.part(),
            format!("{}/.unzip-{}", self.dir, self.n.zcount),
        );
        // The archive is read to the room its own download took: of five files
        // of 1 MB the cycle gets four and the fifth is left out by the room. The
        // 1 MB of one file is the ceiling of one entry.
        let room = ONE_FILE.max(ZLIMIT - self.n.fused);
        let most_files = MOST_FILES - self.n.fcount;
        // Made anew, the user's alone: what a run that died left under this name
        // is gone before anything is written into it.
        let made = rm_rf(&zdir).and_then(|()| {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new().mode(0o700).create(&zdir)
        });
        if made.is_err() {
            let _ = rm_rf(&part);
            let _ = rm_rf(&zdir);
            self.leave(l, self.wants.notzip(), None);
            return Ok(());
        }
        let args = [
            part.clone(),
            zdir.clone(),
            "--max-bytes".to_string(),
            room.to_string(),
            "--max-files".to_string(),
            most_files.to_string(),
        ];
        let unpacked = self.net.attach("unpack", &args);
        if unpacked.is_err() {
            let _ = rm_rf(&part);
            let _ = rm_rf(&zdir);
        }
        // What the archive did is left out with a reason, so its author cannot
        // stop a cycle with it. A program that is missing or did not answer is
        // the host's fault, and nothing starts on it.
        let answer = match unpacked {
            Ok(answer) => answer,
            Err(Failure {
                word: "unreadable" | "crashed",
                ..
            }) => {
                self.leave(l, self.wants.notzip(), None);
                return Ok(());
            }
            Err(Failure {
                word: "timeout", ..
            }) => {
                let why = "An archive that took more than 60 seconds to unpack.";
                self.leave(l, why, None);
                return Ok(());
            }
            Err(f) => return Err(format!("{} could not be unpacked ({})", l.link, f.why)),
        };
        rm_f(&part);
        let got = match answer["entries"].as_array() {
            Some(entries) => self.entries(l, entries, &zdir),
            None => Err(format!(
                "{} could not be unpacked (attach answered what cannot be read)",
                l.link
            )),
        };
        let _ = rm_rf(&zdir);
        got
    }

    fn entries(&mut self, l: &Linked, entries: &[Value], zdir: &str) -> Result<(), String> {
        let other = self.wants.other();
        // A name the directory already holds keeps its file: this one goes in
        // front of it with a number, counted over this archive.
        let mut n = 2;
        for e in entries {
            if let Some(skip) = e.get("skip") {
                // An entry attach refused, named as the archive named it: nothing
                // was ever written for it. One of more than the room it was read
                // at is said of that room, not of the 10 MB of the lab's ticket.
                let f = as_listed(skip.as_str().unwrap_or_default());
                let reason = e["reason"].as_str().unwrap_or_default();
                let reason = if reason == TOOBIG { FBIG } else { reason };
                self.leave(l, reason, Some(&f));
                continue;
            }
            let f = e["name"].as_str().unwrap_or_default();
            let from = format!("{zdir}/{f}");
            let plain = fs::symlink_metadata(&from).is_ok_and(|m| m.is_file());
            if !inside(f) || !plain {
                self.leave(l, OUTSIDE, Some(&as_listed(f)));
                continue;
            }
            let ext = f.rsplit_once('.').map_or("", |(_, ext)| ext);
            let ext = ext.to_ascii_lowercase();
            // Both say an image: the bytes are those of one and the extension is
            // one of the five a cycle is given as an image. An entry whose bytes
            // look like a png under another extension is the file its extension
            // says, as a link of the issue is.
            let image = is_image_ext(&ext) && e["image"].is_string();
            let (kind, name) = if image {
                (
                    format!("image/{}", ext.replacen("jpg", "jpeg", 1)),
                    format!(
                        "image-{}.{}",
                        self.n.count + 1,
                        ext.replacen("jpeg", "jpg", 1)
                    ),
                )
            } else if let Some(kind) = text_type(&ext).filter(|_| e["text"] == true) {
                (
                    kind.to_string(),
                    format!("file-{}.{ext}", self.n.fcount + 1),
                )
            } else {
                self.leave(l, left_reason(f, other), Some(f));
                continue;
            };
            let size = e["bytes"].as_i64().unwrap_or(0);
            if image {
                if self.n.count >= MOST_IMAGES {
                    self.leave(l, &format!("More than {MOST_IMAGES} images."), None);
                    continue;
                }
                if LIMIT - self.n.used <= 0 {
                    self.leave(l, OVER, None);
                    continue;
                }
            } else if self.n.fcount >= MOST_FILES {
                self.leave(l, &format!("More than {MOST_FILES} files."), None);
                continue;
            } else if FLIMIT - self.n.fused <= 0 || size > FLIMIT - self.n.fused {
                // Unpacked within what was left when it was fetched; a file that
                // no longer fits is left out, as a link over it is. The files
                // before it stand.
                self.leave(l, FOVER, None);
                continue;
            }
            let mut name = name;
            while Path::new(&format!("{}/{name}", self.dir)).exists() {
                name = format!("{n}-{}", without_number(&name));
                n += 1;
            }
            self.place(&from, &name)?;
            let g = Given {
                file: &name,
                link: &l.link,
                name: (!image).then(|| last_part(f)),
                kind: &kind,
                bytes: size,
                sha256: e["sha256"].as_str().unwrap_or_default(),
                source: &l.source,
                path: Some(f),
            };
            let entry = manifest::given(&g);
            if image {
                self.n.count += 1;
                self.n.used += size;
                self.images.push(entry);
            } else {
                self.n.fcount += 1;
                self.n.fused += size;
                self.files.push(entry);
            }
        }
        Ok(())
    }
}

/// `${name#[0-9]-}`
fn without_number(name: &str) -> &str {
    match name.as_bytes() {
        [d, b'-', ..] if d.is_ascii_digit() => &name[2..],
        _ => name,
    }
}

/// What `attach issue` is told.
pub struct IssueRun<'a> {
    pub issue: &'a str,
    pub dir: &'a str,
    pub repo: &'a str,
    pub approved: &'a str,
    pub record: &'a str,
    pub wants: Wants,
    pub comments: bool,
    pub allowed: &'a str,
    pub millwright: &'a str,
    /// The answer the caller already read for the issue; empty when it read none.
    pub data: &'a str,
}

fn read_issue(repo: &str, issue: &str) -> Option<String> {
    let out = Command::new("gh")
        .args(["api", &format!("repos/{repo}/issues/{issue}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Remake the directory from the issue's text, with a manifest.json. Err, with
/// the reason, when the issue's images cannot be given and nothing may start.
pub fn issue(run: &IssueRun, net: &dyn Net) -> Result<(), String> {
    const NOT_READ: &str = "the issue could not be read for its images";
    let dir = run.dir;
    rm_rf(dir)
        .and_then(|()| fs::create_dir_all(dir))
        .map_err(|_| "the directory for the issue's images could not be made".to_string())?;

    // One start asks GitHub for the issue once, whatever it is asked for.
    let read;
    let data = if run.data.trim_end_matches('\n').is_empty() {
        read = read_issue(run.repo, run.issue).ok_or(NOT_READ)?;
        read.as_str()
    } else {
        run.data
    };
    let answer: Value = serde_json::from_str(data).map_err(|_| NOT_READ)?;
    if !answer.is_object() || !(answer["body"].is_null() || answer["body"].is_string()) {
        return Err(NOT_READ.to_string());
    }

    // The links as the approval saw them, with a record: no record of which
    // links were approved is no ground to fetch any.
    let (approval, links) = if run.record.is_empty() {
        let source = |link| Linked {
            source: "body".to_string(),
            link,
            by: String::new(),
        };
        let body = answer["body"].as_str().unwrap_or_default();
        let links = given::text_links(body).into_iter().map(source).collect();
        (Ordered::Plain(Value::Null), links)
    } else {
        let values = given::values_of(run.record).map_err(|_| NOT_READ)?;
        let record = values.first().filter(|r| r.is_object()).ok_or(NOT_READ)?;
        let plain = |key: &str| Ordered::Plain(record[key].clone());
        let approval = Ordered::map([
            ("at", plain("at")),
            ("by", plain("by")),
            ("how", plain("how")),
        ]);
        (approval, given::record_links(record, run.comments))
    };

    let mut take = Take {
        net,
        dir,
        wants: run.wants,
        approved: run.approved == "approved",
        who: answer["author_association"].as_str().unwrap_or_default(),
        allowed: run.allowed,
        n: Counts::default(),
        images: Vec::new(),
        files: Vec::new(),
        left: Vec::new(),
    };
    take.all(&links)?;

    let written = || NOT_WRITTEN.to_string();
    let mut context = None;
    if !run.record.is_empty() {
        let text = given::retry_context_of(run.record, run.comments, run.millwright)
            .map_err(|_| written())?;
        if let Some(text) = text {
            let path = format!("{dir}/retry-context.md");
            fs::write(&path, format!("{text}\n"))
                .and_then(|()| chmod(&path, 0o644))
                .map_err(|_| written())?;
            context = Some("retry-context.md");
        }
    }
    let number: Value = serde_json::from_str(run.issue).map_err(|_| written())?;
    let manifest = manifest::of_issue(
        take.images,
        run.wants.files.then_some(take.files),
        take.left,
        number,
        approval,
        context,
    );
    let path = format!("{dir}/manifest.json");
    fs::write(&path, manifest.pretty()).map_err(|_| written())?;
    rm_f(&format!("{dir}/.part"));
    chmod(dir, 0o755)
        .and_then(|()| chmod(&path, 0o644))
        .map_err(|_| written())
}

/// What `attach answer` is told.
pub struct AnswerRun<'a> {
    pub dir: &'a str,
    pub text: &'a str,
    pub id: &'a str,
    pub wants: Wants,
}

/// The counts of the files a manifest lists: numbering goes on from them, under
/// the same limits counted with them.
fn counted(images: &[Value], files: &[Value]) -> Result<Counts, &'static str> {
    let sum = |list: &[Value]| -> Result<i64, &'static str> {
        let mut total = 0.0;
        for e in list {
            total += given::get(e, "bytes")?.as_f64().unwrap_or(0.0);
        }
        Ok(total as i64)
    };
    let mut archives: Vec<&Value> = Vec::new();
    for e in images.iter().chain(files) {
        let zip = &e["zip"];
        if !zip.is_null() && !archives.contains(&zip) {
            archives.push(zip);
        }
    }
    Ok(Counts {
        count: images.len() as i64,
        used: sum(images)?,
        fcount: files.len() as i64,
        fused: sum(files)?,
        zcount: archives.len() as i64,
    })
}

/// The manifest as its lists, or None for one of another shape.
fn session_lists(m: &Value) -> Option<(&Vec<Value>, &Vec<Value>)> {
    if !m["leftOut"].is_array() {
        return None;
    }
    Some((
        m["images"].as_array()?,
        given::or_none(&m["files"]).as_array()?,
    ))
}

/// A planning session's directory grows by what each reply links. The reply is
/// the text the dispatcher accepted as the answer, from an allowed account, so
/// its links are approved as the label approves the body's. A link already there
/// is not fetched again. The work is done on a copy, which replaces the directory
/// only when it is whole. Answers the lines that say what was added and what was
/// left out; Err, with the reason and the directory as it was, when a link could
/// not be fetched.
pub fn answer(run: &AnswerRun, net: &dyn Net) -> Result<(String, String), String> {
    const NOT_READ: &str = "the session's images could not be read";
    let (dir, source) = (run.dir, format!("answer:{}", run.id));
    let path = format!("{dir}/manifest.json");
    let text = given::read(&path).map_err(|_| NOT_READ)?;
    let m: Value = serde_json::from_str(&text).map_err(|_| NOT_READ)?;
    let kept: Ordered = serde_json::from_str(&text).map_err(|_| NOT_READ)?;
    let (images, files) = session_lists(&m).ok_or(NOT_READ)?;
    let n = counted(images, files).map_err(|_| NOT_READ)?;

    let copy = format!("{dir}.new");
    let copied = rm_rf(&copy).is_ok()
        && Command::new("cp")
            .args(["-r", dir, &copy])
            .stdin(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
    if !copied {
        return Err("the session's images could not be copied".to_string());
    }

    let have: Vec<&str> = images
        .iter()
        .chain(files)
        .filter_map(|e| e["link"].as_str())
        .collect();
    let reply = given::read(run.text).unwrap_or_default();
    let links: Vec<Linked> = given::text_links(&reply)
        .into_iter()
        .filter(|l| !have.contains(&l.as_str()))
        .map(|link| Linked {
            source: source.clone(),
            link,
            by: String::new(),
        })
        .collect();

    let mut take = Take {
        net,
        dir: &copy,
        wants: run.wants,
        approved: true,
        who: "",
        allowed: "",
        n,
        images: Ordered::items(kept.get("images"))
            .iter()
            .map(Ordered::clone)
            .collect(),
        files: Ordered::items(kept.get("files"))
            .iter()
            .map(Ordered::clone)
            .collect(),
        left: Vec::new(),
    };
    if let Err(why) = take.all(&links) {
        let _ = rm_rf(&copy);
        return Err(why);
    }

    let mut whole = kept;
    let mut left: Vec<Ordered> = Ordered::items(whole.get("leftOut")).to_vec();
    left.append(&mut take.left);
    whole.set("images", Ordered::List(take.images));
    if run.wants.files {
        whole.set("files", Ordered::List(take.files));
    }
    whole.set("leftOut", Ordered::List(left));
    let new_path = format!("{copy}/manifest.json");
    if fs::write(&new_path, whole.pretty()).is_err() {
        let _ = rm_rf(&copy);
        return Err(NOT_WRITTEN.replace("issue's", "session's"));
    }
    rm_f(&format!("{copy}/.part"));

    let ours = |e: &&Ordered| e.shown("source") == source;
    let list = |key| Ordered::items(whole.get(key)).iter();
    let new: Vec<String> = list("images")
        .chain(list("files"))
        .filter(ours)
        .map(|e| format!("- {}, from {}", e.shown("file"), e.shown("link")))
        .collect();
    let out: Vec<String> = list("leftOut")
        .filter(ours)
        .map(|e| format!("- {}: {}", e.shown("link"), e.shown("reason")))
        .collect();

    let old = format!("{dir}.old");
    let swapped = chmod(&copy, 0o755)
        .and_then(|()| chmod(&new_path, 0o644))
        .and_then(|()| rm_rf(&old))
        .and_then(|()| fs::rename(dir, &old))
        .and_then(|()| fs::rename(&copy, dir))
        .and_then(|()| rm_rf(&old));
    if swapped.is_err() {
        if !Path::new(dir).exists() {
            let _ = fs::rename(&old, dir);
        }
        let _ = rm_rf(&copy);
        return Err(NOT_WRITTEN.replace("issue's", "session's"));
    }
    Ok((new.join("\n"), out.join("\n")))
}
