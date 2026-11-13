//! The recipe of the sandbox image, for host/sandbox/build.sh, which keeps
//! the docker calls. What the two stages are for and why docker is handed a
//! copy: host/docs/host.md, What two instance users of one host share.
//!
//!   console sandbox recipe <dir> <build|if-moved|check>
//!     <dir>/Dockerfile and <dir>/base.Dockerfile are there, and the Dockerfile
//!     holds one line that reads exactly `FROM shared`, one that reads exactly
//!     `ARG TEHAS_UID=` and no other `ARG TEHAS_UID` line. A staged copy that
//!     is not a plain file is refused, a plain one is said with the time it was
//!     written, and the temporaries of earlier runs are taken back.
//!   console sandbox stage <dir>
//!     The copy docker is handed, written owner-only to a temporary beside it,
//!     checked by the same rule and renamed to <dir>/sandbox-uid.Dockerfile.
//!     On a refusal nothing is left at the temporary.
//!   console sandbox current <image> <label> <commit> <what>
//!     Says which commit <image> is on, from its tehas.rukas <label>. Exit 0
//!     when that is <commit>, 1 when not.
//!
//! Exit 0 done; 1 refused, with the reason on stderr; 64 any other words.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tehas_core::time;

const USAGE: &str = "usage: console sandbox recipe <dir> <build|if-moved|check>
       console sandbox stage <dir>
       console sandbox current <image> <label> <commit> <what>";

pub const STAGED: &str = "sandbox-uid.Dockerfile";
const BOUNDARY: &[u8] = b"FROM shared";
const UID_ARG: &[u8] = b"ARG TEHAS_UID=";
const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

pub fn main(given: &[String]) -> u8 {
    let done = match given {
        [verb, dir, mode]
            if verb == "recipe" && ["build", "if-moved", "check"].contains(&mode.as_str()) =>
        {
            recipe(Path::new(dir), mode == "build")
        }
        [verb, dir] if verb == "stage" => stage(Path::new(dir)),
        [verb, image, label, commit, what] if verb == "current" => {
            let (line, same) = current(image, label, commit, what);
            tehas_core::outln!("{line}");
            return if same { 0 } else { 1 };
        }
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    match done {
        Ok(()) => 0,
        Err(why) => {
            eprintln!("sandbox: {why}");
            1
        }
    }
}

/// The line `build.sh --check` prints, and whether the image is on `commit`.
pub fn current(image: &str, label: &str, commit: &str, what: &str) -> (String, bool) {
    if !commit.is_empty() && label == commit {
        return (format!("pi-rukas: {image} is on {commit}, {what}"), true);
    }
    let on = if label.is_empty() {
        "no known commit"
    } else {
        label
    };
    (
        format!("pi-rukas: {image} is on {on}, {what} is {commit}"),
        false,
    )
}

/// Whether the recipe can be cut: one boundary, one bare uid argument, and
/// no other declaration of it. Whole lines only, so an indented step or a
/// default after the `=` is refused rather than read one way here and
/// another by docker.
pub fn cuttable(text: &[u8]) -> bool {
    let lines = || text.split(|&b| b == b'\n');
    let count = |f: &dyn Fn(&[u8]) -> bool| lines().filter(|l| f(l)).count();
    count(&|l| l == BOUNDARY) == 1
        && count(&|l| l == UID_ARG) == 1
        && count(&|l| l.starts_with(b"ARG TEHAS_UID")) == 1
}

/// The copy docker is handed: the lines up to the boundary, the boundary
/// that opens the second stage, and the lines below it, each ending in a
/// newline. None when the recipe cannot be cut.
pub fn cut(text: &[u8]) -> Option<Vec<u8>> {
    if !cuttable(text) {
        return None;
    }
    let mut lines: Vec<&[u8]> = text.split(|&b| b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let at = lines.iter().position(|l| *l == BOUNDARY)?;
    let mut out = Vec::with_capacity(text.len() + 1);
    for l in lines[..at]
        .iter()
        .chain([&BOUNDARY])
        .chain(&lines[at + 1..])
    {
        out.extend_from_slice(l);
        out.push(b'\n');
    }
    Some(out)
}

/// Whether `name` is a temporary of the copy: the six characters of the
/// alphabet `mktemp XXXXXX` uses after the copy's name, the shape the
/// cleaner and .gitignore read too.
pub fn temp_shaped(name: &str) -> bool {
    name.strip_prefix(STAGED)
        .and_then(|s| s.strip_prefix('.'))
        .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn shape_refused(file: &Path) -> String {
    format!(
        "{} does not hold one 'FROM shared' stage and one 'ARG TEHAS_UID=' line, so the shared layers cannot be cut",
        file.display()
    )
}

fn recipe(dir: &Path, build: bool) -> Result<(), String> {
    let shown = dir.display();
    let dockerfile = dir.join("Dockerfile");
    if !dockerfile.is_file() {
        return Err(format!(
            "no Dockerfile beside {shown}, so the image cannot be built"
        ));
    }
    if !dir.join("base.Dockerfile").is_file() {
        return Err(format!(
            "no base.Dockerfile beside {shown}, so the base cannot be built"
        ));
    }
    let text = fs::read(&dockerfile)
        .map_err(|e| format!("could not read {}: {e}", dockerfile.display()))?;
    if !cuttable(&text) {
        return Err(shape_refused(&dockerfile));
    }
    // A link under the copy's name is written through and a directory stops
    // docker; a plain file is the copy of a run killed past its cleanup.
    let staged = dir.join(STAGED);
    match fs::symlink_metadata(&staged) {
        Ok(m) if !m.is_file() => {
            return Err(format!(
                "refusing to build, {} is already there and is not a plain file",
                staged.display()
            ))
        }
        Ok(m) => {
            let said = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| time::iso(d.as_secs() as i64))
                .unwrap_or_else(|| "unknown (stat failed)".into());
            let who = if build {
                "this build"
            } else {
                "the next build"
            };
            eprintln!(
                "sandbox: {} is the copy an earlier build left, written {said}; {who} replaces it",
                staged.display()
            );
        }
        Err(_) => {}
    }
    sweep(dir)
}

/// The temporaries of earlier runs have no owner but the run that made them.
/// One that is not a plain file is refused with the run, not removed: a name
/// does not prove who wrote it.
fn sweep(dir: &Path) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|e| format!("could not list {}: {e}", dir.display()))?;
    let mut left: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().is_some_and(temp_shaped))
        .map(|e| e.path())
        .collect();
    left.sort();
    if let Some(odd) = left
        .iter()
        .find(|p| !fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
    {
        return Err(format!(
            "refusing to build, {} is named like a build's temporary and is not a plain file",
            odd.display()
        ));
    }
    let kept = left.iter().filter(|p| fs::remove_file(p).is_err()).count();
    if kept > 0 {
        let names: Vec<String> = left.iter().map(|p| p.display().to_string()).collect();
        return Err(format!(
            "could not take back {}, the temporary an earlier build left",
            names.join(" ")
        ));
    }
    Ok(())
}

/// A new temporary beside the copy, made owner-only, in the shape the sweep
/// and the cleaner take back.
fn new_temp(dir: &Path) -> io::Result<(PathBuf, fs::File)> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut seed = nanos ^ (u64::from(std::process::id()) << 32);
    for _ in 0..100 {
        // splitmix64: enough to tell two tries apart, nothing secret rides on it.
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let suffix: String = (0..6)
            .map(|i| ALPHABET[((z >> (i * 8)) % ALPHABET.len() as u64) as usize] as char)
            .collect();
        let path = dir.join(format!("{STAGED}.{suffix}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free temporary name",
    ))
}

fn stage(dir: &Path) -> Result<(), String> {
    let shown = dir.display();
    let dockerfile = dir.join("Dockerfile");
    let text = fs::read(&dockerfile)
        .map_err(|e| format!("could not read {}: {e}", dockerfile.display()))?;
    let copy = cut(&text).ok_or_else(|| shape_refused(&dockerfile))?;
    let (tmp, mut f) =
        new_temp(dir).map_err(|_| format!("could not stage a recipe beside {shown}"))?;
    let done = (|| {
        f.write_all(&copy)
            .and_then(|()| f.flush())
            .map_err(|_| format!("could not write the staged recipe beside {shown}"))?;
        drop(f);
        // The copy is asked what the tracked file was, before docker is
        // asked anything of it.
        let written = fs::read(&tmp)
            .map_err(|_| format!("could not write the staged recipe beside {shown}"))?;
        if !cuttable(&written) {
            return Err(
                "the staged recipe does not hold one 'FROM shared' line and one 'ARG TEHAS_UID=' line".to_string(),
            );
        }
        let staged = dir.join(STAGED);
        fs::rename(&tmp, &staged)
            .map_err(|_| format!("could not put the staged recipe at {}", staged.display()))
    })();
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tehas_core::testdir::TestDir;

    const VALID: &str = "ARG PI_RUKAS_IMAGE\nFROM ${PI_RUKAS_IMAGE} AS shared\nRUN true\nFROM shared\nARG TEHAS_UID=\nRUN usermod -u \"$TEHAS_UID\" vscode\n";

    fn host(recipe: &str) -> TestDir {
        let d = TestDir::new("sandbox-recipe");
        fs::write(d.join("Dockerfile"), recipe).unwrap();
        fs::write(d.join("base.Dockerfile"), "FROM scratch\n").unwrap();
        d
    }

    #[test]
    fn a_valid_recipe_is_cut_and_staged_owner_only() {
        assert!(cuttable(VALID.as_bytes()));
        assert_eq!(cut(VALID.as_bytes()).unwrap(), VALID.as_bytes());
        let d = host(VALID);
        recipe(d.path(), true).unwrap();
        stage(d.path()).unwrap();
        let staged = d.join(STAGED);
        assert_eq!(fs::read_to_string(&staged).unwrap(), VALID);
        assert_eq!(
            fs::metadata(&staged).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let names: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 3, "no temporary left: {names:?}");
    }

    #[test]
    fn a_recipe_without_a_trailing_newline_gets_one() {
        let text = VALID.trim_end();
        assert_eq!(cut(text.as_bytes()).unwrap(), VALID.as_bytes());
    }

    #[test]
    fn a_recipe_with_no_boundary_is_refused() {
        for text in [
            VALID.replace("FROM shared\n", ""),
            VALID.replace("FROM shared\n", "  FROM shared\n"),
            VALID.replace("FROM shared\n", "FROM shar+ed\n"),
        ] {
            assert!(cut(text.as_bytes()).is_none(), "{text}");
            let d = host(&text);
            let why = recipe(d.path(), true).unwrap_err();
            assert!(
                why.contains("does not hold one 'FROM shared' stage"),
                "{why}"
            );
            assert!(stage(d.path()).is_err());
            assert!(!d.join(STAGED).exists());
        }
    }

    #[test]
    fn a_recipe_with_two_boundaries_is_refused() {
        let text = VALID.replace("FROM shared\n", "FROM shared\nRUN true\nFROM shared\n");
        assert!(!cuttable(text.as_bytes()));
        assert!(recipe(host(&text).path(), true).is_err());
    }

    #[test]
    fn a_recipe_without_the_bare_uid_argument_is_refused() {
        for text in [
            VALID.replace("ARG TEHAS_UID=\n", ""),
            VALID.replace("ARG TEHAS_UID=\n", "ARG TEHAS_UID=1000\n"),
            VALID.replace("ARG TEHAS_UID=\n", "ARG TEHAS_UID\n"),
            VALID.replace("ARG TEHAS_UID=\n", "ARG TEHAS_UID=\nARG TEHAS_UID=\n"),
            VALID.replace("ARG TEHAS_UID=\n", "ARG TEHAS_UID=\nARG TEHAS_UIDX\n"),
        ] {
            assert!(!cuttable(text.as_bytes()), "{text}");
        }
    }

    #[test]
    fn a_staged_copy_that_is_a_link_is_refused_and_kept() {
        let d = host(VALID);
        let target = d.join("foreign");
        fs::write(&target, "kept\n").unwrap();
        std::os::unix::fs::symlink(&target, d.join(STAGED)).unwrap();
        let why = recipe(d.path(), true).unwrap_err();
        assert!(
            why.contains("is already there and is not a plain file"),
            "{why}"
        );
        assert!(fs::symlink_metadata(d.join(STAGED))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "kept\n");
    }

    #[test]
    fn a_staged_copy_that_is_a_directory_is_refused() {
        let d = host(VALID);
        fs::create_dir(d.join(STAGED)).unwrap();
        assert!(recipe(d.path(), false).is_err());
        assert!(d.join(STAGED).is_dir());
    }

    #[test]
    fn a_left_copy_is_replaced_and_old_temporaries_are_taken_back() {
        let d = host(VALID);
        fs::write(d.join(STAGED), "FROM whatever\n").unwrap();
        fs::write(d.join(format!("{STAGED}.123456")), "old\n").unwrap();
        recipe(d.path(), true).unwrap();
        assert!(!d.join(format!("{STAGED}.123456")).exists());
        stage(d.path()).unwrap();
        assert_eq!(fs::read_to_string(d.join(STAGED)).unwrap(), VALID);
    }

    #[test]
    fn a_link_named_like_a_temporary_is_refused_not_removed() {
        let d = host(VALID);
        let link = d.join(format!("{STAGED}.abcdef"));
        std::os::unix::fs::symlink(d.join("nowhere"), &link).unwrap();
        let why = recipe(d.path(), true).unwrap_err();
        assert!(why.contains("is named like a build's temporary"), "{why}");
        assert!(fs::symlink_metadata(&link).is_ok());
    }

    #[test]
    fn a_missing_recipe_is_refused() {
        let d = host(VALID);
        fs::remove_file(d.join("Dockerfile")).unwrap();
        assert!(recipe(d.path(), true)
            .unwrap_err()
            .contains("no Dockerfile beside"));
        let d = host(VALID);
        fs::remove_file(d.join("base.Dockerfile")).unwrap();
        assert!(recipe(d.path(), true)
            .unwrap_err()
            .contains("no base.Dockerfile beside"));
    }

    #[test]
    fn temporaries_have_the_mktemp_shape() {
        assert!(temp_shaped("sandbox-uid.Dockerfile.aB3xZ9"));
        for n in [
            "sandbox-uid.Dockerfile",
            "sandbox-uid.Dockerfile.",
            "sandbox-uid.Dockerfile.abcde",
            "sandbox-uid.Dockerfile.toolong",
            "sandbox-uid.Dockerfile.a-b_c",
            "sandbox-uid.Dockerfilex123456",
        ] {
            assert!(!temp_shaped(n), "{n}");
        }
        let d = TestDir::new("sandbox-temp");
        let (p, _) = new_temp(d.path()).unwrap();
        assert!(temp_shaped(p.file_name().unwrap().to_str().unwrap()));
    }

    #[test]
    fn the_image_is_current_only_on_the_commit_followed() {
        let c = "a".repeat(40);
        let (line, same) = current("tehas/pi-rukas:uid1000", &c, &c, "release v1");
        assert!(same);
        assert_eq!(
            line,
            format!("pi-rukas: tehas/pi-rukas:uid1000 is on {c}, release v1")
        );
        let (line, same) = current("img", &"b".repeat(40), &c, "release v1");
        assert!(!same);
        assert!(line.ends_with(&format!(", release v1 is {c}")));
        let (line, same) = current("img", "", &c, "release v1");
        assert!(!same);
        assert!(line.contains("is on no known commit"));
        assert!(!current("img", "", "", "x").1);
    }
}
