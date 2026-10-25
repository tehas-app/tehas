# Plan: the dispatcher's decisions in Rust, run in shadow (#253)

Part of #244. Builds on #245 (spec `docs/specs/2026-10-21-state-store.md`),
#248 (the workspace), #249 to #252 and #257 (the store and its tables).

## What changed since the issue was written

- `libexec/dispatch` is 3,688 lines, not about 1,100. Its tests are 21 files,
  `test/dispatch_*_test.sh`, with 784 cases, not one file of 339.
- The header's rules are 1, 1b, 2, 3, 4, 4a, 4b, 4c, 5, 5a, 6 and 7.
- The workspace is `console/`, not `crates/`: a new crate is a member there.
- The spec's section 6 is the GraphQL query, and its cost figures are a model
  that this issue must measure first.

## Decisions

D1. **Four pull requests**, each reviewable alone. Only the last changes
anything on a host.

| PR | What | Tests ported from |
| --- | --- | --- |
| 1 | The crate, its types, rule 1b, the holds of rules 2 and 3, the order of rule 7, and eligibility by label (allowed list, control issue, urgent, next, blocked, planning open, cycle running, closed by a pull request) | slots, queue, room, control, limit, red (156 cases) |
| 2 | The rest of rule 6 (epic, cross-project, size, images, fixer, the park of rule 4c, the low gate) and the clean-ups of rule 2 (quiet cycles, finished cycles, marks with no cycle) | guard, images, fixer, finish, sweep (190) |
| 3 | Rules 4, 4a, 4b, 5 and 5a: ticks, outage retries, refreshes, planning turns, refines | park, plan, plan_answer, plan_store, refine, refresh, turns, approval (330) |
| 4 | `snapshot()`, the `tehas-dispatch --shadow` binary, the shadow log and the manual | start, checkout (the I/O), and the shadow test |

The start itself (`start_cycles`: approval record, label, comment, unit,
wait) is #254's: it is acting, and this issue only decides.

D2. **A library crate `console/dispatch`** (`tehas-dispatch`). PRs 1 to 3 add
no binary and nothing is installed, so they cannot change a host. PR 4 adds
the binary.

D3. **`decide(&Snapshot, &Settings, now) -> Decision`**, pure. A `Decision`
holds:

- `actions`: the writes, in order, as data: `Label {issue, add, remove}`,
  `Comment {issue, body}`, `Untick {issue, comment}`, `StartUnit {issue,
  kind}`, `StopUnit {issue, why}`, `Record {...}`.
- `report`: what `live/dispatch.json` says today (`why`, `hold`, `count`,
  `room`, `free`, `awaitRoom`, `limitRoom`, `refreshRoom`).
- `log`: the lines the bash poll logs, word for word, so the shadow can be
  compared with `logs/dispatch.log` line by line.

D4. **The snapshot holds facts, not answers to questions.** The bash asks some
things only when it needs them: the low gate (rule 7), the model endpoint
(rule 3), the disk, the branch check. `decide` takes them as optional facts.
When one is needed and absent, the decision says so (`needs: [LowGate]`), the
caller asks, and `decide` runs again with the answer. It stays pure, and the
number of asks stays what the bash makes.

D5. **Fixtures as JSON**, `console/dispatch/tests/fixtures/<suite>/<case>.json`:
`{case, snapshot, settings, now, expect: {actions, report, log}}`. Each names
the bash case it was ported from by its `t_case` text. A case that tests I/O
only (a gh call that hangs, a fetch, a lock) is not ported, and the PR lists it.

D6. **Text is ported byte for byte.** Comments and log lines are the ones the
bash writes today, so the shadow's difference is empty when both agree.

D7. **The GraphQL cost is measured first** (spec section 6): PR 4 records the
`rateLimit { cost }` of one real query of the spec's shape before it pages,
and replaces the model's figures with what it read.

## For the owner to sign off

- D1: four pull requests in place of the issue's one, and the start left to
  #254.
- D4: two passes of `decide` when a lazy fact is needed.

## The issue's body

The issue's Done-when names `crates/dispatch`, 339 cases and rules 3a and a
second 6. The plan above replaces those; the body should be rewritten to
match once the owner signs off.
