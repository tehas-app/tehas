//! attach: the bytes of a file attached to an issue, read by a program and
//! not by the shell. lib/attachments.sh runs it as $TEHAS_HOME/libexec/attach
//! for tehas work, tehas plan, tehas dev and tehasbot protolab. It fetches
//! nothing and reads no setting: the shell fetches with curl and keeps the
//! rules of who may attach and how much an issue is given.
//!
//!   attach sniff <file>
//!     What the bytes of a regular file say. Answers
//!     {"api":1,"verb":"sniff","ok":true,"bytes":N,"sha256":"<hex>",
//!      "image":"png"|"jpg"|"gif"|"webp"|null,"zip":bool,"text":bool}.
//!     image is what the first bytes say, zip is true for the four bytes that
//!     start an archive, text is true for valid UTF-8 with no NUL byte.
//!   attach unpack <zip> <dir> --max-bytes N --max-files N
//!     The archive into <dir>, by the rules in src/unpack.rs. Answers
//!     {"api":1,"verb":"unpack","ok":true,"entries":[...]}, one entry per
//!     entry of the archive in its order: {"name","kind","bytes","sha256",
//!     "image","text"} for a file written under <dir>, {"skip","reason"} for
//!     one left out.
//!
//! Each answers one line on standard output. Exit: 0 the answer; 64 an
//! argument the verb does not know; 1 the file cannot be read (`attach:
//! unreadable: ...`, which for unpack is an archive it cannot read and
//! nothing was written) or <dir> cannot be made (`attach: unwritable: ...`).

mod sniff;
mod unpack;
mod zip;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::exit;

/// The most bytes of an archive read at all: a fetch is capped well below it.
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;

fn usage(why: &str) -> ! {
    eprintln!("attach: usage: {why}");
    eprintln!("usage: attach sniff <file> | attach unpack <zip> <dir> --max-bytes N --max-files N");
    exit(64);
}

fn refuse(word: &str, why: &str) -> ! {
    eprintln!("attach: {word}: {why}");
    exit(1);
}

fn answer(verb: &str, mut v: Value) {
    v["api"] = json!(1);
    v["verb"] = json!(verb);
    v["ok"] = json!(true);
    println!("{v}");
}

/// A regular file and not a link, opened for reading.
fn open_file(path: &str) -> fs::File {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => refuse("unreadable", &format!("{path} is not a regular file")),
        Err(e) => refuse("unreadable", &format!("{path}: {e}")),
    }
    fs::File::open(path).unwrap_or_else(|e| refuse("unreadable", &format!("{path}: {e}")))
}

fn sniff_verb(args: &[String]) {
    let [path] = args else { usage("sniff <file>") };
    let mut file = open_file(path);
    let mut sniffer = sniff::Sniffer::default();
    let mut buf = [0u8; 64 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => sniffer.feed(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => refuse("unreadable", &format!("{path}: {e}")),
        }
    }
    let s = sniffer.finish();
    answer(
        "sniff",
        json!({"bytes": s.bytes, "sha256": s.sha256, "image": s.image, "zip": s.zip, "text": s.text}),
    );
}

fn unpack_verb(args: &[String]) {
    let mut pos = Vec::new();
    let (mut max_bytes, mut max_files) = (None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--max-bytes" => {
                let v = it.next().and_then(|v| v.parse::<u64>().ok());
                max_bytes = Some(v.unwrap_or_else(|| usage("--max-bytes needs a whole number")));
            }
            "--max-files" => {
                let v = it.next().and_then(|v| v.parse::<i64>().ok());
                max_files = Some(v.unwrap_or_else(|| usage("--max-files needs a whole number")));
            }
            s if s.starts_with("--") => usage(&format!("unpack does not know {s}")),
            _ => pos.push(a),
        }
    }
    let ([zip, dir], Some(max_bytes), Some(max_files)) = (pos.as_slice(), max_bytes, max_files)
    else {
        usage("unpack <zip> <dir> --max-bytes N --max-files N")
    };
    let mut data = Vec::new();
    let file = open_file(zip);
    if let Err(e) = file.take(MAX_ARCHIVE + 1).read_to_end(&mut data) {
        refuse("unreadable", &format!("{zip}: {e}"));
    }
    if data.len() as u64 > MAX_ARCHIVE {
        refuse("unreadable", &format!("{zip} is over {MAX_ARCHIVE} bytes"));
    }
    let archive = zip::Archive::open(&data)
        .unwrap_or_else(|why| refuse("unreadable", &format!("{zip}: {why}")));
    let entries = unpack::unpack(&archive, Path::new(dir), max_bytes, max_files)
        .unwrap_or_else(|e| refuse("unwritable", &format!("{dir}: {e}")));
    answer("unpack", json!({"entries": entries}));
}

fn main() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| {
            a.into_string()
                .unwrap_or_else(|_| usage("an argument that is not UTF-8"))
        })
        .collect();
    match args.first().map(String::as_str) {
        Some("sniff") => sniff_verb(&args[1..]),
        Some("unpack") => unpack_verb(&args[1..]),
        Some(other) => usage(&format!("no verb {other}")),
        None => usage("no verb"),
    }
}
