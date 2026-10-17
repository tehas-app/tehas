# Spec: the factory resolves conflicts in its own pull requests

**Status**: Draft 2026-10-17, asked for by the owner. Not built.

## Purpose

A pull request the factory opened waits in `tehas:awaiting-merge` for a
person. While it waits, other work merges to the default branch, and the
pull request starts to conflict with it. Today it stays that way until a
person resolves the conflict by hand, or closes it and starts again, which
loses the review. Meanwhile it holds a place under `TEHAS_MAX_AWAITING`.
Cycles that run side by side touch the same files, so this happens often
(`host/docs/features.md`, `host/docs/host.md`).

A conflict needs no judgement to find: GitHub says a pull request is
`CONFLICTING`. So the factory takes it on itself. On a schedule it finds
its own pull requests that conflict and runs a cycle that merges the
default branch in, resolves the conflicts and pushes to the same pull
request.

## Relation to the fixer

tehas#294 part 2 ("stale pull requests") had the fixer decide when to
refresh a pull request: on a conflict, a failing check or requested
changes. Conflicts move here, to the factory, because they need no
decider. Failing checks and requested changes stay with the fixer. Both
use the same way of continuing a pull request (below), so part 2 builds on
this.

## What it needs from pi-rukas

A cycle cannot continue an open pull request today: pi-rukas stops with
`existing-pr-detected`. trail-openers/pi-rukas#1468 adds an opt-in flag
(`--continue-pr` there, the name to check when it lands), advertised in
`--capabilities`: check out the pull request's head branch, merge the
default branch in with a merge commit, work from there, push fast-forward
to the same branch. Never a rebase, never a force-push, never a second
pull request.

This can be built and tested now with stubs. On a host it does nothing
until the sandbox image advertises the flag, and says so.

## Settings

In `lib/config.sh`, documented in `instances/example/instance.env`:

- `TEHAS_REFRESH_CONFLICTS` [0]: 1 turns the feature on. Off by default:
  it lets the factory start cycles with no fresh label. Anything other than
  0 or 1 stops every command that loads settings with exit 78.
- `TEHAS_REFRESH_MAX` [2]: refreshes of one pull request since the person's
  approval its cycle rests on. A positive integer, or exit 78.

Turning it on for tehas-tehas is a change to a real instance (rule 8): the
pull request proposes it in its text and does not make it.

## The schedule

`tehasbot refresh`, run by `tehas-refresh.service` from
`tehas-refresh.timer` once an hour (`OnUnitInactiveSec=1h`, after a delay on
boot, as `tehas-preview-sweep.timer` does). Deterministic, no model, no
container. With the setting off it exits 0 at once and calls nothing.

A run lists the bot's open pull requests in `TEHAS_REPO` and keeps those
where all of these hold:

- GitHub says the pull request closes exactly one issue, and that issue is
  open, carries `tehas:awaiting-merge` and is not the control issue;
- that issue is the only one it closes and it is the only open pull request
  that closes the issue;
- its head branch is in `TEHAS_REPO`, not a fork, and it is not a draft;
- GitHub's `mergeable` is `CONFLICTING`. `UNKNOWN` is skipped this run:
  GitHub computes it lazily and the next run reads it again.

For each it writes a request, `state/refresh/<issue>.json`:
`{at, pr, head_sha, base_sha}`. It removes a request whose pull request no
longer qualifies. It writes one line per run to `logs/refresh.log`: how many
pull requests it read, how many conflict, how many requests it wrote. A
call to GitHub that fails writes nothing and exits 1; old requests stay as
they are and the dispatcher checks each again before it starts.

The run decides only that a pull request conflicts. Whether a refresh may
start is the dispatcher's, at the start.

## The dispatcher's rule

A new rule after 4a. A refresh request starts a cycle on the word of the
person whose approval the pull request's cycle rests on: the approval
record's `by`, or its `rests_on.by` when that cycle was the fixer's. It
starts only when all of these hold, read at the start:

- `TEHAS_REFRESH_CONFLICTS` is 1, and the sandbox image advertises the flag
  of pi-rukas#1468. Otherwise it starts nothing, and the hold is said once
  in the log and in `tehas status`;
- the issue is open, carries `tehas:awaiting-merge`, and carries none of
  `tehas:running`, `tehas:question`, `tehas:fixer-escalated`, `tehas:plan`;
  no cycle runs on it and no planning session is open;
- the issue's body still has the `body_sha256` of that approval, and nobody
  outside the allowed list renamed or edited it since (`unchanged_since`);
- the pull request in the request is still the one open pull request that
  closes the issue, still the bot's, its head still at `head_sha`, and
  GitHub still says `CONFLICTING`. A person who pushed to the branch since
  is handling it;
- fewer than `TEHAS_REFRESH_MAX` refreshes of it since that approval, and no
  refresh of it since that approval parked. A parked refresh is a person's
  to look at; the factory does not try again on its own.

A refresh that fails a check is dropped: the request is removed and the log
says why. It is not said on the issue unless a person can act on it: a
refresh held at its cap or after a park says so on the issue once.

Rules 1, 2 and the slice's room hold refreshes as they hold any start:
pause, all-hands, `tehas limit` and concurrency. The awaiting-merge cap
does not: the start takes `tehas:awaiting-merge` off, so a refresh moves
one pull request from waiting to running and adds none.

Order, rule 7: `tehas:urgent` first, then refreshes, then `tehas:next`, then
the lowest issue number. Work that is done but stuck goes before new work.

The approval record of a refresh has `how: "refresh"`, `by` the person,
`rests_on` the approval it rests on (as the fixer's does), `refreshes` the
count since it, and `refresh_of: {pr, head_sha, base_sha}`. Its
`retry_context` is one fixed entry, source `refresh`, written by tehas, not
taken from any comment:

> This pull request conflicts with `<branch>`. Merge `<branch>` into its
> branch, resolve the conflicts so the change still does what the issue
> asks, run the gate, and push to the same branch. Do no other work.

The start comment on the issue says it is a refresh, names the pull request
and whose approval it rests on.

## The cycle

`libexec/work` passes the continue flag only when the approval record's
`how` is `refresh` and the image advertises it; any other start runs as
today. Nothing else about the container changes: no new mount, variable or
token (rule 4, pinned by `test/work_test.sh`).

## Marking the pull request while it is refreshed

The pull request carries the label `tehas:refreshing` while its refresh
runs, so a person sees not to merge or push to it now. `tehas labels` makes
the label.

- The dispatcher applies it to the pull request with the start, beside
  `tehas:running` on the issue. A start whose label change fails starts
  nothing, as today.
- `tehasbot cycle` takes it off when the cycle ends, whatever the outcome:
  pushed, parked or failed.
- The reaper (rule 2) takes it off a pull request whose issue has no cycle
  running, so a cycle that died does not leave it behind. Only the bot's
  `tehas:refreshing` counts; one applied by anyone else is taken off.
- A refresh request on a pull request that already carries it waits.

Not a draft. A draft is a person's signal too: the factory could not tell
its own draft from a person's, and a cycle that died would leave the pull
request a draft until someone noticed. A label is tehas's alone, and the
reaper can clear it.

## The outcome

The outcome is read as any cycle's. A refresh that pushed goes back to
`tehas:awaiting-merge` with the same pull request; its handoff names the
files it resolved. A refresh that parks parks the issue as today, and the
rule above keeps the factory from trying again.

## The factory API

`tehas api --json` lists `refresh-conflicts` in `features` once this is
built (`host/docs/api.md`), so industry can tell a factory that has it. A
refresh waiting for room counts in `tehas status --json` demand like a
waiting `tehas:ready`.

## Not in this

- Failing checks and requested changes: the fixer's, tehas#294 part 2.
- Pull requests the bot did not open, from forks, or closing several
  issues.
- Rebasing or force-pushing, merging, closing a pull request.
- Conflicts between two open pull requests of the factory. Each is resolved
  against the default branch when the other merges.

## Security

This widens who starts a cycle: the factory itself, with no fresh label.
Say so in the pull request. What keeps it narrow is the point of the design:
off by default; only the bot's own pull request of an issue a person
approved; the body that person approved, unchanged; only a conflict GitHub
reports, checked again at the start; the same branch, fast-forward only; a
cap per pull request and no retry after a park; a fixed `retry_context`, so
no comment text reaches the cycle through this path. Do not relax any of it
to make a test pass.

## Done when

- `tehasbot refresh` exists with a header comment (what it does, usage, exit
  statuses), shellcheck clean, and the two units in `host/systemd/`,
  installed by `install.sh`;
- `test/refresh_test.sh` covers: the setting off calls nothing; a
  conflicting pull request gets a request; `MERGEABLE` and `UNKNOWN` get
  none; a fork, a draft, a pull request closing two issues, two open pull
  requests for an issue get none; a request removed when its pull request
  no longer qualifies; a failed GitHub call writes nothing, exit 1;
- `test/dispatch_test.sh` covers each check of the rule: a refresh started;
  held while the image lacks the flag; dropped on a changed body, a changed
  title, a moved head, a pull request that no longer conflicts, a running
  cycle, `tehas:question`, `tehas:fixer-escalated`; `tehas:refreshing`
  applied to the pull request with the start, a start whose label change
  fails starting nothing, a leftover `tehas:refreshing` cleared by the
  reaper, one applied by someone else taken off; held at the cap and after
  a parked refresh, said once; held by pause and by concurrency but not by
  the awaiting-merge cap; ordered after `tehas:urgent` and before
  `tehas:next`; the approval record's fields and fixed `retry_context`;
- `test/cycle_test.sh`: `tehas:refreshing` comes off the pull request on
  every outcome;
- `tehas labels` makes `tehas:refreshing`;
- `test/work_test.sh`: the flag is passed for a refresh when advertised and
  never otherwise; the container's mounts and environment are unchanged;
- `lib/config.sh` validates the two settings, `instances/example/instance.env`
  documents them, and the settings tests cover exit 78;
- `host/docs/operating.md` has a section on refreshes (what it reads, when it
  starts, how to turn it off, the log line), the dispatcher's order lists
  refreshes, `host/docs/api.md` lists the feature, and
  `host/docs/features.md` lists it as not yet proven on a host;
- `scripts/verify.sh` passes.
