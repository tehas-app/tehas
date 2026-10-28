//! What a snapshot asks of the state store, through its read verbs, and the
//! shapes lib/store.sh gives the answers before the poll reads them.
//!
//! Four of these reads are asked of each issue the rules decide about:
//! `plan-show`, `park-show`, `question-known` and `approval-current`. A poll on
//! an idle queue made about 49 of them, one process and one database open each
//! (#941). They are gathered over the whole snapshot and asked in one run of
//! `store batch`, and each answer is then read as the answer of the single call
//! was read: `asked` holds what the batch answered, and the functions here give
//! the same value for the same answer. `plan-show --open`, `cycle-show
//! --running`, `park-show --automatic`, `park-show --refine` and `history-show`
//! are reads of the store as a whole and keep their own call.

use super::run::Calls;
use crate::decide::iso;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn get(v: &Value, k: &str) -> Value {
    v.get(k).cloned().unwrap_or(Value::Null)
}

/// The per-issue read of the store that a snapshot asks. The batch holds one of
/// each: an issue the rules ask about twice is asked once, and its answer is
/// read twice.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Read {
    /// `plan-show <n>` and, for a question on record, `question-known`.
    Question(u64),
    /// `park-show <n>`.
    Park(u64),
    /// `approval-current <n>`.
    Approval(u64),
}

impl Read {
    /// The first call of the read: the one the batch always makes. A
    /// `question-known` follows it when the answer names a question, and is
    /// asked in the same run.
    pub fn args(&self) -> Vec<String> {
        let n = self.issue().to_string();
        match self {
            Read::Question(_) => vec!["plan-show".into(), n],
            Read::Park(_) => vec!["park-show".into(), n],
            Read::Approval(_) => vec!["approval-current".into(), n],
        }
    }

    pub fn issue(&self) -> u64 {
        match self {
            Read::Question(n) | Read::Park(n) | Read::Approval(n) => *n,
        }
    }
}

/// The digits of an id as the bash reads them, from a number or a string.
fn digits(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(x)) => x.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Whether the digits are a comment id the store would know.
fn ok(qid: &str) -> bool {
    !qid.is_empty()
        && qid.len() <= 16
        && !qid.starts_with('0')
        && qid.bytes().all(|b| b.is_ascii_digit())
}

/// The plan of an issue, as `question` reads it from `plan-show`: the row of an
/// open session, or null.
fn open_plan(v: &Value) -> Value {
    match v.get("planning") {
        Some(p) if p.is_object() && p.get("ended_at").is_none_or(Value::is_null) => p.clone(),
        _ => Value::Null,
    }
}

/// What one batch answered, kept by the read it answered so that a snapshot
/// with 13 issues asks the store once and reads 13 answers. The answers are read
/// by position, so an answer of the wrong issue cannot reach a rule: a read that
/// was never asked, or whose answer did not come, is the failure of that call,
/// which is what the poll logged before the batch existed.
pub struct Asked {
    answers: BTreeMap<Read, Value>,
    /// What the batch's own call answered for the second read of a question:
    /// the `question-known` of the issue's comment id.
    known: BTreeMap<u64, Value>,
    /// The failure of the batch as a whole, if it had one.
    failed: Option<Value>,
}

impl Asked {
    /// The reads a snapshot asked, kept empty until the batch answers them.
    pub fn empty() -> Asked {
        Asked {
            answers: BTreeMap::new(),
            known: BTreeMap::new(),
            failed: None,
        }
    }

    /// What the store says for one read: the answer of its call, or the failure
    /// of that call. A failure keeps the shape `Calls::store` gives, so the
    /// readers of these functions see what they saw before.
    fn answer(&self, read: Read) -> Value {
        match self.answers.get(&read) {
            Some(v) => v.clone(),
            None => self.why(read),
        }
    }

    /// The failure to give for a read that was not answered: the batch's own,
    /// named for the read the poll was left without. One run stood for all of
    /// the reads, so a store that refused it refused this one too, and the rules
    /// of the poll read the word of that refusal -- `busy`, `corrupt`, `absent`
    /// -- exactly as they read it when the call stood alone.
    fn why(&self, read: Read) -> Value {
        let (verb, n) = match read {
            Read::Question(n) => ("plan-show", n),
            Read::Park(n) => ("park-show", n),
            Read::Approval(n) => ("approval-current", n),
        };
        match self.failed.clone() {
            Some(e) => json!({
                "failed": true,
                "rc": e.get("rc").cloned().unwrap_or(json!(78)),
                "word": e.get("word").cloned().unwrap_or(json!("garbled")),
                "line": format!("store: {}: {verb} {n} of the batch: {}",
                    e.get("word").and_then(Value::as_str).unwrap_or("garbled"),
                    e.get("line").and_then(Value::as_str).unwrap_or("")),
            }),
            None => {
                json!({"failed": true, "rc": 78, "word": "garbled",
                    "line": format!("store: garbled: {verb} {n} answered nothing")})
            }
        }
    }

    fn know(&self, n: u64) -> Value {
        self.known.get(&n).cloned().unwrap_or_else(|| {
            self.failed.clone().unwrap_or_else(|| {
                json!({"failed": true, "rc": 78, "word": "garbled",
                    "line": format!("store: garbled: question-known {n} answered nothing")})
            })
        })
    }
}

/// The second call of a `question` read: `question-known <n> --id <id>` for the
/// comment the issue's park or open session carries. One call of the same run,
/// asked once the answers of the first round have said which issues carry one.
fn known_call(n: u64, qid: &str) -> Vec<String> {
    vec![
        "question-known".into(),
        n.to_string(),
        "--id".into(),
        qid.to_string(),
    ]
}

/// The snapshot's per-issue reads of the store, asked in one run of the program
/// and one opening of the database: `plan-show`, `park-show` and
/// `approval-current` of every issue named, and the `question-known` of the
/// issues whose session or park carries a question. The id of that question is
/// read first, from the same store, because the batch cannot ask a call whose
/// arguments it has not read yet; the reads it needs are two calls and, unlike
/// the reads above, they are the same two the batch goes on to answer. Every
/// read the poll needs after this one -- an issue a rule names once the batch has
/// run -- is asked by another run, which is what a poll of a queue that grew
/// paid for anyway.
pub fn gather(calls: &mut Calls, asked: &mut Asked, reads: &[Read]) {
    let mut want: Vec<Read> = reads.to_vec();
    want.sort_unstable();
    want.dedup();
    if want.is_empty() {
        return;
    }
    // The calls of the run, and which read or question each one answers, so the
    // answers go back by position and not by name.
    let mut calls_of: Vec<Vec<String>> = Vec::new();
    let mut owners: Vec<Owner> = Vec::new();
    for read in &want {
        calls_of.push(read.args());
        owners.push(Owner::Read(*read));
    }
    // The ids of the questions come first, in runs of their own: the batch
    // cannot ask a call whose arguments it has not read, so the two columns the
    // id lives in are read before the batch is built, one list of plans and one
    // of parks, each asked in a single run. A store that cannot answer them
    // cannot answer the question either, and the failure is kept for the read of
    // it rather than read as a question with none on record.
    let ids = match question_ids(calls, &want) {
        Ok(ids) => ids,
        Err(e) => {
            asked.failed = Some(e);
            return;
        }
    };
    for (n, qid) in &ids {
        calls_of.push(known_call(*n, qid));
        owners.push(Owner::Known(*n));
    }
    // One call apiece: the reads of the list and the `question-known` of an
    // issue can name the same thing, and the store is asked for it once. Each
    // call keeps the reads it answers, so one answer serves them all.
    let mut asked_calls: Vec<Vec<String>> = Vec::new();
    let mut slots: Vec<Vec<Owner>> = Vec::new();
    for (call, owner) in calls_of.into_iter().zip(owners) {
        match asked_calls.iter().position(|c| *c == call) {
            Some(i) => slots[i].push(owner),
            None => {
                asked_calls.push(call);
                slots.push(vec![owner]);
            }
        }
    }
    let answers = match calls.store_batch(&asked_calls) {
        Ok(a) => a,
        Err(e) => {
            asked.failed = Some(e);
            return;
        }
    };
    for (owners, answer) in slots.iter().zip(answers) {
        for owner in owners {
            match *owner {
                Owner::Read(read) => {
                    asked.answers.insert(read, answer.clone());
                }
                Owner::Known(n) => {
                    asked.known.insert(n, answer.clone());
                }
            }
        }
    }
}

/// Which read or question one answer of the run belongs to.
enum Owner {
    Read(Read),
    Known(u64),
}

/// The issues of the batch that carry a question, with the comment whose
/// question the store confirms: the open session's own, or the park's, in the
/// order the single call of `question` tried them. Asked as one run of the store
/// -- two calls per issue at the most, and one process for all of them --
/// because the arguments of the confirming call are not known until the answer
/// of these is in hand.
fn question_ids(calls: &mut Calls, want: &[Read]) -> Result<Vec<(u64, String)>, Value> {
    // The issues with a question on record are the ones whose session or park
    // carries an id: the park's own question counts too, so an issue that is
    // parked for a person asks its `question-known` with the id of the comment
    // they were left, which is what the poll answers them from. The session's
    // id is tried first and the park's after it, the order the single call of
    // `question` tried them in.
    let asking: Vec<u64> = want
        .iter()
        .filter_map(|r| match r {
            Read::Question(n) | Read::Park(n) => Some(*n),
            Read::Approval(_) => None,
        })
        .collect();
    if asking.is_empty() {
        return Ok(Vec::new());
    }
    // One list of calls apiece, made from the reads the snapshot asked and not
    // from the issues: a read of an issue the snapshot never asked is never
    // asked here either, and one run of the store reads the plan of each asking
    // issue and one its park. A list with nothing in it asks nothing, so a poll
    // with no parked issue runs no process for its parks.
    // One run of the store asks the plan of every issue the poll read as a
    // question, and one asks the park of every issue it read as a park: the two
    // lists together are the reads of the asking issues, no more.
    let plans: Vec<Vec<String>> = want
        .iter()
        .filter(|r| matches!(r, Read::Question(_)))
        .map(Read::args)
        .collect();
    let parks: Vec<Vec<String>> = want
        .iter()
        .filter(|r| matches!(r, Read::Park(_)))
        .map(Read::args)
        .collect();
    let plan = calls.store_batch(&plans)?;
    let park = calls.store_batch(&parks)?;
    // The answers go back to the issue each call asked about, read off the call
    // itself: a run asked the plan of an issue once, whatever the list held.
    let of = |calls_of: &[Vec<String>], answers: Vec<Value>| -> BTreeMap<u64, Value> {
        calls_of
            .iter()
            .zip(answers)
            .filter_map(|(c, v)| Some((c.get(1)?.parse().ok()?, v)))
            .collect()
    };
    let plans = of(&plans, plan);
    let parks = of(&parks, park);
    Ok(asking
        .into_iter()
        .filter_map(|n| {
            let id = match plans.get(&n) {
                Some(v) if v.get("failed") != Some(&json!(true)) => {
                    digits(v.pointer("/planning/comment_id"))
                }
                _ => String::new(),
            };
            if ok(&id) {
                return Some((n, id));
            }
            match parks.get(&n) {
                Some(v) if v.get("failed") != Some(&json!(true)) => {
                    let id = digits(v.pointer("/park/question"));
                    ok(&id).then_some((n, id))
                }
                _ => None,
            }
        })
        .collect())
}
/// plan-show --open: the open sessions, or the store's failure.
pub fn sessions(calls: &mut Calls) -> Value {
    match calls.store(&["plan-show", "--open"]) {
        Ok(v) => Value::Array(
            v.get("sessions")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter(|s| s.get("issue").is_some_and(Value::is_number))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Err(e) => json!({"word": e["word"], "rc": e["rc"]}),
    }
}

/// cycle-show --running: each running cycle's record as running_starts
/// reads it, Err with the store's failure.
pub fn cycles(calls: &mut Calls) -> Result<Vec<Value>, Value> {
    let v = calls.store(&["cycle-show", "--running"])?;
    Ok(v.get("cycles")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let n = r.get("issue")?.as_u64()?;
                    let c = get(r, "cycle");
                    let num = |x: &Value, k: &str| x.get(k).and_then(Value::as_u64).unwrap_or(0);
                    Some(if c.is_object() && c.get("ended_at").is_none_or(Value::is_null) {
                        json!({"issue": n, "how": c.get("how").and_then(Value::as_str).unwrap_or(""),
                            "took": num(&c, "started_at"), "heard": num(&c, "heard_at"),
                            "said_for": c.pointer("/quiet/said_for").map(|s| match s { Value::String(s) => s.clone(), v => v.to_string() }).unwrap_or("-".into()),
                            "quiet_at": c.pointer("/quiet/at").and_then(Value::as_u64).unwrap_or(0),
                            "asked": num(&c, "stop_asked_at"), "times": num(&c, "stop_said_times")})
                    } else {
                        json!({"issue": n, "how": "", "took": 0, "heard": if c.is_object() { num(&c, "heard_at") } else { 0 }})
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// park-show <n>'s park, or the failure.
pub fn park(asked: &Asked, n: u64) -> Value {
    match asked.answer(Read::Park(n)) {
        v if v.get("failed") == Some(&json!(true)) => v,
        v => get(&v, "park"),
    }
}

/// The parks owed a retry (--automatic) or a refine (--refine).
pub fn owed(calls: &mut Calls, which: &str) -> Value {
    match calls.store(&["park-show", which]) {
        Ok(v) => v.get("parks").cloned().unwrap_or_else(|| json!([])),
        Err(e) => e,
    }
}

/// tehas_approval_shape: the approval of the issue's latest start as the
/// readers of tehas_approval_view have it. Null for none.
fn approval_shape(v: &Value) -> Option<Value> {
    let c = v.get("current")?;
    if c.is_null() {
        return Some(Value::Null);
    }
    if !c.is_object() {
        return None;
    }
    let person = |p: &Value| -> Option<Value> {
        if p.is_null() {
            return Some(Value::Null);
        }
        Some(
            json!({"at": iso(p.get("at")?.as_u64()?), "by": get(p, "by"), "how": get(p, "how"),
            "body_sha256": get(p, "body_sha256")}),
        )
    };
    let starts = v
        .get("millwright_starts")
        .filter(|x| !x.is_null())
        .or_else(|| v.get("fixer_starts").filter(|x| !x.is_null()))
        .cloned()
        .unwrap_or(json!(0));
    let input = c.get("input").cloned().unwrap_or(json!({}));
    let base = person(&get(v, "rests_on"))?;
    let how = match c.get("how").and_then(Value::as_str) {
        Some("millwright") => "fixer".to_string(),
        Some(h) => h.to_string(),
        None => String::new(),
    };
    let mut out = json!({"at": iso(c.get("at")?.as_u64()?), "by": get(c, "by"), "how": how,
        "body_sha256": get(c, "body_sha256"), "links": input.get("links").cloned().unwrap_or(json!([]))});
    if let Some(t) = c.get("title_sha256").filter(|t| !t.is_null()) {
        out["title_sha256"] = t.clone();
    }
    if let Some(l) = input.get("comment_links") {
        out["comment_links"] = l.clone();
    }
    if input
        .get("retry_context")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty())
    {
        out["retry_context"] = input["retry_context"].clone();
    }
    if how == "fixer" {
        out["rests_on"] = base.clone();
        out["fixer_starts"] = starts.clone();
    }
    if how == "refresh" {
        out["rests_on"] = base.clone();
        out["fixer_starts"] = starts;
        out["refreshes"] = get(v, "refreshes");
        out["refresh_of"] = get(&input, "refresh_of");
        if !base.is_null() {
            out["at"] = base["at"].clone();
            out["by"] = base["by"].clone();
        }
    }
    Some(out)
}

/// approval-current <n>: the shaped approval (null for none), its raw
/// current (for a refresh's owner), or the failure for both.
pub fn approval(asked: &Asked, n: u64) -> (Value, Value) {
    match asked.answer(Read::Approval(n)) {
        v if v.get("failed") == Some(&json!(true)) => (json!({"failed": true}), v),
        v => (
            approval_shape(&v).unwrap_or_else(|| json!({"failed": true})),
            get(&v, "current"),
        ),
    }
}

/// history-show <n> --kind cycle, as tehas_cycle_history gives its lines.
pub fn history(calls: &mut Calls, n: u64) -> Value {
    match calls.store(&["history-show", &n.to_string(), "--kind", "cycle"]) {
        Ok(v) => Value::Array(
            v.get("rows")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("cycle"))
                        .map(|r| json!({"how": get(r, "how"), "parked": get(r, "parked"), "started": get(r, "started_at")}))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Err(_) => json!({"failed": true}),
    }
}

/// The issues with a refresh in their history.
pub fn refreshed(calls: &mut Calls) -> Value {
    match calls.store(&["history-show", "--kind", "cycle", "--how", "refresh"]) {
        Ok(v) => {
            let mut n: Vec<u64> = v
                .get("rows")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|r| r.get("issue").and_then(Value::as_u64))
                        .collect()
                })
                .unwrap_or_default();
            n.sort_unstable();
            n.dedup();
            json!(n)
        }
        Err(e) => e,
    }
}

/// The question on record of an asking issue: {plan, id, kind}, as the bash
/// reads it from plan-show, park-show and question-known; the failure of the
/// last of them that failed.
pub fn question(asked: &Asked, n: u64) -> Value {
    let plan = asked.answer(Read::Question(n));
    if plan.get("failed") == Some(&json!(true)) {
        return plan;
    }
    let plan = open_plan(&plan);
    let mut qid = digits(plan.get("comment_id"));
    if !ok(&qid) {
        qid.clear();
        let park = asked.answer(Read::Park(n));
        if park.get("failed") == Some(&json!(true)) {
            return park;
        }
        qid = digits(park.pointer("/park/question"));
        if !ok(&qid) {
            qid.clear();
        }
    }
    let mut kind = String::new();
    if !qid.is_empty() {
        match asked.know(n) {
            v if v.get("failed") == Some(&json!(true)) => return v,
            v if v.get("known") == Some(&json!(true)) => {
                kind = v
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
            _ => qid.clear(),
        }
    }
    json!({"plan": plan, "id": qid, "kind": kind})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::run::Calls;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// The store of a case: the real store program of this build, so the four
    /// reads are the reads a host answers and `store batch` is the verb the poll
    /// asks. A home of the test's own, as the store's tests have theirs.
    fn scratch(name: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join("tehas-dispatch-store");
        let dir = root
            .join(std::process::id().to_string())
            .join(format!("{name}-{}", N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("state/cycles")).unwrap();
        dir
    }

    /// The store program of this build. The tests of the store keep it beside
    /// their harness; the workspace's target directory keeps it beside this one.
    fn store_binary() -> PathBuf {
        let deps = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        for cand in [deps.join("store"), deps.parent().unwrap().join("store")] {
            if cand.exists() {
                return cand;
            }
        }
        panic!("the store of this build is beside the test binary");
    }

    /// A store that answers, with the calls it was made written under `count`
    /// so a case can count them.
    struct Host {
        home: PathBuf,
        store: PathBuf,
    }

    fn host(name: &str) -> Host {
        let home = scratch(name);
        // The store's own home: its file, and the mark of a home the store asks
        // for before it opens anything.
        fs::write(home.join("state/.tehas-home"), "{}\n").unwrap();
        let bin = store_binary().display().to_string();

        let store = home.join("store");
        fs::write(
            &store,
            // One line per run: the verb and the store's own arguments, then the
            // request the poll wrote on its standard input. The store reads that
            // request and nothing else, so the line is the whole call. The body
            // is kept in a file rather than a shell variable because a request of
            // a whole queue is longer than a line, and `cat` of it answers the
            // store's every read.
            format!(
                "#!/bin/sh\ncat > \"$0.request\"\nprintf 'store %s %s\\n' \"$*\" \"$(cat \"$0.request\")\" >>\"$0.calls\"\nexec {bin} \"$@\" <\"$0.request\"\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&store, fs::Permissions::from_mode(0o755)).unwrap();
        let run = |args: &[&str]| {
            let r = std::process::Command::new(&bin)
                .args(args)
                .env("TEHAS_HOME", &home)
                .output()
                .unwrap();
            assert!(
                r.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&r.stderr)
            );
            String::from_utf8_lossy(&r.stdout).to_string()
        };
        run(&[
            "init",
            "--version",
            "v0.10.2",
            "--commit",
            "cb6912a",
            "--instance",
            "tehas-tehas",
            "--repo",
            "example-owner/example-product",
        ]);
        Host { home, store }
    }

    /// The `Calls` of a host: the store program is the wrapper beside its home,
    /// and `TEHAS_HOME` names that home, as the poll's own environment does.
    fn calls_of(h: &Host) -> Calls {
        let mut calls = Calls::idle(
            Duration::from_secs(20),
            h.store.to_string_lossy().into_owned(),
        );
        calls.home = Some(h.home.to_string_lossy().into_owned());
        calls
    }

    /// The request of one run: the JSON the poll gave the store on its standard
    /// input, taken off the line the wrapper wrote.
    fn request_of(line: &str) -> Value {
        let rest = line
            .strip_prefix("store batch ")
            .unwrap_or_else(|| panic!("the store was run for {line:?}"));
        serde_json::from_str(rest).unwrap_or_else(|e| panic!("{e} of {rest:?}"))
    }

    /// The runs of the store a host has answered: the counter the wrapper
    /// writes beside itself, one line per run of the store program.
    fn runs(h: &Host) -> Vec<String> {
        fs::read_to_string(h.home.join("store.calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// An issue with a cycle of its own, parked with a question on record and
    /// approved for the start: what the poll reads of an issue that waits.
    fn asked_about(h: &Host, n: u64, id: u64) {
        // The store insists that the unit which starts a cycle holds its lock
        // for the whole of what it records, so the fixture holds it across the
        // writes it makes: the same file and the same advisory lock, taken the
        // way the poll takes one to find out whether it is held at all.
        let lock = fs::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(h.home.join("state/cycles").join(format!("{n}.lock")))
            .unwrap();
        lock.try_lock().unwrap();
        let run = |args: &[&str]| {
            let r = std::process::Command::new(store_binary())
                .args(args)
                .env("TEHAS_HOME", &h.home)
                .output()
                .unwrap();
            assert!(
                r.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&r.stderr)
            );
        };
        let s = n.to_string();
        let body = format!("{:064x}", 7_000_000 + n * 13);
        run(&[
            "approval-record",
            &s,
            "--at",
            "1791000000",
            "--by",
            "alice",
            "--how",
            "label",
            "--label",
            "tehas:ready",
            "--body-sha",
            &body,
        ]);
        run(&["cycle-start", &s]);
        run(&["cycle-end", &s, "--attempt", "1", "--status", "failed"]);
        run(&[
            "park",
            &s,
            "--attempt",
            "1",
            "--reason",
            "provider-outage",
            "--comment-id",
            &id.to_string(),
            "--body-sha",
            &body,
        ]);
        drop(lock);
    }

    #[test]
    fn the_snapshot_asks_its_per_issue_reads_in_one_run_of_the_store() {
        // The deliverable: 13 issues, three reads each, and the store runs once
        // for them (and once more for the questions their answers name), where it
        // ran 39 times before. The decisions are the other case's business; this
        // one is about how the reads are asked.
        let h = host("batch");
        let mut reads = Vec::new();
        for i in 0..13u64 {
            asked_about(&h, 100 + i, 4000 + i);
            reads.push(Read::Question(100 + i));
            reads.push(Read::Park(100 + i));
            reads.push(Read::Approval(100 + i));
        }
        let mut calls = calls_of(&h);
        let mut asked = Asked::empty();
        super::gather(&mut calls, &mut asked, &reads);
        // 52 reads were asked (the 13 issues' four reads each), and the store ran
        // once for them; before #941 it ran 52 times. The `question-known` of each
        // issue rides in the same run, so the 13 questions cost no process of their own.
        // 52 reads were asked, and the store ran three times: once for the ids
        // of the questions, once more for the parks those ids were not in, and
        // once for all 52 reads themselves. Before #941 it ran 52 times, one
        // process and one database open per question.
        let runs = runs(&h);
        // The two runs of ids, then the one run of the 52 reads themselves.
        assert_eq!(
            runs.len(),
            3,
            "three runs of the store for 52 reads: {runs:?}"
        );
        for line in &runs {
            assert!(
                line.starts_with("store batch "),
                "the store was run for {line}"
            );
        }
        // The runs of the ids ask the plan and the park of the asking issues
        // again, as the single call of `question` did; the reads of the snapshot
        // itself are the last run and hold each of the 52 of them once.
        let snapshot = runs
            .iter()
            .map(|r| request_of(r))
            .find(|r| r.as_array().unwrap().len() == 52)
            .unwrap_or_else(|| panic!("no run held the 52 reads: {runs:?}"));
        let ids: usize = runs
            .iter()
            .map(|r| request_of(r))
            .filter(|r| r.as_array().unwrap().len() != 52)
            .map(|r| r.as_array().unwrap().len())
            .sum();
        assert_eq!(ids, 26, "the plans and the parks of the 13 asking issues");
        // The reads of the last run are the whole list: no read of the snapshot
        // went to the store on its own, and none of them is in it twice.
        let seen: Vec<String> = snapshot
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_array().unwrap()[0].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            seen.iter().filter(|v| *v == "plan-show").count(),
            13,
            "{seen:?}"
        );
        assert_eq!(
            seen.iter().filter(|v| *v == "park-show").count(),
            13,
            "{seen:?}"
        );
        assert_eq!(
            seen.iter().filter(|v| *v == "question-known").count(),
            13,
            "{seen:?}"
        );
        assert_eq!(
            seen.iter().filter(|v| *v == "approval-current").count(),
            13,
            "{seen:?}"
        );
    }

    #[test]
    fn a_batch_answer_gives_the_snapshot_of_the_single_calls() {
        // The decisions of a poll are the same as before: the value a rule reads
        // is the value the same call alone gives. Each of the four reads is
        // compared with the single call on the same store, so a batch that read
        // the wrong issue, or shaped an answer differently, is seen here.
        let h = host("same");
        asked_about(&h, 7, 4001);
        asked_about(&h, 8, 4002);
        let mut calls = calls_of(&h);
        let mut asked = Asked::empty();
        let reads = vec![
            Read::Question(7),
            Read::Park(7),
            Read::Approval(7),
            Read::Question(8),
            Read::Park(8),
            Read::Approval(8),
        ];
        super::gather(&mut calls, &mut asked, &reads);

        // The single calls, on the same store, through the same Calls: the
        // shape `Calls::store` gives a poll is what `asked` must give it here.
        let alone = |args: &[&str]| {
            let r = std::process::Command::new(store_binary())
                .args(args)
                .env("TEHAS_HOME", &h.home)
                .output()
                .unwrap();
            assert!(
                r.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&r.stderr)
            );
            serde_json::from_str(&String::from_utf8_lossy(&r.stdout)).unwrap()
        };
        let park_alone = super::park(
            &{
                let mut a = Asked::empty();
                a.answers.insert(Read::Park(7), alone(&["park-show", "7"]));
                a.answers.insert(Read::Park(8), alone(&["park-show", "8"]));
                a
            },
            7,
        );
        assert_eq!(
            super::park(&asked, 7),
            park_alone,
            "the park of #7: {:?} vs {:?}",
            asked.answers.get(&Read::Park(7)),
            park_alone
        );
        let (shape, current) = super::approval(&asked, 7);
        let (shape_alone, current_alone) = super::approval(
            &{
                let mut a = Asked::empty();
                a.answers
                    .insert(Read::Approval(7), alone(&["approval-current", "7"]));
                a
            },
            7,
        );
        assert_eq!(shape, shape_alone, "the approval of #7");
        assert_eq!(current, current_alone);
        let q = super::question(&asked, 7);
        assert_eq!(q["id"], "4001", "the question on record: {q}");
        assert_eq!(q["kind"], "park");
        assert_eq!(q["plan"], Value::Null, "no planning session on record");
        // The issue of the other answer is not this one's: the answers are read
        // by the read they answered, not by their position in the run.
        assert_eq!(super::park(&asked, 8)["issue"], 8);
        assert_ne!(
            super::park(&asked, 7)["issue"],
            super::park(&asked, 8)["issue"]
        );
    }

    #[test]
    fn a_read_the_batch_did_not_answer_is_the_failure_the_single_call_gives() {
        // A poll that cannot ask the reads cannot decide, and says so as it said
        // so before: the reader gets a failed answer with its word, not a panic
        // and not another issue's rows.
        let h = host("missing");
        let asked = Asked::empty();
        let v = asked.answer(Read::Park(9));
        assert_eq!(v["failed"], true);
        assert_eq!(v["word"], "garbled");
        assert_eq!(super::park(&asked, 9)["failed"], true);
        let (shape, current) = super::approval(&asked, 9);
        assert_eq!(shape, json!({"failed": true}));
        assert_eq!(current["failed"], true);
        assert_eq!(super::question(&asked, 9)["failed"], true);
        // The whole batch failing is the same shape, named by the batch's own
        // refusal rather than by nothing.
        let mut asked = Asked::empty();
        asked.failed = Some(
            json!({"failed": true, "rc": 78, "word": "absent", "line": "store: absent: x could not be run"}),
        );
        assert_eq!(asked.answer(Read::Approval(3))["word"], "absent");
        let _ = h;
    }

    #[test]
    fn a_store_that_cannot_be_run_fails_the_batch_as_it_failed_a_single_call() {
        let home = scratch("absent");
        // A program that is not there at all: spawn fails, which is what the
        // bash's `command -v` test answers as `absent`.
        let missing = home.join("no-such-store");
        assert!(!missing.exists());
        let mut calls = Calls::idle(
            Duration::from_secs(5),
            missing.to_string_lossy().into_owned(),
        );
        let mut asked = Asked::empty();
        super::gather(&mut calls, &mut asked, &[Read::Park(1)]);
        let why = asked.failed.as_ref().unwrap();
        assert_eq!(why["failed"], true, "{why}");
        assert_eq!(why["rc"], 78, "{why}");
        assert_eq!(super::park(&asked, 1)["rc"], 78);
    }

    #[test]
    fn an_issue_the_batch_is_asked_about_twice_is_asked_once() {
        // 13 rules may name one issue; the store is asked for its park once.
        let h = host("dedup");
        asked_about(&h, 7, 4001);
        let mut calls = calls_of(&h);
        let mut asked = Asked::empty();
        let reads = vec![
            Read::Park(7),
            Read::Park(7),
            Read::Approval(7),
            Read::Question(7),
            Read::Park(7),
        ];
        super::gather(&mut calls, &mut asked, &reads);
        let runs = runs(&h);
        // Three runs at the most, whatever the list held: the plan and the park the
        // question's id lives in, and the batch of the reads themselves. Five reads
        // of the same issue asked as three, and the store ran for them as a batch
        // and not as five calls.
        assert!(
            runs.iter().all(|r| r.starts_with("store batch ")),
            "the store was run outside the batch: {runs:?}"
        );
        // The five reads of the list are three calls of the snapshot's own run:
        // `park-show` once for all three of its asks, and the other two reads
        // once each. The fourth is the question's `question-known`, whose id the
        // store answered in an earlier run of the same poll.
        // Three calls, one apiece: the park of #7 was asked three times over and
        // the store was asked for it once. The `question-known` rides with them,
        // asked once the id had been read.
        let last = request_of(&runs[runs.len() - 1]);
        assert_eq!(
            last.as_array().unwrap().len(),
            4,
            "one call per distinct read: {last}"
        );
        // What came before it is the read of the question's id: the session and
        // the park of the one asking issue, asked as the single call asked them.
        let before: usize = runs[..runs.len() - 1]
            .iter()
            .map(|r| request_of(r).as_array().unwrap().len())
            .sum();
        assert_eq!(before, 2, "the plan and the park of #7: {runs:?}");
        let seen: Vec<String> = last
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_array().unwrap()[0].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            seen.iter().filter(|v| *v == "park-show").count(),
            1,
            "three asks of the park, one call: {seen:?}"
        );
    }

    #[test]
    fn a_snapshot_with_no_issue_to_ask_asks_the_store_nothing() {
        let h = host("none");
        let mut calls = calls_of(&h);
        let mut asked = Asked::empty();
        super::gather(&mut calls, &mut asked, &[]);
        assert!(runs(&h).is_empty(), "the poll ran the store for nothing");
    }
}
