# Spec: ticket concurrency, several issues worked on at once

**Status**: Proposed 2026-10-10. Not implemented.

## Purpose

A host works on one issue at a time. While a cycle runs, which can take hours,
every other labelled issue waits, even when the host and the model backends
have capacity to spare. The goal is throughput: more issues finished per day
on the same host.

One setting, `TEHAS_TICKET_CONCURRENCY`, says how many issues a host works on
side by side. It is 1 by default, and at 1 a host behaves as it does today.

Still one host and one product repository. Each issue gets a cycle, a
container and a workspace of its own. Every dispatch rule stays: the pause
switch, who applied the label, urgent first, the cap on work awaiting merge.

## What holds it to one today

| Assumption | Where |
| --- | --- |
| The dispatcher holds `state/dispatch.lock` for the whole cycle | `libexec/dispatch` |
| The dispatch unit is a oneshot that lasts the whole cycle, and its timer fires again only once the unit is inactive | `host/systemd/tehas-dispatch.*` |
| One workspace, `$TEHAS_WORK_DIR`, removed and cloned again for every cycle | `libexec/work` |
| The code index is keyed by that one path | `libexec/index` |
| The console shows every agent transcript touched since the cycle began. A transcript's name carries a run and a role, not an issue | `libexec/work`, `console/src/tail.rs` |
| One `live/running.json`, written by the one cycle | `libexec/work`; read by `libexec/health`, `libexec/dev`, `host/docs/dashboard.frag`, `host/fun/game-view.js` |
| `tehas index` takes the dispatcher's lock to know that no cycle is running | `libexec/index` |
| `TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` default to nearly the whole host and are given to the one container | `lib/config.sh`, `libexec/work` |
| `TEHAS_MAX_AWAITING` counts finished work only, which is enough when nothing else is running | `libexec/dispatch` |

## What was decided, and what was not chosen

| Question | Decision | Not chosen, and why |
| --- | --- | --- |
| The control | One setting, `TEHAS_TICKET_CONCURRENCY`, default 1 | Always on: a host that has not asked for it must not change |
| The dispatcher's shape | The poll stays one short unit. Each cycle runs as its own unit, `tehas-cycle@<issue>.service` | N copies of today's dispatcher, each with a timer: `install.sh` would enable and disable timers as the setting changes, and every poll would repeat the GitHub calls N times. One long-running supervisor: it gives up the model of one deterministic poll that exits, which the tests and the manual are built on |
| How running cycles are counted | By the cycle locks that are held | By the `tehas:running` label: a label outlives a crashed cycle, a lock does not |
| CPU and memory | `TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` are the total. Each cycle gets the total divided by the setting | Limits per cycle, checked against the host: two settings to keep in step. Limits per cycle, unchecked: two cycles at the default memory limit use the whole host and processes are killed |
| The share when fewer cycles run | Fixed. A cycle running alone still gets its share | Rebalancing: a running container's limits would have to change under it |
| `TEHAS_CONCURRENCY` | Per issue, as today. Up to `TEHAS_TICKET_CONCURRENCY` times that many workstreams run at once | A total across issues: an existing setting would change meaning. A stated backend limit: one more setting, and the operator can already lower either of the two |
| The workspace | One per issue on the host, mounted at the same path inside every container | A different path per container: the code index is keyed by the path, so every cycle would need an index of its own |
| Transcripts | A runs directory per issue | Telling them apart by time, as today: two cycles would show each other's agents |
| The index while cycles run | Refreshed only when no cycle is running | Refreshed before every cycle: it would be rewritten under a cycle that is reading it |

## The setting

`TEHAS_TICKET_CONCURRENCY` is a whole number of 1 or more. It defaults to 1.
`lib/config.sh` validates it with the other counts, so a malformed value stops
every command with exit 78.

`lib/config.sh` also works out each cycle's share and exports it:

| Variable | Value |
| --- | --- |
| `TEHAS_CYCLE_CPUS` | `TEHAS_WORK_CPUS` divided by the setting, to two decimals, rounded down |
| `TEHAS_CYCLE_MEMORY` | `TEHAS_WORK_MEMORY` divided by the setting, in whole MiB, rounded down |

With the setting at 1 both are the totals as written.

A share below one CPU is refused when settings load: exit 78, with a message
that names `TEHAS_TICKET_CONCURRENCY` and `TEHAS_WORK_CPUS` and gives the share.
This also keeps the setting at or below the host's CPU count. Working the
share out runs no git command, makes no network call and writes nothing.

`tehas work` passes the share to `docker run`. `tehas index` keeps the totals:
it runs only when no cycle does. `tehas dev` is unchanged.

## The poll: `libexec/dispatch`

The command keeps its name, its timer, its arguments and its exit statuses. It
no longer runs a cycle. One poll:

1. Take `state/dispatch.lock`. It is held for the poll only. A poll that
   cannot take it exits 0, as today.
2. Refresh the status and check the repository's name, as today.
3. The pause switch, as today.
4. Count the running cycles: the files `state/cycles/<issue>.lock` whose lock
   is held.
5. Work out how many cycles to start: the smaller of
   - `TEHAS_TICKET_CONCURRENCY` minus the running cycles, and
   - `TEHAS_MAX_AWAITING` minus the issues awaiting merge minus the running
     cycles.

   At zero or below, the poll ends.
6. The mesh check, once, as today.
7. Walk the candidates in today's order, urgent first and then by number, with
   today's check of who applied the label and of later edits, until that many
   are found. An issue that carries `tehas:running` is not a candidate, as
   today.
8. If any were found: fast-forward the product checkout, once. If no cycle is
   running, refresh the index, once.
9. For each issue found, in order:
   - mark it `tehas:running` and remove `tehas:ready`. If that fails, the
     issue is not started and the poll ends with exit 1, as today,
   - post the start comment,
   - start `tehas-cycle@<issue>.service`,
   - wait up to 30 seconds until that cycle's lock is held. If it is not, the
     issue gets `needs-human-attention` in place of `tehas:running` and a
     comment that says the cycle did not start.

`--dry-run` says how many cycles are running, how many it would start and for
which issues, and changes nothing.

The unit is started with `systemctl --user start`. `TEHAS_CYCLE_START_CMD`
replaces that command, from the environment and for tests only, as
`TEHAS_WORK_CMD` does for `tehas work`.

A running cycle is never stopped to make room. An urgent issue takes the next
free slot.

The loop that refreshed the status every two minutes during a cycle is
removed. The poll now runs every two minutes whatever the cycles are doing,
and refreshes the status itself.

With the setting at 1 the poll starts nothing while a cycle runs, and the
labels and comments on an issue come in today's order.

## The cycle: `libexec/cycle`

New. `tehasbot cycle <issue>` runs one cycle and reports it; `bin/tehasbot`
gains the subcommand. It is the second half of today's `libexec/dispatch`,
with the same behaviour:

1. Take `state/cycles/<issue>.lock`, waiting up to 5 seconds. If it is still
   held, a cycle for this issue is running: exit 3.
2. Decide on `--restart` from `state/cycles/<issue>`, as today.
3. Run `tehas work <issue>`, with its output in
   `logs/dispatch-work-<issue>.log`.
4. Read the cycle's state file from the workspace, as data.
5. Find the pull request the bot opened from the cycle's branch.
6. Relabel and comment: merged, awaiting merge, or `needs-human-attention`.
7. Remove the workspace if the cycle ended merged or with a pull request.
   Otherwise leave it for diagnosis; the next cycle for that issue replaces it.
8. Refresh the status.

It checks no authorisation, as `tehas work` checks none. The poll did that
before it started the cycle. Its header comment says so.

Exit: 0 the cycle ran and was reported; 3 a cycle for this issue is already
running; 64 a bad argument; 78 instance settings are missing or malformed.

### The unit

`host/systemd/tehas-cycle@.service` is a user unit with one instance per
issue. It sources `env.sh` and runs `tehasbot cycle %i`, appending to
`logs/dispatch.log` as the dispatch unit does. It has no start timeout: a
cycle is bounded by `TEHAS_WORK_TIMEOUT` in `tehas work`.

```
systemctl --user status 'tehas-cycle@*'    # what is running
systemctl --user stop tehas-cycle@12       # end the cycle for issue 12
```

Stopping the unit sends `TERM`, which `tehas work` already handles: it stops
the container and records the cycle as interrupted. `libexec/cycle` passes the
signal on, waits, and then reports as for any other cycle that stopped without
a pull request.

`install.sh` installs the unit with the dispatch feature. Nothing is enabled:
an instance is started by the poll.

## What a cycle's container is given

This section changes mounts that `test/work_test.sh` pins. It is a security
change under hard rule 4. The pull request says so, and the test is changed to
pin the new values exactly, not relaxed.

| Mount | Today | After |
| --- | --- | --- |
| The workspace, at `$TEHAS_WORK_DIR` in the container | from `$TEHAS_WORK_DIR` on the host | from `$TEHAS_HOME/work/<name>-<issue>` on the host |
| `/home/vscode/.pi/agent/ensemble-runs` | from `~/.pi/agent/ensemble-runs` | from `~/.pi/agent/ensemble-runs/issue-<issue>` |

Nothing new is mounted and no target path changes. The second row narrows what
a cycle can read: today a container sees the transcripts of every earlier
cycle, afterwards its own. Still not given: the Docker socket, SSH keys,
`~/.config/tehas`, the product checkout.

`tehas_sandbox_mounts` in `lib/sandbox.sh` takes the host's workspace and the
runs directory as arguments. `tehas dev` passes the values it uses today, so
its mounts and `test/dev_test.sh` do not change.

The workspace:

- `tehas work` removes and clones `$TEHAS_HOME/work/<name>-<issue>` at its
  start, as it does the one workspace today. It touches no other issue's.
- The marker `$TEHAS_HOME/work/.tehas-workspaces` keeps its meaning: only a
  directory under a marked directory is ever removed.
- The preflight push uses the same mount as the cycle.
- `TEHAS_WORK_DIR` remains the path inside the container, and the value of
  `PI_ENSEMBLE_WORKSPACE_ROOT`. No directory of that name is left on the host.
  The single workspace of an earlier version is not removed by an upgrade.

The transcripts:

- The console is started with `--runs` on the issue's directory. Inside it
  the days and file names are as today, so the console's code does not change.
- The directories are never cleared, as today.

Shared between cycles, as they are today between the workstreams of one cycle
and with `tehas dev`: `~/.vipune`, Pi's `sessions`, and the two cache volumes.
This is accepted, not proven for several cycles.

## The index

- `tehas index` takes a new `state/index.lock` while it writes.
- Run by hand, it refuses while any cycle lock is held, with today's message
  and exit 3.
- The poll refreshes the index only when no cycle is running. Otherwise it
  logs that the index stays at the commit it has. The dispatcher already
  treats an index that is not fresh as a warning.
- On a host that is never idle the index falls behind the branch. The log
  line is how a person sees it.

## The live view

- `tehas work` writes `live/cycles/<issue>.json`, in the shape `running.json`
  has today.
- `live/running.json` is assembled from those files and replaced with `mv`.
  Its shape becomes:

  ```
  {"cycles": [<each running cycle>], "last": <the cycle that finished most recently, or null>}
  ```

- `tehas work` assembles it each time it writes its own file. Two cycles
  writing at once can leave it one tick old. The next tick, ten seconds later,
  corrects it.
- The file of a finished cycle that is not `last` is removed when the view is
  assembled.
- `libexec/health` applies its rule to `last`: the factory fails when the
  cycle that finished most recently failed. `health.json` keeps its format.
- The dashboard shows a line per running cycle and the console of the selected
  one. The game view reads `last`. `tehas dev` names every running issue.

## Failures

| What fails | What happens |
| --- | --- |
| A GitHub call during the poll | The poll ends with exit 1. Cycles already started carry on |
| Marking an issue `tehas:running` | The issue is not started. The poll ends with exit 1 |
| A cycle unit does not come up | `needs-human-attention`, with a comment |
| A cycle process dies | Its lock is released and the slot is free at the next poll. The issue keeps `tehas:running`, as it does today |
| An upgrade while the old dispatcher runs a cycle | The old dispatcher holds `state/dispatch.lock` to the end of its cycle. The new poll exits until then |

## Limits

- Two issues that touch the same files give two pull requests that conflict.
  tehas does not detect it. The manual says to label such issues one after
  the other.
- An issue left on `tehas:running` after a reboot or a killed cycle stays so
  until a person relabels it. This is true today and is not changed here.
- Up to `TEHAS_TICKET_CONCURRENCY` times `TEHAS_CONCURRENCY` workstreams are in
  flight. Nothing checks that against what the model backends can serve.
- One host. Several hosts on one repository are not part of this.

## Tests

Written before the code. Shell tests need bash, git, jq and shellcheck only.

| File | Cases |
| --- | --- |
| `test/config_test.sh` | The default; a malformed value; the split of CPUs and memory; totals passed as written at 1; a share below one CPU is exit 78 |
| `test/dispatch_test.sh` | Nothing started when the slots are full; the awaiting-merge cap counts running cycles; several issues started in order, urgent first; an unauthorised issue does not use a slot; the checkout is fast-forwarded once; the index is refreshed only when no cycle runs; a unit that does not come up; `--dry-run`; the lock is free after the poll |
| `test/cycle_test.sh`, new | The reporting cases moved from `dispatch_test.sh`: merged, awaiting merge, stopped without a pull request, the usage table; `--restart`; a second cycle for the same issue is exit 3; the workspace is removed or kept |
| `test/work_test.sh` | Every mount and environment variable, pinned; the workspace and runs directory of the issue; another issue's workspace is not touched; the share passed to Docker; the cycle's own live file and the assembled `running.json` |
| `test/index_test.sh` | The new lock; refusal while a cycle lock is held |
| `test/status_test.sh` | `health.json` from the new shape of `running.json` |
| `test/install_test.sh` | The cycle unit is installed with the dispatch feature |
| Front page tests | The dashboard with none, one and two running cycles |

Starting a unit is stubbed with `t_stub`, as `gh` and `docker` are.

## The manual

Changed in the commits that change the behaviour:

- `host/docs/host.md`: limits and concurrency, and the table of what a
  container is given.
- `host/docs/operating.md`: how issues are taken, and how to see and stop a
  cycle.
- `host/docs/tools.md`: the setting, `tehasbot cycle`, the note on
  `tehas index`.
- `host/docs/troubleshooting.md`: where transcripts are.
- `host/docs/llms.txt`, `host/docs/pi-software-factory.md`: "one at a time"
  becomes the setting's default.
- `host/docs/features.md`: a row for ticket concurrency. The default is
  covered by tests. A value above 1 is not yet observed on a host.

`instances/example/instance.env` gets the setting as a comment.
`instances/tehas-tehas/` is not touched.

## Commits

Each passes the gate on its own.

1. The setting and the split of the budget.
2. `libexec/dispatch` split into the poll and `libexec/cycle`, with the unit
   and the index lock. The index lock goes here because from this commit on
   the dispatcher's lock no longer says that a cycle is running. Still one at
   a time.
3. A workspace and a runs directory per issue. The security change.
4. The live view and the dashboard.
5. Slots: the poll starts up to `TEHAS_TICKET_CONCURRENCY` cycles.
