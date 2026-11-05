#!/usr/bin/env bash
# Move the pinned tools of this tree to the newest release of the major
# version each has, with what moves with each: `console pins`, built from this
# tree's workspace (console/src/pins/mod.rs says what it reads and writes).
# scripts/tag-release.sh runs `move` before it tags. Needs cargo, curl, tar,
# and npm for the npm trees.
#
# Usage: scripts/pins.sh check
#        scripts/pins.sh move [--release vX.Y.Z]
#   check    print each tool's pin and what it would move to; change nothing
#   move     write the moves into this tree and the release's CHANGELOG.md
#            section, --release's or else Unreleased
#
# Exit: 0 done; 1 a source could not be read or a move failed; 64 usage
set -uo pipefail

case "${1:-}" in
  -h | --help) sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; exit 0 ;;
  check | move) ;;
  *) echo "pins: usage: pins.sh check | move [--release vX.Y.Z]" >&2; exit 64 ;;
esac
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 1
exec cargo run --quiet --locked --manifest-path "$root/console/Cargo.toml" -p console -- \
  pins "$@" --root "$root"
