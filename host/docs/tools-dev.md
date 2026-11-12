---
title: tehas dev and tehas index
description: "An interactive session on the host, and the code index the agents search."
---

# tehas dev and tehas index

## tehas dev

An interactive Pi session with pi-rukas, in the sandbox image, in a clone of
the product that is kept. For working by hand; see
[Operating](/operating).

```
tehas dev [<name>] [--author "Name <email>"] [--issue <number>] [-- <pi arguments>]
tehas dev --help
```

`<name>` names the clone, `~/tehas/dev/<name>`: letters, digits, `_` and `-`,
`main` when left out. It is made on first use from the product checkout and
never reset or removed. `--author` is stored in `~/tehas/dev/<name>.author` and
becomes git's name and email in every session of that clone; without one the
bot's are used. What follows `--` is passed to `pi`.

`--issue <number>` gives the session the images and text files that issue
links, with `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, by the rules of
[tehas work](/tools-work#tehas-work) without an approval record. The person running the
command approved nothing, so the images are taken when an allowed account
applied the issue's last `tehas:ready`, or else when an owner, member or
collaborator opened it. They go to `~/tehas/dev/<name>.images`, remade at
every start with `--issue`, and the container gets that directory read-only
at the same path and `PI_ENSEMBLE_EXTRA_IMAGE_DIRS`, which names it. What was
given and left out is printed; nothing is said on the issue. Without
`--issue` the session gets neither, and an instance's
`PI_ENSEMBLE_EXTRA_IMAGE_DIRS` is not passed on. A slice that cannot be used, a
clone's directory that is not a clone or a stored author that is malformed
stops the command before GitHub is called and before anything is written.

| Exit | Meaning                                                                                   |
| ---- | ----------------------------------------------------------------------------------------- |
| any  | The session's own exit status, as Docker reports it                                       |
| 4    | With `--issue`: an image or file the issue links could not be fetched, or the issue or its label events could not be read; no container |
| 64   | A wrong argument, a malformed author, no bot token or no git identity; no container       |
| 78   | The instance settings are invalid, the clone's directory is not a clone, another session of that clone holds its lock and none is given within five minutes, the clone's cargo target directory cannot be made or is not a directory of the user the session runs as, `tehas.slice` cannot be used, or `--issue` was given and the instance gives no images or files; no container |

Arguments are checked before the settings are read, so `--help` works on a
host with no instance and a mistyped argument makes no clone.

The container is the one `tehas work` starts, with a terminal: the same image,
the same limits in `tehas.slice`, mounts, token and settings, and a cargo target
directory of its own besides (see below). It is named
`tehas-dev-<name>`, so a clone has one session at a time. Before the clone is
made or the container started, the command takes the lock
`~/tehas/state/dev/<name>.lock` and holds it for the whole session: a second
`tehas dev` of the same clone waits for it up to five minutes and then says who
holds it, and `tehasbot clean` asks the same lock and takes nothing of a clone
whose session is open. Before it starts, the
command prints how many more cycles the slice has room for, because the session
takes room from them. The command does not take the dispatcher's lock and
writes nothing under `~/tehas/live`. It needs a terminal; Docker refuses to
start it without one.

After the clone is made the host runs no git command in it, because agents
have written there since.

The session's own memory, Pi sessions, transcripts, code index and cargo build
output are in `~/tehas/dev/<name>.state`, beside the clone and kept with it: a
person resumes their sessions and keeps the index their session built, and no
cycle shares either. The host's index of the product is not copied there,
because it is keyed to the path a cycle works at, which is not the path of this
clone. A session that ran before the state directory existed has its history in
`~/.pi/agent/sessions`, which no container is given now: the next session
begins a new history in `<name>.state/sessions`.

`<name>.state/target` is the session's cargo target directory: the container is
given it read-write at the path `TARGET_DIR` names in
`console/core/src/sandbox.rs`, and `CARGO_TARGET_DIR` names that path. Only the build output
is kept for a session. A cycle's is in its own container's layer and is gone
with the container, and so is cargo's crate registry, which stays in the image's
layer on purpose: a registry kept between cycles would be code one cycle leaves
for the next one's gate (see `host/sandbox/Dockerfile`). A cycle's gate therefore
builds from nothing and fetches the locked crates again; the gate of the next
session of a clone builds on what the last one built. The path inside the
container is named by that function, not a setting, as the path of the code
index's copy is.

The command makes the directory before the container starts and refuses to start
a session when it is not a directory of the user the session runs as. The
cleaner takes it when the disk is under `TEHAS_CLEAN_MIN_FREE` and it has not
been written to for `TEHAS_CLEAN_AFTER` hours, of a clone that no session holds:
see [clean](/tools-clean#tehasbot-clean). Nothing else under `<name>.state` is ever
taken, so the clone, its memory, sessions, transcripts, code index, author and
images stay whatever the disk does:

```
du -sh ~/tehas/dev/*.state/target               # what the sessions have built
rm -rf ~/tehas/dev/<name>.state/target          # clear one clone's build by hand
tehasbot clean                                  # or let the cleaner decide
```

Every `tehasbot clean`, and `--dry-run`, logs its size, which is how an operator
sees it before the disk fills.

### From another account

```
tehas dev [-- <pi arguments>]
```

An account in the group `tehas` types the same words and gets a session in
the clone named after its login, with `user.name` and `user.email` from its
own git configuration as the author. It takes no name and no `--author`.

That `tehas` is another file: `/usr/local/bin/tehas`, installed for everyone by
`sudo ./bootstrap.sh dev`. It reads the account's git identity and asks `sudo`
to run `~/tehas/libexec/dev-login` as the tehas user, which is the one thing
the group's `sudo` rule allows. `dev-login` takes the clone's name from the
account `sudo` was asked by, loads the tehas user's environment and starts the
session above. Any other command typed there is refused with exit 64.

It is a copy of `host/login/tehas`, and only root installs it, so an update of
the platform can leave those accounts typing an older command. No tehas command
replaces it: `install.sh --check` reports the difference as `Login: NOT CURRENT`
and a real install prints `later: sudo ~/tehas/platform/bootstrap.sh dev boss`. Run that command
again after an update that changed `host/login/tehas`.

| Exit | Meaning                                                                                   |
| ---- | ----------------------------------------------------------------------------------------- |
| any  | The session's own exit status                                                             |
| 64   | A wrong argument, or the account has no `user.name` and `user.email`; nothing was started |
| 78   | The host was not set up for it: run `sudo ./bootstrap.sh dev`                             |

## tehas index

Builds the code index that a cycle's agents search, or refreshes it.

```
tehas index                  # build the index
```

pi-rukas tells its agents to find code with the `codebase_memory` tools. They
answer from an index, and nothing builds one on its own: interactive pi-rukas
does it in `/start`, which a headless cycle never runs. Without an index every
such search fails and the agents fall back to reading files.

The index is kept in `~/tehas/state/index`, the host's own directory, under
the name of the tree it was made from. No cycle, planning or manual container
is given that directory.
`tehas work` and `tehas plan` copy it into the cache directory of the container
they start, so every cycle reads the host's index and no cycle changes it for
the others. This command runs the indexer in the sandbox image, over a clone
mounted at the path a cycle works in, so what it writes is the index a cycle
looks for. The clone is made at the tip the product checkout last fetched and
is removed afterwards. The cycle's workspace is not touched.

One run at a time: it holds `~/tehas/state/index.lock` while it runs, and a
poll starts no cycle while that lock is held. It refuses, with exit 3, while a
cycle is running or another index run holds that lock. Before it looks at the
cycles it waits, up to a minute, for a poll that holds the dispatcher's lock,
because that poll may be starting a cycle; then it lets that lock go. The index
is only ever written when nothing is reading it, and no poll waits for it.

| Exit | Meaning                                                |
| ---- | ------------------------------------------------------ |
| 0    | The index was built, or was already current            |
| 1    | The sandbox image is missing, the indexer failed or did not finish in 30 minutes, or what another sandbox image wrote could not be set aside |
| 3    | A cycle is running, another index run holds its lock, or a poll held the dispatcher's lock for a minute; nothing was done |
| 64   | An argument this command does not know                 |
| 78   | The instance settings are invalid, or `tehas.slice` cannot be used; nothing was done |

The container gets the clone, read-only, and `~/tehas/state/index`, which is
the only thing it may write: no network, no token and none of the settings. It
runs in `tehas.slice` with a cycle's limits. The indexer's answer is kept in
`~/tehas/logs/index-<time>.log`, and `~/tehas/state/index.json` records the
commit that was indexed and the id of the sandbox image that built it. A run
that finds a different id there, or none, sets aside what that image wrote and
builds from scratch, and says which two images it compared; the same image
leaves the index where it is for the indexer to update. What it set aside waits
in `~/tehas/tmp`: it goes back when the run wrote no index, and is dropped when
the run wrote one, so a rebuild that fails still leaves the cycles an index to
search and never buries the one a run just wrote. A run that was killed leaves
its set-aside there, and the next run puts those files back when the index
holds no database, or removes them when it does.

A cycle is given a copy, not the directory: the databases and the indexer's own
configuration, not its logs and not the staging file of an index run that was
stopped, which is about half again the size of the database it was writing.
`tehas index` removes what earlier runs left when it writes a new index, so the
copy does not grow with the number of runs. For this repository that is a few
megabytes and costs about a second per cycle. A cycle on a host that has never
run this command gets nothing and works as it did before an index existed: its
agents search by reading files, which is slower and finds less.

`install.sh` runs it once, the first time it finds the sandbox image and the
product checkout and no index. After that the dispatcher runs the script with
`--before-cycle`, as `tehas-index.service`, when it would start cycles and no
cycle is running, and only when `~/tehas/state/index` does not already hold
what the record says is the branch's tip. A run that finds the record naming
the tip says so and does nothing, even when the sandbox image has been rebuilt
since: the new image indexes the tree at the next commit, or at once when a
person runs `tehas index`. Its output goes to
`~/tehas/logs/dispatch-index.log`. An empty directory counts as
no index, so a host upgraded from the version that kept the index in a cache
volume builds it into the new one at its next idle start rather than reporting
an index that no cycle is given. Cycles started then begin with an index of
the tree they are cloned from. A cycle started while another runs gets the
index as it is, and the dispatcher's log says `the code index stays as it is:
a cycle is running on #<issue>`. On a host that is never idle the index falls
behind the branch. What a cycle changes, in its workspace or its worktrees, is
never in the index.

The indexer may run for 30 minutes, and `tehas-index.service` for 40, so a hung
indexer is stopped and the next poll starts the cycles. TERM, INT and HUP end a
run at once: it puts back what it set aside, removes its clone and exits 128
plus the signal. A container named `tehas-index` that a killed run left behind
is removed before each run.
When the indexer fails, `~/tehas/state/index.json` stays as it was and
`~/tehas/state/index.failed` holds the tip that failed. Before a cycle that tip
is not tried again: the dispatcher does not start the unit for it, and the
cycles start with the index as it is, an index written by another sandbox image
included, until the branch moves.

Run by hand it does the work even when the record says the index is current, a
failed tip too, and it holds the index's lock while it does, so no cycle starts
in the middle. A cycle started by hand with `tehas work` refreshes nothing.
