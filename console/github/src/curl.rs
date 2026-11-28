//! Plain HTTP to GitHub: the one owner of it in the adapter. [`Curl`] asks
//! `https://api.github.com/` alone, with GETs only, and holds no token,
//! header of the caller's or config file: its arguments are made here
//! whole, it is started with an environment of [`ENV`] alone, and `-q`
//! keeps curl from reading a `.curlrc`. Its time limit and its size cap
//! are its own. What an answer means is the handle's, in
//! `port/anonymous.rs`.
//!
//! [`Authorised`] is the same curl with a credential: a bearer token on a
//! GET of the API, or a sign-in's code traded with the App's client at
//! `https://github.com/login/oauth/access_token`. The same arguments, host
//! check, limits and cap; the credential goes in a config on curl's
//! standard input, never on its command line or in its environment, and a
//! value that could break out of the config's quotes is refused with no
//! request.
//!
//! A public release file is fetched by [`Curl::fetch`]: a GET of one
//! address on exactly one of [`HOSTS`], `https` alone, curl following no
//! redirect (the handle follows at most [`REDIRECTS`], each to one of
//! [`HOSTS`]). The body goes into a file the caller gives, held to a byte
//! cap while it is read, within a time limit of at most [`DOWNLOAD_LIMIT`].
//!
//! This module is the one owner of the host check and of the caps, for
//! both: every request of either is started by `spawn`, which refuses an
//! address whose parsed host is not one of [`HOSTS`]; every argument list
//! is made by `args`, with its time limit and size cap; and every curl
//! that overruns its time is stopped by the same grace. Each variant then
//! narrows what it may ask: the anonymous GET the API's paths alone, the
//! fetch [`address_ok`], the authorised curl the API's paths and
//! [`TRADE`].

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use tehas_ports::{Outcome, Passing, Refusal, Sent, Unread};

/// The one host it asks, as the start of every address.
pub(crate) const API: &str = "https://api.github.com/";

/// The API's host, whose answers alone carry the anonymous budget.
pub(crate) const API_HOST: &str = "api.github.com";

/// The hosts any request of this module may reach, each whole: an
/// address's parsed host is one of these exactly, never matched by its
/// end. The API answers a release asset asked by its id with a redirect to
/// the release assets host (seen on an anonymous read of a public release,
/// 2026-11-24); the objects host is GitHub's older place for the same
/// files; `github.com` is a release's download page and [`TRADE`]'s host.
pub const HOSTS: [&str; 4] = [
    "api.github.com",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// The most redirects a release file's fetch follows, each held to
/// [`address_ok`]. curl itself follows none.
pub const REDIRECTS: usize = 2;

/// The most time one release file's fetch takes, its redirects in.
pub const DOWNLOAD_LIMIT: Duration = Duration::from_secs(600);

/// The most bytes of a fetch's headers.
const HEAD_CAP: u64 = 64 * 1024;

/// Where [`Authorised`] trades a sign-in's code: GitHub's web host, this
/// one address alone.
const TRADE: &str = "https://github.com/login/oauth/access_token";

/// The environment curl runs in, whole: nothing of the caller's reaches it,
/// so no token, no proxy and no config file's place.
pub const ENV: [(&str, &str); 2] = [("PATH", "/usr/bin:/bin"), ("LC_ALL", "C")];

/// The most time one request takes, connecting and answering.
const LIMIT: Duration = Duration::from_secs(30);

/// The most time it takes to connect.
const CONNECT: u64 = 10;

/// The most bytes of one answer, its headers in: a page of a hundred
/// releases with their notes is about 1.5 MB.
const CAP: u64 = 16 * 1024 * 1024;

/// How long past its own time limit curl is waited for before it is
/// stopped.
const GRACE: Duration = Duration::from_secs(5);

/// curl, asking GitHub's API with no credential. It has no field a token
/// could be kept in.
#[derive(Debug, Clone)]
pub struct Curl {
    program: PathBuf,
    cap: u64,
}

impl Default for Curl {
    fn default() -> Self {
        Curl::new()
    }
}

/// One answer: its status, its headers with their names in lower case, and
/// its body. Its `Debug` shows the status and the headers' names alone: a
/// trade's body holds the person's token.
pub(crate) struct Answer {
    pub(crate) status: u16,
    headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
    /// The bytes a fetch wrote into its file; 0 for a GET.
    pub(crate) written: u64,
}

impl std::fmt::Debug for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("Answer")
            .field("status", &self.status)
            .field("headers", &names)
            .field("body", &format_args!("{} bytes", self.body.len()))
            .finish()
    }
}

impl Answer {
    /// The value of a header, by its name in lower case.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

impl Curl {
    /// curl as the host has it, in `/usr/bin` or `/bin`.
    pub fn new() -> Curl {
        Curl::at("curl")
    }

    /// The program at `program` in curl's place, as a test's stub: it is
    /// given the same arguments and environment.
    pub fn at(program: impl Into<PathBuf>) -> Curl {
        Curl {
            program: program.into(),
            cap: CAP,
        }
    }

    /// The same, with answers capped at `cap` bytes.
    pub fn capped(self, cap: u64) -> Curl {
        Curl { cap, ..self }
    }

    /// The arguments of a GET of `path` under [`API`] within `limit`, or
    /// `Refused(Invalid)` for a path that could leave the API's host.
    pub fn argv(&self, path: &str, limit: Duration) -> Result<Vec<String>, Outcome> {
        if !path_ok(path) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        Ok(self.args(
            limit,
            &[
                "--header",
                "Accept: application/vnd.github+json",
                "--header",
                "X-GitHub-Api-Version: 2022-11-28",
            ],
            &format!("{API}{path}"),
            LIMIT,
            self.cap,
        ))
    }

    /// The arguments every request has, with `headers` before the user
    /// agent and the address last: its time limit `limit` held to `most`,
    /// and its size cap `cap`.
    fn args(
        &self,
        limit: Duration,
        headers: &[&str],
        address: &str,
        most: Duration,
        cap: u64,
    ) -> Vec<String> {
        let secs = limit.min(most).as_secs().max(1);
        let mut argv: Vec<String> = [
            "-q",
            "--silent",
            "--include",
            "--proto",
            "=https",
            "--max-redirs",
            "0",
            "--connect-timeout",
            &CONNECT.min(secs).to_string(),
            "--max-time",
            &secs.to_string(),
            "--max-filesize",
            &cap.to_string(),
        ]
        .map(String::from)
        .to_vec();
        argv.extend(headers.iter().map(|h| h.to_string()));
        argv.extend(["--user-agent", "tehas", address].map(String::from));
        argv
    }

    /// One GET of `path` under [`API`], within `limit` and the cap. A
    /// failure of the request is an outcome; an answer of any status is
    /// the caller's to read.
    pub(crate) fn get(&self, path: &str, limit: Duration) -> Result<Answer, Outcome> {
        let argv = self.argv(path, limit)?;
        self.send(&argv, None, limit)
    }

    /// curl with `argv`, and `config` on its standard input when given
    /// (`argv` then reads it with `--config -`), within `limit` and the
    /// cap.
    fn send(
        &self,
        argv: &[String],
        config: Option<&str>,
        limit: Duration,
    ) -> Result<Answer, Outcome> {
        let (mut child, out) = self.spawn(argv, config)?;
        let cap = self.cap;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let read = out.take(cap + 1).read_to_end(&mut bytes).is_ok();
            let _ = tx.send((read, bytes));
        });
        let Ok((read, bytes)) = rx.recv_timeout(limit.min(LIMIT) + GRACE) else {
            stop(&mut child);
            return Err(unavailable(Passing::Timeout));
        };
        if bytes.len() as u64 > cap {
            stop(&mut child);
            return Err(Outcome::Unread(Unread::TooLarge));
        }
        finish(&mut child, read)?;
        parse(&bytes).ok_or(Outcome::Unread(Unread::Malformed))
    }

    /// curl started with `argv`, and `config` written to its standard input
    /// when given, in an environment of [`ENV`] alone: the one place any
    /// request of this module starts. An address (`argv`'s last) whose
    /// parsed host is not one of [`HOSTS`] is `Refused(Invalid)`, and
    /// nothing runs.
    fn spawn(
        &self,
        argv: &[String],
        config: Option<&str>,
    ) -> Result<(Child, ChildStdout), Outcome> {
        let host_ok = argv
            .last()
            .and_then(|a| parse_address(a))
            .is_some_and(|p| HOSTS.contains(&p.host));
        if !host_ok {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let mut child = Command::new(&self.program)
            .args(argv)
            .env_clear()
            .envs(ENV)
            .stdin(if config.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable(Passing::Unreachable))?;
        if let Some(config) = config {
            // Dropped once written, so curl reads to its end.
            let wrote = child
                .stdin
                .take()
                .is_some_and(|mut i| i.write_all(config.as_bytes()).is_ok());
            if !wrote {
                stop(&mut child);
                return Err(Outcome::Unavailable {
                    why: Passing::Unreachable,
                    sent: Sent::No,
                });
            }
        }
        let Some(out) = child.stdout.take() else {
            stop(&mut child);
            return Err(unavailable(Passing::Unreachable));
        };
        Ok((child, out))
    }

    /// The arguments of a fetch of `address`, one of [`HOSTS`] by
    /// [`address_ok`], within `limit` (at most [`DOWNLOAD_LIMIT`]) and
    /// `cap` bytes, or `Refused(Unsupported)` for any other address. It
    /// asks for the file's bytes and carries no credential.
    pub fn fetch_argv(
        &self,
        address: &str,
        limit: Duration,
        cap: u64,
    ) -> Result<Vec<String>, Outcome> {
        if !address_ok(address) {
            return Err(Outcome::Refused(Refusal::Unsupported));
        }
        Ok(self.args(
            limit,
            &[
                "--header",
                "Accept: application/octet-stream",
                "--header",
                "X-GitHub-Api-Version: 2022-11-28",
            ],
            address,
            DOWNLOAD_LIMIT,
            cap,
        ))
    }

    /// One GET of `address` within `limit` and `cap`. A 2xx answer's body
    /// goes into `file` (made empty first), held to `cap` on the stream:
    /// no more than `cap` bytes are ever written, and a body longer stops
    /// curl and is `Unread(TooLarge)`. `Content-Length` is not trusted;
    /// curl's `--max-filesize` is a second guard. Any other answer's body
    /// is not read, and nothing is written. The answer's body is empty: its
    /// `written` says how much the file holds.
    pub(crate) fn fetch(
        &self,
        address: &str,
        limit: Duration,
        cap: u64,
        file: &Path,
    ) -> Result<Answer, Outcome> {
        let argv = self.fetch_argv(address, limit, cap)?;
        let into = File::create(file).map_err(|_| Outcome::Unread(Unread::Unknown))?;
        let (mut child, out) = self.spawn(&argv, None)?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(stream(out, into, cap));
        });
        let Ok(got) = rx.recv_timeout(limit.min(DOWNLOAD_LIMIT) + GRACE) else {
            stop(&mut child);
            return Err(unavailable(Passing::Timeout));
        };
        let Some((mut answer, whole)) = got else {
            stop(&mut child);
            return Err(Outcome::Unread(Unread::Malformed));
        };
        if answer.written > cap {
            stop(&mut child);
            return Err(Outcome::Unread(Unread::TooLarge));
        }
        if !(200..300).contains(&answer.status) {
            stop(&mut child);
            answer.written = 0;
            return Ok(answer);
        }
        finish(&mut child, whole)?;
        Ok(answer)
    }
}

/// curl's end, once its output is read: 0 with the output whole is the
/// answer's to read; curl's timeout and size codes are the outcomes they
/// name; anything else is unreachable.
fn finish(child: &mut Child, whole: bool) -> Result<(), Outcome> {
    match child.wait().ok().and_then(|s| s.code()) {
        Some(0) if whole => Ok(()),
        Some(28) => Err(unavailable(Passing::Timeout)),
        Some(63) => Err(Outcome::Unread(Unread::TooLarge)),
        _ => Err(unavailable(Passing::Unreachable)),
    }
}

/// curl's `--include` output read off `out`: the head, then for a 2xx
/// answer the body into `into`, never more than `cap` bytes of it. A body
/// longer than `cap` reads one byte more, which is not written, and its
/// answer's `written` is `cap` + 1. None for a head that is not one; with
/// the answer, whether the body was read to its end.
fn stream(out: impl Read, mut into: File, cap: u64) -> Option<(Answer, bool)> {
    let mut reader = BufReader::new(out);
    let mut head = Vec::new();
    loop {
        let start = head.len();
        let read = Read::by_ref(&mut reader)
            .take(HEAD_CAP.saturating_sub(start as u64))
            .read_until(b'\n', &mut head)
            .ok()?;
        let line = &head[start..];
        if read == 0 || !line.ends_with(b"\n") {
            return None;
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
    }
    let mut answer = parse(&head)?;
    if !(200..300).contains(&answer.status) {
        return Some((answer, false));
    }
    // At most `cap` bytes reach the file; one byte more is read, never
    // written, and says the stream is over the cap.
    let copied = std::io::copy(&mut Read::by_ref(&mut reader).take(cap), &mut into);
    let whole = copied.is_ok() && into.flush().is_ok();
    answer.written = copied.unwrap_or(0);
    let mut more = [0u8; 1];
    if whole && answer.written == cap && matches!(reader.read(&mut more), Ok(1)) {
        answer.written = cap.saturating_add(1);
    }
    Some((answer, whole))
}

/// curl with a credential, given per request and never kept: see the
/// module's doc. `Default` is curl as the host has it.
#[derive(Debug, Clone, Default)]
pub struct Authorised {
    curl: Curl,
}

impl Authorised {
    /// The program at `program` in curl's place, as a test's stub.
    pub fn at(program: impl Into<PathBuf>) -> Authorised {
        Authorised {
            curl: Curl::at(program),
        }
    }

    /// The arguments of a GET of `path` under [`API`] with a credential:
    /// the anonymous GET's, reading the rest from standard input.
    fn argv(&self, path: &str, limit: Duration) -> Result<Vec<String>, Outcome> {
        let mut argv = self.curl.argv(path, limit)?;
        let address = argv.pop().unwrap_or_default();
        argv.extend(["--config".to_string(), "-".to_string(), address]);
        Ok(argv)
    }

    /// The arguments of a trade at [`TRADE`]: a POST whose form is read
    /// from standard input.
    fn trade_argv(&self, limit: Duration) -> Vec<String> {
        let mut argv = self.curl.args(
            limit,
            &["--header", "Accept: application/json"],
            TRADE,
            LIMIT,
            self.curl.cap,
        );
        let address = argv.pop().unwrap_or_default();
        argv.extend(["--config".to_string(), "-".to_string(), address]);
        argv
    }

    /// One GET of `path` under [`API`] as `bearer`. `Refused(Invalid)` with
    /// no request for a path that could leave the host or a token that
    /// could leave the config's quotes.
    pub(crate) fn get(&self, path: &str, bearer: &str, limit: Duration) -> Result<Answer, Outcome> {
        let argv = self.argv(path, limit)?;
        let header = format!("Authorization: Bearer {}", quotable(bearer)?);
        let config = format!("header = \"{header}\"\n");
        self.curl.send(&argv, Some(&config), limit)
    }

    /// One POST at [`TRADE`] of `form`, its names and values url-encoded by
    /// curl. `Refused(Invalid)` with no request for a name or value that
    /// could leave the config's quotes.
    pub(crate) fn trade(&self, form: &[(&str, &str)], limit: Duration) -> Result<Answer, Outcome> {
        let mut config = String::new();
        for (name, value) in form {
            if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') || name.is_empty() {
                return Err(Outcome::Refused(Refusal::Invalid));
            }
            config.push_str(&format!(
                "data-urlencode = \"{name}={}\"\n",
                quotable(value)?
            ));
        }
        self.curl
            .send(&self.trade_argv(limit), Some(&config), limit)
    }
}

/// `value` when it can sit inside a config's double quotes as it is: one
/// to 4096 printable ASCII bytes with no `"` and no `\`. Anything else is
/// `Refused(Invalid)`: no escape is attempted.
fn quotable(value: &str) -> Result<&str, Outcome> {
    let ok = (1..=4096).contains(&value.len())
        && value
            .bytes()
            .all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\');
    ok.then_some(value)
        .ok_or(Outcome::Refused(Refusal::Invalid))
}

fn unavailable(why: Passing) -> Outcome {
    Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    }
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Whether `path` names a place under [`API`] and nowhere else: no scheme,
/// host, user part, empty segment, `..` or byte outside an escaped path
/// and query.
pub(crate) fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains("//")
        && !path.contains("..")
        && path.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'-' | b'.' | b'_' | b'~' | b'/' | b'?' | b'=' | b'&' | b'%'
                )
        })
}

/// The path of an address GitHub gave (a page's next, a redirect), if it
/// is under [`API`] and keeps [`path_ok`], its path hiding no `/`, `.` or
/// backslash in an escape: the adapter did not escape it itself.
pub(crate) fn api_path(address: &str) -> Option<&str> {
    address.strip_prefix(API).filter(|p| {
        let path = p.split_once('?').map_or(*p, |(path, _)| path);
        path_ok(p) && !hides_a_separator(path)
    })
}

/// Whether `path` holds an escaped `/`, `.` or backslash, which a server
/// may read as a segment or a step up.
fn hides_a_separator(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    ["%2f", "%2e", "%5c"].iter().any(|e| lower.contains(e))
}

/// An address taken apart as curl takes it: its host, the path and the
/// query. Only `https`, with no user part and no port: the authority ends at
/// the first `/`, `?`, `#` or backslash, and must hold neither `@` nor `:`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Parsed<'a> {
    pub(crate) host: &'a str,
    path: &'a str,
    query: Option<&'a str>,
}

pub(crate) fn parse_address(address: &str) -> Option<Parsed<'_>> {
    let rest = address.strip_prefix("https://")?;
    let end = rest.find(['/', '?', '#', '\\']).unwrap_or(rest.len());
    let (host, rest) = rest.split_at(end);
    if host.is_empty() || host.contains(['@', ':']) {
        return None;
    }
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    Some(Parsed { host, path, query })
}

/// Whether `address` is one GitHub may send a release file's fetch to: its
/// parsed host equal to one of [`HOSTS`], never matched by a prefix or an
/// end, then a path of plain bytes (no `..`, empty segment, escaped `/` or
/// `.`) and a query of plain bytes, with no fragment, backslash, space or
/// control byte. Every address, each redirect's among them, is held to it
/// before it is asked.
pub(crate) fn address_ok(address: &str) -> bool {
    let Some(parsed) = parse_address(address) else {
        return false;
    };
    let plain = |b: u8, more: &[u8]| {
        b.is_ascii_alphanumeric() || b"-._~%+:,;".contains(&b) || more.contains(&b)
    };
    HOSTS.contains(&parsed.host)
        && parsed.path.starts_with('/')
        && !parsed.path.contains("//")
        && !parsed.path.contains("..")
        && !hides_a_separator(parsed.path)
        && parsed.path.bytes().all(|b| plain(b, b"/"))
        && parsed
            .query
            .is_none_or(|q| q.bytes().all(|b| plain(b, b"/?=&")))
}

/// `segment` escaped for one segment of a path: every byte but an
/// unreserved one as `%XX`.
pub(crate) fn escape(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// curl's `--include` output: a status line, headers, a blank line and the
/// body.
fn parse(bytes: &[u8]) -> Option<Answer> {
    let (end, gap) = find(bytes, b"\r\n\r\n")
        .map(|e| (e, 4))
        .or_else(|| find(bytes, b"\n\n").map(|e| (e, 2)))?;
    let head = std::str::from_utf8(&bytes[..end]).ok()?;
    let mut lines = head.lines();
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Some(Answer {
        status,
        headers,
        body: bytes[end + gap..].to_vec(),
        written: 0,
    })
}

fn find(bytes: &[u8], what: &[u8]) -> Option<usize> {
    bytes.windows(what.len()).position(|w| w == what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_could_leave_the_api_host_is_refused() {
        let curl = Curl::new();
        for bad in [
            "",
            "/repos/a/b",
            "//evil.test/x",
            "repos/a/../b",
            "repos/a/b@evil.test",
            "repos/a/b:443",
            "repos/a/b#x",
            "repos/a/b c",
            "https://evil.test/",
        ] {
            assert_eq!(
                curl.argv(bad, LIMIT).unwrap_err(),
                Outcome::Refused(Refusal::Invalid),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn only_an_address_on_the_api_host_gives_a_path() {
        assert_eq!(
            api_path("https://api.github.com/repositories/1/tags?per_page=100&page=2"),
            Some("repositories/1/tags?per_page=100&page=2")
        );
        for other in [
            "https://api.github.com.evil.test/x",
            "http://api.github.com/x",
            "https://objects.githubusercontent.com/x",
            "https://api.github.com//evil.test/x",
        ] {
            assert_eq!(api_path(other), None, "{other}");
        }
    }

    /// A page's next or a redirect is GitHub's address, not one the adapter
    /// escaped: its path may not hide a `/`, a `.` or a backslash, in either
    /// case. Its query keeps its escapes.
    #[test]
    fn an_api_address_whose_path_hides_a_separator_gives_no_path() {
        for bad in [
            "https://api.github.com/repos/a/b/%2e%2e/c",
            "https://api.github.com/repos/a/b/%2E%2E/c",
            "https://api.github.com/repos/a%2Fb/tags",
            "https://api.github.com/repos/a/b%5Cc/tags",
        ] {
            assert_eq!(api_path(bad), None, "{bad}");
        }
        assert_eq!(
            api_path("https://api.github.com/repos/a/b/tags?after=v1%2E0&page=2"),
            Some("repos/a/b/tags?after=v1%2E0&page=2")
        );
    }

    #[test]
    fn a_value_that_could_leave_the_configs_quotes_is_refused() {
        for bad in ["", "a\"b", "a\\b", "a\nb", "a\rb", "a\0b", "é"] {
            assert_eq!(
                quotable(bad),
                Err(Outcome::Refused(Refusal::Invalid)),
                "{bad:?}"
            );
        }
        assert!(quotable(&"a".repeat(4097)).is_err());
        assert_eq!(
            quotable("gho_16C7e42F292c6912E7710c8"),
            Ok("gho_16C7e42F292c6912E7710c8")
        );
        let a = Authorised::default();
        assert_eq!(
            a.get("user", "x\"\nurl = \"https://evil.test/", LIMIT)
                .unwrap_err(),
            Outcome::Refused(Refusal::Invalid)
        );
        assert_eq!(
            a.trade(&[("code", "x\nurl = https://evil.test/")], LIMIT)
                .unwrap_err(),
            Outcome::Refused(Refusal::Invalid)
        );
        assert_eq!(
            a.trade(&[("a=b", "x")], LIMIT).unwrap_err(),
            Outcome::Refused(Refusal::Invalid)
        );
    }

    #[test]
    fn an_answer_shows_no_header_value_and_no_body() {
        let a =
            parse(b"HTTP/2 200 \r\nset-cookie: s3cret\r\n\r\n{\"access_token\":\"gho_s3cret\"}")
                .unwrap();
        let shown = format!("{a:?}");
        assert!(!shown.contains("s3cret"), "{shown}");
        assert_eq!(
            shown,
            r#"Answer { status: 200, headers: ["set-cookie"], body: 29 bytes }"#
        );
    }

    #[test]
    fn a_fetch_reaches_exactly_the_four_hosts_over_https() {
        for ok in [
            "https://release-assets.githubusercontent.com/github-production-release-asset/1/0d3f?sp=r&sv=2018-11-09&sr=b&spr=https&se=2026-11-24T12%3A00%3A00Z&rscd=attachment%3B+filename%3Dinstall&sig=a%2Bb%3D&jwt=eyJ.eyJ.sig_-&response-content-type=application%2Foctet-stream",
            "https://api.github.com/repos/acme/widget/releases/assets/11",
            "https://github.com/acme/widget/releases/download/v1/install",
            "https://objects.githubusercontent.com/a/1?X-Amz-Signature=a%2Bb&r=attachment%3B+f",
        ] {
            assert!(address_ok(ok), "{ok}");
        }
        for bad in [
            "http://github.com/x",
            "https://github.com",
            "https://github.com.evil.test/x",
            "https://objects.githubusercontent.com.evil.test/x",
            "https://evil.githubusercontent.com/x",
            "https://evil.release-assets.githubusercontent.com/x",
            "https://release-assets.githubusercontent.com.evil.test/x",
            "https://release-assets.githubusercontent.co/x",
            "https://evilgithub.com/x",
            "https://github.com:443/x",
            "https://github.com@evil.test/x",
            "https://user@github.com/x",
            "https://github.com/x#y",
            "https://github.com/x/../y",
            "https://github.com//evil.test/x",
            "https://github.com/x\\y",
            "https://github.com/x y",
            "https://github.com/x\ny",
            " https://github.com/x",
        ] {
            assert!(!address_ok(bad), "{bad}");
            assert_eq!(
                Curl::new().fetch_argv(bad, LIMIT, 1).unwrap_err(),
                Outcome::Refused(Refusal::Unsupported)
            );
        }
    }

    /// The allowlist is held against the host curl would connect to, taken
    /// apart from the address, and compared whole.
    #[test]
    fn the_host_is_the_parsed_one_compared_whole() {
        fn host(a: &str) -> Option<&str> {
            parse_address(a).map(|p| p.host)
        }
        assert_eq!(host("https://github.com/a?b"), Some("github.com"));
        assert_eq!(host("https://github.com?x/y"), Some("github.com"));
        assert_eq!(
            host("https://github.com.evil.test/x"),
            Some("github.com.evil.test")
        );
        assert_eq!(host("https://github.com#@evil.test/x"), Some("github.com"));
        assert_eq!(host("https://github.com\\@evil.test/x"), Some("github.com"));
        assert_eq!(host("https://github.com@evil.test/x"), None, "user part");
        assert_eq!(host("https://github.com:443/x"), None, "port");
        assert_eq!(host("http://github.com/x"), None);
        assert_eq!(host("https:///x"), None);
        // A prefix or an end of an allowed host is not one.
        for bad in [
            "https://github.com.evil.test/x",
            "https://api.github.com.evil.test/x",
            "https://objects.githubusercontent.com.evil.test/x",
            "https://evilgithub.com/x",
            "https://xapi.github.com/x",
            "https://githubusercontent.com/x",
            "https://github.co/x",
            "https://github.com#@evil.test/x",
        ] {
            assert!(!address_ok(bad), "{bad}");
        }
    }

    /// GitHub's signed object addresses escape `/` in their query; a path
    /// the adapter did not escape itself may not hide a `/` or a `.`.
    #[test]
    fn an_escaped_slash_or_dot_is_refused_in_the_path_alone() {
        assert!(address_ok(
            "https://objects.githubusercontent.com/a/1?X-Amz-Credential=K%2F2026%2Fs3&X-Amz-Signature=f"
        ));
        for bad in [
            "https://github.com/a%2F..%2Fb",
            "https://github.com/a%2fb",
            "https://github.com/%2E%2E/b",
            "https://github.com/a%5Cb",
        ] {
            assert!(!address_ok(bad), "{bad}");
        }
    }

    /// A stream over the cap is cut: the file never holds more than the
    /// cap, and the answer says it was over.
    #[test]
    fn a_stream_over_the_cap_writes_no_more_than_the_cap() {
        let dir = tehas_core::testdir::TestDir::new("github-curl-stream-cap");
        let path = dir.join("file");
        let mut served = b"HTTP/2 200 \r\ncontent-length: 10\r\n\r\n".to_vec();
        served.extend([b'x'; 1000]);
        let (answer, _) = stream(&served[..], File::create(&path).unwrap(), 100).unwrap();
        assert_eq!(answer.written, 101, "over the cap");
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            100,
            "cut at the cap"
        );
        // A body of exactly the cap is whole.
        let mut served = b"HTTP/2 200 \r\n\r\n".to_vec();
        served.extend([b'x'; 100]);
        let (answer, whole) = stream(&served[..], File::create(&path).unwrap(), 100).unwrap();
        assert_eq!((answer.written, whole), (100, true));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 100);
        // Any other answer writes nothing.
        let served = b"HTTP/2 302 \r\nlocation: https://github.com/x\r\n\r\nbody".to_vec();
        let (answer, _) = stream(&served[..], File::create(&path).unwrap(), 100).unwrap();
        assert_eq!((answer.status, answer.written), (302, 0));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    }

    /// The one host check: a request of either variant to a host not in
    /// [`HOSTS`] starts nothing, even with arguments made by hand.
    #[test]
    fn no_request_starts_to_a_host_outside_the_list() {
        let curl = Curl::at("/nonexistent/curl-must-not-run");
        for bad in [
            "https://evil.test/x",
            "https://api.github.com.evil.test/x",
            "https://github.com@evil.test/x",
            "http://api.github.com/x",
        ] {
            let argv = vec!["-q".to_string(), bad.to_string()];
            assert_eq!(
                curl.spawn(&argv, None).err(),
                Some(Outcome::Refused(Refusal::Invalid)),
                "{bad}"
            );
        }
        // The authorised trade's own address is one of them.
        assert!(parse_address(TRADE).is_some_and(|p| HOSTS.contains(&p.host)));
    }

    #[test]
    fn a_segment_is_escaped_whole() {
        assert_eq!(escape("v1.2.3"), "v1.2.3");
        assert_eq!(escape("a b:c/d"), "a%20b%3Ac%2Fd");
    }

    #[test]
    fn an_answer_is_read_with_crlf_or_lf() {
        let a = parse(b"HTTP/2 200 \r\nX-RateLimit-Remaining: 59\r\n\r\n[]").unwrap();
        assert_eq!(
            (a.status, a.header("x-ratelimit-remaining")),
            (200, Some("59"))
        );
        assert_eq!(a.body, b"[]");
        let a = parse(b"HTTP/1.1 404 Not Found\nlink: <x>\n\n{}").unwrap();
        assert_eq!((a.status, a.header("link")), (404, Some("<x>")));
        assert!(parse(b"no head").is_none());
    }
}
