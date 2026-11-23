#!/usr/bin/env bash
# Format one Rust file in place for pi-rukas's lean driver: the verify.format
# of .pi/work.json. `xtask lean-format` (console/xtask/src/lean.rs).
#
#   scripts/lean-format.sh <file>
#
# Exit: 0 formatted, 1 rustfmt refused the file, 64 usage.
set -uo pipefail

[ "$#" -eq 1 ] || {
  echo "usage: scripts/lean-format.sh <file>" >&2
  exit 64
}
root=$(cd "$(dirname "$0")/.." && pwd) || exit 1
exec cargo run --quiet --locked --offline --manifest-path "$root/console/Cargo.toml" -p tehas-xtask -- \
  lean-format "$1"
