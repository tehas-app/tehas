//! What the tests of `tehasbot preview` share: a stub gh, a stub docker that
//! remembers its containers and images, a stub df, a stub sleep, a real git
//! with a local upstream, and the fixture builders. The command runs as a host
//! has it: bin/tehasbot of the tree, with this build's console beside it in
//! libexec and no TEHAS_CONSOLE. Moved from test/preview_lib.sh, which the
//! notice, author and sweep shell tests still source.

use crate::common::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// One test per value of a shell loop of cases, each named as its case.
#[allow(unused_macros)] // not every test file has such a loop
macro_rules! cases {
    ($check:ident { $($name:ident: $($arg:expr),+;)* }) => {
        $(
            #[test]
            fn $name() {
                $check($($arg),+);
            }
        )*
    };
}

/// The stub bodies, as test/preview_lib.sh gives them to t_stub.
const GH: &str = include_str!("gh.sh");
const DOCKER: &str = include_str!("docker.sh");
const DF: &str = include_str!("df.sh");
const VERDICT: &str = include_str!("gh-verdict.sh");

/// The jq program of the shell's pull(): an open pull request as GitHub serves
/// it, opened by a member of the repository.
const PULL: &str = r#"
    {number: $n, title: "Change \($n)", html_url: "https://github.com/acme/widget/pull/\($n)", state: "open",
     updated_at: $t,
     head: {sha: $sha, repo: (if $repo == "" then null else {full_name: $repo} end)},
     body: (if $d == "" then null else $d end)}
    + (if $who == "" then {} else {author_association: $who} end)"#;

#[allow(dead_code)] // each test file uses a part of this module
/// A host as reset() leaves it. curl and systemctl are left to the deny list,
/// as the shell's are.
pub fn host(name: &str) -> World {
    let w = World::new(&format!("preview-{name}"));
    for name in ["curl", "systemctl"] {
        fs::remove_file(w.t().join("bin").join(name)).unwrap();
    }
    w.gh_stub(GH);
    w.stub("gh-verdict", VERDICT);
    w.stub("docker", DOCKER);
    w.stub("df", DF);
    w.stub("sleep", "");
    reset(&w);
    w
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$T/fx`, where the stubs read their answers.
pub fn fx(w: &World) -> PathBuf {
    w.t().join("fx")
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$T/fx/<rel>`.
pub fn fx_file(w: &World, rel: &str) -> PathBuf {
    fx(w).join(rel)
}

#[allow(dead_code)] // each test file uses a part of this module
/// The previews' directory of the home ($root of the shell).
pub fn pdir(w: &World) -> PathBuf {
    w.home.join("preview")
}

#[allow(dead_code)] // each test file uses a part of this module
/// The upstream repository the product checkout was cloned from.
pub fn upstream(w: &World) -> PathBuf {
    w.t().join("upstream")
}

#[allow(dead_code)] // each test file uses a part of this module
/// The dashboard's list of previews.
pub fn previews_json(w: &World) -> PathBuf {
    w.live_file("previews.json")
}

#[allow(dead_code)] // each test file uses a part of this module
/// The log of the preview of `n` (a number or main).
pub fn log_of(w: &World, n: &str) -> PathBuf {
    w.home.join(format!("logs/preview-{n}.log"))
}

#[allow(dead_code)] // each test file uses a part of this module
/// A fixture file with these bytes.
pub fn put(w: &World, rel: &str, text: &str) {
    let p = fx_file(w, rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

#[allow(dead_code)] // each test file uses a part of this module
/// A fixture file that is there, empty (`: >"$fx/<rel>"`).
pub fn touch(w: &World, rel: &str) {
    put(w, rel, "");
}

#[allow(dead_code)] // each test file uses a part of this module
/// A fixture file is gone (`rm -f "$fx/<rel>"`).
pub fn unput(w: &World, rel: &str) {
    let _ = fs::remove_file(fx_file(w, rel));
}

#[allow(dead_code)] // each test file uses a part of this module
/// The fixtures, the home and the calls of a case before it says anything:
/// an upstream with one commit on main, the product checkout cloned from it,
/// GH_TOKEN and no settle wait, and an instance with previews.
pub fn reset(w: &World) {
    let _ = fs::remove_dir_all(fx(w));
    let _ = fs::remove_dir_all(&w.home);
    let _ = fs::remove_dir_all(upstream(w));
    clear_calls(w);
    fs::create_dir_all(fx(w)).unwrap();
    fs::create_dir_all(w.home.join("repos")).unwrap();
    let up = upstream(w);
    git_args(&["init", "-q", "-b", "main", &s(&up)]);
    git_in(
        &up,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.org",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "first",
        ],
    );
    let clone = w.home.join("repos/widget");
    git_args(&["clone", "-q", &s(&up), &s(&clone)]);
    w.set_env(&["GH_TOKEN=sekret-token", "TEHAS_PREVIEW_SETTLE=0"]);
    w.minimal(&["TEHAS_PREVIEWS=1", "TEHAS_DOMAIN=example.org"]);
}

#[allow(dead_code)] // each test file uses a part of this module
/// Every file of `$T/calls` is gone (`rm -f "$T"/calls/*`).
pub fn clear_calls(w: &World) {
    rm_calls(w, "");
}

#[allow(dead_code)] // each test file uses a part of this module
/// The calls whose file names start with `prefix` are gone
/// (`rm -f "$T"/calls/<prefix>*`).
pub fn rm_calls(w: &World, prefix: &str) {
    let d = w.t().join("calls");
    fs::create_dir_all(&d).unwrap();
    for e in fs::read_dir(&d).unwrap() {
        let e = e.unwrap();
        if e.file_name().to_string_lossy().starts_with(prefix) {
            fs::remove_file(e.path()).unwrap();
        }
    }
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// git as the test's own, with no configuration of the machine it runs on.
fn git_cmd() -> Command {
    let mut c = Command::new("git");
    c.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(Stdio::null());
    c
}

fn git_args(args: &[&str]) -> String {
    let out = git_cmd().args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[allow(dead_code)] // each test file uses a part of this module
/// `git -C <dir> <args>`, its output without the last newlines.
pub fn git_in(dir: &Path, args: &[&str]) -> String {
    let d = s(dir);
    let mut all = vec!["-C", &d];
    all.extend_from_slice(args);
    git_args(&all).trim_end_matches('\n').to_string()
}

/// What makes each commit's file its own, as $RANDOM did in the shell.
static COMMITS: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)] // each test file uses a part of this module
/// A commit on the upstream's `branch`, from main, with the recipe; its id.
pub fn commit(w: &World, branch: &str) -> String {
    commit_as(w, branch, "recipe")
}

#[allow(dead_code)] // each test file uses a part of this module
/// A commit on the upstream's `branch`, from main; with the recipe when `with`
/// is "recipe", without it otherwise. Its id.
pub fn commit_as(w: &World, branch: &str, with: &str) -> String {
    let up = upstream(w);
    git_in(&up, &["checkout", "-q", "-B", branch, "main"]);
    if with == "recipe" {
        fs::create_dir_all(up.join("preview")).unwrap();
        fs::write(up.join("preview/Dockerfile"), "FROM scratch\n").unwrap();
    } else {
        let _ = fs::remove_dir_all(up.join("preview"));
    }
    let n = COMMITS.fetch_add(1, Ordering::Relaxed);
    fs::write(up.join("file"), format!("{branch} {n}\n")).unwrap();
    git_in(&up, &["add", "-A"]);
    git_in(
        &up,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.org",
            "commit",
            "-q",
            "-m",
            branch,
        ],
    );
    git_in(&up, &["rev-parse", "HEAD"])
}

#[allow(dead_code)] // each test file uses a part of this module
/// jq with these arguments and this standard input: its standard output
/// without the last newlines, as a command substitution has it.
pub fn jq_run(args: &[&str], input: Option<&str>) -> String {
    let mut child = Command::new("jq")
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    if let Some(text) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(jq -r <filter> <file>)`.
pub fn jq_r(filter: &str, file: &Path) -> String {
    jq_run(&["-r", filter, &s(file)], None)
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(jq <flags> <filter> <file>)`.
pub fn jq_file(flags: &[&str], filter: &str, file: &Path) -> String {
    let f = s(file);
    let mut args = flags.to_vec();
    args.push(filter);
    args.push(&f);
    jq_run(&args, None)
}

#[allow(dead_code)] // each test file uses a part of this module
/// `jq <flags> <filter> <file> >"$fx/x" && mv "$fx/x" <file>`.
pub fn jq_edit(w: &World, flags: &[&str], filter: &str, file: &Path) {
    let f = s(file);
    let mut args = flags.to_vec();
    args.push(filter);
    args.push(&f);
    let out = Command::new("jq").args(&args).output().unwrap();
    assert!(out.status.success(), "jq {filter} {f}");
    let x = fx_file(w, "x");
    fs::write(&x, &out.stdout).unwrap();
    fs::rename(&x, file).unwrap();
}

#[allow(dead_code)] // each test file uses a part of this module
/// pull <number> <commit>: an open pull request of the repository, opened by
/// a member, and what GitHub serves as its head. Its JSON, one line, which is
/// also what GitHub answers about it.
pub fn pull(w: &World, n: u64, sha: &str) -> String {
    git_in(
        &upstream(w),
        &["update-ref", &format!("refs/pull/{n}/head"), sha],
    );
    let n_s = n.to_string();
    let text = jq_run(
        &[
            "-n",
            "-c",
            "--argjson",
            "n",
            &n_s,
            "--arg",
            "sha",
            sha,
            "--arg",
            "repo",
            "acme/widget",
            "--arg",
            "who",
            "MEMBER",
            "--arg",
            "d",
            "What the change does.",
            "--arg",
            "t",
            "2026-10-11T09:00:00Z",
            PULL,
        ],
        None,
    );
    put(w, &format!("pull-{n}.json"), &format!("{text}\n"));
    text
}

#[allow(dead_code)] // each test file uses a part of this module
/// pulls <pull request>...: the list of open pull requests GitHub answers.
pub fn pulls(w: &World, items: &[&str]) {
    let mut input = items.join("\n");
    input.push('\n');
    let list = jq_run(&["-s", "."], Some(&input));
    put(w, "pulls.json", &format!("{list}\n"));
}

#[allow(dead_code)] // each test file uses a part of this module
/// close <number>: GitHub answers that pull request as closed.
pub fn close(w: &World, n: u64) {
    jq_edit(
        w,
        &["-c"],
        ".state = \"closed\"",
        &fx_file(w, &format!("pull-{n}.json")),
    );
}

#[allow(dead_code)] // each test file uses a part of this module
/// `tehasbot preview <words>` from the tree, as the shell's `t_run "$preview
/// <words>"`.
pub fn preview(w: &World, words: &str) -> Run {
    let cmd = w.t().join("tree/bin/tehasbot");
    let sep = if words.is_empty() { "" } else { " " };
    w.sh(&format!(
        "unset TEHAS_CONSOLE; {} preview{sep}{words}",
        cmd.display()
    ))
}

#[allow(dead_code)] // each test file uses a part of this module
/// A snippet in the environment of a run, with no TEHAS_CONSOLE, as the
/// shell's t_run had it.
pub fn sh(w: &World, snippet: &str) -> Run {
    w.sh(&format!("unset TEHAS_CONSOLE; {snippet}"))
}

#[allow(dead_code)] // each test file uses a part of this module
/// The console of the tree.
pub fn console(w: &World) -> PathBuf {
    w.t().join("tree/libexec/console")
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(cat <file>)`: its bytes without the last newlines, "" when there is
/// none.
pub fn cat(p: &Path) -> String {
    fs::read_to_string(p)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// state_of <number>: the state of that pull request.
pub fn state_of(w: &World, n: &str) -> String {
    jq_r(".state", &pdir(w).join(n).join("state.json"))
}

#[allow(dead_code)] // each test file uses a part of this module
/// `jq -r .reason` of the state of `n`.
pub fn reason_of(w: &World, n: &str) -> String {
    jq_r(".reason", &pdir(w).join(n).join("state.json"))
}

#[allow(dead_code)] // each test file uses a part of this module
/// count <directory> <name pattern>: how many entries of that name it has.
/// The pattern knows `*` only, which is all the cases use.
pub fn count(dir: &Path, pattern: &str) -> usize {
    let Ok(rd) = fs::read_dir(dir) else {
        return 0;
    };
    rd.filter(|e| {
        let name = e.as_ref().unwrap().file_name();
        glob(pattern, &name.to_string_lossy())
    })
    .count()
}

fn glob(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, rest)) => {
            let Some(tail) = name.strip_prefix(head) else {
                return false;
            };
            (0..=tail.len())
                .filter(|&i| tail.is_char_boundary(i))
                .any(|i| glob(rest, &tail[i..]))
        }
    }
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(t_calls docker)`: the docker calls, one a line.
pub fn docker(w: &World) -> String {
    w.calls("docker").trim_end_matches('\n').to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(t_calls gh)`: the gh calls, one a line.
pub fn gh_calls(w: &World) -> String {
    w.gh().trim_end_matches('\n').to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(t_calls <name>)`.
pub fn calls(w: &World, name: &str) -> String {
    w.calls(name).trim_end_matches('\n').to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The run ended with `want`.
#[track_caller]
pub fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "{}", r.out);
}

#[allow(dead_code)] // each test file uses a part of this module
/// `grep -c <regex>` of `text`, one line at a time; the regex is written in
/// the regex crate's syntax with the same meaning as the shell's.
pub fn grep_c(text: &str, re: &str) -> usize {
    let re = regex::Regex::new(re).unwrap();
    text.lines().filter(|l| re.is_match(l)).count()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The lines of `text` that match `re`, joined by newlines (`grep`).
pub fn grep(text: &str, re: &str) -> String {
    let re = regex::Regex::new(re).unwrap();
    text.lines()
        .filter(|l| re.is_match(l))
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(dead_code)] // each test file uses a part of this module
/// builds: how many images a run built.
pub fn builds(w: &World) -> usize {
    grep_c(&docker(w), "^build ")
}

#[allow(dead_code)] // each test file uses a part of this module
/// runs: how many containers a run started.
pub fn runs(w: &World) -> usize {
    fs::read_dir(w.t().join("calls"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("docker.run.")
        })
        .count()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The arguments of the container of `n`, as `tr '\n' ' '` gives them.
pub fn run_args(w: &World, n: &str) -> String {
    fs::read_to_string(w.t().join(format!("calls/docker.run.{n}")))
        .unwrap()
        .replace('\n', " ")
}

#[allow(dead_code)] // each test file uses a part of this module
/// image_rm_calls: how many images it was told to remove.
pub fn image_rm_calls(w: &World) -> usize {
    grep_c(&docker(w), "^image rm")
}

#[allow(dead_code)] // each test file uses a part of this module
/// image_rm_calls <needle>: how many removals name exactly that id or tag.
pub fn image_rm_calls_of(w: &World, needle: &str) -> usize {
    let line = format!("image rm {needle}");
    docker(w).lines().filter(|l| *l == line).count()
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(writes)`: the writes to GitHub, one a line.
pub fn writes(w: &World) -> String {
    calls(w, "gh-writes")
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(cat "$T/calls/gh-writes" | wc -l)`.
pub fn writes_lines(w: &World) -> String {
    w.calls("gh-writes").matches('\n').count().to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// image_id <number>: the id of the image the tag of that preview holds.
pub fn image_id(w: &World, n: &str) -> String {
    cat(&fx_file(w, &format!("docker/image-{n}")))
}

#[allow(dead_code)] // each test file uses a part of this module
/// untagged <number>: the ids of the images a build of that preview left with
/// no tag, one a line.
pub fn untagged(w: &World, n: &str) -> String {
    cat(&fx_file(w, &format!("docker/untagged-{n}")))
}

#[allow(dead_code)] // each test file uses a part of this module
/// image_rm_unexpected: the image names the docker stub would not remove.
pub fn image_rm_unexpected(w: &World) -> String {
    cat(&fx_file(w, "docker-unexpected"))
}

#[allow(dead_code)] // each test file uses a part of this module
/// The commit a running preview of `n` was started from.
pub fn container_of(w: &World, n: &str) -> String {
    cat(&fx_file(w, &format!("docker/container-{n}")))
}

#[allow(dead_code)] // each test file uses a part of this module
/// image_of <commit>: the id the docker stub gives the image built from that
/// commit, the hash of its archive; the stub's build makes the same of the
/// same bytes. A commit with no archive is a panic.
pub fn image_of(w: &World, sha: &str) -> String {
    let out = git_cmd()
        .args(["-C", &s(&upstream(w)), "archive", sha])
        .output()
        .unwrap();
    assert!(out.status.success(), "no archive for that commit");
    let hex: String = Sha256::digest(&out.stdout)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{}", &hex[..12])
}

#[allow(dead_code)] // each test file uses a part of this module
/// `$(tar -tf "$fx/docker/context.tar" | grep -c '^preview/Dockerfile$')`:
/// how many entries of the build's context are preview/Dockerfile.
pub fn dockerfiles_in_context(w: &World) -> usize {
    let out = Command::new("tar")
        .arg("-tf")
        .arg(fx_file(w, "docker/context.tar"))
        .output()
        .unwrap();
    grep_c(
        &String::from_utf8_lossy(&out.stdout),
        "^preview/Dockerfile$",
    )
}
