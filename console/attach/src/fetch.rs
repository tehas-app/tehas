//! The one fetch of a link: curl with a cap on the bytes, the token only where
//! the link rule allows it, and, for a pinned link, the sha256 its bytes must
//! have. The room an issue has left is counted here too, so every caller pays
//! for files by one rule.

use crate::link;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// curl's exit status for a file over the cap.
pub const TOO_BIG: i32 = 63;
/// curl's exit status for an answer of 400 or more.
pub const REFUSED: i32 = 22;
/// curl's exit status for too many redirects, and [`fetch_within`]'s for a
/// redirect to an address it may not follow.
pub const MOVED: i32 = 47;
/// The redirects a fetch follows.
const HOPS: usize = 3;
/// What curl prints when it is done: the content type, and on its own line the
/// HTTP status of the last answer, so a caller can tell 404 from 503.
const WRITE_OUT: &str = "%{content_type}\\n%{response_code}";
/// What curl prints after one step of [`fetch_within`]: [`WRITE_OUT`], and on
/// a third line the address a redirect sends it on to.
const HOP_WRITE_OUT: &str = "%{content_type}\\n%{response_code}\\n%{redirect_url}";

/// What a fetch ended with: curl's exit status, 128 and the signal when one
/// ended it, 127 when it could not run, the content type it printed and the
/// HTTP status of the last answer, 0 when it printed none.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub code: i32,
    pub content_type: String,
    pub http: u16,
}

/// The content type and the HTTP status of what curl printed for `WRITE_OUT`.
fn answer(out: &str) -> (String, u16) {
    let out = out.trim_end_matches('\n');
    match out.rsplit_once('\n') {
        Some((kind, http)) => (kind.to_string(), http.trim().parse().unwrap_or(0)),
        None => (out.to_string(), 0),
    }
}

/// The content type, the HTTP status and the address it was sent on to, of
/// what curl printed for `HOP_WRITE_OUT`. A part it did not print is empty, or
/// 0 for the status.
fn hop_answer(out: &str) -> (String, u16, String) {
    let mut parts = out.trim_end_matches('\n').splitn(3, '\n');
    let kind = parts.next().unwrap_or("").to_string();
    let http = parts.next().map_or(0, |h| h.trim().parse().unwrap_or(0));
    let moved = parts.next().unwrap_or("").trim().to_string();
    (kind, http, moved)
}

/// curl's arguments, and whether it is given the token on standard input. The
/// token goes only to a whole link of an attachment folder on github.com and
/// never on the command line; curl does not pass it on when GitHub redirects to
/// its storage. curl sends the path as written, so what the rule checked is
/// what GitHub is asked for.
pub fn curl_args(link: &str, file: &str, most: i64, token: &str) -> (Vec<String>, bool) {
    let mut args: Vec<String> = [
        "-sSfL",
        "--path-as-is",
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
    args.extend(["-w".to_string(), WRITE_OUT.to_string()]);
    finish_args(args, link, token)
}

/// curl's arguments for one step of [`fetch_within`]: no redirect is followed,
/// and curl prints the address it was sent on to after the content type and
/// the HTTP status.
pub fn hop_args(link: &str, file: &str, most: i64, token: &str) -> (Vec<String>, bool) {
    let mut args: Vec<String> = [
        "-sSf",
        "--path-as-is",
        "--proto",
        "=https",
        "--max-time",
        "120",
        "--max-filesize",
    ]
    .map(String::from)
    .to_vec();
    args.push(most.to_string());
    args.extend(["-o".to_string(), file.to_string()]);
    args.extend(["-w".to_string(), HOP_WRITE_OUT.to_string()]);
    finish_args(args, link, token)
}

fn finish_args(mut args: Vec<String>, link: &str, token: &str) -> (Vec<String>, bool) {
    let with_token = link::sends_token(link, token);
    if with_token {
        args.extend(["-H".to_string(), "@-".to_string()]);
    }
    args.push(link.to_string());
    (args, with_token)
}

fn status_of(status: std::process::ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

/// Download `link` into `file`, at most `most` bytes.
pub fn fetch(link: &str, file: &Path, most: i64, token: &str) -> Fetched {
    let (code, out) = run_curl(curl_args(link, &file.to_string_lossy(), most, token), token);
    let (content_type, http) = answer(&out);
    Fetched {
        code,
        content_type,
        http,
    }
}

/// What one step of [`fetch_within`] says to do next.
#[derive(Debug, PartialEq)]
pub enum Hop {
    /// The fetch is over: this is its answer.
    Done(Fetched),
    /// Fetch this address next.
    Next(String),
}

/// The step after one answer of curl run with [`hop_args`], `moved` the
/// address it printed: that address when `allowed` takes it and `hops`
/// redirects are not yet used, [`MOVED`] when not, or the answer itself.
pub fn next_hop(got: Fetched, moved: &str, hops: usize, allowed: &dyn Fn(&str) -> bool) -> Hop {
    if got.code != 0 || moved.is_empty() {
        return Hop::Done(got);
    }
    if hops >= HOPS || !allowed(moved) {
        return Hop::Done(Fetched { code: MOVED, ..got });
    }
    Hop::Next(moved.to_string())
}

/// Download `link` into `file` as [`fetch`] does, following a redirect only to
/// an address `allowed` takes, so every address asked is one the caller
/// allows. Ends with [`MOVED`] on any other.
pub fn fetch_within(
    link: &str,
    file: &Path,
    most: i64,
    token: &str,
    allowed: &dyn Fn(&str) -> bool,
) -> Fetched {
    let mut at = link.to_string();
    for hops in 0.. {
        let (code, out) = run_curl(hop_args(&at, &file.to_string_lossy(), most, token), token);
        let (content_type, http, moved) = hop_answer(&out);
        let got = Fetched {
            code,
            content_type,
            http,
        };
        match next_hop(got, &moved, hops, allowed) {
            Hop::Done(got) => {
                if got.code != 0 {
                    let _ = fs::remove_file(file);
                }
                return got;
            }
            Hop::Next(next) => at = next,
        }
    }
    unreachable!("next_hop ends the walk after {HOPS} redirects")
}

/// curl's exit status, as [`Fetched`] has it, and what it printed.
fn run_curl((args, with_token): (Vec<String>, bool), token: &str) -> (i32, String) {
    let mut curl = Command::new("curl");
    curl.args(args).stdout(Stdio::piped());
    curl.stdin(if with_token {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let Ok(mut child) = curl.spawn() else {
        return (127, String::new());
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A curl that went without reading it fails on its own.
        let _ = writeln!(stdin, "Authorization: Bearer {token}");
    }
    match child.wait_with_output() {
        Ok(done) => (
            status_of(done.status),
            String::from_utf8_lossy(&done.stdout).into_owned(),
        ),
        Err(_) => (127, String::new()),
    }
}

/// The sha256 of a file, in lower-case hex.
pub fn sha256_of(file: &Path) -> io::Result<String> {
    let mut f = fs::File::open(file)?;
    let mut sha = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => sha.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(sha.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// A link and the sha256 its bytes were approved with.
pub struct Pin<'a> {
    pub link: &'a str,
    pub sha256: &'a str,
    pub most: i64,
}

/// Why a pinned fetch gave no file.
#[derive(Debug, PartialEq)]
pub enum Unpinned {
    /// curl failed: its answer.
    Fetch(Fetched),
    /// The bytes are not the ones pinned: the sha256 they have.
    Other(String),
    /// The file fetched could not be read back.
    Unreadable(String),
}

/// Whether bytes of this sha256 are the ones `pin` holds. A pin is 64 hex
/// characters, either case.
pub fn holds(pin: &str, sha256: &str) -> bool {
    pin.len() == 64
        && pin.bytes().all(|b| b.is_ascii_hexdigit())
        && pin.eq_ignore_ascii_case(sha256)
}

/// Check a fetched file against its pin; a file of other bytes is removed.
pub fn check_pin(pin: &str, file: &Path) -> Result<(), Unpinned> {
    let sha256 = sha256_of(file).map_err(|e| {
        let _ = fs::remove_file(file);
        Unpinned::Unreadable(e.to_string())
    })?;
    if holds(pin, &sha256) {
        return Ok(());
    }
    let _ = fs::remove_file(file);
    Err(Unpinned::Other(sha256))
}

/// Fetch a pinned link into `file`. The file is there only when its bytes are
/// the ones pinned.
pub fn fetch_pinned(pin: &Pin, file: &Path, token: &str) -> Result<Fetched, Unpinned> {
    let _ = fs::remove_file(file);
    pinned(pin, file, fetch(pin.link, file, pin.most, token))
}

/// [`fetch_pinned`] with the redirects of [`fetch_within`]: every address asked
/// is one `allowed` takes.
pub fn fetch_pinned_within(
    pin: &Pin,
    file: &Path,
    token: &str,
    allowed: &dyn Fn(&str) -> bool,
) -> Result<Fetched, Unpinned> {
    let _ = fs::remove_file(file);
    let got = fetch_within(pin.link, file, pin.most, token, allowed);
    pinned(pin, file, got)
}

fn pinned(pin: &Pin, file: &Path, got: Fetched) -> Result<Fetched, Unpinned> {
    if got.code != 0 {
        let _ = fs::remove_file(file);
        return Err(Unpinned::Fetch(got));
    }
    check_pin(pin.sha256, file)?;
    Ok(got)
}

/// Why there is no room for one more file.
#[derive(Debug, PartialEq)]
pub enum Full {
    /// As many files as the limit allows.
    Count,
    /// No bytes left.
    Bytes,
}

/// The bytes left for one more file: `count` files of at most `most` are held,
/// using `used` of `limit` bytes. The count is asked first.
pub fn room(count: i64, most: i64, used: i64, limit: i64) -> Result<i64, Full> {
    if count >= most {
        return Err(Full::Count);
    }
    let left = limit - used;
    if left <= 0 {
        return Err(Full::Bytes);
    }
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    // sha256 of "abc".
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn the_token_goes_on_standard_input_and_only_to_github() {
        let (args, with) = curl_args("https://github.com/a/b/assets/1/x", "/d/.part", 9, "tok");
        assert!(with);
        assert_eq!(
            args.join(" "),
            "-sSfL --path-as-is --proto =https --proto-redir =https --max-redirs 3 --max-time 120 --max-filesize 9 -o /d/.part -w %{content_type}\\n%{response_code} -H @- https://github.com/a/b/assets/1/x"
        );
        assert!(!args.iter().any(|a| a.contains("tok")));
        let (args, with) = curl_args("https://example.com/x.zip", "f", 9, "tok");
        assert!(!with && !args.contains(&"-H".to_string()));
    }

    #[test]
    fn what_curl_prints_gives_the_content_type_and_the_status() {
        assert_eq!(answer("text/plain\n404"), ("text/plain".into(), 404));
        assert_eq!(answer("\n503"), (String::new(), 503));
        assert_eq!(answer("text/plain\n000"), ("text/plain".into(), 0));
        // A curl that printed the type alone, as a stand-in may.
        assert_eq!(answer("image/png"), ("image/png".into(), 0));
        assert_eq!(answer("image/png\n"), ("image/png".into(), 0));
        assert_eq!(answer(""), (String::new(), 0));
    }

    #[test]
    fn a_step_follows_no_redirect_and_says_where_it_was_sent() {
        let (args, with) = hop_args("https://files.example.org/p.zip", "f", 9, "tok");
        assert!(!with);
        assert_eq!(
            args.join(" "),
            "-sSf --path-as-is --proto =https --max-time 120 --max-filesize 9 -o f -w %{content_type}\\n%{response_code}\\n%{redirect_url} https://files.example.org/p.zip"
        );
        let (args, with) = hop_args("https://github.com/a/b/assets/1/x", "f", 9, "tok");
        assert!(with && args.contains(&"@-".to_string()));
    }

    #[test]
    fn what_curl_prints_for_a_step_gives_the_type_the_status_and_the_address() {
        let moved = |t: &str, h: u16, m: &str| (t.to_string(), h, m.to_string());
        assert_eq!(
            hop_answer("application/zip\n200\n"),
            moved("application/zip", 200, "")
        );
        assert_eq!(
            hop_answer("text/html\n302\nhttps://files.example.org/b.zip"),
            moved("text/html", 302, "https://files.example.org/b.zip")
        );
        assert_eq!(hop_answer("\n404\n"), moved("", 404, ""));
        // A curl that printed the type alone, as a stand-in may.
        assert_eq!(hop_answer("image/png"), moved("image/png", 0, ""));
        assert_eq!(hop_answer(""), moved("", 0, ""));
    }

    fn got(code: i32, content_type: &str, http: u16) -> Fetched {
        Fetched {
            code,
            content_type: content_type.to_string(),
            http,
        }
    }

    #[test]
    fn a_redirect_is_followed_only_to_an_allowed_address_and_three_times() {
        let ok = |a: &str| a.starts_with("https://files.example.org/");
        let zip = got(0, "application/zip", 200);
        assert_eq!(next_hop(zip.clone(), "", 0, &ok), Hop::Done(zip));
        let refused = got(22, "text/html", 404);
        assert_eq!(next_hop(refused.clone(), "", 0, &ok), Hop::Done(refused));
        let sent = got(0, "", 302);
        assert_eq!(
            next_hop(sent.clone(), "https://files.example.org/b.zip", 2, &ok),
            Hop::Next("https://files.example.org/b.zip".to_string())
        );
        assert_eq!(
            next_hop(sent, "https://files.example.org/b.zip", 3, &ok),
            Hop::Done(got(MOVED, "", 302))
        );
        assert_eq!(
            next_hop(
                got(0, "text/html", 301),
                "https://elsewhere.example/b.zip",
                0,
                &ok
            ),
            Hop::Done(got(MOVED, "text/html", 301))
        );
    }

    #[test]
    fn a_file_is_hashed_as_sha256_in_lower_case() {
        let dir = TestDir::new("attach-fetch-sha");
        let file = dir.path().join("f");
        fs::write(&file, "abc").unwrap();
        assert_eq!(sha256_of(&file).unwrap(), ABC);
    }

    #[test]
    fn a_pin_holds_only_its_own_bytes() {
        assert!(holds(ABC, ABC));
        assert!(holds(&ABC.to_uppercase(), ABC));
        assert!(!holds(&ABC[..63], &ABC[..63]));
        assert!(!holds("", ""));
        let other = format!("{}0", &ABC[..63]);
        assert!(!holds(&other, ABC));
        let not_hex = "g".repeat(64);
        assert!(!holds(&not_hex, &not_hex));
    }

    #[test]
    fn a_file_of_other_bytes_is_removed() {
        let dir = TestDir::new("attach-fetch-pin");
        let file = dir.path().join("f");
        fs::write(&file, "abc").unwrap();
        assert_eq!(check_pin(ABC, &file), Ok(()));
        assert!(file.exists());
        fs::write(&file, "abd").unwrap();
        let Err(Unpinned::Other(got)) = check_pin(ABC, &file) else {
            panic!("other bytes held the pin");
        };
        assert_eq!(got.len(), 64);
        assert!(!file.exists());
        assert!(matches!(
            check_pin(ABC, &file),
            Err(Unpinned::Unreadable(_))
        ));
    }

    #[test]
    fn the_count_is_asked_before_the_bytes() {
        assert_eq!(room(0, 8, 0, 10), Ok(10));
        assert_eq!(room(7, 8, 4, 10), Ok(6));
        assert_eq!(room(8, 8, 0, 10), Err(Full::Count));
        assert_eq!(room(8, 8, 10, 10), Err(Full::Count));
        assert_eq!(room(1, 8, 10, 10), Err(Full::Bytes));
        assert_eq!(room(1, 8, 11, 10), Err(Full::Bytes));
    }
}
