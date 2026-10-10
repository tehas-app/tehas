#!/usr/bin/env bash
# The gate of this repository: what must pass before a change is proposed.
#
#   scripts/verify.sh          the lint for product and host names, shellcheck,
#                              every shell test and, where cargo is installed,
#                              the tests of the console
#   scripts/verify.sh --full   the above, plus the format and the lints of the
#                              console (rustfmt, clippy) and the tests of the
#                              front page (node)
#
# Needs bash, git, jq and shellcheck; --full also needs cargo and node. A step
# whose tool is missing is a "skip" line in the fast gate and for rustfmt and
# clippy, which not every toolchain holds, and a failure otherwise. No network,
# no sandbox image, no product checkout: except that cargo fetches the
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
has() { command -v "$1" >/dev/null 2>&1; }
with() { # tool name command...: a step that needs a tool
  local tool=$1 name=$2
  shift 2
  if has "$tool"; then
    step "$name" "$@"
  else
    echo "FAIL  $name ($tool is not installed)"
    failed=1
  fi
}
console=(--manifest-path console/Cargo.toml)
if [ "${1:-}" = "--full" ] || has cargo; then
  with cargo "the console's tests" cargo test --locked --quiet "${console[@]}"
else
  echo "skip  the console's tests (cargo is not installed; --full needs it)"
fi
if [ "${1:-}" = "--full" ]; then
  if has cargo && cargo fmt --version >/dev/null 2>&1; then
    step "the console's format" cargo fmt --check "${console[@]}"
  else
    echo "skip  the console's format (rustfmt is not installed)"
  fi
  if has cargo && cargo clippy --version >/dev/null 2>&1; then
    step "the console's lints" cargo clippy --locked --quiet --all-targets "${console[@]}" -- -D warnings
  else
    echo "skip  the console's lints (clippy is not installed)"
  fi
  with node "the front page's tests" node --test test/*_test.mjs
fi
exit "$failed"
