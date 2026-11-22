//! The commands `.pi/work.json` gives pi-rukas's lean driver: `lean-test` runs
//! one test target's file, and `lean-roster` runs every test target of the
//! workspace and writes a JUnit report of each test, which lean compares
//! between the head and the base of a run.
//!
//! Lean freezes tests as files, so only a file that is a test target of its
//! own counts. Each target runs on its own: cargo prints which target runs on
//! stderr and the results on stdout, so one run per target is what ties a
//! result to its target.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

/// A test target: `cargo test -p <package> --test <target>` runs `file`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Target {
    pub package: String,
    pub target: String,
    /// Relative to the checkout, as console/dispatch/tests/poll.rs.
    pub file: String,
}

/// The target whose file is `file`. A module of another target's file is not
/// one: lean's test there would not run on its own.
pub fn target<'a>(all: &'a [Target], file: &str) -> Result<&'a Target, String> {
    all.iter()
        .find(|t| t.file == file)
        .ok_or_else(|| format!("{file}: not a test target of the workspace"))
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Outcome {
    Passed,
    Failed,
    Ignored,
}

/// One test as libtest reported it.
#[derive(Debug, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    pub outcome: Outcome,
}

/// The `test <name> ... ok|FAILED|ignored` lines of one target's run.
pub fn cases(stdout: &str) -> Vec<Case> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.trim().strip_prefix("test ") else {
            continue;
        };
        let Some((name, verdict)) = rest.rsplit_once(" ... ") else {
            continue;
        };
        let outcome = match verdict.split([' ', ',']).next() {
            Some("ok") => Outcome::Passed,
            Some("FAILED") => Outcome::Failed,
            Some("ignored") => Outcome::Ignored,
            _ => continue,
        };
        out.push(Case {
            name: name.to_string(),
            outcome,
        });
    }
    out
}

/// A JUnit report. Each test is named `<package>::<target>` and its libtest
/// path, the same in any checkout, so the head's run and the base's name it
/// alike. An ignored test is left out, since lean reads a skipped one as
/// passing.
pub fn report(runs: &[(Target, Vec<Case>)]) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n");
    for (t, cases) in runs {
        let ran: Vec<&Case> = cases
            .iter()
            .filter(|c| c.outcome != Outcome::Ignored)
            .collect();
        let class = esc(&format!("{}::{}", t.package, t.target));
        let failures = ran.iter().filter(|c| c.outcome == Outcome::Failed).count();
        let _ = writeln!(
            s,
            "<testsuite name=\"{class}\" tests=\"{}\" failures=\"{failures}\">",
            ran.len()
        );
        for c in ran {
            let _ = write!(
                s,
                "<testcase classname=\"{class}\" name=\"{}\"",
                esc(&c.name)
            );
            if c.outcome == Outcome::Failed {
                s.push_str("><failure message=\"failed\"/></testcase>\n");
            } else {
                s.push_str("/>\n");
            }
        }
        s.push_str("</testsuite>\n");
    }
    s.push_str("</testsuites>\n");
    s
}

fn esc(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn cargo_test(root: &Path, t: &Target) -> Command {
    let mut c = Command::new("cargo");
    c.args(["test", "--locked", "--offline", "--manifest-path"])
        .arg(root.join("console/Cargo.toml"))
        .args(["-p", &t.package, "--test", &t.target])
        .current_dir(root);
    c
}

/// `lean-test`: run the one file. Its status is cargo's.
pub fn run_test(root: &Path, all: &[Target], file: &str) -> u8 {
    let t = match target(all, file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lean-test: {e}");
            return 64;
        }
    };
    match cargo_test(root, t).status() {
        Ok(s) => s.code().map_or(1, |c| u8::try_from(c).unwrap_or(1)),
        Err(e) => {
            eprintln!("lean-test: cargo: {e}");
            1
        }
    }
}

/// `lean-roster`: run every target and write the report. 0 when every test
/// passed, 1 when one failed, 2 when a target failed with no test it
/// reported, as when it did not build: lean reads that as a broken bench, not a verdict.
pub fn run_roster(root: &Path, all: &[Target], report_to: &Path) -> u8 {
    let mut runs = Vec::new();
    let mut status = 0;
    for t in all {
        let out = match cargo_test(root, t).output() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("lean-roster: cargo: {e}");
                return 2;
            }
        };
        let cases = cases(&String::from_utf8_lossy(&out.stdout));
        if cases.iter().any(|c| c.outcome == Outcome::Failed) {
            status = status.max(1);
        } else if !out.status.success() {
            eprintln!(
                "lean-roster: {}::{} failed with no failing test\n{}",
                t.package,
                t.target,
                String::from_utf8_lossy(&out.stderr)
            );
            status = 2;
        }
        runs.push((t.clone(), cases));
    }
    if let Err(e) = std::fs::write(report_to, report(&runs)) {
        eprintln!("lean-roster: {}: {e}", report_to.display());
        return 2;
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(package: &str, target: &str, file: &str) -> Target {
        Target {
            package: package.into(),
            target: target.into(),
            file: file.into(),
        }
    }

    fn all() -> Vec<Target> {
        vec![
            t("console", "surface", "console/tests/surface.rs"),
            t("tehas-dispatch", "poll", "console/dispatch/tests/poll.rs"),
            t("tehas-mcp", "mcp", "console/mcp/tests/mcp.rs"),
        ]
    }

    #[test]
    fn a_target_is_found_by_its_file() {
        let all = all();
        assert_eq!(
            target(&all, "console/dispatch/tests/poll.rs").unwrap(),
            &all[1]
        );
    }

    #[test]
    fn a_file_that_is_no_target_is_refused() {
        for f in [
            "console/dispatch/src/decide.rs",
            "console/mcp/tests/tools.rs",
            "console/dispatch/tests/poll",
            "poll.rs",
        ] {
            assert!(target(&all(), f).is_err(), "{f}");
        }
    }

    const OUT: &str = "
running 3 tests
test a_poll_reads ... ok
test a_poll_fails ... FAILED
test slow ... ignored, needs network
test result: FAILED. 1 passed; 1 failed; 1 ignored
";

    #[test]
    fn cases_are_the_test_lines() {
        let got: Vec<(String, Outcome)> = cases(OUT)
            .into_iter()
            .map(|c| (c.name, c.outcome))
            .collect();
        assert_eq!(
            got,
            [
                ("a_poll_reads".to_string(), Outcome::Passed),
                ("a_poll_fails".to_string(), Outcome::Failed),
                ("slow".to_string(), Outcome::Ignored),
            ]
        );
    }

    #[test]
    fn the_report_names_tests_by_package_and_target_and_leaves_out_ignored() {
        let r = report(&[(all()[1].clone(), cases(OUT))]);
        assert!(r.starts_with("<?xml"));
        assert!(r.contains("name=\"tehas-dispatch::poll\" tests=\"2\" failures=\"1\""));
        assert!(r.contains("<testcase classname=\"tehas-dispatch::poll\" name=\"a_poll_reads\"/>"));
        assert!(r.contains(
            "<testcase classname=\"tehas-dispatch::poll\" name=\"a_poll_fails\"><failure"
        ));
        assert!(!r.contains("slow"));
        assert!(
            !r.contains("console/"),
            "no path, so head and base name a test alike"
        );
    }

    #[test]
    fn names_are_escaped() {
        let c = vec![Case {
            name: "x<\"&\">".into(),
            outcome: Outcome::Passed,
        }];
        assert!(report(&[(t("p", "a", "f"), c)]).contains("name=\"x&lt;&quot;&amp;&quot;&gt;\""));
    }
}
