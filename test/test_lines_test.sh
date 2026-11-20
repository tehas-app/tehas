#!/usr/bin/env bash
# Tests for scripts/check-test-lines.sh: the shell of test/ may not grow past
# scripts/test-lines.max. Each case runs the check in a scratch tree.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
fresh() { # max
  rm -rf "$tree"
  mkdir -p "$tree/scripts" "$tree/test/browser"
  cp "$T_ROOT/scripts/check-test-lines.sh" "$tree/scripts/"
  echo "$1" >"$tree/scripts/test-lines.max"
  printf '%s\n' '#!/usr/bin/env bash' 't_case one' >"$tree/test/a_test.sh"
  printf '%s\n' '# shellcheck shell=bash' 'f() { :; }' >"$tree/test/browser/lib.sh"
  printf '%s\n' 'import x from "y";' 'x();' 'x();' >"$tree/test/view_test.mjs"
}
check="'$tree/scripts/check-test-lines.sh'"

t_case "at the ceiling passes and says the count, and only shell counts"
fresh 4
t_run "$check"
t_eq "$t_rc" 0
t_eq "$t_out" "check-test-lines: 4 of 4: 0 lines of room"

t_case "under the ceiling passes and says to lower it"
fresh 9
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "4 of 9: 5 lines of room"
t_contains "$t_out" "lower scripts/test-lines.max to 4"

t_case "over the ceiling fails, points at Rust and names the largest files"
fresh 3
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "4 lines of shell in test/, over the ceiling of 3: 1 lines over"
t_contains "$t_out" "AGENTS.md, rule 6"
t_contains "$t_out" "test/a_test.sh"

t_case "a ceiling that is no number fails"
fresh 3
echo "many" >"$tree/scripts/test-lines.max"
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "holds no number"
t_done
