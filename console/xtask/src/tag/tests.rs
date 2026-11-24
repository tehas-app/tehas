//! The decisions of the tag over text, and the task itself over a scratch
//! remote with --no-verify. test/tag_release_test.sh runs the script with the
//! gate, the polyfills and the pins, gh and docker stubbed.

use super::*;
use std::fs;
use tehas_core::testdir::TestDir;
use tehas_ports::fake::{FakeSite, World};
use tehas_ports::Write;

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
    assert_eq!(args(&[]).pre, None);
    assert_eq!(args(&["--pre", "beta"]).pre, Some(Pre::Beta));
    assert_eq!(
        parse(&s(&["--pre", "gamma"]), Path::new("/")),
        Err("--pre needs alpha, beta or rc".into())
    );
    assert_eq!(
        parse(&s(&["--pre"]), Path::new("/")),
        Err("--pre needs alpha, beta or rc".into())
    );
    let a = args(&["--local-gate", "--gate-wait", "5"]);
    assert!(a.local_gate && a.verify);
    assert_eq!(a.gate_wait, 5);
    assert!(!Args::default().local_gate);
    assert_eq!(
        parse(&s(&["--gate-wait", "soon"]), Path::new("/")),
        Err("--gate-wait needs a number of minutes".into())
    );
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
fn the_newest_release_passes_over_prereleases() {
    assert_eq!(
        newest("v1.0.0-rc.1\nv1.0.0-beta.2\nv1.0.0-alpha.1\nv0.30.7\n"),
        Some("v0.30.7".into())
    );
}

#[test]
fn the_next_prerelease_counts_on_within_its_stage() {
    let tags = "v1.0.0-alpha.2\nv1.0.0-alpha.10\nv1.0.0-alpha.1\nv0.30.7\nv0.31.0-beta.4\n";
    let next = |tags, stage| next_prerelease(tags, "v1.0.0", stage);
    assert_eq!(
        next(tags, Pre::Alpha),
        Ok(("v1.0.0-alpha.11".into(), Some("v1.0.0-alpha.10".into())))
    );
    assert_eq!(next(tags, Pre::Beta), Ok(("v1.0.0-beta.1".into(), None)));
    assert_eq!(next(tags, Pre::Rc).unwrap().0, "v1.0.0-rc.1");
    assert_eq!(next("", Pre::Alpha), Ok(("v1.0.0-alpha.1".into(), None)));
    // A stage does not go back.
    let beta = "v1.0.0-beta.1\nv1.0.0-alpha.3\n";
    assert_eq!(
        next(beta, Pre::Alpha),
        Err("v1.0.0-beta.1 is out; an alpha of v1.0.0 cannot come after it".into())
    );
    assert_eq!(
        next(beta, Pre::Beta),
        Ok(("v1.0.0-beta.2".into(), Some("v1.0.0-beta.1".into())))
    );
}

#[test]
fn a_named_version_may_skip_releases_but_not_go_back() {
    assert_eq!(
        args(&["--version", "v1.4.0"]).version.as_deref(),
        Some("v1.4.0")
    );
    for bad in [
        &["--version"][..],
        &["--version", "1.4.0"],
        &["--version", "v1.4.0-beta.1"],
    ] {
        assert_eq!(
            parse(&s(bad), Path::new("/")),
            Err("--version needs a release, vX.Y.Z".into())
        );
    }
    let pick = |latest, v| chosen_version(latest, Part::Patch, Some(v));
    assert_eq!(pick(Some("v1.2.0"), "v1.4.0"), Ok("v1.4.0".into()));
    assert_eq!(pick(Some("v1.9.0"), "v1.10.0"), Ok("v1.10.0".into()));
    assert_eq!(pick(None, "v0.3.0"), Ok("v0.3.0".into()));
    assert_eq!(
        pick(Some("v1.2.0"), "v1.2.0"),
        Err("v1.2.0 does not come after v1.2.0".into())
    );
    assert_eq!(
        pick(Some("v1.10.0"), "v1.9.0"),
        Err("v1.9.0 does not come after v1.10.0".into())
    );
    assert_eq!(
        chosen_version(Some("v1.2.0"), Part::Minor, None),
        Ok("v1.3.0".into())
    );
}

#[test]
fn a_named_version_is_tagged_with_its_prerelease() {
    let r = Remote::new();
    r.changelog("## Unreleased\n\n### Changes\n\n- x\n\n### Adopter action\n\nSet X.\n");
    assert_eq!(r.run(&["--version", "v0.1.0"]), 1);
    assert_eq!(r.run(&["--version", "v0.3.0", "--pre", "beta"]), 0);
    assert_eq!(r.tags(), "v0.1.0 v0.3.0-beta.1");
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
fn changes_need_a_line_under_their_heading() {
    assert!(changes("### Changes\n\n- a\n\n### Adopter action\n\nNone."));
    assert!(!changes("### Adopter action\n\nNone."));
    assert!(!changes("### Changes\n\n### Adopter action\n\nNone."));
    assert!(!changes("### Changes\n \t\n"));
    assert!(!changes("### Changeset\n\n- a"));
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
    let issue = |i: &str| i != "i#5";
    let pr = |p: &str| match p {
        "p#1" => Some(Upstream::Open),
        "p#2" => Some(Upstream::Merged),
        "p#3" => Some(Upstream::Closed),
        "p#9" => panic!("an expired polyfill is not asked about"),
        _ => None,
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
    assert_eq!(
        pins_body(
            &[
                "moved jq: 1.8.1 to 1.8.2",
                "kept solid-js 2.0.3: 2.0.4 asks for @solidjs/signals ^2.0.4, and @solidjs/signals would be 2.0.3",
            ],
            "v0.2.0"
        ),
        "Each pinned tool at the newest release of the major version it has, with its checksum, digest or lockfile:\n\n- moved jq: 1.8.1 to 1.8.2\n\nA held tool has a new major version out; moving to it is a person's call. CHANGELOG.md names both.\n\nThese Solid pins stay where they are. A newer release of their major needs a version of another Solid package that this move does not take:\n\n- kept solid-js 2.0.3: 2.0.4 asks for @solidjs/signals ^2.0.4, and @solidjs/signals would be 2.0.3\n\nOpened by scripts/tag-release.sh, which tags v0.2.0 once this is merged."
    );
}

/// A bare remote with one commit tagged v0.1.0 and a clone the task runs in.
/// Its code host is the ports' fake: `code` is the project the remote names,
/// `up` the upstream project of a polyfill, `up-owner/pi-rukas`.
struct Remote {
    dir: TestDir,
    code: World,
    up_world: std::cell::RefCell<World>,
}

/// The project a fixture's remote names.
const PROJECT: &str = "acme/widget";
/// The upstream of the polyfills.
const UP: &str = "up-owner/pi-rukas";

impl Remote {
    fn new() -> Remote {
        let r = Remote {
            dir: TestDir::new("xtask-tag"),
            code: World::new().named(PROJECT),
            up_world: std::cell::RefCell::new(World::new().named(UP)),
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
        let (code, up) = (self.code.clone(), self.up_world.borrow().clone());
        let site = move |p: &str| -> Arc<dyn Site> {
            Arc::new(FakeSite::new(if p == UP { &up } else { &code }))
        };
        let hosts = Hosts {
            project_of: &|_| Some(PROJECT.to_string()),
            site: &site,
        };
        run(&args(&v), &self.dir.join("work"), &hosts, &self.programs())
    }

    /// What the task runs, as the operator's environment would name it: no
    /// console of its own, the task's PATH.
    fn programs(&self) -> Programs {
        let console = self.at("bin/console");
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut bin = self.at("bin").into_os_string();
        bin.push(":");
        bin.push(path);
        Programs {
            console: console.exists().then(|| console.into_os_string()),
            path: Some(bin),
        }
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
    r.changelog("## Unreleased\n\n## v0.2.0 (2026-11-19)\n\n### Changes\n\n- x\n\n### Adopter action\n\nSet X.\n");
    assert_eq!(r.run(&["--minor", "--dry-run"]), 0);
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
fn a_prerelease_is_tagged_on_the_next_version_and_counted_on() {
    let r = Remote::new();
    r.changelog("## Unreleased\n\n### Changes\n\n- x\n\n### Adopter action\n\nNone.\n");
    assert_eq!(r.run(&["--major", "--pre", "alpha", "--dry-run"]), 0);
    assert_eq!(r.tags(), "v0.1.0");
    assert_eq!(r.run(&["--major", "--pre", "alpha"]), 0);
    assert_eq!(r.tags(), "v0.1.0 v1.0.0-alpha.1");
    // The same head again is left as it is.
    assert_eq!(r.run(&["--major", "--pre", "alpha"]), 0);
    assert_eq!(r.tags(), "v0.1.0 v1.0.0-alpha.1");
    r.changelog("## Unreleased\n\n### Changes\n\n- y\n\n### Adopter action\n\nNone.\n");
    assert_eq!(r.run(&["--major", "--pre", "alpha"]), 0);
    assert_eq!(r.run(&["--major", "--pre", "beta"]), 0);
    assert_eq!(
        r.tags(),
        "v0.1.0 v1.0.0-alpha.1 v1.0.0-alpha.2 v1.0.0-beta.1"
    );
    r.changelog("## Unreleased\n\n### Changes\n\n- z\n\n### Adopter action\n\nNone.\n");
    assert_eq!(r.run(&["--major", "--pre", "alpha"]), 1);
    // The release itself still follows the newest release, not a prerelease.
    r.changelog("## Unreleased\n\n## v1.0.0 (2026-11-19)\n\n### Changes\n\n- z\n\n### Adopter action\n\nNone.\n");
    assert_eq!(r.run(&["--major"]), 0);
    assert_eq!(
        r.tags(),
        "v0.1.0 v1.0.0 v1.0.0-alpha.1 v1.0.0-alpha.2 v1.0.0-beta.1"
    );
}

#[test]
fn a_prerelease_is_tagged_on_a_head_that_carries_the_newest_release() {
    let r = Remote::new();
    r.changelog("## Unreleased\n\n### Changes\n\n- x\n\n### Adopter action\n\nNone.\n\n## v0.2.0 (2026-11-19)\n\n### Changes\n\n- x\n\n### Adopter action\n\nNone.\n");
    assert_eq!(r.run(&["--minor"]), 0);
    assert_eq!(r.run(&["--major", "--pre", "alpha"]), 0);
    assert_eq!(r.tags(), "v0.1.0 v0.2.0 v1.0.0-alpha.1");
}

#[test]
fn a_release_without_its_own_section_is_not_tagged_but_named_first() {
    let r = Remote::new();
    r.changelog("## Unreleased\n\n### Changes\n\n- x\n\n### Adopter action\n\nNone.\n");
    // The dry run says what the pull request would do, and tags nothing.
    assert_eq!(r.run(&["--dry-run"]), 0);
    assert_eq!(r.tags(), "v0.1.0");
    // A prerelease needs no section of its own.
    assert_eq!(r.run(&["--pre", "rc"]), 0);
    assert_eq!(r.tags(), "v0.1.0 v0.1.1-rc.1");
}

#[test]
fn a_prerelease_whose_tag_before_has_no_unreleased_is_not_tagged() {
    let r = Remote::new();
    r.changelog("## v0.2.0\n\n### Changes\n\n- x\n\n### Adopter action\n\nNone.\n");
    r.git(&["-C", "up", "tag", "v0.2.0"]);
    r.git(&["-C", "up", "push", "--quiet", "origin", "v0.2.0"]);
    r.changelog(
        "## Unreleased\n\n### Changes\n\n- y\n\n### Adopter action\n\nNone.\n\n## v0.2.0\n",
    );
    assert_eq!(r.run(&["--pre", "beta"]), 1);
    assert_eq!(r.run(&["--pre", "beta", "--dry-run"]), 1);
    assert_eq!(r.tags(), "v0.1.0 v0.2.0");
}

#[test]
fn a_release_without_an_adopter_action_or_a_changelog_is_not_tagged() {
    let r = Remote::new();
    r.changelog("## Unreleased\n\n### Changes\n\n- a\n");
    assert_eq!(r.run(&[]), 1);
    assert_eq!(r.run(&["--dry-run"]), 1);
    // An Adopter action over no changes describes nothing.
    r.changelog("## Unreleased\n\n### Adopter action\n\nNone.\n");
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
    r.changelog("## Unreleased\n\n### Changes\n\n- y\n\n### Adopter action\n\nNone again.\n");
    assert_eq!(r.run(&[]), 1);
    assert_eq!(r.tags(), "v0.1.0 v0.9.0");
}

#[test]
fn a_remote_that_cannot_be_read_stops_it() {
    let r = Remote::new();
    assert_eq!(r.run(&["--remote", "nowhere"]), 1);
}

impl Remote {
    /// A new commit on the remote's main with these files of up/.
    fn files(&self, files: &[(&str, &str)]) {
        for (path, text) in files {
            let p = self.dir.join("up").join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        self.git(&["-C", "up", "add", "-A"]);
        self.git(&["-C", "up", "commit", "--quiet", "-m", "files"]);
        self.git(&["-C", "up", "push", "--quiet", "origin", "main"]);
    }
}

#[test]
fn the_heads_fragments_are_its_md_files_in_issue_order() {
    let r = Remote::new();
    r.files(&[
        ("changelog.d/12.md", "### Changes\n\n- twelve\n"),
        ("changelog.d/3.md", "### Changes\n\n- three\n"),
        ("changelog.d/fix-typo.md", "### Changes\n\n- named\n"),
        ("changelog.d/notes.txt", "not a fragment\n"),
        ("changelog.d/.x.md", "hidden\n"),
    ]);
    let head = r.git(&["-C", "up", "rev-parse", "HEAD"]);
    let got = head_fragments(&r.dir.join("up"), &head).unwrap();
    let names: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        [
            "changelog.d/3.md",
            "changelog.d/12.md",
            "changelog.d/fix-typo.md"
        ]
    );
    assert_eq!(got[0].1, "### Changes\n\n- three\n");
    let first = r.git(&["-C", "up", "rev-list", "--max-parents=0", "HEAD"]);
    assert_eq!(head_fragments(&r.dir.join("up"), &first), Some(Vec::new()));
}

#[test]
fn the_fragments_adopter_action_counts_and_a_bad_fragment_stops_it() {
    let r = Remote::new();
    r.files(&[
        (
            "CHANGELOG.md",
            "## Unreleased\n\n### Changes\n\n- in the file\n",
        ),
        ("changelog.d/3.md", "### Changes\n\n- three\n"),
    ]);
    assert_eq!(r.run(&["--dry-run"]), 1);
    r.files(&[(
        "changelog.d/12.md",
        "### Adopter action\n\n```sh\n# set it\nexport TEHAS_TWELVE=1\n```\n",
    )]);
    assert_eq!(r.run(&["--dry-run"]), 0);
    r.files(&[("changelog.d/7.md", "Text before a heading.\n")]);
    assert_eq!(r.run(&["--dry-run"]), 1);
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn the_pull_request_that_collects_names_the_fragments() {
    assert_eq!(
        fragments_body("Unreleased", &["changelog.d/3.md", "changelog.d/12.md"], "v0.1.1-rc.1", None),
        "The lines of the changelog fragments, moved into the Unreleased section of CHANGELOG.md, and the fragments deleted:\n\n- changelog.d/3.md\n- changelog.d/12.md\n\nOpened by scripts/tag-release.sh, which tags v0.1.1-rc.1 once this is merged."
    );
    assert_eq!(
        fragments_body("Unreleased", &["changelog.d/3.md"], "v0.1.1", Some("2026-11-19")),
        "The Unreleased section of CHANGELOG.md, named v0.1.1 (2026-11-19) for the release's notes, with a new empty Unreleased above it.\n\nThe lines of the changelog fragments, moved into the Unreleased section of CHANGELOG.md, and the fragments deleted:\n\n- changelog.d/3.md\n\nOpened by scripts/tag-release.sh, which tags v0.1.1 once this is merged."
    );
    assert_eq!(
        fragments_body("Unreleased", &[], "v0.1.1", Some("2026-11-19")),
        "The Unreleased section of CHANGELOG.md, named v0.1.1 (2026-11-19) for the release's notes, with a new empty Unreleased above it.\n\nOpened by scripts/tag-release.sh, which tags v0.1.1 once this is merged."
    );
}

#[test]
fn a_tag_pins_the_pi_rukas_the_gate_built_against() {
    let at = |c: char, what: &str| Some(Some((c.to_string().repeat(40), what.to_string())));
    assert_eq!(settle(None, None), Ok(None));
    assert_eq!(
        settle(at('a', "release v0.14.94"), at('a', "release v0.14.94")),
        Ok(Some(("a".repeat(40), "release v0.14.94".to_string())))
    );
    assert_eq!(
        settle(at('a', "release v0.14.94"), at('b', "release v0.14.95")),
        Err("pi-rukas moved from aaaaaaa to bbbbbbb (release v0.14.95) while the gate ran; run again to gate it".to_string())
    );
    let unasked = "cannot ask which pi-rukas commit host/versions.env follows".to_string();
    assert_eq!(settle(Some(None), at('a', "x")), Err(unasked.clone()));
    assert_eq!(settle(at('a', "x"), Some(None)), Err(unasked.clone()));
    assert_eq!(settle(None, at('a', "x")), Err(unasked));
}

#[test]
fn only_an_alpha_or_a_beta_skips_the_proofs() {
    assert!(!needs_proofs(Some(Pre::Alpha)));
    assert!(!needs_proofs(Some(Pre::Beta)));
    assert!(needs_proofs(Some(Pre::Rc)));
    assert!(needs_proofs(None));
}

// --- The polyfills, on the ports' fake (from test/tag_release_test.sh) -------

/// The pi-rukas commit the console's `rukas` names, and the next one.
const RUKAS: &str = "1234567890abcdef1234567890abcdef12345678";
const RUKAS_NEXT: &str = "abcdef1234567890abcdef1234567890abcdef12";

impl Remote {
    fn at(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.at(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, text).unwrap();
    }

    fn exe(&self, rel: &str, text: &str) {
        use std::os::unix::fs::PermissionsExt;
        self.write(rel, text);
        fs::set_permissions(self.at(rel), fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A commit of everything under `up/`, pushed to the remote's main.
    fn commit(&self, message: &str) {
        self.git(&["-C", "up", "add", "-A"]);
        self.git(&["-C", "up", "commit", "--quiet", "-m", message]);
        self.git(&["-C", "up", "push", "--quiet", "origin", "main"]);
    }

    /// What a stub was called with, one call per line.
    fn calls(&self, name: &str) -> String {
        fs::read_to_string(self.at(&format!("calls/{name}"))).unwrap_or_default()
    }

    /// The head the shell's `fresh` made: a section of CHANGELOG.md for each
    /// release a case tags, and a gate that records it ran and ends with
    /// `verify_rc`, after `verify_hook` when there is one.
    fn released(&self) {
        let d = self.dir.display();
        fs::create_dir_all(self.at("calls")).unwrap();
        let mut log = String::from(
            "# Changelog\n\n## Unreleased\n\n### Changes\n\n- first\n\n### Adopter action\n\nNone.\n",
        );
        for v in ["v1.0.1", "v1.0.0", "v0.2.0", "v0.1.3", "v0.1.2", "v0.1.1"] {
            log.push_str(&format!(
                "\n## {v} (2026-01-01)\n\n### Changes\n\n- first\n\n### Adopter action\n\nNone.\n"
            ));
        }
        self.write("up/CHANGELOG.md", &log);
        self.exe(
            "up/scripts/verify.sh",
            &format!(
                "#!/usr/bin/env bash\necho ran >>\"{d}/calls/verify\"\n[ ! -f \"{d}/verify_hook\" ] || . \"{d}/verify_hook\"\nexit \"$(cat \"{d}/verify_rc\" 2>/dev/null || echo 0)\"\n"
            ),
        );
        self.commit("first");
        // The code host knows the default branch the task pushes beside.
        self.code.push("main", "first", "t");
    }

    /// The shell's `polyfilled`: a head that carries a polyfill of
    /// `someFeature`, for the upstream issue #77 and pull request #78, which
    /// the upstream project holds open. The console's `rukas` names RUKAS as
    /// the release versions.env follows, and the image built from it; caps.json
    /// says pi-rukas lacks every feature.
    fn polyfilled(&self) {
        self.released();
        let d = self.dir.display();
        // scripts/polyfills.sh is `xtask polyfills`, tested in
        // src/polyfills.rs; here a stand-in gives its answers for the one
        // polyfill: list it, say it expired when caps.json has its feature,
        // and prune its files and marked line.
        self.exe(
            "up/scripts/polyfills.sh",
            "#!/usr/bin/env bash\nroot=$2 verb=$3 caps=${4:-}\nhas() { grep -q '\"someFeature\":true' \"$caps\"; }\ncase $verb in\n  list) [ ! -f \"$root/lib/polyfill-someFeature.sh\" ] || echo 'someFeature up-owner/pi-rukas#77 up-owner/pi-rukas#78' ;;\n  expired) ! has || echo someFeature ;;\n  prune)\n    has || exit 0\n    rm -f \"$root/lib/polyfill-someFeature.sh\" \"$root/test/polyfill_someFeature_test.sh\"\n    sed -i '/# polyfill someFeature/d' \"$root/libexec/run\"\n    echo lib/polyfill-someFeature.sh ;;\nesac\n",
        );
        self.write(
            "up/host/versions.env",
            "PI_RUKAS_REPO=https://github.com/up-owner/pi-rukas.git\nPI_RUKAS_FOLLOW=release\n",
        );
        self.write(
            "up/lib/polyfill-someFeature.sh",
            "# shellcheck shell=bash\n# Upstream issue: up-owner/pi-rukas#77\n# Upstream PR: up-owner/pi-rukas#78\n",
        );
        self.write(
            "up/test/polyfill_someFeature_test.sh",
            "#!/usr/bin/env bash\n",
        );
        self.write(
            "up/libexec/run",
            "#!/usr/bin/env bash\necho native\necho polyfill # polyfill someFeature\n",
        );
        self.commit("carry a polyfill");
        self.write("caps.json", "{\"features\":{}}");
        self.write("rukas_target", &format!("{RUKAS} release v0.14.0\n"));
        self.exe(
            "bin/console",
            &format!(
                "#!/usr/bin/env bash\necho \"$*\" >>\"{d}/calls/console\"\ncase \"$1 $2 $3\" in\n  \"rukas target release\") cat \"{d}/rukas_target\" ;;\n  \"rukas image \"*) echo \"ghcr.io/up-owner/pi-rukas:sha-${{4:0:12}}\" ;;\n  *) exit 1 ;;\nesac\n"
            ),
        );
        self.upstream(Some(ChangeState::Open));
    }

    /// The upstream project: issue #77, and pull request #78 in `state`, or
    /// none.
    fn upstream(&self, state: Option<ChangeState>) {
        let mut up = World::new().named(UP).ticket(77, |t| t);
        if let Some(state) = state {
            up = up.change(78, "some-feature", |c| c.state(state));
        }
        *self.up_world.borrow_mut() = up;
    }

    /// The task as the script runs it, with the gate here: its status and
    /// what it said.
    fn tag(&self, v: &[&str]) -> (u8, String) {
        let mut v = v.to_vec();
        v.push("--local-gate");
        let code = self.code.clone();
        let up = self.up_world.borrow().clone();
        let site = move |p: &str| -> Arc<dyn Site> {
            Arc::new(FakeSite::new(if p == UP { &up } else { &code }))
        };
        let hosts = Hosts {
            project_of: &|_| Some(PROJECT.to_string()),
            site: &site,
        };
        listen();
        let status = run(&args(&v), &self.dir.join("work"), &hosts, &self.programs());
        (status, heard())
    }

    fn caps(&self) -> String {
        self.at("caps.json").display().to_string()
    }

    /// The pull requests the task opened on the code host.
    fn opened(&self) -> Vec<NewChange> {
        self.code
            .wrote()
            .into_iter()
            .filter_map(|w| match w.what {
                Write::OpenChange(new) => Some(new),
                _ => None,
            })
            .collect()
    }

    /// The links of the pull requests the task opened, as the code host
    /// gives them.
    fn opened_links(&self) -> Vec<String> {
        let site = FakeSite::new(&self.code);
        let mut host = site.code_host(&BY, CALLER).unwrap();
        let open = host
            .changes(
                &ChangeFilter {
                    states: ChangeStates::Open,
                    ..Default::default()
                },
                ALL_MAX,
            )
            .unwrap();
        open.items()
            .iter()
            .map(|d| d.link.as_str().to_string())
            .collect()
    }

    fn origin(&self, a: &[&str]) -> String {
        let mut v = vec!["-C", "origin.git"];
        v.extend_from_slice(a);
        self.git(&v)
    }
}

/// Every case's arrangement says its lines held.
#[track_caller]
fn says(heard: &str, want: &str) {
    assert!(heard.contains(want), "{want:?} not in:\n{heard}");
}

#[test]
fn a_polyfill_whose_feature_pi_rukas_still_lacks_is_kept_and_the_head_is_tagged() {
    let r = Remote::new();
    r.polyfilled();
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 0, "{heard}");
    assert_eq!(r.tags(), "v0.1.0 v0.1.1");
    // The upstream issue and pull request were asked of the code host.
    assert!(r.up_world.borrow().sent() > 0);
    assert!(!heard.contains("is merged"), "{heard}");
    assert_eq!(r.calls("verify"), "ran\n");
    assert_eq!(r.calls("docker"), "");
    let message = r.origin(&["tag", "-l", "--format=%(contents)", "v0.1.1"]);
    assert!(
        message
            .lines()
            .any(|l| l == format!("pi-rukas {RUKAS} release v0.14.0")),
        "{message}"
    );
}

#[test]
fn a_pi_rukas_release_made_while_the_gate_runs_tags_nothing() {
    let r = Remote::new();
    r.polyfilled();
    r.write(
        "verify_hook",
        &format!(
            "echo '{RUKAS_NEXT} release v0.14.1' >\"{}\"\n",
            r.at("rukas_target").display()
        ),
    );
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 1, "{heard}");
    says(&heard, &format!("pi-rukas moved from {} to", &RUKAS[..7]));
    says(
        &heard,
        "(release v0.14.1) while the gate ran; run again to gate it; nothing was tagged",
    );
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn a_polyfill_whose_upstream_pull_request_is_merged_without_the_feature_released_is_said_and_kept()
{
    let r = Remote::new();
    r.polyfilled();
    r.upstream(Some(ChangeState::Merged {
        at: tehas_ports::At::new(1_767_225_600),
        by: None,
        commit: None,
    }));
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 0, "{heard}");
    says(
        &heard,
        &format!(
            "up-owner/pi-rukas#78 is merged, but the pi-rukas of {} does not set features.someFeature",
            r.caps()
        ),
    );
}

#[test]
fn a_polyfill_whose_upstream_pull_request_was_closed_unmerged_or_cannot_be_found_tags_nothing() {
    let closed = ChangeState::Closed {
        at: tehas_ports::At::new(1_767_225_600),
    };
    for (state, why) in [
        (Some(closed), "was closed without a merge"),
        (None, "cannot be found"),
    ] {
        let r = Remote::new();
        r.polyfilled();
        r.upstream(state);
        let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
        assert_eq!(status, 1, "{heard}");
        says(
            &heard,
            "stand in for no work upstream is doing; nothing was tagged",
        );
        says(
            &heard,
            &format!("someFeature: its upstream pull request up-owner/pi-rukas#78 {why}"),
        );
        assert_eq!(r.tags(), "v0.1.0");
    }
}

#[test]
fn a_polyfill_whose_upstream_issue_cannot_be_found_tags_nothing() {
    let r = Remote::new();
    r.polyfilled();
    *r.up_world.borrow_mut() = World::new().named(UP).change(78, "some-feature", |c| c);
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 1, "{heard}");
    says(
        &heard,
        "someFeature: its upstream issue up-owner/pi-rukas#77 cannot be found",
    );
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn an_expired_polyfill_is_removed_even_when_its_upstream_pull_request_is_closed() {
    let r = Remote::new();
    r.polyfilled();
    r.upstream(Some(ChangeState::Closed {
        at: tehas_ports::At::new(1_767_225_600),
    }));
    r.write("caps.json", "{\"features\":{\"someFeature\":true}}");
    r.code.push("remove-polyfills-v0.1.1", "pushed", "t");
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 2, "{heard}");
    says(&heard, "removes them");
}

#[test]
fn pi_rukas_is_asked_through_the_image_of_the_commit_versions_env_follows() {
    let r = Remote::new();
    r.polyfilled();
    r.exe(
        "bin/docker",
        &format!(
            "#!/usr/bin/env bash\necho \"$*\" >>\"{d}/calls/docker\"\n[ \"$1\" != run ] || cat \"{d}/caps.json\"\n",
            d = r.dir.display()
        ),
    );
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 0, "{heard}");
    let image = format!("ghcr.io/up-owner/pi-rukas:sha-{}", &RUKAS[..12]);
    assert_eq!(
        r.calls("docker"),
        format!(
            "pull -q {image}\nrun --rm --init {image} /opt/pi-rukas/bin/pi-rukas-headless --capabilities\n"
        )
    );
}

#[test]
fn a_pi_rukas_that_cannot_be_asked_tags_nothing() {
    let r = Remote::new();
    r.polyfilled();
    r.write(
        "up/host/versions.env",
        "PI_RUKAS_REPO=https://github.com/up-owner/pi-rukas.git\nPI_RUKAS_FOLLOW=nowhere\n",
    );
    r.commit("follow nowhere");
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 1, "{heard}");
    says(&heard, "whether the polyfills of");
    says(&heard, "--rukas-caps <file>");
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn without_tehas_console_the_heads_own_console_is_run_with_cargo() {
    let r = Remote::new();
    r.polyfilled();
    fs::remove_file(r.at("bin/console")).unwrap();
    r.exe(
        "bin/cargo",
        &format!(
            "#!/usr/bin/env bash\necho \"$*\" >>\"{}/calls/cargo\"\nexit 101\n",
            r.dir.display()
        ),
    );
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 1, "{heard}");
    let cargo = r.calls("cargo");
    assert!(
        cargo.contains("run --quiet --locked --manifest-path"),
        "{cargo}"
    );
    assert!(
        cargo.contains(
            "/console/Cargo.toml -p console -- rukas target release https://github.com/up-owner/pi-rukas.git"
        ),
        "{cargo}"
    );
    says(&heard, "whether the polyfills of");
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn a_polyfill_pi_rukas_no_longer_needs_is_removed_in_a_pull_request_and_nothing_is_tagged() {
    let r = Remote::new();
    r.polyfilled();
    r.write("caps.json", "{\"features\":{\"someFeature\":true}}");
    // The branch the task pushes, as the code host then has it.
    r.code.push("remove-polyfills-v0.1.1", "pushed", "t");
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 2, "{heard}");
    says(&heard, "removes them; merge it, then tag");
    says(&heard, "removed from lib/polyfill-someFeature.sh");
    assert_eq!(r.tags(), "v0.1.0");
    assert_eq!(r.calls("verify"), "ran\n");
    let opened = r.opened();
    assert_eq!(opened.len(), 1);
    assert_eq!(
        (opened[0].head.as_str(), opened[0].base.as_str()),
        ("remove-polyfills-v0.1.1", "main")
    );
    assert_eq!(
        opened[0].title,
        "Remove the polyfills pi-rukas no longer needs"
    );
    let Body::Text(body) = &opened[0].body else {
        panic!("a body as text")
    };
    assert!(
        body.contains(
            "features.someFeature, upstream up-owner/pi-rukas#77 and up-owner/pi-rukas#78"
        ),
        "{body}"
    );
    let fixed = r.origin(&["ls-tree", "-r", "--name-only", "remove-polyfills-v0.1.1"]);
    assert!(!fixed.contains("polyfill-someFeature"), "{fixed}");
    assert!(!fixed.contains("polyfill_someFeature"), "{fixed}");
    assert_eq!(
        r.origin(&["show", "remove-polyfills-v0.1.1:libexec/run"]),
        "#!/usr/bin/env bash\necho native"
    );
    assert_eq!(
        r.origin(&["log", "-1", "--format=%s", "remove-polyfills-v0.1.1"]),
        "Remove the polyfills pi-rukas no longer needs"
    );
    assert_eq!(
        r.origin(&["rev-parse", "remove-polyfills-v0.1.1^"]),
        r.origin(&["rev-parse", "main"])
    );
    assert_eq!(
        r.git(&["-C", "work", "worktree", "list"]).lines().count(),
        1
    );
    assert_eq!(
        r.git(&["-C", "work", "branch", "--list", "remove-polyfills-*"]),
        ""
    );
}

#[test]
fn an_open_pull_request_that_removes_polyfills_holds_the_tag_and_nothing_is_pushed_again() {
    let r = Remote::new();
    r.polyfilled();
    r.write("caps.json", "{\"features\":{\"someFeature\":true}}");
    // The world is shared by its clones: the change is the code host's.
    let _ = r.code.clone().change(9, "remove-polyfills-v0.1.1", |c| c);
    let link = format!("https://fake.test/{PROJECT}/c/9");
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 2, "{heard}");
    says(
        &heard,
        &format!("{link} removes polyfills; merge it, then tag"),
    );
    assert!(r.opened().is_empty());
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn a_gate_that_fails_with_the_polyfills_removed_pushes_nothing_and_keeps_a_branch_it_did_not_make()
{
    let r = Remote::new();
    r.polyfilled();
    r.git(&["-C", "work", "branch", "remove-polyfills-v0.1.1"]);
    r.write("caps.json", "{\"features\":{\"someFeature\":true}}");
    r.write("verify_rc", "1");
    let (status, heard) = r.tag(&["--rukas-caps", &r.caps()]);
    assert_eq!(status, 1, "{heard}");
    says(&heard, "the gate failed with the polyfills removed");
    assert_eq!(r.origin(&["branch", "--list", "remove-polyfills-*"]), "");
    assert!(r.opened().is_empty());
    assert!(r
        .git(&["-C", "work", "branch", "--list", "remove-polyfills-*"])
        .contains("remove-polyfills-v0.1.1"));
}

// --- The pinned tools, on the ports' fake (from test/tag_release_test.sh) ----

impl Remote {
    /// The shell's `pinned`: a head with scripts/pins.sh, a stub that records
    /// its arguments and does what `pins_mode` says: move (a line of
    /// host/versions.env changes, and it says so), none, or fail.
    fn pinned(&self) {
        self.released();
        let d = self.dir.display();
        self.write("up/host/versions.env", "JQ_VERSION=1.8.1\n");
        self.exe(
            "up/scripts/pins.sh",
            &format!(
                "#!/usr/bin/env bash\necho \"$*\" >>\"{d}/calls/pins\"\nroot=$(dirname \"$0\")/..\ncase \"$(cat \"{d}/pins_mode\")\" in\n  move) echo JQ_VERSION=1.8.2 >\"$root/host/versions.env\"; echo \"moved jq: 1.8.1 to 1.8.2\" ;;\n  fail) echo \"pins: jq: cannot get\" >&2; exit 1 ;;\nesac\n"
            ),
        );
        self.commit("carry pins");
        self.write("pins_mode", "move");
    }
}

#[test]
fn pinned_tools_that_move_go_to_a_pull_request_and_nothing_is_tagged() {
    let r = Remote::new();
    r.pinned();
    // The branch the task pushes, as the code host then has it.
    r.code.push("pins-v0.1.1", "pushed", "t");
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 2, "{heard}");
    assert_eq!(r.calls("pins"), "move --release v0.1.1\n");
    says(&heard, "tag-release: moved jq: 1.8.1 to 1.8.2");
    let link = &r.opened_links()[0];
    says(
        &heard,
        &format!("{link} moves them; merge it, then tag. Nothing was tagged"),
    );
    assert_eq!(r.tags(), "v0.1.0");
    assert_eq!(r.calls("verify"), "ran\n");
    let opened = r.opened();
    assert_eq!(opened.len(), 1);
    assert_eq!(
        (opened[0].head.as_str(), opened[0].base.as_str()),
        ("pins-v0.1.1", "main")
    );
    assert_eq!(
        opened[0].title,
        "Move the pinned tools to the newest minor and patch"
    );
    let Body::Text(body) = &opened[0].body else {
        panic!("a body as text")
    };
    assert!(body.contains("- moved jq: 1.8.1 to 1.8.2"), "{body}");
    assert_eq!(
        r.origin(&["show", "pins-v0.1.1:host/versions.env"]),
        "JQ_VERSION=1.8.2"
    );
    assert_eq!(
        r.origin(&["rev-parse", "pins-v0.1.1^"]),
        r.origin(&["rev-parse", "main"])
    );
    assert_eq!(
        r.git(&["-C", "work", "worktree", "list"]).lines().count(),
        1
    );
}

#[test]
fn an_open_pull_request_that_moves_the_pinned_tools_holds_the_tag() {
    let r = Remote::new();
    r.pinned();
    let _ = r.code.clone().change(12, "pins-v0.1.1", |c| c);
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 2, "{heard}");
    says(
        &heard,
        &format!("https://fake.test/{PROJECT}/c/12 moves the pinned tools; merge it, then tag"),
    );
    assert_eq!(r.calls("pins"), "");
    assert!(r.opened().is_empty());
    assert_eq!(r.tags(), "v0.1.0");
}

#[test]
fn pinned_tools_that_are_all_at_their_newest_let_the_head_be_tagged() {
    let r = Remote::new();
    r.pinned();
    r.write("pins_mode", "none");
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 0, "{heard}");
    assert_eq!(r.tags(), "v0.1.0 v0.1.1");
    assert_eq!(r.calls("pins"), "move --release v0.1.1\n");
    assert!(r.opened().is_empty());
}

#[test]
fn a_pinned_tool_that_cannot_be_moved_stops_the_release() {
    let r = Remote::new();
    r.pinned();
    r.write("pins_mode", "fail");
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 1, "{heard}");
    // pins.sh's own "pins: jq: cannot get" goes to the task's stderr as it is.
    says(
        &heard,
        "a pinned tool could not be moved; nothing was tagged",
    );
    assert_eq!(r.tags(), "v0.1.0");
    assert_eq!(r.calls("verify"), "");
}

#[test]
fn a_gate_that_fails_with_the_pinned_tools_moved_pushes_nothing() {
    let r = Remote::new();
    r.pinned();
    r.write("verify_rc", "1");
    let (status, heard) = r.tag(&[]);
    assert_eq!(status, 1, "{heard}");
    says(&heard, "the gate failed with the pinned tools moved");
    assert_eq!(r.origin(&["branch", "--list", "pins-*"]), "");
    assert!(r.opened().is_empty());
}

#[test]
fn no_pins_tags_without_moving_the_pinned_tools() {
    let r = Remote::new();
    r.pinned();
    let (status, heard) = r.tag(&["--no-pins"]);
    assert_eq!(status, 0, "{heard}");
    assert_eq!(r.tags(), "v0.1.0 v0.1.1");
    assert_eq!(r.calls("pins"), "");
}
