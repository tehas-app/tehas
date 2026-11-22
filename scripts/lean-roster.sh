#!/usr/bin/env bash
# Run every test target of the workspace in a checkout and write a JUnit
# report of each test, for pi-rukas's lean driver: the verify.roster of
# .pi/work.json. Lean runs it at a run's head and at its base and compares
# the two. `xtask lean-roster` (console/xtask/src/lean.rs).
#
#   scripts/lean-roster.sh <checkout> <report>
#
# Exit: 0 every test passed, 1 a test failed or a target did not build, 2 a
# built target failed with no failing test or the report could not be
# written, 64 usage.
set -uo pipefail

[ "$#" -eq 2 ] || {
  echo "usage: scripts/lean-roster.sh <checkout> <report>" >&2
  exit 64
}
root=$(cd "$1" && pwd) || exit 2
exec cargo run --quiet --locked --offline --manifest-path "$root/console/Cargo.toml" -p tehas-xtask -- \
  lean-roster --root "$root" --report "$2"
