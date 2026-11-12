---
title: tehas status, api, disk, top and analytics
description: "What the factory is doing, what it speaks to a program, how full the host is, and its runs."
---

# tehas status, api, disk, top and analytics

## tehas status

```
tehas status [--json]
```

```
factory   pass: Idle, taking work. (40 s ago)
poll      last 24 s wall, 3.1 s CPU, 12 gh (0 writes); last hour 14 polls, 41 s CPU, 2 writes
queue     2 in line, 0 building, 1 in review, 0 stopped (40 s ago)
cycle     #12 handoff at step review, 3 min ago
plan      #7 turn 2 running, started 12 min ago
slice     room for 2 cycles (4 GiB of 20 GiB promised, reserve 2g, 40 s ago)
platform  cb6912a, 19 commits behind origin/main since 2026-10-13, fetched 20 min ago
```

Prints the state of the factory from the [health file](/tools-health#tehasbot-health): is
it taking work, what is in the queue and how old that snapshot is, which
platform the host runs (the release `~/tehas/current` points at, or how far
the platform checkout is behind origin), the current or last cycle, each
planning turn in flight, and what the dispatcher's last poll read of `tehas.slice`. The `poll` line is
what the polls cost, read from the state store, and is there once a poll is on
record ([What a poll costs](/operating#what-a-poll-costs)). The `slice` line reads
`holding: ...` when that poll held cycles back, and `unknown: no poll has
reported in 10 min` when no poll read the slice in that time. While the factory
is paused it adds one line for each switch that is set:

```
pause     flag, written 4 min ago: waiting for the gate
pause     label: tehas:pause on the control issue
pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off
```

When the last snapshot kept a part from an earlier read, and while GitHub calls
failed in the last ten minutes (`checks.github`):

```
queue     stale: queue and completed kept from an earlier read, GitHub did not answer
github    4 failed calls in the last 10 min, the last 40 s ago: ERROR: gh api graphql failed: HTTP 502
```

While [refreshes](/operating#refreshes) are held on this host, with requests
waiting:

```
refresh   held: the pi-rukas in the sandbox image tehas/pi-rukas:uid1000 does not take --continue-pr
```

For each ready issue the last poll held on its own while it had room, such as
one whose last cycle parked on the body it still has:

```
held      #21: its last cycle parked on `intent-park` and the body has not changed since; edit the body, or tick Retry in the park comment
```

When every issue in line is held that way and no cycle runs, the factory line
is a `warn` that names them instead of `Idle, taking work.`:

```
factory   warn: #21 is held: its last cycle parked on `intent-park` and the body has not changed since. Edit the body, or tick Retry in the park comment. (40 s ago)
```

While [`tehas:blocked`](/operating#3-what-happens-next) holds issues by the
label alone, with no comment that names their blockers (up to 50 numbers):

```
blocked   by the label alone, no blockers named: #7, #9. Each waits until an allowed account comments a `<!-- tehas:blocked by=N -->` marker on it or a person takes tehas:blocked off
```

And while [`tehas:all-hands`](/operating#all-hands) holds:

```
all-hands tehas:all-hands is on the control issue, applied by an allowed account
```

On a host that runs a release (`~/tehas/current`), the `platform` line names
that release and the newest there is, instead of a commit:

```
platform  v0.9.0, newest v0.9.0, fetched 20 min ago
platform  v0.9.0, v0.9.1 is out, fetched 20 min ago
platform  v0.9.0, newest unknown, never fetched
platform  v0.10.0-rc.1, newest v0.9.0, the candidate is newer, fetched 20 min ago
```

A host on a release candidate keeps the newest release it last read, even
though it is older than the candidate.

It reads
that one file and nothing else: no GitHub call, no instance settings, and it
writes nothing. So it is as fresh as the dispatcher's last poll, and the first
line says how old that is.

A stopped dispatcher cannot report itself: its file just stops changing. A
file older than ten minutes is therefore not believed. The first line then
starts `factory   stale:` and asks whether the timer is running.

With `--json` it prints the same facts as one object for a program, with
the demand, the pause switches, the running cycles, what holds cycles back
and the room under each cap. [The factory API](/api#tehas-status---json) has
every field. Without a health file it prints nothing on standard output.

Exit 0 on `pass` and `warn`. Exit 1 on `fail`, on a stale file, or when there
is no health file yet. Exit 64 on any argument but `--json`.

## tehas api

```
tehas api [--json]
```

```
api         1
tehas       v0.8.0 (cb6912a)
repo        tehas-app/tehas
features    status millwright-grant fixer-grant all-hands limit drain history export refresh-conflicts analytics state-move mcp-dashboard-keys clean disk state-check finish
millwright  none
poll        last 24 s wall, 1.5 s CPU, 12 gh; last hour 14 polls, 41 s CPU
```

What this factory speaks: the API version, the release and commit it was
installed from, its repository, what this release can do, the millwright's
login and what the polls cost. A program asks it first. See [the factory
API](/api#tehas-api), where the fields of `poll` are written out. It reads the
instance settings, the version file `install.sh` writes and the state store, a
store that cannot answer being no poll on record; no network, no git, and it
writes nothing. Exit 0, 64 on an argument it does not know, 78 when the
settings are missing or malformed.

## tehas disk

```
tehas disk                 # one line per file system
tehas disk --json          # the same as one object
tehas disk --full          # and what fills them, biggest first
tehas disk --publish       # also write live/disk.json
```

```
disk      /home/boss/tehas: overlay, 20814 GiB of 28614 GiB free (27% used)
disk      /var/lib/docker: df answered nothing
disk      /tmp: tmpfs, 5.0 GiB of 6.0 GiB free (15% used)
          one file system also holds /tmp
fills     /home/boss/tehas/logs: 0.4 GiB
fills     /home/boss/tehas/dev/alpha.state/target: 0.1 GiB
fills     docker system df: read
```

How full this host is, and what fills it. One entry per file system the factory
cares about: this instance's home, the directory Docker keeps its images in as
Docker itself names it, `/tmp`, and `TEHAS_SCRATCH_DIR` when the instance sets
one. Two paths on one file system are one entry that names both, so the same
free bytes are not counted twice; the line of the second says so rather than
repeating the figures. A figure that cannot be read is no figure and the reason
under it, never 0, which is the rule the dispatcher's hold reads by: no figure
is never the same as plenty.

`--full` adds what fills them, biggest first: the entries of `~/tehas` and of
`~/.pi/agent`, each clone's `~/tehas/dev/<name>.state/target` in place of the
clone's own tree, what the scratch disk holds, and `docker system df`. Every outside call and every walk runs under a bound
(`TEHAS_DISK_WAIT`, the one the dispatcher's hold already reads the host's
space under, doubled for a walk) and a place that ran out of its bound
says which one rather than saying nothing. The command reads and removes
nothing: it is what a person looks at before deciding what `tehasbot clean` or
a hand full of `rm` calls should take.

`--publish` also writes the answer to `~/tehas/live/disk.json`, whole, through
a side file renamed into place, so a reader sees a whole answer or the last
one. It is the only form that writes anything. `tehas-clean.service` runs it
every hour after the clean, as a second `ExecStart` that calls the console
directly (`%h/tehas/libexec/console disk %h/tehas --publish`), so on a host that
runs the cleaner's timer the file is under an hour old; a person can run it at
any moment too. The clean's own step is prefixed with `-`, so a clean that
failed still leaves a fresh reading - a full disk is the case the clean is
likeliest to fail in - and a reading that could not be written fails the unit,
seen in `journalctl --user -u tehas-clean`. The
[`factory_disk`](/mcp-server#the-tools) tool of the MCP server reads that file
and so asks the host's disk nothing. The reading, the bounds and the printing
are the console's (`~/tehas/libexec/console disk`, `console/src/disk.rs`);
it reads the home, the scratch disk and the bound of the wait from the
instance's own file, so `bin/tehas` loads nothing for it and hands the
arguments straight over. See
[`tehas disk`](/api#tehas-disk) for every field of `--json`.

Exit 0 when it printed, and with the file written; 1 when the published file
could not be written, which leaves the last answer where it was; 64 on an
argument it does not know; 78 when the settings are missing or malformed or no
console stands beside the command, before any `df`, `docker` or walk.

## tehas top

```
tehas top                # one screen, redrawn every 2 seconds
tehas top --once         # one frame as plain text
tehas top --json         # the snapshot a frame is drawn from
tehas top --every 10     # redraw every 10 seconds
```

One screen for a person, in the manner of `top`: is the factory taking work,
what is in the queue and which issues are behind the counts, what the cycles are
doing, which units run, what the model mesh draws, and the tail of a cycle's
console. It draws again every 2 seconds unless `--every` says otherwise; `q` or
Ctrl-C ends it and leaves the terminal as it was. Without a terminal on standard
output, and with `--once`, it prints one frame as plain text: no escape codes.

It reads what the factory already writes under `~/tehas/live/` - the [health
file](/tools-health#tehasbot-health), the queue snapshot, the cycles, the energy sample and
the console of a cycle - asks `systemctl --user` about its units, and reads the
host's own numbers from `/proc`, through the console beside the commands
(`~/tehas/libexec/console top`, which also puts the snapshot together;
without it there is no snapshot and the command says to run `install.sh`). No
GitHub call, no model call, no Docker call,
no instance settings, and nothing is written. What it shows is as old as those
files, and every section that has an age prints it. A file older than ten
minutes is not believed, the same rule `tehas status` uses.

The frame is drawn from one snapshot and from nothing else. `--json` prints that
snapshot: `generatedAt`, one key per section (`factory`, `queue`, `cycle`,
`host`, `units`, `energy`, `console`) and `reading`, the raw reading of `/proc`
the host section was computed from. A section whose source is missing,
unreadable or too old to believe is `null`; the frame says so in one line and
draws the rest. The factory line and the queue counts are the ones `tehas status`
prints: both commands take them from the console (`console/src/status.rs`), so
they cannot drift apart.
A section added to the snapshot appears in `--json` without anything else
changing.

The host section compares two readings of `/proc`: the machine's cpu count, its
three load averages, what share of its cpus and of its memory is in use between
them, and up to fifteen processes with their pid, their short name from
`/proc/<pid>/stat`, their share of a cpu, their share of memory and how long they
have been running. A view that redraws hands the reading of one frame to the
next, so a redraw measures the seconds since that frame instead of waiting; only
a first frame, and `--json`, `--once` and `tehasbot top`, take a second reading a
second after the first. The reading a snapshot was computed from is in it under
`reading`, which is how it is handed on; no frame draws it and the published file
does not hold it. A process is named by that short name only: the command line,
and everything else under `/proc/<pid>/`, is never opened. A pid that was reused
between the two readings is not shown. The figures are what changed between the
readings, not the average since a process began. They sit between the cycle rows
and the units; when the terminal is short the process rows go first, before the
console lines.

A frame follows the width and height of the terminal, also after a resize. Long
lines are cut; nothing wraps and nothing scrolls. When the terminal is short,
host process rows are dropped first, then console lines, then unit rows, then
queue rows. Text that came out of a file has its control characters removed
before it reaches the terminal. `COLUMNS` and `LINES` say what the size is when
the terminal cannot be asked.

Exit 0 when a frame was drawn or a snapshot printed, whatever the factory says:
a `fail` or a stale health file is shown here, not returned - that is what
`tehas status` is for. Exit 1 when there is no `~/tehas/live/` directory at all.
Exit 64 on an unknown argument, or on a `--every` that is not a whole number
from 1 to 64.

## tehas analytics

```
tehas analytics report                    # the ten-day report, for a person
tehas analytics export <file> [--json]    # a copy of the database
```

The factory's own record of its last 365 days: runs, their steps and roles,
issue events and poll samples, in `~/tehas/state/analytics.db`. Both read it
through `~/tehas/libexec/analytics`, which holds all of its SQL.

`report` prints what the plain dashboard's "Last 10 days" shows, as text: one
line per figure with its count `n`, a table of the ten days, the five most
common stop reasons, the share of runs that got through each step, and one row
per tehas and pi-rukas version pair. A figure that has no value, such as a
yield of no shipped issues, is `-`.

```
Last 10 days, 2026-10-09 to 2026-10-18 (UTC)

Shipped            0 issues
Runs               1: 0 pr, 0 merged, 1 handoff, 0 parked, 0 failed, 0 refused, 0 stopped
Handoff rate       100%  (n=1)
Cycle time         median 30s, p90 30s  (n=1)
```

`export` writes a consistent copy of the database to `<file>` with SQLite's
`VACUUM INTO`, mode 600. It never replaces a file. With `--json` it then prints
what the copy holds, read from the copy:

```json
{"api":1,"schema":1,"from":1792022400,"to":1792800000,"runs":42}
```

`schema` is the copy's schema version, `from` and `to` the oldest start and the
newest end of its runs in Unix seconds, both left out when it has none, and
`runs` how many there are. It takes no lock a cycle holds and makes no network
call. See [the factory API](/api#tehas-analytics-export), and
[Analytics](/analytics) for what the database holds.

Exit 0; 64 on a bad argument; 69 when `TEHAS_ANALYTICS=off`, or the program is
not installed, which reads and writes nothing; 70 when the database cannot be
read; 73 when `<file>` exists or cannot be made; 78 when the settings are
missing or malformed.
