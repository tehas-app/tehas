### Changes

- A release's page on GitHub no longer holds every change since the last release. A release `vX.Y.Z` is published with its own section of `CHANGELOG.md`, and the release workflow publishes nothing when it has none, where it used to take the whole of `Unreleased`. A prerelease without a section of its own is published with what is new since the tag before it, under a first line `New since <tag>.`. `scripts/release-notes.sh` runs `xtask release-notes`.
- `scripts/tag-release.sh` names a release's section itself: for a release whose head has no `## vX.Y.Z` section, its changelog pull request (`changelog-<tag>`) renames `## Unreleased` to `## vX.Y.Z (YYYY-MM-DD)` with a new empty `## Unreleased` above it, with or without changelog fragments. A prerelease whose tag before has no `Unreleased` section is not tagged.
- `CHANGELOG.md` has the sections of v0.31.0 and v0.32.0, which were under `Unreleased`, and `Unreleased` holds only the changes since v0.32.0.

### Adopter action

None.
