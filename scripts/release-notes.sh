#!/usr/bin/env bash
# Print the section of CHANGELOG.md for one release, without its heading.
#
# A section starts at a line "## <name>", with anything after the name, such as
# the date, and ends before the next "## " line. The release workflow calls this
# for the text of a release's page, and scripts/tag-release.sh calls it to check
# that the release has an "### Adopter action" before it tags.
#
# Usage: scripts/release-notes.sh <tag> [<file>]
#   <tag>    a vX.Y.Z tag, or Unreleased for the section at the top
#   <file>   the changelog to read, default the CHANGELOG.md beside scripts/;
#            - reads standard input
#
# Example:
#   scripts/release-notes.sh v0.9.0
#
# Exit: 0 the section is printed; 1 the file has no section for that name, or
#       cannot be read; 64 a bad argument
set -uo pipefail

case "${1:-}" in
  -h | --help) sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; exit 0 ;;
esac
[ "$#" -ge 1 ] && [ "$#" -le 2 ] || { echo "release-notes: usage: release-notes.sh <tag> [<file>]" >&2; exit 64; }
name=$1
file=${2:-$(dirname "${BASH_SOURCE[0]}")/../CHANGELOG.md}
[[ $name =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || [ "$name" = Unreleased ] || {
  echo "release-notes: the tag must be vX.Y.Z or Unreleased, got: $name" >&2
  exit 64
}
where=$file
if [ "$file" = - ]; then
  where="standard input"
  text=$(cat) || exit 1
else
  text=$(cat "$file" 2>/dev/null) || { echo "release-notes: cannot read $file" >&2; exit 1; }
fi

# The name is matched as a string, so the dots of a version are not a pattern.
section=$(awk -v name="$name" '
  /^## / { if (found) exit; found = (substr($0, 4, length(name)) == name && (length($0) == 3 + length(name) || substr($0, 4 + length(name), 1) == " ")); next }
  found { print }
  END { exit !found }
' <<<"$text") || { echo "release-notes: no section for $name in $where" >&2; exit 1; }
# Blank lines at either end are the file's layout, not the release's text.
printf '%s\n' "$section" | sed -e '/./,$!d' | tac | sed -e '/./,$!d' | tac
