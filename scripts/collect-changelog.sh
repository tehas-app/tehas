#!/usr/bin/env bash
# Move the changelog fragments into a section of CHANGELOG.md.
#
# A pull request adds its lines in a file of its own, changelog.d/<issue>.md
# beside CHANGELOG.md, so two open pull requests do not conflict in
# CHANGELOG.md. A pull request with no issue names its file after the branch,
# each "/" of the branch a "-": fix/typo writes changelog.d/fix-typo.md
# (--name prints it). A fragment has up to two sections, "### Changes" and
# "### Adopter action", each holding the lines it adds; text outside them is
# refused. A line inside a fenced code block is text, not a heading; one that
# starts "## " or "### " is refused, since it would read as a heading of
# CHANGELOG.md. The fragments are taken in the order of their issue numbers,
# then the named ones by name.
#
# The lines of each fragment's Changes go at the end of the section's
# "### Changes", and each fragment's Adopter action is a paragraph of its own
# at the end of the section's "### Adopter action", in place of a lone "None.".
# A heading the section lacks is made.
#
# The work is `xtask collect-changelog` (console/xtask/src/changelog.rs), built
# with cargo from the workspace beside this script, or the build TEHAS_XTASK
# names.
#
# Usage: scripts/collect-changelog.sh [--print] [--section <name>] [<file>]
#        scripts/collect-changelog.sh --name <branch>
#   --print     print the changelog with the fragments' lines in it on stdout;
#               change no file
#   --section   the release section to collect into, default Unreleased
#   --name      print the path of the fragment of a branch with no issue, and
#               do nothing else
#   <file>      the changelog, default the CHANGELOG.md beside scripts/; the
#               fragments are in changelog.d/ beside it
#
# Without --print the changelog is rewritten, the fragments are deleted, and
# each deleted fragment's path is a line on stdout. With no fragments nothing
# changes.
#
# Example:
#   scripts/collect-changelog.sh --section v0.25.0
#
# Exit: 0 collected, or printed; 1 the changelog cannot be read or written, it
#       has no such section, or a fragment is refused (above);
#       64 a bad argument; another status when cargo cannot build the task
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 1
xtask=(cargo run --quiet --locked --manifest-path "$here/console/Cargo.toml" -p tehas-xtask --)
[ -z "${TEHAS_XTASK:-}" ] || xtask=("$TEHAS_XTASK")
exec "${xtask[@]}" collect-changelog --script "$here/scripts/collect-changelog.sh" "$@"
