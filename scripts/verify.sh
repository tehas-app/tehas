#!/usr/bin/env bash
# The gate of this repository: what must pass before a change is proposed.
#
#   scripts/verify.sh          the lint for product and host names, shellcheck,
#                              and every shell test
#   scripts/verify.sh --full   the above, plus deno lint over the TypeScript
#
# Needs bash, git, jq and shellcheck; --full also needs deno. No network, no
# sandbox image, no product checkout. Prints one line per step.
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
if [ "${1:-}" = "--full" ]; then
  if command -v deno >/dev/null 2>&1; then
    step "deno lint" deno lint --no-config libexec/console.ts test/mask_test.ts
  else
    echo "FAIL  deno lint (deno is not installed)"
    failed=1
  fi
fi
exit "$failed"
