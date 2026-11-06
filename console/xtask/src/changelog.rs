//! collect-changelog: move the changelog fragments into a section of
//! CHANGELOG.md. scripts/collect-changelog.sh execs it; its header is the
//! usage and the exit statuses, and `--help` prints it.
//!
//! A pull request adds its lines in a file of its own, changelog.d/<issue>.md
//! beside CHANGELOG.md, or changelog.d/<branch>.md with each `/` of the branch
//! a `-` when there is no issue. A fragment has up to two sections,
//! "### Changes" and "### Adopter action"; text outside them is refused. A
//! line in a fenced code block is text, not a heading, except one that starts
//! "## " or "### ": that would read as a heading of CHANGELOG.md once
//! collected.
//!
//! Fragments are taken by issue number, then the named ones by name. Each
//! one's Changes go at the end of the section's "### Changes", and each one's
//! Adopter action is a paragraph at the end of its "### Adopter action", in
//! place of a lone "None.". A heading the section lacks is made.
//!
//! scripts/release-notes.sh reads the Unreleased section with the fragments
//! in it from `--print`; `xtask tag-release` counts their Adopter action with
//! [`collect`] and has the head's own script move them before it tags.

use std::fs;
use std::path::{Path, PathBuf};

/// The arguments of the task.
#[derive(Debug, PartialEq, Default)]
pub struct Args {
    pub print: bool,
    pub section: String,
    pub file: Option<PathBuf>,
    /// --name: print the fragment's path for this branch, and do nothing else.
    pub name: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Help,
    Run(Args),
}

/// The arguments as the script's loop read them; an error ends 64.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut a = Args {
        section: "Unreleased".into(),
        ..Args::default()
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--print" => a.print = true,
            "--section" => match it.next() {
                Some(s) if !s.is_empty() => a.section = s.clone(),
                _ => return Err("--section needs a name".into()),
            },
            "--name" => match it.next() {
                Some(s) => a.name = Some(s.clone()),
                None => return Err("--name needs a branch".into()),
            },
            "-h" | "--help" => return Ok(Parsed::Help),
            other if other.starts_with('-') => return Err(format!("unknown argument: {other}")),
            other => {
                if a.file.is_some() {
                    return Err("one changelog only".into());
                }
                a.file = Some(PathBuf::from(other));
            }
        }
    }
    Ok(Parsed::Run(a))
}

/// The file name of the fragment of a branch with no issue: the branch with
/// each `/` a `-`, then `.md`. None for a name that cannot be one.
pub fn fragment_name(branch: &str) -> Option<String> {
    let name = branch.replace('/', "-");
    let bad = name.is_empty()
        || name.starts_with('.')
        || name.chars().any(|c| c.is_whitespace() || c.is_control());
    (!bad).then(|| format!("{name}.md"))
}

/// The lines of a text as awk reads its records.
fn records(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') || text.is_empty() {
        v.pop();
    }
    v
}

/// A character of `[[:space:]]`.
fn space(c: char) -> bool {
    c == ' ' || ('\t'..='\r').contains(&c)
}

fn blank(line: &str) -> bool {
    line.chars().all(space)
}

/// `line` is `head` with only space after it.
fn is_heading(line: &str, head: &str) -> bool {
    line.strip_prefix(head)
        .is_some_and(|r| r.chars().all(space))
}

/// The fence a line opens or closes: its character and length, after up to
/// three spaces, as CommonMark has it.
fn fence(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let c = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = rest.len() - rest.trim_start_matches(c).len();
    (n >= 3 && !(c == '`' && rest[n..].contains('`'))).then_some((c, n))
}

/// What one fragment adds: its Changes lines and its Adopter action lines,
/// each without blank lines at its ends.
#[derive(Debug, PartialEq, Default)]
pub struct Fragment {
    pub changes: Vec<String>,
    pub action: Vec<String>,
}

/// A fragment's text, read; `path` names it in an error.
pub fn read_fragment(path: &str, text: &str) -> Result<Fragment, String> {
    let outside = |n: usize| {
        format!("{path} line {n}: text outside \"### Changes\" and \"### Adopter action\"")
    };
    let mut changes = Vec::new();
    let mut action = Vec::new();
    let mut at: Option<bool> = None; // Some(true): Changes; Some(false): Adopter action
    let mut open: Option<(char, usize, usize)> = None;
    for (i, line) in records(text).into_iter().enumerate() {
        let n = i + 1;
        if let Some((c, len, from)) = open {
            if line.starts_with("## ") || line.starts_with("### ") {
                return Err(format!(
                    "{path} line {n}: a line of the code block from line {from} starts like a heading of CHANGELOG.md; indent the block's lines"
                ));
            }
            if fence(line)
                .is_some_and(|(d, l)| d == c && l >= len && blank(&line.trim_start()[l..]))
            {
                open = None;
            }
        } else if line.starts_with('#') {
            if is_heading(line, "### Changes") {
                at = Some(true);
            } else if is_heading(line, "### Adopter action") {
                at = Some(false);
            } else {
                return Err(outside(n));
            }
            continue;
        } else if let Some((c, len)) = fence(line) {
            open = Some((c, len, n));
        }
        match at {
            None if !line.trim_matches([' ', '\t']).is_empty() => return Err(outside(n)),
            None => {}
            Some(true) => changes.push(line.to_string()),
            Some(false) => action.push(line.to_string()),
        }
    }
    if let Some((_, _, from)) = open {
        return Err(format!("{path} line {from}: the code block is not closed"));
    }
    Ok(Fragment {
        changes: trim(changes),
        action: trim(action),
    })
}

fn trim(lines: Vec<String>) -> Vec<String> {
    let text = |l: &String| !l.trim_matches([' ', '\t']).is_empty();
    match (lines.iter().position(text), lines.iter().rposition(text)) {
        (Some(f), Some(l)) => lines[f..=l].to_vec(),
        _ => Vec::new(),
    }
}

/// The order fragments are taken in: issue numbers first, by number, then the
/// named ones by name.
pub fn order(names: &mut [String]) {
    let issue = |n: &str| {
        n.strip_suffix(".md")
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .map(|s| s.trim_start_matches('0').to_string())
    };
    names.sort_by(|a, b| match (issue(a), issue(b)) {
        (Some(x), Some(y)) => (x.len(), &x, a).cmp(&(y.len(), &y, b)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.as_bytes().cmp(b.as_bytes()),
    });
}

/// The fragments of a directory: (path as the error and the output name it,
/// file name), in order. Regular *.md files only; none when there is no
/// directory.
pub fn list(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut names = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !name.ends_with(".md") {
            continue;
        }
        if fs::metadata(e.path()).is_ok_and(|m| m.is_file()) {
            names.push(name);
        }
    }
    order(&mut names);
    Ok(names
        .into_iter()
        .map(|n| (format!("{}/{n}", dir.display()), dir.join(n)))
        .collect())
}

/// Why a collection cannot be made.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    /// The changelog has no section of that name.
    NoSection,
    /// A fragment's text, said.
    Fragment(String),
}

/// The changelog `text` with the fragments' lines in the section `name`, its
/// lines ending in one newline. `fragments` are (path, text), in order.
pub fn collect(text: &str, name: &str, fragments: &[(String, String)]) -> Result<String, Refusal> {
    let mut changes: Vec<String> = Vec::new();
    let mut actions: Vec<String> = Vec::new();
    for (path, t) in fragments {
        let f = read_fragment(path, t).map_err(Refusal::Fragment)?;
        changes.extend(f.changes);
        if !f.action.is_empty() {
            actions.push(f.action.join("\n"));
        }
    }
    let l: Vec<&str> = records(text.trim_end_matches('\n'));
    let heading_of = |line: &str| {
        let rest = line.strip_prefix("## ")?.as_bytes();
        Some(
            rest.starts_with(name.as_bytes())
                && (rest.len() == name.len() || rest[name.len()] == b' '),
        )
    };
    let s = l
        .iter()
        .position(|line| heading_of(line) == Some(true))
        .ok_or(Refusal::NoSection)?;
    let e = (s + 1..l.len())
        .find(|&i| l[i].starts_with("## "))
        .unwrap_or(l.len());
    let has_text = |i: usize| !blank(l[i]);
    let (mut ch, mut ad, mut sub1, mut end) = (None, None, None, s);
    for (i, line) in l.iter().enumerate().take(e).skip(s + 1) {
        if is_heading(line, "### Changes") {
            ch = Some(i);
        }
        if is_heading(line, "### Adopter action") {
            ad = Some(i);
        }
        if line.starts_with("### ") && sub1.is_none() {
            sub1 = Some(i);
        }
        if !blank(line) {
            end = i;
        }
    }
    // The last line with text in the subsection that starts at h, or h.
    let last = |h: usize| {
        (h + 1..e)
            .take_while(|&i| !l[i].starts_with("### "))
            .filter(|&i| has_text(i))
            .last()
            .unwrap_or(h)
    };
    let mut after: Vec<(usize, String)> = Vec::new();
    let mut before: Vec<(usize, String)> = Vec::new();
    let mut drop = None;
    let c = changes.join("\n");
    let a = actions.join("\n\n");
    if !c.is_empty() {
        match (ch, sub1) {
            (Some(h), _) => {
                let at = last(h);
                let gap = if at == h { "\n" } else { "" };
                after.push((at, format!("{gap}{c}")));
            }
            (None, Some(first)) => before.push((first, format!("### Changes\n\n{c}\n"))),
            (None, None) => after.push((end, format!("\n### Changes\n\n{c}"))),
        }
    }
    if !a.is_empty() {
        match ad {
            Some(h) => {
                let at = last(h);
                let lines: Vec<&str> = (h + 1..=at).map(|i| l[i]).filter(|x| !blank(x)).collect();
                if lines.len() == 1 && is_heading(lines[0], "None.") {
                    drop = Some(at);
                    after.push((at, a));
                } else {
                    after.push((at, format!("\n{a}")));
                }
            }
            None => after.push((end, format!("\n### Adopter action\n\n{a}"))),
        }
    }
    Ok(join(&l, &before, &after, drop))
}

fn join(
    l: &[&str],
    before: &[(usize, String)],
    after: &[(usize, String)],
    drop: Option<usize>,
) -> String {
    let mut out = String::new();
    for (i, line) in l.iter().enumerate() {
        for (_, t) in before.iter().filter(|(at, _)| *at == i) {
            out += t;
            out.push('\n');
        }
        if drop != Some(i) {
            out += line;
            out.push('\n');
        }
        for (_, t) in after.iter().filter(|(at, _)| *at == i) {
            out += t;
            out.push('\n');
        }
    }
    let mut out = out.trim_end_matches('\n').to_string();
    out.push('\n');
    out
}

/// The task. `default` is the changelog when no file is given. Says its
/// errors on stderr after "collect-changelog: "; returns the exit status.
pub fn run(a: &Args, default: &Path) -> u8 {
    if let Some(branch) = &a.name {
        return match fragment_name(branch) {
            Some(n) => {
                tehas_core::outln!("changelog.d/{n}");
                0
            }
            None => {
                eprintln!("collect-changelog: {branch:?} cannot name a fragment");
                64
            }
        };
    }
    let file = a.file.clone().unwrap_or_else(|| default.to_path_buf());
    let say = |m: &str| eprintln!("collect-changelog: {m}");
    let Ok(text) = fs::read_to_string(&file) else {
        say(&format!("cannot read {}", file.display()));
        return 1;
    };
    let parent = match file.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let dir = parent.join("changelog.d");
    let paths = match list(&dir) {
        Ok(p) => p,
        Err(m) => {
            say(&m);
            return 1;
        }
    };
    let mut fragments = Vec::new();
    for (name, path) in &paths {
        match fs::read_to_string(path) {
            Ok(t) => fragments.push((name.clone(), t)),
            Err(_) => {
                say(&format!("cannot read {name}"));
                return 1;
            }
        }
    }
    if fragments.is_empty() {
        if a.print {
            tehas_core::outln!("{}", text.trim_end_matches('\n'));
        }
        return 0;
    }
    let merged = match collect(&text, &a.section, &fragments) {
        Ok(m) => m,
        Err(Refusal::NoSection) => {
            say(&format!(
                "no section for {} in {}",
                a.section,
                file.display()
            ));
            return 1;
        }
        Err(Refusal::Fragment(m)) => {
            say(&m);
            return 1;
        }
    };
    if a.print {
        tehas_core::out!("{merged}");
        return 0;
    }
    let tmp = PathBuf::from(format!("{}.collect", file.display()));
    if fs::write(&tmp, &merged)
        .and_then(|_| fs::rename(&tmp, &file))
        .is_err()
    {
        let _ = fs::remove_file(&tmp);
        say(&format!("cannot write {}", file.display()));
        return 1;
    }
    for (name, path) in &paths {
        if fs::remove_file(path).is_err() {
            say(&format!("cannot delete {name}"));
            return 1;
        }
        tehas_core::outln!("{name}");
    }
    let _ = fs::remove_dir(&dir);
    0
}

#[cfg(test)]
mod tests;
