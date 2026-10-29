//! What an ended cycle left, as libexec/cycle reads it for its report: the
//! files of the protocol in state/cycles/<issue>.*, the newest run log, the
//! start and image on the cycle's record, the approval it used, and the
//! state of its workspace. One JSON object; `report` decides from it.
//!
//! A file is a string (a regular file and what it holds), {"link": true} (a
//! link, which is never read), or absent. The run log is {"name", "text"}, or
//! {"name", "unreadable": true} for one that could not be read.

use serde_json::Value;
use std::collections::HashMap;

/// A file of the cycle's protocol, as `[ -f ] && [ ! -L ]` sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum File {
    Absent,
    Link,
    Text(String),
}

impl File {
    fn from(v: Option<&Value>) -> File {
        match v {
            Some(Value::String(s)) => File::Text(s.clone()),
            Some(Value::Object(o)) if o.get("link") == Some(&Value::Bool(true)) => File::Link,
            _ => File::Absent,
        }
    }

    /// What a regular file that is not a link holds.
    pub fn text(&self) -> Option<&str> {
        match self {
            File::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// The newest run log of the issue, logs/work-<issue>-*.jsonl.
#[derive(Clone, Debug)]
pub struct RunLog {
    pub name: String,
    /// None for a log that could not be read.
    pub text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Facts {
    pub issue: u64,
    pub repo: String,
    /// $HOME, for a path said in ~ form.
    pub home: String,
    pub hostname: String,
    /// $TEHAS_WORK_DIR-<issue>, the cycle's workspace.
    pub workdir: String,
    /// The parent of the workspace holds .tehas-workspaces.
    pub workspaces_marked: bool,
    /// The workspace, and the one moved aside, are directories now.
    pub workdir_dir: bool,
    pub before_retry_dir: bool,
    /// tehas work's exit status, and whether a signal stopped the cycle.
    pub rc: i64,
    pub signalled: bool,
    /// When the cycle took its lock, and now, in seconds.
    pub started: i64,
    pub now: i64,
    /// The start on record: its attempt, who and how, when it was approved,
    /// and whether it is the dispatcher's own retry after an outage.
    pub attempt: u64,
    pub start_by: String,
    pub start_how: String,
    pub approval_at: String,
    pub requeued: bool,
    files: HashMap<String, File>,
    pub run_log: Option<RunLog>,
    /// The store's answer to cycle-show, or the word of its failure.
    pub record: Result<Value, String>,
    /// The store's answer to approval-current, or the word of its failure.
    pub approval: Result<Value, String>,
    /// When the image on the record was built, as docker prints .Created.
    pub image_built: String,
}

fn st(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn answer(v: Option<&Value>) -> Result<Value, String> {
    match v {
        Some(Value::Object(o)) if o.contains_key("error") => Err(o
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string()),
        Some(v) => Ok(v.clone()),
        None => Err("unknown".into()),
    }
}

impl Facts {
    pub fn from_json(v: &Value) -> Result<Facts, String> {
        let issue = v
            .get("issue")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0)
            .ok_or("the facts name no issue")?;
        let start = v.get("start").cloned().unwrap_or(Value::Null);
        let attempt = start
            .get("attempt")
            .and_then(Value::as_u64)
            .ok_or("the facts name no attempt")?;
        let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        let int = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
        let files = v
            .get("files")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .map(|(k, f)| (k.clone(), File::from(Some(f))))
                    .collect()
            })
            .unwrap_or_default();
        let run_log = v.get("run_log").and_then(Value::as_object).map(|o| RunLog {
            name: o
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            text: o.get("text").and_then(Value::as_str).map(str::to_string),
        });
        Ok(Facts {
            issue,
            repo: st(v, "repo"),
            home: st(v, "home"),
            hostname: st(v, "hostname"),
            workdir: st(v, "workdir"),
            workspaces_marked: flag("workspaces_marked"),
            workdir_dir: flag("workdir_dir"),
            before_retry_dir: flag("before_retry_dir"),
            rc: int("rc"),
            signalled: flag("signalled"),
            started: int("started"),
            now: int("now"),
            attempt,
            start_by: st(&start, "by"),
            start_how: st(&start, "how"),
            approval_at: st(&start, "approval_at"),
            requeued: start.get("requeued") == Some(&Value::Bool(true)),
            files,
            run_log,
            record: answer(v.get("record")),
            approval: answer(v.get("approval")),
            image_built: st(v, "image_built"),
        })
    }

    /// A file of the protocol, state/cycles/<issue>.<name>.
    pub fn file(&self, name: &str) -> &File {
        self.files.get(name).unwrap_or(&File::Absent)
    }
}

/// `read -r -n <n>` of a file's first line, as bash reads it into one
/// variable with IFS empty: up to the line end or `n` bytes.
pub fn first_bytes(text: &str, n: usize) -> String {
    let line = text.split('\n').next().unwrap_or("");
    let mut end = line.len().min(n);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].to_string()
}

/// `read -r a b c` of a first line: the words, as the default IFS splits
/// them. With `n`, only the first `n` bytes are read.
pub fn words(text: &str, n: Option<usize>) -> Vec<String> {
    let line = match n {
        Some(n) => first_bytes(text, n),
        None => text.split('\n').next().unwrap_or("").to_string(),
    };
    line.split([' ', '\t'])
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// `read -r v` of one line: the line with the default IFS's blanks taken
/// off either end.
pub fn line(text: &str) -> String {
    text.split('\n')
        .next()
        .unwrap_or("")
        .trim_matches([' ', '\t'])
        .to_string()
}

/// The first two lines of `head -c 100`, as `{ read -r a; read -r b; }`
/// reads them.
pub fn two_lines(text: &str) -> (String, String) {
    let head = first_bytes_any(text, 100);
    let mut lines = head.split('\n');
    let a = lines.next().unwrap_or("").trim_matches([' ', '\t']);
    let b = lines.next().unwrap_or("").trim_matches([' ', '\t']);
    (a.to_string(), b.to_string())
}

/// The first `n` bytes of a text, across lines.
fn first_bytes_any(text: &str, n: usize) -> &str {
    let mut end = text.len().min(n);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_bashs() {
        assert_eq!(first_bytes("12345\nx", 3), "123");
        assert_eq!(
            words("  3600 tehas  review x\n", Some(200)),
            ["3600", "tehas", "review", "x"]
        );
        assert_eq!(line("  killed \nmore"), "killed");
        assert_eq!(two_lines("60\n review\nx"), ("60".into(), "review".into()));
    }
}
