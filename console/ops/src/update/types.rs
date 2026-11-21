//! The kit's type check, a test of the workspace (host/docs/frontend.md,
//! "The kit"). TypeScript's native `tsc`, pinned by version, reads the kit's
//! modules strict with their JSDoc types (`--checkJs --noEmit`). Nothing is
//! written: the modules stay what is served.
//!
//! It runs when the pinned `tsc` is there: `TEHAS_TSC` names it, or `tsc` on
//! `PATH` is that version. Without one the check is skipped and says so on
//! stderr, except when strict, where it fails: CI installs it and names it.
//!
//! `test/fixtures/kit-types/` holds code that uses the kit. `uses.js` uses it
//! as the manual says and passes. Each `misuse-*.js` makes one mistake, on
//! the line marked `// rejected: TS<code>`, and tsc must reject it there with
//! that code and nowhere else.

use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The `tsc` the check is written for: the native compiler of the
/// `typescript` package. CI installs the same version.
const TSC_VERSION: &str = "7.0.2";

/// The variable that names the `tsc` to run.
const TSC_VAR: &str = "TEHAS_TSC";

/// The kit's modules, relative to the repository.
const KIT: &str = "host/fun/kit";

/// The code that uses the kit, relative to the repository.
const FIXTURES: &str = "test/fixtures/kit-types";

/// The workflows that run the gate and so install the pinned `tsc`.
const WORKFLOWS: [&str; 2] = [
    ".github/workflows/verify.yml",
    ".github/workflows/release-gate.yml",
];

/// How `tsc` reads the files: strict, each file a module as a page loads it,
/// its imports found by path as a browser finds them.
const OPTIONS: [&str; 16] = [
    "--noEmit",
    "--allowJs",
    "--checkJs",
    "--strict",
    "--target",
    "es2022",
    "--module",
    "esnext",
    "--moduleResolution",
    "bundler",
    "--moduleDetection",
    "force",
    "--lib",
    "es2023,dom,dom.iterable",
    "--pretty",
    "false",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The kit's own modules, the `.js` files of `host/fun/kit` but not of
/// `vendor/`, relative to `root`.
fn modules(root: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = root.join(KIT);
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".js") && entry.path().is_file() {
            out.push(Path::new(KIT).join(name));
        }
    }
    out.sort();
    Ok(out)
}

/// The arguments that check `files`.
fn args(files: &[PathBuf]) -> Vec<OsString> {
    let mut out: Vec<OsString> = OPTIONS.iter().map(OsString::from).collect();
    out.extend(files.iter().map(|p| p.clone().into_os_string()));
    out
}

/// The version in what `tsc --version` prints: `Version 7.0.2`.
fn parse_version(out: &str) -> Option<&str> {
    let v = out.trim().strip_prefix("Version ")?;
    (!v.is_empty() && !v.contains(char::is_whitespace)).then_some(v)
}

fn version_of(path: &Path) -> Result<String, String> {
    let out = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    match parse_version(&text) {
        Some(v) if out.status.success() => Ok(v.to_owned()),
        _ => Err(format!(
            "{} --version: no version in {:?}",
            path.display(),
            text.trim()
        )),
    }
}

/// Whether a missing `tsc` fails the check rather than skips it: `CI` or
/// `TEHAS_TEST_STRICT` set to anything, as xtask's `strict` decides for a
/// missing shellcheck.
fn strict(ci: Option<&str>, asked: Option<&str>) -> bool {
    ci.is_some_and(|v| !v.is_empty()) || asked.is_some_and(|v| !v.is_empty())
}

/// Which `tsc` to run. One that is `named` must be the pinned version, or the
/// check fails: it was named on purpose. One found `on_path` is used only when
/// it is that version, since another version reads the files differently.
/// With none, the check is skipped, but when `strict` it fails.
fn choose(
    named: Option<PathBuf>,
    on_path: Option<PathBuf>,
    strict: bool,
    version: impl Fn(&Path) -> Result<String, String>,
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = named {
        let v = version(&path).map_err(|e| format!("{TSC_VAR}: {e}"))?;
        if v != TSC_VERSION {
            return Err(format!(
                "{TSC_VAR} is {}, tsc {v}; the check is written for {TSC_VERSION}",
                path.display()
            ));
        }
        return Ok(Some(path));
    }
    let found = on_path.filter(|p| version(p).is_ok_and(|v| v == TSC_VERSION));
    if found.is_none() && strict {
        return Err(format!(
            "no tsc {TSC_VERSION}, and CI or TEHAS_TEST_STRICT is set: name it in {TSC_VAR}"
        ));
    }
    Ok(found)
}

/// The `tsc` to run, from `TEHAS_TSC`, `PATH`, `CI` and `TEHAS_TEST_STRICT`,
/// as [`choose`] says.
fn find_tsc() -> Result<Option<PathBuf>, String> {
    let named = std::env::var_os(TSC_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("tsc"))
            .find(|p| p.is_file())
    });
    let strict = strict(
        std::env::var("CI").ok().as_deref(),
        std::env::var("TEHAS_TEST_STRICT").ok().as_deref(),
    );
    choose(named, on_path, strict, version_of)
}

/// Checks `files`, relative to `root`, with the `tsc` at `tsc`. What it
/// reports is the error.
fn check(tsc: &Path, root: &Path, files: &[PathBuf]) -> Result<(), String> {
    let out = Command::new(tsc)
        .args(args(files))
        .current_dir(root)
        .output()
        .map_err(|e| format!("{}: {e}", tsc.display()))?;
    if out.status.success() {
        return Ok(());
    }
    let mut report = String::from_utf8_lossy(&out.stdout).into_owned();
    report.push_str(&String::from_utf8_lossy(&out.stderr));
    let report = report.trim();
    if report.is_empty() {
        return Err(format!("tsc failed ({}) and said nothing", out.status));
    }
    Err(report.to_owned())
}

/// An error of a report: its file, its line and its code.
#[derive(Debug, PartialEq)]
struct Error {
    file: String,
    line: usize,
    code: String,
}

/// The errors of a report, one for each line `<file>(<line>,<col>): error
/// TS<n>: …`. The indented lines that explain one are not errors.
fn errors(report: &str) -> Vec<Error> {
    let mut out = Vec::new();
    for line in report.lines() {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((at, rest)) = line.split_once("): error ") else {
            continue;
        };
        let Some((file, place)) = at.rsplit_once('(') else {
            continue;
        };
        let Some(n) = place.split(',').next().and_then(|n| n.parse().ok()) else {
            continue;
        };
        let code = rest.split(':').next().unwrap_or_default().to_owned();
        out.push(Error {
            file: file.to_owned(),
            line: n,
            code,
        });
    }
    out
}

/// The line, from 1, and the code that a misuse's `// rejected: TS<n>`
/// marks. A file has one.
fn marked(text: &str) -> Result<(usize, String), String> {
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some((_, code)) = line.split_once("// rejected: ") {
            let code = code.trim();
            if !code.starts_with("TS")
                || code.len() < 3
                || !code[2..].bytes().all(|b| b.is_ascii_digit())
            {
                return Err(format!("line {}: {code:?} is no TS<n> code", i + 1));
            }
            found.push((i + 1, code.to_owned()));
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        n => Err(format!("{n} lines marked \"// rejected:\", not one")),
    }
}

/// The pinned `tsc`, or `None` after saying the check did not run, on stderr
/// past cargo's capture.
fn tsc_or_skip(test: &str) -> Option<PathBuf> {
    let found = find_tsc().unwrap_or_else(|e| panic!("{e}"));
    if found.is_none() {
        let line = format!(
            "SKIPPED {test}: no tsc {TSC_VERSION}. Name it in {TSC_VAR} or put it on PATH.\n"
        );
        let _ = std::io::stderr().write_all(line.as_bytes());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn misuses(root: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = fs::read_dir(root.join(FIXTURES))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("misuse-") && n.ends_with(".js"))
            .map(|n| Path::new(FIXTURES).join(n))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn the_kit_checks_strict_clean_with_code_that_uses_it() {
        let Some(tsc) = tsc_or_skip("the_kit_checks_strict_clean_with_code_that_uses_it") else {
            return;
        };
        let mut files = modules(&root()).unwrap();
        files.push(Path::new(FIXTURES).join("uses.js"));
        if let Err(e) = check(&tsc, &root(), &files) {
            panic!("the kit's type check:\n{e}");
        }
    }

    #[test]
    fn each_misuse_is_rejected_on_its_line_and_nowhere_else() {
        let Some(tsc) = tsc_or_skip("each_misuse_is_rejected_on_its_line_and_nowhere_else") else {
            return;
        };
        for file in misuses(&root()) {
            let name = file.to_string_lossy().into_owned();
            let text = fs::read_to_string(root().join(&file)).unwrap();
            let (line, code) = marked(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            let report = match check(&tsc, &root(), std::slice::from_ref(&file)) {
                Ok(()) => panic!("{name}: tsc took the misuse on line {line}"),
                Err(report) => report,
            };
            let found = errors(&report);
            assert!(!found.is_empty(), "{name}: no error read in:\n{report}");
            for e in &found {
                assert_eq!(
                    (e.file.as_str(), e.line, e.code.as_str()),
                    (name.as_str(), line, code.as_str()),
                    "{name}: an error other than the misuse:\n{report}"
                );
            }
        }
    }

    #[test]
    fn the_fixtures_hold_the_five_misuses_and_the_kit_its_seven_modules() {
        assert_eq!(
            names(&misuses(&root())),
            [
                "misuse-answer.js",
                "misuse-handle.js",
                "misuse-load.js",
                "misuse-method.js",
                "misuse-option.js",
            ]
        );
        for file in misuses(&root()) {
            let text = fs::read_to_string(root().join(&file)).unwrap();
            marked(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        }
        assert_eq!(
            names(&modules(&root()).unwrap()),
            [
                "feed.js",
                "island.js",
                "parts.js",
                "press.js",
                "request.js",
                "state.js",
                "stream.js",
            ]
        );
    }

    #[test]
    fn the_kit_modules_are_its_js_files_and_not_vendor() {
        let d = dir("types-modules");
        d.file("host/fun/kit/a.js", "");
        d.file("host/fun/kit/b.js", "");
        d.file("host/fun/kit/kit.css", "");
        d.file("host/fun/kit/kit.d.ts", "");
        d.file("host/fun/kit/vendor/solid.js", "");
        assert_eq!(
            modules(d.0.path()).unwrap(),
            [
                Path::new("host/fun/kit/a.js"),
                Path::new("host/fun/kit/b.js")
            ]
        );
    }

    #[test]
    fn the_arguments_are_strict_and_each_file_is_a_module() {
        let files = [PathBuf::from("a.js"), PathBuf::from("b.js")];
        let args = args(&files);
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(&args[..OPTIONS.len()], OPTIONS);
        assert_eq!(&args[OPTIONS.len()..], ["a.js", "b.js"]);
        let strict = OPTIONS.iter().position(|o| *o == "--strict").unwrap();
        assert!(
            OPTIONS[strict + 1].starts_with("--"),
            "--strict takes no false"
        );
        let force = OPTIONS.iter().position(|o| *o == "--moduleDetection");
        assert_eq!(force.map(|i| OPTIONS[i + 1]), Some("force"));
    }

    #[test]
    fn the_version_is_read_from_its_line() {
        assert_eq!(parse_version("Version 7.0.2\n"), Some("7.0.2"));
        assert_eq!(parse_version("Version \n"), None);
        assert_eq!(parse_version("tsc 7.0.2"), None);
        assert_eq!(parse_version("Version 7.0.2 extra"), None);
    }

    #[test]
    fn a_named_tsc_must_be_the_pinned_version_and_none_fails_only_when_strict() {
        let pinned = |_: &Path| Ok(TSC_VERSION.to_owned());
        let older = |_: &Path| Ok("5.9.3".to_owned());
        let broken = |_: &Path| Err("no version".to_owned());
        let named = Some(PathBuf::from("/opt/tsc"));
        let on_path = Some(PathBuf::from("/usr/bin/tsc"));

        for ci in [false, true] {
            assert_eq!(
                choose(named.clone(), on_path.clone(), ci, pinned).unwrap(),
                named
            );
            let err = choose(named.clone(), on_path.clone(), ci, older).unwrap_err();
            assert!(err.contains(TSC_VAR) && err.contains("5.9.3"), "{err}");
            assert!(choose(named.clone(), None, ci, broken).is_err());
            assert_eq!(choose(None, on_path.clone(), ci, pinned).unwrap(), on_path);
        }

        // With no pinned tsc the check is skipped, and when strict it fails.
        assert_eq!(choose(None, on_path.clone(), false, older).unwrap(), None);
        assert_eq!(choose(None, on_path.clone(), false, broken).unwrap(), None);
        assert_eq!(choose(None, None, false, pinned).unwrap(), None);
        for (on_path, version) in [
            (
                on_path.clone(),
                older as fn(&Path) -> Result<String, String>,
            ),
            (on_path, broken),
            (None, pinned),
        ] {
            let err = choose(None, on_path, true, version).unwrap_err();
            assert!(
                err.contains("TEHAS_TEST_STRICT") && err.contains(TSC_VAR),
                "{err}"
            );
        }
    }

    #[test]
    fn strict_is_ci_or_tehas_test_strict_set_to_anything_as_xtask_says() {
        // The same rule as xtask's strict, which fails on a missing shellcheck.
        for value in ["true", "1", "false", "0"] {
            assert!(strict(Some(value), None), "CI={value}");
            assert!(strict(None, Some(value)), "TEHAS_TEST_STRICT={value}");
        }
        assert!(!strict(None, None));
        assert!(!strict(Some(""), Some("")));
    }

    #[test]
    fn what_tsc_reports_is_the_error() {
        let d = dir("types-fake");
        let exe = |name: &str, body: &str| {
            let path = d.0.path().join(name);
            tehas_core::testing::write_exe(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            path
        };
        let tsc = exe(
            "tsc",
            "echo 'a.js(1,1): error TS2304: Cannot find name x.'\nexit 2",
        );
        let err = check(&tsc, d.0.path(), &[]).unwrap_err();
        assert_eq!(err, "a.js(1,1): error TS2304: Cannot find name x.");
        let quiet = exe("quiet", "exit 1");
        let err = check(&quiet, d.0.path(), &[]).unwrap_err();
        assert!(err.contains("said nothing"), "{err}");
        check(&exe("fine", "exit 0"), d.0.path(), &[]).unwrap();

        let old = exe("old", "echo 'Version 5.9.3'");
        assert_eq!(version_of(&old).unwrap(), "5.9.3");
        assert!(version_of(&quiet).is_err());
    }

    #[test]
    fn a_report_reads_as_its_errors_without_their_explanations() {
        let report = "a/b.js(5,32): error TS2339: Property 'data' does not exist.\n  \
                      Property 'data' does not exist on type '{ ok: false; }'.\n\
                      c(1).js(4,1): error TS2551: Did you mean 'put'?\n\
                      Found 2 errors.";
        assert_eq!(
            errors(report),
            [
                Error {
                    file: "a/b.js".into(),
                    line: 5,
                    code: "TS2339".into()
                },
                Error {
                    file: "c(1).js".into(),
                    line: 4,
                    code: "TS2551".into()
                },
            ]
        );
    }

    #[test]
    fn a_misuse_marks_one_line_with_one_code() {
        assert_eq!(
            marked("// a\nf(); // rejected: TS2561\n").unwrap(),
            (2, "TS2561".to_owned())
        );
        assert!(marked("f();\n").is_err());
        assert!(marked("f(); // rejected: TS1\ng(); // rejected: TS2\n").is_err());
        assert!(marked("f(); // rejected: 2561\n").is_err());
        assert!(marked("f(); // rejected: TS\n").is_err());
    }

    #[test]
    fn ci_installs_the_pinned_tsc_and_names_it() {
        for workflow in WORKFLOWS {
            let text = fs::read_to_string(root().join(workflow)).unwrap();
            assert!(
                text.contains(&format!("TSC_VERSION: {TSC_VERSION}\n")),
                "{workflow} installs another tsc than {TSC_VERSION}"
            );
            let at = text.find("TSC_SHA256: ");
            assert!(at.is_some(), "{workflow} checks its sum");
            // The step, up to the next one.
            let step = &text[at.unwrap()..];
            let step = &step[..step.find("\n      - name:").unwrap_or(step.len())];
            assert!(
                step.contains("-o \"$tgz\" --proto '=https' --tlsv1.2 "),
                "{workflow} fetches it over HTTPS only, TLS 1.2 or later"
            );
            // The kept tarball's sum is checked on every run, before tsc is
            // unpacked from it into the job's own temporary directory.
            let check = step.rfind("sha256sum -c").unwrap_or(usize::MAX);
            let unpack = step.find("tar -xzf \"$tgz\" -C \"$RUNNER_TEMP/tsc\"");
            assert!(
                unpack.is_some_and(|u| check < u),
                "{workflow} unpacks tsc each run from a checked tarball"
            );
            assert!(
                text.contains(&format!("{TSC_VAR}=$bin")),
                "{workflow} names it in {TSC_VAR}"
            );
        }
    }
}
