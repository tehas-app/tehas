//! attach: the files attached to an issue, fetched and judged by a program and
//! not by the shell, and what a cycle was given, said as text. lib/attachments.sh
//! runs it as $TEHAS_HOME/libexec/attach for tehas work, tehas plan and tehas dev,
//! and the console for tehasbot protolab. It reads no setting: the shell passes
//! each as a flag, the approval word (rule 5) among them. Only `issue` and
//! `answer` hold GH_TOKEN and reach the network, through curl and gh, and they
//! read what was fetched through `sniff` and `unpack` run as a child with no
//! environment (src/take.rs).
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
//!   attach issue <issue> <dir> --approved WORD --record PATH --repo R
//!       --comments 0|1 --allowed LOGINS --millwright LOGIN
//!       --images 0|1 --files 0|1 --zips 0|1 --attach PATH
//!     Remake <dir> from what the issue links, with a manifest.json. The issue
//!     as JSON on standard input, or nothing to read it with gh. --approved is
//!     the word `approved`, or the links of an outsider's issue are left out.
//!     --attach is this program, run as the child that reads what was fetched.
//!     GH_TOKEN goes to github.com on curl's standard input. Answers "why":
//!     null, or the reason nothing may start.
//!   attach answer <dir> <reply file> <comment id> --images 0|1 --files 0|1
//!       --zips 0|1 --attach PATH
//!     Add to a session's <dir> what the reply links. Answers "why", and "new"
//!     and "left", the lines that say what was added and what was left out.
//!
//! The rules of said, retry-context, changed and left-out are in src/given.rs,
//! those of issue and answer in src/take.rs. Each answers one line on
//! standard output. Exit: 0 the answer; 64 an argument the verb does not know;
//! 1 the file cannot be read (`attach: unreadable: ...`, which for unpack is an
//! archive it cannot read and nothing was written) or <dir> or <file> cannot be
//! written (`attach: unwritable: ...`).

mod given;
mod manifest;
mod sniff;
mod take;
mod unpack;
mod zip;

#[cfg(test)]
mod given_tests;
#[cfg(test)]
mod take_tests;
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
    eprintln!("       attach issue <issue> <dir> --approved WORD --record PATH --repo R --comments 0|1 --allowed LOGINS --millwright LOGIN --images 0|1 --files 0|1 --zips 0|1 --attach PATH");
    eprintln!("       attach answer <dir> <reply file> <comment id> --images 0|1 --files 0|1 --zips 0|1 --attach PATH");
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

fn issue_verb(args: &[String]) {
    let (
        pos,
        [approved, record, repo, comments, allowed, millwright, images, files, zips, program],
    ) = flagged(
        "issue",
        args,
        [
            "--approved",
            "--record",
            "--repo",
            "--comments",
            "--allowed",
            "--millwright",
            "--images",
            "--files",
            "--zips",
            "--attach",
        ],
    );
    let [issue, dir] = pos.as_slice() else {
        usage("issue <issue> <dir> and its flags")
    };
    let mut data = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut data) {
        refuse("unreadable", &format!("standard input: {e}"));
    }
    let token = std::env::var("GH_TOKEN").unwrap_or_default();
    let run = take::IssueRun {
        issue,
        dir,
        repo,
        approved,
        record,
        wants: take::Wants::of(images, files, zips),
        comments: comments == "1",
        allowed,
        millwright,
        data: &String::from_utf8_lossy(&data),
    };
    let net = take::Children {
        token: &token,
        program,
    };
    answer("issue", json!({"why": take::issue(&run, &net).err()}));
}

fn answer_verb(args: &[String]) {
    let (pos, [images, files, zips, program]) = flagged(
        "answer",
        args,
        ["--images", "--files", "--zips", "--attach"],
    );
    let [dir, text, id] = pos.as_slice() else {
        usage("answer <dir> <reply file> <comment id> and its flags")
    };
    let token = std::env::var("GH_TOKEN").unwrap_or_default();
    let run = take::AnswerRun {
        dir,
        text,
        id,
        wants: take::Wants::of(images, files, zips),
    };
    let net = take::Children {
        token: &token,
        program,
    };
    let v = match take::answer(&run, &net) {
        Ok((new, left)) => json!({"why": null, "new": new, "left": left}),
        Err(why) => json!({"why": why, "new": "", "left": ""}),
    };
    answer("answer", v);
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
        Some("issue") => issue_verb(&args[1..]),
        Some("answer") => answer_verb(&args[1..]),
        Some(other) => usage(&format!("no verb {other}")),
        None => usage("no verb"),
    }
}
