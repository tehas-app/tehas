//! The programs a snapshot asks: gh, the state store and the host's own
//! tools, each under a time bound. Only reads leave here: a gh call that could
//! change anything is refused before it runs.

use regex::Regex;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
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
    run_input_env(program, args, input, bound, &[])
}

/// run_input with extra variables in the child's environment, each a name and
/// its value. A poll gives every gh call it makes the token it took once.
pub fn run_input_env(
    program: &str,
    args: &[String],
    input: Option<&str>,
    bound: Duration,
    extra: &[(&str, &str)],
) -> Ran {
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let child = cmd
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
            // The pipe is closed when the words are written: a reader that
            // waits for the end of the request would wait for this otherwise.
            let _ = stdin.flush();
            drop(stdin);
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
    let status = wait(&mut child, bound, Duration::from_secs(10), false);
    Ran {
        status,
        out: reader.join().unwrap_or_default(),
        err: ereader.join().unwrap_or_default(),
    }
}

/// Run a program with a bound and no pipe: nothing it leaves running can hold
/// the poll. Its output goes to `out`, or nowhere.
pub fn run_quiet(
    program: &str,
    args: &[String],
    out: Option<std::fs::File>,
    bound: Duration,
) -> Option<i32> {
    // As the bash runs its host commands: timeout -k 5.
    run_grace(program, args, out, bound, Duration::from_secs(5))
}

/// run_quiet with the grace between TERM and KILL its caller gives, as
/// `timeout -k <grace>`. The program runs in a group of its own, and the
/// signals go to the group, as timeout sends them: what it started goes too.
pub fn run_grace(
    program: &str,
    args: &[String],
    out: Option<std::fs::File>,
    bound: Duration,
    grace: Duration,
) -> Option<i32> {
    use std::os::unix::process::CommandExt;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(out.map_or_else(Stdio::null, Stdio::from))
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    wait(&mut child, bound, grace, true)
}

/// The child's exit status, 128 and the signal for one a signal ended, as
/// a shell gives it; None when it outlived its bound and was stopped. With
/// `group`, the child leads a group of its own and the group is signalled.
pub(crate) fn wait(
    child: &mut std::process::Child,
    bound: Duration,
    grace: Duration,
    group: bool,
) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    let began = Instant::now();
    let target = if group {
        format!("-{}", child.id())
    } else {
        child.id().to_string()
    };
    loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code().or_else(|| s.signal().map(|g| 128 + g)),
            Ok(None) if began.elapsed() < bound => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                // As timeout -k: TERM first, so a docker client stops its
                // container, and KILL when it is still there after the grace.
                let _ = Command::new("kill")
                    .args(["-TERM", "--", &target])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let gone = Instant::now();
                while gone.elapsed() < grace {
                    if let Ok(Some(_)) = child.try_wait() {
                        // Out of time and ended by TERM: timeout's 124.
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                if let Ok(Some(_)) = child.try_wait() {
                    break None;
                }
                if group {
                    let _ = Command::new("kill")
                        .args(["-KILL", "--", &target])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
                let _ = child.kill();
                let _ = child.wait();
                // One that had to be killed: timeout -k answers 137.
                break Some(137);
            }
        }
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

/// What a failed gh call's line says of its error: ": " and the first words
/// of a fixed list in what gh printed, or nothing. Only these words, as an
/// agent can write the rest; the bash's gh_json uses the same list.
pub fn gh_why(err: &str) -> String {
    static WORDS: OnceLock<Regex> = OnceLock::new();
    let words = WORDS.get_or_init(|| {
        Regex::new(r"(?i)HTTP [0-9]{3}|secondary rate limit|rate limit|timed? ?out|connection (reset|refused)|no such host|unexpected EOF")
            .expect("the list is a regex")
    });
    err.lines()
        .find_map(|l| words.find(l))
        .map(|m| format!(": {}", m.as_str()))
        .unwrap_or_default()
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

/// The extra environment of one gh call of the poll: the token it took, when
/// the poll has one. Nothing else is given, so a call without an app is the
/// call it was.
fn gh_env(token: Option<&str>) -> Vec<(&str, &str)> {
    match token {
        Some(t) => vec![("TEHAS_GH_TOKEN", t)],
        None => Vec::new(),
    }
}

/// The calls of one snapshot, and how many went to GitHub.
pub struct Calls {
    /// The instance's home, which `TEHAS_HOME` gives every store call: the
    /// store names its own database from it. lib/store.sh passes it explicitly;
    /// a `Calls` built by a test that means a store of its own sets it, and one
    /// that names no home has no store to ask.
    pub home: Option<String>,
    pub bound: Duration,
    /// lib/store.sh's tehas_store_bound.
    pub store_bound: Duration,
    pub gh: String,
    pub store: String,
    pub gh_count: u32,
    /// The GitHub App's token, taken once at the poll's first gh call and kept
    /// here. Every gh process of the poll gets it as `TEHAS_GH_TOKEN`, which
    /// host/shim/gh hands to gh as `GH_TOKEN` instead of asking
    /// libexec/github-token for one of its own. The poll holds it in memory
    /// only: it is never logged, written, or put in the poll's record, and a
    /// poll with no app named has nothing here.
    pub gh_token: Option<String>,
}

impl Calls {
    /// The calls of a poll's reads, with the store's bound and its path.
    pub fn idle(store_bound: Duration, store: String) -> Calls {
        Calls {
            home: std::env::var("TEHAS_HOME").ok().filter(|h| !h.is_empty()),
            bound: Duration::from_secs(30),
            store_bound,
            gh: "gh".into(),
            store,
            gh_count: 0,
            gh_token: None,
        }
    }
}

/// What a failed store call's line says, cut as the bash cuts it: the control
/// characters of anything a container wrote are no part of a log line, and a
/// line over 300 of them is no part of one either.
fn trim(line: &str) -> String {
    line.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(300)
        .collect()
}

/// The token of the host's GitHub App, asked for once by the poll that first
/// needs it: `$TEHAS_HOME/libexec/github-token` with the poll's own environment,
/// as the bash runs it, printing the token and nothing else. Only when
/// `TEHAS_GITHUB_APP_ID` names an app. When the program could not be run or
/// answered nothing, this gives nothing and the poll's gh calls go on as they
/// did before, each asking for a token of its own.
pub fn read_gh_token(home: &Path, bound: Duration) -> Option<String> {
    if std::env::var("TEHAS_GITHUB_APP_ID")
        .unwrap_or_default()
        .is_empty()
    {
        return None;
    }
    let program = home
        .join("libexec/github-token")
        .to_string_lossy()
        .into_owned();
    let r = run(&program, &[], bound);
    let token = r.out.lines().next().unwrap_or("").trim();
    if r.ok() && !token.is_empty() {
        Some(token.to_string())
    } else {
        None
    }
}

/// A gh call that failed: what the bash's gh_json says for it.
pub struct GhFail(pub String);

impl Calls {
    /// Several reads of gh side by side, each answer as `gh` gives it.
    pub fn gh_all(&mut self, calls: &[Vec<String>]) -> Vec<Result<Value, GhFail>> {
        let bound = self.bound;
        let gh = self.gh.clone();
        // The token, owned, so each spawned call holds its own view of it and
        // the borrow of self ends here.
        let token = self.gh_token.clone();
        let mut out: Vec<Result<Value, GhFail>> = Vec::new();
        for chunk in calls.chunks(8) {
            let answers: Vec<Result<Value, GhFail>> = std::thread::scope(|s| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|args| {
                        let gh = gh.clone();
                        let token = token.clone();
                        s.spawn(move || {
                            let word = format!("ERROR: gh {} {} failed", args[0], args[1]);
                            if !gh_reads(args) {
                                return Err(GhFail(format!("{word}: refused, it is no read")));
                            }
                            let extra = gh_env(token.as_deref());
                            let r = run_input_env(&gh, args, None, bound, &extra);
                            if r.ok() {
                                Ok(parse(&r.out))
                            } else {
                                Err(GhFail(format!("{word}{}", gh_why(&r.err))))
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
        let extra = gh_env(self.gh_token.as_deref());
        let r = run_input_env(&self.gh, &args, None, self.bound, &extra);
        if !r.ok() {
            return Err(GhFail(format!("{word}{}", gh_why(&r.err))));
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
        let extra = gh_env(self.gh_token.as_deref());
        let r = run_input_env(&self.gh, &args, None, self.bound, &extra);
        serde_json::from_str(&r.out).ok()
    }

    /// The answers of several store reads, asked in one run of the store
    /// (`store batch`, #941). Each element is the arguments of one call, given
    /// back in the same order as `store` would have answered it, so a caller
    /// reads an answer exactly as it reads the answer of the single call. The
    /// failure of the batch as a whole is the failure of any one of its calls:
    /// one process stood for all of them, and a poll that could not ask them
    /// could not have asked one of them either.
    pub fn store_batch(&mut self, calls: &[Vec<String>]) -> Result<Vec<Value>, Value> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }
        let request = serde_json::Value::Array(
            calls
                .iter()
                .map(|c| serde_json::Value::Array(c.iter().map(|a| json!(a)).collect()))
                .collect(),
        )
        .to_string();
        let bad = |rc: i32, word: &str, line: String| json!({"failed": true, "rc": rc, "word": word, "line": trim(&line)});
        // The program is tested before it runs, and the home is given to the
        // child, both as lib/store.sh does for every verb: one that is not an
        // executable file is `absent`, which is the answer a poll whose store was
        // never installed gives, and the only one it can give.
        if !std::fs::metadata(&self.store).is_ok_and(|m| {
            use std::os::unix::fs::PermissionsExt;
            m.is_file() && m.permissions().mode() & 0o111 != 0
        }) {
            return Err(bad(
                78,
                "absent",
                format!("store: absent: {} is not an executable file", self.store),
            ));
        }
        // The home of the poll, which is the home the store names its file from.
        // A poll has one, and one that has none has no store to ask.
        let home = self.home.as_deref().unwrap_or_default().to_string();
        let extra = [("TEHAS_HOME", home.as_str())];
        let r = run_input_env(
            &self.store,
            &["batch".into()],
            Some(&request),
            self.store_bound,
            &extra,
        );
        // The bash reads the answer with stderr merged, so anything there makes
        // it no answer; the shape is the one `store` gives its other verbs.
        let said: Value = match r.status {
            Some(0) if r.err.is_empty() => {
                serde_json::from_str(r.out.trim()).unwrap_or_else(|_| json!({"answers": []}))
            }
            Some(0) => {
                return Err(bad(
                    78,
                    "garbled",
                    "store: garbled: batch answered 0 without its answer".to_string(),
                ))
            }
            None => {
                return Err(bad(
                    1,
                    "timeout",
                    format!(
                        "store: timeout: batch did not answer in {} s",
                        self.store_bound.as_secs()
                    ),
                ))
            }
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
                    return Err(bad(
                        78,
                        "garbled",
                        format!("store: garbled: batch refused without a word: {first}"),
                    ));
                }
                return Err(bad(rc, &word, first));
            }
            Some(126 | 127) => {
                return Err(bad(
                    78,
                    "absent",
                    format!("store: absent: {} could not be run", self.store),
                ))
            }
            Some(124 | 137) => {
                return Err(bad(
                    1,
                    "timeout",
                    format!(
                        "store: timeout: batch did not answer in {} s",
                        self.store_bound.as_secs()
                    ),
                ))
            }
            Some(rc) => {
                return Err(bad(
                    78,
                    "garbled",
                    format!("store: garbled: batch exited {rc}"),
                ))
            }
        };
        let answers = said
            .get("answers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if said.get("api") != Some(&json!(1))
            || said.get("ok") != Some(&json!(true))
            || said.get("verb").and_then(Value::as_str) != Some("batch")
            || answers.len() != calls.len()
        {
            return Err(bad(
                78,
                "garbled",
                "store: garbled: batch answered 0 without its answer".to_string(),
            ));
        }
        Ok(answers
            .into_iter()
            .map(|a| {
                // One call's answer, read as the answer of that call: the exit
                // it had, and what it printed, which is the one line the verb
                // writes and so the one JSON object the single call gives.
                let rc = a.get("rc").and_then(Value::as_i64).unwrap_or(78);
                if rc == 0 {
                    match a.get("out").and_then(Value::as_str) {
                        Some(text) => serde_json::from_str(text).unwrap_or(json!({"failed": true})),
                        // A read answered no text: nothing to read.
                        None => json!({"failed": true}),
                    }
                } else {
                    // The word the verb's own line says, as `store` reads it.
                    let line = a.get("out").and_then(Value::as_str).unwrap_or("");
                    let word: String = line
                        .strip_prefix("store: ")
                        .unwrap_or(line)
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || *c == '_' || *c == '-')
                        .collect();
                    json!({"failed": true, "rc": rc, "word": word, "line": trim(line)})
                }
            })
            .collect())
    }

    /// The store's answer to a read verb, or its failure as lib/store.sh
    /// keeps it: {failed, word, rc, line}.
    pub fn store(&mut self, args: &[&str]) -> Result<Value, Value> {
        let verb = args.first().copied().unwrap_or("");
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let r = run(&self.store, &args, self.store_bound);
        let fail = |rc: i32, word: &str, line: String| json!({"failed": true, "rc": rc, "word": word, "line": trim(&line)});
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
            // timeout's answers, as lib/store.sh reads them: out of time.
            Some(124 | 137) => Err(fail(
                1,
                "timeout",
                format!(
                    "store: timeout: {verb} did not answer in {} s",
                    self.store_bound.as_secs()
                ),
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

    /// The cases of the poll's token read and write one environment variable,
    /// so one of them runs at a time.
    static TOKEN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn a(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_failure_is_named_by_the_words_of_the_list_only() {
        assert_eq!(
            gh_why("HTTP 502: Bad Gateway for query with a sk-secret-looking body"),
            ": HTTP 502"
        );
        assert_eq!(
            gh_why("first line\nerror: API Secondary Rate Limit exceeded"),
            ": Secondary Rate Limit"
        );
        assert_eq!(
            gh_why("dial tcp: lookup api.github.com: no such host"),
            ": no such host"
        );
        assert_eq!(
            gh_why("read: connection reset by peer"),
            ": connection reset"
        );
        assert_eq!(gh_why("request timed out"), ": timed out");
        assert_eq!(gh_why("something an agent wrote"), "");
        assert_eq!(gh_why(""), "");
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

    /// A host of one's own for a case of the token: `libexec/github-token` of
    /// the home counts its runs and answers a token of its own, and the stub gh
    /// beside it answers a read with the token it was given. `counter` asks for
    /// the program that counts, which a case that only fails it does not need.
    /// The directory is named by the case, so two cases never share a counter.
    fn token_home(name: &str, counter: bool) -> Option<std::path::PathBuf> {
        let dir = match std::fs::canonicalize(std::env::temp_dir()) {
            Ok(d) => d.join(format!("tehas-{name}-{}", std::process::id())),
            Err(_) => return None,
        };
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(dir.join("libexec")).is_err() {
            return None;
        }
        use std::os::unix::fs::PermissionsExt;
        let token = dir.join("libexec/github-token");
        // The program of a case that fails at making one says its reason on the
        // error stream and ends non-zero, as libexec/github-token does.
        let body = if counter {
            "#!/bin/sh\ncount=\"$0.count\"\nn=$(cat \"$count\" 2>/dev/null || echo 0)\n\
             n=$((n + 1)); echo \"$n\" >\"$count\"\nprintf 'ghs_%s\\n' \"$n\"\n"
        } else {
            "#!/bin/sh\nprintf 'tehas: no GitHub App is set\\n' >&2\nexit 1\n"
        };
        std::fs::write(&token, body).ok()?;
        std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o755)).ok()?;
        let gh = dir.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\nprintf '%s\\n' \"${TEHAS_GH_TOKEN:-untouched}\"\n",
        )
        .ok()?;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).ok()?;
        Some(dir)
    }

    /// The calls of a snapshot that asks its reads of the stub gh of `home`.
    fn calls_of(home: &Path) -> Calls {
        let mut calls = Calls::idle(
            Duration::from_secs(5),
            home.join("store").to_string_lossy().into_owned(),
        );
        calls.gh = home.join("gh").to_string_lossy().into_owned();
        calls
    }

    /// One read of gh, answered with the token the stub was given.
    fn given(calls: &mut Calls) -> String {
        match calls.gh(&["api", "user"]) {
            // gh's own answer keeps the newline printf gave it, as any command's
            // does; the token is what lies before it.
            Ok(v) => v.as_str().unwrap_or_default().trim_end().to_string(),
            Err(f) => panic!("the stub gh answers, not {}", f.0),
        }
    }

    #[test]
    fn a_poll_takes_the_token_once_however_many_calls_it_makes() {
        // The poll's order: the token is taken before its first call, and the
        // calls of the poll go after it. The poll takes it with `read_gh_token`
        // and gives what it printed to every `Calls` of the poll, which is what
        // the poll of `poll.rs` does for its reads and its writes.
        let _guard = TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // With an app named, one run of the token program covers the whole
        // poll, and every gh call of it sees the same token.
        std::env::set_var("TEHAS_GITHUB_APP_ID", "5452597");
        let home = match token_home("token-once", true) {
            Some(d) => d,
            None => return,
        };
        let taken = read_gh_token(&home, Duration::from_secs(5));
        assert_eq!(taken.as_deref(), Some("ghs_1"));
        let mut calls = calls_of(&home);
        calls.gh_token = taken.clone();
        for _ in 0..5 {
            assert_eq!(given(&mut calls), "ghs_1");
        }
        assert_eq!(calls.gh_token, taken);
        assert_eq!(calls.gh_count, 5);
        // Nothing between the take above and these five calls ran the token
        // program: the poll's gh calls were served by the token it kept. A
        // second take asks the program again, which is why the poll keeps what
        // it took instead of asking for every call.
        assert_eq!(
            read_gh_token(&home, Duration::from_secs(5)).as_deref(),
            Some("ghs_2")
        );
        let n = std::fs::read_to_string(home.join("libexec/github-token.count"))
            .unwrap_or_default()
            .trim()
            .to_string();
        assert_eq!(n, "2", "the poll ran the token program once for its calls");
        let _ = std::fs::remove_dir_all(&home);

        // A write of the poll is given the token in the same way.
        let home = match token_home("token-write", true) {
            Some(d) => d,
            None => return,
        };
        let mut calls = calls_of(&home);
        calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
        let gh = calls.gh.clone();
        let extra = gh_env(calls.gh_token.as_deref());
        let r = run_input_env(
            &gh,
            &a(&["issue", "comment", "7"]),
            Some("a body"),
            Duration::from_secs(5),
            &extra,
        );
        assert!(r.ok());
        assert_eq!(r.out.trim(), "ghs_1");
        let _ = std::fs::remove_dir_all(&home);

        // With no app named the poll takes nothing and gh is given nothing:
        // each call pays for a token of its own, as it did before.
        std::env::remove_var("TEHAS_GITHUB_APP_ID");
        let home = match token_home("token-none", true) {
            Some(d) => d,
            None => return,
        };
        let mut calls = calls_of(&home);
        calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
        for _ in 0..3 {
            assert_eq!(given(&mut calls), "untouched");
        }
        assert_eq!(calls.gh_token, None);
        assert_eq!(calls.gh_count, 3);
        assert!(
            !home.join("libexec/github-token.count").exists(),
            "no token program runs when the instance names no app"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_token_that_could_not_be_had_leaves_the_calls_as_they_were() {
        let _guard = TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = match token_home("token-failed", false) {
            Some(d) => d,
            None => return,
        };
        use std::os::unix::fs::PermissionsExt;
        let program = home.join("libexec/github-token");
        std::env::set_var("TEHAS_GITHUB_APP_ID", "5452597");
        let mut calls = calls_of(&home);

        // The program is not there at all: nothing is kept, and the calls go
        // as they always did, each asking for a token of its own.
        std::fs::remove_file(&program).ok();
        calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
        assert_eq!(calls.gh_token, None);
        assert_eq!(given(&mut calls), "untouched");

        // One that answers nothing is no token; one that prints a word of a
        // token and still fails is no token; and the one of this home, which
        // says its reason and ends 1, is no token either.
        for body in [
            "#!/bin/sh\nexit 0\n",
            "#!/bin/sh\nprintf 'ghs_x\\n'\nexit 1\n",
            "#!/bin/sh\nprintf 'tehas: no GitHub App is set\\n' >&2\nexit 1\n",
        ] {
            std::fs::write(&program, body).expect("the token program is written");
            let _ = std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755));
            calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
            assert_eq!(calls.gh_token, None, "a failed run of {body}");
            assert_eq!(given(&mut calls), "untouched");
        }
        std::env::remove_var("TEHAS_GITHUB_APP_ID");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_call_past_its_bound_is_cut_off() {
        let r = run("sleep", &a(&["5"]), Duration::from_millis(100));
        assert_eq!(r.status, None);
        assert!(run("true", &[], Duration::from_secs(5)).ok());
    }
}
