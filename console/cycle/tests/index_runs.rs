//! `tehas index`: what the indexing container is given, which tree it reads,
//! and that nothing is started when it should not be. Each test is a case of
//! the shell file it replaced, by the same name.

mod index;

use index::*;
use std::time::{Duration, Instant};

#[test]
fn an_invalid_instance_starts_no_container() {
    let h = Host::new("invalid");
    h.instance(&["TEHAS_CONTROL_ISSUE=1"]);
    let r = h.index(&[]);
    rc(&r, 78);
    assert_eq!(h.docker(), "");
}

#[test]
fn an_argument_is_a_usage_error() {
    let h = Host::new("usage");
    for bad in [&["now"][..], &["--before-cycle", "now"], &["--force"]] {
        let r = h.index(bad);
        rc(&r, 64);
        assert_eq!(h.docker(), "", "{bad:?}");
    }
}

#[test]
fn a_checkout_of_another_repository_starts_no_container() {
    let h = Host::new("other-repo");
    h.git(
        &h.checkout,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/other.git",
        ],
    );
    let r = h.index(&[]);
    rc(&r, 78);
    has(&r.out, "not of acme/widget");
    assert_eq!(h.docker(), "");
}

#[test]
fn a_checkout_that_does_not_say_its_default_branch_starts_no_container_and_says_both_remedies() {
    let h = Host::new("no-head");
    h.git(&h.checkout, &["remote", "set-head", "origin", "-d"]);
    let r = h.index(&[]);
    rc(&r, 78);
    has(
        &r.out,
        &format!(
            "the default branch of {} is unknown: run install.sh, or set TEHAS_BRANCH",
            h.checkout.display()
        ),
    );
    assert_eq!(h.docker(), "");
}

#[test]
fn tehas_branch_wins_over_the_checkout_and_a_branch_it_has_not_fetched_starts_no_container() {
    let h = Host::new("branch");
    h.minimal(&["TEHAS_BRANCH=release"]);
    // With the checkout's default branch known, and without it, as the shell
    // file's run had it.
    for unset in [false, true] {
        if unset {
            h.git(&h.checkout, &["remote", "set-head", "origin", "-d"]);
        }
        let r = h.index(&[]);
        rc(&r, 78);
        has(
            &r.out,
            &format!(
                "tehas index: {} has no origin/release, nothing was indexed",
                h.checkout.display()
            ),
        );
        assert_eq!(h.docker(), "");
    }
}

#[test]
fn without_the_sandbox_image_nothing_is_indexed_and_the_build_is_named() {
    let h = Host::new("no-image");
    h.set_env(&["IMAGE_RC=1"]);
    let r = h.index(&[]);
    rc(&r, 1);
    has(&r.out, "sandbox/build.sh");
    assert_eq!(h.runs(), 0);
    assert!(!h.marker().exists());
}

#[test]
fn while_a_poll_holds_the_dispatchers_lock_for_all_of_the_wait_nothing_is_indexed() {
    let h = Host::new("poll-held");
    let _poll = hold(h.dispatch_lock());
    let r = h.index(&[]);
    rc(&r, 3);
    has(
        &r.out,
        "a poll held the dispatcher's lock for 1s, nothing was indexed",
    );
    assert_eq!(h.docker(), "");
}

#[test]
fn a_run_waits_for_a_poll_that_lets_the_dispatchers_lock_go_and_lets_it_go_itself() {
    let h = Host::new("poll-lets-go");
    let poll = hold(h.dispatch_lock());
    let holder = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        drop(poll);
    });
    std::thread::sleep(Duration::from_millis(200));
    h.set_env(&["TEHAS_INDEX_LOCK_WAIT=5"]);
    let r = h.index(&[]);
    holder.join().unwrap();
    rc(&r, 0);
    assert_eq!(h.runs(), 1);
    let f = std::fs::File::open(h.dispatch_lock()).unwrap();
    assert!(f.try_lock().is_ok(), "the dispatcher's lock is free");
}

#[test]
fn while_another_index_run_holds_its_lock_nothing_is_indexed() {
    let h = Host::new("index-held");
    let _other = hold(h.index_lock());
    let r = h.index(&["--before-cycle"]);
    rc(&r, 3);
    has(&r.out, "another index run holds its lock");
    assert_eq!(h.docker(), "");
}

#[test]
fn before_a_cycle_a_cycle_that_runs_is_not_indexed_under() {
    let h = Host::new("cycle-before");
    h.indexed();
    h.moved("cycle");
    let _cycle = hold(h.cycle_lock(12));
    let r = h.index(&["--before-cycle"]);
    rc(&r, 3);
    has(&r.out, "a cycle is running (issue 12)");
    assert_eq!(h.docker(), "");
}

#[test]
fn while_a_cycle_holds_its_lock_nothing_is_indexed_and_the_issue_is_named() {
    let h = Host::new("cycle-held");
    let _cycle = hold(h.cycle_lock(12));
    let r = h.index(&[]);
    rc(&r, 3);
    has(&r.out, "a cycle is running (issue 12)");
    assert_eq!(h.docker(), "");
}

#[test]
fn the_lock_file_of_a_cycle_that_ended_holds_nothing_back() {
    let h = Host::new("cycle-ended");
    drop(hold(h.cycle_lock(12)));
    assert!(h.cycle_lock(12).exists());
    let r = h.index(&[]);
    rc(&r, 0);
}

/// A run of the indexer given a token in its environment, as the four cases
/// after the first of them read it.
fn with_token(name: &str) -> (Host, Run) {
    let h = Host::new(name);
    h.set_env(&["GH_TOKEN=test-token"]);
    let r = h.index(&[]);
    rc(&r, 0);
    (h, r)
}

#[test]
fn the_index_is_built_in_the_sandbox_image_at_the_path_a_cycle_works_in() {
    let (h, _) = with_token("built");
    let args = h.run_args();
    let at = args.iter().position(|a| *a == h.image).expect("the image");
    let tail: String = args[at..].iter().map(|a| format!("{a} ")).collect();
    assert_eq!(
        tail,
        format!(
            "{} cli index_repository --repo-path {} ",
            h.image,
            h.ws.display()
        )
    );
    assert_eq!(h.after("-w"), [h.ws.display().to_string()]);
    assert_eq!(h.after("--entrypoint"), ["codebase-memory-mcp"]);
}

#[test]
fn the_tree_that_is_indexed_is_a_clone_at_the_branchs_tip_read_only() {
    let (h, _) = with_token("clone");
    assert_eq!(h.noted("indexed.head"), h.tip());
    let mut got = h.after("-v");
    got.sort();
    let mut want = vec![
        format!("{}:{}:ro", h.noted("indexed.dir"), h.ws.display()),
        format!("{}:{CACHE}", h.indexdir().display()),
    ];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn the_tree_that_is_indexed_has_the_history_of_a_cycles_workspace_tehas_clone_depth_commits() {
    let h = Host::new("depth");
    h.set_env(&["GH_TOKEN=test-token"]);
    h.commit("second");
    h.commit("third");
    h.fetch();
    for depth in ["2", "0"] {
        h.reset();
        h.minimal(&[&format!("TEHAS_CLONE_DEPTH={depth}")]);
        let r = h.index(&[]);
        rc(&r, 0);
        assert_eq!(h.noted("indexed.head"), h.tip());
        let want = if depth == "0" {
            h.git(&h.upstream, &["rev-list", "--count", "main"])
        } else {
            "2".to_string()
        };
        assert_eq!(h.noted("indexed.count"), want, "depth {depth}");
    }
}

#[test]
fn the_indexer_writes_the_hosts_own_index_directory_and_nothing_else() {
    let (h, _) = with_token("writes");
    assert_eq!(
        h.after("-v").iter().filter(|v| !v.ends_with(":ro")).count(),
        1
    );
    let mut env = h.after("-e");
    env.sort();
    assert_eq!(
        env,
        [format!("CBM_CACHE_DIR={CACHE}"), "HOME=/home/vscode".into()]
    );
}

#[test]
fn the_clone_is_gone_afterwards_and_the_cycles_workspace_was_never_made() {
    let (h, _) = with_token("gone");
    let dir = h.noted("indexed.dir");
    assert!(!dir.is_empty());
    assert!(!std::path::Path::new(&dir).exists(), "{dir} is gone");
    assert!(!h.th("work").exists());
}

#[test]
fn the_indexing_container_gets_no_network_no_token_and_no_option_beyond_these() {
    let (h, _) = with_token("locked");
    assert_eq!(h.after("--network"), ["none"]);
    assert_eq!(h.after("--cap-drop"), ["ALL"]);
    assert_eq!(h.after("--security-opt"), ["no-new-privileges:true"]);
    assert_eq!(h.after("--user"), [user()]);
    // An init collects what git and the indexer leave behind, as in a cycle.
    assert_eq!(h.run_args().iter().filter(|a| *a == "--init").count(), 1);
    let valued =
        format!("--name --network --cap-drop --security-opt {LIMITS} --user -e -w -v --entrypoint");
    assert_eq!(
        unknown_options(&h.run_args(), &h.image, &valued, "--rm --init"),
        Vec::<String>::new()
    );
    let args = h.all_args();
    for word in [
        "test-token",
        "GH_TOKEN",
        "docker.sock",
        ".ssh",
        ".config/tehas",
        &h.checkout.display().to_string(),
    ] {
        lacks(&args, word);
    }
}

#[test]
fn what_was_indexed_is_reported_and_recorded() {
    let (h, r) = with_token("recorded");
    has(&r.out, "5 nodes, 7 edges");
    assert_eq!(h.field("commit"), h.tip());
    assert_eq!(h.field("branch"), "main");
    assert_eq!(
        (h.field("nodes"), h.field("edges")),
        ("5".into(), "7".into())
    );
}

#[test]
fn the_indexer_gets_the_slice_and_a_cycles_limits() {
    let h = Host::new("limits");
    h.minimal(&["TEHAS_CYCLE_RESERVE=1g"]);
    let r = h.index(&[]);
    rc(&r, 0);
    for (flag, want) in [
        ("--cgroup-parent", "tehas.slice"),
        ("--memory", "2048m"),
        ("--memory-swap", "2048m"),
        ("--memory-reservation", "1g"),
        ("--pids-limit", "4096"),
        ("--cpu-shares", "1024"),
        ("--network", "none"),
        ("--cpus", "15.00"),
    ] {
        assert_eq!(h.after(flag), [want], "{flag}");
    }
    assert!(!h.run_args().iter().any(|a| a.contains("--cidfile")));
    let valued =
        format!("--name --network --cap-drop --security-opt {LIMITS} --user -e -w -v --entrypoint");
    assert_eq!(
        unknown_options(&h.run_args(), &h.image, &valued, "--rm --init"),
        Vec::<String>::new()
    );
}

#[test]
fn the_slice_is_read_once_one_systemctl_call_one_docker_info() {
    let h = Host::new("slice-once");
    h.minimal(&["TEHAS_CYCLE_RESERVE=1g"]);
    let r = h.index(&[]);
    rc(&r, 0);
    assert_eq!(
        h.calls("systemctl"),
        "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax\n"
    );
    assert_eq!(
        h.docker()
            .lines()
            .filter(|l| l.starts_with("info "))
            .count(),
        1
    );
}

// --- Without a usable slice ----------------------------------------------------

/// A host indexed once, its record kept, with the calls forgotten and the
/// slice as it should be.
fn without(name: &str) -> (Host, String) {
    let h = Host::new(name);
    h.indexed();
    let kept = h.record();
    assert!(!kept.is_empty());
    (h, kept)
}

/// 78, no call to docker but the question about the slice, no clone, and the
/// record as it was.
#[track_caller]
fn refused(h: &Host, r: &Run, kept: &str) {
    rc(r, 78);
    assert_eq!(h.docker_args_files(), 0);
    assert_eq!(
        h.docker()
            .lines()
            .filter(|l| !l.starts_with("info "))
            .count(),
        0
    );
    assert_eq!(h.tmp_count(), 0);
    assert_eq!(h.record(), kept);
}

const NO_SLICE: &str = "FragmentPath=\nActiveState=inactive\nMemoryMax=infinity\n";

#[test]
fn refuses_with_78_without_the_slice_before_any_docker_run() {
    let (h, kept) = without("no-slice");
    h.slice_show(NO_SLICE);
    let r = h.index(&[]);
    refused(&h, &r, &kept);
    has(
        &r.out,
        "tehas index: no slice file on this host (sudo ./bootstrap.sh slice)",
    );
}

#[test]
fn a_cgroupfs_driver_cgroup_v1_and_a_silent_docker_are_refused_the_same_way() {
    let (h, kept) = without("driver");
    for answer in ["cgroupfs 2", "systemd 1", ""] {
        h.reset();
        h.slice_good();
        h.docker_info(&format!("{answer}\n"));
        let r = h.index(&[]);
        refused(&h, &r, &kept);
        has(&r.out, "tehas index: ");
        has(&r.out, "ocker");
    }
}

#[test]
fn a_reserve_above_the_ceiling_is_refused() {
    let (h, kept) = without("reserve");
    h.minimal(&["TEHAS_CYCLE_RESERVE=3g"]);
    let r = h.index(&[]);
    refused(&h, &r, &kept);
    has(
        &r.out,
        "tehas index: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m",
    );
}

#[test]
fn before_a_cycle_without_the_slice_nothing_is_indexed_either() {
    let (h, kept) = without("before-no-slice");
    h.moved("unseen");
    h.slice_show(NO_SLICE);
    let r = h.index(&["--before-cycle"]);
    refused(&h, &r, &kept);
    h.reset();
    h.slice_good();
    let r = h.index(&[]);
    rc(&r, 0);
}

#[test]
fn it_indexes_what_the_checkout_last_fetched_not_the_checkouts_own_tree() {
    let h = Host::new("fetched");
    h.indexed();
    h.moved("second");
    let r = h.index(&[]);
    rc(&r, 0);
    assert_eq!(h.noted("indexed.head"), h.tip());
    assert_eq!(h.field("commit"), h.tip());
    assert_ne!(
        h.git(&h.checkout, &["rev-parse", "HEAD"]),
        h.tip(),
        "the checkout's own tree moved"
    );
}

#[test]
fn before_a_cycle_an_index_already_at_the_branchs_tip_is_left_alone() {
    let h = Host::new("current");
    h.indexed();
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    has(&r.out, "already indexed");
    assert_eq!(h.docker(), "");
}

#[test]
fn before_a_cycle_a_branch_that_moved_is_indexed() {
    let h = Host::new("moved");
    h.indexed();
    h.moved("third");
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    assert_eq!(h.runs(), 1);
    assert_eq!(h.field("commit"), h.tip());
    let _poll = hold(h.dispatch_lock());
    let r = h.index(&[]);
    rc(&r, 3);
}

#[test]
fn before_a_cycle_an_empty_index_directory_is_built_again_though_the_record_names_the_tip() {
    let h = Host::new("empty-dir");
    h.indexed();
    std::fs::remove_dir_all(h.indexdir()).unwrap();
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    assert_eq!(h.runs(), 1);
    lacks(&r.out, "already indexed");
    assert_eq!(entries(&h), 1);
}

/// How many entries the index directory holds.
fn entries(h: &Host) -> usize {
    std::fs::read_dir(h.indexdir()).unwrap().count()
}

#[test]
fn before_a_cycle_an_index_directory_with_only_the_indexers_logs_is_built_again() {
    let h = Host::new("only-logs");
    h.indexed();
    h.index_holds(&[("logs/", "")]);
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    assert_eq!(h.runs(), 1);
    lacks(&r.out, "already indexed");
}

#[test]
fn before_a_cycle_leftovers_are_no_index_the_config_and_a_staging_file_are_built_over() {
    let h = Host::new("leftovers");
    h.indexed();
    h.index_holds(&[
        ("logs/", ""),
        ("_config.db", "cfg"),
        ("widget.db.stage.abc123.tmp.945328.0x200002bc000", "left"),
    ]);
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    assert_eq!(h.runs(), 1);
    lacks(&r.out, "already indexed");
}

#[test]
fn a_run_removes_what_an_earlier_one_left_in_the_index_directory() {
    let h = Host::new("prune");
    h.indexed();
    h.index_holds(&[
        ("logs/", ""),
        ("other-tree.db", "old"),
        ("widget.db.stage.abc123.tmp.945328.0x200002bc000", "left"),
        ("scratch.tmp", "gone"),
        ("logs/index-1.log", "log"),
    ]);
    age(h.indexdir().join("other-tree.db"), 120);
    let r = h.index(&[]);
    rc(&r, 0);
    assert_eq!(h.index_names(), "logs widget.db ");
    assert_eq!(h.index_file("widget.db"), "index");
    has(
        &r.out,
        &format!(
            "removed 3 file(s) an earlier run left in {}",
            h.indexdir().display()
        ),
    );
    let dirs: Vec<String> = std::fs::read_dir(h.indexdir())
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(dirs, ["logs"]);
}

#[test]
fn a_database_written_by_this_run_is_never_pruned_nor_the_indexers_own_logs() {
    let h = Host::new("never-pruned");
    h.indexed();
    std::fs::remove_dir_all(h.indexdir()).unwrap();
    // The same name as the one this run writes: pruned only when it predates
    // the run.
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    lacks(&r.out, "removed ");
    assert_eq!(h.index_names(), "widget.db ");
    h.moved("prune");
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    lacks(&r.out, "removed ");
    assert_eq!(h.index_file("widget.db"), "index");
}

#[test]
fn a_failed_run_prunes_nothing_and_leaves_the_leftovers_where_they_were() {
    let h = Host::new("failed-prune");
    h.indexed();
    h.index_holds(&[
        ("logs/", ""),
        ("other-tree.db", "old"),
        ("widget.db.stage.abc123.tmp.945328.0x200002bc000", "left"),
    ]);
    h.set_env(&["RUN_RC=1"]);
    let r = h.index(&[]);
    rc(&r, 1);
    lacks(&r.out, "removed ");
    assert_eq!(
        h.index_names(),
        "logs other-tree.db widget.db.stage.abc123.tmp.945328.0x200002bc000 "
    );
}

#[test]
fn a_failed_indexer_exits_1_leaves_the_record_as_it_was_and_notes_the_tip() {
    let h = Host::new("failed");
    h.indexed();
    let before = h.record();
    h.set_env(&["RUN_RC=1"]);
    let r = h.index(&[]);
    rc(&r, 1);
    has(&r.out, &format!("{}/index-", h.th("logs").display()));
    assert_eq!(h.record(), before);
    assert_eq!(h.failed_at(), h.tip());
}

#[test]
fn an_answer_that_does_not_say_indexed_counts_as_failed() {
    let h = Host::new("not-indexed");
    h.indexed();
    let before = h.record();
    h.set_env(&[r#"RUN_OUT={"error":"no such path"}"#]);
    let r = h.index(&[]);
    rc(&r, 1);
    assert_eq!(h.record(), before);
    assert_eq!(h.failed_at(), h.tip());
}

#[test]
fn a_record_that_cannot_be_written_counts_as_failed_and_a_poll_does_not_run_again_at_that_tip() {
    let h = Host::new("unwritable");
    h.indexed();
    h.moved("unwritable");
    let before = h.record();
    let mut new = h.marker().into_os_string();
    new.push(".new");
    let new = std::path::PathBuf::from(new);
    std::fs::create_dir_all(new.join("x")).unwrap();
    let r = h.index(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("its record {} was not written", h.marker().display()),
    );
    assert_eq!(h.record(), before);
    assert_eq!(h.failed_at(), h.tip());
    let r = h.index(&["--before-cycle"]);
    rc(&r, 1);
    has(&r.out, "not tried again before the branch moves");
    assert_eq!(h.runs(), 1);
}

#[test]
fn a_container_left_by_a_killed_run_is_removed_before_the_run_under_timeout() {
    let h = Host::new("left-container");
    let r = h.index(&[]);
    rc(&r, 0);
    let firsts: String = h
        .docker()
        .lines()
        .filter(|l| !l.starts_with("info "))
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            format!("{} ", f[..f.len().min(3)].join(" "))
        })
        .collect();
    assert_eq!(
        firsts,
        format!(
            "image inspect {} rm -f tehas-index run --pull=never --rm ",
            h.image
        )
    );
    let bounded: Vec<String> = h
        .calls("timeout")
        .lines()
        .filter(|l| l.starts_with("-k ") && l.contains(" docker "))
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            f[..f.len().min(5)].join(" ")
        })
        .collect();
    assert_eq!(bounded, ["-k 30 1800 docker run"]);
    assert!(!h.failed().exists());
}

#[test]
fn an_indexer_that_hangs_is_stopped_at_the_bound_and_its_container_removed() {
    let h = Host::new("hangs");
    let began = Instant::now();
    h.set_env(&["RUN_SLEEP=60", "TIMEOUT_BOUND=1"]);
    let r = h.index(&[]);
    rc(&r, 1);
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "the run was not stopped at the bound"
    );
    has(&r.out, "the indexer did not finish within");
    assert_eq!(
        h.docker()
            .lines()
            .filter(|l| *l == "rm -f tehas-index")
            .count(),
        2
    );
}

/// A host whose last run succeeded, then a branch that moved and a failure at
/// its tip before a cycle.
fn failed_before_cycle(name: &str) -> (Host, String) {
    let h = Host::new(name);
    h.indexed();
    h.moved("fails");
    let before = h.field("commit");
    h.set_env(&["RUN_RC=1"]);
    let r = h.index(&["--before-cycle"]);
    rc(&r, 1);
    (h, before)
}

#[test]
fn before_a_cycle_two_failures_at_one_tip_give_one_run() {
    let (h, before) = failed_before_cycle("two-failures");
    let tip = h.tip();
    let r = h.index(&["--before-cycle"]);
    rc(&r, 1);
    has(&r.out, "not tried again before the branch moves");
    assert_eq!(h.runs(), 1);
    assert_eq!(h.field("commit"), before);
    assert_eq!(h.failed_at(), tip);
}

#[test]
fn by_hand_a_failed_tip_is_tried_again_and_a_branch_that_moved_is_tried_before_a_cycle() {
    let (h, _) = failed_before_cycle("by-hand");
    h.reset();
    let r = h.index(&[]);
    rc(&r, 1);
    assert_eq!(h.runs(), 1);
    h.set_env(&[]);
    h.moved("mended");
    h.reset();
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    assert_eq!(h.runs(), 1);
    assert_eq!(h.field("commit"), h.tip());
    assert!(!h.failed().exists());
}
