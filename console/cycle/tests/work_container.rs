//! What the cycle's container is given, its mounts, variables, limits and
//! console, and that nothing starts without a usable slice. That the limit
//! and security words are the documented ones is decided in Rust:
//! tehas_core::slice::limits makes them and misshaped in
//! console/cycle/src/work.rs refuses a list that is not it, both tested beside
//! themselves. The words are pinned by test/slice_test.sh, the readers of
//! work/mod.rs and the goldens of test/sandbox_argv_test.sh, and read off the
//! docker line here. The mounts and variables are compared whole, sorted, as
//! AGENTS.md rule 4 asks.

mod work;

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use work::{
    count_of, git_out, has, lacks, security_line, security_of, security_tmpfs, unknown_options,
    Run, World, DOCKER_OPTS, GONE, HOLD, RUNNER, SECURITY_OPTS, WANT,
};

const ALIAS: &str = "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9";

fn lines(s: &str) -> Vec<String> {
    s.lines().map(String::from).collect()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The names of `NAME=value` words.
fn names(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .map(|e| e.split_once('=').map_or(e.clone(), |(n, _)| n.to_string()))
        .collect()
}

/// WANT and these names, sorted.
fn want_and(more: &[&str]) -> Vec<String> {
    let mut v = lines(WANT);
    v.extend(more.iter().map(|s| s.to_string()));
    sorted(v)
}

fn env_names(w: &World) -> Vec<String> {
    sorted(names(w.after("-e")))
}

/// The value of a variable the cycle is given, "" for none.
fn given_value(w: &World, name: &str) -> String {
    let start = format!("{name}=");
    w.after("-e")
        .iter()
        .filter_map(|e| e.strip_prefix(&start))
        .collect::<Vec<_>>()
        .join("\n")
}

fn mounts(w: &World) -> Vec<String> {
    sorted(w.after("-v"))
}

fn out(s: &str) -> String {
    s.trim_end_matches('\n').to_string()
}

fn id(flag: &str) -> String {
    let o = Command::new("id").arg(flag).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn user() -> String {
    format!("{}:{}", id("-u"), id("-g"))
}

/// The names of the files of calls/ that start with `start`, sorted.
fn call_files(w: &World, start: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(w.path("calls"))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(start))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn docker_runs(w: &World) -> usize {
    call_files(w, "docker.args.").len()
}

/// What the files of calls/ that start with `start` hold, one after another.
fn cat_calls(w: &World, start: &str) -> String {
    call_files(w, start)
        .iter()
        .map(|n| w.calls(n))
        .collect::<String>()
}

/// The lines of a call file that `keep` keeps.
fn count_lines(text: &str, keep: impl Fn(&str) -> bool) -> usize {
    text.lines().filter(|l| keep(l)).count()
}

/// The names in `dir`, sorted, each followed by a space.
fn entries(dir: &Path, dirs_only: bool) -> String {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| !dirs_only || e.path().is_dir())
        .map(|e| format!("{} ", e.file_name().to_string_lossy()))
        .collect();
    v.sort();
    v.concat()
}

/// The console's call that names the log.
fn progress_line(w: &World) -> String {
    w.calls("console")
        .lines()
        .filter(|l| l.starts_with("--progress "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn cycle_file(w: &World, rel: &str) -> String {
    out(&fs::read_to_string(w.home.join("state/cycles").join(rel)).unwrap_or_default())
}

fn cycle_exists(w: &World, rel: &str) -> bool {
    w.home.join("state/cycles").join(rel).exists()
}

/// The cycle's words from the image on, each followed by a space.
fn verb_line(w: &World) -> String {
    let args = w.cycle_args();
    match args.iter().position(|a| *a == w.image) {
        Some(i) => args[i..].iter().map(|a| format!("{a} ")).collect(),
        None => String::new(),
    }
}

fn classic(w: &World, more: &str) -> String {
    format!("{} {RUNNER} work 7 --timeout 360 --json {more}", w.image)
}

fn lean(w: &World) -> String {
    format!("{} {RUNNER} work-lean 7 --timeout 360 --json ", w.image)
}

/// A world with one start of #7 run, which the stub ends at once.
fn ran(name: &str) -> (World, Run) {
    let w = World::new(name);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    (w, r)
}

#[track_caller]
fn ok2(r: &Run) {
    assert_eq!(r.rc, 2, "{}", r.out);
}

#[test]
fn a_work_directory_tehas_did_not_make_is_left_alone() {
    let w = World::new("not-ours");
    w.minimal(&[]);
    w.env_ok();
    w.home_file("work/widget/notes.txt", "keep\n");
    w.home_file("work/widget-7/notes.txt", "keep\n");
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "was not made by tehas");
    assert_eq!(cycle_file(&w, "7.refused"), "tree-not-ours");
    let _ = fs::remove_file(w.home.join("state/cycles/7.refused"));
    assert_eq!(
        fs::read_to_string(w.home.join("work/widget/notes.txt")).unwrap(),
        "keep\n"
    );
    assert_eq!(
        fs::read_to_string(w.home.join("work/widget-7/notes.txt")).unwrap(),
        "keep\n"
    );
    assert_eq!(docker_runs(&w), 0);
}

#[test]
fn an_invalid_instance_starts_no_container() {
    let w = World::new("invalid");
    w.clear_calls();
    w.instance(&["TEHAS_CONTROL_ISSUE=1"]);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(format!("{}{}", w.calls("docker"), w.calls("systemctl")), "");
}

#[test]
fn the_cycle_gets_the_slice_a_ceiling_of_a_quarter_of_it_no_swap_a_floor_a_pids_limit_a_cpu_weight_and_no_privileges(
) {
    let (w, _) = ran("slice");
    assert_eq!(w.after("--cgroup-parent"), ["tehas.slice"]);
    assert_eq!(w.after("--memory"), ["2048m"]);
    assert_eq!(w.after("--memory-swap"), ["2048m"]);
    assert_eq!(w.after("--memory-reservation"), ["2g"]);
    assert_eq!(w.after("--pids-limit"), ["4096"]);
    assert_eq!(w.after("--cpu-shares"), ["1024"]);
    assert_eq!(
        w.after("--cidfile"),
        [w.cid_file.to_string_lossy().into_owned()]
    );
    assert_eq!(w.after("--cpus"), ["15.00"]);
    // Every capability of Docker's default list is taken away, and only what
    // a gate needs is given back: see tehas_sandbox_security.
    let args = w.cycle_args();
    assert_eq!(security_line(&args), format!("{SECURITY_OPTS} "));
    assert_eq!(sorted(security_of(&args, "--cap-drop")), ["ALL"]);
    assert_eq!(sorted(security_of(&args, "--cap-add")), ["CHOWN", "FOWNER"]);
    assert_eq!(
        security_of(&args, "--security-opt"),
        ["no-new-privileges:true"]
    );
    // One bounded /tmp of its own, exec for the product's own gate. The
    // equals form counts as a mount too, and a second mount of another path
    // is a second flag wherever it stands, so the line holds exactly one.
    assert_eq!(security_of(&args, "--tmpfs"), [security_tmpfs()]);
    // Half of --memory 2048m, so a full /tmp is ENOSPC and not an OOM kill.
    assert_eq!(
        security_of(&args, "--tmpfs"),
        ["/tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"]
    );
    assert_eq!(count_of(&args, "--tmpfs"), 1);
    let words = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(count_of(&words(&["run", "--tmpfs=/tmp"]), "--tmpfs"), 1);
    assert_eq!(
        count_of(
            &words(&["run", "--tmpfs", "/tmp:rw", "--tmpfs", "/dev/shm"]),
            "--tmpfs"
        ),
        2
    );
    // The image's entrypoint drops from root with setpriv, which the dropped
    // SETUID and SETGID refuse: the container starts as the host's user.
    assert_eq!(w.after("--user"), [user()]);
}

#[test]
fn the_slice_is_read_once_one_systemctl_call_one_docker_info() {
    let w = World::new("slice-once");
    w.clear_calls();
    let r = w.work7();
    ok2(&r);
    assert_eq!(
        out(&w.calls("systemctl")),
        "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
    );
    let docker = w.calls("docker");
    let info: Vec<&str> = docker.lines().filter(|l| l.starts_with("info ")).collect();
    assert_eq!(
        info,
        [r#"info --format {{.CgroupDriver}} {{.CgroupVersion}}{{"\n"}}{{.NCPU}}"#]
    );
    lacks(&r.out, "Docker daemon counts");
}

#[test]
fn the_cpus_are_capped_at_what_the_docker_daemon_counts_and_the_log_says_so() {
    let w = World::new("cpus-capped");
    w.stub("nproc", "echo 48");
    w.fs("docker.info", "systemd 2\n16\n");
    let r = w.work7();
    ok2(&r);
    assert_eq!(w.after("--cpus"), ["15.00"]);
    has(&r.out, "tehas work: the host has 48 CPUs and its Docker daemon counts 16, so containers get --cpus of 15; restart the Docker daemon to give them the host's CPUs");
}

#[test]
fn a_count_of_the_docker_daemon_that_cannot_be_read_keeps_the_hosts() {
    let w = World::new("cpus-unread");
    w.stub("nproc", "echo 48");
    w.fs("docker.info", "systemd 2\n<no value>\n");
    let r = w.work7();
    ok2(&r);
    assert_eq!(w.after("--cpus"), ["47.00"]);
    lacks(&r.out, "Docker daemon counts");
}

#[test]
fn the_cid_file_is_gone_when_the_cycle_is_over_and_no_count_of_kills_is_left() {
    let (w, r) = ran("cid-gone");
    assert!(!w.cid_file.exists());
    assert!(!w.oom_file.exists());
    lacks(&r.out, "killed");
}

#[test]
fn the_image_and_the_working_directory_come_from_the_settings() {
    let (w, _) = ran("image");
    has(
        &w.cycle_args().join(" "),
        &format!("{} {RUNNER} work 7 --timeout 360 --json", w.image),
    );
    assert_eq!(w.after("-w"), [w.inside.to_string_lossy().into_owned()]);
}

#[test]
fn the_console_is_the_program_beside_this_one_given_the_cycles_log_the_transcripts_and_its_file() {
    let (w, _) = ran("console-given");
    let console = w.calls("console");
    has(
        &console,
        &format!("--progress {}/logs/work-7-", w.home.display()),
    );
    has(
        &console,
        &format!(
            ".jsonl --runs {} --out {}/live/console-7.txt --since ",
            w.runs.display(),
            w.home.display()
        ),
    );
}

#[test]
fn a_console_that_fails_is_named_with_its_exit_status_and_where_it_wrote_why() {
    let (w, r) = ran("console-fails");
    lacks(&r.out, "the console ended");
    w.fs(
        "tree/libexec/console",
        "#!/usr/bin/env bash\necho \"cannot write\" >&2\nexit 1\n",
    );
    let r = w.work7();
    ok2(&r);
    has(
        &r.out,
        &format!(
            "tehas work: the console ended with 1, see {}/logs/work-7-",
            w.home.display()
        ),
    );
    let mut said: Vec<String> = fs::read_dir(w.home.join("logs"))
        .unwrap()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("work-7-") && n.ends_with(".jsonl.console")
        })
        .flat_map(|e| lines(&fs::read_to_string(e.path()).unwrap()))
        .collect();
    said.sort();
    said.dedup();
    assert_eq!(said, ["cannot write"]);
}

#[test]
fn without_a_console_no_cycle_starts_exit_78_no_container_and_it_says_to_install() {
    let w = World::new("no-console");
    let console = w.tree.join("libexec/console");
    fs::rename(&console, w.path("console.kept")).unwrap();
    w.clear_calls();
    // Only the console of this tree may answer, as on a host.
    let r = w.sh(&format!(
        "unset TEHAS_WATCH_TICK TEHAS_CONSOLE_CMD TEHAS_WATCH_CLOCK; exec {} 7",
        w.work_cmd().display()
    ));
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!(
            "tehas work: no console beside this command at {}, run install.sh",
            console.display()
        ),
    );
    assert_eq!(cat_calls(&w, "docker"), "");
}

/// The console's masks are read off a start whose log has a line in it.
fn masked(name: &str) -> World {
    let w = World::new(name);
    w.minimal(&["PI_ENSEMBLE_HOST_ALIASES=gpu1:192.0.2.9,gb:192.0.2.10,,v6host:2001:db8::1"]);
    w.fs(
        "home/.pi/agent/models.json",
        "{\"providers\":{\"other\":{\"baseUrl\":\"http://elsewhere.lan/v1\"},\"test-provider\":{\"baseUrl\":\"http://models.lan:8086/v1\"}}}\n",
    );
    w.fs(
        "container.sh",
        "echo \"{\\\"v\\\":1,\\\"kind\\\":\\\"log\\\",\\\"message\\\":\\\"working\\\"}\" >&2\n",
    );
    w
}

#[test]
fn the_console_masks_the_host_aliases_the_model_host_and_the_bots_email_not_names_under_4_characters(
) {
    let w = masked("masks");
    w.rm("calls/console");
    w.work7();
    let given = progress_line(&w);
    has(&given, " --private gpu1 --private 192.0.2.9 --private 192.0.2.10 --private v6host --private 2001:db8::1 --private models.lan --private bot@example.com");
    lacks(&given, "--private gb ");
    lacks(&given, "elsewhere");
    lacks(&w.calls("console"), "--mask");
    has(
        &progress_line(&w),
        &format!("--now {}/live/console-7.txt.now", w.home.display()),
    );
    lacks(&w.calls("console"), "test-token");
}

/// The console that keeps the two lines a case wrote in now.step and now.text.
const NOW_CONSOLE: &str = r#"#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/console"
now=
prev=
for a in "$@"; do
  [ "$prev" = --now ] && now=$a
  prev=$a
done
[ -n "$now" ] || exit 0
[ -e "$T/now.step" ] || exit 0
printf '%s\n%s\n' "$(cat "$T/now.step")" "$(cat "$T/now.text")" >"$now.new"
mv "$now.new" "$now"
exit 0
"#;

fn live_like(w: &World, end: &str) -> usize {
    fs::read_dir(&w.live)
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(end))
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn the_status_line_is_read_from_the_consoles_now_file_and_a_stale_one_is_taken_away() {
    let w = World::new("now-file");
    w.container(r#"progress "{\"v\":1,\"kind\":\"log\",\"message\":\"a notice\"}"; exit 0"#);
    let console = w.tree.join("libexec/console");
    w.fs("tree/libexec/console", NOW_CONSOLE);
    fs::set_permissions(&console, fs::Permissions::from_mode(0o755)).unwrap();
    // The file of a cycle before this one, which the start takes away.
    w.home_file("live/console-7.txt.now", "stale\n");
    w.fs("now.step", "develop");
    w.fs("now.text", "step 3 of the cycle");
    w.clear_calls();
    w.work7();
    has(
        &progress_line(&w),
        &format!("--now {}/console-7.txt.now", w.live.display()),
    );
    // The end of the cycle takes it away: nothing else clears live/.
    assert_eq!(live_like(&w, ".now"), 0);
    assert_eq!(live_like(&w, ".now.new"), 0);

    // Without the file the step is starting and the notice empty.
    w.fs(
        "tree/libexec/console",
        "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>\"$T/calls/console\"\nexit 0\n",
    );
    fs::set_permissions(&console, fs::Permissions::from_mode(0o755)).unwrap();
    w.home_file("live/console-7.txt.now", "stale\n");
    w.clear_calls();
    w.rm("now.step");
    w.work7();
    lacks(
        &w.row(&["cycle-show", "7"]).to_string(),
        r#""step":"develop""#,
    );
    assert!(!w.live.join("console-7.txt.now").exists());
}

#[test]
fn the_console_masks_the_host_of_a_roles_own_provider_too() {
    let w = masked("masks-role");
    w.minimal(&[
        "PI_ENSEMBLE_MODEL_DEVELOPER=big",
        "PI_ENSEMBLE_PROVIDER_DEVELOPER=other",
    ]);
    w.rm("calls/console");
    w.work7();
    has(
        &progress_line(&w),
        "--private models.lan --private elsewhere.lan --private bot@example.com",
    );
}

#[test]
fn without_a_baseurl_or_aliases_the_console_is_given_the_bots_email_only() {
    let w = masked("masks-none");
    w.minimal(&[]);
    w.fs("home/.pi/agent/models.json", "{\"providers\":{}}\n");
    w.rm("calls/console");
    w.work7();
    let line = progress_line(&w);
    let i = line.find("--private ").expect("a --private");
    assert_eq!(&line[i..], "--private bot@example.com");
}

#[test]
fn the_cycle_works_in_a_fresh_clone_at_the_branchs_tip_with_the_products_origin() {
    let (w, _) = ran("clone");
    assert_eq!(
        git_out(&w.ws, &["rev-parse", "HEAD"]),
        git_out(&w.upstream, &["rev-parse", "main"])
    );
    assert_eq!(git_out(&w.ws, &["branch", "--show-current"]), "main");
    assert_eq!(
        git_out(&w.ws, &["config", "--get", "remote.origin.url"]),
        git_out(&w.checkout, &["config", "--get", "remote.origin.url"])
    );
    assert_eq!(
        git_out(
            &w.ws,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]
        ),
        "origin/main"
    );
}

#[test]
fn the_product_checkout_is_not_mounted_anywhere() {
    let (w, _) = ran("no-checkout");
    lacks(
        &cat_calls(&w, "docker.args."),
        &w.checkout.to_string_lossy(),
    );
}

#[test]
fn exactly_the_documented_paths_are_mounted() {
    let (w, _) = ran("mounts");
    assert_eq!(mounts(&w), sorted(lines(&w.mounts_off())));
}

#[test]
fn the_agents_are_told_the_path_inside_the_container_not_the_hosts() {
    let (w, _) = ran("inside");
    let env = w.after("-e").join("\n");
    has(
        &env,
        &format!("PI_ENSEMBLE_WORKSPACE_ROOT={}", w.inside.display()),
    );
    lacks(&env, "widget-7");
}

#[test]
fn nothing_is_made_on_the_host_at_the_path_inside_the_container() {
    let (w, _) = ran("inside-unmade");
    assert!(!w.inside.exists());
}

#[test]
fn the_cycles_transcripts_directory_is_its_own_and_is_made_for_it() {
    let (w, _) = ran("runs");
    assert!(w.runs.is_dir());
}

#[test]
fn the_cycles_own_state_is_made_beside_its_workspace_and_no_shared_directory_is_mounted() {
    let (w, _) = ran("state");
    assert_eq!(entries(&w.state, true), "cache memory relay sessions ");
    let args = cat_calls(&w, "docker.args.");
    lacks(&args, "pi-ensemble");
    lacks(&args, &w.path("home/.pi/agent/sessions").to_string_lossy());
    lacks(&args, &w.path("home/.vipune").to_string_lossy());
}

/// The host's code index, with a log of its own run beside it.
fn index(w: &World) {
    fs::create_dir_all(w.hostindex.join("logs")).unwrap();
    fs::write(w.hostindex.join("widget.db"), "index\n").unwrap();
    fs::write(w.hostindex.join("_config.db"), "cfg\n").unwrap();
    fs::write(w.hostindex.join("logs/index-1.log"), "log\n").unwrap();
}

#[test]
fn the_cycle_is_given_a_copy_of_the_hosts_code_index_and_the_agents_are_told_where() {
    let w = World::new("index");
    index(&w);
    w.clear_calls();
    w.work7();
    let cache = w.state.join("cache");
    assert_eq!(entries(&cache, false), "_config.db widget.db ");
    assert_eq!(
        fs::read_to_string(cache.join("widget.db")).unwrap(),
        "index\n"
    );
    assert_eq!(
        fs::read_to_string(w.hostindex.join("widget.db")).unwrap(),
        "index\n"
    );
    let found: Vec<String> = w
        .after("-e")
        .into_iter()
        .filter(|e| e.starts_with("CBM_CACHE_DIR"))
        .collect();
    assert_eq!(found, ["CBM_CACHE_DIR=/home/vscode/.tehas-cache"]);
}

#[test]
fn the_copy_holds_the_index_and_not_what_an_interrupted_index_run_left_behind() {
    let w = World::new("index-left");
    index(&w);
    fs::write(
        w.hostindex
            .join("widget.db.stage.abc123.tmp.945328.0x200002bc000"),
        "left\n",
    )
    .unwrap();
    fs::write(w.hostindex.join("scratch.tmp"), "gone\n").unwrap();
    w.clear_calls();
    w.work7();
    assert_eq!(
        entries(&w.state.join("cache"), false),
        "_config.db widget.db "
    );
}

#[test]
fn what_one_cycle_wrote_in_its_own_state_is_not_what_the_next_cycle_of_the_issue_is_given() {
    let w = World::new("state-again");
    index(&w);
    w.work7();
    fs::write(w.state.join("memory/note.json"), "planted\n").unwrap();
    fs::write(w.state.join("cache/widget.db"), "planted\n").unwrap();
    w.clear_calls();
    w.work7();
    assert_eq!(fs::read_dir(w.state.join("memory")).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(w.state.join("cache/widget.db")).unwrap(),
        "index\n"
    );
}

#[test]
fn exactly_the_documented_variables_are_passed_in() {
    let (w, _) = ran("variables");
    assert_eq!(env_names(&w), want_and(&[]));
}

#[test]
fn the_remote_path_on_a_contractor_the_container_gets_exactly_the_same_mounts_and_variables() {
    // A cycle run for a contractor (outsource on, TEHAS_CONTRACTOR named)
    // renews its token and relays its issue writes through the contractor
    // program, and gives its container nothing more and nothing else
    // (AGENTS.md rule 4).
    let (w, _) = ran("contractor");
    let local_mounts = mounts(&w);
    w.stub(
        "contractor",
        r#"case $1 in token) echo ghs_from_factory ;; relay) printf %s "{\"out\":\"\",\"err\":\"\",\"rc\":0}" ;; own) printf %s "$3" >"$T/calls/contractor.own.as"; shift 3; printf "%s\n" "$@" >"$T/calls/contractor.own" ;; esac"#,
    );
    w.minimal(&[ALIAS, "TEHAS_OUTSOURCE=on"]);
    w.env("TEHAS_TEST_FEATURES", "outsource");
    let contractor = w.path("bin/contractor").to_string_lossy().into_owned();
    w.env("TEHAS_CONTRACTOR", &contractor);
    w.clear_calls();
    let r = w.work7();
    ok2(&r);
    assert_eq!(mounts(&w), local_mounts);
    assert_eq!(env_names(&w), want_and(&[]));
    let renew = w.read("renew.calls");
    has(
        renew.lines().last().unwrap_or_default(),
        &format!("renew --cmd {contractor} --arg token "),
    );
    // A contractor holds no app token: the container's token is the cycle
    // token the factory made, and no app token was asked for.
    assert_eq!(out(&w.calls("docker.token")), "ghs_from_factory");
    lacks(&cat_calls(&w, "docker.args."), "ghs_from_factory");
    lacks(&w.calls("docker.token"), "ghs_app");
    // Before the container starts, the program is asked to hand the factory's
    // account every folder the container writes: the sources of its
    // read-write mounts, no more.
    let written: Vec<String> = w
        .after("-v")
        .into_iter()
        .filter(|m| !m.ends_with(":ro"))
        .map(|m| m.split(':').next().unwrap().to_string())
        .collect();
    assert_eq!(sorted(lines(&w.calls("contractor.own"))), sorted(written));
    // They go only to the account the container runs as: its --user.
    assert_eq!(w.calls("contractor.own.as"), w.after("--user").join("\n"));
}

#[test]
fn read_only_a_kept_cycle_token_made_before_the_setting_which_may_write_reaches_no_container() {
    let w = World::new("read-only-kept");
    w.minimal(&[ALIAS, "TEHAS_READ_ONLY=1"]);
    w.clear_calls();
    let r = w.work7();
    assert_ne!(r.rc, 0, "a cycle started with a token that may write");
    assert!(w.preflight_args().is_empty());
    assert!(w.cycle_args().is_empty());
}

#[test]
fn read_only_the_container_is_told_so_and_its_git_pushes_to_origin_land_in_the_workspace() {
    let w = World::new("read-only");
    w.minimal(&[ALIAS, "TEHAS_READ_ONLY=1"]);
    // The mark of a token made on a read-only instance (console/ops/src/github.rs).
    w.home_file("state/github-cycle/token.read-only", "");
    w.clear_calls();
    w.work7();
    assert_eq!(
        env_names(&w),
        want_and(&[
            "GIT_CONFIG_KEY_4",
            "GIT_CONFIG_KEY_5",
            "GIT_CONFIG_VALUE_4",
            "GIT_CONFIG_VALUE_5",
            "TEHAS_READ_ONLY",
        ])
    );
    let picked: String = w
        .after("-e")
        .iter()
        .filter(|e| {
            [
                "GIT_CONFIG_COUNT=",
                "GIT_CONFIG_KEY_4=",
                "GIT_CONFIG_VALUE_5=",
                "TEHAS_READ_ONLY=",
            ]
            .iter()
            .any(|p| e.starts_with(p))
        })
        .map(|e| format!("{e} "))
        .collect();
    assert_eq!(
        picked,
        "GIT_CONFIG_COUNT=6 GIT_CONFIG_KEY_4=remote.origin.pushurl GIT_CONFIG_VALUE_5=https://github.com/ TEHAS_READ_ONLY=1 "
    );
    assert_eq!(
        given_value(&w, "GIT_CONFIG_VALUE_4"),
        given_value(&w, "PI_ENSEMBLE_WORKSPACE_ROOT")
    );
    // The push check is given the same: its dry run asks the workspace.
    has(
        &w.preflight_args().join(" "),
        "GIT_CONFIG_KEY_4=remote.origin.pushurl",
    );
}

#[test]
fn the_polls_token_which_the_load_keeps_is_not_passed_in() {
    let w = World::new("poll-token");
    w.env("TEHAS_GH_TOKEN", "ghs_poll");
    w.clear_calls();
    w.work7();
    assert_eq!(env_names(&w), want_and(&[]));
    lacks(&w.cycle_args().join("\n"), "ghs_poll");
}

#[test]
fn the_child_bound_is_tehas_not_the_instances_and_takes_its_default_when_the_case_names_none() {
    let w = World::new("child-bound");
    w.minimal(&[
        "PI_ENSEMBLE_SPAWN_TIMEOUT_MS=60000",
        "TEHAS_CHILD_TIMEOUT=5400",
    ]);
    w.clear_calls();
    w.work7();
    let spawn: Vec<String> = w
        .after("-e")
        .into_iter()
        .filter(|e| e.starts_with("PI_ENSEMBLE_SPAWN_TIMEOUT_MS"))
        .collect();
    assert_eq!(spawn, ["PI_ENSEMBLE_SPAWN_TIMEOUT_MS=5400000"]);
    w.minimal(&[]);
    w.clear_calls();
    w.work7();
    assert_eq!(given_value(&w, "PI_ENSEMBLE_SPAWN_TIMEOUT_MS"), "7200000");
}

#[test]
fn a_roles_own_model_is_passed_in_under_the_names_pi_rukas_reads_and_nothing_else_is() {
    let w = World::new("role-model");
    w.minimal(&["PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model"]);
    w.clear_calls();
    w.work7();
    assert_eq!(
        env_names(&w),
        want_and(&[
            "PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST",
            "PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST",
        ])
    );
    let n = w
        .cycle_args()
        .iter()
        .filter(|a| {
            *a == "PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST"
                || *a == "PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST"
        })
        .count();
    assert_eq!(n, 2);
}

/// A provider with a backup, each endpoint with a token models.json names,
/// where only the backup answers.
fn two_endpoints(w: &World) {
    w.minimal(&[]);
    let models = json!({"providers": {"test-provider": {"baseUrl": "https://main.test/v1",
        "apiKey": "$TEHAS_MODEL_KEY_MAIN", "backupBaseUrl": "https://fleet.test/v1",
        "backupApiKey": "$TEHAS_MODEL_KEY_FLEET", "models": [{"id": "test-model"}]}}});
    w.fs("home/.pi/agent/models.json", format!("{models}\n"));
    w.stub(
        "curl",
        r#"case "$*" in *fleet.test*) echo "{\"data\":[{\"id\":\"test-model\"}]}" ;; *) exit 7 ;; esac"#,
    );
}

#[test]
fn the_tokens_models_json_names_are_passed_in_by_name_and_the_cycle_is_on_the_endpoint_picked_at_its_start(
) {
    let w = World::new("model-keys");
    two_endpoints(&w);
    w.env("TEHAS_MODEL_KEY_MAIN", "main-secret");
    w.env("TEHAS_MODEL_KEY_FLEET", "fleet-secret");
    w.clear_calls();
    let r = w.work7();
    assert_eq!(
        env_names(&w),
        want_and(&["TEHAS_MODEL_KEY_FLEET", "TEHAS_MODEL_KEY_MAIN"])
    );
    lacks(&w.cycle_args().join("\n"), "secret");
    let m: Value =
        serde_json::from_str(&fs::read_to_string(w.state.join("models.json")).unwrap()).unwrap();
    let p = &m["providers"]["test-provider"];
    assert_eq!(
        json!([p["baseUrl"], p["apiKey"], p.get("backupBaseUrl").is_some()]),
        json!(["https://fleet.test/v1", "$TEHAS_MODEL_KEY_FLEET", false])
    );
    has(
        &r.out,
        "tehas work: model endpoint: provider test-provider is on its backup",
    );
    has(
        &progress_line(&w),
        "--private main.test --private fleet.test",
    );
}

#[test]
fn a_token_models_json_names_and_the_host_has_not_set_stops_the_cycle_before_docker_exit_78() {
    let w = World::new("model-key-unset");
    two_endpoints(&w);
    w.env("TEHAS_MODEL_KEY_MAIN", "main-secret");
    w.clear_calls();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "models.json names TEHAS_MODEL_KEY_FLEET and it is not set",
    );
    assert_eq!(format!("{}{}", w.calls("docker"), w.calls("curl")), "");
}

#[test]
fn without_models_json_nothing_runs_exit_78_and_no_docker_call() {
    let w = World::new("no-models");
    w.rm("home/.pi/agent/models.json");
    w.clear_calls();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!(
            "no {}: run install.sh",
            w.path("home/.pi/agent/models.json").display()
        ),
    );
    assert_eq!(w.calls("docker"), "");
}

#[test]
fn a_role_that_pi_rukas_does_not_know_stops_the_cycle_before_docker() {
    let w = World::new("unknown-role");
    w.minimal(&["PI_ENSEMBLE_MODEL_REVIEWER=small-model"]);
    w.clear_calls();
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "PI_ENSEMBLE_MODEL_REVIEWER names no role");
    assert_eq!(w.calls("docker"), "");
}

#[test]
fn the_cycles_container_gets_no_option_beyond_the_documented_ones() {
    let (w, _) = ran("options");
    let unknown = unknown_options(
        &w.cycle_args(),
        &w.image,
        &format!("--name {DOCKER_OPTS} --user --cidfile -w -v -e"),
        "--rm --init",
    );
    assert!(unknown.is_empty(), "{unknown:?}");
    assert_eq!(verb_line(&w), classic(&w, ""));
}

// The runner is no init: without one, a process that outlives its parent is
// never collected and counts against the pids limit until the container ends.
#[test]
fn the_cycles_container_has_an_init() {
    let (w, _) = ran("init");
    assert_eq!(w.cycle_args().iter().filter(|a| *a == "--init").count(), 1);
}

// The image is built on the host and its name is a Docker Hub name: nothing
// of that name is pulled and given the cycle's token. Each container of a
// start names the policy right after run.
#[test]
fn no_container_of_a_start_pulls_its_image() {
    let (w, _) = ran("no-pull");
    let (cycle, preflight) = (w.cycle_args(), w.preflight_args());
    assert_eq!(cycle.get(1).map(String::as_str), Some("--pull=never"));
    assert_eq!(preflight.get(1).map(String::as_str), Some("--pull=never"));
    assert_eq!(cycle.iter().filter(|a| a.starts_with("--pull")).count(), 1);
    assert_eq!(
        preflight.iter().filter(|a| a.starts_with("--pull")).count(),
        1
    );
    let caps = w.calls("docker.caps");
    assert!(!caps.is_empty(), "no --capabilities call");
    assert_eq!(
        count_lines(&caps, |l| !l.starts_with("run --pull=never --rm --init ")),
        0
    );
}

// --- Without a usable slice -------------------------------------------------

/// A world without its calls, workspaces and logs, on the good slice.
fn without(name: &str) -> World {
    let w = World::new(name);
    w.clear_calls();
    let _ = fs::remove_dir_all(w.home.join("work"));
    let _ = fs::remove_dir_all(w.home.join("logs"));
    w.slice_good();
    w
}

/// 78, and neither a clone nor a container was made.
#[track_caller]
fn refused(w: &World, r: &Run) {
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(docker_runs(w), 0);
    lacks(&w.calls("docker"), "run ");
    assert!(!w.home.join("work").exists());
    assert!(!w.home.join("logs").exists());
}

#[test]
fn without_the_slice_no_container_starts() {
    let w = without("no-slice");
    w.fs(
        "slice.show",
        "FragmentPath=\nActiveState=inactive\nMemoryMax=infinity\n",
    );
    let r = w.work7();
    refused(&w, &r);
    has(
        &r.out,
        "tehas work: no slice file on this host (sudo ./bootstrap.sh slice)",
    );
}

#[test]
fn a_cgroupfs_driver_is_refused_the_same_way() {
    let w = without("cgroupfs");
    w.fs("docker.info", "cgroupfs 2\n");
    let r = w.work7();
    refused(&w, &r);
    has(&r.out, "tehas work: Docker runs containers with the cgroupfs cgroup driver on cgroup v2, tehas needs systemd on v2");
}

#[test]
fn cgroup_v1_is_refused() {
    let w = without("cgroup-v1");
    w.fs("docker.info", "systemd 1\n");
    let r = w.work7();
    refused(&w, &r);
    has(&r.out, "on cgroup v1, tehas needs systemd on v2");
}

#[test]
fn a_systemctl_that_answers_without_memorymax_starts_no_container() {
    let w = without("no-memorymax");
    w.fs(
        "slice.show",
        "FragmentPath=/etc/systemd/system/tehas.slice\nActiveState=active\n",
    );
    let r = w.work7();
    refused(&w, &r);
    has(&r.out, "tehas work: tehas.slice has no memory limit");
}

#[test]
fn a_systemctl_that_fails_starts_no_container() {
    let w = without("systemctl-fails");
    w.stub("systemctl", "exit 1");
    let r = w.work7();
    refused(&w, &r);
    has(&r.out, "tehas work: systemctl gave no answer");
}

#[test]
fn a_cgroup_tree_without_the_controllers_starts_no_container() {
    let w = without("controllers");
    w.fs("cgroup/cgroup.controllers", "cpuset io\n");
    let r = w.work7();
    refused(&w, &r);
    has(
        &r.out,
        "cgroup.controllers does not list cpu, memory and pids",
    );
}

#[test]
fn a_reserve_above_the_ceiling_is_refused_and_names_both_numbers_and_the_smallest_host() {
    let w = without("reserve");
    w.minimal(&["TEHAS_CYCLE_RESERVE=3g"]);
    let r = w.work7();
    refused(&w, &r);
    has(&r.out, "tehas work: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m, tehas.slice's MemoryMax over TEHAS_SLICE_CONTAINERS=4); the host needs about 16 GiB");
}

#[test]
fn a_checkout_of_another_repository_starts_no_container() {
    let w = World::new("other-repo");
    w.clear_calls();
    git_out(
        &w.checkout,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/other.git",
        ],
    );
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "not of acme/widget");
    assert_eq!(docker_runs(&w), 0);
}

#[test]
fn no_token_to_be_had_starts_no_container() {
    let w = World::new("no-token");
    w.clear_calls();
    let _ = fs::remove_dir_all(w.home.join("state/github"));
    let _ = fs::remove_dir_all(w.home.join("state/github-cycle"));
    let key = w.path("no-such-key.pem").to_string_lossy().into_owned();
    w.env("TEHAS_GITHUB_APP_KEY", &key);
    let r = w.work7();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "tehas work: no GitHub token for the app");
    assert_eq!(docker_runs(&w), 0);
}

#[test]
fn the_cycles_containers_are_given_the_cycle_token_and_the_hosts_token_is_nowhere_in_them() {
    let w = World::new("cycle-token");
    w.clear_calls();
    w.work7();
    assert_eq!(out(&w.calls("docker.token")), "ghs_cycle");
    has(&w.cycle_args().join("\n"), "pi-rukas-headless");
    let args = cat_calls(&w, "docker.args.");
    lacks(&args, "ghs_app");
    lacks(&args, "ghs_cycle");
    has(&w.preflight_args().join("\n"), "GH_TOKEN");
}

#[test]
fn with_a_github_app_the_container_is_given_the_apps_narrowed_cycle_token_never_the_apps_own() {
    let w = World::new("app-token");
    w.clear_calls();
    w.work7();
    assert_eq!(out(&w.calls("docker.token")), "ghs_cycle");
    let run_args = w.cycle_args().join("\n");
    has(&run_args, "pi-rukas-headless");
    has(
        &run_args,
        &format!(
            "{}/state/github-cycle:/run/tehas/github:ro",
            w.home.display()
        ),
    );
    lacks(&run_args, "state/github:");
    lacks(&run_args, "ghs_app");
    lacks(&run_args, "user-token");
    lacks(&run_args, "github-app.pem");
    lacks(&cat_calls(&w, "docker."), "user-token");
}

#[test]
fn with_a_github_app_and_no_cycle_token_to_be_had_no_cycle_starts() {
    let w = World::new("no-cycle-token");
    w.clear_calls();
    let _ = fs::remove_file(w.home.join("state/github-cycle/token"));
    let _ = fs::remove_file(w.home.join("state/github-cycle/token.expires"));
    // No key to make one with: tehasbot github-token --cycle fails.
    let key = w.path("no-such-key.pem").to_string_lossy().into_owned();
    w.env("TEHAS_GITHUB_APP_KEY", &key);
    let r = w.work7();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "tehas work: no cycle token from the app, not starting a cycle",
    );
    assert_eq!(docker_runs(&w), 0);
}

fn scratch(w: &World) -> String {
    let dir = w.path("scratch").to_string_lossy().into_owned();
    w.minimal(&[&format!("TEHAS_SCRATCH_DIR={dir}")]);
    dir
}

#[test]
fn a_scratch_disk_takes_the_place_of_the_tmpfs_at_the_same_path_and_for_this_cycle_alone() {
    let w = World::new("scratch");
    fs::create_dir_all(w.path("scratch")).unwrap();
    let dir = scratch(&w);
    w.work7();
    assert_eq!(count_of(&w.cycle_args(), "--tmpfs"), 0);
    has(&w.cycle_args().join("\n"), &format!("{dir}/widget-7:/tmp"));
    let mode = fs::metadata(w.path("scratch/widget-7"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(format!("{:o}", mode & 0o7777), "1777");
    assert_eq!(w.after("--user"), [user()]);
}

#[test]
fn a_scratch_disk_that_is_not_there_starts_no_container() {
    let w = World::new("scratch-gone");
    let dir = scratch(&w);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert!(w.cycle_args().is_empty());
    assert!(w.preflight_args().is_empty());
    assert_eq!(
        cycle_file(&w, "7.refused"),
        format!("scratch-unmade {dir}/widget-7")
    );
}

// --- The watch and the cycle clock ------------------------------------------

/// The limits of a watched cycle, with no grace at its end.
fn clocked(name: &str) -> World {
    let w = World::new(name);
    w.fresh();
    w.minimal(&[
        "TEHAS_WORK_TIMEOUT=600",
        "TEHAS_CHILD_TIMEOUT=600",
        "TEHAS_WORK_STALL=60",
        "TEHAS_VERIFY_TIMEOUT=1",
    ]);
    w.env("TEHAS_WORK_GRACE", "0");
    w
}

#[test]
fn the_watch_loop_stops_a_cycle_by_the_cycle_clock_not_the_wall_clock() {
    // The wall clock stands still and the cycle clock goes past the stall of
    // 60 s while the limit of 600 s is still ahead: the cycle ends on its own.
    let w = clocked("clock-wall");
    w.fs("clock.now", "1000\n1010\n1020\n1030\n1040\n1050\n");
    w.container("says handoff; exit 10");
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    lacks(&r.out, "time limit of 600s passed");
    lacks(&r.out, "stopping it as stalled");
}

#[test]
fn logs_that_stand_still_do_not_move_the_stalls_mark_and_the_mark_is_a_clock_reading() {
    let w = clocked("clock-mark");
    w.rm("clock.calls");
    let readings: String = (5000..=5200)
        .step_by(10)
        .map(|n| format!("{n}\n"))
        .collect();
    w.fs("clock.now", readings);
    w.fs("clock.check", "ok\n");
    // The container stays until the loop has passed three times, however
    // loaded the host is, and gives up after 30 s.
    w.container(r#"for _ in $(seq 1 300); do [ "$(wc -l <"$T/clock.calls" 2>/dev/null || echo 0)" -ge 3 ] && break; "$nap" 0.1; done; exit 10"#);
    w.work7();
    let calls = w.read("clock.calls");
    let mut since: Vec<u64> = calls
        .lines()
        .filter_map(|l| {
            let (_, rest) = l.split_once("--since ")?;
            rest.split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        })
        .collect();
    since.sort();
    since.dedup();
    assert!(calls.lines().count() >= 3, "{calls}");
    assert!(since.len() <= 2, "{since:?}");
    assert!(since.iter().all(|s| *s <= 6000), "{since:?}");
}

#[test]
fn the_cycle_clock_saying_stalled_stops_the_cycle_as_stalled() {
    let w = clocked("clock-stalled");
    let _ = fs::remove_file(w.home.join("state/cycles/7.stalled"));
    w.fs("clock.check", "stalled\n");
    w.container(&format!("{HOLD}; {GONE}"));
    let r = w.work7();
    ok2(&r);
    has(
        &r.out,
        "none of the cycle's logs changed for 60s, stopping it as stalled",
    );
    has(&r.out, "status=stalled");
    lacks(&r.out, "time limit");
}

#[test]
fn the_cycle_clock_saying_over_stops_the_cycle_at_its_time_limit() {
    let w = clocked("clock-over");
    w.fs("clock.check", "over\n");
    w.container(&format!("{HOLD}; {GONE}"));
    let r = w.work7();
    ok2(&r);
    has(&r.out, "time limit of 600s passed");
    has(&r.out, "status=timeout");
    lacks(&r.out, "stalled");
}

#[test]
fn a_cycle_clock_that_answers_unknown_stops_nothing_the_cycle_ends_on_its_own() {
    let w = clocked("clock-unknown");
    w.fs("clock.check", "unknown\n");
    w.container("says handoff; exit 10");
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    lacks(&r.out, "time limit of 600s passed");
    lacks(&r.out, "stopping it as stalled");
    lacks(&r.out, "status=timeout");
}

fn freeze_left(w: &World, names: &[&str]) -> Vec<String> {
    fs::read_dir(w.home.join("state/cycles"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| names.contains(&n.as_str()))
        .collect()
}

#[test]
fn a_frozen_cycle_is_paused_is_not_stopped_as_stalled_and_its_freeze_ends_with_it() {
    let w = clocked("frozen");
    w.fs("clock.check", "stalled\n");
    w.home_file("state/cycles/7.freeze", "{}\n");
    w.container(r#""$nap" 1; says handoff; exit 10"#);
    let r = w.work7();
    assert_eq!(r.rc, 10, "{}", r.out);
    has(&r.out, "#7 frozen: its container is paused");
    lacks(&r.out, "stopping it as stalled");
    assert_eq!(out(&w.calls("docker.freeze")), "pause");
    assert!(freeze_left(&w, &["7.freeze", "7.frozen"]).is_empty());
}

#[test]
fn a_thaw_unpauses_the_container_moves_its_start_and_its_clocks_count_again() {
    let w = clocked("thaw");
    w.fs("clock.check", "stalled\n");
    w.home_file("state/cycles/7.freeze", "{}\n");
    // The container thaws itself a second in, as tehas thaw would, and is
    // then stopped as stalled: the clock's answer counts once it is thawed.
    w.container(&format!(
        r#""$nap" 1; rm -f "$T/home/tehas/state/cycles/7.freeze"; {HOLD}; {GONE}"#
    ));
    let r = w.work7();
    ok2(&r);
    has(&r.out, "#7 frozen: its container is paused");
    has(&r.out, "#7 thawed after");
    has(
        &r.out,
        "none of the cycle's logs changed for 60s, stopping it as stalled",
    );
    let freeze: String = w
        .calls("docker.freeze")
        .lines()
        .map(|l| format!("{l} "))
        .collect();
    assert_eq!(freeze, "pause unpause ");
    assert!(freeze_left(&w, &["7.frozen"]).is_empty());
}

#[test]
fn the_container_is_given_a_deadline_of_the_wall_clock_and_the_mark_is_the_cycles() {
    let w = World::new("deadline");
    w.fresh();
    w.work7();
    let deadline: u64 = given_value(&w, "PI_ENSEMBLE_CYCLE_DEADLINE")
        .parse()
        .unwrap();
    assert!(deadline > 0 && deadline.is_multiple_of(1000), "{deadline}");
    assert!(cycle_file(&w, "7.clock")
        .chars()
        .any(|c| c.is_ascii_digit()));
}

// --- The driver ---------------------------------------------------------------

/// What a runner that can run lean answers: work-lean with its own flags,
/// the schema 5 result and its named features.
fn caps_lean(w: &World) {
    w.caps_good();
    w.caps_edit(|v| {
        v["verbs"]["work-lean"] = json!({"flags": ["--json", "--timeout"]});
        let f = v["features"].as_object_mut().unwrap();
        f.insert("workLeanResultSchema".into(), json!(5));
        f.insert("leanDeadline".into(), json!(true));
        f.insert("leanHandoffHost".into(), json!(true));
    });
}

fn lean_fresh(w: &World) {
    w.fresh();
    let mut retry = w.ws.clone().into_os_string();
    retry.push(".before-retry");
    for d in [w.ws.clone(), w.state.clone(), retry.into()] {
        let _ = fs::remove_dir_all(d);
    }
    let _ = fs::remove_file(w.home.join("state/cycles/7.lean-refused"));
    let _ = fs::remove_file(w.home.join("state/cycles/7.refused"));
    fs::create_dir_all(w.fx.join("net")).unwrap();
    w.issue_is("Words only.", "", "");
}

fn lean_record(w: &World) {
    w.record_is("alice", "Words only.", &[]);
    w.record_edit(|v| v["driver"] = json!("lean"));
}

fn driver(name: &str) -> World {
    let w = World::new(name);
    w.stub_github();
    lean_fresh(&w);
    w
}

/// lean on, as a test build may turn it on.
fn lean_on(name: &str) -> World {
    let w = driver(name);
    w.env("TEHAS_TEST_FEATURES", "lean");
    w.minimal(&[ALIAS, "TEHAS_LEAN=on"]);
    w
}

fn cidfile_calls(w: &World) -> usize {
    count_lines(&w.calls("docker"), |l| l.contains("--cidfile"))
}

// lean is still being built: off, a lean start of any kind is 78, starts
// nothing and never runs classic in its place.
#[test]
fn with_lean_off_driver_lean_is_78_starts_no_container_and_names_the_feature() {
    let w = driver("lean-off");
    caps_lean(&w);
    let r = w.work(&["7", "--driver", "lean"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "tehas work: --driver lean needs the feature lean, and lean is still being built in this release; not starting a cycle");
    assert!(w.cycle_args().is_empty());
    assert_eq!(cidfile_calls(&w), 0);
}

#[test]
fn with_lean_off_a_record_that_says_driver_lean_is_78_and_does_not_run_classic() {
    let w = driver("lean-off-record");
    caps_lean(&w);
    lean_record(&w);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "the approval of #7, which names driver lean, needs the feature lean",
    );
    assert!(w.cycle_args().is_empty());
}

#[test]
fn with_lean_off_driver_classic_and_a_record_with_no_driver_run_classic_work() {
    let w = driver("lean-off-classic");
    caps_lean(&w);
    let r = w.work(&["7", "--driver", "classic"]);
    ok2(&r);
    assert_eq!(verb_line(&w), classic(&w, ""));
}

#[test]
fn a_record_with_no_driver_runs_classic_work() {
    let w = lean_on("lean-none");
    caps_lean(&w);
    w.record_is("alice", "Words only.", &[]);
    let r = w.work7();
    ok2(&r);
    assert_eq!(verb_line(&w), classic(&w, ""));
}

#[test]
fn a_record_that_says_driver_lean_runs_work_lean_bounded_by_the_same_limit_and_deadline() {
    let w = lean_on("lean-record");
    caps_lean(&w);
    lean_record(&w);
    let r = w.work7();
    ok2(&r);
    assert_eq!(verb_line(&w), lean(&w));
    let deadline: u64 = given_value(&w, "PI_ENSEMBLE_CYCLE_DEADLINE")
        .parse()
        .unwrap();
    assert!(deadline > 0);
    has(
        &w.after("-e").join("\n"),
        "PI_ENSEMBLE_HANDOFF_CHANNEL=host",
    );
    let exclude = fs::read_to_string(w.ws.join(".git/info/exclude")).unwrap();
    assert_eq!(count_lines(&exclude, |l| l == "/.pi/lean-state/"), 1);
    assert!(!cycle_exists(&w, "7.lean-refused"));
}

#[test]
fn driver_lean_by_hand_runs_work_lean_and_driver_classic_runs_work() {
    let w = lean_on("lean-hand");
    caps_lean(&w);
    let r = w.work(&["7", "--driver", "lean"]);
    ok2(&r);
    assert_eq!(verb_line(&w), lean(&w));
    lean_fresh(&w);
    lean_record(&w);
    w.work(&["7", "--driver", "classic"]);
    assert_eq!(verb_line(&w), classic(&w, ""));
}

#[test]
fn driver_other_is_64_and_starts_nothing() {
    let w = lean_on("lean-other");
    let r = w.work(&["7", "--driver", "other"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "--driver takes lean or classic, not other");
    assert_eq!(w.calls("docker"), "");
    let r = w.work(&["7", "--driver"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    assert_eq!(w.calls("docker"), "");
}

#[test]
fn a_lean_start_on_an_image_with_no_lean_is_78_starts_no_container_and_does_not_run_classic() {
    let w = lean_on("lean-no-image");
    w.caps_good();
    lean_record(&w);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "the feature workLeanResultSchema 5");
    has(&r.out, "the feature leanHandoffHost");
    has(
        &r.out,
        "(driver lean, work-lean; it does not run classic in its place)",
    );
    assert!(cycle_exists(&w, "7.lean-refused"));
    let refused = cycle_file(&w, "7.refused");
    assert_eq!(refused.lines().next(), Some("caps-lacking"));
    has(&refused, "the feature leanHandoffHost");
    assert!(w.cycle_args().is_empty());
    assert!(w.preflight_args().is_empty());
    assert!(!w.ws.exists());
}

#[test]
fn a_lean_restart_runs_work_lean_without_restart_every_cycle_is_a_fresh_clone_so_lean_has_no_state_to_discard(
) {
    let w = lean_on("lean-restart");
    caps_lean(&w);
    let r = w.work(&["7", "--driver", "lean", "--restart"]);
    ok2(&r);
    lacks(&r.out, "the flag --restart");
    assert_eq!(verb_line(&w), lean(&w));
    assert!(!cycle_exists(&w, "7.lean-refused"));
}

#[test]
fn a_classic_restart_still_passes_restart() {
    let w = lean_on("classic-restart");
    caps_lean(&w);
    let r = w.work(&["7", "--driver", "classic", "--restart"]);
    ok2(&r);
    assert_eq!(verb_line(&w), classic(&w, "--restart "));
}

#[test]
fn the_cycle_starts_the_token_renewer_and_one_that_cannot_start_refuses_the_cycle_before_its_container(
) {
    let w = World::new("renewer");
    w.minimal(&[]);
    w.clear_calls();
    w.rm("renew.calls");
    let r = w.work7();
    ok2(&r);
    has(
        &w.read("renew.calls"),
        &format!(
            "renew --cmd {}/bin/tehasbot --arg github-token --arg --cycle --every 300 --retry 30",
            w.home.display()
        ),
    );
    let renewer = w.home.join("libexec/tehas-cycle");
    fs::rename(&renewer, w.path("renewer.kept")).unwrap();
    w.clear_calls();
    w.rm("renew.calls");
    let r = w.work7();
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "tehas work: the token renewer could not start (");
    has(&r.out, "), not starting a cycle");
    assert_eq!(cidfile_calls(&w), 0);
    fs::rename(w.path("renewer.kept"), &renewer).unwrap();
}
