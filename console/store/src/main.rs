//! The state store: `$TEHAS_HOME/state/tehas.db`, the host's own records as
//! rows rather than as about 35 kinds of file, and the only writer of that
//! database. `docs/specs/2026-10-21-state-store.md` is the design; section 3
//! is the schema (src/schema.sql) and section 4 this command.
//!
//!   store init [--force] [--version <v>] [--commit <sha>] [--instance <name>]
//!                [--repo <owner/name>] [--json]
//!     Makes the database: schema v2 in an empty file and PRAGMA
//!     user_version 2, and the row of meta that says when it was made, by
//!     which version and commit of tehas, and for which instance and
//!     repository. The four are required: the command that runs the store has
//!     loaded them, and the store refuses a call that arrived without one (64)
//!     rather than guessing a default. Without --force it refuses a file that
//!     already exists and changes nothing; with --force it replaces a file
//!     that holds nothing and refuses one that holds tables, because a
//!     database with rows in it is not the store's to discard (ADR 0003).
//!     Answers
//!     {"api":1,"verb":"init","ok":true,"schema":2,"created":true}, or
//!     "created":false with --force over a file the store then filled.
//!   store migrate --backup <dir> [--json]
//!     Backs the database up and then brings it forward, in that order: a
//!     consistent copy of the store as it stands is written to
//!     <dir>/tehas-<to>-<time>.db with VACUUM INTO before a byte of the store is
//!     changed, and a backup that cannot be made migrates nothing. The copy the
//!     answer names is that one, so no step runs without the copy a caller is
//!     told to open already being on disk; a step that runs also leaves a second
//!     copy under the version the file came with. From 0, an SQLite file with no
//!     tables and user_version 0 (what an empty file is), the schema, the rows of
//!     meta and the version are written in one transaction. From 1, the step to 2
//!     adds columns and a table, also in one transaction, and keeps every row. A
//!     database init made is already at 2, and says "from":2, "to":2 and still
//!     writes its backup, the one copy of the two. <dir> is the
//!     state/ of this instance's home, or state/backup/ below it, which the verb
//!     makes 0700 when it is not there; the copy takes the 0700 of the directory
//!     and the 0600 of the file. Another directory is refused, because the store
//!     was not given leave to write a copy of itself in a person's.
//!     Answers
//!     {"api":1,"verb":"migrate","ok":true,"from":1,"to":2,
//!     "backup":"..."}.
//!   store check [--json]
//!     Reads only: the schema version, PRAGMA integrity_check, and the rows of
//!     every table. Answers {"api":1,"verb":"check","ok":true,"schema":2,
//!     "integrity":"ok","rows":{...}}. A database that fails its integrity
//!     check is one the store cannot use: 78 `corrupt`, with what failed and
//!     the file. It is never moved aside.
//!
//!   store import [--dry-run] [--json]
//!     The records an older tehas kept as files under state/ (the cycle's,
//!     the park's and the planning session's), made rows, and each file moved
//!     to state/imported-<YYYYMMDD>/ once its rows read back (src/import.rs).
//!     $TEHAS_HOME/.tehas-home must be there, else 78 `unmarked`. A cycle or
//!     planning lock held while there is anything to import is 1 `live`; a
//!     file that is a link, over 1 MiB or does not parse stays where it is,
//!     with its issue's other files, and the verb exits 1 `held` with one
//!     line per file. --dry-run reads the files only. Answers
//!     {"api":1,"verb":"import","ok":true,"imported":{<table>:N},"moved":N,
//!     "held":[],"dry_run":false}.
//!
//!   --say on init, migrate and import prints the lines a person reads in
//!   place of the answer: `store: made <db>`, `store: <db> at schema <n>,
//!   backup <file>`, and `store: imported ...; moved <n> files to <dir>/` when
//!   an import moved any. `import --dry-run --say` is install.sh's gate before
//!   its first copy: silent when the install may go on, else install.sh's own
//!   refusal lines and exit 1 (src/say.rs).
//!
//! This part of the store is the schema, the two housekeeping verbs, migrate,
//! import, the record verbs of cycles, parks, questions, approvals and
//! planning, and the history (src/verbs.rs and the files it names):
//!
//!   store cycle-start <issue> [--rukas <commit>] [--by-hand]
//!   store cycle-image <issue> --attempt N --image <id> [--rukas <commit>] [--settings <file>]
//!   store cycle-end <issue> --attempt N --status <word> [...] | --refused | --stopped
//!   store cycle-show <issue> | --running
//!   store settings-view <issue> [--attempt N]
//!   store cycle-note <issue> --kind quiet --said-for <key>
//!   store cycle-stop-asked <issue> [--said-only]
//!   store cycle-heartbeat <issue> [--attempt N] --running|--ended --status <word>
//!                [--step <word>] [--text <notice>]
//!   store publish [--dispatch <file>] [--health <file>]
//!   store order-show | order-set --issues N[,N...] [--only] [--until <unix>]
//!                [--reason TEXT] | order-clear
//!   store park <issue> --attempt N --reason <w> [--step <w>] [--pr N]
//!                [--comment-id <id> --body-sha <sha256>]
//!   store park-show <issue> | --automatic | --refine
//!   store park-retry <issue>
//!   store park-told <issue>
//!   store question-asked <issue> --id <id> --kind park|plan [--attempt N] [--body-sha <sha256>]
//!   store question-known <issue> --id <id>
//!   store question-answered <issue> --id <id> --by <account> | --withdraw
//!   store approval-record <issue> --at <unix> --by <account> --how <how> ...
//!                [--input-file <file>] | --undo --at <unix>
//!                (the hows are label, refresh, retry-tick, outage-retry and
//!                millwright; the old `fixer` is taken too, and is written as
//!                `millwright` until the shell stops sending it)
//!   store approval-current <issue>
//!   store approval-view <issue> [--attempt N]
//!   store plan-begin <issue> --by <account> --label <label> --brief-file <file> [--auto]
//!                | --undo
//!   store plan-answer <issue> --turn N --by <account> [--millwright] [--text-file <file>]
//!                | --turn N --undo
//!   store plan-end <issue> --turn N --outcome <o> [...] | --posted | --close
//!   store plan-note <issue> --kind millwright-refused|held --id <id>
//!                (the old `fixer-refused` is taken too, and is written as
//!                `millwright-refused`)
//!   store plan-show <issue> | --open
//!   store plan-view <issue> [--open]
//!   store history-show [<issue>] [--kind cycle|plan] [--how <how>]
//!   store backup [--show]
//!   store copy --to <file>
//!                (a copy of the store at one moment, by VACUUM INTO on a
//!                reader's connection; answers its schema, rows and ms)
//!   store load --from <file>
//!                (a copy loaded into a store that holds no history, in one
//!                transaction; answers the copy's schema, rows and ms:
//!                src/load.rs)
//!   store prune --days N
//!                (endings, settled actions with their journal, the heartbeats
//!                of ended cycles, and the history of issues ended more than N
//!                days ago, each issue keeping its last row of each history. A
//!                cycle's row is never removed: src/upkeep.rs)
//!   store sweep-locks [--days N] [--max N] [--dry-run]
//!   store locks-held
//!   store upkeep [--dry-run]
//!   store poll-record --started <unix[.frac]> --cpu-ms <n> --gh <n>
//!                --writes-log <file> [--starts <n>]
//!   store poll-show | poll-show --api --now <unix>
//!   store poll-idle --since <seconds> [--budget <spec>]
//!                (what each poll cost, and the cost of the idle ones: src/poll.rs)
//!   store poll-begin
//!   store review-issue <path> <branch> <commit>
//!                (the review issue's title and body, masked, and the count of
//!                its findings, from the document of one review run:
//!                tehas_core::review, src/review_issue.rs)
//!   store review-file --repo <owner/name> --doc <path> --branch <b> --commit <sha>
//!                [--ask <line>] [--private <name>]... [--gh <program>]
//!                (the filing of one run: the render above and the GitHub
//!                calls that go with it, src/review_issue.rs)
//!   store journal-record --poll <at> --kind <kind> --issue <n> --args <json>
//!                --before <at>
//!   store journal-attempt --action N
//!   store journal-settle --action N (--result <result> | --abandoned)
//!                [--evidence <json>]
//!   store journal-open
//!                (the action journal: src/journal.rs)
//!   store batch  (reads a JSON array of calls on standard input, one element
//!                per call, each an array of arguments -- ["plan-show","12"] --
//!                and answers a JSON array in the same order, one
//!                {"rc": <exit>, "out": <what the verb prints>} per call. It
//!                opens the database once, in one transaction, so the calls
//!                read one snapshot. It answers the four reads a poll asks of
//!                each issue -- plan-show, park-show, question-known and
//!                approval-current; any other verb in the list gets 64 and
//!                nothing is written, and none of the four writes. The calls
//!                are given on standard input because one argument would be
//!                shell data: the poll of read/store.rs is its only caller
//!                (src/batch.rs).
//!
//! Each answers one line, {"api":1,"verb":"<verb>","ok":true,...}. A record
//! verb that refuses says `store: usage <verb>: ...` (64) or
//! `store: <word> #<issue>: ...` (1). The rest of the 13 groups and
//! state-audit are later parts of epic #244; an argument this command does
//! not know is exit 64. A build with debug assertions also has `test-write`,
//! the writer its own tests run (src/exercise.rs), and reads its clock from
//! TEHAS_STORE_TEST_NOW when that is set; a release build has neither. The same
//! build names every wait on a lock it survived, one line to standard error for
//! each -- `store debug: busy retry <try>` -- when TEHAS_STORE_TEST_BUSY_REPORT
//! holds how many attempts of one wait to name: tests/concurrency.rs counts
//! those lines to tell a wait on the store apart from a slow machine. No value,
//! or 0, prints nothing, which is what every caller of the store gives it, and a
//! release build never reads the variable. A value that is no count is 64.
//! The busy
//! and lock bounds are not a setting at all: no variable and no instance setting
//! reaches them, and the waits are the constants of `src/db.rs` and `src/live.rs`
//! until the unit tests shorten them for their own thread. A debug build also
//! answers `--test-store-bound-check`, which is how they prove it.
//!
//! The database is $TEHAS_HOME/state/tehas.db, mode 0600, made so before
//! SQLite opens it, in WAL mode with synchronous=FULL and foreign_keys ON on
//! every connection. A connection that writes waits up to 5000 ms for a lock,
//! asking again every millisecond; one that reads waits up to a second. Those
//! are the bounds of the program a host runs. Its directory, state/, is kept
//! mode 0700: with WAL on, a readable state/ is a readable copy of the store.
//! The path is refused when it is a link or sits under live/ or state/index;
//! no container sees the file (AGENTS.md rule 4). The store never moves a
//! damaged file aside: the four classes of failure stop the command with 78
//! and say which one they are, as ADR 0003 decides.
//!
//! A cycle or a planning turn runs from its own copy of the code, made when it
//! started (tehas-cycle run), and calls this store, the installed one, after an
//! update. So a verb, an argument or a field of an answer that a released
//! version's scripts use stays, with its meaning, for at least one release
//! after the release that stops using it, and a migration only adds.
//!
//! The values meta records about the installation are the caller's: a command
//! that runs the store has itself loaded the instance settings and passes what
//! the verb names. The store does not read the settings of a host itself, so
//! the rules of what a valid setting is stay in one place, lib/config.sh.
//!
//! Exit: 0 the verb did its work; 64 an argument this verb does not know, or
//! settings that did not arrive in the shape the verb states, and nothing was
//! written; 78 the database: `missing`, `corrupt`, `unreadable` or `future`,
//! on standard error as the first word of the line, and nothing was written;
//! 1 a call failed or a precondition was not met, or the database was locked
//! past the busy timeout, which says `busy`.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let (args, say) = store::strip_say(&std::env::args().skip(1).collect::<Vec<_>>());
    // The one setting of this build that is read from the environment at all,
    // and the only thing it buys is the line of a wait on a lock. It is judged
    // before anything is run -- the bound check included, which is the very
    // argument the test of the notice uses -- so a caller that asked for the
    // count badly is told at once rather than left reading the silence of a call
    // that waited for nothing: a value of `x` and a value of 0 would otherwise
    // answer the same. A release build compiles this away, so the store a host
    // runs neither reads the variable nor prints the notice.
    #[cfg(debug_assertions)]
    if let Some(bad) = bad_busy_report() {
        eprintln!("store: {TEST_BUSY_REPORT} is not a count: {bad}");
        return ExitCode::from(64);
    }
    #[cfg(debug_assertions)]
    if args.first().is_some_and(|a| a == store::BOUND_CHECK) {
        let Some(file) = args.get(1) else {
            eprintln!("usage: store {} <file>", store::BOUND_CHECK);
            return ExitCode::from(64);
        };
        // The one caller that does not ask for the short pair, so it waits what
        // the constants say. `tests::a_store_the_tests_did_not_shorten_waits_the_installed_bounds`
        // runs a store this way against a lock it holds, to show the installed
        // bounds are still the installed bounds.
        return match store::bound_check(std::path::Path::new(file)) {
            Ok(line) => {
                tehas_core::outln!("{line}");
                ExitCode::SUCCESS
            }
            Err((code, why)) => {
                eprintln!("store: {why}");
                ExitCode::from(code)
            }
        };
    }
    let home = std::env::var_os("TEHAS_HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if say {
        let said = store::said(&home, &args);
        for line in &said.out {
            tehas_core::outln!("{line}");
        }
        for line in &said.err {
            eprintln!("{line}");
        }
        return ExitCode::from(said.code);
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match store::call(&home, &args, &mut std::io::stdin()) {
        Ok(line) => {
            tehas_core::outln!("{line}");
            ExitCode::SUCCESS
        }
        Err((code, line)) => {
            eprintln!("{line}");
            ExitCode::from(code)
        }
    }
}

/// The name of the variable of the busy notice, spelled once in `src/db.rs`.
#[cfg(debug_assertions)]
use store::TEST_BUSY_REPORT;

/// What the variable of the busy notice holds, when it holds anything at all
/// that is neither a count nor empty: the text of the 64 line. An unset variable
/// is no value and no line, which is what every caller of the store gives it; so
/// is 0, which asks for no line of any wait.
#[cfg(debug_assertions)]
fn bad_busy_report() -> Option<String> {
    let given = std::env::var_os(TEST_BUSY_REPORT)?;
    let text = given.to_str().map(str::to_string).unwrap_or_default();
    if text.is_empty() || tehas_core::grammar::count(&text).is_some() {
        return None;
    }
    Some(text)
}
