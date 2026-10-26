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

t_case "under the ceiling says one line and names no file"
fresh 7
t_run "$check"
t_eq "$t_rc" 0
t_eq "$t_out" "check-bash-lines: 6 of 7"

t_case "over the ceiling without git lists the largest files and the rule"
fresh 5
printf '%s\n' '#!/usr/bin/env bash' 'a' 'b' 'c' >"$tree/libexec/big"
t_run "GIT_CEILING_DIRECTORIES='$T' $check"
t_eq "$t_rc" 1
t_contains "$t_out" "Take out 5 lines."
t_contains "$t_out" "which issue takes the lines out again"
t_contains "$t_out" "Largest files:"
t_contains "$t_out" "4  libexec/big"
t_contains "$t_out" "2  bin/tool"
t_contains "$t_out" "take out as many lines as you add"
t_contains "$t_out" "Do not raise the ceiling"

t_case "over the ceiling in a clone names the changed files with their deltas"
fresh 6
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q -m base
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b topic
printf '%s\n' 'echo three' 'echo four' >>"$tree/libexec/cmd"
printf '%s\n' '#!/usr/bin/env bash' 'echo new' >"$tree/libexec/new"
rm "$tree/bin/tool"
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "8 lines of bash, over the ceiling of 6"
t_contains "$t_out" "Take out 2 lines."
t_contains "$t_out" "merge base with origin/main"
t_contains "$t_out" "+2      4  libexec/cmd"
t_contains "$t_out" "+2      2  libexec/new"
t_contains "$t_out" "-2      0  bin/tool"
t_lacks "$t_out" "lib/a.sh"
t_lacks "$t_out" "Largest files:"

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
