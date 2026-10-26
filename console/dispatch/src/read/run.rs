//! The programs a snapshot asks: gh, the state store and the host's own
//! tools, each under a time bound. Only reads leave here: a gh call that could
//! change anything is refused before it runs.

use serde_json::Value;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a call gave: its exit status and what it printed, or None when it
/// could not be run or ran out of time.
pub struct Ran {
    pub status: Option<i32>,
    pub out: String,
    pub err: String,
}

impl Ran {
    pub fn ok(&self) -> bool {
        self.status == Some(0)
    }
}

/// Run a program with a bound, its standard input closed.
pub fn run(program: &str, args: &[String], bound: Duration) -> Ran {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return Ran {
            status: None,
            out: String::new(),
            err: format!("{program} could not be run"),
        };
    };
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut o = String::new();
        if let Some(s) = out.as_mut() {
            let _ = s.read_to_string(&mut o);
        }
        o
    });
    let ereader = std::thread::spawn(move || {
        let mut e = String::new();
        if let Some(s) = err.as_mut() {
            let _ = s.read_to_string(&mut e);
        }
        e
    });
    let began = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code(),
            Ok(None) if began.elapsed() < bound => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    Ran {
        status,
        out: reader.join().unwrap_or_default(),
        err: ereader.join().unwrap_or_default(),
    }
}

/// Whether a gh call only reads: `api graphql` with a query, `api` of a path
/// with no method or field, or a list or view.
pub fn gh_reads(args: &[String]) -> bool {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["api", "graphql", rest @ ..] => {
            let query = rest
                .iter()
                .find_map(|x| x.strip_prefix("query="))
                .unwrap_or("");
            query.trim_start().starts_with("query") && !query.contains("mutation")
        }
        ["api", path, rest @ ..] => {
            !path.starts_with('-')
                && !rest.iter().any(|x| {
                    matches!(
                        *x,
                        "-X" | "--method" | "-f" | "-F" | "--field" | "--raw-field" | "--input"
                    ) || x.starts_with("--method=")
                })
        }
        ["issue" | "pr", "list" | "view", ..] => true,
        _ => false,
    }
}

/// The calls of one snapshot, and how many went to GitHub.
pub struct Calls {
    pub bound: Duration,
    pub gh: String,
    pub store: String,
    pub gh_count: u32,
}

/// A gh call that failed: what the bash's gh_json says for it.
pub struct GhFail(pub String);

impl Calls {
    /// gh's answer as JSON, Err with the line the bash poll logs.
    pub fn gh(&mut self, args: &[&str]) -> Result<Value, GhFail> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let word = format!("ERROR: gh {} {} failed", args[0], args[1]);
        if !gh_reads(&args) {
            return Err(GhFail(format!("{word}: refused, it is no read")));
        }
        self.gh_count += 1;
        let r = run(&self.gh, &args, self.bound);
        if !r.ok() {
            return Err(GhFail(word));
        }
        Ok(serde_json::from_str(&r.out).unwrap_or(Value::String(r.out)))
    }

    /// The store's answer to a read verb, or its failure as lib/store.sh
    /// keeps it: {failed, word, rc, line}.
    pub fn store(&mut self, args: &[&str]) -> Result<Value, Value> {
        let verb = args.first().copied().unwrap_or("");
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let r = run(&self.store, &args, self.bound);
        let fail = |rc: i32, word: &str, line: String| {
            serde_json::json!({"failed": true, "rc": rc, "word": word,
                "line": line.chars().map(|c| if c.is_control() { '?' } else { c }).take(300).collect::<String>()})
        };
        match r.status {
            Some(0) => {
                let v: Value = serde_json::from_str(r.out.trim()).unwrap_or(Value::Null);
                if !r.out.trim().contains('\n')
                    && v.get("api") == Some(&serde_json::json!(1))
                    && v.get("ok") == Some(&serde_json::json!(true))
                    && v.get("verb").and_then(Value::as_str) == Some(verb)
                {
                    Ok(v)
                } else {
                    Err(fail(
                        78,
                        "garbled",
                        format!("store: garbled: {verb} answered 0 without its answer"),
                    ))
                }
            }
            None => Err(fail(
                1,
                "timeout",
                format!(
                    "store: timeout: {verb} did not answer in {} s",
                    self.bound.as_secs()
                ),
            )),
            Some(rc @ (1 | 64 | 78)) => {
                let text = if r.err.is_empty() { &r.out } else { &r.err };
                let first = text.lines().next().unwrap_or("").to_string();
                let word: String = first
                    .strip_prefix("store: ")
                    .map(|w| {
                        w.chars()
                            .take_while(|c| c.is_ascii_lowercase() || *c == '_' || *c == '-')
                            .collect()
                    })
                    .unwrap_or_default();
                if rc == 1 && word.is_empty() {
                    Err(fail(
                        78,
                        "garbled",
                        format!("store: garbled: {verb} refused without a word: {first}"),
                    ))
                } else {
                    Err(fail(rc, &word, first))
                }
            }
            Some(126 | 127) => Err(fail(
                78,
                "absent",
                format!("store: absent: {} could not be run", self.store),
            )),
            Some(rc) => Err(fail(
                78,
                "garbled",
                format!("store: garbled: {verb} exited {rc}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_reads_pass() {
        assert!(gh_reads(&a(&[
            "api",
            "graphql",
            "-F",
            "owner=a",
            "-f",
            "query=query { viewer { login } }"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "graphql",
            "-f",
            "query=mutation { x }"
        ])));
        assert!(gh_reads(&a(&["api", "user"])));
        assert!(gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/events?per_page=100",
            "--paginate"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "-X",
            "PATCH",
            "repos/a/b/issues/comments/1"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/labels",
            "-f",
            "labels[]=x"
        ])));
        assert!(gh_reads(&a(&["pr", "list", "--repo", "a/b"])));
        assert!(!gh_reads(&a(&["issue", "edit", "7"])));
        assert!(!gh_reads(&a(&["issue", "comment", "7"])));
    }

    #[test]
    fn a_call_past_its_bound_is_cut_off() {
        let r = run("sleep", &a(&["5"]), Duration::from_millis(100));
        assert_eq!(r.status, None);
        assert!(run("true", &[], Duration::from_secs(5)).ok());
    }
}
