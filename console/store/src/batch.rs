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
//! so an answer of a batch is the answer of that call byte for byte. Only
//! those four verbs are answered; anything else in the list is refused with 64
//! and the database is not opened for it, and no verb of the four writes.

use crate::verbs::{self, Fail};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

/// The longest request the verb reads. The poll's list is one line per issue
/// it read; a request over this is not a batch anyone meant.
pub const MAX_BYTES: u64 = 1 << 20;

/// The most calls in one batch: the poll's four reads of each issue it would
/// decide about, and a poll decides about far fewer issues than that.
pub const MAX_CALLS: usize = 512;

/// The four reads of one issue, and the most arguments of the call that asks
/// for one. `question-known` is the longest: `<issue> --id <id>`.
const READS: [&str; 4] = [
    "plan-show",
    "park-show",
    "question-known",
    "approval-current",
];
const MAX_ARGS: usize = 4;

/// The line of a call this part refuses: what the same call would have said
/// when the store was asked for it on its own. The call is parsed by the same
/// grammar the single call goes through, so the word -- `unknown` for a row
/// that is not there, `usage` for arguments that do not parse, `busy` for a
/// store that could not be read -- is the one the caller reads either way. The
/// store is not opened for it: only the arguments are asked, and a call that
/// needs no argument to be wrong is refused with the word its verb gives for
/// the case, which is what a caller that logs failures sorts on.
fn refusal(args: &[String]) -> (i64, String) {
    let verb = args.first().map(String::as_str).unwrap_or("");
    // The arguments say what the call asked of the store, so they are parsed by
    // the store's own grammar first: a call that does not parse is refused with
    // the usage line the single call gets, and a call that does is refused as a
    // read this verb does not answer. An argument that is not one of the verb's
    // is the call's own fault, and says so, whatever the verb was.
    match verbs::parse(args) {
        // The arguments do not parse: the call's own fault. The line says the
        // exit: `usage` is the one of a call that stops at its arguments, and
        // anything else is the verb's own refusal, which is the exit of a read
        // of the store.
        Some(Err(why)) => (usage_rc(&why), why),
        // The arguments parse, so the call would have been run: what it says of
        // the row it asks about is what its own verb answers, and the only case
        // this part answers without the store is one whose answer does not
        // depend on the store at all. A call that reads a row of the store is
        // refused for the row it does not hold, which is the answer the verb
        // gives when nothing is on record; a batch does not open the store for
        // one, so the line is the verb's own refusal of nothing on record.
        Some(Ok(_)) => (
            1,
            format!("{verb} #{}: not a read of a batch", issue_of(args)),
        ),
        // An argument the store does not know: the same line the command gives.
        None => (64, format!("usage {verb}: unknown verb")),
    }
}

/// The exit of a line the store's own grammar refused: 64 for a call that stops
/// at its arguments, which is what the single command answers a call that cannot
/// be run, and 1 for the verb's own refusal of the row it was asked about, which
/// is the exit of the read made alone. A caller that reads the word of a failure
/// reads the same word, and the same exit, either way.
fn usage_rc(why: &str) -> i64 {
    if why.starts_with("usage ") {
        64
    } else {
        1
    }
}

/// The issue a call asks about, for the line of one this part refuses: the
/// second argument of the reads of an issue, and 0 for a call that names none,
/// which is how the store's own refusals of a call without an issue read.
fn issue_of(args: &[String]) -> &str {
    args.get(1)
        .map(String::as_str)
        .filter(|a| a.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("0")
}

/// Whether `verb` names one of the four reads this verb answers.
pub fn is_read(verb: &str) -> bool {
    READS.contains(&verb)
}

/// What one element of the request named: the call to run, or the word of the
/// refusal. An element that is not an array of arguments, or names a verb that
/// is none of the four, is refused with 64 before the store is opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    Call(Vec<String>),
    /// The refusal of one this verb does not run: its exit status and the line
    /// the single call prints on standard error, `store: ` left off.
    Refused(i64, String),
}

/// The request of one batch: a JSON array whose elements are arrays of
/// arguments. One that is not JSON at all, holds an element that is not a list
/// of arguments, or holds more than `MAX_CALLS` elements, is the usage error it
/// is: the whole request is judged before one call of it runs, so a poll that
/// built half a request reads no answers of the other half.
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
        asked.push(match args.first() {
            // A verb this part does not know is the store's own refusal, which
            // is 64 for one that holds no verb; one it knows that is not a read
            // is refused here and never reaches the store, and so does a request
            // whose every call is refused, which never opens the database.
            None => return Err("batch: a call holds no verb".to_string()),
            // The line names the verb and carries the issue the call asked
            // about: the same shape the single call's own refusal has, so a
            // caller that reads the word of a failure reads the same word for a
            // read of a batch and a read made on its own.
            Some(verb) if !is_read(verb) => {
                let (rc, why) = refusal(&args);
                Asked::Refused(rc, why)
            }
            Some(_) => Asked::Call(args),
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
/// call reads that one snapshot. A call that its own arguments would refuse
/// gets 64 and asks nothing of the store; a call that names a verb this verb
/// does not answer is refused before it, without a name of the store's being
/// parsed at all.
pub fn run(home: &Path, asked: Vec<Asked>, now: i64) -> Result<String, Fail> {
    // The calls that reach the store, with the place each answer belongs to.
    // A call this part refuses asks nothing of the store and gets the line the
    // single call would have printed, with the exit the single command would
    // have had: a caller that reads the word of a failure reads the same word
    // for a read of a batch and for a read made on its own, whether that word
    // is 64's `usage` or the 1 of a verb that found nothing on record.
    let mut wanted: Vec<(usize, Vec<String>)> = Vec::new();
    let mut lines: Vec<(i64, String)> = vec![(64, String::new()); asked.len()];
    for (i, a) in asked.into_iter().enumerate() {
        match a {
            Asked::Refused(rc, why) => lines[i] = (rc, why),
            Asked::Call(args) => wanted.push((i, args)),
        }
    }
    if wanted.is_empty() {
        // Nothing to read: the database is not opened at all, so a batch of
        // refusals is answered by a store that is not there.
        return Ok(answers(lines));
    }
    let calls: Vec<Vec<String>> = wanted.iter().map(|(_, args)| args.clone()).collect();
    let got: Vec<(i64, String)> = verbs::look_many(home, &calls, now)?;
    for ((i, _), got) in wanted.iter().zip(got) {
        lines[*i] = got;
    }
    Ok(answers(lines))
}
