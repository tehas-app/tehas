---
title: tehas limit, drain, history, export, state, pause, resume and stop
description: "The commands that hold work back, move the state store, and end a cycle."
---

## tehas limit

```
tehas limit [--json]
tehas limit set --cycles N [--plan on|off] [--for SECONDS | --until TIME] [--reason TEXT]
tehas limit clear
```

```
limit     1 cycle at once, planning off, until 2026-10-17T14:05:00Z: held for alpha
```

Sets, shows or clears the limit in `~/tehas/state/limit.json` (mode 600). The
dispatcher reads it on every poll: room for new cycles is the smaller of
`--cycles` and `TEHAS_TICKET_CONCURRENCY`, less the cycles running, and its
hold is `limit`. With `--plan off` no new planning session starts, and
[tehasbot improve](/tools-protolab#tehasbot-improve) waits; an answer to a planning question
still runs. One that expired or cannot be read holds every start, answers and
retry ticks included, while the poll goes on cleaning up. See
[Limit and drain](/operating#limit-and-drain) and
[the factory API](/api#tehas-limit) for the JSON. The reading, the checks of an
argument that reaches it, the write and the words of the answer are the
console's (`~/tehas/libexec/console flags limit`); the command checks the shape
of its arguments, loads the settings, and stops with 78 without an instance or
1 when the file could not be written. Exit 0; 64 on a bad
argument, which writes nothing; 78 without an instance.

## tehas drain

```
tehas drain [--wait SECONDS] [--json]
```

Sets the limit `--cycles 0 --plan off --reason draining`, which does not
expire, then with `--wait` waits up to that long for the running cycles to
end. Exit 0 when none is running, 1 when some still are or a poll held its
lock for the whole wait (`not drained:`, `drained: false` with `--json`). Each
exit 0 or 1 prints the answer. `tehas limit clear` ends it.

## tehas history

```
tehas history [--json] [--after CURSOR] [--issue N]
```

What has ended, oldest first: one line for each cycle and each planning turn,
in the order the state store wrote them, with `kind` telling them apart. It reads both from the state store
(`history-show`), so it needs no network, takes no lock a running cycle holds, and writes
nothing. A line it cannot read is left out, not an error. A store that does not
answer prints nothing, never a listing without the cycles: exit 1 when it was
busy, 78 when it is not installed or cannot be used.

`store prune` removes the history of an issue whose cycle ended more than its
window ago, keeping that issue's last row of each of the two tables, so a
`tehas history` of a long-closed issue gives one line of it and `--after` still
reads the lines it has not seen. `store upkeep` runs it with the window
`tehasbot clean` uses; see [Backups and restore](/operating#backups-and-restore).

`--after` takes the `next` the command printed before, so a program that keeps
it reads each line once. `--issue` takes one issue's lines, its cycle and its
planning turns together. Every field and the line it comes from are in
[the factory API](/api#tehas-history).

## tehas export

```
tehas export [--json] --after CURSOR <dir>
```

Copies what a person reading a cycle needs into `<dir>`: the cycle's result,
its logs, its agents' transcripts and its Protolab page, each cycle in a
directory of its own named by its cursor. `--after` is required, so a program
that keeps the cursor it is given copies each cycle once.

Every text file goes through the console's mask, so a token or a private address
that reached a log does not reach the copy; only a file the mask itself will not
read is copied as it is, and one over 32 MiB is left out. The mask catches
accidents and cannot stop what no pattern knows, so an export is still a copy of
what a cycle wrote. An export is never written under the factory's home, not
even through a link. The dispatcher's sweep removes
`~/.pi/agent/ensemble-runs/issue-<issue>` with the issue's workspace, so an
export of an issue the sweep has taken has no transcripts: its records, logs and
Protolab page are still copied. See
[the factory API](/api#tehas-export).

## tehas state

```
tehas state pack [--rehearse] -
tehas state unpack [--rehearse] -
```

Moves a stopped factory's state to another machine as one tar: `pack` writes it
to standard output and `unpack` reads it from standard input. It holds
`state/`, `repos/`, `live/` and `mcp/` of the home and the store as a copy the
store made of itself, behind a `tehas-state.json` that names the release and
the schema; the live token (`state/github`) is left out, and the target keeps
its own. `pack` refuses while a cycle or a planning turn runs or a `tehas-*`
unit is active; `--rehearse` does not, changes nothing and marks the tar a
rehearsal that `unpack` takes only with `--rehearse`, which replaces the
target's state as `unpack` does: the old state is gone, so use it only on a
machine that will be rebuilt. `unpack` refuses while a
unit is active, another release or schema, a bad manifest, a link that is
absolute, leaves its directory or has a `..` after a name, a device, pipe or
socket, and a setuid, setgid or world-writable entry. It replaces the
four directories only after it has checked the tar and the store in a
directory beside the home, and starts no unit.

`pack` sends the tar only after tar has ended well, so a `pack` that fails sends
nothing and `unpack` replaces nothing. Run the pipe with `set -o pipefail`; the
source's files are untouched by a failed run, so run it again; its units stay
down until you start them (see
[Backups and restore](/operating#backups-and-restore)). `unpack` needs room
beside the home for the archive and the extracted tree. After `unpack`, restart
the docs container, because `live/` is a bind mount:
`. ~/tehas/env.sh && docker compose -f ~/tehas/edge/docs.compose.yaml up -d --force-recreate`. See
[the factory API](/api#tehas-state).

## tehas pause

```
tehas pause              # pause this host
tehas pause waiting for the gate   # and say why
```

Writes the pause flag, `~/tehas/state/pause.json`, holding who set it, when and
why, in a file with mode 600. While it is there the dispatcher starts nothing:
no cycle, no planning turn, no answer to a planning question, no retry by a
tick. A cycle already running finishes, and the poll still stops one whose issue
is closed or whose pull request is merged or closed, and still clears an issue
marked `tehas:running` with no cycle behind it. The flag is read without asking
GitHub anything, so a host can be paused when GitHub cannot be reached. Pausing a
paused host succeeds and keeps the first flag. The reason is one argument, kept
to 200 characters, and it reaches pages that may be public.

The reading, the checks of the reason, the write and the words of the answer are
the console's (`~/tehas/libexec/console flags pause`); the command checks the
number of its arguments, loads the settings, and stops with 78 without an
instance or a console. A flag is written through `pause.json.new`, mode 600, and
put in place by a rename, so what is there is always a whole flag.

Exit 0 when the host is paused, 64 on more than one argument, 78 when the
instance settings are missing or malformed, which writes nothing, 1 when the flag
could not be written, a directory in its way among them.

## tehas resume

```
tehas resume     # remove this host's pause flag
```

Removes the flag, and succeeds when there is none; the name the flag is written
through goes with it, so a leftover of an interrupted `tehas pause` cannot
outlive the flag. A name of the flag is taken away only when it holds a regular
file of this account, so a symlink left on one of them is reported and stays
where it is rather than being followed.
Removing it is the console's (`console flags pause clear`);
what is left here is the other switch, which only `gh` can read: `tehas:pause` on
the control issue. Only an account in
`TEHAS_ALLOWED_LABELERS` lifts that label, so when it is gone and its most recent
`tehas:pause` event is a removal by anyone else, or is an application no event
took back, this says so and that the factory stays paused. It never changes a
label.

Exit 0 when the flag is gone, whatever the label says, 64 on any argument, 78
when the instance settings are missing or malformed, 1 when a name of the flag
could not be removed, or is not this account's to remove.

## tehas stop

```
tehas stop 12                      # end the cycle for issue 12, take its workspace too
tehas stop 12 --keep-workspace     # end it and keep the workspace to read
```

Ends a cycle by hand and clears what it left on this host. It asks
`tehas-cycle@12.service` to stop, waits up to 10 seconds for the cycle to let go
of its lock, and removes the container `tehas-work-12` when it is still there.
Then it removes what says a cycle runs, owes something, or is parked:
`~/tehas/state/cycles/12.lock` with `.cid`, `.done`, `.stalled`,
`.timed`, `.signal`, `.oom`, `.forks`, `.nostart`, `.lean-refused`, `.refresh-conflict`, `.progress.json` and
`.result.json`, `~/tehas/live/console-12.txt`, `~/.pi/agent/ensemble-runs/issue-12`, and the
workspace `~/tehas/work/<name>-12` with its `.state`, `.before-retry` and
`.images` beside it. `--keep-workspace` keeps the workspace for you to read.

Before it asks the unit to stop, it writes `state/cycles/12.stopped`: the time
and the account that ran it. The cycle's record then says "A person stopped it
with `tehas stop` (<account>, at <UTC time>)", and its status stays
`interrupted`. When the file cannot be written, the stop goes through and says
so on stderr.

A cycle killed outright never reports, so its record in the state store stays
open. Once the unit is gone and the lock is free, `tehas stop` ends that record
as stopped, before the lock file goes, and says `ended the record of the cycle
of #12 as stopped`. An open record counts as something to stop. A record that
cannot be read or ended leaves the lock file and everything else where it is,
is said on stderr, and exits 1: run it again. A state store that is not
installed changes nothing, exit 78.

The park of issue 12, on the state store, then asks for nothing more, before
the lock file goes: its question is taken off the record (`took the park
question of #12 off the record`), so a box ticked in it starts nothing, and its
one retry is spent (`spent the retry of the park of #12`), so the dispatcher
does not start the issue again on its own after a provider outage. A park that
still has a question or an unspent retry counts as something to stop. A park
that cannot be read or cleared leaves everything where it is and exits 1, or 78
when the store cannot be used.

It keeps the cycle's record and every log: its record, history, park and
approval in the state store, `state/cycles/12.json`, `.manifest.json`, `.runner_exit` and
`.status`, and everything under `~/tehas/logs/`. A cycle stopped by hand did not
end well, and nothing here records its pi-rukas as proven.

It prints one line for each thing it stopped or removed, and
`nothing to stop for #12` when there is nothing to do, so running it twice is
safe. It names only paths for the number it was given: stopping 12 touches
nothing of 120. Nothing is removed while a cycle holds that issue's lock, and a
cycle that still holds it after the stop is left whole and said.

Its last step is the issue's own. When it stopped or removed something on this
host, and nothing of that failed, it reads the labels of issue 12
(`gh issue view 12 --repo <repo> --json labels --jq '.labels[].name'`) and takes
`tehas:running` off when it is there and no cycle of issue 12 holds its lock
again (`gh issue edit 12 --repo <repo> --remove-label tehas:running`), said as
`removed tehas:running from #12`. It adds no other label and writes no comment.
A run that found nothing to do here makes no GitHub call, and a stop whose host
part failed leaves the label where it is: while a container still works on with
the bot's token, `tehas:running` is what the poll needs to park the issue. Each
call gets 10 seconds. A label it cannot read is said as `WARN: the labels of #12
could not be read, so tehas:running stays on the issue`, one it cannot take off
as `WARN: tehas:running could not be taken off #12`, and a cycle that took this
issue's lock again while the host was cleared as `WARN: a cycle for #12 holds
its lock again, so tehas:running stays on the issue`. For the first two the
host is already cleared, and a second run finds nothing to clear here and asks
GitHub for nothing: the label then goes by hand, or the poll parks the issue.
For the third there is a cycle of issue 12 on this host, and that cycle owns
`tehas:running`: it takes the label off when it ends, and taking it off by hand
while it runs takes the label out from under it. Running this again ends that
cycle too, as stopping any other cycle does.

Exit 0 when the leftovers are gone or there was nothing to do, 1 when the cycle
still holds its lock after the stop, something could not be removed,
`tehas:running` could not be read or taken off the issue, or a cycle of that
issue took its lock again before the label was read, or the cycle's record
could not be read or ended, 64 with no issue number, an issue that is not a
positive number, or an argument it does not know, 78 when the instance
settings are missing or malformed or the state store is not installed or
cannot be used, which changes nothing.
