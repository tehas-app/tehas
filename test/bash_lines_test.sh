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
t_contains "$t_out" "6 of 6: 0 lines of room"

t_case "over the ceiling fails and points at the Rust workspace"
fresh 5
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "6 lines of bash, over the ceiling of 5: 1 lines over"
t_contains "$t_out" "console/"

t_case "under the ceiling says one line and names no file"
fresh 7
t_run "$check"
t_eq "$t_rc" 0
t_eq "$t_out" "check-bash-lines: 6 of 7: 1 lines of room"

t_case "under the ceiling prints the room in one line"
fresh 10
t_run "$check"
t_eq "$t_rc" 0
t_eq "$t_out" "check-bash-lines: 6 of 10: 4 lines of room"

t_case "over the ceiling prints the lines over in the first line"
fresh 4
t_run "GIT_CEILING_DIRECTORIES='$T' $check"
t_eq "$t_rc" 1
[ "$(printf '%s\n' "$t_out" | head -n 1)" = "check-bash-lines: 6 lines of bash, over the ceiling of 4: 2 lines over (scripts/bash-lines.max)." ] \
  || t_bad "first line: $t_out"
t_contains "$t_out" "Take out 2 lines."

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
printf '%s\n' '#!/usr/bin/env bash' 'echo big' 'echo big' 'echo big' >"$tree/libexec/big"
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
t_contains "$t_out" "12 lines of bash, over the ceiling of 6: 6 lines over"
t_contains "$t_out" "Take out 6 lines."
t_contains "$t_out" "merge base with origin/main"
t_contains "$t_out" "+2      4  libexec/cmd"
t_contains "$t_out" "+2      2  libexec/new"
t_contains "$t_out" "-2      0  bin/tool"
t_lacks "$t_out" "Largest files:"
t_contains "$t_out" "Pay from: the five largest counted files the change did not touch."
t_contains "$t_out" "     4  libexec/big"
t_contains "$t_out" "     2  lib/a.sh"
# The pay-from list holds only files the change never touched: the file it
# added, the file it deleted and the file it grew are all on the table above
# and on none of this one. The line after a pay-from name is another name or
# the rule, so a tab before a name belongs to the table and not the list.
t_lacks "$t_out" "	libexec/new"
t_lacks "$t_out" "	bin/tool"
t_lacks "$t_out" "	libexec/cmd"

# The room a run starts from is the base's, not the working tree's: the tree
# below holds fourteen counted lines at the ceiling of six, and the base holds
# eleven of them, so the change adds three and the room it spent is named. The
# pay-from list holds only files the change never touched.
t_case "the base's figure is the room the change starts from"
fresh 6
printf '%s\n' '#!/usr/bin/env bash' 'echo big' 'echo big' 'echo big' >"$tree/libexec/big"
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q -m base
printf '%s\n' '#!/usr/bin/env bash' 'echo big' 'echo big' 'echo big' 'echo big' >"$tree/libexec/big"
g add -A
g commit -q -m big
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b topic
printf '%s\n' 'echo three' 'echo four' >>"$tree/libexec/cmd"
g add -A
g commit -q -m more
printf '%s\n' 'echo five' >>"$tree/libexec/cmd"
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "14 lines of bash, over the ceiling of 6"
t_contains "$t_out" "base 11 of 6: -5 lines of room; this change +3"

t_case "a run under the ceiling names the base's count, its room and the delta"
fresh 20
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q -m base
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b topic
printf '%s\n' 'echo three' 'echo four' 'echo five' >>"$tree/libexec/cmd"
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "9 of 20: 11 lines of room; base 6 of 20: 14 lines of room; this change +3"
t_contains "$t_out" "working tree: includes uncommitted work"

t_case "a committed change still says what it adds to the base"
fresh 20
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q -m base
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b topic
printf '%s\n' 'echo three' 'echo four' >>"$tree/libexec/cmd"
g add -A
g commit -q -m work
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "8 of 20: 12 lines of room; base 6 of 20: 14 lines of room; this change +2"
t_lacks "$t_out" "uncommitted"

t_case "a tree with uncommitted work says the figure includes it"
fresh 20
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q -m base
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b topic
printf '%s\n' 'echo three' >>"$tree/libexec/cmd"
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "this change +1 (working tree: includes uncommitted work)"

t_case "untracked work that is not bash still says the tree is not clean"
fresh 20
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q -m base
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b topic
printf '%s\n' 'a note' >"$tree/notes.md"
t_run "$check"
t_eq "$t_rc" 0
t_contains "$t_out" "this change +0 (working tree: includes uncommitted work)"

t_case "a run with no merge base prints the figure it printed before"
fresh 7
t_run "GIT_CEILING_DIRECTORIES='$T' $check"
t_eq "$t_rc" 0
t_eq "$t_out" "check-bash-lines: 6 of 7: 1 lines of room"

# The five largest counted files the change did not touch: where the lines can
# come from. A file the change grew is where the lines went in, not where they
# come out, so f1 is off the list even though f2, which the change left alone,
# is smaller than it. The fresh tree's files all count two lines, so the six
# f* files stand above them and the five-line big file joins the list.
t_case "over the ceiling it names where to pay from, and not the files it grew"
fresh 5
printf '%s\n' '#!/usr/bin/env bash' 'echo big' 'echo big' 'echo big' 'echo big' >"$tree/libexec/big"
g() { git -C "$tree" -c user.name=t -c user.email=t@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q -m base
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b topic
for n in 1 2 3 4 5 6; do
  printf '%s\n' '#!/usr/bin/env bash' >"$tree/libexec/f$n"
  for _ in $(seq 1 "$n"); do echo "echo $n"; done >>"$tree/libexec/f$n"
done
g add -A
g commit -q -m files
for _ in 1 2; do echo 'echo grown'; done >>"$tree/libexec/f1"
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "base 11 of 5: -6 lines of room; this change +29"
t_contains "$t_out" "Pay from: the five largest counted files the change did not touch."
t_contains "$t_out" "     7  libexec/f6"
t_contains "$t_out" "     6  libexec/f5"
t_contains "$t_out" "     5  libexec/f4"
t_contains "$t_out" "     5  libexec/big"
t_contains "$t_out" "     2  lib/a.sh"
t_lacks "$t_out" "	libexec/f1"
t_lacks "$t_out" "	libexec/f2"
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

t_case "a ceiling with leading zeros counts in base ten"
fresh 010
t_run "$check"
t_eq "$t_rc" 0
t_eq "$t_out" "check-bash-lines: 6 of 10: 4 lines of room"

t_case "a ceiling with leading zeros over the ceiling says the lines over"
fresh 005
t_run "GIT_CEILING_DIRECTORIES='$T' $check"
t_eq "$t_rc" 1
t_contains "$t_out" "6 lines of bash, over the ceiling of 5: 1 lines over"
t_contains "$t_out" "Take out 1 lines."

t_case "a ceiling too big for an integer fails as no number"
fresh 99999999999999999999
t_run "$check"
t_eq "$t_rc" 1
t_contains "$t_out" "holds no number"

t_case "the repository is at or under its own ceiling"
t_run "'$T_ROOT/scripts/check-bash-lines.sh'"
t_eq "$t_rc" 0

t_done
