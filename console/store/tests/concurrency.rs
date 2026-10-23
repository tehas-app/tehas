//! The store under the load it was built for: eight writers and eight readers of
//! one database for ten seconds, every one of them a process of the store
//! itself. The claim the test is for is section 4 of the design: a database that
//! is busy past the busy timeout is exit 1 with the word `busy`. A store that
//! holds eight writers at once is a store that never says that word to one of
//! them, so the test is an assertion about two counts as much as about sixteen
//! exit statuses: no writer and no reader was turned away, and every write a
//! writer said it committed is a row afterwards.

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const STORE: &str = env!("CARGO_BIN_EXE_store");

/// The storm of the deliverable: eight writers and eight readers.
const WRITERS: usize = 8;
const READERS: usize = 8;

/// How long the storm lasts, as the deliverable states it. Long enough that a
/// writer that would be turned away by a lock has been turned away many times
/// over: at the ten writes a second of one writer this is a thousand transactions
/// of contention per writer.
const STORM_SECONDS: u64 = 10;

/// The row a writer of the storm writes and the test counts: a heartbeat of one
/// issue, which is the row a supervisor's tick of ten seconds really writes. One
/// issue for all eight writers, so the count the test reads back is the eight of
/// them together and not eight counts added up by a test that has to trust each
/// writer's own arithmetic.
const STORM_ISSUE: i64 = 4_242;

/// The store's home for one run: `state/` of mode 0700, as every verb of the
/// store keeps it, and made by the test rather than by the store so that the
/// storm is asked of a directory the store did not choose the mode of.
fn home(dir: &Path, name: &str) -> PathBuf {
    let home = dir.join(name);
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(home.join("state")).unwrap();
    fs::set_permissions(home.join("state"), fs::Permissions::from_mode(0o700)).unwrap();
    home
}

/// The store as a command: the word that names the verb or the shape, then the
/// home named twice -- once by the environment, which is how `main` reads the
/// verbs of section 4, and once by `--home`, which is how `run` reads the four
/// shapes of the storm. Whichever word follows, the process is pointed at one
/// scratch home and no other, and it carries no environment of the test beyond
/// that.
fn store(home: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(STORE);
    cmd.env("TEHAS_HOME", home);
    cmd.args(args);
    // The storm's shapes want their home on the command line and the verbs of
    // section 4 refuse an argument they do not know, so the flag is added only
    // for the words that take it.
    if args.first().is_some_and(|word| word.starts_with("__")) {
        cmd.arg("--home");
        cmd.arg(home);
    }
    cmd
}

/// The store asked for one thing, with its answer read back. The home is named
/// twice, once by the environment for the verbs of section 4 and once by
/// `--home` for the shapes of the storm, because `main` reads only the first and
/// `run` only the second: whichever word follows, the process is pointed at one
/// scratch home and no other.
fn call(home: &Path, args: &[&str]) -> (i32, String, String) {
    let out = store(home, args).output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// One number out of one of the store's own answers, by field name. The answers
/// are the store's one-line objects, so this reads the number out of the line
/// rather than pulling a JSON parser into a test for the sake of one field.
fn field(line: &str, name: &str) -> Option<u64> {
    let key = format!("\"{name}\":");
    let rest = line.split(&key).nth(1)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// One process of the storm: its pid for the report, and the two pipes its answer
/// comes back on.
struct Process {
    child: Child,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    n: usize,
}

impl Process {
    /// The whole of what the process says about itself: its answer, what it
    /// said on standard error, and its exit. Reads the pipes to the end, so a
    /// process that died without answering is seen as that rather than as an
    /// empty answer.
    fn answer(&mut self) -> (String, String, Option<i32>) {
        let mut out = String::new();
        if let Some(pipe) = self.stdout.as_mut() {
            pipe.read_to_string(&mut out).unwrap();
        }
        let mut err = String::new();
        if let Some(pipe) = self.stderr.as_mut() {
            pipe.read_to_string(&mut err).unwrap();
        }
        (out, err, self.child.wait().unwrap().code())
    }
}

/// One process of the storm, let go and not waited for: the point is the sixteen
/// of them meeting the same database at once, which a test that started each one
/// and joined it before the next has never seen.
fn spawn(home: &Path, role: &str, n: usize) -> Process {
    let mut cmd = store(
        home,
        &[
            role,
            "--issue",
            &STORM_ISSUE.to_string(),
            "--for",
            &STORM_SECONDS.to_string(),
        ],
    );
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    Process {
        child,
        stdout,
        stderr,
        n,
    }
}

/// The whole storm: the database made by the store's own `init`, the sixteen
/// processes let go at once, and the rows read back by the store's own `__ask`.
#[test]
fn eight_writers_and_eight_readers_hold_for_ten_seconds() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let home = home(&dir, "concurrency");

    // The store makes its own database: the test does not hand it a file, and the
    // sixteen processes of the storm are of the same binary that made it.
    let (code, out, err) = call(
        &home,
        &[
            "init",
            "--version",
            "v0.10.2",
            "--commit",
            "cb6912a",
            "--instance",
            "tehas-tehas",
            "--repo",
            "example-owner/example-product",
        ],
    );
    assert_eq!(code, 0, "init said {err}");
    assert!(out.contains("\"created\":true"), "{out}");

    let mut writers: Vec<Process> = (0..WRITERS).map(|n| spawn(&home, "__storm", n)).collect();
    let mut readers: Vec<Process> = (0..READERS).map(|n| spawn(&home, "__reader", n)).collect();

    // Every writer, then. The exit a write refused by a lock gets is 1 and the
    // word on standard error is `busy`; the deliverable says none of the eight
    // gets that, so none of them may print it, and none of them may fail with
    // anything else either.
    let mut claimed = 0u64;
    for writer in writers.iter_mut() {
        let (out, err, code) = writer.answer();
        assert_eq!(
            code,
            Some(0),
            "writer {} exited {code:?} with: {err}",
            writer.n
        );
        assert!(
            !err.contains("busy"),
            "busy reached writer {}: {err}",
            writer.n
        );
        let written = field(&out, "writes")
            .unwrap_or_else(|| panic!("writer {} answered {out:?} with no count", writer.n));
        assert!(
            written > 0,
            "writer {} committed nothing in {STORM_SECONDS}s",
            writer.n
        );
        claimed += written;
    }

    // And every reader: WAL mode lets a reader read while a writer writes, so a
    // reader that was told busy met something it should not have met.
    for reader in readers.iter_mut() {
        let (out, err, code) = reader.answer();
        assert_eq!(
            code,
            Some(0),
            "reader {} exited {code:?} with: {err}",
            reader.n
        );
        assert!(
            !err.contains("busy"),
            "busy reached reader {}: {err}",
            reader.n
        );
        let reads =
            field(&out, "reads").unwrap_or_else(|| panic!("reader {} answered {out:?}", reader.n));
        assert!(reads > 0, "reader {} read nothing", reader.n);
    }

    // The rows, read back by the store rather than by a connection of the test's:
    // every write every writer said it committed is in the table, and the count
    // is not short by the number of writes a lock turned away.
    let (code, report, err) = call(&home, &["__ask", "--issue", &STORM_ISSUE.to_string()]);
    assert_eq!(code, 0, "the report of the storm said {err}");
    let rows = field(&report, "heartbeats")
        .unwrap_or_else(|| panic!("the report answered {report:?} with no count"));
    assert_eq!(
        rows, claimed,
        "the writers committed {claimed} rows and the table holds {rows}"
    );
    // The ten seconds were ten seconds of contention and not of an idle store: a
    // storm whose writers together wrote fewer rows than one writer at the rate of
    // section 3 in one second did not write at all.
    assert!(
        rows > (WRITERS as u64) * 10,
        "the storm wrote {rows} rows, which is not contention"
    );

    // The store's own verb reads what the storm left: `check` answers, and
    // answers ok, after sixteen processes have had the file at once.
    let (code, out, err) = call(&home, &["check"]);
    assert_eq!(code, 0, "check after the storm said {err}");
    assert!(out.contains("\"integrity\":\"ok\""), "{out}");
}
