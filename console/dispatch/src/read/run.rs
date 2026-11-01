//! The programs a snapshot asks: gh, the state store and the host's own
//! tools, each under a time bound. Only reads leave here: a gh call that could
//! change anything is refused before it runs. How a program is run is
//! tehas_core::call.

use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tehas_core::call::{self, store_executable, store_failed, store_failure, word_of, GRACE};
pub use tehas_core::call::{
    gh_env, gh_why, run, run_grace, run_grace_env, run_input, run_input_bounded, run_input_env,
    run_quiet, wait, Ran,
};

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

/// The request `store batch` reads: the same 1 MiB the store bounds its own
/// standard input with (src/batch.rs). The store bounds what it is given and
/// not what it prints, and one `plan-show` of a queue may answer a brief of the
/// store's own bound, so the answer of a batch is the poll's own risk to bound:
/// what lies past the size of a legal request is not an answer anyone meant,
/// and is not read.
const BATCH_BOUND: u64 = 1 << 20;

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
    ///
    /// The answer is bounded as the request is: the store bounds what it reads
    /// and not what it prints, and one call of `plan-show` may print a brief of
    /// its own bound, so the whole of a queue's briefs is a legal answer of one
    /// batch. Nothing past the bound of the store's own request is read, and an
    /// answer that goes past it is the garbled answer it is.
    pub fn store_batch(&mut self, calls: &[Vec<String>]) -> Result<Vec<Value>, Value> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }
        // A `Calls` that names no home has no store to ask: the failure says so
        // rather than handing the store an empty `TEHAS_HOME` and reading its
        // refusal of the environment as the absence of the program.
        let Some(home) = self.home.clone() else {
            return Err(store_failed(
                78,
                "absent",
                "store: absent: the poll has no home to name the store's file",
            ));
        };
        let request = serde_json::Value::Array(
            calls
                .iter()
                .map(|c| serde_json::Value::Array(c.iter().map(|a| json!(a)).collect()))
                .collect(),
        )
        .to_string();
        // The program is tested before it runs, and the home is given to the
        // child, both as lib/store.sh does for every verb: one that is not an
        // executable file is `absent`, which is the answer a poll whose store was
        // never installed gives, and the only one it can give.
        if !store_executable(&self.store) {
            return Err(store_failed(
                78,
                "absent",
                &format!("store: absent: {} is not an executable file", self.store),
            ));
        }
        let extra = [("TEHAS_HOME", home.as_str())];
        let r = run_input_bounded(
            &self.store,
            &["batch".into()],
            Some(&request),
            self.store_bound,
            &extra,
            BATCH_BOUND,
        );
        // The exit ladder is the bash's, shared with the single call. An answer
        // past the ceiling is no answer either: the store bounds its request and
        // not what it prints, and the poll keeps neither the memory nor the
        // seconds of a queue's worth of briefs.
        if r.truncated {
            return Err(store_failed(
                78,
                "garbled",
                &format!(
                    "store: garbled: batch answered over the {BATCH_BOUND} bytes of one request"
                ),
            ));
        }
        if let Some((rc, word, line)) = store_failure("batch", &r, self.store_bound, &self.store) {
            return Err(store_failed(rc, &word, &line));
        }
        // The bash reads the answer with stderr merged, so anything there makes
        // it no answer, and the answer is one line of the shape `store` gives.
        let said: Value = if r.err.is_empty() {
            serde_json::from_str(r.out.trim()).unwrap_or(Value::Null)
        } else {
            Value::Null
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
            return Err(store_failed(
                78,
                "garbled",
                "store: garbled: batch answered 0 without its answer",
            ));
        }
        Ok(answers
            .into_iter()
            .map(|a| {
                // One call's answer, read as the answer of that call: the exit
                // it had, and what it printed, which is the one line the verb
                // writes and so the one JSON object the single call gives. An
                // element whose shape is not that is no answer of the store, and
                // is read as one: the garbled 78 the single call gives for the
                // same bytes, not a refusal of a word of nothing.
                let Some(rc) = a.get("rc").and_then(Value::as_i64) else {
                    return store_failed(
                        78,
                        "garbled",
                        "store: garbled: an answer of the batch holds no exit",
                    );
                };
                let Some(text) = a.get("out").and_then(Value::as_str) else {
                    return store_failed(
                        78,
                        "garbled",
                        "store: garbled: an answer of the batch holds no line",
                    );
                };
                if rc == 0 {
                    match serde_json::from_str(text) {
                        Ok(v) => v,
                        Err(_) => store_failed(
                            78,
                            "garbled",
                            &format!("store: garbled: a read answered no JSON: {text}"),
                        ),
                    }
                } else {
                    match word_of(text) {
                        // The word the verb's own line says, as the single call
                        // reads it, and the same garbled answer for a refusal
                        // that does not name one.
                        Some(word) => store_failed(rc as i32, word, text),
                        None if rc == 1 => store_failed(
                            78,
                            "garbled",
                            &format!("store: garbled: a read refused without a word: {text}"),
                        ),
                        None => store_failed(rc as i32, "", text),
                    }
                }
            })
            .collect())
    }

    /// The store's answer to a read verb, or its failure as lib/store.sh
    /// keeps it: {failed, word, rc, line}.
    pub fn store(&mut self, args: &[&str]) -> Result<Value, Value> {
        call::store(&self.store, args, self.store_bound, GRACE, &[])
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
}
