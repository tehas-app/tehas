# Changelog

What each release of tehas changes, and what an adopter must do after
updating to it. Newest first. A release's section is `## vX.Y.Z (YYYY-MM-DD)`
with `### Changes` and `### Adopter action`; "None." is a valid action.
`scripts/tag-release.sh` refuses to tag without an Adopter action, and the
release workflow takes the release's page from its section
(`scripts/release-notes.sh`). Work in progress goes under `## Unreleased

### Changes

- None yet.

### Adopter action

- None.

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
