# Changelog

What each release of tehas changes, and what an adopter must do after
updating to it. Newest first. A release's section is `## vX.Y.Z (YYYY-MM-DD)`
with `### Changes` and `### Adopter action`; "None." is a valid action.
`scripts/tag-release.sh` refuses to tag without an Adopter action, and the
release workflow takes the release's page from its section
(`scripts/release-notes.sh`). Work in progress goes under `## Unreleased`; a
person renames it to the version in the release's pull request and starts a new
empty one above it.

## Unreleased

### Changes

- None yet.

### Adopter action

- None.

## v0.12.0 (2026-10-24)

### Changes

- The state store's schema is version 2. `install.sh` migrates a version 1
  `state/tehas.db` in place, after its backup, and keeps every row: the step
  only adds columns and one table. The store gains the verbs `history-show`,
  `park-told` and `plan-note`, and `approval-record --input-file` and
  `plan-answer --text-file`.
- The analytics program no longer reads a cycle's history, approval, settings
  and commit files, or a planning record, itself: `analytics run` and
  `analytics plan` take them as facts in a file their caller writes
  (`--facts <file>`). The rows are the same. The file is
  `tmp/analytics-facts.*`; `tehasbot clean` removes one a killed call left.
- A cycle's records are in the state store, no longer files of
  `state/cycles/`: the approval the dispatcher records before the label (in
  place of `<n>.approved.json`), the cycle's start and its account (`.taken`,
  `.by`, `.requeued`, `.rukas`), the image and settings `tehas work` saw as the
  container started (`.image`, `.settings`), the end with its branch (`.branch`)
  and one line of history per end (`<n>.history.jsonl`). An approval starts one
  cycle. A cycle started by `tehas-cycle@.service` needs the dispatcher's
  approval and exits 77 without one, 75 when the store stays busy and 78 when
  it cannot be used, before any label or comment; one typed by hand is on
  record as `hand`. `tehas work` exits 4 when the approval of its start cannot
  be read, and never takes that for a run by hand. `tehas stop` ends the record
  of a cycle killed outright. `tehas dispatch`, `tehasbot cycle`, `tehas stop`,
  `tehas history` and `tehas export` exit 78 when the store is not installed.
  `tehas history` lists the cycles from the store.
- `host/systemd/tehas-cycle@.service` sets `TEHAS_CYCLE_FROM=unit`.
- A park and its question are in the state store, no longer
  `state/cycles/<n>.park` and `state/park-<n>.id`. `tehasbot cycle` puts the
  park on record before it posts, and asks no question (the plain comment,
  no `tehas:question`) when the store does not take it. A tick counts only in
  the question of the issue's current park; a store that cannot be read skips
  the issue for that poll, and never unticks. A start takes the park's
  question off the record and spends its one retry after an outage before the
  issue is marked, and puts the question back when the mark fails. The retry
  after an outage is owed only to a cycle started on an approval, rests on the
  account that started it, and is spent by `tehas stop`; a cycle started by
  hand says it waits for a person. The hold of a relabelled start on an
  unchanged body is told once per park through the store.
- The poll keeps a stuck cycle's episode and its asks to stop in the state
  store, no longer in `state/cycles/<n>.quiet`. The timing and the log lines
  are as before. When the store is busy and cannot record the episode, the
  poll asks nothing on that poll and says so; a store that cannot be used ends
  the poll with exit 78. A cycle whose record has ended, or that has none, is
  not stopped by the poll, which says so on every poll for a person. A dry run
  says what the poll would do about each quiet cycle. `tehas stop` no longer
  removes a `.quiet` file.
- A planning session is in the state store, no longer `state/plan-<n>.json`
  with its `.brief`, `.answer` and `.turn`: the session, its brief, each
  answer with its text, the issues it filed, its open question, the comments
  the poll spoke about once, and one line of history per turn (in place of
  `state/cycles/plan-<n>.history.jsonl`). Each change is one write. What the
  runner carries between turns moves out of `state/` to
  `work/<name>-plan-<n>.keep`, beside the workspace; `.out`, `.rc` and `.lock`
  keep their names. A turn's end the store would not take keeps the turn's
  exit status, and the next poll posts it once. A session pi-rukas lost ends
  with the outcome `lost`. An answer that begins a turn answers its question on
  record, so a second tick in it runs nothing. The poll reads the open
  sessions once: an issue with one open starts no cycle, no refresh and no
  outage retry; a list that cannot be read ends the poll (1 when busy, 78 when
  the store cannot be used). `tehas history` lists the planning turns from the
  store, with `started` and `status`, which it left out before, and `label` on
  every line. `tehas export`'s `plan.json` is the session as the store has it
  (`plan-show`), a new shape. `tehasbot health` exits 1 or 78, writing nothing,
  when the store cannot list the sessions.
- `install.sh` imports the records an older tehas kept as files under
  `state/` (the cycle's, the park question ids, the planning sessions) into the
  store with `store import`, and moves each file to
  `state/imported-<YYYYMMDD>/` once its rows read back. Nothing is deleted.
  It holds `state/dispatch.lock` from before its first copy until after the
  import, and stops before the first copy, installing nothing, while a cycle or
  planning turn runs and such files are there, or when a file cannot be
  imported. Each `state/plan-<n>.json.keep` moves to
  `$TEHAS_WORK_DIR-plan-<n>.keep`.
- `install.sh` no longer leaves the commands of the earlier layout for a later
  run while a poll holds the dispatcher's lock: it waits for the lock instead,
  up to `TEHAS_DISPATCH_LOCK_WAIT` seconds (900 by default).
- A poll that finds a record file of an older tehas in `state/` (one the
  import did not take, or one a cycle that ran the old code across the update
  left) stops with exit 78 before it calls GitHub, and logs `ERROR: records of
  an older tehas at <path>; run install.sh again`. `install.sh` run again
  imports it.

### Adopter action

- Before updating, drain: `tehas drain --wait 3600`, and let planning turns
  end. `install.sh` refuses while a cycle or planning turn runs and old records
  are there. After the update, `tehas limit clear`. Remove
  `~/tehas/state/imported-<YYYYMMDD>/` once satisfied. If the poll logs
  `records of an older tehas`, run `install.sh` again.
- A cycle or poll that was running keeps the commands it started with, and
  those call `analytics run` and `analytics plan` without `--facts`: the new
  program refuses that with exit 64 (`WARN analytics run: exit 64` in the
  cycle's log), so that run or planning turn is missing from the analytics
  report. Nothing else about the run changes.
- Run `install.sh` with its units step, so the cycle unit sets
  `TEHAS_CYCLE_FROM=unit`; a unit file not refreshed is still known by
  systemd's `INVOCATION_ID`.

## v0.11.1 (2026-10-24)

The first published release of v0.11. The v0.11.0 tag has no release: its
build failed on the aarch64 tarball. This release is v0.11.0 without it; see
v0.11.0 below for what changed since v0.10.6.

### Changes

- A release has only the `linux-x86_64` tarball again. The aarch64 cross
  build could not link, and no aarch64 host can run tehas yet.

### Adopter action

- As for v0.11.0.

## v0.11.0 (2026-10-24)

From the default branch. It carries the GitHub App work of v0.10.6 and the
state store, which nothing reads yet. The rest of epic #244 comes in a later
release.

### Changes

- A cycle's container may use every CPU of the host but one, not a fixed
  share. `--cpu-shares` splits them when several gates run at once. A
  resource change of the container: its mounts, environment, memory and pids
  limits do not change.
- The conflict check runs every `TEHAS_REFRESH_EVERY` minutes (1 to 1440,
  default 10). `tehas-refresh.timer` fires every minute and `tehasbot
  refresh` returns at once when its last run is younger than that.
- Refreshes run in their own lane of `TEHAS_REFRESH_SLOTS` (0 to 8, default
  1) and no longer count against `TEHAS_TICKET_CONCURRENCY`. The lane keeps
  a slice slot, so a 4-slot host runs 3 cycles and 1 refresh. `0` keeps the
  old sharing.
- The conflict check also refreshes a pull request handed off with
  `needs-human-attention`, and the refresh clears the handoff.
- `tehas tidy` adds `tehas:shipped` when a merged pull request closed the
  issue, not only for the newest closed issues.
- When free disk falls under `TEHAS_CLEAN_MIN_FREE` (default 30g), `tehasbot
  clean` removes kept workspaces' build output and Docker's build cache at
  once instead of waiting for their age.
- A cycle ended by an abort gets an account in its `err.txt`, as a cycle
  ended at a cap does, instead of an empty file.
- The installer and `tehas update` fetch the release asset for the machine's
  architecture (`uname -m`), and stop before downloading on one they do not
  know.
- `tehas setup` takes a GitHub App in place of a token: `--app-id`,
  `--app-installation` and `--app-key-file`, all three or none. It checks the
  app's permissions and that it reaches the repository before it writes
  anything, then installs the key and writes the three settings.
- Fewer processes per command, for the same results. Loading the settings
  checks each value in the shell, the commands read the clock from the shell,
  and `install.sh` leaves the record of steps owed as it is when a copy adds
  nothing to it. The tests of `install.sh`, the status, the update, planning
  and the cycle record fork 15 to 44% fewer processes.
- `install.sh` builds and installs the state store, `~/tehas/libexec/store`,
  and gives it its database: `store init` makes `~/tehas/state/tehas.db` when
  there is none, and every later install runs `store migrate`, which first
  copies the database into `state/backup/`. `state/` is kept mode 700. A
  store that fails stops the install with exit 1. Nothing reads the database
  yet.
- A release carries the store and a `COMMIT` file with the commit it was
  built from. `install.sh` refuses a tree whose commit it cannot read.
- `tehasbot top --every` reads the clock from bash instead of starting
  `date` between snapshots. A `date` stopped mid-write printed "Broken pipe"
  into the unit's log, and made a test fail now and then.
- The store has the verbs for the cycle, park, question, approval and
  planning records (`store cycle-start`, `park`, `plan-begin` and the rest).
  Nothing calls them yet; the shell commands move to them in later releases.
- The store waits its turn under load: a writer asks for the lock every
  millisecond for up to 5 s, and a reader waits up to 1 s. Tests prove it
  under 8 writers and 8 readers, and with a writer killed mid-transaction.
- `tehas update --pending` runs the install steps an update recorded, for a
  host whose platform checkout cannot move.
- `install.sh` records `tehas labels` as an update step when the labels
  program changes.
- A fresh cycle does not start on an issue whose last cycle parked on the
  same issue text.
- The improvement loop can skip findings about pi-rukas
  (`TEHAS_IMPROVE_RUKAS_SKIP`).
- The name lint checks file names too, and reads them NUL-separated.

### Adopter action

- None. The first install after this release makes the database. Old copies
  in `~/tehas/state/backup/` are not removed; remove them by hand when there
  are too many.
- With `TEHAS_REFRESH_CONFLICTS` on, the refresh lane takes one cycle slot
  from new cycles. Set `TEHAS_REFRESH_SLOTS=0` to keep the old sharing.

## v0.10.6 (2026-10-24)

A patch release from v0.10.5, not from the default branch: it carries the
GitHub App work (#639) and nothing of epic #244.

### Changes

- A factory can act as a GitHub App instead of a machine user.
  `TEHAS_GITHUB_APP_ID`, `TEHAS_GITHUB_APP_INSTALLATION` and `TEHAS_BOT_LOGIN`
  name it, all three or none; the key is `~/.config/tehas/github-app.pem`.
  `tehasbot github-token` makes and keeps its one-hour installation token, and
  `--check` says whether it reaches the repository.
- With an app named, the host's `gh` and git act as the app through
  `~/tehas/shim/gh`, a user `GH_TOKEN` is dropped, and the dispatcher, the
  refresh and the Protolab take the bot's login from `TEHAS_BOT_LOGIN`.
- A cycle's container reads the app's token from `~/tehas/state/github`
  (read-only, never the key) through the image's own `gh`, and `tehas work`
  keeps the token fresh while the cycle runs. A security change: the container
  is given one more read-only directory.

### Adopter action

- None for a factory that keeps its machine user: without the three settings
  nothing changes. The sandbox image is rebuilt by the update.
- To move to an app: make one as `docs/specs/2026-10-24-github-app.md` says
  (under "For adopters"), put its key at `~/.config/tehas/github-app.pem` with
  mode 600, add the three settings to the instance, set git's `user.name` and
  `user.email` for the tehas user to the app's (`<slug>[bot]` and
  `<id>+<slug>[bot]@users.noreply.github.com`), and run
  `tehasbot github-token --check`.

## v0.10.5 (2026-10-23)

### Changes

- The dispatcher starts no cycle while the disk of `~/tehas`, or the one
  Docker keeps its images on, has less than `TEHAS_CYCLE_MIN_FREE` free (10g
  by default). Running cycles and planning turns are not held.
- `tehas stop` leaves a marker, and the cycle's record names who stopped it
  and when.
- The workspace sweep also removes what a cycle left outside its workspace:
  the agents' transcripts, the console file and the planning session's
  transcripts of that issue, on the same 7 day rule.
- A second `tehas update` waits for a running one, `TEHAS_UPDATE_LOCK_WAIT`
  seconds (10 by default), before it says another update is running.
- `tehasbot clean` prints what it logs, so a timer run's output is in
  `journalctl --user -u tehas-clean` too. Each line was written to
  `clean.log` twice.
- The front page's Shipped figure counts every closed issue with
  `tehas:shipped`, not the fifty newest.
- `tehas_headless_mode` answers `error` for an answer it cannot read, never
  `polyfill`.
- The console's state store backs itself up before it migrates.
- `install.sh` starts fewer processes for each file it copies, so an install
  or `tehas update` takes about half the time it did. What it installs and the
  follow-ups it records are unchanged.

### Adopter action

- A host with less than 10 GiB free starts no new cycle after the update. Free
  space, or set `TEHAS_CYCLE_MIN_FREE` in the instance's `instance.env`.

## v0.10.4 (2026-10-23)

### Changes

- `tehasbot clean` lists the base image's repository and the sandbox image in
  two calls of `docker image ls`, which takes at most one name. In v0.10.3 it
  asked both in one call, docker refused, and the clean ended with exit 1
  having removed no old base image.
- The head of the default branch has a preview of its own at
  `uat-main.<domain>`, built from its `preview/Dockerfile` like a pull
  request's and listed first under Previews as `main`. It does not count
  against `TEHAS_PREVIEW_MAX` and nothing removes it.
- `tehas dev` keeps a session's cargo build output in
  `~/tehas/dev/<name>.state/target`, so a later session of the same clone does
  not rebuild from scratch. The cleaner does not touch it.
- `install.sh` installs every `lib/polyfill-*.sh` into `~/tehas/lib/`.
- A sandbox rebuild is asked for only when `versions.env` changes in a line
  the image build reads, not for any change to the file.
- `tehas update --check` on a host installed from a checkout lists the
  releases since the newest tag at or before the checkout's HEAD.
- `tehas api` reports the release tag of a release install, where it said no
  release.
- The console's state store has its first schema and migrations. Nothing reads
  it yet.
- `tehas update` prints `later: <checkout>/bootstrap.sh user` when the sandbox
  image was built on a pi-rukas commit the host's clone is not on, even when
  `versions.env` did not change. Until then `install.sh --check` fails on
  pi-rukas.
- When a test file fails, the gate's summary line names the first failing case
  as well as the file.

### Adopter action

- If `tehas update` prints the `bootstrap.sh user` line, run it as the tehas
  user. A host that followed pi-rukas to a new release since its last
  bootstrap needs it once.
- A proxy in front of the edge router has to pass the name `uat-main` as well
  as `uat-<pr>`. Without a `preview/Dockerfile` on the default branch nothing
  is built.
- After `tehas update`, the first clean removes the old base images v0.10.3
  left behind.

## v0.10.3 (2026-10-22)

### Changes

- `tehasbot clean` removes every old `sha-` tag of the base image's
  repository, not only the one the pin names, so each `tehas update` no longer
  leaves another base image behind. The current base, a base a container was
  made from and a base a kept sandbox tag stands on are kept.
- A `tehas:refine` planning turn is told that the issue it discusses is the
  one its plan replaces, so it no longer parks itself at its first turn as a
  duplicate.
- `tehas update` gives its lock only to `install.sh`. A child that outlived the
  update kept the lock, and the next update said another update was running.

### Adopter action

- None. The first `tehasbot clean` after the update removes the old base
  images that earlier updates left behind, several GB each.

## v0.10.2 (2026-10-22)

### Changes

- `tehas check-product --sandbox` runs the gates on a work branch one empty
  commit past the default branch, as a cycle's gate runs on its own branch. A
  gate that refuses to certify a diff of HEAD against itself, such as a
  file-size ratchet, no longer fails `tehas setup` at the default branch's tip.
- An entry of a zip linked in an issue reaches a cycle as an image only when
  both its bytes and its extension say it is one. A file named `x.html` that
  holds PNG bytes is given as the HTML file it is named.
- `tehas:refine` and `tehas:plan` start a planning session on a parked issue,
  one that carries `tehas:question`, as `tehas:ready` starts a cycle on one.
  The planning queue dropped such an issue without a word.
- The MCP server's tests wait for the log lines they check, so they no longer
  fail now and then on a loaded machine.

### Adopter action

- Run `tehas update` on a host whose `tehas setup` stopped at the sandbox step
  with "compared nothing" or another complaint of a gate about its base, then
  `tehas setup` again.

## v0.10.1 (2026-10-22)

### Changes

- `tehas analytics report` says how issues of each size fare: the starts
  measured in the window from `state/sizes.jsonl`, in the buckets 1-2, 3-4 and
  5+ named files, with how many of each shipped and their median lead time, so
  the threshold of `TEHAS_SIZE_MAX_FILES` can be set from outcomes.
- Both state ledgers the report reads are now bounded the same way, and both
  writers trim: `state/refined.jsonl` keeps its last 500 lines as
  `state/sizes.jsonl` does, the readers take a plain file of at most 1 MiB, and
  a stamp of a year past 2100 is refused before it can overflow the date
  arithmetic.
- `tehas status` writes `live/status.json` on a repository with many issues.
  The list of every issue went to `jq` as one argument, which Linux caps at
  128 KiB, so `jq` failed unseen and the status said the dispatcher had written
  no snapshot. The lists now go in as files, and a snapshot that cannot be
  written says why on standard error. The health file reads a big snapshot,
  and the protolab long comment threads and many tickets, the same way.
- `tehas check-product --sandbox` clones the product as a cycle's workspace is
  made: the default branch checked out, with `origin/<default branch>` and
  `origin/HEAD` set. A gate that diffs against `origin/main` no longer fails
  `tehas setup` for want of a base.
- `tehas status` says a release is out only when the newest is greater than
  the one installed. A cached newest older than the host's release is dropped,
  and the next poll asks GitHub again.
- Every sandbox container (a cycle, a planning turn, a dev session, the push
  check) has a `/tmp` that is a size-bounded tmpfs, so a runaway log ends that
  container at its memory ceiling instead of filling the host's disk.
- The gate, `scripts/verify.sh`, needs cargo and runs the tests of the whole
  workspace. The front page's node tests run where node is installed.

### Adopter action

- Run `tehas update` on a host that shows "No queue snapshot yet" with a long
  issue list, or whose `tehas setup` failed at the sandbox step on a base
  branch.

## v0.10.0 (2026-10-22)

### Changes

- The cleaner removes images too: the sandbox image's dated tags older than
  `TEHAS_CLEAN_AFTER` hours, keeping the newest and any a container or the
  `uid` tag holds, and the pulled base image of that age, by its own name. No
  volume is removed, no image a container was made from, and no preview image.
- A zip linked in an issue reaches the cycle with `TEHAS_WORK_ZIPS=1`, which
  needs `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: at most 2 archives of
  4 MB each, every file in them checked as a linked file is.
- With `TEHAS_SIZE_MAX_FILES` set, the dispatcher holds a `tehas:ready` issue
  whose approved body names more files than that: the label comes off,
  `needs-human-attention` goes on, one comment says why. Unset, nothing changes.
- A cycle's start records the size of its approved body in
  `~/tehas/state/sizes.jsonl`; `TEHAS_SIZE_MAX_BYTES` and
  `TEHAS_SIZE_MAX_LINES` bound the measure. Nothing reads the ledger yet.
- `tehas setup` gives the host's git the gh credential helper for github.com,
  so a private product is reachable by plain git.
- `install.sh` builds the programs with cargo `--offline` first and fetches
  only when a crate is missing from the cache.
- The MCP server's live-file tools (`factory_status`, `factory_issues`,
  `factory_endings`, `factory_console`, `factory_stats`) describe their output
  and every input in `tools/list`.
- The analytics program opens a new database safely from several processes at
  once.
- The console, the analytics program and the MCP server are one cargo
  workspace.
- tehas can carry a pi-rukas feature that has no release yet as a polyfill,
  `lib/polyfill-<feature>.sh`, used only while the sandbox image's
  `--capabilities` lacks the feature. The tag script removes a polyfill the
  release's pi-rukas no longer needs, in a pull request of its own. None is
  carried yet.
- A cycle whose runner never came up ends with its own cap,
  `runner-never-started`, naming the container's exit status.
- On a host that runs a release, `tehas status` names the installed release
  and whether a newer one is published, instead of the platform checkout.

### Adopter action

- The cleaner now removes old sandbox tags and the pulled base image. A host
  that disabled `tehas-clean.timer` while waiting for this can enable it
  again: `systemctl --user enable --now tehas-clean.timer`. A dated tag older
  than `TEHAS_CLEAN_AFTER` hours (24 by default) is no longer there to roll
  back to; the `release-<version>` tags stay.
- `TEHAS_WORK_ZIPS`, `TEHAS_SIZE_MAX_FILES`, `TEHAS_SIZE_MAX_BYTES` and
  `TEHAS_SIZE_MAX_LINES` are new optional settings; see
  `instances/example/optional.env` and the manual. Nothing changes until set.

## v0.9.0 (2026-10-21)

### Changes

- tehas is installed from a release: a signed tarball (SHA256 and minisign)
  published on GitHub for each tag, unpacked side by side under
  `~/tehas/versions/<version>` with `~/tehas/current` pointing at one.
- `tehas update` installs releases: `--check` shows the installed and newest
  version and the notes between them, `--to vX.Y.Z` pins or rolls back.
- pi-rukas follows the newest tag of a major version, `PI_RUKAS_FOLLOW=release:0`
  before 1.0, so a new pi-rukas arrives with `tehas update`. An instance may set
  `PI_RUKAS_FOLLOW` itself, down to one commit.
- One root command prepares a host, `tehas setup` writes the instance, and
  `tehas check-product` and `tehas init-product` check and scaffold the
  product's files.
- The instance names the session model once, in `instance.env`. Its
  `settings.json` is optional, and a `models.json` entry needs only `id`,
  `baseUrl`, `api` and `apiKey`. A product's `.pi/settings.json` is optional.
- A first install puts the `env.sh` line in `~/.bashrc`, makes the labels and
  enables the core units. A cycle's clone keeps pi-rukas's state files out of
  git by `.git/info/exclude`.
- The MCP server shows a key holder the sessions of the last 30 days, why they
  failed and their masked logs (`factory_failures`, `factory_sessions`,
  `factory_session`, `factory_log`), and its handshake says which tools to call.
- `tehas:refine` is counted: how much it is used and what became of its issues.
  A refined issue's title starts with its conventional-commit type.
- A beesknees26 theme for the front page.
- The gate does not run a fork's pull request on the self-hosted runner.

### Adopter action

- A host installed from a git checkout keeps working from it. To move it to
  releases, follow "Move a checkout host to releases" in the manual
  (`host/docs/host.md`); until then `tehas update` updates the checkout as
  before.
- Read the new settings in `instances/example/` and compare your instance
  with them; `install.sh --check` names any that are missing.
- An MCP key now reads 30 days of sessions and their logs, not only what the
  dashboard shows. On a factory that works on a private repository that
  includes code, issue text and model output: check who holds a key.
