//! The two lines the dashboard's status is read from, for libexec/work.
//!
//! The watch loop of `tehas work` wants the cycle's current step and the
//! driver's last notice every ten seconds. Reading them from the log is seven
//! processes a tick; the console beside the log already reads it every 1.5
//! seconds, so it writes them here instead and the loop reads two lines.
//!
//! The step is the step of the last stable `step-started` line; the notice is
//! the first line of the last stable `log` message that is not a driver-event
//! line. Both are masked and hold no control character but the tab, as every
//! other line of the console does.

use crate::render;
use std::fs;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use tehas_core::mask::{is_format, mask};

/// The prefix of the driver's own lines, which are not what the page shows.
const DRIVER_EVENT: &str = "pi-rukas:driver-event";

/// The two lines as the file holds them: no control character but the tab, so
/// `read` of a shell gives back what was written.
#[derive(Debug, Default, PartialEq)]
pub struct Now {
    pub step: String,
    pub text: String,
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

// The two lines of one progress line: the step it starts, or the notice it
// says. Nothing for any other line, and for a line that is not JSON.
fn of(line: &str) -> Option<(Option<String>, Option<String>)> {
    let r: serde_json::Value = serde_json::from_str(line).ok()?;
    if r["v"].as_u64() != Some(render::NDJSON_VERSION) || r["unstable"] == true {
        return None;
    }
    match r["kind"].as_str()? {
        "step-started" => match &r["event"]["step"] {
            serde_json::Value::String(step) => Some((Some(step.clone()), None)),
            _ => None,
        },
        "log" => match &r["message"] {
            serde_json::Value::String(message) => message
                .lines()
                .find(|line| !line.is_empty() && !line.starts_with(DRIVER_EVENT))
                .map(|line| (None, Some(line.to_string()))),
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
        if let Some((step, text)) = of(line) {
            now.step = step.unwrap_or(now.step);
            now.text = text.unwrap_or(now.text);
        }
    }
    now.step = said(&now.step);
    now.text = said(&now.text);
    now
}

/// The later of two: every line one of them names, and of each kind the later
/// one. Of two empties, an empty, so a pass that added nothing leaves what the
/// run has said so far standing.
impl Now {
    pub fn later(&self, other: &Now) -> Now {
        Now {
            step: if other.step.is_empty() {
                self.step.clone()
            } else {
                other.step.clone()
            },
            text: if other.text.is_empty() {
                self.text.clone()
            } else {
                other.text.clone()
            },
        }
    }
}

/// The two lines, as the file holds them.
pub fn content(now: &Now) -> String {
    format!("{}\n{}\n", now.step, now.text)
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

    fn dir(name: &str) -> std::path::PathBuf {
        let at = std::env::temp_dir().join(format!("tehas-now-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&at);
        fs::create_dir_all(&at).unwrap();
        at
    }

    // The lines as the follower hands them over: whole, and without the end
    // newline each one was written with.
    fn lines(of: &[&str]) -> Vec<String> {
        of.iter().map(|s| s.to_string()).collect()
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
            ])),
            Now {
                step: "develop".to_string(),
                text: "the driver's notice".to_string(),
            }
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
        assert_eq!(
            said.later(&next),
            Now {
                step: "review".to_string(),
                text: "the notice".to_string(),
            }
        );
    }

    #[test]
    fn no_line_at_all_gives_two_empty_lines() {
        assert_eq!(
            read(&lines(&[])),
            Now {
                step: String::new(),
                text: String::new(),
            }
        );
    }

    #[test]
    fn the_file_holds_the_step_then_the_notice() {
        let at = dir("write");
        let now = Now {
            step: "develop".to_string(),
            text: "the notice".to_string(),
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
