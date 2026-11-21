//! What the sandbox image says it supports, and what a cycle is refused or
//! given because of it; what the product's commit holds, the push check, the
//! product's install and Pi's settings.

mod work;

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use work::{
    body_sha256, commit, count_of, epoch, git_out, git_rc, has, lacks, root, security_doc_rows,
    security_line, security_of, unknown_options, Run, World, RUNNER, SECURITY_OPTS, WANT,
};

fn world(name: &str) -> World {
    let w = World::new(&format!("caps-{name}"));
    w.stub_github();
    w
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn before_retry(w: &World) -> PathBuf {
    with_suffix(&w.ws, ".before-retry")
}

fn there(p: &Path) -> bool {
    p.symlink_metadata().is_ok()
}

fn up(w: &World) -> String {
    w.upstream.to_string_lossy().into_owned()
}

/// How many docker runs were recorded.
fn docker_runs(w: &World) -> usize {
    fs::read_dir(w.path("calls"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("docker.args."))
                .count()
        })
        .unwrap_or(0)
}

/// The note a refused start leaves for the park.
fn refused(w: &World) -> String {
    fs::read_to_string(w.home.join("state/cycles/7.refused"))
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

/// 78, the image asked, and no workspace made, nothing fetched and no cycle
/// started.
#[track_caller]
fn caps_refused(w: &World, r: &Run) {
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(w.calls("docker.caps").lines().count(), 1);
    assert!(w.cycle_args().is_empty());
    assert!(!there(&w.ws));
    assert_eq!(w.gh(), "");
}

fn clear_calls_dir(w: &World) {
    if let Ok(d) = fs::read_dir(w.path("calls")) {
        for e in d.flatten() {
            let _ = fs::remove_file(e.path());
        }
    }
}

fn caps_fresh(w: &World) {
    clear_calls_dir(w);
    for p in [w.ws.clone(), w.state.clone(), before_retry(w)] {
        let _ = fs::remove_dir_all(p);
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn names(v: Vec<String>) -> Vec<String> {
    sorted(
        v.into_iter()
            .map(|e| e.split('=').next().unwrap().to_string())
            .collect(),
    )
}

fn want_with(extra: &[&str]) -> Vec<String> {
    sorted(
        WANT.lines()
            .chain(extra.iter().copied())
            .map(String::from)
            .collect(),
    )
}

/// The cycle's variables that start with `prefix`.
fn env_lines(w: &World, prefix: &str) -> Vec<String> {
    w.after("-e")
        .into_iter()
        .filter(|e| e.starts_with(prefix))
        .collect()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn id(flag: &str) -> String {
    let out = Command::new("id").arg(flag).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// --- What the product's commit holds ------------------------------------------

fn untrack_all(w: &World) {
    if !git_out(&w.upstream, &["ls-files"]).is_empty() {
        git_out(&w.upstream, &["rm", "-rq", "--cached", "--", "."]);
    }
}

fn checkout_at_main(w: &World) {
    git_out(
        &w.checkout,
        &[
            "fetch",
            "-q",
            &up(w),
            "+refs/heads/main:refs/remotes/origin/main",
        ],
    );
    git_out(&w.checkout, &["checkout", "-q", "main"]);
}

/// One commit holding just `f` with these lines on the product's main, and
/// the checkout fetched at it.
fn product_commit(w: &World, f: &str, lines: &[&str]) {
    let _ = fs::remove_dir_all(&w.ws);
    let _ = fs::remove_dir_all(&w.state);
    clear_calls_dir(w);
    untrack_all(w);
    let path = w.upstream.join(f);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    fs::write(&path, text).unwrap();
    git_out(&w.upstream, &["add", "-f", "--", f]);
    commit(&w.upstream, "product", None);
    checkout_at_main(w);
}

/// The product whose commit holds nothing at all, which is what parks a cycle.
fn product_nothing(w: &World) {
    let _ = fs::remove_dir_all(&w.ws);
    let _ = fs::remove_dir_all(&w.state);
    clear_calls_dir(w);
    let _ = fs::remove_file(w.upstream.join(".pi/verify-cmd"));
    untrack_all(w);
    commit(&w.upstream, "nothing", None);
    assert_eq!(
        git_out(&w.upstream, &["ls-tree", "-r", "--name-only", "main"]),
        "",
        "the product of nothing still holds a file"
    );
    checkout_at_main(w);
}

const NO_GATE: &str = "has no .pi/verify-cmd and no package.json or Cargo.toml";

#[test]
fn a_product_with_no_gate_and_no_stack_the_runner_can_guess_parks_the_cycle_78_named_and_no_container(
) {
    let w = world("no-gate");
    product_nothing(&w);
    let _ = fs::remove_dir_all(before_retry(&w));
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "acme/widget has no .pi/verify-cmd and no package.json or Cargo.toml",
    );
    assert_eq!(docker_runs(&w), 0);
    assert_eq!(git_rc(&w.ws, &["rev-parse", "--verify", "origin/main"]), 0);
}

#[test]
fn a_park_leaves_the_workspace_it_made_and_what_the_last_cycle_left_and_says_so() {
    let w = world("park-leaves");
    // The work directory is one tehas made.
    product_nothing(&w);
    assert_eq!(w.work7().rc, 78);
    let br = before_retry(&w);
    fs::create_dir_all(&br).unwrap();
    fs::write(br.join("left-over.txt"), "unrecoverable\n").unwrap();
    product_nothing(&w);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(
        fs::read_to_string(br.join("left-over.txt")).unwrap(),
        "unrecoverable\n"
    );
    has(&r.out, "holds what the last cycle left");
    assert_eq!(docker_runs(&w), 0);
}

#[test]
fn a_start_that_parks_before_its_container_puts_back_the_workspace_of_the_cycle_that_ran() {
    let w = world("park-puts-back");
    product_nothing(&w);
    assert_eq!(w.work7().rc, 78);
    product_nothing(&w);
    let _ = fs::remove_dir_all(before_retry(&w));
    fs::create_dir_all(&w.ws).unwrap();
    fs::write(w.ws.join("finished.txt"), "finished\n").unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    lacks(&r.out, "was not made by tehas");
    assert_eq!(
        fs::read_to_string(w.ws.join("finished.txt")).unwrap(),
        "finished\n"
    );
    assert!(!there(&before_retry(&w)));
    assert_eq!(docker_runs(&w), 0);
}

#[test]
fn a_gate_of_comments_and_blank_lines_only_is_no_gate_the_cycle_parks() {
    let w = world("gate-comments");
    product_commit(&w, ".pi/verify-cmd", &["# scripts/verify.sh", "", "   "]);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, NO_GATE);
    assert_eq!(docker_runs(&w), 0);
}

#[test]
fn the_checkouts_own_files_decide_nothing_a_product_with_a_gate_in_its_commit_runs() {
    let w = world("checkout-files");
    product_commit(&w, ".pi/verify-cmd", &["scripts/verify.sh"]);
    let _ = fs::remove_dir_all(w.checkout.join(".pi"));
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
}

#[test]
fn a_product_whose_gate_names_the_script_runs_and_the_clone_of_it_holds_the_gate() {
    let w = world("gate-script");
    product_commit(&w, ".pi/verify-cmd", &["scripts/verify.sh"]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        fs::read_to_string(w.ws.join(".pi/verify-cmd")).unwrap(),
        "scripts/verify.sh\n"
    );
}

#[test]
fn a_product_of_package_json_and_no_gate_runs_pi_rukas_guesses_the_stack() {
    let w = world("package-json");
    product_commit(&w, "package.json", &[r#"{"name":"widget"}"#]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
}

#[test]
fn a_product_of_cargo_toml_and_no_gate_runs() {
    let w = world("cargo-toml");
    product_commit(&w, "Cargo.toml", &["[package]", "name = \"widget\""]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
}

#[test]
fn a_stack_file_only_in_the_checkouts_worktree_is_no_stack_the_cycle_parks() {
    let w = world("stack-worktree");
    product_nothing(&w);
    fs::write(w.checkout.join("package.json"), "{\"name\":\"widget\"}\n").unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, NO_GATE);
}

// --- What the image supports -----------------------------------------------------

/// The first line of the docker calls that holds `what`, numbered from 1.
fn docker_line(w: &World, what: &str) -> Option<usize> {
    w.calls("docker")
        .lines()
        .position(|l| l.contains(what))
        .map(|i| i + 1)
}

#[test]
fn the_image_is_asked_what_it_supports_by_the_runners_path_before_the_workspace_is_made() {
    let w = world("asked");
    caps_fresh(&w);
    w.work7();
    assert_eq!(
        w.calls("docker.caps").trim_end_matches('\n'),
        format!(
            "run --pull=never --rm --init {} {RUNNER} --capabilities",
            w.image
        )
    );
    let caps_at = docker_line(&w, "--capabilities").expect("asked");
    let push_at = docker_line(&w, "push --dry-run").expect("push checked");
    assert!(caps_at < push_at);
}

fn drop_input(w: &World, name: &str) {
    w.caps_edit(|v| {
        v["inputs"]
            .as_array_mut()
            .unwrap()
            .retain(|i| i["name"] != name)
    });
}

#[test]
fn an_input_the_container_would_be_given_that_the_image_does_not_list_stops_the_cycle_78_named() {
    let w = world("input-unlisted");
    caps_fresh(&w);
    drop_input(&w, "PI_ENSEMBLE_SUBAGENT_MODEL");
    let r = w.work7();
    caps_refused(&w, &r);
    has(
        &r.out,
        "does not support what this cycle is given, not starting a cycle",
    );
    has(&r.out, "the input PI_ENSEMBLE_SUBAGENT_MODEL");
    // The cause is left for the park, which does not read the run log.
    assert_eq!(
        refused(&w),
        "caps-lacking\nthe input PI_ENSEMBLE_SUBAGENT_MODEL"
    );
}

#[test]
fn an_image_that_does_not_read_the_childs_bound_stops_the_cycle_78_named() {
    let w = world("child-bound");
    caps_fresh(&w);
    drop_input(&w, "PI_ENSEMBLE_SPAWN_TIMEOUT_MS");
    let r = w.work7();
    caps_refused(&w, &r);
    has(
        &r.out,
        "does not support what this cycle is given, not starting a cycle",
    );
    has(&r.out, "the input PI_ENSEMBLE_SPAWN_TIMEOUT_MS");
}

#[test]
fn an_instance_line_for_an_input_the_image_does_not_read_stops_the_cycle_78_named() {
    let w = world("not-a-lever");
    caps_fresh(&w);
    w.minimal(&["PI_ENSEMBLE_NOT_A_LEVER=1"]);
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the input PI_ENSEMBLE_NOT_A_LEVER");
}

#[test]
fn an_input_pi_rukas_sets_on_its_own_children_is_not_one_a_host_may_set() {
    let w = world("subagent-mode");
    caps_fresh(&w);
    w.minimal(&["PI_ENSEMBLE_SUBAGENT_MODE=1"]);
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the input PI_ENSEMBLE_SUBAGENT_MODE");
}

#[test]
fn a_roles_model_is_one_of_the_inputs_the_image_lists_as_a_family() {
    let w = world("role-family");
    caps_fresh(&w);
    w.minimal(&["PI_ENSEMBLE_MODEL_DEVELOPER=big"]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "does not support");
}

#[test]
fn a_role_the_image_does_not_list_as_a_member_of_its_family_stops_the_cycle_78_named() {
    let w = world("role-member");
    caps_fresh(&w);
    w.caps_edit(|v| {
        for i in v["inputs"].as_array_mut().unwrap() {
            if i["name"] == "PI_ENSEMBLE_MODEL_<ROLE>" || i["name"] == "PI_ENSEMBLE_PROVIDER_<ROLE>"
            {
                i["members"] = json!(["OPS"]);
            }
        }
    });
    w.minimal(&["PI_ENSEMBLE_MODEL_DEVELOPER=big"]);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "the input PI_ENSEMBLE_MODEL_DEVELOPER");
    has(&r.out, "the input PI_ENSEMBLE_PROVIDER_DEVELOPER");
    let empty = fs::read_dir(&w.ws).map_or(true, |mut d| d.next().is_none());
    assert!(empty, "the workspace holds files");
    caps_fresh(&w);
    w.minimal(&["PI_ENSEMBLE_MODEL_OPS=big"]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "does not support");
}

/// The image's labels: the polyfills it names, and the inputs of one of them.
fn polyfill_labels(w: &World, list: &str, inputs: &str) {
    let v = json!({"tehas.rukas.polyfills": list, "tehas.rukas.polyfill.example.inputs": inputs});
    w.fs("image.labels", format!("{v}\n"));
}

const EXAMPLE: &[&str] = &[
    "PI_ENSEMBLE_EXAMPLE_URL=https://example.test/q",
    "PI_ENSEMBLE_EXAMPLE_SCOPE=r1",
];

#[test]
fn an_input_the_runner_does_not_list_passes_when_a_polyfill_the_image_names_declares_it() {
    let w = world("polyfill-declares");
    caps_fresh(&w);
    polyfill_labels(
        &w,
        "example@0.1.0",
        "PI_ENSEMBLE_EXAMPLE_URL PI_ENSEMBLE_EXAMPLE_SCOPE",
    );
    w.minimal(EXAMPLE);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "does not support");
    has(
        &r.out,
        "the input PI_ENSEMBLE_EXAMPLE_URL is read by the polyfill example",
    );
    has(&w.after("-e").join("\n"), "PI_ENSEMBLE_EXAMPLE_URL");
}

#[test]
fn an_input_of_a_polyfill_the_image_does_not_name_stops_the_cycle_78_named() {
    let w = world("polyfill-unnamed");
    caps_fresh(&w);
    w.minimal(EXAMPLE);
    polyfill_labels(
        &w,
        "issue-comments@0.1.0",
        "PI_ENSEMBLE_EXAMPLE_URL PI_ENSEMBLE_EXAMPLE_SCOPE",
    );
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the input PI_ENSEMBLE_EXAMPLE_URL");
    has(&r.out, "the input PI_ENSEMBLE_EXAMPLE_SCOPE");
}

#[test]
fn a_polyfill_declares_one_input_and_the_other_still_stops_the_cycle_78_named() {
    let w = world("polyfill-one");
    caps_fresh(&w);
    w.minimal(EXAMPLE);
    polyfill_labels(&w, "example@0.1.0", "PI_ENSEMBLE_EXAMPLE_URL");
    let r = w.work7();
    caps_refused(&w, &r);
    assert_eq!(
        refused(&w),
        "caps-lacking\nthe input PI_ENSEMBLE_EXAMPLE_SCOPE"
    );
}

#[test]
fn a_polyfill_does_not_open_an_input_the_runner_keeps_to_itself() {
    let w = world("polyfill-internal");
    caps_fresh(&w);
    polyfill_labels(&w, "example@0.1.0", "PI_ENSEMBLE_SUBAGENT_MODE");
    w.minimal(&["PI_ENSEMBLE_SUBAGENT_MODE=1"]);
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the input PI_ENSEMBLE_SUBAGENT_MODE");
}

#[test]
fn the_switches_of_pi_rukass_own_docker_wrapper_are_not_given_to_the_container() {
    let w = world("wrapper-switches");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let e = w.after("-e").join("\n");
    lacks(&e, "PI_ENSEMBLE_NO_DOCKER_SOCKET");
    lacks(&e, "PI_ENSEMBLE_NO_SSH");
}

fn other_contract(name: &str, change: impl FnOnce(&mut Value)) {
    let w = world(name);
    caps_fresh(&w);
    w.caps_edit(change);
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "version");
}

#[test]
fn an_image_of_other_contract_versions_stops_the_cycle_contracts_exitcodes_version_2() {
    other_contract("contract-exit", |v| {
        v["contracts"]["exitCodes"]["version"] = json!(2)
    });
}

#[test]
fn an_image_of_other_contract_versions_stops_the_cycle_contracts_resultschema_version_3() {
    other_contract("contract-result", |v| {
        v["contracts"]["resultSchema"]["version"] = json!(3)
    });
}

#[test]
fn an_image_of_other_contract_versions_stops_the_cycle_contracts_ndjson_stable_version_2() {
    other_contract("contract-ndjson", |v| {
        v["contracts"]["ndjson"]["stable"]["version"] = json!(2)
    });
}

#[test]
fn an_image_of_other_contract_versions_stops_the_cycle_del_contracts() {
    other_contract("contract-none", |v| {
        v.as_object_mut().unwrap().remove("contracts");
    });
}

#[test]
fn a_flag_the_runner_does_not_take_stops_the_cycle_and_only_when_it_is_passed() {
    let w = world("flag");
    caps_fresh(&w);
    w.caps_edit(|v| v["verbs"]["work"]["flags"] = json!(["--json", "--timeout"]));
    let r = w.work(&["7", "--restart"]);
    caps_refused(&w, &r);
    has(&r.out, "the flag --restart");
    caps_fresh(&w);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
}

// --- Refreshes ----------------------------------------------------------------

fn record_input(w: &World, name: &str, v: &Value) -> String {
    let p = w.path(name);
    fs::write(&p, v.to_string()).unwrap();
    p.to_string_lossy().into_owned()
}

/// A start of how `how` on record. A refresh's names a head the clone holds,
/// which merges cleanly with main.
fn refresh_record(w: &World, how: &str) {
    fs::create_dir_all(w.fx.join("net")).unwrap();
    fs::create_dir_all(w.home.join("state/cycles")).unwrap();
    w.issue_is("", "NONE", "");
    w.store_db();
    let sum = body_sha256("");
    let mut args: Vec<String> = vec!["--body-sha".into(), sum];
    if how == "refresh" {
        let u = &w.upstream;
        git_out(u, &["checkout", "-q", "-b", "refresh-holds-1"]);
        w.commit_upstream(
            "the request's own file",
            Some(("refresh-1.txt", "the request")),
        );
        git_out(u, &["checkout", "-q", "main"]);
        w.commit_upstream("main moves", Some(("main-moved-1.txt", "main moved on")));
        let head = git_out(u, &["rev-parse", "refresh-holds-1"]);
        w.fetched();
        let input = record_input(
            w,
            "refresh-input.json",
            &json!({"links": [], "refresh_of": {"pr": 50, "head_sha": head, "branch": "main"}}),
        );
        args.extend([
            "--input-file".into(),
            input,
            "--refresh-pr".into(),
            "50".into(),
            "--refresh-head".into(),
            head,
        ]);
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    w.approve(7, "alice", how, epoch("2026-10-01T00:00:00Z"), &a);
    w.started(7, &[]);
}

/// The words of the cycle from the image on, each followed by a space.
fn from_image(w: &World) -> String {
    let args = w.cycle_args();
    let at = args
        .iter()
        .position(|a| *a == w.image)
        .unwrap_or(args.len());
    args[at..].iter().map(|a| format!("{a} ")).collect()
}

#[test]
fn a_refresh_passes_continue_pr_when_the_image_takes_it_with_the_same_mounts_and_variables_as_any_start(
) {
    let w = world("refresh-continue");
    caps_fresh(&w);
    w.caps_continue();
    w.work7();
    let plain_v = sorted(w.after("-v"));
    let plain_e = names(w.after("-e"));
    lacks(&w.cycle_args().join(" "), "--continue-pr");
    caps_fresh(&w);
    refresh_record(&w, "refresh");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        from_image(&w),
        format!(
            "{} {RUNNER} work 7 --timeout 360 --json --continue-pr ",
            w.image
        )
    );
    assert_eq!(sorted(w.after("-v")), plain_v);
    assert_eq!(names(w.after("-e")), plain_e);
}

#[test]
fn a_refresh_on_an_image_that_does_not_take_continue_pr_starts_nothing() {
    let w = world("refresh-no-continue");
    caps_fresh(&w);
    refresh_record(&w, "refresh");
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the flag --continue-pr");
}

#[test]
fn a_lean_refresh_is_78_before_anything_moves_though_the_image_takes_continue_pr_for_work() {
    let w = world("lean-refresh");
    caps_fresh(&w);
    w.caps_continue();
    w.caps_edit(|v| {
        v["verbs"]["work-lean"] = json!({"flags": ["--json", "--timeout"]});
        let f = v["features"].as_object_mut().unwrap();
        f.insert("workLeanResultSchema".into(), json!(5));
        f.insert("leanDeadline".into(), json!(true));
        f.insert("leanHandoffHost".into(), json!(true));
    });
    fs::create_dir_all(w.fx.join("net")).unwrap();
    fs::create_dir_all(w.home.join("state/cycles")).unwrap();
    w.issue_is("", "NONE", "");
    w.store_db();
    let input = record_input(
        &w,
        "lean-input.json",
        &json!({"links": [], "driver": "lean"}),
    );
    let head = "a".repeat(40);
    w.approve(
        7,
        "alice",
        "refresh",
        epoch("2026-10-01T00:00:00Z"),
        &[
            "--body-sha",
            &body_sha256(""),
            "--input-file",
            &input,
            "--refresh-pr",
            "50",
            "--refresh-head",
            &head,
        ],
    );
    w.started(7, &[]);
    w.env("TEHAS_TEST_FEATURES", "lean");
    w.minimal(&["PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9", "TEHAS_LEAN=on"]);
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the flag --continue-pr");
    has(&r.out, "driver lean, work-lean");
}

/// A refresh of pull request 50 at `head`, its record as the dispatcher
/// writes it.
fn refresh_at(w: &World, head: &str) {
    fs::create_dir_all(w.fx.join("net")).unwrap();
    fs::create_dir_all(w.home.join("state/cycles")).unwrap();
    w.issue_is("", "NONE", "");
    w.store_db();
    let input = record_input(
        w,
        "refresh-input.json",
        &json!({"links": [], "refresh_of": {"pr": 50, "head_sha": head, "branch": "main"}}),
    );
    w.approve(
        7,
        "alice",
        "refresh",
        epoch("2026-10-01T00:00:00Z"),
        &[
            "--body-sha",
            &body_sha256(""),
            "--input-file",
            &input,
            "--refresh-pr",
            "50",
            "--refresh-head",
            head,
        ],
    );
    w.started(7, &[]);
}

/// A branch of the product off main that writes `line` to `file`, and main
/// moved on with `file` written too for a conflict: the branch's head.
fn pr_branch(w: &World, branch: &str, file: &str, line: &str, conflict: bool) -> String {
    let u = &w.upstream;
    git_out(u, &["checkout", "-q", "-b", branch]);
    w.commit_upstream("the pull request", Some((file, line)));
    let head = git_out(u, &["rev-parse", "HEAD"]);
    git_out(u, &["checkout", "-q", "main"]);
    if conflict {
        w.commit_upstream("main moves", Some((file, "main moved on")));
    }
    w.fetched();
    head
}

fn note7(w: &World) -> PathBuf {
    w.home.join("state/cycles/7.refresh-conflict")
}

#[test]
fn a_refresh_that_conflicts_in_one_file_starts_no_container_on_an_image_that_cannot_verify_its_merge_and_names_the_file(
) {
    let w = world("refresh-conflict");
    caps_fresh(&w);
    w.caps_continue();
    let head = pr_branch(
        &w,
        "feature/one",
        "conflict.txt",
        "the pull request's line",
        true,
    );
    refresh_at(&w, &head);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    has(
        &r.out,
        "pull request #50 conflicts with origin/main in 1 file(s)",
    );
    has(&r.out, "    conflict.txt");
    let got: Value = serde_json::from_str(&fs::read_to_string(note7(&w)).unwrap()).unwrap();
    assert_eq!(
        got.to_string(),
        r#"{"branch":"main","more":0,"paths":["conflict.txt"],"pr":50}"#
    );
    // The workspace is left as it was made: main's tip, no merge in it.
    assert_eq!(git_out(&w.ws, &["status", "--porcelain"]), "");
    assert_eq!(
        fs::read_to_string(w.ws.join("conflict.txt")).unwrap(),
        "main moved on\n"
    );
}

#[test]
fn the_same_refresh_runs_on_an_image_that_verifies_a_continued_pull_requests_merge() {
    let w = world("refresh-verify");
    caps_fresh(&w);
    w.caps_continue();
    let head = pr_branch(
        &w,
        "feature/one",
        "conflict.txt",
        "the pull request's line",
        true,
    );
    w.caps_edit(|v| v["features"]["continuePrMergeVerify"] = json!(true));
    refresh_at(&w, &head);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&w.cycle_args().join(" "), "--continue-pr");
    assert!(!there(&note7(&w)));
}

#[test]
fn a_refresh_that_merges_cleanly_runs_as_before() {
    let w = world("refresh-clean");
    caps_fresh(&w);
    w.caps_continue();
    let head = pr_branch(&w, "feature/two", "clean.txt", "a new file", false);
    refresh_at(&w, &head);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(&w.cycle_args().join(" "), "--continue-pr");
    assert!(!there(&note7(&w)));
}

fn never_continues(how: &str) {
    let w = world(&format!("how-{how}"));
    caps_fresh(&w);
    w.caps_continue();
    refresh_record(&w, how);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{how}: {}", r.out);
    lacks(&w.cycle_args().join(" "), "--continue-pr");
}

#[test]
fn a_start_of_how_label_never_passes_continue_pr_also_when_the_image_takes_it() {
    never_continues("label");
}

#[test]
fn a_start_of_how_retry_tick_never_passes_continue_pr_also_when_the_image_takes_it() {
    never_continues("retry-tick");
}

#[test]
fn a_start_of_how_outage_retry_never_passes_continue_pr_also_when_the_image_takes_it() {
    never_continues("outage-retry");
}

#[test]
fn a_start_of_how_fixer_never_passes_continue_pr_also_when_the_image_takes_it() {
    never_continues("fixer");
}

#[test]
fn a_start_of_how_host_never_passes_continue_pr_also_when_the_image_takes_it() {
    never_continues("host");
}

// --- Features and what the cycle is given -------------------------------------

#[test]
fn an_image_without_the_host_handoff_channel_stops_the_cycle_and_one_without_a_restart_that_clears_the_label_stops_a_restart(
) {
    let w = world("features");
    caps_fresh(&w);
    w.caps_edit(|v| v["features"]["handoffChannel"] = json!(false));
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the feature handoffChannel");
    w.caps_good();
    caps_fresh(&w);
    w.caps_edit(|v| {
        v["features"]
            .as_object_mut()
            .unwrap()
            .remove("restartClearsAttention");
    });
    let r = w.work(&["7", "--restart"]);
    caps_refused(&w, &r);
    has(&r.out, "the feature restartClearsAttention");
    caps_fresh(&w);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
}

/// The deadline given is `seconds` after `before`, within a minute.
#[track_caller]
fn deadline_in(w: &World, before: i64, seconds: i64) {
    let d = env_lines(w, "PI_ENSEMBLE_CYCLE_DEADLINE=");
    assert_eq!(d.len(), 1, "{d:?}");
    let ms: i64 = d[0]["PI_ENSEMBLE_CYCLE_DEADLINE=".len()..].parse().unwrap();
    assert!(
        ms >= (before + seconds) * 1000 && ms <= (before + seconds + 60) * 1000,
        "{ms} is not {seconds}s after {before}"
    );
}

#[test]
fn the_handoff_channel_and_allowance_the_deadline_and_the_gate_runs_at_once_are_this_commands_not_the_instances(
) {
    let w = world("handoff");
    caps_fresh(&w);
    w.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9",
        "PI_ENSEMBLE_HANDOFF_CHANNEL=forge",
        "PI_ENSEMBLE_CYCLE_BUDGET_MS=60000",
        "PI_ENSEMBLE_VERIFY_CONCURRENCY=9",
        "PI_ENSEMBLE_CYCLE_DEADLINE=1",
        "PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=5000",
    ]);
    let before = now();
    w.work7();
    assert_eq!(env_lines(&w, "PI_ENSEMBLE_HANDOFF_CHANNEL").len(), 1);
    has(
        &w.after("-e").join("\n"),
        "PI_ENSEMBLE_HANDOFF_CHANNEL=host",
    );
    assert_eq!(
        env_lines(&w, "PI_ENSEMBLE_HANDOFF_TIMEOUT_MS"),
        ["PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=480000"]
    );
    lacks(&w.after("-e").join("\n"), "PI_ENSEMBLE_CYCLE_BUDGET_MS");
    // A container of a 2048m ceiling runs one gate at a time.
    assert_eq!(
        env_lines(&w, "PI_ENSEMBLE_VERIFY_CONCURRENCY"),
        ["PI_ENSEMBLE_VERIFY_CONCURRENCY=1"]
    );
    assert_eq!(env_lines(&w, "PI_ENSEMBLE_CYCLE_DEADLINE").len(), 1);
    // Six hours less the handoff allowance.
    deadline_in(&w, before, 21120);
}

#[test]
fn the_deadline_does_not_grow_with_the_run_of_the_gate() {
    let w = world("deadline-gate");
    caps_fresh(&w);
    w.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9",
        "TEHAS_WORK_TIMEOUT=7200",
        "TEHAS_VERIFY_TIMEOUT=2400",
    ]);
    let before = now();
    w.work7();
    // 7200 - 480
    deadline_in(&w, before, 6720);
    assert_eq!(
        env_lines(&w, "PI_ENSEMBLE_HANDOFF_TIMEOUT_MS"),
        ["PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=480000"]
    );
}

fn no_deadline_feature(v: &mut Value) {
    v["features"]
        .as_object_mut()
        .unwrap()
        .remove("cycleDeadline");
}

#[test]
fn a_time_limit_with_no_room_for_a_deadline_gives_none_says_so_and_does_not_need_the_feature() {
    let w = world("no-room");
    caps_fresh(&w);
    w.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9",
        "TEHAS_WORK_TIMEOUT=480",
        "TEHAS_VERIFY_TIMEOUT=120",
        "TEHAS_CHILD_TIMEOUT=480",
    ]);
    w.caps_edit(no_deadline_feature);
    let r = w.work7();
    lacks(&r.out, "does not support");
    has(
        &r.out,
        "the time limit of 480s leaves no room for a deadline before it, the cycle has none",
    );
    lacks(&w.after("-e").join("\n"), "PI_ENSEMBLE_CYCLE_DEADLINE");
    w.caps_good();
    caps_fresh(&w);
    w.caps_edit(no_deadline_feature);
    w.minimal(&["PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9"]);
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the feature cycleDeadline");
}

#[test]
fn a_workaround_whose_release_the_image_has_reached_stops_the_cycle_78_named() {
    let w = world("workaround");
    caps_fresh(&w);
    w.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9",
        "PI_ENSEMBLE_COMMENT_RETENTION=0",
        "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_COMMENT_RETENTION@0.12.80",
    ]);
    let r = w.work7();
    caps_refused(&w, &r);
    has(
        &r.out,
        "TEHAS_RUKAS_WORKAROUNDS names a workaround this image no longer needs",
    );
    has(&r.out, "PI_ENSEMBLE_COMMENT_RETENTION works around a pi-rukas defect fixed in 0.12.80, and the image's pi-rukas is 0.12.80");
    has(&r.out, "Remove the setting and its entry from");
    caps_fresh(&w);
    w.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9",
        "PI_ENSEMBLE_COMMENT_RETENTION=0",
        "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_COMMENT_RETENTION@0.12.81",
    ]);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&r.out, "no longer needs");
    caps_fresh(&w);
    w.caps_edit(|v| v["version"] = json!("main"));
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the image's pi-rukas version could not be read");
}

#[test]
fn with_images_on_the_image_must_read_the_images_directorys_variable() {
    let w = world("images-var");
    caps_fresh(&w);
    w.minimal(&["TEHAS_WORK_IMAGES=1"]);
    drop_input(&w, "PI_ENSEMBLE_EXTRA_IMAGE_DIRS");
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the input PI_ENSEMBLE_EXTRA_IMAGE_DIRS");
}

#[test]
fn an_image_that_gives_no_answer_or_not_json_stops_the_cycle() {
    let w = world("no-answer");
    caps_fresh(&w);
    w.env("CAPS_RC", "1");
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "gave no --capabilities answer (exit 1)");
    assert_eq!(refused(&w), "caps-unreadable 1");
    w.env_unset("CAPS_RC");
    caps_fresh(&w);
    w.fs("caps.json", "error: Module not found\n");
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "gave no --capabilities answer");
}

#[test]
fn names_in_the_answer_are_not_run_as_patterns_beyond_their_own_characters() {
    let w = world("patterns");
    caps_fresh(&w);
    w.caps_edit(|v| {
        let i = v["inputs"].as_array_mut().unwrap();
        i.push(json!({"name": "PI_ENSEMBLE_.*", "scope": "cycle"}));
        i.retain(|x| x["name"] != "PI_ENSEMBLE_SUBAGENT_MODEL");
    });
    let r = w.work7();
    caps_refused(&w, &r);
    has(&r.out, "the input PI_ENSEMBLE_SUBAGENT_MODEL");
}

// --- The push check and Pi's settings -------------------------------------------

#[test]
fn the_push_checks_container_gets_the_workspace_the_same_variables_and_the_security_options_no_more(
) {
    let w = world("push-check");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let pa = w.preflight_args();
    assert!(!pa.is_empty());
    assert_eq!(
        unknown_options(
            &pa,
            &w.image,
            "--user -e -w -v --entrypoint --cap-drop --cap-add --security-opt --tmpfs",
            "--rm"
        ),
        Vec::<String>::new()
    );
    assert_eq!(security_line(&pa), format!("{SECURITY_OPTS} "));
    assert_eq!(sorted(security_of(&pa, "--cap-add")), ["CHOWN", "FOWNER"]);
    // The whole line is counted, not just the block security_line matches.
    assert_eq!(count_of(&pa, "--tmpfs"), 1);
    assert_eq!(
        security_of(&pa, "-v"),
        [format!("{}:{}", w.ws.display(), w.inside.display())]
    );
    assert_eq!(names(security_of(&pa, "-e")), want_with(&["HOME"]));
    assert_eq!(
        security_of(&pa, "--user"),
        [format!("{}:{}", id("-u"), id("-g"))]
    );
}

#[test]
fn the_cycles_log_names_the_provider_retry_its_pi_runs_with() {
    let w = world("retry-log");
    let r = w.work7();
    let line = "provider retry: timeoutMs=600000 maxRetries=8 maxRetryDelayMs=60000 httpIdleTimeoutMs=unset agentMaxRetries=10 agentBaseDelayMs=5000 agentMaxDelayMs=120000";
    has(&r.out, &format!("tehas work: {line}"));
    let log = r
        .out
        .lines()
        .find_map(|l| l.strip_prefix("tehas work: issue 7 started, log "))
        .expect("the log is named");
    let pre = fs::read_to_string(format!("{log}.preflight")).unwrap();
    assert_eq!(pre.lines().last().unwrap(), line);
}

#[test]
fn the_products_own_pi_settings_json_is_merged_over_the_hosts_as_pi_merges_them() {
    let w = world("retry-merge");
    let prod = w.path("prod");
    w.fs(
        "prod/.pi/settings.json",
        "{\"retry\":{\"provider\":{\"maxRetries\":2}},\"httpIdleTimeoutMs\":120000}\n",
    );
    let retry = format!(
        "'{}' sandbox retry '{}'",
        env!("CARGO_BIN_EXE_tehas-cycle"),
        prod.display()
    );
    let r = w.sh(&retry);
    assert_eq!(r.out, "provider retry: timeoutMs=600000 maxRetries=2 maxRetryDelayMs=60000 httpIdleTimeoutMs=120000 agentMaxRetries=10 agentBaseDelayMs=5000 agentMaxDelayMs=120000");
    w.fs("prod/.pi/settings.json", "{\"retry\":\n");
    let r = w.sh(&retry);
    has(&r.out, "provider retry: unknown");
    has(&r.out, "is not JSON");
    w.rm("home/.pi/agent/settings.json");
    let r = w.sh(&retry);
    assert_eq!(
        r.out,
        format!(
            "provider retry: unknown, {}/home/.pi/agent/settings.json is missing",
            w.t().display()
        )
    );
}

#[test]
fn the_generic_and_example_settings_let_an_agent_wait_out_a_busy_model_not_give_up_after_three_tries(
) {
    for f in ["host/pi/settings.json", "instances/example/settings.json"] {
        let v: Value = serde_json::from_str(&fs::read_to_string(root().join(f)).unwrap()).unwrap();
        let r = &v["retry"];
        assert_eq!(
            json!([r["maxRetries"], r["baseDelayMs"], r["maxAgentDelayMs"]]),
            json!([10, 5000, 120000]),
            "{f}"
        );
    }
}

#[test]
fn without_pis_settings_json_nothing_runs_no_docker_call_the_workspace_and_the_kept_one_as_they_were(
) {
    let w = world("no-settings");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let settings = w.path("home/.pi/agent/settings.json");
    fs::remove_file(&settings).unwrap();
    let br = before_retry(&w);
    fs::create_dir_all(&br).unwrap();
    fs::write(br.join("marker"), "kept\n").unwrap();
    fs::write(w.ws.join("marker"), "current\n").unwrap();
    let snapshot = format!(
        "find '{0}' '{0}.before-retry' -printf '%P %s %T@\\n' 2>/dev/null | LC_ALL=C sort | md5sum",
        w.ws.display()
    );
    let before = w.sh(&snapshot).out;
    w.clear_calls();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!(
            "no {}/home/.pi/agent/settings.json: run install.sh",
            w.t().display()
        ),
    );
    assert_eq!(format!("{}{}", w.calls("docker"), w.calls("systemctl")), "");
    assert_eq!(docker_runs(&w), 0);
    assert_eq!(w.sh(&snapshot).out, before);
    assert_eq!(
        fs::read_to_string(w.ws.join("marker")).unwrap(),
        "current\n"
    );
    assert_eq!(fs::read_to_string(br.join("marker")).unwrap(), "kept\n");
    assert!(!there(&settings));
}

#[test]
fn the_token_is_passed_by_name_never_written_into_the_arguments() {
    let w = world("token");
    w.work7();
    let args = w.cycle_args();
    assert!(!args.is_empty());
    lacks(&args.join("\n"), "ghs_app");
}

/// Every docker run recorded, its words one per line.
fn all_docker_words(w: &World) -> String {
    let mut names: Vec<String> = fs::read_dir(w.path("calls"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("docker.args."))
        .collect();
    names.sort();
    names.iter().map(|n| w.calls(n)).collect()
}

#[test]
fn nothing_that_controls_the_host_is_reachable() {
    let w = world("host-control");
    w.work7();
    let given = all_docker_words(&w);
    has(&given, "pi-rukas-headless");
    lacks(&given, "docker.sock");
    lacks(&given, ".ssh");
    lacks(&given, ".config/tehas");
    lacks(&given, "--privileged");
}

/// The options as `tehas-cycle sandbox limits` prints them, one per line.
fn words_of(w: &World) -> Run {
    w.sh(&format!(
        ". '{}/lib/config.sh'; tehas_load || exit 9; '{}' sandbox limits 'tehas work' 'a cycle' | tr '\\0' '\\n'; exit \"${{PIPESTATUS[0]}}\"",
        root().display(),
        w.tree.join("libexec/tehas-cycle.real").display()
    ))
}

#[test]
fn the_security_options_drop_every_capability_and_take_privileges_away() {
    let w = world("security");
    let r = words_of(&w);
    assert_eq!(r.rc, 0, "{}", r.out);
    let block: String = r
        .out
        .lines()
        .skip(14)
        .take(10)
        .map(|l| format!("{l} "))
        .collect();
    assert_eq!(block, format!("{SECURITY_OPTS} "));
    // Nothing is added beyond the two a gate needs.
    assert!(!r.out.lines().any(|l| l.starts_with("--cap-add=")));
}

#[test]
fn the_tmp_tmpfs_is_half_the_containers_memory_ceiling_and_no_ceiling_gives_no_options() {
    let w = world("tmpfs");
    let r = words_of(&w);
    has(&r.out, "/tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777");
    // size=0 would be no bound at all, so a ceiling under 10 MiB is no ceiling.
    w.fs(
        "slice.show",
        "FragmentPath=/etc/systemd/system/tehas.slice\nActiveState=active\nMemoryMax=33554432\n",
    );
    w.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9",
        "TEHAS_CYCLE_RESERVE=6m",
    ]);
    let r = words_of(&w);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "tehas work: tehas_sandbox_security gave 0 words, wants 0: not starting a cycle",
    );
}

/// The rows of the manual's table of what a container may not do, as
/// `<option>|<value>`.
fn manual_rows() -> Vec<String> {
    let doc = fs::read_to_string(root().join("host/docs/host.md")).unwrap();
    let mut inside = false;
    let mut rows = vec![];
    for line in doc.lines() {
        if line == "### What a container may not do" {
            inside = true;
            continue;
        }
        if inside && line.starts_with("##") {
            inside = false;
        }
        if inside && line.starts_with("| `") {
            let row = &line[3..];
            let opt = row.split('`').next().unwrap();
            let val = match row.find('|') {
                Some(i) => row[i + 1..].trim_start_matches([' ', '`']),
                None => row,
            };
            let val = val.split('`').next().unwrap();
            if !opt.is_empty() && !val.is_empty() {
                rows.push(format!("{opt}|{val}"));
            }
        }
    }
    rows
}

#[test]
fn the_manuals_table_names_the_options_the_container_is_given() {
    assert_eq!(manual_rows(), security_doc_rows());
}

#[test]
fn no_container_of_a_cycle_a_turn_a_session_or_the_push_check_is_started_privileged_or_with_the_hosts_network(
) {
    let w = world("privileged");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    let given = all_docker_words(&w);
    assert!(!given.is_empty());
    lacks(&given, "--privileged");
    lacks(&given, "--net=host");
    lacks(&given, "--network host");
}

/// The values of the cycle's variable `name`, one per entry.
fn env_values(w: &World, name: &str) -> Vec<String> {
    let p = format!("{name}=");
    w.after("-e")
        .into_iter()
        .filter_map(|e| e.strip_prefix(&p).map(String::from))
        .collect()
}

#[test]
fn the_helper_a_containers_git_runs_is_the_one_key_and_the_one_command() {
    let w = world("git-helper");
    w.work7();
    assert_eq!(
        env_values(&w, "GIT_CONFIG_KEY_0"),
        ["credential.https://github.com.helper"]
    );
    assert_eq!(
        env_values(&w, "GIT_CONFIG_KEY_1"),
        ["credential.https://github.com.helper"]
    );
    assert_eq!(
        env_values(&w, "GIT_CONFIG_VALUE_1"),
        ["!gh auth git-credential"]
    );
    // The first value is empty on purpose: it ends any list an operator's file
    // left before it.
    assert_eq!(
        w.after("-e")
            .iter()
            .filter(|e| *e == "GIT_CONFIG_VALUE_0=")
            .count(),
        1
    );
}

#[test]
fn host_aliases_become_add_host_entries() {
    let w = world("aliases");
    w.work7();
    has(&w.cycle_args().join("\n"), "--add-host=models:192.0.2.9");
}

// --- The product's install ----------------------------------------------------

/// A lockfile on top of the product, gate and all.
fn lockfile(w: &World) {
    fs::create_dir_all(w.upstream.join("extension")).unwrap();
    fs::write(w.upstream.join("extension/bun.lock"), "{}\n").unwrap();
    git_out(&w.upstream, &["add", "-f", "--", "extension/bun.lock"]);
    commit(&w.upstream, "lock", None);
    git_out(
        &w.checkout,
        &[
            "fetch",
            "-q",
            &up(w),
            "+refs/heads/main:refs/remotes/origin/main",
        ],
    );
}

fn locked(name: &str) -> World {
    let w = world(name);
    lockfile(&w);
    caps_fresh(&w);
    w
}

/// The preflight logs of #7, one after the other.
fn preflight_logs(w: &World) -> String {
    let mut files: Vec<PathBuf> = fs::read_dir(w.home.join("logs"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("work-7-") && n.ends_with(".jsonl.preflight")
        })
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| fs::read_to_string(p).unwrap())
        .collect()
}

#[test]
fn a_product_with_a_lockfile_is_installed_in_its_workspace_in_the_sandbox_before_the_cycle() {
    let w = locked("lockfile");
    let r = w.work7();
    has(
        &r.out,
        "tehas work: install extension/: bun install --frozen-lockfile",
    );
    let ia = w.install_args();
    has(
        &ia.join("\n"),
        "(cd 'extension' && bun install --frozen-lockfile)",
    );
    has(&w.cycle_args().join("\n"), "pi-rukas-headless");
    // The cycle's limits and security options, the workspace where the cycle
    // has it, and no variable but HOME: no token reaches the product's install.
    assert_eq!(security_of(&ia, "--memory"), ["2048m"]);
    assert_eq!(security_line(&ia), format!("{SECURITY_OPTS} "));
    assert_eq!(ia[1], "--pull=never");
    assert_eq!(
        security_of(&ia, "-v"),
        [format!("{}:{}", w.ws.display(), w.inside.display())]
    );
    assert_eq!(
        security_of(&ia, "-w"),
        [w.inside.to_string_lossy().into_owned()]
    );
    assert_eq!(security_of(&ia, "-e"), ["HOME=/home/vscode"]);
    lacks(&ia.join("\n"), "GH_TOKEN");
    // The cycle's CPU quota too, which Docker refuses above what its daemon
    // counts.
    assert_eq!(security_of(&ia, "--cpus"), ["15.00"]);
    has(
        &preflight_logs(&w),
        "install extension/: bun install --frozen-lockfile",
    );
}

#[test]
fn an_install_that_fails_starts_no_cycle_78_with_what_it_said() {
    let w = locked("install-fails");
    w.env("INSTALL_RC", "1");
    w.env("INSTALL_SAYS", "error: lockfile had changes");
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "the product's install failed (exit 1), not starting a cycle",
    );
    has(&r.out, "error: lockfile had changes");
    assert!(w.cycle_args().is_empty());
    // The park reads why from the note: the run log is on the host only.
    assert_eq!(refused(&w), "install-failed 1");
}

#[test]
fn an_install_docker_refused_for_its_cpus_leaves_both_numbers_for_the_park() {
    let w = locked("install-cpus");
    w.env("INSTALL_RC", "125");
    w.env("INSTALL_SAYS", "docker: Error response from daemon: range of CPUs is from 0.01 to 8.00, as there are only 8 CPUs available");
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(refused(&w), "install-failed 125\ndocker-cpus 15.00 8");
    assert!(w.cycle_args().is_empty());
}

#[test]
fn a_silent_install_that_fails_shows_none_of_the_logs_earlier_lines_as_its_own() {
    let w = locked("install-silent");
    w.env("INSTALL_RC", "1");
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "the product's install failed (exit 1), not starting a cycle",
    );
    lacks(&r.out, "    install extension/");
    assert!(w.cycle_args().is_empty());
}

#[test]
fn a_tool_the_image_lacks_is_named_and_no_cycle_starts() {
    let w = locked("install-lacks");
    w.env("INSTALL_RC", "69");
    w.env("INSTALL_SAYS", "no bun in the sandbox image");
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "tehas work: no bun in the sandbox image, which the product's install needs; not starting a cycle");
    lacks(&r.out, "install failed");
    assert_eq!(refused(&w), "install-lacks\nbun");
    assert!(w.cycle_args().is_empty());
}
