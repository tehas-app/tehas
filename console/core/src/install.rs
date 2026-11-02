//! A product's install step, found from the product itself: the lockfiles at
//! the root of a commit and one directory down, each with the install it is
//! made for. A product that names its own in .pi/install-cmd has that run in
//! place of the detection. check-product and a cycle run the script of the
//! steps in the sandbox, at the root of their clone, before the gate.

use std::process::{Command, Stdio};

/// The exit status of the script when a tool is not in the image. Nothing is
/// installed then; the script names each missing tool on its error stream.
pub const MISSING: i32 = 69;

/// One install: the directory it runs in ("" the root), the program it needs,
/// and the command.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub dir: String,
    pub tool: String,
    pub command: String,
}

impl Step {
    /// How check-product and a log name it: `extension/: bun install`.
    pub fn line(&self) -> String {
        let dir = if self.dir.is_empty() {
            "./".to_string()
        } else {
            format!("{}/", self.dir)
        };
        format!("{dir}: {}", self.command)
    }
}

/// yarn 1 refuses --immutable and later yarns ignore --frozen-lockfile.
const YARN: &str = "if yarn --version | grep -q '^1\\.'; then yarn install --frozen-lockfile; \
                    else yarn install --immutable; fi";

/// The lockfiles of one ecosystem, the first found wins, and its install.
const KINDS: &[&[(&str, &str, &str)]] = &[
    &[
        ("pnpm-lock.yaml", "pnpm", "pnpm install --frozen-lockfile"),
        ("yarn.lock", "yarn", YARN),
        ("bun.lock", "bun", "bun install --frozen-lockfile"),
        ("bun.lockb", "bun", "bun install --frozen-lockfile"),
        ("package-lock.json", "npm", "npm ci"),
    ],
    &[("Cargo.lock", "cargo", "cargo fetch --locked")],
    &[("uv.lock", "uv", "uv sync --frozen")],
    &[("poetry.lock", "poetry", "poetry install --no-root")],
    &[("go.sum", "go", "go mod download")],
    &[("Gemfile.lock", "bundle", "bundle install")],
    &[("composer.lock", "composer", "composer install")],
];

/// Directories whose lockfiles are another project's, not the product's.
const NOT_OURS: &[&str] = &["node_modules", "vendor", "target"];

/// The steps of a listing as `git ls-tree -r <rev>` prints it. A submodule is
/// a commit in it, not a tree, so nothing under one is listed.
pub fn detect(listing: &str) -> Vec<Step> {
    let mut files: Vec<(&str, &str)> = Vec::new();
    for line in listing.lines() {
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        if meta.split_whitespace().nth(1) != Some("blob") {
            continue;
        }
        let (dir, name) = match path.split_once('/') {
            None => ("", path),
            Some((dir, name)) if !name.contains('/') => (dir, name),
            Some(_) => continue,
        };
        if NOT_OURS.contains(&dir) {
            continue;
        }
        files.push((dir, name));
    }
    let mut dirs: Vec<&str> = files.iter().map(|(d, _)| *d).collect();
    dirs.sort_unstable();
    dirs.dedup();
    let mut steps = Vec::new();
    for dir in dirs {
        for kind in KINDS {
            let found = kind
                .iter()
                .find(|(lock, _, _)| files.contains(&(dir, *lock)));
            if let Some((_, tool, command)) = found {
                steps.push(Step {
                    dir: dir.to_string(),
                    tool: tool.to_string(),
                    command: command.to_string(),
                });
            }
        }
    }
    steps
}

/// The step a product names in .pi/install-cmd: its first command, run at the
/// root. It needs no tool of the list.
pub fn named(text: &str) -> Option<Step> {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))?;
    Some(Step {
        dir: String::new(),
        tool: String::new(),
        command: line.to_string(),
    })
}

fn git_out(repo: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("timeout")
        .args(["-k", "10", "60", "git", "-C"])
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The steps of <rev> in <repo>: what .pi/install-cmd names, or else what the
/// lockfiles say. None when git could not list the commit.
pub fn steps(repo: &str, rev: &str) -> Option<Vec<Step>> {
    let own = git_out(repo, &["show", &format!("{rev}:.pi/install-cmd")]);
    if let Some(step) = own.as_deref().and_then(named) {
        return Some(vec![step]);
    }
    git_out(repo, &["ls-tree", "-r", rev]).map(|l| detect(&l))
}

fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// The script that runs <steps> from the root of a clone: first that every
/// tool is in the image, then each install in its directory, stopping at the
/// first that fails.
pub fn script(steps: &[Step]) -> String {
    let mut tools: Vec<&str> = steps
        .iter()
        .map(|s| s.tool.as_str())
        .filter(|t| !t.is_empty())
        .collect();
    tools.sort_unstable();
    tools.dedup();
    let mut s = String::from("set -e\nmissing=\n");
    for tool in tools {
        s.push_str(&format!(
            "command -v {tool} >/dev/null 2>&1 || {{ echo \"no {tool} in the sandbox image\" >&2; missing=1; }}\n"
        ));
    }
    s.push_str(&format!("[ -z \"$missing\" ] || exit {MISSING}\n"));
    for step in steps {
        let dir = if step.dir.is_empty() {
            ".".to_string()
        } else {
            quoted(&step.dir)
        };
        s.push_str(&format!(
            "echo {} >&2\n(cd {dir} && {})\n",
            quoted(&format!("install {}", step.line())),
            step.command
        ));
    }
    s
}

/// The tools a script of [`script`] said were missing, from its error stream.
pub fn missing(said: &str) -> Vec<String> {
    said.lines()
        .filter_map(|l| {
            l.strip_prefix("no ")?
                .strip_suffix(" in the sandbox image")
                .map(str::to_string)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn blob(path: &str) -> String {
        format!("100644 blob 0000000000000000000000000000000000000000\t{path}\n")
    }

    fn lines(steps: &[Step]) -> Vec<String> {
        steps.iter().map(Step::line).collect()
    }

    #[test]
    fn each_lockfile_at_the_root_and_one_down_is_installed() {
        for (lock, command) in [
            ("bun.lock", "bun install --frozen-lockfile"),
            ("bun.lockb", "bun install --frozen-lockfile"),
            ("package-lock.json", "npm ci"),
            ("pnpm-lock.yaml", "pnpm install --frozen-lockfile"),
            ("yarn.lock", YARN),
            ("Cargo.lock", "cargo fetch --locked"),
            ("uv.lock", "uv sync --frozen"),
            ("poetry.lock", "poetry install --no-root"),
            ("go.sum", "go mod download"),
            ("Gemfile.lock", "bundle install"),
            ("composer.lock", "composer install"),
        ] {
            let listing = format!("{}{}", blob(lock), blob(&format!("sub/{lock}")));
            assert_eq!(
                lines(&detect(&listing)),
                [format!("./: {command}"), format!("sub/: {command}")],
                "{lock}"
            );
        }
    }

    #[test]
    fn what_is_not_the_products_is_left_alone() {
        let listing = [
            blob("README.md"),
            blob("a/b/bun.lock"),
            blob("node_modules/package-lock.json"),
            blob("vendor/composer.lock"),
            blob("target/Cargo.lock"),
            "160000 commit 0000000000000000000000000000000000000000\tlib\n".to_string(),
            "040000 tree 0000000000000000000000000000000000000000\tsub\n".to_string(),
        ]
        .concat();
        assert_eq!(detect(&listing), []);
        assert_eq!(detect(""), []);
    }

    #[test]
    fn one_install_per_ecosystem_and_directory() {
        let listing = [
            blob("extension/bun.lock"),
            blob("extension/package-lock.json"),
            blob("Cargo.lock"),
            blob("package-lock.json"),
        ]
        .concat();
        assert_eq!(
            lines(&detect(&listing)),
            [
                "./: npm ci",
                "./: cargo fetch --locked",
                "extension/: bun install --frozen-lockfile"
            ]
        );
    }

    #[test]
    fn a_named_install_is_its_first_command() {
        let step = named("#!/bin/sh\n# why\n\nmake deps\nother\n").unwrap();
        assert_eq!(step.line(), "./: make deps");
        assert_eq!(step.tool, "");
        assert_eq!(named("# nothing\n\n"), None);
    }

    fn sh(script: &str, dir: &std::path::Path, path: &str) -> std::process::Output {
        Command::new("sh")
            .args(["-c", script])
            .current_dir(dir)
            .env("PATH", path)
            .output()
            .unwrap()
    }

    #[test]
    fn the_script_names_a_missing_tool_and_installs_nothing() {
        let d = std::env::temp_dir().join(format!("tehas-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("it's")).unwrap();
        let steps = [
            Step {
                dir: "it's".to_string(),
                tool: "sh".to_string(),
                command: "pwd > ../ran".to_string(),
            },
            Step {
                dir: String::new(),
                tool: "tehas-no-such-tool".to_string(),
                command: "true".to_string(),
            },
        ];
        let path = std::env::var("PATH").unwrap();
        let out = sh(&script(&steps), &d, &path);
        assert_eq!(out.status.code(), Some(MISSING));
        let said = String::from_utf8_lossy(&out.stderr);
        assert_eq!(missing(&said), ["tehas-no-such-tool"]);
        assert!(!d.join("ran").exists());

        let out = sh(&script(&steps[..1]), &d, &path);
        assert!(out.status.success());
        let ran = std::fs::read_to_string(d.join("ran")).unwrap();
        assert!(ran.trim_end().ends_with("it's"));

        let failing = [Step {
            dir: String::new(),
            tool: String::new(),
            command: "exit 3".to_string(),
        }];
        assert_eq!(sh(&script(&failing), &d, &path).status.code(), Some(3));
        let _ = std::fs::remove_dir_all(&d);
    }
}
