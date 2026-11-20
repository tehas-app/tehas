//! release-notes: the text of a release's page, from CHANGELOG.md.
//! scripts/release-notes.sh execs it; its header is the usage and the exit
//! statuses, and `--help` prints it.
//!
//! A tag with a section of its own gets that section. A release vX.Y.Z
//! without one gets nothing: scripts/tag-release.sh names the section in its
//! changelog pull request, so a release without one was tagged by hand. A
//! prerelease without one gets what is new since the tag before it: the items
//! of Unreleased at the tag that Unreleased at the previous tag did not have,
//! both read with git. Never the whole Unreleased section: it holds every
//! prerelease's changes since the last release, and grows with each.

use crate::changelog::{collect, list, Refusal};
use crate::tag::section;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::version::{self, Stage};

/// The arguments of the task: the name and the changelog, `-` for stdin.
#[derive(Debug, PartialEq)]
pub struct Args {
    pub name: String,
    pub file: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Help,
    Run(Args),
}

pub const USAGE: &str = "usage: release-notes.sh <tag> [<file>]";

/// The arguments; an error ends 64.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        return Ok(Parsed::Help);
    }
    let (name, file) = match args {
        [name] => (name, None),
        [name, file] => (name, Some(file.clone())),
        _ => return Err(USAGE.into()),
    };
    if !version::is_tag(name) && name != "Unreleased" {
        return Err(format!(
            "the tag must be vX.Y.Z, vX.Y.Z-alpha.N, -beta.N, -rc.N or Unreleased, got: {name}"
        ));
    }
    Ok(Parsed::Run(Args {
        name: name.clone(),
        file,
    }))
}

/// The order of tags: by version, a prerelease before its release, alpha
/// before beta before rc, N as a number. None for a name that is no tag.
pub fn key(tag: &str) -> Option<(u64, u64, u64, u8, u64)> {
    if !version::is_tag(tag) {
        return None;
    }
    let (base, rank, n) = match version::prerelease(tag) {
        Some((base, stage, n)) => {
            let rank = match stage {
                Stage::Alpha => 0,
                Stage::Beta => 1,
                Stage::Rc => 2,
            };
            (base, rank, n.parse().ok()?)
        }
        None => (tag, 3, 0),
    };
    let mut p = base[1..].split('.').map(|p| p.parse::<u64>().ok());
    Some((p.next()??, p.next()??, p.next()??, rank, n))
}

/// The tag before `tag`: the highest among `ancestors` (the tags its commit
/// reaches) that sorts below it.
pub fn previous<'a>(tag: &str, ancestors: &[&'a str]) -> Option<&'a str> {
    let at = key(tag)?;
    ancestors
        .iter()
        .filter_map(|t| key(t).map(|k| (k, *t)))
        .filter(|(k, _)| *k < at)
        .max()
        .map(|(_, t)| t)
}

/// A `### ` heading of a section and its items, in order. The lines before
/// the first heading have the heading "".
#[derive(Debug, PartialEq)]
pub struct Block {
    pub heading: String,
    pub items: Vec<String>,
}

const ACTION: &str = "### Adopter action";

fn blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// The fence a line opens: its character and length.
fn fence(line: &str) -> Option<(char, usize)> {
    let rest = line.trim_start();
    let c = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = rest.len() - rest.trim_start_matches(c).len();
    (n >= 3).then_some((c, n))
}

/// The items of a section. Under "### Adopter action" an item is a
/// paragraph; under any other heading it is a bullet with its continuation
/// lines. A line that is indented, or in a fenced code block, continues the
/// item it follows, blank lines between included. A lone "None." is no item.
pub fn blocks(section: &str) -> Vec<Block> {
    let mut out = vec![Block {
        heading: String::new(),
        items: Vec::new(),
    }];
    let mut cur: Vec<&str> = Vec::new();
    let mut blanks = 0;
    let mut open: Option<(char, usize)> = None;
    let flush = |cur: &mut Vec<&str>, out: &mut Vec<Block>| {
        let block = out.last_mut().expect("a block");
        let text = item_text(cur);
        if !text.is_empty() && !(block.heading == ACTION && text == "None.") {
            block.items.push(text);
        }
        cur.clear();
    };
    for line in section.lines() {
        if let Some((c, n)) = open {
            cur.extend(std::iter::repeat_n("", blanks));
            blanks = 0;
            cur.push(line);
            if fence(line).is_some_and(|(d, m)| d == c && m >= n) {
                open = None;
            }
            continue;
        }
        if line.starts_with("### ") {
            flush(&mut cur, &mut out);
            out.push(Block {
                heading: line.trim_end().to_string(),
                items: Vec::new(),
            });
            blanks = 0;
            continue;
        }
        if blank(line) {
            blanks += 1;
            continue;
        }
        let indented = line.starts_with([' ', '\t']);
        let bullet = line.starts_with("- ") || line.starts_with("* ");
        let action = out.last().is_some_and(|b| b.heading == ACTION);
        let starts = !indented && (blanks > 0 || (bullet && !action));
        if cur.is_empty() || starts {
            flush(&mut cur, &mut out);
        } else {
            cur.extend(std::iter::repeat_n("", blanks));
        }
        blanks = 0;
        cur.push(line);
        open = fence(line);
    }
    flush(&mut cur, &mut out);
    out
}

/// An item's lines, each without trailing space: what two versions of a
/// changelog are compared by.
fn item_text(lines: &[&str]) -> String {
    let v: Vec<&str> = lines.iter().map(|l| l.trim_end()).collect();
    v.join("\n").trim_matches('\n').to_string()
}

/// The page of a prerelease: the items of `now` (its Unreleased section)
/// that `before` (Unreleased at the previous tag) did not have, under their
/// headings in order, empty headings left out, and "None." under an Adopter
/// action with nothing new. The first line names the tag before.
pub fn delta(now: &str, before: &str, since: &str) -> String {
    let old: HashSet<String> = blocks(before).into_iter().flat_map(|b| b.items).collect();
    let mut parts = vec![format!("New since {since}.")];
    let mut action = false;
    for b in blocks(now) {
        let new: Vec<&String> = b.items.iter().filter(|i| !old.contains(*i)).collect();
        let is_action = b.heading == ACTION;
        action |= is_action;
        if new.is_empty() && !is_action {
            continue;
        }
        if !b.heading.is_empty() {
            parts.push(b.heading.clone());
        }
        if new.is_empty() {
            parts.push("None.".into());
            continue;
        }
        let bullet = |i: &str| i.starts_with("- ") || i.starts_with("* ");
        let mut body = String::new();
        for (k, item) in new.iter().enumerate() {
            if k > 0 {
                let list = !is_action && bullet(item) && bullet(new[k - 1]);
                body += if list { "\n" } else { "\n\n" };
            }
            body += item;
        }
        parts.push(body);
    }
    if !action {
        parts.push(ACTION.into());
        parts.push("None.".into());
    }
    parts.join("\n\n")
}

/// Why a page cannot be made: said after "release-notes: ", exit 1.
#[derive(Debug, PartialEq)]
pub struct Fail(pub String);

/// What git says in `dir`, None when it fails.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let o = Command::new("git")
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// The tag before `tag` in the repository at `dir`, None when it has none.
pub fn previous_tag(dir: &Path, tag: &str) -> Result<Option<String>, Fail> {
    let merged = git(dir, &["tag", "--list", "v*", "--merged", tag])
        .ok_or_else(|| Fail(format!("cannot list the tags {tag} reaches")))?;
    let tags: Vec<&str> = merged.lines().collect();
    Ok(previous(tag, &tags).map(str::to_string))
}

/// The Unreleased section of the changelog `name` (a path git reads from
/// `dir`, such as ./CHANGELOG.md) at `rev`.
fn unreleased_at(dir: &Path, rev: &str, name: &str) -> Result<String, Fail> {
    let text = git(dir, &["show", &format!("{rev}:{name}")])
        .ok_or_else(|| Fail(format!("cannot read {name} at {rev}")))?;
    section(&text, "Unreleased")
        .ok_or_else(|| Fail(format!("{name} at {rev} has no Unreleased section")))
}

/// What is new in the prerelease `tag` since the tag before it, read with
/// git from the repository at `dir`, the changelog being `name` there.
pub fn since_previous(dir: &Path, tag: &str, name: &str) -> Result<String, Fail> {
    let now = unreleased_at(dir, tag, name)?;
    match previous_tag(dir, tag)? {
        Some(prev) => {
            let before = unreleased_at(dir, &prev, name)?;
            Ok(delta(&now, &before, &prev))
        }
        None => Ok(delta(&now, "", "the start")),
    }
}

/// The page for `a`. `default` is the changelog when no file is given.
pub fn notes(a: &Args, default: &Path) -> Result<String, Fail> {
    let stdin = a.file.as_deref() == Some("-");
    let file = a
        .file
        .as_ref()
        .map_or_else(|| default.to_path_buf(), PathBuf::from);
    let (where_, text) = if stdin {
        let mut t = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut t)
            .map_err(|_| Fail("cannot read standard input".into()))?;
        ("standard input".to_string(), t)
    } else {
        let t = std::fs::read_to_string(&file)
            .map_err(|_| Fail(format!("cannot read {}", file.display())))?;
        (file.display().to_string(), t)
    };
    let dir = match file.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let text = if a.name == "Unreleased" && !stdin {
        with_fragments(&text, &dir)?
    } else {
        text
    };
    if let Some(s) = section(&text, &a.name) {
        return Ok(s);
    }
    let missing = format!("no section for {} in {where_}", a.name);
    if !version::is_prerelease(&a.name) {
        let fix = if a.name == "Unreleased" {
            String::new()
        } else {
            format!(
                "; a release's page is its own section, \"## {} (YYYY-MM-DD)\", which scripts/tag-release.sh names in its changelog pull request",
                a.name
            )
        };
        return Err(Fail(format!("{missing}{fix}")));
    }
    if stdin {
        return Err(Fail(format!(
            "{missing}, and what is new since the tag before it is read with git, not from standard input"
        )));
    }
    let name = format!(
        "./{}",
        file.file_name().unwrap_or_default().to_string_lossy()
    );
    since_previous(&dir, &a.name, &name).map_err(|Fail(m)| {
        Fail(format!(
            "{missing}, and what is new since the tag before it cannot be made: {m}"
        ))
    })
}

/// The changelog with the fragments of changelog.d/ in `dir` collected into
/// its Unreleased section.
fn with_fragments(text: &str, dir: &Path) -> Result<String, Fail> {
    let paths = list(&dir.join("changelog.d")).map_err(Fail)?;
    let mut fragments = Vec::new();
    for (name, path) in paths {
        let t = std::fs::read_to_string(&path).map_err(|_| Fail(format!("cannot read {name}")))?;
        fragments.push((name, t));
    }
    if fragments.is_empty() {
        return Ok(text.to_string());
    }
    match collect(text, "Unreleased", &fragments) {
        Ok(t) => Ok(t),
        Err(Refusal::NoSection) => Ok(text.to_string()),
        Err(Refusal::Fragment(m)) => Err(Fail(m)),
    }
}

/// The task: prints the page, says a failure; the exit status.
pub fn run(a: &Args, default: &Path) -> u8 {
    match notes(a, default) {
        Ok(s) => {
            if !s.is_empty() {
                tehas_core::outln!("{s}");
            }
            0
        }
        Err(Fail(m)) => {
            eprintln!("release-notes: {m}");
            1
        }
    }
}

#[cfg(test)]
mod tests;
