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

D1. **Five pull requests**, each reviewable alone. Only the last changes
anything on a host. Part 3 was split in two (3a, 3b) on 2026-10-26, the
owner's choice: its eight suites hold 336 cases.

| PR | What | Tests ported from |
| --- | --- | --- |
| 1 | The crate, its types, rules 1 and 1b (the pause switches), the holds of rules 2 and 3 (the slots, the refresh lane, the limit, the slice's room, the endpoint, the disk, a red branch, the review cap), the order of rule 7 with the low gate, eligibility by label (allowed list, control issue, urgent, next, blocked, planning open, cycle running, closed by a pull request), and the demand | slots, queue, room, control, limit, red: their cases of the start path (166) |
| 2 | The rest of rule 6 (epic, cross-project, size, images, fixer, the park of rule 4c) and the clean-ups of rule 2 (quiet cycles, finished cycles, marks with no cycle, tehas:refreshing with no refresh) | guard, images, fixer, finish, sweep |
| 3a | Rules 4, 4a and 4b: the retry tick of a park's question, the outage retry and refreshes, the refresh half of the demand, and the cases of the earlier suites that touch them | park, refresh, approval |
| 3b | Rule 5 and 5a, and the planning half of rule 4: planning turns in flight, their answers, new sessions and refines | plan, plan_answer, plan_store, turns, refine |
| 4 | `snapshot()`, the `tehas-dispatch --shadow` binary, the shadow log and the manual | start, checkout (the I/O), and the shadow test |

The demand of PR 1 counts the ready issues only. The refresh requests it also
counts, and the hold of refreshes it writes, come with the refresh rules in
PR 3.

Each fixture file lists the cases of its suite that it does not port and why:
`later` for those of a later pull request, `not_ported` for those that test
I/O only (a count of calls, how a command runs).

The start itself (`start_cycles`: approval record, label, comment, unit,
wait) is #254's: it is acting, and this issue only decides.

PR 2 also ports the pause cases of the guard suite, which PR 1 left without
fixtures, and from the suites of PR 3 and PR 4 the cases of its own rules:
the park's hold of rule 4c (park), the stray tehas:refreshing (refresh), and
the epics, cross-project issues and marks with no cycle (start).

What PR 2 decides, and what it leaves to the caller:

- A write whose result changes what a poll says is the caller's: the comment
  that follows a label change that worked, the line of a container docker
  removed, the record of a tell. The action carries the line or the comment
  for that case.
- A quiet cycle's episode is read from the store's record, as a dry run reads
  it; the caller writes the note (`quiet`) and asks the stop (`stop`) or
  records the line (`asked`). A store that refuses those writes is the
  caller's to say.
- A finished cycle that is stopped frees its slot in the same decision, as if
  the stop worked. The bash waits for the lock and counts again; the shadow
  of PR 4 will show any difference. Its mark is left to its own end, not
  reaped.
- The sweep of unused locks (sweep-locks) is the store's own rule, and the
  caller runs it on every poll. Which names an issue has on the disk, and how
  old they are, are the snapshot's (`leftovers`).
- The size measure ports the count of files alone, the one count the poll
  reads.
- A fact the snapshot lacks is named in `needs`, never read as none: the
  marked issues, the clock, the host's name, the refresh history, the
  leftovers, an issue's park, approval and current approval.
- Two known differences from jq, left for the shadow to measure. The texts of
  an answer are read in key order, where jq reads them in GitHub's order, so
  of two promised files the log may name the other one; keeping GitHub's
  order needs serde_json's `preserve_order`, which would change the order of
  every crate's JSON in the workspace. And jq's `$` also matches before a
  final newline, which no field the checks read ends with.
- The fixtures match a log line by its text, not as a whole line: the shadow
  of PR 4 is what compares the log line by line.

What PR 3a decides:

- A retry tick or an outage retry starts one cycle and the decision ends
  there, as the poll does. The untick of the tick follows its start.
- A planning question that has an answer names `planning` in `needs`: its
  turn is part 3b's. One with no answer decides nothing, as in the bash.
- The writes a refresh or a park leaves to the caller: the request removed
  (`refresh_drop`), the note of what was said (`refresh_said`), the hold of
  refreshes (`refresh_held`), the retry spent (`park_retry`), the answer
  recorded (`answered`).

D2. **A library crate `console/dispatch`** (`tehas-dispatch`). PRs 1 to 3 add
no binary and nothing is installed, so they cannot change a host. PR 4 adds
the binary.

D3. **`decide(&Snapshot, &Settings, dry) -> Decision`**, pure. Time enters
through the snapshot (a limit already read as expired or not), so the same
snapshot always gives the same decision. A `Decision` holds:

- `exit`: the poll's exit status.
- `log`: the lines the bash poll logs, word for word, so the shadow can be
  compared with `logs/dispatch.log` line by line.
- `actions`: the writes, in order, as data: a label change with the comment
  that follows it once it worked, a start with its approval and the labels it
  takes off, the low gate's marker, the branch check's note.
- `report` and `demand`: what `live/dispatch.json` and `live/demand.json` say,
  without their time. A dry run writes neither.
- `picked`: the issues whose cycle starts, in order.

D4. **The snapshot holds facts, not answers to questions.** The bash asks some
things only when it needs them: the low gate (rule 7), the disk, the branch
check, the issues awaiting merge. `decide` takes them as optional facts. When
one is needed and absent, the decision names it in `needs`, the caller asks,
and `decide` runs again with the answer. It stays pure, and the number of asks
stays what the bash makes. The model endpoint is asked by the snapshot, which
gives `decide` its hold, the word for `live/dispatch.json` and the lines it
logged.

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
