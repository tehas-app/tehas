# Verb contract: cycle, park, question, approval and planning records (#244 unit for #250, #251)

Sources read: spec `docs/specs/2026-10-21-state-store.md` sections 2, 3.1-3.8,
3.13, 3.14, 4, 5, 7, 9; `console/store/src/{main.rs,db.rs,schema.sql,tests.rs}`;
issues #250, #251; `libexec/{dispatch,cycle,work,plan,stop,clean,history}`,
`lib/{cycles.sh,planning.sh,improve.sh,fixer.sh,refresh.sh,live.sh,headless.sh,attachments.sh}`.

This is the final contract after three reviews (spec fidelity, caller fit,
failure modes). Section 8 lists what was rejected and why.

No caller changes in this unit. Every verb below is built so that a later PR
can replace the named bash write with one call and keep behaviour. Where the
design needs the spec to say something else, the same PR changes
`docs/specs/2026-10-21-state-store.md` (section 7 lists each change) and the
pull request asks the owner to accept the ones marked **sign-off**.

## 0. Layout and shared rules

### Files

New code goes in new files, so the #595 change to `src/tests.rs` merges cleanly:

| File | Holds |
| --- | --- |
| `src/verbs.rs` | argument parsing, value grammars and cleaning, the output line, the store open for data verbs, the issues stub, the error mapping |
| `src/lock.rs` | the cycle lock probe |
| `src/cycle.rs` | `cycle-start`, `cycle-image`, `cycle-end`, `cycle-show`, `cycle-note`, `cycle-stop-asked` |
| `src/park.rs` | `park`, `park-show`, `park-retry`, `question-asked`, `question-known`, `question-answered` |
| `src/approval.rs` | `approval-record`, `approval-current` |
| `src/plan.rs` | `plan-begin`, `plan-answer`, `plan-end`, `plan-show` |
| `src/verbs_tests.rs`, `src/lock_tests.rs`, `src/cycle_tests.rs`, `src/park_tests.rs`, `src/approval_tests.rs`, `src/plan_tests.rs` | the tests, `#[cfg(test)] mod` from main.rs |

`main.rs` changes only: the `mod` lines, one `Verb::Record(verbs::Call)`
variant, `parse` falls through to `verbs::parse(args)` for a verb name it does
not know, `execute` routes `Verb::Record` to `verbs::run(home, call, now)`, the
USAGE line and the header comment's verb list.

`db.rs` gets one additive change: `pub fn open_look(file)`, the same as
`open_read_only` but with a busy timeout of 1000 ms (a new `Purpose::Look`
arm in `prepare`). See "Opening the store". Nothing else in db.rs changes.

`tests.rs` is not changed. Its `an_unknown_verb_or_argument_is_64` keeps
passing because unknown names still end in `None`, and its schema test still
holds after the schema changes below (it inserts `how = 'because'`, still
refused, and `started_by = 'a'`, still allowed).

`Cargo.toml`: `rust-version = "1.89"` (was 1.80), because the probe uses
`std::fs::File::try_lock` (stable in 1.89; the host pin is 1.99). No new crate.
No `unsafe`, no `libc`.

`schema.sql`: still `user_version` 1, because no host has a `tehas.db` yet
(`install.sh` does not run the store). Changes, each mirrored in spec section 3
in the same PR (section 7):

- `cycles.how` CHECK gains `'hand'`; `cycles.started_by` loses NOT NULL (D12).
- `cycle_history` gains `started_at INTEGER, rc INTEGER, cap TEXT, park_reason TEXT, failure_kind TEXT, failure_cause TEXT, provider INTEGER NOT NULL DEFAULT 0, parked INTEGER NOT NULL DEFAULT 0` (D1).
- `plan_history` gains `started_at INTEGER, rc INTEGER, by_account TEXT, label TEXT, filed_json TEXT` (D1).
- `planning_filed` gains `url TEXT NOT NULL`, and its key becomes `(issue, url)` (D23).
- Comments: `cycle_progress.written_at` is the one `*_at` taken from a file's
  mtime (D25); an ended `cycles` row with `end_reason` NULL is a start the cycle
  refused (D22); a withdrawn question is a deleted row (D4). The header's
  claim, "every table, column, key and index of the state store spec, section
  3.1 to 3.13", stays true because the spec changes in the same PR.

### Settings

None of these verbs takes an instance setting. They need `TEHAS_HOME` only,
which `main()` already requires (78 when it is not an absolute directory).
Every row is keyed by an issue number inside this instance's own store; nothing
here names the repository, the control issue or the allowed labellers. A12
(control issue) and F1 (allowed labellers) stay in bash (#253, AGENTS.md
rule 5).

### Parsing (every verb)

- Verb name first, then positional `<issue>` where the table shows it, then
  flags. A switch takes no value. A flag that takes a value **consumes the next
  argument as it is**, as `init` does (`main.rs:206-211`): `--step -` is the
  step `-`. The value's grammar decides, not its first character. `--json` is
  accepted anywhere and changes nothing.
- 64, nothing opened, nothing written, when: an unknown flag; a flag given
  twice (except the repeatable `--option` and `--filed`); a required flag
  missing; a value flag at the end with no value; a **strict** value outside its
  grammar; two modes of one verb at once; a flag the chosen mode does not take.
- Every 64 prints one stderr line `store: usage <verb>: <flag> <why>`, and for
  an enumerated flag (`--how`, `--kind`, `--outcome`, `--host-saw`, `--label`)
  the word given, Debug-escaped and cut to 64 bytes (spec 3.4).
- **Strict** values are tehas's own (numbers, accounts, hashes, ids, times,
  enumerations). **Cleaned** values come from a container's output through
  bash; they never cause 64. A cleaned value has the characters outside its
  set removed and is cut to its bound, as the bash that produced it does
  (`libexec/cycle:423-424`, `:786-802`). A value that is empty after cleaning
  is stored as NULL (a status as `unknown`). Every flag whose value was changed
  or dropped is named in the JSON list `"cleaned":["--step",...]` (empty list
  when none). The end of a cycle and the end of a planning turn are therefore
  always committed (spec 3.3).

| Name | Kind | Grammar or set |
| --- | --- | --- |
| issue, pr, turn, attempt | strict | `^[1-9][0-9]{0,9}$` |
| count (`--oom`, `--forks`, `--stalled`) | strict | `^(0\|[1-9][0-9]{0,8})$` |
| exit (`--rc`, `--runner-exit`) | strict | `^(0\|[1-9][0-9]{0,8})$` (the caller accepts up to 999, `libexec/cycle:567`) |
| unix time (`--at`) | strict | `^[1-9][0-9]{0,10}$` |
| comment id | strict | `^[1-9][0-9]{0,15}$`, at most 2^53-1 |
| account | strict | `^[A-Za-z0-9-]{1,39}$` |
| sha256 | strict | `^[0-9a-f]{64}$` |
| commit | strict | `^[0-9a-f]{7,40}$` |
| head sha | strict | `^[0-9a-f]{40}$` |
| image | strict | `^sha256:[0-9a-f]{64}$` |
| approval `--how` | strict | `label`, `refresh`, `retry-tick`, `outage-retry`, `fixer` (spec 3.4) |
| approval `--label` | strict | `^tehas:[a-z][a-z0-9-]{0,39}$` |
| plan `--label` | strict | `tehas:plan`, `tehas:refine` |
| plan `--outcome` | strict | `finished`, `failed`, `interrupted`, `not-started`, `not-readable`, `lost` |
| note `--said-for` | strict | `^[A-Za-z0-9_.:-]{1,64}$` |
| file argument | strict | an absolute path |
| status (`--status`) | cleaned | set `[A-Za-z0-9_.-]`, 40 bytes; empty is `unknown` |
| step (`--step`) | cleaned | set `[A-Za-z0-9_.-]`, 40 bytes |
| cap (`--cap`), park reason (`park --reason`) | cleaned | set `[A-Za-z0-9_.:-]`, 80 bytes (`libexec/cycle:793`); an empty park reason is `unknown` |
| runner park reason (`cycle-end --park-reason`) | cleaned | set `[a-z-]`, 40 bytes (`libexec/cycle:786`) |
| failure kind | cleaned | set `[a-z-]`, 40 bytes (`libexec/cycle:797-798`) |
| failure cause | cleaned | set `[A-Za-z0-9_.:-]`, 60 bytes (`libexec/cycle:801-802`) |
| branch | cleaned | kept only when it matches `tehas_branch_pattern` for the issue (`^feature/issue-<n>(-[A-Za-z0-9._-]{1,80})?$`) or `^[A-Za-z0-9][A-Za-z0-9._/-]{0,79}$` (`libexec/cycle:770-771`); else NULL |
| session | cleaned | kept only when it matches `^[A-Za-z0-9_.:-]{1,128}$`; else NULL |
| option label | cleaned | control characters removed, cut to 300 bytes on a character boundary (`q_clean 300`, `lib/questions.sh:80`); an empty label becomes `-` so a box keeps its place; options past the 20th dropped (`Q_MAX_OPTIONS`, `[0:$max]` at `planning.sh:300`) |
| filed (`--filed`) | cleaned | kept only when it matches `^https://github\.com/[^/]+/[^/]+/issues/[1-9][0-9]{0,9}$` (`planning.sh:111-116`) |

### Opening the store

- **Files first.** Every file argument (`--result`, `--progress`,
  `--settings`, `--brief-file`, `--input-file`, `--text-file`) is read, size-checked and parsed **before** the
  store is opened, so no other writer waits behind a 16 MiB parse. Only the
  comparison with a row (G3) happens inside the transaction. The read:
  `symlink_metadata` must say regular file (a link, directory or device is
  refused), open, `metadata()` of the open file must have the same dev and
  inode, size within the bound, read at most bound + 1 bytes. Bounds: result
  16 MiB, progress 1 MiB, settings 64 KiB (`lib/improve.sh:88`), brief 1 MiB,
  approval inputs 1 MiB, answer text 64 KiB (D30, D34). How a refused file
  ends is stated per verb.
- **Writers:** `db::lock(file)` (BEGIN IMMEDIATE, busy timeout 5 s), every
  read and write of the verb, the issues stub first where the verb has one,
  then COMMIT. On any error, ROLLBACK: nothing is committed, the stub included.
- **Readers** (`cycle-show`, `park-show`, `question-known`,
  `approval-current`, `plan-show`, `history-show`): `db::open_look` (read only, busy timeout
  1 s), write nothing. A bounded wait, not zero: in WAL mode the last
  connection to close takes an exclusive lock to checkpoint and remove the
  `-wal`, and every store call is its own process, so a reader that opens at
  that moment would otherwise get `busy` at once.
- After open, a data verb requires `PRAGMA user_version` = 2, the version this
  binary writes (D29). A file at 0 (made by a `touch`, never migrated) is 78
  with the word `unreadable` and the detail `the store holds no schema; run
  store migrate` (spec section 4's table: the file is there and cannot be used;
  `missing` stays "no file at all"). A file at 1 is 78 `unreadable` with `the
  store is at schema 1, this store writes 2; run store migrate`.
- **Error mapping** (`verbs.rs`, db.rs's `classify` unchanged): `Busy` is exit
  1 `busy`. An SQLite constraint failure (`SQLITE_CONSTRAINT`, any extended
  code) is exit 1 with the word `conflict` and the message SQLite gave, never
  78: it says a row disagreed, not that the store could not be opened. Every
  other `DbError` keeps its word and 78 through `main::line`.

### The issues stub

`cycles`, `approvals`, `parks`, `questions` and `planning` reference
`issues (issue)`, and `foreign_keys` is ON. A writer that may create the first
row for an issue runs, **as the first statement inside its own BEGIN
IMMEDIATE transaction**:

```sql
INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (?1, 'unreadable', 0, 0);
```

An existing `issues` row is never touched. `kind_at = 0` and `checked_at = 0`
say "never read": #253's snapshot refreshes it, and nothing reads a stub as
work (D16, A14). A verb refused after the stub (exit 1, busy, conflict) rolls it
back with everything else.

### Output

One line on stdout: `{"api":1,"verb":"<verb>","ok":true,` then the verb's
keys, then `}`. Keys are serialised with `serde_json`. Booleans are JSON
booleans; SQL NULL is `null`; times are Unix seconds. On failure nothing goes
to stdout and one stderr line starts `store: <word>`.

Row objects used below:

- **cycle**: `{"issue","attempt","started_at","started_by","how","requeued"(bool),"rukas_commit","image","branch","end_reason","ended_at","oom_kills","forks_refused","stalled_seconds","timed_out"(bool),"runner_exit","watched"(bool),"host_saw","stop_asked_at","stop_said_times","settings":{key:value},"quiet":{"said_for","at"}|null}`
- **lock**: `{"state":"free"|"held"|"unknown","probe_rc":0|1|2,"file":bool,"seen_at":<now>}` plus `"why"` when `probe_rc` is 2
- **approval**: `{"issue","at","by","how","label","body_sha256","title_sha256","comment_id","refresh_pr","refresh_head","used_at","used_by","input":{...}|null}`
- **park**: `{"issue","attempt","at","reason","step","pr","retries","started_at","started_by","automatic"(bool),"question":<id>|null,"told_at","body_sha256"}`
- **question**: `{"id","issue","attempt","kind","posted_at","body_sha256","answered_at","answered_by"}`
- **planning**: `{"issue","session","turns","options":[...],"pending"(bool),"pending_outcome","failed"(bool),"running"(bool),"started_at","comment_id","lost"(bool),"by","label","brief","turn_rc","opened_at","ended_at","answers":[{"turn","by","fixer"(bool)}],"filed":[<url>...],"answer":<text>|null,"notes":{"fixer_refused":[<id>...],"held":[<id>...]}}`

### Exit statuses (every verb)

| Exit | When |
| --- | --- |
| 0 | the verb did its work, or there was nothing to do, or a read found nothing (`null` in the JSON) |
| 64 | an argument as above; nothing opened |
| 78 | `TEHAS_HOME` unset; the database `missing`, `corrupt`, `unreadable` (schema 0 included), `future` |
| 1 | `busy`, `conflict`, or a precondition the verb states, with its word first on stderr: `store: <word> #<issue>: <why>`. Nothing committed |

For a reader, exit 1 `busy` means "could not look", exactly like 78 and like
`probe_rc 2`: the caller fails closed. No verb in this unit exits 5.

### Journal and exit 5

`cycle-start` and `park` write no `actions` or `journal` row and never exit 5 in
this unit: (1) `actions.poll_at` is an enforced foreign key to `polls (at)`, and
nothing writes `polls` before #252/#253; (2) the intent the journal records is
the poll's (label, comment, container start), and the poll is not a caller of
these verbs yet; (3) exit 5 means "journaled and not settled", which has no
meaning without `journal-settle`. #254 adds the journal write to these verbs,
taking `--poll-at` and `--before` from the poll, in the same transaction
(section 5 rule 1), and adds exit 5 then. A1, A2 and A3 are owned by #254; the
tests named for them here are **partial** (the store half only).

### Stays a file in this unit

No verb here writes or replaces these; #250 and #251 decide each:

- `state/cycles/<n>.lock` (C5, C7) and `state/plan-<n>.json.lock` (a turn's liveness proof, spec 3.5).
- `.done`: written by the dispatcher (`dispatch:2028`), read by the cycle (`cycle:1010-1012`), which passes the word on as `--status`.
- `.stopped`, `.stopped.new`: written by `tehas stop` (`stop:166-187`), read by the cycle (`cycle:740-751`).
- work's handoff files: `.runner_exit`, `.status`, `.oom`, `.forks`, `.stalled`, `.timed`, `.nostart`, `.cid`, `.image`, `.settings`, `.result.json`, `.progress.json`, `.manifest.json`, `.images.md`, `.retry-context.md`. The verbs take their values; how `work` hands them to `cycle` is #250's.
- planning: `.out` and `.keep/` (a container reads or writes them; the row names `.out` in `pending_outcome`), `.rc`, `.answer` (the reply text; schema 2 has a column for it, D34, which the caller uses once it moves), `.turn` (the export copy, `planning.sh:242-245`), and the brief file `libexec/plan --brief <file>` reads (`dispatch:2858`): the caller writes it from `plan-show`'s `brief` each turn.
- `state/park-<n>.json`: the body handed to `q_render`, not a record.
- `state/refined.jsonl` (D9) and `state/cycles/plan-<n>.history.jsonl` until the import (D18).

## 1. The lock probe (`src/lock.rs`)

The probe is the liveness answer (C5). `cycle-show` runs it live;
`cycle-start`, `cycle-image` and `cycle-end` run it as a precondition. It never
reads or writes `cycle_locks`.

Path: `$TEHAS_HOME/state/cycles/<issue>.lock`, built from the issue number only.

1. `symlink_metadata` of `state/cycles`: a link or not a directory gives
   `probe_rc 2` (`why: "state/cycles is not a directory"`). NotFound: the lock
   file is absent (step 5). Any other error: `probe_rc 2`.
2. `symlink_metadata` of the lock: NotFound is **absent** (step 5); any other
   error is `probe_rc 2`; a link, directory or other non-regular file is
   `probe_rc 2`.
3. Open read-only, **never create** (`libexec/clean:233-239` says why). An
   error is `probe_rc 2`. The open file's dev and inode must equal step 2's,
   else `probe_rc 2`.
4. `File::try_lock()` (exclusive, non-blocking, flock(2) as `flock -n`):
   `Ok` is **free** (`probe_rc 0`), released at once; `WouldBlock` is **held**
   (`probe_rc 1`); any other error is `probe_rc 2`.
5. Absent: **free** (`probe_rc 0`, `file:false`) when the issue has no open
   cycle row (`ended_at IS NULL`); **unknown** (`probe_rc 2`,
   `why: "no lock file for a cycle on record as running"`) when it has one
   (D10, **sign-off**).

A caller that holds the lock and runs the store (fd 9 inherited from
`libexec/cycle` or `libexec/work`) reads held: the store's open is a new open
file description, and flock conflicts between descriptions.

## 2. Cycle verbs (`src/cycle.rs`)

### cycle-start

`store cycle-start <issue> [--rukas <commit>] [--by-hand] [--json]`

Called by `libexec/cycle` right after `flock -w 5 9` (`cycle:231-232`), in place
of the writes at `:238-239`.

- 64: grammar of `<issue>`, `--rukas`.
- Preconditions (exit 1, nothing committed):
  - `unlocked` the probe says free; `probe` the probe says 2. C7: the issue
    lock is taken before any change.
  - `unapproved` (without `--by-hand`) the issue has no unused approval.
- One transaction:
  - issues stub.
  - `attempt` = 1 + max(`cycles.attempt` of the issue, max
    `cycle_history.attempt` of the issue, 0).
  - `cycles` row by `INSERT ... ON CONFLICT(issue) DO UPDATE SET` every column,
    never a delete or `INSERT OR REPLACE` (a delete would cascade the issue's
    heartbeats). Values: `attempt`, `started_at = now`, `rukas_commit =
    --rukas` or NULL, every other column at its default. Without `--by-hand`:
    `started_by = approval.by_account`, `how = approval.how`, `requeued =
    (approval.how = 'outage-retry')`. With `--by-hand`: `started_by` NULL,
    `how = 'hand'`, `requeued` 0, and no approval is touched (an unused one
    stays for the start it was recorded for).
  - `DELETE FROM cycle_settings, cycle_notes, cycle_locks, cycle_results,
    cycle_progress WHERE issue = ?`: they are the previous attempt's, as
    `libexec/cycle:360-365` and `libexec/work:739` remove the files at a start.
    One result and one progress row per issue at most, the latest end's, kept
    until the next start for `tehas export` (`libexec/export:257-258`).
    Heartbeats (bounded by `prune`) and history stay.
  - Without `--by-hand`: the one unused approval gets `used_at = now`,
    `used_by = attempt`. Then the retention rule of section 4: used rows older
    than what `approval-current` can need are deleted.
- JSON: `"issue","attempt","started_at","started_by","how","requeued","approval_at"(null by hand),"replaced_unended"(bool: the previous row had ended_at NULL)`.
- What `libexec/cycle` does on exit 1: it runs nothing, logs the store's word,
  and exits non-zero before any label or comment; #250 picks the status and
  states it in the header. Whether a run is by hand is #250's to tell (for
  example from systemd's `INVOCATION_ID`).
- Replaces: `libexec/cycle:238` (`.rukas`), `:239` (`.taken`); the
  dispatcher's `:1574` (`.by`) and `:1575` (`.requeued`), both now read from
  the approval the dispatcher recorded; the cycle's clear of `.requeued` and
  `.park` (`cycle:360-365`). The dispatcher's `:1570` and `:1573` are not
  replaced by this verb (they run before the unit starts): see `park-retry` and
  `question-answered`.
- The dispatcher's wait for "taken" (`dispatch:1411-1427`, 30 s) becomes: read
  `cycle-show <issue>` for the attempt before `systemctl start`, then poll it
  every 0.5 s until `cycle.attempt` is greater (or a row appears). The lock
  fallback at `:1422` stays.
- Invariants: A2, A3 (partial, see above), B7 (the commit at start), C7, F6
  (`started_by` is the account of the approval).

### cycle-image

`store cycle-image <issue> --attempt N --image <image> [--rukas <commit>] [--settings <file>] [--json]`

Called by `libexec/work` as the container starts (`work:757-770`), with the
attempt `cycle-start` returned (passed down, for example as
`TEHAS_CYCLE_ATTEMPT`; a `tehas work` run on its own has none and does not
call it). Not in the spec's list: D3, **sign-off**.

- 64: grammars.
- Preconditions (exit 1): `nocycle` no `cycles` row; `attempt` the row's
  attempt is not N; `ended` the row has `ended_at`; `unlocked` / `probe` as
  `cycle-start`.
- `--settings` never fails the verb: the image is the fact B7 needs. The file
  is read before the store (section 0). Refused (a link, over 64 KiB, not JSON,
  not the shape) gives JSON `"settings":"refused"`; any value matching one of
  the six shapes of `tehas_improve_secret` (`lib/improve.sh:195-199`:
  `gh[pousr]_[A-Za-z0-9]{36,}`, `github_pat_[A-Za-z0-9_]{22,}`,
  `(^|[^A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}`, `xox[abprs]-[A-Za-z0-9-]{10,}`,
  `AKIA[0-9A-Z]{16}`, `-----BEGIN [A-Z ]*PRIVATE KEY-----`) gives
  `"settings":"secret"`. Either way no settings row is written.
- Settings shape, as `tehas_improve_settings` writes it (`improve.sh:166-191`)
  and `improve.sh:87-94` checks it: `{"container":{PI_ENSEMBLE_*: string},"host":{TEHAS_*: string},"concurrentCycles": int|null}`.
  Container keys `^PI_ENSEMBLE_[A-Z0-9_]+$`, host keys `^TEHAS_[A-Z0-9_]+$`,
  values strings of at most 4096 bytes. The prefix says which half a key came
  from; `concurrentCycles` is stored as that key when it is a number (D24).
- One transaction: `cycles.image`; `cycles.rukas_commit = --rukas` when given
  (the image's label as the container started is the fact,
  `lib/improve.sh:80-82`); with good settings, delete the issue's
  `cycle_settings` and insert one row per key.
- JSON: `"issue","attempt","image","rukas_commit","settings":<rows written>|"refused"|"secret"|null`.
- Replaces: `libexec/work:764` (`.image`), `:769-770` (`.settings`).
- Invariants: B7, F7 (no token is stored; a link is refused).

### cycle-end

```
store cycle-end <issue> --attempt N --status <word> [--step <word>] [--rc N]
  [--runner-exit N] [--watched] [--host-saw killed] [--oom N] [--forks N]
  [--stalled N] [--timed] [--branch <name>] [--pr N]
  [--result <file>] [--progress <file>]
  [--cap <w>] [--park-reason <w>] [--failure-kind <w>] [--failure-cause <w>]
  [--provider] [--parked] [--never-ran] [--json]
store cycle-end <issue> --attempt N --refused [--json]
store cycle-end <issue> --attempt N --stopped [--json]
```

**Report mode** (the first form). Called once by `libexec/cycle` at the end of
its report, where it writes the history line today (`cycle:1236-1251`).

- 64: strict grammars; `--host-saw` other than `killed`. Cleaned values never
  64 (section 0).
- Preconditions (exit 1): `nocycle`; `attempt` mismatch.
- Already ended: an end for attempt N when the row of attempt N already has
  `ended_at` is exit 0, `"already":true`, the row as it is, nothing written. A
  retry after a commit whose answer was lost gets success.
- `--status` is the word the cycle logs (`cycle:1232`). `end_reason` is that
  word when it is in the schema's set, else `unknown` (D15); the word itself
  goes to `cycle_history.status`, `endings.status` and `heartbeats.status`.
- `--result <file>` never fails the verb. Empty: no row, `"result":"none"`. A
  link or refused file: `"refused"`. Over 16 MiB: `"too-large"`. Not a JSON
  object with an integer `schemaVersion`: `"unreadable"`. Otherwise one
  `cycle_results` row `(issue, received_at = now, schema_version, payload = the bytes as read)`,
  `"result":{"schema_version":N}`. Any version is stored as a claim (G2).
- `--progress <file>` never fails the verb. Refused, too large (1 MiB),
  unreadable as above. The file's mtime is read once, before the store opens
  (D25). Inside the transaction, an mtime older than `cycles.started_at` is
  not stored, `"progress":"stale"` (G3). Otherwise one `cycle_progress` row:
  `received_at = now`, `reason` (string or NULL), `review_held` (`reviewHeld`
  as 0/1 or NULL), `steps_json`, `roles_json`, `written_at` = the mtime.
- One transaction:
  - `cycles`: `end_reason`, `ended_at = now`, `oom_kills`, `forks_refused`,
    `stalled_seconds`, `timed_out` (`--timed`), `runner_exit`, `watched`
    (`--watched` or `--runner-exit` given), `host_saw`, `branch` (when kept).
  - `cycle_results`, `cycle_progress` as above (the start removed the earlier
    ones, so `(issue, received_at)` cannot collide).
  - `endings`: INSERT OR REPLACE `(issue, started_at, ended_at = now, status, step)`,
    then every row but the 10 newest by `ended_at` deleted (`lib/live.sh:67`).
  - `heartbeats`: INSERT OR REPLACE `(issue, now, running = 0, step, status)`.
  - `cycle_history`: one row: `attempt`, `at = now`, `status`, `step`,
    `by_account = cycles.started_by`, `how = cycles.how`, `pr`, `started_at =
    cycles.started_at`, `rc`, `cap`, `park_reason`, `failure_kind`,
    `failure_cause`, `provider`, `parked` (D1); then only the 50 newest rows of
    the issue kept (`tehas_cycle_history_add`, `lib/fixer.sh:154-155`; D1).
  - `--never-ran`: delete every **used** approval of the issue (`used_by IS NOT
    NULL`); an unused one stays. Today the record of a start that never ran is
    removed and with it the only record there is (`cycle:1249-1251`). See D11.
- JSON: `"cycle":<row after the end>,"result":...,"progress":...,"history_id":N,"cleaned":[...],"already":false`.

**Refused mode.** For every exit of `libexec/cycle` after a successful
`cycle-start` that does not reach the report: the refusals at `cycle:254` and
`:262` (exit 65), and any other early exit #250 adds. Precondition: the probe
says held (the caller holds the lock), else `unlocked` / `probe`. One
transaction: `ended_at = now`, `end_reason` stays NULL (D22). No history,
endings, heartbeat or result; the approval stays used (today `.approved.json`
stays and the poll clears `tehas:running`, so a person reapplies the label).
Already ended: exit 0 `"already":true`.

**Stopped mode.** For `tehas stop` (`libexec/stop`), after the unit is gone,
when `cycle-show <issue>` shows an open row: the cycle was killed outright and
will never report. Precondition: the probe says free (else exit 1 `running` or
`probe`). One transaction: `ended_at = now`, `end_reason = 'stopped'`. No
history (stop writes none today), no endings. `tehas stop` calls this
**before** it removes the lock file, or the issue reads `probe_rc 2` (D10).
Already ended: exit 0 `"already":true`.

- Replaces: `libexec/work:798` (`.result.json`), `:938` (`.runner_exit`),
  `:968` (`.timed`), `:970` (`.stalled`), `:981` (`.status`), `:1013`
  (`.oom`), `:1017` (`.forks`), `:1040` (`.progress.json`) as records; the
  files stay as work's carrier until #250. `libexec/cycle:1124` (`.branch`),
  `:1237-1244` (the history line), `:1250` (`rm .approved.json`). From
  `lib/live.sh:61-76` only the ending of a cycle that reports; a cycle killed
  outright is recorded as `lost` by `live.sh:38-46`, which this verb does not
  replace: that needs #252's sweep.
- Invariants: F7/G1 (the result is a stored claim with its version), G3, C13
  (`host_saw`, `oom_kills`, `forks_refused` are what the caller observed), B4.

### cycle-show

`store cycle-show <issue> [--json]` or `store cycle-show --running [--json]`

Read only. The probe runs live for every entry.

- 64: neither or both of `<issue>` and `--running`.
- `<issue>`: `"issue":N,"lock":<lock>,"cycle":<cycle>|null`.
- `--running`: entries for the union of (a) every `cycles` row with `ended_at
  IS NULL` and (b) every name in `state/cycles/` matching
  `^[1-9][0-9]{0,9}\.lock$` whose probe says held or 2. Lock files that probe
  free with no open row are left out. Ordered by issue.
  `"probe_rc":0|2,"cycles":[{"issue":N,"lock":<lock>,"cycle":<cycle>|null}]`.
  Top-level `probe_rc` is 2 (with `"why"`) when any entry is 2, when
  `state/cycles` exists and cannot be listed, or when any directory entry
  cannot be read. A caller that reads only the top level then fails closed, as
  `tehas_cycles_running` counts an unprobeable lock as running
  (`lib/cycles.sh:44`). A missing `state/cycles` lists only (a), each probed
  per section 1.
- Exit: 0, 64, 78, 1 (`busy` only, "could not look").
- Writes nothing, never `cycle_locks` (D6).
- Replaces the readers `tehas_cycle_running`, `tehas_cycle_lock_free`,
  `tehas_cycles_running` (`lib/cycles.sh:42-61`) and the `.taken` mtime reads
  (`dispatch:1929`, `:2005`; `started_at` now).
- Invariants: C5, D2.

### cycle-note

`store cycle-note <issue> --kind quiet --said-for <text> [--json]`

The quiet-cycle episode (`dispatch:1891-1984`). `said_for` is the time the
cycle was last heard from, the episode's key.

- 64: `--kind` other than `quiet` (the set is closed); grammar.
- Preconditions (exit 1): `nocycle`; `ended`.
- One transaction, `cycle_notes` only: a `quiet` note of the issue with another
  `said_for` is deleted (that episode is over); a note with this `said_for` is
  left as it is (its `at` stays the episode's first sight); otherwise one is
  inserted with `at = now`.
- JSON: `"said":bool` (true only when this call recorded the episode for the
  first time), `"episode_at":<note.at>`, `"times":<asks in this episode>`,
  `"stop_asked_at"`. `times` is `cycles.stop_said_times` when `stop_asked_at
  >= note.at`, else 0 (no ask yet in this episode).
- Replaces: the note's episode key (`dispatch:1962`, `[ noted = heard ] ||
  asks=0`). The removals at `:1940`, `:1953` and `retire_quiet_notes`
  (`:1917-1921`) need no verb: a note of an older episode is replaced by the
  next episode's, and `cycle-start` removes the issue's notes.

### cycle-stop-asked

`store cycle-stop-asked <issue> [--said-only] [--json]`

- Preconditions (exit 1): `nocycle`; `ended`; `nonote` the issue has no
  `quiet` note (the caller calls `cycle-note` first).
- One transaction, `cycles` only:
  - An ask (no switch): when `stop_asked_at` is NULL or older than the note's
    `at`, this is the episode's first ask: `stop_said_times = 1`; otherwise
    `stop_said_times += 1`. Then `stop_asked_at = now`.
  - `--said-only` (the "asks no more" line, `dispatch:1971`): `stop_asked_at =
    now`, the count unchanged, so the line is said once per its own interval.
- JSON: `"times":N,"asked_at":now`.
- The caller's rule for the next ask or say, unchanged from today: when
  `times < quiet_asks`, at `stop_asked_at + quiet_retry`; else at
  `stop_asked_at + quiet_retry * quiet_asks`.
- Replaces with `cycle-note` the writes at `dispatch:1971` and `:1982`.
- Invariant: C6 (the store half).

## 3. Park and question verbs (`src/park.rs`)

**Current park** of an issue: the `parks` row with `attempt = cycles.attempt`.
A new `cycle-start` advances the attempt, so the old park stops being current
without a delete.

**Question on record** (the trust root of F5):
- park: the newest `questions` row with `kind = 'park'`, `answered_at IS NULL`,
  `attempt = cycles.attempt`, while the issue has a current park.
- plan: `planning.comment_id` of the open session, which must have a
  `questions` row of kind `plan` with `answered_at IS NULL`.

A **withdrawn** question is a deleted row: the host gave it up and no person
answered it (D4).

### park

`store park <issue> --attempt N --reason <reason> [--step <word>] [--pr N] [--comment-id <id> --body-sha <sha256>] [--json]`

Called by `park()` in `libexec/cycle` (`:1041`).

- 64: strict grammars; `--comment-id` without `--body-sha` or the reverse.
  `--reason` and `--step` are cleaned, never 64.
- `--reason`: `provider-outage` when the runner hit a provider outage
  (`cycle:1204`), else the cap, else the status word (the caller chooses). One
  grammar, the cap's, covers all three (section 0).
- Preconditions (exit 1): `nocycle`; `attempt` mismatch; `parked` a park of
  this attempt exists with another reason, step or pr (the same three: exit 0,
  unchanged); `taken` the comment id is already a question of another issue,
  or of this issue with kind `plan` (the same check as `question-asked`).
- One transaction: issues stub; `parks (issue, attempt, at = now, reason, step, pr, retries = 0)`;
  with `--comment-id`, a `questions` row `(id, issue, attempt, 'park', posted_at = now, body_sha256)`.
- JSON: `"issue":N,"question":<id>|null,"park":<park>,"cleaned":[...]`.
- Replaces: `libexec/cycle:1048-1050` (`.park`).
- Invariants: C12 (`retries` starts at 0), F5.

### park-show

`store park-show <issue> [--json]` or `store park-show --automatic [--json]`. Read only.

- 64: neither or both.
- `<issue>`: `"park":<park>|null`, the current park. `started_at`,
  `started_by` are the cycle's; `automatic` is `reason = 'provider-outage' AND
  retries = 0 AND cycles.requeued = 0 AND cycles.how <> 'hand' AND
  cycles.started_by IS NOT NULL` (today's `.retry`, `cycle:1048`, `:1205`);
  `question` is the park question on record.
- `told_at`: when `park-told` said the hold, or `null`. `body_sha256`: the
  body the parked attempt's approval bound (the approval with `used_by` =
  the park's attempt), `null` for a run by hand or a start that never ran
  (D32).
- `--automatic`: `"parks":[<park>...]`, every current park with `automatic`
  true, ordered by issue. It replaces rule 4a's loop over
  `tehas_cycle_note_files park` (`dispatch:2878`).
- Replaces the readers of `.park` (`dispatch:2877-2896`: `.retry`,
  `.started`), `.by` (`:2886`) and `park-<n>.id` (`:2538`).
- Invariants: F6 (the retry rests on `started_by` of the parked cycle), C12.

### park-retry

`store park-retry <issue> [--json]`

- Preconditions (exit 1): `unparked` no current park. A caller that calls it
  without knowing whether there is one treats `unparked` as done.
- One transaction: `parks.retries += 1` of the current park.
- JSON: `"retries":N,"automatic":bool` (whether an automatic retry was owed
  before this call).
- Called, in place of removing `.park`:
  - at every start, before `start_cycle` (`dispatch:1573`), so a start whose
    unit never comes up (`:1588-1594`) leaves no automatic retry behind;
  - by rule 4a for a park that can start nothing (`dispatch:2894`);
  - by `tehas stop` (`stop:114`).
- Invariant: C12 (one automatic retry, then a person).

### park-told

`store park-told <issue> [--json]` (D32)

- Preconditions (exit 1): `nopark` no current park.
- One transaction: `told_at = now` on the current park when it has none.
- JSON: `"said":true` the first time, `false` when `told_at` was set already,
  and `"park":<park>`.
- Replaces `park_holds`'s `.held = true` in `.park` (`dispatch:1799-1805`).
  A new attempt's park starts untold.

### question-asked

`store question-asked <issue> --id <id> --kind park|plan [--attempt N] [--body-sha <sha256>] [--json]`

- 64: grammars, kind.
- Preconditions (exit 1): park: `unparked` no current park; plan: `noplan` no
  open session; `attempt` when `--attempt` is given and is not the derived one;
  `taken` the id is a question of another issue or of the other kind.
- Derived attempt: park, `cycles.attempt`; plan, `planning.turns`.
- One transaction: issues stub; insert the `questions` row (the same id, issue
  and kind already there: exit 0, unchanged); for `plan`,
  `planning.comment_id = id` too.
- JSON: `"question":<question>`.
- Replaces: `libexec/cycle:1060` (`park-<n>.id`), `lib/planning.sh:133`
  (`.comment = $c`).
- Invariants: F5, G4.

### question-known

`store question-known <issue> --id <id> [--json]`. Read only.

- JSON: `"known":bool,"kind":"park"|"plan"|null,"recorded":bool,"answered":bool`.
  `known` is true **only** when the id is the question on record of this issue
  (section 3 above), which is the one a tick may count in. `recorded` says a
  row with that id and issue exists at all; `answered` that it has
  `answered_at`. An id on another issue is `known:false`, `recorded:false`.
- Replaces the id comparison the poll makes before it honours a tick
  (`dispatch:2529-2565`).
- Invariants: F5 (only the comment this host has on record counts), G4.

### question-answered

`store question-answered <issue> --id <id> --by <account> [--json]`
`store question-answered <issue> --id <id> --withdraw [--json]`

Not in the spec's list: D4.

- 64: grammars; neither or both of `--by` and `--withdraw`.
- Preconditions (exit 1): `unknown` no question with that id on that issue.
- `--by`: a person's answer. `answered_at = now`, `answered_by`. Already
  answered: exit 0, unchanged.
- `--withdraw`: the row is deleted. For a plan question that is
  `planning.comment_id`, `comment_id = NULL` too.
- JSON: `"question":<question>` (by), `"withdrawn":true` (withdraw).
- Replaces: `dispatch:2608` (a person chose another box: `--by` the ticker),
  `dispatch:1570` (withdrawn at a start), `libexec/cycle:1066` (withdrawn: the
  label could not be added), `stop:114` (`park-<n>.id`). The planning start
  (`dispatch:2849`) withdraws through `plan-begin`.
- Invariant: F5.

## 4. Approval verbs (`src/approval.rs`)

An issue has **at most one unused approval**, as it has one `.approved.json`
today. **Current** is the used row with the highest `used_by`: the approval
of the latest start. Order never depends on `at`, which mixes GitHub's clock
(label, tick) with the host's (refresh, outage retry).

### approval-record

```
store approval-record <issue> --at <unix> --by <account> --how <how> [--label <label>]
  --body-sha <sha256> [--title-sha <sha256>] [--comment-id <id>]
  [--refresh-pr N --refresh-head <sha>] [--input-file <file>] [--json]
store approval-record <issue> --undo --at <unix> [--json]
```

Called by `approved_record` (`dispatch:1240`), before the label.

- 64: grammars; `--how` outside the set (the word on stderr); `--refresh-pr`
  and `--refresh-head` both or neither, and both exactly when `--how refresh`;
  `--label` given exactly when `--how` is `label` or `fixer`.
- `label` column: the given label for `label` and `fixer`; for `retry-tick`,
  `outage-retry` and `refresh`, which start without a label event
  (`dispatch:2658`, `:2905-2909`, `:1709-1713`), the `how` word itself (D21).
- `--at` is when the approval was given, as `given_at` is today. For
  `refresh` and `outage-retry` the caller passes the instant it decided the
  start (now), not the `at` of the approval they rest on (D5).
- Preconditions (exit 1): `used` a used row `(issue, at)` exists (A3: an
  approval starts one cycle; the caller tries again on its next poll).
- One transaction: issues stub; delete the issue's other unused rows; upsert
  `approvals (issue, at)` with every column given, `used_at`, `used_by` NULL.
- `--undo --at T`: deletes the unused row at `(issue, T)`; nothing else. No
  such unused row: exit 0, `"undone":false`. This is `restore_record`
  (`dispatch:1382-1393`) for every give-up after the record (`:1546`, `:1562`,
  `:1584`): the record put back is the previous one, which is still `current`.
- JSON: `"approval":<approval>` or `"undone":bool`.
- `--input-file`: the cycle's inputs, read before the lock (a regular file,
  1 MiB). Exit 1 `input` when it is refused, over the bound, not UTF-8, not a
  JSON object, or holds a key other than `links`, `comment_links`,
  `retry_context` (arrays) and `refresh_of` (an object or null). Stored in
  `input_json` and answered as the approval's `input`; without the flag the
  row has none. `--undo` removes them with the row (D30).
- Replaces the authorisation fields of `approved_record`
  (`dispatch:1240-1270`), and with `--input-file` its inputs (`links`,
  `comment_links`, `retry_context`, `refresh_of`): D2, D30.
- Invariants: A3, F3 (the approval binds `body_sha256` and `title_sha256`).

### approval-current

`store approval-current <issue> [--json]`. Read only.

- JSON:
  - `"approval"`: the unused approval, or `null` (the spec's "the open
    approval, or `{"approval":null}`"). A3: it is the one start still owed.
    Each approval object carries its `input` (D30).
  - `"current"`: the used row with the highest `used_by`, or `null`. It is
    what `.approved.json` is for its readers after a start
    (`libexec/cycle:1190-1216`, `:1242`, `lib/fixer.sh:86`,
    `lib/attachments.sh:115`, `libexec/work:604-611`).
  - `"rests_on"`: `{"at","by","how","body_sha256","used_by"}` of the person's
    approval `current` rests on, from **used rows only**: `current` itself when
    its `how` is `label`, `retry-tick` or `outage-retry`; for `fixer` and
    `refresh`, the used row of those three kinds with the highest `used_by`
    below `current.used_by`. `null` when none. An approval that started nothing
    is never a base (`libexec/cycle:1196`). The allowed-labeller check stays in
    bash (`tehas_fixer_base`, `fixer.sh:70`).
  - `"fixer_starts"`, `"refreshes"`: used rows of `how` `fixer` and `refresh`
    with `used_by` above `rests_on.used_by` (`current` included, as the record
    of a fixer start counts itself today). The caller adds one for a start it is
    about to make (`fixer.sh:112`).
- Replaces the readers above and `tehas_fixer_base`, `tehas_fixer_starts`,
  `tehas_refresh_count` (`fixer.sh:59-78`, `refresh.sh:75-79`) as far as they
  parse the file.
- Invariants: A3, F3.

**Retention.** `cycle-start`, after it uses an approval, deletes the issue's
used rows with `used_by` below `rests_on.used_by` of the new `current` (all
used rows but `current` when `rests_on` is null). What stays is exactly what
`approval-current` reads: the base and the fixer and refresh starts on it, at
most `1 + TEHAS_FIXER_MAX_RETRIES + the refresh limit` rows per issue (B4).

## 5. Planning verbs (`src/plan.rs`)

The **open session** of an issue is its `planning` row with `ended_at IS NULL`.
An ended row stays until the next `plan-begin` of the issue replaces it.

### plan-begin

`store plan-begin <issue> --by <account> --label tehas:plan|tehas:refine --brief-file <file> [--json]`
`store plan-begin <issue> --undo [--json]`

The first turn of a session (`plan_turn ... first`, `planning.sh:369-372`).

- 64: grammars; `--undo` with any other flag.
- Preconditions (exit 1): `running` the open session has `running = 1` (A13: a
  turn in flight, or a lost session not yet closed); `brief` the file is
  refused, over 1 MiB, or not UTF-8.
- One transaction: issues stub; an existing row of the issue, ended **or open
  and not running**, is replaced as `dispatch:2849` does today (`rm -rf "$st"`
  on any record): its unanswered plan questions are withdrawn (deleted), then
  the row is deleted (cascading answers and filed); every unanswered `park`
  question of the issue is withdrawn (`dispatch:2849` removes `park-<n>.id`);
  insert `planning`: `session` NULL, `turns` 0, `options_json` `[]`,
  `pending` 0, `failed` 0, `running` 1, `started_at = now`, `comment_id` NULL,
  `lost` 0, `by_account`, `label`, `brief` = the file's text, `turn_rc` NULL,
  `pending_outcome` NULL, `opened_at = now`, `ended_at` NULL.
- JSON: `"planning":<planning>,"replaced":"none"|"ended"|"open"`.
- Undo: the open session must be in its first turn (`turns` 0, `running` 1,
  `session` NULL), else exit 1 `turn`. Deletes the row. `"undone":true`.
  Replaces `lib/planning.sh:397` (`rm -f "$st"` when the label could not be
  set). It does not bring back a session it replaced; neither does today's
  code.
- Replaces: `lib/planning.sh:369-372`, `dispatch:2849-2858` (the brief text is
  now the row).
- Invariants: A13, B8, F5.

### plan-answer

`store plan-answer <issue> --turn N --by <account> [--fixer] [--text-file <file>] [--json]`
`store plan-answer <issue> --turn N --undo [--json]`

The begin of a later turn, which is an answer (`plan_turn ... next`,
`planning.sh:374-380`). One write today, one transaction here (D7).

- 64: grammars; `--undo` with `--by`, `--fixer` or `--text-file`.
- Preconditions (exit 1): `answer` the `--text-file` is refused, over 64 KiB
  or not UTF-8 (read before the lock); `noplan` no open session; `turn` N is
  not `turns + 1`, or the session is running, or `session` is NULL, or `lost`
  is 1.
- One transaction: `planning_answers (issue, N, by, fixer, text)`, `text`
  the file's text or NULL (D34); `planning`
  `running = 1`, `started_at = now`, `pending = 0`, `comment_id = NULL`; the
  question that was `comment_id` gets `answered_at = now`, `answered_by = --by`.
- JSON: `"turn":N,"planning":<planning>`.
- Undo: requires `running` 1, `turns + 1 = N` and an answer row for N, else
  exit 1 `turn`. Deletes answer N; `running = 0`; the plan question with
  `answered_at = planning.started_at` and `answered_by` = answer N's account
  (both written by the same transaction above) gets its answer cleared and
  becomes `comment_id` again. When none matches (the answer closed no question,
  for example after a failed turn whose note had none), `comment_id` stays
  NULL. Replaces `lib/planning.sh:395`.
- Replaces: `lib/planning.sh:374-380`.
- Invariants: B8, F5 (the answered question is closed in the same write).

### plan-end

Three modes, exactly one per call:

```
store plan-end <issue> --turn N --outcome <o> [--rc N] [--session <id>]
  [--option <label>]... [--filed <url>]... [--out <file>] [--json]
store plan-end <issue> --posted [--json]
store plan-end <issue> --close [--json]
```

**Turn end** (`plan_finish`, `planning.sh:241-340`):

- 64: strict grammars; `--outcome` outside the set; `--rc` with `finished`
  other than 0, or missing with `finished` or `failed`; `--rc 0` with `failed`;
  `--out` not an absolute path under `$TEHAS_HOME/` or holding a `..`
  component. `--session`, `--option`, `--filed` are cleaned, never 64.
- The caller maps today's rc to `--outcome` as `plan_history` does
  (`planning.sh:175-181`): 0 `finished`, `interrupted`, `not started` as
  `not-started`, `not readable` as `not-readable`, any other number `failed`
  with `--rc`; a turn whose outcome says the session is lost is `lost` (B8).
- `--session` is the session the runner's outcome named, given only when it
  named one (`planning.sh:278`, `:284`).
- Preconditions (exit 1): `noplan`; `turn` the session is not running or N is
  not `turns + 1`.
- One transaction:
  - `plan_history (issue, turn = N, at = now, outcome, session = --session or the row's, started_at = planning.started_at, rc, by_account, label, filed_json = the kept --filed URLs)`;
    then only the 50 newest rows of the issue kept (D1).
  - Then the session, by the first rule that applies:
    1. `lost`: `lost = 1`, `turn_rc`. `running`, `turns` and the rest are
       unchanged, as `planning.sh:268-276` changes only `.lost`: a poll that
       cannot take the labels off finds the session again through `running`
       (`dispatch:2470`, `plan_recover`, `plan_lost`) and tries again. The
       session ends with `--close`.
    2. `finished` with `--session` given: `session`, `turns = N`,
       `options_json` = the `--option` labels in order, `pending = 1`,
       `pending_outcome = --out`, `failed = 0`, `running = 0`, `turn_rc = 0`;
       each kept `--filed` into `planning_filed (issue, url, filed_issue, at = now)`
       OR IGNORE (`planning.sh:297-307`).
    3. `--session` given, or a session on the row: `session` (given, else the
       row's), `turns = N`, `options_json` `[]`, `pending = 1`,
       `pending_outcome = --out`, `failed = 1`, `running = 0`, `turn_rc`;
       `planning_filed` is kept (`planning.sh:308-311`; D8). A `finished`
       without `--session` lands here, as today's `ok=0`.
    4. no session at all: `turns = N`, `running = 0`, `turn_rc`, `ended_at =
       now` (`planning.sh:313-332`, the record goes); its notes are removed
       (D33).
- `pending_outcome` is stored relative to `$TEHAS_HOME` (for example
  `state/plan-12.json.out`): the row names the file the container's outcome is
  in until it is posted (#251).
- JSON: `"planning":<planning>,"outcome":"<word>","history_id":N,"cleaned":[...]`.

**Posted** (`plan_post` step three, `planning.sh:148`): `pending = 0`,
`pending_outcome = NULL` of the open session. No open session, or already 0:
exit 0, unchanged. `"planning":<planning>|null`.

**Close** (`plan_end`, `planning.sh:93-105`; the turn limit at `:358`;
`plan_lost`): the open session gets `ended_at = now`, `running = 0`, `pending
= 0`, `pending_outcome = NULL`, `comment_id = NULL`; its unanswered plan
questions are withdrawn (deleted) and its notes removed (D33). No open session: exit 0, `"closed":false`.
`"closed":true,"planning":<planning>`.

- Replaces: `lib/planning.sh:104`, `:148`, `:186-194` (the history line),
  `:270`, `:306`, `:311`, `:331`.
- Not written here: `state/refined.jsonl` (D9).
- Invariants: B8 (session, brief and filed survive every turn; a lost session
  has its own flag and its own outcome `lost`), A13.

### plan-show

`store plan-show <issue> [--json]` or `store plan-show --open [--json]`. Read only.

- 64: neither or both.
- `<issue>`: `"planning":<planning>|null` (an ended row is returned, with
  `ended_at`).
- `--open`: `"sessions":[<planning>...]` with `ended_at IS NULL`, by issue.
  The caller retries `plan_lost` for `lost` true there.
- Replaces the readers of `state/plan-<n>.json` (`dispatch` rules 4b and 5,
  `plan_state` globs, `:2531-2533`, `libexec/health:115`,
  `libexec/export:255`, `libexec/plan`), and `plan_filed`
  (`planning.sh:111-116`): `filed` holds the URLs, so the "Already filed in
  this session" line (`:411-414`) is unchanged.
- `answer`: the text of the answer of the turn that is running (`turns + 1`)
  or whose outcome waits to be posted (`turns`, `pending`), else `null`.
  `notes`: the session's `plan-note` ids by kind, oldest first (D33, D34).
- Invariant: A13.

### plan-note

`store plan-note <issue> --kind fixer-refused|held --id <comment id> [--json]` (D33)

- 64: grammars; `--kind` outside the set (the word on stderr).
- Preconditions (exit 1): `noplan` no open session.
- One transaction: `planning_notes (issue, kind, id, at = now)`, kept when
  it is there already.
- JSON: `"said":true` when the note is new, `false` when it was there, and
  `"planning":<planning>`.
- The notes go with the session: `plan-begin` replacing it and `plan-begin
  --undo` delete the row (the notes cascade), `plan-end --close` and a turn
  end that leaves no session delete them (B4).
- Replaces the record's `fixer_refused` and `held` lists
  (`dispatch:1191-1194`, `:1209-1212`).

### history-show

`store history-show [<issue>] [--kind cycle|plan] [--how <how>] [--json]`. Read only (D31).

- 64: `<issue>` not a number; `--kind` or `--how` outside its set (`--how` is
  a `cycles.how` word: `label`, `refresh`, `retry-tick`, `outage-retry`,
  `fixer`, `hand`); `--how` with `--kind plan`.
- JSON: `"rows":[...]`, oldest first by `at`, then issue, then id. A cycle row
  is `{"kind":"cycle","id","issue","attempt","at","status","step","by","how","pr","started_at","rc","cap","park_reason","failure_kind","failure_cause","provider"(bool),"parked"(bool)}`;
  a plan row is `{"kind":"plan","id","issue","turn","at","outcome","session","started_at","rc","by","label","filed":[<url>...]}`.
  The two tables number their ids apart, so the id orders rows of one kind.
- `--how` keeps the cycle rows started that way; a plan row has no `how`, so
  none is kept.
- Replaces the readers of the history files: `libexec/history:96-101`,
  `tehas_cycle_history` (`lib/fixer.sh:158-162`, used by `lib/refresh.sh:83`)
  and the analytics run's facts.

## 6. Tests

All in Rust, in the new `*_tests.rs` files, each test with its own temp
`TEHAS_HOME` and a store made by `execute(Verb::Init ...)`. The lock is held in
tests with `File::lock()` on the lock file. Each verb gets at least: a happy
path that reads the rows back with SQL; every 64 case of its table; every
exit-1 word; a busy case (a second connection holds BEGIN IMMEDIATE: exit 1,
`busy`, nothing committed); and one table-driven test over all verbs for the
four 78 words and schema 0 (`unreadable`).

Shared cases (`verbs_tests.rs`):
`a_refused_verb_leaves_no_issues_row` (cycle-start `unapproved`, park
`nocycle`, approval-record `used`, and busy: `issues` stays empty);
`a_flag_value_that_starts_with_a_dash_is_taken_as_given` (`cycle-end --step -`
stores `-`); `a_constraint_failure_is_exit_1_conflict_not_78`;
`files_are_read_before_the_write_lock` (a held BEGIN IMMEDIATE and an
unreadable result: the verb reports busy only after the file was judged, and
the file judgement needs no lock); `a_reader_waits_out_a_short_lock`
(a lock held for 200 ms and released: `cycle-show` answers 0);
`every_64_names_the_flag_and_an_enumerated_word`.

Invariant cases:

| Invariant | Test |
| --- | --- |
| A2 (partial) | `cycle_start_with_the_lock_free_leaves_the_approval_unused`; `cycle_start_marks_the_approval_used_by_its_attempt_in_one_transaction`; `cycle_start_that_is_busy_uses_nothing` |
| A3 (partial) | `a_second_cycle_start_without_a_new_approval_is_unapproved`; `approval_record_over_a_used_approval_is_refused`; `approval_record_keeps_one_unused_row`; `cycle_start_uses_the_unused_row_whatever_its_at`; `approval_undo_removes_only_the_unused_row`; `approval_current_approval_is_null_once_used` |
| B4 | `cycle_start_clears_the_previous_attempt_s_settings_notes_locks_results_and_progress`; `cycle_start_keeps_earlier_heartbeats_and_history`; `endings_keep_the_ten_newest`; `history_keeps_fifty_per_issue`; `approvals_keep_only_the_base_and_its_starts`; `plan_begin_replaces_an_ended_session_and_its_children`; `plan_begin_replaces_an_open_session_that_is_not_running` |
| B7 | `cycle_start_records_the_commit_it_was_given`; `cycle_image_records_image_commit_and_settings_at_container_start`; `cycle_image_records_the_image_when_settings_are_refused`; `cycle_end_does_not_change_image_or_commit` |
| B8 | `plan_session_brief_and_filed_survive_a_failed_turn`; `plan_answer_keeps_the_session_and_brief`; `a_lost_session_is_marked_lost_with_its_own_outcome`; `a_lost_session_stays_running_until_closed`; `a_finished_turn_without_a_session_is_a_failed_turn`; `filed_keeps_the_url_of_another_repository` |
| C6 | `cycle_note_says_once_per_episode`; `a_new_episode_counts_its_asks_from_one`; `said_only_moves_the_time_and_not_the_count` |
| C12 | `park_retry_spends_the_automatic_retry_once`; `a_requeued_cycle_s_outage_park_owes_no_automatic_retry`; `park_show_automatic_lists_only_owed_retries`; `a_run_by_hand_owes_no_automatic_retry` |
| C5 | `cycle_show_reports_held_while_the_test_holds_the_lock`; `cycle_show_writes_no_cycle_locks_row` |
| D2 | `cycle_show_without_a_lock_file_and_no_cycle_is_free`; `cycle_show_without_a_lock_file_for_a_running_cycle_is_probe_rc_2`; `cycle_show_of_a_link_or_directory_is_probe_rc_2`; `cycle_show_running_is_probe_rc_2_when_any_entry_is`; `cycle_show_of_an_unlistable_directory_is_probe_rc_2`; `the_probe_never_creates_a_lock_file`; `park_show_with_nothing_is_null_and_0`; the 78 table |
| A13 | `plan_begin_refuses_a_running_session`; `plan_begin_refuses_a_lost_session_not_yet_closed`; `plan_show_open_lists_only_open_sessions` |
| F5 | `question_known_is_false_for_an_id_never_asked`; `question_known_is_false_for_an_id_of_another_issue`; `question_known_is_false_for_an_answered_question`; `a_park_question_is_not_known_after_a_new_cycle_start`; `a_withdrawn_question_is_not_known`; `plan_answer_closes_the_question_it_answers`; `plan_answer_undo_never_reopens_an_older_question`; `park_refuses_an_id_that_is_a_plan_question` |
| F6 | `park_show_names_the_account_that_started_the_parked_cycle` (approval by A used, later unused approval by B: `started_by` is A) |
| F3 | `approval_current_rests_only_on_used_rows`; `an_unused_label_approval_is_never_a_base` |
| F7 | `cycle_end_stores_a_result_of_an_unknown_schema_as_a_claim`; `cycle_end_does_not_store_a_result_that_is_not_json`; `a_result_or_settings_file_that_is_a_link_is_refused`; `cycle_image_refuses_each_of_the_six_secret_shapes` |
| G3 | `cycle_end_does_not_store_progress_older_than_the_start` |
| Ends always commit | `an_unknown_status_word_ends_as_unknown`; `every_word_libexec_cycle_assigns_is_in_the_end_reason_set` (`closed`, `merged`, `handoff`, `killed`, `stalled`, `timeout`, `interrupted`, `unknown`, and the `.done` words); `cleaned_values_never_fail_an_end` (status and step over 40, an uppercase cap over 80, a container branch, a session with a space, a 400-byte option with a newline, 25 options); `a_second_end_of_one_attempt_is_already_and_writes_nothing`; `a_refused_start_ends_the_row_with_no_history`; `a_stopped_end_needs_a_free_lock` |

## 7. Disagreements with the spec, and the spec text this PR changes

Each item says what the PR does to `docs/specs/2026-10-21-state-store.md`.
**Sign-off** marks the ones the owner has to accept in review.

**D1. History rows carry today's history line.** `cycle_history` in spec 3.8
lacks `started`, `rc`, `cap`, `park_reason`, `failure_kind`, `failure_cause`,
`provider`, `parked`, which the line at `libexec/cycle:1237-1244` has,
`tehas history --json` publishes (`libexec/history:150-160`: "nothing here
renames or removes one") and `lib/refresh.sh:83` queries. `plan_history`
lacks `started`, `rc`, `by`, `label`, `filed`. Columns are added (section 0).
The four runner words are claims, cleaned to tehas's own short sets before
they reach the store (section 0), which is how the history file holds them
today. Both histories keep the 50 newest rows per issue, as
`tehas_cycle_history_add` does (`lib/fixer.sh:154-155`): B4 needs a bound and
`prune` does not list the histories. **Spec change:** 3.8's DDL gets the
columns, "Append-only, one row per end" becomes "one row per end, the 50
newest per issue kept", and section 3's convention line about free text gets
"a container's word is stored only cleaned to a fixed set and bound, as a
claim".

**D2. The approval file carries the cycle's inputs, the table does not.**
`links`, `comment_links`, `retry_context`, `refresh_of.base_sha`,
`refresh_of.branch` (`dispatch:1240-1270`) have no column. `rests_on`,
`fixer_starts`, `refreshes` are derived by `approval-current`. The inputs are
what the cycle is given, like `.manifest.json`: #250 decides between a file
kept like the manifest and an added `approvals.input_json`. No spec change.

**D3. `cycle-image` (sign-off).** The spec puts `--image` and `.settings` on
`cycle-start` (section 2 row 1, section 4). The image is resolved as the
container starts (`libexec/work:757-770`), minutes after the lock and the
dispatcher's 30 s wait, and B7 says record a fact when it becomes true; an
image read at lock time could differ from the one the container runs.
**Spec change:** section 2 row 1 names `cycle-start` + `cycle-image`; the
section 4 table gets a `cycle-image` row and `cycle-start` loses `--image` and
`--branch` (the branch is known at the end and goes on `cycle-end`).

**D4. `question-answered` (sign-off).** The listed verbs cannot record a
person's answer to a park question (`dispatch:2608`) or a question the host
gave up (`dispatch:1570`, `libexec/cycle:1066`, `stop:114`); without that,
`question-known` would honour a question the host withdrew, which is wider
than today (F5). A withdrawn question is a deleted row, so `answered_by` keeps
the spec's meaning (a person's answer). **Spec change:** a section 4 row; a
line under 3.7: "A question the host withdrew is deleted."

**D5. Approvals key `(issue, at)`.** A refresh's `given_at` is the `at` of the
person's approval it rests on (`dispatch:1713`), which is a used row with that
key. The caller passes the instant it decided the start for `refresh` and
`outage-retry`; both check the text unchanged up to that moment
(`dispatch:1670-1672`, `:2892`), so a later `at` claims nothing more. At most
one unused row per issue, and `current` is chosen by `used_by`, so `at` never
orders rows. **Spec change:** 3.4 says so.

**D6. `cycle-show` writes nothing; readers may say `busy`.** Section 4's prose
names `cycle-show` as a writer of `cycle_locks`; its table row writes nothing.
The table wins: a liveness read never takes the write lock. Readers wait at
most 1 s and then exit 1 `busy`, which the common rules already define;
`cycle_locks` is written by `poll-commit` and `sweep` (#252/#253). **Spec
change:** the prose drops `cycle-show` from the cache's writers; the readers'
exit columns gain 1.

**D7. `plan-answer` begins a later turn.** Today the answer and the running
mark are one rewrite (`planning.sh:378`); two verbs would leave an answer for
a turn that never ran, which `tehas_fixer_answer` counts (`fixer.sh:116`).
`plan-begin` is the first turn only and drops the spec's `[--session]
[--turn N]`. **Spec change:** the two rows.

**D8. A failed turn keeps `filed`.** `planning.sh:311` rewrites the record
from `{by, answers, label}` and loses `filed`, against B8. `planning_filed`
rows are never removed by a turn end. No spec change (B8 says it).

**D9. `refined` is not written.** The ledger line is `{at, source, filed,
url}` (`planning.sh:212-216`, read by `console/analytics`); the table has
`issue, at, kept, outcome`, and the spec does not list it among `plan-end`'s
writes. `state/refined.jsonl` stays until the analytics reader moves. No spec
change.

**D10. A lock file that is not there (sign-off).** Section 4 lists "no lock
file" as could-not-tell. Read literally, every issue that never ran a cycle
would hold its caller. `tehas_cycle_lock_free` and `tehas_cycle_running` read
an absent file as free (`lib/cycles.sh:42-50`), and `tehas stop` removes the
lock as its way of clearing (`stop:16`, `:105`). A process that holds a cycle
lock made the file first (`cycle:231`, `work:409-410`), so an absent file with
no open row hides no holder; an open row with no file is the case the spec
means, and stays `probe_rc 2`. `cycle-end --refused` and `--stopped` make
sure no row stays open by accident. **Spec change:** section 4's probe text:
"no lock file for a cycle on record as running".

**D11. `cycle-end` does not clear `approvals`.** Section 2 group 2 and the
section 4 row say it does. Today the record stays after an end: the fixer and
a refresh read it later (`fixer.sh:86`, `dispatch:1664`). The table is bounded
by `cycle-start`'s retention rule instead (section 4). `--never-ran` deletes
every used row of the issue, as today's `rm -f` leaves nothing for the fixer
to rest on; deleting only that start's row would let the fixer rest on an
older approval. **Spec change:** the `cycle-end` row says "deletes the used
approvals of a start that never ran", group 2's `.approved.json` note follows.

**D12. `cycle-start` arguments; runs by hand.** The spec gives `--by --how
[--requeued] --lock <path>`. `by` and `how` are the approval's and `requeued`
is `how = outage-retry` (`dispatch:2905-2909`), so as arguments they could
disagree with the approval the same transaction uses; the lock path is the
issue's. None of the four is an argument. A run by hand (`libexec/cycle:8-10`)
keeps its records with `--by-hand`: `how = 'hand'`, `started_by` NULL, no
approval used. Without that, its history, park, park question and quiet-cycle
bound would be lost. **Schema and spec change:** `cycles.how` CHECK gains
`hand`, `started_by` becomes nullable (3.3), and the row in section 4.

**D13. `park --comment-id` is optional.** The park is recorded before the
question is posted, and the post can fail (`cycle:1048-1080`). The id goes on
record by `question-asked` once posted. The spec's form still works. **Spec
change:** the row's brackets.

**D14. Park reason and the park file.** `.park` holds `{cap, reason,
provider, retry, started}`; `parks` has `reason, step, pr, retries`.
`parks.reason` is `provider-outage`, the cap or the status; `retry` and
`started` are derived (`park-show`). The runner's free reason text is not
stored (3.14). No spec change.

**D15. `--status` outside the set.** Spec 3.3 says a value outside the set is
a verb error, and that the end is always committed. The word comes from the
runner's result (`lib/headless.sh:308`, `.status` passed through), which
tehas does not control, so a 64 there would lose an end. A word outside the
set is `end_reason = 'unknown'` and is kept as given in history and endings;
a test pins that every word `libexec/cycle` itself assigns is in the set.
**Spec change:** 3.3's sentence.

**D16. The issues stub.** Every record table references `issues`, and nothing
writes `issues` before #253. The stub of section 0 (`kind 'unreadable'`,
`kind_at 0`, `checked_at 0`), inside the verb's own transaction, never over a
real row. **Spec change:** 3.2: "`kind_at` 0 is a row no read has filled yet".

**D17. Quiet episodes.** The spec's `cycle-stop-asked <issue>` has no episode
key. The episode is the issue's `quiet` note; `cycle-stop-asked` counts from 1
when its last ask is older than the note, and `--said-only` records the "asks
no more" line's time. `cycle-note` writes only `cycle_notes`. **Spec change:**
the two rows.

**D18. Plan history file.** The spec (1.4, 3.8) names
`state/plan-<n>.history.jsonl`; the writer is `tehas_cycle_history_add
"plan-$n"` (`planning.sh:186`), which writes
`state/cycles/plan-<n>.history.jsonl`. No verb is affected; the importer reads
the second path. **Spec change:** the path.

**D19. Rust version.** `File::try_lock` needs 1.89; `Cargo.toml` says 1.80.
Raised to 1.89; `host/versions.env` pins 1.99. No spec change.

**D20. `plan-end` and `plan-begin` modes.** Kept from the spec: `--outcome`,
`--rc N` (a number), `--session`, `--turn` (now required, a check). Added:
`--option` (the labels the next question offers, which `options_json` needs),
`--out` (`pending_outcome`, #251), `--posted` (`plan_post` step three), `--close`
(`plan_end`), and `--undo` on `plan-begin` and `plan-answer` (a turn whose label
could not be set leaves the record as it was, `planning.sh:393-398`). `--filed`
is a repeatable URL instead of `1,2,3` (D23). The outcome set gains `lost`
(B8). **Spec change:** the three rows.

**D21. `approval-record` additions.** `--at` (the key's second half), `--undo
--at` (`restore_record`), exit 1 `used` (A3), and `--label` only for `label`
and `fixer`, the `how` word stored for the other three. **Spec change:** the
row; 3.4's `label` comment: "the tehas:* label it rests on, or the how word
for a start no label made".

**D22. `cycle-end --refused` and `--stopped`.** A cycle that refuses its
number after `cycle-start` (`cycle:254`, `:262`) and a cycle killed outright
that `tehas stop` clears both leave an open row nothing else ends. An ended row
with `end_reason` NULL is a start refused before it ran. **Spec change:** a
line under 3.3 and the row.

**D23. `filed` keeps URLs.** `plan_filed` and the history line carry full URLs
of any `owner/repo` (`planning.sh:111-116`, `:183-194`); numbers alone would
rebuild the wrong URL for an issue filed elsewhere and change `tehas history`'s
`filed`. `planning_filed` gains `url`, keyed `(issue, url)`; `filed_issue` is
the number from the URL; `plan_history.filed_json` holds the URLs. **Schema
and spec change:** 3.5, 3.8.

**D24. Settings keys.** Spec 3.6 and 3.14 say `cycle_settings` holds
`PI_ENSEMBLE_*` names. `.settings` today also holds the host's `TEHAS_*`
values and `concurrentCycles`, which the improvement record reads
(`improve.sh:87-94`). All three are kept, and no value of a secret shape is
stored. **Spec change:** the two sentences.

**D25. `cycle_progress.written_at` is a file's mtime.** G3 compares the
progress's time with `started_at`, and the carrier file `work` writes has no
time inside it (`tehas_headless_progress`, `lib/headless.sh:280-290`). The
mtime is read once, at the boundary. **Schema comment and spec change:** 3.6
names the exception.

**D26. Schema 0.** A file at `user_version` 0 is `unreadable` with its own
detail. No spec change.

**D27. The answer text.** #251 says "the answer and the brief are rows";
`planning_answers` has no text column, and `libexec/plan` reads the answer
from a file (`--answer <file>`, `planning.sh:411-427`). The `.answer` file
stays in this unit; #251 adds a column if it wants the row. No spec change.

**D28. `conflict`.** A constraint failure is exit 1 `conflict`, not 78. **Spec
change:** the common rules name the word.

D29 on were added by the plan for the callers
(`docs/plans/2026-10-24-store-callers.md`, unit 1), after the verbs above were
on main.

**D29. Schema 2 (sign-off).** Schema 1 is on main and `install.sh` writes it,
so the additions below are a real step, not an edit of schema 1. `db::migrate`
steps from 1 to 2 in one transaction with `meta.schema` and `user_version`:
`approvals.input_json TEXT`, `parks.told_at INTEGER`, `planning_answers.text
TEXT`, and the table `planning_notes (issue, kind, comment_id, at)` keyed
`(issue, kind, comment_id)`, referencing `planning (issue)` ON DELETE CASCADE,
`kind` in `fixer-refused`, `held`. ADD COLUMN and CREATE TABLE lose no row. A
store at 0 still goes straight to 2. Each added column comes last in its table
in `schema.sql` too, so a migrated store and one `init` made hold the same
columns, which a test pins. The data verbs require 2; a store at 1 is 78
`unreadable` until it is migrated. `check` reports a store at 1 as it is,
without the tables it lacks. **Spec change:** section 3's DDL and the version
line.

**D30. The approval's inputs are a column (sign-off).** D2 left the choice to
#250. `approval-record --input-file` stores them in `approvals.input_json`, so
the inputs and their approval are written, undone and pruned in one
transaction, and no file can belong to another approval. The object may hold
only `links`, `comment_links`, `retry_context` and `refresh_of`; the readers
in `lib/attachments.sh` still check the entries. The rest of today's file
(`rests_on`, `fixer_starts`, `refreshes`, `title_sha256`) comes from
`approval-current`. **Spec change:** 3.4's DDL.

**D31. `history-show` (sign-off).** No verb read `cycle_history` or
`plan_history`, and three readers need them (`libexec/history`,
`tehas_cycle_history`, the analytics run). A read verb over both, with the
filters those readers use. **Spec change:** a row in section 4's table.

**D32. The park's tell is a column (sign-off).** `park_holds` writes `.held =
true` into `.park` once it has said the hold, and `cycle-note` refuses an
ended cycle by design. `parks.told_at` and `park-told` replace it, and
`park-show` gives `told_at` and the `body_sha256` the hold compares (today
copied into `.park`; now the used approval's, which binds the same text).
**Spec change:** 3.7's DDL and a row in section 4.

**D33. Planning notes (sign-off).** The planning record's `fixer_refused` and
`held` lists had no column. `planning_notes` holds them, bounded by the
session (B4), and `plan-note` writes one. **Spec change:** 3.5's DDL and a row
in section 4.

**D34. The answer text is a row (sign-off).** Replaces D27: #251 says "the
answer and the brief are rows". `plan-answer --text-file` stores the reply in
`planning_answers.text` (64 KiB), and `plan-show` returns the answer of the
running or pending turn. The `.answer` file goes when #251's caller moves.
**Spec change:** 3.5's DDL.

**D35. A test clock.** A build with debug assertions takes `now` from
`TEHAS_STORE_TEST_NOW` when it is set (a Unix time; anything else is 78), the
same gate as `test-write`. `install.sh` builds with `--release`, which never
reads it. The shell tests use it instead of `touch -d`. No spec change.

## 8. Rejected critique

- **Spec fidelity 5, "do not add `failure_cause`, `failure_kind`, `cap`; read
  them from `cycle_results.payload`".** Rejected. `tehas history --json`
  publishes them and promises never to remove a field
  (`libexec/history:150-152`); `failure_kind` and `failure_cause` come from
  the progress file, not the result (`libexec/cycle:797-802`); and the payload
  is bounded to the latest attempt (accepted item 6), so older lines would
  lose them. Accepted in part: the values are cleaned to the cycle's own sets
  and bounds (`failure_cause` is a 60-byte word, not 200 bytes of text), and
  spec 3.8 and its convention line change in the same PR (D1).
- **Spec fidelity 7, "on `--never-ran` delete only the row with `used_by =
  attempt`".** Rejected. Today `rm -f .approved.json` leaves the fixer nothing
  to rest on; deleting only that row would make the previous approval the base
  again, which is wider. The counters going to 0 does no harm, because with no
  base the fixer starts nothing. Accepted: unused rows are kept, and the
  retention rule bounds the table (D11).
- **Spec fidelity 11, "a status outside the set is exit 64".** Rejected: the
  word is the runner's (`lib/headless.sh:308`), and a 64 commits nothing, which
  breaks "the end of a cycle is always committed" in the same paragraph.
  Accepted: the test that every word tehas assigns is in the set, and the spec
  sentence changes (D15).
- **Spec fidelity 12, "follow the spec: no lock file is `probe_rc 2`".**
  Rejected as stated: every issue that never ran would hold its caller.
  Narrowed instead (an open row with no file is `probe_rc 2`), the ways a row
  could stay open by accident are closed (`--refused`, `--stopped`), and the
  rule is marked for the owner's sign-off (D10).
- **Spec fidelity 14, "drop `question-answered`".** Rejected: the listed verbs
  cannot record an answer to a park question or a withdrawal, and without one
  `question-known` is wider than today (F5). Accepted: no undefined meaning; a
  withdrawal deletes the row (D4).
- **Spec fidelity 17, second half, "one grammar for both park reasons".**
  Rejected: they are two fields. `cycle-end --park-reason` is the runner's
  `parkReason` (`[a-z-]`, 40, `libexec/cycle:786`), published as `parkReason`;
  `park --reason` is tehas's own word for the park (`provider-outage`, the cap
  or the status). Accepted: `park --reason` now takes the cap's grammar, which
  covers all three, and the `taken` check is the same as `question-asked`.
- **Spec fidelity 4, "put `--image`, `--settings`, `--rukas` on
  `cycle-start`".** Not taken; the critic's second option is: D3 is marked for
  sign-off and spec sections 2 and 4 change in the same PR. Also rejected:
  limiting the keys to `PI_ENSEMBLE_*` (the improvement record reads the host
  half; the spec text changes instead, D24).
- **Spec fidelity 19, "take `written_at` from the payload".** The payload has
  no time; the critic's second option is taken (D25).
- **Failure modes 12, first half, "`cycle-start` retried after a lost answer
  exits 0 `already`".** Rejected: nothing the caller holds before the answer
  tells its own retry from a later start of the same issue over a row a killed
  cycle left open, and adopting that row would start a cycle on no approval.
  Such a row is replaced by the next start (`replaced_unended`) and ended by
  `--stopped` or #252's sweep. Accepted for `cycle-end` (the attempt names it).
- **Failure modes 8, "set the busy timeout in verbs.rs, db.rs unchanged".**
  Not possible as stated: `open_read_only` runs its pragmas, which can meet the
  WAL cleanup lock, before a caller could set a timeout. The fix is one
  additive function in db.rs (`open_look`).
- **Caller fit 2, "or order by insertion (`rowid`)".** The first option is
  taken (one unused row, `current` by `used_by`); `approvals` is keyed `(issue,
  at)` and a `rowid` order would still mix with the dispatcher's give-ups.

Everything else in the three reviews was right and is applied above:
spec fidelity 1, 2, 3, 6, 8, 9, 10, 13, 15, 16, 18; caller fit 1, 3-15; failure
modes 1-7, 9-11 and the second half of 12.

## 9. Notes for the implementer

- `db::open`/`prepare` run `PRAGMA quick_check` on every connection. The poll
  will call `cycle-show` and `question-known` several times a poll once callers
  move; the cost grows with the file. Not changed in this unit; one line in the
  PR.
- Do not touch `src/tests.rs`. `cargo fmt`, `cargo clippy --locked
  --all-targets -- -D warnings` clean, no blanket allows, then
  `scripts/verify.sh`.
- The spec changes listed in section 7 go in the same PR, in their own commit.
- On failure nothing is printed on stdout; the one stderr line starts
  `store: <word>`.
