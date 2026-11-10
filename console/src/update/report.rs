//! What install.sh says about a host without changing it.
//!
//!   console update retired
//!   console update report <root> <instance> <options> < <drift lines>
//!   console update notes <root> <options>
//!
//! `retired` prints the files an older tehas installed and this one removes,
//! one path under $TEHAS_HOME a line, from src/update/retired.list.
//!
//! `report` prints the --check report, one finding a line, and exits 1 when it
//! found something to do, 0 otherwise. `notes` prints the "note:" lines a real
//! run ends with for the slice and the sandbox image, and exits 0. $TEHAS_HOME
//! is the home. The options:
//!
//!   --slice <file>              the installed tehas.slice, compared with
//!                               <root>/host/slice/tehas.slice; left out, it
//!                               is not checked (inside the tests)
//!   --images <setting> <image>  TEHAS_WORK_IMAGES and PI_ENSEMBLE_IMAGE
//!
//! and for `report` only:
//!
//!   --in-test                   inside the tests: bootstrap.sh is not run
//!   --tests <suite>             run the tests: <suite>, else scripts/test.sh
//!   --drift <n>                 how many files differ; standard input says
//!                               which, one line each
//!   --shell-missing <bashrc>    ~/.bashrc does not read env.sh
//!   --store <refusal>           why the store would refuse; empty for none
//!   --login <file> <fix>        the login command and how to fix it; an empty
//!                               fix is nothing to fix
//!   --steps <console>           the console that reads the steps owed
//!
//! Exit 64 for any other words.

use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use super::{code, output, say, tell};

const RETIRED: &str = include_str!("retired.list");

const USAGE: &str =
    "console update: usage: retired | report <root> <instance> <options> | notes <root> <options>";

/// The paths of retired.list, in order.
pub(crate) fn retired() -> impl Iterator<Item = &'static str> {
    RETIRED
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

pub fn retired_verb(given: &[String]) -> u8 {
    if !given.is_empty() {
        tell(USAGE);
        return 64;
    }
    for path in retired() {
        say(path);
    }
    0
}

/// What the report and the notes are told.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Asked {
    pub root: PathBuf,
    pub instance: String,
    pub home: PathBuf,
    pub slice: Option<PathBuf>,
    pub images: Option<(String, String)>,
    pub in_test: bool,
    pub tests: Option<String>,
    pub drift: usize,
    pub drift_lines: Vec<String>,
    pub shell_missing: Option<String>,
    pub store: String,
    pub login: Option<(PathBuf, String)>,
    pub steps: Option<PathBuf>,
}

fn parse(given: &[String], report: bool) -> Option<Asked> {
    let mut a = Asked::default();
    let mut words = given.iter();
    a.root = PathBuf::from(words.next()?);
    if report {
        a.instance = words.next()?.clone();
    }
    while let Some(word) = words.next() {
        match word.as_str() {
            "--slice" => a.slice = Some(PathBuf::from(words.next()?)),
            "--images" => a.images = Some((words.next()?.clone(), words.next()?.clone())),
            "--in-test" if report => a.in_test = true,
            "--tests" if report => a.tests = Some(words.next()?.clone()),
            "--drift" if report => a.drift = words.next()?.parse().ok()?,
            "--shell-missing" if report => a.shell_missing = Some(words.next()?.clone()),
            "--store" if report => a.store = words.next()?.clone(),
            "--login" if report => {
                a.login = Some((PathBuf::from(words.next()?), words.next()?.clone()))
            }
            "--steps" if report => a.steps = Some(PathBuf::from(words.next()?)),
            _ => return None,
        }
    }
    Some(a)
}

fn home() -> PathBuf {
    match env::var_os("TEHAS_HOME").filter(|h| !h.is_empty()) {
        Some(home) => PathBuf::from(home),
        None => Path::new(&env::var_os("HOME").unwrap_or_default()).join("tehas"),
    }
}

pub fn report_verb(given: &[String]) -> u8 {
    let Some(mut asked) = parse(given, true) else {
        tell(USAGE);
        return 64;
    };
    asked.home = home();
    let mut lines = String::new();
    let _ = io::stdin().read_to_string(&mut lines);
    asked.drift_lines = lines.lines().map(str::to_string).collect();
    let (said, found) = report(&asked, &mut Real);
    for line in said {
        say(&line);
    }
    u8::from(found)
}

pub fn notes_verb(given: &[String]) -> u8 {
    let Some(mut asked) = parse(given, false) else {
        tell(USAGE);
        return 64;
    };
    asked.home = home();
    for line in notes(&asked, &mut Real) {
        say(&line);
    }
    0
}

/// What the report reads outside this host's files.
pub(crate) trait Outside {
    /// The image's `<tehas.rukas>|<tehas.rukas.images>` labels; None when
    /// there is no such image.
    fn labels(&mut self, image: &str) -> Option<String>;
    /// The exit status and what the program printed on both outputs, as one.
    fn merged(&mut self, program: &Path, args: &[&str]) -> (i32, String);
    /// The exit status and what the program printed on standard output.
    fn output(&mut self, program: &Path, args: &[&str]) -> (i32, String);
}

struct Real;

impl Outside for Real {
    fn labels(&mut self, image: &str) -> Option<String> {
        const LABELS: &str = r#"{{index .Config.Labels "tehas.rukas"}}|{{index .Config.Labels "tehas.rukas.images"}}"#;
        let (rc, out) = output(
            Command::new("docker")
                .args(["image", "inspect", "--format", LABELS, image])
                .stdin(Stdio::null())
                .stderr(Stdio::null()),
        );
        (rc == 0).then_some(out)
    }

    fn merged(&mut self, program: &Path, args: &[&str]) -> (i32, String) {
        let Ok((mut from, to)) = io::pipe() else {
            return (127, String::new());
        };
        let child = {
            let mut cmd = Command::new(program);
            cmd.args(args).stdin(Stdio::null());
            match to.try_clone() {
                Ok(other) => cmd.stdout(other).stderr(to),
                Err(_) => return (127, String::new()),
            };
            cmd.spawn()
        };
        let Ok(mut child) = child else {
            return (127, String::new());
        };
        let mut out = Vec::new();
        let _ = from.read_to_end(&mut out);
        let rc = child.wait().map_or(127, code);
        let out = String::from_utf8_lossy(&out);
        (rc, out.trim_end_matches('\n').to_string())
    }

    fn output(&mut self, program: &Path, args: &[&str]) -> (i32, String) {
        output(Command::new(program).args(args).stdin(Stdio::null()))
    }
}

/// Each line of `out`, as `printf '%s\n' "$out" | sed 's/^/  /'` gives it.
fn indented(out: &str) -> impl Iterator<Item = String> + '_ {
    out.split('\n').map(|l| format!("  {l}"))
}

#[derive(Debug, PartialEq)]
enum Slice {
    Ok,
    Missing,
    Differs,
}

fn slice_state(root: &Path, installed: &Path) -> Slice {
    if !installed.is_file() {
        return Slice::Missing;
    }
    let shipped = fs::read(root.join("host/slice/tehas.slice"));
    match (shipped, fs::read(installed)) {
        (Ok(a), Ok(b)) if a == b => Slice::Ok,
        _ => Slice::Differs,
    }
}

/// TEHAS_WORK_IMAGES=1 promises a cycle the images its issue links, and the
/// sandbox image says by a label its build writes whether its pi-rukas reads
/// them. A disagreement is said, not failed. The status and the sentence, or
/// nothing: the setting is off, the image reads them, or there is no image.
fn images(asked: &Asked, o: &mut impl Outside) -> Option<(&'static str, String)> {
    let (setting, image) = asked.images.as_ref()?;
    if setting != "1" {
        return None;
    }
    let labels = o.labels(image)?;
    let said = labels.split_once('|').map_or(labels.as_str(), |(_, i)| i);
    match said {
        "1" => None,
        "0" => Some(("NOT READ", format!("TEHAS_WORK_IMAGES is 1 and the sandbox image {image} says its pi-rukas does not read the images a cycle is given. The dispatcher holds every cycle while that stands, not only an issue whose text links a picture: rebuild the image on a pi-rukas that reads them (tehas update), or set TEHAS_WORK_IMAGES=0."))),
        _ => Some(("NOT SAID", format!("TEHAS_WORK_IMAGES is 1 and the sandbox image {image} does not say whether its pi-rukas reads the images a cycle is given. Rebuild it with {}, which records the answer.", asked.home.join("sandbox/build.sh").display()))),
    }
}

/// A value as jq's string interpolation writes it.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// jq's `a // b`.
fn or<'a>(a: &'a Value, b: &'a Value) -> &'a Value {
    if matches!(a, Value::Null | Value::Bool(false)) {
        b
    } else {
        a
    }
}

fn json(file: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(file).ok()?).ok()
}

/// What the factory's health last said of the sandbox image (checks.rukas).
#[derive(Debug, PartialEq)]
enum Sandbox {
    /// The image it read is behind: the image and the commit followed.
    Behind(String, String),
    /// The image is not the one it read: an update built it since.
    Rebuilt,
}

/// The health is written by the status run of a poll, so it can be older
/// than an image an update has just built. `label` is the tehas.rukas label
/// of the image now, when it has one: an image the health did not read is
/// not called behind on its word.
fn behind(home: &Path, label: Option<&str>) -> Option<Sandbox> {
    let health = json(&home.join("live/health.json"))?;
    let rukas = health.get("checks")?.get("rukas")?;
    if rukas.get("behind") != Some(&Value::Bool(true)) {
        return None;
    }
    let read = rukas.get("image").and_then(Value::as_str);
    if let (Some(now), Some(read)) = (label.filter(|l| !l.is_empty()), read) {
        if now != read {
            return Some(Sandbox::Rebuilt);
        }
    }
    let field = |k: &str| rukas.get(k).unwrap_or(&Value::Null);
    Some(Sandbox::Behind(
        text(or(field("imageVersion"), field("image"))),
        text(or(field("targetVersion"), field("target"))),
    ))
}

/// The last tehas check-product, read and not run.
fn product(file: &Path) -> Option<Vec<String>> {
    let done = json(file)?;
    let failures = done.get("failures")?.as_array()?;
    let ok = !matches!(
        done.get("ok"),
        None | Some(Value::Null | Value::Bool(false))
    );
    let verdict = if ok {
        "OK".to_string()
    } else {
        format!("{} TO FIX", failures.len())
    };
    let at = text(done.get("at").unwrap_or(&Value::Null));
    let mut lines = vec![format!(
        "Product:      {verdict} (tehas check-product, {at})"
    )];
    lines.extend(failures.iter().map(|f| format!("  {}", text(f))));
    Some(lines)
}

/// The --check report, and whether it found something to do.
pub(crate) fn report(a: &Asked, o: &mut impl Outside) -> (Vec<String>, bool) {
    let mut lines = vec![format!("Instance:     {} OK", a.instance)];
    let mut found = false;

    let bootstrap = a.root.join("bootstrap.sh");
    if a.in_test {
        lines.push("Dependencies: not checked (running inside the tests)".into());
    } else if executable(&bootstrap) {
        let (rc, out) = o.merged(&bootstrap, &["check"]);
        if rc == 0 {
            lines.push("Dependencies: OK".into());
        } else {
            lines.push("Dependencies: NOT OK".into());
            lines.extend(indented(&out));
            found = true;
        }
    } else {
        lines.push("Dependencies: not checked (no bootstrap.sh in this checkout)".into());
    }

    // Every container runs in the slice, and only root can install it.
    match a.slice.as_deref().map(|s| slice_state(&a.root, s)) {
        Some(Slice::Ok) => lines.push("Slice:        OK".into()),
        Some(Slice::Missing) => {
            lines.push("Slice:        MISSING (sudo ./bootstrap.sh slice)".into());
            found = true;
        }
        Some(Slice::Differs) => {
            lines.push("Slice:        DIFFERS (sudo ./bootstrap.sh slice)".into());
            found = true;
        }
        None => lines.push("Slice:        not checked (running inside the tests)".into()),
    }

    if let Some((status, sentence)) = images(a, o) {
        lines.push(format!("Images:       {status}"));
        lines.push(format!("  {sentence}"));
    }

    let label = a.images.as_ref().and_then(|(_, image)| o.labels(image));
    let label = label
        .as_deref()
        .map(|l| l.split_once('|').map_or(l, |(r, _)| r));
    match behind(&a.home, label) {
        Some(Sandbox::Behind(image, target)) => {
            lines.push(format!("Sandbox:      BEHIND (the image runs pi-rukas {image}, the host follows {target}; tehas update builds it)"));
            found = true;
        }
        // Not found: the next status run reads the new image, and the
        // health says behind again if it is.
        Some(Sandbox::Rebuilt) => lines.push(
            "Sandbox:      NOT READ (the image was built after the last status run; the next one reads it)".into(),
        ),
        None => {}
    }

    // The product is not this host's install: it does not count as found.
    let checked = a.home.join("state/check-product.json");
    if fs::File::open(&checked).is_ok() {
        match product(&checked) {
            Some(said) => lines.extend(said),
            None => lines.push(format!("Product:      NOT READ ({})", checked.display())),
        }
    }

    if a.drift == 0 {
        lines.push("Installed:    OK".into());
    } else {
        lines.push(format!("Installed:    DRIFT ({} files)", a.drift));
        lines.extend(indented(&a.drift_lines.join("\n")));
        found = true;
    }

    // Root's to install, so no run of install.sh clears it: named apart.
    if let Some(bashrc) = &a.shell_missing {
        lines.push(format!(
            "Shell:        MISSING ({bashrc} does not source {}; ./install.sh puts the line at its top)",
            a.home.join("env.sh").display()
        ));
        found = true;
    } else {
        lines.push("Shell:        OK".into());
    }
    if a.home.join("state/first-install").is_file() {
        lines
            .push("First install: NOT DONE (labels and core units; ./install.sh does them)".into());
        found = true;
    }
    if !a.store.is_empty() {
        lines.push(format!(
            "Store:        REFUSED ({}; ./install.sh stops before it writes)",
            a.store
        ));
        found = true;
    }
    if let Some((bin, fix)) = a.login.as_ref().filter(|(_, fix)| !fix.is_empty()) {
        lines.push("Login:        NOT CURRENT".into());
        let how = if bin.exists() { "differs" } else { "missing" };
        lines.push(format!("  {how}: {}   ({fix})", bin.display()));
        found = true;
    }

    // A follow-up an earlier run never finished is still owed, and a record
    // this account cannot read is no answer of "nothing owed".
    let owed = a.home.join("state/update-pending");
    if owed.exists() {
        let (rc, said) = match &a.steps {
            Some(steps) => o.output(steps, &["update", "steps", "owed"]),
            None => (64, String::new()),
        };
        if rc != 0 {
            lines.push(format!(
                "Follow-ups:   NOT DONE (the record {} cannot be read)",
                owed.display()
            ));
            found = true;
        } else if !said.is_empty() {
            lines.push(format!(
                "Follow-ups:   NOT DONE ({} from an earlier run, see {})",
                said.split('\n').count(),
                owed.display()
            ));
            found = true;
        }
    }

    match &a.tests {
        None => lines.push("Tests:        not run (./install.sh --check --tests runs them)".into()),
        Some(suite) if suite.is_empty() && a.in_test => {
            lines.push("Tests:        not checked (running inside the tests)".into())
        }
        Some(suite) => {
            let suite = match suite.as_str() {
                "" => a.root.join("scripts/test.sh"),
                given => PathBuf::from(given),
            };
            let (rc, out) = o.merged(&suite, &[]);
            if rc == 0 {
                lines.push("Tests:        OK".into());
            } else {
                lines.push("Tests:        FAILED".into());
                let all: Vec<&str> = out.split('\n').collect();
                let tail = all[all.len().saturating_sub(30)..].join("\n");
                lines.extend(indented(&tail));
                found = true;
            }
        }
    }
    (lines, found)
}

/// The notes a real run ends with: the slice and the sandbox image.
pub(crate) fn notes(a: &Asked, o: &mut impl Outside) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(slice) = &a.slice {
        let at = slice.display();
        match slice_state(&a.root, slice) {
            Slice::Missing => lines.push(format!(
                "note: {at} is missing (sudo ./bootstrap.sh slice)"
            )),
            Slice::Differs => lines.push(format!("note: {at} differs from host/slice/tehas.slice in this checkout (sudo ./bootstrap.sh slice)")),
            Slice::Ok => {}
        }
    }
    if let Some((_, sentence)) = images(a, o) {
        lines.push(format!("note: {sentence}"));
    }
    lines
}

fn executable(file: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(file).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{dir, Dir};
    use super::*;
    use std::collections::{BTreeMap, HashSet};
    use std::os::unix::fs::PermissionsExt;

    /// The outside world as a test sets it.
    #[derive(Default)]
    struct Fake {
        labels: Option<String>,
        ran: BTreeMap<PathBuf, (i32, String)>,
        asked: Vec<PathBuf>,
    }

    impl Outside for Fake {
        fn labels(&mut self, _image: &str) -> Option<String> {
            self.labels.clone()
        }
        fn merged(&mut self, program: &Path, _args: &[&str]) -> (i32, String) {
            self.asked.push(program.to_path_buf());
            self.ran
                .get(program)
                .cloned()
                .unwrap_or((127, String::new()))
        }
        fn output(&mut self, program: &Path, args: &[&str]) -> (i32, String) {
            self.merged(program, args)
        }
    }

    fn words(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
    }

    // A checkout and a home, and the report of a host where all is well.
    fn host() -> (Dir, Asked) {
        let d = dir("report");
        d.file("root/host/slice/tehas.slice", "[Slice]\n");
        fs::create_dir_all(d.0.join("home/state")).unwrap();
        let asked = Asked {
            root: d.0.join("root"),
            instance: "/i".into(),
            home: d.0.join("home"),
            in_test: true,
            ..Asked::default()
        };
        (d, asked)
    }

    fn said(a: &Asked, o: &mut Fake) -> (String, bool) {
        let (lines, found) = report(a, o);
        (lines.join("\n"), found)
    }

    #[test]
    fn the_retired_files_are_paths_under_the_home_that_this_tree_no_longer_ships() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut seen = HashSet::new();
        for path in retired() {
            assert!(seen.insert(path), "{path} is listed twice");
            assert!(
                !path.starts_with('/') && !path.split('/').any(|p| p == ".." || p.is_empty()),
                "{path} is not a plain path under the home"
            );
            // An installed file comes from the top of the tree or from host/.
            for shipped in [root.join(path), root.join("host").join(path)] {
                assert!(
                    !shipped.exists(),
                    "{path} is retired, and {} ships it",
                    shipped.display()
                );
            }
        }
        assert!(seen.len() > 60);
        assert_eq!(retired().next(), Some("bin/tehas-console.ts"));
        assert!(seen.contains("libexec/github-token"));
    }

    #[test]
    fn the_options_are_read_and_a_wrong_one_is_no_answer() {
        let a = parse(
            &words(&[
                "/r",
                "/i",
                "--slice",
                "/etc/s",
                "--images",
                "1",
                "img",
                "--in-test",
                "--tests",
                "",
                "--drift",
                "2",
                "--shell-missing",
                "/b",
                "--store",
                "why",
                "--login",
                "/l",
                "fix",
                "--steps",
                "/c",
            ]),
            true,
        )
        .unwrap();
        assert_eq!(a.slice, Some("/etc/s".into()));
        assert_eq!(a.images, Some(("1".into(), "img".into())));
        assert_eq!(
            (a.in_test, a.tests.as_deref(), a.drift),
            (true, Some(""), 2)
        );
        assert_eq!(a.login, Some(("/l".into(), "fix".into())));
        assert_eq!(a.steps, Some("/c".into()));
        assert_eq!(parse(&words(&["/r", "/i", "--drift", "two"]), true), None);
        assert_eq!(parse(&words(&["/r", "/i", "--images", "1"]), true), None);
        assert_eq!(parse(&words(&["/r"]), true), None);
        // The notes take only the slice and the images.
        assert!(parse(&words(&["/r", "--slice", "/s"]), false).is_some());
        assert_eq!(parse(&words(&["/r", "--in-test"]), false), None);
    }

    #[test]
    fn a_host_with_nothing_to_do_says_six_lines_and_finds_nothing() {
        let (_d, a) = host();
        assert_eq!(
            said(&a, &mut Fake::default()),
            (
                "Instance:     /i OK
Dependencies: not checked (running inside the tests)
Slice:        not checked (running inside the tests)
Installed:    OK
Shell:        OK
Tests:        not run (./install.sh --check --tests runs them)"
                    .into(),
                false
            )
        );
    }

    #[test]
    fn dependencies_are_bootstrap_shs_check_with_its_words_indented() {
        let (d, mut a) = host();
        a.in_test = false;
        let (out, _) = said(&a, &mut Fake::default());
        assert!(out.contains("Dependencies: not checked (no bootstrap.sh in this checkout)"));
        let bootstrap = d.file("root/bootstrap.sh", "");
        fs::set_permissions(&bootstrap, fs::Permissions::from_mode(0o755)).unwrap();
        let mut o = Fake::default();
        o.ran.insert(bootstrap.clone(), (0, String::new()));
        assert!(said(&a, &mut o).0.contains("Dependencies: OK\nSlice:"));
        o.ran
            .insert(bootstrap, (1, "missing: jq\nmissing: gh".into()));
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Dependencies: NOT OK\n  missing: jq\n  missing: gh\nSlice:"));
        assert!(found);
    }

    #[test]
    fn the_slice_is_ok_missing_or_differs_from_the_one_this_checkout_ships() {
        let (d, mut a) = host();
        let installed = d.0.join("etc/tehas.slice");
        a.slice = Some(installed.clone());
        let mut o = Fake::default();
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Slice:        MISSING (sudo ./bootstrap.sh slice)") && found);
        let at = installed.display();
        assert_eq!(
            notes(&a, &mut o),
            [format!("note: {at} is missing (sudo ./bootstrap.sh slice)")]
        );
        d.file("etc/tehas.slice", "[Slice]\nMemoryMax=10%\n");
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Slice:        DIFFERS (sudo ./bootstrap.sh slice)") && found);
        assert_eq!(notes(&a, &mut o), [format!("note: {at} differs from host/slice/tehas.slice in this checkout (sudo ./bootstrap.sh slice)")]);
        d.file("etc/tehas.slice", "[Slice]\n");
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Slice:        OK") && !found);
        assert!(notes(&a, &mut o).is_empty());
    }

    #[test]
    fn images_on_against_an_image_that_does_not_read_them_is_said_and_not_found() {
        let (_d, mut a) = host();
        a.images = Some(("1".into(), "tehas/pi-rukas:uid1".into()));
        let mut o = Fake {
            labels: Some("c|0".into()),
            ..Fake::default()
        };
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Images:       NOT READ\n  TEHAS_WORK_IMAGES is 1 and the sandbox image tehas/pi-rukas:uid1 says its pi-rukas does not read the images a cycle is given."));
        assert!(!found);
        assert!(notes(&a, &mut o)[0].starts_with(
            "note: TEHAS_WORK_IMAGES is 1 and the sandbox image tehas/pi-rukas:uid1 says"
        ));

        o.labels = Some(String::new());
        let build = a.home.join("sandbox/build.sh");
        assert_eq!(notes(&a, &mut o), [format!("note: TEHAS_WORK_IMAGES is 1 and the sandbox image tehas/pi-rukas:uid1 does not say whether its pi-rukas reads the images a cycle is given. Rebuild it with {}, which records the answer.", build.display())]);
        assert!(said(&a, &mut o).0.contains("Images:       NOT SAID\n"));

        // An image that reads them, no image, or the setting off: nothing.
        for (labels, setting) in [(Some("c|1"), "1"), (None, "1"), (Some("c|0"), "0")] {
            o.labels = labels.map(str::to_string);
            a.images = Some((setting.into(), "img".into()));
            assert!(!said(&a, &mut o).0.contains("Images:"));
            assert!(notes(&a, &mut o).is_empty());
        }
    }

    #[test]
    fn an_image_the_health_calls_behind_is_found_with_its_versions_or_commits() {
        let (d, a) = host();
        let health = |rukas: &str| {
            d.file(
                "home/live/health.json",
                &format!(r#"{{"checks":{{"rukas":{rukas}}}}}"#),
            )
        };
        health(
            r#"{"image":"aaa","imageVersion":"v0.14.53","target":"bbb","targetVersion":"v0.14.54","behind":true}"#,
        );
        let (out, found) = said(&a, &mut Fake::default());
        assert!(out.contains("Sandbox:      BEHIND (the image runs pi-rukas v0.14.53, the host follows v0.14.54; tehas update builds it)\nInstalled:"));
        assert!(found);
        health(r#"{"image":"aaa","imageVersion":null,"target":"bbb","behind":true}"#);
        assert!(said(&a, &mut Fake::default())
            .0
            .contains("pi-rukas aaa, the host follows bbb;"));
        health(r#"{"image":"bbb","target":"bbb","behind":false}"#);
        assert!(!said(&a, &mut Fake::default()).1);
        d.file("home/live/health.json", "not json");
        assert!(!said(&a, &mut Fake::default()).0.contains("Sandbox:"));
    }

    #[test]
    fn an_image_built_after_the_health_was_written_is_not_called_behind() {
        let (d, mut a) = host();
        a.images = Some(("0".into(), "tehas/pi-rukas:uid1000".into()));
        d.file(
            "home/live/health.json",
            r#"{"checks":{"rukas":{"image":"aaa","imageVersion":"v0.14.63","target":"bbb","targetVersion":"v0.14.69","behind":true}}}"#,
        );
        let mut o = Fake {
            labels: Some("bbb|1".into()),
            ..Fake::default()
        };
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Sandbox:      NOT READ (the image was built after the last status run; the next one reads it)\nInstalled:"));
        assert!(!found);
        // The image the health read is still the one there: behind.
        o.labels = Some("aaa|1".into());
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Sandbox:      BEHIND (the image runs pi-rukas v0.14.63"));
        assert!(found);
        // An image without the label, or none: the health's word stands.
        for labels in [Some("|1".to_string()), None] {
            o.labels = labels;
            assert!(said(&a, &mut o).1);
        }
    }

    #[test]
    fn the_last_check_product_is_shown_and_never_found() {
        let (d, a) = host();
        assert!(!said(&a, &mut Fake::default()).0.contains("Product:"));
        let file = d.file(
            "home/state/check-product.json",
            r#"{"v":1,"at":"2026-10-21T09:00:00Z","ok":false,"failures":["no AGENTS.md"],"sandbox":null}"#,
        );
        let (out, found) = said(&a, &mut Fake::default());
        assert!(out.contains("Product:      1 TO FIX (tehas check-product, 2026-10-21T09:00:00Z)\n  no AGENTS.md\nInstalled:"));
        assert!(!found);
        d.file(
            "home/state/check-product.json",
            r#"{"at":"t","ok":true,"failures":[]}"#,
        );
        assert!(said(&a, &mut Fake::default())
            .0
            .contains("Product:      OK (tehas check-product, t)\nInstalled:"));
        d.file("home/state/check-product.json", "{");
        let not_read = format!("Product:      NOT READ ({})", file.display());
        assert!(said(&a, &mut Fake::default()).0.contains(&not_read));
    }

    #[test]
    fn a_program_run_for_the_report_gives_both_outputs_as_one() {
        let sh = Path::new("/bin/sh");
        let said = Real.merged(sh, &["-c", "echo one; echo two >&2; echo three; exit 3"]);
        assert_eq!(said, (3, "one\ntwo\nthree".into()));
        assert_eq!(Real.merged(Path::new("/no/such/program"), &[]).0, 127);
    }

    #[test]
    fn drift_names_the_count_and_each_line() {
        let (_d, mut a) = host();
        a.drift = 2;
        a.drift_lines = vec!["differs: /h/a".into(), "stale: /h/b".into()];
        let (out, found) = said(&a, &mut Fake::default());
        assert!(
            out.contains("Installed:    DRIFT (2 files)\n  differs: /h/a\n  stale: /h/b\nShell:")
        );
        assert!(found);
    }

    #[test]
    fn the_shell_first_install_store_and_login_each_say_what_clears_them() {
        let (d, mut a) = host();
        a.shell_missing = Some("/h/.bashrc".into());
        a.store = "no store".into();
        let login = d.0.join("bin/tehas");
        a.login = Some((login.clone(), "sudo ./bootstrap.sh dev alice".into()));
        d.file("home/state/first-install", "");
        let (out, found) = said(&a, &mut Fake::default());
        let env = a.home.join("env.sh");
        for line in [
            format!("Shell:        MISSING (/h/.bashrc does not source {}; ./install.sh puts the line at its top)", env.display()),
            "First install: NOT DONE (labels and core units; ./install.sh does them)".into(),
            "Store:        REFUSED (no store; ./install.sh stops before it writes)".into(),
            format!("Login:        NOT CURRENT\n  missing: {}   (sudo ./bootstrap.sh dev alice)", login.display()),
        ] {
            assert!(out.contains(&line), "{line} not in\n{out}");
        }
        assert!(found);
        d.file("bin/tehas", "");
        assert!(said(&a, &mut Fake::default()).0.contains("  differs: "));
        // An empty fix is nothing to fix.
        a.login = Some((login, String::new()));
        assert!(!said(&a, &mut Fake::default()).0.contains("Login:"));
    }

    #[test]
    fn follow_ups_are_counted_by_the_steps_console_and_an_unread_record_is_not_done() {
        let (d, mut a) = host();
        let steps = PathBuf::from("/c");
        a.steps = Some(steps.clone());
        let mut o = Fake::default();
        assert!(!said(&a, &mut o).0.contains("Follow-ups:"));
        assert!(o.asked.is_empty(), "no record, no question");
        let record = d.file("home/state/update-pending", "sandbox\ndocs\n");
        o.ran
            .insert(steps.clone(), (0, "sandbox\ndocs --force-recreate".into()));
        let (out, found) = said(&a, &mut o);
        assert!(out.contains(&format!(
            "Follow-ups:   NOT DONE (2 from an earlier run, see {})",
            record.display()
        )));
        assert!(found);
        o.ran.insert(steps.clone(), (0, String::new()));
        assert!(!said(&a, &mut o).0.contains("Follow-ups:"));
        o.ran.insert(steps, (64, String::new()));
        let (out, found) = said(&a, &mut o);
        assert!(out.contains(&format!(
            "Follow-ups:   NOT DONE (the record {} cannot be read)",
            record.display()
        )));
        assert!(found);
    }

    #[test]
    fn the_tests_run_the_suite_given_else_the_checkouts_and_show_the_last_thirty_lines() {
        let (_d, mut a) = host();
        a.tests = Some(String::new());
        let mut o = Fake::default();
        assert!(said(&a, &mut o)
            .0
            .ends_with("Tests:        not checked (running inside the tests)"));
        assert!(o.asked.is_empty());
        a.in_test = false;
        let suite = a.root.join("scripts/test.sh");
        o.ran.insert(suite.clone(), (0, "ok".into()));
        let (out, _) = said(&a, &mut o);
        assert!(out.ends_with("Tests:        OK"));
        assert_eq!(o.asked.last(), Some(&suite));
        let given = PathBuf::from("/t/suite");
        a.tests = Some("/t/suite".into());
        a.in_test = true;
        let all: Vec<String> = (1..=40).map(|n| format!("line {n}")).collect();
        o.ran.insert(given, (1, all.join("\n")));
        let (out, found) = said(&a, &mut o);
        assert!(out.contains("Tests:        FAILED\n  line 11\n"), "{out}");
        assert!(out.ends_with("  line 40") && !out.contains("line 10\n") && found);
    }
}
