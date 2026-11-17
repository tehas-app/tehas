//! One call of the job channel: a POST of a JSON body to the factory's
//! address, one request per connection, plain HTTP/1.1, the shape the
//! factory's listener answers (proxy/src/serve.rs).

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The path every call of the channel starts with.
pub const PREFIX: &str = "/outsource/v1/";

/// The header that names the contractor on a call made with its key.
pub const NAME_HEADER: &str = "x-tehas-contractor";

/// The largest answer read: a job with its models.json, or a relayed write's
/// output.
const ANSWER_MAX: u64 = 8 << 20;

/// What the factory said.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// 200, with the answer's body.
    Ok(Value),
    /// A refusal: the status and the word the factory gave.
    No(u16, String),
    /// No answer at all: the factory could not be reached, or said nothing it
    /// meant. The words are for the log and name no token.
    Down(String),
}

impl Reply {
    /// Whether the factory said the contractor is revoked: the one answer that
    /// means push nothing.
    pub fn revoked(&self) -> bool {
        matches!(self, Reply::No(_, word) if word == "revoked")
    }
}

/// Who a call is made as: a contractor by its name and key, or a job by its
/// token, or nobody (a join).
#[derive(Debug, Clone, Copy)]
pub enum As<'a> {
    Nobody,
    Contractor { name: &'a str, key: &'a str },
    Job(&'a str),
}

/// The host and port of a factory's address, with or without `http://`.
pub fn host_port(addr: &str) -> &str {
    let addr = addr.strip_prefix("http://").unwrap_or(addr);
    addr.split('/').next().unwrap_or(addr)
}

/// POST `body` to the call `name` of the factory at `addr`.
pub fn post(addr: &str, name: &str, who: As, body: &Value, wait: Duration) -> Reply {
    let at = host_port(addr);
    let Some(sock) = at.to_socket_addrs().ok().and_then(|mut a| a.next()) else {
        return Reply::Down(format!("{at} is no address"));
    };
    let mut stream = match TcpStream::connect_timeout(&sock, wait) {
        Ok(s) => s,
        Err(e) => return Reply::Down(format!("{at}: {e}")),
    };
    let _ = stream.set_read_timeout(Some(wait));
    let _ = stream.set_write_timeout(Some(wait));
    let text = body.to_string();
    let mut head = format!(
        "POST {PREFIX}{name} HTTP/1.1\r\nhost: {at}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
        text.len()
    );
    match who {
        As::Nobody => {}
        As::Contractor { name, key } => {
            head.push_str(&format!(
                "authorization: Bearer {key}\r\n{NAME_HEADER}: {name}\r\n"
            ));
        }
        As::Job(token) => head.push_str(&format!("authorization: Bearer {token}\r\n")),
    }
    head.push_str("\r\n");
    let sent = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(text.as_bytes()))
        .and_then(|()| stream.flush());
    if let Err(e) = sent {
        return Reply::Down(format!("{at}: {e}"));
    }
    let mut answer = Vec::new();
    if let Err(e) = stream.take(ANSWER_MAX).read_to_end(&mut answer) {
        return Reply::Down(format!("{at}: {e}"));
    }
    read_answer(&answer)
}

/// A file the factory served: the fields of its answer, by lower-case name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub fields: Vec<(String, String)>,
}

impl Fetched {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// GET the download `path` of the factory at `addr` into `dest`. With
/// `resume`, a part of the file already at `dest` is kept and only the rest
/// is asked for. The whole file is held to the sha256 the answer names, and
/// removed when it does not match. A refusal is the factory's [`Reply`].
pub fn fetch(
    addr: &str,
    path: &str,
    who: As,
    dest: &Path,
    resume: bool,
    wait: Duration,
) -> Result<Fetched, Reply> {
    let at = host_port(addr);
    let Some(sock) = at.to_socket_addrs().ok().and_then(|mut a| a.next()) else {
        return Err(Reply::Down(format!("{at} is no address")));
    };
    let mut stream =
        TcpStream::connect_timeout(&sock, wait).map_err(|e| Reply::Down(format!("{at}: {e}")))?;
    let _ = stream.set_read_timeout(Some(wait));
    let _ = stream.set_write_timeout(Some(wait));
    let had = if resume {
        fs::metadata(dest).map(|m| m.len()).unwrap_or(0)
    } else {
        let _ = fs::remove_file(dest);
        0
    };
    let mut head = format!("GET {PREFIX}{path} HTTP/1.1\r\nhost: {at}\r\nconnection: close\r\n");
    match who {
        As::Nobody => {}
        As::Contractor { name, key } => head.push_str(&format!(
            "authorization: Bearer {key}\r\n{NAME_HEADER}: {name}\r\n"
        )),
        As::Job(token) => head.push_str(&format!("authorization: Bearer {token}\r\n")),
    }
    if had > 0 {
        head.push_str(&format!("range: bytes={had}-\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|e| Reply::Down(format!("{at}: {e}")))?;
    let mut reader = BufReader::new(stream);
    let mut got = Vec::new();
    loop {
        let mut line = Vec::new();
        let n = (&mut reader)
            .take(FIELD_MAX)
            .read_until(b'\n', &mut line)
            .map_err(|e| Reply::Down(format!("{at}: {e}")))?;
        if n == 0 || got.len() > HEAD_MAX {
            return Err(Reply::Down("an answer with no head".to_string()));
        }
        got.extend_from_slice(&line);
        if line == b"\r\n" || line == b"\n" {
            break;
        }
    }
    let text = String::from_utf8_lossy(&got).into_owned();
    let status = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let fields: Vec<(String, String)> = text
        .lines()
        .skip(1)
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect();
    let fetched = Fetched { fields };
    // A resume refused as a range of no bytes is a file kept whole from
    // before, say one never loaded: start again from nothing.
    if status == 416 && had > 0 {
        let _ = fs::remove_file(dest);
        return fetch(addr, path, who, dest, false, wait);
    }
    if status != 200 && status != 206 {
        let mut body = Vec::new();
        let _ = reader.take(ANSWER_MAX).read_to_end(&mut body);
        let mut answer = got;
        answer.extend_from_slice(&body);
        return Err(read_answer(&answer));
    }
    // A 200 to a resumed ask is the whole file again.
    let append = status == 206 && had > 0;
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(dest)
        .map_err(|e| Reply::Down(format!("{}: {e}", dest.display())))?;
    let want = fetched
        .field("content-length")
        .and_then(|n| n.parse::<u64>().ok());
    let copied = io::copy(&mut reader, &mut out).map_err(|e| Reply::Down(format!("{at}: {e}")))?;
    if want.is_some_and(|w| w != copied) {
        return Err(Reply::Down(format!(
            "the download ended after {copied} bytes"
        )));
    }
    if let Some(sha) = fetched.field("x-tehas-sha256") {
        let have = sha256_of(dest).unwrap_or_default();
        if have != sha {
            let _ = fs::remove_file(dest);
            return Err(Reply::Down(
                "the download is not the file the factory named".to_string(),
            ));
        }
    }
    Ok(fetched)
}

/// The hex sha256 of a file.
pub fn sha256_of(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The longest line of an answer's head, and the longest head.
const FIELD_MAX: u64 = 16 * 1024;
const HEAD_MAX: usize = 64 * 1024;

/// The reply an answer's bytes make.
pub fn read_answer(answer: &[u8]) -> Reply {
    let Some(end) = answer.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Reply::Down("an answer with no head".to_string());
    };
    let head = String::from_utf8_lossy(&answer[..end]);
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok());
    let Some(status) = status else {
        return Reply::Down("an answer with no status".to_string());
    };
    let body: Value = serde_json::from_slice(&answer[end + 4..]).unwrap_or(Value::Null);
    if status == 200 {
        return Reply::Ok(body);
    }
    let word = body["refused"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("status {status}"));
    // A refusal of the factory's own names a word; an answer of no word from a
    // proxy in between is no answer of the factory's. A 503 is the factory
    // saying "not now", whatever its word: it is asked again.
    if body["refused"].is_string() && status != 503 {
        Reply::No(status, word)
    } else {
        Reply::Down(word)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_reads_as_ok_a_refusal_or_nothing() {
        let ok = b"HTTP/1.1 200 OK\r\ncontent-length: 9\r\n\r\n{\"a\": 1}\n";
        assert_eq!(read_answer(ok), Reply::Ok(serde_json::json!({ "a": 1 })));
        let no = b"HTTP/1.1 403 Forbidden\r\n\r\n{\"refused\":\"revoked\"}";
        assert!(read_answer(no).revoked());
        let lost = b"HTTP/1.1 403 Forbidden\r\n\r\n{\"refused\":\"lost\"}";
        assert_eq!(read_answer(lost), Reply::No(403, "lost".to_string()));
        assert!(!read_answer(lost).revoked());
        let busy = b"HTTP/1.1 503 Service Unavailable\r\n\r\n{\"busy\":\"store\"}";
        assert!(matches!(read_answer(busy), Reply::Down(_)));
        let worded = b"HTTP/1.1 503 Service Unavailable\r\n\r\n{\"refused\":\"revoked\"}";
        assert!(matches!(read_answer(worded), Reply::Down(_)));
        let other = b"HTTP/1.1 502 Bad Gateway\r\n\r\n<html>";
        assert!(matches!(read_answer(other), Reply::Down(_)));
        assert!(matches!(read_answer(b"garbage"), Reply::Down(_)));
    }

    #[test]
    fn an_address_is_taken_with_or_without_its_scheme() {
        assert_eq!(host_port("http://192.0.2.7:8090/"), "192.0.2.7:8090");
        assert_eq!(host_port("192.0.2.7:8090"), "192.0.2.7:8090");
    }

    /// A server that answers one request with `answer` and keeps the
    /// request it read.
    fn serve_once(answer: Vec<u8>) -> (String, std::sync::mpsc::Receiver<String>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = l.local_addr().unwrap().to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut head = Vec::new();
            let mut one = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && s.read(&mut one).unwrap_or(0) == 1 {
                head.push(one[0]);
            }
            let _ = tx.send(String::from_utf8_lossy(&head).into_owned());
            let _ = s.write_all(&answer);
        });
        (at, rx)
    }

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn a_download_resumes_from_what_it_has_and_is_held_to_its_sha256() {
        let d = tehas_core::testdir::TestDir::new("contractor-fetch");
        let dest = d.path().join("image.pack");
        let whole = b"0123456789";
        fs::write(&dest, &whole[..4]).unwrap();
        let answer = format!(
            "HTTP/1.1 206 Partial Content\r\ncontent-length: 6\r\nx-tehas-sha256: {}\r\nx-tehas-pack: tar\r\n\r\n456789",
            sha(whole)
        );
        let (at, asked) = serve_once(answer.into_bytes());
        let who = As::Contractor {
            name: "box1",
            key: "k",
        };
        let got = fetch(&at, "image?id=x", who, &dest, true, Duration::from_secs(5)).unwrap();
        assert_eq!(got.field("x-tehas-pack"), Some("tar"));
        assert_eq!(fs::read(&dest).unwrap(), whole.to_vec());
        let head = asked.recv().unwrap();
        assert!(
            head.starts_with("GET /outsource/v1/image?id=x HTTP/1.1\r\n"),
            "{head}"
        );
        assert!(head.contains("range: bytes=4-\r\n"), "{head}");
        assert!(head.contains("x-tehas-contractor: box1\r\n"), "{head}");
        // A file that is not the one the factory named is not kept.
        let answer = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: 3\r\nx-tehas-sha256: {}\r\n\r\nabc",
            sha(b"xyz")
        );
        let (at, _) = serve_once(answer.into_bytes());
        let r = fetch(
            &at,
            "index",
            As::Job("t"),
            &dest,
            false,
            Duration::from_secs(5),
        );
        assert!(matches!(r, Err(Reply::Down(_))), "{r:?}");
        assert!(!dest.exists());
        // A refusal is the factory's word.
        let (at, _) = serve_once(
            b"HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\n\r\n{\"refused\":\"no index\"}\n"
                .to_vec(),
        );
        let r = fetch(
            &at,
            "index",
            As::Job("t"),
            &dest,
            false,
            Duration::from_secs(5),
        );
        assert_eq!(r, Err(Reply::No(404, "no index".to_string())));
        // A download cut short is no file.
        let (at, _) = serve_once(b"HTTP/1.1 200 OK\r\ncontent-length: 9\r\n\r\nabc".to_vec());
        let r = fetch(
            &at,
            "bundle",
            As::Job("t"),
            &dest,
            false,
            Duration::from_secs(5),
        );
        assert!(matches!(r, Err(Reply::Down(_))), "{r:?}");
    }

    #[test]
    fn a_whole_file_kept_from_before_is_fetched_again_not_stuck_on_416() {
        let d = tehas_core::testdir::TestDir::new("contractor-fetch-416");
        let dest = d.path().join("image.pack");
        let whole = b"0123456789";
        // Downloaded in full, then the program stopped before the load.
        fs::write(&dest, whole).unwrap();
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = l.local_addr().unwrap().to_string();
        let (tx, asked) = std::sync::mpsc::channel();
        let full = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: 10\r\nx-tehas-sha256: {}\r\n\r\n0123456789",
            sha(whole)
        );
        let answers = vec![
            b"HTTP/1.1 416 Range Not Satisfiable\r\ncontent-type: application/json\r\n\r\n{\"refused\":\"a range of no bytes\"}\n".to_vec(),
            full.into_bytes(),
        ];
        std::thread::spawn(move || {
            for answer in answers {
                let Ok((mut s, _)) = l.accept() else { return };
                let mut head = Vec::new();
                let mut one = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && s.read(&mut one).unwrap_or(0) == 1 {
                    head.push(one[0]);
                }
                let _ = tx.send(String::from_utf8_lossy(&head).into_owned());
                let _ = s.write_all(&answer);
            }
        });
        let who = As::Contractor {
            name: "box1",
            key: "k",
        };
        let got = fetch(&at, "image?id=x", who, &dest, true, Duration::from_secs(5));
        assert!(got.is_ok(), "{got:?}");
        assert_eq!(fs::read(&dest).unwrap(), whole.to_vec());
        assert!(asked.recv().unwrap().contains("range: bytes=10-\r\n"));
        assert!(!asked.recv().unwrap().contains("range:"));
    }

    #[test]
    fn a_factory_that_is_not_there_is_down() {
        let r = post(
            "127.0.0.1:1",
            "hello",
            As::Nobody,
            &serde_json::json!({}),
            Duration::from_secs(2),
        );
        assert!(matches!(r, Reply::Down(_)), "{r:?}");
    }
}
