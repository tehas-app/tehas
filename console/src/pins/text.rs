//! The text edits of a move: versions, the lines of `host/versions.env`, and
//! the release's section of CHANGELOG.md. No network and no files here.

use std::cmp::Ordering;
use std::fmt;

/// A release version X.Y.Z. A pre-release or any other form is not one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ver(pub u64, pub u64, pub u64);

impl Ver {
    pub fn parse(s: &str) -> Option<Ver> {
        let mut parts = s.split('.');
        let mut next = || -> Option<u64> {
            let p = parts.next()?;
            let digits = !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
            digits.then(|| p.parse().ok()).flatten()
        };
        let v = Ver(next()?, next()?, next()?);
        parts.next().is_none().then_some(v)
    }
}

impl fmt::Display for Ver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// What to pin from the versions a source has: the newest of the current
/// major, never below the current pin, and the newest of a later major when
/// there is one, for a person to decide.
pub fn choose(current: Ver, found: &[Ver]) -> (Ver, Option<Ver>) {
    let same = found.iter().filter(|v| v.0 == current.0).max().copied();
    let pin = match same {
        Some(v) if v.cmp(&current) == Ordering::Greater => v,
        _ => current,
    };
    let later = found.iter().filter(|v| v.0 > current.0).max().copied();
    (pin, later)
}

/// The value of `KEY=` in a versions.env text.
pub fn env_get(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(str::to_string)
}

/// The text with the line `KEY=` set to `value`. The line must be there.
pub fn env_set(text: &str, key: &str, value: &str) -> Result<String, String> {
    let mut found = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        if body.strip_prefix(key).is_some_and(|r| r.starts_with('=')) {
            found = true;
            out.push_str(&format!("{key}={value}"));
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    found
        .then_some(out)
        .ok_or_else(|| format!("host/versions.env has no {key}"))
}

/// A pin that moved, in the words of the changelog.
pub struct Moved {
    pub name: String,
    pub from: String,
    pub to: String,
}

/// A newer major version that was not taken.
pub struct Held {
    pub name: String,
    pub current: String,
    pub newest: String,
}

fn moved_prefix(name: &str) -> String {
    format!("- The pinned {name} moves from ")
}

/// The changelog with this release's moves under `### Changes` and what was
/// held back under `### Held back`. The section is the release's own when a
/// person has already named it, else Unreleased. A move named by an earlier
/// run keeps its first version: the line says where the release started.
pub fn changelog(
    text: &str,
    release: Option<&str>,
    moved: &[Moved],
    held: &[Held],
) -> Result<String, String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let heading = |l: &str, name: &str| {
        l.strip_prefix("## ")
            .and_then(|r| r.strip_prefix(name))
            .is_some_and(|r| r.is_empty() || r.starts_with(' '))
    };
    let start = release
        .and_then(|r| lines.iter().position(|l| heading(l, r)))
        .or_else(|| lines.iter().position(|l| heading(l, "Unreleased")))
        .ok_or("CHANGELOG.md has no section for the release or Unreleased")?;
    let end = |lines: &[String]| {
        lines[start + 1..]
            .iter()
            .position(|l| l.starts_with("## "))
            .map_or(lines.len(), |i| start + 1 + i)
    };
    // The bounds of a `### <name>` block in the section: its heading and the
    // line after its last line of text.
    let block = |lines: &[String], name: &str| -> Option<(usize, usize)> {
        let e = end(lines);
        let h = (start + 1..e).find(|&i| lines[i].trim_end() == format!("### {name}"))?;
        let stop = (h + 1..e)
            .find(|&i| lines[i].starts_with("### "))
            .unwrap_or(e);
        let last = (h + 1..stop).rev().find(|&i| !lines[i].trim().is_empty());
        Some((h, last.map_or(h + 1, |i| i + 1)))
    };

    for m in moved {
        let prefix = moved_prefix(&m.name);
        let e = end(&lines);
        let old = (start + 1..e).find(|&i| lines[i].starts_with(&prefix));
        let from = old
            .and_then(|i| {
                lines[i][prefix.len()..]
                    .split(' ')
                    .next()
                    .map(str::to_string)
            })
            .unwrap_or_else(|| m.from.clone());
        let line = format!("{prefix}{from} to {}.", m.to);
        if let Some(i) = old {
            lines[i] = line;
            continue;
        }
        let at = match block(&lines, "Changes") {
            Some((_, after)) => after,
            None => {
                let at = start + 1;
                for (k, l) in ["", "### Changes", ""].into_iter().enumerate() {
                    lines.insert(at + k, l.to_string());
                }
                at + 3
            }
        };
        lines.insert(at, line);
    }

    if let Some((h, after)) = block(&lines, "Held back") {
        let mut stop = after;
        while stop < lines.len() && stop < end(&lines) && lines[stop].trim().is_empty() {
            stop += 1;
        }
        lines.drain(h..stop);
    }
    if !held.is_empty() {
        let mut add = vec!["### Held back".to_string(), String::new()];
        for h in held {
            add.push(format!(
                "- {} {} is out, a new major version. `host/versions.env` keeps {}: the move is a person's call.",
                h.name, h.newest, h.current
            ));
        }
        add.push(String::new());
        let at = match block(&lines, "Adopter action") {
            Some((h, _)) => h,
            None => {
                let e = end(&lines);
                if e == lines.len() {
                    add.insert(0, String::new());
                    add.pop();
                }
                e
            }
        };
        for (k, l) in add.into_iter().enumerate() {
            lines.insert(at + k, l);
        }
    }

    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}
