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

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use tehas_ports::{Outcome, Passing, Refusal, Sent, Unread};

/// The one host it asks, as the start of every address.
pub(crate) const API: &str = "https://api.github.com/";

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
        ))
    }

    /// The arguments every request has, with `headers` before the user
    /// agent and the address last.
    fn args(&self, limit: Duration, headers: &[&str], address: &str) -> Vec<String> {
        let secs = limit.min(LIMIT).as_secs().max(1);
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
            &self.cap.to_string(),
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
        let code = child.wait().ok().and_then(|s| s.code());
        match code {
            Some(0) if read => parse(&bytes).ok_or(Outcome::Unread(Unread::Malformed)),
            Some(28) => Err(unavailable(Passing::Timeout)),
            Some(63) => Err(Outcome::Unread(Unread::TooLarge)),
            _ => Err(unavailable(Passing::Unreachable)),
        }
    }
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
        let mut argv = self
            .curl
            .args(limit, &["--header", "Accept: application/json"], TRADE);
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
/// is under [`API`] and keeps [`path_ok`].
pub(crate) fn api_path(address: &str) -> Option<&str> {
    address.strip_prefix(API).filter(|p| path_ok(p))
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
