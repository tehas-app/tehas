# Plan: one publisher for `live/` (#252)

Part of #244. Builds on #683 (schema 2, the shell commands on the store).

## What changes

Today four commands write the files under `live/` that describe cycles and
health, each in its own way:

- `libexec/work` writes `live/cycles/<n>.json` every 10 s and then calls
  `tehas_live_assemble`, which writes `running.json`, read-modify-writes
  `endings.json` with no lock and prunes `live/cycles/`.
- `libexec/health` calls `tehas_live_assemble` too, then writes `health.json`
  through a fixed `.new` name, then creates or removes `health.down`.
- `libexec/dispatch` writes `dispatch.json` through a fixed `.new` name, and
  its 20-minute stuck rule reads `updatedAt` from `live/cycles/<n>.json`.
- `libexec/status` and `libexec/energy` write `status.json`, `platform.json`
  and `energy.json` through fixed `.new` names.

After this issue:

- The heartbeat is a row. `tehas work` calls the store verb `cycle-heartbeat`
  every 10 s. The stuck rule reads the newest heartbeat from `cycle-show`.
- One store verb, `publish`, writes `live/cycles/<n>.json`, `running.json`,
  `endings.json` and, when given, `dispatch.json`, `health.json` and
  `health.down`. It holds `state/publish.lock` while it writes, writes each
  file to a unique hidden temp name and renames it.
- `status`, `platform` and `energy` keep their own files and writers, with
  unique temp names.

## Decisions

D1. **Schema 3 adds `heartbeats.text`.** The published cycle file carries
`statusText`, the driver's last notice after the console's mask. The heartbeat
table has no column for it. The step to 3 is one `ALTER TABLE ... ADD COLUMN`,
like step 2. #676 also wants a schema 3 for the millwright grant. Whichever
merges second takes schema 4.

D2. **`cycle-heartbeat <issue> --attempt N (--running | --ended) --status <w>
--step <s> [--text <t>]`.** It refuses a heartbeat for an attempt that is not
the open one (`conflict`), so a late tick of an ended cycle cannot make it
look running again. It keeps the newest 100 rows per issue, the bound the spec
gives `prune`. The text is cleaned like any container-made value and cut to
300 characters.

D3. **`cycle-end` carries the text forward.** Its heartbeat row copies the
`text` of the issue's newest heartbeat, so the last cycle on the page keeps
its last notice, as it does today.

D4. **`cycle-show` answers `heard_at`**, the newest heartbeat of the open
attempt, or null. The stuck rule in `dispatch` reads it in place of the file.
The rest of the rule (a fresh start counts as heard from, the 1200 s bound,
the episode record) is unchanged.

D5. **`publish` renders the cycle views from the rows.**

- A cycle shown as running: the open attempt, whose newest heartbeat says
  running.
- A running heartbeat older than 120 s whose issue lock is free is shown as
  `lost`, `running: false`. Its `updatedAt` is the heartbeat's time plus 120,
  so the view does not change on every publish. Today the time was the moment
  it was found. Nothing is written to the tables: `publish` only reads.
- `last`: the finished cycle (newest heartbeat not running, or lost) with the
  newest `updatedAt`.
- `startedAt` is `cycles.started_at`, the start on record. Today it was the
  moment the container started, a few seconds later. The `endings` rows
  already use the start on record, so the two now agree.
- `endings.json`: the `endings` rows, oldest first, plus any lost cycle not
  among them, the last ten.
- `live/cycles/<n>.json` exists for each running cycle and for `last`; any
  other `live/cycles/<number>.json` is removed. Names that are not
  `<number>.json` are left alone.

D6. **Shapes stay byte-compatible.** `live/cycles/<n>.json` is pretty-printed
with two spaces, as `jq -n` wrote it; `running.json` and `endings.json` are
one line, as `jq -c` wrote them; key order is as today. Fixtures are captured
from the current `tehas_live_assemble` before it is removed
(`test/fixtures/live/`), and the store's tests compare against them.

D7. **`dispatch.json` and `health.json` pass through `publish`** as
`--dispatch <file>` and `--health <file>`. Their commands still compute them:
their tables (`polls`, `health_samples`) have no writer before #253 and #254
move the poll, and `health.json` holds much more than `health_samples`. The
publisher checks each is a JSON object (and `health.json` has a string
`status`), writes the bytes as given and orders `health.down`: on `fail` it
creates `health.down` before it renames `health.json`; otherwise it renames
`health.json` before it removes `health.down`. **Owner sign-off:** this is
"one writer, one lock, atomic" for these two files, not yet "rendered from the
database".

D8. **The lock.** `publish` takes `state/publish.lock` with `flock` semantics
(`File::lock`), trying for up to 5 s, then answers `busy` (exit 1). The lock
file is made 0600 if missing.

D9. **Temp names.** `live/.<name>.<pid>.<n>.new`, created with `O_EXCL`,
mode 0644. The docs server already hides `/.*` and `*.new`. A failed write or
rename removes the temp file and fails the call (B1); files already renamed in
the same call stay, since each is whole.

D10. **No `--dir`.** The spec names `publish --dir <live>`. The store already
knows the home, so it writes `$TEHAS_HOME/live` and takes no path.

D11. **Shell callers.**

- `lib/live.sh` keeps its name and becomes the wrapper:
  `tehas_live_publish [--health <file>] [--dispatch <file>]`.
- `tehas work`: `now_file` calls `cycle-heartbeat`, then `tehas_live_publish`.
  It no longer writes `live/cycles/`. A heartbeat the store refuses is logged
  once per cycle and the cycle runs on.
- `tehasbot cycle`: `tehas_live_publish` after `cycle-end`.
- `tehasbot health`: `tehas_live_publish` first (fresh views), then the report
  to a temp file under `$TEHAS_HOME/tmp`, then
  `tehas_live_publish --health <file>`.
- `tehas dispatch` (`report_hold`): the report to a temp file, then
  `tehas_live_publish --dispatch <file>`.
- `status`, `platform`, `energy`: `mktemp` in `live/` with a leading dot and
  the suffix `.new`.

## Units

Each unit is a branch stacked on the one before, built tests first.

1. **`publish-live-fixtures`**: a test that runs today's `tehas_live_assemble`
   on chosen cycle files and checks the output against
   `test/fixtures/live/`. The fixtures are what D6 compares against.
2. **`publish-live-store`**: schema 3, `cycle-heartbeat`, `cycle-show`'s
   `heard_at`, `cycle-end`'s text, and `publish`, with Rust tests: shapes
   against the fixtures, the lost rule, pruning of `live/cycles/`, the
   `health.down` order, a held lock, a failed rename, the 100-row bound.
3. **`publish-live-callers`**: the shell callers of D11, `lib/live.sh`, the
   stuck rule, shell tests, and the manual (`host/docs/tools.md`,
   `troubleshooting.md`, `operating.md` where they name the files).

## Invariants

- A15: `health.json` is the file `health` wrote; its age checks are unchanged.
- B1: every file `publish` writes is renamed from a unique temp name, and a
  failed rename fails the call.
- C6: the stuck rule reads the heartbeat row.

## Out of scope

`demand.json`, `top.json`, `previews.json`, `protolab.json`, `analytics.json`
and `console-<n>.txt` keep their writers. The tables `polls`,
`health_samples`, `energy_samples` and `platform_state` get writers with
#253 and #254.
