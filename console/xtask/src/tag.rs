//! tag-release: tag the head of the remote's default branch as the next
//! release. scripts/tag-release.sh execs it; its header is the usage, the
//! order of the steps and the exit statuses, and `--help` prints it.
//!
//! The steps run in the order the script has always run them, with the same
//! lines on stderr and the tag alone on stdout. git, gh, docker, the console's
//! `rukas`, scripts/polyfills.sh, scripts/pins.sh, scripts/verify.sh and
//! the head's scripts/collect-changelog.sh are still the programs that do
//! the work; what moved here is what decides between them: the next version, the remote's head, the changelog's
//! section and its Adopter action with the head's changelog fragments
//! counted (src/changelog.rs), which polyfills still stand in for work
//! upstream is doing, and the pull requests' text.
//!
//! Every command runs in the checkout the script lives in, as the script ran
//! them after its `cd`; the gate and the commits of a pull request run in a
//! worktree of the head, removed when the task ends.

use crate::changelog::{collect, order, Refusal};
use crate::pack::{is_exec, logical, Stage};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, PartialEq)]
pub struct Args {
    pub part: Part,
    pub dry: bool,
    pub verify: bool,
    /// The tag's message; empty takes the head's subject.
    pub message: String,
    pub remote: String,
    /// The --capabilities answer, absolute, when --rukas-caps gave one.
    pub rukas_caps: Option<PathBuf>,
    pub pins: bool,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            part: Part::Patch,
            dry: false,
            verify: true,
            message: String::new(),
            remote: "origin".into(),
            rukas_caps: None,
            pins: true,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Help,
    Run(Args),
}

/// The arguments, as the script's loop read them: the first error in their
/// order, or --help wherever it comes before one. An error is said after
/// "tag-release: " and ends 64. A relative --rukas-caps is read from `pwd`.
pub fn parse(args: &[String], pwd: &Path) -> Result<Parsed, String> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--patch" => a.part = Part::Patch,
            "--minor" => a.part = Part::Minor,
            "--major" => a.part = Part::Major,
            "--dry-run" => a.dry = true,
            "--no-verify" => a.verify = false,
            "--no-pins" => a.pins = false,
            "--message" => a.message = it.next().ok_or("--message needs a text")?.clone(),
            "--remote" => a.remote = it.next().ok_or("--remote needs a name")?.clone(),
            "--rukas-caps" => match it.next() {
                Some(v) if pwd.join(v).is_file() => a.rukas_caps = Some(caps_path(pwd, v)),
                _ => return Err("--rukas-caps needs a file".into()),
            },
            "-h" | "--help" => return Ok(Parsed::Help),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Parsed::Run(a))
}

/// `$(cd "$(dirname "$v")" && pwd)/$(basename "$v")`.
fn caps_path(pwd: &Path, v: &str) -> PathBuf {
    let p = Path::new(v);
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty());
    let dir = logical(pwd, dir.unwrap_or(Path::new(".")));
    match p.file_name() {
        Some(name) => dir.join(name),
        None => dir,
    }
}

/// The default branch and its commit, from `git ls-remote --symref <remote>
/// HEAD`: the first line names the branch, the second its commit.
pub fn remote_head(text: &str) -> Option<(String, String)> {
    let mut branch = None;
    let mut head = None;
    for line in text.lines() {
        if branch.is_none() {
            if let Some(rest) = line.strip_prefix("ref: refs/heads/") {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                if rest[end..].trim_start() == "HEAD" {
                    branch = Some(rest[..end].to_string());
                }
            }
        }
        let mut f = line.split_whitespace();
        if let (Some(first), Some("HEAD")) = (f.next(), f.next()) {
            if first != "ref:" && head.is_none() {
                head = Some(first.to_string());
            }
        }
    }
    match (branch, head) {
        (Some(b), Some(h)) if !b.is_empty() => Some((b, h)),
        _ => None,
    }
}

/// vX.Y.Z, each part one or more digits.
fn is_release(tag: &str) -> bool {
    crate::pack::is_tag(tag)
}

/// The newest release among tags `git tag --sort=-v:refname` listed, newest
/// first: the first that is vX.Y.Z.
pub fn newest(tags: &str) -> Option<String> {
    tags.lines().find(|t| is_release(t)).map(str::to_string)
}

/// The tag after `latest` (none: v0.0.0), with `part` raised and the parts
/// below it reset.
pub fn next_version(latest: Option<&str>, part: Part) -> String {
    let parts: Vec<u64> = latest
        .and_then(|l| l.strip_prefix('v'))
        .map(|l| l.split('.').map(|p| p.parse().unwrap_or(0)).collect())
        .unwrap_or_default();
    let n = |i: usize| parts.get(i).copied().unwrap_or(0);
    let (major, minor, patch) = (n(0), n(1), n(2));
    match part {
        Part::Major => format!("v{}.0.0", major + 1),
        Part::Minor => format!("v{major}.{}.0", minor + 1),
        Part::Patch => format!("v{major}.{minor}.{}", patch + 1),
    }
}

/// The lines of a text as awk reads its records: a last newline ends a line
/// and starts none.
fn records(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') || text.is_empty() {
        v.pop();
    }
    v
}

/// The section of a changelog for one release, as scripts/release-notes.sh
/// prints it: the lines after "## <name>" (alone, or followed by a space and
/// anything) up to the next "## " line, blank lines at either end taken off.
/// None when the text has no such heading.
pub fn section(text: &str, name: &str) -> Option<String> {
    let mut found = false;
    let mut lines = Vec::new();
    for line in records(text) {
        if line.starts_with("## ") {
            if found {
                break;
            }
            let rest = &line.as_bytes()[3..];
            found = rest.starts_with(name.as_bytes())
                && (rest.len() == name.len() || rest[name.len()] == b' ');
            continue;
        }
        if found {
            lines.push(line);
        }
    }
    if !found {
        return None;
    }
    let first = lines.iter().position(|l| !l.is_empty());
    let last = lines.iter().rposition(|l| !l.is_empty());
    Some(match (first, last) {
        (Some(f), Some(l)) => lines[f..=l].join("\n"),
        _ => String::new(),
    })
}

/// A character of `[[:space:]]`.
fn space(c: char) -> bool {
    c == ' ' || ('\t'..='\r').contains(&c)
}

/// Whether the section has an "### Adopter action" heading with a line under
/// it, before the next heading.
pub fn adopter_action(section: &str) -> bool {
    let mut inside = false;
    for line in records(section) {
        if line
            .strip_prefix("### Adopter action")
            .is_some_and(|r| r.chars().all(space))
        {
            inside = true;
            continue;
        }
        if line.starts_with("##") {
            inside = false;
        }
        if inside && !line.trim_matches([' ', '\t']).is_empty() {
            return true;
        }
    }
    false
}

/// A polyfill the head carries, as `polyfills.sh list` names it: the feature,
/// the upstream issue and the upstream pull request, as owner/repo#N.
#[derive(Debug, PartialEq)]
pub struct Carried {
    pub feature: String,
    pub issue: String,
    pub pr: String,
}

/// The lines of `polyfills.sh list`, read as `read -r feature issue pr`.
pub fn carried(text: &str) -> Vec<Carried> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start_matches([' ', '\t']);
        let (feature, rest) = word(line);
        if feature.is_empty() {
            continue;
        }
        let (issue, rest) = word(rest);
        out.push(Carried {
            feature: feature.into(),
            issue: issue.into(),
            pr: rest.trim_matches([' ', '\t']).into(),
        });
    }
    out
}

fn word(s: &str) -> (&str, &str) {
    let end = s.find([' ', '\t']).unwrap_or(s.len());
    (&s[..end], s[end..].trim_start_matches([' ', '\t']))
}

/// owner/repo#N as gh takes it: the repository before the last '#', the
/// number after the first.
pub fn split_ref(r: &str) -> (&str, &str) {
    let repo = r.rfind('#').map_or(r, |i| &r[..i]);
    let num = r.find('#').map_or(r, |i| &r[i + 1..]);
    (repo, num)
}

/// What the kept polyfills' upstream work says: the lines said for a merged
/// pull request whose feature is not set, in order, and the lines of each
/// polyfill that stands in for no work upstream is doing. `issue_state` and
/// `pr_state` give what gh says of one (empty: not found).
pub fn upstream(
    carried: &[Carried],
    expired: &[&str],
    rukas: &str,
    mut issue_state: impl FnMut(&str) -> String,
    mut pr_state: impl FnMut(&str) -> String,
) -> (Vec<String>, Vec<String>) {
    let mut merged = Vec::new();
    let mut untracked = Vec::new();
    for c in carried {
        if expired.contains(&c.feature.as_str()) {
            continue;
        }
        if c.issue.is_empty() || issue_state(&c.issue).is_empty() {
            let issue = if c.issue.is_empty() {
                "is not named"
            } else {
                &c.issue
            };
            untracked.push(format!(
                "    {}: its upstream issue {issue} cannot be found",
                c.feature
            ));
            continue;
        }
        let state = if c.pr.is_empty() {
            String::new()
        } else {
            pr_state(&c.pr)
        };
        match state.as_str() {
            "OPEN" => {}
            "MERGED" => merged.push(format!(
                "tag-release: {} is merged, but {rukas} does not set features.{}: its polyfill stays until a release has it; check the feature's name",
                c.pr, c.feature
            )),
            "CLOSED" => untracked.push(format!(
                "    {}: its upstream pull request {} was closed without a merge",
                c.feature, c.pr
            )),
            _ => {
                let pr = if c.pr.is_empty() { "is not named" } else { &c.pr };
                untracked.push(format!(
                    "    {}: its upstream pull request {pr} cannot be found",
                    c.feature
                ));
            }
        }
    }
    (merged, untracked)
}

/// The body of the pull request that removes the expired polyfills.
pub fn polyfill_body(rukas: &str, carried: &[Carried], expired: &[&str], next: &str) -> String {
    let mut body = format!(
        "{rukas} sets these features in its --capabilities answer, so tehas no longer needs its own polyfills of them:\n\n"
    );
    for c in carried
        .iter()
        .filter(|c| expired.contains(&c.feature.as_str()))
    {
        body += &format!(
            "- features.{}, upstream {} and {}\n",
            c.feature, c.issue, c.pr
        );
    }
    body + &format!("\nOpened by scripts/tag-release.sh, which tags {next} once this is merged.")
}

/// The body of the pull request that moves the pinned tools, one line of
/// `moved` each.
pub fn pins_body(moved: &[&str], next: &str) -> String {
    let lines: Vec<String> = moved.iter().map(|m| format!("- {m}")).collect();
    format!(
        "Each pinned tool at the newest release of the major version it has, with its checksum, digest or lockfile:\n\n{}\n\nA held tool has a new major version out; moving to it is a person's call. CHANGELOG.md names both.\n\nOpened by scripts/tag-release.sh, which tags {next} once this is merged.",
        lines.join("\n")
    )
}

/// `${s:0:7}`.
fn short(s: &str) -> &str {
    s.get(..7).unwrap_or(s)
}

/// What `$(...)` keeps of a command's stdout: the text, trailing newlines
/// taken off, and whether it ended 0. stderr is the task's.
fn capture(cmd: &mut Command) -> (String, bool) {
    match cmd
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
    {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            (text.trim_end_matches('\n').to_string(), o.status.success())
        }
        Err(e) => {
            eprintln!("tag-release: {}: {e}", cmd.get_program().to_string_lossy());
            (String::new(), false)
        }
    }
}

/// The stdout of a command that ended 0.
fn ok_out(cmd: &mut Command) -> Option<String> {
    let (out, ok) = capture(cmd);
    ok.then_some(out)
}

/// Whether a command ran and ended 0, its stdout on stderr when `quiet_out`
/// is false and nowhere when true.
fn runs(cmd: &mut Command, quiet_out: bool) -> bool {
    let out = if quiet_out {
        Stdio::null()
    } else {
        Stdio::from(std::io::stderr())
    };
    ends_0(cmd.stdout(out))
}

/// Whether a command ran and ended 0, its stdout where it was set.
fn ends_0(cmd: &mut Command) -> bool {
    match cmd.status() {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!("tag-release: {}: {e}", cmd.get_program().to_string_lossy());
            false
        }
    }
}

/// The head checked out for the polyfills and the gate, in a scratch
/// directory; the worktree is removed and the directory with it on drop.
struct Scratch {
    root: PathBuf,
    tree: PathBuf,
    dir: Stage,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .current_dir(&self.root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.tree)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The task: `root` is the checkout the script lives in. Returns the exit
/// status; every line it says is on stderr, the tag on stdout.
pub fn run(a: &Args, root: &Path) -> u8 {
    let git = || {
        let mut c = Command::new("git");
        c.current_dir(root);
        c
    };
    let remote = a.remote.as_str();
    if !runs(
        git().args(["fetch", "--quiet", "--tags", "--force", remote]),
        false,
    ) {
        eprintln!("tag-release: git fetch --tags --force from {remote} failed");
        return 1;
    }
    let Some(listing) = ok_out(git().args(["ls-remote", "--symref", remote, "HEAD"])) else {
        eprintln!("tag-release: cannot read HEAD of {remote}");
        return 1;
    };
    let Some((branch, head)) = remote_head(&listing) else {
        eprintln!("tag-release: {remote} did not name its default branch");
        return 1;
    };

    let (tags, _) = capture(git().args(["tag", "--list", "v*", "--sort=-v:refname"]));
    let latest = newest(&tags);
    if let Some(latest) = &latest {
        let rev = format!("{latest}^{{commit}}");
        let Some(latest_commit) = ok_out(git().args(["rev-parse", "--verify", "--quiet", &rev]))
        else {
            eprintln!("tag-release: {latest} names no commit");
            return 1;
        };
        if latest_commit == head {
            eprintln!(
                "tag-release: {branch} at {} is already {latest}",
                short(&head)
            );
            tehas_core::outln!("{latest}");
            return 0;
        }
        if !runs(
            git().args(["merge-base", "--is-ancestor", &latest_commit, &head]),
            false,
        ) {
            eprintln!("tag-release: {latest} is not on {branch}; tag by hand");
            return 1;
        }
    }
    let next = next_version(latest.as_deref(), a.part);

    // An adopter reads what to do after an update from the release's section
    // of CHANGELOG.md, read from the commit that is tagged.
    let show = git()
        .args(["show", &format!("{head}:CHANGELOG.md")])
        .stdin(Stdio::inherit())
        .stderr(Stdio::null())
        .output();
    let changelog = match show {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            eprintln!(
                "tag-release: {} has no CHANGELOG.md; nothing was tagged",
                short(&head)
            );
            return 1;
        }
    };
    // The head's changelog fragments count as lines of the section they will
    // be collected into.
    let Some(fragments) = head_fragments(root, &head) else {
        eprintln!(
            "tag-release: cannot read the changelog fragments of {}; nothing was tagged",
            short(&head)
        );
        return 1;
    };
    let found = [next.as_str(), "Unreleased"]
        .into_iter()
        .find(|name| section(&changelog, name).is_some());
    let collected = match found {
        Some(name) if !fragments.is_empty() => match collect(&changelog, name, &fragments) {
            Ok(c) => Some((name, c)),
            Err(Refusal::Fragment(m)) => {
                eprintln!("tag-release: {m}; nothing was tagged");
                return 1;
            }
            Err(Refusal::NoSection) => None,
        },
        Some(name) => Some((name, changelog.clone())),
        None => None,
    };
    let (name, text) = match collected.and_then(|(n, c)| section(&c, n).map(|t| (n, t))) {
        Some((name, text)) if !text.is_empty() => (name, text),
        _ => {
            eprintln!("tag-release: CHANGELOG.md has no section for {next} or Unreleased; nothing was tagged");
            return 1;
        }
    };
    if !adopter_action(&text) {
        eprintln!("tag-release: the {name} section of CHANGELOG.md needs an \"### Adopter action\" heading with a line under it (\"None.\" will do); nothing was tagged");
        return 1;
    }

    let range = match &latest {
        Some(l) => format!("{l}..{head}"),
        None => head.clone(),
    };
    let (count, _) = capture(git().args(["rev-list", "--count", &range]));
    let since = latest.as_deref().unwrap_or("the start");
    if a.dry {
        if !fragments.is_empty() {
            eprintln!(
                "tag-release: would collect {} changelog fragments into the {name} section of CHANGELOG.md:",
                fragments.len()
            );
            for line in text.split('\n') {
                eprintln!("    {line}");
            }
        }
        eprintln!(
            "tag-release: would tag {branch} at {} as {next} ({count} commits since {since})",
            short(&head)
        );
        tehas_core::outln!("{next}");
        return 0;
    }

    let Ok(dir) = Stage::new("tag-release") else {
        eprintln!("tag-release: cannot make a scratch directory");
        return 1;
    };
    let scratch = Scratch {
        root: root.to_path_buf(),
        tree: dir.0.join("tree"),
        dir,
    };
    let tree = scratch.tree.as_path();
    if !runs(
        git()
            .args(["worktree", "add", "--quiet", "--detach"])
            .arg(tree)
            .arg(&head),
        false,
    ) {
        eprintln!("tag-release: cannot check out {}", short(&head));
        return 1;
    }
    let ctx = Ctx {
        root,
        tree,
        remote,
        branch: &branch,
        verify: a.verify,
    };

    if let Some(code) = polyfills(&ctx, a, &scratch.dir.0, &head, &next) {
        return code;
    }
    if !fragments.is_empty() {
        return fragments_pr(&ctx, name, &next, &head);
    }
    if a.pins && is_exec(&tree.join("scripts/pins.sh")) {
        if let Some(code) = pins(&ctx, &next) {
            return code;
        }
    }

    if a.verify {
        eprintln!("tag-release: running the gate on {}", short(&head));
        if !ctx.gate() {
            eprintln!(
                "tag-release: the gate failed on {}; nothing was tagged",
                short(&head)
            );
            return 1;
        }
    }

    let message = if a.message.is_empty() {
        capture(git().args(["log", "-1", "--format=%s", &head])).0
    } else {
        a.message.clone()
    };
    let note = format!("Release {next} - {message}");
    if !runs(git().args(["tag", "-a", &next, "-m", &note, &head]), false) {
        eprintln!("tag-release: cannot create {next}");
        return 1;
    }
    if !runs(
        git().args(["push", "--quiet", remote, &format!("refs/tags/{next}")]),
        false,
    ) {
        runs(git().args(["tag", "-d", &next]), true);
        eprintln!("tag-release: pushing {next} to {remote} failed; the local tag is removed");
        return 1;
    }
    eprintln!(
        "tag-release: tagged {branch} at {} as {next} ({count} commits since {since})",
        short(&head)
    );
    tehas_core::outln!("{next}");
    0
}

struct Ctx<'a> {
    root: &'a Path,
    tree: &'a Path,
    remote: &'a str,
    branch: &'a str,
    verify: bool,
}

impl Ctx<'_> {
    /// A command of the checkout, run there.
    fn cmd(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut c = Command::new(program);
        c.current_dir(self.root);
        c
    }

    /// A script of the head, run from the checkout.
    fn script(&self, name: &str) -> Command {
        self.cmd(self.tree.join("scripts").join(name))
    }

    /// The head's gate, run in the head, its output on stderr.
    fn gate(&self) -> bool {
        runs(self.script("verify.sh").current_dir(self.tree), false)
    }

    /// The URL of an open pull request from a branch whose name starts with
    /// `prefix`, empty when there is none; None when gh cannot list them.
    fn open_pr(&self, prefix: &str) -> Option<String> {
        let jq =
            format!("[.[] | select(.headRefName | startswith(\"{prefix}\")) | .url][0] // \"\"");
        ok_out(self.cmd("gh").current_dir(self.tree).args([
            "pr",
            "list",
            "--state",
            "open",
            "--json",
            "url,headRefName",
            "--jq",
            &jq,
        ]))
    }

    /// Commit the tree's changes as `title` with `body`, push them as the
    /// branch `name` and open a pull request: its URL. A failure is said.
    fn propose(&self, name: &str, title: &str, body: &str) -> Option<String> {
        let git = || {
            let mut c = Command::new("git");
            c.arg("-C").arg(self.tree).current_dir(self.root);
            c
        };
        if !runs(git().args(["add", "-A"]), false)
            || !runs(
                git().args(["commit", "--quiet", "-m", title, "-m", body]),
                false,
            )
            || !runs(
                git().args([
                    "push",
                    "--quiet",
                    self.remote,
                    &format!("HEAD:refs/heads/{name}"),
                ]),
                false,
            )
        {
            eprintln!("tag-release: cannot commit or push {name}; nothing was tagged");
            return None;
        }
        let url = ok_out(self.cmd("gh").current_dir(self.tree).args([
            "pr",
            "create",
            "--base",
            self.branch,
            "--head",
            name,
            "--title",
            title,
            "--body",
            body,
        ]));
        if url.is_none() {
            eprintln!("tag-release: {name} is pushed, but no pull request could be opened for it; nothing was tagged");
        }
        url
    }

    /// The console's `rukas`: TEHAS_CONSOLE, or the head's own built with cargo.
    fn rukas(&self, args: &[&str]) -> Option<String> {
        let mut c = match std::env::var_os("TEHAS_CONSOLE").filter(|c| !c.is_empty()) {
            Some(console) => self.cmd(console),
            None => {
                let mut c = self.cmd("cargo");
                c.args(["run", "--quiet", "--locked", "--manifest-path"])
                    .arg(self.tree.join("console/Cargo.toml"))
                    .args(["-p", "console", "--"]);
                c
            }
        };
        ok_out(c.arg("rukas").args(args))
    }

    /// The image of the pi-rukas host/versions.env follows, and what that
    /// commit is, as the console names them.
    fn followed(&self) -> Option<(String, String)> {
        let env = std::fs::read_to_string(self.tree.join("host/versions.env")).ok()?;
        let follow = tehas_core::env_file::value(&env, "PI_RUKAS_FOLLOW").unwrap_or_default();
        let repo = tehas_core::env_file::value(&env, "PI_RUKAS_REPO").unwrap_or_default();
        let target = self.rukas(&["target", &follow, &repo])?;
        let commit = target.split(' ').next().unwrap_or_default();
        let what = target.split_once(' ').map_or(target.as_str(), |(_, w)| w);
        let image = self.rukas(&["image", &repo, commit])?;
        let what = what.lines().next().unwrap_or_default().to_string();
        Some((image.lines().next().unwrap_or_default().to_string(), what))
    }
}

/// The changelog fragments of a commit, (path, text) in the order they are
/// collected: the regular *.md files of its changelog.d/. None when git
/// cannot read them.
pub fn head_fragments(root: &Path, commit: &str) -> Option<Vec<(String, String)>> {
    let git = || {
        let mut c = Command::new("git");
        c.current_dir(root).stderr(Stdio::null());
        c
    };
    let o = git()
        .args(["ls-tree", "-z", commit, "changelog.d/"])
        .output()
        .ok()?;
    if !o.status.success() {
        return None;
    }
    let mut blobs = Vec::new();
    for entry in o.stdout.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let (meta, path) = entry.split_once('\t')?;
        let mut m = meta.split(' ');
        let (mode, kind, sha) = (m.next()?, m.next()?, m.next()?);
        let name = path.strip_prefix("changelog.d/")?;
        if kind != "blob" || mode == "120000" || name.starts_with('.') || !name.ends_with(".md") {
            continue;
        }
        blobs.push((name.to_string(), sha.to_string()));
    }
    let mut names: Vec<String> = blobs.iter().map(|(n, _)| n.clone()).collect();
    order(&mut names);
    let mut out = Vec::new();
    for n in names {
        let sha = &blobs.iter().find(|(b, _)| *b == n)?.1;
        let o = git().args(["cat-file", "blob", sha]).output().ok()?;
        if !o.status.success() {
            return None;
        }
        out.push((
            format!("changelog.d/{n}"),
            String::from_utf8_lossy(&o.stdout).into_owned(),
        ));
    }
    Some(out)
}

/// The head's changelog fragments moved into the `name` section of its
/// CHANGELOG.md by the head's own scripts/collect-changelog.sh, in a pull
/// request; nothing is tagged. The status the task ends with.
fn fragments_pr(ctx: &Ctx, name: &str, next: &str, head: &str) -> u8 {
    let Some(open) = ctx.open_pr("changelog-") else {
        eprintln!("tag-release: cannot list the open pull requests");
        return 1;
    };
    if !open.is_empty() {
        eprintln!("tag-release: {open} collects the changelog fragments; merge it, then tag. Nothing was tagged");
        return 2;
    }
    if !is_exec(&ctx.tree.join("scripts/collect-changelog.sh")) {
        eprintln!("tag-release: {} has changelog fragments but no scripts/collect-changelog.sh to collect them; nothing was tagged", short(head));
        return 1;
    }
    let (collected, ok) = capture(
        ctx.script("collect-changelog.sh")
            .args(["--section", name])
            .arg(ctx.tree.join("CHANGELOG.md")),
    );
    if !ok {
        eprintln!(
            "tag-release: the changelog fragments could not be collected; nothing was tagged"
        );
        return 1;
    }
    if ctx.verify && !ctx.gate() {
        eprintln!("tag-release: the gate failed with the changelog fragments collected; nothing was pushed or tagged");
        return 1;
    }
    let tree = format!("{}/", ctx.tree.display());
    let paths: Vec<&str> = collected
        .lines()
        .map(|p| p.strip_prefix(&tree).unwrap_or(p))
        .collect();
    let body = fragments_body(name, &paths, next);
    let title = "Collect the changelog fragments into CHANGELOG.md";
    let Some(url) = ctx.propose(&format!("changelog-{next}"), title, &body) else {
        return 1;
    };
    eprintln!("tag-release: {url} collects them; merge it, then tag. Nothing was tagged");
    2
}

/// The body of the pull request that collects the changelog fragments.
pub fn fragments_body(name: &str, paths: &[&str], next: &str) -> String {
    let lines: Vec<String> = paths.iter().map(|p| format!("- {p}")).collect();
    format!(
        "The lines of the changelog fragments, moved into the {name} section of CHANGELOG.md, and the fragments deleted:\n\n{}\n\nOpened by scripts/tag-release.sh, which tags {next} once this is merged.",
        lines.join("\n")
    )
}

/// The polyfills the head carries: kept while upstream works on them, removed
/// in a pull request when that pi-rukas has their features. Some(status) ends
/// the task; None goes on to the pins.
fn polyfills(ctx: &Ctx, a: &Args, scratch: &Path, head: &str, next: &str) -> Option<u8> {
    if !is_exec(&ctx.tree.join("scripts/polyfills.sh")) {
        return None;
    }
    let polyfills = |args: &[&str]| {
        let mut c = ctx.script("polyfills.sh");
        c.arg("--root").arg(ctx.tree).args(args);
        c
    };
    let Some(list) = ok_out(&mut polyfills(&["list"])) else {
        eprintln!("tag-release: cannot list the polyfills of {}", short(head));
        return Some(1);
    };
    if list.is_empty() {
        return None;
    }
    let carried = carried(&list);
    let (caps, rukas) = match &a.rukas_caps {
        Some(file) => (file.clone(), format!("the pi-rukas of {}", file.display())),
        None => {
            let caps = scratch.join("caps.json");
            let (image, what) = ctx.followed().unwrap_or_default();
            let asked = !image.is_empty()
                && runs(
                    ctx.cmd("docker")
                        .args(["pull", "-q", &image])
                        .stderr(Stdio::null()),
                    true,
                )
                && File::create(&caps).is_ok_and(|out| {
                    ends_0(
                        ctx.cmd("docker")
                            .args(["run", "--rm", "--init", &image])
                            .args(["/opt/pi-rukas/bin/pi-rukas-headless", "--capabilities"])
                            .stdout(out)
                            .stderr(Stdio::null()),
                    )
                });
            if !asked {
                let who = if image.is_empty() {
                    "the pi-rukas host/versions.env follows"
                } else {
                    &image
                };
                eprintln!("tag-release: cannot ask {who} whether the polyfills of {} are still needed; nothing was tagged", short(head));
                eprintln!("tag-release: --rukas-caps <file> gives its --capabilities answer");
                return Some(1);
            }
            (caps, format!("pi-rukas {what}"))
        }
    };
    let caps_arg = caps.to_string_lossy().into_owned();
    let Some(expired) = ok_out(&mut polyfills(&["expired", &caps_arg])) else {
        return Some(1);
    };
    let expired: Vec<&str> = expired.lines().collect();
    let gh_state = |kind: &str, r: &str| {
        let (repo, num) = split_ref(r);
        let o = ctx
            .cmd("gh")
            .args([
                kind, "view", num, "-R", repo, "--json", "state", "--jq", ".state",
            ])
            .stderr(Stdio::null())
            .output();
        o.map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim_end_matches('\n')
                .to_string()
        })
        .unwrap_or_default()
    };
    let (merged, untracked) = upstream(
        &carried,
        &expired,
        &rukas,
        |i| gh_state("issue", i),
        |p| gh_state("pr", p),
    );
    for line in merged {
        eprintln!("{line}");
    }
    if !untracked.is_empty() {
        eprintln!("tag-release: these polyfills stand in for no work upstream is doing; nothing was tagged:");
        for line in untracked {
            eprintln!("{line}");
        }
        eprintln!(
            "tag-release: name the upstream issue and an open pull request in each, or remove it"
        );
        return Some(1);
    }
    if expired.is_empty() {
        return None;
    }
    let Some(open) = ctx.open_pr("remove-polyfills-") else {
        eprintln!("tag-release: cannot list the open pull requests");
        return Some(1);
    };
    if !open.is_empty() {
        eprintln!("tag-release: {open} removes polyfills; merge it, then tag. Nothing was tagged");
        return Some(2);
    }
    let have: String = expired.iter().map(|f| format!("{f} ")).collect();
    eprintln!("tag-release: {rukas} has {have}and no longer needs their polyfills");
    let (pruned, ok) = capture(&mut polyfills(&["prune", &caps_arg]));
    for line in pruned.lines() {
        eprintln!("tag-release: removed from {line}");
    }
    if !ok {
        eprintln!("tag-release: the polyfills could not be removed; nothing was pushed");
        return Some(1);
    }
    if ctx.verify && !ctx.gate() {
        eprintln!(
            "tag-release: the gate failed with the polyfills removed; nothing was pushed or tagged"
        );
        return Some(1);
    }
    let body = polyfill_body(&rukas, &carried, &expired, next);
    let fix = format!("remove-polyfills-{next}");
    let title = "Remove the polyfills pi-rukas no longer needs";
    let Some(url) = ctx.propose(&fix, title, &body) else {
        return Some(1);
    };
    eprintln!("tag-release: {url} removes them; merge it, then tag. Nothing was tagged");
    Some(2)
}

/// The pinned tools moved to the newest of their major: when anything moved,
/// a pull request. Some(status) ends the task; None goes on to the gate.
fn pins(ctx: &Ctx, next: &str) -> Option<u8> {
    let Some(open) = ctx.open_pr("pins-") else {
        eprintln!("tag-release: cannot list the open pull requests");
        return Some(1);
    };
    if !open.is_empty() {
        eprintln!(
            "tag-release: {open} moves the pinned tools; merge it, then tag. Nothing was tagged"
        );
        return Some(2);
    }
    let Some(moves) = ok_out(ctx.script("pins.sh").args(["move", "--release", next])) else {
        eprintln!("tag-release: a pinned tool could not be moved; nothing was tagged");
        eprintln!("tag-release: --no-pins tags without moving them");
        return Some(1);
    };
    let moved: Vec<&str> = moves.split('\n').collect();
    if !moves.is_empty() {
        for m in &moved {
            eprintln!("tag-release: {m}");
        }
    }
    let mut status = Command::new("git");
    status.arg("-C").arg(ctx.tree).current_dir(ctx.root);
    let (changes, _) = capture(status.args(["status", "--porcelain"]));
    if changes.is_empty() {
        return None;
    }
    if ctx.verify && !ctx.gate() {
        eprintln!("tag-release: the gate failed with the pinned tools moved; nothing was pushed or tagged");
        return Some(1);
    }
    let body = pins_body(&moved, next);
    let title = "Move the pinned tools to the newest minor and patch";
    let Some(url) = ctx.propose(&format!("pins-{next}"), title, &body) else {
        return Some(1);
    };
    eprintln!("tag-release: {url} moves them; merge it, then tag. Nothing was tagged");
    Some(2)
}

#[cfg(test)]
mod tests;
