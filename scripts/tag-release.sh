#!/usr/bin/env bash
# Tag the head of the remote's default branch as the next release.
#
# Finds the newest vX.Y.Z tag and, when it is not already on the branch head,
# runs the gate on the head and, when it passes, tags the head with the next
# version and pushes that tag. Nothing else is pushed. What it tags is the
# remote's commit, not this checkout's HEAD, so it can run from any branch or
# worktree: the gate runs in a temporary worktree at that commit.
#
# The commit it tags must have a CHANGELOG.md whose section for the new tag,
# or else its Unreleased section, has an "### Adopter action" heading with at
# least one line under it. Only the move of the pinned tools below edits
# CHANGELOG.md: a person renames Unreleased to the version in the release's
# pull request, or leaves it.
#
# The tag is annotated: "Release <tag> - <message>", the message being the
# subject of the head commit unless --message gives one.
#
# A release carries no polyfill the pi-rukas it builds on no longer needs
# (scripts/polyfills.sh). When the head carries polyfills, the script asks that
# pi-rukas for its --capabilities: the commit host/versions.env follows, in the
# image upstream publishes for it, as `console rukas` of the head names them
# (built with cargo, or TEHAS_CONSOLE when set). When the answer has the feature of one of
# them, it removes those polyfills on a branch remove-polyfills-<tag>, runs the
# gate there, pushes the branch and opens a pull request, and tags nothing: the
# tag waits for a person to merge it. An open pull request from such a branch
# holds the tag the same way.
#
# Each polyfill it keeps must still stand in for work upstream is doing: the
# issue and the pull request its header names must exist on GitHub, and the
# pull request must not be closed unmerged. Otherwise it tags nothing, since a
# polyfill nobody upstream will replace is tehas's own code under another name.
# A merged pull request whose feature the answer still lacks is said and kept:
# it is not released yet, or came under another name.
#
# Before it tags, the pinned tools move to the newest release of the major
# version each has (scripts/pins.sh, console pins): every pin in
# host/versions.env, host/npm/ and host/sandbox/npm/, with its checksum, digest
# or lockfile, and the release's section of CHANGELOG.md names each move and
# each later major held back for a person. When anything moved, it runs the
# gate there, pushes a branch pins-<tag>, opens a pull request, and tags
# nothing: verify.yml runs on that commit, and the tag waits for a person to
# merge it. An open pull request from such a branch holds the tag the same
# way. A move that fails stops the release. A head without scripts/pins.sh
# moves nothing.
#
# The tag is the only line on stdout, so a script can take it:
#   tag=$(scripts/tag-release.sh) || exit
#
# The work is `xtask tag-release` (console/xtask/src/tag.rs), built with cargo
# from the workspace beside this script, or the build TEHAS_XTASK names. It
# still runs git, gh, docker and the head's scripts for the steps above.
#
# Usage: scripts/tag-release.sh [--patch|--minor|--major] [--message <text>]
#                               [--no-verify] [--dry-run] [--remote <name>]
#                               [--rukas-caps <file>] [--no-pins]
#   --patch      the default: v0.7.6 becomes v0.7.7
#   --minor      v0.7.6 becomes v0.8.0
#   --major      v0.7.6 becomes v1.0.0
#   --message    the tag's message instead of the head's subject
#   --no-verify  skip scripts/verify.sh
#   --dry-run    check CHANGELOG.md and print the tag it would make; make none,
#                run no gate and check no polyfill
#   --remote     the remote to read and push to, default origin
#   --rukas-caps the --capabilities answer of that pi-rukas, from a file,
#                instead of asking its image
#   --no-pins    tag without moving the pinned tools
#
# Exit: 0 tagged, or the head already carries the newest tag; 1 git, the
#       remote, GitHub or docker failed, the newest tag is not on the branch,
#       CHANGELOG.md has no Adopter action for the release, a polyfill it keeps
#       has no live upstream issue and pull request, a pinned tool could not be
#       moved, or the gate failed; 2 nothing tagged: a pull request, named on
#       stderr, removes polyfills or moves the pinned tools first; 64 a bad
#       argument; another status when cargo cannot build the task
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 1
xtask=(cargo run --quiet --locked --manifest-path "$here/console/Cargo.toml" -p tehas-xtask --)
[ -z "${TEHAS_XTASK:-}" ] || xtask=("$TEHAS_XTASK")
exec "${xtask[@]}" tag-release --script "$here/scripts/tag-release.sh" "$@"
