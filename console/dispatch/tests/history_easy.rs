//! The refresh-kind rules of #1031 driven by what a host actually answers: the
//! history rows of the real store program's `history-show`, and the pull
//! request of the real PULL query (#1751). Before this the store answered no
//! `easy` on a history row and the query asked for no `statusCheckRollup`, so
//! both rules read nothing and every refresh after the first easy merge took
//! the full path while the fixtures, hand-built, said otherwise.
//!
//! The store is this build's `store` binary, and the history rows are its own
//! answer and its own approval record, not written fixtures. The pull request
//! node is what `read::github::pull` leaves behind for a GitHub answer of the
//! shape the PULL query gets.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::Duration;
use tehas_core::testdir::TestDir;
use tehas_dispatch::read::run::Calls;
use tehas_dispatch::read::store;
use tehas_dispatch::text::body_sha256;
use tehas_dispatch::{decide, Settings, Snapshot};

const REPO: &str = "acme/widget";
const BOT: &str = "tehas-bot";
/// The check the instance names, and a name the rule must not read.
const CHECK: &str = "gate";
const OTHER: &str = "something else";
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "cccccccccccccccccccccccccccccccccccccccc";
/// The body the approval binds, as the refresh fixtures have it.
const WORDS: &str = "Words.";
/// The pull request the refresh reads, and the issue it closes.
const PR: u64 = 50;
const ISSUE: u64 = 7;
/// alice's approval of the cycle that waits for a merge, and the starts of the
/// record, each 200 s after the last. A refresh that began before the approval
/// it rests on is one the poll holds back, so the record's starts are after it.
const APPROVED: i64 = 1_790_869_201;
/// The approval the poll of a refresh rests on: written after every row of the
/// record, and held unused until the start the poll makes, as the record of a host
/// with a refresh to do holds it. `owed` writes it.
const OWED_AT: i64 = 1_790_900_001;
/// When the person ticks the box that merges the branch: after the refresh that ran
/// as the easy merge, so its row follows that one in the record and is the newest
/// start the store answers, and before the rows the world writes after it.
const MERGE_AT: i64 = OWED_AT + 300;

/// The store program of this build. This harness is a library's, so cargo names
/// no binary of the package for it and `CARGO_BIN_EXE_store` is absent; the
/// directories above the harness hold the store of the profile under test. A
/// run that built nothing yet is no failure: the store is built, not skipped,
/// so the case is honest about the program a host runs.
fn store_binary() -> PathBuf {
    if let Some(found) = store_above_harness() {
        return found;
    }
    build_store();
    store_above_harness().expect("the store of this build is above the test binary")
}

/// The nearest directory above the harness that holds the store.
fn store_above_harness() -> Option<PathBuf> {
    let exe = std::env::current_exe().unwrap();
    exe.ancestors()
        .skip(1)
        .map(|dir| dir.join("store"))
        .find(|p| p.is_file())
}

/// One build of the store for the whole harness, into the profile's own target
/// directory. `Once` because the cases run in parallel and cargo's lock makes a
/// second build wait for the first anyway.
fn build_store() {
    static BUILT: Once = Once::new();
    let mut outcome: Option<Result<(), String>> = None;
    BUILT.call_once(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"));
        let built = Command::new("cargo")
            .arg("build")
            .args(["--locked", "--offline", "--quiet", "--manifest-path"])
            .arg(&manifest)
            .args(["--target-dir"])
            .arg(&target)
            .args(["-p", "store"])
            .args((!cfg!(debug_assertions)).then_some("--release"))
            .stdin(Stdio::null())
            .output();
        outcome = Some(match built {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
            Err(e) => Err(format!("cargo: {e}")),
        });
    });
    if let Some(Err(words)) = outcome.as_ref() {
        panic!("the store of this build could not be built: {words}");
    }
}

/// The home of a case: the store's file, the mark of a home, and the cycle
/// directory whose lock every start is made under.
fn init(name: &str) -> TestDir {
    let home = TestDir::new(&format!("dispatch-easy-{name}"));
    fs::create_dir_all(home.join("state/cycles")).unwrap();
    fs::write(home.join("state/.tehas-home"), "{}\n").unwrap();
    run(
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
    home
}

/// One call of the store program at the store's own clock of `now`, whose
/// success the case insists on: a store that refuses is a broken fixture, not
/// an answer to read. `TEHAS_STORE_TEST_NOW` is the clock a debug build reads,
/// so the rows of the record carry the times of the world the case means rather
/// than the wall clock's, and the rules that order one start against another
/// read what a host's record would say.
fn run_at(home: &Path, now: i64, args: &[&str]) -> String {
    let r = {
        let mut c = store_command(home, args);
        c.env("TEHAS_STORE_TEST_NOW", now.to_string());
        finish(&mut c)
    };
    String::from_utf8_lossy(&r.stdout).to_string()
}

fn run(home: &Path, args: &[&str]) -> String {
    // No clock named: the store's own, which is what a host's calls carry.
    let r = finish(&mut store_command(home, args));
    String::from_utf8_lossy(&r.stdout).to_string()
}

/// The store of `home` and one call of its program: the arguments, no standard
/// input, and its exit and output read out.
fn store_command(home: &Path, args: &[&str]) -> std::process::Command {
    let mut c = Command::new(store_binary());
    c.args(args).env("TEHAS_HOME", home).stdin(Stdio::null());
    c
}

fn finish(c: &mut std::process::Command) -> std::process::Output {
    let r = c.output().unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    r
}

/// The lock of an issue's cycle, held by this process while one start is
/// written: the store insists the unit that starts a cycle holds it.
fn hold(home: &Path, issue: u64) -> fs::File {
    let dir = home.join("state/cycles");
    fs::create_dir_all(&dir).unwrap();
    let f = fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("{issue}.lock")))
        .unwrap();
    f.try_lock().unwrap();
    f
}

/// What one host's #7 went through, and what the poll then decided. The store
/// holds alice's approval of the cycle that waits for a merge, the refreshes the
/// dispatcher started after it, and one approval held unused: the refresh the poll
/// is asked about, which is what the pull request rests on. `easy` marks the last
/// refresh as the easy merge, and `pushed` says that merge went out, which is what
/// an easy merge's push is on the record. `rollup` is GitHub's answer for the pull
/// request's checks, and `check` whether the instance names one.
struct World {
    home: TestDir,
    history: Vec<Value>,
    rollup: Value,
    check: bool,
    /// The attempt of the start that ran as the easy merge, which the world notes
    /// as it writes it: the approval of that start is the base the poll's refresh
    /// rests on.
    easy_start: Option<i64>,
    /// Whether the approval the poll's refresh runs as is marked the easy merge:
    /// `true` once the record's own approval has had its one easy merge, which is
    /// what the poll reads and what holds a second easy merge back.
    had_easy: bool,
    /// Whether the world has written the cycle of the merge that an easy merge
    /// pushed: the row of the person's tick that ends as `merged`. Once it is on
    /// record the refresh the poll is owed rests on that tick and no longer on
    /// alice's approval, which is what the record of a host says (#1751).
    merged: bool,
    /// Whether the person has ticked the box of the retry since the last easy merge:
    /// a fresh tick is a fresh approval, owed its own easy merge (#1031). The world
    /// starts with one, because the first refresh of a pull request runs on a tick.
    ticked: bool,
    /// The next time the world writes a row: after the last row it wrote, so the
    /// approval of the poll's refresh is the newest row of the record and no start of
    /// the world takes it. An approval is stamped with it, never with the time of the
    /// cycle that runs on it: `cycle-start` takes the newest unused row, so the order
    /// of `at` is what says which row a start takes, and the order of the record is
    /// what `approval-current` reads the last start off.
    clock: i64,
}
impl World {
    fn new(name: &str) -> World {
        World {
            home: init(name),
            history: Vec::new(),
            rollup: Value::Null,
            check: true,
            easy_start: None,
            had_easy: false,
            merged: false,
            ticked: false,
            clock: MERGE_AT + 1000,
        }
    }

    /// alice's approval of the cycle whose pull request now waits for a merge.
    fn approved(&mut self) -> &mut World {
        self.one_start_at("label", APPROVED, "handoff", false, false);
        self
    }

    /// The approval the poll's refresh rests on, held unused on the record: the
    /// record of a host with a refresh to do holds the approval of it, written after
    /// every row of the record and taken by no start. The store answers the base of
    /// the issue's last start beside it, and that is the row the refresh of the poll
    /// runs on: its counts are the counts of the base the rules read.
    fn owed(&mut self) -> &mut World {
        let n = ISSUE.to_string();
        // The newest row of the record: the approval the pull request of the refresh
        // rests on, held unused until the start the poll makes.
        let at = self.next_at();
        let at = at.to_string();
        let pr = PR.to_string();
        let sha = body_sha256(&json!({"body": WORDS}));
        let args = vec![
            "approval-record",
            n.as_str(),
            "--at",
            at.as_str(),
            "--by",
            "alice",
            "--how",
            "refresh",
            "--body-sha",
            sha.as_str(),
            "--refresh-pr",
            pr.as_str(),
            "--refresh-head",
            HEAD,
        ];
        run_at(&self.home, at.parse().unwrap(), &args);
        self
    }

    /// `n` refreshes of the pull request, the last of them the easy merge when
    /// `easy`, the last one ended as `status` says. A refresh runs on the tick of the
    /// person who asked for it, so the world must hold a tick before it starts one --
    /// the row of that tick is the row the refresh runs on, one approval and one start
    /// (#1751). Each start is written as the dispatcher writes it: the store marks the
    /// approval of an easy start, and `history-show` reads the mark back from it, so a
    /// refresh that ran as one is a start whose own approval says so.
    fn refreshes(&mut self, n: u64, easy: bool, parked: bool, status: &str) -> &mut World {
        self.refreshes_at(n, easy, parked, status, 0)
    }

    /// The `n` refreshes with the cycles of the world moved `after` the approval each
    /// refresh runs on. The hold of a park reads a row's start against the stamp of
    /// the approval the refresh rests on, so a case that holds a park places its
    /// refresh after that approval, as a host's poll does.
    fn refreshes_at(
        &mut self,
        n: u64,
        easy: bool,
        parked: bool,
        status: &str,
        after: i64,
    ) -> &mut World {
        // Each refresh runs on the tick of the person who asked for it, and the row of
        // that tick is the row the start runs on: one approval, one start. A cycle
        // began and ended after the approval it ran on, which is where the row of the
        // tick sits in the record -- a start that ran before the approval it rests on is
        // a start that predates it, and the holds that read a row's start measure it
        // against the stamp of that approval. The approval of the poll's refresh is
        // written last, after every row of the record, which is how the record of a
        // host with a refresh to do holds it.
        for i in 1..=n {
            // The time the cycle of this refresh ran: after the approval the person
            // gave, and ahead of the person's tick that merges the branch -- that row
            // is the last start of a record whose merge went out. Each of the world's
            // cycles begins and ends before it.
            let began = (MERGE_AT - 300 * (n - i + 1) as i64 + after)
                .min(MERGE_AT - 301 * (n - i + 1) as i64);
            let easy_this = i == n && easy;
            // A refresh that parked is written by `parked`, not here: the world of a
            // case that ends one way does not also write the other, and a row that both
            // merged and parked is a row no host has.
            let said = if easy_this && status == "merged" {
                "failed"
            } else {
                status
            };
            let attempt = self.one_start_at("refresh", began, said, easy_this, i == n && parked);

            if easy_this {
                self.easy_start = Some(attempt);
                // The easy merge spends the tick of the person who asked for it: a
                // second one on the same approval is owed nothing, which is what the
                // store's mark of the base answers (#1031).
                self.had_easy = true;
                self.ticked = false;
            }
        }
        self.owed();
        self
    }

    /// The person ticks the box of the retry, and the refresh of the pull request runs
    /// on that tick: one row of the record holds both the approval the pull request
    /// rests on and the word its cycle ended with, which is how a world writes a
    /// refresh whose cycle parked -- the park the poll holds the refresh on is read off
    /// the row of the approval it rests on. The tick is a fresh approval, owed the one
    /// easy merge of #1031, and `easy` is what says the refresh that ran on it was the
    /// easy one: the store takes the mark on a refresh row and nowhere else.
    fn tick(&mut self, status: &str, parked: bool, easy: bool) -> &mut World {
        // The row of the tick is a refresh row when its start ran as the easy merge:
        // the store takes `--easy` on a refresh row and nowhere else, and the row of
        // the tick is the row the start ran on (#1751).
        let how = if easy { "refresh" } else { "retry-tick" };
        // The cycle of the tick's start: the row the park is read off, placed where a
        // start that runs on an approval sits -- after the approval it rests on.
        let began = self.clock.max(MERGE_AT - 301);
        self.one_start_at(how, began, status, easy, parked);
        self.ticked = true;
        // The record of a host with a refresh to do holds the approval of the poll's
        // refresh too: the row the dispatcher wrote when it decided the refresh, held
        // unused until the start the poll makes.
        self.owed()
    }

    /// One start of the record on alice's approval of `how`, written as the only
    /// unused approval and taken by the start that runs on it: recorded at `began`,
    /// started a second later and ended 300 s after it as `status` says, on the
    /// store's clock of `began` so the record carries the times of the world the case
    /// means. The approval is written at `began`, which is where the cycle that runs
    /// on it sits in the order the starts take them: `cycle-start` runs on the newest
    /// unused row, and the keeping and `approval-current` read the record in the order
    /// of `at`, so a row is never written before one whose cycle already ran.
    /// `easy` marks the approval as the easy merge, the mark the store's rows are
    /// read through, and `parked` says the start was told to stop before it ran.
    /// The attempt the store named is the answer, and the record holds no unused
    /// approval when it returns -- until the world is owed its next start.
    fn one_start_at(
        &mut self,
        how: &str,
        began: i64,
        status: &str,
        easy: bool,
        parked: bool,
    ) -> i64 {
        let n = ISSUE.to_string();
        // The row sits in the record where its cycle sits: `cycle-start` runs on the
        // newest unused approval and the keeping reads the record in the order of
        // `at`, so a row written after one whose cycle ran later would be taken by a
        // start that comes after it and leave this cycle with none -- or worse, make
        // the record say a start ran after the one that really ran last. Ahead of
        // every row of the world still, so the row of the poll's refresh -- written
        // last of all, at `owed` -- stays the newest of the record and the one no
        // start takes.
        // Stamped as the next row of the record, not with the time of its cycle: the
        // start takes the newest unused approval, so an approval is always newer than
        // the one before it, and the cycle of `began` is the row that says what ran
        // last. The row of the poll's refresh -- written last of all, at `owed` --
        // stays the newest of the record and the one no start takes.
        let at = self.next_at();
        let at = at.to_string();
        let pr = PR.to_string();
        let sha = body_sha256(&json!({"body": WORDS}));
        let mut args: Vec<&str> = vec![
            "approval-record",
            n.as_str(),
            "--at",
            at.as_str(),
            "--by",
            "alice",
            "--how",
            how,
            "--body-sha",
            sha.as_str(),
        ];
        match how {
            // The row of a refresh carries the tick the person gave for it: the person
            // ticks the box of the retry once for each refresh of the pull request,
            // and the row of that tick is the row the refresh runs on (#1751).
            "refresh" => args.extend([
                "--comment-id",
                "1",
                "--refresh-pr",
                pr.as_str(),
                "--refresh-head",
                HEAD,
            ]),
            "label" => args.extend(["--label", "tehas:ready"]),
            _ => args.extend(["--comment-id", "1"]),
        }
        if easy {
            args.push("--easy");
        }
        run_at(&self.home, began, &args);
        let attempt = {
            let _lock = hold(&self.home, ISSUE);
            let v: Value =
                serde_json::from_str(&run_at(&self.home, began + 1, &["cycle-start", &n])).unwrap();
            v["attempt"].as_i64().unwrap()
        };
        let attempt = attempt.to_string();
        let mut args = vec![
            "cycle-end",
            n.as_str(),
            "--attempt",
            &attempt,
            "--status",
            status,
        ];
        if parked {
            args.push("--parked");
        }
        run_at(&self.home, began + 300, &args);
        attempt
            .parse()
            .expect("the store names the attempt it started")
    }

    /// The row of the merge that an easy merge leaves behind. The rules read the
    /// newest row of the record and ask whether it started after the row of the easy
    /// merge, so the record of a host whose merge went out holds a cycle of its own
    /// started after every other row. The person ticks the box of the retry on a
    /// row of its own, because the branch was merged and that is the word the person
    /// ticks; the cycle of the merge runs on the tick and ends as the merge, which is
    /// what carries the push on the record (#1751). The row of the merge then sits
    /// behind the row of the easy merge, and the approval the poll's refresh is owed
    /// goes on the record last: unused, newest, and the row the rules read as the
    /// approval the refresh of the poll rests on.
    ///
    /// `pushed` is not the whole record: the poll of #7 also holds the approval the
    /// refresh it is asked about, unused until the start the poll makes, which is what
    /// `owed` writes last of all.
    fn pushed(&mut self) -> &mut World {
        let n = ISSUE.to_string();
        let sha = body_sha256(&json!({"body": WORDS}));
        // The tick of the person who merged the branch goes on the record first, and
        // the cycle of the merge runs on it: `cycle-start` runs on the newest unused
        // approval, so a tick written after the row the merge's cycle ran on would be
        // taken by no start at all -- and a used row of no cycle is a start the record
        // cannot place, whose base it then answers as none, which is the record the
        // poll drops. The cycle is placed at `MERGE_AT`, after the row of the easy
        // merge, so the record says the merge is the start that ran last and
        // `approval-current` answers the tick as the base the poll's refresh rests on.
        // The row of the refresh the poll is owed goes on last of all, at `owed`.
        let at_s = self.next_at().to_string();
        run_at(
            &self.home,
            MERGE_AT,
            &[
                "approval-record",
                &n,
                "--at",
                &at_s,
                "--by",
                "alice",
                "--how",
                "retry-tick",
                "--body-sha",
                &sha,
                "--comment-id",
                "1",
            ],
        );
        let attempt = {
            let _lock = hold(&self.home, ISSUE);
            let v: Value =
                serde_json::from_str(&run_at(&self.home, MERGE_AT + 1, &["cycle-start", &n]))
                    .unwrap();
            v["attempt"].as_i64().unwrap()
        };
        let attempt = attempt.to_string();
        run_at(
            &self.home,
            MERGE_AT + 300,
            &["cycle-end", &n, "--attempt", &attempt, "--status", "merged"],
        );
        // The record of a host whose merge went out: the cycle of the merge is the
        // last start, so `approval-current` answers the tick of the merge as the base
        // of the refresh the poll is owed. The tick is a fresh approval of the person,
        // owed its own easy merge -- the easy merge of #1031 is one per approval -- and
        // the rules read the mark of the merge that ran off the rows of the history.
        self.merged = true;
        self.had_easy = false;
        self.ticked = true;
        self.owed();
        self
    }

    /// The next row of the record: after every row the world has written. Two rows of
    /// one world never share a time, which is what makes `at` alone say which approval
    /// a start takes.
    fn next_at(&mut self) -> i64 {
        self.clock += 1;
        self.clock
    }

    /// GitHub's `statusCheckRollup` for the pull request, as the PULL query
    /// answers it: the rollup of the node GitHub gives for the pull request.
    fn checks(&mut self, runs: Vec<Value>) -> &mut World {
        self.rollup = json!({"nodes": runs});
        self
    }

    /// Whether the instance names the branch's check.
    fn named(&mut self, yes: bool) -> &mut World {
        self.check = yes;
        self
    }

    /// The rows of `history-show`, as `read::store::history` shapes them.
    fn rows(&self) -> Vec<Value> {
        history_of(&self.home)
    }

    /// What the poll decided, given the store's own history and this host's
    /// rollup. The record of the approval the pull request rests on, which the rules
    /// of the refresh read for the kind and the count of the refreshes, is the
    /// record the store holds when the poll asks.
    fn decide(&mut self) -> Decision {
        self.history = self.rows();
        // The count the store makes on the base, asked of the same store while the
        // approval of the poll's refresh still stands unused: the rules read it off
        // the approval and hold the refresh at the limit, so a record that ran the
        // refreshes it ran carries its own count and no case has to name one.
        let mut snap = snapshot(&self.history, pull_answer(&self.rollup));
        // The rules read the approval the pull request rests on, the one the
        // dispatcher wrote when it decided the refresh: the latest of the record,
        // held unused until the start. The world of a case leaves one unused by
        // writing the starts on the approvals of the labels they rest on.
        snap["issues"][ISSUE.to_string()]["approval"] = approval_of(&self.home);
        let d = decide(
            &Snapshot::parse(&snap),
            &Settings::parse(&settings(self.check)),
            true,
        );
        Decision { log: d.log }
    }
}

/// What the poll said about one refresh: which kind it chose, and the line it
/// said. `why` is the words of a full refresh, none when it found nothing to
/// say.
struct Decision {
    log: Vec<String>,
}

impl Decision {
    /// Whether the poll picked a start for #7 at all: the line it says when it
    /// does names the start and the person whose approval it runs on.
    fn started(&self) -> bool {
        self.kind().is_some()
    }

    /// The kind the poll chose, read off the line it said: `easy` for an easy
    /// merge, `refresh` for the full one, none for a start it did not pick. The
    /// line is what a host's log carries, so a case asks the same question a
    /// person reading it would.
    fn kind(&self) -> Option<String> {
        self.log
            .iter()
            .find_map(|l| l.strip_prefix(&format!("would run a cycle for #{ISSUE} (")))
            .and_then(|r| r.split(" by ").next())
            .map(str::to_string)
    }

    /// True when the poll picked a start and said the line of an easy merge.
    fn easy(&self) -> bool {
        self.kind().as_deref() == Some("easy")
    }

    fn why(&self) -> Option<String> {
        self.log
            .iter()
            .find(|l| l.contains("its refresh is the full one"))
            .map(|l| {
                l.split(": its refresh is the full one: ")
                    .last()
                    .unwrap_or(l)
                    .to_string()
            })
    }
}

/// The calls of a case: the store of this build, and the home the case's own,
/// which is what `TEHAS_HOME` is set to for the run. A `Calls` of `idle` takes
/// the home of the environment it was made in, so `home` is named here and not
/// left to whatever the harness's process happens to carry.
fn calls_of(home: &Path) -> Calls {
    let mut calls = Calls::idle(
        Duration::from_secs(20),
        store_binary().to_string_lossy().into_owned(),
    );
    calls.home = Some(home.to_string_lossy().into_owned());
    calls
}

/// The history of the issue as the poll reads it: the store's `history-show`
/// answer, through `read::store::history`, the one place the poll shapes them.
fn history_of(home: &Path) -> Vec<Value> {
    let mut calls = calls_of(home);
    // The read the poll asks, and the same call made through the store program
    // directly, so a failure says what the store itself said rather than the
    // one word `read::store::history` leaves behind.
    let why = Command::new(store_binary())
        .args(["history-show", &ISSUE.to_string(), "--kind", "cycle"])
        .env("TEHAS_HOME", home)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let rows = store::history(&mut calls, ISSUE);
    assert!(
        rows.is_array(),
        "the store's history failed: rc {:?} out {} err {}",
        why.status.code(),
        String::from_utf8_lossy(&why.stdout),
        String::from_utf8_lossy(&why.stderr)
    );
    rows.as_array().unwrap().clone()
}

/// The approval the rules of the refresh read, taken off the same store: what
/// `approval-current` answers for the issue, in the shape the poll's own read
/// gives it, so the record in the snapshot is the record a host's read answers
/// and carries the store's own `easy` mark. The rules hold back a refresh that
/// rests on an approval the record has already used, so the world of a case keeps
/// the latest approval unused, as the record of a host with a refresh to do does.
/// With no approval on record: none, which is what the rules read as nothing to
/// refresh.
/// The approval the rules of the refresh read: the record of the latest start, in
/// the shape the poll's own read gives it, so the record in the snapshot is the
/// record a host's read answers, with the store's own `easy` for the approval: the
/// one easy merge it has had. A world with no easy merge on record has no such start
/// and the rules read the latest of the record, which is the answer of a host whose
/// approval has never had one.
fn approval_of(home: &Path) -> Value {
    // The rules of the refresh read the record of the approval the pull request rests
    // on, in the shape the poll's own read of the store gives it: the approval of the
    // refresh, the person's base it rests on, and the count of the starts on that
    // base. One approval is one start, so the record of a host with a refresh to do
    // holds the approval of the poll's refresh as its newest unused row -- `approval`
    // of the record's own answer, which is the row a host's read gives at the moment
    // the poll decides. The base and the counts come from the same answer, and the
    // rows of the starts that ran keep what they were after the keeping deleted them,
    // which is what carries the base of a record whose easy merge has gone (#1751).
    let mut calls = calls_of(home);
    let answer = calls
        .store(&["approval-current", &ISSUE.to_string()])
        .expect("the approval of #7");
    let approval = answer
        .get("approval")
        .filter(|a| a.is_object())
        .expect("the record holds the approval of the refresh the poll starts: {answer}");
    assert_eq!(
        approval["how"],
        json!("refresh"),
        "the unused approval is the refresh the pull request rests on: {answer}"
    );
    let shaped = store::approval_shape(&answer)
        .expect("the shaped approval of #7")
        .as_object()
        .expect("a shaped object")
        .clone();
    // The record of every start the world writes names the base the poll's refresh
    // rests on. A used approval that belongs to no cycle answers none, and a poll that
    // reads no base drops the request of the refresh and the request does not come
    // back, so a world that wrote such a row would decide nothing and its case would
    // read as a refresh no host ever gets (#1751).
    assert!(
        answer["rests_on"].is_object(),
        "the record names the base the poll's refresh rests on: {answer}"
    );
    // The mark of the easy merge the store answers for the base the pull request rests
    // on, as the read gives it: `approval-current` answers whether that base has had
    // its one easy merge, and the rules hold the second one back on it (#1751).
    shaped
        .into_iter()
        .chain([("easy".to_string(), answer["easy"].clone())])
        .collect()
}

/// GitHub's answer to the PULL query for pull request 50, as `read::github::pull`
/// hands it over: the node under `/data/repository/pullRequest`.
fn pull_answer(rollup: &Value) -> Value {
    let mut node = json!({
        "state": "OPEN", "number": PR,
        "url": format!("https://github.com/{REPO}/pull/{PR}"),
        "isDraft": false, "isCrossRepository": false,
        "headRefOid": HEAD, "baseRefName": "main", "baseRefOid": BASE,
        // The rules of the refresh start only a pull request that conflicts: the
        // easy merge of the world merged and the branch moved under it, which is the
        // state of every case of this file.
        "mergeable": "CONFLICTING",
        "author": {"login": BOT}, "headRepository": {"nameWithOwner": REPO},
        "labels": {"nodes": []},
        "closingIssuesReferences": {"totalCount": 1, "nodes": [{
            "number": ISSUE, "state": "OPEN",
            "labels": {"nodes": [{"name": "tehas:awaiting-merge"}]},
            "closedByPullRequestsReferences": {"nodes": [{"number": PR, "state": "OPEN"}]}}]},
    });
    if !rollup.is_null() {
        node["statusCheckRollup"] = rollup.clone();
    }
    node
}

/// One run of the `CheckRun` of `name`, and one of a `StatusContext`.
fn check_run(name: &str, conclusion: &str) -> Value {
    json!({"__typename": "CheckRun", "name": name, "conclusion": conclusion})
}

fn status_context(context: &str, state: &str) -> Value {
    json!({"__typename": "StatusContext", "context": context, "state": state})
}

/// The settings of a host that refreshes and may choose easy merges.
fn settings(check: bool) -> Value {
    let mut s = json!({
        "TEHAS_REPO": REPO, "TEHAS_CONTROL_ISSUE": 1,
        "TEHAS_ALLOWED_LABELERS": "alice", "TEHAS_TICKET_CONCURRENCY": 2,
        "TEHAS_BOT_LOGIN": BOT, "TEHAS_REFRESH_CONFLICTS": "1",
        "TEHAS_EASY_MERGE": "1",
    });
    if check {
        s["TEHAS_BRANCH_CHECK"] = json!(CHECK);
    }
    s
}

/// The snapshot of #7 waiting for a merge of pull request 50, with the rows the
/// store answered, the node GitHub did, and the refreshes the store counts on
/// the approval the pull request rests on.
fn snapshot(history: &[Value], pull: Value) -> Value {
    json!({
        "repo": REPO,
        "control": {"labels": [], "events": []},
        "plan_open": [], "running": [], "ready": [], "blocked": [],
        "awaiting": [{"number": ISSUE}],
        "limit": {"state": "none"},
        "slice": {"show": {"FragmentPath": "/etc/systemd/system/tehas.slice",
                           "ActiveState": "active", "MemoryMax": "21474836480"},
                  "docker": "systemd 2", "controllers": "cpuset cpu io memory pids",
                  "cpus": "8", "root": "/sys/fs/cgroup", "scopes": {},
                  "pressure": "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n"},
        "endpoint": {}, "disk_kb": 41_943_040, "now": 1_793_404_800,
        "hostname": "tehas-test", "bot_prs": [], "marked": [], "planning": [],
        "refresh_history": [ISSUE], "refresh_requests": [ISSUE], "refreshing": [],
        "leftovers": [], "asking": [], "owed": [], "plans": [], "refines": [],
        "refines_owed": [],
        "continues": true,
        "pulls": {PR.to_string(): pull},
        "issues": {
            "7": {
                "events": [],
                "request": {"at": "2026-10-17T09:00:00Z", "pr": PR,
                            "head_sha": HEAD, "base_sha": BASE},
                "work": {"number": ISSUE, "state": "open",
                         "labels": [{"name": "tehas:awaiting-merge"}]},
                "edit": {"lastEditedAt": null, "editor": null, "body": WORDS},
                "comments": {"data": {"repository": {"issue": {"comments": {"nodes": []}}}}},
                "history": history,
            }
        },
    })
}

/// The store's own row of the last refresh: the one an easy merge ends, told by
/// its `easy` mark and nothing else.
fn last_refresh(rows: &[Value]) -> Value {
    rows.iter()
        .rev()
        .find(|r| r.get("how").and_then(Value::as_str) == Some("refresh"))
        .unwrap_or_else(|| panic!("the store answered no refresh row: {rows:?}"))
        .clone()
}

#[test]
fn the_store_answers_the_easy_mark_of_the_refresh_that_ran_as_one() {
    // The half of the fix that is the store's: the end keeps no mark of its
    // own, so `history-show` reads it off the approval of its start.
    let mut w = World::new("mark");
    w.approved().refreshes(2, true, false, "failed");
    let rows = w.rows();
    assert_eq!(rows.len(), 3, "one row per end: {rows:?}");
    assert_eq!(rows[0]["easy"], false, "alice's cycle: {}", rows[0]);
    assert_eq!(
        rows[1]["easy"], false,
        "a refresh that ran in full: {}",
        rows[1]
    );
    assert_eq!(
        rows[2]["easy"], true,
        "the refresh that ran as one: {}",
        rows[2]
    );
    assert_eq!(last_refresh(&rows)["easy"], true);
}

#[test]
fn a_pushed_easy_merge_and_a_conflicting_pull_request_are_refreshed_easy_again() {
    // The trigger of #1751, on the answers a host gives: the easy merge pushed,
    // the pull request conflicts again with a green check, and the refresh is
    // the easy one again.
    let mut w = World::new("again");
    w.approved().refreshes(1, true, false, "merged");
    let rows = w.rows();
    assert_eq!(
        last_refresh(&rows)["easy"],
        true,
        "the store says which start was the easy merge"
    );
    w.pushed();
    let d = w.checks(vec![check_run(CHECK, "SUCCESS")]).decide();
    assert!(d.started(), "the refresh started: {:?}", d.log);
    assert!(d.easy(), "refreshed easy again, said: {:?}", d.log);
    assert_eq!(d.why(), None, "a full refresh says why; this one did not");
}

#[test]
fn a_red_named_check_after_the_easy_merge_is_the_full_refresh() {
    let mut w = World::new("red");
    w.approved().refreshes(1, true, false, "merged").pushed();
    let d = w
        .checks(vec![
            check_run(OTHER, "SUCCESS"),
            check_run(CHECK, "FAILURE"),
        ])
        .decide();
    assert!(d.started(), "the refresh started: {:?}", d.log);
    assert!(
        !d.easy(),
        "the named check went red, so the refresh is full"
    );
    assert_eq!(
        d.why().as_deref(),
        Some("the check on its pull request failed after the last easy merge")
    );
}

#[test]
fn a_red_commit_status_of_the_named_context_is_the_full_refresh() {
    let mut w = World::new("status");
    w.approved().refreshes(1, true, false, "merged").pushed();
    let d = w.checks(vec![status_context(CHECK, "FAILURE")]).decide();
    assert!(
        !d.easy(),
        "a commit status of the named context says it too"
    );
}

#[test]
fn a_red_run_of_another_name_is_no_answer_and_the_refresh_stays_easy() {
    let mut w = World::new("other");
    w.approved().refreshes(1, true, false, "merged").pushed();
    let d = w.checks(vec![check_run(OTHER, "FAILURE")]).decide();
    assert!(
        d.easy(),
        "only the check the instance named gates it: {:?}",
        d.log
    );
}

#[test]
fn with_no_check_named_a_red_run_holds_nothing_back() {
    let mut w = World::new("unnamed");
    w.approved().refreshes(1, true, false, "merged").pushed();
    let d = w
        .checks(vec![check_run(CHECK, "FAILURE")])
        .named(false)
        .decide();
    assert!(
        d.easy(),
        "with none named only a new conflict decides: {:?}",
        d.log
    );
}

#[test]
fn an_easy_merge_that_pushed_nothing_is_the_full_refresh() {
    // No cycle after the merge's row: the issue still stands where the merge
    // left it, and the full refresh is the safe kind.
    let mut w = World::new("no-push");
    w.approved().refreshes(1, true, false, "failed");
    let d = w.checks(vec![check_run(CHECK, "SUCCESS")]).decide();
    assert!(d.started(), "the refresh started: {:?}", d.log);
    assert!(!d.easy());
    assert_eq!(
        d.why().as_deref(),
        Some("the last easy merge pushed nothing")
    );
}

#[test]
fn an_approval_that_has_had_no_easy_merge_is_owed_one() {
    // The rows of a host that never chose one, or of a store written before the
    // mark existed: none says easy, and `easy` of the approval says none was had.
    let mut w = World::new("none");
    w.approved().refreshes(1, false, false, "failed");
    let rows = w.rows();
    assert!(
        rows.iter().all(|r| r.get("easy") != Some(&json!(true))),
        "no refresh of this host's record ran as an easy merge: {rows:?}"
    );
    let d = w.checks(vec![check_run(CHECK, "SUCCESS")]).decide();
    assert!(d.started(), "the refresh started: {:?}", d.log);
    assert!(
        d.easy(),
        "the one easy merge this approval is owed: {:?}",
        d.log
    );
}

#[test]
fn a_parked_refresh_is_held_and_starts_nothing() {
    // A refresh that parked is not offered again, whatever its mark: the row's
    // `easy` does not buy a start past the park. The person ticks the box again and
    // the refresh runs on that tick, so the approval the pull request rests on and
    // the row of the park are one row of the record, as they are on a host.
    let mut w = World::new("parked");
    // A parked refresh is the row of the tick the person gave for it: the approval the
    // pull request rests on and the park the poll holds the refresh on are one row of
    // the record, as they are on a host. `tick` writes that row and leaves the record
    // holding the approval of the refresh the poll is owed.
    w.approved().tick("failed", true, true);
    let d = w.checks(vec![check_run(CHECK, "SUCCESS")]).decide();
    assert!(!d.started(), "a parked refresh is held: {:?}", d.log);
    assert!(!d.easy());
}
