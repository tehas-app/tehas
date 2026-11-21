---
title: The commands that hold work back
description: "An outside cap, a drain, the records of what ended, and stopping a cycle by hand."
---

# The commands that hold work back

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
`--cycles` and `TEHAS_MAX_CYCLES`, less the cycles running, and its
hold is `limit`. With `--plan off` no new planning session starts; an answer to a planning
question still runs. One that expired or cannot be read holds every start, answers and
retry ticks included, while the poll goes on cleaning up. A `set` or `clear`
that lets more cycles run while the last poll's demand has an issue waiting
asks for a poll, as `tehas wake` does. See
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
end. Exit 0 when none is running, 3 when some still are or a poll held its
lock for the whole wait (`not drained:`, `drained: false` with `--json`). Each
exit 0 or 3 prints the answer. `tehas limit clear` ends it.

While the factory stands drained the dispatcher's hold line names the way out:
it ends `; tehas limit clear starts the factory`, after the words a limit of no
cycles at once has always said (`holding: the limit allows 0 cycles at once and
0 are running: draining`). The suffix comes only when nothing runs, planning is
off and the limit carries no time that has run out; a limit that merely holds
every slot, or one that expired, keeps its words. `tehas update` on such a
factory ends with the same command in a line of its own (see
[tehas update](/tools-update#from-a-release)).

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
unit is active. A cycle killed before it reported, by a power loss or the OOM
killer, leaves its record open with its lock free: `pack` refuses that too and
says `the cycle for #N was killed before it reported and its record is open:
tehas stop N ends it`. `--rehearse` does not refuse, changes nothing and marks the tar a
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
tehas pause                                   # pause this host
tehas pause --reason "waiting for the gate"   # and say why
```

Writes the pause flag, `~/tehas/state/pause.json`, holding who set it, when and
why, in a file with mode 600. While it is there the dispatcher starts nothing:
no cycle, no planning turn, no answer to a planning question, no retry by a
tick. A cycle already running finishes, and the poll still stops one whose issue
is closed or whose pull request is merged or closed, and still clears an issue
marked `tehas:running` with no cycle behind it. The flag is read without asking
GitHub anything, so a host can be paused when GitHub cannot be reached. Pausing a
paused host succeeds and keeps the first flag. The reason is the value of
`--reason`, kept to 200 characters, and it reaches pages that may be public. A
reason given without `--reason` is refused with 64 and names the flag.

The reading, the checks of the reason, the write and the words of the answer are
the console's (`~/tehas/libexec/console flags pause`); the command checks its
arguments, loads the settings, and stops with 78 without an
instance or a console. A flag is written through `pause.json.new`, mode 600, and
put in place by a rename, so what is there is always a whole flag.

Exit 0 when the host is paused, 64 on a bad argument, 78 when the
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
`.timed`, `.signal`, `.oom`, `.forks`, `.disk`, `.nostart`, `.lean-refused`,
`.refused`, `.refresh-conflict`, `.comments`, `.progress.json`, `.result.json`, `.freeze`,
`.frozen` and `.heads`, a `.nostart.new`, `.runner_exit.new` or `.status.new` whose
write never finished, `~/tehas/live/console-12.txt`, `~/.pi/agent/ensemble-runs/issue-12`, and the
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

## tehas outsource

```
tehas outsource invite [--name NAME] [--json]
tehas outsource list [--json]
tehas outsource status [--json]
tehas outsource revoke <contractor> [--json]
tehas outsource drain <contractor> [--json]
```

The contractors that run this factory's cycles: invite a host, list them,
count them, end one at once, or ask one to take no new job. Outsource is still
being built in this release, so every subcommand exits 78. See
[outsource](/outsource#the-commands).

## tehas plugin

```
tehas plugin [--json]
tehas plugin review <name> [<commit>] [--json]
tehas plugin run <name>
tehas plugin hook <name> <pr>
tehas plugin log <name> [--hook <pr>]
tehas plugin check <dir> [--json]
tehas plugin call <name> <tool> --args <json> [--login <login>] [--json]
```

The plugins of this factory, as its settings set them: each one's pin, what
is granted at the pin, its state: `unbuilt`, `building`, `ready` or
`refused` with why, as the last `tehas update` left it, its last run and the
model tokens its runs used today, from the state store, and why its hook's
last commit status could not be written, when it could not. With the feature on,
`tehas update` fetches each pin into `~/tehas/plugins/<name>/repo.git`,
checks its manifest, skills and Dockerfile, pulls its base images by digest
and builds its image with no network, then prunes the repository to the pin's
commit. A plugin that breaks a rule is refused and left off, and the update
exits 1 at the end. A plugin taken out of `TEHAS_PLUGINS` loses its images
and its directory.

`review` fetches a commit, the pin by default, and prints what it asks, what
each ask lets it do and whether it is granted, the base images and their
ONBUILD triggers, where the model runs (the provider of
`PI_ENSEMBLE_SUBAGENT_PROVIDER`, on this host or another), what changed since
the pin and the lines to write. It writes nothing but the fetched objects and
the base images.
Exit 0; 1 the fetch or the read failed; 3 refused, with why; 64; 78.

`run` asks for a refresh now and `hook` for the plugin's hook on an open pull
request of the bot's, at its current head. Each writes a request under
`~/tehas/plugins/<name>/` and starts a unit with `systemctl --user start
--no-block`, and waits for nothing: `tehas-plugin@<name>.service` for a
refresh, `tehas-plugin-hook@<name>-<pr>.service` for a hook. Exit 0 asked; 3
one is running, the plugin is not ready, or there is no such open bot pull
request; 1 GitHub or systemd failed; 64; 78. A plugin refused because its
`/state` passed the disk limit runs again only through `run`.

`log` prints the last run's log, at most 1 MiB, as the plugin wrote it: the
last refresh's, or with `--hook` the last hook's on that pull request.
Exit 0; 3 none; 64; 78.

A run is one container of the plugin's image, started by `tehas-plugin`:
the arguments of [the security part of the
spec](https://github.com/tehas-app/tehas/blob/main/docs/specs/2026-11-21-plugins-security.md),
a row in `plugin_runs`, a clone of the product's mirror at the run's head
when `product` is granted, the model socket when `model` is (a refresh only:
a hook gets no model in v1), `docker kill` at
the manifest's timeout, then `/out` read through the tar reader: a refresh's
view swapped into `~/tehas/plugins/<name>/view/` with one rename, a hook's
`/out/_tehas/verdict.json` (`api/plugin-verdict.schema.json`) kept as
`hooks/<pr>.verdict.json` until the tick removes it. The volumes are measured against `disk`, and the
container, its `/work` and `/out` and the run's files are removed. The row
ends `done`, `failed`, `timeout` or `lost`, with the model's tokens. A hook
waits for a refresh of the same plugin to end, at most one hook of a plugin
and two in all run at once, and a newer request for the same pull request
runs after the one that runs.

A cycle asks for the hooks itself. After its report, a cycle that was not
stopped by a signal, was not refused at its start and left an open pull
request of the bot's, a refresh's too, reads that pull request from GitHub.
For each plugin granted `hook:after:work` and ready at its pin it writes
`~/tehas/plugins/<name>/hooks/<pr>.json` (the pull request, its head as
GitHub gave it, its base and the issue) and starts
`tehas-plugin-hook@<name>-<pr>.service`. It waits for none of them. A hook
holds no slot, lock or label. A merged or closed pull request gets no hook.

When a hook ends, `tehas-plugin` asks GitHub for the pull request's head and
posts one commit status on the head the hook read, and nothing else, never
a comment: context `tehas/plugin/<name>`; `success` for a `pass` or `note`
verdict; `error` with "the plugin's hook did not finish" for a hook that
failed, ran out of time or left a verdict that breaks its schema. The
description is the verdict's summary, cleaned as a question comment is
(check boxes, `<!--` markers and `@` mentions disarmed, secret-looking text
masked) and cut at 140 characters. With `view` granted the status links to
`https://plugin-<name>.<TEHAS_DOMAIN>/_hooks/<pr>`, or the one-level name
when `TEHAS_EDGE_ONE_LEVEL` is on, where the gate shows the findings,
which nothing installs yet. A post that fails is tried once more. GitHub
refuses a status with 403 when the App lacks "Commit statuses: Read and
write" ([the App's permissions](/github-app)): the refusal is kept in
`~/tehas/plugins/<name>/status-unwritten`, and `tehas plugin` says "the
status could not be written" (`status_unwritten` in `--json`) until a status
is written. A head that moved while the hook ran gets the status on the
commit the hook read, then a new request for the new head, which runs next.

`call` runs one call of a tool of a ready plugin, granted `tool:<id>` at
the pin, and prints its answer, one JSON object, or `{"api":1,"answer":...}`
with `--json`. The arguments are checked against the tool's `params` and
are at most 2 KiB. The image runs attached by the arguments of
`test/fixtures/argv/plugin-call`: no network, no product, no model,
`/state` read only, the arguments at `/run/call.json`, no log kept, killed
at the tool's timeout. Its stdout is the answer, at most 16 KiB, and its
stderr is dropped. At most two calls of a plugin run at once and one of
each caller; one past that is refused as busy and leaves no row. The row,
kind `call`, keeps the time, the result and the answer's size, and no
login, and the run's files are removed. A call with `--login` is refused
until the gate's access check is in core. `tehas plugin` leaves calls out
of its last run. Exit 0 answered; 1 the tool did not answer, with why (in
`--json` as `{"api":1,"error":"..."}`); 3 refused, with why; 64 arguments
the tool's `params` refuse; 78.

`tehas-plugin.timer` runs `tehasbot plugin tick` every 10 minutes. It reads
the head the poll fetched into the product's mirror, asks GitHub nothing,
and asks for a refresh of each ready plugin whose last refresh began at
another head. It starts nothing while the factory is paused by
`state/pause.json`, drained, read-only, or while `tehas update` runs, and
says why in `~/tehas/logs/plugins.log`, where the runs write too. It removes
the containers, volumes and files a run that died left, and ends its row as
`lost`, but not while a call of the plugin runs. It removes a hook's `hooks/<pr>.verdict.json`, and that pull
request's request unless its hook runs, when the verdict's head is in the
default branch of the mirror (merged), or when the verdict is older than 30
days (closed or left). Each removal is logged with why. `install.sh` installs `tehas-plugin`, the two templates, the timer
and its service only while the feature is on, and a first install enables
the timer.

`check` reads a plugin's folder on this machine as `review` reads a commit:
the manifest, its skills' text and the Dockerfile's rules, each ask known and
not reserved. It needs no instance settings, runs no git or docker and
reaches no network, so a plugin's author runs it anywhere tehas unpacks, with
the feature off too. It reads only the folder's regular files, through no
link. The bases' ONBUILD triggers need a pull, so `check` leaves them to
`review` and `tehas update` and says so. Exit 0 the folder keeps the rules;
3 refused, with why, a folder with no manifest among them; 64.

Plugins are still being built in this release, so every subcommand but
`check` exits 78. See the
`TEHAS_PLUGIN` rows of
[the settings table](/tools#instance-settings) and [what the feature leaves
visible](/contract#plugins).
