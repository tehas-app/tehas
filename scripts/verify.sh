#!/usr/bin/env bash
# The gate of this repository: what must pass before a change is proposed.
#
#   scripts/verify.sh          the lint for product and host names, shellcheck,
#                              every shell test, the tests of the cargo
#                              workspace and, where node is installed, the
#                              tests of the front page
#   scripts/verify.sh --full   the above, plus the format and the lints of the
#                              workspace (rustfmt, clippy), and the front page's
#                              tests even where node is missing
#
# Needs bash, git, jq, shellcheck and cargo; --full also needs node. Without
# cargo the gate fails, fast or full: the dispatcher's rules are Rust tests. A
# missing node is a "skip" line in the fast gate and a failure in the full one;
# rustfmt and clippy, which not every toolchain holds, are "skip" lines. No
# network, no sandbox image, no product checkout: except that cargo fetches the
# workspace's crates the first time it builds it. The workspace's tests run at
# the same time as the shell tests; each step's output is printed in the order
# below. Prints one line per step.
#
# Exit: 0 everything passed, 1 a step failed.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1
failed=0
full=0
[ "${1:-}" = "--full" ] && full=1

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT

report() { # name rc output-file
  if [ "$2" = 0 ]; then
    echo "ok    $1"
  else
    echo "FAIL  $1"
    tail -n 40 "$3" | sed 's/^/      /'
    failed=1
  fi
}
step() { # name command...
  local name=$1 rc=0
  shift
  "$@" >"$tmp/out" 2>&1 || rc=$?
  report "$name" "$rc" "$tmp/out"
}
has() { command -v "$1" >/dev/null 2>&1; }

# The workspace's root manifest is console/Cargo.toml.
workspace=(--manifest-path console/Cargo.toml --workspace)
cargo_pid=
if has cargo; then
  cargo test --locked --quiet "${workspace[@]}" >"$tmp/cargo" 2>&1 &
  cargo_pid=$!
fi

step "no product or host names in platform files" scripts/check-generic.sh
step "shellcheck and the shell tests" scripts/test.sh
if [ -n "$cargo_pid" ]; then
  rc=0
  wait "$cargo_pid" || rc=$?
  report "the workspace's tests" "$rc" "$tmp/cargo"
else
  echo "FAIL  the workspace's tests (cargo is not installed)"
  failed=1
fi
if has node; then
  step "the front page's tests" node --test test/*_test.mjs
elif [ "$full" = 1 ]; then
  echo "FAIL  the front page's tests (node is not installed)"
  failed=1
else
  echo "skip  the front page's tests (node is not installed; --full needs it)"
fi
if [ "$full" = 1 ]; then
  if has cargo && cargo fmt --version >/dev/null 2>&1; then
    step "the workspace's format" cargo fmt --check --manifest-path console/Cargo.toml --all
  else
    echo "skip  the workspace's format (rustfmt is not installed)"
  fi
  if has cargo && cargo clippy --version >/dev/null 2>&1; then
    step "the workspace's lints" cargo clippy --locked --quiet --all-targets "${workspace[@]}" -- -D warnings
  else
    echo "skip  the workspace's lints (clippy is not installed)"
  fi
fi
exit "$failed"
