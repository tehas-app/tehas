//! The flags one instance keeps under `state/`: the limit `tehas limit` sets
//! and the pause `tehas pause` sets. Reading, validating, writing and printing
//! them, for `libexec/limit`, `libexec/pause` and `libexec/resume`, so that
//! every command which honours a flag reads it through one Rust function.
//!
//!   console flags limit show [--json]
//!   console flags limit set --cycles N [--plan on|off]
//!       [--for SECONDS | --until TIME] [--reason TEXT] [--json]
//!   console flags limit clear [--json]
//!   console flags limit read [--now <epoch seconds>] [--line]
//!
//!   console flags pause show [--json]
//!   console flags pause set [--reason TEXT]
//!   console flags pause clear
//!   console flags pause read [--line]
//!
//! The home is `TEHAS_HOME`, or `$HOME/tehas` with no `TEHAS_HOME`, as
//! `lib/config.sh` resolves it. The file is `$TEHAS_HOME/state/limit.json`,
//! mode 0600, holding `{v: 1, cycles, plan, until, reason}` written whole:
//!
//! - `cycles`: the most cycles running at once, a whole number 0 to 9999
//! - `plan`: false holds a new planning turn and an improvement run
//! - `until`: when it expires, `2026-10-17T14:05:00Z`, or null while it lasts
//!   until cleared
//! - `reason`: one line of at most 200 characters, shown as the hold's reason
//!
//! `show` prints the limit as it then is, in the words `tehas status` prints,
//! or with `--json` the object `tehas limit --json` prints:
//!
//!   limit     1 cycle at once, planning off, until 2026-10-17T14:05:00Z: held for alpha
//!   {"api":1,"state":"ok","cycles":1,"plan":false,"until":"...","reason":"..."}
//!
//! `read` prints the same reading as one object with no `api`: the answer
//! `lib/limit.sh` gives the dispatcher, `tehasbot improve` and `tehasbot
//! health`, and the object `tehas status --json` publishes, so it keeps
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
//! a program which sets it on every poll changes nothing. A limit is written
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
//! `show` prints the flag as it then is, in the words `tehas status` prints,
//! or with `--json` the object `tehas status --json` publishes for the switch:
//!
//!   pause     flag, written at 2026-10-17T14:05:00Z: waiting for the gate
//!   {"api":1,"state":"ok","by":"boss","at":"...","reason":"..."}
//!
//! `read` prints the same reading as one object with no `api`, so the readers
//! which share the file share this answer too; with `--line` it prints the
//! words instead. `state` is `none` when no file is there and `ok` when one
//! holds a flag, and `unreadable` when something is there which is no flag -
//! a file no reader can make sense of, which every reader counts as paused.
//! A command which is told the file is no flag says so in the words its
//! readers have always used for one.
//!
//! `set` writes the flag with the account of this run and the moment of the
//! write, and `clear` takes it off; `set` on a host already paused says what
//! already is and leaves the flag alone. A flag is written through
//! `pause.json.new`, mode 0600, and put in place by a rename, so what is there
//! is always a whole flag; `clear` takes that name away too, and takes away
//! either name only when it holds a regular file of this account, so it never
//! follows a link someone left under one of them.
//!
//! Exit: 0 shown, set or cleared; 1 a file could not be written or removed, or
//! a name of the flag is not this account's to remove; 64 a bad argument, which
//! writes nothing.

use serde_json::Value;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tehas_core::time;

/// The clock a run reads: the host's, or the whole number of seconds in
/// `TEHAS_FLAGS_TEST_NOW` when that names one, which is how a shell test reads
/// a limit, or sets one, at a moment of its own. A value which is no whole
/// number stops here rather than being read as now, as the store does with its
/// own.
fn now() -> i64 {
    match env::var("TEHAS_FLAGS_TEST_NOW") {
        Ok(s) => s.parse().expect("TEHAS_FLAGS_TEST_NOW is not a Unix time"),
        Err(_) => tehas_core::time::now(),
    }
}

/// The longest reason the flag holds.
const REASON: usize = 200;
/// The largest limit a person can set, and the widest `--for` in seconds: the
/// bounds `tehas limit` has always checked its arguments against.
const MAX_CYCLES: u64 = 9999;
const MAX_FOR: u64 = 999_999_999;

/// The longest prefix of the text of a value which jq's strtof reads as a
/// number: an optional sign, digits, a point and digits, an exponent, and
/// nothing more. jq stops at the first byte which cannot belong to the number
/// and reads what came before, so `1x` is the number 1 and `1 ` is the
/// number 1 with a space nobody asked about after it.
fn leading_number(text: &str) -> Option<&str> {
    let b = text.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+') | Some(b'-')) {
        i += 1;
    }
    let mut digits = false;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        digits = true;
        i += 1;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            digits = true;
            i += 1;
        }
    }
    if matches!(b.get(i), Some(b'e') | Some(b'E')) {
        let save = i;
        i += 1;
        if matches!(b.get(i), Some(b'+') | Some(b'-')) {
            i += 1;
        }
        let mut power = false;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            power = true;
            i += 1;
        }
        if !power {
            i = save;
        }
    }
    digits.then(|| &text[..i])
}

/// The number of a document, printed as jq prints the number of that text.
/// A whole number of digits is kept byte for byte, so the limit `tehas limit`
/// wrote comes back as it went in. Anything else became a double on its way
/// The number of a document, printed the way jq prints a number which came
/// out of that text. An integer of digits is kept byte for byte, so the limit
/// `tehas limit` wrote comes back exactly as it went in. Anything else jq
/// holds as a double and prints from the shortest digits which read back as
/// the value: a document which wrote the point keeps the point, so `1.0` and
/// `0.0` come back with it, and a document which wrote none prints the few
/// digits and the exponent which carries the rest. Everything which reads as
/// nothing prints as `0`, the way jq has always printed the number.
fn number(text: &str) -> String {
    let body = match leading_number(text) {
        Some(body) => body,
        None => return text.to_string(),
    };
    // The number of the document comes back as jq printed it from that
    // document: the bytes of the number, kept whole, so the limit which
    // `tehas limit` wrote is read again exactly as it went in — `1` as `1`,
    // `1.0` as `1.0`, `-0.0` as `-0.0`, and a whole number of digits without
    // the point jq would have given a value it held as a double.
    if number_body(body) {
        return body.to_string();
    }
    // Only a number the document did not write whole, such as one with the
    // digits of a whole number behind an exponent, is rebuilt from the value
    // jq holds: the fewest digits which read back, and the exponent which
    // carries the rest.
    let f: f64 = body.parse().unwrap_or(f64::NAN);
    if !f.is_finite() {
        return format!("{}1E+309", if f < 0.0 { "-" } else { "" });
    }
    let digits = format!("{f:e}");
    let (mant, power) = digits.split_once('e').unwrap_or((digits.as_str(), "0"));
    let power: i32 = power.parse().unwrap_or(0);
    if power == 0 {
        return if f == f.trunc() {
            format!("{f:.1}")
        } else {
            format!("{f}")
        };
    }
    let mant = mant.trim_end_matches('0').trim_end_matches('.');
    format!("{mant}E{}{power}", if power < 0 { "" } else { "+" })
}

/// Whether `body` is the whole of a number of the document: nothing else
/// stands behind it but white, which is what jq's reader prints as written.
fn number_body(body: &str) -> bool {
    let text = body.trim_start();
    if text.contains(['e', 'E']) {
        return false;
    }
    let mut b = text.as_bytes();
    b = b.strip_prefix(b"-").unwrap_or(b);
    !b.is_empty() && b.iter().all(|c| c.is_ascii_digit() || *c == b'.')
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

    fn said(&self) -> String {
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
    /// `lib/limit.sh` printed, which `tehas status --json` republishes.
    fn text(&self) -> String {
        let mut parts = vec![format!("\"state\":\"{}\"", self.state)];
        parts.extend(
            self.fields
                .iter()
                .map(|(name, value)| format!("\"{name}\":{value}")),
        );
        format!("{{{}}}", parts.join(","))
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

fn usage(why: &str) -> u8 {
    eprintln!("console flags: {why}");
    eprintln!("usage: console flags limit show [--json] | set --cycles N [--plan on|off] [--for SECONDS | --until TIME] [--reason TEXT] | clear | read [--now <epoch seconds>]");
    eprintln!(
        "       console flags pause show [--json] | set [--reason TEXT] | clear | read [--line]"
    );
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
    let file = here.join("state/limit.json");
    match verb {
        // --json prints the object tehas limit --json promises, api first;
        // read prints the same reading without it, which is what tehas status
        // --json publishes.
        // show prints the words tehas limit prints, or the object with api
        // first when asked for --json.
        "show" => {
            let mut words = false;
            for arg in rest {
                if arg != "--json" || words {
                    return usage("--json, once, after show");
                }
                words = true;
            }
            let out = read(&file, now());
            println!("{}", if words { api(out) } else { out.said() });
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
            println!("{}", if line { out.said() } else { out.text() });
            0
        }
        // --json prints the object with api first, the answer tehas limit
        // --json has always printed; set and clear print the limit as it then
        // is, or that object when asked for it.
        "set" => match set_args(rest) {
            Ok(want) => set(&file, &want),
            Err(bad) => usage(bad),
        },
        "clear" => {
            if rest.len() > 1 || rest.first().is_some_and(|a| a != "--json") {
                return usage("--json, once, after clear");
            }
            clear(&file, !rest.is_empty())
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
        None => return usage("show, set, clear or read is wanted"),
    };
    let file = here.join("state/pause.json");
    match verb {
        // show prints the words tehas pause prints, or the object with api
        // first when asked for --json.
        "show" => {
            let mut words = false;
            for arg in rest {
                if arg != "--json" || words {
                    return usage("--json, once, after show");
                }
                words = true;
            }
            let out = read_pause(&file);
            println!("{}", if words { api(out) } else { out.said() });
            0
        }
        // read prints the object every command which honours the flag reads,
        // with no api, and the words of the reading with --line.
        "read" => {
            let mut line = false;
            for arg in rest {
                if arg != "--line" || line {
                    return usage("--line, once, after read");
                }
                line = true;
            }
            let out = read_pause(&file);
            println!("{}", if line { out.said() } else { out.text() });
            0
        }
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
            pause_clear(&file)
        }
        _ => usage("show, set, clear or read, not a word this command knows"),
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

/// The account of this run: the real uid, and the name passwd gives for it.
fn uid() -> u32 {
    // SAFETY: getuid takes no argument and has no preconditions.
    unsafe { libc::getuid() }
}

/// The account which wrote the flag, as `id -un` names it: the real uid, the
/// name in passwd for it, and nothing for an account passwd does not name.
fn account() -> String {
    let entry = unsafe { libc::getpwuid(uid()) };
    if entry.is_null() {
        return String::new();
    }
    let name = unsafe { std::ffi::CStr::from_ptr((*entry).pw_name) };
    name.to_string_lossy().into_owned()
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
            println!("{}", pause_kept(&of("at"), &of("by"), &of("reason"), file));
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
    let (by, at) = (account(), time::iso(now()));
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
    let tmp = file.with_extension("json.new");
    let _ = fs::remove_file(&tmp);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(doc.as_bytes()))
        .and_then(|()| fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)))
        .and_then(|()| fs::rename(&tmp, file));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        eprintln!("tehas pause: could not write {}", file.display());
        return 1;
    }
    println!(
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
/// command from saying the host resumed.
fn pause_clear(file: &Path) -> u8 {
    let tmp = file.with_extension("json.new");
    let there = file.symlink_metadata().is_ok() || tmp.symlink_metadata().is_ok();
    // A name is ours to remove only when it holds a regular file of this run.
    // A symlink is not: the command that follows it would be this one, and the
    // link stays where whoever made it left it. The flag path reads through a
    // symlink, so the same rule holds there.
    for name in [file, &tmp] {
        if name.symlink_metadata().is_ok() && !owned_regular(name) {
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
    println!(
        "{}",
        if there {
            "resumed: the pause flag is gone"
        } else {
            "no pause flag on this host"
        }
    );
    0
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
    match fs::symlink_metadata(file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Reading::of_pause("none"),
        Err(_) => Reading::of_pause("unreadable"),
        Ok(_) => match fs::read(file) {
            Ok(bytes) => read_pause_bytes(&bytes),
            // A directory has no bytes to read: it is no flag, and the command
            // which asks says so.
            Err(_) => Reading::of_pause("unreadable"),
        },
    }
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

/// The reading of the bytes of a pause flag, from the text of the file: every
/// check of tehas_pause_read, one for one. An object whose `at` and `reason`
/// are strings is a flag; anything else is no flag. `by` may be anything and
/// reads as nobody, and nothing which reaches a page publishes it.
fn read_pause_bytes(bytes: &[u8]) -> Reading {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Reading::of_pause("unreadable"),
    };
    let keys = match Skip::new(bytes).object() {
        Some(keys) => keys,
        None => return Reading::of_pause("unreadable"),
    };
    let held = |want: &str| {
        keys.iter()
            .find(|(name, _)| name == want)
            .map(|(_, at)| &text[at.0..at.1])
    };
    // The checks of tehas_pause_read, one for one: an object whose at and
    // reason are strings. by may be anything and reads as nobody.
    let at = match held("at").and_then(quoted) {
        Some(at) => at,
        None => return Reading::of_pause("unreadable"),
    };
    let reason = match held("reason").and_then(quoted) {
        Some(reason) => oneline(&reason),
        None => return Reading::of_pause("unreadable"),
    };
    let by = held("by").and_then(quoted).unwrap_or_default();
    let mut out = Reading::of_pause("ok");
    out.line = format!("pause     {}", pause_line(&at, &reason));
    out.fields
        .push(("by".into(), Value::String(by).to_string()));
    out.fields
        .push(("at".into(), Value::String(at).to_string()));
    if !reason.is_empty() {
        out.fields
            .push(("reason".into(), Value::String(reason).to_string()));
    }
    out
}

/// What `tehas limit set` was asked for, in the words it was asked in.
#[derive(Default)]
struct Wanted {
    cycles: Option<String>,
    plan: Option<String>,
    for_: Option<String>,
    until: Option<String>,
    reason: Option<String>,
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
            "--json" if !want.json => want.json = true,
            _ => return Err("an option set does not know"),
        }
        i += if rest[i] == "--json" { 1 } else { 2 };
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
    let printed = out.text();
    format!("{{\"api\":1,{}", &printed[1..])
}

fn set(file: &Path, want: &Wanted) -> u8 {
    let shown = |out: Reading| {
        if want.json {
            api(out)
        } else {
            out.said()
        }
    };
    let cycles = want.cycles.clone().unwrap_or_else(|| String::from("0"));
    let until = match (want.for_.as_deref(), want.until.as_deref()) {
        (Some(secs), _) => {
            let secs: u64 = secs.parse().unwrap_or(0);
            time::iso(now().saturating_add(secs as i64))
        }
        (None, Some(u)) => u.to_string(),
        (None, None) => String::new(),
    };
    // The three values are what a JSON document holds them as: a boolean, a
    // string or null, and a string with its escapes.
    let held = |text: &str| Value::String(text.to_string()).to_string();
    let doc = format!(
        "{{\"v\":1,\"cycles\":{cycles},\"plan\":{},\"until\":{},\"reason\":{}}}\n",
        want.plan.as_deref() != Some("off"),
        if until.is_empty() {
            "null".to_string()
        } else {
            held(&until)
        },
        held(&oneline(want.reason.as_deref().unwrap_or("")).to_string()),
    );
    let dir = match file.parent() {
        Some(d) => d,
        None => {
            eprintln!("tehas limit: {} holds no directory", file.display());
            return 1;
        }
    };
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("tehas limit: could not create {}: {e}", dir.display());
        return 1;
    }
    // The name is the one name, so a leftover of a run killed mid-write, or
    // something planted in state/ under it, is removed first and the new file
    // made from nothing at mode 0600: what lands is this document, whatever
    // was there before.
    let tmp = file.with_extension("json.new");
    let _ = fs::remove_file(&tmp);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(doc.as_bytes()))
        .and_then(|()| fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)))
        .and_then(|()| fs::rename(&tmp, file));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        eprintln!("tehas limit: could not write {}", file.display());
        return 1;
    }
    println!("{}", shown(read(file, now())));
    0
}

fn clear(file: &Path, json: bool) -> u8 {
    if file.symlink_metadata().is_ok() {
        if let Err(e) = fs::remove_file(file) {
            eprintln!("tehas limit: could not remove {}: {e}", file.display());
            return 1;
        }
    }
    let out = read(file, now());
    println!("{}", if json { api(out) } else { out.said() });
    0
}

/// tehas_limit_read: the limit as every command which honours it reads it.
/// A file which is not there is no limit; anything else which is not a limit
/// of version 1 is unreadable, never none, so a reader holds every start
/// rather than finding no limit where a file is.
pub fn read(file: &Path, now: i64) -> Reading {
    match fs::symlink_metadata(file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Reading::of("none"),
        Err(_) => return Reading::of("unreadable"),
        Ok(_) => {}
    }
    match fs::read(file) {
        Ok(bytes) => parse(&bytes, now),
        Err(_) => Reading::of("unreadable"),
    }
}

/// The reading of the bytes of a file, unreadable when they are no limit of
/// version 1. The checks are jq's select, one for one, including what an
/// absent key reads as: `.until == null` is true of a document with no until
/// at all. jq keeps a number of the file it can print again, so the cycles are
/// printed from the bytes they were written with.
fn parse(bytes: &[u8], now: i64) -> Reading {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Reading::of("unreadable"),
    };
    // The checks of tehas_limit_read, one jq select at a time, over the
    // document it read. What an absent key reads as is part of the check:
    // `.until == null` is true of a document with no until at all.
    let keys = match Skip::new(bytes).object() {
        Some(keys) => keys,
        // jq's reader is more forgiving than a strict one where a number is
        // concerned: it reads the number at the front of a value and stops at
        // the first byte which cannot belong to it, so `01` is the number 1
        // and `1e999` is a number of no use to anyone. The scanner below is
        // as forgiving, so a document which is a document reaches the checks;
        // what is no document at all is no limit.
        None => return Reading::of("unreadable"),
    };
    let held = |want: &str| {
        keys.iter()
            .find(|(name, _)| name == want)
            .map(|(_, at)| &text[at.0..at.1])
    };
    match held("v") {
        Some(value) if leading_number(value).is_some_and(|n| n.parse::<f64>() == Ok(1.0)) => {}
        _ => return Reading::of("unreadable"),
    }
    let cycles = match held("cycles") {
        Some(value) => value,
        None => return Reading::of("unreadable"),
    };
    // The number is there, at least 0, and whole. A number over what tehas
    // limit writes is still a limit and is read as one, because a reader
    // which refused it would hold every start.
    match leading_number(cycles).and_then(|n| n.parse::<f64>().ok()) {
        Some(f) if f >= 0.0 && f.is_finite() && f == f.trunc() => {}
        _ => return Reading::of("unreadable"),
    }
    match held("plan") {
        Some("true") | Some("false") => {}
        _ => return Reading::of("unreadable"),
    }
    let until = match held("until") {
        None | Some("null") => None,
        Some(value) => match quoted(value) {
            Some(s) => Some(s),
            None => return Reading::of("unreadable"),
        },
    };
    let expired = match &until {
        Some(s) => match epoch(s) {
            Some(at) => at <= now,
            None => return Reading::of("unreadable"),
        },
        None => false,
    };
    let reason = match held("reason") {
        None | Some("null") => String::new(),
        Some(value) => match quoted(value) {
            Some(s) => oneline(&s),
            None => return Reading::of("unreadable"),
        },
    };
    let mut out = Reading::of(if expired { "expired" } else { "ok" });
    out.fields.push(("cycles".into(), number(cycles)));
    out.fields
        .push(("plan".into(), held("plan").unwrap_or("null").to_string()));
    if let Some(s) = until {
        out.fields
            .push(("until".into(), Value::String(s).to_string()));
    }
    if !reason.is_empty() {
        out.fields
            .push(("reason".into(), Value::String(reason).to_string()));
    }
    out
}

/// Whether a path is a regular file of this run. A symlink is never ours: the
/// name is ours to remove only when it holds our own regular file, not one
/// planted under it or left there by a container which ran as another account.
fn owned_regular(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type().is_file() && meta.uid() == uid(),
        Err(_) => false,
    }
}

/// The text of a quoted string, with its escapes as they came. Only a reason
/// and an until are wanted, and both are text.
fn quoted(value: &str) -> Option<String> {
    let unescaped = value.strip_prefix('"')?.strip_suffix('"')?;
    // The escapes of a JSON string become the characters the file meant, the
    // same way jq's parser does it, so that the reason of a limit is the
    // reason its author wrote.
    let mut out = String::with_capacity(unescaped.len());
    let mut chars = unescaped.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let mut code = String::new();
                code.extend(chars.by_ref().take(4));
                let at = u32::from_str_radix(&code, 16).ok()?;
                out.push(char::from_u32(at).unwrap_or('\u{fffd}'));
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The hand which walks the bytes of a document. `object` is jq's own first
/// question: is this a document at all, and what does each of its top-level
/// keys hold? The bytes of every value are kept as the document holds them, so
/// a number prints again the way it came in, and a name buried in a reason is
/// never taken for a second reading of the limit.
struct Skip<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Skip<'a> {
    fn new(bytes: &'a [u8]) -> Skip<'a> {
        Skip { bytes, at: 0 }
    }

    fn object(mut self) -> Option<Vec<(String, (usize, usize))>> {
        self.white();
        self.open(b'{')?;
        self.white();
        let mut keys = Vec::new();
        if self.peek() == Some(b'}') {
            return Some(keys);
        }
        loop {
            self.white();
            let name = self.name()?;
            self.white();
            self.open(b':')?;
            self.white();
            let from = self.at;
            self.value()?;
            keys.push((name, (from, self.at)));
            self.white();
            if self.peek() == Some(b',') {
                self.at += 1;
                continue;
            }
            self.open(b'}')?;
            return Some(keys);
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// One byte of structure, or nothing for the reader to refuse.
    fn open(&mut self, b: u8) -> Option<()> {
        (self.peek() == Some(b)).then(|| self.at += 1)
    }

    /// jq's whitespace: the space, the tab, the newline and the return.
    fn white(&mut self) {
        while matches!(
            self.peek(),
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            self.at += 1;
        }
    }

    /// The text of a key, its escapes become what they stand for, so a key
    /// written `\u0063ycles` is the cycles of the file.
    fn name(&mut self) -> Option<String> {
        let from = self.at;
        self.string()?;
        let raw = std::str::from_utf8(&self.bytes[from..self.at]).ok()?;
        quoted(raw)
    }

    /// A string, with its escapes kept as they are: the bytes of a reason are
    /// the bytes of the file, and the escapes are let go only when the text
    /// of the reason is wanted.
    fn string(&mut self) -> Option<()> {
        self.open(b'"')?;
        while let Some(c) = self.peek() {
            match c {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return Some(());
                }
                _ => self.at += 1,
            }
        }
        None
    }

    /// The bytes of any value: a literal, a string, a number, an array or an
    /// object, each entered only as far as it reaches.
    fn value(&mut self) -> Option<()> {
        match self.peek()? {
            b'"' => self.string(),
            b'[' | b'{' => {
                let close = if self.peek() == Some(b'[') {
                    b']'
                } else {
                    b'}'
                };
                self.at += 1;
                self.white();
                if self.peek() == Some(close) {
                    self.at += 1;
                    return Some(());
                }
                let nested = self.peek() == Some(b'{');
                loop {
                    if nested {
                        self.white();
                        self.string()?;
                        self.white();
                        self.open(b':')?;
                        self.white();
                    }
                    self.value()?;
                    self.white();
                    if self.peek() == Some(b',') {
                        self.at += 1;
                        continue;
                    }
                    return self.open(close);
                }
            }
            b't' => self.lit(b"true"),
            b'f' => self.lit(b"false"),
            b'n' => self.lit(b"null"),
            _ => {
                let from = self.at;
                while matches!(self.peek(), Some(c) if matches!(c, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
                {
                    self.at += 1;
                }
                (self.at > from).then_some(())
            }
        }
    }

    fn lit(&mut self, word: &[u8]) -> Option<()> {
        if self.bytes[self.at..].starts_with(word) {
            self.at += word.len();
            return Some(());
        }
        None
    }
}

/// Whether `tehas_pause_oneline` replaces this character with a space: the C0 and C1
/// controls, U+2028 and U+2029. The space itself is not one of them.
fn replaced(c: char) -> bool {
    matches!(c, '\u{1}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}')
}

/// A reason as the health file holds it, the rule of `lib/status.sh` in
/// words: a run of the characters jq replaces becomes one space, a run of
/// spaces stays as it was written, 200 characters is the most, counted as
/// characters and cut before the ends, and the ends go last. The line
/// `tehas status` prints is one fact, so a reason which carried a newline
/// would otherwise print a line which is not there.
fn oneline(s: &str) -> String {
    // The rule of `tehas_pause_oneline` in `lib/status.sh`, which is the rule
    // the shell has always applied to a reason: jq's `gsub` reads its pattern
    // as POSIX basic regex, where the `+` after the class is one of the
    // characters being replaced rather than a repeat, so each of them becomes
    // a space of its own and a run of spaces which were written keeps every
    // character of itself; 200 characters is the most, cut before the ends go,
    // and only the spaces which touch the two ends of what is left go with
    // them.
    let folded: String = s
        .chars()
        .map(|c| if replaced(c) { ' ' } else { c })
        .collect();
    let cut = folded.chars().take(REASON).collect::<String>();
    cut.trim_matches(edges).to_string()
}

fn edges(c: char) -> bool {
    c == ' ' || replaced(c)
}

/// Seconds since the epoch of an ISO 8601 UTC time, as jq's `fromdateiso8601`
/// reads it: the shape is checked byte for byte, and a day out of range is
/// counted on rather than refused, as jq counts it.
pub fn epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|w| w.iter().all(u8::is_ascii_digit));
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
        || !digits(0..4)
        || !digits(5..7)
        || !digits(8..10)
        || !digits(11..13)
        || !digits(14..16)
        || !digits(17..19)
    {
        return None;
    }
    let at = |r: std::ops::Range<usize>| s[r].parse::<i64>().ok();
    let (y, m, d) = (at(0..4)?, at(5..7)?, at(8..10)?);
    let (hh, mm, ss) = (at(11..13)?, at(14..16)?, at(17..19)?);
    if !(1..=12).contains(&m) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    Some(time::days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let d = env::temp_dir().join(format!(
            "tehas-flags-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    // The file of a home, with its state/ directory made.
    fn written(d: &Dir) -> PathBuf {
        fs::create_dir_all(d.0.join("state")).unwrap();
        d.0.join("state/limit.json")
    }

    fn args(given: &[&str]) -> Vec<String> {
        given.iter().map(|s| s.to_string()).collect()
    }

    const NOW: i64 = 1_791_942_400;

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

    fn set_in(d: &Dir, given: &[&str]) -> (u8, String) {
        let file = written(d);
        let rc = set(&file, &set_args(&args(given)).unwrap());
        (rc, read(&file, NOW).text())
    }

    #[test]
    fn a_set_writes_the_file_and_reads_back_as_ok() {
        let d = dir("set");
        let (rc, out) = set_in(&d, &["--cycles", "2"]);
        assert_eq!(rc, 0);
        assert_eq!(out, r#"{"state":"ok","cycles":2,"plan":true}"#);
        let file = written(&d);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "{\"v\":1,\"cycles\":2,\"plan\":true,\"until\":null,\"reason\":\"\"}\n"
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
        set_in(&d, &["--cycles", "1", "--for", "300"]);
        let until = read(&written(&d), NOW).field("until");
        assert_eq!(epoch(&until), Some(now() + 300));
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
        ] {
            assert!(set_args(&bad).is_err(), "{bad:?} was taken");
        }
        // A refused set never reaches the file, so nothing of it is written.
        assert!(!d.0.join("state").exists());
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
            read(&d.0.join("state/limit.json"), NOW).text(),
            r#"{"state":"none"}"#
        );
        let state = d.0.join("state");
        fs::create_dir_all(state.join("limit.json")).unwrap();
        assert_eq!(
            read(&state.join("limit.json"), NOW).text(),
            r#"{"state":"unreadable"}"#
        );
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
    fn a_number_of_the_file_is_printed_as_jq_prints_it() {
        // The reading echoes the number back, and jq prints a whole number of
        // digits byte for byte and everything else as the double it became.
        for (doc, cycles) in [
            ("1", "1"),
            ("0", "0"),
            ("1e0", "1.0"),
            ("0.0", "0.0"),
            ("2.0", "2.0"),
            ("100000000000000000000.0", "100000000000000000000.0"),
            // The text of the file is the text jq prints, point and all.
            ("-0", "-0"),
            ("-0.0", "-0.0"),
            ("1.0", "1.0"),
            ("100.0", "100.0"),
            ("1e2", "1E+2"),
            ("1E+2", "1E+2"),
            ("9999", "9999"),
        ] {
            let text = of(&format!(r#"{{"v":1,"cycles":{doc},"plan":true}}"#));
            assert_eq!(
                text,
                format!(r#"{{"state":"ok","cycles":{cycles},"plan":true}}"#),
                "{doc}"
            );
        }
    }

    #[test]
    fn a_limit_behind_a_link_is_the_file_it_points_at() {
        let d = dir("link");
        let real = d.0.join("elsewhere.json");
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
    fn the_reading_keeps_its_own_field_order_whatever_the_file_s() {
        assert_eq!(
            of(r#"{"reason":"r","plan":false,"cycles":0,"v":1,"until":"2036-01-01T00:00:00Z"}"#),
            r#"{"state":"ok","cycles":0,"plan":false,"until":"2036-01-01T00:00:00Z","reason":"r"}"#
        );
    }

    #[test]
    fn set_refuses_a_directory_in_its_way() {
        let d = dir("dirblock");
        fs::create_dir_all(d.0.join("state/limit.json")).unwrap();
        let file = d.0.join("state/limit.json");
        assert_eq!(set(&file, &set_args(&args(&["--cycles", "1"])).unwrap()), 1);
    }

    #[test]
    fn clear_takes_the_file_off_and_clearing_twice_is_fine() {
        let d = dir("clear");
        set_in(&d, &["--cycles", "1"]);
        let file = written(&d);
        assert_eq!(clear(&file, false), 0);
        assert!(!file.exists());
        assert_eq!(clear(&file, false), 0);
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
        assert_eq!(
            &format!("{{\"api\":1,{}", &text[1..]),
            r#"{"api":1,"state":"ok","cycles":1,"plan":false,"reason":"r"}"#
        );
    }

    #[test]
    fn the_clock_reads_the_test_variable_when_it_names_a_time() {
        env::set_var("TEHAS_FLAGS_TEST_NOW", "1700000000");
        assert_eq!(now(), 1_700_000_000);
        env::remove_var("TEHAS_FLAGS_TEST_NOW");
        assert!(now() > 1_791_942_400);
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

    fn pause_file(d: &Dir) -> PathBuf {
        fs::create_dir_all(d.0.join("state")).unwrap();
        d.0.join("state/pause.json")
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
        let file = d.0.join("state/pause.json");
        assert_eq!(read_pause(&file).text(), r#"{"state":"none"}"#);
        assert_eq!(read_pause(&file).said(), "pause     none");
    }

    #[test]
    fn a_flag_behind_a_link_is_the_file_it_points_at() {
        let d = dir("pause-link");
        let real = d.0.join("elsewhere.json");
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
        assert_eq!(pause_set(&file, "waiting for the gate"), 0);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!(
                "{{\"by\":\"{}\",\"at\":\"{}\",\"reason\":\"waiting for the gate\"}}\n",
                account(),
                time::iso(now()),
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
        assert_eq!(pause_set(&file, ""), 0);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!(
                "{{\"by\":\"{}\",\"at\":\"{}\",\"reason\":\"\"}}\n",
                account(),
                time::iso(now()),
            )
        );
        // The reading of such a flag has no reason, and its line says none.
        assert_eq!(
            read_pause(&file).text(),
            format!(
                "{{\"state\":\"ok\",\"by\":\"{}\",\"at\":\"{}\"}}",
                account(),
                time::iso(now()),
            )
        );
        assert_eq!(
            read_pause(&file).said(),
            format!("pause     flag, written {}", time::iso(now()))
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
        let file = d.0.join("state/pause.json");
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
        let outside = d.0.join("victim");
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, file.with_extension("json.new")).unwrap();
        assert_eq!(pause_clear(&file), 1);
        assert!(outside.is_dir());
        assert!(file.with_extension("json.new").symlink_metadata().is_ok());
        // The flag path reads through a symlink, so taking the link off would
        // be following it: the same rule holds there.
        let d = dir("pause-clear-flag-symlink");
        let file = pause_file(&d);
        let outside = d.0.join("keepme");
        fs::write(&outside, "not a flag").unwrap();
        symlink(&outside, &file).unwrap();
        assert_eq!(pause_clear(&file), 1);
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
        fs::set_permissions(d.0.join("state"), fs::Permissions::from_mode(0o500)).unwrap();
        assert_eq!(pause_clear(&file), 1);
        fs::set_permissions(d.0.join("state"), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(file.exists() || file.with_extension("json.new").exists());
    }

    #[test]
    fn resume_takes_the_flag_and_its_temporary_off() {
        let d = dir("pause-clear");
        let file = pause_file(&d);
        assert_eq!(pause_clear(&file), 0);
        assert_eq!(pause_set(&file, "x"), 0);
        // A leftover of a run killed mid-write goes with the flag, so it
        // cannot outlive it.
        fs::write(
            file.with_extension("json.new"),
            r#"{"by":"b","at":"2026-10-15T10:00:00Z","reason":""}"#,
        )
        .unwrap();
        assert_eq!(pause_clear(&file), 0);
        assert!(!file.exists());
        assert!(!file.with_extension("json.new").exists());
        // Clearing a host which is not paused succeeds and says so.
        assert_eq!(pause_clear(&file), 0);
    }

    #[test]
    fn the_verbs_of_the_pause_flag_check_their_arguments() {
        for bad in [
            args(&[]),
            args(&["bogus"]),
            args(&["show", "--json", "--json"]),
            args(&["show", "--json", "extra"]),
            args(&["read", "--line", "--line"]),
            args(&["read", "--json"]),
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
    fn the_name_of_the_account_which_writes_the_flag_is_its_name() {
        // The same name id -un prints: the real uid of the run, and the name
        // passwd gives for it. An empty name would be a flag which says
        // nothing about who wrote it.
        assert!(!account().is_empty());
        let out = std::process::Command::new("id")
            .arg("-un")
            .output()
            .expect("id -un");
        assert_eq!(account(), String::from_utf8_lossy(&out.stdout).trim());
    }
}
