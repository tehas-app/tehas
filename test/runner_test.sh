#!/usr/bin/env bash
# Tests for scripts/test.sh: the run names every check that failed after all the
# output of the test files, so a caller that keeps only the end of the log
# (scripts/verify.sh keeps the last 40 lines) still says which files failed,
# whatever their place in name order. Each case runs the runner over a scratch
# checkout with fake test files of its own.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

repo=$T/checkout

fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/scripts" "$repo/test"
  cp "$T_ROOT/scripts/test.sh" "$repo/scripts/"
}

# One fake test file for the scratch checkout: it prints <lines> lines, then
# passes or exits <rc>.
fake() { # name rc [lines]
  local n=$1 rc=$2 lines=${3:-1} i
  {
    echo '#!/usr/bin/env bash'
    for ((i = 1; i <= lines; i++)); do
      echo "echo ran $n line $i"
    done
    if [ "$rc" != 0 ]; then echo "echo FAIL: $n has a case that failed"; fi
    echo "exit $rc"
  } >"$repo/test/$n"
  chmod +x "$repo/test/$n"
}

tail_of() { # n: the last n lines of the run just made
  printf '%s\n' "$t_out" | tail -n "$1"
}

badlib() {
  mkdir -p "$repo/lib"
  # shellcheck disable=SC2016  # the line is shell for the lint to refuse, not expanded here
  printf 'echo $unquoted\n' >"$repo/lib/bad.sh"
}

t_in_tail() { # needle n: named within the last n lines
  local needle=$1 n=$2 lines
  lines=$(tail_of "$n")
  case "$lines" in *"$needle"*) ;; *) t_bad "'$needle' is not in the last $n lines: $lines" ;; esac
}

t_case "a run where every check passes names no failure"
fresh
fake a_test.sh 0 3
fake b_test.sh 0 3
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 0
t_contains "$t_out" "ran a_test.sh line 3"
t_contains "$t_out" "ran b_test.sh line 3"
t_lacks "$t_out" "failed checks"

t_case "a failing file is named in the last 40 lines, whatever its place in name order"
fresh
fake a_test.sh 1 20
for n in b c d e f; do fake "${n}_test.sh" 0 20; done
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
# Its own log is the first of six; the name is repeated at the end of the run.
t_in_tail "test/a_test.sh" 40
t_eq "$(tail_of 1)" "    test/a_test.sh"
# Names are relative to the checkout, so they are paths a person can open.
t_lacks "$t_out" "$repo"

t_case "every failing file is named, and no passing one"
fresh
fake a_test.sh 1 2
fake b_test.sh 0 2
fake c_test.sh 1 2
fake d_test.sh 0 2
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 3)" "test.sh: failed checks (2)
    test/a_test.sh
    test/c_test.sh"
t_lacks "$t_out" "test/b_test.sh"
t_lacks "$t_out" "test/d_test.sh"

t_case "a shellcheck failure is named beside a failing test file"
fresh
badlib
fake a_test.sh 1 2
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 3)" "test.sh: failed checks (2)
    shellcheck
    test/a_test.sh"

t_case "a shellcheck failure alone is named, and the test files are not"
fresh
badlib
fake a_test.sh 0 2
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1)
    shellcheck"
t_lacks "$t_out" "test/a_test.sh"

t_case "a checkout with nothing to check is exit 2, not a pass"
rm -rf "$repo"
mkdir -p "$repo"
cp "$T_ROOT/scripts/test.sh" "$repo/runner.sh"
t_run "bash '$repo/runner.sh'"
t_eq "$t_rc" 2
t_contains "$t_out" "nothing could be checked"
t_lacks "$t_out" "failed checks"

t_done
