#!/usr/bin/env bash
# Print the release notes of a tag from CHANGELOG.md: the text of its page.
#
# A section starts at a line "## <name>", with anything after the name, such as
# the date, and ends before the next "## " line. A tag with a section of its
# own gets that section, without its heading. The release workflow calls this
# for the text of a release's page, and a failure stops the publish.
#
# A release vX.Y.Z needs a section of its own: without one it is an error.
# scripts/tag-release.sh names it in its changelog pull request, renaming
# Unreleased to "## vX.Y.Z (YYYY-MM-DD)" before it tags.
#
# A prerelease without a section of its own gets what is new since the tag
# before it: a line "New since <tag>." and then the items of Unreleased at the
# prerelease's tag that Unreleased at the tag before did not have, under their
# headings, with "None." under an Adopter action with nothing new. An item is
# a bullet with its continuation lines, or a paragraph under "### Adopter
# action". The tag before is the highest vX.Y.Z or prerelease tag, below this
# one, that its commit reaches. Both are read with git from the repository of
# the file, so the tag must be there; the file itself is read only for a
# section of the tag's own. When the tag before cannot be read, it fails: it
# never prints the whole of Unreleased, which holds every prerelease's changes
# since the last release.
#
# Unreleased also has the lines of the changelog fragments in changelog.d/
# beside the file, under their headings (scripts/collect-changelog.sh). A
# tagged release is read from the file alone.
#
# The work is `xtask release-notes` (console/xtask/src/notes.rs), built with
# cargo from the workspace beside this script, or the build TEHAS_XTASK names.
#
# Usage: scripts/release-notes.sh <tag> [<file>]
#   <tag>    a vX.Y.Z tag, or vX.Y.Z-alpha.N, -beta.N or -rc.N for a
#            prerelease, or Unreleased for the section at the top
#   <file>   the changelog to read, default the CHANGELOG.md beside scripts/;
#            - reads standard input, with no fragments, and a prerelease
#            read from it needs a section of its own
#
# Example:
#   scripts/release-notes.sh v1.0.0-rc.2
#
# Exit: 0 the notes are printed; 1 the file has no section for a release or
#       Unreleased, or cannot be read, or a fragment cannot, or what is new
#       in a prerelease cannot be read with git; 64 a bad argument; another
#       status when cargo cannot build the task
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 1
xtask=(cargo run --quiet --locked --manifest-path "$here/console/Cargo.toml" -p tehas-xtask --)
[ -z "${TEHAS_XTASK:-}" ] || xtask=("$TEHAS_XTASK")
exec "${xtask[@]}" release-notes --script "$here/scripts/release-notes.sh" "$@"
