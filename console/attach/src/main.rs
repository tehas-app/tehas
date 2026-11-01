//! attach: the bytes of a file attached to an issue, read by a program and
//! not by the shell, and what a cycle was given, said as text. lib/attachments.sh
//! runs it as $TEHAS_HOME/libexec/attach for tehas work, tehas plan, tehas dev
//! and tehasbot protolab. It fetches nothing and reads no setting: the shell
//! fetches with curl, keeps the rules of who may attach and how much an issue
//! is given, and passes a setting as a flag.
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
//!   attach said <dir> <what> --reads R --build PATH
//!     The comment on what <dir>/manifest.json gave a <what>. --reads is what
//!     the sandbox image says of its pi-rukas, --build the script that records
//!     it. Answers {"api":1,"verb":"said","ok":true,"text":"..."|null}.
//!   attach retry-context <record> --comments 0|1 --millwright LOGIN
//!     The retry-context.md of an approval record. Answers "text" as said does.
//!   attach changed <kept manifest> <manifest>
//!     Answers "link": the first link that gave other bytes under the same
//!     approval, or null, also when a manifest cannot be read.
//!   attach left-out <issue> <file> <record> --images 0|1 --files 0|1
//!     What a cycle given nothing was not given, into <file> whole or not at
//!     all. --images and --files are the instance's settings. Answers "gap":
//!     the line that says it, or null when there is nothing to say.
//!
//! The rules of the last four are in src/given.rs. Each answers one line on
//! standard output. Exit: 0 the answer; 64 an argument the verb does not know;
//! 1 the file cannot be read (`attach: unreadable: ...`, which for unpack is an
//! archive it cannot read and nothing was written) or <dir> or <file> cannot be
//! written (`attach: unwritable: ...`).

mod given;
mod sniff;
mod unpack;
mod zip;

#[cfg(test)]
mod given_tests;
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
    eprintln!("       attach said <dir> <what> --reads R --build PATH");
    eprintln!("       attach retry-context <record> --comments 0|1 --millwright LOGIN");
    eprintln!("       attach changed <kept manifest> <manifest>");
    eprintln!("       attach left-out <issue> <file> <record> --images 0|1 --files 0|1");
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

/// The arguments that are no flag, and the value of each flag in `known`, in
/// that order; a flag not given is empty.
fn flagged<'a, const N: usize>(
    verb: &str,
    args: &'a [String],
    known: [&str; N],
) -> (Vec<&'a str>, [&'a str; N]) {
    let mut pos = Vec::new();
    let mut values = [""; N];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match known.iter().position(|k| k == a) {
            Some(i) => match it.next() {
                Some(v) => values[i] = v,
                None => usage(&format!("{a} needs a value")),
            },
            None if a.starts_with("--") => usage(&format!("{verb} does not know {a}")),
            None => pos.push(a.as_str()),
        }
    }
    (pos, values)
}

fn refused<T>(r: Result<T, given::Refused>) -> T {
    r.unwrap_or_else(|(word, why)| refuse(word, &why))
}

fn said_verb(args: &[String]) {
    let (pos, [reads, build]) = flagged("said", args, ["--reads", "--build"]);
    let [dir, what] = pos.as_slice() else {
        usage("said <dir> <what> --reads R --build PATH")
    };
    let text = refused(given::said_of(dir, what, reads, build));
    answer("said", json!({"text": text}));
}

fn retry_context_verb(args: &[String]) {
    let (pos, [comments, millwright]) =
        flagged("retry-context", args, ["--comments", "--millwright"]);
    let ([record], "0" | "1") = (pos.as_slice(), comments) else {
        usage("retry-context <record> --comments 0|1 --millwright LOGIN")
    };
    let text = refused(given::retry_context_of(record, comments == "1", millwright));
    answer("retry-context", json!({"text": text}));
}

fn changed_verb(args: &[String]) {
    let [kept, new] = args else {
        usage("changed <kept manifest> <manifest>")
    };
    answer("changed", json!({"link": given::changed_of(kept, new)}));
}

fn left_out_verb(args: &[String]) {
    let (pos, [images, files]) = flagged("left-out", args, ["--images", "--files"]);
    let [issue, file, record] = pos.as_slice() else {
        usage("left-out <issue> <file> <record> --images 0|1 --files 0|1")
    };
    let gap = refused(given::left_out(issue, file, record, images, files));
    answer("left-out", json!({"gap": gap}));
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
        Some("said") => said_verb(&args[1..]),
        Some("retry-context") => retry_context_verb(&args[1..]),
        Some("changed") => changed_verb(&args[1..]),
        Some("left-out") => left_out_verb(&args[1..]),
        Some(other) => usage(&format!("no verb {other}")),
        None => usage("no verb"),
    }
}
