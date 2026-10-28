//! `store batch`: several of the store's reads in one process and one opening
//! of the database.
//!
//! An idle poll used to start the store once per question it asked, about 49
//! processes for one poll (#941). The per-issue reads of a snapshot --
//! `plan-show <n>`, `park-show <n>`, `question-known <n> --id <id>` and
//! `approval-current <n>` -- are the same four verbs asked of each issue, so
//! the poll asks them once here: a JSON array of calls on standard input, each
//! call an array of arguments, and one JSON array back in the same order, each
//! answer `{"rc": <exit status>, "out": <what the verb prints>}`.
//!
//! What stays outside this file is the contract the four verbs have on their
//! own: each element is parsed and run by the same code the single call runs,
//! so an answer of a batch is the answer of that call byte for byte. Only the
//! verbs of `READS` are answered; anything else is refused before it reaches
//! the store, which `verbs::parse_read` holds as a type -- a write is not a
//! `ReadCall`, so there is no call a batch can run that writes.

use crate::verbs::{self, Fail};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

/// The longest request this verb reads, counted in bytes of the whole JSON
/// array on its standard input. At the ~40 bytes of one call of the poll's
/// list this is room for about 13k calls, far more than `MAX_CALLS` allows, so
/// in practice the count is the bound that bites and the bytes are defence
/// against a writer that is not the poll. A request over it is the usage error
/// it is, and the store is not opened for it.
pub const MAX_BYTES: u64 = 1 << 20;

/// The most calls in one batch, counted in elements of the JSON array. The
/// poll's list is the four reads of each issue it would decide about -- four
/// calls for every issue it read -- and a host that asks about more than this
/// is refused the whole request with 64 rather than answered in part, because
/// half a snapshot's answers are no use to a poll that decides by all of them.
pub const MAX_CALLS: usize = 512;

/// The verbs one batch answers. `verbs::parse_read` takes its four arms from
/// this list, so a name added here without an audit of its body is a name
/// `parse_read` will try to parse; the arms it does not know are refused.
pub const READS: [&str; 4] = [
    "plan-show",
    "park-show",
    "question-known",
    "approval-current",
];

/// The longest call of the four in arguments: `question-known <issue> --id
/// <id>`. The bound is not what keeps a write out of a batch -- `verbs::parse_read`
/// answers for that, by name -- it is what keeps a request that is not four
/// reads of an issue from being read at all. A call of more arguments is refused
/// with the whole request, before one call of it runs.
const MAX_ARGS: usize = 4;

/// Whether `verb` names one of the four reads this verb answers.
pub fn is_read(verb: &str) -> bool {
    READS.contains(&verb)
}

/// What one element of the request named: the call to run, or the refusal of
/// it. An element that is not an array of arguments, or holds a verb that is
/// none of the four, is refused before the store is opened. The variants are
/// built only by `parse`, and a `Request` holds only what `parse` judged.
#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    /// A call the store will read: parsed by `verbs::parse_read`, so it is one
    /// of the four reads and carries the arguments already read.
    Call(verbs::ReadCall),
    /// The refusal of one this verb does not run: its exit status and the line
    /// the single call prints on standard error, `store: ` left off. A verb
    /// outside the four is refused by its name alone, with the line naming the
    /// verb and the issue it was asked about.
    Refused(u8, String),
}

/// The request of one batch, held by a type whose only constructor is
/// `Request::of`: the judgements of `parse` and nothing else. `main` builds it
/// and `execute` runs it, so a caller of `execute` cannot name a verb the batch
/// is allowed to read.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    asked: Vec<Asked>,
}

impl Request {
    /// The request of one batch: the standard input read whole and judged by
    /// `parse`. A caller with no usable home asks for nothing, so this is not
    /// reached before the guard of `main` has passed.
    pub fn read() -> Result<Request, String> {
        parse(&read_input()?).map(|asked| Request { asked })
    }
}

/// The request of one batch: a JSON array whose elements are arrays of
/// arguments. One that is not JSON at all, holds an element that is not a list
/// of arguments, holds a call of more than `MAX_ARGS` arguments, or holds more
/// than `MAX_CALLS` elements, is the usage error it is: the whole request is
/// judged before one call of it runs, so a poll that built half a request reads
/// no answers of the other half.
pub fn parse(text: &str) -> Result<Vec<Asked>, String> {
    let given: Value =
        serde_json::from_str(text).map_err(|e| format!("batch: stdin is not JSON: {e}"))?;
    let given = given
        .as_array()
        .ok_or_else(|| "batch: stdin is not a JSON array".to_string())?;
    if given.len() > MAX_CALLS {
        return Err(format!(
            "batch: {} calls, more than the {MAX_CALLS} of one batch",
            given.len()
        ));
    }
    let mut asked = Vec::with_capacity(given.len());
    for call in given {
        let args = call
            .as_array()
            .ok_or_else(|| "batch: a call is not a list of arguments".to_string())?;
        if args.len() > MAX_ARGS {
            return Err(format!(
                "batch: a call holds {} arguments, more than {MAX_ARGS}",
                args.len()
            ));
        }
        let args = args
            .iter()
            .map(|a| {
                a.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "batch: an argument is not text".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if args.is_empty() {
            return Err("batch: a call holds no verb".to_string());
        }
        // The verb's own grammar answers for it: one of the four reads becomes
        // a call, and anything else -- a `plan-show --open`, a write, a name
        // the store does not have -- becomes the refusal the single command
        // prints for the same arguments, with the exit it gives with them.
        asked.push(match verbs::parse_read(&args) {
            Ok(call) => Asked::Call(call),
            Err((rc, why)) => Asked::Refused(rc, why),
        });
    }
    Ok(asked)
}

/// The request on standard input, read whole and bounded. The caller closes
/// the pipe when there is nothing to read, as the poll does.
pub fn read_input() -> Result<String, String> {
    let mut text = String::new();
    // The poll writes its request from a thread and does not close the pipe, so
    // a plain read would wait for the other end forever. A read that returns
    // nothing is the end of what the writer sent: what the writer did not send
    // is not missing, it is the whole of the request. A request still empty
    // after that is a pipe that was never written, which is the usage error it
    // is: the poll that wrote nothing asked for nothing, and a batch of no
    // calls is not read.
    let mut stdin = std::io::stdin().take(MAX_BYTES + 1);
    let mut buf = [0u8; 8192];
    loop {
        let got = stdin
            .read(&mut buf)
            .map_err(|e| format!("batch: stdin could not be read: {e}"))?;
        if got == 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&buf[..got]));
        if text.len() as u64 > MAX_BYTES {
            return Err(format!("batch: stdin is over {MAX_BYTES} bytes"));
        }
    }
    Ok(text)
}

/// The answers, one object per call, in the order of the request: rc first and
/// out second. `out` is the text the verb prints -- its one JSON line, or the
/// word of its refusal -- so a batch's answer and the standard output of the
/// single call are the same bytes: a caller that reads one can read the other.
fn answers(lines: Vec<(i64, String)>) -> String {
    let items: Vec<String> = lines
        .into_iter()
        .map(|(rc, out)| format!("{{\"api\":1,\"rc\":{rc},\"out\":{}}}", verbs::j(out)))
        .collect();
    format!(
        "{{\"api\":1,\"verb\":\"batch\",\"ok\":true,\"answers\":[{}]}}",
        items.join(",")
    )
}

/// Runs the batch: the database is opened once, in one transaction, and every
/// call reads that one snapshot. A call its own arguments would refuse is
/// answered from the request alone, and the store is not opened at all for a
/// batch that holds nothing to read.
pub fn run(home: &Path, asked: &Request) -> Result<String, Fail> {
    let asked = &asked.asked;
    // The calls that reach the store, with the place each answer belongs to.
    // A refusal asks nothing of the store and holds the line the single call
    // would have printed, with the exit the single command would have had, so
    // a caller that reads the word of a failure reads the same word for a read
    // of a batch and for a read made on its own.
    let mut wanted: Vec<(usize, verbs::ReadCall)> = Vec::new();
    let mut lines: Vec<(i64, String)> = vec![(64, String::new()); asked.len()];
    for (i, a) in asked.iter().enumerate() {
        match a {
            Asked::Refused(rc, why) => lines[i] = (i64::from(*rc), why.clone()),
            Asked::Call(call) => wanted.push((i, call.clone())),
        }
    }
    if wanted.is_empty() {
        // Nothing to read: the database is not opened at all, so a batch of
        // refusals is answered by a store that is not there.
        return Ok(answers(lines));
    }
    let calls: Vec<verbs::ReadCall> = wanted.iter().map(|(_, c)| c.clone()).collect();
    let got: Vec<(i64, String)> = verbs::look_many(home, calls.as_slice())?;
    for ((i, _), got) in wanted.iter().zip(got) {
        lines[*i] = got;
    }
    Ok(answers(lines))
}
