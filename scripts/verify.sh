#!/usr/bin/env bash
# The gate of this repository: what must pass before a change is proposed.
#
#   scripts/verify.sh          the lint for product and host names, shellcheck,
#                              and every shell test
#   scripts/verify.sh --full   the above, plus the tests of the console (cargo)
#                              and of the front page (node)
#
# Needs bash, git, jq and shellcheck; --full also needs cargo and node. No
# network, no sandbox image, no product checkout: except that cargo fetches the
# console's crates the first time it builds it. Prints one line per step.
#
# Exit: 0 everything passed, 1 a step failed.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1
failed=0

step() { # name command...
  local name=$1 out
  shift
  if out=$("$@" 2>&1); then
    echo "ok    $name"
  else
    echo "FAIL  $name"
    printf '%s\n' "$out" | tail -n 40 | sed 's/^/      /'
    failed=1
  fi
}

step "no product or host names in platform files" scripts/check-generic.sh
step "shellcheck and the shell tests" scripts/test.sh
with() { # tool name command...: a step that needs a tool
  local tool=$1 name=$2
  shift 2
  if command -v "$tool" >/dev/null 2>&1; then
    step "$name" "$@"
  else
    echo "FAIL  $name ($tool is not installed)"
    failed=1
  fi
}
if [ "${1:-}" = "--full" ]; then
  with cargo "the console's tests" cargo test --locked --quiet --manifest-path console/Cargo.toml
  with node "the front page's tests" node --test test/energy_view_test.mjs test/brick_wall_test.mjs
fi
exit "$failed"
