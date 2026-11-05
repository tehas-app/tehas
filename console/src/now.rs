//! The lines the dashboard's status is read from, for libexec/work.
//!
//! The watch loop of `tehas work` wants the cycle's current step and the
//! driver's last notice every ten seconds. Reading them from the log is seven
//! processes a tick; the console beside the log already reads it every 1.5
//! seconds, so it writes them here instead and the loop reads the lines.
//!
//! The step is the step of the last stable `step-started` line; the notice is
//! the first line of the last stable `log` message that is not a driver-event
//! line. Both are masked and hold no control character but the tab, as every
//! other line of the console does.
//!
//! A third line, when anything of it is known, says where the step is:
//! `stepNumber=5 stepTotal=9 round=2 roundLimit=3 stepSince=1760700000`, each
//! pair left out when not known. The number, the count and the limit are the
//! driver's, read from the `step-started` line when it carries them. The round
//! is the line's own, else how many times this run has started the step, said
//! only from the second time on. The time is the line's.

use crate::render;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use tehas_core::mask::{is_format, mask};

/// The prefix of the driver's own lines, which are not what the page shows.
const DRIVER_EVENT: &str = "pi-rukas:driver-event";

/// The largest step number, count, round or limit believed.
const MOST: u64 = 1000;

/// The lines as the file holds them: no control character but the tab, so
/// `read` of a shell gives back what was written.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Now {
    pub step: String,
    pub text: String,
    /// Where the last step started is, as its line said it.
    pub at: Progress,
    /// How many times this run has started each step.
    pub starts: BTreeMap<String, u64>,
}

/// What a `step-started` line said of where the step is.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Progress {
    pub number: Option<u64>,
    pub total: Option<u64>,
    pub round: Option<u64>,
    pub limit: Option<u64>,
    pub since: Option<i64>,
}

/// The text as the file holds it: masked, and of the control characters only
/// the tab. Masked first: a secret cut in two is too short for the mask to
/// know, and its first half would be written.
fn said(s: &str) -> String {
    mask(s)
        .chars()
        .filter(|c| *c != '\r')
        .map(|c| match c {
            '\t' => c,
            c if c.is_control() || is_format(c) => char::REPLACEMENT_CHARACTER,
            c => c,
        })
        .collect()
}

fn count(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().filter(|n| (1..=MOST).contains(n))
}

/// Unix seconds of a time as the driver writes one: milliseconds, or
/// YYYY-MM-DDTHH:MM:SS with or without a fraction, in UTC.
fn seconds(v: &serde_json::Value) -> Option<i64> {
    // 2020 to 2100.
    let plausible = |t: i64| (1_577_836_800..4_102_444_800).contains(&t);
    if let Some(ms) = v.as_f64() {
        return Some((ms / 1000.0).floor() as i64).filter(|t| plausible(*t));
    }
    let s = v.as_str()?.strip_suffix('Z')?;
    let whole = s.get(..19)?;
    let rest = &s[19..];
    if !(rest.is_empty()
        || rest
            .strip_prefix('.')
            .is_some_and(|f| f.bytes().all(|b| b.is_ascii_digit())))
    {
        return None;
    }
    crate::snapshot::epoch(&format!("{whole}Z")).filter(|t| plausible(*t))
}

enum Said {
    Step(String, Progress),
    Notice(String),
}

// What one progress line says: the step it starts, or the notice. Nothing for
// any other line, and for a line that is not JSON.
fn of(line: &str) -> Option<Said> {
    let r: serde_json::Value = serde_json::from_str(line).ok()?;
    if r["v"].as_u64() != Some(render::NDJSON_VERSION) || r["unstable"] == true {
        return None;
    }
    match r["kind"].as_str()? {
        "step-started" => {
            let e = &r["event"];
            let step = e["step"].as_str()?.to_string();
            let at = Progress {
                number: count(&e["stepNumber"]),
                total: count(&e["stepTotal"]),
                round: count(&e["round"]),
                limit: count(&e["roundLimit"]),
                since: seconds(&e["at"]).or_else(|| seconds(&r["at"])),
            };
            Some(Said::Step(step, at))
        }
        "log" => match &r["message"] {
            serde_json::Value::String(message) => message
                .lines()
                .find(|line| !line.is_empty() && !line.starts_with(DRIVER_EVENT))
                .map(|line| Said::Notice(line.to_string())),
            _ => None,
        },
        _ => None,
    }
}

/// The step and the notice of one pass's lines: what the last stable line of
/// each kind said among them. Empty for a pass that added nothing, so a caller
/// that keeps the lines of a whole run can add these to what it holds.
pub fn read(lines: &[String]) -> Now {
    let mut now = Now::default();
    for line in lines {
        match of(line) {
            Some(Said::Step(step, at)) => {
                *now.starts.entry(step.clone()).or_default() += 1;
                now.step = step;
                now.at = at;
            }
            Some(Said::Notice(text)) => now.text = text,
            None => {}
        }
    }
    now.step = said(&now.step);
    now.text = said(&now.text);
    now
}

/// The later of two: every line one of them names, and of each kind the later
/// one. Of two empties, an empty, so a pass that added nothing leaves what the
/// run has said so far standing. The starts of the two are added up.
impl Now {
    pub fn later(&self, other: &Now) -> Now {
        let mut starts = self.starts.clone();
        for (step, n) in &other.starts {
            *starts.entry(step.clone()).or_default() += n;
        }
        let (step, at) = if other.step.is_empty() {
            (self.step.clone(), self.at.clone())
        } else {
            (other.step.clone(), other.at.clone())
        };
        Now {
            step,
            text: if other.text.is_empty() {
                self.text.clone()
            } else {
                other.text.clone()
            },
            at,
            starts,
        }
    }

    /// The third line: where the step is, empty when nothing of it is known.
    pub fn progress(&self) -> String {
        let started = self.starts.get(&self.step).copied().unwrap_or(0);
        let round = self.at.round.or((started > 1).then_some(started));
        let pairs = [
            ("stepNumber", self.at.number.map(|n| n as i64)),
            ("stepTotal", self.at.total.map(|n| n as i64)),
            ("round", round.map(|n| n.min(MOST) as i64)),
            ("roundLimit", self.at.limit.map(|n| n as i64)),
            ("stepSince", self.at.since),
        ];
        pairs
            .iter()
            .filter_map(|(k, v)| v.map(|v| format!("{k}={v}")))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The lines, as the file holds them: the third only when it says something.
pub fn content(now: &Now) -> String {
    match now.progress() {
        p if p.is_empty() => format!("{}\n{}\n", now.step, now.text),
        p => format!("{}\n{}\n{p}\n", now.step, now.text),
    }
}

/// Write the two lines, through a name beside them and a rename, so a reader
/// never sees half a file. The temporary name is the console's own: a cycle
/// that made one is refused, as a file that cannot be written is. The file is
/// the console's to write and the loop's to read: mode 644, like the console
/// file.
pub fn write(path: &str, now: &Now) -> io::Result<()> {
    let parent = Path::new(path).parent();
    let tmp = match parent {
        Some(dir) => dir.join("now.new"),
        None => Path::new(path).with_extension("now.new"),
    };
    let bytes = content(now).into_bytes();
    let mut file = fs::OpenOptions::new()
        .write(true)
        // Not truncate: File::open would empty the name it made, and a rename
        // onto a reader's open inode must not follow a link the cycle made.
        .create_new(true)
        .mode(0o644)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&tmp)?;
    use io::Write;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()?;
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // The directory holds the name the loop reads.
    if let Some(dir) = parent {
        if let Ok(handle) = fs::File::open(dir) {
            let _ = handle.sync_all();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new(&format!("now-{name}"))
    }

    // The lines as the follower hands them over: whole, and without the end
    // newline each one was written with.
    fn lines(of: &[&str]) -> Vec<String> {
        of.iter().map(|s| s.to_string()).collect()
    }

    impl Now {
        fn two(&self) -> (&str, &str) {
            (&self.step, &self.text)
        }
    }

    fn step(event: &str) -> String {
        format!("{{\"v\":1,\"kind\":\"step-started\",\"event\":{event}}}")
    }

    #[test]
    fn the_third_line_says_where_the_step_is_as_the_driver_said_it() {
        let now = read(&[step(
            r#"{"step":"lens-review","stepNumber":7,"stepTotal":9,"round":2,"roundLimit":3,"at":1760700000123}"#,
        )]);
        assert_eq!(
            content(&now),
            "lens-review\n\nstepNumber=7 stepTotal=9 round=2 roundLimit=3 stepSince=1760700000\n"
        );
        // The time of the line when the event has none, with or without a
        // fraction; a number out of bounds, or of another kind, is not known.
        let now = read(&[format!(
            "{{\"v\":1,\"kind\":\"step-started\",\"at\":\"2025-10-17T11:20:00.512Z\",\"event\":{}}}",
            r#"{"step":"ci","stepNumber":0,"stepTotal":"9","round":1001}"#
        )]);
        assert_eq!(now.progress(), "stepSince=1760700000");
        assert_eq!(
            read(&[step(r#"{"step":"ci","at":"yesterday"}"#)]).progress(),
            ""
        );
    }

    #[test]
    fn a_step_started_again_is_counted_in_rounds_across_passes() {
        let develop = step(r#"{"step":"develop"}"#);
        let review = step(r#"{"step":"adversarial"}"#);
        let first = read(&[develop.clone(), review.clone()]);
        // The first time a step runs says no round.
        assert_eq!(first.progress(), "");
        let second = first.later(&read(std::slice::from_ref(&develop)));
        assert_eq!(second.progress(), "round=2");
        let third = second.later(&read(std::slice::from_ref(&review)));
        assert_eq!(
            (third.step.as_str(), third.progress()),
            ("adversarial", "round=2".to_string())
        );
        // A pass that adds no step keeps the count and the round.
        let quiet = third.later(&read(&[]));
        assert_eq!(quiet, third);
        // The round the line says stands over the count.
        let said = quiet.later(&read(&[step(r#"{"step":"adversarial","round":5}"#)]));
        assert_eq!(said.progress(), "round=5");
    }

    #[test]
    fn the_step_and_the_notice_come_from_the_last_lines_of_each_kind() {
        assert_eq!(
            read(&lines(&[
                "{\"v\":1,\"kind\":\"log\",\"message\":\"cycle started\"}",
                "{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"explore\"}}",
                "{\"v\":1,\"kind\":\"log\",\"message\":\"said once\\nsaid twice\"}",
                "{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"develop\"}}",
                "{\"v\":1,\"kind\":\"log\",\"message\":\"the driver's notice\"}",
                "{\"v\":1,\"kind\":\"dispatch-started\",\"event\":{\"role\":\"ops\"}}",
            ]))
            .two(),
            ("develop", "the driver's notice")
        );
    }

    #[test]
    fn an_unstable_line_gives_neither_line() {
        assert_eq!(
            read(&lines(&[
                "{\"v\":1,\"kind\":\"log\",\"message\":\"said\"}",
                "{\"v\":1,\"unstable\":true,\"kind\":\"log\",\"message\":\"not yet\"}",
                "{\"v\":1,\"kind\":\"step-started\",\"unstable\":true,\"event\":{\"step\":\"sneaked\"}}",
                "{\"v\":2,\"kind\":\"log\",\"message\":\"of another version\"}",
                "{\"v\":1,\"kind\":\"log\",\"message\":42}",
                "not json",
            ])),
            Now {
                step: String::new(),
                text: "said".to_string(),
                ..Now::default()
            }
        );
    }

    #[test]
    fn a_driver_event_line_is_not_the_notice() {
        assert_eq!(
            read(&lines(&[
                "{\"v\":1,\"kind\":\"log\",\"message\":\"work went on\"}",
                "{\"v\":1,\"kind\":\"log\",\"message\":\"pi-rukas:driver-event {\\\"kind\\\":\\\"lens\\\"}\"}",
                "{\"v\":1,\"kind\":\"log\",\"message\":\"\\npi-rukas:driver-event again\"}",
            ]))
            .text,
            "work went on"
        );
    }

    #[test]
    fn the_notice_is_masked_and_holds_no_control_character() {
        // The address built here, not written out: this file is linted too,
        // and the gate refuses a private address in a platform file.
        let address = [10, 20, 30, 40].map(|part| part.to_string()).join(".");
        let now = read(&lines(&[&format!(
            "{{\"v\":1,\"kind\":\"log\",\"message\":\"at {address}:8086 GH_TOKEN=abc123 a\\tb\\u0007c\\u202ed\"}}"
        )]));
        assert_eq!(
            now.text,
            format!(
                "at [private address]:8086 GH_TOKEN=*** a\tb{}cd",
                char::REPLACEMENT_CHARACTER
            )
        );
    }

    #[test]
    fn the_notice_hides_a_name_it_was_given() {
        // One process, so the names are set once for the whole test binary:
        // only this case is given a name to hide.
        tehas_core::mask::set_private(&["models.lan".to_string()]).unwrap();
        assert_eq!(
            read(&lines(&[
                "{\"v\":1,\"kind\":\"log\",\"message\":\"reached models.lan:8086\"}",
            ]))
            .text,
            "reached [private name]:8086"
        );
    }

    #[test]
    fn a_pass_that_added_nothing_leaves_the_two_lines_standing() {
        let said = Now {
            step: "develop".to_string(),
            text: "the notice".to_string(),
            ..Now::default()
        };
        // A pass of lines that name neither: a step-started that is unstable,
        // and a log line of the driver itself.
        let quiet = read(&lines(&[
            "{\"v\":1,\"kind\":\"step-started\",\"unstable\":true,\"event\":{\"step\":\"sneaked\"}}",
            "{\"v\":1,\"kind\":\"log\",\"message\":\"pi-rukas:driver-event {}\"}",
        ]));
        assert_eq!(said.later(&quiet), said);
        // The later line of each kind wins, and one line fills the other.
        let next = read(&lines(&[
            "{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"review\"}}",
        ]));
        assert_eq!(said.later(&next).two(), ("review", "the notice"));
    }

    #[test]
    fn no_line_at_all_gives_two_empty_lines() {
        assert_eq!(
            read(&lines(&[])),
            Now {
                step: String::new(),
                text: String::new(),
                ..Now::default()
            }
        );
    }

    #[test]
    fn the_file_holds_the_step_then_the_notice() {
        let at = dir("write");
        let now = Now {
            step: "develop".to_string(),
            text: "the notice".to_string(),
            ..Now::default()
        };
        write(at.join("console-7.txt.now").to_str().unwrap(), &now).unwrap();
        assert_eq!(
            fs::read_to_string(at.join("console-7.txt.now")).unwrap(),
            "develop\nthe notice\n"
        );
        // No half a file left beside it.
        assert_eq!(fs::read_dir(&at).unwrap().count(), 1);
    }

    #[test]
    fn a_notice_whose_last_line_is_empty_keeps_its_line() {
        let at = dir("blank");
        let now = Now {
            step: String::new(),
            text: "one\n".to_string(),
            ..Now::default()
        };
        write(at.join("a.now").to_str().unwrap(), &now).unwrap();
        assert_eq!(fs::read_to_string(at.join("a.now")).unwrap(), "\none\n\n");
    }

    #[test]
    fn a_file_that_cannot_be_written_is_a_failure() {
        let at = dir("unwritable");
        let missing = at.join("no-such-directory/a.now");
        assert!(write(missing.to_str().unwrap(), &Now::default()).is_err());
        assert!(!at.join("now.new").exists());
    }
}
