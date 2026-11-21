// The shared checkout: factory_git_log, factory_git_show and
// factory_git_blame over a repository each test makes with git.

use mcp::git::{ask, call_in, fetch_report, open, BLAME, FETCH_FILE, LOG, SHOW};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use tehas_core::testdir::TestDir;

/// git in `dir`, with no config but the repository's own and fixed times,
/// so the commits of a fixture are the same on every machine.
fn git(dir: &Path, args: &[&str], when: u32) -> String {
    let date = format!("{} +0000", 1_700_000_000 + when * 60);
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Ada")
        .env("GIT_AUTHOR_EMAIL", "ada@example.org")
        .env("GIT_COMMITTER_NAME", "Ada")
        .env("GIT_COMMITTER_EMAIL", "ada@example.org")
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, file: &str, body: &str, message: &str, when: u32) -> String {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    git(dir, &["add", "-A"], when);
    git(dir, &["commit", "-q", "-m", message], when);
    git(dir, &["rev-parse", "HEAD"], when)
}

/// Three commits: a.txt made, b.txt made, a.txt changed.
fn fixture(name: &str) -> (TestDir, Vec<String>) {
    let t = TestDir::new(name);
    git(&t, &["init", "-q", "-b", "main"], 0);
    let c1 = commit(
        &t,
        "a.txt",
        "one\ntwo\nthree\n",
        "Make a\n\nThe first file.",
        1,
    );
    let c2 = commit(&t, "src/b.txt", "bee\n", "Make b", 2);
    let c3 = commit(&t, "a.txt", "one\nTWO\nthree\nfour\n", "Change a", 3);
    (t, vec![c1, c2, c3])
}

fn ask_tool(dir: &Path, tool: &str, args: Value) -> Result<Value, String> {
    let repo = open(dir).unwrap();
    call_in(&repo, tool, &args)
}

fn ids(v: &Value) -> Vec<String> {
    v["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["commit"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_log_is_newest_first_from_head_and_names_it() {
    let (t, c) = fixture("mcp-git-log");
    let v = ask_tool(&t, LOG, json!({})).unwrap();
    assert_eq!(ids(&v), vec![c[2].clone(), c[1].clone(), c[0].clone()]);
    assert_eq!(v["head"], c[2].as_str());
    assert_eq!(v["more"], false);
    let first = &v["commits"][2];
    assert_eq!(first["subject"], "Make a");
    assert_eq!(first["body"], "The first file.");
    assert_eq!(first["author"], "Ada");
    assert_eq!(first["parents"], json!([]));
    assert_eq!(first["time"], "2023-11-14T22:14:20Z");
    // The address is a person's, and never served.
    assert!(!v.to_string().contains("ada@example.org"), "{v}");
    assert!(v["untrusted"].as_str().unwrap().contains("third-party"));
}

#[test]
fn the_log_of_a_path_holds_only_the_commits_that_changed_it() {
    let (t, c) = fixture("mcp-git-path");
    let v = ask_tool(&t, LOG, json!({"path": "a.txt"})).unwrap();
    assert_eq!(ids(&v), vec![c[2].clone(), c[0].clone()]);
    let v = ask_tool(&t, LOG, json!({"path": "src/b.txt"})).unwrap();
    assert_eq!(ids(&v), vec![c[1].clone()]);
    let v = ask_tool(&t, LOG, json!({"path": "src"})).unwrap();
    assert_eq!(ids(&v), vec![c[1].clone()]);
    let v = ask_tool(&t, LOG, json!({"path": "nothing.txt"})).unwrap();
    assert_eq!(ids(&v), Vec::<String>::new());
}

#[test]
fn grep_finds_the_text_in_a_message_as_given() {
    let (t, c) = fixture("mcp-git-grep");
    let v = ask_tool(&t, LOG, json!({"grep": "first file"})).unwrap();
    assert_eq!(ids(&v), vec![c[0].clone()]);
    let v = ask_tool(&t, LOG, json!({"grep": "FIRST"})).unwrap();
    assert_eq!(ids(&v), Vec::<String>::new());
}

#[test]
fn a_page_of_the_log_goes_on_after_its_next() {
    let (t, c) = fixture("mcp-git-page");
    let v = ask_tool(&t, LOG, json!({"limit": 2})).unwrap();
    assert_eq!(ids(&v), vec![c[2].clone(), c[1].clone()]);
    assert_eq!(v["more"], true);
    let next = v["next"].as_str().unwrap().to_string();
    let v = ask_tool(&t, LOG, json!({"limit": 2, "after": next})).unwrap();
    assert_eq!(ids(&v), vec![c[0].clone()]);
    assert_eq!(v["more"], false);
    // An abbreviated id is a commit as well.
    let v = ask_tool(&t, LOG, json!({"after": &c[1][..8]})).unwrap();
    assert_eq!(ids(&v), vec![c[0].clone()]);
}

#[test]
fn show_gives_the_message_the_files_and_the_diff() {
    let (t, c) = fixture("mcp-git-show");
    let v = ask_tool(&t, SHOW, json!({"commit": c[2]})).unwrap();
    assert_eq!(v["subject"], "Change a");
    assert_eq!(v["files"], json!([{"path": "a.txt", "change": "modified"}]));
    assert_eq!(v["cut"], false);
    let diff = v["diff"].as_str().unwrap();
    assert!(
        diff.starts_with("diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ "),
        "{diff}"
    );
    for line in ["-two\n", "+TWO\n", "+four\n", " one\n"] {
        assert!(diff.contains(line), "{line:?} in {diff}");
    }
    let v = ask_tool(&t, SHOW, json!({"commit": c[0]})).unwrap();
    assert_eq!(v["files"], json!([{"path": "a.txt", "change": "added"}]));
    assert!(v["diff"]
        .as_str()
        .unwrap()
        .contains("--- /dev/null\n+++ b/a.txt\n"));
}

#[test]
fn a_large_diff_is_cut_and_says_so() {
    let (t, _) = fixture("mcp-git-cut");
    let big: String = (0..20_000).map(|i| format!("line {i}\n")).collect();
    let id = commit(&t, "big.txt", &big, "Big", 4);
    let v = ask_tool(&t, SHOW, json!({"commit": id})).unwrap();
    assert_eq!(v["cut"], true);
    assert!(v["diff"].as_str().unwrap().len() <= 64 * 1024);
}

#[test]
fn blame_matches_git_blame_line_by_line() {
    let (t, c) = fixture("mcp-git-blame");
    let v = ask_tool(&t, BLAME, json!({"path": "a.txt"})).unwrap();
    let porcelain = git(&t, &["blame", "--line-porcelain", "a.txt"], 0);
    let want: Vec<String> = porcelain
        .lines()
        .filter(|l| l.len() > 40 && l.as_bytes()[..40].iter().all(u8::is_ascii_hexdigit))
        .map(|l| l[..40].to_string())
        .collect();
    let got: Vec<String> = v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["commit"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(got, want);
    assert_eq!(
        got,
        vec![c[0].clone(), c[2].clone(), c[0].clone(), c[2].clone()]
    );
    assert_eq!(
        v["lines"][1],
        json!({"line": 2, "commit": c[2], "text": "TWO"})
    );
    assert_eq!(v["total"], 4);
    assert_eq!(v["commits"][&c[0]]["subject"], "Make a");
    // A range is the lines asked for, numbered as in the file.
    let v = ask_tool(&t, BLAME, json!({"path": "a.txt", "start": 2, "end": 3})).unwrap();
    let lines: Vec<u64> = v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![2, 3]);
    assert!(ask_tool(&t, BLAME, json!({"path": "a.txt", "start": 9})).is_err());
    assert!(ask_tool(&t, BLAME, json!({"path": "a.txt", "start": 3, "end": 2})).is_err());
    assert!(ask_tool(&t, BLAME, json!({"path": "src"})).is_err());
}

#[test]
fn commit_text_and_code_are_masked() {
    let (t, _) = fixture("mcp-git-mask");
    let token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let id = commit(
        &t,
        "c.txt",
        &format!("key={token}\n"),
        &format!("Leak {token}"),
        4,
    );
    let v = ask_tool(&t, SHOW, json!({"commit": id})).unwrap();
    assert!(!v.to_string().contains(token), "{v}");
    assert!(v["subject"].as_str().unwrap().contains("ghp_***"), "{v}");
    let v = ask_tool(&t, BLAME, json!({"path": "c.txt"})).unwrap();
    assert!(!v.to_string().contains(token), "{v}");
    let v = ask_tool(&t, LOG, json!({"limit": 1})).unwrap();
    assert!(!v.to_string().contains(token), "{v}");
}

// The checkout's own config may name a textconv, an external diff or a
// filter. None of them is run: the marker each would write stays absent.
#[test]
fn no_program_the_checkout_names_is_run() {
    let (t, _) = fixture("mcp-git-programs");
    let marker = t.join("ran");
    std::fs::write(t.join(".gitattributes"), "* diff=evil filter=evil\n").unwrap();
    git(&t, &["add", ".gitattributes"], 4);
    git(&t, &["commit", "-q", "-m", "Attributes"], 4);
    let id = commit(&t, "a.txt", "one\nchanged\n", "Change a again", 5);
    // Set last: git itself would run them for the commits above.
    let run = format!("sh -c 'touch {}'", marker.display());
    for (k, v) in [
        ("diff.evil.textconv", run.as_str()),
        ("diff.external", run.as_str()),
        ("filter.evil.smudge", run.as_str()),
        ("filter.evil.clean", run.as_str()),
        ("core.fsmonitor", run.as_str()),
    ] {
        git(&t, &["config", k, v], 0);
    }
    // The program is one that runs: git's own diff runs it.
    git(&t, &["show", "--ext-diff", "--textconv", &id], 0);
    assert!(marker.exists(), "the fixture's program does not run");
    std::fs::remove_file(&marker).unwrap();
    let v = ask_tool(&t, SHOW, json!({"commit": id})).unwrap();
    assert!(v["diff"].as_str().unwrap().contains("+changed\n"), "{v}");
    ask_tool(&t, BLAME, json!({"path": "a.txt"})).unwrap();
    ask_tool(&t, LOG, json!({"path": "a.txt"})).unwrap();
    assert!(
        !marker.exists(),
        "a program of the checkout's config was run"
    );
}

#[test]
fn arguments_are_checked() {
    let (t, c) = fixture("mcp-git-args");
    for (tool, args) in [
        (LOG, json!({"path": "../x"})),
        (LOG, json!({"path": "/etc/passwd"})),
        (LOG, json!({"limit": 0})),
        (LOG, json!({"limit": 201})),
        (LOG, json!({"grep": ""})),
        (LOG, json!({"after": "0000000"})),
        (LOG, json!({"what": 1})),
        (SHOW, json!({})),
        (SHOW, json!({"commit": "HEAD~1"})),
        (SHOW, json!({"commit": "HEAD"})),
        (SHOW, json!({"commit": &c[0][..6]})),
        (SHOW, json!({"commit": c[0].to_uppercase()})),
        (BLAME, json!({})),
        (BLAME, json!({"path": "a.txt", "start": 0})),
        (BLAME, json!({"path": "missing.txt"})),
    ] {
        assert!(ask_tool(&t, tool, args.clone()).is_err(), "{tool} {args}");
    }
    assert!(ask_tool(&t, "factory_git_push", json!({})).is_err());
}

#[test]
fn a_shallow_clone_a_directory_and_an_empty_repository_are_refused() {
    let (t, _) = fixture("mcp-git-refused");
    let shallow = TestDir::new("mcp-git-shallow");
    let url = format!("file://{}", t.display());
    git(&shallow, &["clone", "-q", "--depth", "1", &url, "c"], 0);
    let why = open(&shallow.join("c")).err().unwrap();
    assert!(why.contains("shallow"), "{why}");
    let plain = TestDir::new("mcp-git-plain");
    assert!(open(&plain).err().unwrap().contains("not a git repository"));
    git(&plain, &["init", "-q"], 0);
    assert!(open(&plain).err().unwrap().contains("no commit"));
    // A whole clone is served.
    let whole = TestDir::new("mcp-git-whole");
    git(&whole, &["clone", "-q", &url, "c"], 0);
    assert!(open(&whole.join("c")).is_ok());
}

#[test]
fn a_seen_head_that_moved_is_told_with_the_commits_since() {
    let (t, c) = fixture("mcp-git-seen");
    let v = ask_tool(&t, LOG, json!({"limit": 1, "seen": c[2]})).unwrap();
    assert!(v.get("checkoutUpdated").is_none(), "{v}");
    let v = ask_tool(&t, BLAME, json!({"path": "a.txt", "seen": &c[0][..8]})).unwrap();
    assert_eq!(
        v["checkoutUpdated"],
        json!({"from": c[0], "to": c[2], "commits": 2})
    );
    // A head the clone does not know, as after a rewrite of its history.
    let v = ask_tool(&t, SHOW, json!({"commit": c[1], "seen": "1234567"})).unwrap();
    assert_eq!(v["checkoutUpdated"], json!({"from": "1234567", "to": c[2]}));
    assert!(ask_tool(&t, LOG, json!({"seen": "HEAD"})).is_err());
}

#[test]
fn the_report_of_the_fetch_is_copied_by_its_own_keys() {
    let d = TestDir::new("mcp-git-report");
    assert_eq!(fetch_report(&d), None);
    std::fs::write(
        d.join(FETCH_FILE),
        r#"{"api":1,"time":"2026-11-21T10:00:00Z","ok":false,"error":"origin said no to ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ","path":"/srv/secret"}"#,
    )
    .unwrap();
    assert_eq!(
        fetch_report(&d),
        Some(
            json!({"time": "2026-11-21T10:00:00Z", "ok": false, "error": "origin said no to ghp_***"})
        )
    );
    for bad in [
        r#"{"time":"soon","ok":true}"#,
        r#"{"time":"2026-11-21T10:00:00Z"}"#,
        "not json",
    ] {
        std::fs::write(d.join(FETCH_FILE), bad).unwrap();
        assert_eq!(fetch_report(&d), None, "{bad}");
    }
}

#[test]
fn a_request_for_a_fetch_is_a_connection_and_nothing_written() {
    use std::io::Read;
    let d = TestDir::new("mcp-git-ask");
    let socket = d.join("fetch.sock");
    assert!(!ask(&socket), "no socket takes no request");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert!(ask(&socket));
    let (mut conn, _) = listener.accept().unwrap();
    let mut got = Vec::new();
    conn.read_to_end(&mut got).unwrap();
    assert!(got.is_empty(), "{got:?}");
}
