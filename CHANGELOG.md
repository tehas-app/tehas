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

- `tehasbot clean` lists the base image's repository and the sandbox image in
  two calls of `docker image ls`, which takes at most one name. In v0.10.3 it
  asked both in one call, docker refused, and the clean ended with exit 1
  having removed no old base image.

### Adopter action

- None.

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
