#!/usr/bin/env bash
# Tests for .github/workflows/verify.yml: it runs on pull requests into and pushes to
# main, a release/ branch or the v2 branch, and on nothing else (a train's pull request
# is the batch gate), and it bounds what the runner keeps on disk.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

workflow=$T_ROOT/.github/workflows/verify.yml
# The on: block, from its line to the next top-level key.
triggers=$(awk '/^on:$/ {s = 1; next} s && /^[^ ]/ {exit} s {print}' "$workflow")

t_case "pull requests run the gate only when they are into main, a release/ branch or the v2 branch"
t_eq "$(grep -A1 '^  pull_request:' <<<"$triggers")" "  pull_request:
    branches: [main, 'release/**', refactor/coding-standard-v2]"

t_case "pushes run the gate only on main, the release/ branches and the v2 branch"
t_eq "$(grep -A1 '^  push:' <<<"$triggers")" "  push:
    branches: [main, 'release/**', refactor/coding-standard-v2]"

t_case "a train is not a release/ branch, so it runs no gate"
t_lacks "$triggers" 'train'

t_case "nothing else starts it"
t_eq "$(grep -c '^  [a-z_]*:' <<<"$triggers")" 2

t_case "the header says when it runs"
t_contains "$(sed -n '1,6p' "$workflow")" 'every pull request into main, a release/ branch or'

t_case "the kept build is pruned past 40 GiB, and only this user's old temp dirs go"
kept=$(sed -n '/- name: the cargo build kept between runs$/,/- name: the type checker/p' "$workflow")
t_contains "$kept" "if [ \"\${size:-0}\" -gt 40 ]; then"
t_contains "$kept" "find /tmp -maxdepth 1 -name 'tmp.*' -user \"\$(id -un)\" -mmin +180 -exec rm -rf {} +"

t_done
