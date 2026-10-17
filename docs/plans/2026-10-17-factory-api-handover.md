# Handover, 2026-10-17: building the factory API

For the session that builds the factory API in tehas. Read `AGENTS.md`, then
`docs/specs/2026-10-17-factories.md` (merged in #296, amended since for the
fixer). This page says where things stand, what to build first and what to
watch for.

## Where things stand

- The spec is merged. No command of it exists yet.
- One issue per step, in this order. Industry waits on the first three.

  | Step | Issue | What |
  | --- | --- | --- |
  | 1 | #299 | `tehas api --json`, `tehas status --json`, demand written on every poll, `host/docs/api.md` started |
  | 2 | #300 | `tehas:all-hands`: label, rule 5 check, reported only |
  | 3 | #301 | `tehas limit`, `tehas drain`; the dispatcher, planning turns and the improvement loop honour it |
  | 4 | #302 | `tehas history --json`, `tehas export` |
  | 5 | #303 | `tehas update --to <tag>` |
  | 6 | #304 | the host page; unattended install |

- The fixer's grant is merged (#297). It writes
  `state/cycles/<issue>.history.jsonl`, which `tehas history` (step 4)
  builds on. `tehas api` lists `fixer-grant` and the `fixer` login (spec,
  "tehas api").
- tehas-industry (private, `janit/tehas-industry`) builds against these
  commands. Its session starts from that repository's handover and will use
  the JSON shapes in the spec as fixtures.

## Where the code is today

- `tehas status` is `libexec/summary`. It reads only `live/health.json`,
  through `lib/status.sh`. `tehasbot health` (`libexec/health`) writes that
  file at every poll and at the end of a cycle.
- The hold reason and slice numbers are in `live/dispatch.json`, written by
  `report_hold` in `libexec/dispatch`, and only by a poll that reaches the
  room check. Step 1 has to make the demand, pause, all-hands and limit
  state current on every poll that gets past the settings, a paused one
  included, and have `status --json` read it from one place.
- A planning turn is its own unit, `tehas-plan-<issue>.service`, since #305.
  Step 3's `--plan off` holds the start of that unit; an answer to a waiting
  question is not held.
- `bin/tehas` maps each command to a script in its `script()` case. New
  commands (`api`, `limit`, `drain`, `history`, `export`) go there and in its
  usage text, and `test/cli_test.sh` pins that.
- Existing exit statuses (spec, "Principles of the API", 4) and the header
  comment of every script, which is its `--help` text.

## Watch for

- **Others are in `libexec/dispatch` right now.** Session tehas-07 is
  building the fixer's factory-side parts on branches `park-marker` (#298),
  `fixer-order`, `fixer-refresh` and `fixer-plan-answers`, all in the
  dispatcher and `test/dispatch_test.sh`. Expect conflicts. Rebase on
  `origin/main` before each PR, and message tehas-07 (`ListAgents`,
  `SendMessage`) before a change that moves code they touch.
- **The park marker and history fields.** #298 adds park facts that step 4's
  `history` must carry (spec, "tehas history"). Build step 4 after #298
  merges, or agree the fields with tehas-07 first.
- **Rule 4.** None of this should change a cycle container's mounts or
  environment. If a step seems to need it, stop and ask.
- **Fail closed, but no exit 78 for a limit.** An expired or unreadable
  limit holds every start and says why; the poll still does its cleanup.
  A paused factory stays paused under any limit.
- **Tests pin shapes.** Each `--json` command gets a test of its fields and
  types (spec, "Tests"). A factory with no limit set behaves exactly as
  today in every existing dispatch case.

## Working here

- Work in a worktree beside the shared checkout, for example
  `git worktree add -b factory-api-status ../tehas-factory-api origin/main`.
  Other sessions switch branches in `~/work/tehas`.
- One pull request per issue, in order. Reference the issue in the pull
  request, not in the commit subject.
- Run `scripts/verify.sh` to a file and check its own exit status and every
  `FAIL` line; do not pipe it. Quote its output in the pull request.
- The manual describes what is true: change `host/docs/api.md`,
  `operating.md` and `features.md` (covered by tests only) in the same pull
  request as the behaviour.
- Nothing merges on its own. Open the pull request and stop; the owner
  merges.
