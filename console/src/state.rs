//! tehas state pack and unpack: a stopped factory's state as one tar, to move
//! it to another machine (libexec/state says how and why).
//!
//!   console state pack [--rehearse] -    the tar on standard output
//!   console state unpack [--rehearse] -  the tar on standard input
//!
//! The home is $TEHAS_HOME, which libexec/state has loaded.
//!
//! The tar holds tehas-state.json first, {"api":1,"tag":...,"schema":...},
//! then state/, repos/, live/ and mcp/ of the home where they exist. The
//! store is not copied as a file: state/tehas.db is a copy the store made of
//! itself (store copy), so what is still in its WAL is in it, and neither
//! tehas.db-wal nor tehas.db-shm is packed. state/github, the live token, is
//! not packed either: the target mints its own, and unpack keeps the target's.
//!
//! Pack refuses (1) while a cycle or a planning turn runs, or a tehas-* unit
//! is active; --rehearse skips those refusals to size or try a pack on a
//! running factory, changes nothing in the home, and marks the manifest
//! "rehearsal":true, which unpack takes only with --rehearse. Unpack refuses
//! (1) while a tehas-* unit is active, another release, another schema, a
//! manifest that is not the first entry, a tar that tar itself will not
//! extract safely, anything staged but the manifest and the four directories,
//! a link that is absolute, leaves its directory or has a `..` after a name, a
//! device, pipe or socket, a setuid, setgid or world-writable entry, and a
//! store that does not check. It reads the tar to a directory beside the
//! home, extracts and checks it there, and only then swaps the four
//! directories, so a refused or broken tar leaves the home as it was. It
//! starts no unit. A SIGTERM, SIGHUP or SIGINT removes the directories made
//! here, except once the swap has begun.
//!
//! unpack --rehearse takes a rehearsal tar, and it replaces the state as unpack
//! does: the old state is gone afterwards. It is for a machine that will be
//! rebuilt.
//!
//! Neither calls GitHub, Docker or the network.
//!
//! Exit: 0 done; 1 refused or failed, said on standard error; 64 usage; 78
//! the release or the state store of the home cannot be read.

use crate::store;
use serde_json::{json, Value};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

const USAGE: &str = "usage: tehas state pack [--rehearse] - | unpack [--rehearse] -";
const MANIFEST: &str = "tehas-state.json";
const DIRS: [&str; 4] = ["state", "repos", "live", "mcp"];
const ACTIVE: &str = "--state=active,activating,deactivating,reloading";

type Fail = (u8, String);

/// The directory a signal removes. The handler holds this lock from taking it
/// to the exit, so a disarm or a drop waits for a signal already under way.
static CLEAN: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn main(given: &[String]) -> u8 {
    let given: Vec<&str> = given.iter().map(String::as_str).collect();
    let Some(home) = std::env::var_os("TEHAS_HOME").map(PathBuf::from) else {
        eprintln!("tehas state: TEHAS_HOME is not set");
        return 78;
    };
    let done = match given.as_slice() {
        ["pack", "-"] => pack(&home, false),
        ["pack", "--rehearse", "-"] => pack(&home, true),
        ["unpack", "-"] => unpack(&home, false),
        ["unpack", "--rehearse", "-"] => unpack(&home, true),
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    match done {
        Ok(()) => 0,
        Err((rc, why)) => {
            eprintln!("tehas state: {why}");
            rc
        }
    }
}

fn io_fail(what: &str) -> impl Fn(io::Error) -> Fail + '_ {
    move |e| (1, format!("{what}: {e}"))
}

/// The release in use, read as libexec/api reads it: the first line of
/// docs/version.txt, its tag when it is "v1.2.3+abc1234", the whole when it is
/// a bare tag or a bare commit.
fn tag(home: &Path) -> Result<String, Fail> {
    let text = fs::read_to_string(home.join("docs/version.txt")).unwrap_or_default();
    let first = text.lines().next().unwrap_or("").trim();
    let tag = first.split('+').next().unwrap_or("");
    if tag.is_empty()
        || tag.len() > 100
        || !tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    {
        return Err((
            78,
            format!("{}/docs/version.txt names no release", home.display()),
        ));
    }
    Ok(tag.to_string())
}

/// What `store check` says of a store: its schema. A refusal or a store that
/// does not check is 1; a store that cannot be run is 78 when it is the
/// host's own (`own`) and 1 when it came in a tar.
fn checked(home: &Path, own: bool) -> Result<i64, Fail> {
    let unusable = if own { 78 } else { 1 };
    let v = match store::call(home, &["check"]) {
        Ok(v) => v,
        Err(r) if r.rc == 1 => {
            return Err((1, format!("the state store does not check: {}", r.line)))
        }
        Err(r) => {
            return Err((
                unusable,
                format!("the state store cannot be used: {}", r.line),
            ))
        }
    };
    if v["integrity"] != json!("ok") {
        return Err((
            1,
            format!("the state store does not check: {}", v["integrity"]),
        ));
    }
    v["schema"]
        .as_i64()
        .ok_or_else(|| (unusable, "the state store gave no schema".to_string()))
}

/// The tehas-* units `systemctl --user` reports as running or on their way.
fn active_units() -> Result<Vec<String>, Fail> {
    let out = Command::new("systemctl")
        .args([
            "--user",
            "list-units",
            "--plain",
            "--no-legend",
            ACTIVE,
            "tehas-*",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(io_fail("could not ask systemctl"))?;
    if !out.status.success() {
        return Err((
            1,
            "could not ask systemctl which units are active".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|u| u.starts_with("tehas-"))
        .map(String::from)
        .collect())
}

fn no_active_unit() -> Result<(), Fail> {
    let units = active_units()?;
    if units.is_empty() {
        return Ok(());
    }
    Err((1, format!("a unit is active: {}", units.join(" "))))
}

/// An answer of the host's own store: 1 when it refused, 78 when it cannot be
/// used.
fn answer(home: &Path, args: &[&str]) -> Result<Value, Fail> {
    store::call(home, args).map_err(|r| {
        (
            if r.rc == 1 { 1 } else { 78 },
            format!("the state store did not answer {}: {}", args[0], r.line),
        )
    })
}

fn numbers(v: &Value, path: &str) -> Vec<i64> {
    v.pointer(path)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_i64().or_else(|| x["issue"].as_i64()))
                .collect()
        })
        .unwrap_or_default()
}

/// A cycle or a planning turn that runs, as the store has it.
fn no_work_running(home: &Path) -> Result<(), Fail> {
    let shown = answer(home, &["cycle-show", "--running"])?;
    let mut cycles = numbers(&shown, "/cycles");
    cycles.extend(numbers(&answer(home, &["locks-held"])?, "/held"));
    cycles.sort_unstable();
    cycles.dedup();
    if let Some(n) = cycles.first() {
        return Err((1, format!("a cycle is running for #{n}")));
    }
    if shown["probe_rc"].as_i64() != Some(0) {
        return Err((
            1,
            format!(
                "could not tell whether a cycle runs: {}",
                shown["why"].as_str().unwrap_or("the store could not probe")
            ),
        ));
    }
    let plans = answer(home, &["plan-show", "--open"])?;
    let running = plans["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["running"] == json!(true));
    if let Some(s) = running {
        return Err((1, format!("a planning turn is running for #{}", s["issue"])));
    }
    Ok(())
}

/// A directory made 0700, removed when dropped and when a signal
/// comes (but not while the swap runs, which has the old state in it).
struct Scratch {
    dir: PathBuf,
    keep: bool,
}

impl Scratch {
    fn new(parent: &Path, name: &str) -> Result<Scratch, Fail> {
        let dir = parent.join(format!(".{name}-{}", std::process::id()));
        remove_all(&dir);
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(io_fail(&format!("could not make {}", dir.display())))?;
        *CLEAN.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.clone());
        if let Ok(mut signals) = Signals::new([SIGTERM, SIGHUP, SIGINT]) {
            std::thread::spawn(move || {
                for sig in signals.forever() {
                    // Nothing to clean once the swap has begun: it is disarmed.
                    let mut held = CLEAN.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(d) = held.take() {
                        remove_all(&d);
                        std::process::exit(128 + sig);
                    }
                }
            });
        }
        Ok(Scratch { dir, keep: false })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        *CLEAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if !self.keep {
            remove_all(&self.dir);
        }
    }
}

/// Makes the directories below `dir` writable, which tar -p may have taken
/// away, so the removal can go through.
fn make_writable(dir: &Path) {
    let Ok(meta) = fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            make_writable(&e.path());
        }
    }
}

/// Removes `dir`, and says so if it could not.
fn remove_all(dir: &Path) {
    if !dir.exists() {
        return;
    }
    make_writable(dir);
    if let Err(e) = fs::remove_dir_all(dir) {
        eprintln!("tehas state: could not remove {}: {e}", dir.display());
    }
}

fn pack(home: &Path, rehearse: bool) -> Result<(), Fail> {
    if !rehearse {
        no_active_unit()?;
        no_work_running(home)?;
    }
    let tag = tag(home)?;
    let schema = checked(home, true)?;
    let scratch = Scratch::new(&std::env::temp_dir(), "tehas-state")?;
    let mut manifest = json!({"api": 1, "tag": tag, "schema": schema});
    if rehearse {
        manifest["rehearsal"] = json!(true);
    }
    fs::write(scratch.dir.join(MANIFEST), format!("{manifest}\n"))
        .map_err(io_fail("could not write the manifest"))?;
    let copy = scratch.dir.join("tehas.db.copy");
    let whole = scratch.dir.join("state.tar");
    answer(home, &["copy", "--to", &copy.to_string_lossy()])?;
    let mut tar = Command::new("tar");
    tar.env("LC_ALL", "C")
        .args(["-c", "--anchored", "--numeric-owner", "-f"])
        .arg(&whole)
        .args([
            "--exclude=state/tehas.db",
            "--exclude=state/tehas.db-wal",
            "--exclude=state/tehas.db-shm",
            "--exclude=state/github",
        ])
        .args(["--transform", "s|^tehas\\.db\\.copy$|state/tehas.db|"]);
    if rehearse {
        // A running factory changes files under tar: that is not a failure.
        tar.args(["--warning=no-file-changed", "--ignore-failed-read"]);
    }
    tar.arg("-C")
        .arg(&scratch.dir)
        .arg(MANIFEST)
        .arg("-C")
        .arg(home);
    for dir in DIRS.iter().filter(|d| home.join(d).is_dir()) {
        tar.arg(dir);
    }
    tar.arg("-C").arg(&scratch.dir).arg("tehas.db.copy");
    // The tar is finished before a byte of it is sent, so a tar that failed
    // never reaches the other end looking whole.
    let status = tar
        .stdin(Stdio::null())
        .status()
        .map_err(io_fail("could not run tar"))?;
    if !(status.success() || (rehearse && status.code() == Some(1))) {
        return Err((1, format!("tar failed: {status}")));
    }
    let mut made = fs::File::open(&whole).map_err(io_fail("could not read the tar"))?;
    io::copy(&mut made, &mut io::stdout().lock()).map_err(io_fail("could not write the tar"))?;
    Ok(())
}

fn tar_out(args: &[&str], archive: &Path) -> Result<Vec<u8>, Fail> {
    let out = Command::new("tar")
        .env("LC_ALL", "C")
        .args(args)
        .arg("-f")
        .arg(archive)
        .stdin(Stdio::null())
        .output()
        .map_err(io_fail("could not run tar"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err((
            1,
            format!(
                "the archive does not read as a tar: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ))
    }
}

/// The manifest's tag, schema and rehearsal mark, when it is one this command
/// reads.
fn manifest(text: &[u8]) -> Result<(String, i64, bool), Fail> {
    let bad = || {
        (
            1,
            format!("{MANIFEST} is not the manifest of an archive this command reads"),
        )
    };
    let v: Value = serde_json::from_slice(text).map_err(|_| bad())?;
    match (v["api"].as_i64(), v["tag"].as_str(), v["schema"].as_i64()) {
        (Some(1), Some(tag), Some(schema)) => {
            Ok((tag.to_string(), schema, v["rehearsal"] == json!(true)))
        }
        _ => Err(bad()),
    }
}

/// Why a link may not be kept: its target is absolute, or resolved lexically
/// from the link's place it leaves the top directory it is in. `place` is the
/// link's directory, relative to the staging directory, starting at the top.
fn bad_link(place: &Path, target: &Path) -> Option<&'static str> {
    if target.is_absolute() {
        return Some("is absolute");
    }
    let mut depth = place.components().count();
    let mut named = false;
    for c in target.components() {
        match c {
            // `..` after a name could climb out through a link of the tar.
            Component::ParentDir if named => return Some("has a .. after a name"),
            Component::ParentDir => {
                depth -= 1;
                if depth == 0 {
                    return Some("leaves its directory");
                }
            }
            Component::Normal(_) => named = true,
            _ => {}
        }
    }
    None
}

/// The staged tree, entry by entry, without following a link: only the
/// manifest and the four directories at the top, each a real directory, and
/// below them files, directories and links that stay in their directory.
fn walk(dir: &Path, root: &Path) -> Result<(), Fail> {
    let entries =
        fs::read_dir(dir).map_err(io_fail(&format!("could not read {}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(io_fail("could not read the staged archive"))?;
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let meta =
            fs::symlink_metadata(&path).map_err(io_fail("could not read the staged archive"))?;
        let kind = meta.file_type();
        if !kind.is_symlink() && meta.mode() & 0o6002 != 0 {
            return Err((
                1,
                format!(
                    "the archive holds a setuid, setgid or world-writable entry: {}",
                    rel.display()
                ),
            ));
        }
        let top = dir == root;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if top {
            let fine = if name == MANIFEST {
                kind.is_file()
            } else {
                DIRS.contains(&name.as_ref()) && kind.is_dir()
            };
            if !fine {
                return Err((
                    1,
                    format!(
                        "the archive holds an entry outside the state: {}",
                        rel.display()
                    ),
                ));
            }
        }
        if kind.is_dir() {
            walk(&path, root)?;
        } else if kind.is_symlink() {
            let target = fs::read_link(&path).map_err(io_fail("could not read a link"))?;
            let place = rel.parent().unwrap_or(Path::new(""));
            if let Some(why) = bad_link(place, &target) {
                return Err((
                    1,
                    format!(
                        "the archive holds a link that {why}: {} to {}",
                        rel.display(),
                        target.display()
                    ),
                ));
            }
        } else if kind.is_block_device()
            || kind.is_char_device()
            || kind.is_fifo()
            || kind.is_socket()
        {
            return Err((
                1,
                format!(
                    "the archive holds a device, pipe or socket: {}",
                    rel.display()
                ),
            ));
        }
    }
    Ok(())
}

fn unpack(home: &Path, rehearse: bool) -> Result<(), Fail> {
    no_active_unit()?;
    let mine = tag(home)?;
    let schema = checked(home, true)?;
    let parent = home
        .parent()
        .ok_or((78, "the home has no parent directory".to_string()))?;
    let mut scratch = Scratch::new(parent, "tehas-incoming")?;
    let archive = scratch.dir.join("archive.tar");
    let mut file = fs::File::create(&archive).map_err(io_fail("could not write the archive"))?;
    io::copy(&mut io::stdin().lock(), &mut file).map_err(io_fail("could not read the archive"))?;
    drop(file);

    let names = tar_out(&["-t"], &archive)?;
    if names.split(|b| *b == b'\n').next() != Some(MANIFEST.as_bytes()) {
        return Err((
            1,
            format!("{MANIFEST} is not the first entry of the archive"),
        ));
    }
    let (theirs, their_schema, rehearsal) = manifest(&tar_out(&["-x", "-O", MANIFEST], &archive)?)?;
    if rehearsal && !rehearse {
        return Err((
            1,
            "the archive is a rehearsal of a running factory; unpack --rehearse takes it"
                .to_string(),
        ));
    }
    if theirs != mine {
        return Err((
            1,
            format!("the archive is from release {theirs}, this host runs {mine}"),
        ));
    }
    if their_schema != schema {
        return Err((
            1,
            format!(
                "the archive holds schema {their_schema}, this host's store is at schema {schema}"
            ),
        ));
    }

    let new = scratch.dir.join("new");
    fs::create_dir(&new).map_err(io_fail("could not make the staging directory"))?;
    let out = Command::new("tar")
        .env("LC_ALL", "C")
        .args(["-x", "-p", "--no-same-owner", "-f"])
        .arg(&archive)
        .arg("-C")
        .arg(&new)
        .stdin(Stdio::null())
        .output()
        .map_err(io_fail("could not run tar"))?;
    if !out.status.success() {
        return Err((
            1,
            format!(
                "tar refused the archive: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ));
    }
    walk(&new, &new)?;
    // The live token is the target's own, never the source's.
    let token = new.join("state/github");
    if fs::symlink_metadata(&token).is_ok() {
        fs::remove_dir_all(&token).map_err(io_fail("could not drop the archive's token"))?;
    }
    if fs::symlink_metadata(new.join("state/tehas.db")).is_err() {
        return Err((1, "the archive holds no state store".to_string()));
    }
    // The store is checked where it is, with the installed store binary.
    std::os::unix::fs::symlink(home.join("libexec"), new.join("libexec"))
        .map_err(io_fail("could not link libexec"))?;
    let got = checked(&new, false)?;
    if got != their_schema {
        return Err((
            1,
            format!("the unpacked store is at schema {got}, the manifest says {their_schema}"),
        ));
    }
    fs::remove_file(new.join("libexec")).map_err(io_fail("could not unlink libexec"))?;
    no_active_unit()?;
    // From here a signal must not take the directory that holds the old state.
    *CLEAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let own_token = home.join("state/github");
    if fs::symlink_metadata(&own_token).is_ok() {
        fs::rename(&own_token, &token).map_err(io_fail("could not keep this host's token"))?;
    }
    let done = swap(home, &new, &scratch.dir.join("old"));
    if let Err(e) = done {
        // The target's token goes back where it was found.
        let _ = fs::rename(&token, &own_token);
        if e.1.contains("left in ") {
            scratch.keep = true;
        }
        return Err(e);
    }
    Ok(())
}

/// Each directory of the state moves out of the home and its new one moves in;
/// if any move fails, what moved is put back, and if a put-back fails the
/// directory holding the rest is kept and named.
fn swap(home: &Path, new: &Path, old: &Path) -> Result<(), Fail> {
    fs::create_dir(old).map_err(io_fail("could not make the directory for the old state"))?;
    let there = |p: PathBuf| fs::symlink_metadata(p).is_ok();
    let mut aside: Vec<&str> = Vec::new();
    let mut placed: Vec<&str> = Vec::new();
    let moved: Result<(), Fail> = (|| {
        for d in DIRS {
            if there(home.join(d)) {
                fs::rename(home.join(d), old.join(d))
                    .map_err(io_fail(&format!("could not move {d} aside")))?;
                aside.push(d);
            }
            if there(new.join(d)) {
                fs::rename(new.join(d), home.join(d))
                    .map_err(io_fail(&format!("could not place {d}")))?;
                placed.push(d);
            }
        }
        Ok(())
    })();
    let Err((rc, why)) = moved else {
        return Ok(());
    };
    let mut stuck = Vec::new();
    for d in placed {
        if fs::rename(home.join(d), new.join(d)).is_err() {
            stuck.push(d);
        }
    }
    for d in aside {
        if fs::rename(old.join(d), home.join(d)).is_err() {
            stuck.push(d);
        }
    }
    if stuck.is_empty() {
        return Err((rc, why));
    }
    let scratch = old.parent().unwrap_or(old);
    Err((
        rc,
        format!(
            "{why}; could not put back {}, they are left in {}",
            stuck.join(" "),
            scratch.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_inside_its_directory_is_kept() {
        for (place, target) in [
            ("repos/p", "main.c"),
            ("repos/p", "../q/file"),
            ("repos/p/deep", "../../q"),
            ("repos", "p/main.c"),
            ("live", "./a.json"),
        ] {
            assert_eq!(
                bad_link(Path::new(place), Path::new(target)),
                None,
                "{place} {target}"
            );
        }
    }

    #[test]
    fn a_link_that_is_absolute_or_leaves_its_directory_is_refused() {
        for (place, target) in [
            ("repos/p", "/etc/passwd"),
            ("repos/p", "../../state/tehas.db"),
            ("repos", "../state"),
            ("repos/p/deep", "../../../mcp"),
            ("live", ".."),
            ("repos/p", "up/.."),
            ("repos/p", "up/../.."),
            ("repos/p", "a/b/../../.."),
        ] {
            assert!(
                bad_link(Path::new(place), Path::new(target)).is_some(),
                "{place} {target}"
            );
        }
    }

    #[test]
    fn a_manifest_needs_api_1_a_tag_and_a_schema() {
        assert_eq!(
            manifest(br#"{"api":1,"tag":"v0.17.0","schema":9}"#).unwrap(),
            ("v0.17.0".to_string(), 9, false)
        );
        assert!(
            manifest(br#"{"api":1,"tag":"v1","schema":9,"rehearsal":true}"#)
                .unwrap()
                .2
        );
        for bad in [
            r#"{"api":2,"tag":"v1","schema":9}"#,
            r#"{"api":1,"schema":9}"#,
            r#"{"api":1,"tag":"v1"}"#,
            "nope",
        ] {
            assert!(manifest(bad.as_bytes()).is_err(), "{bad}");
        }
    }
}
