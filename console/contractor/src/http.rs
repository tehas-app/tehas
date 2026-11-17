//! One call of the job channel: a POST of a JSON body to the factory's
//! address, one request per connection, plain HTTP/1.1, the shape the
//! factory's listener answers (proxy/src/serve.rs).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::Value;

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
    // proxy in between is no answer of the factory's.
    if body["refused"].is_string() {
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
        let other = b"HTTP/1.1 502 Bad Gateway\r\n\r\n<html>";
        assert!(matches!(read_answer(other), Reply::Down(_)));
        assert!(matches!(read_answer(b"garbage"), Reply::Down(_)));
    }

    #[test]
    fn an_address_is_taken_with_or_without_its_scheme() {
        assert_eq!(host_port("http://192.0.2.7:8090/"), "192.0.2.7:8090");
        assert_eq!(host_port("192.0.2.7:8090"), "192.0.2.7:8090");
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
