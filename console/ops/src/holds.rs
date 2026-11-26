//! The holds one instance keeps under `state/`: the limit `tehas limit` sets
//! and the pause `tehas pause` sets. Reading, validating, writing and printing
//! them, for `tehas limit`, `tehas pause`, `tehas resume` and `tehas drain`, so
//! that every command which honours a hold reads it through one Rust function.
//! The subcommand keeps its old name, `console flags`.
//!
//!   console flags limit show [--json]
//!   console flags limit set --cycles N [--plan on|off]
//!       [--for SECONDS | --until TIME] [--reason TEXT]
//!   console flags limit clear
//!   console flags limit read [--now <epoch seconds>] [--line]
//!
//!   console flags pause set [--reason TEXT]
//!   console flags pause clear
//!
//! The home is `TEHAS_HOME`, or `$HOME/tehas` with no `TEHAS_HOME`, as
//! `lib/config.sh` resolves it. The file is `$TEHAS_HOME/state/limit.json`,
//! mode 0600, holding `{v: 1, cycles, plan, until, reason}` written whole:
//!
//! - `cycles`: the most cycles running at once, a whole number 0 to 9999
//! - `plan`: false holds a new planning turn
//! - `until`: when it expires, `2026-10-17T14:05:00Z`, or null while it lasts
//!   until cleared
//! - `reason`: one line of at most 200 characters, shown as the hold's reason
//! - `set`: when the limit was written, `2026-10-17T14:05:00Z`. A file
//!   written before tehas kept the time has none
//!
//! `show` prints the limit as it then is, in the words `tehas status` prints,
//! or with `--json` the object `tehas limit --json` prints:
//!
//!   limit     1 cycle at once, planning off, until 2026-10-17T14:05:00Z: held for alpha
//!   {"api":1,"state":"ok","cycles":1,"plan":false,"until":"...","reason":"...","set":"..."}
//!
//! `tehas drain --json` prints the limit with its `set` too, and so does
//! `set --json`, in place of its words.
//! `read` prints the same reading as one object with no `api` and no `set`: the object
//! `tehas status --json` publishes, so it keeps
//! exactly the fields it has today; with `--line` it prints the words of that
//! reading instead, which is how the shell prints the line of a limit it has
//! just read. `state` is `none` when no file is there,
//! `ok`, or `expired` once `until` has passed, and `unreadable` when what is
//! there is not a limit of version 1. A field with no value is left out, never
//! null. A limit that has expired or cannot be read holds every start, so a
//! reader never takes `unreadable` for no limit: `read` prints it and exits 0.
//!
//! `set` replaces the whole limit and `clear` takes it off; each prints the
//! limit as it then is. Setting the same limit twice leaves the same file, so
//! a program which sets it on every poll changes nothing. The same limit is
//! the same `cycles`, `plan` and `reason`: it keeps its `set`, also when
//! `until` moves, and any other limit takes the moment it is written at, or
//! the second after the `set` there when that is no earlier
//! (`tehas_core::limit`). A limit is written
//! through `limit.json.new`, mode 0600, and put in place by a rename: what is
//! there is always a whole limit, never the half of one.
//!
//! The clock is the host's, or the whole number of seconds in
//! `TEHAS_FLAGS_TEST_NOW` when that names one, which is how a shell test reads
//! a limit at a moment of its own.
//!
//! The pause flag is `$TEHAS_HOME/state/pause.json`, mode 0600, holding
//! `{by, at, reason}` written whole - there is no version field, and the shape
//! has been that since the flag was first written:
//!
//! - `by`: the account which wrote the flag, any JSON value; anything but a
//!   string reads as nobody, and nothing which reaches a page publishes it
//! - `at`: when it was written, `2026-10-17T14:05:00Z`, which must be a string
//! - `reason`: one line of at most 200 characters, which must be a string
//!
//! The reading of the flag, its fields and its words, is `read_pause`, and
//! every command which writes the flag or publishes its state takes its answer
//! from there: `tehas pause` itself, the health file, and the dashboard's pause
//! line. Only the poll's own decision reads the file by its own rules, so it
//! asks the same questions of it. `state` is `none` when no file is there and
//! `ok` when one holds a flag, and `unreadable` when something is there which
//! is no flag - a file no reader can make sense of, which every reader counts
//! as paused. A command which is told the file is no flag says so in the words
//! its readers have always used for one.
//!
//! `set` writes the flag with the account of this run and the moment of the
//! write, and prints `paused at <time> by <account>[: reason]`; on a host
//! already paused it prints `already paused …` and leaves the flag alone.
//! `clear` prints that the flag is gone, or that this host had none. A file
//! which is no flag is named on standard error before `set` replaces it.
//! A flag is written through
//! `pause.json.new`, mode 0600, and put in place by a rename, so what is there
//! is always a whole flag; `clear` takes that name away too, and takes away
//! either name only when it holds a regular file of this account, so it never
//! follows a link someone left under one of them.
//!
//! Exit: 0 for a verb of either flag which did its work; 1 a file could not be
//! written or removed, or a name of the flag is not this account's to remove;
//! 64 a bad argument, which writes nothing.

use crate::account;
use serde_json::Value;
use std::env;
use std::fs::{self, OpenOptions, TryLockError};
use std::num::NonZeroU64;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::events::{pause_holds_marks, UNKNOWN_ACTOR};
use tehas_core::file::Temp;
use tehas_core::home::{Home, Live, State};
use tehas_core::limit::{self as limit_file, leading_number};
pub use tehas_core::limit::{epoch, oneline, REASON};
use tehas_core::lock::{self, Probe};
use tehas_core::time;
use tehas_ports::events::Dispatcher;
use tehas_ports::{Act, HappenedKind, Mark, TicketKey, Tickets, Window, ALL_MAX};

/// The clock a run reads: the host's, or the whole number of seconds in
/// `TEHAS_FLAGS_TEST_NOW` when that names one, which is how a shell test reads
/// a limit, or sets one, at a moment of its own. A value which is no whole
/// number stops here rather than being read as now, as the store does with its
/// own.
fn now() -> i64 {
    clock(env::var("TEHAS_FLAGS_TEST_NOW").ok())
}

// Apart from `now` so a test reads it without setting the variable that every
// other test of the process reads too.
fn clock(given: Option<String>) -> i64 {
    match given {
        Some(s) => s.parse().expect("TEHAS_FLAGS_TEST_NOW is not a Unix time"),
        None => tehas_core::time::now(),
    }
}

/// The largest limit a person can set, and the widest `--for` in seconds: the
/// bounds `tehas limit` has always checked its arguments against.
const MAX_CYCLES: u64 = 9999;
const MAX_FOR: u64 = 999_999_999;

/// The limit as every command which honours it reads it: the reading of
/// `tehas_core::limit`, which the dispatcher and the health file take too.
pub fn read(file: &Path, now: i64) -> Reading {
    let limit_file::Limit { state, fields, set } = limit_file::read(file, now);
    Reading {
        fields,
        set,
        ..Reading::of(state)
    }
}

/// Whether a number of the file means one cycle, which is the only number
/// whose word is `cycle`: jq's `== 1` asks the value, so `1.0` is one too.
fn is_one(text: &str) -> bool {
    leading_number(text).is_some_and(|body| body.parse::<f64>() == Ok(1.0))
}

/// The reading of a flag: what the file said, the words which print it, and
/// the heading those words stand under - `limit` or `pause`.
#[derive(Debug, PartialEq, Clone)]
pub struct Reading {
    state: String,
    /// The fields in the order they print, with the number already printed as
    /// jq prints it. `state` is not in here: it prints first, always.
    fields: Vec<(String, String)>,
    /// When the limit was written. `tehas limit --json` and `tehas drain
    /// --json` print it; `read`, the health file and `tehas status` do not.
    set: Option<String>,
    /// The words of the reading, whole: the line of the flag as its command
    /// and its readers print it, and the name of the flag in front of them.
    line: String,
    /// The name of the flag: the heading of the line, and the word the words
    /// of a flag which is not one are about.
    flag: &'static str,
}
impl Reading {
    fn of(state: &str) -> Reading {
        Reading {
            state: state.to_string(),
            fields: Vec::new(),
            set: None,
            line: String::new(),
            flag: "limit",
        }
    }

    /// A reading of the pause flag: the same four states as the limit's, under
    /// the name of this flag.
    fn of_pause(state: &str) -> Reading {
        Reading {
            flag: "pause",
            ..Reading::of(state)
        }
    }

    pub fn said(&self) -> String {
        if !self.line.is_empty() {
            return self.line.clone();
        }
        if self.flag == "pause" {
            // A pause reading with no line of its own is a host with no flag, or
            // a file which is no flag. The one word of the second is the words
            // the dispatcher has always logged for it; the command which knows
            // the name of the file says more, with not_a_flag.
            return match self.state.as_str() {
                "unreadable" => "pause.json is not a pause flag: taking it as paused".to_string(),
                _ => format!("{}     none", self.flag),
            };
        }
        let field = |k: &str| self.field(k);
        let words = match self.state.as_str() {
            "none" => "none".to_string(),
            "unreadable" => {
                "unreadable: nothing starts until it is set again or cleared".to_string()
            }
            "expired" => format!(
                "expired at {}: nothing starts until it is set again or cleared",
                field("until")
            ),
            _ => format!(
                "{} cycle{} at once, planning {}, {}",
                field("cycles"),
                if is_one(&field("cycles")) { "" } else { "s" },
                if field("plan") == "true" { "on" } else { "off" },
                match self.fields.iter().find(|(name, _)| name == "until") {
                    Some((_, u)) => format!("until {}", u.trim_matches('"')),
                    None => "until cleared".to_string(),
                }
            ),
        };
        // Only a limit which is in force says its reason: the two words of an
        // expired or unreadable one already say all the health file has.
        let reason = match self.state.as_str() {
            "ok" => field("reason"),
            _ => String::new(),
        };
        if reason.is_empty() {
            return format!("limit     {words}");
        }
        format!("limit     {words}: {reason}")
    }

    /// The reading as one compact object, `state` first: the order
    /// `tehas status --json` republishes.
    fn text(&self) -> String {
        let mut parts = vec![format!("\"state\":\"{}\"", self.state)];
        parts.extend(
            self.fields
                .iter()
                .map(|(name, value)| format!("\"{name}\":{value}")),
        );
        format!("{{{}}}", parts.join(","))
    }

    /// The reading with `set` last: the limit as `tehas limit --json` and
    /// `tehas drain --json` print it.
    fn with_set(&self) -> String {
        let text = self.text();
        match &self.set {
            Some(set) => format!(
                "{},\"set\":{}}}",
                &text[..text.len() - 1],
                Value::String(set.clone())
            ),
            None => text,
        }
    }
}

impl Reading {
    /// none, ok, expired or unreadable.
    pub fn state(&self) -> &str {
        &self.state
    }

    /// The value of one field as JSON text, when the reading has it.
    pub fn json_of(&self, want: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(name, _)| name == want)
            .map(|(_, value)| value.as_str())
    }

    /// The reading as `flags limit read` prints it.
    pub fn json(&self) -> String {
        self.text()
    }

    /// The printed value of one field, with a quoted string unquoted: what a
    /// reader of the object would take from it.
    fn field(&self, want: &str) -> String {
        let held = self
            .fields
            .iter()
            .find(|(name, _)| name == want)
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        held.trim_matches('"').to_string()
    }
}

pub fn main(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("limit") => limit(&given[1..]),
        Some("pause") => pause(&given[1..]),
        Some(_) => usage("what is wanted of the flags"),
        None => usage("a flag to work on: limit or pause"),
    }
}

/// The second line of the usage: what the pause flag takes. Pinned, because it
/// is what an operator on a stale host, or with a script from v0.16.2, is
/// shown before the 64.
const PAUSE_USAGE: &str = "       console flags pause set [--reason TEXT] | clear";

fn usage(why: &str) -> u8 {
    eprintln!("console flags: {why}");
    eprintln!("usage: console flags limit show [--json] | set --cycles N [--plan on|off] [--for SECONDS | --until TIME] [--reason TEXT] [--json] | clear | read [--now <epoch seconds>]");
    eprintln!("{PAUSE_USAGE}");
    64
}

/// The instance's home, as `lib/config.sh` resolves it.
fn home() -> Option<PathBuf> {
    let from = |name: &str| match env::var_os(name) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    };
    from("TEHAS_HOME").or_else(|| from("HOME"))
}

fn limit(given: &[String]) -> u8 {
    let here = match home() {
        Some(h) => h,
        None => {
            eprintln!("console flags: neither TEHAS_HOME nor HOME is set");
            return 64;
        }
    };
    let (verb, rest) = match given.split_first() {
        Some((verb, rest)) => (verb.as_str(), rest),
        None => return usage("show, set, clear or read is wanted"),
    };
    let file = Home::new(&here).state(State::Limit);
    match verb {
        // --json prints the object tehas limit --json promises, api first;
        // read prints the same reading without it, which is what tehas status
        // --json publishes.
        // show prints the words tehas limit prints, or the object with api
        // first when asked for --json.
        "show" => {
            let mut words = false;
            for arg in rest {
                if arg != JSON || words {
                    return usage("--json, once, after show");
                }
                words = true;
            }
            let out = read(&file, now());
            tehas_core::outln!("{}", if words { api(out) } else { out.said() });
            0
        }
        // read prints the object `tehas limit --json` prints, with no api: it
        // takes the moment to read at, which is how a shell test reads a limit
        // at a time of its own, and prints the line instead with --line, so
        // that the words of a reading are printed where the reading came from.
        "read" => {
            let mut at: Option<String> = None;
            let mut line = false;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--line" if !line => line = true,
                    "--now" if at.is_none() => {
                        let Some(v) = rest.get(i + 1) else {
                            return usage("--now wants epoch seconds");
                        };
                        at = Some(v.clone());
                        i += 1;
                    }
                    _ => return usage("an argument this command does not know"),
                }
                i += 1;
            }
            let when = match &at {
                Some(v) => match v.parse::<i64>() {
                    Ok(v) => v,
                    Err(_) => return usage("--now is not a whole number of seconds"),
                },
                None => now(),
            };
            let out = read(&file, when);
            tehas_core::outln!("{}", if line { out.said() } else { out.text() });
            0
        }
        // set and clear print the limit as it then is, in words; set with
        // --json prints the object `tehas limit --json` prints.
        "set" => match set_args(rest) {
            Ok(want) => set(&file, &want),
            Err(bad) => usage(bad),
        },
        "clear" => {
            if !rest.is_empty() {
                return usage("nothing after clear");
            }
            clear(&file)
        }
        _ => usage("show, set, clear or read, not a word this command knows"),
    }
}

/// The pause flag: the words of `tehas pause`, the reason of the flag it wrote
/// and the name of the account which wrote it are decided here.
fn pause(given: &[String]) -> u8 {
    let here = match home() {
        Some(h) => h,
        None => {
            eprintln!("console flags: neither TEHAS_HOME nor HOME is set");
            return 64;
        }
    };
    let (verb, rest) = match given.split_first() {
        Some((verb, rest)) => (verb.as_str(), rest),
        None => return usage("set or clear is wanted"),
    };
    let file = Home::new(&here).state(State::Pause);
    match verb {
        "set" => match pause_args(rest) {
            Ok(reason) => pause_set(&file, &reason),
            Err(bad) => usage(bad),
        },
        "clear" => {
            // No --json here: the answer of a cleared host is the one line
            // which says the flag is gone, and tehas resume prints it.
            if !rest.is_empty() {
                return usage("clear takes no argument");
            }
            pause_clear(&file, account::uid())
        }
        _ => usage("set or clear, not a word this command knows"),
    }
}

/// The reason of `tehas pause set`: one argument, given once.
fn pause_args(rest: &[String]) -> Result<String, &'static str> {
    let mut reason: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        let Some(value) = rest.get(i + 1) else {
            return Err("an option without its value");
        };
        match rest[i].as_str() {
            "--reason" if reason.is_none() => reason = Some(value.clone()),
            _ => return Err("an option pause set does not know"),
        }
        i += 2;
    }
    Ok(reason.unwrap_or_default())
}

/// `tehas pause`: the first pause wins. A host already paused says what already
/// is and its flag is kept as it was; a file which is no flag holds no flag to
/// keep, so it is replaced with one which can be read.
fn pause_set(file: &Path, reason: &str) -> u8 {
    match read_pause(file) {
        Reading { state, fields, .. } if state == "ok" && !fields.is_empty() => {
            // The field of the flag with its quotes off: what a reader of the
            // object would take from it.
            let of = |want: &str| {
                fields
                    .iter()
                    .find(|(name, _)| name == want)
                    .map(|(_, value)| value.trim_matches('"').to_string())
                    .unwrap_or_default()
            };
            tehas_core::outln!("{}", pause_kept(&of("at"), &of("by"), &of("reason"), file));
            return 0;
        }
        Reading { state, fields, .. } => {
            if state != "none" && (state != "ok" || fields.is_empty()) {
                // The file in the way is named by this command, which is the
                // one that knows its path, and says what it is not.
                eprintln!("tehas pause: {}; writing a new one", not_a_flag(file));
            }
        }
    }
    if file.is_dir() {
        eprintln!("tehas pause: {}", not_a_flag(file));
        return 1;
    }
    let dir = match file.parent() {
        Some(d) => d,
        None => {
            eprintln!("tehas pause: {} holds no directory", file.display());
            return 1;
        }
    };
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("tehas pause: could not create {}: {e}", dir.display());
        return 1;
    }
    let said = oneline(reason);
    let (by, at) = (account::name(), time::iso(now()));
    let held = |text: &str| Value::String(text.to_string()).to_string();
    let doc = format!(
        "{{\"by\":{},\"at\":{},\"reason\":{}}}\n",
        held(&by),
        held(&at),
        held(&said),
    );
    // The name is the one name, so a leftover of a run killed mid-write, or
    // something planted in state/ under it, is removed first and the new file
    // made from nothing at mode 0600: what lands is this document.
    if let Err(e) = tehas_core::file::write_atomic(file, doc.as_bytes(), 0o600, &Temp::Predictable)
    {
        eprintln!("tehas pause: could not write {}: {e}", file.display());
        return 1;
    }
    tehas_core::outln!(
        "{}\nno new cycle starts on this host: {}",
        pause_written(&at, &by, &said),
        file.display()
    );
    0
}

/// `tehas resume`: the flag is gone, whatever the other switch says. The name
/// pause writes through goes too, so a leftover of it cannot outlive the flag.
/// Each of the two names is ours only when we can take it: what is left of
/// another uid's run, or planted under one of them, is reported and keeps the
/// command from saying the host resumed. <me> is the uid of this run, asked
/// once by the caller: a case which wants a run that does not know its own uid
/// gives None.
fn pause_clear(file: &Path, me: Option<u32>) -> u8 {
    let tmp = file.with_extension("json.new");
    let there = file.symlink_metadata().is_ok() || tmp.symlink_metadata().is_ok();
    // A name is ours to remove only when it holds a regular file of this run.
    // A symlink is not: the command that follows it would be this one, and the
    // link stays where whoever made it left it. The flag path reads through a
    // symlink, so the same rule holds there.
    for name in [file, &tmp] {
        if name.symlink_metadata().is_ok() && !owned_regular(name, me) {
            eprintln!("tehas resume: {} is not ours to remove", name.display());
            return 1;
        }
    }
    for name in [file, &tmp] {
        match fs::remove_file(name) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                eprintln!("tehas resume: could not remove {}", name.display());
                return 1;
            }
        }
    }
    tehas_core::outln!(
        "{}",
        if there {
            "resumed: the pause flag is gone"
        } else {
            "no pause flag on this host"
        }
    );
    0
}

/// 64 with the usage: `tehas resume` takes no argument.
pub fn resume_words(given: &[String]) -> Result<(), u8> {
    if given.is_empty() {
        return Ok(());
    }
    eprintln!("usage: tehas resume");
    Err(64)
}

/// What the other switch, the pause mark on the control issue, says once the
/// host's own is off: a line for standard output, one for standard error, or
/// nothing when it holds nothing. Only an allowed account can lift it
/// (AGENTS.md rule 5).
pub fn other_switch(
    tickets: &mut dyn Tickets,
    control: u64,
    allowed: &dyn Fn(&str) -> bool,
) -> Option<Result<String, String>> {
    let pause = Mark::Act(Act::Pause);
    let key = NonZeroU64::new(control).map(TicketKey::Number);
    let marks = key
        .as_ref()
        .and_then(|k| tickets.several(std::slice::from_ref(k)).pop())
        .and_then(|(_, d)| d.ok());
    let (Some(key), Some(digest)) = (key, marks) else {
        return Some(Err(format!(
            "tehas resume: the labels of issue #{control} could not be read: check tehas:pause there yourself"
        )));
    };
    if digest.marks.contains(&pause) {
        return Some(Ok(format!(
            "the factory stays paused: tehas:pause is still on issue #{control}"
        )));
    }
    let kinds = [HappenedKind::Marked, HappenedKind::Unmarked];
    let Ok(read) = tickets.history(&key, Window::Whole(ALL_MAX), Some(&kinds)) else {
        return Some(Err(format!(
            "tehas resume: the label events of issue #{control} could not be read"
        )));
    };
    let (lifted, by) = pause_holds_marks(&read.items, allowed)?;
    Some(Ok(if lifted {
        let by = if by.is_empty() { UNKNOWN_ACTOR } else { &by };
        format!(
            "the factory stays paused: tehas:pause was taken off #{control} by '{by}', who may not authorise work here"
        )
    } else {
        format!(
            "the factory stays paused: tehas:pause is not on issue #{control}, but its last tehas:pause event only applied it"
        )
    }))
}

/// `tehas resume`: the host's own switch comes off first, whatever GitHub then
/// says, and the exit status is that of the flag alone. `tickets` reads the
/// control issue; the console's wiring makes it.
pub fn resume(given: &[String], tickets: &mut dyn Tickets) -> u8 {
    if let Err(rc) = resume_words(given) {
        return rc;
    }
    let setting = |name: &str| env::var(name).unwrap_or_default();
    let Ok(control) = setting("TEHAS_CONTROL_ISSUE").parse::<u64>() else {
        eprintln!("tehas resume: TEHAS_CONTROL_ISSUE is no issue number");
        return 78;
    };
    if pause(&["clear".to_string()]) != 0 {
        return 1;
    }
    let list = setting("TEHAS_ALLOWED_LABELERS");
    let allowed = |who: &str| tehas_core::events::allowed_in(&list, who);
    match other_switch(tickets, control, &allowed) {
        Some(Ok(line)) => tehas_core::outln!("{line}"),
        Some(Err(line)) => eprintln!("{line}"),
        None => {}
    }
    0
}

/// What `tehas drain` was asked.
#[derive(Debug, PartialEq)]
pub struct Drain {
    wait: Option<u64>,
    json: bool,
}

/// The longest `--wait`, in digits, and the seconds a poll is waited for
/// without one.
const WAIT_DIGITS: usize = 7;
const POLL_WAIT: u64 = 600;
/// How often a lock another process holds is tried again.
const LOCK_TRY: Duration = Duration::from_millis(50);

fn wait_seconds(word: &str) -> Option<u64> {
    let digits = !word.is_empty() && word.bytes().all(|c| c.is_ascii_digit());
    let plain = word == "0" || !word.starts_with('0');
    (digits && plain && word.len() <= WAIT_DIGITS)
        .then(|| word.parse().ok())
        .flatten()
}

pub fn drain_words(given: &[String]) -> Result<Drain, u8> {
    let mut asked = Drain {
        wait: None,
        json: false,
    };
    let mut words = given.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--wait" if asked.wait.is_none() => {
                asked.wait = words.next().and_then(|w| wait_seconds(w));
                if asked.wait.is_none() {
                    eprintln!("usage: tehas drain [--wait SECONDS] [--json]");
                    return Err(64);
                }
            }
            "--json" if !asked.json => asked.json = true,
            _ => {
                eprintln!("usage: tehas drain [--wait SECONDS] [--json]");
                return Err(64);
            }
        }
    }
    Ok(asked)
}

/// Take the poll's lock and give it back, within `wait` seconds. A poll past
/// its read of the limit starts its cycles whatever the file says, so this
/// comes before the count, and its cycles are in it.
fn poll_passed(lock: &Path, wait: u64) -> bool {
    let Ok(f) = OpenOptions::new().append(true).create(true).open(lock) else {
        eprintln!("tehas drain: could not open {}", lock.display());
        return false;
    };
    match try_until(|| f.try_lock(), wait) {
        Ok(taken) => taken,
        Err(e) => {
            eprintln!("tehas drain: could not probe {}: {e}", lock.display());
            false
        }
    }
}

/// Try a lock until it is taken or `wait` seconds have passed: `Ok(false)`
/// when it was held the whole time, `Err` when it could not be tried at all.
fn try_until(
    mut attempt: impl FnMut() -> Result<(), TryLockError>,
    wait: u64,
) -> Result<bool, std::io::Error> {
    let deadline = Instant::now() + Duration::from_secs(wait);
    loop {
        match attempt() {
            Ok(()) => return Ok(true),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => sleep(LOCK_TRY),
            Err(TryLockError::WouldBlock) => return Ok(false),
            Err(TryLockError::Error(e)) => return Err(e),
        }
    }
}

/// The issues whose cycles hold their locks, lowest first. A lock that cannot
/// be probed counts as held: drained is never said on a guess.
fn cycles_running(dir: &Path) -> Vec<u64> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut held: Vec<u64> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let issue: u64 = path
                .file_name()?
                .to_str()?
                .strip_suffix(".lock")?
                .parse()
                .ok()?;
            matches!(lock::probe(&path), Probe::Held | Probe::Unknown(_)).then_some(issue)
        })
        .collect();
    held.sort_unstable();
    held
}

/// The answer of a drain. `passed` is false when a poll held its lock for the
/// whole wait: its cycles may not be counted, so nothing is drained.
fn drain_report(said: &str, running: &[u64], passed: bool, limit: &str, json: bool) -> String {
    let numbers = |sep: &str, mark: &str| {
        let each: Vec<String> = running.iter().map(|n| format!("{mark}{n}")).collect();
        each.join(sep)
    };
    if json {
        return format!(
            "{{\"api\":1,\"drained\":{},\"running\":[{}],\"limit\":{limit}}}",
            passed && running.is_empty(),
            numbers(",", "")
        );
    }
    let mut out = said.to_string();
    if !passed {
        out += "\nnot drained: a poll held its lock for the whole wait";
    }
    if !running.is_empty() {
        out += &format!("\nstill running: {}", numbers(", ", "#"));
    } else if passed {
        out += "\ndrained: no cycle is running";
    }
    out
}

/// The limit `tehas drain` sets, in the words of `tehas limit set`.
const DRAIN: [&str; 6] = ["--cycles", "0", "--plan", "off", "--reason", "draining"];

/// Writes the limit `tehas drain` sets and raises `factory.limit-changed`
/// of the write that changed it, and of it alone: draining a drained
/// factory writes the same limit and raises nothing.
fn drain_write(file: &Path) -> Result<(), u8> {
    let words = DRAIN.map(String::from);
    let Ok(want) = set_args(&words) else {
        return Err(64);
    };
    if write_limit(file, &want)? {
        raise_written(file);
    }
    Ok(())
}

/// `tehas drain`: the limit of no new work, then the count of what still runs.
pub fn drain(given: &[String]) -> u8 {
    let asked = match drain_words(given) {
        Ok(asked) => asked,
        Err(rc) => return rc,
    };
    let Some(here) = home() else {
        eprintln!("console flags: neither TEHAS_HOME nor HOME is set");
        return 64;
    };
    let here = Home::new(here);
    let file = here.state(State::Limit);
    if let Err(rc) = drain_write(&file) {
        return rc;
    }
    let said = read(&file, now()).said();
    let passed = poll_passed(
        &here.state(State::DispatchLock),
        asked.wait.unwrap_or(POLL_WAIT),
    );
    let deadline = Instant::now() + Duration::from_secs(asked.wait.unwrap_or(0));
    let running = loop {
        let running = cycles_running(&here.cycles_dir());
        if !passed || running.is_empty() || Instant::now() >= deadline {
            break running;
        }
        sleep(Duration::from_secs(1));
    };
    let limit = read(&file, now()).with_set();
    tehas_core::outln!(
        "{}",
        drain_report(&said, &running, passed, &limit, asked.json)
    );
    if !passed || !running.is_empty() {
        tehas_core::exit::NO
    } else {
        tehas_core::exit::OK
    }
}

/// The name the commands which honour a limit read it by: what `tehas drain`
/// writes and what every reader of the drained state opens.
pub const LIMIT_FILE: &str = State::Limit.relative();

/// Whether the limit in force holds every start, as `tehas drain` set it: no
/// cycle at once and planning off.
pub fn drained(file: &Path) -> bool {
    let r = read(file, now());
    r.state() == "ok" && r.json_of("cycles") == Some("0") && r.json_of("plan") == Some("false")
}

/// Whether the factory stands drained as `tehas drain` left it, by the file at
/// `LIMIT_FILE` of a home.
pub fn home_drained(home: &Path) -> bool {
    drained(&home.join(LIMIT_FILE))
}

/// `tehas_pause_read`: the flag as every command which honours it reads it.
/// A file which is not there is no flag, and anything else which is no flag is
/// `unreadable`, never `none`, so a reader holds every start rather than
/// finding no flag where a damaged file is. The reason comes back on one line,
/// kept to the 200 characters tehas pause writes, whatever the file holds.
/// The words of a reading are those of the pause line of `tehas status`; a
/// file which is no flag has no line, and is named by the command which found
/// it, because only there is the name of the file known.
pub fn read_pause(file: &Path) -> Reading {
    reading_of_pause(tehas_core::pause::read(file))
}

/// The flag, its time and its reason, in the words the readers of the switch
/// print it: tehas status for the pause line, and tehas pause for the flag it
/// wrote. A flag with no time of its own was not written by tehas pause, so
/// the time is not known. The heading is put on by the reading which prints
/// the line, so that it is the name of the flag and not of the limit.
fn pause_line(at: &str, reason: &str) -> String {
    let at = if at.is_empty() { "an unknown time" } else { at };
    if reason.is_empty() {
        format!("flag, written {at}")
    } else {
        format!("flag, written {at}: {reason}")
    }
}

/// The pause flag as the command which set it says it: `paused at <time> by
/// <account>`, with the reason after a colon when the flag has one.
fn pause_written(at: &str, by: &str, reason: &str) -> String {
    if reason.is_empty() {
        format!("paused at {at} by {by}")
    } else {
        format!("paused at {at} by {by}: {reason}")
    }
}

/// The pause flag as the command which found it already there says it.
fn pause_kept(at: &str, by: &str, reason: &str, file: &Path) -> String {
    let said = if reason.is_empty() {
        format!("already paused at {at} by {by}")
    } else {
        format!("already paused at {at} by {by}: {reason}")
    };
    format!("{said}\nthe flag is unchanged: {}", file.display())
}

/// The words a command says about a file which is no flag: the name of the
/// file, and what it is not. A directory is named as one, because a flag which
/// cannot be written over one is the case a person has to put right.
fn not_a_flag(file: &Path) -> String {
    if file.is_dir() {
        return format!("{} is a directory, not a flag", file.display());
    }
    format!("{} is not a pause flag", file.display())
}

/// The reading of the bytes of a pause flag, as tehas_core reads them.
#[cfg(test)]
fn read_pause_bytes(bytes: &[u8]) -> Reading {
    reading_of_pause(tehas_core::pause::parse(bytes))
}

/// The words and fields of a pause reading. `reason` is left out when empty.
fn reading_of_pause(p: tehas_core::pause::Pause) -> Reading {
    let mut out = Reading::of_pause(p.state);
    if p.state != "ok" {
        return out;
    }
    out.line = format!("pause     {}", pause_line(&p.at, &p.reason));
    out.fields
        .push(("by".into(), Value::String(p.by).to_string()));
    out.fields
        .push(("at".into(), Value::String(p.at).to_string()));
    if !p.reason.is_empty() {
        out.fields
            .push(("reason".into(), Value::String(p.reason).to_string()));
    }
    out
}

/// The word that asks `show` or `set` for the object in place of the words.
const JSON: &str = "--json";

/// What `tehas limit set` was asked for, in the words it was asked in.
#[derive(Default)]
struct Wanted {
    cycles: Option<String>,
    plan: Option<String>,
    for_: Option<String>,
    until: Option<String>,
    reason: Option<String>,
    /// Print the object `tehas limit --json` prints, not the words.
    json: bool,
}

fn set_args(rest: &[String]) -> Result<Wanted, &'static str> {
    let mut want = Wanted::default();
    let take = |what: &mut Option<String>, value: &str| -> Result<(), &'static str> {
        if what.is_some() {
            return Err("an option given twice");
        }
        *what = Some(value.to_string());
        Ok(())
    };
    let mut i = 0;
    while i < rest.len() {
        // The one word with no value: after --reason it is the reason.
        if rest[i] == JSON {
            if want.json {
                return Err("an option given twice");
            }
            want.json = true;
            i += 1;
            continue;
        }
        let Some(value) = rest.get(i + 1) else {
            return Err("an option without its value");
        };
        match rest[i].as_str() {
            "--cycles" => {
                take(&mut want.cycles, value)?;
                if !whole(value, 0, MAX_CYCLES) {
                    return Err("--cycles wants a whole number of cycles, 0 to 9999");
                }
            }
            "--plan" => {
                take(&mut want.plan, value)?;
                if value != "on" && value != "off" {
                    return Err("--plan wants on or off");
                }
            }
            "--for" => {
                take(&mut want.for_, value)?;
                if !whole(value, 1, MAX_FOR) {
                    return Err("--for wants a whole number of seconds, 1 to 999999999");
                }
            }
            "--until" => {
                take(&mut want.until, value)?;
                if epoch(value).is_none_or(|at| at <= now()) {
                    return Err(
                        "--until wants a time of the form 2026-10-17T14:05:00Z, later than now",
                    );
                }
            }
            "--reason" => take(&mut want.reason, value)?,
            _ => return Err("an option set does not know"),
        }
        i += 2;
    }
    if want.cycles.is_none() {
        return Err("--cycles is required");
    }
    if want.for_.is_some() && want.until.is_some() {
        return Err("--for and --until are one rule, not both");
    }
    Ok(want)
}

/// A string of digits, no sign and no leading zero, at or under `max` and at
/// least `min`: the argument of --cycles or --for, as the flag's own rules
/// bound it. A limit written from anything else would not read back.
fn whole(s: &str, min: u64, max: u64) -> bool {
    let digits = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits
        && (s.len() == 1 || s.as_bytes()[0] != b'0')
        && s.parse::<u64>().is_ok_and(|v| (min..=max).contains(&v))
}

/// The reading with `api` first: the object `tehas limit --json` prints.
fn api(out: Reading) -> String {
    let printed = out.with_set();
    format!("{{\"api\":1,{}", &printed[1..])
}

fn set(file: &Path, want: &Wanted) -> u8 {
    let before = read(file, now());
    match write_limit(file, want) {
        Ok(changed) => {
            let out = read(file, now());
            poll_if_raised(file, &before, &out);
            // `factory.limit-changed` of a write that wrote another limit:
            // a renewal that only moved `until` and the same limit set
            // again keep their `set` and raise nothing (T11.9).
            if changed {
                raise_written(file);
            }
            tehas_core::outln!("{}", if want.json { api(out) } else { out.said() });
            0
        }
        Err(rc) => rc,
    }
}

/// The dispatcher of the commands that write the limit: the factory's log
/// listener, one line per event, as every other raiser writes.
fn events(home: &Path) -> Dispatcher {
    tehas_core::raise::factory_logged(home, "tehas limit")
}

/// The home whose `state/` holds the limit file, or nothing for a path that
/// is not under a state directory.
fn home_of(file: &Path) -> Option<&Path> {
    file.parent().and_then(Path::parent)
}

fn warn(line: &str) {
    eprintln!("tehas limit: {line}");
}

/// `factory.limit-changed` of the limit just written: `set` and the key are
/// the time the write put beside the limit, so this raiser and a consumer
/// that polls `tehas limit --json` make one key. A reading with no `set`
/// raises nothing.
fn raise_written(file: &Path) {
    let Some(home) = home_of(file) else {
        return;
    };
    if let Some(p) = tehas_core::raise::written_limit(&limit_file::read(file, now())) {
        tehas_core::raise::raise(&events(home), p, &warn);
    }
}

/// `factory.limit-changed` of a clear: the file is gone, there is no `set`
/// to read, so tehas keys the change on the moment it made it.
fn raise_cleared(file: &Path) {
    let Some(home) = home_of(file) else {
        return;
    };
    let p = tehas_core::raise::cleared_limit(now());
    tehas_core::raise::raise(&events(home), p, &warn);
}

/// The cycles a reading lets run at once: no limit lets any number, and one
/// that expired or cannot be read lets none.
fn cycles_let(r: &Reading) -> f64 {
    match r.state() {
        "none" => f64::INFINITY,
        "ok" => r
            .json_of("cycles")
            .and_then(leading_number)
            .and_then(|n| n.parse().ok())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Whether the last poll's demand, as live/demand.json holds it, has an
/// issue waiting.
fn demand_waits(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("waiting").and_then(Value::as_u64))
        .is_some_and(|n| n > 0)
}

/// A limit raised while work waits asks for a poll, as `tehas wake` does:
/// the poll that acts on it comes now, not at the next timer. The ask is
/// best effort; the timer's poll reads the limit all the same.
fn poll_if_raised(file: &Path, before: &Reading, after: &Reading) {
    if cycles_let(after) <= cycles_let(before) {
        return;
    }
    let Some(home) = file.parent().and_then(Path::parent) else {
        return;
    };
    let home = Home::new(home);
    let waits = fs::read_to_string(home.live(Live::Demand)).is_ok_and(|t| demand_waits(&t));
    if waits {
        let _ = fs::File::create(home.state(State::DispatchAgain));
    }
}

/// Writes the limit. True when it wrote another limit than the one there:
/// the same `cycles`, `plan` and `reason` is the same limit, so a renewal
/// that only moves `until` keeps its `set` and raises no event.
fn write_limit(file: &Path, want: &Wanted) -> Result<bool, u8> {
    // One moment for the whole write: the `until` of a `--for` counts from
    // the `set` beside it.
    let now = now();
    let cycles = want.cycles.clone().unwrap_or_else(|| String::from("0"));
    let until = match (want.for_.as_deref(), want.until.as_deref()) {
        (Some(secs), _) => {
            let secs: u64 = secs.parse().unwrap_or(0);
            time::iso(now.saturating_add(secs as i64))
        }
        (None, Some(u)) => u.to_string(),
        (None, None) => String::new(),
    };
    let plan = want.plan.as_deref() != Some("off");
    let reason = oneline(want.reason.as_deref().unwrap_or(""));
    // The three values are what a JSON document holds them as: a boolean, a
    // string or null, and a string with its escapes.
    let held = |text: &str| Value::String(text.to_string()).to_string();
    let before = limit_file::read(file, now);
    let changed = !before.same(&cycles, plan, &reason);
    let set = match before.set_of(&cycles, plan, &reason, now) {
        Some(set) => format!(",\"set\":{}", held(&set)),
        None => String::new(),
    };
    let doc = format!(
        "{{\"v\":1,\"cycles\":{cycles},\"plan\":{plan},\"until\":{},\"reason\":{}{set}}}\n",
        if until.is_empty() {
            "null".to_string()
        } else {
            held(&until)
        },
        held(&reason),
    );
    let dir = match file.parent() {
        Some(d) => d,
        None => {
            eprintln!("tehas limit: {} holds no directory", file.display());
            return Err(1);
        }
    };
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("tehas limit: could not create {}: {e}", dir.display());
        return Err(1);
    }
    // The name is the one name, so a leftover of a run killed mid-write, or
    // something planted in state/ under it, is removed first and the new file
    // made from nothing at mode 0600: what lands is this document, whatever
    // was there before.
    if let Err(e) = tehas_core::file::write_atomic(file, doc.as_bytes(), 0o600, &Temp::Predictable)
    {
        eprintln!("tehas limit: could not write {}: {e}", file.display());
        return Err(1);
    }
    Ok(changed)
}

fn clear(file: &Path) -> u8 {
    let before = read(file, now());
    // There was a limit to clear only when a file was there: `tehas limit
    // clear` of no limit raises nothing, and clearing twice raises once.
    let had = file.symlink_metadata().is_ok();
    if had {
        if let Err(e) = fs::remove_file(file) {
            eprintln!("tehas limit: could not remove {}: {e}", file.display());
            return 1;
        }
    }
    let out = read(file, now());
    poll_if_raised(file, &before, &out);
    if had {
        raise_cleared(file);
    }
    tehas_core::outln!("{}", out.said());
    0
}

/// Whether a path is a regular file of <me>. A symlink is never ours: the name
/// is ours to remove only when it holds our own regular file, not one planted
/// under it or left there by a container which ran as another account. No uid
/// owns anything: a run whose `id` does not answer removes nothing of its own.
fn owned_regular(path: &Path, me: Option<u32>) -> bool {
    match (fs::symlink_metadata(path), me) {
        (Ok(meta), Some(me)) => meta.file_type().is_file() && meta.uid() == me,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tehas_core::testdir::TestDir;

    fn dir(name: &str) -> TestDir {
        TestDir::new(&format!("holds-{name}"))
    }

    // The file of a home, with its state/ directory made.
    fn written(d: &TestDir) -> PathBuf {
        fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
        Home::new(d.path()).state(State::Limit)
    }

    fn args(given: &[&str]) -> Vec<String> {
        given.iter().map(|s| s.to_string()).collect()
    }

    const NOW: i64 = 1_791_942_400;

    /// The only way these tests put a written time into the expectation they
    /// compare a file against: the clock read either side of the call which
    /// wrote it brackets what that call could have stamped, so a run which
    /// crosses a second boundary inside the call is still accepted and a time a
    /// minute out stops the test here, with the span in its text. `must_use`
    /// and `#[track_caller]` together mean a test which drops the check fails
    /// the lints rather than passing without them: the stamp it compares a file
    /// against only comes out of the check.
    #[must_use = "a written time counts only once the span around the call holds it"]
    #[track_caller]
    fn iso_in(t: i64, from: i64, to: i64) -> String {
        assert!(
            from <= t && t <= to,
            "written time {} is outside [{}, {}]",
            time::iso(t),
            time::iso(from),
            time::iso(to),
        );
        time::iso(t)
    }

    /// The reading of a document of one's own making.
    fn of(doc: &str) -> String {
        let d = dir("doc");
        let file = written(&d);
        fs::write(&file, doc).unwrap();
        read(&file, NOW).text()
    }

    fn said(doc: &str) -> String {
        let d = dir("said");
        let file = written(&d);
        fs::write(&file, doc).unwrap();
        read(&file, NOW).said()
    }

    fn set_in(d: &TestDir, given: &[&str]) -> (u8, String) {
        let file = written(d);
        let rc = set(&file, &set_args(&args(given)).unwrap());
        (rc, read(&file, NOW).text())
    }

    /// A home whose last poll left `waiting` issues in its demand.
    fn demand_of(d: &TestDir, waiting: u64) -> PathBuf {
        fs::create_dir_all(Home::new(d.path()).live_dir()).unwrap();
        fs::write(
            Home::new(d.path()).live(Live::Demand),
            format!("{{\"waiting\":{waiting},\"time\":\"2026-11-21T15:23:01Z\"}}\n"),
        )
        .unwrap();
        Home::new(d.path()).state(State::DispatchAgain)
    }

    #[test]
    fn a_limit_raised_from_none_while_work_waits_asks_for_a_poll() {
        let d = dir("raise-waits");
        let again = demand_of(&d, 1);
        set_in(&d, &["--cycles", "0"]);
        assert!(!again.exists(), "a limit of 0 asked for a poll");
        set_in(&d, &["--cycles", "1"]);
        assert!(again.exists(), "the raised limit waits for the timer");
        // A limit lowered asks for nothing; one raised again does.
        fs::remove_file(&again).unwrap();
        set_in(&d, &["--cycles", "3"]);
        assert!(again.exists());
        fs::remove_file(&again).unwrap();
        set_in(&d, &["--cycles", "2"]);
        assert!(!again.exists());
        set_in(&d, &["--cycles", "2", "--plan", "off"]);
        assert!(!again.exists());
        // A clear lifts every limit.
        assert_eq!(clear(&written(&d)), 0);
        assert!(again.exists());
    }

    #[test]
    fn a_limit_raised_with_nothing_waiting_asks_for_no_poll() {
        let d = dir("raise-idle");
        let again = demand_of(&d, 0);
        set_in(&d, &["--cycles", "0"]);
        set_in(&d, &["--cycles", "2"]);
        assert!(!again.exists());
        // No demand on record is none waiting.
        fs::remove_file(Home::new(d.path()).live(Live::Demand)).unwrap();
        set_in(&d, &["--cycles", "0"]);
        set_in(&d, &["--cycles", "2"]);
        assert!(!again.exists());
    }

    // A home whose logs/ stands, so the raiser's line lands where the test
    // reads it.
    fn logged(d: &TestDir) -> PathBuf {
        let log = Home::new(d.path()).factory_log();
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        log
    }

    // The factory's log, as `tehas limit` and `tehas drain` write their
    // event lines into it.
    fn log_events(d: &TestDir) -> Vec<tehas_ports::events::Envelope> {
        let log = logged(d);
        match fs::read_to_string(&log) {
            Ok(text) => text
                .lines()
                .filter_map(|l| {
                    l.split_once(" event {")
                        .map(|(_, rest)| format!("{{{rest}"))
                })
                .map(|json| serde_json::from_str(&json).expect("an event line"))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    // The moment the file says the limit was set, as its text.
    fn written_set(d: &TestDir) -> String {
        let doc: Value = serde_json::from_str(
            &fs::read_to_string(Home::new(d.path()).state(State::Limit)).unwrap(),
        )
        .unwrap();
        doc["set"].as_str().unwrap_or_default().to_string()
    }

    #[test]
    fn a_written_limit_raises_once_the_same_limit_raises_nothing() {
        let d = dir("event-set");
        logged(&d);
        set_in(&d, &["--cycles", "2"]);
        let all = log_events(&d);
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(
            all[0].event.name(),
            tehas_ports::events::Name::FactoryLimitChanged
        );
        let row = serde_json::to_value(&all[0]).unwrap();
        assert_eq!(row["data"]["state"], "ok");
        assert_eq!(row["data"]["cycles"], 2);
        assert_eq!(row["data"]["plan"], true);
        // The key and `at` are the `set` beside the limit: the raiser, the
        // printer of `tehas limit --json` and a poll make one key.
        let doc = written_set(&d);
        assert_eq!(row["key"], doc);
        assert_eq!(row["at"], doc);
        // The same limit set again keeps its `set` and raises nothing.
        set_in(&d, &["--cycles", "2"]);
        assert_eq!(
            log_events(&d).len(),
            1,
            "the same limit set twice raises once"
        );
        assert_eq!(written_set(&d), doc);
        // A renewal that only moves `until` keeps `set` and raises nothing.
        set_in(&d, &["--cycles", "2", "--for", "600"]);
        assert_eq!(log_events(&d).len(), 1);
        assert_eq!(written_set(&d), doc);
        // Another limit raises again, with its own `set`.
        set_in(&d, &["--cycles", "1"]);
        let all = log_events(&d);
        assert_eq!(all.len(), 2);
        let row = serde_json::to_value(&all[1]).unwrap();
        assert_eq!(row["data"]["cycles"], 1);
        assert_ne!(row["key"], doc);
    }

    #[test]
    fn a_clear_raises_observed_and_clearing_twice_raises_once() {
        let d = dir("event-clear");
        logged(&d);
        let file = written(&d);
        assert_eq!(clear(&file), 0);
        assert!(
            log_events(&d).is_empty(),
            "a clear of no limit raises nothing"
        );
        set_in(&d, &["--cycles", "2"]);
        assert_eq!(log_events(&d).len(), 1);
        assert_eq!(clear(&file), 0);
        let all = log_events(&d);
        assert_eq!(all.len(), 2);
        assert_eq!(
            all[1].event.name(),
            tehas_ports::events::Name::FactoryLimitChanged
        );
        let row = serde_json::to_value(&all[1]).unwrap();
        assert_eq!(row["data"]["state"], "none");
        // The file is gone: no `set` to read, so tehas keys the change on
        // the moment it made it.
        assert!(row["data"].get("set").is_none(), "{row}");
        assert!(row["data"].get("observed").is_some(), "{row}");
        assert_eq!(row["key"], row["at"]);
        assert_eq!(clear(&file), 0);
        assert_eq!(log_events(&d).len(), 2, "clearing twice raises once");
        // An unreadable file is still a limit that was there.
        fs::write(&file, "not a limit").unwrap();
        assert_eq!(clear(&file), 0);
        let all = log_events(&d);
        assert_eq!(all.len(), 3);
        let row = serde_json::to_value(&all[2]).unwrap();
        assert_eq!(row["data"]["state"], "none");
    }

    #[test]
    fn draining_raises_the_limit_once_and_draining_twice_raises_nothing() {
        let d = dir("event-drain");
        logged(&d);
        let file = written(&d);
        drain_write(&file).unwrap();
        let all = log_events(&d);
        assert_eq!(all.len(), 1, "{all:?}");
        let row = serde_json::to_value(&all[0]).unwrap();
        assert_eq!(row["data"]["cycles"], 0);
        assert_eq!(row["data"]["plan"], false);
        assert_eq!(row["data"]["reason"], "draining");
        let doc = written_set(&d);
        assert_eq!(row["key"], doc);
        // The same drained limit stands: nothing changed, nothing is raised.
        drain_write(&file).unwrap();
        assert_eq!(log_events(&d).len(), 1);
        // A drained limit that a person lifted is raised by the drain again.
        set_in(&d, &["--cycles", "2"]);
        drain_write(&file).unwrap();
        assert_eq!(log_events(&d).len(), 3);
    }

    #[test]
    fn a_limit_that_expired_or_cannot_be_read_lets_no_cycle_so_a_set_raises_it() {
        let d = dir("raise-expired");
        let again = demand_of(&d, 2);
        let file = written(&d);
        fs::write(&file, "{\"v\":1,\"cycles\":4,\"plan\":true,\"until\":\"2020-01-01T00:00:00Z\",\"reason\":\"\"}\n").unwrap();
        set_in(&d, &["--cycles", "1"]);
        assert!(again.exists());
        fs::remove_file(&again).unwrap();
        fs::write(&file, "not a limit").unwrap();
        set_in(&d, &["--cycles", "1"]);
        assert!(again.exists());
        assert!(!demand_waits("{}"));
        assert!(!demand_waits("not json"));
        assert!(demand_waits(r#"{"waiting":1}"#));
    }

    #[test]
    fn a_set_writes_the_file_and_reads_back_as_ok() {
        let d = dir("set");
        let before = now();
        let (rc, out) = set_in(&d, &["--cycles", "2"]);
        let after = now();
        assert_eq!(rc, 0);
        assert_eq!(out, r#"{"state":"ok","cycles":2,"plan":true}"#);
        let file = written(&d);
        let doc = fs::read_to_string(&file).unwrap();
        assert_eq!(
            doc,
            format!(
                "{{\"v\":1,\"cycles\":2,\"plan\":true,\"until\":null,\"reason\":\"\",\"set\":\"{}\"}}\n",
                iso_in(time_of(&doc, "set"), before, after),
            )
        );
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!file.with_extension("json.new").exists());
    }

    #[test]
    fn the_whole_limit_in_one_set() {
        let d = dir("whole");
        let (_, out) = set_in(
            &d,
            &[
                "--cycles",
                "1",
                "--plan",
                "off",
                "--until",
                "2036-01-01T14:05:00Z",
                "--reason",
                "held for alpha",
            ],
        );
        assert_eq!(
            out,
            r#"{"state":"ok","cycles":1,"plan":false,"until":"2036-01-01T14:05:00Z","reason":"held for alpha"}"#
        );
    }

    #[test]
    fn the_words_of_a_limit() {
        assert_eq!(
            said(r#"{"v":1,"cycles":2,"plan":true,"until":null}"#),
            "limit     2 cycles at once, planning on, until cleared"
        );
        assert_eq!(
            said(
                r#"{"v":1,"cycles":1,"plan":false,"until":"2036-10-14T01:46:40Z","reason":"held for alpha"}"#
            ),
            "limit     1 cycle at once, planning off, until 2036-10-14T01:46:40Z: held for alpha"
        );
        // A document which is no limit at all is read as unreadable, never as
        // no limit: the health file then holds every start.
        assert_eq!(
            said("{}"),
            "limit     unreadable: nothing starts until it is set again or cleared"
        );
        assert_eq!(
            said(r#"{"cycles":2,"plan":true}"#),
            "limit     unreadable: nothing starts until it is set again or cleared"
        );
        assert_eq!(
            said("not json"),
            "limit     unreadable: nothing starts until it is set again or cleared"
        );
        assert_eq!(said(r#"{"v":1,"cycles":2,"plan":true,"until":"2026-10-14T01:46:40Z","reason":"r"}"#),
            "limit     expired at 2026-10-14T01:46:40Z: nothing starts until it is set again or cleared");
    }

    #[test]
    fn the_same_set_twice_leaves_the_same_file() {
        let d = dir("twice");
        let want = [
            "--cycles",
            "3",
            "--plan",
            "off",
            "--until",
            "2036-01-01T14:05:00Z",
            "--reason",
            "r",
        ];
        set_in(&d, &want);
        let first = fs::read_to_string(written(&d)).unwrap();
        assert_eq!(set_in(&d, &want).0, 0);
        assert_eq!(fs::read_to_string(written(&d)).unwrap(), first);
    }

    #[test]
    fn a_new_set_replaces_the_whole_limit() {
        let d = dir("replace");
        set_in(
            &d,
            &[
                "--cycles", "1", "--plan", "off", "--for", "60", "--reason", "old",
            ],
        );
        let (_, out) = set_in(&d, &["--cycles", "4"]);
        assert_eq!(out, r#"{"state":"ok","cycles":4,"plan":true}"#);
    }

    #[test]
    fn for_makes_an_until_from_now() {
        let d = dir("for");
        let before = now();
        set_in(&d, &["--cycles", "1", "--for", "300"]);
        let after = now();
        // The flag stamps its own clock and adds what it was asked for, so the
        // file holds a second 300 on from one the clock passed through while
        // the call ran. A minute out either way is outside that span. The stamp
        // comes from the file, not from `read` at `NOW`, which would make any
        // `until` agree with itself.
        let doc = fs::read_to_string(written(&d)).unwrap();
        let until = time_of(&doc, "until");
        assert_eq!(
            doc,
            format!(
                "{{\"v\":1,\"cycles\":1,\"plan\":true,\"until\":\"{}\",\"reason\":\"\",\"set\":\"{}\"}}\n",
                iso_in(until, before + 300, after + 300),
                iso_in(time_of(&doc, "set"), before, after),
            )
        );
    }

    /// A limit as tehas wrote it at `SET`, long before these tests run.
    const SET: &str = "2026-10-01T00:00:00Z";
    const HELD: &[&str] = &["--cycles", "3", "--plan", "off", "--reason", "r"];

    fn held_since_set(d: &TestDir, until: &str) -> PathBuf {
        let file = written(d);
        let doc = format!(
            "{{\"v\":1,\"cycles\":3,\"plan\":false,\"until\":{until},\"reason\":\"r\",\"set\":\"{SET}\"}}\n"
        );
        fs::write(&file, doc).unwrap();
        file
    }

    fn set_of(file: &Path) -> Option<String> {
        limit_file::read(file, NOW).set
    }

    #[test]
    fn the_same_limit_set_again_keeps_its_set() {
        let d = dir("set-kept");
        let file = held_since_set(&d, "null");
        let first = fs::read_to_string(&file).unwrap();
        assert_eq!(set_in(&d, HELD).0, 0);
        assert_eq!(fs::read_to_string(&file).unwrap(), first);
    }

    #[test]
    fn a_renewal_that_only_moves_until_keeps_set() {
        let d = dir("set-renewed");
        let file = held_since_set(&d, "\"2036-01-01T14:05:00Z\"");
        for until in [
            &["--until", "2037-01-01T14:05:00Z"][..],
            &["--for", "60"],
            &[],
        ] {
            set_in(&d, &[HELD, until].concat());
            assert_eq!(set_of(&file).as_deref(), Some(SET), "{until:?}");
        }
        // An expired limit set again is renewed too.
        let file = held_since_set(&d, "\"2026-10-02T00:00:00Z\"");
        assert_eq!(read(&file, now()).state(), "expired");
        set_in(&d, HELD);
        assert_eq!(read(&file, now()).state(), "ok");
        assert_eq!(set_of(&file).as_deref(), Some(SET));
    }

    #[test]
    fn another_cycles_plan_or_reason_takes_the_moment_it_is_written_at() {
        for other in [
            &["--cycles", "4", "--plan", "off", "--reason", "r"][..],
            &["--cycles", "3", "--plan", "on", "--reason", "r"],
            &["--cycles", "3", "--plan", "off", "--reason", "other"],
            &["--cycles", "3", "--plan", "off"],
        ] {
            let d = dir("set-moved");
            let file = held_since_set(&d, "null");
            let before = now();
            set_in(&d, other);
            let after = now();
            let doc = fs::read_to_string(&file).unwrap();
            let set = iso_in(time_of(&doc, "set"), before, after);
            assert_eq!(set_of(&file), Some(set), "{other:?}");
        }
    }

    #[test]
    fn a_limit_over_one_that_cannot_be_read_takes_the_moment() {
        let d = dir("set-unreadable");
        let file = written(&d);
        fs::write(&file, format!("{{\"v\":2,\"set\":\"{SET}\"}}")).unwrap();
        let before = now();
        set_in(&d, HELD);
        let doc = fs::read_to_string(&file).unwrap();
        let set = iso_in(time_of(&doc, "set"), before, now());
        assert_eq!(set_of(&file), Some(set));
    }

    #[test]
    fn a_file_of_1_4_loads_and_stays_as_it_is_until_the_limit_changes() {
        let d = dir("set-1-4");
        let file = written(&d);
        let old = "{\"v\":1,\"cycles\":3,\"plan\":false,\"until\":null,\"reason\":\"r\"}\n";
        fs::write(&file, old).unwrap();
        assert_eq!(
            api(read(&file, NOW)),
            r#"{"api":1,"state":"ok","cycles":3,"plan":false,"reason":"r"}"#
        );
        // The same limit again is the same bytes: no set appears on a limit
        // nobody changed.
        set_in(&d, HELD);
        assert_eq!(fs::read_to_string(&file).unwrap(), old);
        let before = now();
        set_in(&d, &["--cycles", "1"]);
        let doc = fs::read_to_string(&file).unwrap();
        let set = iso_in(time_of(&doc, "set"), before, now());
        assert_eq!(set_of(&file), Some(set));
    }

    #[test]
    fn set_is_printed_last_and_not_by_read() {
        let d = dir("set-printed");
        let file = held_since_set(&d, "\"2036-01-01T14:05:00Z\"");
        assert_eq!(
            api(read(&file, NOW)),
            format!(
                r#"{{"api":1,"state":"ok","cycles":3,"plan":false,"until":"2036-01-01T14:05:00Z","reason":"r","set":"{SET}"}}"#
            )
        );
        assert_eq!(
            read(&file, NOW).with_set(),
            format!(
                r#"{{"state":"ok","cycles":3,"plan":false,"until":"2036-01-01T14:05:00Z","reason":"r","set":"{SET}"}}"#
            )
        );
        assert_eq!(
            read(&file, NOW).text(),
            r#"{"state":"ok","cycles":3,"plan":false,"until":"2036-01-01T14:05:00Z","reason":"r"}"#
        );
    }

    #[test]
    fn set_takes_json_anywhere_but_as_the_value_of_another_option() {
        for given in [
            &["--json", "--cycles", "2"][..],
            &["--cycles", "2", "--json"],
        ] {
            let want = set_args(&args(given)).unwrap();
            assert!(want.json, "{given:?}");
            assert_eq!(want.cycles.as_deref(), Some("2"));
        }
        let want = set_args(&args(&["--cycles", "2", "--reason", JSON])).unwrap();
        assert!(!want.json);
        assert_eq!(want.reason.as_deref(), Some(JSON));
        assert!(!set_args(&args(&["--cycles", "2"])).unwrap().json);
    }

    #[test]
    fn two_different_limits_written_in_one_second_do_not_share_a_set() {
        let d = dir("set-second");
        let file = written(&d);
        // The limit there was written a second from now, as a write in this
        // second would look to the next whatever second the test crosses.
        let there = now() + 1;
        fs::write(
            &file,
            format!(
                "{{\"v\":1,\"cycles\":3,\"plan\":false,\"until\":null,\"reason\":\"r\",\"set\":\"{}\"}}\n",
                time::iso(there)
            ),
        )
        .unwrap();
        set_in(&d, &["--cycles", "4"]);
        assert_eq!(set_of(&file), Some(time::iso(there + 1)));
        set_in(&d, &["--cycles", "5"]);
        assert_eq!(set_of(&file), Some(time::iso(there + 2)));
        // The same limit again stays where it is.
        set_in(&d, &["--cycles", "5"]);
        assert_eq!(set_of(&file), Some(time::iso(there + 2)));
    }

    #[test]
    fn a_clear_and_an_unreadable_limit_print_no_set() {
        let d = dir("set-cleared");
        let file = held_since_set(&d, "null");
        assert_eq!(clear(&file), 0);
        assert_eq!(api(read(&file, NOW)), r#"{"api":1,"state":"none"}"#);
        fs::write(&file, format!("{{\"v\":2,\"set\":\"{SET}\"}}")).unwrap();
        assert_eq!(api(read(&file, NOW)), r#"{"api":1,"state":"unreadable"}"#);
    }

    #[test]
    fn the_limit_of_a_drain_written_over_a_drain_keeps_its_set() {
        let d = dir("set-drain");
        let file = written(&d);
        fs::write(
            &file,
            format!("{{\"v\":1,\"cycles\":0,\"plan\":false,\"until\":null,\"reason\":\"draining\",\"set\":\"{SET}\"}}\n"),
        )
        .unwrap();
        set_in(&d, &DRAIN);
        assert_eq!(set_of(&file).as_deref(), Some(SET));
    }

    #[test]
    fn a_bad_argument_is_refused() {
        let d = dir("writes");
        for bad in [
            args(&[]),
            args(&["--cycles"]),
            args(&["--cycles", "-1"]),
            args(&["--cycles", "1.5"]),
            args(&["--cycles", "x"]),
            args(&["--cycles", "10000"]),
            args(&["--cycles", "1", "--cycles", "2"]),
            args(&["--cycles", "1", "--plan", "maybe"]),
            args(&["--cycles", "1", "--plan"]),
            args(&["--cycles", "1", "--for", "0"]),
            args(&["--cycles", "1", "--for", "5s"]),
            args(&["--cycles", "1", "--for", "1000000000"]),
            args(&[
                "--cycles",
                "1",
                "--for",
                "60",
                "--until",
                "2036-01-01T14:05:00Z",
            ]),
            args(&["--cycles", "1", "--until", "tomorrow"]),
            args(&["--cycles", "1", "--until", "2026-10-17 14:05:00Z"]),
            args(&["--cycles", "1", "--until", "2026-10-17T14:05:00+00:00"]),
            args(&["--cycles", "1", "--until", "2036-13-01T00:00:00Z"]),
            args(&["--cycles", "1", "--bogus"]),
            args(&["--cycles", "1", "extra", "x"]),
            args(&["--cycles", "1", "--reason"]),
            args(&["--reason", "said without a limit"]),
            args(&["--cycles", "2", JSON, JSON]),
            args(&["--cycles", "2", JSON, "x"]),
        ] {
            assert!(set_args(&bad).is_err(), "{bad:?} was taken");
        }
        // A refused set never reaches the file, so nothing of it is written.
        assert!(!Home::new(d.path()).state_dir().exists());
    }

    #[test]
    fn a_time_in_the_past_is_refused_and_a_day_out_of_range_names_the_day_it_names() {
        assert!(set_args(&args(&["--cycles", "1", "--until", "2000-01-01T00:00:00Z"])).is_err());
        // jq's fromdateiso8601 counts a day out of range on, so the 30th of
        // February is the 1st of March, and later than now.
        assert!(set_args(&args(&["--cycles", "1", "--until", "2036-02-30T00:00:00Z"])).is_ok());
        assert_eq!(epoch("2036-02-30T00:00:00Z"), epoch("2036-03-01T00:00:00Z"));
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2026-10-17T14:05:00Z"), Some(1_792_245_900));
        assert_eq!(epoch("9999-12-31T23:59:59Z"), Some(253_402_300_799));
        assert_eq!(epoch("1970-01-01T00:00:61Z"), None);
        assert_eq!(epoch("1970-01-01T00:00:00z"), None);
        assert_eq!(epoch("1970-01-01T00:00:00"), None);
        assert_eq!(epoch("1970-01-01T00:00:000Z"), None);
        assert_eq!(epoch("1970-01-1T00:00:00Z"), None);
        assert_eq!(epoch("1970-00-01T00:00:00Z"), None);
    }

    #[test]
    fn whole_numbers_only() {
        assert!(whole("0", 0, MAX_CYCLES));
        assert!(whole("9999", 0, MAX_CYCLES));
        assert!(!whole("10000", 0, MAX_CYCLES));
        assert!(!whole("", 0, MAX_CYCLES));
        assert!(!whole("+1", 0, MAX_CYCLES));
        assert!(!whole("1e2", 0, MAX_CYCLES));
        assert!(!whole("-1", 0, MAX_CYCLES));
        assert!(!whole("1.5", 0, MAX_CYCLES));
        assert!(!whole("0", 1, MAX_FOR));
        assert!(whole("1", 1, MAX_FOR));
        assert!(whole("999999999", 1, MAX_FOR));
        assert!(!whole("1000000000", 1, MAX_FOR));
    }

    #[test]
    fn a_reason_is_one_line_of_at_most_200_characters() {
        assert_eq!(oneline("a\tb\nc\u{1}d"), "a b c d");
        assert_eq!(oneline("a\t\t\tb"), "a   b");
        assert_eq!(oneline("  padded  "), "padded");
        assert_eq!(oneline("a\u{7f}b"), "a b");
        assert_eq!(oneline(&"x".repeat(300)).chars().count(), 200);
        assert_eq!(oneline("é\u{2028}ê"), "é ê");
        assert_eq!(oneline(""), "");
        assert_eq!(oneline("\t\u{85}\u{2029} \u{7f}x"), "x");
        assert_eq!(oneline("a  \tb  c"), "a   b  c");
        assert_eq!(oneline("a  \t  b"), "a     b");
        assert_eq!(oneline("a \t b"), "a   b");
        // 200 characters of a multibyte text, not 200 bytes.
        assert_eq!(oneline(&"é".repeat(250)), "é".repeat(200));
        // jq's slice comes before the edges are cut, so the 200th character is
        // kept and a space which followed it is gone with everything beyond.
        assert_eq!(
            oneline(&format!("{}x ", "y".repeat(199))),
            "y".repeat(199) + "x"
        );
        assert_eq!(oneline(&"x".repeat(200)), "x".repeat(200));
        assert_eq!(oneline(&format!("{}  x", "y".repeat(199))), "y".repeat(199));
    }

    #[test]
    fn the_reason_of_a_set_comes_back_as_one_line() {
        let d = dir("reason");
        let (_, out) = set_in(&d, &["--cycles", "1", "--reason", "\t a\u{1}b\n c \t"]);
        assert_eq!(
            out,
            r#"{"state":"ok","cycles":1,"plan":true,"reason":"a b  c"}"#
        );
    }

    #[test]
    fn no_file_is_none_and_a_directory_is_unreadable() {
        let d = dir("none");
        assert_eq!(
            read(&Home::new(d.path()).state(State::Limit), NOW).text(),
            r#"{"state":"none"}"#
        );
        let file = Home::new(d.path()).state(State::Limit);
        fs::create_dir_all(&file).unwrap();
        assert_eq!(read(&file, NOW).text(), r#"{"state":"unreadable"}"#);
    }

    #[test]
    fn what_is_no_limit_is_unreadable_never_none() {
        for bad in [
            "not json",
            "[]",
            "null",
            "\"limit\"",
            "1",
            r#"{"v":2,"cycles":1,"plan":true,"until":null}"#,
            r#"{"v":"1","cycles":1,"plan":true,"until":null}"#,
            r#"{"v":1,"cycles":-1,"plan":true,"until":null}"#,
            r#"{"v":1,"cycles":1.5,"plan":true,"until":null}"#,
            r#"{"v":1,"cycles":1e999,"plan":true,"until":null}"#,
            r#"{"v":1,"cycles":1.5,"plan":true,"until":null}"#,
            r#"{"v":1,"cycles":1e999,"plan":true,"until":null}"#,
            r#"{"cycles":1,"plan":true,"until":null}"#,
            r#"{"v":1,"plan":true,"until":null}"#,
            r#"{"v":1,"cycles":1,"plan":"yes","until":null}"#,
            r#"{"v":1,"cycles":1,"plan":true,"until":1}"#,
            r#"{"v":1,"cycles":1,"plan":true,"until":"soon"}"#,
            r#"{"v":1,"cycles":1,"plan":true,"until":"1970-01-01T00:00:00"}"#,
            r#"{"v":1,"cycles":1,"plan":true,"until":"1970-01-01T00:00:00z"}"#,
            r#"{"v":1,"cycles":1,"plan":true,"until":null,"reason":1}"#,
            r#"{"v":1,"cycles":1,"plan":true,"until":null,"reason":{}}"#,
            "",
            " ",
        ] {
            assert_eq!(of(bad), r#"{"state":"unreadable"}"#, "{bad} was read");
        }
    }

    #[test]
    fn a_number_of_the_file_is_plain_digits_or_the_limit_is_unreadable() {
        // The dispatcher reads cycles as an integer, so only plain digits are
        // a limit; the reading echoes them back byte for byte.
        for cycles in ["1", "0", "9999", "100000"] {
            let text = of(&format!(r#"{{"v":1,"cycles":{cycles},"plan":true}}"#));
            assert_eq!(
                text,
                format!(r#"{{"state":"ok","cycles":{cycles},"plan":true}}"#),
                "{cycles}"
            );
        }
        for doc in [
            "1e0", "0.0", "2.0", "-0", "-0.0", "1.0", "1e2", "1E+2", "01",
        ] {
            let text = of(&format!(r#"{{"v":1,"cycles":{doc},"plan":true}}"#));
            assert_eq!(text, r#"{"state":"unreadable"}"#, "{doc}");
        }
    }

    #[test]
    fn a_limit_behind_a_link_is_the_file_it_points_at() {
        let d = dir("link");
        let real = d.join("elsewhere.json");
        fs::write(
            &real,
            r#"{"v":1,"cycles":2,"plan":true,"until":null,"reason":""}"#,
        )
        .unwrap();
        let file = written(&d);
        symlink(&real, &file).unwrap();
        assert_eq!(
            read(&file, NOW).text(),
            r#"{"state":"ok","cycles":2,"plan":true}"#
        );
        // What cannot be read through the name is never read as no limit.
        fs::remove_file(&real).unwrap();
        assert_eq!(read(&file, NOW).text(), r#"{"state":"unreadable"}"#);
    }

    #[test]
    fn a_limit_whose_until_has_passed_is_expired() {
        let doc = r#"{"v":1,"cycles":2,"plan":true,"until":"2026-10-14T01:46:40Z","reason":"r"}"#;
        assert_eq!(
            of(doc),
            r#"{"state":"expired","cycles":2,"plan":true,"until":"2026-10-14T01:46:40Z","reason":"r"}"#
        );
        let d = dir("expired");
        let file = written(&d);
        fs::write(
            &file,
            r#"{"v":1,"cycles":2,"plan":true,"until":"2026-10-14T01:46:40Z","reason":""}"#,
        )
        .unwrap();
        // The second it expires is the first that has passed, and the second
        // before it is not: jq's rule is `until <= now`.
        assert!(read(&file, NOW).text().starts_with(r#"{"state":"expired""#));
        assert!(read(&file, NOW - 1).text().starts_with(r#"{"state":"ok""#));
    }

    #[test]
    fn what_a_reader_is_given_at_a_moment_of_its_own() {
        let d = dir("reader");
        let file = Home::new(d.path()).state(State::Limit);
        assert_eq!(read(&file, NOW).text(), r#"{"state":"none"}"#);
        set_in(
            &d,
            &[
                "--cycles",
                "2",
                "--plan",
                "off",
                "--until",
                "2036-01-01T14:05:00Z",
                "--reason",
                "r",
            ],
        );
        let until = epoch("2036-01-01T14:05:00Z").unwrap();
        assert_eq!(
            read(&file, until - 300).text(),
            r#"{"state":"ok","cycles":2,"plan":false,"until":"2036-01-01T14:05:00Z","reason":"r"}"#
        );
        assert!(read(&file, until)
            .text()
            .starts_with(r#"{"state":"expired""#));
    }

    #[test]
    fn the_reading_keeps_its_own_field_order_whatever_the_file_s() {
        assert_eq!(
            of(r#"{"reason":"r","plan":false,"cycles":0,"v":1,"until":"2036-01-01T00:00:00Z"}"#),
            r#"{"state":"ok","cycles":0,"plan":false,"until":"2036-01-01T00:00:00Z","reason":"r"}"#
        );
    }

    /// The drained state read from bytes written straight to the file: a
    /// reading of a number which is no whole 0, or of a plan which is no
    /// boolean, is never drained.
    fn drained_of(doc: &str) -> bool {
        let d = dir("drained");
        let file = written(&d);
        fs::write(&file, doc).unwrap();
        drained(&file)
    }

    #[test]
    fn a_drain_is_drained_and_everything_else_is_not() {
        // What tehas drain writes.
        assert!(drained_of(
            r#"{"v":1,"cycles":0,"plan":false,"until":null,"reason":"draining"}"#
        ));
        // The reason of no drain, and the plan of one. A limit of no cycles
        // that carries a future `until` is still a drained factory: the clock
        // ends nothing that `tehas limit clear` does not.
        assert!(drained_of(r#"{"v":1,"cycles":0,"plan":false}"#));
        assert!(drained_of(
            r#"{"v":1,"cycles":0,"plan":false,"until":"2036-01-01T00:00:00Z"}"#
        ));
        for not in [
            // No file at all is no limit, which is work allowed.
            r#"{}"#,
            r#"not json"#,
            r#"{"v":1,"cycles":1,"plan":false,"until":null}"#,
            r#"{"v":1,"cycles":0,"plan":true,"until":null}"#,
            // A limit of no cycles whose clock has run out is held by the clock
            // as expired, which is another state than the one a drain leaves.
            r#"{"v":1,"cycles":0,"plan":false,"until":"2000-01-01T00:00:00Z"}"#,
        ] {
            assert!(!drained_of(not), "{not} read as drained");
        }
        // A limit that has expired holds every start and is not the state a
        // drain leaves: tehas limit clear frees it too, but the factory was
        // never drained.
        let d = dir("drained-expired");
        let file = written(&d);
        fs::write(
            &file,
            r#"{"v":1,"cycles":0,"plan":false,"until":"2020-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert!(!drained(&file));
    }

    #[test]
    fn a_drained_factory_is_named_at_the_home_s_own_file() {
        let d = dir("drained-home");
        assert!(!home_drained(d.path()));
        let file = written(&d);
        fs::write(
            &file,
            r#"{"v":1,"cycles":0,"plan":false,"until":null,"reason":"draining"}"#,
        )
        .unwrap();
        assert!(home_drained(d.path()));
        fs::remove_file(&file).unwrap();
        assert!(!home_drained(d.path()));
        // The name the readers share.
        assert_eq!(file, d.join(LIMIT_FILE));
    }

    #[test]
    fn set_refuses_a_directory_in_its_way() {
        let d = dir("dirblock");
        fs::create_dir_all(Home::new(d.path()).state(State::Limit)).unwrap();
        let file = Home::new(d.path()).state(State::Limit);
        assert_eq!(set(&file, &set_args(&args(&["--cycles", "1"])).unwrap()), 1);
    }

    #[test]
    fn clear_takes_the_file_off_and_clearing_twice_is_fine() {
        let d = dir("clear");
        set_in(&d, &["--cycles", "1"]);
        let file = written(&d);
        assert_eq!(clear(&file), 0);
        assert!(!file.exists());
        assert_eq!(clear(&file), 0);
    }

    #[test]
    fn a_planted_temporary_does_not_become_the_limit() {
        let d = dir("planted");
        let file = written(&d);
        fs::write(
            file.with_extension("json.new"),
            r#"{"v":1,"cycles":9,"plan":false,"until":null,"reason":"planted"}"#,
        )
        .unwrap();
        assert_eq!(set(&file, &set_args(&args(&["--cycles", "1"])).unwrap()), 0);
        assert_eq!(
            read(&file, NOW).text(),
            r#"{"state":"ok","cycles":1,"plan":true}"#
        );
    }

    #[test]
    fn the_words_of_a_drain() {
        assert_eq!(
            drain_words(&args(&["--wait", "20", "--json"])),
            Ok(Drain {
                wait: Some(20),
                json: true
            })
        );
        assert_eq!(drain_words(&args(&["--wait", "0"])).unwrap().wait, Some(0));
        assert_eq!(
            drain_words(&args(&["--wait", "9999999"])).unwrap().wait,
            Some(9_999_999)
        );
        for bad in [
            &["--wait"][..],
            &["--wait", "x"],
            &["--wait", "-1"],
            &["--wait", "01"],
            &["--wait", ""],
            &["--wait", "10000000"],
            &["--wait", "1", "--wait", "2"],
            &["--json", "--json"],
            &["now"],
            &[""],
        ] {
            assert_eq!(drain_words(&args(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn a_drain_reports_as_it_always_has() {
        let limit = r#"{"state":"ok"}"#;
        assert_eq!(
            drain_report("said", &[], true, limit, true),
            r#"{"api":1,"drained":true,"running":[],"limit":{"state":"ok"}}"#
        );
        assert_eq!(
            drain_report("said", &[7, 12], true, limit, true),
            r#"{"api":1,"drained":false,"running":[7,12],"limit":{"state":"ok"}}"#
        );
        assert_eq!(
            drain_report("said", &[], true, limit, false),
            "said\ndrained: no cycle is running"
        );
        assert_eq!(
            drain_report("said", &[7, 12], true, limit, false),
            "said\nstill running: #7, #12"
        );
    }

    #[test]
    fn a_poll_that_kept_its_lock_is_not_drained_and_still_answered() {
        let limit = r#"{"state":"ok"}"#;
        assert_eq!(
            drain_report("said", &[], false, limit, true),
            r#"{"api":1,"drained":false,"running":[],"limit":{"state":"ok"}}"#
        );
        assert_eq!(
            drain_report("said", &[], false, limit, false),
            "said\nnot drained: a poll held its lock for the whole wait"
        );
        assert_eq!(
            drain_report("said", &[7], false, limit, false),
            "said\nnot drained: a poll held its lock for the whole wait\nstill running: #7"
        );
    }

    #[test]
    fn only_a_held_lock_of_an_issue_is_a_running_cycle() {
        let d = dir("cycles");
        assert_eq!(cycles_running(&d.join("none")), Vec::<u64>::new());
        fs::write(d.join("5.lock"), "").unwrap();
        let held = File::create(d.join("12.lock")).unwrap();
        held.lock().unwrap();
        let held2 = File::create(d.join("7.lock")).unwrap();
        held2.lock().unwrap();
        fs::write(d.join("x.lock"), "").unwrap();
        fs::write(d.join("9.status"), "").unwrap();
        assert_eq!(cycles_running(d.path()), vec![7, 12]);
    }

    #[test]
    fn a_held_poll_lock_runs_out_its_wait_and_a_free_one_is_passed() {
        let d = dir("poll");
        let lock = d.join("dispatch.lock");
        assert!(poll_passed(&lock, 0));
        // The holder is a process of its own, as in lock::tests. A lock this
        // process took would belong to its open file description, which a
        // child another test forks in the meantime shares until it execs, so
        // the lock could outlive its drop here. The holder prints when it
        // holds and gives the lock up when its stdin closes.
        let mut holder = std::process::Command::new("flock")
            .arg("-x")
            .arg(&lock)
            .arg("-c")
            .arg("echo held; read _")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut said = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(holder.stdout.take().unwrap()),
            &mut said,
        )
        .unwrap();
        assert_eq!(said.trim(), "held", "the holder never took the lock");
        assert!(!poll_passed(&lock, 0));
        drop(holder.stdin.take().unwrap());
        holder.wait().unwrap();
        assert!(poll_passed(&lock, 0));
        assert!(!poll_passed(&d.join("no/such/dir"), 0));
    }

    #[test]
    fn a_lock_that_cannot_be_tried_is_an_error_not_a_held_lock() {
        let mut tries = 0;
        let held = try_until(
            || {
                tries += 1;
                Err(TryLockError::WouldBlock)
            },
            0,
        );
        assert_eq!((held.ok(), tries), (Some(false), 1));
        let mut tries = 0;
        let freed = try_until(
            || {
                tries += 1;
                if tries < 3 {
                    Err(TryLockError::WouldBlock)
                } else {
                    Ok(())
                }
            },
            1,
        );
        assert_eq!((freed.ok(), tries), (Some(true), 3));
        let broken = try_until(
            || Err(TryLockError::Error(std::io::Error::other("no locks"))),
            1,
        );
        assert_eq!(broken.unwrap_err().to_string(), "no locks");
    }

    #[test]
    fn a_verb_of_its_own_or_none_is_64() {
        env::set_var("TEHAS_HOME", "/nonexistent-tehas-home");
        assert_eq!(limit(&args(&[])), 64);
        assert_eq!(limit(&args(&["bogus"])), 64);
        assert_eq!(limit(&args(&["clear", "now"])), 64);
        assert_eq!(limit(&args(&["read", "--now", "x"])), 64);
        assert_eq!(limit(&args(&["read", "--now"])), 64);
        assert_eq!(limit(&args(&["show", "--json", "--json"])), 64);
        assert_eq!(limit(&args(&["read", "--json"])), 64);
        assert_eq!(main(&args(&[])), 64);
        assert_eq!(main(&args(&["other"])), 64);
        env::remove_var("TEHAS_HOME");
    }

    #[test]
    fn the_reading_of_a_shown_limit_has_no_api_and_a_shown_one_has_it_first() {
        let text = of(r#"{"v":1,"cycles":1,"plan":false,"until":null,"reason":"r"}"#);
        assert_eq!(
            text,
            r#"{"state":"ok","cycles":1,"plan":false,"reason":"r"}"#
        );
        let d = dir("shown");
        let file = written(&d);
        fs::write(
            &file,
            r#"{"v":1,"cycles":1,"plan":false,"until":null,"reason":"r"}"#,
        )
        .unwrap();
        assert_eq!(
            api(read(&file, NOW)),
            r#"{"api":1,"state":"ok","cycles":1,"plan":false,"reason":"r"}"#
        );
    }

    #[test]
    fn the_clock_reads_the_test_variable_when_it_names_a_time() {
        assert_eq!(clock(Some("1700000000".into())), 1_700_000_000);
        assert!(clock(None) > 1_791_942_400);
    }

    // The pause flag: the file tehas pause writes, read as every command which
    // honours it reads it. `of_pause` reads bytes, so a case says what is at
    // the path without needing the account of the run.
    fn of_pause(doc: &str) -> String {
        read_pause_bytes(doc.as_bytes()).text()
    }

    fn line_of_pause(doc: &str) -> String {
        read_pause_bytes(doc.as_bytes()).said()
    }

    /// The `until` or the `set` of a written limit document as the seconds
    /// since the epoch it names.
    fn time_of(doc: &str, key: &str) -> i64 {
        let time = serde_json::from_str::<Value>(doc)
            .unwrap()
            .get(key)
            .and_then(Value::as_str)
            .expect("the limit document holds no such time")
            .to_string();
        epoch(&time).expect("the time of a limit is no time")
    }

    /// The `at` of a pause document as the seconds since the epoch it names.
    fn pause_at(doc: &str) -> i64 {
        let at = serde_json::from_str::<Value>(doc)
            .unwrap()
            .get("at")
            .and_then(Value::as_str)
            .expect("the pause document holds no at")
            .to_string();
        epoch(&at).expect("the at of a pause is no time")
    }

    fn pause_file(d: &TestDir) -> PathBuf {
        fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
        Home::new(d.path()).state(State::Pause)
    }

    #[test]
    fn a_flag_reads_as_the_object_the_readers_are_given() {
        assert_eq!(
            of_pause(
                r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}"#
            ),
            r#"{"state":"ok","by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}"#
        );
        // A reason of no characters is a flag with no reason, and the field is
        // left out of the reading as tehas_pause_read leaves it out.
        assert_eq!(
            of_pause(r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":""}"#),
            r#"{"state":"ok","by":"boss","at":"2026-10-15T10:00:00Z"}"#
        );
        // by may be anything, and anything but a string reads as nobody.
        assert_eq!(
            of_pause(r#"{"by":7,"at":"2026-10-15T10:00:00Z","reason":"r"}"#),
            r#"{"state":"ok","by":"","at":"2026-10-15T10:00:00Z","reason":"r"}"#
        );
        assert_eq!(
            of_pause(r#"{"at":"2026-10-15T10:00:00Z","reason":"r"}"#),
            r#"{"state":"ok","by":"","at":"2026-10-15T10:00:00Z","reason":"r"}"#
        );
    }

    #[test]
    fn the_words_of_a_flag_are_the_pause_line_of_tehas_status() {
        assert_eq!(
            line_of_pause(
                r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}"#
            ),
            "pause     flag, written 2026-10-15T10:00:00Z: waiting for the gate"
        );
        assert_eq!(
            line_of_pause(r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":""}"#),
            "pause     flag, written 2026-10-15T10:00:00Z"
        );
    }

    #[test]
    fn the_reason_of_a_flag_is_one_line_of_at_most_200_characters() {
        // The rule of tehas_pause_read: the reader applies it, so a flag which
        // reached the file some other way forges no line of tehas status.
        assert_eq!(
            of_pause(r#"{"by":"b","at":"2026-10-15T10:00:00Z","reason":"a\tb\nc\u0001d"}"#),
            r#"{"state":"ok","by":"b","at":"2026-10-15T10:00:00Z","reason":"a b c d"}"#
        );
        let long = "x".repeat(300);
        let doc = format!(r#"{{"by":"b","at":"2026-10-15T10:00:00Z","reason":"{long}"}}"#);
        assert_eq!(
            read_pause_bytes(doc.as_bytes())
                .field("reason")
                .chars()
                .count(),
            200
        );
    }

    #[test]
    fn what_is_no_flag_is_unreadable_never_none() {
        // A damaged flag holds the factory paused, so it is never read as no
        // flag: the same rule the limit reads by.
        for bad in [
            "not a flag at all",
            "",
            "[]",
            "null",
            "7",
            r#"{"at": 7}"#,
            r#"{"at":"2026-10-15T10:00:00Z"}"#,
            r#"{"at":"2026-10-15T10:00:00Z","reason":7}"#,
            r#"{"at":null,"reason":""}"#,
            "{",
        ] {
            assert_eq!(of_pause(bad), r#"{"state":"unreadable"}"#, "{bad} was read");
            // The words of such a reading are those the dispatcher has always
            // logged, so a reader which prints the line says the same thing
            // the reader which reads the file says today.
            assert_eq!(
                line_of_pause(bad),
                "pause.json is not a pause flag: taking it as paused",
                "{bad} was said"
            );
        }
    }

    #[test]
    fn no_flag_is_none() {
        let d = dir("pause-none");
        let file = Home::new(d.path()).state(State::Pause);
        assert_eq!(read_pause(&file).text(), r#"{"state":"none"}"#);
        assert_eq!(read_pause(&file).said(), "pause     none");
    }

    #[test]
    fn a_flag_behind_a_link_is_the_file_it_points_at() {
        let d = dir("pause-link");
        let real = d.join("elsewhere.json");
        fs::write(
            &real,
            r#"{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"r"}"#,
        )
        .unwrap();
        let file = pause_file(&d);
        symlink(&real, &file).unwrap();
        assert_eq!(
            read_pause(&file).text(),
            r#"{"state":"ok","by":"boss","at":"2026-10-15T10:00:00Z","reason":"r"}"#
        );
        fs::remove_file(&real).unwrap();
        assert_eq!(read_pause(&file).text(), r#"{"state":"unreadable"}"#);
    }

    #[test]
    fn a_pause_writes_the_flag_reads_back_and_keeps_the_first_one() {
        let d = dir("pause-set");
        let file = pause_file(&d);
        let before = now();
        assert_eq!(pause_set(&file, "waiting for the gate"), 0);
        let after = now();
        let at = iso_in(pause_at(&fs::read_to_string(&file).unwrap()), before, after);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!(
                "{{\"by\":\"{}\",\"at\":\"{}\",\"reason\":\"waiting for the gate\"}}\n",
                account::name(),
                at,
            )
        );
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!file.with_extension("json.new").exists());
        // The first pause wins: the second says what already is and leaves the
        // file alone, whatever it was asked for.
        assert_eq!(pause_set(&file, "second"), 0);
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("waiting for the gate"));
    }

    #[test]
    fn a_pause_with_nothing_to_say_writes_an_empty_reason() {
        let d = dir("pause-bare");
        let file = pause_file(&d);
        let before = now();
        assert_eq!(pause_set(&file, ""), 0);
        let after = now();
        let at = iso_in(pause_at(&fs::read_to_string(&file).unwrap()), before, after);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!(
                "{{\"by\":\"{}\",\"at\":\"{}\",\"reason\":\"\"}}\n",
                account::name(),
                at,
            )
        );
        // The reading of such a flag has no reason, and its line says none.
        assert_eq!(
            read_pause(&file).text(),
            format!(
                "{{\"state\":\"ok\",\"by\":\"{}\",\"at\":\"{}\"}}",
                account::name(),
                at
            )
        );
        assert_eq!(
            read_pause(&file).said(),
            format!("pause     flag, written {}", at)
        );
    }

    #[test]
    fn the_reason_of_a_pause_is_one_line_before_it_is_written() {
        let d = dir("pause-reason");
        let file = pause_file(&d);
        assert_eq!(pause_set(&file, "\t a\u{1}b\n c \t"), 0);
        assert_eq!(read_pause(&file).field("reason"), "a b  c");
        // 200 characters of a multibyte reason, not 200 bytes.
        assert_eq!(pause_set(&file, &"é".repeat(250)), 0);
        // The first flag was kept, so the long reason was never written; set
        // again with no flag there to keep.
        fs::remove_file(&file).unwrap();
        assert_eq!(pause_set(&file, &"é".repeat(250)), 0);
        assert_eq!(read_pause(&file).field("reason"), "é".repeat(200));
    }

    #[test]
    fn a_flag_that_is_no_flag_is_replaced_and_said() {
        let d = dir("pause-bad");
        let file = pause_file(&d);
        fs::write(&file, "{\"at\": 7}").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(pause_set(&file, "fresh"), 0);
        assert_eq!(read_pause(&file).field("reason"), "fresh");
        // A directory in the way is reported and nothing is written: the one
        // case where the command names the file rather than the flag in it.
        let d = dir("pause-dir");
        let file = Home::new(d.path()).state(State::Pause);
        fs::create_dir_all(&file).unwrap();
        assert_eq!(pause_set(&file, "x"), 1);
        assert!(file.is_dir());
    }

    #[test]
    fn a_planted_temporary_does_not_become_the_flag() {
        let d = dir("pause-planted");
        let file = pause_file(&d);
        fs::write(
            file.with_extension("json.new"),
            r#"{"by":"plant","at":"2026-10-15T10:00:00Z","reason":"planted"}"#,
        )
        .unwrap();
        assert_eq!(pause_set(&file, "mine"), 0);
        assert_eq!(read_pause(&file).field("reason"), "mine");
        assert!(!file.with_extension("json.new").exists());
    }

    #[test]
    fn resume_refuses_a_name_which_is_not_ours() {
        // A symlink on either name is not a leftover of ours: the link stays
        // where a person left it, its target is not touched, and the command
        // does not report the flag as gone.
        let d = dir("pause-clear-symlink");
        let file = pause_file(&d);
        let outside = d.join("victim");
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, file.with_extension("json.new")).unwrap();
        assert_eq!(pause_clear(&file, account::uid()), 1);
        assert!(outside.is_dir());
        assert!(file.with_extension("json.new").symlink_metadata().is_ok());
        // The flag path reads through a symlink, so taking the link off would
        // be following it: the same rule holds there.
        let d = dir("pause-clear-flag-symlink");
        let file = pause_file(&d);
        let outside = d.join("keepme");
        fs::write(&outside, "not a flag").unwrap();
        symlink(&outside, &file).unwrap();
        assert_eq!(pause_clear(&file, account::uid()), 1);
        assert!(outside.exists());
        assert!(file.symlink_metadata().is_ok());
    }

    #[test]
    fn resume_says_so_when_a_name_cannot_be_removed() {
        // Nothing may be written under a state directory which is not ours to
        // write in, and the command does not print that the flag is gone when
        // something still sits on one of its names.
        let d = dir("pause-clear-readonly");
        let file = pause_file(&d);
        assert_eq!(pause_set(&file, "x"), 0);
        fs::write(file.with_extension("json.new"), "half a flag").unwrap();
        fs::set_permissions(
            Home::new(d.path()).state_dir(),
            fs::Permissions::from_mode(0o500),
        )
        .unwrap();
        assert_eq!(pause_clear(&file, account::uid()), 1);
        // The state directory, given back its mode.
        fs::set_permissions(
            Home::new(d.path()).state_dir(),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(file.exists() || file.with_extension("json.new").exists());
    }

    #[test]
    fn resume_takes_the_flag_and_its_temporary_off() {
        let d = dir("pause-clear");
        let file = pause_file(&d);
        assert_eq!(pause_clear(&file, account::uid()), 0);
        assert_eq!(pause_set(&file, "x"), 0);
        // A leftover of a run killed mid-write goes with the flag, so it
        // cannot outlive it.
        fs::write(
            file.with_extension("json.new"),
            r#"{"by":"b","at":"2026-10-15T10:00:00Z","reason":""}"#,
        )
        .unwrap();
        assert_eq!(pause_clear(&file, account::uid()), 0);
        assert!(!file.exists());
        assert!(!file.with_extension("json.new").exists());
        // Clearing a host which is not paused succeeds and says so.
        assert_eq!(pause_clear(&file, account::uid()), 0);
    }

    #[test]
    fn resume_removes_nothing_when_the_run_does_not_know_its_uid() {
        // The failure mode of the ownership check: with no uid nothing can be
        // told apart from something planted on the name of a flag, so both the
        // flag and its temporary stay and the host is not reported resumed.
        let d = dir("pause-clear-no-uid");
        let file = pause_file(&d);
        assert_eq!(pause_set(&file, "x"), 0);
        fs::write(file.with_extension("json.new"), "half a flag").unwrap();
        assert_eq!(pause_clear(&file, None), 1);
        assert!(file.exists());
        assert!(file.with_extension("json.new").exists());
    }

    #[test]
    fn the_verbs_of_the_pause_flag_check_their_arguments() {
        for bad in [
            args(&[]),
            args(&["bogus"]),
            // The retired verbs are refused as unknown words: a script carried
            // over from an older release gets 64, not a reading of the flag.
            args(&["show", "--json"]),
            args(&["read", "--line"]),
            args(&["set", "--reason"]),
            args(&["set", "--reason", "one", "--reason", "two"]),
            args(&["set", "--cycles", "1"]),
            args(&["set", "extra"]),
            args(&["clear", "now"]),
        ] {
            env::set_var("TEHAS_HOME", "/nonexistent-tehas-home");
            assert_eq!(pause(&bad), 64, "pause {bad:?} was taken");
            env::remove_var("TEHAS_HOME");
        }
        // The reasons a set takes, including none at all.
        assert_eq!(pause_args(&args(&[])).unwrap(), "");
        assert_eq!(pause_args(&args(&["--reason", "one"])).unwrap(), "one");
        assert_eq!(pause_args(&args(&["--reason", ""])).unwrap(), "");
    }

    #[test]
    fn the_usage_names_the_verbs_the_pause_flag_takes_and_no_other() {
        // The verbs of the pause flag, as the operator of a stale host or a
        // carried-over script is told them before the 64: the two which are
        // there, and neither of the two which are not. The limit's line keeps
        // its own verbs, `read` among them, which console/tests/limit.rs runs.
        assert_eq!(
            PAUSE_USAGE,
            "       console flags pause set [--reason TEXT] | clear"
        );
        assert!(!PAUSE_USAGE.contains("show"));
        assert!(!PAUSE_USAGE.contains("read"));
    }

    #[test]
    fn the_name_of_the_account_which_writes_the_flag_is_its_name() {
        // The same name id -un prints. An empty name would be a flag which
        // says nothing about who wrote it.
        assert!(!account::name().is_empty());
        let out = std::process::Command::new("id")
            .arg("-un")
            .output()
            .expect("id -un");
        assert_eq!(account::name(), String::from_utf8_lossy(&out.stdout).trim());
    }

    #[test]
    fn only_a_regular_file_of_this_run_is_ours_to_remove() {
        let home = dir("owned-regular");
        fs::create_dir_all(Home::new(home.path()).state_dir()).unwrap();
        let mine = Home::new(home.path()).state(State::Pause);
        fs::write(&mine, "{}").unwrap();
        assert!(owned_regular(&mine, account::uid()));
        // A directory is no flag to take off, a link is not ours whoever
        // owns what it points at, and an absent name is nothing to remove.
        assert!(!owned_regular(
            &Home::new(home.path()).state_dir(),
            account::uid()
        ));
        symlink(&mine, Home::new(home.path()).state_dir().join("link.json")).unwrap();
        assert!(!owned_regular(
            &Home::new(home.path()).state_dir().join("link.json"),
            account::uid()
        ));
        assert!(!owned_regular(
            &Home::new(home.path()).state_dir().join("gone.json"),
            account::uid()
        ));
        // A file of another account is not ours to remove. The file is made
        // here, so the case does not lean on what the host happens to hold.
        // Only a run under uid 0 can hand a file to another account, and the
        // rest are told the case was skipped instead of left to pass unseen.
        let other = Home::new(home.path()).state_dir().join("other.json");
        fs::write(&other, "x").unwrap();
        if account::uid() == Some(0) {
            let said = std::process::Command::new("chown")
                .arg("1")
                .arg(&other)
                .output()
                .expect("chown");
            assert!(
                said.status.success(),
                "could not give {} to uid 1: {}",
                other.display(),
                String::from_utf8_lossy(&said.stderr)
            );
            assert!(
                !owned_regular(&other, Some(0)),
                "{} is not ours",
                other.display()
            );
        } else {
            eprintln!("skipped: only a run under uid 0 can give a file to another account");
        }
        // The same refusal every run can answer: an uid which is not the
        // owner's owns nothing, and no uid owns anything.
        if let Some(me) = account::uid() {
            assert!(!owned_regular(&mine, Some(me.wrapping_add(1))));
        }
        assert!(!owned_regular(&mine, None));
    }
}
