//! Each subcommand over a scratch tree. test/polyfills_test.sh runs the
//! script, and checks this checkout.

use super::*;
use tehas_core::testdir::TestDir;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|a| a.to_string()).collect()
}

// The marker is put together here, so this file marks no line of its own.
fn mark(f: &str) -> String {
    format!("{} {f}", ["#", "polyfill"].join(" "))
}

fn put(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel)).unwrap()
}

/// A tree with a polyfill of someFeature and one of otherFeature, each with
/// its test and a caller that uses it on a line of its own, and a record and
/// an instance that name a marker.
fn tree(name: &str) -> TestDir {
    let t = TestDir::new(name);
    for (f, n) in [("someFeature", 11), ("otherFeature", 12)] {
        put(
            &t,
            &format!("lib/polyfill-{f}.sh"),
            &format!(
                "# shellcheck shell=bash\n# Upstream issue: up-owner/pi-rukas#{n}\n# Upstream PR: up-owner/pi-rukas#{}\ntehas_polyfill_{f}() {{ :; }}\n",
                n + 100
            ),
        );
        put(
            &t,
            &format!("test/polyfill_{f}_test.sh"),
            "#!/usr/bin/env bash\n",
        );
    }
    put(
        &t,
        "libexec/run",
        &format!(
            "#!/usr/bin/env bash\ncase \"$mode\" in\n  native) echo native ;;\n  polyfill) tehas_polyfill_someFeature ;; {}\n  off) echo off ;;\nesac\ntehas_polyfill_otherFeature {}\n",
            mark("someFeature"),
            mark("otherFeature")
        ),
    );
    put(
        &t,
        "docs/notes.md",
        &format!("a line {}\n", mark("someFeature")),
    );
    put(
        &t,
        "instances/a/instance.env",
        &format!("a line {}\n", mark("someFeature")),
    );
    t
}

const SOME: &[u8] = br#"{"features":{"someFeature":true,"otherFeature":false}}"#;

#[test]
fn the_arguments_are_read_as_the_script_read_them() {
    let t = TestDir::new("polyfills-args");
    let caps = t.join("caps.json");
    fs::write(&caps, "{}").unwrap();
    let c = caps.display().to_string();
    let dir = t.path().display().to_string();
    assert_eq!(
        parse(&s(&["list"])),
        Ok(Parsed::Run(Args {
            root: None,
            verb: Verb::List
        }))
    );
    assert_eq!(
        parse(&s(&["--root", &dir, "prune", &c])),
        Ok(Parsed::Run(Args {
            root: Some(t.path().to_path_buf()),
            verb: Verb::Prune(caps.clone())
        }))
    );
    assert_eq!(
        parse(&s(&["expired", &c])),
        Ok(Parsed::Run(Args {
            root: None,
            verb: Verb::Expired(caps.clone())
        }))
    );
    assert_eq!(parse(&s(&["--help"])), Ok(Parsed::Help));
    for bad in [&[][..], &["bogus"], &["list", "extra"], &["check", "x"]] {
        assert_eq!(parse(&s(bad)), Err(Bad::Usage), "{bad:?}");
    }
    let nope = t.join("nope").display().to_string();
    assert_eq!(
        parse(&s(&["expired"])),
        Err(Bad::Say(
            "expired needs a --capabilities answer file".into()
        ))
    );
    assert_eq!(
        parse(&s(&["prune", &nope])),
        Err(Bad::Say("prune needs a --capabilities answer file".into()))
    );
    assert_eq!(
        parse(&s(&["--root", &nope, "list"])),
        Err(Bad::Say("--root needs a directory".into()))
    );
    assert_eq!(
        parse(&s(&["--root"])),
        Err(Bad::Say("--root needs a directory".into()))
    );
}

#[test]
fn a_marker_is_the_name_at_the_end_of_its_line() {
    assert_eq!(
        marker(&format!("x {}", mark("someFeature"))),
        Some("someFeature")
    );
    assert_eq!(
        marker(&format!("x {} \t\r", mark("someFeature"))),
        Some("someFeature")
    );
    assert_eq!(
        marker(&format!("{} {}", mark("one"), mark("two"))),
        Some("two")
    );
    assert_eq!(marker(&format!("{} after", mark("someFeature"))), None);
    assert_eq!(marker(&mark("Upper")), None);
    assert_eq!(marker(&mark("a-b")), None);
    assert_eq!(marker(&mark(&"a".repeat(64))).map(str::len), Some(64));
    assert_eq!(marker(&mark(&"a".repeat(65))), None);
    assert_eq!(marker("no marker here"), None);
}

#[test]
fn an_upstream_line_names_owner_repo_and_number() {
    let code = b"# Upstream issue: up-owner/pi-rukas#7  \n# Upstream issue: other/x#8\n# Upstream PR: bad#9\n";
    assert_eq!(upstream(code, "issue"), "up-owner/pi-rukas#7");
    assert_eq!(upstream(code, "PR"), "");
    for bad in [
        "# Upstream PR: o/r#",
        "# Upstream PR: /r#1",
        "# Upstream PR: o/r#1x",
        "# Upstream PR: o/r/s#1",
        " # Upstream PR: o/r#1",
    ] {
        assert_eq!(upstream(bad.as_bytes(), "PR"), "", "{bad}");
    }
}

#[test]
fn list_names_each_polyfill_its_upstream_issue_and_pull_request() {
    let t = tree("polyfills-list");
    assert_eq!(
        list(&t),
        [
            "otherFeature up-owner/pi-rukas#12 up-owner/pi-rukas#112",
            "someFeature up-owner/pi-rukas#11 up-owner/pi-rukas#111"
        ]
    );
    // A file that cannot name a feature is not a polyfill.
    put(&t, "lib/polyfill-Bad.sh", "");
    put(&t, "lib/polyfill-.sh", "");
    assert_eq!(features(&t), ["otherFeature", "someFeature"]);
    let empty = TestDir::new("polyfills-list-none");
    assert!(list(&empty).is_empty());
}

#[test]
fn list_leaves_a_missing_upstream_line_empty() {
    let t = tree("polyfills-list-missing");
    put(&t, "lib/polyfill-someFeature.sh", "# Upstream PR: o/r#3\n");
    assert_eq!(list(&t)[1], "someFeature  o/r#3");
}

#[test]
fn expired_is_the_polyfills_whose_feature_the_answer_sets_to_true() {
    let t = tree("polyfills-expired");
    assert_eq!(expired(&t, SOME), Some(s(&["someFeature"])));
    assert_eq!(
        expired(&t, br#"{"features":{"someFeature":"true"}}"#),
        Some(vec![])
    );
    assert_eq!(expired(&t, br#"{"features":[]}"#), Some(vec![]));
    assert_eq!(expired(&t, b"{}"), Some(vec![]));
}

#[test]
fn an_answer_that_is_not_an_object_is_refused() {
    let t = tree("polyfills-not-object");
    for bad in [&b"[]"[..], b"not json", b"\"x\"", b"true", b""] {
        assert_eq!(expired(&t, bad), None, "{}", String::from_utf8_lossy(bad));
    }
    let caps = t.join("caps.json");
    fs::write(&caps, "[]").unwrap();
    let a = Args {
        root: None,
        verb: Verb::Prune(caps.clone()),
    };
    assert_eq!(run(&a, &t), 1);
    assert!(t.join("lib/polyfill-someFeature.sh").exists());
    let a = Args {
        root: None,
        verb: Verb::Expired(caps),
    };
    assert_eq!(run(&a, &t), 1);
}

#[test]
fn prune_removes_an_expired_polyfill_its_test_and_its_lines() {
    let t = tree("polyfills-prune");
    let gone = expired(&t, SOME).unwrap();
    let p = prune(&t, &gone).unwrap();
    assert_eq!(
        p.said,
        [
            "libexec/run",
            "lib/polyfill-someFeature.sh",
            "test/polyfill_someFeature_test.sh"
        ]
    );
    assert_eq!(p.changed, [t.join("libexec/run")]);
    assert!(!t.join("lib/polyfill-someFeature.sh").exists());
    assert!(!t.join("test/polyfill_someFeature_test.sh").exists());
    assert!(t.join("lib/polyfill-otherFeature.sh").exists());
    assert!(t.join("test/polyfill_otherFeature_test.sh").exists());
    let run_file = read(&t, "libexec/run");
    assert!(!run_file.contains("someFeature"), "{run_file}");
    assert!(run_file.contains("tehas_polyfill_otherFeature"));
    assert!(run_file.contains("off) echo off ;;"));
    assert!(unparsed(&t, &p.changed).is_empty());
    // The records and the instances are not the tree's code.
    assert!(read(&t, "docs/notes.md").contains("someFeature"));
    assert!(read(&t, "instances/a/instance.env").contains("someFeature"));
    assert!(check(&t).is_empty());
}

#[test]
fn prune_keeps_a_file_without_its_last_newline_as_it_was() {
    let t = tree("polyfills-prune-newline");
    put(
        &t,
        "host/x.conf",
        &format!("keep\n{}\nlast", mark("someFeature")),
    );
    prune(&t, &s(&["someFeature"])).unwrap();
    assert_eq!(read(&t, "host/x.conf"), "keep\nlast");
}

#[test]
fn prune_with_nothing_expired_changes_nothing() {
    let t = tree("polyfills-prune-none");
    let before = read(&t, "libexec/run");
    let a = Args {
        root: None,
        verb: Verb::Prune(t.join("caps.json")),
    };
    fs::write(t.join("caps.json"), r#"{"features":{}}"#).unwrap();
    assert_eq!(run(&a, &t), 0);
    assert_eq!(read(&t, "libexec/run"), before);
    assert_eq!(features(&t).len(), 2);
}

#[test]
fn a_marked_line_inside_a_larger_statement_is_said_when_its_removal_breaks_the_file() {
    let t = tree("polyfills-prune-broken");
    put(
        &t,
        "libexec/broken",
        &format!(
            "#!/usr/bin/env bash\nif true; then {}\n  :\nfi\n",
            mark("someFeature")
        ),
    );
    // A file that is neither named *.sh nor run by bash is not parsed.
    put(
        &t,
        "host/notes.txt",
        &format!("if {}\n", mark("someFeature")),
    );
    let p = prune(&t, &s(&["someFeature"])).unwrap();
    assert_eq!(unparsed(&t, &p.changed), ["libexec/broken"]);
    let t = tree("polyfills-prune-broken-run");
    put(
        &t,
        "libexec/broken.sh",
        &format!("if true; then {}\n  :\nfi\n", mark("someFeature")),
    );
    fs::write(t.join("caps.json"), SOME).unwrap();
    let a = Args {
        root: None,
        verb: Verb::Prune(t.join("caps.json")),
    };
    assert_eq!(run(&a, &t), 1);
}

#[test]
fn check_names_a_polyfill_missing_its_upstream_lines_or_test_and_a_marker_without_a_polyfill() {
    let t = tree("polyfills-check");
    put(
        &t,
        "lib/polyfill-someFeature.sh",
        "# shellcheck shell=bash\n# Upstream PR: up-owner/pi-rukas#111\n",
    );
    put(
        &t,
        "lib/polyfill-otherFeature.sh",
        "# shellcheck shell=bash\n# Upstream issue: up-owner/pi-rukas#12\n",
    );
    fs::remove_file(t.join("test/polyfill_otherFeature_test.sh")).unwrap();
    let mut run_file = read(&t, "libexec/run");
    run_file.push_str(&format!("x {}\n", mark("missingFeature")));
    put(&t, "libexec/run", &run_file);
    // Nor are what is built and the files of a package manager the tree's.
    put(
        &t,
        "console/target/x",
        &format!("{}\n", mark("builtFeature")),
    );
    put(&t, "node_modules/x", &format!("{}\n", mark("builtFeature")));
    // A binary file holds no marked line.
    put(&t, "bin/blob", &format!("\0{}\n", mark("blobFeature")));
    assert_eq!(
        check(&t),
        [
            "lib/polyfill-otherFeature.sh names no upstream pull request",
            "lib/polyfill-otherFeature.sh has no test/polyfill_otherFeature_test.sh",
            "lib/polyfill-someFeature.sh names no upstream issue",
            "libexec/run marks a line for missingFeature, which has no lib/polyfill-missingFeature.sh",
        ]
    );
    let a = Args {
        root: None,
        verb: Verb::Check,
    };
    assert_eq!(run(&a, &t), 1);
}

#[test]
fn a_tree_without_polyfills_passes_check() {
    let t = TestDir::new("polyfills-check-none");
    put(&t, "README.md", "nothing\n");
    assert!(check(&t).is_empty());
    assert_eq!(expired(&t, SOME), Some(vec![]));
}
