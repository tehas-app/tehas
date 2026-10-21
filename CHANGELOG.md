# Changelog

What each release of tehas changes, and what an adopter must do after
updating to it. Newest first. A release's section is `## vX.Y.Z (YYYY-MM-DD)`
with `### Changes` and `### Adopter action`; "None." is a valid action.
`scripts/tag-release.sh` refuses to tag without an Adopter action, and the
release workflow takes the release's page from its section
(`scripts/release-notes.sh`). Work in progress goes under `## Unreleased`; a
person renames it to the version in the release's pull request.

## Unreleased

### Changes

- None yet.

### Adopter action

- None.

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
