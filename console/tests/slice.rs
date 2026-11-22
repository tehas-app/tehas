//! `console slice` as a shell reads it: whether the slice on this host is
//! usable and how much room it has left, and the limits a container is
//! started with, which `tehas-cycle sandbox limits` makes from the same
//! reading. The settings come of lib/config.sh, the answer of `slice ok` is
//! eval'd and the room line is read with `read -r`, as the dispatcher did. The
//! tables of the limits are tested in console/core/src/slice.rs. Moved from
//! test/slice_test.sh.

mod common;

use common::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Instant;

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;
const MAX: u64 = 20 * GIB;
const RESERVE: u64 = 2 * GIB;
const FILE: &str = "/etc/systemd/system/tehas.slice";

/// The settings loaded, and two helpers: slice_verb runs a verb with what
/// slice_ok read, as the dispatcher hands it on; slice_ok evals the
/// assignments `console slice ok` prints.
const PRE: &str = r#". "$T_ROOT/lib/config.sh"; tehas_load || exit 9
slice_verb() { TEHAS_SLICE_MAX=${TEHAS_SLICE_MAX:-} TEHAS_SLICE_ACTIVE=${TEHAS_SLICE_ACTIVE:-} TEHAS_SLICE_CEILING_MB=${TEHAS_SLICE_CEILING_MB:-} "$TEHAS_CONSOLE_CMD" slice "$@" 2>/dev/null; }
slice_ok() { local out rc=0; out=$(slice_verb ok) || rc=1; eval "$out"; return "$rc"; }
"#;

/// The probe and what it left, on one line.
const OK: &str = r#"slice_ok; echo "rc=$? max=$TEHAS_SLICE_MAX active=$TEHAS_SLICE_ACTIVE ceiling=$TEHAS_SLICE_CEILING_MB why=[$TEHAS_SLICE_WHY]""#;

/// The words the program hands a command, on one line: the fourteen of the
/// limits, then the ten of the security options.
const LIMITS: &str =
    r#""$T_CYCLE" sandbox limits 'tehas t' 'a test' | tr '\0' ' '; echo "rc=${PIPESTATUS[0]}""#;

fn cycle_binary() -> PathBuf {
    static BUILT: Once = Once::new();
    built("tehas-cycle", &BUILT)
}

fn cgroup(w: &World) -> PathBuf {
    w.t().join("cgroup")
}

fn slice_dir(w: &World) -> PathBuf {
    cgroup(w).join("tehas.slice")
}

/// What systemctl show answers.
fn systemd(w: &World, file: &str, state: &str, max: &str) {
    w.stub(
        "systemctl",
        &format!("printf '%s\\n' 'MemoryMax={max}' 'ActiveState={state}' 'FragmentPath={file}'"),
    );
}

/// The cgroup tree, started again: the controllers, and the slice's pressure
/// with the two averages. No child.
fn slice_state(w: &World, full: &str, some: &str) {
    let _ = fs::remove_dir_all(cgroup(w));
    fs::create_dir_all(slice_dir(w)).unwrap();
    fs::write(
        cgroup(w).join("cgroup.controllers"),
        "cpuset cpu io memory hugetlb pids rdma misc\n",
    )
    .unwrap();
    fs::write(
        slice_dir(w).join("memory.pressure"),
        format!(
            "some avg10=0.00 avg60={some} avg300=0.00 total=0\nfull avg10=0.00 avg60={full} avg300=0.00 total=0\n"
        ),
    )
    .unwrap();
}

/// The memory a child holds beyond its anon and slab, in bytes.
#[derive(Default, Clone, Copy)]
struct Extra {
    file: u64,
    shmem: u64,
    kernel: u64,
}

/// One container in the slice: memory.stat, memory.low and memory.events.
fn child_with(w: &World, id: &str, anon: &str, slab: u64, low: &str, x: Extra) {
    let dir = scope(w, id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("memory.stat"),
        format!(
            "anon {anon}\nfile {}\nkernel {}\nkernel_stack 0\nshmem {}\nslab_reclaimable {slab}\nslab_unreclaimable 0\n",
            x.file,
            slab + x.kernel,
            x.shmem
        ),
    )
    .unwrap();
    fs::write(dir.join("memory.low"), format!("{low}\n")).unwrap();
    fs::write(
        dir.join("memory.events"),
        "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\noom_group_kill 0\n",
    )
    .unwrap();
}

fn child(w: &World, id: &str, anon: u64, slab: u64, low: &str) {
    child_with(w, id, &anon.to_string(), slab, low, Extra::default());
}

fn scope(w: &World, id: &str) -> PathBuf {
    slice_dir(w).join(format!("docker-{id}.scope"))
}

/// A container id: that digit 64 times.
fn id(digit: char) -> String {
    std::iter::repeat_n(digit, 64).collect()
}

/// A host where everything is in place: the slice loaded from its file and
/// active, Docker on the systemd driver and cgroup v2, an empty slice.
fn good(w: &World) {
    w.clear_calls();
    systemd(w, FILE, "active", &MAX.to_string());
    w.stub("docker", "echo 'systemd 2'");
    w.stub("nproc", "echo 8");
    slice_state(w, "0.00", "0.00");
    w.minimal(&[]);
    w.set_env(&[&format!("TEHAS_CGROUP_ROOT={}", cgroup(w).display())]);
}

fn host() -> World {
    let w = World::new("slice");
    w.env_always("TEHAS_CONSOLE_CMD", &w.console().to_string_lossy());
    w.env_always("T_CYCLE", &cycle_binary().to_string_lossy());
    good(&w);
    w
}

/// A snippet after the settings and the two helpers.
fn sh(w: &World, snippet: &str) -> String {
    w.sh(&format!("{PRE}{snippet}")).out
}

fn ok(w: &World) -> String {
    sh(w, OK)
}

/// What `console slice room <args>` prints after a probe.
fn room(w: &World, args: &str) -> String {
    sh(
        w,
        &format!(r#"slice_ok || echo "not ok: $TEHAS_SLICE_WHY"; slice_verb room {args}"#),
    )
}

fn calls(w: &World, name: &str) -> usize {
    w.calls(name).lines().count()
}

/// The room line: room, committed, max, reserve, and the rest.
fn line(room: u64, committed: u64, max: u64, rest: &str) -> String {
    format!("{room} {committed} {max} {RESERVE}{rest}")
}

fn usable(ceiling: u64) -> String {
    format!("rc=0 max={MAX} active=active ceiling={ceiling} why=[]")
}

fn unusable(why: &str) -> String {
    format!("rc=1 max= active= ceiling= why=[{why}]")
}

// --- slice ok ----------------------------------------------------------------

#[test]
fn ok_passes_with_a_file_a_limit_systemd_on_v2_and_the_controllers_in_one_call_each() {
    let w = host();
    assert_eq!(ok(&w), usable(5120));
    assert_eq!(
        w.calls("systemctl").trim_end(),
        "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
    );
    assert_eq!(
        w.calls("docker").trim_end(),
        r#"info --format {{.CgroupDriver}} {{.CgroupVersion}}{{"\n"}}{{.NCPU}}"#
    );
}

#[test]
fn an_inactive_slice_answers_from_its_file() {
    let w = host();
    systemd(&w, FILE, "inactive", &MAX.to_string());
    assert_eq!(
        ok(&w),
        format!("rc=0 max={MAX} active=inactive ceiling=5120 why=[]")
    );
}

#[test]
fn the_ceiling_is_a_quarter_of_the_max_in_mib_rounded_down() {
    let w = host();
    systemd(&w, FILE, "active", &(MAX + 3 * MIB + 5).to_string());
    let out = ok(&w);
    has(&out, "rc=0 ");
    has(&out, "ceiling=5120 ");
}

#[test]
fn a_slice_with_no_file_is_refused_and_names_the_root_step() {
    let w = host();
    systemd(&w, "", "inactive", "infinity");
    assert_eq!(
        ok(&w),
        unusable("no slice file on this host (sudo ./bootstrap.sh slice)")
    );
    assert_eq!(w.calls("docker"), "");
}

#[test]
fn a_slice_with_no_memory_limit_is_refused() {
    let w = host();
    systemd(&w, FILE, "active", "infinity");
    assert_eq!(
        ok(&w),
        unusable("tehas.slice has no memory limit (sudo ./bootstrap.sh slice)")
    );
    // An answer without MemoryMax is no memory limit.
    good(&w);
    w.stub("systemctl", &format!("echo FragmentPath={FILE}"));
    let out = ok(&w);
    has(&out, "rc=1 ");
    has(&out, "why=[tehas.slice has no memory limit");
}

#[test]
fn a_driver_or_a_cgroup_version_other_than_systemd_on_v2_is_refused_and_named() {
    let w = host();
    w.stub("docker", "echo 'cgroupfs 2'");
    let out = ok(&w);
    has(&out, "rc=1 max= active= ceiling= ");
    has(&out, "why=[Docker runs containers with the cgroupfs cgroup driver on cgroup v2, tehas needs systemd on v2]");
    good(&w);
    w.stub("docker", "echo 'systemd 1'");
    let out = ok(&w);
    has(&out, "rc=1 ");
    has(&out, "why=[Docker runs containers with the systemd cgroup driver on cgroup v1, tehas needs systemd on v2]");
}

#[test]
fn a_systemctl_that_fails_or_prints_nothing_or_is_silent_is_no_answer() {
    let w = host();
    w.stub(
        "systemctl",
        &format!("echo FragmentPath={FILE}; echo MemoryMax={MAX}; exit 1"),
    );
    assert_eq!(ok(&w), unusable("systemctl gave no answer"));

    good(&w);
    w.stub("systemctl", "");
    has(&ok(&w), "why=[systemctl gave no answer]");

    good(&w);
    w.stub("systemctl", "sleep 20");
    w.env("TEHAS_SLICE_WAIT", "1");
    let started = Instant::now();
    let out = ok(&w);
    has(&out, "rc=1 ");
    has(&out, "why=[systemctl gave no answer]");
    assert!(started.elapsed().as_secs() < 6, "{:?}", started.elapsed());
}

#[test]
fn a_docker_that_fails_or_answers_half_is_no_answer() {
    let w = host();
    w.stub("docker", "echo 'systemd 2'; exit 1");
    assert_eq!(ok(&w), unusable("docker gave no answer"));
    good(&w);
    w.stub("docker", "echo systemd");
    has(&ok(&w), "why=[docker gave no answer]");
}

#[test]
fn controllers_without_cpu_or_no_cgroup_root_are_refused() {
    let w = host();
    let controllers = cgroup(&w).join("cgroup.controllers");
    fs::write(&controllers, "cpuset io memory pids\n").unwrap();
    assert_eq!(
        ok(&w),
        unusable(&format!(
            "{} does not list cpu, memory and pids",
            controllers.display()
        ))
    );
    good(&w);
    let nowhere = w.t().join("nowhere");
    w.set_env(&[&format!("TEHAS_CGROUP_ROOT={}", nowhere.display())]);
    let out = ok(&w);
    has(&out, "rc=1 ");
    has(
        &out,
        &format!(
            "why=[{}/cgroup.controllers does not list cpu, memory and pids]",
            nowhere.display()
        ),
    );
}

#[test]
fn a_reserve_above_a_container_s_share_is_refused_with_both_numbers_and_the_host_it_needs() {
    let w = host();
    systemd(&w, FILE, "active", &(6 * GIB).to_string());
    assert_eq!(
        ok(&w),
        unusable("TEHAS_CYCLE_RESERVE=2g is more than a container may use here (1536m, tehas.slice's MemoryMax over TEHAS_SLICE_CONTAINERS=4); the host needs about 11 GiB of memory for this reserve")
    );
    // With six containers the share is a sixth and the host is larger.
    good(&w);
    w.minimal(&["TEHAS_SLICE_CONTAINERS=6"]);
    systemd(&w, FILE, "active", &(6 * GIB).to_string());
    assert_eq!(
        ok(&w),
        unusable("TEHAS_CYCLE_RESERVE=2g is more than a container may use here (1024m, tehas.slice's MemoryMax over TEHAS_SLICE_CONTAINERS=6); the host needs about 16 GiB of memory for this reserve")
    );
    // A reserve of exactly a quarter is accepted.
    good(&w);
    systemd(&w, FILE, "active", &(8 * GIB).to_string());
    let out = ok(&w);
    has(&out, "rc=0 ");
    has(&out, "ceiling=2048 ");
}

#[test]
fn a_failed_probe_leaves_nothing_from_the_one_before_it() {
    let w = host();
    assert_eq!(
        sh(
            &w,
            r#"slice_ok; rm "$TEHAS_CGROUP_ROOT/cgroup.controllers"; slice_ok; echo "rc=$? max=[$TEHAS_SLICE_MAX] ceiling=[$TEHAS_SLICE_CEILING_MB]""#
        ),
        "rc=1 max=[] ceiling=[]"
    );
}

// --- tehas-cycle sandbox limits ----------------------------------------------

#[test]
fn ok_counts_the_host_s_cpus_and_refuses_a_host_whose_cpus_cannot_be_counted() {
    let w = host();
    assert_eq!(
        sh(&w, r#"slice_ok; echo "rc=$? cpus=$TEHAS_SLICE_CPUS""#),
        "rc=0 cpus=8"
    );
    for answer in ["exit 1", "echo", "echo 0", "echo eight"] {
        good(&w);
        w.stub("nproc", answer);
        assert_eq!(
            sh(&w, &format!("{OK}{}", " cpus=[$TEHAS_SLICE_CPUS]")),
            format!(
                "{} cpus=[]",
                unusable("nproc gave no count of the host's CPUs")
            ),
            "{answer}"
        );
    }
}

#[test]
fn limits_fills_the_fourteen_words_with_a_quarter_of_the_max_in_one_call_each() {
    let w = host();
    assert_eq!(
        sh(&w, LIMITS),
        "--cgroup-parent tehas.slice --memory 5120m --memory-swap 5120m --memory-reservation 2g --cpus 7.00 --pids-limit 2048 --cpu-shares 1024 --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=2560m,mode=1777 rc=0"
    );
    assert_eq!((calls(&w, "systemctl"), calls(&w, "docker")), (1, 1));
    // Its words start with the command.
    systemd(&w, "", "inactive", &MAX.to_string());
    let out = sh(&w, LIMITS);
    has(
        &out,
        "tehas t: no slice file on this host (sudo ./bootstrap.sh slice)",
    );
    has(&out, "rc=78");
}

#[test]
fn limits_without_a_concurrency_says_it_could_not_make_them() {
    let w = host();
    let out = sh(&w, &format!("TEHAS_MAX_CYCLES=; {LIMITS}"));
    has(
        &out,
        "tehas t: the container limits could not be made, not starting",
    );
    has(&out, "rc=78");
}

#[test]
fn limits_carries_the_reserve_as_the_instance_wrote_it() {
    let w = host();
    w.minimal(&["TEHAS_CYCLE_RESERVE=512m"]);
    has(&sh(&w, LIMITS), " --memory-reservation 512m ");
}

#[test]
fn the_gate_runs_at_once_are_read_off_the_words_of_the_limits() {
    let w = host();
    let verify = |words: &str| {
        sh(
            &w,
            &format!(r#""$T_CYCLE" sandbox verify{words}; echo "rc=$?""#),
        )
    };
    assert_eq!(verify(" --cpus 7.00 --memory 5120m"), "1\nrc=0");
    // No limits, no count of gate runs.
    assert_eq!(verify(""), "rc=1");
    assert_eq!(verify(" --cpus x --memory 2048m"), "rc=1");
}

// --- console slice room ------------------------------------------------------

#[test]
fn an_inactive_slice_with_no_directory_promises_a_reserve_per_cycle() {
    let w = host();
    systemd(&w, FILE, "inactive", &MAX.to_string());
    fs::remove_dir_all(slice_dir(&w)).unwrap();
    assert_eq!(room(&w, "0"), line(4, 0, MAX, ""));
    assert_eq!(room(&w, "1"), line(3, RESERVE, MAX, ""));
    assert_eq!(room(&w, "11"), line(0, 22 * GIB, MAX, " room"));
    // Capped as many as their ceilings fit, the same.
    assert_eq!(room(&w, "4"), line(0, 8 * GIB, MAX, " full"));
}

#[test]
fn no_directory_with_the_slice_active_is_unreadable() {
    let w = host();
    fs::remove_dir_all(slice_dir(&w)).unwrap();
    let want = line(
        0,
        0,
        MAX,
        &format!(
            " unreadable tehas.slice is active and {} is missing",
            slice_dir(&w).display()
        ),
    );
    assert_eq!(room(&w, "1"), want);
    assert_eq!(room(&w, "0"), want);
}

#[test]
fn an_empty_slice_has_room_for_four() {
    let w = host();
    assert_eq!(room(&w, "0"), line(4, 0, MAX, ""));
}

#[test]
fn cycles_count_their_floor_or_their_use_whichever_is_more() {
    let w = host();
    let (a, b) = (id('a'), id('b'));
    let two = RESERVE.to_string();
    child(&w, &a, 500 * MIB, 0, &two);
    child(&w, &b, 500 * MIB, 0, &two);
    w.fx("a.cid", &format!("{a}\n"));
    w.fx("b.cid", &b);
    let cids = r#""$T/a.cid" "$T/b.cid""#;
    assert_eq!(room(&w, &format!("2 {cids}")), line(2, 4 * GIB, MAX, ""));
    // A child at 3 GiB counts 3 GiB.
    child(&w, &a, 3 * GIB, 0, &two);
    assert_eq!(room(&w, &format!("2 {cids}")), line(2, 5 * GIB, MAX, ""));
}

#[test]
fn a_container_that_is_no_cycle_counts_too() {
    let w = host();
    child(&w, &id('c'), 3 * GIB, 0, &RESERVE.to_string());
    assert_eq!(room(&w, "0"), line(3, 3 * GIB, MAX, ""));
}

#[test]
fn a_floor_of_max_reads_as_the_reserve_and_no_floor_is_the_use() {
    let w = host();
    child(&w, &id('a'), 100 * MIB, 0, "max");
    child(&w, &id('b'), 100 * MIB, 0, "0");
    assert_eq!(room(&w, "0"), line(2, 2 * GIB + 100 * MIB, MAX, ""));
}

#[test]
fn cid_files_and_the_count_each_add_a_reserve_for_a_cycle_with_no_child() {
    let w = host();
    let (a, b) = (id('a'), id('b'));
    child(&w, &a, 500 * MIB, 0, &RESERVE.to_string());
    w.fx("a.cid", &format!("{a}\n"));
    w.fx("b.cid", &format!("{b}\n"));
    // A cid file with no child adds one reserve.
    assert_eq!(
        room(&w, r#"2 "$T/a.cid" "$T/b.cid""#),
        line(2, 4 * GIB, MAX, "")
    );
    // A cycle counted with no cid file adds one reserve.
    assert_eq!(
        room(&w, r#"3 "$T/a.cid" "$T/missing.cid""#),
        line(1, 6 * GIB, MAX, "")
    );
    // More children than cycles add nothing for the count.
    assert_eq!(room(&w, r#"0 "$T/a.cid""#), line(3, 2 * GIB, MAX, ""));
}

#[test]
fn a_cid_file_that_is_not_64_hex_characters_is_ignored() {
    let w = host();
    let a = id('a');
    child(&w, &a, 500 * MIB, 0, &RESERVE.to_string());
    child(&w, "short", 500 * MIB, 0, &RESERVE.to_string());
    w.fx("a.cid", "short\n");
    w.fx("b.cid", &format!("../docker-{a}\n"));
    assert_eq!(
        room(&w, r#"1 "$T/a.cid" "$T/b.cid""#),
        line(1, 6 * GIB, MAX, "")
    );
}

#[test]
fn reclaimable_slab_and_file_cache_do_not_count_shared_and_kernel_memory_do() {
    let w = host();
    let two = RESERVE.to_string();
    child(&w, &id('a'), 3 * GIB, 6 * GIB, &two);
    assert_eq!(room(&w, "0"), line(3, 3 * GIB, MAX, ""));

    good(&w);
    let file = Extra {
        file: 9 * GIB,
        ..Extra::default()
    };
    child_with(&w, &id('a'), &(3 * GIB).to_string(), 0, &two, file);
    assert_eq!(room(&w, "0"), line(3, 3 * GIB, MAX, ""));

    good(&w);
    let held = Extra {
        shmem: GIB,
        kernel: GIB,
        ..Extra::default()
    };
    child_with(&w, &id('a'), &(3 * GIB).to_string(), GIB, &two, held);
    assert_eq!(room(&w, "0"), line(3, 5 * GIB, MAX, ""));
}

#[test]
fn a_full_slice_has_no_room_and_says_so() {
    let w = host();
    child(&w, &id('a'), 19 * GIB, 0, &RESERVE.to_string());
    assert_eq!(room(&w, "0"), line(0, 19 * GIB, MAX, " room"));
    // More promised than the max is still room 0.
    assert_eq!(room(&w, "4"), line(0, 27 * GIB, MAX, " room"));
}

#[test]
fn no_more_containers_than_their_ceilings_fit_whatever_the_memory_says() {
    let w = host();
    for d in ['a', 'b', 'c'] {
        child(&w, &id(d), GIB, 0, "0");
    }
    assert_eq!(room(&w, "0"), line(1, 3 * GIB, MAX, ""));
    assert_eq!(room(&w, "1"), line(0, 5 * GIB, MAX, " full"));
    for d in ['d', 'e'] {
        child(&w, &id(d), GIB, 0, "0");
    }
    assert_eq!(room(&w, "0"), line(0, 5 * GIB, MAX, " full"));
}

#[test]
fn a_24_gib_slice_also_fits_four() {
    let w = host();
    systemd(&w, FILE, "active", &(24 * GIB).to_string());
    assert_eq!(room(&w, "3"), line(1, 6 * GIB, 24 * GIB, ""));
    assert_eq!(room(&w, "4"), line(0, 8 * GIB, 24 * GIB, " full"));
}

#[test]
fn six_containers_in_a_48_gib_slice_each_take_a_sixth() {
    let w = host();
    w.minimal(&["TEHAS_SLICE_CONTAINERS=6"]);
    systemd(&w, FILE, "active", &(48 * GIB).to_string());
    has(&ok(&w), "ceiling=8192 ");
    assert_eq!(room(&w, "5"), line(1, 10 * GIB, 48 * GIB, ""));
    assert_eq!(room(&w, "6"), line(0, 12 * GIB, 48 * GIB, " full"));
}

#[test]
fn no_room_in_memory_is_said_before_full() {
    let w = host();
    child(&w, &id('a'), 19 * GIB, 0, "0");
    assert_eq!(room(&w, "3"), line(0, 25 * GIB, MAX, " room"));
}

#[test]
fn full_pressure_above_10_holds_when_more_than_half_the_slice_is_promised() {
    let w = host();
    slice_state(&w, "12.00", "0.00");
    assert_eq!(room(&w, "6"), line(0, 12 * GIB, MAX, " pressure"));

    // Exactly 10 does not hold, a hundredth more does.
    let two = RESERVE.to_string();
    slice_state(&w, "10.00", "0.00");
    child(&w, &id('a'), 11 * GIB, 0, &two);
    assert_eq!(room(&w, "0"), line(3, 11 * GIB, MAX, ""));
    slice_state(&w, "10.01", "0.00");
    child(&w, &id('a'), 11 * GIB, 0, &two);
    assert_eq!(room(&w, "0"), line(0, 11 * GIB, MAX, " pressure"));
}

#[test]
fn pressure_does_not_hold_while_half_the_slice_or_less_is_promised() {
    let w = host();
    slice_state(&w, "12.00", "0.00");
    assert_eq!(room(&w, "0"), line(4, 0, MAX, ""));
    child(&w, &id('a'), 10 * GIB, 0, &RESERVE.to_string());
    assert_eq!(room(&w, "0"), line(3, 10 * GIB, MAX, ""));
}

#[test]
fn one_container_thrashing_at_its_ceiling_holds_nothing_near_the_promise_it_holds() {
    let w = host();
    slice_state(&w, "12.00", "0.00");
    child(&w, &id('a'), 5 * GIB, 0, &RESERVE.to_string());
    assert_eq!(room(&w, "0"), line(3, 5 * GIB, MAX, ""));
    assert_eq!(room(&w, "3"), line(0, 11 * GIB, MAX, " pressure"));
    // Some pressure alone does not hold.
    slice_state(&w, "0.08", "55.00");
    assert_eq!(room(&w, "0"), line(4, 0, MAX, ""));
    // The pressure is printed as the kernel wrote it.
    slice_state(&w, "3.41", "55.00");
    assert_eq!(sh(&w, r#"slice_verb pressure; echo "rc=$?""#), "3.41\nrc=0");
}

#[test]
fn memory_pressure_unreadable_or_with_no_full_line_is_unreadable() {
    let w = host();
    child(&w, &id('a'), 500 * MIB, 0, &RESERVE.to_string());
    let pressure = slice_dir(&w).join("memory.pressure");
    fs::remove_file(&pressure).unwrap();
    let why = format!("{} cannot be read", pressure.display());
    assert_eq!(
        room(&w, "0"),
        line(0, RESERVE, MAX, &format!(" unreadable {why}"))
    );
    assert_eq!(
        sh(&w, r#"slice_verb pressure; echo "rc=$?""#),
        format!("{why}\nrc=1")
    );

    good(&w);
    fs::write(
        &pressure,
        "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n",
    )
    .unwrap();
    assert_eq!(
        room(&w, "0"),
        line(
            0,
            0,
            MAX,
            &format!(" unreadable {} has no full avg60", pressure.display())
        )
    );
}

fn stat(w: &World, id: &str, lines: &[String]) {
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(scope(w, id).join("memory.stat"), text).unwrap();
}

#[test]
fn a_memory_stat_with_no_number_for_one_of_the_four_keys_is_unreadable() {
    let w = host();
    let (a, b) = (id('a'), id('b'));
    let two = RESERVE.to_string();
    child(&w, &a, 500 * MIB, 0, &two);
    child_with(&w, &b, "lots", 0, &two, Extra::default());
    let named = |id: &str, key: &str| {
        line(
            0,
            0,
            MAX,
            &format!(
                " unreadable {}/memory.stat has no number for {key}",
                scope(&w, id).display()
            ),
        )
    };
    assert_eq!(room(&w, "0"), named(&b, "anon"));

    good(&w);
    child(&w, &a, 500 * MIB, 0, &two);
    stat(
        &w,
        &a,
        &[
            "anon 1".into(),
            "shmem 0".into(),
            "kernel 0".into(),
            "slab_reclaimable lots".into(),
        ],
    );
    assert_eq!(room(&w, "0"), named(&a, "slab_reclaimable"));

    // A memory.stat with no anon line still holds and names the key.
    good(&w);
    child(&w, &a, GIB, 0, &two);
    stat(
        &w,
        &a,
        &[
            format!("shmem {GIB}"),
            "kernel 0".into(),
            "slab_reclaimable 0".into(),
        ],
    );
    assert_eq!(room(&w, "0"), named(&a, "anon"));
}

#[test]
fn a_memory_stat_with_no_kernel_line_counts_anon_and_shmem_not_slab() {
    let w = host();
    let a = id('a');
    child(&w, &a, 3 * GIB, GIB, "0");
    stat(
        &w,
        &a,
        &[
            format!("anon {}", 3 * GIB),
            format!("shmem {GIB}"),
            format!("slab_reclaimable {GIB}"),
        ],
    );
    assert_eq!(room(&w, "0"), line(3, 4 * GIB, MAX, ""));
}

/// A starting container has its scope before the kernel fills in the file:
/// no slab_reclaimable line reads as a slab of 0, so the charge errs high.
#[test]
fn a_memory_stat_with_no_slab_reclaimable_line_counts_its_kernel_whole() {
    let w = host();
    let a = id('a');
    child(&w, &a, GIB, 0, "0");
    stat(
        &w,
        &a,
        &[
            format!("anon {}", 500 * MIB),
            format!("shmem {GIB}"),
            format!("kernel {GIB}"),
        ],
    );
    assert_eq!(room(&w, "0"), line(3, 2671771648, MAX, ""));
}

#[test]
fn a_memory_low_that_is_missing_or_no_number_is_unreadable_with_no_fallback() {
    let w = host();
    let a = id('a');
    child(&w, &a, 500 * MIB, 0, &RESERVE.to_string());
    let low = scope(&w, &a).join("memory.low");
    fs::remove_file(&low).unwrap();
    fs::write(scope(&w, &a).join("memory.current"), "1\n").unwrap();
    assert_eq!(
        room(&w, "0"),
        line(
            0,
            0,
            MAX,
            &format!(" unreadable {} cannot be read", low.display())
        )
    );
    good(&w);
    child(&w, &a, 500 * MIB, 0, "lots");
    assert_eq!(
        room(&w, "0"),
        line(
            0,
            0,
            MAX,
            &format!(" unreadable {} is not a number", low.display())
        )
    );
}

#[test]
fn a_count_that_is_no_number_or_no_probe_is_unreadable() {
    let w = host();
    assert_eq!(
        room(&w, "many"),
        line(0, 0, MAX, " unreadable the count of cycles is not a number")
    );
    assert_eq!(
        sh(&w, "slice_verb room 0"),
        line(0, 0, 0, " unreadable the slice was not probed")
    );
}

/// Every path under `dir`, sorted.
fn tree(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![dir.to_path_buf()];
    if dir.is_dir() && !dir.is_symlink() {
        for e in fs::read_dir(dir).unwrap().flatten() {
            out.extend(tree(&e.path()));
        }
    }
    out.sort();
    out
}

#[test]
fn room_makes_no_call_and_writes_nothing() {
    let w = host();
    child(&w, &id('a'), 500 * MIB, 0, &RESERVE.to_string());
    let home = w.t().join("home");
    let before = (tree(&cgroup(&w)), tree(&home));
    room(&w, "1");
    assert_eq!((tree(&cgroup(&w)), tree(&home)), before);
    assert_eq!((calls(&w, "systemctl"), calls(&w, "docker")), (1, 1));
}

#[test]
fn the_room_line_is_read_whole_by_the_line_the_dispatcher_uses() {
    let w = host();
    let pressure = slice_dir(&w).join("memory.pressure");
    fs::remove_file(&pressure).unwrap();
    assert_eq!(
        sh(
            &w,
            r#"slice_ok; read -r room committed max reserve why note < <(slice_verb room 0); echo "[$room] [$why] [$note]""#
        ),
        format!("[0] [unreadable] [{} cannot be read]", pressure.display())
    );

    good(&w);
    child(&w, &id('a'), 500 * MIB, 0, &RESERVE.to_string());
    assert_eq!(
        sh(
            &w,
            r#"slice_ok; read -r room committed max reserve why < <(slice_verb room 2); echo "[$room] [$committed] [$max] [$reserve] [$why]""#
        ),
        format!("[1] [{}] [{MAX}] [{RESERVE}] []", 6 * GIB)
    );
}

#[test]
fn a_reserve_that_holds_a_quote_comes_back_whole_and_runs_nothing() {
    let w = host();
    assert_eq!(
        sh(
            &w,
            r#"TEHAS_CYCLE_RESERVE="2g'; echo ran; '"; slice_ok; echo "rc=$? [$TEHAS_SLICE_WHY]""#
        ),
        "rc=1 [TEHAS_CYCLE_RESERVE=2g'; echo ran; ' is not a size]"
    );
}

/// The reserve field of a word that is no size is that word, never 0 and never
/// empty, so why and the note keep their places under read -r.
#[test]
fn a_reserve_that_is_no_size_is_named_in_its_own_field_and_keeps_the_places() {
    let w = host();
    assert_eq!(
        sh(&w, "slice_ok; TEHAS_CYCLE_RESERVE=2gg; slice_verb room 0"),
        format!("0 0 {MAX} 2gg unreadable TEHAS_CYCLE_RESERVE=2gg is not a size")
    );
    assert_eq!(
        sh(
            &w,
            r#"slice_ok; TEHAS_CYCLE_RESERVE=2gg; read -r room committed max reserve why note < <(slice_verb room 0); echo "[$room] [$reserve] [$why] [$note]""#
        ),
        "[0] [2gg] [unreadable] [TEHAS_CYCLE_RESERVE=2gg is not a size]"
    );
    assert_eq!(
        sh(
            &w,
            r#"slice_ok; TEHAS_CYCLE_RESERVE="2 g; echo ran"; read -r room committed max reserve why note < <(slice_verb room 0); echo "[$reserve] [$why] [$note]""#
        ),
        "[2_g;_echo_ran] [unreadable] [TEHAS_CYCLE_RESERVE=2 g; echo ran is not a size]"
    );
}
