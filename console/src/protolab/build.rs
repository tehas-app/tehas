//! One pass of `tehasbot protolab`: which tickets are at the stage, what each
//! one holds, and the order of the work. Settings come from the environment
//! libexec/protolab loaded.

use super::{front_page, live_file, number_of, refs, ticket_page};
use crate::edge;
use crate::forge::{gh_program, repo, runnable, CALL_BOUND};
use crate::jq::{obj, J};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::forge::{Bounded, Forge, GhRunner, GitHub};

const LABEL: &str = "tehas:prototype";
const LIMIT: i64 = 10 * 1024 * 1024;
const MAX_FILES: i64 = 40;
const OVER: &str = "Over the 10 MB limit of a ticket.";
const NOT_HOSTED: &str = "Not a type the lab hosts.";

/// One line of what a pass did. A log that went away does not stop the pass.
fn say(line: &str) {
    let _ = writeln!(io::stdout(), "{line}");
}

/// The settings of a pass.
struct Lab {
    home: PathBuf,
    site: PathBuf,
    work: PathBuf,
    repo: String,
    allowed: String,
    domain: String,
    url: String,
    token: String,
}

pub fn run() -> u8 {
    let setting = |name: &str| env::var(name).unwrap_or_default();
    let (home, allowed) = (setting("TEHAS_HOME"), setting("TEHAS_ALLOWED_LABELERS"));
    let Some(repo) = repo().filter(|_| !home.is_empty() && !allowed.is_empty()) else {
        eprintln!(
            "tehasbot protolab: TEHAS_HOME, TEHAS_REPO and TEHAS_ALLOWED_LABELERS must be set"
        );
        return 78;
    };
    let home = PathBuf::from(home);
    let site = home.join("protolab");
    let live = home.join("live");
    if let Err(e) = fs::create_dir_all(&site).and_then(|()| fs::create_dir_all(&live)) {
        eprintln!("tehasbot protolab: cannot make its directories: {e}");
        return 1;
    }
    let Some(_lock) = edge::lock(&home, "protolab", None) else {
        say("another tehasbot protolab is running");
        return 0;
    };
    let work = site.join(".work");
    if let Err(e) = edge::remove(&work).and_then(|()| fs::create_dir_all(&work)) {
        eprintln!("tehasbot protolab: cannot make {}: {e}", work.display());
        return 1;
    }
    let mut lab = Lab {
        home,
        site,
        work,
        repo,
        allowed,
        domain: setting("TEHAS_DOMAIN"),
        url: setting("TEHAS_PROTOLAB_URL"),
        token: String::new(),
    };
    let rc = pass(&mut lab, &live);
    let _ = edge::remove(&lab.work);
    rc
}

fn pass(lab: &mut Lab, live: &Path) -> u8 {
    let program = gh_program(lab.home.to_str(), &env::var("PATH").unwrap_or_default())
        .unwrap_or_else(|| "gh".to_string());
    let mut gh = Bounded::new(program.clone(), CALL_BOUND);
    lab.token = token(&mut gh);
    let home_url = home_url(&lab.domain, &lab.url);
    let generator = generator(&lab.domain);

    // The bot's own login, asked once before anything of a ticket is read,
    // built or removed: whatever a cycle labels, it labels as the bot. A login
    // that cannot be read ends the pass too, because no label below could then
    // be checked against the allowed list. A GitHub App's token cannot ask GET
    // /user, so an app's login is the setting.
    let bot = match env::var("TEHAS_BOT_LOGIN").unwrap_or_default() {
        set if set.is_empty() => login(&mut gh),
        set => set,
    };
    if bot.is_empty() {
        say("the bot's own login could not be read, nothing built or removed");
        return 0;
    }
    if allowed(&bot, &lab.allowed) {
        say(&format!(
            "ERROR: the bot account {bot} is in TEHAS_ALLOWED_LABELERS; the bot never authorises work"
        ));
        return 78;
    }

    // The first hundred; a failed or odd answer leaves everything as it is.
    let list = format!(
        "repos/{}/issues?state=open&labels={LABEL}&per_page=100",
        lab.repo
    );
    let Some(tickets) = gh.run(&["api", &list]).ok().and_then(|t| tickets(&t)) else {
        return 0;
    };

    let mut forge = GitHub::new(
        lab.repo.clone(),
        Box::new(Bounded::new(program, CALL_BOUND)),
    );
    let mut keep = Vec::new();
    for ticket in &tickets {
        let n = ticket["number"].as_u64().unwrap_or_default();
        // The label is checked before the ticket is published: who applied it
        // most recently. An agent in a cycle holds the bot's token, and the bot
        // can label.
        let who = match forge.applied_by(n, LABEL) {
            Ok(approval) => approval.map(|a| a.actor).unwrap_or_default(),
            Err(_) => {
                // Not a refusal: nothing is built, and the removal keeps it.
                say(&format!(
                    "#{n}: its label events could not be read, nothing built or removed"
                ));
                keep.push(n.to_string());
                continue;
            }
        };
        if !allowed(&who, &lab.allowed) {
            say(&denied(n, &who));
            continue;
        }
        keep.push(n.to_string());
        let stamp = format!("{generator} {}", raw(&ticket["updatedAt"]));
        let place = lab.site.join(n.to_string());
        if fs::read_to_string(place.join(".stamp"))
            .is_ok_and(|had| had.trim_end_matches('\n') == stamp)
        {
            continue;
        }
        match build(lab, &mut gh, n, ticket, &stamp, &home_url) {
            Some(line) => say(&line),
            None => say(&format!("#{n}: could not be read, left as it was")),
        }
    }

    for n in edge::numbers(&lab.site) {
        if !keep.contains(&n) && edge::remove(&lab.site.join(&n)).is_ok() {
            say(&format!("#{n}: no longer at the prototype stage, removed"));
        }
    }

    let metas = metas(&lab.site);
    let now = tehas_core::time::iso(tehas_core::time::now());
    let front = front_page(&metas, &lab.domain, &home_url, LIMIT);
    let list = live_file(&metas, &now, &lab.url, &lab.domain, LIMIT);
    let written = edge::put(&lab.site.join("index.html"), front.as_bytes())
        .and_then(|()| edge::put(&live.join("protolab.json"), list.as_bytes()));
    if let Err(e) = written {
        eprintln!("tehasbot protolab: cannot write the list: {e}");
        return 1;
    }
    0
}

/// GH_TOKEN, or what gh holds. Fetching takes it on standard input.
fn token(gh: &mut Bounded) -> String {
    match env::var("GH_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => gh
            .run(&["auth", "token"])
            .map(|out| out.trim_end_matches('\n').to_string())
            .unwrap_or_default(),
    }
}

fn login(gh: &mut Bounded) -> String {
    gh.run(&["api", "user"])
        .ok()
        .and_then(|out| serde_json::from_str::<Value>(&out).ok())
        .and_then(|me| me["login"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// With a domain every ticket has a name of its own and the list is elsewhere.
fn home_url(domain: &str, url: &str) -> String {
    if domain.is_empty() {
        "/".to_string()
    } else {
        format!("{url}/")
    }
}

/// What every stamp starts with: the console writes the pages, so a page whose
/// words changed is built again. Read through /proc, which still works while an
/// update has replaced the file.
fn generator(domain: &str) -> String {
    let bytes = fs::read("/proc/self/exe").unwrap_or_default();
    let mut hash = DefaultHasher::new();
    bytes.hash(&mut hash);
    stamp_of(hash.finish(), bytes.len(), domain)
}

fn stamp_of(hash: u64, len: usize, domain: &str) -> String {
    let domain = if domain.is_empty() { "path" } else { domain };
    format!("{hash:016x}-{len}-{domain}")
}

/// Whole logins, split by spaces, exact case. An empty name is nobody.
fn allowed(who: &str, list: &str) -> bool {
    !who.is_empty() && list.split(' ').any(|login| login == who)
}

fn denied(n: u64, who: &str) -> String {
    let who = if who.is_empty() { "unknown" } else { who };
    format!("#{n}: {LABEL} was applied by '{who}', not an allowed account")
}

/// A value as `jq -r` prints it.
fn raw(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The tickets of GitHub's list, newest first. None when it is no list. A pull
/// request is no ticket, nor is a row without a number or a title.
fn tickets(answer: &str) -> Option<Vec<Value>> {
    let Value::Array(rows) = serde_json::from_str(answer).ok()? else {
        return None;
    };
    let mut found: Vec<Value> = rows
        .iter()
        .filter(|row| row["pull_request"].is_null())
        .filter_map(|row| {
            let title: String = row["title"].as_str()?.chars().take(180).collect();
            let body = if row["body"].is_null() {
                Value::from("")
            } else {
                row["body"].clone()
            };
            Some(serde_json::json!({
                "number": row["number"].as_u64()?,
                "title": title,
                "url": row["html_url"],
                "author": row["user"]["login"],
                "association": row["author_association"],
                "updatedAt": row["updated_at"],
                "comments": row["comments"],
                "body": body,
            }))
        })
        .collect();
    found.sort_by_key(|row| std::cmp::Reverse(number_of(row)));
    Some(found)
}

/// Every ticket's meta, newest first. One that cannot be read is left out, so
/// it does not cost the list its other rows.
fn metas(site: &Path) -> Vec<Value> {
    let mut names: Vec<String> = fs::read_dir(site)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    let mut found: Vec<Value> = names
        .iter()
        .filter_map(|name| fs::read_to_string(site.join(name).join(".meta.json")).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    found.sort_by_key(|row| std::cmp::Reverse(number_of(row)));
    found
}

/// What one ticket holds so far.
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

/// Make the ticket's directory in the work area and put it in place. The line
/// for the log, or None when it could not be read; what is there is then kept.
fn build(
    lab: &Lab,
    gh: &mut Bounded,
    n: u64,
    ticket: &Value,
    stamp: &str,
    home_url: &str,
) -> Option<String> {
    let dir = lab.work.join(n.to_string());
    fs::create_dir_all(dir.join("files")).ok()?;
    let replies = if has_replies(&ticket["comments"]) {
        let path = format!("repos/{}/issues/{n}/comments?per_page=100", lab.repo);
        let answer = gh.run(&["api", &path]).ok()?;
        match serde_json::from_str(&answer).ok()? {
            Value::Array(rows) => rows,
            _ => return None,
        }
    } else {
        Vec::new()
    };
    let found = refs(ticket, &replies);

    let mut held = Held::default();
    for link in &found.fetch {
        take(lab, &dir, link, &mut held)?;
    }

    let (bytes, files, skipped) = (held.used, held.files.len(), held.skipped.len());
    let of = |key: &str| J::parse(&ticket[key].to_string()).unwrap_or(J::Null);
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
    let page = ticket_page(
        &row,
        row["files"].as_array()?,
        row["skipped"].as_array()?,
        found.outside,
        home_url,
        LIMIT,
    );
    fs::write(dir.join("index.html"), page).ok()?;
    let _ = fs::remove_file(dir.join(".part"));
    // Without a stamp the next run builds the ticket again.
    if !held.retry {
        let _ = fs::write(dir.join(".stamp"), format!("{stamp}\n"));
    }
    edge::place(&dir, &lab.site.join(n.to_string())).ok()?;
    Some(format!(
        "#{n}: {files} files, {bytes} bytes, {skipped} left out"
    ))
}

/// `.comments // 0` is not 0.
fn has_replies(comments: &Value) -> bool {
    match comments {
        Value::Null | Value::Bool(false) => false,
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

/// One link of the ticket, fetched into the directory or said to be left out.
/// None when the ticket cannot be built at all.
fn take(lab: &Lab, dir: &Path, link: &str, held: &mut Held) -> Option<()> {
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
    if held.count >= MAX_FILES {
        held.skip(&name, &more_than());
        return Some(());
    }
    let left = LIMIT - held.used;
    if left <= 0 {
        held.skip(&name, OVER);
        return Some(());
    }

    let part = dir.join(".part");
    let _ = fs::remove_file(&part);
    let (code, content_type) = fetch(link, &part, left, &lab.token);
    match code {
        0 => {}
        63 => {
            held.skip(&name, OVER);
            return Some(());
        }
        22 => {
            held.skip(&name, "GitHub did not hand it out.");
            return Some(());
        }
        _ => {
            held.skip(&name, "Could not be fetched. The lab tries again.");
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
        unpack(lab, dir, &name, left, held);
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

fn more_than() -> String {
    format!("More than {MAX_FILES} files.")
}

/// The last part of the address, kept to the characters a file name may hold,
/// so it cannot leave the ticket's directory.
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

/// curl's exit status and the content type it printed. The token goes to
/// github.com only, on standard input and never on the command line; curl does
/// not pass it on when GitHub redirects to its storage. 63 is a file over
/// `most`, 22 one GitHub did not hand out.
fn fetch(link: &str, file: &Path, most: i64, token: &str) -> (i32, String) {
    let mut curl = Command::new("curl");
    curl.args(["-sSfL", "--proto", "=https", "--proto-redir", "=https"])
        .args(["--max-redirs", "3", "--max-time", "120"])
        .arg("--max-filesize")
        .arg(most.to_string())
        .arg("-o")
        .arg(file)
        .args(["-w", "%{content_type}"])
        .stdout(Stdio::piped());
    let header = link.starts_with("https://github.com/") && !token.is_empty();
    if header {
        curl.args(["-H", "@-"]).stdin(Stdio::piped());
    } else {
        curl.stdin(Stdio::null());
    }
    let Ok(mut child) = curl.arg(link).spawn() else {
        return (127, String::new());
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "Authorization: Bearer {token}");
    }
    match child.wait_with_output() {
        Ok(done) => (
            status_of(done.status),
            String::from_utf8_lossy(&done.stdout)
                .trim_end_matches('\n')
                .to_string(),
        ),
        Err(_) => (127, String::new()),
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
fn unpack(lab: &Lab, dir: &Path, name: &str, left: i64, held: &mut Held) {
    let base = name.rsplit_once('.').map_or(name, |(base, _)| base);
    let base = if base.is_empty() { "zip" } else { base };
    let files = dir.join("files");
    let base = free_name(&files, base.to_string());
    let folder = files.join(&base);
    let part = dir.join(".part");
    let answer = attach(
        &lab.home.join("libexec/attach"),
        &part,
        &folder,
        left,
        MAX_FILES - held.count,
    );
    let _ = fs::remove_file(&part);
    let entries = match answer {
        Unpacked::Entries(entries) => entries,
        failed => {
            let _ = edge::remove(&folder);
            let reason = match failed {
                Unpacked::NoHost => "This host cannot unpack a zip.",
                _ => "Not a zip the lab can read.",
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
    if !runnable(bin) {
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
    let line: String = line
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(300)
        .collect();
    eprintln!("tehas: attach: unpack exited {code}: {line}");
    Unpacked::NoHost
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("protolab-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_allowed_list_is_whole_logins_in_exact_case() {
        assert!(allowed("alice", "alice bob"));
        assert!(allowed("bob", "alice bob"));
        assert!(!allowed("ali", "alice bob"));
        assert!(!allowed("Alice", "alice bob"));
        assert!(!allowed("", "alice  bob"));
    }

    #[test]
    fn the_denied_line_names_the_account_or_says_unknown() {
        assert_eq!(
            denied(7, "mallory"),
            "#7: tehas:prototype was applied by 'mallory', not an allowed account"
        );
        assert_eq!(
            denied(7, ""),
            "#7: tehas:prototype was applied by 'unknown', not an allowed account"
        );
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
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_stamp_says_the_console_and_the_domain() {
        assert_eq!(stamp_of(255, 7, ""), "00000000000000ff-7-path");
        assert_eq!(stamp_of(255, 7, "a.org"), "00000000000000ff-7-a.org");
    }

    #[test]
    fn the_list_is_issues_newest_first_and_nothing_else() {
        assert!(tickets("{\"message\":\"Bad credentials\"}").is_none());
        assert!(tickets("nope").is_none());
        let got = tickets(
            r#"[{"number":3,"title":"c","body":null,"user":{"login":"a"}},
                {"number":11,"title":"a"},
                {"number":4,"title":"pull","pull_request":{}},
                {"number":5,"title":null},
                {"title":"no number"}]"#,
        )
        .unwrap();
        let numbers: Vec<i64> = got.iter().map(number_of).collect();
        assert_eq!(numbers, [11, 3]);
        assert_eq!(got[1]["body"], "");
        assert_eq!(got[1]["author"], "a");
        let long = format!("[{{\"number\":1,\"title\":\"{}\"}}]", "ä".repeat(200));
        let cut = tickets(&long).unwrap();
        assert_eq!(cut[0]["title"].as_str().unwrap().chars().count(), 180);
    }

    #[test]
    fn replies_are_read_unless_the_count_is_none() {
        assert!(has_replies(&Value::from(3)));
        assert!(!has_replies(&Value::from(0)));
        assert!(!has_replies(&Value::Null));
    }

    #[test]
    fn a_broken_meta_costs_the_list_only_its_own_row() {
        let site = scratch("metas");
        for (name, text) in [
            ("3", "{\"number\":3}"),
            ("11", "{\"number\":11}"),
            ("7", "not json"),
            (".old", "{\"number\":99}"),
        ] {
            fs::create_dir_all(site.join(name)).unwrap();
            fs::write(site.join(name).join(".meta.json"), text).unwrap();
        }
        let numbers: Vec<i64> = metas(&site).iter().map(number_of).collect();
        assert_eq!(numbers, [11, 3]);
        fs::remove_dir_all(&site).unwrap();
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
