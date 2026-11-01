//! `tehas export`: each cycle of the history after a cursor, masked, in a
//! directory of its own. Its text is src/usage/export.txt, which says what is
//! copied and every exit status. It runs under `libexec/with-settings`.
//!
//! Every file goes through the mask, with the names of `mask::host_names`. A
//! file of a Protolab page that is not UTF-8 is an image, a video or the like,
//! which the mask would only corrupt: it is copied as it is. Anything else is
//! masked whatever bytes it holds. A link is not followed and a file over
//! 32 MiB is left out.

use crate::history::{self, Cursor, Query};
use crate::jq::J;
use crate::review::git_global;
use crate::store;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tehas_core::{mask, models};

const USAGE: &str = "usage: tehas export [--json] --after CURSOR <dir>";

const BIGGEST: u64 = 32 * 1024 * 1024;

/// What the arguments ask.
#[derive(Debug, PartialEq)]
pub struct Asked {
    json: bool,
    after: Cursor,
    dir: String,
}

/// A name that says nothing of where it leads. U+FFFD is how a word that was
/// not text reaches `check`.
fn refused_name(dir: &str) -> bool {
    matches!(dir, "." | "..")
        || dir.ends_with("/.")
        || dir.ends_with("/..")
        || dir.starts_with("./")
        || dir.starts_with("../")
        || dir.contains('\u{fffd}')
}

/// What to do, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let bad = || {
        eprintln!("{USAGE}");
        64
    };
    let (mut json, mut after, mut dir) = (false, None, "");
    let mut given = given.iter();
    while let Some(word) = given.next() {
        match word.as_str() {
            "--json" if !json => json = true,
            "--after" if after.is_none() => {
                after = given.next().and_then(|c| Cursor::parse(c));
                after.as_ref().ok_or_else(bad)?;
            }
            w if w.starts_with('-') || !dir.is_empty() || refused_name(w) => return Err(bad()),
            w => dir = w,
        }
    }
    match after {
        Some(after) if !dir.is_empty() => Ok(Asked {
            json,
            after,
            dir: dir.to_string(),
        }),
        _ => Err(bad()),
    }
}

/// Whether the name itself, or its cycles/, is in the home. This compares the
/// parts of the name, so a doubled slash is no way past it.
fn named_in(dir: &str, home: &Path) -> bool {
    let dir = Path::new(dir);
    dir.starts_with(home) || dir.join("cycles").starts_with(home)
}

/// Where a path comes to, what is not there yet taken as named: a link made by
/// hand may lead into the home before the directory is made under it.
fn comes_to(path: &Path) -> Option<PathBuf> {
    let mut left = Vec::new();
    let mut at = path;
    loop {
        let here = if at.as_os_str().is_empty() {
            Path::new(".")
        } else {
            at
        };
        if let Ok(real) = fs::canonicalize(here) {
            return Some(left.iter().rev().fold(real, |p, name| p.join(name)));
        }
        left.push(at.file_name()?);
        at = at.parent()?;
    }
}

fn same_place(a: &Path, b: &Path) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// The working directory by the name the shell gave it, not its real path.
fn working_directory() -> PathBuf {
    let real = env::current_dir().unwrap_or_default();
    match env::var_os("PWD").map(PathBuf::from) {
        Some(named) if named.is_absolute() && same_place(&named, &real) => named,
        _ => real,
    }
}

/// A directory's path as `cd` and `pwd` give it: `.` gone, `..` taken off by
/// name, and links kept. Where that is not the directory, its real path.
fn logical(dir: &str, real: &Path) -> PathBuf {
    let mut at = if dir.starts_with('/') {
        PathBuf::from("/")
    } else {
        working_directory()
    };
    for part in dir.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                at.pop();
            }
            _ => at.push(part),
        }
    }
    if same_place(&at, real) {
        at
    } else {
        real.to_path_buf()
    }
}

/// The directory to export into, made; or the line that says why not. Never
/// the factory's home: copies there would stand where the dashboard and the
/// units look for the originals. Both the name given and where it leads are
/// compared, before the directory is made and after.
fn target(dir: &str, home: &Path) -> Result<PathBuf, String> {
    let in_home = || {
        format!(
            "{dir} is in the factory's home ({}), choose another directory",
            home.display()
        )
    };
    let real_home = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    if named_in(dir, home) || comes_to(Path::new(dir)).is_some_and(|p| p.starts_with(&real_home)) {
        return Err(in_home());
    }
    if fs::create_dir_all(dir).is_err() {
        return Err(format!("could not make {dir}"));
    }
    let entered = fs::metadata(Path::new(dir).join("."));
    let Some(real) = entered.ok().and_then(|_| fs::canonicalize(dir).ok()) else {
        return Err(format!("{dir} is not a directory"));
    };
    if real.starts_with(&real_home) {
        return Err(in_home());
    }
    Ok(logical(dir, &real))
}

pub fn main(given: &[String]) -> u8 {
    let asked = match words(given) {
        Ok(asked) => asked,
        Err(rc) => return rc,
    };
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    if !store::ready(&home) {
        eprintln!("tehas export: ERROR: the state store is not installed (run install.sh)");
        return 78;
    }
    let to = match target(&asked.dir, &home) {
        Ok(to) => to,
        Err(said) => {
            eprintln!("tehas export: {said}");
            return 64;
        }
    };
    let query = Query {
        after: Some(asked.after),
        issue: None,
    };
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let listing = match history::listing(&home, &repo, &query) {
        Ok(listing) => listing,
        Err(no) => {
            eprintln!("{}", no.said);
            return no.rc;
        }
    };
    // Without a name the mask still takes every token-shaped string, so a
    // file of models that cannot be read stops nothing.
    let names = mask::host_names(&models::host_path(), &git_global("user.email"));
    if let Err(e) = mask::set_private(&names) {
        eprintln!("{e}");
        return 1;
    }
    let user = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    let run = Export {
        home,
        runs: user.join(".pi/agent/ensemble-runs"),
        cycles: to.join("cycles"),
    };
    let lines: Vec<String> = listing.cycles.iter().map(J::dump).collect();
    let mut printed = String::new();
    let mut ok = make_dir(&to) && make_dir(&run.cycles);
    if ok {
        let (copied, every) = run.all(&lines);
        ok = every;
        if !asked.json {
            printed = text(&lines, copied, &to, listing.next.as_deref());
        }
    } else {
        eprintln!("tehas export: could not make {}", run.cycles.display());
    }
    if asked.json {
        let mut object = listing.object();
        if let J::Obj(fields) = &mut object {
            fields.push(("to".to_string(), J::str(to.to_string_lossy())));
        }
        printed = object.dump() + "\n";
    }
    let mut out = io::stdout();
    let written = out.write_all(printed.as_bytes()).and_then(|()| out.flush());
    u8::from(!ok || written.is_err())
}

/// What the command prints without `--json`: a line for each cycle and one for
/// the directory, with the cursor to keep.
fn text(lines: &[String], copied: usize, to: &Path, next: Option<&str>) -> String {
    let mut printed = String::new();
    for line in lines {
        let v: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        printed.push_str(&format!(
            "{}  {}  #{}\n",
            raw(&v["cursor"]),
            raw(&v["kind"]),
            raw(&v["issue"])
        ));
    }
    printed.push_str(&format!(
        "exported  {copied} cycles to {} (next: {})\n",
        to.display(),
        next.unwrap_or("none")
    ));
    printed
}

/// A value as jq -r prints it.
fn raw(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The history makes the cursor, of digits and dashes only.
fn cursor_shaped(c: &str) -> bool {
    let parts: Vec<&str> = c.split('-').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && !parts[1].starts_with('0')
        && !parts[2].starts_with('0')
}

/// A directory and every level of it this makes, 0755 whatever the umask: an
/// archive is for somebody else to open.
fn make_dir(d: &Path) -> bool {
    if d.is_dir() {
        let _ = fs::set_permissions(d, fs::Permissions::from_mode(0o755));
        return true;
    }
    if let Some(up) = d.parent() {
        if !up.as_os_str().is_empty() && !up.is_dir() && !make_dir(up) {
            return false;
        }
    }
    (fs::create_dir(d).is_ok() || d.is_dir())
        && fs::set_permissions(d, fs::Permissions::from_mode(0o755)).is_ok()
}

fn plain_file(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file())
}

/// The bytes as they go into the archive: masked, unless `raw_ok` and they
/// are not text.
fn masked(bytes: Vec<u8>, raw_ok: bool) -> Vec<u8> {
    match String::from_utf8(bytes) {
        Ok(text) => mask::mask(&text).into_bytes(),
        Err(e) if raw_ok => e.into_bytes(),
        Err(e) => mask::mask(&String::from_utf8_lossy(e.as_bytes())).into_bytes(),
    }
}

/// Writes <dir>/<name> whole, 0644: through a file beside it, so a copy cut
/// short never stands under the name.
fn put(dir: &Path, name: &str, bytes: &[u8]) -> bool {
    if !make_dir(dir) {
        return false;
    }
    let part = dir.join(format!(".tehas-masked.{}", std::process::id()));
    let to = dir.join(name);
    let done = fs::write(&part, bytes).is_ok()
        && fs::set_permissions(&part, fs::Permissions::from_mode(0o644)).is_ok()
        && fs::rename(&part, &to).is_ok();
    if !done {
        let _ = fs::remove_file(&part);
    }
    done
}

/// One file into <dir>/<name>. A file that is not there, is a link or is over
/// 32 MiB is no error and is not copied.
fn copy_in(from: &Path, dir: &Path, name: &str, raw_ok: bool) -> bool {
    let Ok(meta) = fs::symlink_metadata(from) else {
        return true;
    };
    if !meta.file_type().is_file() || meta.len() > BIGGEST {
        return true;
    }
    match fs::read(from) {
        Ok(bytes) => put(dir, name, &masked(bytes, raw_ok)),
        Err(_) => false,
    }
}

/// The plain files of a directory, not its links and not its dot names, in
/// the order of their names.
fn files_of(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.as_encoded_bytes().starts_with(b"."))
        })
        .filter(|p| plain_file(p))
        .collect();
    found.sort();
    found
}

/// Every plain file under a directory, as find -type f finds them: links are
/// neither followed nor taken.
fn tree(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            tree(&entry.path(), found);
        } else if kind.is_file() {
            found.push(entry.path());
        }
    }
}

/// Seconds since 1970 of a time as the history writes it; 0 when it is not.
fn epoch(v: &Value) -> f64 {
    v.as_str()
        .and_then(crate::snapshot::epoch)
        .map_or(0.0, |t| t as f64)
}

struct Export {
    home: PathBuf,
    runs: PathBuf,
    cycles: PathBuf,
}

impl Export {
    /// Every cycle, each one history line: how many were copied, and whether
    /// all were. One that could not be written is said on standard error.
    fn all(&self, lines: &[String]) -> (usize, bool) {
        let mut copied = 0;
        let mut failed = String::new();
        for line in lines {
            match self.cycle(line) {
                Ok(()) => copied += 1,
                Err(Some(cursor)) => failed.push_str(&format!(" {cursor}")),
                Err(None) => failed.push_str(" <unreadable>"),
            }
        }
        if !failed.is_empty() {
            eprintln!("tehas export: could not write{failed}");
        }
        (copied, failed.is_empty())
    }

    /// One cycle: Err with its cursor when it could not be written, with
    /// None when the line holds no cursor.
    fn cycle(&self, line: &str) -> Result<(), Option<String>> {
        let v: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        let cursor = v["cursor"]
            .as_str()
            .filter(|c| cursor_shaped(c))
            .ok_or(None)?;
        let failed = || Some(cursor.to_string());
        // A number names the files of the issue, and nothing else may.
        let issue = raw(&v["issue"]);
        if issue.is_empty() || !issue.bytes().all(|b| b.is_ascii_digit()) {
            return Err(failed());
        }
        let out = self.cycles.join(cursor);
        // An old copy is replaced whole, so a line that has since lost a field
        // cannot leave its value behind.
        if fs::symlink_metadata(&out).is_ok() {
            let gone = if out.is_dir() && !out.is_symlink() {
                fs::remove_dir_all(&out)
            } else {
                fs::remove_file(&out)
            };
            gone.map_err(|_| failed())?;
        }
        if !put(&out, "history.json", format!("{line}\n").as_bytes()) {
            return Err(failed());
        }
        let mut ok = true;
        if v["kind"] == "plan" {
            ok &= self
                .plan(&issue, epoch(&v["ended"]), &out)
                .ok_or_else(failed)?;
        } else {
            let state = self.home.join("state").join("cycles");
            let logs = self.home.join("logs");
            let into = out.join("logs");
            for (file, name) in [
                (state.join(format!("{issue}.result.json")), "result.json"),
                (
                    state.join(format!("{issue}.progress.json")),
                    "progress.json",
                ),
                (
                    state.join(format!("{issue}.manifest.json")),
                    "manifest.json",
                ),
                (
                    logs.join(format!("dispatch-work-{issue}.log")),
                    "dispatch-work.log",
                ),
            ] {
                ok &= copy_in(&file, &into, name, false);
            }
            // The runner's progress and its side streams, whatever stamp each
            // carries: a cycle started twice has more than one.
            let prefix = format!("work-{issue}-");
            for f in files_of(&logs) {
                let name = f.file_name().unwrap_or_default().to_string_lossy();
                if name.starts_with(&prefix)
                    && [".jsonl", ".jsonl.err", ".jsonl.console", ".jsonl.preflight"]
                        .iter()
                        .any(|end| name.ends_with(end))
                {
                    ok &= copy_in(&f, &into, &name, false);
                }
            }
            for f in files_of(&self.runs.join(format!("issue-{issue}"))) {
                let name = f.file_name().unwrap_or_default().to_string_lossy();
                ok &= copy_in(&f, &out.join("transcripts"), &name, false);
            }
        }
        let page = self.home.join("protolab").join(&issue);
        if page.is_dir() && !page.is_symlink() {
            let mut found = Vec::new();
            tree(&page, &mut found);
            for f in found {
                let name = f.file_name().unwrap_or_default().to_string_lossy();
                if name == ".stamp" {
                    continue;
                }
                let rel = f.strip_prefix(&page).unwrap_or(&f);
                let dir = out
                    .join("protolab")
                    .join(rel.parent().unwrap_or(Path::new("")));
                ok &= copy_in(&f, &dir, &name, true);
            }
        }
        if ok {
            Ok(())
        } else {
            Err(failed())
        }
    }

    /// A planning turn's session as the store has it, when it is the turn's:
    /// opened before the turn ended. The store keeps one session an issue, so
    /// one opened after is another turn's. None when the store did not answer.
    fn plan(&self, issue: &str, ended: f64, out: &Path) -> Option<bool> {
        let answer = match store::call(&self.home, &["plan-show", issue]) {
            Ok(v) => v,
            Err(no) => {
                if no.rc != 1 {
                    eprintln!(
                        "tehas export: ERROR: the state store cannot be used: {}",
                        no.line
                    );
                }
                return None;
            }
        };
        let session = &answer["planning"];
        let its =
            session.is_object() && session["opened_at"].as_f64().is_some_and(|at| at <= ended);
        if !its {
            return Some(true);
        }
        let text = serde_json::to_string_pretty(session).unwrap_or_default() + "\n";
        Some(put(out, "plan.json", &masked(text.into_bytes(), false)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("tehas-export-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("home/logs")).unwrap();
        dir
    }

    #[test]
    fn the_arguments_are_those_of_the_script() {
        let asked = words(&strings(&["--after", "5-1", "/a/b", "--json"])).unwrap();
        assert!(asked.json);
        assert_eq!(asked.after, Cursor::parse("5-1").unwrap());
        assert_eq!(asked.dir, "/a/b");
        assert!(!words(&strings(&["/a", "--after", "1-1-1"])).unwrap().json);
        // An empty word is no directory, and the one after it is taken.
        assert_eq!(
            words(&strings(&["--after", "1-1", "", "x"])).unwrap().dir,
            "x"
        );
        for dir in ["a/b", "a/../b", "a/", ".a", "..a", "a/.b", "...", "/"] {
            assert!(words(&strings(&["--after", "1-1", dir])).is_ok(), "{dir}");
        }
        for bad in [
            &[][..],
            &["--json"],
            &["--after", "1-1"],
            &["/a"],
            &["--json", "/a"],
            &["--after"],
            &["--after", "12", "/a"],
            &["--after", "1-0", "/a"],
            &["--after", "1-1-0", "/a"],
            &["--after", "1-01", "/a"],
            &["--after", "x", "/a"],
            &["--after", "12345678901-1", "/a"],
            &["--after", "1-1-1234567890123456", "/a"],
            &["--after", "1-1", "--after", "1-2", "/a"],
            &["--json", "--json", "--after", "1-1", "/a"],
            &["--after", "1-1", "/a", "/b"],
            &["--after", "1-1", "/a", ""],
            &["--after", "1-1", "-x"],
            &["--after", "1-1", "/a", "--help"],
            &["--after", "1-1", "."],
            &["--after", "1-1", ".."],
            &["--after", "1-1", "/a/."],
            &["--after", "1-1", "/a/.."],
            &["--after", "1-1", "./a"],
            &["--after", "1-1", "../a"],
            &["--after", "1-1", "/a/\u{fffd}"],
        ] {
            assert_eq!(words(&strings(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn the_name_of_the_home_or_of_anything_in_it_is_refused() {
        let home = Path::new("/x/home");
        for dir in [
            "/x/home",
            "/x/home/",
            "/x/home/a",
            "/x/home//a",
            "/x//home/a",
            "/x/home/.a",
        ] {
            assert!(named_in(dir, home), "{dir}");
        }
        for dir in ["/x/homes", "/x/home2/a", "/x", "/y/x/home", "home/a"] {
            assert!(!named_in(dir, home), "{dir}");
        }
        // Its cycles/ would be the home.
        assert!(named_in("/x", Path::new("/x/cycles")));
    }

    #[test]
    fn a_link_into_the_home_is_refused_and_makes_nothing() {
        let top = scratch("link");
        let home = top.join("home");
        std::os::unix::fs::symlink(&home, top.join("to-home")).unwrap();
        let link = top.join("to-home");
        let link = link.to_str().unwrap();
        for dir in [link.to_string(), format!("{link}/new/deeper")] {
            let no = target(&dir, &home).unwrap_err();
            assert!(no.contains("is in the factory's home"), "{no}");
        }
        assert!(!home.join("new").exists());
        // By a home that is itself a link, and to a level already there.
        let named = top.join("home-link");
        std::os::unix::fs::symlink(&home, &named).unwrap();
        let in_logs = home.join("logs");
        assert!(target(in_logs.to_str().unwrap(), &named).is_err());
        fs::remove_dir_all(&top).unwrap();
    }

    #[test]
    fn a_directory_is_made_and_a_file_is_refused() {
        let top = scratch("target");
        let home = top.join("home");
        let made = top.join("a/b");
        assert_eq!(target(made.to_str().unwrap(), &home).unwrap(), made);
        assert!(made.is_dir());
        let up_and_over = format!("{}/../c//", made.display());
        assert_eq!(target(&up_and_over, &home).unwrap(), top.join("a/c"));
        let other = top.join("other");
        fs::create_dir(&other).unwrap();
        let link = top.join("by-link");
        std::os::unix::fs::symlink(&other, &link).unwrap();
        // A link of the caller's keeps its name.
        assert_eq!(target(link.to_str().unwrap(), &home).unwrap(), link);
        let file = top.join("file");
        fs::write(&file, "").unwrap();
        let no = target(file.to_str().unwrap(), &home).unwrap_err();
        assert!(no.starts_with("could not make"), "{no}");
        fs::remove_dir_all(&top).unwrap();
    }

    #[test]
    fn a_path_not_made_yet_comes_to_where_its_levels_lead() {
        let top = scratch("comes");
        let real = fs::canonicalize(&top).unwrap();
        assert_eq!(comes_to(&top.join("no/such")), Some(real.join("no/such")));
        std::os::unix::fs::symlink(top.join("home"), top.join("l")).unwrap();
        assert_eq!(comes_to(&top.join("l/x")), Some(real.join("home/x")));
        fs::remove_dir_all(&top).unwrap();
    }

    /// Run alone with the settings a host would give, as the variables can
    /// only be set for a process of their own.
    #[test]
    fn the_names_are_the_aliases_every_role_s_endpoints_and_the_email() {
        let Some(models) = env::var_os("TEHAS_TEST_NAMES_FILE") else {
            let top = scratch("names");
            let file = top.join("models.json");
            fs::write(
                &file,
                r#"{"providers":{
                  "alpha":{"baseUrl":"https://user:pw@main.lan:8086/v1","backupBaseUrl":"http://backup.lan/v1"},
                  "beta":{"baseUrl":"http://[2001:db8::7]:8080/v1"},
                  "gamma":{"baseUrl":"bare.lan:1234","backupBaseUrl":7},
                  "unused":{"baseUrl":"http://unused.lan/v1"}}}"#,
            )
            .unwrap();
            let done = std::process::Command::new(env::current_exe().unwrap())
                .args([
                    "--exact",
                    "export::tests::the_names_are_the_aliases_every_role_s_endpoints_and_the_email",
                ])
                .env("TEHAS_TEST_NAMES_FILE", &file)
                .env(
                    "PI_ENSEMBLE_HOST_ALIASES",
                    "gpu-one:192.0.2.5,solo-host,x:y\nnot:this",
                )
                .env("PI_ENSEMBLE_SUBAGENT_PROVIDER", "alpha")
                .env("PI_ENSEMBLE_SUBAGENT_MODEL", "m1")
                .env("PI_ENSEMBLE_PROVIDER_OPS", "beta")
                .env("PI_ENSEMBLE_MODEL_OPS", "m2")
                .env("PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST", "gamma")
                .env("PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST", "m3")
                .status()
                .unwrap();
            fs::remove_dir_all(&top).unwrap();
            assert!(done.success());
            return;
        };
        assert_eq!(
            mask::host_names(models.to_str().unwrap(), "bot@mail.example"),
            [
                "gpu-one",
                "192.0.2.5",
                "solo-host",
                "main.lan",
                "backup.lan",
                "2001:db8::7",
                "bare.lan",
                "bot@mail.example"
            ]
        );
    }

    #[test]
    fn the_text_says_each_cycle_and_the_cursor_to_keep() {
        let lines = [r#"{"cursor":"5-7-2","kind":"plan","issue":7}"#.to_string()];
        assert_eq!(
            text(&lines, 1, Path::new("/a"), Some("5-7-2")),
            "5-7-2  plan  #7\nexported  1 cycles to /a (next: 5-7-2)\n"
        );
        assert_eq!(
            text(&[], 0, Path::new("/a"), None),
            "exported  0 cycles to /a (next: none)\n"
        );
    }

    #[test]
    fn a_cursor_is_three_numbers_and_the_last_two_never_start_with_naught() {
        assert!(cursor_shaped("1760700000-12-1"));
        assert!(cursor_shaped("0-1-1"));
        for bad in [
            "1760700000-12",
            "1-012-1",
            "1-1-0",
            "a-1-1",
            "1--1",
            "../1-1-1",
            "1-1-1-1",
        ] {
            assert!(!cursor_shaped(bad), "{bad}");
        }
    }

    #[test]
    fn text_is_masked_and_only_a_page_file_that_is_not_text_is_kept_as_it_is() {
        let token = "ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let text = format!("a \0 stray {token}\n").into_bytes();
        assert_eq!(masked(text.clone(), false), b"a \0 stray ghp_***\n");
        assert_eq!(masked(text, true), b"a \0 stray ghp_***\n");
        let mut image = vec![0x89, b'P', b'N', b'G', 0xff, 0xfe];
        image.extend_from_slice(token.as_bytes());
        assert_eq!(masked(image.clone(), true), image);
        let log = masked(image, false);
        assert!(String::from_utf8(log).unwrap().ends_with("ghp_***"));
    }

    #[test]
    fn jq_raw_prints_strings_bare_and_the_rest_as_json() {
        assert_eq!(raw(&Value::from("plan")), "plan");
        assert_eq!(raw(&Value::from(12)), "12");
        assert_eq!(raw(&Value::Null), "null");
    }
}
