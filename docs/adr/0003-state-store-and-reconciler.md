# ADR: the host's state in SQLite, the dispatcher as a reconciler

**Status**: Proposed, 2026-10-21. A person accepts it before #246 starts.

Design record for epic #244. The companion document, with the inventory, the
schema, the verbs, the journal, the snapshot, the import and the baselines, is
[state-store spec](../specs/2026-10-21-state-store.md).

## Context

The host keeps its own records in about 35 kinds of file under `$TEHAS_HOME`
(the inventory in the spec lists them). They fall into 13 groups that have to
change together, and nothing makes them do that: a poll writes
`state/cycles/<n>.approved.json`, then changes a label, then writes
`state/cycles/<n>.taken`, and a failure between those leaves the issue marked
running with no approval record, or recorded as started with no mark.

The research over 132 issues, 86 merged pull requests and 366 commits sorted
the failures behind that:

| Root cause | Issues |
| --- | --- |
| The same fact on GitHub and on the host, changed in steps that can fail between them | 10 |
| Local state written in pieces, unchecked writes, leftovers | 16 |
| No timeouts, hung or orphaned processes, locks held for ever | 25 |
| Bash itself: word splitting, `null` vs missing, SIGPIPE, dropped settings | 4 |
| Performance | 9 |

The shape of the worst group is not a bad file format. It is a change spread
over several files and one or more GitHub calls with no transaction and no
record of what was intended. A poll that dies between the mark and the start
leaves a fact on GitHub the host does not agree with, and the next poll has no
way to tell an action that finished from one that never began.

Two facts about the code have moved since the research was taken, and both
matter here.

- Cycles now run through the pi-rukas headless runner (#234), which gives
  `libexec/work` an exit code and a JSON result. The cycle supervisor can be
  reasoned about as a program that ends with an answer, which is what a
  reconciler needs.
- tehas no longer reads pi-rukas internals (#229) and posts its own park
  comment (#232). The state that used to be borrowed from `.pi/work-state` is
  gone, so the inventory below is tehas's own.

## Decision

1. **One SQLite database for the host's own records**: the state store, at
   `$TEHAS_HOME/state/tehas.db` — a path this decision names, not one the code
   has yet created: nothing under `libexec/`, `lib/` or `test/` mentions it
   today, and `state/analytics.db` is the file beside where it will sit
   (`console/analytics/src/db.rs:20` names that one). Behind a Rust command with
   typed verbs, bash never writes SQL.
   Each of the 13 groups of files that change together becomes one verb that
   writes in one transaction.
2. **The dispatcher becomes a reconciler.** It reads a snapshot (GitHub, the
   database, held locks, the slice), computes actions with a pure function, and
   applies each through an action journal that records the intent before the
   GitHub call and the result after it. It runs in shadow beside the bash
   dispatcher before it takes over.
3. **The cycle supervisor** (`libexec/cycle`, `libexec/work`) moves to Rust
   once the headless runner gives it an exit code and a JSON result.
4. **The rest stays bash**: install, update, bootstrap, the units, preview,
   protolab, improve, dev, status and health. They read and write through the
   command.
5. **What stays a file stays a file**, for the reasons under *What stays a
   file* below. This is not a decision to put everything in a database.

Not in scope: replacing `gh`, changing the GitHub label protocol, and
pi-durable (see ADR 0001, and how this one reads its point 4 below).

## What stays a file, and why

**The flock locks.** `state/cycles/<n>.lock`, `state/dispatch.lock`,
`state/update.lock`, `state/improve.lock`, `state/index.lock`,
`state/clean.lock`, `state/<name>.lock` for the edge, and
`state/plan-<n>.json.lock`. The kernel releases an flock when the process dies,
and the docker client inherits the cycle lock, so a killed parent still holds
the slot. A row cannot do that without a heartbeat, and a heartbeat is what
#194 was about: a cycle that was merely slow was called dead.

**`live/`.** Caddy serves it read-only over HTTP and the front page reads it.
Caddy has no database driver and the page fetches plain files with Range
requests. The database is the source; `live/` is a view published from it by
one publisher. The published shapes do not change (#252).

**Everything mounted into a container.** `state/cycles/<n>.cid` (docker writes
it), the container's logs, its config, `state/cache`, `state/memory`,
`state/sessions`, `state/models.json` and `state/index`. A container must not
see the database, and a mount is a file. This is AGENTS.md rule 4 and
`test/work_test.sh` pins it.

**`state/update-pending` and `state/index.json`.** `install.sh` reads them
before the new command exists on a fresh host, and an install that cannot find
its command has no way to say so. They stay files, and the command reads them.

**`state/analytics.db`.** It is a second database on purpose: it is append-only
history that a reader prunes, its schema version and its retention rule are
its own (`console/analytics/src/schema.sql`), and its readers (`tehas status`,
`tehasbot top`, the MCP server) tolerate it being absent or stale. Folding it
into the state store would make every dashboard read contend with every poll's
write, and would tie a 365-day history to the lifetime of the current design.
It keeps its own file, its own `PRAGMA user_version` and its own migrations.

The state store borrows that program's **schema conventions** and none of its
recovery. The two databases fail in opposite ways and must. The analytics
program quarantines a database it cannot use: `open` moves it to
`analytics.db.broken`, or to `analytics.db.schema-<v>` with its side files, and
builds a fresh one (`console/analytics/src/db.rs:24-26`, `:38-45`), because its
readers tolerate absence and the data is ten days deep. The state store is the
opposite case: its rows are the host's only record of an approval, of a park
question id (the F5 trust root) and of an unsettled journaled action, and
quarantining any of that erases the record behind the two largest groups of the
Context table above — 16 issues of local state written in pieces and 10 of the
host and GitHub disagreeing. **The state store never moves a damaged database
aside and carries on.** It stops with 78 and the word `corrupt` or `unreadable`
(spec section 4), and the damaged file stays exactly where a person can read it.
A future reader extending that code path inherits this rule, not the analytics
one.

**The `state/index/` tree, `state/plan-<n>.json.keep/`, and `~/.pi/agent/`**.
Directories of other programs' making: the indexer's databases, the runner's
per-turn payload, pi-rukas's runs and sessions. tehas moves them and does not
read them.

## Rationale

**A transaction is the only fix for the largest group.** Sixteen issues are
local state written in pieces, and ten more are the host and GitHub disagreeing
because the write is spread across files and calls. Neither is fixed by a
better file layout. `BEGIN` and `COMMIT` are, for the host's half, and the
journal covers the other half: an action is written before it is attempted, so
the next poll knows what was intended.

**The locks are the reason this is not "everything in the database".** The
liveness truth of a cycle is a lock nobody holds (C5), and only the kernel gives
that for free. Keeping the locks as files and the records as rows is the
division that the history supports: the files were never the problem for locks,
and the rows are never a substitute for them.

**Bash cannot own the SQL and still be the shell this repository writes.**
The epic's rule that "bash never writes SQL" is what keeps D3 (JSON numbers and
fields are validated) and D2 (read failed and empty are different types) from
decaying: a jq filter written inline in a shell function has no type to fail
against.

**The reconciler separates deciding from applying, which is what makes the
rules testable.** The 19 `dispatch_*` test files on main hold 611 `t_case`. The
split inside that 611 — how many run a whole poll through stubbed `gh` and
`docker` to ask a rule question, and how many are about a process (a lock, a
systemd start, a hang) — is **not measured**, and #254 owns measuring it before
anyone quotes a number. What is measured is the cost that motivates the
separation, and the figures are the spec's to own: see *What is true today* in
[the spec](../specs/2026-10-21-state-store.md) and its section 10 for the 611,
the share of the serial suite the 19 files cost and the three slowest
`dispatch_*` files with their seconds — all taken in this cycle's sandbox and
all open for a tehas-tehas re-taking. Quoted here rather than copied, so one
re-measurement moves one document. A case that asks a rule question pays the
fork of a stubbed `gh`, a stubbed `docker` and a whole `libexec/dispatch`. A
pure decide function takes a snapshot and returns a list of actions, and a test
of it costs microseconds. The epic's target of 200 rule tests as Rust unit tests
under 5 s together is the statement of that difference, and it is the line #254
has to meet; the process cases (locks, systemd start, hangs) stay as shell tests
against the binary. #254 counts the 611 into the two kinds and reports both
numbers, so the next reader has the split rather than an estimate.

**A snapshot replaces the per-issue reads.** An idle poll today makes about ten
`gh` calls; `report_demand` alone pays one REST call per queued issue for its
events. One GraphQL query with pages, plus bounded per-candidate calls, is the
target of 3 or fewer for an idle poll (measured against the cost model in the
spec, section 6). Requests are the easy half: at the 2-minute timer the host
actually runs, about 30 polls an hour, the GraphQL budget of 5,000 points leaves
about 165 points a poll, and that — not the request count — is what fixes the
page size and the per-connection caps. The spec prices a full page of 100 issues
at several times the whole hour — a model of GitHub's node formula, not yet a
measurement, and #253's first job is to record the measured delta against it
(spec, section 6) — which is why the snapshot pages three issues at
a time and stops on a window rather than reading the queue.

## Alternatives considered

**Stay with files, and fix them.** Fix the `.new` collisions (`status`,
`health`, `energy` write a fixed `.new` name so two writers can collide), give
`running.json` and `endings.json` a lock, make every write unique-temp-and-
rename, and bound every call. This is #247 plus a year of care, and it does not
reach the two largest groups: a file per fact still cannot commit a label
change and a record change together, and there is still no place to write down
that an action was intended. The 10 issues in the GitHub/host group and the 16
in the pieces group are the ones that keep coming back. Refused as the whole
answer; #247 is kept as a sub-issue anyway because its test-suite work is
independent of the store and pays for itself either way.

**Adopt pi-durable** (ADR 0001). Refused, and it answers the wrong question:
pi-durable is the agent harness, and tehas already drives a complete harness
inside a sandbox image pinned by digest. Its storage model is one storage per
process with no cross-process locking, which is the opposite of a host whose
dispatcher, status poller and dashboard read the same state without talking to
a live agent. ADR 0001 stands and its revisit triggers stand. What changes is
one of its four reasons; see the next section.

**A key-value or document store instead of SQLite** (LMDB, sled, or a directory
of JSON files with a lock). Refused: none gives the ad-hoc reads the reconciler
needs (running cycles with their image, open sessions with their turns, actions
not yet settled) without a query language of its own, and none is already in
this workspace. rusqlite with `bundled` is already a dependency of the
analytics program in `console/analytics/Cargo.toml`, so the SQLite version is
the crate's and not the host's today.

**Write the store in bash with a C or Python helper.** Refused: the repository's
Rust standard (no `unsafe`, clippy clean, `--locked`, one workspace with the
console) already applies to the console workspace's programs — the console, the
analytics program and the MCP server, the three members *How this ADR reads ADR
0001* counts — and the value here is a type that
distinguishes a failed read from an empty one. In bash that distinction is a
convention, and D2 exists because the convention failed.

## How this ADR reads ADR 0001's "bash house style"

ADR 0001's rationale point 4 says:

> **Bash house style.** tehas is shell scripts plus two small Deno programs.
> pi-durable is a Node/TypeScript harness dependency - a different shape of
> project.

That reason was written on 2026-10-07, when the count was two Deno programs.
It is stale on both halves, and this record is where the correction lives:
ADR 0001 itself is not edited (its context is what was true when it was
written). Read today, the point is:

> **House style.** tehas is shell scripts plus Rust programs in one cargo
> workspace (`console/`, whose members are the console, the analytics program
> and the MCP server). What the point excludes is a dependency of a *different
> shape of project*: a harness whose release cycle the host does not control.
> It never excluded Rust, and it does not exclude a host-side store.

Two consequences, so nobody has to guess:

- Point 4 is no longer a reason to refuse a Rust program. It was never the
  reason for that refusal; points 1, 2 and 3 (layer mismatch, storage model,
  experimental dependency pinned by digest) were, and those three stand
  unchanged. Anyone re-arguing ADR 0001 argues those three.
- Point 4 does not license the state store either. The state store earns its
  place on the arguments under *Rationale*, not on "Rust is house style now".
  A new Rust program is judged on whether bash could hold the same invariant,
  the way the console, the analytics program and the MCP server were.

## Consequences

- The design is in [the spec](../specs/2026-10-21-state-store.md): the file
  inventory with each file's writer, reader and fate; schema v1 with every
  table, column, key and index and the files it replaces; the verbs, their
  arguments, their JSON and their exit statuses; the action journal; the GitHub
  snapshot and its request count; the import; the vendoring decision; the
  invariants with the sub-issue that tests each; and the baselines, with the
  commands to take them again.
- Nothing in this ADR changes a line of code, a setting or the manual. No
  behaviour becomes true until #249 through #257 land, and
  `host/docs/features.md` marks each piece not proven until it has run on
  tehas-tehas.
- The baselines in the spec were taken in the cycle's own sandbox, not on
  tehas-tehas, because this cycle has no access to that host. They are
  labelled as such and each carries the command that takes it on tehas-tehas.
  **The issue's acceptance criterion is a tehas-tehas baseline, so this part is
  open and a person or an ops run on tehas-tehas must take it before #246 is
  scheduled on the strength of those numbers.**
- The gate does not run cargo today: `scripts/verify.sh` runs the console's
  tests where cargo is installed, and `.github/workflows/verify.yml` runs the
  fast gate on the organisation's runner. Making cargo required is #246, which
  AGENTS.md rule 7 gives to a person.
- `verify.sh --full` fails on main today, before any change of this cycle: three
  lints under a rustc ahead of the `host/versions.env` pin. The full entry, with
  the three lints, their file and line and the step of the gate that turns them
  into errors, is in *What is true today* in
  [the spec](../specs/2026-10-21-state-store.md). It is the lints step of #246's
  workspace, not a consequence of this ADR, and it is named here so the next
  reader does not attribute it to #245.
- Vendoring: `cargo vendor --locked` of the console workspace yields 62 crates
  and 21 MB for `libsqlite3-sys` alone; the counts, the byte figures and the
  checkout comparison are the spec's section 8 measurement, and it is the source
  for them. The recommendation and the size cost are there.
- #248 (one cargo workspace) and #253 (dispatcher part 1) stay parked until this
  ADR is accepted and #246 is done. Starting them out of order is why both
  parked before.
