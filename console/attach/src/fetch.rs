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
/// What curl prints when it is done: the content type, and on its own line the
/// HTTP status of the last answer, so a caller can tell 404 from 503.
const WRITE_OUT: &str = "%{content_type}\\n%{response_code}";

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
    let (args, with_token) = curl_args(link, &file.to_string_lossy(), most, token);
    let mut curl = Command::new("curl");
    curl.args(args).stdout(Stdio::piped());
    curl.stdin(if with_token {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let none = |code| Fetched {
        code,
        content_type: String::new(),
        http: 0,
    };
    let Ok(mut child) = curl.spawn() else {
        return none(127);
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A curl that went without reading it fails on its own.
        let _ = writeln!(stdin, "Authorization: Bearer {token}");
    }
    match child.wait_with_output() {
        Ok(done) => {
            let (content_type, http) = answer(&String::from_utf8_lossy(&done.stdout));
            Fetched {
                code: status_of(done.status),
                content_type,
                http,
            }
        }
        Err(_) => none(127),
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
    let got = fetch(pin.link, file, pin.most, token);
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
