//! The issue relay of a cycle. A cycle's token reads issues and cannot write
//! them (#1814), so a label or a `<!-- tehas: -->` marker the bot writes is
//! the host's own. The three issue writes a cycle's agents make, `gh issue
//! create`, `gh issue edit` and `gh issue comment`, go to the host instead:
//! the container's gh (host/sandbox/gh-shim) leaves a request in the cycle's
//! relay directory, and this runs it with the app's token when it is one of
//! them and carries a title, a body and an issue number only.
//!
//! The directory is `<workspace>.state/relay` on the host and
//! `/run/tehas/relay` in the container, which can write anything there, so
//! every request is read as hostile:
//!   req.<pid>/args   the arguments from the verb on, each ended by a NUL
//!   req.<pid>/body   the file of --body-file/-F, copied by the shim
//!   req.<pid>/ready  made last: the request is complete
//!   res.<pid>/out, err, rc, then done   the answer, written here
//! `<pid>` is one to ten digits; any other name is left alone. Every file is
//! opened relative to a directory opened without following a link, with
//! O_NOFOLLOW, and read only when it is a regular file of at most 1 MiB, so a
//! link to a file of the host is never read. The answer directory is made
//! here, after whatever stood at its name is removed, and its files are made
//! with O_EXCL inside it, so nothing is written outside the directory. gh runs
//! from an argument list made here, never through a shell, each value joined
//! to its flag (`--title=...`), with `--repo=TEHAS_REPO` always given and a
//! body file on its standard input. The request is removed before gh runs,
//! so it runs once.
//!
//! Refused, with the reason in the answer and status 1: any verb but the
//! three, any flag but --title/-t, --body/-b, --body-file/-F and --repo/-R,
//! naming the flag (labels, assignees, milestones, projects, state, --add-*,
//! --remove-*, --delete-last, --edit-last among them); a --repo that is not
//! TEHAS_REPO; anything but one issue number in digits for edit and comment;
//! and a title or body with a tehas marker in it. A marker is found after
//! this normalisation: at every `<!--`, the characters that follow for which
//! Unicode says white space are skipped, and the next six are compared with
//! `tehas:` without regard to ASCII case. So `<!--  TEHAS:` is one, wherever
//! it stands: in a code block or a quoted email too.
//!
//! The token is asked of `tehasbot github-token` for every request: the one
//! the start had runs out after an hour, and that command keeps a token with
//! at least ten minutes left.

use rustix::fs::{AtFlags, FileType, Mode, OFlags};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The most a body file, or the arguments, may hold.
pub const MAX_BODY: u64 = 1 << 20;

pub const MARKER: &str = "gh-shim: the relay refuses text with a tehas marker (<!-- tehas:)";

/// How often the directory is read.
const POLL: Duration = Duration::from_millis(500);

/// How long one gh may take. The shim waits 120 s.
const GH_WAIT: Duration = Duration::from_secs(100);

fn has_marker(text: &str) -> bool {
    text.match_indices("<!--").any(|(at, _)| {
        let rest = text[at + 4..].trim_start_matches(char::is_whitespace);
        rest.get(..6)
            .is_some_and(|w| w.eq_ignore_ascii_case("tehas:"))
    })
}

fn no(why: &str) -> String {
    format!("gh-shim: the relay {why}")
}

/// The flags the relay takes, by their long name.
fn long(flag: &str) -> Option<&'static str> {
    match flag {
        "--title" | "-t" => Some("--title"),
        "--body" | "-b" => Some("--body"),
        "--body-file" | "-F" => Some("--body-file"),
        "--repo" | "-R" => Some("--repo"),
        _ => None,
    }
}

/// The argument list gh runs with for a request, or why it is refused. `args`
/// start at the verb; `body` is the request's body file. A body file goes to
/// gh as `--body-file -`, its text on standard input.
pub fn allowed(args: &[String], body: Option<&str>, repo: &str) -> Result<Vec<String>, String> {
    let verb = match args {
        [issue, verb, ..] if issue == "issue" => verb.as_str(),
        _ => "",
    };
    if !matches!(verb, "create" | "edit" | "comment") {
        return Err(no("runs issue create, issue edit and issue comment only"));
    }
    let (mut title, mut text, mut file, mut repo_given, mut number) =
        (None, None, false, None, None);
    let mut rest = args[2..].iter();
    while let Some(a) = rest.next() {
        if a.starts_with('-') && a.len() > 1 {
            let (name, inline) = match a.split_once('=') {
                Some((n, v)) if a.starts_with("--") => (n, Some(v.to_string())),
                _ => (a.as_str(), None),
            };
            let Some(flag) = long(name) else {
                let name = if name.starts_with("--") {
                    name
                } else {
                    &name[..2.min(name.len())]
                };
                return Err(no(&format!(
                    "refuses {name}: an issue write of a cycle takes a title, a body and an issue number only"
                )));
            };
            let Some(value) = inline.or_else(|| rest.next().cloned()) else {
                return Err(no(&format!("needs a value for {flag}")));
            };
            let twice = match flag {
                "--title" => title.replace(value).is_some(),
                "--body" => text.replace(value).is_some() || file,
                "--body-file" => std::mem::replace(&mut file, true) || text.is_some(),
                _ => repo_given.replace(value).is_some(),
            };
            if twice {
                return Err(no("takes one title, one body and one --repo"));
            }
        } else if verb != "create"
            && number.is_none()
            && !a.is_empty()
            && a.len() <= 20
            && a.bytes().all(|b| b.is_ascii_digit())
        {
            number = Some(a.clone());
        } else {
            return Err(no(
                "takes one issue number in digits, and none for issue create",
            ));
        }
    }
    if verb != "create" && number.is_none() {
        return Err(no(
            "takes one issue number in digits, and none for issue create",
        ));
    }
    if let Some(r) = &repo_given {
        if !r.eq_ignore_ascii_case(repo) {
            return Err(no(&format!("writes to {repo} only, not {r}")));
        }
    }
    if file && body.is_none() {
        return Err(no("got no body file for --body-file"));
    }
    let texts = [title.as_deref(), text.as_deref(), body];
    if texts.into_iter().flatten().any(has_marker) {
        return Err(MARKER.to_string());
    }
    // Each value joined to its flag, so a title such as `--label` stays a title.
    let mut argv = vec!["issue".to_string(), verb.to_string()];
    argv.extend(number);
    argv.extend(title.map(|t| format!("--title={t}")));
    argv.extend(text.map(|t| format!("--body={t}")));
    if file {
        argv.push("--body-file=-".to_string());
    }
    argv.push(format!("--repo={repo}"));
    Ok(argv)
}

/// What the relay runs: gh, and the command that prints the app's token.
pub struct Via {
    pub gh: PathBuf,
    pub token: PathBuf,
    pub token_args: Vec<String>,
}

impl Via {
    /// gh from PATH (TEHAS_GH_BIN in tests) and `<home>/bin/tehasbot
    /// github-token`, the command the renewer asks too.
    pub fn from_env(home: &Path) -> Via {
        let gh = std::env::var_os("TEHAS_GH_BIN")
            .filter(|v| !v.is_empty())
            .map_or_else(|| PathBuf::from("gh"), PathBuf::from);
        Via {
            gh,
            token: home.join("bin/tehasbot"),
            token_args: vec!["github-token".to_string()],
        }
    }
}

/// Serves the relay directory `dir` for `repo` until `stop` is set, then
/// answers what is left with 1.
pub fn serve(dir: &Path, repo: &str, stop: &AtomicBool) {
    let home = PathBuf::from(std::env::var_os("TEHAS_HOME").unwrap_or_default());
    serve_via(dir, repo, stop, &Via::from_env(&home));
}

pub fn serve_via(dir: &Path, repo: &str, stop: &AtomicBool, via: &Via) {
    loop {
        let ending = stop.load(Ordering::SeqCst);
        pass(dir, repo, via, ending);
        if ending {
            return;
        }
        let until = Instant::now() + POLL;
        while Instant::now() < until && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// One reading of the directory: every complete request, answered.
fn pass(dir: &Path, repo: &str, via: &Via, ending: bool) {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let Ok(top) = rustix::fs::open(dir, flags, Mode::empty()) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut pids: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_prefix("req.").map(String::from))
        .filter(|p| (1..=10).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    pids.sort();
    for pid in pids {
        handle(dir, &top, &pid, repo, via, ending);
    }
}

fn handle(dir: &Path, top: &OwnedFd, pid: &str, repo: &str, via: &Via, ending: bool) {
    let name = format!("req.{pid}");
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let Ok(req) = rustix::fs::openat(top, &name, flags, Mode::empty()) else {
        // A link or a file in place of the directory: never followed, removed.
        let _ = rustix::fs::unlinkat(top, &name, AtFlags::empty());
        return;
    };
    match regular(&req, "ready", 0) {
        Ok(Some(_)) => {}
        _ => return,
    }
    let args = regular(&req, "args", MAX_BODY).and_then(|a| {
        let a = a.unwrap_or_default();
        let a = String::from_utf8(a).map_err(|_| no("takes arguments of UTF-8 text"))?;
        let Some(a) = a.strip_suffix('\0').or(a.is_empty().then_some("")) else {
            return Err(no("takes arguments each ended by a NUL"));
        };
        Ok(a.split('\0').map(String::from).collect::<Vec<_>>())
    });
    let body = regular(&req, "body", MAX_BODY).and_then(|b| {
        b.map(|b| String::from_utf8(b).map_err(|_| no("takes a body of UTF-8 text")))
            .transpose()
    });
    drop(req);
    if std::fs::remove_dir_all(dir.join(&name)).is_err() {
        answer(dir, top, pid, "", &no("could not take the request"), 1);
        return;
    }
    let ran = args.and_then(|a| {
        let body = body?;
        let argv = allowed(&a, body.as_deref(), repo)?;
        Ok((argv, body))
    });
    let (argv, body) = match ran {
        Ok(r) => r,
        Err(why) => {
            eprintln!("tehas relay: refused request {pid}: {}", why.escape_debug());
            answer(dir, top, pid, "", &why, 1);
            return;
        }
    };
    if ending {
        answer(
            dir,
            top,
            pid,
            "",
            &no("did not run this: the cycle ended"),
            1,
        );
        return;
    }
    let (out, err, rc) = run(via, &argv, body.as_deref());
    eprintln!("tehas relay: gh {} {}: {rc}", argv[0], argv[1]);
    answer(dir, top, pid, &out, &err, rc);
}

/// The regular file `name` in `dir`, at most `max` bytes. None when there is
/// none; Err when it is a link, not a regular file, or too large.
fn regular(dir: &OwnedFd, name: &str, max: u64) -> Result<Option<Vec<u8>>, String> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rustix::fs::openat(dir, name, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(_) => return Err(no("takes regular files only")),
    };
    let st = rustix::fs::fstat(&fd).map_err(|_| no("takes regular files only"))?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile {
        return Err(no("takes regular files only"));
    }
    let mut data = Vec::new();
    let read = File::from(fd).take(max + 1).read_to_end(&mut data);
    if read.is_err() {
        return Err(no("could not read the request"));
    }
    if data.len() as u64 > max {
        return Err(no("takes a body and arguments of at most 1 MiB"));
    }
    Ok(Some(data))
}

/// gh with the app's token: its output, its error stream and its status.
fn run(via: &Via, argv: &[String], body: Option<&str>) -> (String, String, i32) {
    let token = Command::new(&via.token)
        .args(&via.token_args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty());
    let Some(token) = token else {
        return (String::new(), no("has no GitHub token from the host"), 1);
    };
    // timeout signals gh's whole process group, so a gh behind the host's
    // shim does not outlive it and hold the pipes.
    let wait = GH_WAIT.as_secs().to_string();
    let child = Command::new("timeout")
        .args(["-k", "10", &wait])
        .arg(&via.gh)
        .args(argv)
        .env("GH_TOKEN", token)
        .env("GH_PROMPT_DISABLED", "1")
        .current_dir("/")
        .stdin(if body.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return (String::new(), no("could not start gh"), 1);
    };
    let input = child.stdin.take().map(|mut i| {
        let body = body.unwrap_or_default().to_string();
        std::thread::spawn(move || {
            let _ = i.write_all(body.as_bytes());
        })
    });
    let done = child.wait_with_output();
    if let Some(t) = input {
        let _ = t.join();
    }
    let Ok(done) = done else {
        return (String::new(), no("lost gh"), 1);
    };
    let out = String::from_utf8_lossy(&done.stdout).into_owned();
    let mut err = String::from_utf8_lossy(&done.stderr).into_owned();
    let status = done.status.code().unwrap_or(1);
    if status == 124 {
        err.push_str(&no(&format!("stopped gh after {wait} s\n")));
    }
    (out, err, status)
}

/// The answer to request `pid`: made in a directory made here, so the
/// container cannot have it written anywhere else.
fn answer(dir: &Path, top: &OwnedFd, pid: &str, out: &str, err: &str, rc: i32) {
    let name = format!("res.{pid}");
    let gone = match rustix::fs::statat(top, &name, AtFlags::SYMLINK_NOFOLLOW) {
        Err(_) => true,
        // std's remove_dir_all removes a link it meets, never what it names.
        Ok(st) if FileType::from_raw_mode(st.st_mode) == FileType::Directory => {
            std::fs::remove_dir_all(dir.join(&name)).is_ok()
        }
        Ok(_) => rustix::fs::unlinkat(top, &name, AtFlags::empty()).is_ok(),
    };
    if !gone || rustix::fs::mkdirat(top, &name, Mode::from_raw_mode(0o700)).is_err() {
        eprintln!("tehas relay: could not answer request {pid}");
        return;
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let Ok(res) = rustix::fs::openat(top, &name, flags, Mode::empty()) else {
        eprintln!("tehas relay: could not answer request {pid}");
        return;
    };
    let rc = format!("{rc}\n");
    for (file, text) in [
        ("out", out),
        ("err", err),
        ("rc", rc.as_str()),
        ("done", ""),
    ] {
        if !write_new(&res, file, text) {
            eprintln!("tehas relay: could not answer request {pid}");
            return;
        }
    }
}

fn write_new(dir: &OwnedFd, name: &str, text: &str) -> bool {
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    match rustix::fs::openat(dir, name, flags, Mode::from_raw_mode(0o600)) {
        Ok(fd) => File::from(fd).write_all(text.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tehas_core::testdir::TestDir;

    const REPO: &str = "acme/widget";

    fn args(line: &[&str]) -> Vec<String> {
        line.iter().map(|s| s.to_string()).collect()
    }

    fn refused(line: &[&str], body: Option<&str>) -> String {
        allowed(&args(line), body, REPO).expect_err("refused")
    }

    #[test]
    fn create_with_title_and_body_runs() {
        assert_eq!(
            allowed(
                &args(&[
                    "issue",
                    "create",
                    "--title",
                    "A bug",
                    "--body",
                    "It breaks."
                ]),
                None,
                REPO
            ),
            Ok(args(&[
                "issue",
                "create",
                "--title=A bug",
                "--body=It breaks.",
                "--repo=acme/widget"
            ]))
        );
    }

    #[test]
    fn the_three_verbs_and_their_flags_in_every_spelling_run() {
        assert_eq!(
            allowed(
                &args(&["issue", "comment", "3", "-b", "hi", "-R", REPO]),
                None,
                REPO
            ),
            Ok(args(&[
                "issue",
                "comment",
                "3",
                "--body=hi",
                "--repo=acme/widget"
            ]))
        );
        assert_eq!(
            allowed(
                &args(&["issue", "edit", "--body-file=/tmp/b.md", "12", "-t", "New"]),
                Some("the text"),
                REPO
            ),
            Ok(args(&[
                "issue",
                "edit",
                "12",
                "--title=New",
                "--body-file=-",
                "--repo=acme/widget"
            ]))
        );
        assert_eq!(
            allowed(
                &args(&[
                    "issue",
                    "create",
                    "-t",
                    "T",
                    "-F",
                    "-",
                    "--repo=acme/widget"
                ]),
                Some(""),
                REPO
            ),
            Ok(args(&[
                "issue",
                "create",
                "--title=T",
                "--body-file=-",
                "--repo=acme/widget"
            ]))
        );
    }

    #[test]
    fn a_value_that_looks_like_a_flag_stays_a_value() {
        assert_eq!(
            allowed(
                &args(&["issue", "comment", "3", "--body", "--add-label=tehas:ready"]),
                None,
                REPO
            ),
            Ok(args(&[
                "issue",
                "comment",
                "3",
                "--body=--add-label=tehas:ready",
                "--repo=acme/widget"
            ]))
        );
    }

    #[test]
    fn labels_are_refused() {
        assert!(refused(&["issue", "edit", "3", "--label", "x"], None).contains("--label"));
        assert!(refused(&["issue", "edit", "3", "--add-label", "x"], None).contains("--add-label"));
        assert!(refused(&["issue", "edit", "3", "--add-label=x"], None).contains("--add-label"));
        assert!(
            refused(&["issue", "create", "-t", "a", "-b", "b", "-l", "x"], None).contains("-l")
        );
        assert!(refused(&["issue", "create", "-t", "a", "-b", "b", "-lx"], None).contains("-l"));
    }

    #[test]
    fn every_other_flag_is_refused_by_name() {
        for flag in [
            "--assignee",
            "--add-assignee",
            "--remove-assignee",
            "--milestone",
            "--remove-milestone",
            "--project",
            "--add-project",
            "--remove-label",
            "--state",
            "--delete-last",
            "--edit-last",
            "--web",
            "--editor",
            "--template",
            "--recover",
            "-a",
            "-m",
            "-p",
            "-w",
            "-e",
            "--",
        ] {
            let why = refused(&["issue", "comment", "3", flag, "x"], None);
            assert!(why.contains(flag), "{flag}: {why}");
        }
    }

    #[test]
    fn only_three_verbs_are_relayed() {
        for line in [
            &["issue", "close", "3"][..],
            &["issue", "reopen", "3"],
            &["issue", "delete", "3"],
            &["issue", "transfer", "3", "x/y"],
            &["issue", "lock", "3"],
            &["issue", "pin", "3"],
            &["issue", "develop", "3"],
            &["pr", "comment", "3", "--body", "x"],
            &["label", "create", "x"],
            &["api", "repos/acme/widget/issues/3/labels"],
            &["issue"],
            &[],
        ] {
            let why = allowed(&args(line), None, REPO).expect_err("refused");
            assert!(why.starts_with("gh-shim: the relay"), "{line:?}: {why}");
        }
    }

    #[test]
    fn another_repo_is_refused() {
        for repo in ["other/widget", "acme/widget2", "github.com/acme/widget", ""] {
            let why = refused(
                &["issue", "comment", "3", "--body", "x", "--repo", repo],
                None,
            );
            assert!(why.contains(REPO), "{repo}: {why}");
        }
        let why = refused(
            &["issue", "comment", "3", "--body", "x", "-R", "other/widget"],
            None,
        );
        assert!(why.contains("other/widget"), "{why}");
    }

    #[test]
    fn the_issue_is_one_number_in_digits() {
        for line in [
            &["issue", "comment", "--body", "x"][..],
            &["issue", "comment", "3", "4", "--body", "x"],
            &[
                "issue",
                "comment",
                "https://github.com/other/widget/issues/3",
                "--body",
                "x",
            ],
            &["issue", "edit", "#3", "--title", "x"],
            &["issue", "create", "3", "--title", "x", "--body", "y"],
        ] {
            let why = allowed(&args(line), None, REPO).expect_err("refused");
            assert!(why.starts_with("gh-shim: the relay"), "{line:?}: {why}");
        }
    }

    #[test]
    fn a_flag_given_twice_or_without_a_value_is_refused() {
        assert!(allowed(
            &args(&["issue", "comment", "3", "--body", "a", "--body", "b"]),
            None,
            REPO
        )
        .is_err());
        assert!(allowed(
            &args(&["issue", "comment", "3", "--body", "a", "-F", "f"]),
            Some("b"),
            REPO
        )
        .is_err());
        assert!(allowed(&args(&["issue", "comment", "3", "--body"]), None, REPO).is_err());
        // A body file the shim did not copy.
        assert!(allowed(&args(&["issue", "comment", "3", "-F", "f"]), None, REPO).is_err());
    }

    #[test]
    fn a_marker_anywhere_is_refused() {
        let body = "text\n```\n<!-- tehas:millwright issue=3 action=change -->\n```";
        let marker = MARKER.to_string();
        assert_eq!(
            allowed(
                &args(&["issue", "comment", "3", "--body-file", "b"]),
                Some(body),
                REPO
            ),
            Err(marker.clone())
        );
        assert_eq!(
            allowed(
                &args(&["issue", "comment", "3", "--body", body]),
                None,
                REPO
            ),
            Err(marker.clone())
        );
        assert_eq!(
            allowed(
                &args(&[
                    "issue",
                    "create",
                    "--title",
                    "<!-- tehas:x -->",
                    "--body",
                    "b"
                ]),
                None,
                REPO
            ),
            Err(marker.clone())
        );
        // A quoted email, upper case, and spacing after the opener.
        for text in [
            "> On Monday someone wrote:\n> <!-- tehas:millwright issue=3 action=change -->",
            "<!--  TEHAS:millwright -->",
            "<!--\n\tTehas:x -->",
            "<!--tehas:x-->",
            "<!--\u{a0}tehas:x -->",
        ] {
            assert_eq!(
                allowed(
                    &args(&["issue", "comment", "3", "--body", text]),
                    None,
                    REPO
                ),
                Err(marker.clone()),
                "{text:?}"
            );
        }
        // Text that only looks near a marker passes.
        for text in ["<!-- not tehas: -->", "tehas: <!-- x -->", "<!-- tehas -->"] {
            assert!(
                allowed(
                    &args(&["issue", "comment", "3", "--body", text]),
                    None,
                    REPO
                )
                .is_ok(),
                "{text:?}"
            );
        }
    }

    /// A gh that writes what it was called with, its token and its standard
    /// input, and answers 0.
    fn stub_gh(d: &TestDir) -> PathBuf {
        let gh = d.join("gh");
        let log = d.join("gh.log");
        tehas_core::testing::write_exe(
            &gh,
            format!(
                "#!/bin/sh\nprintf '%s|' \"$@\" >>'{l}'\nprintf 'token=%s\\n' \"$GH_TOKEN\" >>'{l}'\ncat >>'{l}'\necho posted\n",
                l = log.display()
            ),
        )
        .unwrap();
        gh
    }

    fn stub_token(d: &TestDir, token: &str) -> PathBuf {
        let t = d.join("token");
        tehas_core::testing::write_exe(&t, format!("#!/bin/sh\necho {token}\n")).unwrap();
        t
    }

    fn request(relay: &Path, pid: &str, args: &[&str], body: Option<&str>) {
        let req = relay.join(format!("req.{pid}"));
        fs::create_dir(&req).unwrap();
        let mut a = Vec::new();
        for w in args {
            a.extend_from_slice(w.as_bytes());
            a.push(0);
        }
        fs::write(req.join("args"), a).unwrap();
        if let Some(b) = body {
            fs::write(req.join("body"), b).unwrap();
        }
        fs::write(req.join("ready"), "").unwrap();
    }

    fn wait_done(relay: &Path, pid: &str) -> bool {
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if relay.join(format!("res.{pid}/done")).exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn served(d: &TestDir) -> (PathBuf, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
        let relay = d.join("relay");
        fs::create_dir(&relay).unwrap();
        let via = Via {
            gh: stub_gh(d),
            token: stub_token(d, "ghs_full"),
            token_args: vec![],
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (r, s) = (relay.clone(), stop.clone());
        let t = std::thread::spawn(move || serve_via(&r, REPO, &s, &via));
        (relay, stop, t)
    }

    fn read(p: PathBuf) -> String {
        fs::read_to_string(p).unwrap_or_default()
    }

    #[test]
    fn serve_answers_a_request() {
        let d = TestDir::new("relay-serve");
        let (relay, stop, t) = served(&d);
        request(
            &relay,
            "42",
            &["issue", "comment", "3", "--body", "hi"],
            None,
        );
        assert!(wait_done(&relay, "42"));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.42/rc")), "0\n");
        assert_eq!(read(relay.join("res.42/out")), "posted\n");
        assert!(!relay.join("req.42").exists());
        assert_eq!(
            read(d.join("gh.log")),
            "issue|comment|3|--body=hi|--repo=acme/widget|token=ghs_full\n"
        );
    }

    #[test]
    fn serve_gives_a_body_file_on_standard_input() {
        let d = TestDir::new("relay-body");
        let (relay, stop, t) = served(&d);
        request(
            &relay,
            "7",
            &["issue", "edit", "3", "--body-file", "/x/b.md"],
            Some("line one\nline two\n"),
        );
        assert!(wait_done(&relay, "7"));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.7/rc")), "0\n");
        assert_eq!(
            read(d.join("gh.log")),
            "issue|edit|3|--body-file=-|--repo=acme/widget|token=ghs_full\nline one\nline two\n"
        );
    }

    #[test]
    fn serve_refuses_without_running_gh() {
        let d = TestDir::new("relay-refuse");
        let (relay, stop, t) = served(&d);
        request(
            &relay,
            "8",
            &["issue", "edit", "3", "--add-label", "tehas:ready"],
            None,
        );
        assert!(wait_done(&relay, "8"));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.8/rc")), "1\n");
        assert!(read(relay.join("res.8/err")).contains("--add-label"));
        assert!(!d.join("gh.log").exists());
    }

    #[test]
    fn a_symlink_or_an_oversized_body_is_refused() {
        let d = TestDir::new("relay-hostile");
        let secret = d.join("secret");
        fs::write(&secret, "the key").unwrap();
        let (relay, stop, t) = served(&d);
        // A body that names a file of the host.
        let req = relay.join("req.9");
        fs::create_dir(&req).unwrap();
        fs::write(req.join("args"), "issue\0comment\x003\0-F\0b\0").unwrap();
        std::os::unix::fs::symlink(&secret, req.join("body")).unwrap();
        fs::write(req.join("ready"), "").unwrap();
        // A body over the bound.
        request(
            &relay,
            "10",
            &["issue", "comment", "3", "-F", "b"],
            Some(&"x".repeat(MAX_BODY as usize + 1)),
        );
        // A request directory that is a link to another directory.
        let elsewhere = d.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("args"), "issue\0comment\x003\0--body\0x\0").unwrap();
        fs::write(elsewhere.join("ready"), "").unwrap();
        std::os::unix::fs::symlink(&elsewhere, relay.join("req.11")).unwrap();
        // An answer directory the container made in advance, as a link.
        let trap = d.join("trap");
        fs::create_dir(&trap).unwrap();
        std::os::unix::fs::symlink(&trap, relay.join("res.12")).unwrap();
        request(
            &relay,
            "12",
            &["issue", "comment", "3", "--body", "x"],
            None,
        );
        // A name that is no process id.
        request(
            &relay,
            "x1",
            &["issue", "comment", "3", "--body", "x"],
            None,
        );
        for pid in ["9", "10", "12"] {
            assert!(wait_done(&relay, pid), "{pid}");
        }
        std::thread::sleep(Duration::from_millis(800));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.9/rc")), "1\n");
        assert_eq!(read(relay.join("res.10/rc")), "1\n");
        assert!(read(relay.join("res.10/err")).contains("1 MiB"));
        assert_eq!(read(relay.join("res.12/rc")), "0\n");
        assert!(fs::symlink_metadata(relay.join("res.12")).unwrap().is_dir());
        assert_eq!(fs::read_dir(&trap).unwrap().count(), 0);
        assert!(!relay.join("res.11").exists());
        assert!(elsewhere.join("args").exists());
        assert!(!relay.join("res.x1").exists());
        assert!(relay.join("req.x1").exists());
        assert!(!read(d.join("gh.log")).contains("the key"));
        assert_eq!(read(d.join("gh.log")).matches("token=").count(), 1);
    }

    #[test]
    fn a_request_left_at_the_end_is_answered_1() {
        let d = TestDir::new("relay-end");
        let relay = d.join("relay");
        fs::create_dir(&relay).unwrap();
        request(
            &relay,
            "5",
            &["issue", "comment", "3", "--body", "hi"],
            None,
        );
        let via = Via {
            gh: stub_gh(&d),
            token: stub_token(&d, "ghs_full"),
            token_args: vec![],
        };
        serve_via(&relay, REPO, &AtomicBool::new(true), &via);
        assert_eq!(read(relay.join("res.5/rc")), "1\n");
        assert!(relay.join("res.5/done").exists());
        assert!(!d.join("gh.log").exists());
    }
}
