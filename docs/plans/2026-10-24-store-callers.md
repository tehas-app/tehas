# Plan: the shell callers move onto the state store (#250, #251)

Base: `epic-244-finish` at `4e638a7`. Every `file:line` here is from that tree
and was read for this plan. The verbs spec
(`docs/specs/2026-10-23-store-verbs.md`) quotes older line numbers in places;
read the code before an edit, not the number.

This plan was made from three designs: a risk-first one, a tests-first one and a
minimal-diff one. The risk-first design is the base: it is the one that is
strict about "could not look" never reading as "nothing on record". From the
tests-first design come the test harness, the behavioural "no old file
written" check and the per-poll caches. From the minimal-diff design come the
analytics facts unit, the view helpers that keep `lib/attachments.sh`,
`lib/sizing.sh` and `lib/fixer.sh` unchanged, the cycle's exit statuses and
the order "park question on the store, plan question still on the file" inside
the park unit. Section 9 lists what was rejected and why.

## 0. Findings that shape the plan

1. **Nothing in bash calls a record verb.** `grep -rn 'libexec/store' lib
   libexec bin` finds nothing; only `install.sh:1217` runs the store
   (`init`/`migrate`).
2. **Schema 1 is written by `install.sh` on main, but not released.** The
   CHANGELOG has a `v0.11.0` section (`CHANGELOG.md:21`) and no tag exists
   (`git ls-remote --tags origin` ends at `v0.10.5`). A dev host that ran
   main's `install.sh` has a schema 1 `tehas.db`. So the schema additions below
   are a real 1 to 2 step in `db::migrate` (`console/store/src/db.rs:689-722`
   knows only 0 to 1), additive only. That is correct whether or not v0.11.0 is
   tagged before the epic ends.
3. **The analytics program reads the record files itself.**
   `console/analytics/src/run.rs:7-33` reads `history.jsonl`, `result.json`,
   `progress.json`, `.settings`, `.approved.json` and `.rukas`;
   `console/analytics/src/plan.rs:36,53` reads `plan-<n>.json` and its `.out`.
   `run.rs:12-13`: "The run is not recorded without [the history line]". If the
   writer moves and this reader does not, every run silently stops being
   recorded. Unit 3 fixes the interface first.
4. **No verb reads history.** `cycle-end` and `plan-end` write `cycle_history`
   and `plan_history`. The readers have nothing to call:
   `libexec/history:96-101`, `tehas_cycle_history` (`lib/fixer.sh:158-162`,
   used by `lib/refresh.sh:83`), and the analytics run.
5. **Plan history for one issue is never listed today (D18).** The writer is
   `tehas_cycle_history_add "plan-$n"` (`lib/planning.sh:186`), which writes
   `state/cycles/plan-<n>.history.jsonl` (`lib/fixer.sh:151`). `tehas history
   <n>` reads `state/plan-<n>.history.jsonl` (`libexec/history:98`). The
   listing of all issues finds the plan lines only because its
   `state/cycles/*.history.jsonl` glob matches them. Unit 4 fixes the path,
   unit 7 moves the lines to the store.
6. **Records with no column.**
   - The approval's inputs: `links`, `comment_links`, `retry_context`,
     `refresh_of` (`libexec/dispatch:1306-1314`). Verbs spec D2 leaves the
     choice to #250.
   - The park's "told once" mark: `park_holds` writes `.held = true` into
     `.park` (`libexec/dispatch:1799-1805`). `parks` has no such column, and
     `cycle-note` refuses an ended cycle.
   - The planning record's `fixer_refused` and `held` id lists
     (`libexec/dispatch:1191-1194`, `:1209-1212`).
   - The answer text. #251 says "the answer and the brief are rows"; verbs spec
     D27 left it to #251.
7. **A dead read.** `libexec/cycle:481-482` reads `.branch`, but the cycle's own
   start removes it (`:367`) and its only writer is `:1134`, at the end. Drop
   the read; the branch goes to `cycle-end --branch`.
8. **"No record" is the permissive branch in several places.** A failed store
   read must never land there:
   - `libexec/work:540-546` and `:603-612`: no record file means a run by hand,
     which skips the text check and the labeller check.
   - `libexec/dispatch:2618-2633`: no question id on record. The poll then
     treats a ticked box as one in a question this host has no record of, and
     unticks a person's real answer.
   - `[ -e "$(plan_state "$n")" ]` at `libexec/dispatch:1704`, `:1833`,
     `:1865`, `:2389`, `:2983`, `:3026`, `:3041`, `:3089`: an open session
     blocks a start (A13).
9. **The gate already needs cargo** (`scripts/verify.sh:12-13`), but AGENTS.md
   rule 6 says the shell tests need bash, git, jq and shellcheck only. After
   unit 4 most dispatch, cycle and work tests need the real store, which needs
   cargo. This is the owner's to settle (rule 7); each unit's pull request
   text says so.
10. **`libexec/dev`** (named in #250) reads only liveness
    (`libexec/dev:312-313`, `tehas_cycles_running`). Liveness stays a file
    probe (section 1, decision 5), so dev does not change.

## 1. Decisions

| # | Decision | Why |
| --- | --- | --- |
| 1 | One wrapper, `lib/store.sh`, runs `$TEHAS_HOME/libexec/store`. No environment override. | The binary that matches the database is the installed one. Tests install the real store at that path, and a failing store is a wrapper script at the same path (section 3). No new seam on a host. |
| 2 | One schema step 1 to 2, all in unit 1: `approvals.input_json`, `parks.told_at`, `planning_answers.text`, and a table `planning_notes`. | Finding 2. ADD COLUMN and CREATE TABLE cannot lose rows. The inputs live with their approval, so `--undo` and retention stay one transaction (no file that can belong to another approval). |
| 3 | New read verb `history-show`, and `park-told`, `plan-note`, `plan-answer --text-file`, `approval-record --input-file`, all in unit 1. | Findings 4 and 6. |
| 4 | Carriers stay files. work's `.runner_exit .status .oom .forks .stalled .timed .nostart .result.json .progress.json`, the dispatcher's `.done`, stop's `.stopped(.new)`, the container inputs `.cid .manifest.json .images.md .retry-context.md`, planning's `.out .rc .lock` and `.keep/`, and `state/refined.jsonl` (D9). `.image` and `.settings` go: work calls `cycle-image`. `state/park-<n>.json` becomes a `mktemp` file. | Each is a signal between two processes, a mount, or a container's output. `result.json` and `progress.json` stay until the next start, as today (`libexec/cycle:365`), so `tehas export` keeps copying them. |
| 5 | Liveness stays a file probe. `tehas_cycle_running`, `tehas_cycle_lock_free` and `tehas_cycles_running` (`lib/cycles.sh:42-63`) are unchanged. `cycle-show` is called only where a record is needed: the attempt, `started_at`, `started_by`, `requeued`, the image and settings. | The lock is the truth (C5) and is group A, a file. The store's probe is the same `flock`. Routing `drain`, `index`, `clean`, `tidy` and `dev` through the store adds busy and 78 failures to paths that need no record. **Owner sign-off**: the verbs spec says `cycle-show` replaces these readers. |
| 6 | The poll reads `cycle-show --running` once per poll for `started_at` (quiet loop, done loop) and `plan-show --open` once per poll for the A13 set and the three planning loops. | One process each, instead of a call per issue per rule. A failed list read ends the poll before it acts (A14, D2). |
| 7 | A cycle is "from the unit" when `TEHAS_CYCLE_FROM=unit` (set by `host/systemd/tehas-cycle@.service`) **or** `INVOCATION_ID` is set. Otherwise it runs `cycle-start --by-hand`. From the unit, a refused `cycle-start` stops the cycle; it never falls back to a run by hand. | A by-hand run skips the approval, so the guess must lean towards "unit". A unit file not yet refreshed by install still sets `INVOCATION_ID`. |
| 8 | The cycle's exits when the store will not start it: 75 `busy` or `timeout` (after three tries), 77 `unapproved`, `unlocked` or `probe`, 78 the store is unusable. All before any label, comment or container. | Distinct statuses tell a person what to do. The dispatcher's wait then times out, finds the lock free and gives up the start as today (`libexec/dispatch:1631-1636`). |
| 9 | analytics takes its facts from the caller (`--facts <file>`), built by `lib/analytics.sh`. It never opens `tehas.db`. | analytics stays out of the store's schema. Unit 3 makes the change with facts built from today's files, so main stays consistent. |
| 10 | View helpers rebuild today's JSON shapes from the store into a private temp file: `tehas_approval_view` (the `.approved.json` shape) and `plan_view` (the `plan-<n>.json` shape). | `lib/attachments.sh:115-200`, `lib/sizing.sh:136-164`, `tehas_fixer_base`, `tehas_fixer_starts`, `tehas_refresh_count` and `tehas_fixer_answer` keep their file interface and their tests. |
| 11 | The importer is separate work (section 6). Units do not import. `install.sh` runs `store import` after `store migrate`, refuses while a cycle or planning turn runs and old records exist, and holds `state/dispatch.lock` from the check to the import. The last unit adds a poll guard that stops at 78 on any old record file. | A cycle running across the deploy runs the old `libexec/cycle` (install replaces files by rename, `install.sh:582-583`) and would write old files after the import. |
| 12 | The test clock: under `#[cfg(debug_assertions)]` only, the store's `now()` (`console/store/src/main.rs:345`) reads `TEHAS_STORE_TEST_NOW` when set. | Same gating as the test-only `test-write` verb (`main.rs:103`, `:199`, `:287`). `install.sh` builds with `--release` (`install.sh:782`), which never reads it. Replaces `touch -d` on `.taken` in the tests. |

## 2. The wrapper, `lib/store.sh`

```bash
# shellcheck shell=bash
# The state store, for the shell commands: $TEHAS_HOME/libexec/store
# (console/store). One call runs one verb. Sourcing this file does nothing,
# and no function here exits (lib/ is sourced by login shells, rule 3).
#
#   tehas_store <verb> [args...]
#       Runs the store once with fd 8 closed, stdin from /dev/null, bounded by
#       timeout -k 2 $tehas_store_bound (20 s). Sets:
#         tehas_store_out   the one JSON line, on 0
#         tehas_store_word  the store's word on failure (busy, conflict,
#                           unapproved, used, missing, corrupt, ...), or ours:
#                           timeout, absent, garbled
#         tehas_store_line  the store's stderr line on failure, cut to 300 bytes
#       Returns the store's status, or ours:
#         0   answered: one line, .api == 1, .ok == true, .verb == <verb>
#         1   refused, busy, conflict, or timeout (word timeout)
#         64  usage: a bug in the caller
#         78  unusable: the store's 78, a binary that is missing or not
#             executable (word absent), or a 0 whose line is not the shape
#             above (word garbled)
#   tehas_store_get <jq filter>   jq -r of $tehas_store_out; only after a 0
#   tehas_store_try <tries> <verb> [args...]
#       tehas_store again while the word is busy or timeout, 1 s apart
#   tehas_store_looked            0 when the last call answered (returned 0)
#   tehas_store_ready             0 when the binary is executable and
#                                 state/tehas.db is a regular file and not a
#                                 link: two tests, no process
```

- It captures `out=$(... 2>&1)`. The store prints nothing on stderr on success
  and nothing on stdout on failure (verbs spec section 0, "Output"), so the
  status decides which stream `out` holds. A 0 whose line fails `jq -e '.api
  == 1 and .ok == true and .verb == $v'` is 78 `garbled`: fail closed.
- `timeout` exit 124 or 137 becomes 1 with the word `timeout`: "could not look",
  like `busy`.
- It never logs. The caller logs in its own voice (the poll's `log`, work's
  stderr), so log formats do not change.
- **The rule for readers.** A caller reads a field only after a 0, and reads
  JSON `null` as "none on record". After 1, 64 or 78 it never uses an empty
  value: it takes its fail-closed branch. One helper per read enforces this,
  for example:

  ```bash
  # tehas_cycle_attempt <issue>: sets tehas_cycle_attempt_is to the attempt on
  # record, 0 when none. Returns 1 when it could not look, and then the value
  # is not set. Called in the caller's shell, never in $(...), so
  # tehas_store_word stays readable.
  tehas_cycle_attempt() {
    tehas_cycle_attempt_is=""
    tehas_store cycle-show "$1" || return 1
    tehas_cycle_attempt_is=$(tehas_store_get '.cycle.attempt // 0')
  }
  ```

- **Preflight.** Each command that calls the store checks `tehas_store_ready`
  right after `tehas_load` and before any GitHub, Docker or git call, and exits
  78 with `ERROR: the state store is not installed (run install.sh)` when it
  fails: `dispatch`, `cycle`, `stop`, `history`, `export`, `health`, and `work`
  when `TEHAS_CYCLE_ATTEMPT` is set or it reads an approval. The first real verb
  proves the schema; its 78 is handled per caller (section 4).
- The store reads `TEHAS_HOME`. Nothing here is passed to a container: the
  container's `-e` list in `libexec/work` is explicit, the store is not
  mounted, and `test/work_test.sh` must not change (AGENTS.md rule 4).
  `TEHAS_CYCLE_ATTEMPT` is work's own environment, not the container's.

### The view helpers (also in `lib/store.sh`)

- `tehas_approval_view <issue> [<attempt>]`: runs `approval-current`. Prints
  the path of a 0600 temp file under `$TEHAS_HOME/tmp` (made 0700) holding
  today's `.approved.json` shape: `{at: ISO(current.at), by, how, body_sha256,
  title_sha256?, links, comment_links?, retry_context?, refresh_of?, rests_on:
  {at: ISO, by, how, body_sha256}, fixer_starts, refreshes}`, the inputs taken
  from `current.input`. With `<attempt>`, `current` counts only when
  `current.used_by == attempt`. Returns 0 with a path, 1 when there is no such
  `current` (none on record), 2 when the store could not answer. The caller
  removes the file.
- `plan_view <issue>` (unit 7): today's `plan-<n>.json` shape from `plan-show`
  (`started_at` to ISO `startedAt`, `comment_id` to `comment`, `options`,
  `answers`, `filed`, `fixer_refused` and `held` from the notes). Same returns.

## 3. Test harness

Lands in unit 2, used by every later unit.

- **Build once per run.** `scripts/test.sh` builds the store before it forks
  the test files: `cargo build --locked --offline --quiet -p store
  --target-dir console/target/shell`, and exports `T_STORE_BIN`. A separate
  target directory, because the gate's `cargo test` holds `console/target`'s
  lock (`scripts/verify.sh:53-56`). `t_store` (`test/lib.sh:198-210`) copies
  `$T_STORE_BIN` when it is set, else builds as now (into the same separate
  directory), so a test file still runs alone. `scripts/test.sh` is not on
  rule 7's list, but it is next to the gate: the pull request says so. Without
  cargo, `scripts/test.sh` builds nothing and `t_store` reports why.
- **`t_store_home <home>`**: `t_store || t_fail "$t_store_why"`, then install
  the binary at `<home>/libexec/store` and run `t_store_db <home>`. A record
  test fails, not skips, without the store: the gate needs cargo anyway.
  `test/dispatch_lib.sh` `reset` (`:283` removes `libexec`), `test/cycle_lib.sh`
  (`:150`) and `test/work_lib.sh` call it after their clean-up.
- **`t_row <verb> <args...> -- <jq>`**: runs the installed store and prints the
  jq result. Reads go through read verbs only; no SQL in shell tests.
- **`t_at <epoch> <verb> <args...>`**: runs the store with
  `TEHAS_STORE_TEST_NOW` (decision 12). It replaces `touch -d` on `.taken`
  (`test/dispatch_sweep_test.sh:321` and its kind).
- **`t_store_answers <verb> <rc> <line> [<times>]`**: puts a wrapper script at
  `<home>/libexec/store` that answers that verb with that status and line (for
  the first `<times>` calls, default all) and runs the real binary for every
  other call. This is how busy, 78, garbled and a hang are tested from bash.
- **The fake cycle unit** (`test/dispatch_lib.sh:143-148`) stops writing
  `.taken`. It takes the issue lock in a subshell and runs the real
  `cycle-start` under it. `start-silent` still skips it. The same goes for
  `hold_proc` and the other holders that make a running cycle
  (`grep -n 'taken' test/dispatch_*`).
- **The at-mark snapshots** (`test/dispatch_lib.sh:121`, `:123`) record
  `approval-current <n>` and `plan-show <n>` from the store at the moment the
  label is added, so A2's "on record before the label" cases keep their
  meaning with new jq paths.
- **`t_no_old_state <home>`**: fails when an old record file exists. Called
  from the `t_done` hook of `dispatch_lib.sh`, `cycle_lib.sh` and `work_lib.sh`,
  so every case in those files checks it. Its list starts empty in unit 2; each
  record unit adds the names it moved.
- **`test/store_paths_test.sh`** (the grep test, modelled on
  `test/internals_test.sh`): greps `git ls-files bin libexec lib install.sh
  console/analytics/src` for forbidden path tokens. Each pattern is proved
  against a sample line, as `internals_test.sh` does. It keeps an allow list of
  the carriers of decision 4, each with its reason, and a case that the carrier
  names appear only in their allowed files (work's carriers in `libexec/work`,
  `libexec/cycle`, `libexec/export`, `libexec/stop`). The forbidden list starts
  empty in unit 2; each record unit adds its names, so a regression is caught
  from the unit that moved them.

## 3a. Failure rules per call site

The caller applies these; the wrapper decides nothing.

| Call site | 1 busy or timeout | 1 with a precondition word | 64 or 78 |
| --- | --- | --- | --- |
| dispatch `approval-record` (before the label) | no label, `failed=1`, next poll | `used`: log, skip this issue this poll (A3) | ERROR; exit 78 if nothing was done this poll, else stop starting and exit 1 |
| dispatch `cycle-show` for `before` (before the label) | no start, the approval is undone | n/a | same as busy |
| dispatch wait for the start | keep looking until the deadline, then the lock fallback | n/a | stop looking, lock fallback |
| dispatch `approval-record --undo` | WARN, as `restore_record` does today (`libexec/dispatch:1438`): an unused row stays and is replaced by the next record | `undone:false` is fine | WARN |
| dispatch reads for the question id, A13, a park hold, rule 4a | skip the issue (`continue`): never untick, never start | n/a | ERROR, exit 78 |
| dispatch list reads (`cycle-show --running`, `plan-show --open`, `park-show --automatic`) | ERROR, exit 1 before acting | n/a | ERROR, exit 78 |
| dispatch `park-retry`, `question-answered --withdraw` at a start (before the label) | no start | `unparked`, `unknown`: done | no start, ERROR |
| dispatch `cycle-note`, `cycle-stop-asked` | ask nothing, stop nothing | `ended`, `nocycle`: skip | same |
| cycle `cycle-start` | `tehas_store_try 3`, then exit 75 | exit 77 | exit 78 |
| cycle `cycle-end` (`--refused`, the report, `--never-ran`) | `tehas_store_try 5` within `TimeoutStopSec`; then ERROR with the whole argument line, the row stays open, the next `cycle-start` replaces it (`replaced_unended`) | `attempt`, `nocycle`: ERROR | ERROR, the cycle's exit is unchanged |
| cycle `park` | no boxes: the plain comment (`libexec/cycle:1078-1088`); a question not on record is never offered (F5) | same | same |
| cycle `question-asked` | WARN "its boxes will not work" (`libexec/cycle:1070-1071`) | `taken`: same | same |
| work `approval-current` (with an attempt) | exit 4, "the record could not be read" (work's header, `:66-73`); never the by-hand branch | n/a | exit 4 |
| work `cycle-image` | WARN; the start goes on (today `:765` also only warns) | `attempt`, `ended`: exit 4 | exit 4 |
| stop | the lock file is not removed; the record step is named on stderr; exit 1; run it again | `running`: as "still holds its lock" | same |
| `plan-begin`, `plan-answer` (before the label) | no label, no turn | `running`, `turn`: skip, next poll | ERROR |
| `plan-end` (turn end) | the `.rc` file is kept, so `plan_collect` runs again next poll | `turn`: ERROR, keep `.rc` | same |
| `history`, `export`, `health` | exit 1, print nothing (never an empty list) | n/a | exit 78 |

## 4. Units, in order

Each unit is one branch `store-callers-<name>`, one pull request, with ordered
commits. Inside a record unit the commits need not each be green (an approval
written by one verb is read after a start by another); run the affected tests
while iterating and the fast gate once on the unit's final state. Each record
unit moves every writer and every reader of its group, adds its names to
`test/store_paths_test.sh` and `t_no_old_state`, changes the manual pages that
name its files in the same commit as the behaviour
(`grep -nE '<its names>' host/docs/*.md`), and adds a CHANGELOG `Unreleased`
line.

Order: 1 `store-v2`, 2 `wrapper`, 3 `analytics-facts`, 4 `cycle-records`,
5 `park-records`, 6 `quiet-notes`, 7 `plan-records`, 8 `old-paths`. Units 1, 2
and 3 do not depend on each other. Unit 4 needs all three. Units 5 and 6 need 4.
Unit 7 needs 4 and 5 (the shared question lookup). Unit 8 needs 4 to 7 and the
importer.

### Unit 1: `store-v2` (Rust only, no caller)

Files: `console/store/src/schema.sql`, `db.rs`, `main.rs`, `verbs.rs`,
`cycle.rs`, `park.rs`, `approval.rs`, `plan.rs`, new `history.rs` and
`history_tests.rs`, the `*_tests.rs` beside each;
`docs/specs/2026-10-23-store-verbs.md` section 7 (new items D29 on, sign-off
marked); `docs/specs/2026-10-21-state-store.md` section 3 DDL.

- `SCHEMA_VERSION = 2`. `schema.sql` is the whole v2 schema (`init` writes it).
  `db::migrate` gains the step 1 to 2, in one transaction with the
  `user_version` and `meta.schema` update:
  - `ALTER TABLE approvals ADD COLUMN input_json TEXT`
  - `ALTER TABLE parks ADD COLUMN told_at INTEGER`
  - `ALTER TABLE planning_answers ADD COLUMN text TEXT`
  - `CREATE TABLE planning_notes (issue INTEGER NOT NULL REFERENCES planning
    (issue) ON DELETE CASCADE, kind TEXT NOT NULL CHECK (kind IN
    ('fixer-refused','held')), comment_id INTEGER NOT NULL, at INTEGER NOT
    NULL, PRIMARY KEY (issue, kind, comment_id))`
  - From 0, `migrate` still runs `write_schema` (straight to 2). The data verbs'
    check (`verbs.rs:463-465`) follows `SCHEMA_VERSION`.
- `approval-record ... --input-file <abs path>`: read before the lock like the
  other file arguments (regular file, same dev and inode, 1 MiB), must parse as
  a JSON object holding only `links`, `comment_links`, `retry_context`,
  `refresh_of`; stored in `input_json`. `approval-current` returns
  `current.input` and `approval.input` (the parsed object or `null`).
- `history-show [<issue>] [--kind cycle|plan] [--how <how word>]`: read only
  (`open_look`). `"rows":[...]` oldest first (by `at`, then issue, then id),
  every column of `cycle_history` or `plan_history` plus `"kind"`, plan `filed`
  as a list of URLs. `--how` filters cycle rows (used for "a refresh ever ran
  here").
- `park-told <issue>`: sets `told_at = now` on the current park; JSON
  `"said":true` the first time, `false` when already set; `nopark` exit 1 when
  there is none. `park-show` returns `told_at` and `body_sha256` (from the used
  approval whose `used_by == park.attempt`, else `null`).
- `plan-note <issue> --kind fixer-refused|held --id <comment id>`: `"said":true`
  when the note is new. `plan-show` returns `"notes":{"fixer_refused":[...],
  "held":[...]}`. A replaced or closed session cascades.
- `plan-answer ... --text-file <abs path>` (64 KiB, read before the lock):
  stored in `planning_answers.text`. `plan-show` returns `"answer"`: the text of
  the answer of the turn that is running or pending, else `null`.
- The test clock (decision 12), with a Rust test that it is read in a debug
  build.
- Tests: migration 1 to 2 keeps every row of a schema 1 file (made by the v1
  DDL kept as a test fixture); `init` writes 2; each new verb and flag
  (refusals, 64s, a link as `--input-file`, the bounds); `approval-record
  --undo` removes the inputs with the row; `history-show` order and filters;
  `park-told` once; `plan-note` cascade on `plan-begin` replacing a session.
- Invariants: B4 (notes bounded by the session), D2 (readers exit 1 `busy`, never
  an empty answer).
- Manual: none (no caller). CHANGELOG: the schema step.
- `cargo fmt`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test
  -p store`.

### Unit 2: `wrapper` (no record moves)

Files: `lib/store.sh` (new, section 2, without `plan_view` which unit 7 adds);
`test/store_lib_test.sh` (new); `test/lib.sh` (section 3 helpers);
`scripts/test.sh` (the build once per run); `test/dispatch_lib.sh`,
`test/cycle_lib.sh`, `test/work_lib.sh` (`t_store_home` in their resets, the
`t_no_old_state` hook with an empty list); `test/store_paths_test.sh` (the
frame, empty forbidden list, the carrier allow list).

- Tests in `test/store_lib_test.sh`, each through `t_store_answers` or a stub
  binary: 0 with a good line; 0 with a non-JSON line (78 `garbled`); 0 with
  `.ok == false` (78); 1 `busy`; 1 `conflict`; 64; 78; a missing binary (78
  `absent`); a hang past the bound (1 `timeout`, using a small
  `tehas_store_bound`); `tehas_store_try` retries busy and stops on `used`;
  fd 8 is closed in the store's process; `tehas_store_ready` refuses a link
  and a directory; sourcing the file runs nothing. `tehas_approval_view` against
  the real store: no `current` returns 1, a store answering busy returns 2 and
  prints nothing, the shape matches a fixture `.approved.json` field for field
  (`at` as ISO).
- Invariant: D2.
- Manual: `host/docs/tools.md` under `scripts/test.sh` (the store build).

### Unit 3: `analytics-facts` (no behaviour change)

Files: `console/analytics/src/run.rs`, `plan.rs`, `main.rs` (or wherever the
subcommands parse), its tests; `lib/analytics.sh`; `libexec/cycle:1265`;
`lib/planning.sh:256`; `test/analytics_test.sh`.

- `analytics run <issue> --facts <file>`: `{line: <the history line as
  today>, attempt, approval: {at: ISO, how}|null, settings: {...}|null, rukas:
  "<commit>"|null}`. `analytics plan <issue> --facts <file>`: today's
  `plan-<n>.json` object. The file is read like the others (regular, no link,
  bounded). `run.rs` and `plan.rs` take these facts instead of reading
  `history.jsonl`, `approved.json`, `.settings`, `.rukas` and `plan-<n>.json`.
  `result.json`, `progress.json`, `plan-<n>.json.out` and the logs are still
  read from disk: they are carriers.
- `lib/analytics.sh` builds the facts **from today's files** in this unit.
  Units 4 and 7 switch the source to the store.
- Tests: the Rust tests feed facts and get the same row as before;
  `test/analytics_test.sh` unchanged in meaning; a facts file that is a link is
  refused and the run is not recorded, as a missing history line is today.
- Manual: `host/docs/analytics.md` (`:82-96`, `:154`): what the run is read from.

### Unit 4: `cycle-records` (#250: approvals, start, image and settings, end, history)

Moves every writer and reader of `.approved.json`, `.taken`, `.by`,
`.requeued`, `.rukas`, `.image`, `.settings`, `.branch` and
`cycles/<n>.history.jsonl`. They cannot be split: `cycle-start` refuses
without an unused approval, and `current` exists only after a start.

**libexec/dispatch**
- `approved_record` (`:1288-1316`): build the same object; write the inputs
  (`links`, `comment_links`, `retry_context`, `refresh_of`) to a 0600 temp file
  under `$TEHAS_HOME/tmp`; then `approval-record <n> --at <epoch> --by --how
  [--label tehas:ready for label and fixer] --body-sha [--title-sha]
  [--comment-id for retry-tick] [--refresh-pr --refresh-head for refresh]
  --input-file <tmp>`; remove the temp file. `--at` is `date -u -d
  "$given_at" +%s`, except `refresh` and `outage-retry`, which pass now (verbs
  spec D5: a refresh's `given_at` is its base's key, a used row). `given_at`
  stays the comment window for `comment_links` and `fixer_note`.
- `restore_record` (`:1430-1441`) becomes `approval-record <n> --undo --at
  <the at just recorded>`; the `previous` capture (`:1588-1591`) goes.
- The analytics `accepted` time (`:1602-1603`) is the `--at` just passed.
- `:1621-1623` (`rm .park .requeued`, write `.by`, `.requeued`): the `.by` and
  `.requeued` writes go (`cycle-start` derives both from the approval). The
  `.park` removal and `park_id` removal at `:1618` stay until unit 5.
- The wait for the start (`start_cycle`, `:1456-1478`): section 5.
- The quiet loop (`:2018`) and the done loop (`:2094`): `took` comes from one
  `cycle-show --running` per poll, kept in an associative array of
  `.cycle.started_at`. An entry with `cycle: null` (a held lock with no row)
  gives `took=0`, as a failed `stat` does today. The `.quiet` file stays until
  unit 6.
- `refresh_ok` (`:1712-1716`), `:1744`, `:1772`: `record=$(tehas_approval_view
  "$n")`; return 2 means skip this issue this poll; then `tehas_fixer_base`,
  `tehas_refresh_count`, `tehas_fixer_starts` unchanged.
- The refreshing scan (`:2218-2229`): the guard becomes `TEHAS_REFRESH_CONFLICTS
  = 1` or `history-show --how refresh` has a row or a running entry's
  `.cycle.how == "refresh"` (so a host that ever refreshed still lists stale
  marks, as the glob over every `.approved.json` does today). The owner of a
  pull request is the running issue whose `approval-current .current.how ==
  "refresh"` and `.current.refresh_pr == pr`. An owner that is not running is
  unmarked, as today. A failed read ends the scan without unmarking.
- Rule 4a's `by` (`:2978`) is `cycle-show <n> .cycle.started_by` (F6). Its
  `.park` read stays until unit 5.
- Preflight (section 2) after the poll lock (`:390-397`), before the first `gh`.

**libexec/cycle**
- `:240-242` become `cycle-start "$issue" --rukas "$rukas" [--by-hand]` per
  decision 7, through `tehas_store_try 3`. Keep `attempt`, `started_at` and
  `requeued` from the answer; run work with `TEHAS_CYCLE_ATTEMPT=$attempt`.
  Exits per decision 8; the header (`:15-26`) names 75 and 77 and drops
  `.taken`.
- `host/systemd/tehas-cycle@.service` gains `Environment=TEHAS_CYCLE_FROM=unit`.
- `:254`, `:262` (exit 65): `cycle-end --attempt A --refused` first.
- `:362`: `requeued` from the answer. `:363-369`: stop removing `.branch`,
  `.image`, `.requeued` (`.park` stays until unit 5).
- `:481-482`: the dead `.branch` read goes (finding 7).
- `:708-713`: the image and its commit from `cycle-show .cycle.image` and
  `.cycle.rukas_commit`.
- `:1056` (park body hash) stays until unit 5. `:1201-1203` (never ran text),
  `:1226` (the refreshed pull request), `:1251-1252` (history `by`, `how`): one
  `approval-current` at the report (`current`, `current.input.refresh_of`) and
  the `cycle-start` answer.
- `:1134` (`.branch`) and `:1247-1261` (the history line and `rm
  .approved.json`) become one call, after the comments and labels and before
  analytics: `cycle-end "$issue" --attempt A --status --step --rc
  [--runner-exit] [--host-saw killed] --oom --forks [--stalled] [--timed]
  [--branch] [--pr <number>] --result <carrier> --progress <carrier> [--cap]
  [--park-reason] [--failure-kind] [--failure-cause] [--provider] [--parked]
  [--never-ran]`, through `tehas_store_try 5`. `--pr` is `${pr##*/}` when `pr`
  matches `^https://github.com/$repo/pull/[1-9][0-9]*$`. The carrier paths are
  passed as they are: the store refuses a link itself (F7). The carriers are
  read and removed as today.
- `tehas_analytics run` gets `--facts`: the history line as the jq at
  `:1247-1255` builds it today, the attempt, `{at, how}` of `current`, and the
  `settings` and `rukas_commit` of `cycle-show` read before the end.

**libexec/work**
- `:378`: no `record` path. When `TEHAS_CYCLE_ATTEMPT` is set:
  `tehas_approval_view "$issue" "$TEHAS_CYCLE_ATTEMPT"`. 0: `record` is the temp
  path. 1: allowed only when `cycle-show .cycle.how == "hand"` (a by-hand
  cycle, from the store, not from a missing record), which takes the existing
  by-hand branch; otherwise exit 4. 2: exit 4, never the by-hand branch.
  Without `TEHAS_CYCLE_ATTEMPT` (a bare `tehas work` by hand):
  `tehas_approval_view "$issue"` (the latest `current`, as today's leftover
  file is); 1 is the by-hand branch; 2 is exit 4. `:498`, `:540-568`,
  `:606-611`, `:725` read `$record` as now. `tehas_record_is_file` stays: a
  temp file is a regular file.
- `:740`: stop clearing `.image` and `.settings`. `:757-770`:
  `tehas_improve_settings` writes to a temp file; with `TEHAS_CYCLE_ATTEMPT`
  set, `cycle-image "$issue" --attempt A --image "$image_id" [--rukas
  "$image_rukas"] --settings <tmp>`; without it, nothing is recorded.

**lib/**
- `lib/fixer.sh:150-162`: `tehas_cycle_history_add` is renamed
  `tehas_plan_history_add` and used only by `lib/planning.sh:186` until unit 7.
  `tehas_cycle_history <issue>` maps `history-show <issue> --kind cycle` rows to
  today's line keys (`at` ISO, `started` epoch, `pr` as
  `https://github.com/$TEHAS_REPO/pull/<n>`); it returns 1 when the store could
  not answer, and `tehas_refresh_parked` (`lib/refresh.sh:81-84`) treats 1 as
  "do not start".
- `lib/improve.sh:80-95`: `rukas` and `settings` from `cycle-show`
  (`.cycle.rukas_commit`; `.cycle.settings` regrouped by prefix: `PI_ENSEMBLE_*`
  into `container`, `TEHAS_*` into `host`, `concurrentCycles`). The mtime check
  goes: the row is this attempt's.
- `lib/analytics.sh`: the run facts from the store.
- `lib/cycles.sh:7-35`: the header names the store for the records.

**Other commands**
- `libexec/history:96-110`: cycle lines from `history-show [<issue>] --kind
  cycle` mapped as `tehas_cycle_history` does; plan lines from
  `state/cycles/plan-<n>.history.jsonl` (fixes D18) until unit 7. The publish
  jq and the cursor rule are unchanged, so `--json` keeps every field. A store
  failure is exit 78 or 1 and prints nothing.
- `libexec/stop`: `notes` (`:114`) loses `taken`, `requeued`, `branch`,
  `image`. When `cycle-show` has an open row, the unit is gone and the lock
  probes free, it runs `cycle-end --attempt A --stopped` **before** it removes
  the lock file (verbs spec D10). An open row counts as "present" for "nothing
  to stop" (`:153-157`).
- `libexec/export`: no change (the carriers stay until the next start).

**Tests** (changed): `dispatch_start`, `dispatch_approval`, `dispatch_park`
(the at-mark paths only), `dispatch_refresh`, `dispatch_fixer`,
`dispatch_slots`, `dispatch_finish`, `dispatch_room`, `dispatch_sweep` (done
loop), `cycle_start`, `cycle_record`, `cycle_result`, `cycle_crash`,
`cycle_handoff`, `cycle_stop`, `work_approval`, `work_host`, `work_images`,
`work_result`, `work_stop`, `work_caps`, `work_files`, `fixer`, `refresh`,
`improve_record`, `history`, `export`, `analytics`, `stop`, `attachments`,
`sizing` (fixtures only). Fixtures write rows through verbs: a helper
`t_approve <home> <n> <by> <how> [<at>]` and a held-lock `cycle-start`.

**New cases**
- A2: the label edit fails after `approval-record`: the approval is undone and
  `current` is the earlier one. A unit that never comes up leaves the attempt
  unchanged and the approval undone. `cycle-start` busy: exit 75, the approval
  unused, no label, no comment.
- A3: a second poll on the same label event gets `used` and starts nothing; a
  unit with no unused approval exits 77 and changes no label; a refresh
  recorded at now does not collide with its base.
- B4: 51 ends keep 50 history rows (driven by `t_at` loops of `cycle-start` and
  `cycle-end` straight to the store, for speed); `approval-current` keeps only
  the base and its fixer and refresh starts; the next start clears the settings.
- B7: the work stub's docker asserts at `run` that `cycle-show .cycle.image`
  and the settings are on record; `rukas_commit` is on record before work runs.
- F3: work under a unit with the store answering 78 or busy exits 4 and does
  not take the by-hand branch (no labeller-check call recorded by its stub).
- F6: `started_by` is the approval's `by`, even when a later unused approval
  by another account exists.
- F7, G1, G3: a `result.json` that is a link: the end is committed and says
  `refused`; a settings value of a secret shape stores no settings row; a
  progress file older than `started_at` is not stored.
- D2: store missing: the poll exits 78 before any `gh` stub call; the cycle
  exits 78 before `gh api`; `cycle-show` busy before the start changes no
  label; a busy `cycle-show --running` ends the poll with exit 1 and starts
  nothing; `tehas history` with the store at 78 exits 78 and prints nothing.
- The wait (section 5): the deadline holds when each `cycle-show` takes 2 s; a
  busy store during the wait still finds the start; a cycle that refused after
  `cycle-start` counts as started.
- `--never-ran` leaves no base, so the fixer starts nothing.
- `tehas stop` ends an open row `stopped` before the lock goes; a second stop
  is a no-op.
- A by-hand cycle (`TEHAS_CYCLE_FROM` and `INVOCATION_ID` unset) records `how
  hand` and a null `started_by`, and touches no approval.

**Forbidden names added**: `.taken`, `.by`, `.requeued`, `.rukas`, `.image`,
`.settings`, `.branch`, `.approved.json`, `cycles/<n>.history.jsonl` (not
`plan-<n>.history.jsonl` yet), `tehas_cycle_history_add`.

**Manual**: `host/docs/tools.md` (dispatch: the approval and its inputs, the
wait; work: the approval, `cycle-image`; tehasbot cycle: start, exits 75, 77,
78, branch, history; stop; history: its source), `features.md:105`, `:117`,
`:147`, `analytics.md:82-96`, `:154`, `operating.md:960`, `:1193`, `:1197`,
`host.md:43`, `troubleshooting.md` where it names these files.

**Pull request text** says: the systemd unit gains an environment line;
`TEHAS_CYCLE_ATTEMPT` is work's environment and nothing new reaches the
container (rule 4, `test/work_test.sh` unchanged).

### Unit 5: `park-records` (#250: parks and park questions)

**libexec/cycle** `park()` (`:1041-1091`):
1. `park "$issue" --attempt A --reason <provider-outage | the cap | the
   status> [--step] [--pr <number>]` before posting, in place of `:1054-1060`.
   The `retry` local goes (`park-show .park.automatic` derives it); `park_sum`
   (`:1055-1056`) goes (`park-show .park.body_sha256`).
2. The body goes to a `mktemp` file, not `state/park-<n>.json`.
3. After `post_question`: `question-asked "$issue" --id "$posted" --kind park
   --body-sha <sha of the posted body>` in place of `:1070`.
4. The label fails: `question-answered "$issue" --id "$posted" --withdraw` in
   place of `:1076`.
5. `:363-369` stops removing `.park`.

**libexec/dispatch**
- At a start (`:1618`, `:1621`), both **before the label** (moved above the
  `gh issue edit`): `question-answered --withdraw` of `park-show
  .park.question` when there is one, then `park-retry` (`unparked` is done). A
  failure means no start.
- `park_holds` (`:1788-1815`): `park-show` (current, not `provider-outage`,
  `retries == 0`), the body hash from `.park.body_sha256`, the cap from
  `.park.reason`, and the tell through `park-told` (`said:false` is "already
  told"). The WARN texts stay.
- The question id (`:2618-2633`): the plan branch stays on the file until
  unit 7. The park branch reads `park-show .park.question`, then
  `question-known <n> --id` before any tick counts (F5, G4). A failed read
  logs `skip #<n>: its question on record could not be read` and continues:
  never untick.
- Another box ticked (`:2695-2698`): `question-answered <n> --id --by <who>`
  after the label comes off.
- The plan start (`:2941`): withdraw the park question with
  `question-answered --withdraw` (unit 7 removes the call: `plan-begin` does it).
- Rule 4a (`:2970-2990`): loop over `park-show --automatic`; `started` from
  `.started_at`, `by` from `.started_by` (F6); `rm -f "$pf"` becomes
  `park-retry`. The approval it records is at now.

**libexec/stop** (`:114`, `:128-132`): `park-retry` and a withdraw of the park
question, before the lock goes; `park` leaves `notes`.

**lib/cycles.sh:125-129**: `park_id` goes.

**Tests**: `cycle_park`, `cycle_crash`, `cycle_result`, `dispatch_park`,
`dispatch_control`, `dispatch_finish`, `dispatch_turns`, `dispatch_room`,
`questions`, `stop`. Fixtures make parks with `cycle-start`, `cycle-end`,
`park` and `question-asked` through the store.

**New cases**
- F5, G4: a tick in a comment whose id is not on record starts nothing; a tick
  in a question withdrawn after a failed label starts nothing; after a new
  `cycle-start` the old question is not honoured; a tick on another issue's
  recorded id starts nothing.
- The untick guard: `park-show` busy or 78 records no untick call.
- A3: a tick counts once (the second poll finds the question answered).
- F6: rule 4a starts as `started_by` of the parked attempt even when a later
  approval by another account exists; when that account has left the allowed
  list, `park-retry` runs and nothing starts.
- C12: one automatic retry, then a person; a requeued cycle's outage park owes
  none; a by-hand park owes none; `tehas stop` spends the retry.
- B4: the park of the old attempt is not current after a start.
- A park hold is told once across two polls.
- A park that fails to record posts the plain comment, with no `tehas:question`.

**Forbidden names added**: `.park`, `state/park-`, `park_id`.

**Manual**: `tools.md` (the question on record, the automatic retry and who it
rests on, the park hold, stop), `troubleshooting.md` (a tick that starts
nothing), `features.md` park lines, `host.md:43`.

### Unit 6: `quiet-notes` (#250: the quiet-cycle note)

- `libexec/dispatch:1995-2073`: `retire_quiet_notes` and the link handling
  (`:2004-2031`) go (`cycle-start` deletes the notes; a link cannot exist).
  `heard` as now (`max(updatedAt, took[n])`). The `.quiet` read becomes
  `cycle-note <n> --kind quiet --said-for <heard>`, which gives `times` and
  `stop_asked_at`. The ask is `cycle-stop-asked <n>`; the "asks no more" line
  is `cycle-stop-asked <n> --said-only`. A dry run reads `.cycle.quiet` from
  the cached `--running` answer and writes nothing. Log texts unchanged.
- **Timing**: keep today's `next` exactly. Write the case first that pins it
  (the asks, then the line once per interval). If the verbs cannot express it
  (verbs spec D17 says `--said-only` records the line's time), stop and report
  in the handoff rather than change the timing.
- `lib/cycles.sh`: `tehas_cycle_note_files` goes (no caller left; check with
  grep). `libexec/stop`: `quiet` leaves `notes`.
- Tests: `dispatch_sweep_test.sh` quiet cases (`:317-489`): `quiet_cycle()`
  uses `t_at` for `cycle-start`; `notes()` reads `cycle-show .cycle.quiet`; the
  "quiet note that is a link" case is deleted. New: C6, one ask per episode, six
  asks then the line once per interval, a new episode counts from one, a stop
  that failed is asked again after `quiet_retry`; D2, `cycle-note` busy means no
  stop.
- Forbidden names added: `.quiet`, `tehas_cycle_note_files`.
- Manual: `tools.md` quiet-cycle text (`:423`), `troubleshooting.md` (a stuck
  cycle), `host.md` C6 table if it names the file.

### Unit 7: `plan-records` (#251)

**lib/planning.sh**
- `plan_state` keeps returning the base name `state/plan-<n>.json`, for the
  sidecars only: `.out`, `.rc`, `.lock` keep their names (a lock renamed across
  a deploy excludes nothing). Its comment says the record is the store's.
  `plan_keep <n>` returns `$TEHAS_WORK_DIR-plan-<n>.keep` (#251: out of
  `state/`). `plan_record_number` goes.
- `plan_turn` (`:341-428`): `turns` and `session` from `plan-show`. First turn:
  `plan-begin <n> --by --label --brief-file <tmp>`; the same temp file is
  `--brief` for that turn (`libexec/plan` keeps `brief.md` in the keep dir,
  `libexec/plan:266`). Later turns: `plan-answer <n> --turn N --by [--fixer]
  --text-file <tmp>`; the answer carrier is written from the row into
  `$(plan_keep n)/answer.md` and passed as `--answer`. A label failure:
  `plan-begin --undo` or `plan-answer --turn N --undo`. "Already filed"
  (`:410-416`) reads `plan-show .filed`. `--keep "$(plan_keep "$n")"`.
- `plan_post` (`:117-152`): `question-asked <n> --id --kind plan` in place of
  `.comment`; `plan-end <n> --turn N --posted` in place of `.pending=false`.
- `plan_finish` (`:228-339`): `tehas_analytics plan --facts <plan_view>` first.
  Then one `plan-end <n> --turn N --outcome <o> [--rc] [--session] [--option
  ...] [--filed ...] --out "$st.out"` per path (lost: `--outcome lost`, then
  `plan_lost`). The `.turn` copy (`:241-246`, `:329`) goes: an ended row stays
  readable through `plan-show` until the next `plan-begin`. `plan_history`
  (`:155-195`) goes (`plan-end` writes the line). `plan_refined` stays (D9).
- `plan_end`, `plan_lost` (`:93-105`): `plan-end <n> --close` after the labels,
  then remove `.out`, `.rc` and the keep dir.
- `plan_in_flight`, `plan_collect`, `plan_recover`: `lost`, `turns`, `session`
  from `plan-show`; `.rc` and `.lock` unchanged.
- `tehas_fixer_answer` (`lib/fixer.sh:116-147`) is given `plan_view`.

**libexec/dispatch**
- Rule 4b (`:2556-2609`): the three globs become the poll's `plan-show --open`
  (read again after the first loop changes it). The closed-issue removal
  becomes `plan-end --close` plus the sidecar removal.
- The question id (`:2618-2622`): `plan-show .planning.comment_id` when the
  session is open, else the park branch of unit 5; then `question-known`. The
  options (`:2797-2848`) come from the same answer.
- `fixer_answer_ok`, `fixer_held` (`:1170-1216`): `plan_view` for the reads,
  `plan-note` for the writes.
- The plan start (`:2938-2953`): `plan-begin` (it withdraws the park question
  itself); only the sidecars are removed.
- Every `[ -e "$(plan_state "$n")" ]` (finding 8) becomes `plan_open <n>`, a
  lookup in the poll's `plan-show --open` set, refreshed after any `plan-*`
  write of the poll. A failed read makes every issue count as open: start
  nothing that poll (A13).

**Other commands**
- `libexec/health:115-125`: `plan-show --open` filtered to `running`, with the
  `.lock` and `.rc` checks as now.
- `libexec/export:253-255`: `plan.json` is `plan-show <n> .planning`. The
  bundle's shape changes; `api.md:296-301` says so.
- `libexec/history`: plan lines from `history-show --kind plan`.
- `libexec/plan`: only the keep location in its header (`:14-30`).
- `console/analytics`: no change (facts from unit 3).

**#251's `$tree.state` item**: the turn start removes `$tree.state`
(`libexec/plan:331`), but the Pi sessions are in `$tree/.git/pi-plan-sessions`
(`:202`) and carried through `$keep/plan-sessions` (`:373`, `:455-456`). Add a
`plan_test` case where turn 2 resumes the session turn 1 kept. Green: say so in
the pull request (the finding is closed). Red: fix it here if it is tehas's,
file it if it needs pi-rukas.

**Tests**: `dispatch_plan` (`jq ... plan-7.json` becomes `t_row plan-show 7 --
...`), `dispatch_plan_answer`, `dispatch_turns`, `dispatch_queue`,
`dispatch_fixer` (notes), `dispatch_sweep`, `plan`, `fixer`, `history`,
`export`, `status` or wherever health is tested, `analytics`.

**New cases**
- B8: brief, session and filed survive a failed turn (today `planning.sh:311`
  loses `filed`); a lost session ends with outcome `lost` and stays until
  closed; a finished turn with no session is a failed turn.
- F5: a reply or tick in a comment that is not `comment_id` runs no turn; after
  `plan-answer` the old question is answered.
- A13: an open session blocks a cycle start, a refresh and rule 4a; `plan-begin`
  on a running session refuses and the poll starts nothing; a busy `plan-show
  --open` blocks every start that poll.
- D2: `plan-end` busy keeps `.rc`, and the next poll posts once.
- B4: a new `tehas:plan` replaces an ended session's answers, filed and notes.
- The keep dir is not under `state/`.

**Forbidden names added**: the record `state/plan-<n>.json` read or written
whole (allowing `.json.out`, `.json.rc`, `.json.lock`), `.brief`, `.answer`,
`.answer.new`, `.turn`, `.json.keep`, `plan-<n>.history.jsonl`,
`plan_record_number`, `tehas_plan_history_add`, `"$st"` and `"$st.new"` as a
record.

**Manual**: `tools.md:357-397` (planning, `tehas plan` keep location, health),
`operating.md:575`, `:595`, `:1182`, `:1197`, `host.md:43` and the plan lines,
`features.md:52`, `api.md:296-301`, `analytics.md:86`.

### Unit 8: `old-paths` (close-out; needs units 4 to 7 and the importer)

- `test/store_paths_test.sh`: check the forbidden list against the inventory
  of `docs/specs/2026-10-21-state-store.md` section 1.3 and 1.4 and this plan's
  decision 4; the final list of kept files and the reason for each goes in the
  test's header. `t_no_old_state` covers every name.
- **The poll's legacy guard** (`libexec/dispatch`, after the preflight): any of
  `state/cycles/[1-9]*.{approved.json,park,by,requeued,taken,rukas,quiet,image,settings,branch,history.jsonl}`,
  `state/park-[1-9]*.{id,json}`, `state/plan-[1-9]*.json` (exact name),
  `state/plan-[1-9]*.json.{brief,answer,turn,keep}`,
  `state/cycles/plan-[1-9]*.history.jsonl` (with `compgen -G`, narrow enough
  never to match `state/dispatch.taken`) makes the poll log `ERROR: records of
  an older tehas at <path>; run install.sh again` and exit 78 before any `gh`
  call. Tests: each kind stops the poll; `dispatch.taken` does not. This
  catches a failed import and a cycle that ran old code across a deploy.
- `host/docs/features.md`: "records in the store: tested, not seen on a host".
- `host/docs/operating.md`: the upgrade step (drain, install, what `install.sh`
  refuses, `state/imported-<date>/`, `tehas limit clear` after).
- `docs/specs/2026-10-23-store-verbs.md` section 0 "Stays a file": the final
  list of decision 4. `docs/adr/0003-state-store-and-reconciler.md` "What stays
  a file": the carriers, `.done`, `.stopped`.
- A last `grep -nE` over `host/docs/*.md` for every forbidden name; each hit is
  rewritten or explained.
- CHANGELOG `Unreleased`, Adopter action: run `tehas drain --wait <seconds>`
  and wait for planning turns to end before updating; `install.sh` refuses
  while a cycle or a planning turn runs and old records exist; `tehas limit
  clear` after.

## 5. The dispatcher's wait for the start

```bash
# In start_cycles, per picked issue, before the label edit: a start that cannot
# know the attempt it waits past changes nothing.
if ! tehas_store cycle-show "$n"; then
  log "ERROR: #$n: the store could not say which attempt is on record ($tehas_store_word)"
  # undo the approval just recorded, failed=1, continue
fi
before_attempt[$n]=$(tehas_store_get '.cycle.attempt // 0')

start_cycle() {
  local n=$1 got deadline
  # ...start the unit, unchanged, 8>&-...
  deadline=$((SECONDS + start_wait))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if tehas_store cycle-show "$n"; then
      got=$(tehas_store_get '.cycle.attempt // 0')
      [ "$got" -gt "${before_attempt[$n]}" ] && return 0
    elif [ "$tehas_store_word" != busy ] && [ "$tehas_store_word" != timeout ]; then
      break # an unusable store: the lock decides
    fi
    sleep 0.5
  done
  if tehas_cycle_running "$n"; then
    log "WARN: the cycle for #$n holds its lock but did not say so in ${start_wait}s; taking it as started"
    return 0
  fi
  return 1
}
```

`tehas_store` is called in the poll's own shell, never in `$(...)`, so
`tehas_store_word` is the caller's to read.

- The bound is wall-clock `SECONDS` against `start_wait`
  (`TEHAS_CYCLE_START_WAIT`, 30 s), not a count of loops: a reader can wait 1 s
  for its busy timeout plus `quick_check`.
- `rm -f "$taken"` goes. An earlier attempt never counts: the comparison is
  with `before`.
- A cycle that refuses after `cycle-start` (`cycle-end --refused`) has still
  raised the attempt, so it counts as started, as `.taken` did
  (`libexec/cycle:21-25`).
- A unit that comes up after the deadline finds its approval undone and exits
  77 without running. Today such a late unit's `work` took a missing record as
  a run by hand; this is narrower.

## 6. The importer (separate, parallel work)

See the importer brief. Its contract, which units 4 to 8 rely on:

- **Verb**: `store import [--dry-run] [--json]`, reading `TEHAS_HOME` like every
  other verb (the spec's `--home` is `TEHAS_HOME`). It needs schema 2 (unit 1).
- **Root proof first**: `$HOME/.tehas-home` must exist (the marker
  `install.sh` requires) or exit 78 `unmarked`, touching nothing (B6).
- **Live refusal**: exit 1 `live` when there is anything to import and any
  `state/cycles/<n>.lock` or `state/plan-<n>.json.lock` probes held or
  unknown.
- **Scope**: the group B and C records of this plan: cycles (`.taken`, `.by`,
  `.requeued`, `.rukas`, `.image`, `.branch`, `.settings`), approvals with
  their inputs, parks, park questions, cycle history, planning records with
  answers, filed, notes, brief and answer text, plan history from both paths.
  Carriers stay where they are. `.quiet` is moved without rows (no cycle runs
  after a drain).
- **Idempotent**: natural keys and `INSERT OR IGNORE`; each file is moved to
  `state/imported-<YYYYMMDD>/<its path under state/>` only after its rows are
  read back after the commit; nothing is deleted; a second run with nothing
  left is exit 0 with zero counts. It may run again whenever old files exist
  (spec section 7's "a second import is 78 unless `--force`" is replaced).
- **Held**: a file that is a link, too large, or does not parse (times are
  parsed, never coerced) is `held`, stays, and is listed. Exit 1 when anything
  is held.
- **Answer**: `{"api":1,"verb":"import","ok":true,"imported":{<kind>:N},"moved":N,"held":[<path>...],"dry_run":bool}`.
- **install.sh**: holds `state/dispatch.lock` from before the copy to after the
  import; refuses (exit 1, "nothing was installed") while old files exist and a
  cycle or plan lock is held; runs `--dry-run` with the built store before the
  copy and refuses on `held`; runs `store import` after `store migrate`; moves
  each `state/plan-<n>.json.keep` to `$TEHAS_WORK_DIR-plan-<n>.keep`.
- **A cycle running across the install** is refused by the check. One that slips
  through (a person's `tehasbot cycle` after the check) writes old files at its
  end; unit 8's poll guard stops every poll at 78 until `install.sh` runs again,
  and the re-run imports them (ending the open imported row from the new
  history line).

## 7. Invariants by unit

| Invariant | Unit | What pins it in the shell tests |
| --- | --- | --- |
| A2 | 4 | label failure undoes the approval; a unit that never comes up leaves the attempt |
| A3 | 4, 5 | one approval, one start; one tick, one start |
| A13 | 7 | an open session blocks; busy blocks |
| B4 | 4, 5, 7 | 50 history rows; approval retention; the old park is not current; a session replaced |
| B7 | 4 | image and settings on record as the container starts |
| B8 | 7 | session, brief and filed survive a failed turn; `lost` |
| C6 | 6 | one ask per episode |
| C12 | 5 | one automatic retry |
| D2 | every unit | a failed read is never "none": work, the tick, A13, rule 4a, history, the wait |
| F3 | 4 | work never takes the by-hand branch on a store failure |
| F5 | 5, 7 | an unrecorded or withdrawn id is never honoured; no untick on a failed read |
| F6 | 4, 5 | `started_by` from the used approval |
| F7 | 4 | result and settings are claims; a link is refused |

## 8. Owner sign-off, to raise in the pull requests

- Decision 5 (liveness stays a file probe), against the verbs spec.
- Decision 7 (who counts as "from the unit") and decision 8 (exits 75, 77, 78).
  A unit started by hand with no approval is now refused.
- Unit 1's schema step and new verbs (verbs spec D29 on).
- The importer running again and moving instead of verify-and-remove (spec
  section 7), and the install refusal while cycles or turns run.
- AGENTS.md rule 6 against the cargo-built store in shell tests, and the
  `scripts/test.sh` build step next to the gate.
- The export bundle's `plan.json` becomes the store's planning object (unit 7).

## 9. What was rejected

- **An environment override for the store binary** (risk-first and
  minimal-diff designs). A wrapper script at the installed path tests every
  failure without a seam on a host, and an override could point at a binary of
  another schema.
- **Moving liveness onto `cycle-show`** (tests-first design). See decision 5.
- **A separate `inputs.json` file bound by `at`** (minimal-diff design). The
  column keeps the inputs and their approval in one transaction and one undo.
- **analytics opening `tehas.db` read only** (risk-first design). A second crate
  would depend on the schema. Facts keep it out.
- **`cycle-note --kind park-held` with no schema change** (minimal-diff design).
  `cycle-note` refuses an ended cycle by design, and the schema step is needed
  anyway for the inputs and the answer text.
- **A cargo feature for the test clock** (tests-first design). The
  `debug_assertions` gate already exists for `test-write` and needs no second
  build.
- **Splitting the cycle start from the cycle end** (risk-first design). Between
  the two, every row stays open and `--never-ran` has no caller, so the fixer
  could rest on an approval that started nothing. One unit with ordered commits
  instead.
- **Per-unit importer kinds** (tests-first design). The importer is one piece of
  work in parallel; growing it inside each unit would serialise it behind them.
- **Removing carriers after `cycle-end` and exporting from `cycle-show
  --payloads`** (tests-first design). Today's "until the next start" keeps
  `export` unchanged.
- **A `.turn` file written from `plan-show`** (minimal-diff design). An ended
  row is readable until the next session, so the copy has no reader left.
