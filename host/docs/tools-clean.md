---
title: tehasbot clean and tehasbot preview
description: "What the cleaner removes and when, and the preview of every pull request."
---

# tehasbot clean and tehasbot preview

## tehasbot clean

```
tehasbot clean                # one run; one line per removal in ~/tehas/logs/clean.log
tehasbot clean --dry-run      # log what it would remove, with its size, and remove nothing
systemctl --user enable --now tehas-clean.timer
tail ~/tehas/logs/clean.log
journalctl --user -u tehas-clean
```

Removes what the host can make again, of anything untouched for
`TEHAS_CLEAN_AFTER` hours (24 by default), and keeps the logs and the state
store in bounds. Seven steps, and a step that fails
is logged as a warning while the others still run. One exception to the age:
when less than `TEHAS_CLEAN_MIN_FREE` (30g by default) is free on the file
system of `~/tehas` or on the one Docker keeps its images on, whichever has
less, steps 1 and 2 take build output and the build cache whatever their age -
step 2 with `until=0h`, which prunes the whole cache - and the run opens with
the line `clean: low on disk: <n> GiB free, under TEHAS_CLEAN_MIN_FREE=<value>:
build output and build cache go whatever their age`. Steps 3, 4, 5 and 6 keep the
age, step 7 runs only then and keeps the age too, the free figure is read once for the whole run, and a figure that cannot
be read changes nothing. The lock rule is not relaxed: a workspace whose lock a
cycle holds, and a clone whose lock `tehas dev` holds, are left alone low on
disk or not. It sits above
`TEHAS_CYCLE_MIN_FREE`, so the cleaner frees space before the poll stops
starting cycles:

1. **Build output in kept workspaces.** For each issue with a kept workspace
   (`<name>-<n>` and `<name>-<n>.before-retry`) that no cycle is running, and
   whose lock and directories have all gone untouched for that long: every
   directory anywhere in the tree whose name is one of `TEHAS_CLEAN_DIRS`, and
   `<name>-<n>.state/cache`, the cycle's copy of the code index. The issue's
   lock is held while they go, as the 7-day sweep holds it, so no cycle starts
   meanwhile. What stays is what a person reads: `.git`, the sources, what the
   agents wrote there, uncommitted changes, `.state/memory` and
   `.state/sessions`. It runs no git command in a workspace except one:
   `git -C <worktree> rev-parse --absolute-git-dir`, to name the gitdir of a
   worktree for the age rule, in a tree a cycle wrote. That call is confined: the
   bound and git itself are named by absolute path before the environment is
   cleared, so neither is answered by a helper a cycle planted on the tehas user's
   `PATH`; no config file is read at all - no system file, no global file, none of
   the tree's own - and `core.fsmonitor`, the one hook `rev-parse` could act on,
   is pinned off on the command line; and the answer is used only when it names an
   existing directory
   under `~/tehas/work`. A cycle makes its worktrees in its container, where the
   workspace is mounted at the shared checkout's path, so a worktree's `.git`
   names a gitdir under `~/tehas/work/<name>/.git/worktrees/`. When git cannot
   answer for such a name, the cleaner reads the same gitdir under the tree's own
   workspace (`<name>-<n>/.git/worktrees/`), kept on the same terms. When it is
   not there either, the record is stale: it is no proof of a cycle at work, and
   the tree is dated by itself. The call is bounded, so a git that wedges cannot wedge the
   timer with it, and the issue is left whole when git cannot answer for a
   worktree, on the line
   `kept the build output of #<issue>: git could not name the gitdir of one of its
   worktrees` - which is what a stale worktree record reads as, and is a different
   thing from the young answer beside it. It follows no
   symlink (a link named `target` stays a link and what it points at is not
   touched). It enters no dot-name: every name `TEHAS_CLEAN_DIRS` can hold is
   un-dotted, so this gives up no build output, and it is what keeps a
   workspace's own trees, whatever a container called them, out of the removal.
   A removal needs the issue's lock to be there and freeable. The lock is asked
   for once per issue before any removal, since taking the lock of a name that is
   not there would make it: no lock file, or one that is not a plain file, means
   none of the issue's removals are even tried, and the log says so
   with `kept the build output of #<issue>: no lock to hold, so nothing of it was
   removed` - the line an issue with no build output in any of its trees does not
   get, since it is reported as `nothing to remove in #<issue>`, and the line a
   lock that exists but could not be taken does not get either, since that removal
   is reported by its warning. A lock held by a cycle is asked for first, before
   the age, and is `kept the build output of #<issue>: its lock is held`. A link
   or a directory at the lock path, and a lock no one can open, are asked of the
   one probe the library keeps (`tehas_core::lock`, `console/core/src/lock.rs`),
   which the dispatcher and the health page ask too, and answer the same as a lock
   a cycle holds: kept. A dangling link is not free, so nothing of the issue goes
   under it. The lock is taken
   for the instant of each removal. The build output of a manual session is no
   part of this step: the walk enters no directory under `~/tehas/dev` for
   build-output names, and that tree belongs to step 7 (see
   [tehas dev](/tools-dev#tehas-dev)).
2. **Docker's build cache** of that age, with `docker builder prune -f --filter
   until=<N>h`, or `until=0h` for the whole cache when the host is low on disk.
   This is the build cache of the whole host's Docker daemon: tehas
   is one project on one host, and the cache of anything else on it is made
   again too. The next sandbox build is slower after a prune. No image and no
   volume is removed. A dry run does not prune: it says what it would.
3. **Host scratch.** Files under `~/tehas/tmp/dl` and
   `~/tehas/tmp/node-compile-cache`, `~/tehas/tmp/analytics-facts.*` and
   `~/tehas/tmp/health.*` files, and
   `~/tehas/tmp/tehas-test.*` directories, of that age. Nothing else under `~/tehas`, and nothing outside it: not
   `~/tehas/dev`, so nothing under a manual session's `<name>.state` is walked
   here. Step 7 is the only step that names a path under `~/tehas/dev`.
4. **Old sandbox images.** The dated tags of the sandbox image before the
   newest two builds, whatever their age, taken one `docker rmi` at a time: the
   current build and the one before it stay, for a rollback, a `<date>-product`
   counted with its `<date>`, and so does any tag a container or a `uid` tag was
   made from: a cycle runs from those images, so one in use is
   never old. The build keeps the same two itself
   (see [Sandbox image](/host#sandbox-image)).
5. **The pulled base image.** The image upstream publishes for the commit
   `versions.env` follows, which a sandbox build started from before the base
   was built on the host, removed by name
   with one `docker rmi` of its own once it is of that age and no container was
   made from it. Nothing else is offered to docker: the step asks for that one
   name, and never for the images that have lost their tag.
6. **Logs and the state store.** The logs in `~/tehas/logs` of cycles
   (`work-*`, `dispatch-work-*`), planning turns (`plan-*.jsonl`,
   `dispatch-plan-*.log`), index runs (`index-*`), reviews (`review-*.json`,
   `review-*.jsonl`) and the improvement loop's analyst (`improve-analyst*.log`)
   older than 7 days, whether or not the
   improvement loop is on. Then `store backup`, a copy a day of the state store
   kept seven days, and `store prune`, which also removes the history of issues
   whose cycle ended more than its window ago, each of them keeping its last row
   of each history. The window and what the rule keeps are under
   [Backups and restore](/operating#backups-and-restore). All of it is one call,
   `store upkeep`, and says one line per log removed and then
   `clean: store upkeep: <answer>`. A dry run names the logs and leaves the
   store alone.
7. **The build output of a manual session.** `<name>.state/target` of every
   clone under `~/tehas/dev`, a plain directory of the account the cleaner runs
   as and no link. Every run logs one line per clone with the size of it. It
   goes only under `TEHAS_CLEAN_MIN_FREE` and once it has gone untouched for
   `TEHAS_CLEAN_AFTER` hours; above that figure a clone keeps its build, so a
   healthy host never loses one. Nothing else under `<name>.state` is
   named: the clone, its memory, sessions, runs, transcripts, author and images
   stay. A session is open when the lock `tehas dev` holds for its whole life,
   `~/tehas/state/dev/<name>.lock`, is held, or when `docker ps` names the
   session's container `tehas-dev-<name>`; the containers of the host are listed
   once for the whole step, as the image steps list the host's images once. A
   lock that is a link, a directory or a file no one can open is the same as
   held, and so is a docker that will not say whether the container stands, on
   the line `docker gave no answer for the session of clone <name>, so its build
   output was kept, <n> bytes`: an unknown is never a reason to take a build
   away. The cleaner holds the lock through the removal, so a session that starts
   then waits and does not meet a target half removed. A kept clone says `kept
   the build output of clone <name>: its session is open, <n> bytes`, `the disk
   is not low, <n> bytes`, or `its target was written within <N> hours, <n>
   bytes`, and a taken one
   `removed <path>: the build output of clone <name>, <n> bytes`.

A dry run measures what it names. Each path it would take is logged as
`clean: would remove <path>: <what>, <n> bytes`, with the same figure
`bytes_of` counts for the real removal, and the run closes with
`clean: would free <n> bytes, which leaves out the build cache, the image tags
and the store's upkeep, whose size is not known beforehand` rather than
`clean: freed 0 bytes`. Three steps cannot give a size beforehand - Docker's
build cache, the image tags of steps 4
and 5, which share layers with tags that stay so no tag's removal frees the
size docker reports for its image, and step 6, whose store answers names and not
sizes - and each says so on its own line. A dry run removes nothing, so it takes no
issue's lock, and backs the state store up not at all. Its figure is an estimate
of a tree that moves while it is measured: a workspace a cycle writes between
two dry runs answers with two figures.

The walk that measures one path stops after a million names or ten seconds,
so a huge tree does not hold up the rest of the pass. A figure cut short
there reads `at least <n> bytes`, on the path's line and on the closing
`freed` or `would free` line. The removal itself is not bounded: the path
still goes.

Preview images are `tehasbot preview`'s and no step here touches them. Docker
volumes are never touched (see [the host](/host#what-a-container-may-not-do),
where they are a person's call). A docker that answers no image list, or nothing
about the containers on the host, leaves every image where it is: an unknown is
never a reason to take an image away.

`tehas-clean.timer` runs it once an hour, the first time 15 minutes after boot.
It starts no work, so the pause label does not hold it. One run at a time: a
second one that finds the first working leaves it to that one and exits 3.
Exit 0 when every step worked; 1 when a step failed (what could be removed was
removed); 3 when another run holds the lock, which changes nothing; 64 on an
argument this command does not know; 78 when the settings are missing or
malformed, or the console is not installed beside the command, before any
docker, find or rm call. The steps are the console's
(`~/tehas/libexec/console clean`), and so are the two image names; the command
loads the settings and reads the free disk.

Each line that goes to `clean.log` is printed as well, and the unit adds no
redirect of its own, so a timer run's output is in the journal. The exits that
happen before the log can be opened - 78, 1 and the 3 of a held lock - are said
on stderr only: `journalctl --user -u tehas-clean` is where a timer run that
left nothing in `clean.log` explains itself.

## tehasbot preview

```
tehasbot preview            # one run; a line per preview that changed
tehasbot preview --sweep    # the second check of what is left on the host
systemctl --user enable --now tehas-preview.timer tehas-preview-sweep.timer
tail ~/tehas/logs/preview.log
```

Keeps a running preview of every open pull request whose head branch is in
the product repository and that was opened by an owner, member or
collaborator of it: a container `tehas-preview-<pr>` that the edge router
answers with at `uat-<pr>.<domain>`. Deterministic, no model. It needs
`TEHAS_PREVIEWS=1` and `TEHAS_DOMAIN`. Setting it up, and what a preview can
and cannot reach, is on [Previews](/edge).

The head of the repository's default branch has a preview too: a container
`tehas-preview-main` at `uat-main.<domain>`, image `tehas-preview:main`, state
in `~/tehas/preview/main/state.json`, log `~/tehas/logs/preview-main.log`. Every
run asks GitHub for the default branch and its head (`gh api repos/<repo>` and
the branch under it), fetches it into the same clone and builds it as a pull
request's commit is built, with the same options and the same free-space floor.
A new commit is built while the old container keeps serving; a commit without
`preview/Dockerfile` has no main preview and the state is `no-recipe`. It does
not count against `TEHAS_PREVIEW_MAX`, there is no notice for it, and neither
the run nor `--sweep` removes it. A call to GitHub that fails or answers
nothing clear leaves what runs: `main: GitHub did not say what the default
branch is at, left as it is`. `previews.json` carries it as the first entry,
with `name` set to `main` and no `number`. Its log lines start `main:` where a
pull request's start `#<pr>:`. It is looked at before the pull requests, so a
list of them that cannot be read does not stop it.

The two timers are enabled by the operator, with the line above. `tehas update`
restarts only a unit that is already active, so on a host that was installed
before previews existed the timers are installed and do not run until they
are enabled. Both write to `~/tehas/logs/preview.log`.

**A run**, every five minutes:

1. It asks Docker whether it answers and for its version, and makes the
   network `tehas-previews` if it is not there. Then it checks what keeps a
   preview from the host: Docker is 27.1 or later, and the network is
   internal, a bridge, without IPv6 and made with the option that gives the
   host no address on it. If one is missing, the run stops here and the log
   says `the network tehas-previews does not keep the host out (<reason>);
   nothing was built`. [Previews](/edge) lists the reasons. A run that stops
   here still writes `~/tehas/live/previews.json`, with the free space. So
   does one that stops because Docker does not answer, with no figure for the
   free space and marked low, so `tehasbot health` warns. The time in that file is therefore the time of the last run,
   whatever the run could do.
2. It measures the free space: the smaller of the file system of `~/tehas`
   and the one Docker keeps its images on. This is done once Docker has
   answered, before the network is checked.
3. It asks GitHub for the open pull requests, all of them, in pages of 100,
   newest first, and keeps the bytes of each page. It stops when a page is
   short, which is where GitHub ends.
   When the tenth page comes back full it asks for one more: an empty answer
   means the list ended there, so a thousand open pull requests is a list it
   has read. Anything in that answer means it stopped before the end, and it
   builds and removes nothing: `the open pull requests are more than 1000: the
   list was not read, nothing was built or removed`. No answer, an answer with
   nothing in it, or one that is not a list ends the run the same way: nothing
   is built and nothing is removed. A list with no pull request in it is an
   answer, and every preview is then looked at one by one, as step 5 says. A
   page that does not say who opened one of the repository's own pull requests
   is not a clear answer either: `the list of open pull requests does not say
   who opened each; nothing was built or removed`. Each page is an answer of
   its own, taken at its own moment, so the list can move while it is read: a
   pull request that comes back on two pages counts once.
4. The console reads those pages and chooses what is done with each of them.
   A pull request is built when its head is in the product repository itself
   and an owner, member or collaborator of the repository opened it; the newest
   `TEHAS_PREVIEW_MAX` of those are built and the rest say `over-limit`. A
   head in another repository is counted in the log and never built: `<k> pull
   requests from elsewhere, not built`, nor one opened by anyone else: `<k>
   pull requests opened by someone outside the repository, not built`. On a
   public repository anyone can open a pull request between two branches that
   are already there. The console reads a list it cannot read, or one that
   does not say who opened one of the repository's own pull requests, as no
   choice at all, and the run builds and removes nothing on it.
5. What is on the host for a pull request the list does not have is removed:
   the container, the image, the fetched commit, the state and the build log.
   Membership in that list is not enough on its own, because a pull request can
   fall between two pages and look closed. Before it removes anything the run
   asks GitHub about that pull request by itself, as `tehasbot preview --sweep`
   does, and what GitHub answers is what the log says: `#<pr>: pull request no
   longer open, removed` for one that is closed or not there at all, `#<pr>:
   its branch is not in <repo>, removed` and `#<pr>: it was not opened by
   someone of <repo>, removed` for one that is open but not built here. Nothing
   is removed where GitHub does not answer clearly: `#<pr>: no answer from
   GitHub, kept`, nor where it says the pull request is open and one of ours:
   `#<pr>: open though the list did not say so, kept`.
6. Each pull request that is due is fetched and built. The commit is fetched
   into the bare clone `~/tehas/preview/repo.git`, and its
   `preview/Dockerfile` is built with the commit as the context, in at most
   15 minutes. An image that declares a volume outside `/tmp`, `/config`
   and `/data` is not run: that is a failed build, and the image is removed.
   The new container replaces the old one. Five seconds after
   its start Docker is asked whether it still runs and was never restarted.
   The old container went before the new one was started, so when the new one
   is not running then, nothing serves for that pull request: the address
   answers 404 until a commit starts. A container that was never created can
   be the host and not the commit: it is tried again, and its image keeps the
   tag for that try. A container that ran and then stopped is the commit: its
   last lines go in the build log, and it is removed with the image its build
   made and the image of the earlier commit, which nothing runs any more. That
   commit is not built again, so the pull request has no preview until a new
   commit is pushed. Both states are `build-failed`.
7. The pull request is told, at the top of its own description, and
   `~/tehas/live/previews.json` is written for the dashboard and for
   [tehasbot health](/tools-health#tehasbot-health).

Each pull request has a state, in `~/tehas/preview/<pr>/state.json`:

| State | Meaning | Tried again |
| --- | --- | --- |
| `serving` | A container of that commit was started and was still running five seconds later, without a restart. It is not asked whether it answers | Not needed |
| `building` | The build is under way | |
| `no-recipe` | The commit has no `preview/Dockerfile`. A preview of an earlier commit is removed | At a new commit |
| `build-failed` | The build failed or ran over 15 minutes, or the recipe declares a volume outside `/tmp`, `/config` and `/data`. A preview of an earlier commit keeps serving | At a new commit, not for the same one |
| `build-failed` | The container started and stopped within five seconds. It was removed, with the image its build made and the image of the earlier commit. Nothing serves for that pull request, not the earlier commit either | At a new commit, not for the same one |
| `build-failed` | The container was never created. Nothing serves for that pull request, not the earlier commit either, until a commit starts | At each of the next three runs, then at a new commit |
| `no-space` | Less than `TEHAS_PREVIEW_MIN_FREE` is free, or the free space could not be read. What serves stays, and removal still runs | At every run, until there is space |
| `over-limit` | More than `TEHAS_PREVIEW_MAX` pull requests are open and this is not one of the newest. A preview it had is removed | At every run |

A commit that could not be fetched gets no new state. The log says
`#<pr>: the commit could not be fetched, tried again at the next run`, and it
is. A new commit is always tried.

A build's output is in `~/tehas/logs/preview-<pr>.log`, written anew at each
build.

**What a preview's container is given.** Nothing of the host: no mount, no
variable, no published port, no token. A read-only root with `/tmp`, `/config`
and `/data` in memory, every capability dropped but the one to bind a port,
256 MB, one CPU, 128 processes, and the network `tehas-previews` only. That
network has the router and the other previews on it, and neither the internet
nor the host. What it prints is kept by Docker in at most two files of 10 MB
(`--log-opt max-size=10m --log-opt max-file=2`).
`test/preview_build_test.sh` pins these arguments.

It writes nothing else to the host's disk. A `VOLUME` line in a recipe would
have Docker make a volume there, which a read-only root does not stop and the
memory limit does not count. So an image that declares a volume anywhere but
`/tmp`, `/config` and `/data` is not run, and every container is removed
with `docker rm -f -v`, which takes a volume Docker made for it along.

**The notice in the description.** The run writes it as it goes. The preview is said at the top of the pull
request's own description, between the marker lines `<!-- tehas-preview -->`
and `<!-- /tehas-preview -->`: the address when the preview serves, or
"Unable to create a preview" with the reason for `build-failed` and
`no-space`. It is said once for each commit and state, so a full disk is not an
edit every five minutes. For `over-limit` and `no-recipe` a pull request whose
description has no notice gets none. One that has a notice from an earlier
state has it changed to "There is no preview" with the reason, so that it does
not go on giving an address that answers 404. `building` is on the dashboard
only.

**Which block belongs to the bot.** The one at the very top of the
description: its opening marker as the first line, and the closing marker under
it. It is the bot's only when everything between those two lines is one of the
notices this command writes: a line beginning `Preview of \``, `Unable to create
a preview` or `There is no preview`, and for a preview that was made or
attempted, the sentence `It is rebuilt when the branch changes and removed when
this pull request is closed.` after a blank line. A notice is written with a
blank line under its opening marker, so that it renders as text and not as part
of the HTML comment, and one is allowed there and nowhere else. All of it or
none of it. Text
between marker lines that this command did not write is not written over, and
neither is a marker line anywhere else in the description: the log says
`#<pr>: the description has a preview notice that is not this one, left alone`,
once for each commit and state. Write your own text outside the two marker
lines. To hand the block back after changing it, remove both marker lines: the
notice comes at the run that sees the change.
Everything after the closing marker line of a block of the bot's is kept as
GitHub gave it, blank lines at the end included.

**What a notice is proof of.** It is proof of what this host decided, read from
`~/tehas/live/previews.json`, the dashboard and `/health`. Anyone can write the
marker lines and the sentence under them, and for a pull request this host does
not build, one from a fork or one opened by someone outside the repository,
nothing reads the description at all. An address at the top of such a
description is the author's own text.

**One record for each commit and state.** What was said about a commit and state
is on record in `~/tehas/preview/<pr>/.notice`, as `<commit> <state> <what>
[attempts] [time]`: `none`, `said`, `refused`, `failed` with how many writes
GitHub did not take, or `gaveup`. The time is the one GitHub gives for that pull
request at the last look. Nothing is written or said twice about a commit and
state: a full disk is not an edit every five minutes, and the log carries one
line for each write GitHub did not take, the last of them saying it is left to
the next commit or state.

The record does not stop the reading. The description is read again whenever
GitHub gives a later time for the pull request than the one on record, which is
what editing a description does. A notice a person removes therefore comes back
at the next run, and so is a block someone wrote at the top in the shape of the
bot's: ownership of a description is only its text, so it is looked at again
whenever the text can have changed. A look that finds nothing to do costs one
read, writes nothing and says nothing. A record with no time on it is looked at
again too.

A call that
failed is tried at the next run: `#<pr>: the description could not be read (no
such pull request (HTTP 404)), tried again at the next run`, with
`GitHub refused the call (HTTP 403)` when GitHub refuses the token, and
`#<pr>: the answer about this pull request was not understood ...` when it
answered with something that is not that pull request's description. Three
writes GitHub did not take are said once, with the third of them, as
`#<pr>: the notice was not written three times, left to the next commit or
state`. A description that already says what
it should say is not written at all.

The description is asked for at the moment of the edit, with `gh api
repos/<repo>/pulls/<pr>`, and not from the list of open pull requests the run
was given minutes ago: an edit made before that read is kept, and so is what
GitHub says about the pull request itself. One merged or closed in between gets
nothing written, and the log says `#<pr>: the pull request is no longer open,
nothing was written`. GitHub takes no condition on a write of a description, so
an edit made in the moment between the read and the write is not kept, and the
later write wins: [The host](/host) says an agent in a cycle may edit a
description with the same token. What the command read is
written to `~/tehas/preview/<pr>/.description`, mode `0600`, before it writes,
and the log says `#<pr>: the notice was written`, so a description written over
is visible on the host and can be copied back from there.

**What one pull request leaves on the host.** Under `~/tehas/preview/<pr>/`:
`state.json` (the state, above), `pull.json` (its title and address), `.retry`
(a commit whose container did not start, and how many starts it had), `.notice`
(what was said about a commit and state) and `.description` (what GitHub had as
the description before the last write). All of it goes with the pull request,
at the next run after it is merged or closed.

**What is not removed.** A notice in a description stays when its pull request
closes: nothing of previews edits the description of a merged or closed pull
request, so a merged one keeps an address that answers 404 from then on. So does
a comment written by the version of this command that commented instead (until
2026-10-11): nothing reads or removes comments now, and those comments are left
where they are.

**The sweep**, every three hours, is the second check that nothing of a merged
or closed pull request stays on the host. It does not work from GitHub's list
of open pull requests. It starts from what it finds on the host: containers
and images with the label `tehas.preview`, fetched commits and state
directories. It waits up to 30 minutes for a run to end, and no run starts
while it works.

- A container with the label that belongs to no pull request of this
  repository is removed, and so is an image with the label that is not tagged
  `tehas-preview:<pr>` or is another repository's. Docker refuses to remove
  an image that a container runs.
- It then asks GitHub for the repository itself. If GitHub does not show it,
  nothing more is removed, and the log says `GitHub does not show <repo>;
  nothing was swept by its word`. A token that lost its access gets a 404 for
  every pull request, and that must not remove every preview.
- It asks about each pull request by itself. Only four answers remove: a
  clear `closed`, a 404, an open pull request whose head is outside the
  repository, or an open one that the answer says was opened by someone who
  is not an owner, member or collaborator (`#<pr>: swept, it was not opened
  by someone of <repo>`). An open one whose answer does not say who opened
  it is kept. Every container that carries the pull request's label is then
  removed by its id, with the image, the fetched commit, the state and the
  build log. When Docker did not remove one of them, the rest stays as well,
  and the log says `#<pr>: a container could not be removed, tried again at
  the next sweep`. Any other answer keeps everything: `#<pr>: no answer from
  GitHub, kept`.
- For a pull request that is open and ours, a container of its number that
  has a name, none of which is `tehas-preview-<pr>`, is removed.
- A container is removed by the id Docker listed it with. A line of the
  listing whose id is not one is left alone: `an id that is not one,
  skipped`.

A line with `swept` in the log is something the five minute run missed. The
time of the last sweep and how many things it removed are `lastSweep` in
`~/tehas/live/previews.json`.

| Exit | Meaning |
| ---- | ------- |
| 0    | Done. Also when GitHub or Docker did not answer and what runs was left running, and when another run holds the lock |
| 1    | It cannot write under `~/tehas` |
| 64   | A wrong argument |
| 78   | The instance settings are invalid, `TEHAS_PREVIEWS` is not `1`, or the console is not installed beside the command (run `install.sh`) |

The [plain dashboard](/dashboard) lists the previews under "Previews", each
with its address, or with its state and the reason the run wrote into its state
file - which is what tells a recipe whose volume was refused from a build that
failed - and says when the list was written. A time that is more than a few
minutes old means the runs have stopped: see the timer. A host without previews
has no `previews.json` and no such section.
