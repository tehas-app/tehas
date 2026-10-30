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
use std::collections::{BTreeMap, BTreeSet};

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

    /// The verb of the read's own call, the one `args` begins with. The rules
    /// name a failure by it and the batch names its answers by it, so the
    /// mapping lives beside the arguments it is read from.
    fn name(&self) -> &'static str {
        match self {
            Read::Question(_) => "plan-show",
            Read::Park(_) => "park-show",
            Read::Approval(_) => "approval-current",
        }
    }
}

/// The digits of a comment id, from the number or the text the store answered,
/// or nothing when they are not an id the store would know: the same rule as
/// `verbs::comment_id` on the store's side, which is the bound the store
/// refuses a `question-known` by. One function holds the whole rule, so the
/// poll and the store agree on which ids are legal.
fn digits(v: Option<&Value>) -> Option<String> {
    let s = match v {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => return None,
    };
    let below_float = s.parse::<u64>().is_ok_and(|n| n < (1u64 << 53));
    (!s.is_empty()
        && s.len() <= 16
        && !s.starts_with('0')
        && s.bytes().all(|b| b.is_ascii_digit())
        && below_float)
        .then_some(s)
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
            None => self.failure(read.name(), read.issue()),
        }
    }

    /// The failure to give for a read of the batch that was never answered: the
    /// batch's own, named for the read the poll was left without. One run stood
    /// for all of the reads, so a store that refused it refused this one too, and
    /// the rules of the poll read the word of that refusal -- `busy`, `corrupt`,
    /// `absent` -- exactly as they read it when the call stood alone.
    ///
    /// A batch that failed with nothing to name itself by is not read as one of
    /// the store's own refusals: it is `unasked`, a word no verb of the store
    /// gives, so a break of the batch's contract cannot be mistaken for a refusal
    /// of the state store, and cannot be mistaken for the per-issue skip a rc of
    /// 1 means. The rc is the 78 of a store that cannot be used, which is the
    /// fail-closed reading of a poll that has no answers at all.
    fn failure(&self, verb: &str, n: u64) -> Value {
        match self.failed.as_ref() {
            Some(e) => json!({
                "failed": true,
                "rc": e.get("rc").cloned().unwrap_or(json!(78)),
                "word": e.get("word").cloned().unwrap_or(json!("unasked")),
                "line": format!("store: {}: {verb} {n} of the batch: {}",
                    e.get("word").and_then(Value::as_str).unwrap_or("unasked"),
                    e.get("line").and_then(Value::as_str).unwrap_or("")),
            }),
            None => json!({"failed": true, "rc": 78, "word": "unasked",
                "line": format!("store: unasked: {verb} {n} answered nothing")}),
        }
    }

    fn know(&self, n: u64) -> Value {
        self.known
            .get(&n)
            .cloned()
            .unwrap_or_else(|| self.failure("question-known", n))
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

/// The most calls of one run of the store, the ceiling `store batch` itself
/// answers above with 64 (src/batch.rs). The list of a snapshot is built from
/// the whole queue, so a host with a queue four times this long would lose every
/// read of the poll to one refusal; asking in runs of this size costs a busy
/// host more processes and no answers.
const MAX_CALLS: usize = 512;

/// The snapshot's per-issue reads of the store, asked in runs of the program
/// and one opening of the database apiece: `plan-show`, `park-show` and
/// `approval-current` of every issue named, and the `question-known` of the
/// issues whose session or park carries a question. The id of that question is
/// read first, from the same store, because the batch cannot ask a call whose
/// arguments it has not read yet. Every read the poll needs after this one -- an
/// issue a rule names once the batch has run -- is asked by another run, which is
/// what a poll of a queue that grew paid for anyway.
pub fn gather(calls: &mut Calls, asked: &mut Asked, reads: &[Read]) {
    let mut want: Vec<Read> = reads.to_vec();
    want.sort_unstable();
    want.dedup();
    if want.is_empty() {
        return;
    }
    // The ids of the questions come first, in runs of their own: the batch
    // cannot ask a call whose arguments it has not read, so the two columns the
    // id lives in are read before the batch is built, one list of plans and one
    // of parks, each asked in a single run.
    let ids = match question_ids(calls, &want) {
        Ok(ids) => ids,
        Err(e) => {
            asked.failed = Some(e);
            return;
        }
    };
    // The calls of the run, and which read or question each one answers, so the
    // answers go back by position and not by name.
    let mut calls_of: Vec<Vec<String>> = Vec::new();
    let mut owners: Vec<Owner> = Vec::new();
    for read in &want {
        calls_of.push(read.args());
        owners.push(Owner::Read(*read));
    }
    for (n, qid) in &ids {
        calls_of.push(known_call(*n, qid));
        owners.push(Owner::Known(*n));
    }
    // One call apiece: the reads of the list and the `question-known` of an
    // issue can name the same thing, and the store is asked for it once. Each
    // call keeps the reads it answers, so one answer serves them all. The keys
    // are ordered, so a `BTreeMap` of them dedups without a scan of the list.
    let mut slots: BTreeMap<Vec<String>, Vec<Owner>> = BTreeMap::new();
    for (call, owner) in calls_of.into_iter().zip(owners) {
        slots.entry(call).or_default().push(owner);
    }
    let asked_calls: Vec<Vec<String>> = slots.keys().cloned().collect();
    // A list longer than one batch is asked in more than one: the store refuses
    // a request of more than `MAX_CALLS` calls whole, and every read of the
    // poll would come back as the failure of that one refusal.
    for chunk in asked_calls.chunks(MAX_CALLS) {
        match calls.store_batch(chunk) {
            Ok(answers) => record(asked, chunk, &answers, &slots),
            Err(e) => {
                asked.failed = Some(e);
                return;
            }
        }
    }
}

/// Which read or question one answer of the run belongs to.
enum Owner {
    Read(Read),
    Known(u64),
}

/// Puts the answers of one run back where they belong: the answer of a call
/// serves every read that call answers.
fn record(
    asked: &mut Asked,
    chunk: &[Vec<String>],
    answers: &[Value],
    slots: &BTreeMap<Vec<String>, Vec<Owner>>,
) {
    for (call, answer) in chunk.iter().zip(answers) {
        let Some(owners) = slots.get(call) else {
            continue;
        };
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

/// The issues of the batch that carry a question, with the comment whose
/// question the store confirms: the open session's own, or the park's, in the
/// order the single call of `question` tried them. Asked as one run of the store
/// apiece -- two calls per issue at the most, and one process for each list --
/// because the arguments of the confirming call are not known until the answer
/// of these is in hand.
///
/// A read the store refused is not a question with none on record: the failure
/// of either list is given back rather than folded into an empty id, because the
/// rules that gate a tick count a tick only on the comment the host has on
/// record, and a `busy` answer must not read as "this issue asks nothing".
fn question_ids(calls: &mut Calls, want: &[Read]) -> Result<Vec<(u64, String)>, Value> {
    // The two lists are built from the reads the snapshot asked, as pairs of the
    // issue and its call: a read of an issue the snapshot never asked is never
    // asked here either, the answer goes back to the issue by the pair it was
    // built from, and a list with nothing in it asks nothing of the store.
    let plans: Vec<(u64, Vec<String>)> = want
        .iter()
        .filter(|r| matches!(r, Read::Question(_)))
        .map(|r| (r.issue(), r.args()))
        .collect();
    let parks: Vec<(u64, Vec<String>)> = want
        .iter()
        .filter(|r| matches!(r, Read::Park(_)))
        .map(|r| (r.issue(), r.args()))
        .collect();
    if plans.is_empty() && parks.is_empty() {
        return Ok(Vec::new());
    }
    let mut asked_of = |list: &[(u64, Vec<String>)]| -> Result<BTreeMap<u64, Value>, Value> {
        // One run of the store per chunk of the list, so a long list is asked in
        // more than one rather than refused whole; each answer belongs to the
        // pair its own call was built from.
        let mut got: BTreeMap<u64, Value> = BTreeMap::new();
        for chunk in list.chunks(MAX_CALLS) {
            let calls_of: Vec<Vec<String>> = chunk.iter().map(|(_, c)| c.clone()).collect();
            let answers = calls.store_batch(&calls_of)?;
            for ((n, _), answer) in chunk.iter().zip(answers) {
                got.insert(*n, answer);
            }
        }
        Ok(got)
    };
    let plans = asked_of(&plans)?;
    let parks = asked_of(&parks)?;
    let failed = |v: &Value| v.get("failed") == Some(&json!(true));
    let mut ids = Vec::new();
    for n in plans
        .keys()
        .chain(parks.keys())
        .copied()
        .collect::<BTreeSet<u64>>()
    {
        if let Some(v) = plans.get(&n) {
            if failed(v) {
                return Err(v.clone());
            }
            if let Some(id) = digits(v.pointer("/planning/comment_id")) {
                ids.push((n, id));
                continue;
            }
        }
        if let Some(v) = parks.get(&n) {
            if failed(v) {
                return Err(v.clone());
            }
            if let Some(id) = digits(v.pointer("/park/question")) {
                ids.push((n, id));
            }
        }
    }
    Ok(ids)
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
    if let Some(d) = input.get("driver").filter(|d| !d.is_null()) {
        out["driver"] = d.clone();
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
        // #1031: whether the one easy merge this approval is owed has been had.
        // Absent, as on an answer of an older store: no easy merge has been.
        if v.get("easy") == Some(&json!(true)) {
            out["easy"] = json!(true);
        }
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

/// How many of `running` a fed start began: the millwright's start resting on
/// no person's approval. `{"failed": true}` when one cannot be read.
pub fn fed(asked: &Asked, running: &[u64]) -> Value {
    let mut count = 0;
    for n in running {
        let v = asked.answer(Read::Approval(*n));
        if v.get("failed") == Some(&json!(true)) {
            return json!({"failed": true});
        }
        let how = v
            .pointer("/current/how")
            .and_then(Value::as_str)
            .unwrap_or("");
        if tehas_core::grammar::MILLWRIGHT_WORDS.contains(&how)
            && v.get("rests_on").is_none_or(Value::is_null)
        {
            count += 1;
        }
    }
    json!(count)
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
                        .map(|r| json!({"how": get(r, "how"), "parked": get(r, "parked"), "started": get(r, "started_at"), "easy": get(r, "easy")}))
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
    let mut qid = digits(plan.get("comment_id")).unwrap_or_default();
    if qid.is_empty() {
        let park = asked.answer(Read::Park(n));
        if park.get("failed") == Some(&json!(true)) {
            return park;
        }
        qid = digits(park.pointer("/park/question")).unwrap_or_default();
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
mod approval_shape_tests {
    use serde_json::{json, Value};

    fn shaped(how: &str, input: Value) -> Value {
        let base = json!({"at": 1, "by": "alice", "how": "label", "body_sha256": "a"});
        super::approval_shape(&json!({"rests_on": base, "current": {"at": 2, "by": "bot",
            "how": how, "body_sha256": "a", "input": input}}))
        .unwrap()
    }

    /// Every shape of a start names the driver of its own row, and a record
    /// with none, as every record before the driver is, names none.
    #[test]
    fn the_driver_of_a_start_is_read_off_its_own_record() {
        for how in [
            "label",
            "retry-tick",
            "outage-retry",
            "millwright",
            "refresh",
        ] {
            assert_eq!(shaped(how, json!({"driver": "lean"}))["driver"], "lean");
            assert_eq!(shaped(how, json!({})).get("driver"), None);
            assert_eq!(shaped(how, Value::Null).get("driver"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::run::Calls;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Once;
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

    /// The store program of this build. cargo builds a package's own binary
    /// beside its harness and names it in `CARGO_BIN_EXE_store`; this harness
    /// is a library's, so no build of this package makes the store and the
    /// variable is absent. The directories are tried first because they hold
    /// the store of the profile under test, `target/<profile>/deps` and
    /// `target/<profile>`.
    ///
    /// A run that built nothing yet is not a failure: `t_store` (test/lib.sh)
    /// builds the store into a target directory of its own rather than
    /// requiring it, and this does the same. Building is what keeps these cases
    /// honest about the program a host runs, so an absent store is built, not
    /// skipped.
    fn store_binary() -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        let deps = exe.parent().unwrap();
        for cand in [deps.to_path_buf(), deps.parent().unwrap().to_path_buf()] {
            let cand = cand.join("store");
            if cand.is_file() {
                return cand;
            }
        }
        build_store();
        for cand in [deps.to_path_buf(), deps.parent().unwrap().to_path_buf()] {
            let cand = cand.join("store");
            if cand.is_file() {
                return cand;
            }
        }
        panic!("the store of this build is beside the test binary");
    }

    /// One build of the store for the whole harness, into the directory the
    /// harness's own profile uses. `Once` because the cases run in parallel and
    /// cargo's lock makes a second build wait for the first anyway; the build's
    /// own words are kept for the panic.
    fn build_store() {
        static BUILT: Once = Once::new();
        let mut outcome: Option<Result<(), String>> = None;
        BUILT.call_once(|| {
            let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
            let profile = if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            };
            // cargo names the directory when it was asked for one of its own;
            // the workspace's default is one above this package's manifest.
            let target = std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"));
            let built = std::process::Command::new("cargo")
                .arg("build")
                .args(["--locked", "--offline", "--quiet", "--manifest-path"])
                .arg(&manifest)
                .args(["--target-dir"])
                .arg(&target)
                .args(["-p", "store"])
                .arg(format!("--{profile}"))
                .output();
            outcome = Some(match built {
                Ok(out) if out.status.success() => Ok(()),
                Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
                Err(e) => Err(format!("cargo: {e}")),
            });
        });
        if let Some(Err(words)) = outcome.as_ref() {
            panic!("the store of this build could not be built: {words}");
        }
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
            wrapper(&bin, None),
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

    /// The store of a case: the one shell script that records a call and hands
    /// the request to the real store. `test/dispatch_lib.sh`'s `store_refuses`
    /// writes the same script for the shell's cases; this is the one the Rust
    /// cases use, so the batch's request is handled in one place and one shell
    /// dialect. With `busy_for` the wrapper answers that call's JSON text busy
    /// on its error stream and runs nothing, which is how a case makes the store
    /// refuse one read of a batch.
    fn wrapper(bin: &str, busy_for: Option<&str>) -> String {
        // One line per run: the verb and the store's own arguments, then the
        // request the poll wrote on its standard input. The store reads that
        // request and nothing else, so the line is the whole call. The body is
        // kept in a file rather than a shell variable because a request of a
        // whole queue is longer than a line, and `cat` of it answers the store's
        // every read.
        let refuse = match busy_for {
            Some(needle) => format!(
                "if grep -qF {needle:?} \"$0.request\"; then\n  \
echo 'store: busy #0: the database is locked' >&2\n  exit 1\nfi\n"
            ),
            None => String::new(),
        };
        format!(
            "#!/bin/sh\ncat > \"$0.request\"\n{refuse}\
printf 'store %s %s\\n' \"$*\" \"$(cat \"$0.request\")\" >>\"$0.calls\"\nexec {bin} \"$@\" <\"$0.request\"\n"
        )
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
        // The deliverable: 52 reads of 13 issues, asked in one run of the store
        // where the poll made 52 before #941. The decisions are the other case's
        // business; this one is about how the reads are asked.
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
        // Three runs of the store for the 52 reads: one for the plans of the
        // asking issues, one for their parks -- the two lists the id of a
        // question lives in, which the batch cannot ask until they are read --
        // and one for all 52 reads themselves. Before #941 it ran 52 times, one
        // process and one database open per question.
        let runs = runs(&h);
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
    fn fed_counts_the_millwrights_starts_on_no_persons_approval() {
        let mut a = Asked::empty();
        let fed = |how: &str, base: Value| json!({"current": {"how": how}, "rests_on": base});
        a.answers
            .insert(Read::Approval(7), fed("millwright", Value::Null));
        a.answers
            .insert(Read::Approval(8), fed("fixer", Value::Null));
        a.answers
            .insert(Read::Approval(9), fed("millwright", json!({"by": "alice"})));
        a.answers
            .insert(Read::Approval(10), fed("label", Value::Null));
        assert_eq!(super::fed(&a, &[7, 8, 9, 10]), json!(2));
        assert_eq!(super::fed(&a, &[]), json!(0));
        // One that cannot be read leaves the count unknown, not low.
        assert_eq!(super::fed(&a, &[7, 11])["failed"], true);
    }

    #[test]
    fn a_read_the_batch_did_not_answer_is_named_as_no_refusal_of_the_store() {
        // A poll that cannot ask the reads cannot decide, and says so. A read of
        // a batch that was never answered is not one of the store's own
        // refusals: it carries `unasked`, a word no verb gives, at the 78 of a
        // store that cannot be used, so a break of the batch's contract is never
        // read as a refusal of a row (or as the per-issue skip of a rc of 1).
        let h = host("missing");
        let asked = Asked::empty();
        let v = asked.answer(Read::Park(9));
        assert_eq!(v["failed"], true);
        assert_eq!(v["word"], "unasked");
        assert_eq!(v["rc"], 78);
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
        // A batch that failed without a word of its own is still `unasked`, and
        // still the 78 that ends the poll rather than the 1 that skips an issue.
        let mut asked = Asked::empty();
        asked.failed = Some(json!({"failed": true, "line": "store: "}));
        assert_eq!(asked.answer(Read::Approval(3))["word"], "unasked");
        assert_eq!(asked.answer(Read::Approval(3))["rc"], 78);
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
        // store answered in one of the runs before it. Three calls, one apiece:
        // the park of #7 was asked three times over and the store was asked for
        // it once.
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
    fn a_busy_id_read_is_the_failure_of_the_question_and_not_no_question() {
        // The gate of a tick counts a tick only on the comment the host has on
        // record, so a store that could not say is never read as an issue that
        // asks nothing: the failure of either list of ids is given back, at the
        // store's own exit, and the rules see `busy` rather than `id: ""`.
        let h = host("ids-busy");
        asked_about(&h, 7, 4001);
        // The same wrapper, with the one call it refuses: the plan of #7, which
        // is the read the id of a question lives in. The parks and the reads of
        // the snapshot still go to the real store.
        let body = wrapper(
            &store_binary().display().to_string(),
            Some("[\"plan-show\",\"7\"]"),
        );
        std::fs::write(&h.store, body).unwrap();
        std::fs::set_permissions(&h.store, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut calls = calls_of(&h);
        let mut asked = Asked::empty();
        super::gather(
            &mut calls,
            &mut asked,
            &[Read::Question(7), Read::Park(7), Read::Approval(7)],
        );
        let q = super::question(&asked, 7);
        assert_eq!(q["failed"], true, "a busy plan read is a question: {q}");
        assert_eq!(q["word"], "busy", "{q}");
        assert_eq!(q["rc"], 1, "{q}");
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
