//! What the bytes of a file say it is, read once: its size, its sha256, the
//! image its first bytes say, whether they start a zip archive, and whether
//! the whole is text a cycle may be given (valid UTF-8 with no NUL byte).

use sha2::{Digest, Sha256};
use std::io::{self, Write};

pub struct Sniffer {
    bytes: u64,
    sha: Sha256,
    head: Vec<u8>,
    // At most three bytes of a UTF-8 sequence that the next write ends.
    pending: Vec<u8>,
    text: bool,
}

pub struct Sniffed {
    pub bytes: u64,
    pub sha256: String,
    pub image: Option<&'static str>,
    pub zip: bool,
    pub text: bool,
}

impl Default for Sniffer {
    fn default() -> Self {
        Sniffer {
            bytes: 0,
            sha: Sha256::new(),
            head: Vec::with_capacity(12),
            pending: Vec::new(),
            text: true,
        }
    }
}

impl Sniffer {
    pub fn feed(&mut self, chunk: &[u8]) {
        self.bytes += chunk.len() as u64;
        self.sha.update(chunk);
        let room = 12 - self.head.len();
        self.head.extend_from_slice(&chunk[..room.min(chunk.len())]);
        if !self.text {
            return;
        }
        if chunk.contains(&0) {
            self.text = false;
            return;
        }
        let joined;
        let bytes = if self.pending.is_empty() {
            chunk
        } else {
            joined = [self.pending.as_slice(), chunk].concat();
            &joined
        };
        match std::str::from_utf8(bytes) {
            Ok(_) => self.pending.clear(),
            // A sequence cut by the end of the chunk is kept for the next one.
            Err(e) if e.error_len().is_none() => self.pending = bytes[e.valid_up_to()..].to_vec(),
            Err(_) => self.text = false,
        }
    }

    pub fn finish(self) -> Sniffed {
        let sha256 = self
            .sha
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Sniffed {
            bytes: self.bytes,
            sha256,
            image: image_type(&self.head),
            zip: self.head.starts_with(b"PK\x03\x04"),
            text: self.text && self.pending.is_empty(),
        }
    }
}

impl Write for Sniffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.feed(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The image the first bytes say, by the extension tehas names it with.
pub fn image_type(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if head.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some("gif")
    } else if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}
