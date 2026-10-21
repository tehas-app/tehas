# Spec: the host's state store and the reconciler

**Status**: Draft, 2026-10-21, for issue #245. Nothing is built. Design only:
this change writes no code and changes no setting. The decision record is
[ADR 0003](../adr/0003-state-store-and-reconciler.md).

Part of epic #244, first sub-issue, done after #234 closed. It takes the
epic's inventory, invariants and numbers again against main after #234, because
the research was taken at `e244478`, before the headless runner (#6), before
tehas stopped reading pi-rukas internals (#229) and before tehas posted its own
park comment (#232).

What "done" means for this document, criterion by criterion, is the last
section, *Done when*.

## Purpose

The host keeps its own records in about 35 kinds of file under `$TEHAS_HOME`.
Sixteen issues in the research set are local state written in pieces; ten more
are the host and GitHub disagreeing because a change is spread over files and
calls with nothing recording what was intended. This spec designs the store
that fixes both halves: one SQLite database behind a Rust command with typed
verbs, one transaction per group of files that must change together, and an
action journal that lets the next poll settle what the last one intended.

The 13 groups of section 2 are the unit of the design: each becomes exactly one
verb, and one verb is one transaction.

## What is true today

Checked on main at `5e2d5fa` (the base of this cycle) in this repository, and
in the sandbox the cycle runs in. Nothing here was measured on tehas-tehas;
section 10 says what that costs and who has to close it.

- `libexec/dispatch` is **2,888** lines, with **24** `gh` call sites and **7**
  uses of `timeout`. The epic says 1,961 at `4e1c7c4` and 1,454 at `e244478`.
- `libexec/cycle` is **1,195** lines, `libexec/work` **1,010**, `libexec/plan`
  **513**. The epic says 701 and 778.
- The dispatcher's rules live in prose in its header: 13 numbered rules before
  the first line of code.
- `test/` holds **3,343 cases in 121 files** as `scripts/test.sh` counts them
  (it counts every `t_case` line, including those in a header comment); counting
  the calls themselves gives 3,027. The 19 `dispatch_*` files hold
  **611** of them — that figure is exact, counted line by line. The epic says 420
  dispatcher cases at `4e1c7c4`.
- The cargo workspace is `console/Cargo.toml`, members `console`, `analytics`,
  `mcp`. There is no root manifest. `rusqlite` 0.38 with `bundled` is already a
  dependency of `analytics` (`console/analytics/Cargo.toml`).
- `state/analytics.db` already exists, with `PRAGMA user_version` and times as
  Unix seconds (`console/analytics/src/schema.sql`; the version constant at
  `console/analytics/src/db.rs:10`, the PRAGMA reads at `:140` and `:170`).
- The fast gate passes. `scripts/verify.sh --full` fails on main today with
  three lints, two of them `clippy::unnecessary_sort_by` at
  `console/mcp/src/sessions.rs:636` and `:641` and one deprecated
  `Atomic::fetch_update` at `console/mcp/src/logs.rs:63`, all under rustc 1.99,
  ahead of the `host/versions.env` pin. The step of the gate that reports them is
  `cargo clippy --locked --quiet --all-targets -- -D warnings`
  (`scripts/verify.sh:62`), which turns all three into errors; `cargo clippy`
  without `-D warnings` prints the same three as warnings. That is #246's work,
  not this change's, and is recorded so the next reader does not blame #245.

## 1. The inventory

Every file under `$TEHAS_HOME` that tehas writes or reads, with the command
that writes it, the commands that read it, and its fate. `fate` is **table**
(it becomes rows), **file** (it stays a file, with the reason), or **removed**
(it stops existing).

A file left out of this table is a bug in this spec: the whole point of the
inventory is that a file nobody thought about is a file that will be orphaned
by the import in section 7.

### 1.1 Root of the home

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `$TEHAS_HOME/env.sh` | `install.sh` | login shell, every unit, `bootstrap.sh` | file: it is the PATH, and a store behind a command cannot be read before the command exists |
| `$TEHAS_HOME/.tehas-home` (the marker) | `install.sh` | `install.sh` | file: it is the proof the directory is tehas's, read before anything else |
| `bin/`, `libexec/`, `lib/`, `logs/`, `tmp/`, `work/`, `preview/`, `protolab/`, `platform/`, `tools/`, `edge/` | `install.sh`, `bootstrap.sh`, the units | their own commands | file: installed programs, logs and trees, not records |

`logs/` is not state and is not imported. Its rotation is a separate finding
of the research (`libexec/improve:130` rotates only when the loop is on) and
belongs to #257.

### 1.2 Group A: the locks

All under `$TEHAS_HOME/state/`. Every one of them stays a file, for the reason
in ADR 0003: the kernel releases an flock when the process dies.

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/cycles/<n>.lock` | created and held by `libexec/work` (`:394`) and `libexec/cycle` (`:224`) | `lib/cycles.sh` (`tehas_cycle_running`, `tehas_cycle_lock_free`, `tehas_cycles_running`), `libexec/dispatch` (rules 2, 3, the sweep), `lib/live.sh:38`, `libexec/clean:201`, `libexec/stop`, `libexec/dev:272`, `libexec/index` | **file**, and its *held* state is mirrored into `cycle_locks` (section 3.3) as a cache, never as the truth |
| `state/dispatch.lock` | `libexec/dispatch:361`, held for the poll; also taken by `libexec/index:245` and `install.sh:1157` | every later poll; `host/docs/tools.md` | file: same reason |
| `state/update.lock` | `libexec/update:252`, `install.sh:325` | each other | file |
| `state/improve.lock` | `libexec/improve:117` | the loop | file |
| `state/index.lock` | `libexec/index:243` | `libexec/index`, `libexec/dispatch:1422` (a poll will not start cycles while it is held) | file |
| `state/clean.lock` | `libexec/clean:107` | `libexec/clean` | file |
| `state/<name>.lock` for the edge (router, docs) | `lib/edge.sh:28` | `lib/edge.sh`, `libexec/update` | file |
| `state/plan-<n>.json.lock` | `lib/planning.sh` (`plan-turn` holds it for the turn) | `lib/planning.sh` (`plan_in_flight`, `plan_collect`), `libexec/health:115`, `libexec/dispatch:2282` | file |

`state/plan-<n>.json.lock` is the one lock in this group that the planning
records in group C also claim. It stays a file: it is the liveness proof of a
turn, exactly as the cycle lock is for a cycle (C5, C7).

### 1.3 Group B: the cycle's records, `state/cycles/<n>.*`

Written by `libexec/work` (the runner's side) and `libexec/cycle` (the
supervisor's side), read by `libexec/cycle`, `libexec/dispatch`, `libexec/stop`,
`libexec/export`, `libexec/history`, `lib/improve.sh`, `lib/attachments.sh`,
`lib/fixer.sh` and `lib/cycles.sh`. All become rows; the fate column names the
table of section 3.

| File | Writer | Notable readers | Fate |
| --- | --- | --- | --- |
| `.lock` | work, cycle | group A | file (A) |
| `.taken` | `cycle:232` (`: >`) at the start, under the lock; `dispatch:1344` also creates it | `dispatch:1861`, `:1937` (mtime is the cycle's start time; the stuck-stop rule) | table: `cycles.started_at`. **Today the start time is a file mtime** |
| `.approved.json` | `dispatch:1173` (`approved_record`): `{at, by, how, body_sha256, ...}` | `dispatch:1315,1473,1487,1596`, `cycle:1116,1141,1167`, `lib/fixer.sh:86`, `lib/attachments.sh:115`, `dispatch:2061` (`grep -Elqs` over `*.approved.json`) | table: `approvals` (3.4). One JSON per issue at a time, read by glob today |
| `.by` | `dispatch:1506` one line, the account that started it | `cycle:1166`, `dispatch:2693` (an automatic retry is authorised by this account) | table: `cycles.started_by` |
| `.requeued` | `dispatch:1507` (touch) | `dispatch`, `cycle:352`, `libexec/stop` | table: `cycles.requeued` |
| `.done` | `dispatch:1960` one word: `closed`, `merged`, `handoff` | `cycle:935` (reads and removes it) | table: `cycles.end_reason` |
| `.park` | `cycle:975` the park reason | `dispatch` (rule 4a), `libexec/stop` | table: `parks` (3.7) |
| `.rukas` | `cycle:231` the pi-rukas commit, at the start | `lib/improve.sh:82` | table: `cycles.rukas_commit` (B7: recorded when it becomes true) |
| `.image` | `work:375` | `cycle`, `libexec/export` | table: `cycles.image` |
| `.settings` | `work:376` the `PI_ENSEMBLE_*` values the run got | `lib/improve.sh:87` | table: `cycle_settings` (3.6) |
| `.branch` | `cycle:1049` | `cycle`, `libexec/stop` | table: `cycles.branch` |
| `.quiet` | `dispatch:1864` the episode that was said once | `dispatch:1848` (glob `tehas_cycle_note_files quiet`) | table: `cycle_notes` (3.6) |
| `.cid` | **docker** writes it (`work:360`) | `dispatch:2174` (the slice's room), `libexec/dev:272`, `libexec/stop` | **file**: a mount, written by docker, not by tehas (AGENTS.md rule 4) |
| `.oom` | `work:361` the count of OOM kills the host saw | `cycle` | table: `cycles.oom_kills` (C13) |
| `.forks` | `work:362` pids-limit refusals | `cycle` | table: `cycles.forks_refused` |
| `.stalled` | `work:363` the seconds | `cycle` | table: `cycles.stalled_seconds` |
| `.timed` | `work:364` | `cycle`, `libexec/work` | table: `cycles.timed_out` |
| `.runner_exit` | `work:373`; its *existence* is the proof the run was watched | `cycle` | table: `cycles.runner_exit` + `cycles.watched` |
| `.status` | `work:374`, the single word `killed` when the container ended 137 | `cycle` | table: `cycles.host_saw` |
| `.result.json` | `work:368`, the runner's JSON result | `cycle:392`, `lib/headless.sh`, `libexec/export` | table: `cycle_results` (3.6), whole payload as one row with its schema version |
| `.progress.json` | `work:369`, the summary of the run's NDJSON | `cycle:393`, `lib/cycles.sh` (its freshness proves whose log this is) | table: `cycle_progress` |
| `.manifest.json` | `work:367`, with `images.md` and `retry-context.md` beside it | `lib/attachments.sh:200`, `libexec/describe`, `libexec/export`, `libexec/stop` | **file**: mounted into the container as input (rule 4) |
| `.history.jsonl` | `libexec/cycle` at each end (`libexec/history`) | `libexec/history`, `lib/fixer.sh:151,159` | table: `cycle_history` (3.8) |
| `state/park-<n>.json` | `cycle:981` the question body, transiently | `cycle:982` (`post_question`), removed at `:1005` | table: `parks` (3.7). The `state/` round trip disappears |
| `state/park-<n>.id` | `cycle`, via `park_id` (`lib/cycles.sh:103`), the bot's comment id, one line | `libexec/dispatch` rule 4 (a tick counts only in this comment), `libexec/stop`, `host/docs/tools.md` | table: `questions` (3.7). This is the trust root of F5 |

### 1.4 Group C: the planning session's records

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/plan-<n>.json` | `lib/planning.sh:73` (`plan_state`), rewritten by every step of a turn | `libexec/dispatch` rules 4b and 5, `libexec/health:115`, `libexec/export:255`, `libexec/plan` | table: `planning` (3.5). The 13 fields are the columns: `session, turns, options, pending, failed, running, startedAt, comment, lost, by, label, answers, filed` |
| `state/plan-<n>.json.out` | `lib/planning.sh` while a turn's outcome is unposted | `plan_post`, `plan_finish` | table: `planning.pending_outcome` |
| `state/plan-<n>.json.brief` | `lib/planning.sh`, the approved text | `libexec/plan`, `plan_launch` | table: `planning.brief` |
| `state/plan-<n>.json.rc` | `plan-turn` (`lib/planning.sh:435`), the turn's exit status | `plan_collect`, `plan_recover` | table: `planning.turn_rc` |
| `state/plan-<n>.json.turn` | `lib/planning.sh:318` (moved aside) | `libexec/export:253` | table: `planning.turn_path` or removed once the export can read the row |
| `state/plan-<n>.json.keep/` (a directory: `plan-sessions`, `pi-plan-cache`) | `libexec/plan:448` (`carry`) | `libexec/plan:366` | **file, and flagged**: it holds **container-written** JSON inside `state/`, the directory trusted only because no container can write `state/`. It stays a directory (the runner's payload) and moves out of `state/` in #251, and nothing reads its contents as fact (F7) |
| `state/plan-<n>.json.lock` | group A | group A | file (A) |
| `state/plan-<n>.history.jsonl` | `libexec/history`, one line per turn | `libexec/history` | table: `plan_history` (3.8) |
| `state/refined.jsonl` | `lib/planning.sh:209` (`plan_refined`), one line per refine session | `host/docs/analytics.md:229`, `libexec/status` via the report | table: `refined` (3.8) |

### 1.5 Group D: the queue snapshot the poll publishes

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `live/dispatch.json` | `libexec/dispatch` (`report_hold`, `:674`): `why`, the slice's numbers, `endpoint` | `libexec/energy:26` (which endpoint to sample), `libexec/health:88`, `libexec/status`, `host/docs` | table: `polls` (3.9) as the source; **the published file stays** (view) |
| `live/demand.json` | `libexec/dispatch` (`report_demand`, `:1658`) `{$time, $waiting, $oldest, refreshHold}` | `libexec/health:97`, the pages | table: `poll_demand`; file stays (view) |
| `live/status.json` | `libexec/status:301`, **a fixed `.new` name** | `libexec/health:55`, `lib/top.sh:134`, the pages | table: `queue_snapshot`; file stays (view). The fixed `.new` is the collision the epic names |
| `live/health.json` | `libexec/health:239`, fixed `.new` | `libexec/summary:61`, `lib/top.sh:118`, Caddy at `/health` and `/healthz` | table: `health_samples`; file stays (view) |
| `live/health.down` | `libexec/health:242` | Caddy (`docs.Caddyfile:34`) answers 503 while it exists | **file**: it is an HTTP status code, decided by the server |
| `live/energy.json` | `libexec/energy:52`, fixed `.new` + `.` marker | `libexec/health`, `lib/top.sh:191`, `host/fun/energy-view.js` | table: `energy_samples`; file stays (view) |
| `live/platform.json` | `libexec/status:142`, fixed `.new` | `libexec/health:81`, the pages | table: `platform_state`; file stays (view) |
| `live/analytics.json` | `libexec/status` (the ten-day report) | the pages, MCP | file: it is a projection of `analytics.db`, not of this store (#252) |
| `live/top.json` | `libexec/top-publish:46`, marker `live/top.json.` | `host/fun/host-view.js` | file: host load, sampled by a unit every 5 s, not host state |
| `live/previews.json` | `libexec/preview:691`, marker `live/previews.json.` | `libexec/health:72`, the pages | file: previews are a different feature (#252 publishes it) |
| `live/protolab.json` | `libexec/protolab:432` | the pages | file: protolab is a different feature |
| `live/running.json` | `lib/live.sh` (`tehas_live_assemble`, `:27`; the rename is at `:79`) | `libexec/dev:258`, `libexec/health:56`, `lib/top.sh:149`, the pages | table: `cycles` is the source; the assembled file stays (view) |
| `live/cycles/<n>.json` | `libexec/work:785`, each cycle its own file, every 10 s | `lib/live.sh` (`tehas_live_assemble`, `:27`) | table: `heartbeats` (3.3); file stays as a view (B1, B3) |
| `live/endings.json` | `lib/live.sh` (`tehas_live_assemble`, the write at `:74`; **read-modify-written with no lock**) | `libexec/health:66` | table: `endings` (3.3); file stays (view) |
| `live/console-<n>.txt` | `libexec/work:759` | the pages, `libexec/stop:123`, MCP | **file**: the masked console, served with Range requests |
| `live/.running.<pid>.mark.new`, `.new`, `.view.new`, `live/.endings.<pid>.new` | `lib/live.sh:54-57` (inside `tehas_live_assemble`, which begins at `:27`) | `lib/live.sh` | **removed**: replaced by a transaction (#252). They are hidden from Caddy by `docs.Caddyfile`'s `@unfinished` matcher |

### 1.6 Group E: the poll's own records

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/dispatch.again` | `libexec/cycle:1192`, `lib/planning.sh:435`; **removed by the poll before it tries its lock** (`dispatch:365`) | `host/systemd/tehas-dispatch.path` (`PathExists=%h/tehas/state/dispatch.again`) | **file**: a systemd path unit triggers on a file's existence. It cannot trigger on a row |
| `state/dispatch.taken` | `dispatch:378`, `date +%s` | the poll itself (`:372`) and `host/docs/troubleshooting.md:212` | table: `polls.lock_taken_at`; the file stays only as long as the poll's own hang warning reads it, and #254 moves that to the row |
| `state/dispatch.stuck` | `dispatch:380`, the `taken` value it last complained about | the poll (dedupes the WARN) | table: `polls.stuck_said_for` |
| `state/pause.json` | `libexec/pause:52` (`{at, reason, by}`); `libexec/resume:50` removes it | `libexec/dispatch:1733`, `libexec/health:138`, `libexec/status:239` | table: `pause_flag` (3.10). A row that is absent means not paused; a row that cannot be read means paused (fail closed, as today) |
| `state/limit.json` | `lib/limit.sh:26` (`{v:1, cycles, plan, until, reason}`) | `libexec/limit`, `libexec/status`, `libexec/dispatch` rule 3, `tehas api` | table: `limits` (3.10) |
| `state/low-gate.held` | `dispatch:577` (touch), removed at `:587` | `dispatch:577` (`[ -e ]`), `host/docs/tools.md:251` | table: `poll_state.low_gate_held` |
| `state/branch-check.<something>` | `dispatch:657`, `:660` | `dispatch` (holds cycles when the branch is red) | table: `branch_checks` (3.10) |
| `state/check-product.json` (+ `.new`) | `libexec/check-product:146` | `libexec/status`, `libexec/summary:90`, `install.sh:938` | table: `check_product`; file stays because `install.sh` reads it before the command is trusted on a fresh host |
| `state/all-hands.ignored` | `libexec/status:219` | `libexec/status` (says it once) | table: `poll_state` |
| `state/platform-fetch` | `libexec/status:141`, the last fetch time | `libexec/status` (fetch at most once an hour) | table: `platform_state.last_fetch_at` |

### 1.7 Group F: refresh requests

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/refresh/<n>.json` | `libexec/refresh:13` (`tehas_refresh_dir`, `lib/refresh.sh:33`): `{at, head, ...}` | `libexec/dispatch` rule 4b, `libexec/status` | table: `refresh_requests` (3.11) |
| `state/refresh/.held` | `libexec/dispatch` (`report_demand`, the reason string) | `dispatch` (compares the 300 bytes before it logs) | table: `refresh_hold` |

### 1.8 Group G: the model endpoint's state

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/models.json` | `lib/sandbox.sh:126` (`tehas_models_resolve` writes the host's with each provider on the endpoint picked now) | **mounted read-only** into the container at `/home/vscode/.pi/agent/models.json`; `lib/models.sh` | **file**: a mount (rule 4) |
| `state/endpoints` | `lib/sandbox.sh:126`, the redirect target | `lib/models.sh` | table: `endpoints` (3.12); the file stays as what the mount is built from |

### 1.9 Group H: what the container is given

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/cache/` | `lib/sandbox.sh:169-176` copies the code index into it per issue; `libexec/clean` removes stale ones (`:245-269`) | **mounted** into the container | **file**: a mount, and `libexec/clean`'s per-issue age rule is a filesystem rule |
| `state/memory/` | **the container** (it is `/home/vscode/.vipune`) | the container | **file**: container-written; a claim, never a fact (F7) |
| `state/sessions/` | **the container** (`/home/vscode/.pi/agent/sessions`) | the container | **file**: same |
| `state/cycles/<n>.manifest.json`, `.images.md`, `.retry-context.md` | `work:367`, `lib/attachments.sh` | the container | **file**: mounted input (rule 4) |
| `dev/<name>.state/runs` | **the container** (`/home/vscode/.pi/agent/ensemble-runs`); the path is given at `libexec/dev:169` (`runs=$state/runs`) | the container; the mount is pinned by `test/dev_test.sh:211` and the set of subdirectories by `:221` | **file**: a mount, and a container writes it — the dev session's copy of `~/.pi/agent/ensemble-runs/`, so group L's rule for a cycle's runs applies: tehas removes the directory with the session and never parses it |
| `state/index/` | **the indexing container** writes it (`lib/sandbox.sh:50`, `libexec/index:9`) | `lib/cycles.sh` `tehas_index_state` (`:190`, looks for `*.db` and `_config.db`) | **file**: a container writes it; no cycle may see it |
| `state/index.json` | `libexec/index:105`, `{commit}` | `lib/cycles.sh:194`, `install.sh:1065` | table: `index_state`; **the file stays** because `install.sh` reads it on a host where the command may not exist yet |
| `state/index.failed` | `libexec/index:108`, the commit it failed at | `lib/cycles.sh:199` (`head -c 64`; `tehas_index_state` begins at `lib/cycles.sh:190`) | table: `index_state.failed_commit` |
| `state/index.rc` | the **index unit's own shell**, launched by `libexec/dispatch:1372` (`index_rc=$TEHAS_HOME/state/index.rc`) and written at `:1436` by a `mv` from `.rc.new`; `libexec/index` writes nothing to it | `libexec/dispatch` `index_report` (`:1410`: `[ -f "$index_rc" ] \|\| return 1`) and `index_busy` (`:1423`: `[ ! -f "$index_rc" ] || return 1`); both remove it
after reading (`:1412`, `:1436`). `test/dispatch_checkout_test.sh:67,73,78` pin the read, the removal and both exit codes | **file**: its *existence* is the signal that a run has ended and has not yet been reported, and the unit that writes it is the only reader — a row cannot be the event that a poll looks for before it knows whether to look (the same shape as `state/dispatch.again` in group D, which differs in one way worth naming: the poll removes that file *before* it tries its lock, at `dispatch:365`, whereas `index.rc` is removed *after* the poll has read it) |

### 1.10 Group I: the release and install records

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/update-pending` | `lib/update-steps.sh:177` (`tehas_pending_file`), one line per owed step | `install.sh:56`, `install.sh --check`, `libexec/update` | **file**: `install.sh` reads it before the command exists |
| `state/update-hold` | `touch` by a person, checked at `libexec/update:231` | `libexec/update` | **file**: a person must be able to make it with `touch`, with no command installed |
| `state/first-install` | `install.sh:544` | `install.sh` | **file** |
| `state/sandbox.Dockerfile.sha256` | `install.sh:457` | `install.sh` (decides whether the sandbox build is owed) | **file**: read by `install.sh` before anything else runs |
| `state/release-history` | `lib/update-release.sh:231` (append), kept to 3 | `lib/update-release.sh:238`, `host/release/install:130` | table: `release_history` (3.12); the file stays as the fallback for `host/release/install` |
| `state/edge-takeover` | `libexec/update:721`, the time it was made | `libexec/update` (the edge's compose may be edited by hand for 48 h) | table: `edge_takeover` |

### 1.11 Group J: the improvement loop

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/improve.cap` | `libexec/improve:106` (a run that finishes in time doubles it) | `libexec/improve`, `host/docs/tools.md:2273` | table: `improve_cap` |
| `state/improve.lock` | group A | group A | file (A) |

### 1.12 Group K: the analytics database

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `state/analytics.db` (+ `-wal`, `-shm`) | `libexec/analytics` (`console/analytics/`), called through `lib/analytics.sh` by `cycle`, `dispatch`, `status`; `lib/cycles.sh` | `libexec/status`, `libexec/analytics-cmd`, `libexec/mcp`, `tehas api` | **file, deliberately** (ADR 0003): append-only history with its own `PRAGMA user_version` (`console/analytics/src/db.rs:10`, `:140`, `:170`, and `schema.sql`), its own readers and its own prune |
| `state/analytics.db.broken`, `state/analytics.db.schema-<v>` | `console/analytics/src/db.rs:24-26` | the same program | file: it is that program's recovery path |

### 1.13 Group L: outside `$TEHAS_HOME`, but the same life cycle

Named because `libexec/stop` removes them and the epic counts them.

| Path | Writer | Readers | Fate |
| --- | --- | --- | --- |
| `~/.pi/agent/ensemble-runs/issue-<n>` | pi-rukas, for a cycle (`libexec/work:356`) | pi-rukas | **file**, and never read as fact (#229): tehas removes it and does not parse it |
| `~/.pi/agent/ensemble-runs/plan-<n>` | pi-rukas, for a turn (`libexec/plan:191`) | pi-rukas | file: same |
| `work/<repo>-<n>`, `.state`, `.before-retry`, `.images` | `libexec/work`, `libexec/plan` | the cycle, `libexec/clean`, `libexec/dispatch`'s sweep (`:2117`) | **file** (B5: a retry never deletes unpushed work); the sweep's *decisions* become rows in `sweep_log` (3.13) |
| `logs/dispatch.log`, `logs/cycle-<n>-<stamp>.log`, `logs/plan-<n>-<stamp>.jsonl`, `logs/low-gate.log`, `logs/dispatch-plan-<n>.log` | the units | people, `libexec/export`, the MCP server | file, with a retention rule that #257 must fix (today: rotated only when the loop is on) |

### 1.14 Files the epic's research counted that no longer exist

Checked on main: **gone**, so a plan built on the old inventory would move
state that is not there.

- the pi-rukas canary and `state/rukas-proven` (removed in `09757e9`)
- every read of `.pi/work-state/<n>.json` (#229). The only trace left is
  `test/internals_test.sh:53`, which pins that tehas does *not* read it
- tehas's own park comment from the runner's copy (#232)
- the OOM and image records tehas used to keep for the runner (#229)

**Added since the research**, and in the tables above: `state/cycles/<n>.result.json`,
`.progress.json`, `.history.jsonl`, `.settings`, `.timed`, `.done`, `.status`,
`.runner_exit`, `.oom`, `.forks`, `.stalled`, `.branch`, `.quiet`, `.by`;
`state/dispatch.again`, `state/endpoints`, `state/models.json`,
`state/pause.json`, `state/limit.json`, `state/low-gate.held`,
`state/branch-check.*`, `state/refresh/<n>.json` and `.held`,
`state/release-history`, `state/edge-takeover`, `state/all-hands.ignored`,
`state/platform-fetch`.

### 1.15 Group M: `state/` files no command reads

The audit verb reads the rule, and this section is what says so. A name in
`state/` that **no writer claims** is either an orphan
from an older version or something a container touched. A name that working code
**does** claim and that appears nowhere in the tables above is the other failure,
and the worse one: the import leaves it behind and nothing reports it. So the
audit verb reports two sets, not one, and the second is the check that keeps this
inventory honest:

- `unclaimed` — names in `state/` with no writer. The sweep of `install.sh:1008-1013`
  already clears one such family (`state/cycles/<n>`, one empty file per issue,
  from a version that predates the extensions).
- `undocumented` — names on disk that some command writes but no row of section 1
  names, taken from a list of the paths the installed code touches. `state/index.rc`
  and `dev/<name>.state/runs` are the two this spec found the slow way, by
  reading, after its first draft. #250 adds the check to the gate so the next
  one is found by a failing test.

## 2. The 13 groups that change together

Each row is one verb of section 4, and one verb is one transaction. The files
column is the set that must not be observable half-changed.

| # | Group | Files that move together today | Verb |
| --- | --- | --- | --- |
| 1 | A cycle's start | `cycles/<n>.taken`, `.by`, `.requeued`, `.approved.json`, `.rukas`, `.image`, `.settings`, the label `tehas:running`, and the container's start | `cycle-start` |
| 2 | A cycle's end | `.runner_exit`, `.status`, `.oom`, `.forks`, `.stalled`, `.timed`, `.progress.json`, `.result.json`, `.done`, `.history.jsonl`, `.approved.json` (removed), `live/cycles/<n>.json`, `live/endings.json` | `cycle-end` |
| 3 | A park | `.park`, `state/park-<n>.json`, `state/park-<n>.id`, the labels, the question comment | `park` |
| 4 | A queue transition | `live/dispatch.json`, `live/demand.json`, `state/low-gate.held`, `state/branch-check.*`, the analytics sample | `poll-commit` |
| 5 | Pause and resume | `state/pause.json`, `live/status.json`'s pause fields | `pause-set` / `pause-clear` |
| 6 | A planning turn | `plan-<n>.json`, `.out`, `.brief`, `.rc`, `.turn`, `plan-<n>.history.jsonl`, the `tehas:planning` label, the question comment | `plan-turn-begin` / `plan-turn-end` |
| 7 | A refresh request | `state/refresh/<n>.json`, `state/refresh/.held`, `tehas:refreshing` on the pull request, `.approved.json` | `refresh-request` / `refresh-settle` |
| 8 | A label decision | the label on GitHub, `cycles/<n>.approved.json`, `state/park-<n>.id`, the comment that explains it, the journal | `journal-record` + the host verb of the record the label belongs to (`cycle-start`, `park` or `refresh-request`): the joint write this spec calls `label-apply`, not a verb |
| 9 | The limits | `state/limit.json`, `live/status.json`'s limit fields, `tehas api`'s answer | `limit-set` / `limit-clear` |
| 10 | The endpoint | `state/endpoints`, `state/models.json`, `live/dispatch.json`'s `endpoint` | `endpoint-settle` |
| 11 | The index | `state/index.json`, `state/index.failed`, `state/index.rc`, `state/index/`, `state/cache/` | `index-record` |
| 12 | The release | `state/release-history`, `state/update-pending`, `state/edge-takeover`, `state/sandbox.Dockerfile.sha256`, `state/check-product.json` | `release-record` |
| 13 | The sweep | `live/endings.json`, `live/cycles/*.json`, the workspaces and their `.state`, `.before-retry`, `.images`, `state/cycles/<n>.quiet` | `sweep` |

## 3. Schema v1

`$TEHAS_HOME/state/tehas.db`, mode 0600, owned by the tehas user. Never under
`live/`, never under `state/index`, never in a path a container mounts: no
container sees it (AGENTS.md rule 4).

Conventions, taken from `console/analytics/src/schema.sql` so the two databases
read alike:

- Times are **columns**, UTC Unix seconds, never file mtimes. A column named
  `*_at` is an instant; `*_seconds` is a duration.
- The version is `PRAGMA user_version`. It is 1 for this schema. Migrations run
  forward only, from install or update under `state/update.lock`, after a
  `VACUUM INTO` backup.
- WAL, `busy_timeout` 5000 ms, `synchronous=FULL`, `foreign_keys=ON`. Writes
  stay under one a second, so durability costs nothing worth saving.
- Booleans are `INTEGER` 0 or 1. Lists that are queried are child tables; lists
  that are only ever read whole are JSON text with a `_json` suffix.
- Every table that a person would ask about has a `*` free-text column only if
  the value is tehas's own words. Nothing from a container is stored (F7).

The "replaces" note under each table names the files of section 1 it retires.
Where a file stays a file, the row is the source and the file is a view.

### 3.1 `meta`

```sql
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

Holds `schema` (the `user_version` mirror, for a reader that cannot PRAGMA),
`created_at`, `tehas_version`, `tehas_commit`, `instance`, `repo`. Nothing about
the host's identity, no token, no address. Replaces nothing; it is the record
the files never had.

### 3.2 `issues`

Every number the factory has an opinion about, keyed by the number itself.
Replaces: no single file; it is the join target the files all lacked.

```sql
CREATE TABLE issues (
  issue        INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL CHECK (kind IN ('issue', 'pr', 'closed', 'unreadable')),
  kind_at      INTEGER NOT NULL,          -- when kind was last read (A7)
  title        TEXT,
  body_sha256  TEXT,                      -- the text an approval binds to (F3)
  edited_at    INTEGER,
  edited_by    TEXT,
  state        TEXT,                       -- 'open' | 'closed', as GitHub answered
  state_at     INTEGER,
  labels_json  TEXT,                      -- the label names, as one read returned them
  labels_at    INTEGER,
  pr_number    INTEGER,                    -- the bot's open pull request that closes it
  pr_url       TEXT,
  pr_state     TEXT,
  pr_head_sha  TEXT,
  pr_at        INTEGER,
  checked_at   INTEGER NOT NULL            -- when this row was last refreshed
);
CREATE INDEX issues_checked ON issues (checked_at);
```

An answer that could not be read is a row with `kind='unreadable'` and a recent
`kind_at`, never a missing row: the poll must not read "no row" as "not work"
(D2, A14). `checked_at` is what makes A15 ("status shows the age of its data")
a query.

### 3.3 `cycles`, `cycle_locks`, `heartbeats`, `endings`

Replaces: `.taken`, `.by`, `.requeued`, `.done`, `.rukas`, `.image`, `.branch`,
`.oom`, `.forks`, `.stalled`, `.timed`, `.runner_exit`, `.status`, and
`live/cycles/<n>.json`, `live/endings.json`.

```sql
CREATE TABLE cycles (
  issue          INTEGER PRIMARY KEY REFERENCES issues (issue) ON DELETE CASCADE,
  attempt        INTEGER NOT NULL DEFAULT 1,
  started_at     INTEGER NOT NULL,        -- was: the mtime of cycles/<n>.taken
  started_by     TEXT NOT NULL,           -- was: cycles/<n>.by  (F6)
  how            TEXT NOT NULL,           -- ready|requeued|retry|refresh|outage
  requeued       INTEGER NOT NULL DEFAULT 0,
  rukas_commit   TEXT,                    -- was: cycles/<n>.rukas (B7)
  image          TEXT,                    -- was: cycles/<n>.image (B7)
  branch         TEXT,                    -- was: cycles/<n>.branch
  end_reason     TEXT CHECK (end_reason IN ('closed','merged','handoff','parked','failed','stopped','timeout','killed','lost')),
  ended_at       INTEGER,
  oom_kills      INTEGER,                 -- was: .oom  (C13)
  forks_refused  INTEGER,                 -- was: .forks
  stalled_seconds INTEGER,               -- was: .stalled
  timed_out      INTEGER NOT NULL DEFAULT 0,   -- was: .timed
  runner_exit    INTEGER,                 -- was: .runner_exit
  watched        INTEGER NOT NULL DEFAULT 0,   -- .runner_exit existed at all
  host_saw       TEXT CHECK (host_saw IN ('killed')),  -- was: .status, the word 137
  stop_asked_at  INTEGER,                 -- C6: the stop that was asked, spaced
  stop_said_times INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX cycles_running ON cycles (ended_at, started_at);

-- The cache of what the flocks say. NEVER the truth (C5): a cycle is running
-- while its lock is held, and this table only records that a lock was found
-- held the last time something looked.
CREATE TABLE cycle_locks (
  issue      INTEGER PRIMARY KEY REFERENCES cycles (issue) ON DELETE CASCADE,
  held       INTEGER NOT NULL,
  seen_at    INTEGER NOT NULL,           -- when the probe ran
  probe_rc   INTEGER NOT NULL            -- 0 free, 1 held, 2 could not tell
);

-- One row per cycle per poll or per tick of the supervisor's 10-second write.
-- Replaces live/cycles/<n>.json, whose mtime was the heartbeat.
CREATE TABLE heartbeats (
  issue      INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  at         INTEGER NOT NULL,
  running    INTEGER NOT NULL,
  step       TEXT,
  status     TEXT,
  PRIMARY KEY (issue, at)
);
CREATE INDEX heartbeats_at ON heartbeats (at);

-- The ten last ends, oldest first: was live/endings.json, read-modified-written
-- with no lock (B1).
CREATE TABLE endings (
  issue       INTEGER NOT NULL,
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER NOT NULL,
  status      TEXT NOT NULL,
  step        TEXT,
  PRIMARY KEY (issue, started_at)
);
CREATE INDEX endings_recent ON endings (ended_at DESC);
```

### 3.4 `approvals`

Replaces `state/cycles/<n>.approved.json` (which today is grepped by glob from
`dispatch:2061` to find any refresh-driven start).

```sql
CREATE TABLE approvals (
  issue        INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  at           INTEGER NOT NULL,
  by_account   TEXT NOT NULL,             -- the actor of the labeled event (F1)
  how          TEXT NOT NULL,             -- ready|urgent|plan|prototype|retry|refresh
  label        TEXT NOT NULL,             -- the tehas:* label it rests on
  body_sha256  TEXT NOT NULL,             -- the text that was approved (F3)
  title_sha256 TEXT,
  comment_id   INTEGER,                   -- the comment that carries the tick
  refresh_pr   INTEGER,                   -- for how='refresh': the pull request
  refresh_head TEXT,
  used_at      INTEGER,                   -- when it authorised a start (A2, A3)
  used_by      INTEGER,                   -- the cycle that consumed it
  PRIMARY KEY (issue, at)
);
CREATE INDEX approvals_open ON approvals (issue, used_at);
```

One approval is one start: a `used_at` that is null is one start still owed
(A3), and a retry tick counts only on a parked issue (A2). The label change and
this row are written in one `label-apply` transaction with the journal row
(section 5).

### 3.5 `planning`

Replaces `state/plan-<n>.json` and its `.out`, `.brief`, `.rc`, `.turn`. The
13 fields of `lib/planning.sh:10-31` become 13 columns; nothing was dropped.

```sql
CREATE TABLE planning (
  issue         INTEGER PRIMARY KEY REFERENCES issues (issue) ON DELETE CASCADE,
  session       TEXT,                     -- the Pi session id; a lost one ends the session (B8)
  turns         INTEGER NOT NULL DEFAULT 0,
  options_json  TEXT,                     -- the labels of the open question, in order
  pending       INTEGER NOT NULL DEFAULT 0,
  pending_outcome TEXT,                   -- was: <record>.out
  failed        INTEGER NOT NULL DEFAULT 0,
  running       INTEGER NOT NULL DEFAULT 0,
  started_at    INTEGER,                  -- when the turn in flight started
  comment_id    INTEGER,                  -- the open question comment
  lost          INTEGER NOT NULL DEFAULT 0,
  by_account    TEXT,                     -- who applied tehas:plan|tehas:refine
  label         TEXT CHECK (label IN ('tehas:plan','tehas:refine')),
  brief         TEXT,                     -- was: <record>.brief
  turn_rc       INTEGER,                  -- was: <record>.rc
  opened_at     INTEGER NOT NULL,
  ended_at      INTEGER
);
CREATE TABLE planning_answers (           -- was: the `answers` array
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  turn  INTEGER NOT NULL,
  by_account TEXT NOT NULL,
  fixer      INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (issue, turn)
);
CREATE TABLE planning_filed (             -- was: the `filed` array
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  filed_issue INTEGER NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, filed_issue)
);
```

`planning` keeps its brief, its filed markers and its session across turns
(B8), and a lost session fails with its own reason. `<record>.lock` stays a
file: it is a turn's liveness proof.

### 3.6 `cycle_settings`, `cycle_notes`, `cycle_results`, `cycle_progress`

Replaces `.settings`, `.quiet`, `.result.json`, `.progress.json`.

```sql
CREATE TABLE cycle_settings (             -- was: cycles/<n>.settings
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  key   TEXT NOT NULL,                    -- a PI_ENSEMBLE_* name
  value TEXT NOT NULL,                    -- never a token
  PRIMARY KEY (issue, key)
);
CREATE TABLE cycle_notes (                -- was: cycles/<n>.quiet
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  kind  TEXT NOT NULL,                    -- 'quiet' today; the set is closed by tests
  said_for TEXT NOT NULL,                 -- the episode that was said once
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, kind, said_for)
);
-- The runner's result, kept whole: its shape is the runner's, and tehas must
-- not lose a field it does not yet parse (D3).
CREATE TABLE cycle_results (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  attempt INTEGER NOT NULL,
  schema_version INTEGER NOT NULL,        -- the runner's own version
  payload TEXT NOT NULL,                  -- the JSON, exactly as written
  received_at INTEGER NOT NULL,
  PRIMARY KEY (issue, attempt)
);
CREATE TABLE cycle_progress (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  attempt INTEGER NOT NULL,
  reason TEXT,
  review_held INTEGER,
  steps_json TEXT,
  roles_json TEXT,
  written_at INTEGER NOT NULL,
  PRIMARY KEY (issue, attempt)
);
```

### 3.7 `parks`, `questions`

Replaces `.park`, `state/park-<n>.json` and `state/park-<n>.id`.

```sql
CREATE TABLE parks (
  issue    INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  attempt  INTEGER NOT NULL,
  at       INTEGER NOT NULL,
  reason   TEXT NOT NULL,                 -- too-large, verify-failed:<step>, provider-outage, ...
  step     TEXT,
  pr       INTEGER,
  retries  INTEGER NOT NULL DEFAULT 0,    -- C12: one automatic retry, then a person
  PRIMARY KEY (issue, attempt)
);
-- The trust root of F5: a tick counts only in a comment this host put here.
CREATE TABLE questions (
  id         INTEGER PRIMARY KEY,         -- GitHub's databaseId
  issue      INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  attempt    INTEGER NOT NULL,
  kind       TEXT NOT NULL CHECK (kind IN ('park','plan','blocked','fixer')),
  posted_at  INTEGER NOT NULL,
  body_sha256 TEXT,
  answered_at INTEGER,
  answered_by TEXT
);
CREATE INDEX questions_issue ON questions (issue, posted_at);
```

`questions` is the only place a comment id is believed. A comment that is not
in this table is not the factory's question, whatever markers it carries.

### 3.8 `cycle_history`, `plan_history`, `refined`

Replaces `state/cycles/<n>.history.jsonl`, `state/plan-<n>.history.jsonl`,
`state/refined.jsonl`. Append-only, one row per end, so `libexec/history` and
`lib/fixer.sh` stop parsing JSONL.

```sql
CREATE TABLE cycle_history (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, attempt INTEGER NOT NULL, at INTEGER NOT NULL,
  status TEXT NOT NULL, step TEXT, by_account TEXT, how TEXT, pr INTEGER
);
CREATE INDEX cycle_history_issue ON cycle_history (issue, at);
CREATE TABLE plan_history (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, turn INTEGER NOT NULL, at INTEGER NOT NULL,
  outcome TEXT NOT NULL, session TEXT
);
CREATE INDEX plan_history_issue ON plan_history (issue, at);
CREATE TABLE refined (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, at INTEGER NOT NULL, kept INTEGER, outcome TEXT
);
```

### 3.9 `polls`, `poll_demand`, `queue_snapshot`, `health_samples`, `energy_samples`, `platform_state`

Replaces the writers of `live/dispatch.json`, `live/demand.json`,
`live/status.json`, `live/health.json`, `live/energy.json`, `live/platform.json`,
`state/dispatch.taken`, `state/dispatch.stuck`, `state/all-hands.ignored`,
`state/platform-fetch`. The live files stay as published views (#252).

```sql
CREATE TABLE polls (
  at            INTEGER PRIMARY KEY,      -- when the poll took its lock
  lock_taken_at INTEGER NOT NULL,         -- was: state/dispatch.taken
  stuck_said_for INTEGER,                 -- was: state/dispatch.stuck
  held          TEXT,                     -- why nothing started; null when nothing held it
  short_by      INTEGER NOT NULL DEFAULT 0,
  slice_room INTEGER, slice_committed INTEGER, slice_max INTEGER, slice_why TEXT,
  endpoint      TEXT CHECK (endpoint IN ('main','backup')),
  paused        INTEGER NOT NULL DEFAULT 0,
  cycles_running INTEGER, in_line INTEGER, awaiting INTEGER, planning INTEGER,
  exit_code     INTEGER,                  -- how the poll ended
  settled       INTEGER NOT NULL DEFAULT 0  -- 0 until its journal rows are settled
);
CREATE INDEX polls_recent ON polls (at DESC);
CREATE TABLE poll_demand (
  at INTEGER PRIMARY KEY REFERENCES polls (at) ON DELETE CASCADE,
  waiting INTEGER NOT NULL, oldest_at INTEGER, refresh_hold TEXT
);
CREATE TABLE queue_snapshot (
  at INTEGER PRIMARY KEY,                 -- when the read was taken, not when it was written
  in_line INTEGER, running INTEGER, awaiting INTEGER, stopped INTEGER,
  completed INTEGER, planning INTEGER, all_hands INTEGER, all_hands_ignored_at INTEGER,
  paused INTEGER, pause_reason TEXT, snapshot_age_seconds INTEGER
);
CREATE TABLE health_samples (
  at INTEGER PRIMARY KEY, ok INTEGER NOT NULL, why TEXT, down INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE energy_samples (
  at INTEGER PRIMARY KEY, endpoint TEXT, watts REAL, source TEXT, ok INTEGER NOT NULL
);
CREATE TABLE platform_state (
  key TEXT PRIMARY KEY, value TEXT NOT NULL   -- commit, behind, last_fetch_at
);
```

### 3.10 `pause_flag`, `limits`, `branch_checks`, `check_product`

Replaces `state/pause.json`, `state/limit.json`, `state/branch-check.*`,
`state/check-product.json` (the file stays for `install.sh`).

```sql
CREATE TABLE pause_flag (                 -- was state/pause.json
  id INTEGER PRIMARY KEY CHECK (id = 1),  -- at most one flag
  at INTEGER NOT NULL, reason TEXT NOT NULL, by_account TEXT NOT NULL,
  unreadable INTEGER NOT NULL DEFAULT 0   -- a flag that cannot be read is paused
);
CREATE TABLE limits (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  cycles INTEGER, plan INTEGER, until_at INTEGER, reason TEXT, set_at INTEGER NOT NULL
);
CREATE TABLE branch_checks (
  name TEXT NOT NULL, head_sha TEXT NOT NULL, at INTEGER NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('red','done','pending','none','unreadable')),
  PRIMARY KEY (name, head_sha)
);
CREATE TABLE check_product (              -- the file stays: install.sh reads it
  at INTEGER PRIMARY KEY, ok INTEGER NOT NULL, failures_json TEXT
);
```

### 3.11 `refresh_requests`, `refresh_hold`

Replaces `state/refresh/<n>.json` and `state/refresh/.held`.

```sql
CREATE TABLE refresh_requests (
  issue INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  at INTEGER NOT NULL, head_sha TEXT NOT NULL, pr INTEGER NOT NULL,
  by_account TEXT NOT NULL,               -- whose approval it rests on (F6)
  settled_at INTEGER, outcome TEXT,       -- started | held:<reason> | failed:<reason>
  PRIMARY KEY (issue, at)
);
CREATE TABLE refresh_hold (
  id INTEGER PRIMARY KEY CHECK (id = 1), reason TEXT NOT NULL, since_at INTEGER NOT NULL
);
```

### 3.12 `endpoints`, `index_state`, `release_history`, `edge_takeover`

Replaces `state/endpoints`, `state/index.json` + `state/index.failed` (both
files stay for `install.sh`), `state/release-history`, `state/edge-takeover`.

```sql
CREATE TABLE endpoints (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  main TEXT NOT NULL, backup TEXT, settled_at INTEGER NOT NULL, source TEXT
);
CREATE TABLE index_state (
  commit TEXT PRIMARY KEY, built_at INTEGER, failed_at INTEGER,
  state TEXT NOT NULL CHECK (state IN ('current','failed','due'))
);
CREATE TABLE release_history (
  tag TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (tag, at)
);
CREATE TABLE edge_takeover (
  id INTEGER PRIMARY KEY CHECK (id = 1), made_at INTEGER NOT NULL, note TEXT
);
```

### 3.13 `actions`, `journal`, `sweep_log`

The journal is section 5; `sweep_log` records what the sweep decided, so the
rules of B5 and B6 are auditable rather than inferred from which directories are
left.

```sql
CREATE TABLE sweep_log (
  issue INTEGER NOT NULL, at INTEGER NOT NULL,
  action TEXT NOT NULL CHECK (action IN ('kept','removed','would-remove')),
  what TEXT NOT NULL, bytes INTEGER, reason TEXT NOT NULL,
  PRIMARY KEY (issue, at, what)
);
```

### 3.14 What is deliberately not in the schema

- **No tokens, no addresses, no keys.** Not in a column, not in a JSON blob, not
  in a default. `cycle_settings` holds `PI_ENSEMBLE_*` names and non-secret
  values, exactly as `.settings` does today.
- **Nothing a container wrote is a fact.** `state/memory/`, `state/sessions/`
  and the container's claims stay out of the database and are read, when they
  are, as claims (F7). The row that records `merged` requires a merged pull
  request that names the issue, read from GitHub.
- **No `live/`-shaped tables that only mirror a file.** A table exists because
  something queries it, not because a file existed.

## 4. The command

`tehasbot store <verb> ...`, a Rust binary in the workspace of
`console/Cargo.toml`, installed as `~/tehas/libexec/store`, and the only writer
of `state/tehas.db`. Bash never writes SQL: the shell commands call the verb and
read its JSON. It is a sibling of `libexec/analytics`, and borrows that
program's conventions (`TEHAS_HOME`, `--json`, a bounded call).

Common rules:

- It loads the instance settings from the same instance directory the shell's
  `tehas_load` reads (`lib/config.sh:345`) and validates them the same way, so
  a malformed setting stops it with 78 as it stops every other command. It does
  not source `tehas_load` and it does not need to: the command is run, never
  sourced. **`tehas_load` itself never opens the database** (AGENTS.md rule 3):
  a login shell sources it, and a login must not fail or block on the store.
  Loading settings keeps no side effects, makes no network call and writes
  nothing.
- **A missing database, or one whose `user_version` is newer than this binary
  knows, exits 78 before any GitHub, Docker or git call** and before it writes
  anything. A missing database is not "empty host"; only `store init` creates one.
- `--json` on every verb. Output is one object on stdout, `"api": 1` first, as
  `tehas api` does (`host/docs/api.md`). Diagnostics go to stderr.
- Every external call (`gh`, `docker`, `git`, `systemctl`) has a timeout and a
  typed result: `ok`, `empty`, `read-failed`, `timed-out`. A failed read is
  never taken for an empty one (D2, A14).
- Exit statuses, the same set the rest of tehas uses:

  | Exit | Meaning |
  | --- | --- |
  | 0 | the verb did its work, or there was nothing to do |
  | 64 | an argument this verb does not know; nothing was written |
  | 78 | the instance settings, or the database: missing, malformed, unreadable, or newer than this command |
  | 1 | a call failed, an answer could not be read, or a precondition the verb states was not met. Nothing was committed |
  | 5 | (verbs of section 5 only) the action was journaled and not settled; the next poll settles it. State is consistent, so this is not a failure of the write |

- `PRAGMA busy_timeout=5000`, then a busy database is exit 1 with the word
  `busy` on stderr and **never exit 0 with no line** (D4).
- Writes are one transaction. A verb that fails midway commits nothing, and the
  log line names the verb and the reason.

### The verbs

Arguments are positional where the shell already passes them positionally;
flags otherwise. `[...]` is optional. Output is the interesting part of the JSON
only; every response also carries `{"api":1,"verb":"...","ok":true}`.

| Verb | Arguments | Writes (one transaction) | JSON | Exit |
| --- | --- | --- | --- | --- |
| `init` | `--force` | `meta`, `PRAGMA user_version=1`; refuses if the file exists unless `--force` | `{"schema":1,"created":true}` | 0, 64, 78, 1 |
| `migrate` | `--backup <dir>` | forward migration, after `VACUUM INTO` | `{"from":1,"to":2,"backup":"..."}` | 0, 64, 78, 1 |
| `check` | | reads only: version, integrity_check, row counts, and the section-7 divergence | `{"schema":1,"integrity":"ok","rows":{...}}` | 0, 78, 1 |
| `state-audit` | `--json` | reads only: the two sets of section 1.15 | `{"unclaimed":[{"path":"...","age_seconds":N}],"undocumented":[{"path":"...","writer":"dispatch"}]}` | 0, 78, 1 |
| `issue-read` | `<issue>` `[--force]` | `issues` from a fresh read | the row | 0, 64, 78, 1 |
| `issue-kind` | `<issue>` | `issues.kind`, `kind_at` | `{"kind":"issue"}` | 0, 78, 1 |
| `cycle-start` | `<issue> --by <acct> --how <how> [--requeued] [--rukas <c>] [--image <i>] [--branch <b>] --lock <path>` | **group 1**: `cycles`, `approvals.used_at`, `cycle_settings`, the journal row | `{"issue":12,"attempt":2,"started_at":...}` | 0, 64, 78, 1, 5 |
| `cycle-end` | `<issue> [--result <file>] [--progress <file>] [--oom N] [--forks N] [--stalled N] [--timed] [--host-saw killed] --status <s> [--step <s>]` | **group 2**: `cycles`, `cycle_results`, `cycle_progress`, `endings`, `heartbeats`, `cycle_history`, clears `approvals` | the end row | 0, 64, 78, 1 |
| `cycle-show` | `<issue> \| --running` | | the running cycles with the lock probe's answer | 0, 64, 78 |
| `cycle-heartbeat` | `<issue> --step <s> [--status <s>]` | `heartbeats` | `{"at":...}` | 0, 78 |
| `cycle-note` | `<issue> --kind quiet --said-for <text>` | `cycle_notes` | `{"said":true}` | 0, 78 |
| `cycle-stop-asked` | `<issue>` | `cycles.stop_asked_at`, `stop_said_times` | `{"times":2}` | 0, 78 |
| `park` | `<issue> --reason <r> [--step <s>] [--pr N] --comment-id <id> --body-sha <sum>` | **group 3**: `parks`, `questions`; the comment is posted by the caller through the journal | `{"issue":12,"question":<id>}` | 0, 64, 78, 1, 5 |
| `park-show` | `<issue>` | | the current park, `retries` | 0, 78 |
| `park-retry` | `<issue>` | increments `parks.retries` | `{"retries":1,"automatic":true}` | 0, 78, 1 |
| `question-asked` | `<issue> --id <id> --kind <k> [--attempt N]` | `questions` | the row | 0, 64, 78 |
| `question-known` | `<issue> --id <id>` | | `{"known":true,"kind":"park"}` | 0, 64, 78, 1 |
| `approval-record` | `<issue> --by <acct> --how <h> --label <l> --body-sha <sum> [--comment-id <id>] [--refresh-pr N --refresh-head <sha>]` | `approvals` | the row | 0, 64, 78 |
| `approval-current` | `<issue>` | | the open approval, or `{"approval":null}` | 0, 78 |
| `plan-begin` | `<issue> --by <acct> --label <l> --brief-file <f> [--session <id>] [--turn N]` | **group 6**: `planning`, `plan_history` | the row | 0, 64, 78 |
| `plan-end` | `<issue> --outcome <o> [--session <id>] [--turn N] [--rc N] [--filed 1,2,3]` | `planning`, `plan_history` | the row | 0, 64, 78 |
| `plan-answer` | `<issue> --turn N --by <acct> [--fixer]` | `planning_answers` | `{"turn":2}` | 0, 64, 78 |
| `plan-show` | `<issue> \| --open` | | the rows | 0, 78 |
| `poll-begin` | | `polls` (the lock's instant), clears nothing else | `{"at":...}` | 0, 78 |
| `poll-commit` | `--held <why> \| --held-none [--short N] [--slice-room N --slice-committed N --slice-max N --slice-why <w>] [--endpoint main\|backup] [--paused] [--all-hands 0\|1]` | **group 4**: `polls`, `poll_demand`, `queue_snapshot`, the sample the analytics program is then given | the row | 0, 64, 78 |
| `poll-show` | `--last` | | the last poll, with its age (A15) | 0, 78 |
| `pause-set` | `--reason <r> --by <acct>` | **group 5**: `pause_flag` | the row | 0, 64, 78 |
| `pause-clear` | | `pause_flag` deleted | `{"paused":false}` | 0, 78 |
| `pause-show` | | | `{"paused":true,"at":...,"reason":"..."}` | 0, 78 |
| `limit-set` | `[--cycles N] [--plan] [--until <iso>] --reason <r>` | **group 9**: `limits` | the row | 0, 64, 78 |
| `limit-clear` | | | `{"state":"none"}` | 0, 78 |
| `limit-show` | | | as `tehas api`'s `limit` object | 0, 78 |
| `branch-check-record` | `--name <n> --head <sha> --state <s>` | **group 4** companion: `branch_checks` | the row | 0, 64, 78 |
| `branch-check-show` | `--name <n> --head <sha>` | | `{"state":"pending"}`; `unreadable` holds nothing (rule 3) | 0, 78 |
| `endpoint-settle` | `--main <name> [--backup <name>] --source <s>` | **group 10**: `endpoints` | the row | 0, 64, 78 |
| `index-record` | `--commit <c> --state current\|failed\|due` | **group 11**: `index_state` | the row | 0, 64, 78 |
| `index-show` | `--commit <c>` | | `{"state":"due"}` | 0, 64, 78 |
| `release-record` | `--tag <t>` `[--edge-takeover]` | **group 12**: `release_history`, `edge_takeover` | the row | 0, 64, 78 |
| `refresh-request` | `<issue> --pr N --head <sha> --by <acct>` | **group 7**: `refresh_requests` | the row | 0, 64, 78, 1 |
| `refresh-settle` | `<issue> --at <t> --outcome <o>` | `refresh_requests.settled_at`, `refresh_hold` | the row | 0, 64, 78 |
| `refresh-pending` | | | the open requests | 0, 78 |
| `snapshot-load` | `--from <file>` `[--budget-left N]` | `issues`, the label events, `questions` for the edits, closing pull requests: everything of section 6 that one query returned; refuses a page whose section-6 bound exceeds `--budget-left` | `{"issues":N,"stale":M,"requests":K,"points":P,"pages":G}` | 0, 64, 78, 1 |
| `sweep` | `--removed <n>... \| --kept <n>... --reason <r> [--bytes N]` | **group 13**: `sweep_log`, `endings`, `heartbeats` | the counts | 0, 64, 78 |
| `publish` | `--dir <live>` | nothing: reads the tables and writes the `live/*.json` views, each by unique-temp-and-rename | the files written | 0, 78, 1 |
| `import` | `--home <dir> [--dry-run] [--from <t>]` | section 7, in its order, in one transaction per group | the report | 0, 64, 78, 1 |
| `journal-open` | | the actions intended and not settled, oldest first | `{"actions":[...]}` | 0, 78 |
| `journal-record` | `--kind <k> --issue <n> --args <json> --before <snapshot-id>` | `actions` + `journal` (the intent) | `{"action":N}` | 0, 64, 78 |
| `journal-settle` | `--action N --result ok\|failed\|timed-out [--evidence <json>]` | `journal` (the result) | the row | 0, 64, 78 |
| `prune` | `--days N` | old `heartbeats`, `polls`, `endings`, `journal` rows whose action settled; never a `polls` row an unsettled `actions` row points at (section 5) | the counts | 0, 78, 1 |

`journal-*` are the verbs the reconciler uses; nothing else calls them. The
verbs above each group are the ones the shell commands call, and each group of
section 2 names exactly one of them. The mapping is 13 to 13 with one kind of
exception, and the exception is a name rather than a gap: **group 8, a label
decision, has no verb of its own.** Its GitHub half goes through the journal
(`journal-record`, then `journal-settle`), and its host half is written by the
verb of the group whose rows the label carries: an approval is an `approvals`
row, so the `label-apply` transaction of section 3.4 is a `cycle-start` that
writes the approval and the journal row together; a park is `park`; a refresh is
`refresh-request`. There is no `label-apply` verb and there must not be one — a
verb that took a label name would let a caller change a label without saying
which record it belongs to, which is the A5 failure this store exists to close.
Where a section names `label-apply` it names that joint write, not a command.

## 5. The action journal

Two tables. `actions` holds what the reconciler decided to do; `journal` holds
every attempt at it, so a poll that died mid-action can be read afterwards.

```sql
CREATE TABLE actions (
  id          INTEGER PRIMARY KEY,
  poll_at     INTEGER NOT NULL REFERENCES polls (at),
  kind        TEXT NOT NULL,             -- label-add, label-remove, comment-post,
                                         -- comment-edit, container-start, unit-stop,
                                         -- label-remove-only, refresh-begin, refresh-end
  issue       INTEGER NOT NULL,
  args_json   TEXT NOT NULL,             -- the exact call: repo, label, body, name
  before_snapshot INTEGER NOT NULL,      -- the read this rests on (A5)
  state       TEXT NOT NULL CHECK (state IN ('intended','attempted','done','failed','abandoned')),
  intended_at INTEGER NOT NULL,
  attempted_at INTEGER,
  settled_at  INTEGER,
  result      TEXT,                      -- the typed result: ok|empty|read-failed|timed-out
  evidence_json TEXT                     -- what proves it: a comment id, a label read
);
CREATE INDEX actions_open ON actions (state, intended_at);

CREATE TABLE journal (
  id INTEGER PRIMARY KEY,
  action INTEGER NOT NULL REFERENCES actions (id) ON DELETE CASCADE,
  at INTEGER NOT NULL,
  step TEXT NOT NULL,                    -- intended|attempt|answer|error|settle
  detail TEXT
);
CREATE INDEX journal_action ON journal (action, at);
```

### What a row holds

`kind` plus `args_json` is the whole call, written before it is made.
`before_snapshot` is the `queue_snapshot.at` (or the `issues.checked_at`) it was
decided from, which is what makes A5 checkable: a write either rests on a read
taken after the last thing that could have changed it, or is idempotent.
`evidence_json` holds the proof GitHub gave back: a comment's `databaseId`, the
label list after the edit, the pull request's state.

### When it is written

1. `journal-record` in the **same transaction** as the host-side rows of the
   verb. The intent and the record commit together, so there is no moment where
   the host knows a cycle started and the journal does not know a label was owed.
2. A `journal` row with `step='attempt'` immediately before the call.
3. `journal-settle` with the typed result and the evidence immediately after it,
   in a transaction of its own. It must survive a crash between 2 and 3, because
   that is the case the journal exists for.

### How the next poll settles an action that was intended but not done

The reconciler's first step, before it reads GitHub, is `journal-open`: actions
in `intended` or `attempted`, oldest first, grouped by issue. For each, the
outcome is decided by `kind`, from a fresh read, never from the row alone:

| `kind` | The read, and what settles it |
| --- | --- |
| `label-add`, `label-remove`, `label-remove-only` | the issue's labels. Present where the action wants it and absent where it does not: `done`. Present where it should be gone: the write landed, mark `done` and continue. Neither: `abandoned`, and the decision is recomputed from scratch this poll, which is safe because every label verb is idempotent |
| `comment-post` | the issue's comments, by `before_snapshot` time forward. A comment with the intended body and the bot as author: `done`, and its `databaseId` goes into `evidence_json` **and** into `questions` if the kind is a question. None: re-post. This is why a comment id must never be guessed, and why an old park id is removed rather than trusted (F5) |
| `comment-edit` | the comment's `lastEditedAt` and `editor`: already ahead of the attempt, so `done` |
| `container-start` | `docker` for the pinned name, and the lock. Container there: `done`. Not there and no lock: `abandoned`, so the poll recomputes and may start it again. There and never journaled as started: `done`, and the `cycles` row is written from the host's facts (C13: what the host saw) |
| `unit-stop` | the unit's `ActiveState` and the lock. Inactive and free: `done`. Still active: re-ask, bounded (C11), and after the bound `abandoned` with one line, which is what C6 asks for |
| `refresh-begin`, `refresh-end` | the pull request's labels. `tehas:refreshing` present where wanted: `done`. Rule 4b re-reads the whole precondition anyway, so `abandoned` is safe |

Two rules hold throughout. An action is never replayed blindly: the read decides
whether it happened. And an action that cannot be settled is said, not swallowed:
one log line naming the action id, its kind and its age, and `polls.settled=0`
for the poll that found it, so `poll-show --last` can report a poll with work
outstanding. `prune` removes only rows in `done`, `failed` or `abandoned`, never
one still `intended` or `attempted`.

One foreign key makes that rule load-bearing, and #256 states it in the verb
rather than leaving it to the reader: `actions.poll_at` references `polls (at)`
with no `ON DELETE`, and SQLite's default `foreign_keys` is off, so a `prune
--days N` that deletes a row of `polls` would silently take the unswept actions
of that poll with it. Prune must delete no `polls` row while an action points at
it, which is the same rule as the last sentence and is what its test pins.

## 6. The GitHub snapshot

One GraphQL query with pages for everything the poll decides from the queue,
plus bounded per-candidate reads. `tehas` keeps using `gh` as it does today: no
new token handling, no token in argv, every call under a timeout.

### The query

```graphql
query ($owner: String!, $name: String!, $cursor: String,
      $page: Int = 3, $pulls: Int = 5) {
  repository(owner: $owner, name: $name) {
    nameWithOwner
    viewerPermission
    issues(states: OPEN, first: $page, after: $cursor,
           orderBy: {field: UPDATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number state updatedAt title lastEditedAt body
        labels(first: 10) { nodes { name } }
        timelineItems(itemTypes: [LABELED_EVENT, UNLABELED_EVENT], first: 30) {
          nodes {
            __typename
            ... on LabeledEvent   { createdAt actor { login } label { name } }
            ... on UnlabeledEvent { createdAt actor { login } label { name } }
          }
        }
        comments(last: 5) {
          nodes { databaseId author { login } body createdAt lastEditedAt
                  editor { login } }
        }
        closedByPullRequestsReferences(first: 3) { nodes { url state } }
      }
    }
    pullRequests(states: OPEN, first: $pulls) {
      nodes { number url state headRefName body updatedAt
              labels(first: 5) { nodes { name } } }
    }
  }
}
```

One request returns, per issue: its labels, its `labeled` and `unlabeled`
events with actor and time, its edits (`lastEditedAt`, `editor`), its
`tehas:blocked` and question comments, and the pull requests that close it, with
their state. That is every read the poll makes today about the issues
themselves, in one round trip.

It is one query, paged, with `$page` the page size (**3**, priced in the cost
section below): a repository whose open issues exceed `$page` pages it,
and the pages are sequential by construction (the cursor comes back from the
previous page). The queue the epic counted — 132 issues across all states — is
44 pages at that ceiling, and the stop rule of the cost section keeps an idle
poll to the first of them.

`viewerPermission` is fetched with it, and `gh api user` stays a separate
request, because it is a different resource: the bot's own login must be read
before any label is believed (F4), and a repository the bot cannot see ends the
poll before it decides anything.

### Requests per idle poll

Counted against the 24 `gh` call sites of `libexec/dispatch` at `5e2d5fa` —
the 24 of 29 lines that mention `gh` which are actual calls; the other five are
the `gh_json` wrapper (`:402`) and prose. "Idle" is the case the epic's target
is written for: past the poll's checks, with cycles running, nothing eligible to
start and no answer to post.

| Call site | Today, idle | Under this design |
| --- | --- | --- |
| `:1745` `repo view` | 1 | folded into the snapshot (`nameWithOwner`) |
| `:1757` `api user` | 1 | 1 (kept: F4) |
| `:1787` control issue `labels` | 1 | folded into the snapshot |
| `:763` control issue `events` | 1 | folded (the control issue is in the same list) |
| `:2018` `issue list --label tehas:running` | 1 | folded |
| `:2140` `--label tehas:ready` | 1 | folded |
| `:2227` `--label tehas:awaiting-merge` | 1 | folded |
| `:2324` `--label tehas:question` | 1 | folded |
| `:2614` `--label tehas:plan` | 1 | folded |
| `:2616` `--label tehas:refine` | 1 | folded |
| `:1932` `pr list --author bot` | 1 | folded (`pullRequests`) |
| `:2062` `pr list --label tehas:refreshing` | 1 | folded |
| `:633` branch check-runs | 1 (only with `TEHAS_BRANCH_CHECK`) | 1, kept: a different resource, and only when set |
| `:725` `work_ok` per issue | about 1 per issue the poll would act on | 0 when the snapshot is fresh (its `checked_at` is inside the poll's window), else 1 |
| `:1677` `report_demand` events | 1 per queued issue | 0 (the snapshot's timeline items) |
| `:843`, `:998`, `:1120` per candidate | 3 per candidate | 1 per candidate (the edits read), +1 only for a candidate about to be started |
| **idle poll** | **about 12, and today about 10 as the epic counted it** | **3 with the branch check set, 2 without: `api user` + one page + the branch check, and one page + `api user` when it is not set** |

The two rows the target turn on are the last two: an idle poll acts on no
candidate, so the per-candidate row contributes 0, and no issue has changed
since the last poll, so the `work_ok` row contributes 0 from the store.

That reaches the epic's target of 3 or fewer for an idle poll. The target is
**provisional**: it rests on the cost model below, measured by request-count
arithmetic and one observed response, not on a run against tehas-tehas's
queue. Section 10 carries that as open.

### Cost, and the budget the query must fit

GitHub prices GraphQL by nodes, and the token budget is separate from the REST
budget. GitHub's published formula (`api.github.com/developers/guides/graphql
guidance`, resource limits) charges a **list** `min(5 + children, 500)` plus the
cost of each child; the children of a list are its returned nodes and its
`pageInfo`, and every leaf costs 1, with the whole query costing **1 point at
least**. The 5 is charged once per list, not once per returned node, and a list
is billed for what came back rather than for its `first:` cap — so a cap prices
the worst case while the poll pays the usual one.

Two consequences decide the shape:

- **A page costs 7 even when it returns nothing** (`5` for the list plus `1` for
  its `pageInfo`), so a page of three issues costs 94 with nothing on them and
  382 with all three filled to their caps. The caps are what that 288-point
  spread is made of, and `timelineItems` alone is 60 of the 120 a page's issues
  pay for their four connections: uncapped it would be charged for 100 nodes
  rather than 30.
- **`pullRequests` costs 6 with no open pull request and 96 with five open ones
  carrying five labels each** — `6` for the connection and its `pageInfo`, then
  `5 × (1 + 17)` = 90 for the pulls, each one its own point plus its labels
  connection and its five label nodes — which is why `first:` there is 5 where
  `dispatch:1241` asks for 100: the same connection, priced at what the bot's own
  open work actually is rather than at a cap.

The budget is 5,000 points an hour and the cheapest possible query costs 1
(measured with `gh api rate_limit`, whose `resources.graphql` carries
`limit: 5000`). `host/systemd/tehas-dispatch.timer:6` is `OnUnitInactiveSec=2min`
with `:13` `OnUnitActiveSec=2min`, so the host asks for about **30 polls an
hour** and the whole hour's budget is about **165 points per poll**.

That is the constraint that decides the design, and it rules out the obvious
shape outright. Two numbers frame it: the worst a request of this shape **can**
cost, which is what forces the caps and the paging, and the **floor** no request
of the shape can go under, which is what forces the page size. Call a page
*saturated* when every issue fills every cap and *empty* when every connection
comes back with nothing.

Every figure below is one evaluation of the four lines after the table, so each
can be re-derived by hand from the caps. Read those first: a connection costs
**`2k + 6`** when it returns k nodes and 6 when it returns none; the four
connections of one issue cost **120** at the caps (`26 + 66 + 16 + 12`); an
issue costs **124** with them filled and **28** with nothing on it; a page of `n`
issues is **`125n + 7`** saturated and **`29n + 7`** empty; `pullRequests` costs
**96** with five open pull requests carrying five labels each and **6** with
none.

| Request | Cost, by the four lines after the table | Against 5,000/hr |
| --- | --- | --- |
| The same query with `first: 100` on the issues and today's caps — `labels` 30, `timelineItems` 100, `comments` 20, `closedByPullRequestsReferences` 10 — every issue saturated | `1 + (7 + 100 × 348) + 96 = 35,004`, where one issue is its own node and four scalars (4) plus four connections filled to those caps: `(6+2×30) + (6+2×100) + (6+2×20) + (6+2×10) = 344` | **7.0× the whole hour**: no second query of any kind for the rest of it, and one such request ends polling until the hour rolls over |
| 100 issues, every connection empty — the whole open set read in one page, the shape a `first: 100` poll asks for on a quiet host | `(29 × 100 + 7) + 6 = 2,913` | **0.58×** the hour, and **17.7 polls' worth** of the 30 an hour: reading the whole open set at once is unaffordable even when there is nothing on it, so the page size is not a latency knob |
| **The request as specified: `first: 3` on the issues, `first: 5` on the pulls, three issues saturated** | `(125 × 3 + 7) + 96 = 478`, of which the page is 382 and the pull requests 96 | **0.10×** the hour, but **2.9× one two-minute poll's share**: the page is the largest size whose *usual* cost the timer tolerates, and its saturated worst case is 313 points over |
| The same request on an idle queue: three issues with a few labels, no label event in the window, no comment, no closing pull request, no open pull request | `(29 × 3 + 7) + 6 = 100` | 0.02× the hour, and **0.61× one poll's share**: the idle poll is the only row of this table the timer can actually afford |

The idle poll is the last row, and that is the whole reason the shape works at a
two-minute interval. It is affordable because of what it asks for, not because a
page of issues is cheap: a poll that asked for the whole open set in one page
could not run twice in an hour.

**Paging, stated exactly.** Polls run every two minutes and the store already
holds the last complete read, so a page is never taken to be complete: it is
taken as a set to reconcile against the rows already there, and the poll keeps
only what it did not have. Page 1 is always paid; every page after it is earned.

- **Page 1 is unconditional.** The poll cannot know the tail is dead until it
  has read it: `hasNextPage` and the first page's `updatedAt` values are what
  the stop rule below reads. For an idle queue, sorted `UPDATED_AT DESC`, that
  page is the fourth row above — three issues, nothing on them, 100 points, of
  which 94 is the page and 6 the empty `pullRequests` connection. Page 2 on the
  same queue costs 65, and 2,907 if it is taken with the page size at 100,
  which is the second row above.
- **Page 2, and any page after it, is taken only when the newest issue of the
  page read is inside the window the poll needs.** The window is what A5 needs:
  since the `at` of the last settled poll, widened to the longest interval any
  rule of `libexec/dispatch` trusts (the 30-minute stale-check of rule 3b, the
  slice's room, the refresh hold). A page whose issues all predate it carries
  nothing the poll can act on and is not asked for.
- **The window is not the only reason to keep paging**, and this is the case the
  rule must not lose: an issue that stopped being `tehas:ready`, or lost its
  label altogether, is exactly what the poll must notice, and a `UPDATED_AT DESC`
  page drops it as soon as it ages out of the window. So a page is also kept
  while any row in the store is *live* — running, ready, awaiting-merge, in
  planning, parked, refreshing, or holding a question — and its `updatedAt` is
  older than the newest issue of the page read, which is what pins the set the
  poll must see. A page whose issues are older than every live row in the store
  is dead in both directions and is not asked for.
- **The page is sized to the budget, not to 100, and a page of 100 cannot be
  sized into affordability.** At the caps below, a page of `n` issues costs
  `125n + 7` at worst and `29n + 7` when every connection is empty, and the pull
  requests add a flat 96 when five are open with five labels each (the bound
  after this list). So a saturated page of 25 costs 3,228 with its pull requests,
  about twenty times the poll's share of the hour, and a saturated page of 100
  12,603, seventy-six times it. `first:` on the issues is therefore **3**, not 25
  and not 100 — a page
  is sent only when what it can cost fits what the hour has left, and
  `pages ≤ ceil(open issues / 3)` bounds the rest. (These are the saturated
  worst cases; the same pages read empty are 738 and 2,913, and the second is the
  second row of the table above.) For the queue the
  epic counted — 132 issues across all states — that ceiling is 44 pages, which
  is why the stop rule above is not a performance hint but the thing that keeps a
  poll able to run at all: an idle poll pays one page of three issues (100 points)
  or a little over that, and a busy one pages until it has seen everything it can
  act on. `pullRequests` is not paged: `first: 5`, where today's `dispatch:1241`
  asks for 100 per poll. With `n = 3` the saturated worst case is 478 against a
  165-point share, and with `n = 1` it is 228 — see the bound after this list for
  what that buys. When a measured page still does not fit, the snapshot program
  takes the fallback of the last list here (the issues in one read, the candidates
  in their own) rather than a larger page.

**The per-poll point bound.** Every number the poll asks for is set, so the cost
is bounded by construction rather than by luck. One symbol per term, the
definition is used the same way in every row of the table above, and each figure
is written with its terms shown so it can be checked by hand:

```
GitHub's formula, as the spec applies it to this shape:

  cost(list) = min(5 + children, 500) + sum(cost of each child)
  cost(leaf) = 1                                     // a node or a scalar is one point
  children of a list = its returned nodes + 1 for pageInfo

  A connection therefore costs (5 + (k + 1)) + k = 2k + 6 when it returns k nodes,
  and 5 + 1 = 6 when it returns none.

points per poll ≤ pages × page + pulls

  page = 1 + (2n + 7) + n × i          // repository + the issues list + its issues
  i    = 4 (the issue's node + its four scalar fields)
       + (2l + 6) + (2t + 6) + (2c + 6) + (2r + 6)          // four connections
  pulls = (2·n_pr + 1) + n_pr × (1 + (2·lp + 6))             // pullRequests + each pull
                                                             // + its labels

with the caps of the list after this bound — l = 10, t = 30, c = 5, r = 3,
n_pr = 5, lp = 5:

  i     = 4 + 26 + 66 + 16 + 12 = 124                // a saturated issue
  page  = 1 + (2n + 6) + 124n = 125n + 7             // 132 at n = 1, 382 at n = 3
  pulls = 11 + 5 × 17 = 96                            // flat in n, so it is paid per page

with every connection empty (l = t = c = r = 0, and no pull request open):

  i     = 4 + 6 + 6 + 6 + 6 = 28
  page  = 1 + (2n + 6) + 28n = 29n + 7                // 94 at n = 3
  pulls = 6                                           // the connection with nothing in it

  i_sat / i_empty = 124 / 28 ≈ 4.4 — the caps, and `t` above all of them, are
  what make that ratio as small as it is
```

and the poll is rejected before it is sent if that bound exceeds the budget left
in the hour, which is what the `--budget-left` check of `snapshot-load` in
section 4 is for. The two numbers that decide the size of a page are `t` (30, by
far the largest term of a saturated issue) and `n` itself, and they pull in
opposite directions:

- **At these caps no page whose issues are all saturated fits the share.**
  `125n + 7 + 96 ≤ 165` fails at n = 0 already (13 of the 165 is spent on the
  two empty connections before an issue is asked for), so a page of one
  saturated issue costs 228 and a page of three 478. That is what the caps and the
  page size are argued against, and it is why this section states both numbers
  rather than one: the 165 is a share of the hour the *usual* poll must live
  inside, not a ceiling the worst-case construction does.
- **`n = 3` is what the query above uses**, and the honest reason is that a
  saturated page never occurs. A page of three is saturated only when every one
  of its issues carries thirty label events, ten labels, five comments and three
  closing pull requests at once. The realistic cases — page 1 of an idle queue at
  94 points, a page whose few issues carry a couple of labels and one label event
  in the window at roughly 150 to 300 — sit at or near the 165 points the timer's
  interval allows, and the `--budget-left` check refuses a page whose bound says
  it will not fit rather than sending it and finding out.

#253 owns the resolution of that gap, and it is larger than "pick a number": it
either lowers `t` and `l` until the saturated worst case itself fits 165 (at
`t` = 5, `l` = 5, `c` = 3, `r` = 1 a saturated issue is 56 and a page of three is
274, still over — the caps alone do not close it), or takes `first: 1` on the
issues and reads more pages instead, or lengthens the timer. What it may not do is
keep a page size whose measured cost it has not recorded. The numbers in this
section are a model with its terms shown; #253 replaces them with the `points`
field `snapshot-load` reports.

The caps are what keep a page's *usual* cost near the hour's share at 30 polls
(what a page costs at all is the bound above, and the saturated worst case does
not fit there):

- `timelineItems` is filtered to `LABELED_EVENT, UNLABELED_EVENT` and capped at
  30, not 100: unfiltered, a timeline of comments, reviews, mentions and
  assignments is what makes this query unaffordable, and at 30 the worst case is
  a quarter of the 100 the filter alone would allow. Nothing in
  `libexec/dispatch` reads a timeline item of any other type.
- `comments` is `last: 5`, not `last: 20`: the poll wants the newest question
  and the newest edit, and `dispatch:846` asking for 100 today is per-issue,
  per-candidate, not per-poll.
- `labels` is `first: 10` (a tehas issue carries `tehas:*`, `needs-*`, `bug`/
  `feature` and at most a few more) and `closedByPullRequestsReferences` is
  `first: 3` (today's per-candidate read asks for 20, and one issue does not
  have twenty closing pull requests).
- `pullRequests` is `first: 5`, not 100 as `dispatch:1241` asks per poll today:
  the bot's own open pull requests are few, and the bound above prices the row at
  96 points when full (`11 + 5 × 17`), against 6 when none is open.
- A candidate's 3 per-issue queries of today become 1 because the edits the
  approval binds to (`:1120`) is the only one of the three that a snapshot
  cannot already answer. **And if even the page of three does not fit the hour,
  the snapshot is narrowed before anything else is tried**, in this order:
  `timelineItems` to 10 (the poll's window is two minutes, so no issue has more
  than a handful of label events inside it), then `comments` to `last: 3`, then
  the pull requests split into a second request of their own, then the page to
  1. `first:` is never raised to buy coverage; coverage comes from paging.

The idle poll's cost, named for what it is: **page 1 of a small queue is the
fourth row above — 94 points with every connection empty, and on the order of
150 to 300 once the few issues the poll can act on carry their labels and their
last label event. A poll's worst case, three saturated issues with the pull
requests, is 478** — `125 × 3 + 7` for the page and 96 for the pulls — **2.9×
that share**, which the caps and the `--budget-left` check are there to catch
rather than to hide, and which is the number #253 measures against. `requests ≤
pages + 2 + candidates` is what the epic's target of 3 measures; node cost is the
limit that target does not see, and it is the one this section decides on. A run
against tehas-tehas must
record the `X-RateLimit-Remaining` delta of the graphql resource across ten
polls (section 10), name the page count and the `first:` value of each, and say
what a page actually cost — because the rows above are a model, not a
measurement. The sub-issue that builds the snapshot owns the answer: if the
measured page cost does not fit 165 points at the timer's interval, it adjusts
the page size and the caps, or lengthens the interval, before the reconciler
takes over, not after. It is #253's acceptance line, and #253 does not end until
it has a number.

## 7. The import of today's files

One command, `tehasbot store import --home <dir>`, run once per host, from
`install.sh` under `state/update.lock`, after a `VACUUM INTO` backup.

### Order

Order matters because the groups reference each other.

1. `init` the database, or refuse: an existing database with rows is never
   re-imported by accident (see idempotence).
2. `issues` from the current `live/status.json`, `live/running.json` and each
   `state/cycles/<n>.approved.json`, so the join target exists first. Anything
   whose kind the files cannot tell is `unreadable`, not `issue`.
3. `cycles`, `approvals`, `parks`, `questions`, `cycle_settings`,
   `cycle_results`, `cycle_progress`, `cycle_history`: from `state/cycles/*` and
   `state/park-*.id`, newest first per issue so `attempt` is derived from the
   history rather than guessed.
4. `heartbeats` from `live/cycles/*.json`; `endings` from `live/endings.json`,
   oldest first.
5. `planning`, `planning_answers`, `planning_filed`, `plan_history`, `refined`:
   from `state/plan-*.json` and its sidecars. An unparseable record is reported
   and skipped, never guessed at.
6. `polls`, `poll_demand`, `queue_snapshot`, `branch_checks`, `pause_flag`,
   `limits`, `endpoints`, `index_state`, `release_history`, `edge_takeover`,
   `refresh_requests`, `refresh_hold`, `check_product`, `platform_state`.
7. `sweep_log` from what is left on disk, as `kept` rows, so the sweep's rules
   have a starting point.

Each step is one transaction, and the step that fails is reported by name with
the file that broke it. The import does not run a second time on a host that has
already imported.

### Idempotence

The rule is that an import is a move, not a merge, and a move must be provably
complete before anything is removed.

- Every row is keyed by a natural key from the file it came from: `(issue, at)`
  for history, parks, refresh requests and approvals; `(issue, attempt)` for
  results; `(issue, started_at)` for endings; `id = 1` for the singletons. A
  re-run inserts `OR IGNORE`, so running the import twice changes nothing.
- **A file is removed only by a later run of the same command, and only when
  its own row exists.** The check is per file: `import --verify` re-reads each
  candidate file, recomputes its natural key, and reports it as `moved` only when
  the row matches. Nothing is removed in the pass that wrote the rows.
- A file that cannot be read, parsed, or keyed is reported as `held` and stays
  where it is. The import exits 1 when anything is `held`, and the operator sees
  the list. It never invents a row to make a file disposable.
- `--dry-run` reports the three sets (`moved`, `held`, `removed`) and writes
  nothing.
- The database's `meta.imported_at` records the pass; a second import against a
  home whose `imported_at` is set exits 78 unless `--force`, which re-runs in
  verify-and-remove mode only.

### What happens to the old files

Three fates, decided per file, and the file stays for as long as its fate is
not proven:

1. **Removed** by the import after `--verify`, once every reader has moved to
   the verb. For `state/cycles/*` that is #250; for `state/plan-*.json` it is
   #251. Until then the file stays and the row is written beside it.
2. **Kept as a view**, written from the tables by `publish` (#252), with the
   bytes unchanged: the shapes of `live/*.json` stay the same unless a sub-issue
   says otherwise and changes the manual in the same commit.
3. **Kept as a file**, for the reasons of section 1 and ADR 0003: the locks, the
   mounts, `update-pending`, `update-hold`, `first-install`,
   `sandbox.Dockerfile.sha256`, `index.json`, `analytics.db`, `dispatch.again`
   and the trees of other programs.

Nothing from a container-written file becomes a row without a read from GitHub
behind it (F7). `state/plan-<n>.json.keep/` moves directory, not rows.

## 8. Vendoring the crates

`cargo vendor --locked` was run on the console workspace at `5e2d5fa` in this
cycle's sandbox:

```
cargo vendor --locked --manifest-path console/Cargo.toml <scratch>/vendor-probe
```

**62 crates, 101 MB, 3,444 files.** Of that, `libsqlite3-sys` is **21 MB**
(the bundled SQLite amalgamation, the reason the host's SQLite version never
matters) and `rusqlite` itself is **972 KB**. The rest is the existing
workspace: `regex-automata` and `regex-syntax` are the two largest after
SQLite, and the `wasm-bindgen` / `windows-sys` trees come in through
`serde`'s and `js-sys`'s optional targets.

The cost, stated plainly: **about 101 MB of vendor is roughly 3.5 times the
whole repository checkout**, and the 21 MB of SQLite source is compiled on
every host and on every runner that builds the workspace, which is the +12 s
the epic measured for a bundled build.

**Recommendation: vendor, but not into the product repository.**

- **Vendor the whole workspace, in `console/vendor/`, committed, with
  `console/.cargo/config.toml`.** The case for it is the one the epic made
  (#144): a cycle fetches crates again every time, and a host behind a proxy
  that will not carry `crates.io` cannot install tehas at all. tehas builds
  with `--locked` today, so the versions are already fixed; vendoring fixes the
  bytes too. The counter-case is the review cost: 3,444 files in a public
  repository, whose license is not the repository's (the LICENSE and NOTICE of
  tehas cover tehas, and would need a `THIRD-PARTY-NOTICES` alongside them),
  and a diff nobody reads.
- **The middle path this spec recommends**: keep the crates out of the product
  repository and build them once per host into a tehas-owned cache
  (`$TEHAS_HOME/target/`), which is #248's job; and vendor separately, as a
  release artifact of the platform checkout that `install.sh` can be pointed at
  for an offline host. That keeps the 101 MB out of every clone and out of every
  review, and still gives a host with no network a way to build. The 21 MB of
  SQLite source stays with `rusqlite`'s `bundled` feature, which is not
  negotiable: the alternative is trusting the host's `libsqlite3`, and a store
  whose behaviour changes with the distribution's SQLite is the kind of fact
  this epic exists to remove.
- The gate is the constraint that decides the shape, and it is #246's, owned by
  a person (AGENTS.md rule 7): `.github/workflows/verify.yml` runs
  `scripts/verify.sh` on the organisation's runner, which has cargo, and the
  fast gate today runs the console's tests only where cargo is installed. If
  #246 makes cargo required, the offline question becomes a runner question,
  and a vendored artifact answers it without 101 MB in every clone.

## 9. The invariants

Re-checked against main, with the sub-issue that tests each. The list is the
epic's, unchanged in its numbering, plus the new ones #234 brought.
**Owner** is the sub-issue that carries the test; a blank owner is a gap this
spec reports rather than hides.

### GitHub and host agree

| | The invariant | Owner |
| --- | --- | --- |
| A1 | A cycle starts only after the issue is marked `tehas:running`; if the mark fails, nothing starts | #254 |
| A2 | A trigger is used up only after the action it authorises is committed | #254 (`approvals.used_at`, the journal) |
| A3 | One approval is one start; a retry tick counts once, on a parked issue only | #254 |
| A4 | An open issue marked running with no held lock loses the mark, gets `needs-human-attention` and one comment; a closed one only loses the mark; neither restarts | #253 (decide), #254 (apply) |
| A5 | Every GitHub write rests on a read taken just before it, or is idempotent | #253 (`before_snapshot`) |
| A6 | A comment is posted only after the label change it describes has succeeded | #254 (journal order) |
| A7 | Only open issues are work; re-read before any label change or start; unclear fails closed | #253 (`issues.kind`) |
| A8 | A pull request merged mid-cycle ends the cycle as merged; closed unmerged or issue closed parks without retry boxes; unpushed work is kept | #250 |
| A9 | A pull request lookup covers every state; "could not be read" is not "none"; the branch is kept | #250 |
| A10 | `awaiting-merge` only for the real outcome, never with `needs-human-attention` | #254 |
| A11 | No start on an issue an open pull request closes | #253 |
| A12 | No start, plan or retry on the control issue | #253 |
| A13 | A ready label on a running issue waits; a running cycle is never stopped for it; an open planning session blocks a start | #253 (`planning`) |
| A14 | A failed read of any list or event set ends the poll before it acts | #253 (typed results) |
| A15 | Status shows the age of its data: a snapshot older than 10 min warns; a stale health file is not believed | #252 (`queue_snapshot.at`, `polls.at`) |
| A16 | "Completed" means closed by a merged pull request, read from GitHub | #253 |
| A17 | Locks are host-local truth: only the host that runs the cycles sweeps | #250 |

### Local state

| | The invariant | Owner |
| --- | --- | --- |
| B1 | A file another process reads is written to a unique temp name and renamed; a failed rename fails the operation | #252 (`publish`), and the `.new` names of section 1.5 |
| B2 | One update at a time, shared with `install.sh`; `--check` takes no lock | #249 (the migration path under `update.lock`) |
| B3 | Records leave a window one at a time; the window is never renamed | #252 |
| B4 | Every per-issue record has a named owner and lifecycle | #250, #251, and section 1.15's `state-audit` |
| B5 | A retry never deletes unpushed work; trees go only after a merge, a handoff with a pull request, or 7 days unused, under the issue's lock, at most 3 per poll | #250 (`sweep_log`) |
| B6 | Removal only under a marker-owned root, by the exact issue number, no globs; `TEHAS_HOME` checked first | #250 |
| B7 | Facts are recorded when they become true: the image and pi-rukas commit at container start | #250 (`cycles.rukas_commit`, `.image`) |
| B8 | A planning session keeps its brief, its filed markers and its session across turns; a lost session fails with its own reason | #251 (`planning`) |
| B9 | An update runs with the step library it just pulled | unchanged (bash, #246's gate) |
| B10 | The code index is swapped atomically | #249 (`index_state`) |
| B11 | Sandbox images are bounded; one in use is never removed | #257 |

### Timeouts and supervision

| | The invariant | Owner |
| --- | --- | --- |
| C1 | Every external call has a bound: push preflight and clone 300 s, `docker stop` 60 s then kill, fetch 120 s, slice reads 10 s, `gh` in the stop path 20 s, index 1800 s | #254, #256 |
| C2 | Every unit has a backstop that fits its inner limits (cycle `RuntimeMaxSec` 8 h with the work limit at most 25,200 s; dispatch 3 h with the plan limit at most 7,200 s) | unchanged; the units are bash's |
| C3 | A blocked poll is never silent | #254 (`polls.stuck_said_for`) |
| C4 | Long work does not run under the poll lock; children do not inherit it | #255 |
| C5 | The cycle's lock is the liveness truth; a lock nobody holds is a cycle that ended | **stays file-based**; tested by `test/cycle_*`, and by #253's shadow comparison |
| C6 | A held lock with no heartbeat for 20 min is a stuck cycle: stop it without blocking the poll; the next poll parks it | #253 (`heartbeats`, `cycles.stop_asked_at`) |
| C7 | A manual run and the unit exclude each other: the issue lock is taken before any change | **file-based**, tested in `test/work_*` |
| C8 | Container names are fixed per issue; a leftover is removed before `docker run`; the trap is armed before the container starts | #256 |
| C9 | The end of the agent session is seen promptly: quit means interrupted, silence past the stall limit means stalled | #256 (`cycles.stalled_seconds`) |
| C10 | A time limit or a stop reports the step and the pull request, never `status=running` | #250 |
| C11 | The stop path tries each call once, bounded | #256 |
| C12 | A provider-outage park is retried once automatically; a second waits for a person | #250 (`parks.retries`) |
| C13 | `--init`, pids limit, refused forks counted; a kill or OOM is what the host saw, not what the state file says | #250 (`cycles.host_saw`, `.oom_kills`, `.forks_refused`) |
| C14 | Admission counts every container in the slice and cycles not yet in one; an unreadable slice names the file and starts nothing | #253 |
| C15 | The console wait is bounded and the console recreates an unlinked output file | unchanged (console) |

### Typed rules that replace bash habits

| | The invariant | Owner |
| --- | --- | --- |
| D1 | A missing actor login is `unknown` and refused | #253 |
| D2 | Read failed and empty are different types; no error is swallowed by a WARN | #249 (the store's typed results), #253 |
| D3 | JSON numbers and fields are validated | #249 (`cycle_results.payload` + schema version), #253 |
| D4 | A busy lock is never exit 0 with no log line | #249 |
| D5 | A documented setting is never dropped by the loader | unchanged (`lib/config.sh`, its test) |

### Performance

| | The invariant | Owner |
| --- | --- | --- |
| E1 | The model probe runs only when a start is possible, at most once per poll per issue with images | #253 |
| E2 | A freed slot is filled at once (already done: `09757e9`) | `state/dispatch.again` + the path unit; stays file-based, tested in `test/dispatch_start_test.sh` |
| E3 | Label events are read once per poll per issue | #253 (the snapshot's timeline items) |

### Trust boundary

| | The invariant | Owner |
| --- | --- | --- |
| F1 | Authorisation is the account in the most recent `labeled` event, and it must be in `TEHAS_ALLOWED_LABELERS`; unknown or unreadable means no start (AGENTS.md rule 5) | **stays as it is**; #253 reads it from the snapshot and #253's tests pin it |
| F2 | A label from an account outside the list is removed with one comment | #254 |
| F3 | An approval binds the text: title and body unchanged since the label, by any account | #250 (`approvals.body_sha256`, `issues.edited_at`) |
| F4 | The bot in the allowed list stops the dispatcher | #253 (exit 78, unchanged) |
| F5 | A question counts only if this host has its comment id on record; a tick counts by the last editor | #250 (**`questions` is the trust root of this**) |
| F6 | An automatic retry is authorised by the account that started the parked cycle, if still allowed | #250 (`cycles.started_by`) |
| F7 | The container's state file is a claim: `merged` needs a merged pull request that names the issue; a symlink is refused | #250, #251, and section 3.14 |
| F8 | The container's mounts, environment and options are fixed and pinned by tests (AGENTS.md rule 4) | **unchanged, and must not be relaxed** |
| F9 | Console output is masked before it is cut, format characters stripped, memory bounded | unchanged (console) |

### New, from #234

The runner changed what the host has to be careful about. Each is stated as the
test that would fail without it, and given its owner.

| | The invariant | Owner |
| --- | --- | --- |
| G1 | `state/cycles/<n>.result.json` and `.progress.json` are **the runner's claims**, not the host's facts: `cycles.runner_exit`, `.watched` and `.host_saw` come from what the host observed, and the payload is stored as a claim with its schema version. A result naming a step the runner never started cannot make the cycle's outcome that step | #250 |
| G2 | The runner's JSON is validated by version, not by shape: a payload with an unknown `schema_version` is stored and reported, and the decision falls back to the host's own observation rather than to a guess | #249 |
| G3 | `.progress.json`'s freshness is what says a run log belongs to this cycle; a file older than the cycle's `started_at` is not read for its steps or its branch. Once `started_at` is a column, that test is a comparison of two columns and not of two mtimes | #250 |
| G4 | tehas posts its own park comment (#232) and its id is the only one the poll will honour a tick in: a tick in a comment this host did not post, whatever it says, starts nothing | #250 (`questions`) |
| G5 | The exit code of the runner is the supervisor's input, and an exit that arrives without a journaled `container-start` is settled by the host's read, never by the exit alone (C13) | #256 |
| G6 | tehas reads no pi-rukas internal: nothing under `.pi/work-state`, and no path under `~/.pi/agent/ensemble-runs/` is parsed for a fact. `test/internals_test.sh` keeps the pin as the store lands, so the regression is caught at the same place it is caught now | #250 |

## 10. Baselines

The issue asks for these measured **on tehas-tehas**, not on a developer host.
**They were not taken there**: this cycle has no access to that host, and a
number copied from someone else's terminal is worse than a number labelled as
not yet taken. Every row carries the command that takes it on tehas-tehas, so
the same numbers can be taken after and compared.

**Status: open.** A person, or an ops run on tehas-tehas, must take the right-hand
column before #246 is scheduled on the strength of these numbers. The sandbox
column is included only so the shape of each measurement is fixed and the
person taking it knows what "done" looks like.

| Measure | Sandbox at `5e2d5fa` (12 cores) | tehas-tehas | Command that takes the tehas-tehas number |
| --- | --- | --- | --- |
| Poll wall time, p50 | not taken here | **not measured** | `store poll-show --last --json` gives the last poll's `at`; wall time comes from the log lines below |
| Poll wall time, p95 | not taken here | **not measured** | same: `jq -r '.[0],.[1]'` over the two epochs either side of each poll's `took`, below |
| GitHub requests per idle poll | not taken here | **not measured** | the delta of the two headers below, over ten consecutive polls |
| Lock probes per idle poll | not taken here | **not measured** | the `strace -f -e trace=flock` block below, or the count of `flock` processes in it |
| Fast gate, wall time | 593 s | **not measured** | the timed run below |
| Serial shell suite | 562 s, 3,343 cases as `scripts/test.sh` counts them (3,027 `t_case` calls; 121 files, 3 at a time) | **not measured** | the serial run below |
| Slowest test file | 46.1 s (`update_test.sh`) | **not measured** | the per-file table of the same run |
| shellcheck | 206 files, part of the gate step | **not measured** | inside the gate step above |
| Console `cargo test` | 21 s warm | **not measured** | the cargo line below |
| `cargo vendor`, console workspace | 62 crates, 101 MB, 3,444 files; `libsqlite3-sys` 21 MB, `rusqlite` 972 KB | n/a (a property of the lockfile) | the vendor line of section 8 |
| `verify.sh --full` | **FAIL**: 3 lints, `-D warnings` at `verify.sh:62` — `unnecessary_sort_by` at `console/mcp/src/sessions.rs:636` and `:641`, deprecated `Atomic::fetch_update` at `console/mcp/src/logs.rs:63` | **not measured** | `scripts/verify.sh --full` |

### The commands

Run on tehas-tehas as the tehas user, with the instance's settings loaded. Each
is one command; none writes outside a scratch directory the operator chooses.

Ten consecutive idle polls, their wall time and their request count:

```bash
# one poll, timed, with the request headers on either side
for i in $(seq 1 10); do
  a=$(gh api rate_limit --jq '[.resources.core.remaining, .resources.graphql.remaining] | @tsv')
  s=$(date +%s%N)
  systemctl --user start tehas-dispatch.service --wait
  e=$(date +%s%N)
  b=$(gh api rate_limit --jq '[.resources.core.remaining, .resources.graphql.remaining] | @tsv')
  printf 'poll %s ms %s rest %s graphql %s\n' "$i" $(( (e - s) / 1000000 )) \
    "$(jq -rn --arg a "$a" --arg b "$b" '$a|@tsv')" ""
done
```

p50 and p95 of the ms column; the request count per poll is the difference of
the two `remaining` readings. A poll that started a cycle is not idle: discard
it and take ten more, and say how many were discarded.

The same numbers from the log, when the unit's own timing is enough:

```bash
grep -E 'poll (took|finished)' ~/tehas/logs/dispatch.log
```

Lock probes per poll. The unit's `ExecStart` (`host/systemd/tehas-dispatch.service:6`)
is a login shell that ends in `exec tehasbot dispatch`, so wrapping it traces the
real poll; `trace=flock` is the call a probe makes (`lib/cycles.sh:44`,
`tehas_cycle_running`), and `strace` is **not** in the sandbox image
(`host/sandbox/Dockerfile:63` installs `file` and `python3-pil` beyond the base)
nor in `host/versions.env`, so it must be installed first and the number below is
taken on the host, not in a container:

```bash
# sudo apt-get install -y strace   # once; bootstrap does not provide it
strace -f -e trace=flock -c -o /tmp/probes.txt \
  /bin/bash -c '. ~/tehas/env.sh && exec ~/tehas/bin/tehasbot dispatch --dry-run'
```

The epic's "one probe per issue ever worked" is the count of `flock` calls in
`/tmp/probes.txt` divided by the polls traced. A poll that started a cycle is not
idle; discard it and trace another.

The gate and the serial suite, timed (the same commands the driver's gate
runs):

```bash
time scripts/verify.sh
time scripts/verify.sh --full
time scripts/test.sh --serial
```

The vendor measurement:

```bash
cargo vendor --locked --manifest-path console/Cargo.toml /tmp/vendor-probe
du -sh /tmp/vendor-probe
du -sh /tmp/vendor-probe/libsqlite3-sys /tmp/vendor-probe/rusqlite
find /tmp/vendor-probe -type f | wc -l
```

The targets from the epic, restated as the acceptance line for the sub-issue
that claims each: idle poll ≤ 3 requests **and** ≤ 165 GraphQL points, the share
of the hour a 2-minute poll may spend (section 6); a hung `gh` bounded at 30 s and
logged; a lock probe per running cycle rather than per issue ever worked; ≥ 200
rule tests as Rust unit tests under 5 s together; fast gate ≤ 120 s; slowest
file ≤ 60 s; a hung test file fails after 300 s with its name; serial suite
≤ 900 s. **None is claimed as met here**, because none was measured where the
epic says to measure it.

## 11. Sequencing, and what this spec does not decide

The order of the sub-issue table in #244, with what each one owes this spec:
#246 (a person, gate only) → #248 (workspace, offline build) → #249 (this
schema, the verbs, the import) → #250 / #251 side by side → #252 (the one
publisher) → #253 (shadow reconciler) → #254 (apply through the journal) →
#255 → #256 → #257. #247 runs beside the line from the start.

Left open on purpose, because deciding them here would be a guess:

- Whether `tehasbot store` is a new binary or a subcommand of an existing one.
  It is a sibling of `libexec/analytics` in this design; #248's workspace decides
  the name.
- Whether `analytics.db` is ever folded in. This spec says no, with reasons
  (ADR 0003); a later record can reverse it if a real need to join the two
  appears.
- Whether the console's masking moves into the store. It does not: F9 is the
  console's.
- Whether `state/plan-<n>.json.keep/pi-plan-cache` moves in #251 or #257. It is
  a container-written directory inside `state/`, flagged in section 1.4.

## Done when

The issue's criteria, each against the part of this document that answers it,
with what is left.

| Criterion | Where | Status |
| --- | --- | --- |
| ADR 0003 records the decision, the alternatives (stay with files; pi-durable, see ADR 0001) and what stays a file and why | `docs/adr/0003-state-store-and-reconciler.md`: Decision, *What stays a file*, *Alternatives considered* | done |
| It says how ADR 0001's "bash house style" point now reads | ADR 0003, *How this ADR reads ADR 0001's "bash house style"* (quotes point 4, corrects the count, states which of its reasons still carry weight) | done |
| Schema v1: every table, column, key and index, with the file or files each replaces | section 3 (3.1-3.14), each table with its *Replaces* line | done |
| Times as columns, not mtimes | the conventions under section 3, and `cycles.started_at` which replaces a file mtime | done |
| The command's verbs, arguments, JSON output and exit statuses; each of the 13 groups maps to one verb that writes in one transaction | sections 2 and 4 (the group column of the verb table names the group of section 2) | done |
| The action journal: what a row holds, when it is written, how the next poll settles an action intended and not done | section 5 (schema, *When it is written*, the per-kind settlement table) | done |
| One GraphQL query with pages for the labelled issues, their labels, `labeled` events with actor and time, edits, closing pull requests, the bot's question comments; request count per idle poll and per candidate | section 6 | done |
| The import: order, idempotence, what happens to the old files | section 7 | done |
| Whether to vendor, with the size cost | section 8, with the 62 crates / 101 MB / 3,444 files measured at `5e2d5fa` | done |
| The inventory re-checked after #234: every file under `$TEHAS_HOME` with writer, reader and fate; a file left out is a bug | section 1 (1.1-1.15), including the files that no longer exist and the ones added since the research | done, and **a file left out is a bug**: it is the reviewer's job, not the author's claim, that proves it complete |
| The invariant list re-checked, each mapped to the sub-issue that tests it; new ones from #234 | section 9 (A-F with owners, plus G1-G6 from #234) | done |
| Baselines measured on tehas-tehas: poll p50/p95, requests per poll, lock probes per poll, fast gate, serial suite; the commands in the spec | section 10 | **open**: not measured on tehas-tehas; the commands are given, the sandbox column is labelled as what it is, and a person or an ops run must take them |
