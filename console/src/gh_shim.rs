//! The host's gh shim (host/shim/gh): which call is a write, and the line a
//! write leaves in the write log.
//!
//!   console gh <real gh> <log> [words]
//!
//! The shim takes the token and execs this. A call this does not know as a
//! write is a read and execs the real gh at once: no copy of a stream and no
//! line. A write runs gh, keeps its error stream to read GitHub's status from,
//! and appends one tab-separated line to <log>: the time in UTC, the kind
//! (`api POST`, `issue comment`, ...), the repository, the issue or PR number,
//! gh's exit status, and 403 or 429 when gh's error stream says so, else `-`.
//! Nothing else reaches the line: not the token, not a body, not a field value.
//! `gh api graphql` always posts, so it is told by the operation at the start
//! of its query: a mutation is a write of kind `api POST` (#792).
//!
//! On a read-only instance (TEHAS_READ_ONLY=1, #608) only the calls [`reads`]
//! knows run; every other one, a write or a command it does not know, is
//! refused, says what it would have been, and exits 0.
//!
//! A log that cannot be written changes nothing of gh's result: one word about
//! it comes on the error stream after gh's own.
//!
//! Exit: gh's own; 64 no real gh or no log named; 127 gh cannot be run.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::time;

const USAGE: &str = "usage: console gh <real gh> <log> [words]";

/// What a write is: its kind, and the repository and number it names, each
/// empty when the arguments do not say.
#[derive(Debug, PartialEq, Eq)]
pub struct Write {
    pub kind: String,
    pub repo: String,
    pub number: String,
}

/// The write a call makes, or None for a read. From the arguments alone.
pub fn classify(args: &[String]) -> Option<Write> {
    let (kind, repo, number) = match args.first().map(String::as_str) {
        Some("api") => api(args)?,
        Some(cmd @ ("issue" | "pr" | "label")) => verb(cmd, args)?,
        _ => return None,
    };
    Some(Write { kind, repo, number })
}

// gh decides its own method: -X wins, a call with fields and no method posts,
// a call with neither is a read. graphql always posts, so the first word of
// its document says everything (#792). The document is the value of the field
// named query as -f gives it: -F gives a variable, which is no document.
fn api(args: &[String]) -> Option<(String, String, String)> {
    let mut path = "";
    let mut method = String::new();
    let mut repo = String::new();
    let mut fields = false;
    let mut graphql = false;
    let mut query = "";
    let mut prev = "";
    for a in args {
        let a = a.as_str();
        let given = matches!(prev, "-f" | "--field" | "--raw-field");
        match prev {
            "-R" | "--repo" => repo = a.to_string(),
            "-X" | "--method" => method = a.to_string(),
            _ => {}
        }
        prev = a;
        if let Some(m) = a
            .strip_prefix("-X")
            .filter(|m| m.starts_with(|c: char| c.is_ascii_alphabetic()))
        {
            method = m.to_string();
        } else if let Some(m) = a.strip_prefix("--method=") {
            method = m.to_string();
        } else if let Some(r) = a.strip_prefix("-R=").or_else(|| a.strip_prefix("--repo=")) {
            repo = r.to_string();
        } else if a == "graphql" {
            graphql = args.get(1).is_some_and(|s| s == "graphql");
        } else if matches!(a, "-f" | "-F" | "--field" | "--raw-field" | "--input") {
            fields = true;
        } else if let Some(q) = a.strip_prefix("query=") {
            if given {
                query = q;
            }
        } else if a.starts_with(|c: char| c.is_ascii_alphabetic())
            && a.contains('/')
            && path.is_empty()
        {
            path = a;
        }
    }
    if graphql {
        return mutation(query).then(|| ("api POST".to_string(), repo, String::new()));
    }
    let kind = match method.to_uppercase().as_str() {
        "" if fields => "api POST".to_string(),
        "" => return None,
        m => format!("api {m}"),
    };
    let (path_repo, number) = from_path(path);
    if repo.is_empty() {
        repo = path_repo;
    }
    Some((kind, repo, number))
}

// The operation is the first word of the document, or its second when the
// first is the name it carries. The document's body begins at its first quote
// or bracket, so no word of it stands as the operation, and each word is read
// whole: `mutations` is not `mutation`.
fn mutation(query: &str) -> bool {
    let head = query.split(['"', '\'', '[', '{', '(']).next().unwrap_or("");
    head.split([' ', '\t', '\n', '\r'])
        .filter(|w| !w.is_empty())
        .take(2)
        .any(|w| w.to_lowercase() == "mutation")
}

// repos/<owner>/<name> names the repository, and the word after issues/ or
// pulls/ the number. A query string says nothing; a leading slash names no
// segment.
fn from_path(path: &str) -> (String, String) {
    let p = path.strip_prefix('/').unwrap_or(path);
    let p = p.split(['?', '\n']).next().unwrap_or("");
    let p = p.strip_suffix('/').unwrap_or(p);
    let segs: Vec<&str> = if p.is_empty() {
        Vec::new()
    } else {
        p.split('/').collect()
    };
    let mut repo = String::new();
    let mut number = String::new();
    for (k, s) in segs.iter().enumerate() {
        if repo.is_empty() && *s == "repos" && segs.len() >= k + 3 {
            repo = format!("{}/{}", segs[k + 1], segs[k + 2]);
        }
        if number.is_empty() && k > 0 && matches!(segs[k - 1], "issues" | "pulls") && digits(s) {
            number = s.to_string();
        }
    }
    (repo, number)
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

// The verb is the word after the command, and the number the first word of
// only digits after it. A label names no issue or PR, so it carries none.
fn verb(cmd: &str, args: &[String]) -> Option<(String, String, String)> {
    let v = args.get(1)?;
    if !matches!(
        v.as_str(),
        "comment" | "edit" | "create" | "close" | "reopen" | "ready" | "merge" | "delete"
    ) {
        return None;
    }
    let mut repo = String::new();
    let mut number = String::new();
    let mut prev = "";
    for a in &args[2..] {
        if matches!(prev, "-R" | "--repo") {
            repo = a.clone();
        }
        prev = a;
        if let Some(r) = a.strip_prefix("-R=").or_else(|| a.strip_prefix("--repo=")) {
            repo = r.to_string();
        }
        if cmd != "label" && number.is_empty() && digits(a) {
            number = a.clone();
        }
    }
    Some((format!("{cmd} {v}"), repo, number))
}

/// 403 or 429 when gh's error stream says so, 403 first; else `-`.
pub fn marker(stderr: &[u8]) -> &'static str {
    let has = |n: &[u8]| stderr.windows(n.len()).any(|w| w == n);
    if has(b"HTTP 403") {
        "403"
    } else if has(b"HTTP 429") {
        "429"
    } else {
        "-"
    }
}

/// The log's line: six fields, each one word, so no tab or newline of an odd
/// argument takes a field or a line of its own.
pub fn line(at: i64, w: &Write, rc: i32, marker: &str) -> String {
    let field = |s: &str| {
        if s.is_empty() {
            "-".to_string()
        } else {
            s.replace(['\t', '\n'], " ")
        }
    };
    format!(
        "{}\t{}\t{}\t{}\t{rc}\t{marker}\n",
        time::iso(at),
        field(&w.kind),
        field(&w.repo),
        field(&w.number)
    )
}

/// Whether a call only reads, as a read-only instance (#608) lets it run: a
/// short list, the same as host/sandbox/gh-shim, and anything else counts as
/// a write, so a command gh adds later is refused too. `gh api` reads with
/// `-X GET`, with no method and no field, or as a graphql query.
pub fn reads(args: &[String]) -> bool {
    let a = |k: usize| args.get(k).map(String::as_str).unwrap_or("");
    match (a(0), a(1)) {
        ("search" | "--version" | "version", _) => true,
        ("api", _) => classify(args).is_none_or(|w| w.kind == "api GET"),
        (cmd, v) => matches!(
            (cmd, v),
            ("issue", "view" | "list" | "status")
                | ("pr", "view" | "list" | "status" | "checks" | "diff")
                | ("repo", "view" | "clone")
                | ("label", "list")
                | ("run", "view" | "list")
                | ("release", "view" | "list" | "download")
                | ("auth", "status" | "token" | "git-credential" | "setup-git")
        ),
    }
}

// A call no kind of write names, told by its first two words and the
// repository it gives: `gh release create o/r`.
fn other(args: &[String]) -> Write {
    let mut repo = String::new();
    let mut prev = "";
    for a in args {
        if matches!(prev, "-R" | "--repo") {
            repo = a.clone();
        }
        prev = a;
        if let Some(r) = a.strip_prefix("-R=").or_else(|| a.strip_prefix("--repo=")) {
            repo = r.to_string();
        }
    }
    let kind: Vec<&str> = args
        .iter()
        .take(2)
        .map(String::as_str)
        .take_while(|w| !w.starts_with('-'))
        .collect();
    Write {
        kind: kind.join(" "),
        repo,
        number: String::new(),
    }
}

/// What a refused write was, in the words of its log line: `gh issue edit
/// o/r#7`, with no number or repository when the call names none.
pub fn refused(w: &Write) -> String {
    let mut s = format!("gh {}", w.kind);
    if !w.repo.is_empty() {
        s.push(' ');
        s.push_str(&w.repo);
    }
    if !w.number.is_empty() {
        s.push_str(if w.repo.is_empty() { " #" } else { "#" });
        s.push_str(&w.number);
    }
    s
}

fn append(log: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = log.parent() {
        let _ = fs::create_dir_all(dir);
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?
        .write_all(text.as_bytes())
}

pub fn main(given: &[OsString]) -> u8 {
    let [gh, log, words @ ..] = given else {
        let _ = writeln!(io::stderr(), "{USAGE}");
        return 64;
    };
    let args: Vec<String> = words
        .iter()
        .map(|w| w.to_string_lossy().into_owned())
        .collect();
    let w = classify(&args);
    // A read-only instance (#608): a call that is not a known read is not run
    // and leaves no line in the write log, which counts what reached GitHub.
    // Its caller goes on as after a write that answered nothing.
    if !reads(&args) {
        let what = w.as_ref().map_or_else(|| refused(&other(&args)), refused);
        if tehas_core::read_only::refuse(&what) {
            return 0;
        }
    }
    let Some(w) = w else {
        let e = Command::new(gh).args(words).exec();
        let _ = writeln!(io::stderr(), "gh: {}: {e}", Path::new(gh).display());
        return 127;
    };
    let out = Command::new(gh)
        .args(words)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|child| child.wait_with_output());
    let out = match out {
        Ok(out) => out,
        Err(e) => {
            let _ = writeln!(io::stderr(), "gh: {}: {e}", Path::new(gh).display());
            return 127;
        }
    };
    let rc = out
        .status
        .code()
        .unwrap_or_else(|| 128 + out.status.signal().unwrap_or(0));
    let text = line(time::now(), &w, rc, marker(&out.stderr));
    let logged = append(Path::new(log), &text);
    let mut err = io::stderr();
    let _ = err.write_all(&out.stderr);
    if logged.is_err() {
        let _ = writeln!(
            err,
            "gh: the write log {} could not be written",
            Path::new(log).display()
        );
    }
    u8::try_from(rc).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(words: &[&str]) -> Option<Write> {
        classify(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
    }

    fn w(kind: &str, repo: &str, number: &str) -> Option<Write> {
        Some(Write {
            kind: kind.into(),
            repo: repo.into(),
            number: number.into(),
        })
    }

    #[test]
    fn a_refused_write_is_named_by_its_kind_repository_and_number() {
        let r = |words: &[&str]| refused(&c(words).unwrap());
        assert_eq!(
            r(&["issue", "edit", "7", "-R", "o/r", "--add-label", "x"]),
            "gh issue edit o/r#7"
        );
        assert_eq!(
            r(&["label", "create", "x", "-R", "o/r"]),
            "gh label create o/r"
        );
        assert_eq!(r(&["pr", "comment", "3"]), "gh pr comment #3");
        assert_eq!(r(&["api", "-X", "PATCH", "user"]), "gh api PATCH");
    }

    #[test]
    fn read_only_runs_only_the_known_reads() {
        let r = |words: &[&str]| reads(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>());
        for words in [
            &["issue", "view", "12"][..],
            &["pr", "checks", "3"],
            &["repo", "view", "o/r"],
            &["release", "view", "improve", "--repo", "o/r"],
            &["api", "repos/o/r"],
            &["api", "-X", "GET", "repos/o/r/issues"],
            &["api", "graphql", "-f", "query=query { viewer { login } }"],
            &["auth", "git-credential"],
            // `tehas setup` clones the product and sets up git's credentials.
            &["repo", "clone", "o/r", "/tmp/r"],
            &["auth", "setup-git"],
            &["search", "issues", "x"],
        ] {
            assert!(r(words), "{words:?}");
        }
        for words in [
            &["release", "create", "improve", "--prerelease"][..],
            &[
                "release",
                "upload",
                "improve",
                "a.log",
                "--repo",
                "o/r",
                "--clobber",
            ],
            &["release", "edit", "improve"],
            &["release", "delete", "improve"],
            &["repo", "create", "o/r"],
            &["workflow", "run", "ci.yml"],
            &["secret", "set", "X"],
            &["issue", "comment", "7"],
            &["api", "-X", "POST", "repos/o/r/issues"],
            &["api", "graphql", "-f", "query=mutation { x }"],
            &[],
        ] {
            assert!(!r(words), "{words:?}");
        }
    }

    #[test]
    fn a_refused_call_no_write_names_is_told_by_its_first_words() {
        let o = |words: &[&str]| {
            refused(&other(
                &words.iter().map(|w| w.to_string()).collect::<Vec<_>>(),
            ))
        };
        assert_eq!(
            o(&[
                "release",
                "upload",
                "improve",
                "a.log",
                "--repo",
                "o/r",
                "--clobber"
            ]),
            "gh release upload o/r"
        );
        assert_eq!(
            o(&["release", "create", "improve", "--prerelease"]),
            "gh release create"
        );
        assert_eq!(o(&["workflow", "-R=o/r", "x"]), "gh workflow o/r");
    }

    #[test]
    fn an_issue_comment_is_a_write_with_its_repository_and_number() {
        assert_eq!(
            c(&["issue", "comment", "12", "-R", "o/r", "--body", "secret"]),
            w("issue comment", "o/r", "12")
        );
    }

    #[test]
    fn an_api_write_names_the_repository_and_number_of_its_path() {
        assert_eq!(
            c(&[
                "api",
                "-X",
                "POST",
                "repos/o/r/issues/12/labels",
                "-f",
                "labels[]=x"
            ]),
            w("api POST", "o/r", "12")
        );
    }

    #[test]
    fn fields_and_no_method_post() {
        assert_eq!(
            c(&["api", "repos/o/r/issues/7/comments", "-f", "body=hello"]),
            w("api POST", "o/r", "7")
        );
        assert_eq!(
            c(&["api", "repos/a/b/git/refs", "-F", "name=x"]),
            w("api POST", "a/b", "")
        );
        assert_eq!(c(&["api", "repos/o/r"]), None);
    }

    #[test]
    fn the_method_forms_name_the_same_kind() {
        assert_eq!(c(&["api", "-XPUT", "repos/a/b"]), w("api PUT", "a/b", ""));
        assert_eq!(
            c(&["api", "--method=PUT", "repos/a/b"]),
            w("api PUT", "a/b", "")
        );
        assert_eq!(c(&["api", "-Xput", "repos/a/b"]), w("api PUT", "a/b", ""));
        assert_eq!(
            c(&["api", "--method=delete", "repos/a/b"]),
            w("api DELETE", "a/b", "")
        );
        assert_eq!(
            c(&["api", "-X", "DELETE", "repos/a/b"]),
            w("api DELETE", "a/b", "")
        );
    }

    #[test]
    fn reads_are_no_writes() {
        for words in [
            &["issue", "view", "12"][..],
            &["api", "repos/o/r"],
            &["pr", "view", "3", "--repo", "o/r"],
            &["issue", "list", "--repo", "o/r"],
            &["pr", "status"],
            &["label", "list", "--repo", "o/r"],
            &["label", "get", "tehas:ready"],
            &["auth", "status"],
            &["auth", "git-credential"],
            &[],
        ] {
            assert_eq!(c(words), None, "{words:?}");
        }
    }

    #[test]
    fn the_kinds_are_the_two_words_of_the_call() {
        assert_eq!(
            c(&["pr", "comment", "8", "-R", "o/r"]),
            w("pr comment", "o/r", "8")
        );
        assert_eq!(
            c(&["issue", "create", "--repo", "o/r", "--title", "T"]),
            w("issue create", "o/r", "")
        );
        assert_eq!(c(&["issue", "close", "3"]), w("issue close", "", "3"));
        assert_eq!(
            c(&["pr", "reopen", "11", "--repo=o/r"]),
            w("pr reopen", "o/r", "11")
        );
        assert_eq!(c(&["pr", "ready", "6"]), w("pr ready", "", "6"));
        assert_eq!(
            c(&["pr", "merge", "5", "--repo", "o/r"]),
            w("pr merge", "o/r", "5")
        );
        assert_eq!(
            c(&["issue", "edit", "4", "-R", "o/r", "--title", "T"]),
            w("issue edit", "o/r", "4")
        );
        assert_eq!(
            c(&["label", "create", "tehas:ready", "--repo", "o/r"]),
            w("label create", "o/r", "")
        );
        assert_eq!(
            c(&["label", "edit", "tehas:ready", "--repo", "o/r"]),
            w("label edit", "o/r", "")
        );
        assert_eq!(
            c(&["label", "delete", "tehas:ready", "-R", "o/r"]),
            w("label delete", "o/r", "")
        );
        assert_eq!(c(&["label", "create", "42"]), w("label create", "", ""));
    }

    #[test]
    fn the_number_is_a_word_of_only_digits() {
        // A word that only ends in a digit is no number, so a title never
        // reaches the line.
        assert_eq!(
            c(&["issue", "create", "--title", "v2"]),
            w("issue create", "", "")
        );
        assert_eq!(
            c(&["issue", "comment", "1a2", "7"]),
            w("issue comment", "", "7")
        );
    }

    #[test]
    fn graphql_queries_are_reads() {
        for q in [
            "query { viewer { login } }",
            "query($owner: String!, $name: String!) {\n  repository(owner: $owner, name: $name) { pullRequests(states: OPEN, first: 100) { nodes { mutation } } } }",
            "query AddLabel { addLabels(subjectId: \"x\") { clientMutationId } }",
            "AddLabel { addLabels(subjectId: \"x\") { clientMutationId } }",
            "fragment F on Issue { mutation }",
            "{ viewer { mutationField } }",
            "mutations { x }",
        ] {
            let query = format!("query={q}");
            assert_eq!(c(&["api", "graphql", "-f", &query]), None, "{q}");
        }
        assert_eq!(
            c(&[
                "api",
                "graphql",
                "-F",
                "owner=o",
                "-F",
                "name=r",
                "-f",
                "query=query { a }"
            ]),
            None
        );
        assert_eq!(c(&["api", "graphql", "-F", "query=query { a }"]), None);
        assert_eq!(c(&["api", "graphql", "-F", "query=mutation { x }"]), None);
        assert_eq!(
            c(&[
                "api",
                "graphql",
                "-f",
                "q=repo:o/r is:issue is:closed",
                "-f",
                "query=query { a }"
            ]),
            None
        );
        assert_eq!(
            c(&["api", "graphql", "-XPOST", "-f", "query=query { a }"]),
            None
        );
        assert_eq!(
            c(&["api", "graphql", "--input=x", "-f", "query=query { a }"]),
            None
        );
    }

    #[test]
    fn graphql_mutations_are_api_posts() {
        for q in [
            "mutation { x }",
            "MUTATION { x }",
            "\n    AddLabelling\n\n    mutation\n    ($o: String!) {\n      setLabels(subjectId: $o) { clientMutationId }\n    }",
            "mutation($n: Int!) {\n      addLabels(subjectId: $n, labelIds: [\"mutation\"]) { clientMutationId }\n    }",
            "AddLabel mutation($n: Int!) { updateLabels(subjectId: $n) { x } }",
        ] {
            let query = format!("query={q}");
            assert_eq!(c(&["api", "graphql", "-f", &query]), w("api POST", "", ""), "{q}");
        }
    }

    #[test]
    fn a_rest_call_is_told_by_its_method_and_fields() {
        assert_eq!(
            c(&["api", "repos/o/r/branches", "-f", "query=mutation { x }"]),
            w("api POST", "o/r", "")
        );
        assert_eq!(
            c(&[
                "api",
                "-X",
                "POST",
                "repos/o/r/issues/12/comments?mutation=1",
                "-f",
                "body=x"
            ]),
            w("api POST", "o/r", "12")
        );
        assert_eq!(
            c(&["api", "-X", "PATCH", "repos/o/r/pulls/9/"]),
            w("api PATCH", "o/r", "9")
        );
        assert_eq!(
            c(&["api", "-X", "POST", "repos/o/r", "-R", "x/y"]),
            w("api POST", "x/y", "")
        );
    }

    #[test]
    fn the_marker_is_403_before_429() {
        assert_eq!(marker(b"HTTP 429: Too Many Requests"), "429");
        assert_eq!(
            marker(b"HTTP 403: Resource not accessible by integration"),
            "403"
        );
        assert_eq!(marker(b"HTTP 403: forbidden\nHTTP 429: too many"), "403");
        assert_eq!(marker(b"HTTP 404: Not Found"), "-");
        assert_eq!(marker(b""), "-");
    }

    #[test]
    fn the_line_has_six_fields_of_one_word_each() {
        let l = line(
            0,
            &Write {
                kind: "issue comment".into(),
                repo: "o/\tr\nx".into(),
                number: String::new(),
            },
            1,
            "429",
        );
        assert_eq!(
            l,
            "1970-01-01T00:00:00Z\tissue comment\to/ r x\t-\t1\t429\n"
        );
    }
}
