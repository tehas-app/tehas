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

### From a release

Releases are read from `TEHAS_RELEASE_REPO`, owner/name on GitHub. The
platform's own is the line in `versions.env`; an instance may name another in
`instance.env`. The list and the download use `gh` when it is signed in, which
reaches a private repository with the bot's token, and `curl` from the public
address otherwise.

`tehas update --check` prints the version installed, the newest, and the notes
of each release between them: its section of the changelog, with its
`Adopter action`. Going back with `--to`, it prints the notes of the releases
that would be undone. Then it runs `install.sh --check` of the version
installed. It downloads nothing, changes nothing, and exits 1 when there is a
version to move to.

`tehas update` and `tehas update --to vX.Y.Z` then:

1. download `tehas-<version>-linux-<arch>.tar.gz`, its `.sha256` and its
   `.minisig`, unless that version is still on the host. `<arch>` is the
   architecture `uname -m` answers, `x86_64` or `aarch64`, with `amd64` and
   `arm64` read as those. A machine whose `uname -m` says anything else has no
   release to download and stops with the words it answered, before anything is
   fetched;
2. check the tarball against the `.sha256` and the signature against
   `~/tehas/release/minisign.pub`, the key `install.sh` installed, never a key
   in the download. A download that fails either check is not unpacked. With
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
   units are the copies `install.sh` makes from that tree.

The sandbox image a version ran with keeps the tag
`tehas/pi-rukas:release-<version>`. Going back to a version that still has one
puts that image back under the uid tag and builds nothing. The version
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
`TEHAS_INSTANCE_DIR` and `TEHAS_TICKET_CONCURRENCY` - and names them too, so an
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
refuse at once, at most 300; anything else is the default. It is read from the
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
tag by default, is not the commit the image was built on. It does this also when no file of tehas changed, and
skips it when this run built the image already or a step failed. An image that
cannot be moved, for example because upstream has not published that commit's
image yet, makes the update exit 1. `--check` runs `build.sh --check` and
exits 1 when the commit moved. See
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
the record only once it has run. That record is what makes an update complete: a
run that stops partway, or an install.sh that died with half its files copied,
leaves the rest owed instead of losing it with install.sh's one-shot report of
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
nothing, builds nothing and starts nothing, and exits 1 when there is something
to update. A host whose `/etc/systemd/system/tehas.slice` is missing or is not
the file this version ships is something to update, and only root can do it:
`tehas update --check` stays red until `sudo ./bootstrap.sh slice` has been run.

What `install.sh` prints passes through as it printed it, apart from the `next:`
lines this command runs itself. That includes a `Login: NOT CURRENT` report for
`/usr/local/bin/tehas`, the command the other accounts type: root's to install,
so it is named as a line rather than run as a step.

When `install.sh --check` exits 1, `--check` says which kind it found.
`Installed: DRIFT` and `Follow-ups: NOT DONE` are `something to update`.
`Dependencies: NOT OK`, `Slice: MISSING` or `DIFFERS`, `Login: NOT CURRENT`,
`Home: REFUSED` and `Tests: FAILED` are `something tehas update does not fix`:
running the update
leaves them as they are. A refused home prints `Dependencies:`, `Installed:`
and `Tests:` as `not checked` under it, and nothing else: the script trusts
nothing under a home it cannot prove is tehas's.

| Exit | Meaning                                                                       |
| ---- | ----------------------------------------------------------------------------- |
| 0    | Updated, or already up to date                                                 |
| 1    | Another update is running, the checkout has local changes, is not on a branch, is not on the default branch of origin or does not know it, cannot fast-forward, has no commit yet, a step failed or ran out of its limit, `install.sh` ran out of its limit or gave no answer, a follow-up install.sh asked for is one this command does not know, systemd did not reload the unit files, a container or unit gave no answer, the record of steps owed exists but cannot be read, a status of `install.sh` this command does not know (named in the message), or `--check` found something to update (a checkout behind, files that differ, steps left over from an earlier run) or something an update does not fix (the host's tools, the slice, a host's `/usr/local/bin/tehas` that is not the checkout's, a `TEHAS_HOME` that is not provably tehas's, the tests) |
| 64   | More than one argument, or an argument this command does not know              |
| 78   | The instance settings are missing or malformed, or there is no platform checkout |

## install.sh

Installs tehas on the host from the platform checkout, for the instance the
host points at. See [The host](/host).

```
cd ~/tehas/platform
./install.sh --check     # change nothing; Home, Instance, Dependencies, Slice, Installed, Login, Follow-ups, Tests
./install.sh
```

Exit 0 when done or nothing to report, 1 when `--check` found something, a
step failed or another update is still holding the lock after the
`TEHAS_UPDATE_LOCK_WAIT` seconds the run waits for it (10 by default), 78 when
the instance settings are invalid or the home is not provably tehas's (nothing
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

It also removes the commands of the earlier layout from `~/tehas/bin`, once
systemd has reloaded the units that name them. See [The host](/host).

**The records of an older tehas.** A real run holds `state/dispatch.lock` from
before its first copy until after the store has imported the old records, so no
poll starts a cycle or a planning turn in between. A poll holds that lock for
its whole run, so the run waits for it up to `TEHAS_DISPATCH_LOCK_WAIT` seconds:
a whole number, 900 by default, 0 to refuse at once, at most 3600; anything else
is the default. It is read from the environment and asked of the console
(`console update wait dispatch`); a first install from a checkout has no console
yet and waits 900. A poll that holds the lock
longer stops the run with exit 1 and `a poll is running ...; nothing was
installed`. A home installed before the marker `.tehas-home` came in gets it
here, so its old records are judged too. Before the first copy the store the run installs judges the record
files an older tehas kept under `state/` (`store import --dry-run --say`, which
reads the files only and prints the lines below itself), and the run stops with exit 1 and `nothing was installed` when:

- a cycle or planning turn runs while such a file is there: `a cycle or planning
  turn runs (#N); run tehas drain --wait and let it end, then install again`.
  It would write more of them after the import;
- a file would be held, because it is a link, larger than 1 MiB, or does not
  parse, or a time in it is not `%FT%TZ`: the store's `store: held` line and one
  line per file, which stays where it is with the other files of its issue. Move
  each one out of `state/` or mend it, then run again.

After the database is made or migrated, `store import --say` makes the files rows and
moves them to `state/imported-<YYYYMMDD>/`, under their paths in `state/`, an
issue's files together once all their rows read back; nothing is deleted. The run prints `store: imported <table>
<rows>, ...; moved <n> files to ...` when it moved any. An import that fails
stops the run with exit 1: the files are installed, the database holds what was
imported, and running `install.sh` again finishes it. Each
`state/plan-<n>.json.keep` directory then moves to
`$TEHAS_WORK_DIR-plan-<n>.keep` while no planning turn holds its lock; one held
stays, with a `later:` line. See [The state store](/host#the-state-store).

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
