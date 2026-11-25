//! `tehas state`: what a stopped factory's state moves as, and when pack and
//! unpack refuse; check, copy and load of the state store. The tar is real,
//! the store is this build's, systemctl lists the units in `$T/units`, and gh
//! is no stub, so a call to it fails the case.
//!
//! The factory of `World` is the source a pack reads. An unpack writes a
//! second home, `$T/dst/tehas`, with a store of its own. The answers of copy
//! and load are held to api/state-copy.schema.json and
//! api/state-load.schema.json, with the samples beside them.

mod common;

use common::*;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use tehas_core::home::{Home, State, TokenDir};

/// The schema this build's store writes.
const SCHEMA: u64 = 33;

/// A host as the shell's reset() leaves it: both homes made at v0.17.0, a
/// systemctl that lists `$T/units`, and git a stub that does nothing.
fn host(name: &str) -> World {
    let w = World::new(&format!("state-{name}"));
    fs::remove_file(w.t().join("bin/gh")).unwrap();
    w.stub("git", ":");
    w.stub("systemctl", r#"[ ! -f "$T/units" ] || cat "$T/units""#);
    make_home(&w, &w.home, "v0.17.0");
    fs::create_dir_all(dst(&w)).unwrap();
    store_home(&w, &dst(&w));
    make_home(&w, &dst(&w), "v0.17.0");
    w
}

/// The token directories a pack leaves out.
const TOKENS: [TokenDir; 3] = [TokenDir::App, TokenDir::Cycle, TokenDir::Session];

/// `state/` of a home.
fn st(home: impl AsRef<Path>) -> PathBuf {
    Home::new(home.as_ref()).state_dir()
}

/// `live/` of a home.
fn lv(home: impl AsRef<Path>) -> PathBuf {
    Home::new(home.as_ref()).live_dir()
}

/// A name in an archive: `dir` of the empty home, and `rest` under it.
fn rel(dir: PathBuf, rest: &str) -> String {
    dir.join(rest).display().to_string()
}

fn src(w: &World) -> PathBuf {
    w.home.clone()
}

fn dst(w: &World) -> PathBuf {
    w.t().join("dst/tehas")
}

fn tar_file(w: &World) -> PathBuf {
    w.t().join("state.tar")
}

fn bad_tar(w: &World) -> PathBuf {
    w.t().join("bad.tar")
}

fn x(w: &World) -> PathBuf {
    w.t().join("x")
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

/// The real store in `home`, a fresh database.
fn store_home(w: &World, home: &Path) {
    let libexec = home.join("libexec");
    fs::create_dir_all(&libexec).unwrap();
    fs::copy(w.t().join("store-bin"), libexec.join("store")).unwrap();
    fs::create_dir_all(st(home)).unwrap();
    must(
        w,
        home,
        None,
        &[
            "init",
            "--version",
            "v0.0.0",
            "--commit",
            COMMIT,
            "--instance",
            "instance",
            "--repo",
            "acme/widget",
        ],
    );
}

/// A file in each directory that moves and in each that does not, and the
/// release the home runs.
fn make_home(w: &World, home: &Path, tag: &str) {
    for (rel, text) in [
        ("docs/version.txt", format!("{tag}\n")),
        ("repos/product/main.c", "code\n".into()),
        ("mcp/sessions", "key\n".into()),
        ("logs/work.log", "log\n".into()),
        ("work/w/f", "tree\n".into()),
    ] {
        put(home.join(rel), &text);
    }
    put(st(home).join("ledger.txt"), "ledger\n");
    put(lv(home).join("a.json"), "{}\n");
    fs::create_dir_all(Home::new(home).cycles_dir()).unwrap();
    put(w.t().join("home/.config/tehas/env"), "secret\n");
}

fn put(p: impl AsRef<Path>, text: &str) {
    let p = p.as_ref();
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn read(p: impl AsRef<Path>) -> String {
    fs::read_to_string(p).unwrap_or_default()
}

/// A store verb in `home`, under the lock of issue `lock` when one is named.
fn store_in(w: &World, home: &Path, lock: Option<u64>, args: &[&str]) -> Run {
    let store = w.t().join("store-bin");
    let mut c = match lock {
        Some(n) => {
            let cycles = Home::new(home).cycles_dir();
            fs::create_dir_all(&cycles).unwrap();
            let mut c = Command::new("flock");
            c.args(["-w", "5"])
                .arg(cycles.join(format!("{n}.lock")))
                .arg(&store);
            c
        }
        None => Command::new(&store),
    };
    let out = c
        .env("TEHAS_HOME", home)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Run {
        rc: out.status.code().unwrap_or(-1),
        out: text.trim_end().to_string(),
    }
}

#[track_caller]
fn must(w: &World, home: &Path, lock: Option<u64>, args: &[&str]) {
    let r = store_in(w, home, lock, args);
    assert_eq!(r.rc, 0, "the store refused {args:?}: {}", r.out);
}

/// t_ended <home> <n> -: a cycle started by hand and ended as failed.
fn ended(w: &World, home: &Path, n: u64) {
    let issue = n.to_string();
    must(w, home, Some(n), &["cycle-start", &issue, "--by-hand"]);
    let shown = store_in(w, home, None, &["cycle-show", &issue]);
    let v: Value = serde_json::from_str(&shown.out).unwrap();
    let attempt = v["cycle"]["attempt"].as_u64().unwrap().to_string();
    must(
        w,
        home,
        Some(n),
        &[
            "cycle-end",
            &issue,
            "--attempt",
            &attempt,
            "--status",
            "failed",
        ],
    );
}

/// What `store check` answers for `home`: its line as printed, and the
/// fields the cases read.
struct Check {
    raw: String,
    integrity: String,
    schema: u64,
    cycles: u64,
}

fn check_of(w: &World, home: &Path) -> Check {
    let raw = store_in(w, home, None, &["check"]).out;
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    Check {
        integrity: v["integrity"].as_str().unwrap_or_default().to_string(),
        schema: v["schema"].as_u64().unwrap_or(0),
        cycles: v["rows"]["cycles"].as_u64().unwrap_or(0),
        raw,
    }
}

/// The lock at `p`, held by this process until the value drops (t_hold).
fn hold(p: &Path) -> File {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(p)
        .unwrap();
    f.lock().unwrap();
    f
}

fn units(w: &World, line: &str) {
    put(w.t().join("units"), &format!("{line}\n"));
}

/// `tehas state <words>` in the dst home.
fn state(w: &World, words: &str) -> Run {
    w.sh(&format!(
        r#"TEHAS_HOME='{}' "$T_ROOT/bin/tehas" state {words}"#,
        s(&dst(w))
    ))
}

/// `tehas state <words>` in the source home.
fn state_src(w: &World, words: &str) -> Run {
    w.sh(&format!(r#""$T_ROOT/bin/tehas" state {words}"#))
}

/// tehas state pack from the source into $T/state.tar.
fn pack(w: &World, args: &str) -> Run {
    state_src(w, &format!("pack {args} - >'{}'", s(&tar_file(w))))
}

fn unpack(w: &World, tar: &Path) -> Run {
    state(w, &format!("unpack - <'{}'", s(tar)))
}

fn tar_bytes(w: &World) -> u64 {
    fs::metadata(tar_file(w)).map(|m| m.len()).unwrap_or(0)
}

/// A bash snippet's output, which must succeed.
#[track_caller]
fn sh_ok(w: &World, snippet: &str) -> String {
    let r = w.sh(snippet);
    assert_eq!(r.rc, 0, "{snippet}:\n{}", r.out);
    r.out
}

/// The names in a tar, sorted, without a trailing slash.
fn names(w: &World, tar: &Path) -> String {
    sh_ok(
        w,
        &format!("tar -tf '{}' | sed 's|/$||' | LC_ALL=C sort", s(tar)),
    )
}

fn first_name(w: &World) -> String {
    sh_ok(w, &format!("tar -tf '{}' | head -n 1", s(&tar_file(w))))
}

/// The names and bytes of a tree, but for the files SQLite makes and removes
/// as it is read.
fn fingerprint(w: &World, dir: &Path) -> String {
    sh_ok(
        w,
        &format!(
            "cd '{}' && find . ! -name '*-wal' ! -name '*-shm' | LC_ALL=C sort && find . -type f ! -name '*-wal' ! -name '*-shm' -print0 | LC_ALL=C sort -z | xargs -0 sha256sum",
            s(dir)
        ),
    )
}

/// The target factory and what sits beside it.
fn whole(w: &World) -> String {
    fingerprint(w, &w.t().join("dst"))
}

/// $T/state.tar unpacked into $T/x.
fn extract(w: &World) {
    let _ = fs::remove_dir_all(x(w));
    fs::create_dir_all(x(w)).unwrap();
    sh_ok(
        w,
        &format!("tar -xf '{}' -C '{}'", s(&tar_file(w)), s(&x(w))),
    );
}

/// $T/state.tar with its manifest replaced, the others kept, as $T/bad.tar.
fn retar(w: &World, manifest: &str) {
    extract(w);
    put(x(w).join("tehas-state.json"), &format!("{manifest}\n"));
    rebuilt(w);
}

/// $T/x packed again as $T/bad.tar, the manifest first.
fn rebuilt(w: &World) {
    sh_ok(
        w,
        &format!(
            "tar -cf '{}' -C '{}' tehas-state.json state repos live mcp",
            s(&bad_tar(w)),
            s(&x(w))
        ),
    );
}

/// A pack of the source with $T/x as the tree to edit.
fn staged(name: &str) -> World {
    let w = host(name);
    rc(&pack(&w, ""), 0);
    extract(&w);
    w
}

/// No one but root may write what it is denied.
fn root() -> bool {
    let out = Command::new("id").arg("-u").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim() == "0"
}

fn incoming(w: &World) -> usize {
    fs::read_dir(w.t().join("dst"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".tehas-incoming-")
        })
        .count()
}

fn chmod(p: &Path, mode: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(mode)).unwrap();
}

#[track_caller]
fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "exit {} and not {want}:\n{}", r.rc, r.out);
}

// --- pack --------------------------------------------------------------------

#[test]
fn pack_refuses_a_running_cycle_exit_1_the_issue_nothing_on_stdout() {
    let w = host("pack-running");
    w.cycle(true, "running", "work");
    let lock = hold(&w.at().cycle_note(12, "lock"));
    let r = pack(&w, "");
    drop(lock);
    rc(&r, 1);
    has(&r.out, "a cycle is running for #12");
    assert_eq!(tar_bytes(&w), 0);
}

#[test]
fn pack_refuses_a_cycle_killed_before_it_reported_and_names_tehas_stop() {
    let w = host("pack-killed");
    w.cycle(true, "running", "work");
    let r = pack(&w, "");
    rc(&r, 1);
    has(&r.out, "the cycle for #12 was killed before it reported");
    has(&r.out, "tehas stop 12");
    lacks(&r.out, "a cycle is running");
    assert_eq!(tar_bytes(&w), 0);
}

#[test]
fn pack_refuses_a_planning_turn_exit_1_the_issue_nothing_on_stdout() {
    let w = host("pack-plan");
    drop(w.plan_turn(13, 1, 0));
    let r = pack(&w, "");
    rc(&r, 1);
    has(&r.out, "#13");
    assert_eq!(tar_bytes(&w), 0);
}

#[test]
fn pack_refuses_an_active_unit_exit_1_the_unit_nothing_on_stdout() {
    let w = host("pack-unit");
    units(&w, "tehas-poll.timer loaded active waiting Poll");
    let r = pack(&w, "");
    rc(&r, 1);
    has(&r.out, "tehas-poll.timer");
    assert_eq!(tar_bytes(&w), 0);
}

#[test]
fn rehearse_packs_a_running_factory_and_changes_none_of_its_files() {
    let w = host("rehearse");
    w.cycle(true, "running", "work");
    units(&w, "tehas-poll.timer loaded active waiting Poll");
    let before = fingerprint(&w, &src(&w));
    rc(&pack(&w, "--rehearse"), 0);
    assert_eq!(fingerprint(&w, &src(&w)), before);
    assert_eq!(first_name(&w), "tehas-state.json");
}

#[test]
fn a_pack_whose_reader_goes_away_is_exit_141_with_nothing_on_standard_error() {
    let w = host("sigpipe");
    let err = w.t().join("err");
    let r = w.sh(&format!(
        r#"{{ sleep 0.3; "$T_ROOT/bin/tehas" state pack --rehearse - 2>'{}'; }} | true; exit ${{PIPESTATUS[0]}}"#,
        s(&err)
    ));
    rc(&r, 141);
    assert_eq!(read(&err), "");
}

#[test]
fn pack_holds_the_listed_paths_and_not_the_logs_the_work_or_the_settings() {
    let w = host("pack-list");
    // What outsource left in state/ before 2.0 dropped it is not carried.
    put(st(src(&w)).join("outsource/downloads/a"), "x\n");
    put(st(src(&w)).join("memory/cycles/issue-7.db"), "x\n");
    rc(&pack(&w, ""), 0);
    assert_eq!(first_name(&w), "tehas-state.json");
    assert_eq!(
        sh_ok(
            &w,
            &format!("tar -xOf '{}' tehas-state.json", s(&tar_file(&w)))
        ),
        format!(r#"{{"api":1,"schema":{SCHEMA},"tag":"v0.17.0"}}"#)
    );
    let list = names(&w, &tar_file(&w));
    for want in [
        rel(st(""), "ledger.txt").as_str(),
        "repos/product/main.c",
        &rel(lv(""), "a.json"),
        "mcp/sessions",
        &rel(st(""), "tehas.db"),
    ] {
        has(&list, want);
    }
    for not in [
        "logs",
        "work",
        ".config",
        "tehas.db-wal",
        "tehas.db-shm",
        &rel(st(""), "outsource"),
        &rel(st(""), "memory/cycles"),
    ] {
        lacks(&list, not);
    }
}

/// The store of the tree in $T/x, as a home of its own.
fn x_store(w: &World) -> Check {
    let libexec = x(w).join("libexec");
    fs::create_dir_all(&libexec).unwrap();
    fs::copy(w.t().join("store-bin"), libexec.join("store")).unwrap();
    check_of(w, &x(w))
}

#[test]
fn pack_takes_the_store_from_a_copy_the_store_made_one_that_reads_as_the_store() {
    let w = host("pack-store");
    ended(&w, &src(&w), 5);
    rc(&pack(&w, ""), 0);
    let _ = fs::remove_dir_all(x(&w));
    fs::create_dir_all(st(x(&w))).unwrap();
    sh_ok(
        &w,
        &format!("tar -xf '{}' -C '{}' state", s(&tar_file(&w)), s(&x(&w))),
    );
    let v = x_store(&w);
    assert_eq!(v.integrity, "ok");
    assert_eq!(v.cycles, 1);
}

#[test]
fn pack_leaves_out_the_live_token() {
    let w = host("token");
    for d in TOKENS {
        let dir = Home::new(src(&w)).token_dir(d);
        put(dir.join("token"), "ghs_live\n");
        put(dir.join("installation"), "acme 1\n");
    }
    rc(&pack(&w, ""), 0);
    lacks(&names(&w, &tar_file(&w)), "github");
    has(
        &w.calls("systemctl"),
        "--state=active,activating,deactivating,reloading",
    );
}

#[test]
fn rehearse_goes_on_past_a_file_it_cannot_read_and_pack_stops_at_it() {
    if root() {
        eprintln!("SKIP: root reads every file");
        return;
    }
    let w = host("unreadable");
    let main = src(&w).join("repos/product/main.c");
    chmod(&main, 0o000);
    rc(&pack(&w, ""), 1);
    rc(&pack(&w, "--rehearse"), 0);
    assert_eq!(first_name(&w), "tehas-state.json");
    chmod(&main, 0o644);
}

#[test]
fn pack_refuses_when_it_cannot_tell_whether_a_cycle_runs() {
    let w = host("no-cycles");
    let cycles = Home::new(src(&w)).cycles_dir();
    fs::remove_dir_all(&cycles).unwrap();
    put(&cycles, "not a directory\n");
    rc(&pack(&w, ""), 1);
    assert_eq!(tar_bytes(&w), 0);
}

#[test]
fn pack_holds_a_row_that_is_still_in_the_write_ahead_log() {
    let has_sqlite = Command::new("sh")
        .args(["-c", "command -v sqlite3"])
        .stdout(Stdio::null())
        .status()
        .unwrap()
        .success();
    if !has_sqlite {
        eprintln!("SKIP: no sqlite3 to hold the log open (the store's own copy test covers it)");
        return;
    }
    let w = host("wal");
    let db = st(src(&w)).join("tehas.db");
    let hold_fifo = w.t().join("hold");
    let held = w.t().join("held");
    // A reader holds a transaction open, so the store's write stays in the
    // log and is not folded into the database.
    let mut holder = Command::new("bash")
        .arg("-c")
        .arg(format!(
            r#"mkfifo '{f}'; sqlite3 '{db}' <'{f}' >/dev/null & h=$!; exec 7>'{f}'; printf '%s\n' "BEGIN;" "SELECT count(*) FROM cycles;" ".shell touch '{held}'" >&7; read -r _; exec 7>&-; kill "$h" 2>/dev/null; wait "$h" 2>/dev/null"#,
            f = s(&hold_fifo),
            db = s(&db),
            held = s(&held)
        ))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let begun = Instant::now();
    while !held.exists() && begun.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(held.exists(), "sqlite3 never held the log");
    ended(&w, &src(&w), 5);
    let wal = fs::metadata(st(src(&w)).join("tehas.db-wal")).map(|m| m.len());
    assert!(wal.unwrap_or(0) > 0, "the row was folded already");
    let r = pack(&w, "");
    drop(holder.stdin.take());
    holder.wait().unwrap();
    rc(&r, 0);
    lacks(&names(&w, &tar_file(&w)), "tehas.db-wal");
    let _ = fs::remove_dir_all(x(&w));
    fs::create_dir_all(st(x(&w))).unwrap();
    sh_ok(
        &w,
        &format!("tar -xf '{}' -C '{}' state", s(&tar_file(&w)), s(&x(&w))),
    );
    assert_eq!(x_store(&w).cycles, 1);
}

// --- unpack ------------------------------------------------------------------

#[test]
fn unpack_refuses_an_active_unit_and_writes_nothing() {
    let w = host("unpack-unit");
    rc(&pack(&w, ""), 0);
    units(&w, "tehas-dispatch.service loaded active running Dispatch");
    let before = whole(&w);
    let r = unpack(&w, &tar_file(&w));
    rc(&r, 1);
    has(&r.out, "tehas-dispatch.service");
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_refuses_another_release_names_both_and_writes_nothing() {
    let w = host("release");
    rc(&pack(&w, ""), 0);
    put(dst(&w).join("docs/version.txt"), "v0.16.0\n");
    let before = whole(&w);
    let r = unpack(&w, &tar_file(&w));
    rc(&r, 1);
    has(&r.out, "v0.17.0");
    has(&r.out, "v0.16.0");
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_refuses_another_schema_names_both_and_writes_nothing() {
    let w = host("schema");
    rc(&pack(&w, ""), 0);
    retar(&w, r#"{"api":1,"tag":"v0.17.0","schema":8}"#);
    let before = whole(&w);
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "schema 8");
    has(&r.out, &format!("schema {SCHEMA}"));
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_refuses_a_manifest_that_is_missing_not_first_or_of_an_unknown_api() {
    let w = host("manifest");
    rc(&pack(&w, ""), 0);
    let before = whole(&w);
    retar(&w, r#"{"api":2,"tag":"v0.17.0","schema":16}"#);
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "manifest");
    extract(&w);
    let (b, xd) = (s(&bad_tar(&w)), s(&x(&w)));
    sh_ok(&w, &format!("tar -cf '{b}' -C '{xd}' state repos live mcp"));
    rc(&unpack(&w, &bad_tar(&w)), 1);
    sh_ok(
        &w,
        &format!("tar -cf '{b}' -C '{xd}' state tehas-state.json repos live mcp"),
    );
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "not the first entry");
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_refuses_a_path_outside_the_home_dotdot_and_absolute() {
    let w = host("outside");
    rc(&pack(&w, ""), 0);
    extract(&w);
    put(x(&w).join("evil"), "evil\n");
    let before = whole(&w);
    let (b, xd, t) = (s(&bad_tar(&w)), s(&x(&w)), s(w.t()));
    sh_ok(
        &w,
        &format!(
            "tar -cf '{b}' -C '{xd}' tehas-state.json --transform 's|^evil$|../evil|' evil state"
        ),
    );
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "archive");
    sh_ok(
        &w,
        &format!(
            "tar -cPf '{b}' -C '{xd}' tehas-state.json --transform 's|^evil$|{t}/abs|' evil state"
        ),
    );
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "outside the state");
    assert_eq!(whole(&w), before);
    assert!(!w.t().join("evil").exists() && !w.t().join("abs").exists());
    assert!(!w.t().join("dst/evil").exists());
}

#[test]
fn unpack_replaces_the_state_checks_the_store_and_starts_no_unit() {
    let w = host("replace");
    ended(&w, &src(&w), 5);
    put(st(src(&w)).join("new.txt"), "only on the source\n");
    put(st(dst(&w)).join("old.txt"), "only on the target\n");
    put(lv(dst(&w)).join("stale.json"), "stale\n");
    rc(&pack(&w, ""), 0);
    let _ = fs::remove_file(w.t().join("calls/systemctl"));
    rc(&unpack(&w, &tar_file(&w)), 0);
    let d = dst(&w);
    assert!(!st(&d).join("old.txt").exists() && !lv(&d).join("stale.json").exists());
    assert_eq!(read(st(&d).join("new.txt")), "only on the source\n");
    assert_eq!(read(d.join("repos/product/main.c")), "code\n");
    let v = check_of(&w, &d);
    assert_eq!(v.integrity, "ok");
    assert_eq!(v.cycles, 1);
    assert_eq!(read(d.join("logs/work.log")), "log\n");
    assert_eq!(incoming(&w), 0);
    lacks(&w.calls("systemctl"), "start");
}

#[test]
fn unpack_refuses_a_rehearsal_and_writes_nothing() {
    let w = host("rehearsal");
    rc(&pack(&w, "--rehearse"), 0);
    let manifest = sh_ok(
        &w,
        &format!("tar -xOf '{}' tehas-state.json", s(&tar_file(&w))),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&manifest).unwrap()["rehearsal"],
        true
    );
    let before = whole(&w);
    let r = unpack(&w, &tar_file(&w));
    rc(&r, 1);
    has(&r.out, "rehearsal");
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_rehearse_accepts_a_rehearsal_and_an_unmarked_archive_as_a_normal_unpack() {
    for how in ["--rehearse", ""] {
        let w = host(&format!("unpack-rehearse{how}"));
        put(st(src(&w)).join("new.txt"), "only on the source\n");
        rc(&pack(&w, how), 0);
        let r = state(&w, &format!("unpack --rehearse - <'{}'", s(&tar_file(&w))));
        rc(&r, 0);
        assert_eq!(read(st(dst(&w)).join("new.txt")), "only on the source\n");
    }
}

#[test]
fn unpack_drops_a_token_the_archive_holds_and_keeps_the_targets_own() {
    let w = staged("token-drop");
    for (i, d) in TOKENS.into_iter().enumerate() {
        put(Home::new(x(&w)).token_dir(d).join("token"), "theirs\n");
        put(
            Home::new(dst(&w)).token_dir(d).join("token"),
            &format!("mine-{i}\n"),
        );
    }
    rebuilt(&w);
    rc(&unpack(&w, &bad_tar(&w)), 0);
    for (i, d) in TOKENS.into_iter().enumerate() {
        assert_eq!(
            read(Home::new(dst(&w)).token_dir(d).join("token")),
            format!("mine-{i}\n")
        );
    }
}

#[test]
fn unpack_refuses_a_link_at_a_top_level_name() {
    let w = staged("top-link");
    fs::remove_dir_all(lv(x(&w))).unwrap();
    symlink(w.t().join("outside"), lv(x(&w))).unwrap();
    rebuilt(&w);
    let before = whole(&w);
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "outside the state");
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_refuses_a_link_with_an_absolute_target_and_one_that_leaves_its_directory() {
    let w = staged("abs-link");
    let product = x(&w).join("repos/product");
    symlink("/etc/passwd", product.join("abs")).unwrap();
    rebuilt(&w);
    let before = whole(&w);
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "absolute");
    fs::remove_file(product.join("abs")).unwrap();
    symlink("../../state/tehas.db", product.join("esc")).unwrap();
    rebuilt(&w);
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "leaves its directory");
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_writes_nothing_through_a_link_of_the_archive() {
    let w = staged("through-link");
    let outside = w.t().join("outside");
    fs::create_dir_all(&outside).unwrap();
    put(w.t().join("y/pwn"), "pwn\n");
    symlink(&outside, x(&w).join("repos/l")).unwrap();
    rebuilt(&w);
    sh_ok(
        &w,
        &format!(
            "tar -rf '{}' -C '{}' --transform 's|^pwn$|repos/l/pwn|' pwn",
            s(&bad_tar(&w)),
            s(&w.t().join("y"))
        ),
    );
    let before = whole(&w);
    rc(&unpack(&w, &bad_tar(&w)), 1);
    assert!(!outside.join("pwn").exists());
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_keeps_relative_links_that_stay_in_their_directory_as_a_git_checkout_has() {
    let w = staged("relative-links");
    symlink("main.c", x(&w).join("repos/product/ok")).unwrap();
    fs::create_dir_all(x(&w).join("repos/other")).unwrap();
    symlink("../product/main.c", x(&w).join("repos/other/ok2")).unwrap();
    rebuilt(&w);
    rc(&unpack(&w, &bad_tar(&w)), 0);
    assert_eq!(
        fs::read_link(dst(&w).join("repos/product/ok")).unwrap(),
        Path::new("main.c")
    );
    assert_eq!(read(dst(&w).join("repos/other/ok2")), "code\n");
}

#[test]
fn unpack_refuses_a_tar_with_no_store() {
    let w = staged("no-store");
    fs::remove_file(st(x(&w)).join("tehas.db")).unwrap();
    rebuilt(&w);
    let before = whole(&w);
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, "no state store");
    assert_eq!(whole(&w), before);
}

#[test]
fn a_failed_swap_puts_the_old_state_back_and_leaves_nothing_beside_the_home() {
    if root() {
        eprintln!("SKIP: root can move a directory it may not write");
        return;
    }
    let w = host("swap");
    rc(&pack(&w, ""), 0);
    put(st(dst(&w)).join("old.txt"), "mine\n");
    let live = lv(dst(&w));
    chmod(&live, 0o555);
    let before = whole(&w);
    let r = unpack(&w, &tar_file(&w));
    chmod(&live, 0o755);
    rc(&r, 1);
    has(&r.out, "could not move live aside");
    assert_eq!(whole(&w), before);
    assert_eq!(read(st(dst(&w)).join("old.txt")), "mine\n");
    assert_eq!(check_of(&w, &dst(&w)).integrity, "ok");
    assert_eq!(incoming(&w), 0);
}

#[test]
fn sigterm_while_the_archive_is_still_coming_removes_the_directory_made_beside_the_home() {
    let w = host("sigterm");
    let fifo = w.t().join("in");
    let r = w.sh(&format!(
        r#"mkfifo '{f}'
TEHAS_HOME='{d}' "$T_ROOT/bin/tehas" state unpack - <'{f}' >/dev/null 2>&1 &
u=$!
exec 8>'{f}'
for _ in $(seq 50); do
  compgen -G '{d}/../.tehas-incoming-*' >/dev/null && break
  read -r -t 0.1 <> <(:) || true
done
compgen -G '{d}/../.tehas-incoming-*' >/dev/null && echo there || echo none
kill -TERM "$u"
wait "$u"
echo "rc $?"
exec 8>&-"#,
        f = s(&fifo),
        d = s(&dst(&w))
    ));
    assert_eq!(r.out, "there\nrc 143", "{}", r.out);
    assert_eq!(incoming(&w), 0);
}

#[test]
fn unpack_refuses_a_chain_of_links_that_climbs_out_of_its_directory() {
    let w = staged("chain");
    let product = x(&w).join("repos/product");
    symlink("..", product.join("up")).unwrap();
    symlink("up/../..", product.join("esc")).unwrap();
    rebuilt(&w);
    let before = whole(&w);
    let r = unpack(&w, &bad_tar(&w));
    rc(&r, 1);
    has(&r.out, ".. after a name");
    assert_eq!(whole(&w), before);
}

#[test]
fn unpack_refuses_a_setuid_setgid_or_world_writable_entry() {
    for mode in [0o4755, 0o2755, 0o666, 0o777] {
        let w = staged(&format!("mode-{mode:o}"));
        let odd = x(&w).join("repos/product/odd");
        put(&odd, "x\n");
        chmod(&odd, mode);
        rebuilt(&w);
        let before = whole(&w);
        let r = unpack(&w, &bad_tar(&w));
        rc(&r, 1);
        has(&r.out, "setuid, setgid or world-writable");
        assert_eq!(whole(&w), before);
    }
}

#[test]
fn a_pack_that_fails_in_a_pipe_sends_nothing_and_the_pipe_leaves_the_target_as_it_was() {
    if root() {
        eprintln!("SKIP: root reads every file");
        return;
    }
    let w = host("pipe");
    let main = src(&w).join("repos/product/main.c");
    chmod(&main, 0o000);
    put(st(src(&w)).join("new.txt"), "only on the source\n");
    let before = whole(&w);
    let r = w.sh(&format!(
        r#"set -o pipefail; "$T_ROOT/bin/tehas" state pack - | TEHAS_HOME='{}' "$T_ROOT/bin/tehas" state unpack -"#,
        s(&dst(&w))
    ));
    chmod(&main, 0o644);
    assert_ne!(r.rc, 0, "{}", r.out);
    assert_eq!(whole(&w), before);
    assert!(!st(dst(&w)).join("new.txt").exists());
}

// --- check -------------------------------------------------------------------

#[test]
fn check_json_prints_the_stores_own_answer_on_a_running_factory_too() {
    let w = host("check-json");
    ended(&w, &src(&w), 5);
    w.cycle(true, "running", "work");
    units(&w, "tehas-poll.timer loaded active waiting Poll");
    let before = fingerprint(&w, &src(&w));
    let r = state_src(&w, "check --json 2>/dev/null");
    rc(&r, 0);
    assert_eq!(r.out, check_of(&w, &src(&w)).raw);
    let v = serde_json::from_str::<Value>(&r.out).unwrap();
    assert_eq!(
        [
            &v["api"],
            &v["verb"],
            &v["integrity"],
            &v["schema"],
            &v["rows"]["cycles"]
        ],
        [
            &Value::from(1),
            &"check".into(),
            &"ok".into(),
            &SCHEMA.into(),
            &2.into()
        ]
    );
    assert_eq!(fingerprint(&w, &src(&w)), before);
}

#[test]
fn check_prints_the_schema_the_integrity_and_a_line_per_table_for_a_person() {
    let w = host("check-person");
    ended(&w, &src(&w), 5);
    let r = state_src(&w, "check");
    rc(&r, 0);
    has(&r.out, &format!("schema      {SCHEMA}"));
    has(&r.out, "integrity   ok");
    has(&r.out, "rows        cycles 1");
}

#[test]
fn check_a_store_that_does_not_check_is_1_one_that_cannot_be_used_78_nothing_on_stdout() {
    let w = host("check-fails");
    let db = s(&st(src(&w)).join("tehas.db"));
    w.store_answers(
        "check",
        1,
        &format!("store: integrity *** in database main ***: {db}"),
        0,
    );
    let r = state_src(&w, "check --json 2>/dev/null");
    rc(&r, 1);
    assert_eq!(r.out, "");
    has(
        &state_src(&w, "check --json").out,
        "does not check: store: integrity",
    );
    w.store_answers("check", 78, &format!("store: corrupt: {db}"), 0);
    let r = state_src(&w, "check --json 2>/dev/null");
    rc(&r, 78);
    assert_eq!(r.out, "");
}

#[test]
fn check_takes_only_json() {
    let w = host("check-args");
    rc(&state_src(&w, "check --bogus"), 64);
    rc(&state_src(&w, "check --json -"), 64);
}

// --- copy --------------------------------------------------------------------

#[test]
fn copy_json_copies_a_running_factorys_store_names_its_schema_and_fits_the_schema() {
    let w = host("copy-json");
    ended(&w, &src(&w), 5);
    w.cycle(true, "running", "work");
    let copies = w.t().join("copies");
    fs::create_dir_all(st(&copies)).unwrap();
    chmod(&st(&copies), 0o700);
    let target = st(&copies).join("tehas.db");
    let before = fingerprint(&w, &src(&w));
    let r = state_src(&w, &format!("copy --to '{}' --json", s(&target)));
    rc(&r, 0);
    assert_eq!(schema_errors("state-copy", &r.out), "");
    let v = serde_json::from_str::<Value>(&r.out).unwrap();
    assert_eq!(v["target"], s(&target));
    assert_eq!(v["schema"], check_of(&w, &src(&w)).schema);
    assert!(v["rows"].as_u64().unwrap() > 0);
    let listed: Vec<String> = fs::read_dir(st(&copies))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(listed, ["tehas.db"]);
    assert_eq!(fingerprint(&w, &src(&w)), before);
    let copy = check_of(&w, &copies);
    assert_eq!(copy.integrity, "ok");
    assert_eq!(copy.cycles, 2);
}

#[test]
fn copy_of_a_new_store_is_made_and_says_0_rows_a_person_gets_one_line() {
    let w = host("copy-new");
    let copies = w.t().join("copies");
    fs::create_dir_all(&copies).unwrap();
    let r = w.sh(&format!(
        r#"cd '{}' && "$T_ROOT/bin/tehas" state copy --to new.db"#,
        s(&copies)
    ));
    rc(&r, 0);
    let ms = r
        .out
        .rsplit_once(", ")
        .and_then(|(_, t)| t.strip_suffix(" ms"))
        .expect("a time in ms");
    assert_eq!(
        r.out,
        format!(
            "copied the state store to {}/new.db: schema {SCHEMA}, 0 rows, {ms} ms",
            s(&copies)
        )
    );
    let listed: Vec<String> = fs::read_dir(&copies)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(listed, ["new.db"]);
}

#[test]
fn copy_refuses_a_target_that_is_there_in_one_line_names_it_and_leaves_it() {
    let w = host("copy-taken");
    let taken = w.t().join("copies/taken.db");
    put(&taken, "mine\n");
    let r = state_src(&w, &format!("copy --to '{}' --json 2>/dev/null", s(&taken)));
    rc(&r, 1);
    assert_eq!(r.out, "");
    let r = state_src(&w, &format!("copy --to '{}' --json", s(&taken)));
    assert_eq!(
        r.out,
        format!(
            "tehas state: the target {} already holds a file; name a new one",
            s(&taken)
        )
    );
    assert_eq!(read(&taken), "mine\n");
    let nowhere = w.t().join("nowhere/x.db");
    let r = state_src(&w, &format!("copy --to '{}'", s(&nowhere)));
    rc(&r, 1);
    assert_eq!(
        r.out,
        format!(
            "tehas state: the directory of the target {} is not there",
            s(&nowhere)
        )
    );
}

#[test]
fn copy_takes_to_file_and_json_only() {
    let w = host("copy-args");
    rc(&state_src(&w, "copy"), 64);
    rc(&state_src(&w, "copy --to"), 64);
    let c = w.t().join("c.db");
    rc(
        &state_src(&w, &format!("copy --to '{}' --bogus", s(&c))),
        64,
    );
    assert!(!c.exists());
}

/// The ways a sample of `api/<schema>.<sample>.json`, edited by the jq
/// `edit`, breaks its schema.
fn sample_errors(schema: &str, sample: &str, edit: &str) -> String {
    let file = repo().join(format!("api/{schema}.{sample}.json"));
    let text = fs::read_to_string(file).unwrap();
    schema_errors(schema, &jq(edit, &text))
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}

#[test]
fn the_samples_of_api_state_copy_fit_its_schema_and_the_check_finds_what_does_not() {
    assert_eq!(sample_errors("state-copy", "sample", "."), "");
    assert_eq!(sample_errors("state-copy", "empty", "."), "");
    let out = sample_errors(
        "state-copy",
        "sample",
        ".rows = -1 | del(.target) | .later = 1 | .schema = 11",
    );
    has(&out, ".schema: 11 is under 12");
    has(&out, ".rows: -1 is under 0");
    has(&out, ".: target is missing");
    has(&out, ".later: not in the schema");
}

// --- load --------------------------------------------------------------------

/// $T/copies/c.db: a copy of the source's store with two ended cycles in it.
fn a_copy(w: &World) -> PathBuf {
    let copies = w.t().join("copies");
    let _ = fs::remove_dir_all(&copies);
    fs::create_dir_all(&copies).unwrap();
    ended(w, &src(w), 5);
    ended(w, &src(w), 6);
    let c = copies.join("c.db");
    rc(&state_src(w, &format!("copy --to '{}'", s(&c))), 0);
    c
}

fn drain(w: &World) {
    put(
        Home::new(dst(w)).state(State::Limit),
        "{\"v\":1,\"cycles\":0,\"plan\":false,\"until\":null,\"reason\":\"draining\"}\n",
    );
}

#[test]
fn load_json_fills_a_drained_factorys_new_store_from_a_copy_and_fits_the_schema() {
    let w = host("load-json");
    let c = a_copy(&w);
    drain(&w);
    let r = state(&w, &format!("load --from '{}' --json", s(&c)));
    rc(&r, 0);
    assert_eq!(schema_errors("state-load", &r.out), "");
    let v = serde_json::from_str::<Value>(&r.out).unwrap();
    assert_eq!(v["source"], s(&c));
    assert_eq!(v["schema"], check_of(&w, &dst(&w)).schema);
    assert!(v["rows"].as_u64().unwrap() > 0);
    let after = check_of(&w, &dst(&w));
    assert_eq!(after.integrity, "ok");
    assert_eq!(after.cycles, 2);
    let loads = fs::read_dir(st(dst(&w)))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("load")
        })
        .count();
    assert_eq!(loads, 0);
    let r = w.sh(&format!(
        r#"cd '{}' && TEHAS_HOME='{}' "$T_ROOT/bin/tehas" state load --from c.db"#,
        s(c.parent().unwrap()),
        s(&dst(&w))
    ));
    rc(&r, 1);
    has(&r.out, "tehas state: the state store holds history, ");
}

#[test]
fn load_refuses_a_factory_that_is_not_drained_or_runs_a_cycle_and_leaves_the_store() {
    let w = host("load-drained");
    let c = a_copy(&w);
    let from = format!("load --from '{}'", s(&c));
    let before = fingerprint(&w, &dst(&w));
    let r = state(&w, &format!("{from} --json"));
    rc(&r, 1);
    let not_drained = "tehas state: the factory is not drained: run tehas drain, then load";
    assert_eq!(r.out, not_drained);
    let limit = Home::new(dst(&w)).state(State::Limit);
    put(
        &limit,
        "{\"v\":1,\"cycles\":1,\"plan\":false,\"until\":null,\"reason\":\"x\"}\n",
    );
    assert_eq!(state(&w, &from).out, not_drained);
    fs::remove_file(&limit).unwrap();
    assert_eq!(fingerprint(&w, &dst(&w)), before);
    drain(&w);
    // t_live on the target: a cycle started 600 s ago, still running.
    let d = dst(&w);
    must(&w, &d, Some(12), &["cycle-start", "12", "--by-hand"]);
    must(
        &w,
        &d,
        None,
        &[
            "cycle-heartbeat",
            "12",
            "--running",
            "--status",
            "running",
            "--step",
            "work",
        ],
    );
    let r = state(&w, &from);
    rc(&r, 1);
    assert_eq!(
        r.out,
        "tehas state: the cycle for #12 was killed before it reported and its record is open: tehas stop 12 ends it"
    );
    let lock = hold(&Home::new(&d).cycle_note(12, "lock"));
    let r = state(&w, &from);
    drop(lock);
    rc(&r, 1);
    assert_eq!(r.out, "tehas state: a cycle is running for #12");
}

#[test]
fn load_refuses_a_store_that_holds_history_in_one_line_and_leaves_it() {
    let w = host("load-history");
    let c = a_copy(&w);
    ended(&w, &dst(&w), 9);
    drain(&w);
    let before = fingerprint(&w, &dst(&w));
    let r = state(&w, &format!("load --from '{}' --json 2>/dev/null", s(&c)));
    rc(&r, 1);
    assert_eq!(r.out, "");
    let r = state(&w, &format!("load --from '{}'", s(&c)));
    assert_eq!(
        r.out,
        "tehas state: the state store holds history, 1 rows in cycles; a copy loads only into a new or empty store"
    );
    assert_eq!(fingerprint(&w, &dst(&w)), before);
}

/// The copy's user_version, the four bytes at offset 60 of SQLite's header.
fn set_user_version(db: &Path, v: u8) {
    let mut bytes = fs::read(db).unwrap();
    bytes[60..64].copy_from_slice(&[0, 0, 0, v]);
    fs::write(db, bytes).unwrap();
}

#[test]
fn load_refuses_a_copy_of_a_newer_schema_naming_both_one_older_than_v0_30_0_and_an_unreadable_copy()
{
    let w = host("load-schema");
    let c = a_copy(&w);
    drain(&w);
    let before = fingerprint(&w, &dst(&w));
    let mine = check_of(&w, &dst(&w)).schema;
    set_user_version(&c, 99);
    let r = state(&w, &format!("load --from '{}'", s(&c)));
    rc(&r, 1);
    assert_eq!(
        r.out,
        format!(
            "tehas state: the copy {} holds schema 99, this release reads schema {mine} and older",
            s(&c)
        )
    );
    // 11 is one below the oldest schema this release migrates, the one
    // v0.30.0 wrote.
    set_user_version(&c, 11);
    let r = state(&w, &format!("load --from '{}'", s(&c)));
    rc(&r, 1);
    assert_eq!(
        r.out,
        format!(
            "tehas state: the copy {} holds schema 11, older than v0.30.0 wrote; load it into a v0.30 release first",
            s(&c)
        )
    );
    let text = w.t().join("copies/text.db");
    put(&text, "not a store\n");
    let r = state(&w, &format!("load --from '{}'", s(&text)));
    rc(&r, 1);
    assert_eq!(
        r.out,
        format!(
            "tehas state: the copy {} is not an SQLite database",
            s(&text)
        )
    );
    let gone = w.t().join("copies/gone.db");
    let r = state(&w, &format!("load --from '{}'", s(&gone)));
    rc(&r, 1);
    assert_eq!(
        r.out,
        format!("tehas state: the copy {} is not there", s(&gone))
    );
    assert_eq!(fingerprint(&w, &dst(&w)), before);
}

#[test]
fn load_takes_from_file_and_json_only() {
    let w = host("load-args");
    rc(&state(&w, "load"), 64);
    rc(&state(&w, "load --from"), 64);
    let c = s(&w.t().join("c.db"));
    rc(&state(&w, &format!("load --from '{c}' --bogus")), 64);
}

#[test]
fn the_samples_of_api_state_load_fit_its_schema_and_the_check_finds_what_does_not() {
    assert_eq!(sample_errors("state-load", "sample", "."), "");
    assert_eq!(sample_errors("state-load", "empty", "."), "");
    let out = sample_errors(
        "state-load",
        "sample",
        r#".schema = 0 | del(.source) | .target = "x""#,
    );
    has(&out, ".schema: 0 is under 12");
    has(&out, ".: source is missing");
    has(&out, ".target: not in the schema");
}

// --- the words and the settings ----------------------------------------------

#[test]
fn a_bad_argument_is_64_missing_settings_are_78_and_neither_starts_a_tar() {
    let w = host("args");
    rc(&state_src(&w, "pack"), 64);
    rc(&state(&w, "unpack --bogus -"), 64);
    w.no_instance();
    rc(&pack(&w, ""), 78);
    assert_eq!(tar_bytes(&w), 0);
}
