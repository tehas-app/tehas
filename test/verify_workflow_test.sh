#!/usr/bin/env bash
# Tests for when .github/workflows/verify.yml runs: on pull requests into main
# and on pushes to main, and on nothing else. A pull request onto a release
# train runs no CI; the train's pull request into main is the batch gate.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

workflow=$T_ROOT/.github/workflows/verify.yml
# The on: block, from its line to the next top-level key.
triggers=$(awk '/^on:$/ {s = 1; next} s && /^[^ ]/ {exit} s {print}' "$workflow")

t_case "pull requests run the gate only when they are into main"
t_eq "$(grep -A1 '^  pull_request:' <<<"$triggers")" "  pull_request:
    branches: [main]"

t_case "pushes run the gate only on main"
t_eq "$(grep -A1 '^  push:' <<<"$triggers")" "  push:
    branches: [main]"

t_case "nothing else starts it"
t_eq "$(grep -c '^  [a-z_]*:' <<<"$triggers")" 2

t_case "the header says when it runs"
t_contains "$(sed -n '1,6p' "$workflow")" 'every pull request into main and on every push to main'

t_done
