---
title: tehas update, install.sh and bootstrap.sh
description: "Bringing a host up to date, installing one, and preparing a fresh one."
---

# tehas update, install.sh and bootstrap.sh

## tehas update

Bring this host up to date in one command, from a release or from the platform
checkout.

```
tehas update                  # the newest release, or pull the checkout;
                              # install, run the follow-ups that apply
tehas update --to v0.8.1      # that release, an older one too
tehas update --to v1.0.0-alpha.1  # a prerelease, only ever when named
tehas update --check          # report what would change; change nothing
tehas update --pending        # run the follow-ups the record lists, no git
tehas update --accept-id janit  # as tehas update, and record janit's new user id
tehas update --help           # print the usage of the command, touch nothing
```

Where the new code comes from is `TEHAS_UPDATE_FROM` in `instance.env`:
`release` or `git`. Without it, a host with `~/tehas/current` updates from
releases and a host with a platform checkout from the checkout, so a host
installed before the setting existed goes on as it did. A host with neither
stops with 78 and names the setting. `--to` on a host that updates from its
checkout is refused with 64. Any other word is refused with 78 and
`TEHAS_UPDATE_FROM is not valid: expected git or release`.

### Settings tehas 1.0 renamed or removed

`api/renames-1.0.tsv` lists every public name tehas 1.0 renamed or removed. A
row of kind `setting` still set in `instance.env` or in `~/.config/tehas/env`
stops every command of the new version with 78, and the line names the new
key or why the old one went. tehas does not edit the lines: the instance
directory is a checkout of a settings repository. A row of kind `value` names
a value that changed, an exit status among them, such as
`tehas check-product exit=78` that is now 3; [the contract](/contract#exit-statuses)
has every command's statuses.

Before an update installs anything, install.sh asks the new version's console
to judge the instance. When the instance still sets such a row, that check
lists every one of them, one line each with the new key or the reason, and
nothing is installed: `tehas update` and `tehas update --check` exit 78 and
the host stays as it was, drained if it was drained. Edit the lines, then run
the update again. From a release, `--check` reads the release's own console;
from the checkout, `--check` reads the console an earlier build left, so a
real run, which builds first, is what names the rows there.

The installed version loads the settings too, before it takes the lock. When
that load stops on such a row, it also lists every one of them, the rows of
`instance.env` and of `~/.config/tehas/env` its own `api/renames-1.0.tsv`
names, and exits 78 before install.sh runs.

A feature still being built is refused the same way: its switch set to `on`,
or a line that gives one of its own settings a value, stops every command
with 78 and names the feature, and both checks list every such line at once.
A host that turned the review loop on under an earlier release takes
`TEHAS_REVIEW=on` and `TEHAS_REVIEW_REPO` out before it updates
([the review loop](/operating#the-review-loop)).

### From v0.30.x, v0.31.x or v0.32.x

1.0 updates a host on any v0.30.x, v0.31.x or v0.32.x release, with
`tehas update`. That run is the old release's own: it reads the repository
and the key the old release reads, and they are the ones 1.0 is published
and signed with. It asks 1.0's settings check before it switches
`~/tehas/current`, so an instance that still sets a renamed setting stops with
78 and nothing switched. The check lists every such line of `instance.env`
and `~/.config/tehas/env` at once, with every line of a feature still being
built. From 1.0 on, the repository and the key are the ones
built into tehas, and `versions.env` and `~/tehas/release/minisign.pub` are
no longer read for them.

A v0.30.x host can name only a release candidate or the release:
`tehas update --to v1.0.0-rc.1` or `--to v1.0.0`. Its update does not take an
alpha or a beta. A v0.31.x or v0.32.x host takes any of them when `--to`
names it. To try a beta from v0.30.x, update to v0.32.0 first.

v0.31.x and v0.32.x already refuse every setting 1.0 renamed or removed, so
an instance that loads there needs no rename. A v0.30.x instance does.
The steps a person takes, drain first, and every name 1.0 renamed or removed
are in [Updating from 0.30.x to 1.0](/operating#updating-from-030x-to-10).

**Known limit.** A program that manages the factory and writes 0.30 names
into the factory's `instance.env`, such as `TEHAS_TICKET_CONCURRENCY`, stops
the update with 78 and switches nothing: v0.31.x and later refuse the old
name, and the manager writes it again on each upgrade. `TEHAS_INDUSTRY_VERSION`
in `~/.config/tehas/env` is the one 0.30 name that is ignored, not refused.
Until the manager writes 1.0's names, such a factory cannot be updated
through it.

### From a release

Releases are read from the repository built into tehas, the
`TEHAS_RELEASE_REPO` line of `host/versions.env` when the release was built.
An instance may name another, owner/name on GitHub, with `TEHAS_RELEASE_REPO`
in `instance.env`. The `versions.env` on the host is not read for it: a
release writes that file, so it cannot choose where the next one comes from.
The list and the download use `gh` when it is signed in, which
reaches a private repository with the bot's token, and `curl` from the public
address otherwise.

The newest is the newest release, never a prerelease (`vX.Y.Z-alpha.N`,
`-beta.N` or `-rc.N`): a plain `tehas update` passes over them, and a host
installs one only when `--to` names it. They come in the order alpha, beta,
rc, then the release, so a host on `v1.0.0-beta.1` moves on with a plain
update once `v1.0.0` is out, and stays where it is until then. `tehas status
--json` names the release a host runs and the commit it was built from in
`versions.platform`.

A host that runs from its platform checkout, with no `current` yet, moves to
the newest too, unless the checkout's tag is newer than the newest release, as
a prerelease can be. Then it moves to the release of that tag, and when that
tag is not published it changes nothing and exits 1.

`tehas update --check` prints the version installed, the newest, and the notes
of each release between them: its section of the changelog, with its
`Adopter action`. Going back with `--to`, it prints the notes of the releases
that would be undone. Then it runs `install.sh --check` of the version
installed. It downloads nothing, changes nothing, and exits 3 when there is a
version to move to.

`tehas update` and `tehas update --to vX.Y.Z` then:

1. download `tehas-<version>-linux-<arch>.tar.gz`, its `.sha256` and its
   `.minisig`, unless that version is still on the host. `<arch>` is the
   architecture `uname -m` answers, `x86_64` or `aarch64`, with `amd64` and
   `arm64` read as those. A machine whose `uname -m` says anything else has no
   release to download and stops with the words it answered, before anything is
   fetched;
2. check the tarball against the `.sha256` and the signature against the key
   built into tehas, `host/release/minisign.pub` when the release was built,
   never `~/tehas/release/minisign.pub` or a key in the download. An instance
   that names another repository still checks with this key. A download that fails either check is not unpacked. With
   no `minisign` on the host it stops (`sudo apt-get install minisign`);
3. unpack it into `~/tehas/versions/<version>`, refusing a tarball that holds
   anything but files and directories under `tehas-<version>/`;
4. check the instance against that version's own console. A setting it
   requires and the instance lacks is named with the value of the release's
   `instances/example/instance.env`, and the update stops with 78 with nothing
   switched. The check runs under a limit of 120 seconds and reads at most
   64 KiB of what it says: one that runs out of time or says more stops the
   update with 1, nothing switched. A `TEHAS_` key in `instance.env` that the
   version never names is warned about, and a key the version's tree could not
   be searched for is named in a warning of its own;
5. point `~/tehas/current` at the version, and run `install.sh` and its
   follow-ups from there, as an update from the checkout does. The commands and
   units are the copies `install.sh` makes from that tree. When `install.sh`
   stops before it installs anything and says `nothing was installed`, for
   example because the version's store cannot open this host's database,
   `current` and the uid tag of the sandbox image are put back where they were
   (`release: install.sh did not finish <version>, so current is <version>
   again`), so a plain `tehas update` installs the version the host ran. A
   host that ran from its checkout loses `current` again. An `install.sh` that
   fails after it began to copy, or runs out of time, may have installed the
   version's files and migrated the database, so `current` stays on that
   version and the next `tehas update` finishes it.

The sandbox image a version ran with keeps the tag
`tehas/pi-rukas:release-<version>`. Going back to a version that still has one
puts that image back under the uid tag and builds nothing, unless its
`tehas.layer` label is not the sum of the instance's `sandbox.Dockerfile` as
it is now: then it is not put back, and the build `install.sh` owes runs. The version
`current` names and the two switched to before it stay in `~/tehas/versions`,
in the order `~/tehas/state/release-history` records; older ones are removed
with their image tags.

`~/tehas/state/update-hold` stops a plain `tehas update`. `--to` names its
version and runs with the hold there.

### Accounts by user id

A GitHub login can be renamed, and a login that was freed can be registered by
someone else. So a login alone does not say who applied a label. Before
anything moves, `tehas update` asks GitHub for the user id of each login of
`TEHAS_ALLOWED_LABELERS` and the millwright (`GET /users/<login>`)
and records the pairs in `~/tehas/state/account-ids`, one `<login> <id>` a
line. `install.sh` does the same before its first copy. The pair of a login
taken off the list stays, so the login, listed again, must still be the same
account.

From then on a label, a retry tick, an answer to a planning question and a
comment count only when the account that made them carries the recorded id
(`user.id`, `actor.id`, or GraphQL's `databaseId`). An account with the
login and another id, or with no id at all, does not count: the dispatcher
reads it as `<login>#<id>`, which no list matches, and says why on standard
error. With no id recorded for a login, nothing of it counts until
`tehas update` records one.

- A login that does not resolve stops `install.sh` and `tehas update` with 78
  and names it. Take it out of the settings or correct it.
- A login whose id is not the recorded one stops them with 78 and names both
  ids. If the account was renamed and is still the person the settings mean,
  `tehas update --accept-id <login>` records the new id. If not, take the login
  out of the settings.
- GitHub that cannot be asked stops them with 78 as well, before anything is
  installed.
- The ids are asked with the app's token, made from its key as
  `tehasbot github-token` makes it. That is what a
  first `tehas setup` has: the shim is not in place yet. A token that cannot be
  made stops them with 78.

### From the checkout

It runs, in order and one line per step: a `git fetch` and a fast-forward of
the platform checkout (`$TEHAS_HOME/platform`, in the example
`~/tehas/platform`), then `install.sh`, then the follow-ups install.sh asks for.
An argument it does not know, or `--help` with another one, is refused with
exit 64 before the settings are read. The settings are checked next against the
one required list, which tehas-core holds (`console/core/src/settings.rs`):
`tehas_load` hands it to `lib/config.sh` as `_tehas_required`, and
`tehas_require_instance_settings` checks it. install.sh, `console run`, tehas
setup and the dispatcher check the same five. A poll adds the three a load
always leaves set that it cannot act by without - `TEHAS_HOME`,
`TEHAS_INSTANCE_DIR` and `TEHAS_MAX_CYCLES` - and names them too, so an
instance with a required line still commented out stops with 78 before git runs. After a fast-forward the settings are checked
by the code it pulled: a checkout has no console of its own yet, so install.sh
builds it first and a change that adds a required setting stops with 78 there,
before anything of the host changes. The checkout must be on a branch and have nothing uncommitted, and
only a fast-forward is done: local commits of its own, or a branch that was
rewritten on the remote, are left for a person. Only the remote's default branch
is updated, because nothing reaches it without a person: a checkout on another
branch, or one whose `origin/HEAD` is not set, changes nothing and exits 1. To
install another branch, run `./install.sh` in the checkout by hand.
`git remote set-head origin --auto` sets `origin/HEAD`.

While `~/tehas/state/update-hold` exists it changes nothing and exits 1, naming
the file; `--check` says the hold is there and reports as usual. The file is
how a host is kept on an older release: see
[Rolling back tehas](/host#rolling-back-tehas).

One update runs at a time. A real run holds `$TEHAS_HOME/state/update.lock`,
and so does a real run of `install.sh`; a second one started while the first
holds it waits for it, and if it is still held prints
`another update is running`, naming the seconds it waited, and exits 1 before
git, install.sh, Docker or systemd is called. The wait is
`TEHAS_UPDATE_LOCK_WAIT` - a whole number of seconds, 10 by default, 0 to
refuse at once, at most 300. Any other value stops both commands with 78
before the lock. It is a [host setting](/tools#host-settings), read from the
environment, not from `instance.env`, and both commands ask the console for it
(`console update wait update`). A first `install.sh` from a checkout has no
console yet and waits 10.
The update hands the lock to the install.sh it runs, and to nothing else; the
wait covers a run started right after one that failed and left the lock held a
moment by something it had started. `--check` takes no lock.

The console does all of the update, the lock and install.sh included:
`libexec/update` asks it whether it answers and execs it. A console that does
not start stops an update with exit 1, see [tehas update says the console does not answer](/troubleshooting#tehas-update-says-the-console-does-not-answer).

That pre-check covers the required settings only. The rest of the instance
contract - `models.json` readable, valid JSON, listing the models the settings
name, and the settings directory writable by nobody else - is install.sh's, and
install.sh answers it with 78 before it copies a file.

After the follow-ups, pi-rukas. It is not pinned: where the installed `versions.env` sets
`PI_RUKAS_FOLLOW`, the update runs `sandbox/build.sh --if-moved`, which
rebuilds the sandbox image only when the commit it names, the newest release
tag by default, is not the commit the image was built on, or when the
instance's `sandbox.Dockerfile` is not the one the image was built from. It does this also when no file of tehas changed, and
skips it when this run built the image already or a step failed. An image that
cannot be moved, for example because upstream has not published that commit's
image yet, makes the update exit 1. `--check` runs `build.sh --check` and
exits 3 when the commit moved; a build check that fails or gives no answer
makes it exit 1, and 78 when the instance's settings do not load. See
[pi-rukas comes from upstream](/host#pi-rukas-comes-from-upstream).

The follow-ups are steps named by id in the console, which install.sh prints as
`next: <step line>` and this command runs as its own steps rather than as shell
text. The arguments belong to the step, and the console says which ones each id
accepts and which of them it requires:

| Step                        | install.sh asks for it when         | tehas update runs it when                                        |
| --------------------------- | ------------------------------------ | ----------------------------------------------------------------- |
| `units <unit>...`           | a unit file changed                  | systemd reloaded its unit files, and the unit is active: restarted, never enabled |
| `sandbox`                   | `sandbox/` changed, or `versions.env` changed in a line the image build reads: one named `SHELLCHECK_`, `JQ_`, `PLAYWRIGHT_`, `RUST_`, `RUSTUP_`, `CBM_`, `PI_`, `CTX7_`, `PARALLEL_CLI_`, `NODE_`, `GH_`, `BUN_`, `VIPUNE_` or `OO_` | always                                                            |
| `docs --force-recreate`     | `edge/docs.*` changed                | its container is running: recreated with that flag                 |
| `edge --force-recreate`     | `edge/router.*` changed              | its container is running: recreated with that flag                 |

Both containers are recreated rather than just brought up, because what changed is
the Caddyfile they mount and Caddy reads its config when the process starts:
`docker compose up -d` alone leaves a running container with an unchanged service
definition serving the config it loaded at its start.

A real update also brings the dispatch timer's drop-in in line with
`TEHAS_POLL_SECS`: it writes
`~/.config/systemd/user/tehas-dispatch.timer.d/poll.conf` for a value other than
120, removes it at 120, and leaves it alone when it already says what the
setting says. A drop-in that changed adds the step `units tehas-dispatch.timer`,
so the timer restarts on its new times where it runs, and a restart that fails
stays owed. A value out of bounds stops the update with 78 before the drop-in
is touched.

A service the host does not run is left alone, so an update adds nothing to it.
Whether one runs is asked of Docker and of systemd, and only their answer counts
as an answer, never their exit status on its own. The `units` step asks systemd to
reload its unit files before it restarts anything - a unit file copied onto the
host is not in effect until that has happened - and only silence next to exit 0
means the reload did: a reload that failed, or answered with text, stops the step
and leaves it owed rather than report a changed unit as applied. `is-active` is
asked without `--quiet` so that systemd names the state, and only its own state
words are read: any state of the active family (`active`, `activating`,
`reloading`, `refreshing`, `maintenance`, `deactivating`) restarts the unit,
`inactive`, `failed` and `unknown` (systemd has no such unit on this host) leave it
alone. An answer that is none of those - no user bus, or a `systemctl` wrapper that
prints an error and still exits 0 - stops the update with exit 1 instead of
reporting the service as absent. Liveness is asked of Docker with `docker ps
--filter label=com.docker.compose.project=...`, which needs the project name each
compose file pins.

A step line whose id or arguments are not ones this version of tehas defines is
not run: it is named in a `WARN:`, stays in the record of steps owed, and the
command exits 1, so a producer and a consumer that have drifted apart cannot both
stay green while a step nobody performed is treated as done. The units on a step
line are checked as unit names and against the units this version of tehas
installs, so a step line cannot name some other unit of the host's for an update
to restart. Once install.sh has run, the console it installed reads its answer and
runs the steps, so a unit the new version adds is restarted by the update that
installed it. A `--to` below the release that moved this into the console installs
one that cannot, and `libexec/console.prev`, which install.sh keeps, finishes that
run. A compose line that leaves out its flag is refused for
the same reason: `docker compose up -d` without `--force-recreate` is the weaker
action, and running it would renew nothing while the step was counted as done.

The record itself is read before anything runs, and a record this account cannot
open stops the update with exit 1 rather than reading as "nothing is owed":
steps somebody else recorded would otherwise be replaced with what this run
happened to know. Each rewrite of the record goes through a temporary file moved
over it, so a run that died mid-write leaves the old record rather than half of a
new one.

**Steps owed.** install.sh records each follow-up as it copies the file that calls
for it, in `$TEHAS_HOME/state/update-pending`, and `tehas update` takes a line off
the record only once it has run. A record that cannot be removed after the last
step fails the run with exit 1, naming the file. That record is what makes an
update complete: a run that stops partway, or an install.sh that died with half
its files copied, leaves the rest owed instead of losing it with install.sh's one-shot report of
what changed - the next run compares again and finds those files matching.
`install.sh --check` reports the record as `Follow-ups: NOT DONE` and exits 1
while any of it is left, including a step line no version of tehas here knows.
Steps run in a fixed order - systemd, the sandbox, docs, edge, labels - whatever
order the lines were recorded in, so a step owed from an earlier run does not jump
ahead of this run's unit reload. A hand-run install.sh leaves the same record
behind; running `tehas update` finishes it, and repeating a renewal that has
already happened is harmless. A hand-run install.sh prints its own report only for
what its files changed: when they all match it says `tehas is up to date` even with
steps still in the record, so `install.sh --check`, or `tehas update --check`, is
what names them. There is no command that marks a follow-up done by hand; running
`tehas update` runs it again and clears it.

**`--pending`.** `tehas update --pending` runs the lines that record holds and
nothing else: it takes the update lock, reads `$TEHAS_HOME/state/update-pending`,
runs each step in that order, drops each line as it runs, and calls no git command and no `install.sh`. That is the half of an
install a program can finish on a host the other three forms refuse: a checkout
at a release tag has no branch to fast-forward, and a remote that is a local path
can stop answering, while the copies `install.sh` made are already in place. It
asks nothing of the checkout or of a remote, so it needs neither: a host with no
`~/tehas/platform` and no `~/tehas/current` runs it, where a plain update would
stop with 78 naming `TEHAS_UPDATE_FROM`. A hold does not stop it, because it
moves nothing. An empty record prints `update: nothing is owed` and exits 0; a
step that fails leaves its line in the record and the run exits 1, so the next
run has the same work list; a line no version here knows is named in a `WARN:`,
stays recorded, and exits 1. `--pending` with `--check` or with `--to` is refused
with 64, before the settings are read.

Every call outside the command has a limit, and hitting it fails saying how long:
120s for the fetch, 1800s for install.sh (copies, dependency check and its one
call to a remote), 180s for Docker, 60s for systemctl, 1800s for the sandbox
build. An `install.sh --check` stopped at its limit has copied nothing, and the
line says so. `tehas update --check` does not run the shell tests; run
`./install.sh --check --tests` by hand for them.

`--check` contacts the remote with `git fetch --prune`, so the remote-tracking
refs under `origin/` match what the remote has, including branches the remote
deleted; no file of the checkout or of the host changes. It asks the sandbox
build whether the pi-rukas commit it follows has moved - which builds nothing, as
`build.sh --check` builds nothing, but reports what stops a build from starting at
all: a recipe it cannot cut, or something that is not a plain file at the name of
the copy it stages - and runs `install.sh
--check`, which reports the checkout, the slice, the installed files, any
follow-up an earlier run left undone, and the tests; it merges nothing, installs
nothing, builds nothing and starts nothing. It exits 3 when there is something
to update: a checkout behind, steps left over from an earlier run, files that
differ, or a pi-rukas commit the image is not on. It exits 1 when it found
something an update does not fix: the host's tools, the slice, root's copy of
the command or the tests, even beside something to update. A host whose
`/etc/systemd/system/tehas.slice` is missing or is not the file this version
ships is one of those, and only root can fix it: `tehas update --check` stays
red until `sudo ./bootstrap.sh slice` has been run.

What `install.sh` prints passes through as it printed it, apart from the `next:`
lines this command runs itself. That includes a `Login: NOT CURRENT` report for
`/usr/local/bin/tehas`, the command the other accounts type: root's to install,
so it is named as a line rather than run as a step.

When `install.sh --check` exits 1, `--check` says which kind it found.
`Installed: DRIFT` and `Follow-ups: NOT DONE` are `something to update`.
`Dependencies: NOT OK`, `Slice: MISSING` or `DIFFERS`, `Login: NOT CURRENT`,
`Home: REFUSED`, `Tests: FAILED` and `Cycle token: CANNOT PUSH` are
`something tehas update does not fix`:
running the update
leaves them as they are. A refused home prints `Dependencies:`, `Installed:`
and `Tests:` as `not checked` under it, and nothing else: the script trusts
nothing under a home it cannot prove is tehas's.

| Exit | Meaning                                                                       |
| ---- | ----------------------------------------------------------------------------- |
| 0    | Updated, or already up to date                                                 |
| 1    | Another update is running, the checkout has local changes, is not on a branch, is not on the default branch of origin or does not know it, cannot fast-forward, has no commit yet, a step failed or ran out of its limit, `install.sh` ran out of its limit or gave no answer, a follow-up install.sh asked for is one this command does not know, systemd did not reload the unit files, a container or unit gave no answer, the record of steps owed exists but cannot be read, or cannot be removed once its last step ran, a status of `install.sh` this command does not know (named in the message), or `--check` found something to update (a checkout behind, files that differ, steps left over from an earlier run) or something an update does not fix (the host's tools, the slice, a host's `/usr/local/bin/tehas` that is not the checkout's, a `TEHAS_HOME` that is not provably tehas's, the tests) |
| 64   | More than one argument, or an argument this command does not know              |
| 78   | The instance settings are missing or malformed, or there is no platform checkout |

## install.sh

Installs tehas on the host from the platform checkout, for the instance the
host points at. See [The host](/host).

```
cd ~/tehas/platform
./install.sh --check     # change nothing; Home, Instance, Dependencies, Slice, Installed, Login, Follow-ups, Tests
./install.sh
./install.sh --lab-build # a lab host only: programs that read TEHAS_TEST_FEATURES
```

Exit 0 when done or nothing to report, 1 when `--check` found something, a
step failed or another update is still holding the lock after the
`TEHAS_UPDATE_LOCK_WAIT` seconds the run waits for it (10 by default), 78 when
the instance settings or `TEHAS_UPDATE_LOCK_WAIT` are invalid, the home is
not provably tehas's, or `--lab-build` was run from a release tree (nothing
was changed).
`Home: REFUSED` is what `--check` says for the last of those: one line for the
home, the reason under it, and `not checked` for what a home cannot be asked.
`Slice: MISSING` and `Slice: DIFFERS` are something found: `--check` exits 1
until root has run `sudo ./bootstrap.sh slice`, because every sandbox container
runs in the slice and none starts without it. A real run installs either way and
prints a `note:` line instead.

After a real run it prints one `next: <step>` line per follow-up its changed
files call for - the ids the console defines - and starts nothing
itself; `tehas update` is what runs them. Each of those steps is also recorded in
`$TEHAS_HOME/state/update-pending` as the file that calls for it is copied, by the
console this run installs, so a real run needs one that starts before its first
copy: the release tree's, the one it built, or the one installed. A run that dies
partway still leaves its copies owed, and `--check` adds a
`Follow-ups: NOT DONE` line, and exits 1, while any of them is left. It asks
systemd for nothing, but to reload before it removes the earlier layout: a copied unit file is not in effect until systemd has reloaded
it, and that reload belongs to the `units` step, which cannot report a changed unit
as applied when the reload did not happen.

A real run also builds the code index with [tehas index](/tools-dev#tehas-index), once:
when there is none yet and both the sandbox image and the product checkout are
there. Otherwise it prints the command on a `later:` line, which is for a
person and is not a step of `tehas update`; the dispatcher builds the index
when it next starts a cycle with none running. An indexing that fails is a warning and does not
fail the install.

A real run that replaced `versions.env` prints `later: <checkout>/bootstrap.sh
user`. The pins moved, and the tools on the host, pi-rukas among them, follow
them only when that runs; until then `--check` says `Dependencies: NOT OK`. It
is no step of `tehas update`, because it fetches from the network.

**The cycle token.** A real run that ended well then proves that the GitHub
App's cycle token can push, before the factory starts work on this version. It
asks `tehasbot github-token --cycle` for the token and makes a dry-run push with
it from the product checkout (`TEHAS_WORK_REPO`) to its `origin`, to the branch
the sandbox's own push check names. The token reaches git through its
environment, never a command line. It prints `Cycle token:  OK (<login> can push
to origin)`. A refused push, or no token, prints `Cycle token:  CANNOT PUSH`,
the app's login, git's last lines and what the app needs, and the run exits 1.
On a [read-only](/operating#read-only) instance the token is made and not
pushed with, since it may not push: `Cycle token:  OK (read-only: <login> may
not push, so no push was asked)`.
What was installed stays installed. A drained factory ends with `the factory
stays drained: ...` in place of the line below; a factory that is not drained
ends with `the factory is not drained: ...`, and the poll's push-fault hold
stops it from starting one issue after another. `install.sh --check`, and so
`tehas update --check`, make the same proof and report the same line, as
something `tehas update` does not fix. A host with no product checkout proves
nothing and prints no line. See
[Troubleshooting](/troubleshooting#tehas-update-says-the-cycle-token-cannot-push).

### Proving a building feature on a lab host

A feature still being built ([features still being
built](/contract#features-still-being-built)) is off in every release: its
switch set to `on` stops every command with 78. A lab host proves one before
it ships, by installing a build that reads `TEHAS_TEST_FEATURES` and naming the
feature there. No release reads the variable, so this is never a setting of a
production host.

1. Make a lab build from a checkout. A release tree has no source and is
   refused with 78:

   ```
   cd ~/tehas/platform
   TEHAS_TEST_FEATURES=outsource ./install.sh --lab-build
   ```

   Every program is built with `TEHAS_LAB_BUILD=on`, a marker the programs
   read when they are compiled, and gets a lab build's stamp. A lab build reads
   `TEHAS_TEST_FEATURES` and nothing else of the tests: it has no debug
   assertions, so it does not read the tests' other hooks, such as
   `TEHAS_TEST_STORE_ANSWERS`. `TEHAS_TEST_FEATURES` on the command line is
   for this run only: it lets the install see the feature, so a feature with
   programs or units of its own, as outsource has, gets them. The run prints
   `note: a lab build`.

2. Put the variable in the units' environment. The settings load refuses it
   in `instance.env` and `~/.config/tehas/env`. systemd gives every user unit,
   the dispatcher, each cycle, the Protolab and the console they run, the
   variables of `~/.config/environment.d/`:

   ```
   mkdir -p ~/.config/environment.d
   echo 'TEHAS_TEST_FEATURES=outsource' > ~/.config/environment.d/tehas-lab.conf
   systemctl --user daemon-reload
   systemctl --user restart tehas-mcp.service tehas-top.service   # the long-running units that are active
   ```

   Names are separated by spaces. A word that is no building feature is
   ignored.

3. Switch the feature on in `instance.env` (`TEHAS_OUTSOURCE=on`) and prove
   it. A shell of the tehas user does not read `environment.d`, so a command
   typed there, such as `tehas api --json`, needs
   `TEHAS_TEST_FEATURES=outsource` in front of it, or it stops with 78.

Health warns on a lab build whether the variable is set or not. Set, it says
`A lab build: TEHAS_TEST_FEATURES names outsource, still being built. No
release reads it.` and `checks.testFeatures` says `{"read": true, "features":
["outsource"], "lab": true}`. Unset, it says `A lab build: TEHAS_TEST_FEATURES
is unset, so no building feature is on. ./install.sh without --lab-build goes
back.` and `features` is empty. `tehas status` prints a `features` line either
way.

To go back to a release, switch the feature off, then:

```
rm ~/.config/environment.d/tehas-lab.conf
systemctl --user daemon-reload
tehas update            # or ./install.sh in the checkout
```

Any run without `--lab-build`, from a checkout or a release, installs every
program again: a lab build's stamp matches no other. A run from a checkout
without `--lab-build` builds with `TEHAS_LAB_BUILD=off`, and every run with
`CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false`, whatever the environment it
runs in says. No build takes rustflags or release profile settings from the
environment (`RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`,
`CARGO_TARGET_<triple>_RUSTFLAGS`, `CARGO_PROFILE_RELEASE_*`), and an empty
`CARGO_ENCODED_RUSTFLAGS` outweighs the rustflags of cargo's configuration
files too, so `-C debug-assertions` there does not reach the programs. The
release workflow builds with neither variable set and the same rustflags. `install.sh --check` and
`tehas update --check` report a lab build's programs as `differs`. A run that
finds `~/.config/environment.d/tehas-lab.conf` prints `later: rm ... &&
systemctl --user daemon-reload`, and leaves the file to you. If the variable
stays in the units' environment, health warns on every poll that this build
does not read it.

A real run that ended well on a factory `tehas drain` stopped prints one more
line last: `the factory is drained: tehas limit clear starts the factory, and
this version has run no cycle yet`. A drain has no clock and nothing clears it,
so the version that was just installed has run nothing and is proven only when
`tehas limit clear` runs. The line changes no exit status, and `--check` and
`--pending` print nothing new.

It also removes the commands of the earlier layout from `~/tehas/bin`, once
systemd has reloaded the units that name them. See [The host](/host).

**The dispatcher's lock.** A real run holds `state/dispatch.lock` from before
its first copy until the old files are removed, so no poll starts a cycle or a
planning turn in between. A poll holds that lock for its whole run, so the run
waits for it up to `TEHAS_DISPATCH_LOCK_WAIT` seconds: a whole number, 900 by
default, 0 to refuse at once, at most 3600; anything else is the default. It is
read from the environment and asked of the console (`console update wait
dispatch`); a first install from a checkout has no console yet and waits 900. A
poll that holds the lock longer stops the run with exit 1 and `a poll is running
...; nothing was installed`. A home without the marker `.tehas-home` gets it
here once the home is proved tehas's.

**From v0.30 only.** This release migrates a store that v0.30.0 or a later
release wrote, at schema 12 or newer. Before its first copy, while it holds
the dispatcher's lock, the run asks the store it installs to judge the
database there (`store migrate --dry-run`, which reads only). An older store
stops the run with exit 78, the store's line, `... older than v0.30.0 wrote:
update to a v0.30 release first, then to this one`, and `this release cannot
migrate ...; nothing was installed`: no file is copied, the database is left
as it was and no backup of it is written. The record files a
tehas before the state store kept under `state/` are no longer imported: a
v0.30 install imported them. A home with no marker `.tehas-home` and other
files in it is refused, as any home that is not tehas's is: every install since
v0.30.0 leaves the marker. Each `state/plan-<n>.json.keep` directory moves to
`$TEHAS_WORK_DIR-plan-<n>.keep` while no planning turn holds its lock; one held
stays, with a `later:` line. See [The state store](/host#the-state-store).

One read says what it cannot read: `store journal-open` answers each open action
with the `args` object it recorded, and with `"args_unreadable": true` and no
`args` for a row whose stored arguments are not a JSON object. Such a row comes
from a store written by hand or from an older release: `journal-record` refuses
an `--args` that is not one. The marker keeps a reader from taking the arguments
of an unreadable action for an action that took none.

One file it reports and never touches: `/usr/local/bin/tehas`, the command an
account that is not the tehas user types, which `sudo ./bootstrap.sh dev` puts
there as root from `host/login/tehas`. Where that setup is in place, its marker
`/etc/tehas/manager` names the tehas user, and a copy of the command that is not
this checkout's makes `--check` add a `Login: NOT CURRENT` line naming the command
that installs it (`sudo <root-owned copy>/bootstrap.sh dev boss`) and exit 1. The
command names root's own copy of the platform, not the checkout, which the tehas
user can write: see [Root runs its own copy](/bootstrap#root-runs-its-own-copy). A
real run prints the same finding on a `later:` line. It is never a `next:` step,
because no tehas command runs as root, and nothing is recorded as owed. The name
in that command is the marker's; a marker holding something other than an account
name leaves `<tehas user>` for the person to fill in. A host where
`bootstrap.sh dev` never ran reports nothing. See [The host](/host).
