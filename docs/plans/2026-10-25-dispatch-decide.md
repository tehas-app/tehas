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

D1. **Six pull requests**, each reviewable alone. Only the last changes
anything on a host. Part 3 was split in two (3a, 3b) on 2026-10-26, the
owner's choice: its eight suites hold 336 cases.

| PR | What | Tests ported from |
| --- | --- | --- |
| 1 | The crate, its types, rules 1 and 1b (the pause switches), the holds of rules 2 and 3 (the slots, the refresh lane, the limit, the slice's room, the endpoint, the disk, a red branch, the review cap), the order of rule 7 with the low gate, eligibility by label (allowed list, control issue, urgent, next, blocked, planning open, cycle running, closed by a pull request), and the demand | slots, queue, room, control, limit, red: their cases of the start path (166) |
| 2 | The rest of rule 6 (epic, cross-project, size, images, fixer, the park of rule 4c) and the clean-ups of rule 2 (quiet cycles, finished cycles, marks with no cycle, tehas:refreshing with no refresh) | guard, images, fixer, finish, sweep |
| 3a | Rules 4, 4a and 4b: the retry tick of a park's question, the outage retry and refreshes, the refresh half of the demand, and the cases of the earlier suites that touch them | park, refresh, approval |
| 3b | Rule 5 and 5a, and the planning half of rule 4: planning turns in flight, their answers, new sessions and refines | plan, plan_answer, plan_store, turns, refine |
| 4a | `snapshot()`: GitHub in one paged query and the per-issue reads, the store's read verbs, the locks and the files of `state/`; a `tehas-dispatch` binary that prints a snapshot and its decision and writes nothing | a `gh` stub that fails on any write |
| 4b | The host's probes (the slice, the model endpoints, the disk, the sandbox image), `tehas-dispatch --shadow` beside every poll, `logs/dispatch-shadow.log`, its install and the manual | start, checkout (the I/O), and the shadow test |

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

What PR 3b decides:

- A planning turn is the caller's to run (lib/planning.sh): the decision
  says which step, as an action, and logs what the poll logs before it. A
  turn that ended or was cut off is `plan_collect`, an outcome that waits is
  `plan_post`, a session whose issue no longer asks is `plan_close`, an end
  with its words is `plan_end`, a note the store keeps once is `plan_note`,
  and a turn is `plan_turn` (first, with its label; next, with the answer's
  text, who gave it and the reply's id). A refine of rule 5a spends its park
  (`park_retry`) before its turn.
- A session this poll collects or posts asks a new question, so the answer
  read before it answers nothing, and a session it closes is no longer open
  for the rules after it. The bash reads the open sessions again after such a
  write; that second read, and a store that fails it, is the caller's.
- The question's `plan` is plan-show's `.planning` as the store answers it.
  The decision shapes it as `tehas_plan_shape` does where the bash reads
  plan_view: the refused replies and held ticks from its notes, under both
  names, and an answer's millwright as fixer.
- Rule 5a's owed parks are read field by field. The bash reads them as one
  line with `read -r`, which shifts the fields of a value that is empty or
  holds a space; the store writes neither, and the shadow would show one.

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

   Measured on 2026-10-26 at 08:48Z, against tehas-app/tehas with 47 open
   issues: the spec's query, with `rateLimit { cost limit remaining }` added,
   cost **1 point** at `$page` 3, 10 and 30, and **4 points** at 100, which
   returned all 47 issues with their labels, label events, last five comments
   and closing pull requests, and three open pull requests. The limit was
   5000. GitHub charges by the request's node count divided by 100, not by
   the spec's model (382 for a page of three), so the snapshot pages 100
   issues at a time and needs no budget rule: a poll a minute of the whole
   queue costs about 240 points an hour.

Part 4 is split in two (4a, 4b), as part 3 was: the snapshot alone is about
as large as part 3a.

What PR 4a reads, and how:

- GitHub: the open issues in pages of 100, each with its labels, its label
  and rename events (the last 100), its text and edit, its count of
  sub-issues and the pull requests that close it, with the closed issues
  still marked tehas:running in the same request; `gh api user`; and, as the
  bash does, the comments of each issue asking a question or blocked, the
  states its blockers name, the pull request a refresh request names, the
  bot's pull requests while a cycle runs, the pull requests marked
  tehas:refreshing, and the branch's check runs. An issue the rules ask about
  that is not open is a closed one.
- The store: its read verbs, as lib/store.sh calls them and shapes their
  answers (the approval view, the history, the question on record).
- The host: the files of `state/` and `live/` the poll reads, the locks of
  cycles and planning turns (a lock taken and let go at once, as `flock -n`
  does), the slice (`systemctl show`, `docker info`, the cgroup files,
  `nproc`) and the disk (`df`).
- What 4a does not read, and the decision names in `needs` or reads as
  none: the model endpoints, the sandbox image (images, `--continue-pr`), the
  low gate (a command the shadow must not run a second time) and the
  workspaces of the sweep. They are 4b's.
- Every call has a bound, 30 s or the bash's own; a gh call that is no read is
  refused before it runs.
- `test/dispatch_shadow_test.sh` runs the bash poll and `tehas-dispatch
  decide` as dry runs on the same worlds of the dispatch tests, and their
  lines must be the same; the stub of gh answers the one page from the same
  fixtures the bash's calls read.

What PR 4b adds, and how the shadow runs:

- The reads 4a left: the model endpoints (mesh_check, through curl as the
  bash asks them), the sandbox image's labels, the model's answer to an
  image and whether the image takes `--continue-pr` (these three only when
  the decision names them in `needs`, and it is decided again), and the
  workspaces of the sweep. The comments of the issues that ask a question are
  read side by side, eight at a time.
- The low gate is never run: it is a command of the instance's, and the poll
  has run it already. A decision that would ask it names `gate` in `needs`.
- `tehas-dispatch.service` runs `tehas-dispatch shadow` before the poll and
  `tehas-dispatch compare` after it, both with "-" so neither can fail the
  poll, and keeps the poll's lines in `state/dispatch.last` through a pipe
  that keeps the poll's exit status (`set -o pipefail`). Before the poll, so
  both decide on the same world; the lines a start or a turn logs as it runs
  are the bash's own, and the log says them as `bash_only`.
- `install.sh`, `scripts/pack-release.sh` and the release workflow build and
  carry `libexec/tehas-dispatch`. A host without it polls as before.
  `state/dispatch-shadow.off` stops the shadow.

## For the owner to sign off

- D1: four pull requests in place of the issue's one, and the start left to
  #254.
- D4: two passes of `decide` when a lazy fact is needed.

## The issue's body

The issue's Done-when names `crates/dispatch`, 339 cases and rules 3a and a
second 6. The plan above replaces those; the body should be rewritten to
match once the owner signs off.
