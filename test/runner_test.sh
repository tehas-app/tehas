#!/usr/bin/env bash
# Tests for scripts/test.sh: the run names what did not pass after all the
# output of the test files, so a caller that keeps only the end of the log still
# says what failed, whatever its place in name order. How much a caller keeps is
# read from scripts/verify.sh, because that is the bound the block is sized to
# fit; the last cases measure it. Each case runs the runner over a scratch
# checkout with fake test files of its own and a stub lint, so what the block
# says does not depend on whether this host has shellcheck installed.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

repo=$T/checkout
recs=$T/records
nobin=$T/nobin
# The runner started here would otherwise add another set of test files to the
# CPUs the outer run already keeps busy. Its records go in a directory of this
# file's own, so a fake test file can reach its own record by name.
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")

# The lines scripts/verify.sh keeps of a failing step, read from the line that
# does the cutting (a tail piped into the indent) rather than from any text in
# the file: another step's tail, or a sentence quoting one, must not be taken
# for the bound this file measures against.
cuts=$(sed -n 's/.*tail -n \([0-9][0-9]*\).*| sed .*/\1/p' "$T_ROOT/scripts/verify.sh")
keep=$(printf '%s\n' "$cuts" | head -n 1)

# A helper was given something no case can be built from. Say it once, and leave
# the run that case would have needed unrun, so the rest of the file still runs
# and the tally still says how many cases there were.
broken=

fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/runner" "$repo/test" "$recs"
  # The runner sits where the lint's globs do not read, and its lint is stubbed
  # to pass: no case turns on what a real shellcheck makes of a scratch checkout
  # or of the file under test, and only the cases about the lint pay for one.
  cp "$T_ROOT/scripts/test.sh" "$repo/runner/runner.sh"
  t_stub shellcheck 'exit 0'
}

run() { # the scratch checkout's runner
  if [ -n "$broken" ]; then
    t_bad "$broken"
    broken=
    return
  fi
  t_run "cd '$repo' && bash runner/runner.sh"
}

# One fake test file for the scratch checkout: it prints <lines> lines, then
# passes or exits <rc>. Bash takes an exit status mod 256 and the runner reads
# what it records as a number, so a call outside that range says so instead of
# building a case that quietly passes.
fake() { # name rc [lines]
  local n=$1 rc=$2 lines=${3:-1} i
  if ! [[ $rc =~ ^[0-9]+$ && $lines =~ ^[0-9]+$ ]] || [ "$rc" -gt 255 ] || [ "$lines" -gt 100 ]; then
    broken="fake: $n needs an exit status of 0 to 255 and up to 100 lines, got $rc and $lines"
    return 1
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

# Where a scratch run keeps each test file's record: one run, one directory, so
# a fake test file can name its own record. The pattern is left for the fake
# file to expand, with the directory quoted and the pattern not.
# shellcheck disable=SC2016  # the quotes and the braces are for the fake file to run, not here
recpat='"${TMPDIR:-/tmp}"/tehas-tests.*'

# A fake test file that removes its own log and puts a directory where it was:
# the runner has a status for it and nothing to show of what it said.
log_gone() { # name
  cat >"$repo/test/$1" <<EOF
#!/usr/bin/env bash
echo ran $1 part way
set -- $recpat
d=\$1
rm -f "\$d/test/$1.log"
mkdir "\$d/test/$1.log"
exit 0
EOF
  chmod +x "$repo/test/$1"
}

# A fake test file that leaves a status record it cannot be read back from, so
# the runner has a record and no number in it.
rc_gone() { # name
  cat >"$repo/test/$1" <<EOF
#!/usr/bin/env bash
echo ran $1 part way
set -- $recpat
d=\$1
echo not a number >"\$d/keep-$1"
chmod 000 "\$d/keep-$1"
ln -s "\$d/keep-$1" "\$d/test/$1.rc"
exit 0
EOF
  chmod +x "$repo/test/$1"
}

# Give the runner a lint that refuses it.
lint_refused() {
  t_stub shellcheck 'echo "the stub lint refuses a file" >&2
exit 1'
}

tail_of() { # n: the last n lines of the run just made
  if ! [[ ${1:-} =~ ^[0-9]+$ ]]; then
    # Printed where an assertion can see it: the case fails naming this, and the
    # cases after it still run.
    echo "tail_of: needs a number of lines, got '${1:-}'"
    return 1
  fi
  printf '%s\n' "$t_out" | tail -n "$1"
}

# The block is the last thing a run prints. Counting it says whether a caller
# that keeps only the end of the log keeps the count line too.
t_fits() {
  local n
  n=$(printf '%s\n' "$t_out" | sed -n '/^test\.sh: failed checks/,$p' | wc -l)
  [ "$n" -le "$keep" ] || t_bad "the block is $n lines, past the $keep scripts/verify.sh keeps"
}

# A PATH with the commands scripts/test.sh reaches for and no shellcheck in it,
# so the degraded run is tested on a host that does have one. The list has to
# keep step with the runner: a command it learns and this list does not shows up
# as a failed run naming the command, not as a block that looks wrong.
no_shellcheck() {
  local c p
  rm -rf "$nobin"
  mkdir -p "$nobin"
  for c in bash cat dirname mkdir mktemp nproc rm; do
    p=$(command -v "$c") || continue
    ln -s "$p" "$nobin/$c"
  done
  t_run "cd '$repo' && PATH='$nobin' bash runner/runner.sh"
  case $t_out in
    *'command not found'*)
      t_bad "the runner needs a command the harness left out: $(printf '%s\n' "$t_out" \
        | sed -n 's/.*: command not found//p' | head -n 1)"
      ;;
  esac
}

t_case "a run where every check passes names no failure"
fresh
fake a_test.sh 0 3
fake b_test.sh 0 3
run
t_eq "$t_rc" 0
t_contains "$t_out" "ran a_test.sh line 3"
t_contains "$t_out" "ran b_test.sh line 3"
t_lacks "$t_out" "failed checks"

t_case "a failing file is named at the end of the run, whatever its place in name order"
fresh
fake a_test.sh 1 20
for n in b c d e f; do fake "${n}_test.sh" 0 20; done
run
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
run
t_eq "$t_rc" 1
t_eq "$(tail_of 3)" "test.sh: failed checks (2), not run (0)
    test/a_test.sh
    test/c_test.sh"
t_lacks "$t_out" "test/b_test.sh"
t_lacks "$t_out" "test/d_test.sh"

t_case "a shellcheck failure is named beside a failing test file"
fresh
lint_refused
fake a_test.sh 1 2
run
t_eq "$t_rc" 1
t_eq "$(tail_of 3)" "test.sh: failed checks (2), not run (0)
    shellcheck
    test/a_test.sh"

t_case "a shellcheck failure alone is named, and the test files are not"
fresh
lint_refused
fake a_test.sh 0 2
run
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"
t_lacks "$t_out" "test/a_test.sh"

t_case "a real lint complaint in the checkout is named"
fresh
# The host's own lint, where it has one: this is the one case that wants it.
rm -f "$T/bin/shellcheck"
if ! command -v shellcheck >/dev/null 2>&1; then
  echo "no shellcheck on this host: the gate's own run covers a real lint"
else
  mkdir -p "$repo/lib"
  # shellcheck disable=SC2016  # the line is shell for the lint to refuse, not expanded here
  printf 'echo $unquoted\n' >"$repo/lib/bad.sh"
  fake a_test.sh 0 2
  run
  t_eq "$t_rc" 1
  # The lint names the file it refused, in its own words.
  t_contains "$t_out" "bad.sh"
  t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"
fi

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
run
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    test/b_test.sh: it did not finish, no result recorded"

t_case "a test file whose record cannot be read is named as one"
fresh
fake a_test.sh 0 2
rc_gone b_test.sh
run
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    test/b_test.sh: its recorded result could not be read"

t_case "a test file whose output cannot be read is a failed check"
fresh
log_gone b_test.sh
fake a_test.sh 0 2
run
t_eq "$t_rc" 1
t_contains "$t_out" "test/b_test.sh: its output could not be read"
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    test/b_test.sh: its output could not be read"

t_case "a file name cannot forge a line of the block"
fresh
fake $'weird\tname_test.sh' 1 1
run
t_eq "$t_rc" 1
t_eq "$(tail_of 1)" "    test/weird?name_test.sh"

t_case "the block fits the tail scripts/verify.sh keeps, even when every check fails"
if ! [[ $keep =~ ^[1-9][0-9]*$ ]]; then
  t_bad "scripts/verify.sh has no 'tail -n <number>' to read the bound from"
elif [ "$(printf '%s\n' "$cuts" | wc -l)" -ne 1 ]; then
  t_bad "scripts/verify.sh cuts more than one step: this case cannot tell which bound is the runner's"
elif [ "$keep" -gt 100 ]; then
  # The case builds fake test files to overflow the block. A bound that large
  # is worth measuring once by hand, not by spawning that many runs.
  t_bad "scripts/verify.sh keeps $keep lines: this case would build $((keep + 4)) fake test files"
else
  fresh
  # More failing files than the tail holds: oversized, the block would push the
  # count line out of it. Four more than the bound covers the count line, the
  # line that counts the rest, and the lines the block spends on them.
  many=$((keep + 4))
  last=$(printf 'f%02d_test.sh' "$many")
  for ((i = 1; i <= many; i++)); do
    fake "$(printf 'f%02d' "$i")_test.sh" 1 1
  done
  run
  t_eq "$t_rc" 1
  # The count line and the last names survive the cut; the earliest ones do not.
  t_contains "$(tail_of "$keep")" "test.sh: failed checks ($many), not run (0)"
  t_contains "$(tail_of "$keep")" "    test/$last"
  t_lacks "$(tail_of "$keep")" "test/f01_test.sh"
  t_contains "$(tail_of 1)" "more, in the full log"
  t_fits
fi

t_case "the lint is named even when failing test files are capped"
if ! [[ $keep =~ ^[1-9][0-9]*$ ]] || [ "$keep" -gt 100 ]; then
  t_bad "scripts/verify.sh has no usable 'tail -n <number>' to build a run past it"
else
  fresh
  lint_refused
  many=$((keep + 4))
  for ((i = 1; i <= many; i++)); do
    fake "$(printf 'f%02d' "$i")_test.sh" 1 1
  done
  run
  t_eq "$t_rc" 1
  # The lint is the first name of the block, not one the cap drops: no test
  # file's own output says the shell scripts were refused.
  t_contains "$(tail_of "$keep")" "test.sh: failed checks ($((many + 1))), not run (0)"
  t_contains "$(tail_of "$keep")" "    shellcheck"
  t_lacks "$(tail_of "$keep")" "test/f01_test.sh"
  t_fits
fi

t_case "a checkout with nothing to check is exit 2, not a pass"
fresh
run
t_eq "$t_rc" 2
t_contains "$t_out" "nothing could be checked"
t_lacks "$t_out" "failed checks"

t_done
