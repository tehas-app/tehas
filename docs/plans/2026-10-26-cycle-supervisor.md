# #256: the cycle supervisor in Rust

Part of #244. Follows #253 and #254, whose `tehas-dispatch` decides a poll in
Rust and applies it through the action journal. This part does the same for a
cycle: `tehas-cycle <issue>` replaces `libexec/cycle` and `libexec/work`.

## What the survey of the bash found

Read on 2026-10-26 from main at f8048f52. `libexec/cycle` is 1456 lines,
`libexec/work` 1193; their tests are about 260 cases in eight files.

- **Two processes, one protocol of files.** `cycle` takes the lock and the
  start record, runs `work`, then reports. `work` prepares, runs the
  container and watches it, and leaves its account in `state/cycles/<n>.*`:
  `result.json`, `progress.json`, `runner_exit`, `status`, `nostart`, `oom`,
  `forks`, `stalled`, `timed`; `stopped` and `done` come from `tehas stop` and
  the dispatcher. In one binary most of these become values. The ones others
  write (`stopped`, `done`) and the ones others read (`cid`, the lock, the
  log) stay files.
- **The report is a pure decision.** `cycle` lines 503 to 1365 read those
  files, the outcome, the progress, the start record, the approval and three
  GitHub answers (the bot's pull requests on the branch, whether the issue
  carries needs-human-attention, the issue itself), and choose one of nine
  ends: merged, unreadable pull requests, a merged claim with no merge,
  awaiting merge, handoff parked, closed, pull request closed, died with an
  open pull request, and parked (never ran, provider outage, or plain). Each
  end is a list of writes: labels, a comment or a park question, the store's
  park, question and end, analytics, the improvement record. That is the
  shape `decide` and `apply` already have in `console/dispatch`.
- **The supervision is small.** `work` lines 854 to 1193: start the
  container, start the console, a 10 s loop of heartbeat, cgroup counts, the
  time limit plus grace and the stall, then a bounded `docker stop`, then a
  kill, then the notes. The signal path: TERM reaches `cycle`, which passes it
  to `work`, which stops the container (10 s grace), and `cycle` reports with
  one bounded try per GitHub call.
- **The preparation is most of `work`**, and much of it is shared. Before the
  container: the image's `--capabilities` and the flags and variables it must
  support, the expired workarounds, the approval and the text held to it, the
  size ledger, the attachments (lib/attachments.sh, 1074 lines, also used by
  `tehas plan`, `tehas dev` and the protolab), the workspace clone and the
  move aside, the gate check, the push check, the index seed, the mounts and
  the environment (lib/sandbox.sh, shared with `tehas dev`), the slice limits
  (lib/slice.sh) and the image on the record.
- **Rule 4 is in the preparation.** `test/work_test.sh` pins what the
  container is given. The Rust builds the same argument list and a test pins
  the whole list, in order.

## The pull requests

1. **The report, decided.** A crate `console/cycle`, binary `tehas-cycle`.
   `report` reads the facts of an ended cycle (the files above, the start
   record, the approval, and the three GitHub answers given as files) and
   prints what the bash would do: the status, step, cap and reason, the
   comment or the park question's text, the label changes, the store's park
   and end arguments, the history line. Fixtures come from the bash cases
   of `test/cycle_*_test.sh`. Nothing calls it on a host.
2. **The report's shadow.** `libexec/cycle` runs `tehas-cycle report
   --shadow` after its own report, with the same facts, and logs one line of
   what differs, as `tehas-dispatch shadow` does for a poll. It is off until
   a file under `state/` turns it on, and it costs a cycle nothing when it is
   off. This starts the 7 days early, while the rest is built.
3. **The report, applied.** The writes go through #254's action journal:
   the labels, the comment or question, the refreshing mark, then the store's
   park, question and end, analytics, the improvement record, the status run
   and the next poll. A comment comes after its label change (A6), the park
   on record before its question (B5). Retries and bounds as the bash has
   them, one bounded try each after a stop. `apply` is shared with
   `tehas-dispatch`: it moves to a library crate both use. That move is
   agreed with the session doing #254 first.
4. **The supervisor.** Start the container with the pinned argument list,
   the console beside it, the watch loop (heartbeat, cgroup counts, limit,
   stall), the bounded stop and kill, the notes as values. SIGTERM through
   `signal-hook`, as the console already handles it: the container gets a
   bounded `docker stop`, then a kill, then the report (C10, C11). The
   process cases of `test/work_test.sh` that test the watch run against it.
5. **The preparation.** Capabilities, workarounds, the approval and its
   text, the size ledger, the workspace, the gate check, the push check, the
   index seed, the mounts, the environment, the slice limits, the image on
   the record. A test pins the full `docker run` argument list. The pull
   request says it is a security change and widens nothing. The attachments
   are fetched by a bash command the binary runs.
6. **The start and the switch.** The lock and `cycle-start` with its retry
   and late-take rule, the issue's kind, `--restart`, then one binary. An
   instance setting `TEHAS_CYCLE=bash|rust`, bash by default, a malformed
   value exit 78; `tehasbot cycle` and `tehas work` choose by it. The process
   cases of `test/cycle_*_test.sh` and `test/work_test.sh` run against the
   binary.

Then 7 days of the report's shadow and 7 days of `TEHAS_CYCLE=rust` on
tehas-tehas, recorded in `host/docs/features.md`, and a separate pull request
removes the bash.

## Decisions

Owner, 2026-10-26:

- The six pull requests above, the report first.
- The attachments stay in bash: the binary runs a small bash command for the
  fetch. Porting lib/attachments.sh is an issue of its own.
- The report gets a shadow (part 2), so its 7 days count before the switch.
- The owner merges #256's pull requests.

## Shared ground with #254

`console/dispatch/src/apply/start.rs` starts `tehas-cycle@N.service` and
waits for the start on record. #256 keeps `cycle-start` and `cycle-show` as
they are; a change there is told to the session doing #254 first.

## Progress

- Part 1 (2026-10-26): crate `console/cycle`, `tehas-cycle report`. The
  fixtures were made by running test/cycle_*_test.sh against a copy of
  libexec/cycle that wrote, for each report, what it read (the facts), what
  GitHub answered, and what it did. 265 reports; the 19 other runs refused
  their start and made none. The Rust gives the same warnings, end and park
  arguments, `done` line and GitHub writes for all of them.
- Found while porting, kept as the bash has it: the check that a progress is
  newer than the cycle's lock (`[ ! "$progress" -nt "$lock_taken" ]`)
  compares with a file named by a number, which never exists, so it never
  holds. It costs nothing: the cycle removes the progress before tehas work
  runs. The Rust leaves the check out.
