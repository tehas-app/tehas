//! What install.sh knows of the tree it installs from.
//!
//!   console update tree <root>
//!
//! Two lines: `commit <sha>`, the commit the store records, and `version <v>`,
//! the version in the footer of every page and the store's. A checkout that is
//! a git repository of its own answers from git: its HEAD, and the newest tag
//! it holds with the short commit, `<tag>+<hash>`, or the hash alone. A tree
//! with no git of its own, as a release unpacks, answers from the first lines
//! of its COMMIT and VERSION files, as their bytes are. Either may be empty:
//! install.sh judges the commit and writes no footer version for an empty one.
//! Exit 0, 64 usage.

use super::{output, tell};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn verb(given: &[String]) -> u8 {
    let [root] = given else {
        tell("console update tree: usage: <root>");
        return 64;
    };
    let tree = Tree::of(Path::new(root));
    let mut said = Vec::new();
    for (word, value) in [
        (&b"commit "[..], &tree.commit),
        (b"version ", &tree.version),
    ] {
        said.extend_from_slice(word);
        said.extend_from_slice(value);
        said.push(b'\n');
    }
    let _ = io::stdout().write_all(&said);
    0
}

#[derive(Debug, PartialEq)]
struct Tree {
    commit: Vec<u8>,
    version: Vec<u8>,
}

impl Tree {
    fn of(root: &Path) -> Tree {
        if !own_git(root) {
            return Tree {
                commit: first_line(&root.join("COMMIT")),
                version: first_line(&root.join("VERSION")),
            };
        }
        let hash = git(root, &["rev-parse", "--short=7", "HEAD"]);
        let version = match hash {
            None => Vec::new(),
            Some(hash) => {
                let tag = git(root, &["describe", "--tags", "--abbrev=0"]).unwrap_or_default();
                let shown = match tag.is_empty() {
                    true => hash,
                    false => format!("{tag}+{hash}"),
                };
                shown.bytes().filter(|b| footer_byte(*b)).collect()
            }
        };
        Tree {
            commit: git(root, &["rev-parse", "HEAD"])
                .unwrap_or_default()
                .into_bytes(),
            version,
        }
    }
}

/// The first line of `file` as its bytes are, as `head -n 1` gives it.
fn first_line(file: &Path) -> Vec<u8> {
    let text = fs::read(file).unwrap_or_default();
    text.split(|b| *b == b'\n')
        .next()
        .unwrap_or_default()
        .to_vec()
}

/// Whether `root` is the top of a git work tree, not a directory inside one.
fn own_git(root: &Path) -> bool {
    git(root, &["rev-parse", "--show-prefix"]).is_some_and(|p| p.is_empty())
}

fn footer_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-')
}

/// What git printed, or None when it failed.
fn git(root: &Path, words: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).args(words).stderr(Stdio::null());
    let (rc, said) = output(&mut cmd);
    (rc == 0).then_some(said)
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;

    fn run(root: &Path, words: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(words)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {words:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn the_footers_are_given_the_checkouts_newest_tag_and_its_commit() {
        let d = dir("tree-tagged");
        let root = d.0.join("tagged");
        fs::create_dir_all(&root).unwrap();
        // Not a repository of its own: no version.
        assert_eq!(
            Tree::of(&root),
            Tree {
                commit: Vec::new(),
                version: Vec::new()
            }
        );
        run(&root, &["init", "-q", "-b", "main"]);
        // No commit yet: still none.
        assert_eq!(Tree::of(&root).version, b"");
        assert_eq!(Tree::of(&root).commit, b"");
        run(&root, &["commit", "-q", "--allow-empty", "-m", "first"]);
        let short = run(&root, &["rev-parse", "--short=7", "HEAD"]);
        assert_eq!(Tree::of(&root).version, short.as_bytes());
        run(&root, &["tag", "v9.8.7"]);
        run(&root, &["commit", "-q", "--allow-empty", "-m", "second"]);
        let tree = Tree::of(&root);
        let short = run(&root, &["rev-parse", "--short=7", "HEAD"]);
        assert_eq!(tree.version, format!("v9.8.7+{short}").as_bytes());
        assert_eq!(tree.commit, run(&root, &["rev-parse", "HEAD"]).as_bytes());
        assert_eq!(tree.commit.len(), 40);
    }

    #[test]
    fn a_tag_is_shown_with_only_the_characters_a_footer_takes() {
        let d = dir("tree-chars");
        let root = d.0.join("t");
        fs::create_dir_all(&root).unwrap();
        run(&root, &["init", "-q", "-b", "main"]);
        run(&root, &["commit", "-q", "--allow-empty", "-m", "first"]);
        run(&root, &["tag", "v1.0<b>\"x\"&y"]);
        let short = run(&root, &["rev-parse", "--short=7", "HEAD"]);
        assert_eq!(
            Tree::of(&root).version,
            format!("v1.0bxy+{short}").as_bytes()
        );
    }

    #[test]
    fn a_tree_with_no_git_of_its_own_answers_from_commit_and_version() {
        let d = dir("tree-release");
        // A directory inside someone's repository is no checkout of its own.
        let outer = d.0.join("outer");
        let root = outer.join("tree");
        fs::create_dir_all(&root).unwrap();
        run(&outer, &["init", "-q", "-b", "main"]);
        run(&outer, &["commit", "-q", "--allow-empty", "-m", "outer"]);
        let sha = "0123456789abcdef0123456789abcdef01234567";
        fs::write(root.join("COMMIT"), format!("{sha}\nmore\n")).unwrap();
        // As its bytes are, ones that are no UTF-8 too: the footers bound it.
        fs::write(root.join("VERSION"), b"v1.2.3<x>\xff\nmore\n").unwrap();
        assert_eq!(
            Tree::of(&root),
            Tree {
                commit: sha.as_bytes().to_vec(),
                version: b"v1.2.3<x>\xff".to_vec()
            }
        );
        fs::remove_file(root.join("COMMIT")).unwrap();
        fs::remove_file(root.join("VERSION")).unwrap();
        assert_eq!(Tree::of(&root).commit, b"");
        assert_eq!(Tree::of(&root).version, b"");
    }

    #[test]
    fn a_usage_is_64() {
        assert_eq!(verb(&[]), 64);
        assert_eq!(verb(&["a".into(), "b".into()]), 64);
    }
}
