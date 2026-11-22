//! What the tests of tehas-cycle's caps and runner verbs share: the verbs run
//! as the built program, as tehas work runs them, and the fixtures of the
//! headless runner in test/fixtures/.
//!
//! test/fixtures/headless-0.12.80 is what the v0.12.80 runner printed in a
//! throwaway container with no model and no token: its --capabilities answer,
//! and the NDJSON progress and --json result of `work 7`, which ended in a
//! handoff. headless-1ca933e is the same from the image of pi-rukas main at
//! 1ca933e (still version 0.12.80), whose --capabilities lists family members
//! and features.failureReason and features.usageByRole, and whose result
//! carries failure and usage.byRole. headless-c5ddc0a is the same from the
//! v0.14.0 release (c5ddc0a), the first on result schema 5: its result is the
//! one tehas reads. The ones before it are kept for the progress lines and for
//! the refusal. One default in each answer from main named a private address
//! and is replaced by a documentation one.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::testdir::TestDir;

pub const CYCLE: &str = env!("CARGO_BIN_EXE_tehas-cycle");

/// A fixture directory of test/fixtures/.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures")
        .join(name)
}

/// v0.12.80's directory.
pub fn fx() -> PathBuf {
    fixture("headless-0.12.80")
}

/// 1ca933e's directory.
pub fn nx() -> PathBuf {
    fixture("headless-1ca933e")
}

/// v0.14.0's directory.
pub fn sx() -> PathBuf {
    fixture("headless-c5ddc0a")
}

pub fn read(p: impl AsRef<Path>) -> String {
    fs::read_to_string(p.as_ref()).unwrap()
}

/// What a run printed, its trailing newlines gone as a shell's `$(...)` takes
/// them, and its exit status.
#[derive(Debug)]
pub struct Out {
    pub out: String,
    /// The output whole.
    #[allow(dead_code)] // each test file uses a part of this module
    pub raw: String,
    pub code: i32,
}

/// The program run with `args`, `stdin` given as a here-string gives it, with
/// a newline after.
pub fn exec_at(program: &Path, args: &[&str], stdin: Option<&str>) -> Out {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    if let Some(s) = stdin {
        input.write_all(format!("{s}\n").as_bytes()).unwrap();
    }
    drop(input);
    let o = child.wait_with_output().unwrap();
    let raw = String::from_utf8_lossy(&o.stdout).to_string();
    Out {
        out: raw.trim_end_matches('\n').to_string(),
        raw,
        code: o.status.code().unwrap_or(-1),
    }
}

pub fn exec(args: &[&str], stdin: Option<&str>) -> Out {
    exec_at(Path::new(CYCLE), args, stdin)
}

pub fn scratch() -> TestDir {
    TestDir::new("headless")
}

const MARK: &str = "headless-test-edit-mark";

/// The JSON text with the value at `pointer` set to the JSON `to`, or, with
/// None, removed. The parents of a key that is set are made as jq makes them.
pub fn edit(json: &str, pointer: &str, to: Option<&str>) -> String {
    let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    match to {
        Some(to) => {
            let mut at = &mut v;
            for part in parent.split('/').skip(1) {
                at = match part.parse::<usize>() {
                    Ok(n) if at.is_array() => &mut at[n],
                    _ => &mut at[part],
                };
            }
            // A mark in the value's place, replaced by its text, keeps the
            // order of its keys, as jq keeps it: the program reads them in
            // that order.
            serde_json::from_str::<serde_json::Value>(to).unwrap();
            let mark = serde_json::Value::String(MARK.into());
            match key.parse::<usize>() {
                Ok(n) if at.is_array() => at[n] = mark,
                _ => at[key] = mark,
            }
            return v.to_string().replacen(&format!("\"{MARK}\""), to, 1);
        }
        None => match v.pointer_mut(parent) {
            Some(serde_json::Value::Object(at)) => {
                at.remove(key);
            }
            Some(serde_json::Value::Array(at)) => {
                at.remove(key.parse::<usize>().unwrap());
            }
            _ => {}
        },
    }
    v.to_string()
}
