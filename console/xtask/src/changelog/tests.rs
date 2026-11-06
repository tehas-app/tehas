//! The collection over text, and the task over scratch directories.
//! test/changelog_test.sh runs the script and scripts/release-notes.sh.

use super::*;
use tehas_core::testdir::TestDir;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|a| a.to_string()).collect()
}

fn lines(v: &[&str]) -> String {
    let mut t = v.join("\n");
    t.push('\n');
    t
}

const LOG: &[&str] = &[
    "# Changelog",
    "",
    "## Unreleased",
    "",
    "### Changes",
    "",
    "- in the file",
    "",
    "### Adopter action",
    "",
    "None.",
    "",
    "## v1.2.3 (2026-10-21)",
    "",
    "### Changes",
    "",
    "- old",
    "",
    "### Adopter action",
    "",
    "Set TEHAS_X.",
];

fn frag(path: &str, text: &str) -> (String, String) {
    (path.to_string(), text.to_string())
}

#[test]
fn the_arguments_are_read_in_order() {
    let run = |v: &[&str]| match parse(&s(v)).unwrap() {
        Parsed::Run(a) => a,
        Parsed::Help => panic!("help"),
    };
    assert_eq!(run(&[]).section, "Unreleased");
    let a = run(&["--print", "--section", "v1.0.0", "log.md"]);
    assert!(a.print);
    assert_eq!(
        (a.section.as_str(), a.file),
        ("v1.0.0", Some(PathBuf::from("log.md")))
    );
    assert_eq!(run(&["--name", "fix/x"]).name.as_deref(), Some("fix/x"));
    assert_eq!(
        parse(&s(&["--section"])),
        Err("--section needs a name".into())
    );
    assert_eq!(
        parse(&s(&["--section", ""])),
        Err("--section needs a name".into())
    );
    assert_eq!(
        parse(&s(&["--bogus"])),
        Err("unknown argument: --bogus".into())
    );
    assert_eq!(parse(&s(&["a", "b"])), Err("one changelog only".into()));
    assert_eq!(parse(&s(&["--help", "--bogus"])), Ok(Parsed::Help));
}

#[test]
fn a_branch_names_its_fragment_with_each_slash_a_dash() {
    assert_eq!(fragment_name("fix/typo").as_deref(), Some("fix-typo.md"));
    assert_eq!(fragment_name("a/b/c").as_deref(), Some("a-b-c.md"));
    assert_eq!(fragment_name("plain").as_deref(), Some("plain.md"));
    for bad in ["", ".hidden", "with space", "tab\there"] {
        assert_eq!(fragment_name(bad), None, "{bad:?}");
    }
}

#[test]
fn issues_come_first_by_number_then_names_by_name() {
    let mut v = s(&["my-branch.md", "12.md", "3.md", "a.md", "100.md", "012x.md"]);
    order(&mut v);
    assert_eq!(
        v,
        s(&["3.md", "12.md", "100.md", "012x.md", "a.md", "my-branch.md"])
    );
}

#[test]
fn two_fragments_go_under_their_headings_in_issue_order() {
    let out = collect(
        &lines(LOG),
        "Unreleased",
        &[
            frag(
                "d/3.md",
                "### Adopter action\n\nRun tehas update.\n\n### Changes\n\n- from three\n",
            ),
            frag(
                "d/12.md",
                "### Changes\n\n- from twelve\n  wrapped\n\n### Adopter action\n\nSet TEHAS_TWELVE.\n",
            ),
            frag("d/my-branch.md", "### Changes\n- from a branch\n"),
        ],
    )
    .unwrap();
    let mut want = LOG.to_vec();
    want.splice(
        6..11,
        [
            "- in the file",
            "- from three",
            "- from twelve",
            "  wrapped",
            "- from a branch",
            "",
            "### Adopter action",
            "",
            "Run tehas update.",
            "",
            "Set TEHAS_TWELVE.",
        ],
    );
    assert_eq!(out, lines(&want));
}

#[test]
fn a_section_without_the_headings_gets_them_and_text_beside_an_action_stays() {
    let log = lines(&[
        "# Changelog",
        "",
        "## Unreleased",
        "",
        "## v1.2.3 (2026-10-21)",
        "",
        "### Adopter action",
        "",
        "Set TEHAS_X.",
    ]);
    let f = [frag(
        "5.md",
        "### Changes\n\n- five\n\n### Adopter action\n\nFive.\n",
    )];
    assert_eq!(
        collect(&log, "Unreleased", &f).unwrap(),
        lines(&[
            "# Changelog",
            "",
            "## Unreleased",
            "",
            "### Changes",
            "",
            "- five",
            "",
            "### Adopter action",
            "",
            "Five.",
            "",
            "## v1.2.3 (2026-10-21)",
            "",
            "### Adopter action",
            "",
            "Set TEHAS_X.",
        ])
    );
    assert_eq!(
        collect(&log, "v1.2.3", &f).unwrap(),
        lines(&[
            "# Changelog",
            "",
            "## Unreleased",
            "",
            "## v1.2.3 (2026-10-21)",
            "",
            "### Changes",
            "",
            "- five",
            "",
            "### Adopter action",
            "",
            "Set TEHAS_X.",
            "",
            "Five.",
        ])
    );
    assert_eq!(collect(&log, "v9.9.9", &f), Err(Refusal::NoSection));
}

#[test]
fn text_outside_the_two_headings_is_refused() {
    let refused = |t: &str| read_fragment("7.md", t).unwrap_err();
    assert!(refused("A line.\n\n### Changes\n\n- seven\n").starts_with("7.md line 1: text outside"));
    assert!(refused("### Changes\n\n- seven\n\n### Notes\n\n- no\n").starts_with("7.md line 5"));
    assert!(refused("## Changes\n").starts_with("7.md line 1"));
    assert!(read_fragment("7.md", "\n  \n### Changes\n- x\n").is_ok());
}

#[test]
fn a_fenced_code_block_is_text_not_headings() {
    let f = read_fragment(
        "9.md",
        "### Adopter action\n\nRun:\n\n```sh\n# a comment\n#!/bin/sh\ntehas update\n```\n\n~~~~\n#### deep\n~~~\n~~~~\n",
    )
    .unwrap();
    assert_eq!(
        f.action,
        s(&[
            "Run:",
            "",
            "```sh",
            "# a comment",
            "#!/bin/sh",
            "tehas update",
            "```",
            "",
            "~~~~",
            "#### deep",
            "~~~",
            "~~~~",
        ])
    );
    let out = collect(
        &lines(LOG),
        "Unreleased",
        &[frag("9.md", "### Adopter action\n\n```\n# x\n```\n")],
    )
    .unwrap();
    assert!(
        out.contains("### Adopter action\n\n```\n# x\n```\n\n## v1.2.3"),
        "{out}"
    );
}

#[test]
fn a_fenced_line_that_reads_as_a_changelog_heading_or_an_open_fence_is_refused() {
    let e = read_fragment("9.md", "### Changes\n\n```\n## no\n```\n").unwrap_err();
    assert!(
        e.starts_with("9.md line 4: a line of the code block from line 3"),
        "{e}"
    );
    let e = read_fragment("9.md", "### Changes\n\n```\n### no\n```\n").unwrap_err();
    assert!(e.starts_with("9.md line 4"), "{e}");
    let e = read_fragment("9.md", "### Changes\n\n```\n# open\n").unwrap_err();
    assert_eq!(e, "9.md line 3: the code block is not closed");
    // A shorter or other fence does not close it.
    let e = read_fragment("9.md", "### Changes\n\n````\n```\n~~~~\n").unwrap_err();
    assert_eq!(e, "9.md line 3: the code block is not closed");
}

#[test]
fn an_empty_fragment_changes_nothing() {
    assert_eq!(
        collect(&lines(LOG), "Unreleased", &[frag("1.md", "")]).unwrap(),
        lines(LOG)
    );
}

#[test]
fn the_task_rewrites_deletes_and_names_or_prints() {
    let d = TestDir::new("xtask-changelog");
    let log = d.join("CHANGELOG.md");
    fs::write(&log, lines(LOG)).unwrap();
    fs::create_dir(d.join("changelog.d")).unwrap();
    fs::write(d.join("changelog.d/2.md"), "### Changes\n\n- two\n").unwrap();
    fs::write(d.join("changelog.d/.hidden.md"), "junk\n").unwrap();
    fs::write(d.join("changelog.d/notes.txt"), "junk\n").unwrap();
    let mut a = Args {
        section: "Unreleased".into(),
        print: true,
        ..Args::default()
    };
    assert_eq!(run(&a, &log), 0);
    assert_eq!(fs::read_to_string(&log).unwrap(), lines(LOG));
    a.print = false;
    a.section = "v0.0.1".into();
    assert_eq!(run(&a, &log), 1);
    a.section = "Unreleased".into();
    assert_eq!(run(&a, &log), 0);
    assert!(fs::read_to_string(&log)
        .unwrap()
        .contains("- in the file\n- two\n"));
    assert!(!d.join("changelog.d/2.md").exists());
    assert!(d.join("changelog.d/notes.txt").exists());
    a.file = Some(d.join("none.md"));
    assert_eq!(run(&a, &log), 1);
}
