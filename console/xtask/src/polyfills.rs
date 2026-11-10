//! polyfills: the polyfills a tree carries, which of them a pi-rukas
//! `--capabilities` answer has made unnecessary, and their removal.
//! scripts/polyfills.sh execs it; its header is the usage and the exit
//! statuses, and `--help` prints it.
//!
//! A polyfill of the feature `<f>` is `lib/polyfill-<f>.sh` with two header
//! lines naming its upstream issue and pull request, its test
//! `test/polyfill_<f>_test.sh`, and the lines elsewhere that end in
//! `# polyfill <f>`. It reads the tree and the answer and nothing else: no
//! network, no git. `xtask tag-release` asks GitHub about the upstream work.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What the task is asked to do.
#[derive(Debug, PartialEq)]
pub enum Verb {
    List,
    Check,
    Expired(PathBuf),
    Prune(PathBuf),
}

/// The arguments of the task.
#[derive(Debug, PartialEq)]
pub struct Args {
    /// --root, else the checkout the script is in.
    pub root: Option<PathBuf>,
    pub verb: Verb,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Help,
    Run(Args),
}

/// Why the arguments were refused; either ends 64.
#[derive(Debug, PartialEq)]
pub enum Bad {
    /// The usage goes on stderr.
    Usage,
    /// This line goes on stderr.
    Say(String),
}

pub fn parse(args: &[String]) -> Result<Parsed, Bad> {
    let mut rest = args;
    let mut root = None;
    if rest.first().is_some_and(|a| a == "--root") {
        match rest.get(1) {
            Some(d) if Path::new(d).is_dir() => root = Some(PathBuf::from(d)),
            _ => return Err(Bad::Say("--root needs a directory".into())),
        }
        rest = &rest[2..];
    }
    let verb = match rest {
        [v] if v == "list" => Verb::List,
        [v] if v == "check" => Verb::Check,
        [v, f] if v == "expired" && Path::new(f).is_file() => Verb::Expired(f.into()),
        [v, f] if v == "prune" && Path::new(f).is_file() => Verb::Prune(f.into()),
        [v, ..] if v == "expired" || v == "prune" => {
            return Err(Bad::Say(format!("{v} needs a --capabilities answer file")))
        }
        [v, ..] if v == "-h" || v == "--help" => return Ok(Parsed::Help),
        _ => return Err(Bad::Usage),
    };
    Ok(Parsed::Run(Args { root, verb }))
}

/// A character of `[[:space:]]`.
fn space(c: char) -> bool {
    c == ' ' || ('\t'..='\r').contains(&c)
}

/// A feature's name as the runner's answer has it: a lower-case letter, then
/// up to 63 letters and digits.
pub fn is_name(n: &str) -> bool {
    let mut c = n.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase())
        && n.len() <= 64
        && c.all(|c| c.is_ascii_alphanumeric())
}

/// The feature a line marks: the name after its `# polyfill `, with only
/// space after the name.
pub fn marker(line: &str) -> Option<&str> {
    const MARK: &str = "# polyfill ";
    let mut from = 0;
    while let Some(at) = line[from..].find(MARK) {
        let start = from + at + MARK.len();
        let after = &line[start..];
        let len = after
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(after.len());
        let (name, tail) = after.split_at(len);
        if is_name(name) && tail.chars().all(space) {
            return Some(name);
        }
        from = start;
    }
    None
}

/// The lines of a file's bytes, each with its newline, for matching and for
/// writing back unchanged.
fn lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split_inclusive(|b| *b == b'\n')
}

fn text(line: &[u8]) -> String {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    String::from_utf8_lossy(line).into_owned()
}

/// The features the tree carries a polyfill of, in byte order.
pub fn features(root: &Path) -> Vec<String> {
    let Ok(dir) = fs::read_dir(root.join("lib")) else {
        return Vec::new();
    };
    let mut v: Vec<String> = dir
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let f = name.strip_prefix("polyfill-")?.strip_suffix(".sh")?;
            (is_name(f) && e.path().is_file()).then(|| f.to_string())
        })
        .collect();
    v.sort();
    v
}

/// `owner/repo#n`, each of owner and repo one or more of `[A-Za-z0-9_.-]`.
fn is_ref(s: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    };
    let Some((owner, rest)) = s.split_once('/') else {
        return false;
    };
    let Some((repo, n)) = rest.split_once('#') else {
        return false;
    };
    part(owner) && part(repo) && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())
}

/// The upstream `issue` or `PR` a polyfill's header names, the first line
/// of the kind; empty when it names none.
pub fn upstream(code: &[u8], what: &str) -> String {
    let head = format!("# Upstream {what}: ");
    lines(code)
        .map(text)
        .find_map(|l| {
            let r = l.strip_prefix(&head)?.trim_end_matches(space);
            is_ref(r).then(|| r.to_string())
        })
        .unwrap_or_default()
}

fn polyfill_file(root: &Path, f: &str) -> PathBuf {
    root.join("lib").join(format!("polyfill-{f}.sh"))
}

fn test_file(root: &Path, f: &str) -> PathBuf {
    root.join("test").join(format!("polyfill_{f}_test.sh"))
}

/// The files that hold a marked line, in path order: the tree less .git, the
/// records in docs/, the instances and what is built. A file with a NUL byte
/// is binary and holds none.
pub fn searched(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, top: bool, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.filter_map(|e| e.ok()) {
            let name = e.file_name();
            if name == "target"
                || name == "node_modules"
                || (top && (name == ".git" || name == "docs" || name == "instances"))
            {
                continue;
            }
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                walk(&e.path(), false, out);
            } else if t.is_file() {
                out.push(e.path());
            }
        }
    }
    let mut files = Vec::new();
    walk(root, true, &mut files);
    files.retain(|f| {
        fs::read(f).is_ok_and(|b| !b.contains(&0) && lines(&b).any(|l| marker(&text(l)).is_some()))
    });
    files.sort();
    files
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// One line per polyfill: the feature, its upstream issue and its upstream
/// pull request.
pub fn list(root: &Path) -> Vec<String> {
    features(root)
        .iter()
        .map(|f| {
            let code = fs::read(polyfill_file(root, f)).unwrap_or_default();
            format!("{f} {} {}", upstream(&code, "issue"), upstream(&code, "PR"))
        })
        .collect()
}

/// The problems of the tree's polyfills, one line each: a polyfill without
/// an upstream line or its test, and a marked line that names no polyfill.
pub fn check(root: &Path) -> Vec<String> {
    let have = features(root);
    let mut bad = Vec::new();
    for f in &have {
        let code = fs::read(polyfill_file(root, f)).unwrap_or_default();
        if upstream(&code, "issue").is_empty() {
            bad.push(format!("lib/polyfill-{f}.sh names no upstream issue"));
        }
        if upstream(&code, "PR").is_empty() {
            bad.push(format!(
                "lib/polyfill-{f}.sh names no upstream pull request"
            ));
        }
        if !test_file(root, f).is_file() {
            bad.push(format!(
                "lib/polyfill-{f}.sh has no test/polyfill_{f}_test.sh"
            ));
        }
    }
    for file in searched(root) {
        let bytes = fs::read(&file).unwrap_or_default();
        for l in lines(&bytes).map(text) {
            if let Some(m) = marker(&l).filter(|m| !have.iter().any(|f| f == m)) {
                bad.push(format!(
                    "{} marks a line for {m}, which has no lib/polyfill-{m}.sh",
                    rel(root, &file)
                ));
            }
        }
    }
    bad
}

/// The polyfills whose feature the answer sets to true; None when the
/// answer is not a JSON object.
pub fn expired(root: &Path, caps: &[u8]) -> Option<Vec<String>> {
    let answer: serde_json::Value = serde_json::from_slice(caps).ok()?;
    let answer = answer.as_object()?;
    let set = answer.get("features").and_then(|f| f.as_object());
    Some(
        features(root)
            .into_iter()
            .filter(|f| set.is_some_and(|s| s.get(f) == Some(&serde_json::Value::Bool(true))))
            .collect(),
    )
}

/// What prune did: each path it removed or changed, as it goes, and the
/// files whose lines it changed.
#[derive(Debug, Default, PartialEq)]
pub struct Pruned {
    pub said: Vec<String>,
    pub changed: Vec<PathBuf>,
}

/// Remove the polyfills of `gone`: their marked lines, their code and their
/// test. Err names the file that could not be changed or removed, with what
/// was done up to it.
pub fn prune(root: &Path, gone: &[String]) -> Result<Pruned, (Pruned, String)> {
    let mut p = Pruned::default();
    for f in gone {
        for file in searched(root) {
            let Ok(bytes) = fs::read(&file) else { continue };
            if !lines(&bytes).any(|l| marker(&text(l)) == Some(f.as_str())) {
                continue;
            }
            let kept: Vec<u8> = lines(&bytes)
                .filter(|l| marker(&text(l)) != Some(f.as_str()))
                .flatten()
                .copied()
                .collect();
            if fs::write(&file, kept).is_err() {
                let msg = format!("cannot change {}", file.display());
                return Err((p, msg));
            }
            p.said.push(rel(root, &file));
            if !p.changed.contains(&file) {
                p.changed.push(file);
            }
        }
        for file in [polyfill_file(root, f), test_file(root, f)] {
            if !file.exists() {
                continue;
            }
            if fs::remove_file(&file).is_err() {
                let msg = format!("cannot remove {}", file.display());
                return Err((p, msg));
            }
            p.said.push(rel(root, &file));
        }
    }
    Ok(p)
}

/// The changed shell files that no longer parse: a marked line that was part
/// of a larger statement. A shell file is one whose first line names bash,
/// or one named `*.sh`.
pub fn unparsed(root: &Path, changed: &[PathBuf]) -> Vec<String> {
    changed
        .iter()
        .filter(|file| {
            let Ok(bytes) = fs::read(file) else {
                return false;
            };
            let first = lines(&bytes).next().map(text).unwrap_or_default();
            first.contains("bash") || file.extension().is_some_and(|e| e == "sh")
        })
        .filter(|file| {
            !Command::new("bash")
                .arg("-n")
                .arg(file)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
        .map(|file| rel(root, file))
        .collect()
}

/// The task: its exit status, its lines on stdout and its complaints on
/// stderr.
pub fn run(a: &Args, root: &Path) -> u8 {
    let say = |m: &str| eprintln!("polyfills: {m}");
    let expired_of = |caps: &Path| {
        let answer = fs::read(caps).unwrap_or_default();
        let gone = expired(root, &answer);
        if gone.is_none() {
            say(&format!("{} is not a JSON object", caps.display()));
        }
        gone
    };
    match &a.verb {
        Verb::List => {
            for l in list(root) {
                tehas_core::outln!("{l}");
            }
            0
        }
        Verb::Check => {
            let bad = check(root);
            for l in &bad {
                tehas_core::outln!("{l}");
            }
            u8::from(!bad.is_empty())
        }
        Verb::Expired(caps) => match expired_of(caps) {
            Some(gone) => {
                for f in gone {
                    tehas_core::outln!("{f}");
                }
                0
            }
            None => 1,
        },
        Verb::Prune(caps) => {
            let Some(gone) = expired_of(caps) else {
                return 1;
            };
            let p = match prune(root, &gone) {
                Ok(p) => p,
                Err((p, msg)) => {
                    for l in &p.said {
                        tehas_core::outln!("{l}");
                    }
                    say(&msg);
                    return 1;
                }
            };
            for l in &p.said {
                tehas_core::outln!("{l}");
            }
            let broken = unparsed(root, &p.changed);
            for file in &broken {
                say(&format!("{file} no longer parses"));
            }
            u8::from(!broken.is_empty())
        }
    }
}

#[cfg(test)]
mod tests;
