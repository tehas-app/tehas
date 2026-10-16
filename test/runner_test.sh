#!/usr/bin/env bash
# Tests for scripts/test.sh: the run names what did not pass after all the
# output of the test files, so a caller that keeps only the end of the log still
# says what failed, whatever its place in name order. How much a caller keeps is
# read from scripts/verify.sh, because that is the bound the block is capped to
# fit; the last case measures it. Each case runs the runner over a scratch
# checkout with fake test files of its own.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

repo=$T/checkout
# The runner started here would otherwise add another set of test files to the
# CPUs the outer run already keeps busy.
T_ENV=(TEHAS_TEST_JOBS=1)

# The lines scripts/verify.sh keeps of a step's output: the tail the block has
# to fit inside.
keep=$(sed -n 's/.*tail -n \([0-9][0-9]*\).*/\1/p' "$T_ROOT/scripts/verify.sh" | head -n 1)

fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/scripts" "$repo/test"
  cp "$T_ROOT/scripts/test.sh" "$repo/scripts/"
}

# One fake test file for the scratch checkout: it prints <lines> lines, then
# passes or exits <rc>. A run of the runner is meaningless if either is not a
# number, so it says so instead of testing the wrong thing.
fake() { # name rc [lines]
  local n=$1 rc=$2 lines=${3:-1} i
  if ! [[ $rc =~ ^[0-9]+$ && $lines =~ ^[0-9]+$ ]]; then
    echo "fake: rc and lines must be numbers: $n $rc $lines" >&2
    exit 1
  fi
  {
    echo '#!/usr/bin/env bash'
    for ((i = 1; i <= lines; i++)); do
      echo "echo ran $n line $i"
    done
    if [ "$rc" -ne 0 ]; then echo "echo FAIL: $n has a case that failed"; fi
    echo "exit $rc"
  } >"$repo/test/$n"
  chmod +x "$repo/test/$n"
}

# A fake test file that kills the shell meant to record its status: the runner
# sees a file that never finished.
killed() { # name
  {
    echo '#!/usr/bin/env bash'
    echo 'echo ran '"$1"' part way'
    echo "kill -9 \$PPID"
  } >"$repo/test/$1"
  chmod +x "$repo/test/$1"
}

# Give the runner a lib file its shellcheck refuses.
lint_fail() {
  mkdir -p "$repo/lib"
  # shellcheck disable=SC2016  # the line is shell for the lint to refuse, not expanded here
  printf 'echo $unquoted\n' >"$repo/lib/bad.sh"
}

tail_of() { # n: the last n lines of the run just made
  if ! [[ ${1:-} =~ ^[0-9]+$ ]]; then
    echo "tail_of: needs a number of lines, got '${1:-}'" >&2
    exit 1
  fi
  printf '%s\n' "$t_out" | tail -n "$1"
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

t_case "a failing file is named at the end of the run, whatever its place in name order"
fresh
fake a_test.sh 1 20
for n in b c d e f; do fake "${n}_test.sh" 0 20; done
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
# Its own log is the first of six; the name is repeated at the end of the run.
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
t_eq "$(tail_of 3)" "test.sh: failed checks (2), not run (0)
    test/a_test.sh
    test/c_test.sh"
t_lacks "$t_out" "test/b_test.sh"
t_lacks "$t_out" "test/d_test.sh"

t_case "a shellcheck failure is named beside a failing test file"
fresh
lint_fail
fake a_test.sh 1 2
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 3)" "test.sh: failed checks (2), not run (0)
    shellcheck
    test/a_test.sh"

t_case "a shellcheck failure alone is named, and the test files are not"
fresh
lint_fail
fake a_test.sh 0 2
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"
t_lacks "$t_out" "test/a_test.sh"

t_case "a failing lint is named when only a stub shellcheck is on the host"
fresh
t_stub shellcheck 'echo "the stub lint refuses a file" >&2
exit 1'
fake a_test.sh 1 2
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 3)" "test.sh: failed checks (2), not run (0)
    shellcheck
    test/a_test.sh"
# The stub is only for this case: a later one must see the real lint, or none.
rm -f "$T/bin/shellcheck"

# A PATH with the tools scripts/test.sh needs and no shellcheck in it, so the
# degraded run is tested on a host that does have one.
nobin=$T/nobin
no_shellcheck() { # run the scratch checkout's runner with no shellcheck to find
  local c p
  rm -rf "$nobin"
  mkdir -p "$nobin"
  for c in bash cat dirname env mktemp nproc rm; do
    p=$(command -v "$c") || continue
    ln -s "$p" "$nobin/$c"
  done
  t_run "cd '$repo' && PATH='$nobin' scripts/test.sh"
}

t_case "a lint that was not run is named too, and the run still passes"
fresh
fake a_test.sh 0 2
no_shellcheck
t_eq "$t_rc" 0
t_eq "$(tail_of 2)" "test.sh: failed checks (0), not run (1)
    skipped: shellcheck is not installed"

t_case "a lint that was not run is named beside a failing test file"
fresh
fake a_test.sh 1 2
no_shellcheck
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: shellcheck is not installed, skipping it"
t_eq "$(tail_of 3)" "test.sh: failed checks (1), not run (1)
    test/a_test.sh
    skipped: shellcheck is not installed"

t_case "a test file that never finished is named as one"
fresh
fake a_test.sh 0 2
killed b_test.sh
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    test/b_test.sh: it did not finish, no result recorded"

t_case "a file name cannot forge a line of the block"
fresh
fake $'weird\tname_test.sh' 1 1
t_run "cd '$repo' && scripts/test.sh"
t_eq "$t_rc" 1
t_eq "$(tail_of 1)" "    test/weird?name_test.sh"

t_case "the block fits the tail scripts/verify.sh keeps, even when every check fails"
if ! [[ $keep =~ ^[1-9][0-9]*$ ]]; then
  t_bad "scripts/verify.sh has no 'tail -n <number>' to read the bound from"
else
  fresh
  # More failing files than the tail holds: uncapped, the block would not fit.
  many=$((keep + 4))
  last=$(printf 'f%02d_test.sh' "$many")
  for ((i = 1; i <= many; i++)); do
    fake "$(printf 'f%02d' "$i")_test.sh" 1 1
  done
  t_run "cd '$repo' && scripts/test.sh"
  t_eq "$t_rc" 1
  # The header and the last names survive the cut; the earliest ones do not.
  t_contains "$(tail_of "$keep")" "test.sh: failed checks ($many), not run (0)"
  t_contains "$(tail_of "$keep")" "    test/$last"
  t_lacks "$(tail_of "$keep")" "test/f01_test.sh"
  t_contains "$(tail_of 1)" "more, in the full log"
fi

t_case "a checkout with nothing to check is exit 2, not a pass"
rm -rf "$repo"
mkdir -p "$repo/runner"
# The runner sits where the lint's globs do not read, so the checkout holds
# nothing it could check.
cp "$T_ROOT/scripts/test.sh" "$repo/runner/runner.sh"
t_run "bash '$repo/runner/runner.sh'"
t_eq "$t_rc" 2
t_contains "$t_out" "nothing could be checked"
t_lacks "$t_out" "failed checks"

t_done
