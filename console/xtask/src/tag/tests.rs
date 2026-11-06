//! The decisions of the tag over text, and the task itself over a scratch
//! remote with --no-verify. test/tag_release_test.sh runs the script with the
//! gate, the polyfills and the pins, gh and docker stubbed.

use super::*;
use std::fs;
use tehas_core::testdir::TestDir;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|a| a.to_string()).collect()
}

fn args(v: &[&str]) -> Args {
    match parse(&s(v), Path::new("/")).unwrap() {
        Parsed::Run(a) => a,
        Parsed::Help => panic!("help"),
    }
}

#[test]
fn the_arguments_are_read_in_order() {
    assert_eq!(args(&[]), Args::default());
    let a = args(&[
        "--minor",
        "--no-verify",
        "--message",
        "by hand",
        "--remote",
        "up",
        "--no-pins",
        "--dry-run",
    ]);
    assert_eq!(a.part, Part::Minor);
    assert!(!a.verify && !a.pins && a.dry);
    assert_eq!((a.message.as_str(), a.remote.as_str()), ("by hand", "up"));
    assert_eq!(args(&["--major", "--patch"]).part, Part::Patch);
    assert_eq!(
        parse(&s(&["--bogus"]), Path::new("/")),
        Err("unknown argument: --bogus".into())
    );
    assert_eq!(
        parse(&s(&["--message"]), Path::new("/")),
        Err("--message needs a text".into())
    );
    assert_eq!(
        parse(&s(&["--remote"]), Path::new("/")),
        Err("--remote needs a name".into())
    );
    assert_eq!(
        parse(&s(&["--help", "--bogus"]), Path::new("/")),
        Ok(Parsed::Help)
    );
    assert_eq!(
        parse(&s(&["--bogus", "-h"]), Path::new("/")).unwrap_err(),
        "unknown argument: --bogus"
    );
}

#[test]
fn rukas_caps_needs_a_file_and_is_made_absolute() {
    let d = TestDir::new("xtask-tag-caps");
    fs::create_dir(d.join("sub")).unwrap();
    fs::write(d.join("sub/caps.json"), "{}").unwrap();
    let parsed = |v: &str| parse(&s(&["--rukas-caps", v]), &d);
    for v in [
        "sub/caps.json",
        "./sub/../sub/caps.json",
        &d.join("sub/caps.json").display().to_string(),
    ] {
        match parsed(v).unwrap() {
            Parsed::Run(a) => assert_eq!(a.rukas_caps, Some(d.join("sub/caps.json")), "{v}"),
            Parsed::Help => panic!(),
        }
    }
    for v in ["sub", "none.json", ""] {
        assert_eq!(parsed(v), Err("--rukas-caps needs a file".into()), "{v}");
    }
    assert!(parse(&s(&["--rukas-caps"]), &d).is_err());
}

#[test]
fn the_remote_head_names_the_branch_and_its_commit() {
    let text = "ref: refs/heads/main\tHEAD\n0123abc\tHEAD\n";
    assert_eq!(remote_head(text), Some(("main".into(), "0123abc".into())));
    assert_eq!(
        remote_head("ref: refs/heads/trunk  HEAD\nfff HEAD"),
        Some(("trunk".into(), "fff".into()))
    );
    assert_eq!(remote_head("0123abc\tHEAD\n"), None);
    assert_eq!(remote_head("ref: refs/heads/main\tHEAD\n"), None);
    assert_eq!(remote_head("ref: refs/heads/main\tHEADS\n0\tHEAD\n"), None);
    assert_eq!(remote_head(""), None);
}

#[test]
fn the_newest_release_is_the_first_vxyz() {
    assert_eq!(
        newest("v2.0.0-rc1\nv1.10.0\nv1.9.0\n"),
        Some("v1.10.0".into())
    );
    assert_eq!(newest("vnext\nv1\n"), None);
    assert_eq!(newest(""), None);
}

#[test]
fn the_next_version_raises_one_part_and_resets_those_below() {
    assert_eq!(next_version(Some("v0.7.6"), Part::Patch), "v0.7.7");
    assert_eq!(next_version(Some("v0.7.6"), Part::Minor), "v0.8.0");
    assert_eq!(next_version(Some("v0.7.6"), Part::Major), "v1.0.0");
    assert_eq!(next_version(Some("v1.9.19"), Part::Patch), "v1.9.20");
    assert_eq!(next_version(None, Part::Patch), "v0.0.1");
    assert_eq!(next_version(None, Part::Minor), "v0.1.0");
}

const LOG: &str = "# Changelog\n\n## Unreleased\n\n### Changes\n\n- a\n\n### Adopter action\n\nNone.\n\n## v0.1.1 (2026-10-21)\n\n\n### Adopter action\n\nSet X.\n\n## v0.1.10\n\nten\n";

#[test]
fn a_section_is_its_lines_up_to_the_next_heading() {
    assert_eq!(
        section(LOG, "Unreleased").as_deref(),
        Some("### Changes\n\n- a\n\n### Adopter action\n\nNone.")
    );
    assert_eq!(
        section(LOG, "v0.1.1").as_deref(),
        Some("### Adopter action\n\nSet X.")
    );
    assert_eq!(section(LOG, "v0.1.10").as_deref(), Some("ten"));
    assert_eq!(section(LOG, "v0.1.2"), None);
    assert_eq!(section(LOG, "v0.1"), None);
    assert_eq!(section("## v1.0.0\n\n\n", "v1.0.0").as_deref(), Some(""));
    assert_eq!(section("## v1.0.0", "v1.0.0").as_deref(), Some(""));
}

#[test]
fn an_adopter_action_needs_a_line_under_its_heading() {
    assert!(adopter_action("### Adopter action\n\nNone."));
    assert!(adopter_action(
        "### Changes\n\n- a\n\n### Adopter action  \n  x"
    ));
    assert!(!adopter_action("### Changes\n\n- a"));
    assert!(!adopter_action(
        "### Adopter action\n\n \t\n### Held back\n\n- jq"
    ));
    assert!(!adopter_action("### Adopter actions\n\nNone."));
    // Any heading ends it, a deeper one too, as the awk of the script read it.
    assert!(!adopter_action("### Adopter action\n#### Root\nrun it"));
}

#[test]
fn a_carried_polyfill_is_three_words_the_last_taking_the_rest() {
    let c = carried("someFeature up/r#1 up/r#2\n  other  up/r#3\nbare\n\n");
    assert_eq!(
        c,
        vec![
            Carried {
                feature: "someFeature".into(),
                issue: "up/r#1".into(),
                pr: "up/r#2".into()
            },
            Carried {
                feature: "other".into(),
                issue: "up/r#3".into(),
                pr: "".into()
            },
            Carried {
                feature: "bare".into(),
                issue: "".into(),
                pr: "".into()
            },
        ]
    );
    assert_eq!(
        split_ref("up-owner/pi-rukas#78"),
        ("up-owner/pi-rukas", "78")
    );
    assert_eq!(split_ref("78"), ("78", "78"));
}

#[test]
fn a_kept_polyfill_needs_live_upstream_work() {
    let c = carried("open i#1 p#1\nmerged i#2 p#2\nclosed i#3 p#3\ngone i#4 p#4\nnoissue i#5 p#5\nnopr i#6\nunnamed\nexpired i#9 p#9\n");
    let issue = |i: &str| {
        if i == "i#5" {
            String::new()
        } else {
            "OPEN".into()
        }
    };
    let pr = |p: &str| {
        match p {
            "p#1" => "OPEN",
            "p#2" => "MERGED",
            "p#3" => "CLOSED",
            "p#9" => panic!("an expired polyfill is not asked about"),
            _ => "",
        }
        .to_string()
    };
    let (merged, untracked) = upstream(&c, &["expired"], "the pi-rukas of x", issue, pr);
    assert_eq!(
        merged,
        ["tag-release: p#2 is merged, but the pi-rukas of x does not set features.merged: its polyfill stays until a release has it; check the feature's name"]
    );
    assert_eq!(
        untracked,
        [
            "    closed: its upstream pull request p#3 was closed without a merge",
            "    gone: its upstream pull request p#4 cannot be found",
            "    noissue: its upstream issue i#5 cannot be found",
            "    nopr: its upstream pull request is not named cannot be found",
            "    unnamed: its upstream issue is not named cannot be found",
        ]
    );
}

#[test]
fn the_pull_requests_say_what_they_do_and_what_waits() {
    let c = carried("a i#1 p#1\nb i#2 p#2\n");
    assert_eq!(
        polyfill_body("pi-rukas v1", &c, &["b"], "v0.2.0"),
        "pi-rukas v1 sets these features in its --capabilities answer, so tehas no longer needs its own polyfills of them:\n\n- features.b, upstream i#2 and p#2\n\nOpened by scripts/tag-release.sh, which tags v0.2.0 once this is merged."
    );
    assert_eq!(
        pins_body(&["moved jq: 1.8.1 to 1.8.2", "held node: 24"], "v0.2.0"),
        "Each pinned tool at the newest release of the major version it has, with its checksum, digest or lockfile:\n\n- moved jq: 1.8.1 to 1.8.2\n- held node: 24\n\nA held tool has a new major version out; moving to it is a person's call. CHANGELOG.md names both.\n\nOpened by scripts/tag-release.sh, which tags v0.2.0 once this is merged."
    );
}

/// A bare remote with one commit tagged v0.1.0 and a clone the task runs in.
struct Remote {
    dir: TestDir,
}

impl Remote {
    fn new() -> Remote {
        let r = Remote {
            dir: TestDir::new("xtask-tag"),
        };
        r.git(&["init", "--quiet", "--bare", "-b", "main", "origin.git"]);
        r.git(&["init", "--quiet", "-b", "main", "up"]);
        r.git(&["-C", "up", "remote", "add", "origin", "../origin.git"]);
        r.changelog("## Unreleased\n\n### Adopter action\n\nNone.\n");
        r.git(&["-C", "up", "tag", "v0.1.0"]);
        r.git(&["-C", "up", "push", "--quiet", "origin", "v0.1.0"]);
        r.git(&["clone", "--quiet", "origin.git", "work"]);
        r
    }

    fn git(&self, a: &[&str]) -> String {
        let o = Command::new("git")
            .current_dir(&*self.dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(a)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {a:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    /// A new commit on the remote's main with this CHANGELOG.md.
    fn changelog(&self, text: &str) {
        fs::write(self.dir.join("up/CHANGELOG.md"), text).unwrap();
        self.git(&["-C", "up", "add", "CHANGELOG.md"]);
        self.git(&["-C", "up", "commit", "--quiet", "-m", "changelog"]);
        self.git(&["-C", "up", "push", "--quiet", "origin", "main"]);
    }

    fn run(&self, v: &[&str]) -> u8 {
        let mut v = v.to_vec();
        v.push("--no-verify");
        self.git(&["-C", "work", "config", "user.name", "t"]);
        self.git(&["-C", "work", "config", "user.email", "t@example.com"]);
        run(&args(&v), &self.dir.join("work"))
    }

    fn tags(&self) -> String {
        self.git(&["-C", "origin.git", "tag", "--list"])
            .replace('\n', " ")
    }
}

#[test]
fn a_head_that_carries_the_newest_tag_is_left_as_it_is() {
    let r = Remote::new();
    assert_eq!(r.run(&[]), 0);
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn a_new_head_is_tagged_and_pushed_and_the_worktree_removed() {
    let r = Remote::new();
    r.changelog("## Unreleased\n\n### Adopter action\n\nSet X.\n");
    assert_eq!(r.run(&["--dry-run"]), 0);
    assert_eq!(r.tags(), "v0.1.0");
    assert_eq!(r.run(&["--minor", "--message", "by hand"]), 0);
    assert_eq!(r.tags(), "v0.1.0 v0.2.0");
    assert_eq!(
        r.git(&[
            "-C",
            "origin.git",
            "tag",
            "-l",
            "--format=%(contents:subject)",
            "v0.2.0"
        ]),
        "Release v0.2.0 - by hand"
    );
    assert_eq!(
        r.git(&["-C", "origin.git", "rev-parse", "v0.2.0^{commit}"]),
        r.git(&["-C", "origin.git", "rev-parse", "main"])
    );
    assert_eq!(
        r.git(&["-C", "work", "worktree", "list"]).lines().count(),
        1
    );
}

#[test]
fn a_release_without_an_adopter_action_or_a_changelog_is_not_tagged() {
    let r = Remote::new();
    r.changelog("## Unreleased\n\n### Changes\n\n- a\n");
    assert_eq!(r.run(&[]), 1);
    assert_eq!(r.run(&["--dry-run"]), 1);
    r.changelog("## v0.1.0\n\n### Adopter action\n\nNone.\n");
    assert_eq!(r.run(&[]), 1);
    r.git(&["-C", "up", "rm", "--quiet", "CHANGELOG.md"]);
    r.git(&["-C", "up", "commit", "--quiet", "-m", "none"]);
    r.git(&["-C", "up", "push", "--quiet", "origin", "main"]);
    assert_eq!(r.run(&[]), 1);
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn a_newest_tag_off_the_branch_stops_it() {
    let r = Remote::new();
    r.git(&["-C", "up", "checkout", "--quiet", "-b", "side"]);
    r.git(&[
        "-C",
        "up",
        "commit",
        "--quiet",
        "--allow-empty",
        "-m",
        "side",
    ]);
    r.git(&["-C", "up", "tag", "v0.9.0"]);
    r.git(&["-C", "up", "push", "--quiet", "origin", "v0.9.0"]);
    r.git(&["-C", "up", "checkout", "--quiet", "main"]);
    r.changelog("## Unreleased\n\n### Adopter action\n\nNone again.\n");
    assert_eq!(r.run(&[]), 1);
    assert_eq!(r.tags(), "v0.1.0 v0.9.0");
}

#[test]
fn a_remote_that_cannot_be_read_stops_it() {
    let r = Remote::new();
    assert_eq!(r.run(&["--remote", "nowhere"]), 1);
}
