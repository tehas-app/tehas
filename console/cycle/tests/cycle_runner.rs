//! The runner seam of `tehas-cycle cycle`: `--runner job` names a `--work`
//! program that waits on a job run elsewhere. The report reads the cycle's
//! notes, run log and exit status as it does for tehas work, and nothing of a
//! workspace or an image on this host.

mod common;

use common::{has, lacks, Run, World};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;

const WORDS: &[&str] = &[
    "--work",
    "fake-work",
    "--status-cmd",
    "fake-status",
    "--console",
    "fake-console",
];

/// `tehas-cycle cycle 7 --runner <runner>` as the console execs it.
fn cycle(w: &World, runner: &str) -> Run {
    let mut words = vec!["cycle", "7", "--runner", runner];
    words.extend_from_slice(WORDS);
    w.exec(&words)
}

/// Outsource is still being built: only a test build with the feature named
/// in TEHAS_TEST_FEATURES follows its switch.
fn outsource(w: &mut World) {
    w.env("TEHAS_OUTSOURCE", "on");
    w.env("TEHAS_TEST_FEATURES", "outsource");
}

/// GitHub's address of the product is `remote`.
fn github(w: &mut World, remote: &Path) {
    w.env("GIT_CONFIG_COUNT", "1");
    w.env(
        "GIT_CONFIG_KEY_0",
        &format!("url.{}.insteadOf", remote.display()),
    );
    w.env("GIT_CONFIG_VALUE_0", "https://github.com/acme/widget.git");
}

/// The product's repository, bare, which a job cycle notes the branch heads
/// of before the waiter runs.
fn bare(w: &World) -> std::path::PathBuf {
    let remote = w.t().join("widget.git");
    let ok = Command::new("git")
        .args(["init", "--quiet", "--bare"])
        .arg(&remote)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    remote
}

/// A workspace of an earlier in-house cycle of the issue, marked, with its
/// images, and an image on record that docker would date.
fn planted(w: &World) {
    fs::create_dir_all(w.workdir()).unwrap();
    fs::create_dir_all(images(w)).unwrap();
    fs::write(w.home.join("work/.tehas-workspaces"), "").unwrap();
    w.fx("image.label", "2026-10-29T14:44:16Z\n");
    let id = format!("sha256:{}", "e".repeat(64));
    w.work(&format!(
        r#""$T/store-bin" cycle-image 7 --attempt "$TEHAS_CYCLE_ATTEMPT" --image {id} >/dev/null"#
    ));
}

fn images(w: &World) -> std::path::PathBuf {
    w.home.join("work/widget-7.images")
}

#[test]
fn a_job_runner_with_outsource_off_is_78_before_the_lock_the_store_github_or_the_waiter() {
    let offs: &[&[(&str, &str)]] = &[
        &[],
        &[
            ("TEHAS_OUTSOURCE", "off"),
            ("TEHAS_TEST_FEATURES", "outsource"),
        ],
        &[("TEHAS_OUTSOURCE", "1")],
        &[("TEHAS_OUTSOURCE", "on")],
    ];
    for off in offs {
        let mut w = World::new("runner-off");
        for (k, v) in *off {
            w.env(k, v);
        }
        let r = cycle(&w, "job");
        assert_eq!(r.rc, 78, "{off:?}");
        has(&r.out, "--runner job needs the outsource feature on");
        assert_eq!(
            format!("{}{}{}", w.gh(), w.calls("fake-work"), w.calls("docker")),
            ""
        );
        assert!(w.files(&w.state()).iter().all(|f| !f.starts_with("7.")));
        assert!(w.row(&["cycle-show", "7"])["cycle"].is_null());
    }
}

#[test]
fn a_runner_that_is_neither_work_nor_job_is_64_and_runs_nothing() {
    let mut w = World::new("runner-remote");
    outsource(&mut w);
    let r = cycle(&w, "remote");
    assert_eq!(r.rc, 64);
    assert_eq!(format!("{}{}", w.gh(), w.calls("fake-work")), "");
}

#[test]
fn a_job_cycle_is_reported_from_its_notes_and_no_workspace_or_image_on_this_host_is_read_or_removed(
) {
    // Its pull request was merged while the runner was stopped: the comment
    // says where the workspace is.
    let mut w = World::new("runner-job");
    let remote = bare(&w);
    planted(&w);
    w.ended("running", "handoff", "feature/x", "");
    w.merged_pr();
    w.work_more(r#"cp "$TEHAS_HOME/state/cycles/7.heads" "$TEHAS_HOME/heads-seen""#);
    w.work_more("exit 2");
    outsource(&mut w);
    github(&mut w, &remote);
    let r = cycle(&w, "job");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.calls("fake-work"), "7\n");
    // The heads were noted before the waiter ran, and the end cleared the note.
    assert!(w.home.join("heads-seen").exists());
    assert!(!w.state().join("7.heads").exists());
    assert_eq!(w.calls("docker"), "");
    assert!(w.workdir().is_dir() && images(&w).is_dir());
    let gh = w.gh();
    let edits: Vec<&str> = gh.lines().filter(|l| l.starts_with("issue edit")).collect();
    assert_eq!(
        edits,
        ["issue edit 7 --repo acme/widget --remove-label tehas:running"]
    );
    has(&w.gh(), "This cycle left no workspace on this host");
    lacks(&w.gh(), "The workspace is kept");
    // No job is on record, nor an approval time: a contractor's cycle is not
    // stamped in-house.
    has(&r.out, "WARN: #7: the store did not say which job built it");
    assert_eq!(
        r.out
            .lines()
            .filter(|l| l.contains("WARN") && !l.contains("which job built it"))
            .count(),
        0,
        "{}",
        r.out
    );
    lacks(&w.gh(), "Built by");
    let h = w.history0();
    assert_eq!(
        json!([h["status"], h["step"]]),
        json!(["merged", "handoff"])
    );
    assert!(!w.row(&["cycle-show", "7"])["cycle"]["ended_at"].is_null());
}

#[test]
fn a_job_cycle_whose_branch_heads_cannot_be_read_says_so_and_runs() {
    let mut w = World::new("runner-heads");
    w.ended("running", "handoff", "feature/x", "");
    w.merged_pr();
    w.work_more("exit 2");
    outsource(&mut w);
    let nowhere = w.t().join("nowhere.git");
    github(&mut w, &nowhere);
    let r = cycle(&w, "job");
    assert_eq!(r.rc, 0);
    assert_eq!(w.calls("fake-work"), "7\n");
    has(&r.out, "WARN: #7: the branch heads could not be noted");
}

#[test]
fn the_same_cycle_from_tehas_work_reads_the_image_and_clears_the_workspace_as_before() {
    let w = World::new("runner-work");
    planted(&w);
    w.ended("handoff", "commit-pr", "feature/x", "");
    w.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
    let r = cycle(&w, "work");
    assert_eq!(r.rc, 0);
    has(&w.calls("docker"), "image inspect");
    assert!(!images(&w).exists());
    let h = w.history0();
    assert_eq!(
        json!([h["rc"], h["status"], h["step"]]),
        json!([10, "handoff", "commit-pr"])
    );
}
