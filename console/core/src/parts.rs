//! The commands of other parts of the tehas family: `tehas <part> [words]`
//! runs the program `tehas-<part>` found on `PATH` with the words, as git
//! and cargo run theirs. The full `tehas` of a factory (src/front.rs) and the
//! front of a machine with no factory (front/) keep this one rule.
//!
//! A word of tehas's own always wins: a part's command is looked for only
//! when the word is no command of tehas. The words of [`FAMILY`] are never a
//! command of tehas, so the parts that hold them can rely on them through
//! 1.x. A word in [`OWN`] names a program of tehas itself and is never
//! looked for. Only an absolute directory of `PATH` is searched, and never
//! one of `skip`, the directories tehas's own programs are in.
//!
//! The spec is docs/specs/2026-11-21-part-commands.md.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The parts of the tehas family. tehas never takes one of these words for
/// a command of its own. Only added to within 1.x.
pub const FAMILY: [&str; 5] = ["industry", "kaizen", "kata", "monozukuri", "yokoten"];

/// Words that name a program of tehas itself as `tehas-<word>`: never a part.
pub const OWN: [&str; 6] = ["contractor", "cycle", "dispatch", "front", "mcp", "proxy"];

/// The longest part name.
const MAX: usize = 32;

/// Whether `word` can name a part: a lowercase letter, then lowercase
/// letters and digits, in groups joined by one hyphen, at most 32 bytes.
pub fn is_name(word: &str) -> bool {
    word.len() <= MAX
        && word.starts_with(|c: char| c.is_ascii_lowercase())
        && !word.ends_with('-')
        && !word.contains("--")
        && word
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The program of the part `word`: the first `tehas-<word>` that is an
/// executable file in an absolute directory of `path` not in `skip`. None
/// for a word that cannot name a part, or one of [`OWN`].
pub fn find(word: &str, path: Option<&OsStr>, skip: &[PathBuf]) -> Option<PathBuf> {
    if !is_name(word) || OWN.contains(&word) {
        return None;
    }
    let program = format!("tehas-{word}");
    std::env::split_paths(path?)
        .filter(|dir| dir.is_absolute())
        .filter(|dir| !skip.iter().any(|s| same_dir(s, dir)))
        .map(|dir| dir.join(&program))
        .find(|p| is_executable(p))
}

/// The parts found on `path`, by name, sorted, each once: for the usage.
pub fn list(path: Option<&OsStr>, skip: &[PathBuf]) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };
    let mut names: Vec<String> = std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_prefix("tehas-")
                .map(str::to_string)
        })
        .filter(|w| find(w, Some(path), skip).is_some())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

fn is_executable(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;
    use std::ffi::OsString;
    use std::fs;

    fn program(dir: &Path, name: &str, mode: u32) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).unwrap();
        p
    }

    fn path(dirs: &[&Path]) -> OsString {
        std::env::join_paths(dirs).unwrap()
    }

    #[test]
    fn a_name_is_lowercase_words_joined_by_one_hyphen() {
        for w in [
            "industry",
            "kaizen",
            "a",
            "part2",
            "two-words",
            &"a".repeat(32),
        ] {
            assert!(is_name(w), "{w}");
        }
        for w in [
            "",
            "Industry",
            "2part",
            "-x",
            "x-",
            "a--b",
            "a_b",
            "a.b",
            "../x",
            "a/b",
            "--help",
            "7",
            &"a".repeat(33),
        ] {
            assert!(!is_name(w), "{w}");
        }
    }

    #[test]
    fn the_first_executable_on_the_path_is_the_part() {
        let (a, b) = (TestDir::new("parts-a"), TestDir::new("parts-b"));
        program(&a, "tehas-kaizen", 0o644);
        let found = program(&b, "tehas-kaizen", 0o755);
        program(&a, "tehas-industry", 0o755);
        let p = path(&[&a, &b]);
        assert_eq!(find("kaizen", Some(&p), &[]), Some(found));
        assert_eq!(
            find("industry", Some(&p), &[]),
            Some(a.join("tehas-industry"))
        );
        assert_eq!(find("yokoten", Some(&p), &[]), None);
        assert_eq!(find("kaizen", None, &[]), None);
    }

    #[test]
    fn a_directory_is_not_a_part() {
        let a = TestDir::new("parts-dir");
        fs::create_dir(a.join("tehas-kata")).unwrap();
        assert_eq!(find("kata", Some(&path(&[&a])), &[]), None);
    }

    #[test]
    fn tehas_own_directories_and_programs_are_never_a_part() {
        let (libexec, bin) = (TestDir::new("parts-libexec"), TestDir::new("parts-bin"));
        program(&libexec, "tehas-industry", 0o755);
        for own in OWN {
            program(&bin, &format!("tehas-{own}"), 0o755);
        }
        let p = path(&[&libexec, &bin]);
        assert_eq!(find("industry", Some(&p), &[libexec.to_path_buf()]), None);
        for own in OWN {
            assert_eq!(find(own, Some(&p), &[]), None, "{own}");
        }
        assert!(list(Some(&p), &[libexec.to_path_buf()]).is_empty());
    }

    #[test]
    fn a_relative_or_empty_directory_of_the_path_is_not_searched() {
        let a = TestDir::new("parts-rel");
        program(&a, "tehas-industry", 0o755);
        let here = std::env::current_dir().unwrap();
        let rel = pathdiff(&a, &here);
        let p = OsString::from(format!(":{}:.", rel.display()));
        assert_eq!(find("industry", Some(&p), &[]), None);
    }

    /// `to` relative to `from`, through the root: enough for the test.
    fn pathdiff(to: &Path, from: &Path) -> PathBuf {
        let ups = from.components().count() - 1;
        let mut p: PathBuf = (0..ups).map(|_| "..").collect();
        p.push(to.strip_prefix("/").unwrap());
        p
    }

    #[test]
    fn the_list_names_each_part_once_in_order() {
        let (a, b) = (TestDir::new("parts-la"), TestDir::new("parts-lb"));
        for d in [&a, &b] {
            program(d, "tehas-kaizen", 0o755);
        }
        program(&a, "tehas-industry", 0o755);
        program(&a, "tehas-Bad", 0o755);
        program(&a, "tehas-cycle", 0o755);
        program(&b, "tehas-notes", 0o644);
        program(&b, "other", 0o755);
        assert_eq!(list(Some(&path(&[&a, &b])), &[]), ["industry", "kaizen"]);
    }

    #[test]
    fn the_family_and_tehas_own_names_are_names_and_apart() {
        for w in FAMILY.iter().chain(&OWN) {
            assert!(is_name(w), "{w}");
        }
        assert!(FAMILY.iter().all(|w| !OWN.contains(w)));
    }
}
