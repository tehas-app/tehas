//! A comment of the workspace that names `libexec/<x>` names a file that is
//! there: a script under libexec/ or a program install.sh puts in it.

use std::fs;
use std::path::{Path, PathBuf};

/// The repository: the workspace is its console/.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The programs install.sh copies into libexec/, from its list of them.
fn installed(root: &Path) -> Vec<String> {
    let install = fs::read_to_string(root.join("install.sh")).unwrap();
    let line = install
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("compiled=(") && l.contains("console"))
        .expect("install.sh has its list of the programs");
    line["compiled=(".len()..]
        .split(')')
        .next()
        .unwrap()
        .split_whitespace()
        .map(String::from)
        .collect()
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                sources(&path, out);
            }
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// The comment of a line: all of it after `//`, where `//` starts the line or
/// follows a space, so a URL in a string is not one.
fn comment(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with("//") {
        return Some(t);
    }
    line.find(" //").map(|i| &line[i..])
}

/// The names after each `libexec/` in a comment, less a full stop that ends
/// the sentence.
fn named(comment: &str) -> Vec<&str> {
    comment
        .match_indices("libexec/")
        .map(|(i, m)| {
            let rest = &comment[i + m.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || "-_.".contains(c)))
                .unwrap_or(rest.len());
            rest[..end].trim_end_matches('.')
        })
        .filter(|n| !n.is_empty())
        .collect()
}

#[test]
fn a_name_in_a_comment_is_read_after_libexec() {
    assert_eq!(named("// as libexec/store does."), ["store"]);
    assert_eq!(
        named("// `libexec/tehas-cycle`'s, libexec/"),
        ["tehas-cycle"]
    );
    assert_eq!(
        comment("let u = \"https://x\"; // libexec/store"),
        Some(" // libexec/store")
    );
    assert_eq!(comment("let u = \"https://libexec/a\";"), None);
}

#[test]
fn a_comment_names_only_what_is_in_libexec() {
    let root = root();
    let programs = installed(&root);
    assert!(programs.iter().any(|p| p == "store"), "{programs:?}");
    let mut files = Vec::new();
    sources(&root.join("console"), &mut files);
    let mut gone = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            for name in comment(line).map(named).unwrap_or_default() {
                if !root.join("libexec").join(name).is_file() && !programs.iter().any(|p| p == name)
                {
                    let at = file.strip_prefix(&root).unwrap().display();
                    gone.push(format!("{at}:{}: libexec/{name}", n + 1));
                }
            }
        }
    }
    assert!(
        gone.is_empty(),
        "comments name files that are not there:\n{}",
        gone.join("\n")
    );
}
