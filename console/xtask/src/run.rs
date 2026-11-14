//! scripts/test.sh: the checks of the platform itself, shellcheck over the
//! shell scripts and the shell tests in test/*_test.sh, side by side in one
//! pool. The script's header and host/docs/tools.md say what a run does and
//! prints; this is that, step by step in the old script's order.
//!
//! Each task of the pool is a second process, `xtask test-task`, that runs the
//! check under timeout and records its status and time when it ends (task). A
//! check that kills it leaves no status, which the run reads as one that never
//! finished.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The lines scripts/verify.sh keeps of a failing step: the block is sized to
/// fit them.
pub const KEEP: usize = 40;

/// The slowest files, started first so none of them starts last and holds the
/// run. A fixed list, the same on every host.
const FIRST: [&str; 9] = [
    "work_",
    "dispatch_",
    "install_",
    "preview_",
    "cycle_",
    "top_",
    "improve_",
    "update_",
    "plan_",
];

/// The programs the run builds for the test files, with the variable that
/// names each one to them.
const BINS: [(&str, &str); 5] = [
    ("T_STORE_BIN", "store"),
    ("T_ATTACH_BIN", "attach"),
    ("T_DISPATCH_BIN", "tehas-dispatch"),
    ("T_CYCLE_BIN", "tehas-cycle"),
    ("T_CONSOLE_BIN", "console"),
];

/// `"$root/$a"`: a path under the root as the shell joins it.
fn under(root: &Path, rel: &str) -> PathBuf {
    let mut s = root.as_os_str().to_owned();
    s.push("/");
    s.push(rel);
    PathBuf::from(s)
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// The paths a run was given, or none for every file.
#[derive(Debug, Default)]
pub struct Given(Option<HashSet<PathBuf>>);

impl Given {
    pub fn asked(&self, p: &Path) -> bool {
        self.0.as_ref().is_none_or(|g| g.contains(p))
    }
    pub fn any(&self) -> bool {
        self.0.is_some()
    }
}

/// The paths of the command line, under the root. A test file named that does
/// not exist is the error, with the message.
pub fn given(root: &Path, args: &[String]) -> Result<Given, String> {
    if args.is_empty() {
        return Ok(Given(None));
    }
    let mut set = HashSet::new();
    for a in args {
        let a = a.strip_prefix("./").unwrap_or(a);
        let test = a.starts_with("test/") && a.ends_with("_test.sh") && a.len() >= 13;
        if test && !under(root, a).is_file() {
            return Err(format!("test.sh: no test file {a}"));
        }
        set.insert(under(root, a));
    }
    Ok(Given(Some(set)))
}

/// `"$dir"/*` filtered on the name: the entries a glob lists, in byte order,
/// without the hidden ones.
fn glob(dir: &Path, keep: impl Fn(&[u8]) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<OsString> = entries
        .flatten()
        .map(|e| e.file_name())
        .filter(|n| !n.as_bytes().starts_with(b".") && keep(n.as_bytes()))
        .collect();
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    names.into_iter().map(|n| dir.join(n)).collect()
}

/// The shell scripts the lint reads, among those asked for.
pub fn lint_files(root: &Path, given: &Given) -> Vec<PathBuf> {
    let sh = |n: &[u8]| n.ends_with(b".sh");
    let mut found = Vec::new();
    found.extend(glob(&root.join("bin"), |_| true));
    found.extend(glob(&root.join("libexec"), |_| true));
    found.extend(glob(&root.join("lib"), sh));
    found.extend(glob(&root.join("scripts"), sh));
    found.extend(glob(&root.join("test"), sh));
    for f in [
        "install.sh",
        "bootstrap.sh",
        "host/env.sh",
        "host/shim/gh",
        "host/sandbox/build.sh",
        "host/sandbox/gh-shim",
        "host/login/tehas",
        "host/release/install",
    ] {
        found.push(under(root, f));
    }
    // publish-lib.sh is a vendored copy, checked where it is kept.
    let vendored = root.join("scripts/publish-lib.sh");
    found
        .into_iter()
        .filter(|f| f.is_file() && *f != vendored && given.asked(f))
        .collect()
}

/// `<stem>*_test.sh`: the name starts with the stem and ends with _test.sh
/// after it.
fn stem_test(name: &[u8], stem: &str) -> bool {
    name.len() >= stem.len() + 8 && name.starts_with(stem.as_bytes()) && name.ends_with(b"_test.sh")
}

/// The test files to run, among those asked for: the slowest first, from the
/// list, then the rest in name order.
pub fn test_files(root: &Path, given: &Given) -> Vec<PathBuf> {
    let dir = root.join("test");
    let mut tests: Vec<PathBuf> = Vec::new();
    let ok = |t: &PathBuf| t.is_file() && given.asked(t);
    for p in FIRST {
        tests.extend(glob(&dir, |n| stem_test(n, p)).into_iter().filter(ok));
    }
    for t in glob(&dir, |n| stem_test(n, "")) {
        if ok(&t) && !tests.contains(&t) {
            tests.push(t);
        }
    }
    tests
}

fn digits(s: &str) -> Option<u128> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(s.parse().unwrap_or(u128::MAX))
}

/// `^[1-9][0-9]*$`.
fn positive(s: &str) -> Option<u128> {
    digits(s).filter(|_| !s.starts_with('0'))
}

/// `read -r a b`: the first word of the first line.
fn first_word(text: &str) -> &str {
    text.lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
}

/// The CPUs this run may use: the cgroup's quota (`cpu.max`), rounded up, when
/// it is less than the CPUs nproc counts. A container's quota does not change
/// what nproc says.
pub fn cpus(nproc: u128, cpu_max: Option<&str>) -> u128 {
    let mut words = cpu_max
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace();
    if let (Some(q), Some(p)) = (
        words.next().and_then(positive),
        words.next().and_then(positive),
    ) {
        let n = q.div_ceil(p);
        if n < nproc {
            return n;
        }
    }
    nproc
}

/// The tasks the memory ceiling holds, at two a GiB, when that is less than
/// the jobs: one shellcheck can reach 2 GiB, and at 4 a GiB the kernel killed
/// them in a cycle's container.
pub fn memory_jobs(jobs: u128, memory_max: Option<&str>) -> u128 {
    let mut n = jobs;
    let line = memory_max.unwrap_or("").lines().next().unwrap_or("");
    if let Some(max) = positive(line.trim_matches([' ', '\t'])) {
        if (max >> 29) < n {
            n = max >> 29;
        }
    }
    n.max(1)
}

/// The size of the pool: TEHAS_TEST_JOBS when it is a number, the default when
/// it is unset or empty, else one.
pub fn jobs(setting: Option<&str>, default: impl FnOnce() -> u128) -> u128 {
    match setting.filter(|s| !s.is_empty()) {
        Some(s) => positive(s).unwrap_or(1),
        None => default().max(1),
    }
}

/// TEHAS_TEST_FILE_TIMEOUT when it is a number, else 300.
pub fn limit(setting: Option<&str>) -> u128 {
    setting.and_then(positive).unwrap_or(300)
}

/// TEHAS_TEST_SLOWEST when it is a number, empty or not, else 5: how many of
/// the slowest files a run names.
pub fn slowest_count(setting: Option<&str>) -> u128 {
    setting.and_then(positive).unwrap_or(5)
}

/// The name a core file of this run has in the directory the tasks run in, or
/// none when this host keeps its dumps where no run could look. A pattern that
/// is a program (it starts with `|`) is what the kernel pipes the dump to; one
/// that holds a `/` is written under its own path and one that holds a `%` named
/// after the crash: none of them is a plain file of the working directory. An
/// unset or empty pattern is `core`, what the kernel writes by itself. The name
/// is taken byte by byte, as the names of the directory are, so a host whose
/// pattern is no text still asks for the file its kernel would write.
pub fn core_name(pattern: Option<&OsStr>) -> Option<OsString> {
    let Some(p) = pattern.filter(|p| !p.is_empty()) else {
        return Some(OsString::from("core"));
    };
    let bytes = p.as_bytes();
    if bytes.starts_with(b"|") || bytes.contains(&b'/') || bytes.contains(&b'%') {
        return None;
    }
    Some(p.to_os_string())
}

/// Whether `name` is the core file `pattern`, or that file with the pid of the
/// crash after it: the two spellings a kernel writes into the directory the task
/// runs in. The bytes of both sides are compared, so a pattern that is no UTF-8
/// text and a name that is none are still told apart.
fn core_named(name: &OsStr, pattern: &OsStr) -> bool {
    let (name, pattern) = (name.as_bytes(), pattern.as_bytes());
    if name == pattern {
        return true;
    }
    name.strip_prefix(pattern)
        .filter(|rest| rest.first() == Some(&b'.'))
        .map(|rest| &rest[1..])
        .is_some_and(|rest| !rest.is_empty() && rest.iter().all(u8::is_ascii_digit))
}

/// What a scan of the checkout for core files came back with. There are three
/// answers and not two, because the opt-in run exists to answer one question -
/// does this host write a dump where the tasks run - and "I could not see" is
/// not the answer "no", while a checkout that has gone away is what the run
/// already failed on and needs no second name.
pub enum Scan {
    /// The core files of the checkout, in name order.
    Found(Vec<OsString>),
    /// The directory is there and could not be read, with the reason.
    Unreadable(String),
    /// Nothing to scan: no dump of this run could have a name here, or the
    /// checkout is not there to be read at all.
    None,
}

/// The core files `dir` holds under `name`: that name, and the name a kernel
/// writes from the second crash of a run on, which is `name` with a `.` and the
/// pid of the crash after it. The digits are what tells a dump from any other
/// file that shares the name: `core.txt` is text a test wrote, not a crash. A
/// directory of either spelling is no core file. A directory that cannot be read
/// is `Scan::Unreadable`, so a run that could not look says that rather than
/// that the host wrote no dump; one that is not there is `Scan::None`.
pub fn left(name: &OsStr, dir: &Path) -> Scan {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Scan::None,
        Err(e) => return Scan::Unreadable(format!("{}: {e}", dir.display())),
    };
    let mut found: Vec<OsString> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name())
        .filter(|n| core_named(n, name))
        .collect();
    found.sort();
    Scan::Found(found)
}

/// Whether the core files of what the run starts are kept: TEHAS_TEST_CORES set
/// to anything that is not empty. Without it every task runs at the core limit
/// of 0, so a program that crashes during a run leaves no file in the checkout
/// a later check reads. `0` keeps them like any other value: the setting asks
/// for the dump, it is not a switch with an off spelling.
pub fn cores_kept(setting: Option<&str>) -> bool {
    setting.is_some_and(|s| !s.is_empty())
}

/// Whether the host is busy: the load average's whole part has reached its
/// CPUs. A load that cannot be read holds nothing back.
pub fn busy(loadavg: Option<&str>, host_cpus: u128) -> bool {
    let load = first_word(loadavg.unwrap_or(""));
    let whole: String = load.chars().take_while(char::is_ascii_digit).collect();
    digits(&whole).is_some_and(|n| n >= host_cpus)
}

fn nproc() -> u128 {
    Command::new("nproc")
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| positive(String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or(1)
}

/// The cases a test file said it ran: the last `<name>: N cases, [K skipped, ]M
/// failed` line of its log, which t_done prints as the file ends.
pub fn cases_in(log: &[u8]) -> Option<u128> {
    // The number that ends `rest` and what is before it.
    fn number(rest: &[u8]) -> Option<(&[u8], &[u8])> {
        let n = rest.iter().rev().take_while(|b| b.is_ascii_digit()).count();
        (n > 0).then(|| rest.split_at(rest.len() - n))
    }
    let mut found = None;
    for line in log.split(|b| *b == b'\n') {
        let Some((rest, _)) = line.strip_suffix(b" failed").and_then(number) else {
            continue;
        };
        let rest = match rest.strip_suffix(b" skipped, ").and_then(number) {
            Some((before, _)) => before,
            None => rest,
        };
        let Some((before, n)) = rest.strip_suffix(b" cases, ").and_then(number) else {
            continue;
        };
        if before.ends_with(b": ") {
            found = digits(&String::from_utf8_lossy(n));
        }
    }
    found
}

/// The text after the first `FAIL: ` that starts a line, trimmed as `read -r`
/// trims it; none when there is no such line or it is empty.
pub fn first_fail(log: &[u8]) -> Option<String> {
    let line = log
        .split(|b| *b == b'\n')
        .find_map(|l| l.strip_prefix(b"FAIL: "))?;
    let text = String::from_utf8_lossy(line);
    let text = text.trim_matches([' ', '\t']);
    (!text.is_empty()).then(|| text.to_string())
}

/// Control characters become `?`, so data cannot print escapes or lines.
pub fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// `tail -n 1` of a log, as `$(...)` gives it.
pub fn last_line(log: &[u8]) -> String {
    let log = log.strip_suffix(b"\n").unwrap_or(log);
    let line = log.rsplit(|b| *b == b'\n').next().unwrap_or(b"");
    String::from_utf8_lossy(line).into_owned()
}

/// `$(cat file)`: the contents without their trailing newlines.
fn value(p: &Path) -> Option<String> {
    let text = fs::read(p).ok()?;
    let text = String::from_utf8_lossy(&text);
    Some(text.trim_end_matches('\n').to_string())
}

/// The lines naming the slowest files, at most `count` of them, slowest first.
/// A count over the files names them all: the count is clamped, never narrowed.
pub fn slowest(slow: &[(u128, String)], count: u128) -> Vec<String> {
    let mut sorted: Vec<&(u128, String)> = slow.iter().collect();
    sorted.sort_by_key(|a| std::cmp::Reverse(a.0));
    sorted
        .into_iter()
        // Narrowing a u128 would drop its high bits: 2^64 would become 0 and
        // name nothing, so a count too big for usize names every file.
        .take(usize::try_from(count).unwrap_or(usize::MAX))
        .map(|(ms, name)| format!("    {:>3}.{} s  {name}", ms / 1000, ms % 1000 / 100))
        .collect()
}

/// A test file that did not pass: its path, why beyond that, and the first
/// failing case its own log names.
#[derive(Debug, Clone, PartialEq)]
pub struct Bad {
    pub name: String,
    pub why: String,
    pub first: Option<String>,
}

/// The block that ends a run that did not pass every check: what failed and
/// what never ran, sized to fit the KEEP lines scripts/verify.sh keeps (the
/// count line, the lint when it failed, the names that fit, the line counting
/// the rest, one line per check that did not run, and two spare). The lint is
/// outside the cap: nothing else says the shell scripts were refused. Empty
/// when everything ran and passed.
pub fn block(lint_failed: bool, bad: &[Bad], skipped: &[String]) -> Vec<String> {
    let lintbad = usize::from(lint_failed);
    let nf = bad.len();
    let failed = lintbad + nf;
    if failed == 0 && skipped.is_empty() {
        return Vec::new();
    }
    let cap = KEEP.saturating_sub(5 + lintbad + skipped.len()).max(1);
    let mut lines = vec![
        String::new(),
        format!(
            "test.sh: failed checks ({failed}), not run ({})",
            skipped.len()
        ),
    ];
    if lint_failed {
        lines.push("    shellcheck".into());
    }
    let shown = nf.min(cap);
    for b in &bad[nf - shown..] {
        let entry = clean(&b.name);
        // The file's own first FAIL: line names the case, in column 1 so a
        // caller looking for FAILED: finds it; the text is data, cut and
        // cleaned so one case cannot take the block's lines.
        if let Some(first) = &b.first {
            let first: String = first.chars().take(200).collect();
            lines.push(format!("FAILED: {entry}: {}", clean(&first)));
        } else if b.why.is_empty() {
            lines.push(format!("    {entry}"));
        } else {
            lines.push(format!("    {entry}: {}", b.why));
        }
    }
    if nf > shown {
        lines.push(format!("    and {} more, in the full log", nf - shown));
    }
    // In column 1, as FAILED: is, so a caller can find it in a passing run.
    for s in skipped {
        lines.push(format!("SKIPPED: {}", clean(s)));
    }
    lines
}

/// What the records of a test file say: whether it is to be named, and why
/// beyond its path. `counted` is whether its log has the count line of t_done:
/// a file that exits 0 without it stopped before its last cases.
pub fn judge(
    log_read: bool,
    counted: bool,
    rc: Option<Option<String>>,
    limit: u128,
) -> Option<String> {
    let mut reason = (!log_read).then(|| "its output could not be read".to_string());
    let ok;
    match rc {
        // A status is written only when the file ends.
        None => {
            ok = false;
            reason = Some("it did not finish, no result recorded".into());
        }
        Some(rc) => {
            let rc = rc.unwrap_or_default();
            if digits(&rc).is_some() {
                ok = digits(&rc) == Some(0);
                if rc == "124" || rc == "137" {
                    reason = Some(format!("timed out after {limit}s"));
                } else if ok && log_read && !counted {
                    reason = Some("ended before t_done".into());
                }
            } else {
                ok = false;
                reason = Some("its recorded result could not be read".into());
            }
        }
    }
    match reason {
        Some(r) => Some(r),
        None if !ok => Some(String::new()),
        None => None,
    }
}

/// Write to stdout as the script's cat and echo did; a reader that has gone
/// ends the run with 141.
fn put(bytes: &[u8]) {
    let mut o = io::stdout().lock();
    if let Err(e) = o.write_all(bytes).and_then(|()| o.flush()) {
        if e.kind() == ErrorKind::BrokenPipe {
            std::process::exit(tehas_core::out::CLOSED);
        }
    }
}

fn putln(s: &str) {
    put(format!("{s}\n").as_bytes());
}

/// Whether a missing shellcheck fails the run rather than skips: in CI, as
/// scripts/verify.sh treats a missing rustfmt or clippy, and when
/// TEHAS_TEST_STRICT asks for it (scripts/verify.sh --full).
pub fn strict(ci: Option<&str>, asked: Option<&str>) -> bool {
    ci.is_some_and(|v| !v.is_empty()) || asked.is_some_and(|v| !v.is_empty())
}

/// What the kernel of this host says it names a dump. An empty file, or none
/// at all, is the kernel left alone, which writes `core`: `core_name` owns that
/// default, so this says only what the host said. TEHAS_TEST_CORE_PATTERN
/// stands in for the file, for the runner's own case.
fn kernel_core_pattern() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    if let Some(p) = std::env::var_os("TEHAS_TEST_CORE_PATTERN").filter(|p| !p.is_empty()) {
        return p;
    }
    let mut text = fs::read("/proc/sys/kernel/core_pattern").unwrap_or_default();
    while matches!(text.last(), Some(b'\n' | b'\r')) {
        text.pop();
    }
    OsString::from_vec(text)
}

fn on_path(name: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|d| {
        let p = if d.as_os_str().is_empty() {
            PathBuf::from(name)
        } else {
            d.join(name)
        };
        p.is_file() && crate::pack::is_exec(&p)
    })
}

fn env(name: &str) -> Option<String> {
    std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
}

/// The run's root, `${TMPDIR:-/tmp}/tehas-tests/run.*`, removed when the run
/// ends. The run holds a flock on its lock, and every process it starts
/// inherits the descriptor: a root whose lock no one holds is what a killed
/// run left.
struct Root {
    dir: PathBuf,
    _lock: File,
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn random_name() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0u8; 6];
    if File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_err()
    {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
            ^ u128::from(std::process::id());
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (seed >> (i * 8)) as u8;
        }
    }
    bytes
        .iter()
        .map(|b| CHARS[usize::from(*b) % CHARS.len()] as char)
        .collect()
}

fn make_root(base: &Path) -> Option<Root> {
    fs::create_dir_all(base).ok()?;
    if let Ok(entries) = fs::read_dir(base) {
        for e in entries.flatten() {
            if !e.file_name().as_bytes().starts_with(b"run.") {
                continue;
            }
            let lock = e.path().join("lock");
            if !lock.exists() {
                continue;
            }
            let free = File::open(&lock).is_ok_and(|f| f.try_lock().is_ok());
            if free {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
    let dir = (0..100).find_map(|_| {
        use std::os::unix::fs::DirBuilderExt;
        let d = base.join(format!("run.{}", random_name()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&d)
            .ok()
            .map(|()| d)
    })?;
    let lock = (|| {
        let f = File::create(dir.join("lock.new")).ok()?;
        f.try_lock().ok()?;
        fs::rename(dir.join("lock.new"), dir.join("lock")).ok()?;
        rustix::io::fcntl_setfd(&f, rustix::io::FdFlags::empty()).ok()?;
        Some(f)
    })();
    match lock {
        Some(f) => Some(Root { dir, _lock: f }),
        None => {
            let _ = fs::remove_dir_all(&dir);
            None
        }
    }
}

/// Build the programs the test files use, once, offline, into the workspace's
/// target with its dev profile. When the offline build fails the locked
/// crates are fetched once and it builds again: a fresh container's registry
/// is empty. Err is why there are none.
fn build(root: &Path, target: &str, out: &Path) -> Result<(), String> {
    let manifest = root.join("console/Cargo.toml");
    if !manifest.is_file() {
        return Err("this checkout has no console/Cargo.toml".into());
    }
    if !on_path("cargo") {
        return Err("cargo is not installed".into());
    }
    let cargo = |args: &[&str], log: &Path| -> bool {
        let Ok(f) = File::create(log) else {
            return false;
        };
        let Ok(e) = f.try_clone() else {
            return false;
        };
        Command::new("cargo")
            .args(args)
            .stdin(Stdio::null())
            .stdout(f)
            .stderr(e)
            .status()
            .is_ok_and(|s| s.success())
    };
    let build_log = out.join("store-build.log");
    let manifest = manifest.to_string_lossy();
    let mut build = vec![
        "build",
        "--locked",
        "--offline",
        "--quiet",
        "--manifest-path",
        &manifest,
    ];
    build.extend(["--target-dir", target]);
    for (_, bin) in BINS {
        build.extend(["-p", bin]);
    }
    let fetch = ["fetch", "--locked", "--quiet", "--manifest-path", &manifest];
    let built = || cargo(&build, &build_log);
    if built() || (cargo(&fetch, &out.join("store-fetch.log")) && built()) {
        return Ok(());
    }
    let log = fs::read(&build_log).unwrap_or_default();
    Err(format!(
        "the store did not build offline: {}",
        last_line(&log)
    ))
}

/// The pool: each check a `test-task` process, at most `jobs` at once, and
/// while the host is busy the next waits for one to end. One always runs.
struct Pool<'a> {
    recorder: &'a Path,
    root: &'a Path,
    jobs: u128,
    limit: String,
    cores: bool,
    host_cpus: u128,
    loadavg: PathBuf,
    running: Vec<Child>,
}

impl Pool<'_> {
    fn busy(&self) -> bool {
        busy(
            fs::read_to_string(&self.loadavg).ok().as_deref(),
            self.host_cpus,
        )
    }

    fn wait_one(&mut self) {
        loop {
            for i in 0..self.running.len() {
                if !matches!(self.running[i].try_wait(), Ok(None)) {
                    let _ = self.running.swap_remove(i).wait();
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Start one check, recorded under <key>. Err is the message of a record
    /// directory that cannot be made.
    fn run(&mut self, key: &Path, out: &Path, cmd: &[&std::ffi::OsStr]) -> Result<(), String> {
        if let Some(dir) = key.parent() {
            if fs::create_dir_all(dir).is_err() {
                let rel = key.strip_prefix(out).unwrap_or(key);
                return Err(format!(
                    "test.sh: no record directory for {}",
                    rel.display()
                ));
            }
        }
        // The core limit goes on the task's own command, so it is set once for
        // the check and for every process the check starts, and nothing the run
        // starts later can be the one to leave it raised.
        let core: &[&std::ffi::OsStr] = if self.cores {
            &[]
        } else {
            &["prlimit".as_ref(), "--core=0".as_ref(), "--".as_ref()]
        };
        let child = Command::new(self.recorder)
            .arg("test-task")
            .arg(key)
            .arg(&self.limit)
            .args(core)
            .args(cmd)
            .current_dir(self.root)
            .spawn();
        if let Ok(c) = child {
            self.running.push(c);
        }
        while self.running.len() as u128 >= self.jobs || (!self.running.is_empty() && self.busy()) {
            self.wait_one();
        }
        Ok(())
    }

    fn wait_all(&mut self) {
        while !self.running.is_empty() {
            self.wait_one();
        }
    }
}

/// `xtask test --root <dir> [path...]`: the run. The exit status is the
/// script's.
pub fn main(root: &Path, args: &[String], recorder: &Path) -> u8 {
    let given = match given(root, args) {
        Ok(g) => g,
        Err(msg) => {
            eprintln!("{msg}");
            return 2;
        }
    };
    let mut skipped: Vec<String> = Vec::new();
    let mut ran = false;
    let files = lint_files(root, &given);
    let mut lint = false;
    let mut lint_failed = false;
    if !on_path("shellcheck") {
        if strict(env("CI").as_deref(), env("TEHAS_TEST_STRICT").as_deref()) {
            if !files.is_empty() {
                putln("shellcheck: not installed, and this run needs it");
                ran = true;
                lint_failed = true;
            }
        } else {
            eprintln!("WARN: shellcheck is not installed, skipping it");
            skipped.push("shellcheck is not installed".into());
        }
    } else if !files.is_empty() {
        ran = true;
        lint = true;
    }
    let cores_kept = cores_kept(env("TEHAS_TEST_CORES").as_deref());
    // Every task of a run that keeps no core file starts under prlimit, which
    // the pool puts in front of the check (Pool::run). Named once, as the run
    // names a missing cargo or shellcheck: a host without it has no task that
    // can start, and that is not N failures naming N record paths.
    if !cores_kept && !on_path("prlimit") {
        eprintln!("test.sh: prlimit is not installed, and every task needs it");
        return 2;
    }

    let host_cpus = nproc();
    let jobs = jobs(env("TEHAS_TEST_JOBS").as_deref(), || {
        let quota = fs::read_to_string("/sys/fs/cgroup/cpu.max").ok();
        let memory = env("TEHAS_TEST_MEMORY_MAX")
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "/sys/fs/cgroup/memory.max".into());
        memory_jobs(
            cpus(host_cpus, quota.as_deref()),
            fs::read_to_string(memory).ok().as_deref(),
        )
    });
    let loadavg = env("TEHAS_TEST_LOADAVG")
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "/proc/loadavg".into());
    let limit = limit(env("TEHAS_TEST_FILE_TIMEOUT").as_deref());
    let slowest_of = slowest_count(env("TEHAS_TEST_SLOWEST").as_deref());
    // Where the kernel of this host puts a dump: `|` is a program it pipes the
    // dump to, a `/` is a path and a `%` a template named after the crash, and
    // none of those is a file of the directory the tasks run in. An empty
    // pattern is the host left alone, and `core_name` calls that `core`. The
    // name is read as bytes, so it is the same name the directory is asked for.
    let pattern = kernel_core_pattern();
    let core_name = core_name(Some(&pattern));
    if cores_kept {
        let shown = pattern.to_string_lossy();
        putln(&format!(
            "test.sh: TEHAS_TEST_CORES keeps the core files of what this run starts, \
             on a host whose kernel.core_pattern is {shown}"
        ));
    }
    // The other half of the setting: with the limit on the run must prove it
    // took effect, so it looks for the very files it meant to prevent. The scan
    // is one read of the checkout's root, at the end of the run, and a file
    // there is the failure the whole change exists to stop.
    let scan = !cores_kept && core_name.is_some();

    let tmp = env("TMPDIR")
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "/tmp".into());
    let Some(run_root) = make_root(&PathBuf::from(format!("{tmp}/tehas-tests"))) else {
        eprintln!("test.sh: no root for the run under {tmp}/tehas-tests");
        return 2;
    };
    let out = run_root.dir.clone();
    // Before any thread: the test files and cargo inherit these.
    std::env::set_var("TMPDIR", &out);
    std::env::remove_var("T_STORE_WHY");
    for (var, _) in BINS {
        std::env::remove_var(var);
    }

    // One build for the gate, not a second tree beside it (#1360).
    let target = env("CARGO_TARGET_DIR")
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| format!("{}/console/target", root.display()));
    let shell = root.join("console/target/shell");
    if fs::symlink_metadata(&shell).is_ok_and(|m| m.is_dir()) {
        let _ = fs::remove_dir_all(&shell);
    } else {
        let _ = fs::remove_file(&shell);
    }
    match build(root, &target, &out) {
        Ok(()) => {
            for (var, bin) in BINS {
                std::env::set_var(var, format!("{target}/debug/{bin}"));
            }
        }
        Err(why) => std::env::set_var("T_STORE_WHY", why),
    }

    let tests = test_files(root, &given);
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).to_path_buf();
    let mut pool = Pool {
        recorder,
        root,
        jobs,
        limit: limit.to_string(),
        cores: cores_kept,
        host_cpus,
        loadavg: PathBuf::from(loadavg),
        running: Vec::new(),
    };
    for t in &tests {
        ran = true;
        // One record per test file, by its path relative to the checkout, so
        // no two files share one and the block reads back the right log.
        let key = out.join(rel(t));
        if let Err(msg) = pool.run(&key, &out, &["bash".as_ref(), t.as_os_str()]) {
            eprintln!("{msg}");
            return 2;
        }
    }
    if lint {
        for f in &files {
            let key = out.join("lint").join(rel(f));
            if let Err(msg) = pool.run(
                &key,
                &out,
                &["shellcheck".as_ref(), "-x".as_ref(), f.as_os_str()],
            ) {
                eprintln!("{msg}");
                return 2;
            }
        }
    }
    pool.wait_all();

    if lint {
        let mut refused = 0;
        for f in &files {
            let key = out.join("lint").join(rel(f));
            let rc = value(&with_suffix(&key, ".rc")).unwrap_or_default();
            if rc == "0" {
                continue;
            }
            refused += 1;
            if let Ok(log) = fs::read(with_suffix(&key, ".log")) {
                put(&log);
            }
            if rc == "124" || rc == "137" {
                putln(&format!(
                    "shellcheck: {} timed out after {limit}s",
                    rel(f).display()
                ));
            }
        }
        if refused == 0 {
            putln(&format!("shellcheck: {} files, ok", files.len()));
        } else {
            putln(&format!(
                "shellcheck: failed, {refused} of {} files",
                files.len()
            ));
            lint_failed = true;
        }
    }

    let mut cases: u128 = 0;
    let mut slow: Vec<(u128, String)> = Vec::new();
    let mut bad: Vec<Bad> = Vec::new();
    let mut bad_keys: Vec<PathBuf> = Vec::new();
    for t in &tests {
        let name = rel(t).to_string_lossy().into_owned();
        let key = out.join(rel(t));
        // A file whose output is gone is a failure even when its status says
        // it passed: the run cannot show what the check said.
        let log = fs::read(with_suffix(&key, ".log"));
        let mut counted = false;
        match &log {
            Ok(log) => {
                put(log);
                if let Some(n) = cases_in(log) {
                    cases = cases.saturating_add(n);
                    counted = true;
                }
            }
            Err(_) => putln(&format!("{}: its output could not be read", clean(&name))),
        }
        if let Some(ms) = value(&with_suffix(&key, ".ms")).as_deref().and_then(digits) {
            slow.push((ms, name.clone()));
        }
        let rc_path = with_suffix(&key, ".rc");
        let rc = rc_path.is_file().then(|| value(&rc_path));
        if let Some(why) = judge(log.is_ok(), counted, rc, limit) {
            bad.push(Bad {
                name,
                why,
                first: None,
            });
            bad_keys.push(key);
        }
    }

    // What the run covered and where its time went.
    if !tests.is_empty() {
        putln(&format!(
            "test.sh: {cases} cases in {} files, {jobs} at a time",
            tests.len()
        ));
        putln("test.sh: the slowest files:");
        for line in slowest(&slow, slowest_of) {
            putln(&line);
        }
    }

    // Paths given with nothing among them to run is its own status, not a
    // pass: a caller that expects it (scripts/verify.sh --changed) names it as
    // a skip, and any other caller sees that nothing was checked.
    if !ran && given.any() && skipped.is_empty() {
        putln("test.sh: no test file and no linted script among the paths given");
        return 3;
    }
    if !ran {
        eprintln!("test.sh: nothing could be checked");
        return 2;
    }
    for (b, key) in bad.iter_mut().zip(&bad_keys) {
        b.first = fs::read(with_suffix(key, ".log"))
            .ok()
            .and_then(|l| first_fail(&l));
    }
    // The core files a task left in the checkout, which the run looks for
    // whichever way the limit went. With it on a crash leaves none, and a file
    // standing there means the limit did not apply on this host, so the run
    // says so instead of passing. With TEHAS_TEST_CORES set the run keeps them,
    // names them here and fails, because a file a crash wrote into the tree
    // dirties what a later check of it reads. The directory is what is asked and
    // not a status: the program that crashes is often a helper the file started
    // and waited for, which leaves the file ending at 0 with its dump behind, so
    // a status that says the file passed is no reason to call a core file that
    // is standing there no core file.
    let cores = match &core_name {
        Some(name) if cores_kept || scan => match left(name, root) {
            Scan::Found(found) => found,
            Scan::Unreadable(why) => {
                // Could not look is not the same answer as found nothing: the
                // question of whether this host wrote a dump stays open.
                putln(&format!(
                    "test.sh: no core file reported: could not read {why}"
                ));
                bad.push(Bad {
                    name: "core files could not be checked".into(),
                    why: clean(&why),
                    first: None,
                });
                Vec::new()
            }
            Scan::None => Vec::new(),
        },
        _ => Vec::new(),
    };
    for name in &cores {
        let name = name.to_string_lossy();
        putln(&format!(
            "test.sh: {name} left in the checkout by a program that crashed"
        ));
    }
    if !cores.is_empty() {
        // The names came off the directory: they are data, joined and cleaned
        // like every other string that reaches the block.
        let names = cores
            .iter()
            .map(|n| n.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        bad.push(Bad {
            name: format!("{} core files in the checkout", cores.len()),
            why: clean(&names),
            first: None,
        });
    }
    let lines = block(lint_failed, &bad, &skipped);
    if !lines.is_empty() {
        putln(&lines.join("\n"));
    }
    drop(run_root);
    u8::from(lint_failed || !bad.is_empty())
}

/// `xtask test-task <key> <limit> <command>...`: one task of the pool. It runs
/// the command under `timeout -k 10 <limit>`, its output in <key>.log, and
/// writes the status to <key>.rc and the time in milliseconds to <key>.ms only
/// when the command ends, so a task that never ends leaves no status. The pool
/// puts `prlimit --core=0 --` in front of the command to keep core files out of
/// the checkout (Pool::run), so the host with no prlimit is told so once, before
/// the pool starts, rather than by N tasks that each end on the command they
/// cannot run.
pub fn task(args: &[OsString]) -> u8 {
    let [key, limit, cmd @ ..] = args else {
        eprintln!("xtask: test-task needs <key> <limit> <command>...");
        return 64;
    };
    let key = Path::new(key);
    let start = Instant::now();
    let rc = match File::create(with_suffix(key, ".log")) {
        Ok(log) => match log.try_clone() {
            Ok(err) => status(
                Command::new("timeout")
                    .args(["-k".as_ref(), "10".as_ref(), limit.as_os_str()])
                    .args(cmd)
                    .stdout(log)
                    .stderr(err)
                    .status(),
            ),
            Err(_) => 1,
        },
        Err(_) => 1,
    };
    let _ = fs::write(with_suffix(key, ".rc"), format!("{rc}\n"));
    let ms = start.elapsed().as_millis();
    let _ = fs::write(with_suffix(key, ".ms"), format!("{ms}\n"));
    0
}

/// `$?` of a command: its status, 128 and the signal that killed it, 127 for
/// one not found and 126 for one that cannot run.
fn status(s: io::Result<std::process::ExitStatus>) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    match s {
        Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
        Err(e) if e.kind() == ErrorKind::NotFound => 127,
        Err(_) => 126,
    }
}

#[cfg(test)]
mod tests;
