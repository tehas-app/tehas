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
    run_input(program, args, None, bound)
}

/// Run a program with a bound, given `input` on its standard input.
pub fn run_input(program: &str, args: &[String], input: Option<&str>, bound: Duration) -> Ran {
    let child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
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
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let text = text.to_string();
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        });
    }
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
                // As timeout -k 10: TERM first, so a docker client stops its
                // container, and KILL when it is still there 10 s later.
                let _ = Command::new("kill")
                    .args(["-TERM", &child.id().to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let gone = Instant::now();
                while gone.elapsed() < Duration::from_secs(10) {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
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
        // Only variables and the query, each a flag and its value: no method,
        // no input file, nothing joined to a flag.
        ["api", "graphql", rest @ ..] => {
            rest.len() % 2 == 0
                && rest
                    .chunks(2)
                    .all(|p| matches!(p[0], "-f" | "-F") && p[1].contains('='))
                && rest
                    .chunks(2)
                    .find_map(|p| p[1].strip_prefix("query="))
                    .is_some_and(|q| q.trim_start().starts_with("query") && !q.contains("mutation"))
        }
        // A path, and at most --paginate.
        ["api", path, rest @ ..] => {
            !path.starts_with('-') && rest.iter().all(|x| *x == "--paginate")
        }
        ["issue" | "pr", "list" | "view", rest @ ..] => {
            !rest.iter().any(|x| x.starts_with("--web") || *x == "-w")
        }
        _ => false,
    }
}

/// gh's output as JSON. --paginate prints one array a page, one after
/// another: they are one list, as `jq -s add` makes them.
fn parse(out: &str) -> Value {
    if let Ok(v) = serde_json::from_str(out) {
        return v;
    }
    let pages: Result<Vec<Value>, _> = serde_json::Deserializer::from_str(out)
        .into_iter::<Value>()
        .collect();
    match pages {
        Ok(p) if !p.is_empty() && p.iter().all(Value::is_array) => Value::Array(
            p.into_iter()
                .flat_map(|v| match v {
                    Value::Array(a) => a,
                    _ => Vec::new(),
                })
                .collect(),
        ),
        _ => Value::String(out.to_string()),
    }
}

/// The calls of one snapshot, and how many went to GitHub.
pub struct Calls {
    pub bound: Duration,
    /// lib/store.sh's tehas_store_bound.
    pub store_bound: Duration,
    pub gh: String,
    pub store: String,
    pub gh_count: u32,
}

/// A gh call that failed: what the bash's gh_json says for it.
pub struct GhFail(pub String);

impl Calls {
    /// Several reads of gh side by side, each answer as `gh` gives it.
    pub fn gh_all(&mut self, calls: &[Vec<String>]) -> Vec<Result<Value, GhFail>> {
        let bound = self.bound;
        let gh = self.gh.clone();
        let mut out: Vec<Result<Value, GhFail>> = Vec::new();
        for chunk in calls.chunks(8) {
            let answers: Vec<Result<Value, GhFail>> = std::thread::scope(|s| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|args| {
                        let gh = gh.clone();
                        s.spawn(move || {
                            let word = format!("ERROR: gh {} {} failed", args[0], args[1]);
                            if !gh_reads(args) {
                                return Err(GhFail(format!("{word}: refused, it is no read")));
                            }
                            let r = run(&gh, args, bound);
                            if r.ok() {
                                Ok(parse(&r.out))
                            } else {
                                Err(GhFail(word))
                            }
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| Err(GhFail("ERROR: gh failed".into())))
                    })
                    .collect()
            });
            self.gh_count += chunk.len() as u32;
            out.extend(answers);
        }
        out
    }

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
        Ok(parse(&r.out))
    }

    /// gh's output as JSON, whatever its exit status: the bash reads some
    /// answers so, keeping what came back of a partial one.
    pub fn gh_any(&mut self, args: &[&str]) -> Option<Value> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        if !gh_reads(&args) {
            return None;
        }
        self.gh_count += 1;
        let r = run(&self.gh, &args, self.bound);
        serde_json::from_str(&r.out).ok()
    }

    /// The store's answer to a read verb, or its failure as lib/store.sh
    /// keeps it: {failed, word, rc, line}.
    pub fn store(&mut self, args: &[&str]) -> Result<Value, Value> {
        let verb = args.first().copied().unwrap_or("");
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let r = run(&self.store, &args, self.store_bound);
        let fail = |rc: i32, word: &str, line: String| {
            serde_json::json!({"failed": true, "rc": rc, "word": word,
                "line": line.chars().map(|c| if c.is_control() { '?' } else { c }).take(300).collect::<String>()})
        };
        match r.status {
            Some(0) => {
                let v: Value = serde_json::from_str(r.out.trim()).unwrap_or(Value::Null);
                // The bash reads the answer with stderr merged: anything there
                // makes it no answer.
                if r.err.is_empty()
                    && !r.out.trim().contains('\n')
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
                    self.store_bound.as_secs()
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
        assert!(!gh_reads(&a(&[
            "api",
            "-XPOST",
            "repos/a/b/issues/7/labels"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/labels",
            "-fa=b"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/labels",
            "--field=a=b"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "graphql",
            "--input=x",
            "-f",
            "query=query { a }"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "graphql",
            "-XPOST",
            "-f",
            "query=query { a }"
        ])));
    }

    #[test]
    fn pages_are_one_list() {
        assert_eq!(parse("[1,2][3]"), serde_json::json!([1, 2, 3]));
        assert_eq!(parse("{\"a\":1}"), serde_json::json!({"a": 1}));
        assert_eq!(parse("not json"), serde_json::json!("not json"));
    }

    #[test]
    fn a_call_past_its_bound_is_cut_off() {
        let r = run("sleep", &a(&["5"]), Duration::from_millis(100));
        assert_eq!(r.status, None);
        assert!(run("true", &[], Duration::from_secs(5)).ok());
    }
}
