//! The store against a writer that does not finish: a process of the store is
//! killed with SIGKILL while its transaction holds the write lock, and the
//! database that survives is asked two questions -- does it pass
//! `integrity_check`, and does it hold the row the killed writer was writing.
//! SIGKILL is the point: it is the signal no process can catch, so nothing of
//! the store runs as it dies. No rollback, no close, no checkpoint: only the
//! file, and the write-ahead log beside it, and whatever SQLite makes of the
//! next connection.

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const STORE: &str = env!("CARGO_BIN_EXE_store");

/// The issue of the row the killed writer holds in its open transaction. Kept
/// well above the issues of the storm's test and of the seeds the writer itself
/// commits first, so that the count the test reads back afterwards names one row
/// and only that one.
const KILLED_ISSUE: i64 = 90_001;

/// The rows of other issues that the killed writer commits before it opens the
/// transaction it is killed inside. They are the file's own history: the killed
/// transaction is one row added to a table that already holds these, and a
/// database that lost them as well as the killed row would be a database that
/// rolled back further than its own transaction. The two numbers are the ones
/// `store __hold` uses, restated here so that the assertion about them is the
/// test's and not a constant of the program's borrowed back to it.
const SEEDED: i64 = 400;

/// How long the killed writer is given to say it is holding its transaction
/// before the test gives up on it. Generous because the machine running the gate
/// has the shell tests and the workspace's own tests at the same moment.
const HELD_WAIT: Duration = Duration::from_secs(60);

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

/// The store asked for one thing, with its answer read back. A call that fails
/// is reported with its exit and its line by the caller that asked for it: the
/// processes of this test are the store's own, so a store that cannot make or
/// read its database has nowhere else to be asked.
fn call(home: &Path, args: &[&str]) -> (i32, String, String) {
    let out = store(home, args).output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// One number out of one of the store's own answers.
fn field(line: &str, name: &str) -> Option<i64> {
    let key = format!("\"{name}\":");
    let rest = line.split(&key).nth(1)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// The next line of a pipe, or `None` once the process has ended without one.
fn line(pipe: &mut Option<std::process::ChildStdout>) -> Option<String> {
    let mut seen = String::new();
    let mut byte = [0u8; 1];
    loop {
        let read = pipe.as_mut()?.read(&mut byte).ok()?;
        if read == 0 {
            return if seen.is_empty() { None } else { Some(seen) };
        }
        if byte[0] == b'\n' {
            return Some(seen);
        }
        seen.push(byte[0] as char);
    }
}

/// The killed writer, let go and waited for: the process is not killed until it
/// has said it holds its transaction, because a kill that arrives before the
/// transaction opens proves nothing about a killed writer.
fn spawn(home: &Path, issue: i64) -> (Child, Option<std::process::ChildStdout>) {
    let mut cmd = store(home, &["__hold", "--issue", &issue.to_string()]);
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The line is read with a deadline rather than by blocking on the pipe: a
    // writer that dies on the way to its transaction has to be reported as that
    // and not hang the gate.
    let until = Instant::now() + HELD_WAIT;
    let mut pipe = child.stdout.take();
    loop {
        assert!(
            Instant::now() < until,
            "the writer never said it was holding after {HELD_WAIT:?}"
        );
        if let Some(said) = line(&mut pipe) {
            let held = field(&said, "held")
                .unwrap_or_else(|| panic!("the writer said {said:?} instead of holding"));
            assert_eq!(held, 1, "the writer held {held} rows, not one");
            return (child, pipe);
        }
        // No line yet: either the process is still on its way to the lock or it
        // has ended. The second is the one the test has to answer for.
        if child.try_wait().unwrap().is_some() {
            panic!("the writer ended before it held its transaction");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The database that survives a writer killed with SIGKILL inside its
/// transaction: the whole of the deliverable's second case. The two questions
/// are the store's own -- `integrity_check` as `check` reads it, and the rows as
/// the store counts them -- because the claim is about what the next connection
/// to the file finds.
#[test]
fn a_writer_killed_mid_transaction_leaves_a_sound_database() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let home = home(&dir, "kill");

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

    let (mut writer, _pipe) = spawn(&home, KILLED_ISSUE);

    // SIGKILL: the signal no process catches. Nothing of the store runs on the
    // way out -- no rollback, no close of the connection, no checkpoint of the
    // log -- so what is left is exactly what SQLite is asked to make sense of.
    let killed = Command::new("kill")
        .args(["-9", &writer.id().to_string()])
        .status()
        .expect("kill -9 of the writer's own pid");
    assert!(killed.success(), "kill -9 of the writer failed");
    // Reaped, so that the process is gone and not merely stopped: the file the
    // test reads next is the file a dead writer left, not one it is still
    // writing.
    let _ = writer.wait();

    // The store reads its own database back. The word the report carries is the
    // one `check` prints when the file is ill, and the exit of the verb that
    // finds it is the one that word gets; here the report is asked of the store
    // rather than of SQLite directly so that the answer is the store's own.
    let (code, report, err) = call(&home, &["__ask", "--issue", &KILLED_ISSUE.to_string()]);
    assert_eq!(code, 0, "the report of the surviving store said {err}");
    let integrity = report
        .split("\"integrity\":\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("the report answered {report:?} with no integrity"));
    assert_eq!(
        integrity, "ok",
        "the store left behind by a killed writer failed its check: {report}"
    );

    // The row the killed writer was writing is not there: a partial write is a
    // row that arrived without its transaction committing it, and this is the
    // assertion that says it did not.
    let lock = field(&report, "lock").expect("the report has no killed row count");
    assert_eq!(
        lock, 0,
        "the killed writer's row survived its own kill: {report}"
    );
    // And the rows that were committed before that transaction opened are all
    // there: a database that lost them too has rolled back further than the one
    // transaction it was asked to forget.
    let committed = field(&report, "locks").expect("the report has no row count");
    assert_eq!(
        committed, SEEDED,
        "the store lost committed rows along with the killed one: {report}"
    );

    // The store's own verb reads the file too, which is the way a person on a
    // host meets it: `check` answers, and answers ok, after the kill.
    let (code, out, err) = call(&home, &["check"]);
    assert_eq!(code, 0, "check of the surviving store said {err}");
    assert!(out.contains("\"integrity\":\"ok\""), "{out}");
    assert!(out.contains("\"ok\":true"), "{out}");
}
