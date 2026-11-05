#!/usr/bin/env bash
# Name the checks a change needs, from the paths it touched: the selection
# behind `scripts/verify.sh --changed`. `xtask select-tests`, built from this
# tree's workspace; console/xtask/src/select.rs says how paths map and what
# each line means (full, test, node, cargo, crate, omit).
#
#   scripts/select-tests.sh <path>...
#
# Each path is relative to the root of the checkout.
#
# Exit: 0 the lines were printed, 64 no path was given; another status when
# cargo cannot build the task, which verify.sh takes as a call for the full gate.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd) || exit 1
[ "$#" -gt 0 ] || {
  echo "usage: scripts/select-tests.sh <path>..." >&2
  exit 64
}
exec cargo run --quiet --locked --offline --manifest-path "$root/console/Cargo.toml" -p tehas-xtask -- \
  select-tests --root "$root" "$@"
