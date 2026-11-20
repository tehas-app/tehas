//! The rules of a release's page over text, and what is new in a
//! prerelease over a scratch repository. test/pack_release_test.sh and
//! test/changelog_test.sh run the script.

use super::*;
use std::fs;
use tehas_core::testdir::TestDir;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|a| a.to_string()).collect()
}

#[test]
fn the_arguments_are_a_tag_or_unreleased_and_a_file() {
    assert_eq!(parse(&s(&["--help"])), Ok(Parsed::Help));
    assert_eq!(
        parse(&s(&["v1.0.0-rc.2", "-"])),
        Ok(Parsed::Run(Args {
            name: "v1.0.0-rc.2".into(),
            file: Some("-".into())
        }))
    );
    assert!(parse(&s(&["Unreleased"])).is_ok());
    assert_eq!(parse(&s(&[])), Err(USAGE.into()));
    assert_eq!(parse(&s(&["v1.0.0", "a", "b"])), Err(USAGE.into()));
    for bad in ["v1.0", "v1.0.0-gamma.1", "unreleased"] {
        assert!(parse(&s(&[bad])).unwrap_err().contains("the tag must be"));
    }
}

#[test]
fn tags_sort_by_version_with_prereleases_before_their_release() {
    let mut tags = vec![
        "v1.0.0",
        "v1.0.0-rc.1",
        "v1.0.0-beta.10",
        "v0.32.0",
        "v1.0.0-beta.9",
        "v1.0.0-alpha.2",
        "v0.9.0",
        "v1.0.1-rc.1",
    ];
    tags.sort_by_key(|t| key(t).unwrap());
    assert_eq!(
        tags,
        [
            "v0.9.0",
            "v0.32.0",
            "v1.0.0-alpha.2",
            "v1.0.0-beta.9",
            "v1.0.0-beta.10",
            "v1.0.0-rc.1",
            "v1.0.0",
            "v1.0.1-rc.1"
        ]
    );
    assert_eq!(key("Unreleased"), None);
}

#[test]
fn the_tag_before_is_the_highest_lower_one_the_commit_reaches() {
    let reached = [
        "v0.32.0",
        "v1.0.0-beta.1",
        "v1.0.0-beta.9",
        "v1.0.0-beta.10",
        "v1.0.0-rc.1",
        "notes",
        "v1.0.0-rc.1x",
    ];
    assert_eq!(previous("v1.0.0-rc.1", &reached), Some("v1.0.0-beta.10"));
    assert_eq!(previous("v1.0.0-beta.10", &reached), Some("v1.0.0-beta.9"));
    assert_eq!(previous("v1.0.0-beta.1", &reached), Some("v0.32.0"));
    assert_eq!(previous("v1.0.0", &reached), Some("v1.0.0-rc.1"));
    assert_eq!(
        previous("v1.0.1-beta.1", &["v1.0.0", "v1.0.0-rc.1"]),
        Some("v1.0.0")
    );
    assert_eq!(previous("v0.32.0", &reached), None);
    // A tag of a higher version that the commit reaches is not before it.
    assert_eq!(
        previous("v1.0.0-beta.2", &["v1.0.0-beta.1", "v2.0.0"]),
        Some("v1.0.0-beta.1")
    );
}

#[test]
fn an_item_is_a_bullet_with_its_lines_or_a_paragraph_of_an_adopter_action() {
    let section = "Intro line.\n\n### Changes\n\n- one\n  wrapped\n- two\nlazy\n\n    indented after a blank\n- three\n\n```sh\n# not a heading\n\ncode\n```\n\n### Adopter action\n\nFirst paragraph\nstill first.\n- a list in it\n\nSecond.\n\nNone.\n";
    let b = blocks(section);
    assert_eq!(b.len(), 3);
    assert_eq!(b[0].heading, "");
    assert_eq!(b[0].items, ["Intro line."]);
    assert_eq!(b[1].heading, "### Changes");
    assert_eq!(
        b[1].items,
        [
            "- one\n  wrapped",
            "- two\nlazy\n\n    indented after a blank",
            "- three",
            "```sh\n# not a heading\n\ncode\n```"
        ]
    );
    assert_eq!(b[2].heading, "### Adopter action");
    assert_eq!(
        b[2].items,
        ["First paragraph\nstill first.\n- a list in it", "Second."]
    );
}

#[test]
fn what_is_new_is_the_items_the_tag_before_did_not_have() {
    let before = "### Changes\n\n- old one\n- old two\n  wrapped\n\n### Held back\n\n- held old\n\n### Adopter action\n\nOld action.\n";
    let now = "### Changes\n\n- old one\n- new one\n  wrapped too\n- old two\n  wrapped\n- new two\n\n### Held back\n\n- held old\n\n### Adopter action\n\nOld action.\n\nNew action\nover two lines.\n";
    assert_eq!(
        delta(now, before, "v1.0.0-beta.1"),
        "New since v1.0.0-beta.1.\n\n### Changes\n\n- new one\n  wrapped too\n- new two\n\n### Adopter action\n\nNew action\nover two lines."
    );
}

#[test]
fn an_adopter_action_with_nothing_new_reads_none_and_empty_headings_go() {
    let before = "### Changes\n\n- a\n\n### Adopter action\n\nDo X.\n";
    let now = "### Changes\n\n- a\n- b\n\n### Held back\n\n### Adopter action\n\nDo X.\n";
    assert_eq!(
        delta(now, before, "v0.1.0"),
        "New since v0.1.0.\n\n### Changes\n\n- b\n\n### Adopter action\n\nNone."
    );
    // No Adopter action heading at all still gets one.
    assert_eq!(
        delta("### Changes\n\n- b\n", "", "v0.1.0"),
        "New since v0.1.0.\n\n### Changes\n\n- b\n\n### Adopter action\n\nNone."
    );
    // A lone None. of the tag's own is not news, nor a None. replaced.
    assert_eq!(
        delta(
            "### Adopter action\n\nNone.\n",
            "### Adopter action\n\nNone.\n",
            "v0.1.0"
        ),
        "New since v0.1.0.\n\n### Adopter action\n\nNone."
    );
}

#[test]
fn after_a_release_the_empty_unreleased_makes_everything_new() {
    let now = "### Changes\n\n- first after\n\n### Adopter action\n\nRun it.\n";
    assert_eq!(
        delta(now, "", "v1.0.0"),
        "New since v1.0.0.\n\n### Changes\n\n- first after\n\n### Adopter action\n\nRun it."
    );
}

/// A repository whose CHANGELOG.md is committed and tagged in turn.
struct Repo {
    dir: TestDir,
}

impl Repo {
    fn new() -> Repo {
        let r = Repo {
            dir: TestDir::new("xtask-notes"),
        };
        r.git(&["init", "--quiet", "-b", "main"]);
        r
    }

    fn git(&self, a: &[&str]) {
        let o = Command::new("git")
            .current_dir(&*self.dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(a)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(o.status.success(), "git {a:?}");
    }

    fn tag(&self, text: &str, tag: &str) {
        fs::write(self.dir.join("CHANGELOG.md"), text).unwrap();
        self.git(&["add", "CHANGELOG.md"]);
        self.git(&["commit", "--quiet", "--allow-empty", "-m", tag]);
        self.git(&["tag", tag]);
    }

    fn notes(&self, tag: &str) -> Result<String, Fail> {
        let a = Args {
            name: tag.into(),
            file: None,
        };
        notes(&a, &self.dir.join("CHANGELOG.md"))
    }
}

#[test]
fn a_prerelease_without_a_section_gets_what_is_new_since_the_tag_before() {
    let r = Repo::new();
    r.tag(
        "# Changelog\n\n## Unreleased\n\n## v0.9.0 (2026-01-01)\n\n### Changes\n\n- nine\n\n### Adopter action\n\nNone.\n",
        "v0.9.0",
    );
    r.tag(
        "# Changelog\n\n## Unreleased\n\n### Changes\n\n- a\n\n### Adopter action\n\nDo A.\n\n## v0.9.0 (2026-01-01)\n\n### Changes\n\n- nine\n\n### Adopter action\n\nNone.\n",
        "v1.0.0-beta.1",
    );
    r.tag(
        "# Changelog\n\n## Unreleased\n\n### Changes\n\n- a\n- b\n\n### Adopter action\n\nDo A.\n\n## v0.9.0 (2026-01-01)\n\n### Changes\n\n- nine\n\n### Adopter action\n\nNone.\n",
        "v1.0.0-rc.1",
    );
    assert_eq!(
        r.notes("v1.0.0-beta.1").unwrap(),
        "New since v0.9.0.\n\n### Changes\n\n- a\n\n### Adopter action\n\nDo A."
    );
    assert_eq!(
        r.notes("v1.0.0-rc.1").unwrap(),
        "New since v1.0.0-beta.1.\n\n### Changes\n\n- b\n\n### Adopter action\n\nNone."
    );
    // The release itself has no section of its own in the file: an error.
    let e = r.notes("v1.0.0").unwrap_err().0;
    assert!(e.contains("no section for v1.0.0 in"), "{e}");
    assert!(e.contains("scripts/tag-release.sh names"), "{e}");
    // The file's own section of a tag is what it prints.
    assert_eq!(
        r.notes("v0.9.0").unwrap(),
        "### Changes\n\n- nine\n\n### Adopter action\n\nNone."
    );
    // A tag that is not in the repository cannot be read.
    let e = r.notes("v1.0.0-rc.9").unwrap_err().0;
    assert!(e.contains("cannot be made"), "{e}");
}

#[test]
fn a_tag_before_without_an_unreleased_section_fails_and_prints_nothing() {
    let r = Repo::new();
    r.tag(
        "# Changelog\n\n## v0.9.0\n\n### Changes\n\n- nine\n",
        "v0.9.0",
    );
    r.tag(
        "# Changelog\n\n## Unreleased\n\n### Changes\n\n- a\n\n## v0.9.0\n",
        "v1.0.0-beta.1",
    );
    let e = r.notes("v1.0.0-beta.1").unwrap_err().0;
    assert!(e.contains("at v0.9.0 has no Unreleased section"), "{e}");
}
