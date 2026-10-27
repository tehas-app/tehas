//! What tehas export copies: each cycle of the history it is given, masked,
//! into a directory of its own (libexec/export says what and why).
//!
//!   console export run <home> <runs dir> <to> <next> <text 0|1>
//!     [--private <name>]... <lines
//!
//! Standard input holds the history's cycles, one JSON line each, as tehas
//! history gave them; each is kept byte for byte as the cycle's history.json.
//! <runs dir> is where the agents' transcripts are, one issue-<n>/ each. <to>
//! is the archive, made already; its cycles/ is made here. With <text> 1 it
//! prints a line for each cycle and one for the directory, <next> being the
//! cursor to keep.
//!
//! Every file goes through the mask, with the names given by --private. A
//! file of a Protolab page that is not UTF-8 is an image, a video or the like,
//! which the mask would only corrupt: it is copied as it is. Anything else is
//! masked whatever bytes it holds. A link is not followed and a file over
//! 32 MiB is left out.
//!
//! Exit: 0 copied; 1 a cycle could not be written, said on standard error,
//! the others are copied; 64 usage.

use crate::store;
use serde_json::Value;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tehas_core::mask;

const USAGE: &str =
    "usage: console export run <home> <runs dir> <to> <next> <text 0|1> [--private <name>]... <lines";

const BIGGEST: u64 = 32 * 1024 * 1024;

pub fn main(given: &[String]) -> u8 {
    let (fixed, rest) = given.split_at(given.len().min(6));
    if fixed.len() < 6 || fixed[0] != "run" || !matches!(fixed[5].as_str(), "0" | "1") {
        eprintln!("{USAGE}");
        return 64;
    }
    let mut names = Vec::new();
    let mut rest = rest.iter();
    while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
            ("--private", Some(name)) => names.push(name.clone()),
            _ => {
                eprintln!("{USAGE}");
                return 64;
            }
        }
    }
    if let Err(e) = mask::set_private(&names) {
        eprintln!("{e}");
        return 64;
    }
    let mut lines = String::new();
    if io::stdin().read_to_string(&mut lines).is_err() {
        eprintln!("tehas export: could not read the history");
        return 1;
    }
    let run = Export {
        home: PathBuf::from(&fixed[1]),
        runs: PathBuf::from(&fixed[2]),
        cycles: Path::new(&fixed[3]).join("cycles"),
    };
    if !make_dir(Path::new(&fixed[3])) || !make_dir(&run.cycles) {
        eprintln!("tehas export: could not make {}", run.cycles.display());
        return 1;
    }
    let lines: Vec<&str> = lines.lines().filter(|l| !l.is_empty()).collect();
    let mut copied = 0;
    let mut failed = String::new();
    for line in &lines {
        match run.cycle(line) {
            Ok(()) => copied += 1,
            Err(Some(cursor)) => failed.push_str(&format!(" {cursor}")),
            Err(None) => failed.push_str(" <unreadable>"),
        }
    }
    if !failed.is_empty() {
        eprintln!("tehas export: could not write{failed}");
    }
    if fixed[5] == "1" {
        for line in &lines {
            let v: Value = serde_json::from_str(line).unwrap_or(Value::Null);
            println!(
                "{}  {}  #{}",
                raw(&v["cursor"]),
                raw(&v["kind"]),
                raw(&v["issue"])
            );
        }
        let next = if fixed[4].is_empty() {
            "none"
        } else {
            &fixed[4]
        };
        println!("exported  {copied} cycles to {} (next: {next})", fixed[3]);
    }
    u8::from(!failed.is_empty())
}

/// A value as jq -r prints it.
fn raw(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// tehas history makes the cursor, of digits and dashes only.
fn cursor_shaped(c: &str) -> bool {
    let parts: Vec<&str> = c.split('-').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && !parts[1].starts_with('0')
        && !parts[2].starts_with('0')
}

/// A directory and every level of it this makes, 0755 whatever the umask: an
/// archive is for somebody else to open.
fn make_dir(d: &Path) -> bool {
    if d.is_dir() {
        let _ = fs::set_permissions(d, fs::Permissions::from_mode(0o755));
        return true;
    }
    if let Some(up) = d.parent() {
        if !up.as_os_str().is_empty() && !up.is_dir() && !make_dir(up) {
            return false;
        }
    }
    (fs::create_dir(d).is_ok() || d.is_dir())
        && fs::set_permissions(d, fs::Permissions::from_mode(0o755)).is_ok()
}

fn plain_file(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file())
}

/// The bytes as they go into the archive: masked, unless `raw_ok` and they
/// are not text.
fn masked(bytes: Vec<u8>, raw_ok: bool) -> Vec<u8> {
    match String::from_utf8(bytes) {
        Ok(text) => mask::mask(&text).into_bytes(),
        Err(e) if raw_ok => e.into_bytes(),
        Err(e) => mask::mask(&String::from_utf8_lossy(e.as_bytes())).into_bytes(),
    }
}

/// Writes <dir>/<name> whole, 0644: through a file beside it, so a copy cut
/// short never stands under the name.
fn put(dir: &Path, name: &str, bytes: &[u8]) -> bool {
    if !make_dir(dir) {
        return false;
    }
    let part = dir.join(format!(".tehas-masked.{}", std::process::id()));
    let to = dir.join(name);
    let done = fs::write(&part, bytes).is_ok()
        && fs::set_permissions(&part, fs::Permissions::from_mode(0o644)).is_ok()
        && fs::rename(&part, &to).is_ok();
    if !done {
        let _ = fs::remove_file(&part);
    }
    done
}

/// One file into <dir>/<name>. A file that is not there, is a link or is over
/// 32 MiB is no error and is not copied.
fn copy_in(from: &Path, dir: &Path, name: &str, raw_ok: bool) -> bool {
    let Ok(meta) = fs::symlink_metadata(from) else {
        return true;
    };
    if !meta.file_type().is_file() || meta.len() > BIGGEST {
        return true;
    }
    match fs::read(from) {
        Ok(bytes) => put(dir, name, &masked(bytes, raw_ok)),
        Err(_) => false,
    }
}

/// The plain files of a directory, not its links and not its dot names, in
/// the order of their names.
fn files_of(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.as_encoded_bytes().starts_with(b"."))
        })
        .filter(|p| plain_file(p))
        .collect();
    found.sort();
    found
}

/// Every plain file under a directory, as find -type f finds them: links are
/// neither followed nor taken.
fn tree(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            tree(&entry.path(), found);
        } else if kind.is_file() {
            found.push(entry.path());
        }
    }
}

/// Seconds since 1970 of a time as tehas history writes it; 0 when it is not.
fn epoch(v: &Value) -> f64 {
    v.as_str()
        .and_then(crate::snapshot::epoch)
        .map_or(0.0, |t| t as f64)
}

struct Export {
    home: PathBuf,
    runs: PathBuf,
    cycles: PathBuf,
}

impl Export {
    /// One cycle: Err with its cursor when it could not be written, with
    /// None when the line holds no cursor.
    fn cycle(&self, line: &str) -> Result<(), Option<String>> {
        let v: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        let cursor = v["cursor"]
            .as_str()
            .filter(|c| cursor_shaped(c))
            .ok_or(None)?;
        let failed = || Some(cursor.to_string());
        // A number names the files of the issue, and nothing else may.
        let issue = raw(&v["issue"]);
        if issue.is_empty() || !issue.bytes().all(|b| b.is_ascii_digit()) {
            return Err(failed());
        }
        let out = self.cycles.join(cursor);
        // An old copy is replaced whole, so a line that has since lost a field
        // cannot leave its value behind.
        if fs::symlink_metadata(&out).is_ok() {
            let gone = if out.is_dir() && !out.is_symlink() {
                fs::remove_dir_all(&out)
            } else {
                fs::remove_file(&out)
            };
            gone.map_err(|_| failed())?;
        }
        if !put(&out, "history.json", format!("{line}\n").as_bytes()) {
            return Err(failed());
        }
        let mut ok = true;
        if v["kind"] == "plan" {
            ok &= self
                .plan(&issue, epoch(&v["ended"]), &out)
                .ok_or_else(failed)?;
        } else {
            let state = self.home.join("state").join("cycles");
            let logs = self.home.join("logs");
            let into = out.join("logs");
            for (file, name) in [
                (state.join(format!("{issue}.result.json")), "result.json"),
                (
                    state.join(format!("{issue}.progress.json")),
                    "progress.json",
                ),
                (
                    state.join(format!("{issue}.manifest.json")),
                    "manifest.json",
                ),
                (
                    logs.join(format!("dispatch-work-{issue}.log")),
                    "dispatch-work.log",
                ),
            ] {
                ok &= copy_in(&file, &into, name, false);
            }
            // The runner's progress and its side streams, whatever stamp each
            // carries: a cycle started twice has more than one.
            let prefix = format!("work-{issue}-");
            for f in files_of(&logs) {
                let name = f.file_name().unwrap_or_default().to_string_lossy();
                if name.starts_with(&prefix)
                    && [".jsonl", ".jsonl.err", ".jsonl.console", ".jsonl.preflight"]
                        .iter()
                        .any(|end| name.ends_with(end))
                {
                    ok &= copy_in(&f, &into, &name, false);
                }
            }
            for f in files_of(&self.runs.join(format!("issue-{issue}"))) {
                let name = f.file_name().unwrap_or_default().to_string_lossy();
                ok &= copy_in(&f, &out.join("transcripts"), &name, false);
            }
        }
        let page = self.home.join("protolab").join(&issue);
        if page.is_dir() && !page.is_symlink() {
            let mut found = Vec::new();
            tree(&page, &mut found);
            for f in found {
                let name = f.file_name().unwrap_or_default().to_string_lossy();
                if name == ".stamp" {
                    continue;
                }
                let rel = f.strip_prefix(&page).unwrap_or(&f);
                let dir = out
                    .join("protolab")
                    .join(rel.parent().unwrap_or(Path::new("")));
                ok &= copy_in(&f, &dir, &name, true);
            }
        }
        if ok {
            Ok(())
        } else {
            Err(failed())
        }
    }

    /// A planning turn's session as the store has it, when it is the turn's:
    /// opened before the turn ended. The store keeps one session an issue, so
    /// one opened after is another turn's. None when the store did not answer.
    fn plan(&self, issue: &str, ended: f64, out: &Path) -> Option<bool> {
        let answer = match store::call(&self.home, &["plan-show", issue]) {
            Ok(v) => v,
            Err(no) => {
                if no.rc != 1 {
                    eprintln!(
                        "tehas export: ERROR: the state store cannot be used: {}",
                        no.line
                    );
                }
                return None;
            }
        };
        let session = &answer["planning"];
        let its =
            session.is_object() && session["opened_at"].as_f64().is_some_and(|at| at <= ended);
        if !its {
            return Some(true);
        }
        let text = serde_json::to_string_pretty(session).unwrap_or_default() + "\n";
        Some(put(out, "plan.json", &masked(text.into_bytes(), false)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_is_three_numbers_and_the_last_two_never_start_with_naught() {
        assert!(cursor_shaped("1760700000-12-1"));
        assert!(cursor_shaped("0-1-1"));
        for bad in [
            "1760700000-12",
            "1-012-1",
            "1-1-0",
            "a-1-1",
            "1--1",
            "../1-1-1",
            "1-1-1-1",
        ] {
            assert!(!cursor_shaped(bad), "{bad}");
        }
    }

    #[test]
    fn text_is_masked_and_only_a_page_file_that_is_not_text_is_kept_as_it_is() {
        let token = "ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let text = format!("a \0 stray {token}\n").into_bytes();
        assert_eq!(masked(text.clone(), false), b"a \0 stray ghp_***\n");
        assert_eq!(masked(text, true), b"a \0 stray ghp_***\n");
        let mut image = vec![0x89, b'P', b'N', b'G', 0xff, 0xfe];
        image.extend_from_slice(token.as_bytes());
        assert_eq!(masked(image.clone(), true), image);
        let log = masked(image, false);
        assert!(String::from_utf8(log).unwrap().ends_with("ghp_***"));
    }

    #[test]
    fn jq_raw_prints_strings_bare_and_the_rest_as_json() {
        assert_eq!(raw(&Value::from("plan")), "plan");
        assert_eq!(raw(&Value::from(12)), "12");
        assert_eq!(raw(&Value::Null), "null");
    }
}
