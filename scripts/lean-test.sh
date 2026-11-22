#!/usr/bin/env bash
# Run one test target's file, for pi-rukas's lean driver: the
# verify.test of .pi/work.json. `xtask lean-test` (console/xtask/src/lean.rs).
#
#   scripts/lean-test.sh <file>
#
# The file is relative to the checkout, as console/dispatch/tests/poll.rs.
#
# Exit: cargo's status; 64 no file, or one that is not a test target of the
# workspace.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd) || exit 1
[ "$#" -eq 1 ] || {
  echo "usage: scripts/lean-test.sh <file>" >&2
  exit 64
}
exec cargo run --quiet --locked --offline --manifest-path "$root/console/Cargo.toml" -p tehas-xtask -- \
  lean-test --root "$root" "$1"
