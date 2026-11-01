//! The state store, $TEHAS_HOME/libexec/store, called as lib/store.sh calls
//! it, and the two ways of running a command that call needs.

use serde_json::{json, Value};
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::Duration;

/// What a call that did not answer said, as lib/store.sh sets it: the status
/// (1 refused, busy or timeout; 64 usage; 78 unusable), the store's word or
/// ours (timeout, absent, garbled), and the first line, as `said` keeps it for
/// a log: no control bytes and at most 300 bytes.
#[derive(Debug)]
pub struct Refusal {
    pub rc: u8,
    pub word: String,
    pub line: String,
}

/// The two tests `tehas_store_ready` makes: the store an executable file of
/// this home, and its database a plain file and no link. One copy, so a caller
/// cannot forget one; a stat that errors is answered as a binary that is not
/// there, as the shell's `[ -f ]` and `[ -x ]` answered it.
pub fn ready(home: &Path) -> bool {
    let bin = home.join("libexec").join("store");
    let db = home.join("state").join("tehas.db");
    fs::metadata(&bin).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        && fs::symlink_metadata(&db).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}

/// What a failed call sets, as `tehas_store_said` does: the line is for a log,
/// so every control character becomes `?` and it is cut at 300 bytes. Every
/// refusal of `call` goes through here, so a caller prints the line as it
/// stands.
fn refusal(rc: u8, word: &str, line: &str) -> Refusal {
    let cleaned: String = line
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    let mut cut = cleaned.len().min(300);
    while cut > 0 && !cleaned.is_char_boundary(cut) {
        cut -= 1;
    }
    Refusal {
        rc,
        word: word.to_string(),
        line: cleaned[..cut].to_string(),
    }
}

fn code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// Standard output and standard error as one text, and the exit status: 127
/// for a command that could not be started.
pub fn merged(cmd: &mut Command) -> (i32, String) {
    let Ok((mut reader, writer)) = io::pipe() else {
        return (127, String::new());
    };
    let Ok(other) = writer.try_clone() else {
        return (127, String::new());
    };
    cmd.stdout(writer).stderr(other);
    let child = cmd.spawn();
    // The command holds the writers; the read ends only once they are closed.
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let Ok(mut child) = child else {
        return (127, String::new());
    };
    let mut said = Vec::new();
    let _ = reader.read_to_end(&mut said);
    let rc = child.wait().map(code).unwrap_or(127);
    (rc, String::from_utf8_lossy(&said).into_owned())
}

pub fn bounded(kill: &str, limit: &str, program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new("timeout");
    cmd.args(["-k", kill, limit]).arg(program);
    cmd
}

/// One call of the store: its answer, or what it said instead.
pub fn call(home: &Path, args: &[&str]) -> Result<Value, Refusal> {
    let bin = home.join("libexec").join("store");
    let verb = args[0];
    let (rc, said) = merged(
        bounded("2", "20", &bin)
            .args(args)
            .env("TEHAS_HOME", home)
            .stdin(Stdio::null()),
    );
    let no = |rc: u8, word: &str, line: String| Err(refusal(rc, word, &line));
    let said = said.trim_end_matches('\n');
    match rc {
        0 => {
            let v: Value = serde_json::from_str(said).unwrap_or(Value::Null);
            if !said.contains('\n')
                && v["api"] == json!(1)
                && v["ok"] == json!(true)
                && v["verb"] == json!(verb)
            {
                Ok(v)
            } else {
                no(
                    78,
                    "garbled",
                    format!("store: garbled: {verb} answered 0 without its answer"),
                )
            }
        }
        124 | 137 => no(
            1,
            "timeout",
            format!("store: timeout: {verb} did not answer in 20 s"),
        ),
        126 | 127 => no(
            78,
            "absent",
            format!("store: absent: {} could not be run", bin.display()),
        ),
        1 | 64 | 78 => {
            let first = said.split('\n').next().unwrap_or("");
            let word = first
                .strip_prefix("store: ")
                .map(|r| r.split([':', ' ']).next().unwrap_or("").to_string())
                .filter(|w| {
                    w.bytes().next().is_some_and(|c| c.is_ascii_lowercase())
                        && w.bytes()
                            .all(|c| c.is_ascii_lowercase() || c == b'_' || c == b'-')
                })
                .unwrap_or_default();
            if rc == 1 && word.is_empty() {
                return no(
                    78,
                    "garbled",
                    format!("store: garbled: {verb} refused without a word: {first}"),
                );
            }
            no(rc as u8, &word, first.to_string())
        }
        _ => no(78, "garbled", format!("store: garbled: {verb} exited {rc}")),
    }
}

/// `call` again while the word is busy or timeout, one second apart, at most
/// `tries` calls, as `tehas_store_try` does.
pub fn try_call(home: &Path, tries: u32, args: &[&str]) -> Result<Value, Refusal> {
    let mut made = 1;
    loop {
        match call(home, args) {
            Err(no)
                if no.rc == 1 && made < tries && matches!(no.word.as_str(), "busy" | "timeout") =>
            {
                made += 1;
                sleep(Duration::from_secs(1));
            }
            answer => return answer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Home(PathBuf);

    impl Home {
        /// A home whose store says each line of `said` in turn, the last one
        /// from then on, and counts its calls.
        fn new(name: &str, said: &[&str]) -> Home {
            let home = std::env::temp_dir().join(format!("store-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&home);
            fs::create_dir_all(home.join("libexec")).unwrap();
            let mut script = String::from("#!/bin/sh\necho x >>\"$TEHAS_HOME/calls\"\n");
            script.push_str("case $(wc -l <\"$TEHAS_HOME/calls\") in\n");
            for (i, line) in said.iter().enumerate() {
                let pattern = if i + 1 == said.len() {
                    "*"
                } else {
                    &(i + 1).to_string()
                };
                script.push_str(&format!("  *{pattern}) {line} ;;\n").replace("**", "*"));
            }
            script.push_str("esac\n");
            let bin = home.join("libexec/store");
            fs::write(&bin, script).unwrap();
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
            Home(home)
        }

        fn calls(&self) -> usize {
            fs::read_to_string(self.0.join("calls"))
                .unwrap_or_default()
                .lines()
                .count()
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const BUSY: &str = "echo 'store: busy #5: locked'; exit 1";
    const ANSWER: &str = r#"echo '{"api":1,"ok":true,"verb":"park-retry"}'"#;

    #[test]
    fn a_busy_store_is_asked_again() {
        let home = Home::new("again", &[BUSY, ANSWER]);
        assert!(try_call(&home.0, 3, &["park-retry", "5"]).is_ok());
        assert_eq!(home.calls(), 2);
    }

    #[test]
    fn the_tries_are_a_ceiling_and_the_last_refusal_is_the_answer() {
        let home = Home::new("ceiling", &[BUSY]);
        let no = try_call(&home.0, 3, &["park-retry", "5"]).unwrap_err();
        assert_eq!((no.rc, no.word.as_str()), (1, "busy"));
        assert_eq!(home.calls(), 3);
    }

    #[test]
    fn another_refusal_is_not_asked_again() {
        let home = Home::new("refused", &["echo 'store: conflict #5: no'; exit 1"]);
        let no = try_call(&home.0, 3, &["park-retry", "5"]).unwrap_err();
        assert_eq!(no.word, "conflict");
        assert_eq!(home.calls(), 1);
    }
}
