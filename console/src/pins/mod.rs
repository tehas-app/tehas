//! The release's move of the pinned tools: every pin in `host/versions.env`,
//! `host/npm/` and `host/sandbox/npm/` to the newest release of the major
//! version it has, each with what moves with it.
//!
//!   console pins check --root <dir>
//!   console pins move --root <dir> [--release vX.Y.Z]
//!
//! `check` changes nothing and prints one line per tool:
//! `<name> <pinned> <newest of that major>`, and `held <newest>` after it when
//! a later major is out. `move` writes the moves into the tree at `<dir>` and
//! prints `moved <name>: <from> to <to>` and `held <name>: <pinned>, <newest>
//! is out` lines. Each version is taken from the tool's own source (crates.io, npm,
//! GitHub releases, Docker Hub for Caddy's digest), and moves together with its
//! checksum, its digest, or its lockfile and `_LOCK_SHA256`, as the header of
//! versions.env says. vipune's model moves to the revision the new vipune
//! asks for, with its hashes from Hugging Face. An npm tree is locked again
//! with npm, which must be on PATH; a version younger than npm's
//! `min-release-age` is not taken, since npm would not lock it.
//!
//! A later major is never taken. `move` writes the moves and what was held
//! back into the release's section of CHANGELOG.md (`--release`'s, else
//! Unreleased): one line per move under `### Changes`, and `### Held back`.
//!
//! scripts/tag-release.sh calls `move` through scripts/pins.sh before it tags.
//!
//! Exit: 0 done; 1 a source could not be read, or a move could not be made
//! (the tree may then be half moved: the caller throws it away); 64 usage.

mod text;
mod tools;

#[cfg(test)]
mod tests;

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::outln;
use text::{Held, Moved, Ver};
use tools::{World, CADDY_FILES, TOOLS};

pub fn main(given: &[String]) -> u8 {
    let (cmd, rest) = match given.split_first() {
        Some((c, r)) if c == "check" || c == "move" => (c.as_str(), r),
        _ => return usage(),
    };
    let mut root = None;
    let mut release = None;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match (a.as_str(), it.next()) {
            ("--root", Some(v)) => root = Some(PathBuf::from(v)),
            ("--release", Some(v))
                if cmd == "move" && Ver::parse(v.strip_prefix('v').unwrap_or("")).is_some() =>
            {
                release = Some(v.clone())
            }
            _ => return usage(),
        }
    }
    let Some(root) = root else { return usage() };
    let result = if cmd == "check" {
        check(&root, &Curl).map(|lines| lines.into_iter().for_each(|l| outln!("{l}")))
    } else {
        run(&root, release.as_deref(), &Curl)
            .map(|lines| lines.into_iter().for_each(|l| outln!("{l}")))
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("pins: {e}");
            1
        }
    }
}

fn usage() -> u8 {
    eprintln!("usage: console pins check --root <dir>");
    eprintln!("       console pins move --root <dir> [--release vX.Y.Z]");
    64
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Each tool's pin and what its source offers: (tool, pinned, newest of the
/// major, newest later major).
type Plan = Vec<(&'static tools::Tool, Ver, Ver, Option<Ver>)>;

fn plan(env: &str, world: &dyn World) -> Result<Plan, String> {
    let mut out = Vec::new();
    for tool in TOOLS {
        let pinned = text::env_get(env, tool.key)
            .ok_or_else(|| format!("host/versions.env has no {}", tool.key))?;
        let current =
            Ver::parse(&pinned).ok_or_else(|| format!("{}={pinned} is not X.Y.Z", tool.key))?;
        let found = tools::versions(tool, world).map_err(|e| format!("{}: {e}", tool.name))?;
        let (pin, later) = text::choose(current, &found);
        out.push((tool, current, pin, later));
    }
    Ok(out)
}

fn check(root: &Path, world: &dyn World) -> Result<Vec<String>, String> {
    let env = read(&root.join("host/versions.env"))?;
    Ok(plan(&env, world)?
        .into_iter()
        .map(|(tool, current, pin, later)| {
            let held = later.map(|v| format!(" held {v}")).unwrap_or_default();
            format!("{} {current} {pin}{held}", tool.name)
        })
        .collect())
}

fn run(root: &Path, release: Option<&str>, world: &dyn World) -> Result<Vec<String>, String> {
    let env_path = root.join("host/versions.env");
    let mut env = read(&env_path)?;
    let mut moved = Vec::new();
    let mut held = Vec::new();
    for (tool, current, pin, later) in plan(&env, world)? {
        if let Some(v) = later {
            held.push(Held {
                name: tool.name.into(),
                current: pin.to_string(),
                newest: v.to_string(),
            });
        }
        if pin == current {
            continue;
        }
        let (from, to) = (current.to_string(), pin.to_string());
        let mut values =
            tools::companions(tool, &to, world).map_err(|e| format!("{} {to}: {e}", tool.name))?;
        if let tools::Source::Npm(package, dir) = tool.source {
            let lock = relock(&root.join(dir), package, &from, &to, world)?;
            values.push((tool.key.replace("_VERSION", "_LOCK_SHA256"), lock));
        }
        if tool.key == "CADDY_VERSION" {
            let old = text::env_get(&env, "CADDY_DIGEST")
                .ok_or("host/versions.env has no CADDY_DIGEST")?;
            let new = &values[0].1;
            for f in CADDY_FILES {
                let path = root.join(f);
                let was = read(&path)?;
                let now = was.replace(&format!("caddy@{old}"), &format!("caddy@{new}"));
                let now = now.replace(&format!("caddy {from},"), &format!("caddy {to},"));
                if !now.contains(&format!("caddy@{new}")) {
                    return Err(format!("{f} does not name caddy@{old}"));
                }
                write(&path, &now)?;
            }
        }
        if tool.key == "VIPUNE_VERSION" {
            let rev = text::env_get(&env, "VIPUNE_MODEL_REVISION").unwrap_or_default();
            if values
                .iter()
                .any(|(k, v)| k == "VIPUNE_MODEL_REVISION" && *v != rev)
            {
                let to_rev = &values
                    .iter()
                    .find(|(k, _)| k == "VIPUNE_MODEL_REVISION")
                    .expect("set")
                    .1;
                moved.push(Moved {
                    name: "vipune model".into(),
                    from: rev,
                    to: to_rev.clone(),
                });
            }
        }
        env = text::env_set(&env, tool.key, &to)?;
        for (k, v) in &values {
            env = text::env_set(&env, k, v)?;
        }
        moved.push(Moved {
            name: tool.name.into(),
            from,
            to,
        });
    }
    write(&env_path, &env)?;
    let log = root.join("CHANGELOG.md");
    write(
        &log,
        &text::changelog(&read(&log)?, release, &moved, &held)?,
    )?;
    let mut out: Vec<String> = moved
        .iter()
        .map(|m| format!("moved {}: {} to {}", m.name, m.from, m.to))
        .collect();
    out.extend(
        held.iter()
            .map(|h| format!("held {}: {}, {} is out", h.name, h.current, h.newest)),
    );
    Ok(out)
}

/// The npm tree at `dir` pinned to `to` and locked again; the new lockfile's
/// sha256. The lockfile must pin `to` where bootstrap.sh looks for it.
fn relock(
    dir: &Path,
    package: &str,
    from: &str,
    to: &str,
    world: &dyn World,
) -> Result<String, String> {
    let manifest = dir.join("package.json");
    let was = read(&manifest)?;
    let line = |v: &str| format!("\"{package}\": \"{v}\"");
    if !was.contains(&line(from)) {
        return Err(format!(
            "{} does not pin {package} {from}",
            manifest.display()
        ));
    }
    write(&manifest, &was.replace(&line(from), &line(to)))?;
    world.npm_lock(dir)?;
    let lock_path = dir.join("package-lock.json");
    let lock = read(&lock_path)?;
    let parsed: Value = serde_json::from_str(&lock)
        .map_err(|e| format!("{} is not JSON: {e}", lock_path.display()))?;
    let asked = parsed["packages"][""]["dependencies"][package].as_str();
    let got = parsed["packages"][format!("node_modules/{package}")]["version"].as_str();
    if asked != Some(to) || got != Some(to) {
        return Err(format!(
            "{} does not pin {package} {to} after npm ran",
            lock_path.display()
        ));
    }
    Ok(tools::sha256(lock.as_bytes()))
}

/// The world through curl, tar and npm.
struct Curl;

impl World for Curl {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut cmd = Command::new("curl");
        cmd.args([
            "-fsSL",
            "--max-time",
            "600",
            "--retry",
            "3",
            "-A",
            "tehas-pins",
            "-K",
            "-",
            url,
        ]);
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run curl: {e}"))?;
        // GitHub's API allows more with a token; it goes on standard input.
        let token = std::env::var("GH_TOKEN")
            .or_else(|_| std::env::var("GITHUB_TOKEN"))
            .unwrap_or_default();
        if let Some(mut stdin) = child.stdin.take() {
            let safe = !token.is_empty()
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_');
            if url.starts_with("https://api.github.com/") && safe {
                let _ = writeln!(stdin, "header = \"Authorization: Bearer {token}\"");
            }
        }
        let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            return Err(format!("cannot get {url}: {}", why.trim()));
        }
        Ok(out.stdout)
    }

    fn unpack(&self, archive: &[u8], member: &str) -> Result<String, String> {
        let mut child = Command::new("tar")
            .args(["-xzO", "--wildcards", &format!("*/{member}")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot run tar: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("tar has no stdin")?;
        let bytes = archive.to_vec();
        let feeder = std::thread::spawn(move || stdin.write_all(&bytes));
        let out = child.wait_with_output().map_err(|e| format!("tar: {e}"))?;
        let _ = feeder.join();
        if !out.status.success() || out.stdout.is_empty() {
            return Err(format!("the archive has no {member}"));
        }
        String::from_utf8(out.stdout).map_err(|_| format!("{member} is not text"))
    }

    fn npm_cutoff(&self) -> Option<String> {
        let out = Command::new("npm")
            .args(["config", "get", "min-release-age"])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let days: i64 = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .ok()
            .filter(|d| *d > 0)?;
        let at = tehas_core::time::iso(tehas_core::time::now() - days * 86_400);
        Some(at[..19].to_string())
    }

    fn npm_lock(&self, dir: &Path) -> Result<(), String> {
        let status = Command::new("npm")
            .args([
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
            ])
            .current_dir(dir)
            .stdout(Stdio::null())
            .status()
            .map_err(|e| format!("cannot run npm in {}: {e}", dir.display()))?;
        status.success().then_some(()).ok_or_else(|| {
            format!(
                "npm install --package-lock-only failed in {}",
                dir.display()
            )
        })
    }
}
