#!/usr/bin/env bash
# Tests for scripts/check-bash-lines.sh: the bash of bin/, lib/*.sh and
# libexec/ may not grow past scripts/bash-lines.max. Each case runs the check
# in a scratch tree.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
fresh() { # max
  rm -rf "$tree"
  mkdir -p "$tree/scripts" "$tree/bin" "$tree/lib" "$tree/libexec"
  cp "$T_ROOT/scripts/check-bash-lines.sh" "$tree/scripts/"
  echo "$1" >"$tree/scripts/bash-lines.max"
  printf '%s\n' '#!/usr/bin/env bash' 'echo one' >"$tree/bin/tool"
  printf '%s\n' '#!/usr/bin/env bash' 'f() { :; }' >"$tree/lib/a.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'echo two' >"$tree/libexec/cmd"
}
check="'$tree/scripts/check-bash-lines.sh'"

t_case "at the ceiling passes and says the count"
fresh 6
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "6 of 6"

t_case "over the ceiling fails and points at the Rust workspace"
fresh 5
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "6 lines of bash, over the ceiling of 5"
t_contains "$t_out" "console/"

t_case "files that are not bash are not counted"
fresh 6
printf '%s\n' '#!/usr/bin/env python3' 'print(1)' 'print(2)' >"$tree/libexec/py"
printf '%s\n' 'a prompt' 'of text' >"$tree/lib/brief.md"
t_run "$check"
t_eq "$t_rc" 0

t_case "a ceiling that is not a number fails"
fresh "lots"
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "holds no number"

t_case "the repository is at or under its own ceiling"
t_run "'$T_ROOT/scripts/check-bash-lines.sh'"
t_eq "$t_rc" 0

t_done
