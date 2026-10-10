# Two commands: tehas and tehasbot

**Status**: implemented on the branch `consolidate-cli`, 2026-10-10.

## Why

`~/tehas/bin` held nine files, all on the login shell's `PATH`, and no command
called `tehas`. Three of them are typed by a person. The other six are run by
a timer, a service or another command, print nothing, and were still offered
by tab completion beside the three.

What was found:

| File | Run by | Typed by a person |
| --- | --- | --- |
| `tehas-dispatch` | the timer | yes: `--dry-run`, `--init-labels`, a poll by hand |
| `tehas-work` | `tehas-dispatch` | yes: a cycle by hand, `--restart` |
| `tehas-index` | `install.sh`, `tehas-dispatch` | yes: after the image is built |
| `tehas-status` | `tehas-dispatch` | no |
| `tehas-health` | `tehas-status` | no |
| `tehas-energy` | the service | no |
| `tehas-protolab` | the timer | rarely |
| `tehas-console.ts` | `tehas-work`, through deno | never, it is not executable |
| `tehas-test` | `scripts/verify.sh`, `install.sh --check` | only in a checkout |

- A command found its sibling in three ways: by name on `PATH`, by
  `$TEHAS_HOME/bin/<name>`, and by `%h/tehas/bin/<name>` in a unit.
- `--init-labels` was a setup step filed as a mode of the dispatcher.
- Nothing printed the state of the factory. `tehas-status` writes a file.
- `tehas-test` was installed on a host, where there is no `test/` to run.

## What changes

Two commands on `PATH`. Everything else moves to `libexec/`, one script per
unit as before, so each keeps its own test file.

```
tehas dispatch [--dry-run]       one poll of the queue by hand
tehas work <issue> [--restart]   one cycle by hand
tehas index                      build the code index
tehas labels                     create the tehas labels in the repository
tehas status                     print the factory's state
tehas help [command]

tehasbot dispatch | status | health | energy [--every] | protolab
```

`tehas` is what a person types and the only command the manual teaches.
`tehasbot` is what the units run. Both only choose a script in `libexec/` and
`exec` it: they load no settings, so they add no way for a command to run
without them.

| Was | Is |
| --- | --- |
| `bin/tehas-dispatch` | `libexec/dispatch` |
| `tehas-dispatch --init-labels` | `libexec/labels` |
| `bin/tehas-work` | `libexec/work` |
| `bin/tehas-index` | `libexec/index` |
| `bin/tehas-status` | `libexec/status` |
| `bin/tehas-health` | `libexec/health` |
| `bin/tehas-energy` | `libexec/energy` |
| `bin/tehas-protolab` | `libexec/protolab` |
| `bin/tehas-console.ts` | `libexec/console.ts` |
| `bin/tehas-test` | `scripts/test.sh`, not installed |
| none | `libexec/summary`, behind `tehas status` |

A script in `libexec/` calls a sibling by the path of its own directory, never
by name on `PATH`. The dispatcher keeps `TEHAS_WORK_CMD`, `TEHAS_STATUS_CMD` and
`TEHAS_INDEX_CMD` for its tests.

## What does not change

- The names of the units, the logs, the lock and the containers
  (`tehas-work-<issue>`, `tehas-index`). An enabled timer stays enabled and the
  `docker ps` filters in the manual still match.
- What a cycle's container and the indexing container are given.
  `test/work_test.sh` and `test/index_test.sh` change only the path of the
  script under test and the name in its messages.
- The exit statuses of every script.
- `--before-cycle` and `--every`. They are what the dispatcher and the service
  pass, and no person is told to type them.

## Moving a host

`install.sh` installs the new files, replaces the units and removes the nine
old files from `~/tehas/bin`. A cycle that began before the install was started
by the old dispatcher, which still calls `tehas-status`, `tehas-index` and
`tehas-work` by name. So the install removes the old files only while it holds
the dispatcher's lock itself, after systemd has the new units. When the lock is
taken they are left in place and the install prints the step to run later. On
a host where one cycle follows another that step may have to wait for a gap.
`--check` reports them as stale.

`tehas status` does not believe a health file older than ten minutes: a
stopped dispatcher cannot report itself. A running energy sampler is the old file calling itself every
15 seconds, so when that file is removed the install prints the restart.

`tehas labels` exits 1 when a label could not be made. `--init-labels` exited
0 whatever GitHub answered.

## Gate files

Two files that a work cycle may not edit change here, by a person's hand:
`scripts/verify.sh` (the two paths it runs) and `AGENTS.md` (the map of the
repository and the path in rule 4). Nothing under `.pi/` changes.

## Later

Two open pull requests add commands in the old shape, `tehas-dev` and
`tehas-update`. Rebased on this they become `libexec/dev` and `libexec/update`
with one line each in `bin/tehas`.

## Plan

1. `scripts/test.sh` replaces `bin/tehas-test`; it also checks `libexec/`.
2. Move the scripts to `libexec/`, add `bin/tehas` and `bin/tehasbot`, point
   the units and `install.sh` at them, retire the old files on a host, split
   out `libexec/labels` and add `libexec/summary`. Tests and the manual in the
   same commit, because the two commands name every script from the start.

## Not done here

- The comments in `host/edge/*.Caddyfile` and `protolab.compose.yaml` still
  say `tehas-health` and `tehas-protolab`. Changing them would make
  `install.sh` ask every host to recreate its two containers for a comment.
  They go with the next real change to those files.
- Nothing was installed on a host. The tests and a scratch install are the
  evidence; `host/docs/features.md` says so.
