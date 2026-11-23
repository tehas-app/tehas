//! `tehas disk` as a host runs it: the settings of the instance name the file
//! systems the answer holds, and settings that are malformed stop the run
//! before the host is asked anything. The reading itself is tested in
//! console/ops/src/disk.rs. The answer is held to api/disk-cycles.schema.json.
//! Moved from test/disk_test.sh.

mod common;

use common::*;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// df answers from `$T/fx/df`, or nothing and DF_RC.
const DF: &str = r#"[ -e "$T/fx/df" ] || exit "${DF_RC:-0}"
cat "$T/fx/df""#;

/// Every question a case asks of the daemon, answered from a file under
/// `$T/fx` named for the call. No file is no answer. The images' directory is
/// one no host has, so it is a file system of its own on every host: a real
/// /var/lib/docker may share the home's. The exact listing of the
/// cycles comes before `ps `, which the stopped containers are asked with.
const DOCKER: &str = r#"case "$*" in
  *DockerRootDir*) echo /nonexistent/docker ;;
  "system df"*) printf "TYPE\tTOTAL\tACTIVE\tSIZE\tRECLAIMABLE\nImages\t2\t1\t1.2GB\t0B (0%%)\n" ;;
  "ps --format {{.Names}}") [ -e "$T/fx/ps" ] || exit "${PS_RC:-0}"; cat "$T/fx/ps" ;;
  "ps "*) [ -e "$T/fx/containers" ] || exit "${PS_RC:-0}"; cat "$T/fx/containers" ;;
  inspect\ --size*) [ -e "$T/fx/inspect" ] || exit "${INSPECT_RC:-0}"; cat "$T/fx/inspect" ;;
  stats\ --no-stream*) [ -e "$T/fx/stats" ] || exit "${STATS_RC:-0}"; cat "$T/fx/stats" ;;
esac"#;

/// Two stopped containers: one with a command, one whose command the listing
/// left empty.
const STOPPED: &str = "ab12\ttehas-sandbox:1\t/bin/sh -c sleep\ncd34\tubuntu:24.04\n";

/// A host with the minimal instance, a home with nothing in it but the app's
/// tokens, and df and docker stubbed.
fn host() -> World {
    let w = World::new("disk");
    w.stub("df", DF);
    w.stub("docker", DOCKER);
    fs::remove_dir_all(&w.home).unwrap();
    w.minimal(&[]);
    w
}

/// The 40 GiB free of a host with room on it.
fn df(w: &World) {
    w.fx(
        "fx/df",
        "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/x 60000000 1 41943040 1% /\n",
    );
}

/// `tehas disk <args>`, after the shell words in `env`, from a tree laid out
/// as a host's: bin/tehas copied beside libexec/console, and no TEHAS_CONSOLE,
/// so the command finds the console as an installed one does.
fn disk(w: &World, env: &str, args: &str) -> Run {
    let tehas = w.t().join("tree/bin/tehas");
    if !tehas.exists() {
        fs::copy(root().join("bin/tehas"), &tehas).unwrap();
    }
    w.sh(&format!(
        "unset TEHAS_CONSOLE; {env} {} disk {args}",
        tehas.display()
    ))
}

/// `tehas disk --json <args>`, after the shell words in `env`.
fn disk_json(w: &World, env: &str, args: &str) -> Run {
    disk(w, env, &format!("{args} --json"))
}

fn zeros(path: &Path, kib: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; kib * 1024]).unwrap();
}

/// Every path the entries of a JSON answer name, sorted.
fn paths(answer: &str) -> Vec<String> {
    let v: Value = serde_json::from_str(answer).unwrap();
    let mut got: Vec<String> = v["filesystems"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["paths"].as_array().unwrap().iter())
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    got.sort();
    got
}

fn sorted(want: &[&str]) -> Vec<String> {
    let mut want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
    want.sort();
    want
}

fn docker_calls(w: &World, prefix: &str) -> usize {
    count_lines(&w.calls("docker"), prefix)
}

#[track_caller]
fn fits(value: &str) {
    assert_eq!(schema_errors("disk-cycles", value), "", "{value}");
}

#[test]
fn it_prints_each_file_system_once_and_asks_df_once_each() {
    let w = host();
    df(&w);
    let r = disk(&w, "", "");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, &format!("disk      {}:", w.home.display()));
    has(&r.out, "40 GiB of");
    has(&r.out, "disk      /nonexistent/docker:");
    let calls = w.calls("df");
    let mut seen = BTreeSet::new();
    for line in calls.lines() {
        assert!(seen.insert(line), "df asked twice: {line}\n{calls}");
    }
    assert_eq!(
        w.calls("docker").lines().next(),
        Some("info --format {{.DockerRootDir}}")
    );
}

#[test]
fn the_json_names_the_home_docker_tmp_and_the_scratch_disk_of_the_settings() {
    let w = host();
    df(&w);
    let scratch = w.t().join("scratch");
    fs::create_dir_all(&scratch).unwrap();
    w.minimal(&[&format!("TEHAS_SCRATCH_DIR={}", scratch.display())]);
    let r = disk_json(&w, "", "");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out.lines().count(), 1, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(v["api"], 1);
    let home = w.home.to_string_lossy();
    let scratch = scratch.to_string_lossy();
    assert_eq!(
        paths(&r.out),
        sorted(&[&home, "/nonexistent/docker", "/tmp", &scratch])
    );

    // With no scratch disk in the settings the answer has no entry for one.
    w.minimal(&[]);
    let r = disk_json(&w, "", "");
    assert_eq!(
        paths(&r.out),
        sorted(&[&home, "/nonexistent/docker", "/tmp"])
    );
}

/// A figure that cannot be read is no figure and a reason, never 0. How many
/// entries there are is the mount table's, so every expectation is read off
/// the answer.
#[test]
fn a_df_that_answers_nothing_leaves_each_entry_with_no_figure_and_a_reason() {
    let w = host();
    let scratch = w.t().join("scratch");
    fs::create_dir_all(&scratch).unwrap();
    w.minimal(&[&format!("TEHAS_SCRATCH_DIR={}", scratch.display())]);
    let r = disk_json(&w, "DF_RC=1", "");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let entries = v["filesystems"].as_array().unwrap();
    assert!(!entries.is_empty());
    let got = paths(&r.out);
    for p in [
        &*w.home.to_string_lossy(),
        "/nonexistent/docker",
        "/tmp",
        &scratch.to_string_lossy(),
    ] {
        assert!(got.iter().any(|g| g == p), "{p} in {got:?}");
    }
    for e in entries {
        assert!(e["freeBytes"].is_null(), "{e}");
        assert!(e["why"].is_string(), "{e}");
    }
}

#[test]
fn full_names_the_places_of_the_home_biggest_first_and_the_docker_figure() {
    let w = host();
    df(&w);
    zeros(&w.home.join("logs/big.log"), 200);
    zeros(&w.home.join("dev/alpha.state/target/out"), 50);
    let r = disk(&w, "", "--full --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let places: Vec<&str> = v["full"]["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["path"].as_str().unwrap())
        .collect();
    let home = w.home.to_string_lossy().into_owned();
    let logs = format!("{home}/logs");
    assert!(places.contains(&logs.as_str()), "{places:?}");
    assert!(
        places.contains(&format!("{home}/dev/alpha.state/target").as_str()),
        "{places:?}"
    );
    // The machine's own agent directory is not ours to rank.
    assert_eq!(
        places.iter().find(|p| p.starts_with(&home)),
        Some(&logs.as_str())
    );
    assert_eq!(v["full"]["docker"]["state"], "read");
}

#[test]
fn full_without_publish_names_the_stopped_containers_and_writes_nothing() {
    let w = host();
    df(&w);
    zeros(&w.home.join("logs/big.log"), 200);
    w.fx("fx/containers", STOPPED);
    let r = disk(&w, "", "--full --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let full = &v["full"];
    assert_eq!(full["stoppedContainers"], 2);
    let ids: Vec<&str> = full["containers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["ab12", "cd34"]);
    assert_eq!(full["containers"][0]["image"], "tehas-sandbox:1");
    assert_eq!(full["containers"][0]["command"], "/bin/sh -c sleep");
    assert_eq!(full["containers"][1].get("command"), None);
    assert!(!w.live_file("disk.json").exists());
    assert!(!w.live.exists());
    assert_eq!(docker_calls(&w, "ps --filter status=exited"), 1);
}

#[test]
fn publish_writes_live_disk_json_whole_and_names_the_containers_left_stopped() {
    let w = host();
    df(&w);
    zeros(&w.home.join("logs/big.log"), 200);
    let r = disk(&w, "", "--publish --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let text = fs::read_to_string(w.live_file("disk.json")).unwrap();
    assert_eq!(text.trim_end().lines().count(), 1, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["api"], 1);
    assert_eq!(v["full"]["stoppedContainers"], 0);
    assert_eq!(docker_calls(&w, "ps --filter status=exited"), 1);
    // No side file of the run is left.
    assert_eq!(fs::read_dir(&w.live).unwrap().count(), 1);
}

#[test]
fn an_unknown_argument_is_64_and_writes_nothing() {
    let w = host();
    let r = disk(&w, "", "--everything");
    assert_eq!(r.rc, 64, "{}", r.out);
    assert!(!w.live_file("disk.json").exists());
}

#[test]
fn cycles_names_each_running_cycle_with_both_figures_and_fits_the_schema() {
    let w = host();
    df(&w);
    w.fx(
        "fx/ps",
        "tehas-work-1872\ntehas-work-51x\ntehas-dev-a\ntehas-work-5\n",
    );
    // SizeRw of `inspect --size` is bytes in the order named; BlockIO of
    // `stats` is a decimal size, written over read.
    w.fx("fx/inspect", "1234567890\n2000000000\n");
    w.fx(
        "fx/stats",
        "tehas-work-5\t1.2GB / 0B\ntehas-work-1872\t0B / 512kB\ntehas-dev-a\t9MB / 9MB\n",
    );
    let r = disk(&w, "", "--cycles --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(r.out.lines().count(), 1, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(v["cycles"]["state"], "read");
    let list = v["cycles"]["list"].as_array().unwrap();
    let col = |k: &str| list.iter().map(|c| c[k].clone()).collect::<Vec<_>>();
    assert_eq!(col("issue"), [5, 1872]);
    assert_eq!(col("container"), ["tehas-work-5", "tehas-work-1872"]);
    assert_eq!(col("writableBytes"), [1234567890u64, 2000000000]);
    assert_eq!(col("blockWrittenBytes"), [1200000000u64, 0]);
    assert!(list.iter().all(|c| c.get("why").is_none()), "{list:?}");
    fits(&r.out);
    // Three calls however many cycles run, each once for the whole host.
    assert_eq!(docker_calls(&w, "ps --format"), 1);
    assert_eq!(docker_calls(&w, "inspect --size"), 1);
    assert_eq!(docker_calls(&w, "stats --no-stream"), 1);
}

/// 0 would read as a cycle that wrote nothing, which is another fact.
#[test]
fn cycles_keeps_a_figure_the_daemon_would_not_answer_as_null_with_why() {
    let w = host();
    df(&w);
    w.fx("fx/ps", "tehas-work-7\n");
    w.fx("fx/inspect", "-\n");
    w.fx("fx/stats", "tehas-dev-a\t9MB / 9MB\n");
    let r = disk(&w, "", "--cycles --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let first = &v["cycles"]["list"][0];
    assert!(first["writableBytes"].is_null(), "{first}");
    assert!(first["blockWrittenBytes"].is_null(), "{first}");
    assert_eq!(
        first["why"],
        "writableBytes and blockWrittenBytes: docker inspect --size gave no size / docker stats answered nothing for it"
    );
    fits(&r.out);
}

#[test]
fn cycles_with_no_cycle_running_is_an_empty_list_and_one_question() {
    let w = host();
    df(&w);
    let r = disk(&w, "", "--cycles --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(v["cycles"]["state"], "read");
    assert_eq!(v["cycles"]["list"], serde_json::json!([]));
    assert_eq!(docker_calls(&w, "inspect --size"), 0);
    assert_eq!(docker_calls(&w, "stats --no-stream"), 0);
    let r = disk(&w, "", "--cycles");
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "cycles  no cycle is running");
    assert_eq!(count_lines(&r.out, "cycles"), 1, "{}", r.out);
}

/// With no names there is nothing to ask about.
#[test]
fn cycles_with_a_listing_that_fails_is_unreadable_and_still_prints() {
    let w = host();
    df(&w);
    let r = disk(&w, "PS_RC=1", "--cycles --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(v["cycles"]["state"], "unreadable");
    assert_eq!(v["cycles"].get("list"), None);
    assert!(v["cycles"]["why"].is_string());
    assert_eq!(docker_calls(&w, "inspect --size"), 0);
    assert_eq!(docker_calls(&w, "stats --no-stream"), 0);
    let r = disk(&w, "PS_RC=1", "--cycles");
    has(&r.out, "cycles  no reading: docker answered nothing");
}

fn one_cycle(w: &World) {
    w.fx("fx/ps", "tehas-work-1872\n");
    w.fx("fx/inspect", "1000000000\n");
    w.fx("fx/stats", "tehas-work-1872\t1.2GB / 0B\n");
}

#[test]
fn without_cycles_there_is_no_cycles_key_and_the_daemon_is_asked_nothing_new() {
    let w = host();
    df(&w);
    one_cycle(&w);
    let r = disk_json(&w, "", "");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    assert_eq!(v.get("cycles"), None);
    assert!(paths(&r.out).contains(&w.home.to_string_lossy().into_owned()));
    assert_eq!(docker_calls(&w, "ps --format"), 0);
    assert_eq!(docker_calls(&w, "inspect --size"), 0);
    assert_eq!(docker_calls(&w, "stats --no-stream"), 0);
    lacks(&disk(&w, "", "").out, "cycles");
}

/// The cycles are a person's question, not the cleaner's.
#[test]
fn publish_writes_the_file_it_always_wrote_with_no_cycles_in_it() {
    let w = host();
    df(&w);
    zeros(&w.home.join("logs/big.log"), 200);
    one_cycle(&w);
    let r = disk(&w, "", "--publish --cycles --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let file: Value =
        serde_json::from_str(&fs::read_to_string(w.live_file("disk.json")).unwrap()).unwrap();
    assert_eq!(file["api"], 1);
    assert_eq!(file.get("cycles"), None);
    assert!(json_of(&r).get("cycles").is_some());
    assert_eq!(docker_calls(&w, "inspect --size"), 1);
}

fn api(name: &str) -> String {
    fs::read_to_string(root().join("api").join(name)).unwrap()
}

#[test]
fn the_sample_fits_the_schema_and_the_schema_requires_the_four_keys() {
    fits(&api("disk-cycles.sample.json"));
    let schema: Value = serde_json::from_str(&api("disk-cycles.schema.json")).unwrap();
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["api", "cycles", "filesystems", "time"]);
}

/// The `full` of `--cycles --full` is the one the plain answer prints.
#[test]
fn cycles_full_and_its_sample_fit_the_schema_whose_full_is_that_of_tehas_disk() {
    let w = host();
    df(&w);
    zeros(&w.home.join("logs/big.log"), 20);
    w.fx("fx/containers", STOPPED);
    let r = disk(&w, "", "--cycles --full --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let places = v["full"]["places"].as_array().unwrap();
    assert!(places.iter().all(|p| p.get("cut").is_none()), "{places:?}");
    assert_eq!(v["full"]["containers"][1].get("command"), None);
    fits(&r.out);
    fits(&api("disk-cycles.full.json"));
    let a: Value = serde_json::from_str(&api("disk-cycles.schema.json")).unwrap();
    let b: Value = serde_json::from_str(&api("disk.schema.json")).unwrap();
    assert_eq!(a["properties"]["full"], b["properties"]["full"]);
}

/// api/ has no $ref, so the schema restates `filesystems` and `full`: a key
/// the code prints that the copy does not name is refused here. The reading
/// prints all it can: the places, the stopped containers, and a df that
/// answers nothing so an entry carries its `why`.
#[test]
fn the_shared_part_of_the_schema_names_every_key_the_disk_answer_can_print() {
    let w = host();
    zeros(&w.home.join("logs/big.log"), 20);
    w.fx("fx/containers", STOPPED);
    let r = disk(&w, "DF_RC=1", "--full --cycles --publish --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let printed = [
        json_of(&r),
        serde_json::from_str(&fs::read_to_string(w.live_file("disk.json")).unwrap()).unwrap(),
    ];
    let schema: Value = serde_json::from_str(&api("disk-cycles.schema.json")).unwrap();
    for key in ["filesystems", "full"] {
        let mut got = BTreeSet::new();
        for v in &printed {
            let objects = match &v[key] {
                Value::Array(items) => items.clone(),
                other => vec![other.clone()],
            };
            for o in objects {
                got.extend(o.as_object().unwrap().keys().cloned());
            }
        }
        let p = &schema["properties"][key];
        let declared = p["items"]["properties"]
            .as_object()
            .or_else(|| p["properties"].as_object())
            .unwrap();
        let extra: Vec<_> = got.iter().filter(|k| !declared.contains_key(*k)).collect();
        assert!(
            extra.is_empty(),
            "{key}: the answer prints {extra:?} that the schema does not know"
        );
    }
}

/// The widest issue the platform publishes is nine digits: a container named
/// past that is something else sharing the daemon.
#[test]
fn cycles_passes_over_a_container_named_past_the_widest_issue() {
    let w = host();
    df(&w);
    w.fx("fx/ps", "tehas-work-1000000000\ntehas-work-1872\n");
    w.fx("fx/inspect", "1000000000\n2000000000\n");
    w.fx(
        "fx/stats",
        "tehas-work-1872\t1.2GB / 0B\ntehas-work-1000000000\t1MB / 1MB\n",
    );
    let r = disk(&w, "", "--cycles --json");
    assert_eq!(r.rc, 0, "{}", r.out);
    let v = json_of(&r);
    let names: Vec<&str> = v["cycles"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["container"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["tehas-work-1872"]);
    fits(&r.out);
}

/// Every real instance.env starts with the comments of the manual.
#[test]
fn an_instance_env_that_starts_with_a_comment_still_names_the_scratch_disk() {
    let w = host();
    df(&w);
    let scratch = w.t().join("scratch");
    fs::create_dir_all(&scratch).unwrap();
    w.instance(&[
        "# Tehas instance settings. One host, one product repository.",
        "TEHAS_REPO=acme/widget",
        "# the disk a cycle writes its temporary files to",
        "",
        &format!("TEHAS_SCRATCH_DIR={}", scratch.display()),
    ]);
    let r = disk_json(&w, "", "");
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        paths(&r.out),
        sorted(&[
            &w.home.to_string_lossy(),
            "/nonexistent/docker",
            "/tmp",
            &scratch.to_string_lossy()
        ])
    );
}

#[track_caller]
fn refused(w: &World, env: &str, says: &str) {
    let r = disk(w, env, "");
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(w.calls("df"), "");
    has(&r.out, says);
}

#[test]
fn a_malformed_scratch_disk_in_a_commented_file_is_78_and_asks_the_host_nothing() {
    let w = host();
    w.instance(&[
        "# Tehas instance settings. One host, one product repository.",
        "TEHAS_REPO=acme/widget",
        "# not an absolute path",
        "TEHAS_SCRATCH_DIR=scratch",
    ]);
    refused(&w, "", "TEHAS_SCRATCH_DIR is not an absolute path");
}

#[test]
fn malformed_settings_are_78_and_ask_the_host_nothing() {
    let w = host();
    w.minimal(&["TEHAS_SCRATCH_DIR=scratch"]);
    refused(&w, "", "TEHAS_SCRATCH_DIR is not an absolute path");
}

#[test]
fn a_scratch_disk_the_shell_would_expand_is_78_and_says_the_file_is_read() {
    let w = host();
    w.minimal(&["TEHAS_SCRATCH_DIR=$HOME/scratch"]);
    refused(
        &w,
        "",
        "TEHAS_SCRATCH_DIR holds a $ or a backtick: the file is read, not run as shell",
    );
}

#[test]
fn no_instance_to_read_the_home_from_is_78() {
    let w = host();
    w.no_instance();
    refused(
        &w,
        &format!("TEHAS_INSTANCE_DIR={}", w.t().join("no-instance").display()),
        "could not be read",
    );
}
