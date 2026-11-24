//! `tehas-cycle finish` through the real binary: the wiring reaches the code
//! host through the adapter, lists the open changes, opens the draft and
//! pushes. The decisions are tested on the ports' fake in src/finish/tests.rs;
//! this asserts the stub's methods and counts only.
//!
//! The common stub answers each call with the next file of tests/finish/, in
//! order, and fails a call it has no answer for. The push goes to a local
//! bare repository: the home's git config points the code host's remote at it.

mod common;

use common::{has, write_methods, World};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.email=tehas@invalid", "-c", "user.name=tehas"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

#[test]
fn a_finish_lists_the_open_changes_opens_the_draft_and_pushes() {
    let w = World::new("finish-wiring");
    let t = w.t();
    let origin = t.join("origin.git");
    let seed = t.join("seed");
    git(&["init", "--quiet", "--bare", "-b", "main", &s(&origin)]);
    git(&["init", "--quiet", "-b", "main", &s(&seed)]);
    fs::write(seed.join("readme.md"), "one\n").unwrap();
    git(&["-C", &s(&seed), "add", "-A"]);
    git(&["-C", &s(&seed), "commit", "--quiet", "-m", "one"]);
    git(&["-C", &s(&seed), "push", "--quiet", &s(&origin), "main"]);

    let ws = w.workdir();
    git(&["clone", "--quiet", &s(&origin), &s(&ws)]);
    git(&[
        "-C",
        &s(&ws),
        "checkout",
        "--quiet",
        "-b",
        "feature/issue-7",
    ]);
    fs::write(ws.join("parts.md"), "parts\n").unwrap();
    git(&["-C", &s(&ws), "add", "-A"]);
    git(&["-C", &s(&ws), "commit", "--quiet", "-m", "the change"]);

    fs::write(
        t.join("home/.gitconfig"),
        format!(
            "[user]\n\tname = tehas\n\temail = tehas@invalid\n\
             [url \"{}\"]\n\tinsteadOf = https://github.com/acme/widget.git\n",
            s(&origin)
        ),
    )
    .unwrap();
    let answers = t.join("answers");
    fs::create_dir_all(&answers).unwrap();
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/finish"));
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        fs::copy(&p, answers.join(p.file_name().unwrap())).unwrap();
    }

    let r = w.exec(&[
        "finish",
        "7",
        "--home",
        &s(&w.home),
        "--repo",
        "acme/widget",
        "--default",
        "main",
        "--reason",
        "the review lane ran out of disk",
    ]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "https://github.com/acme/widget/pull/12");
    // Every call the replay answered, the extra one it failed included.
    let calls = fs::read_to_string(t.join("calls/gh.n")).unwrap_or_default();
    assert_eq!(calls.lines().count(), 3, "{}", w.gh());
    assert_eq!(write_methods(&w), ["POST"]);
    let pushed = "refs/heads/finish/issue-7";
    let o = s(&origin);
    assert_eq!(
        git(&["--git-dir", &o, "log", "-1", "--format=%s", pushed]),
        "the change"
    );
    assert_eq!(
        git(&["--git-dir", &o, "rev-parse", &format!("{pushed}^")]),
        git(&["--git-dir", &o, "rev-parse", "main"])
    );
}
