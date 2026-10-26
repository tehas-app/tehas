# #254: the dispatcher applies its decisions through the action journal

Part of #244. Follows #253 (`docs/plans/2026-10-25-dispatch-decide.md`), whose
`decide()` says what a poll does and whose shadow (`tehas-dispatch shadow`)
sets that beside the bash poll. This part makes `tehas-dispatch` do it.

## What the survey of the bash found

Each action `decide()` emits was traced to the code of `libexec/dispatch` and
`lib/` that performs it (2026-10-26). The facts that shape the work:

- **An action does not yet carry all it needs.** The line a failed label
  change logs differs by site for the same shape (the two `tehas:running`
  sites have no suffix, the rest name the flags). A start lacks the verb of
  its `start #N (VERB by BY)` line and the title and body whose sums go into
  the approval record. `reap` lacks its reason. One `park_retry` shape stands
  for three behaviours: one try with a warning (the outage retry), three tries
  with a warning (a skipped refine), and three tries after the brief is
  written, whose failure ends the poll with 1 (a refine about to start).
  `plan_collect` is two paths, chosen by whether the turn's `.rc` file exists.
- **What follows a write depends on whether it worked.** A label change's
  comment and log line come only after it worked; an untick's note only after
  the PATCH worked. The records of `refresh_said` and `park_told` are written
  whether or not their comment worked, because `say` never fails the poll.
- **Some of the poll is no action.** `units_ok`, `index_gate` and
  `refresh_checkout` before a start, `sweep-locks`, the live files, health,
  the status command and the EXIT trap (tidy, `poll-record`, analytics) are the
  poll's, not an action's.
- **Two places decide from an answer, not from the snapshot.** The quiet cycle
  (bash reads the stop counts from the answer of `cycle-note`) and the stop of
  a finished cycle (bash waits up to 330 s and counts the running cycles
  again). The Rust decides from the snapshot. The shadow will show whether
  that differs in practice; where it does, `apply` asks again as the bash does.

## The pull requests

1. **The journal and the plain writes.** The store's journal verbs
   (`poll-begin`, `journal-record`, `journal-attempt`, `journal-settle`,
   `journal-open`; prune keeps a poll's row while an action names it). Each
   action carries what applying it needs: the failure line, the verb, the
   reason. `apply` for the writes that stand alone: the label changes with
   their comment, log line and answered question; the unticks with their note;
   the refreshing mark taken off a pull request; `refresh_said`, `park_told`,
   `plan_note`; the files `refresh_drop`, `refresh_held`, `gate_held`,
   `branch_note`; `reap`. Every GitHub write goes through the journal:
   intended, attempted, settled; one the journal could not record is not
   made. Order rule A6 (a comment after its label change, and none after one
   that failed) is tested. `apply` is a library in this part: nothing calls
   it on a host yet.
2. **Starts and stops**, in two pull requests: 2a the starts, 2b the stops.
   A start reads more than the survey's table shows: its approval's inputs
   are the comments in the approval's window, a retry's comments on the
   bot's pull requests, the millwright's note and the refresh's, so 2a ports
   those reads too. What the poll does before its starts (systemd's answer,
   the code index, the checkout) moves to part 4, where the poll is. The approval record, the mark (A1: mark before
   start; A2: the retry's untick after the mark), the unit and its wait, the undo when a start fails; `stop_done`,
   `quiet`, the sweep. The poll's own steps before a start.
3. **Planning.** `plan_turn` (first and next), `plan_collect` with its two
   paths, `plan_post`, `plan_end`, `plan_close`, the three `park_retry`.
4. **The whole poll.** The journal settled first, against fresh reads, before
   any action (never replayed blindly); a failed read ends the poll before any
   action (A14); the lock, `live/dispatch.json`, the log, the exit statuses;
   the setting `TEHAS_DISPATCHER=bash|rust` (bash the default, malformed is
   78); the process cases of `test/dispatch_test.sh` run against the binary.
   Two things to settle there: a poll that `apply` ends early (a quiet
   cycle's store that cannot be used, exit 78) logs only the decision's lines
   up to the action it reached, as the bash exits there; and no child of the
   poll inherits the poll's lock, as the bash's `8>&-` sees to.

The default changes only after 7 days of shadow and 7 days as the dispatcher
on tehas-tehas, each recorded in `host/docs/features.md`. Removing
`libexec/dispatch` is a pull request of its own after that.

## Decisions

- `journal-record` takes `--poll <at>`, the row `poll-begin` wrote, rather
  than the newest poll: a caller names what it means.
- `journal-settle` takes `--abandoned` beside `--result`: section 5's
  settling poll needs a word for an action its fresh read says did not
  happen and need not.
