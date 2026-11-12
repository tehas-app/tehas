#!/usr/bin/env bash
# Tests for scripts/test.sh: the run names what did not pass after all the
# output of the test files, so a caller that keeps only the end of the log still
# says what failed, whatever its place in name order. How much a caller keeps is
# read from scripts/verify.sh, because that is the bound the block is sized to
# fit; the last cases measure it. Each case runs the runner over a scratch
# checkout with fake test files of its own and a stub lint, so what the block
# says does not depend on whether this host has shellcheck installed.
# shellcheck disable=SC2016  # fake files and stubs are expanded when they run
# says does not depend on whether this host has shellcheck installed.
#
# Two spellings of the block's entries, so a case says which one it means: the
# FAILED: line that names a test file's own first failing case, and the
# indented path for a file with nothing of its own to show.
fail_line() { # name [text]
  printf 'FAILED: test/%s: %s\n' "$1" "${2:-$1 has a case that failed}"
}
bad_line() { # name [reason]
  local e="    test/$1"
  [ -n "${2:-}" ] && e=$e': '$2
  printf '%s\n' "$e"
}

# shellcheck disable=SC2016  # fake files and stubs are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# The runner is the xtask of this checkout, built once and named in
# TEHAS_XTASK, so the cases' cargo stubs reach only the build of the store.
t_xtask || {
  echo "FAIL: no xtask: $t_xtask_why"
  exit 1
}

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
  # Cases read what the lint and cargo were called with, so their records start
  # empty, and a cargo stub of one case is not the next case's.
  rm -f "$T/calls/shellcheck" "$T/calls/cargo" "$T/bin/cargo"
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

# The runner run with the caller's environment carrying these settings, as the
# units of a factory host carry the lines env.sh exported. This file sourced
# lib.sh, which dropped the names from its own shell, so they are given to the
# runner's own command rather than exported here. Each argument is a NAME=VALUE
# assignment: one without an = would reach env as a command to run.
run_env() { # NAME=VALUE...
  local a
  if [ -n "$broken" ]; then
    t_bad "$broken"
    broken=
    return
  fi
  for a in "$@"; do
    [[ $a == *=* ]] || {
      broken="run_env: $a is not NAME=VALUE"
      return 1
    }
  done
  t_run "cd '$repo' && env $(printf '%q ' "$@") bash runner/runner.sh"
}

# One fake test file for the scratch checkout: it prints <lines> lines, then
# passes with the count line of t_done or exits <rc>. Bash takes an exit status mod 256 and the runner reads
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
    if [ "$rc" -eq 0 ]; then printf 'echo %q\n' "$n: 1 cases, 0 failed"; fi
    echo "exit $rc"
  } >"$repo/test/$n"
  chmod +x "$repo/test/$n"
}

# A fake test file of its own: the body is what the file runs, so a case names
# what the file sees rather than what it prints, and a body that ends prints
# the count line of t_done. scripts/test.sh runs the file from the root of the
# scratch checkout, so a body that wants the harness sources test/lib.sh by
# that path.
fake_body() { # name <body...>
  local n=$1
  shift
  { echo '#!/usr/bin/env bash'; printf '%s\n' "$@" "echo '$n: 1 cases, 0 failed'"; } >"$repo/test/$n"
  chmod +x "$repo/test/$n"
}

# A fake test file that kills the shell meant to record its status, the parent
# of the timeout it runs under: the runner sees a file that never finished.
killed() { # name
  {
    echo '#!/usr/bin/env bash'
    echo 'echo ran '"$1"' part way'
    echo 'read -r _ _ _ pp _ </proc/$PPID/stat'
    echo 'kill -9 "$pp"'
  } >"$repo/test/$1"
  chmod +x "$repo/test/$1"
}

# A fake test file whose helper dies of a signal: the crash a run of this file
# must leave no core file for. The helper is a child of the file's own shell and
# the file waits for it, so the run of the file ends knowing it died, and the
# core file the kernel would write goes in the checkout the runner works in.
crashed() { # name
  {
    echo '#!/usr/bin/env bash'
    echo 'echo ran '"$1"' part way'
    echo 'echo "soft=$(ulimit -Sc)"'
    echo 'sh -c '"'"'kill -ABRT $$'"'"' &'
    echo 'wait'
    echo "echo $1: 1 cases, 0 failed"
  } >"$repo/test/$1"
  chmod +x "$repo/test/$1"
}

# The core files of the scratch checkout, the ones the runner's own processes
# would have written there. A glob is not used: a name the shell cannot match
# must say nothing rather than print the pattern back.
repo_cores() {
  local f found=
  for f in "$repo"/core "$repo"/core.*; do
    [ -f "$f" ] && found="$found ${f##*/}"
  done
  printf '%s\n' "${found# }"
}

# Where a scratch run keeps each test file's record: the run's root, which is
# the TMPDIR it gives its test files, so a fake test file can name its own
# record. Left for the fake file to expand.
recpat='"$TMPDIR"'

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

# A fake test file whose own output carries FAIL: lines: one for rc >= 1 and a
# second one for rc > 1, the first naming <case>. The runner reads the first
# one for the block. The line reaches the file's command line through %q, so a
# case name with a byte the shell would take is printed as that byte.
fake_case() { # name rc case [lines]
  local n=$1 rc=$2 case=$3 lines=${4:-1} i q lf
  lf=$(printf 'x\nx')
  lf=${lf/x/}
  if ! [[ $rc =~ ^[0-9]+$ && $lines =~ ^[0-9]+$ ]] || [ "$rc" -gt 255 ] || [ "$lines" -gt 100 ]; then
    broken="fake_case: $n needs an exit status of 0 to 255 and up to 100 lines, got $rc and $lines"
    return 1
  fi
  case "$case" in
    *"$lf"*)
      broken="fake_case: $n needs a case name with no newline"
      return 1
      ;;
  esac
  {
    echo '#!/usr/bin/env bash'
    for ((i = 1; i <= lines; i++)); do
      echo "echo ran $n line $i"
    done
    q=$(printf '%q' "$(printf 'FAIL: %s' "$case")")
    printf 'echo %s\n' "$q"
    if [ "$rc" -gt 1 ]; then
      q=$(printf '%q' "$(printf 'FAIL: %s, and another' "$case")")
      printf 'echo %s\n' "$q"
    fi
    echo "exit $rc"
  } >"$repo/test/$n"
  chmod +x "$repo/test/$n"
}

# The block that names what did not pass, the end of the run just made. The
# lines above it name every file, among the slowest.
block() { printf '%s\n' "$t_out" | sed -n '/^test\.sh: failed checks/,$p'; }

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
# as a failed run naming the command, not as a block that looks wrong. Each
# argument is a NAME=VALUE the runner is given.
no_shellcheck() { # [NAME=VALUE...]
  local c p pre=
  rm -rf "$nobin"
  mkdir -p "$nobin"
  for c in bash cat dirname flock mkdir mktemp mv nproc prlimit rm sed tail timeout; do
    p=$(command -v "$c") || continue
    ln -s "$p" "$nobin/$c"
  done
  [ "$#" = 0 ] || pre=$(printf '%q ' "$@")
  t_run "cd '$repo' && PATH='$nobin' ${pre}bash runner/runner.sh"
  case $t_out in
    *'command not found'*)
      t_bad "the runner needs a command the harness left out: $(printf '%s\n' "$t_out" \
        | sed -n 's/.*: command not found//p' | head -n 1)"
      ;;
  esac
}

# The same PATH without prlimit, for the case that says a host without the tool
# is told so once. TEHAS_TEST_CORES keeps the run off the limit, so a case can
# ask for the setting and still be without the command.
no_prlimit() { # [NAME=VALUE...]
  local c p pre=
  rm -rf "$nobin"
  mkdir -p "$nobin"
  for c in bash cat dirname flock mkdir mktemp mv nproc rm sed tail shellcheck timeout; do
    p=$(command -v "$c") || continue
    ln -s "$p" "$nobin/$c"
  done
  [ "$#" = 0 ] || pre=$(printf '%q ' "$@")
  # Only prlimit is missing from what the run sees. The harness's PATH cannot be
  # kept: $T/bin holds the prlimit that an earlier case's t_stub left, and the
  # commands of the list above are the ones the runner reaches for, so the run of
  # this host is the run of a host that has no prlimit and everything else.
  t_run "cd '$repo' && PATH='$nobin' ${pre}bash runner/runner.sh"
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
# Its own log is the first of six; the block names it with its failing case.
t_eq "$(tail_of 1)" "$(fail_line a_test.sh)"
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
$(fail_line a_test.sh)
$(fail_line c_test.sh)"
t_lacks "$(block)" "test/b_test.sh"
t_lacks "$(block)" "test/d_test.sh"

t_case "a shellcheck failure is named beside a failing test file"
fresh
lint_refused
fake a_test.sh 1 2
run
t_eq "$t_rc" 1
t_eq "$(tail_of 3)" "test.sh: failed checks (2), not run (0)
    shellcheck
$(fail_line a_test.sh)"

t_case "a shellcheck failure alone is named, and the test files are not"
fresh
lint_refused
fake a_test.sh 0 2
run
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"
t_lacks "$(block)" "test/a_test.sh"

t_case "a real lint complaint in the checkout is named"
fresh
# The host's own lint, where it has one: this is the one case that wants it.
rm -f "$T/bin/shellcheck"
if ! command -v shellcheck >/dev/null 2>&1; then
  no_lint "the gate's own run covers a real lint"
else
  mkdir -p "$repo/lib"
  printf 'echo $unquoted\n' >"$repo/lib/bad.sh"
  fake a_test.sh 0 2
  run
  t_eq "$t_rc" 1
  # The lint names the file it refused, in its own words.
  t_contains "$t_out" "bad.sh"
  t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"
fi

# A case that wants the host's own shellcheck, on a host without one: skipped
# by hand, failed in CI, where the gate needs it.
no_lint() { # why the skip is safe
  if [ -n "${CI:-}" ]; then
    t_bad "no shellcheck on this host, and CI needs it"
  else
    t_skip "no shellcheck on this host: $1"
  fi
}

t_case "a lint that was not run is named too, and the run still passes"
fresh
fake a_test.sh 0 2
no_shellcheck
t_eq "$t_rc" 0
t_eq "$(tail_of 2)" "test.sh: failed checks (0), not run (1)
SKIPPED: shellcheck is not installed"

t_case "a lint that was not run is named beside a failing test file"
fresh
fake a_test.sh 1 2
no_shellcheck
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: shellcheck is not installed, skipping it"
t_eq "$(tail_of 3)" "test.sh: failed checks (1), not run (1)
$(fail_line a_test.sh)
SKIPPED: shellcheck is not installed"

t_case "a missing shellcheck fails the run in CI"
fresh
fake a_test.sh 0 2
no_shellcheck CI=true
t_eq "$t_rc" 1
t_contains "$t_out" "shellcheck: not installed, and this run needs it"
t_lacks "$t_out" "skipping it"
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"

t_case "a missing shellcheck fails the run when TEHAS_TEST_STRICT asks"
fresh
fake a_test.sh 0 2
no_shellcheck TEHAS_TEST_STRICT=1
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"

t_case "a test file that exits 0 before t_done fails the run"
fresh
cp "$T_ROOT/test/lib.sh" "$repo/test/lib.sh" || t_bad "no harness to source"
fake_body a_test.sh '. test/lib.sh' 't_case "the first"' 't_eq 1 1' 'exit 0' \
  't_case "the second"' 't_eq 1 2' 't_done'
fake b_test.sh 0 2
run
t_eq "$t_rc" 1
t_lacks "$t_out" "the second"
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
$(bad_line a_test.sh 'ended before t_done')"

t_case "a test file that ends with t_done passes, skips and all"
fresh
cp "$T_ROOT/test/lib.sh" "$repo/test/lib.sh" || t_bad "no harness to source"
fake_body a_test.sh '. test/lib.sh' 't_case "the first"' 't_eq 1 1' \
  't_case "the second"' 't_skip "not here"' 't_done'
run
t_eq "$t_rc" 0
t_contains "$t_out" "a_test.sh: 2 cases, 1 skipped, 0 failed"
t_contains "$t_out" "test.sh: 2 cases in 1 files"

t_case "a test file that never finished is named as one"
fresh
fake a_test.sh 0 2
killed b_test.sh
run
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
$(bad_line b_test.sh 'it did not finish, no result recorded')"

t_case "a test file whose record cannot be read is named as one"
fresh
fake a_test.sh 0 2
rc_gone b_test.sh
run
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
$(bad_line b_test.sh 'its recorded result could not be read')"

t_case "a test file whose output cannot be read is a failed check"
fresh
log_gone b_test.sh
fake a_test.sh 0 2
run
t_eq "$t_rc" 1
t_contains "$t_out" "test/b_test.sh: its output could not be read"
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
$(bad_line b_test.sh 'its output could not be read')"

t_case "a file name cannot forge a line of the block"
fresh
fake $'weird\tname_test.sh' 1 1
run
t_eq "$t_rc" 1
# The name is cleaned, and so is the case line: the fake file's own output
# repeats the name it was given with the byte in it, and the byte is a ? here
# too. (The shell that runs the file collapses that byte to a space on the way
# through the fake, which is not the runner's doing.)
t_eq "$(tail_of 1)" "FAILED: test/weird?name_test.sh: weird name_test.sh has a case that failed"

# The name of the case comes from the file's own log, so it is data too.
t_case "the block names the first failing case of a failing file"
fresh
fake_case a_test.sh 1 "a case that failed early"
fake b_test.sh 0 2
run
t_eq "$t_rc" 1
t_eq "$(block)" "test.sh: failed checks (1), not run (0)
$(fail_line a_test.sh 'a case that failed early')"

t_case "of two failing cases in a file the block names only the first"
fresh
fake_case a_test.sh 2 "the first one"
run
t_eq "$t_rc" 1
# The second FAIL: line is still there, in the file's own output above the
# block; the block itself says the first one once.
t_contains "$t_out" "FAIL: the first one, and another"
t_eq "$(tail_of 1)" "$(fail_line a_test.sh 'the first one')"
t_eq "$(block)" "test.sh: failed checks (1), not run (0)
$(fail_line a_test.sh 'the first one')"

t_case "a case name with a control byte prints ?, like a file name does"
fresh
fake_case a_test.sh 1 $'a case\ttabbed'
run
t_eq "$t_rc" 1
t_eq "$(tail_of 1)" "$(fail_line a_test.sh 'a case?tabbed')"

t_case "a failing file whose log has no FAIL: line keeps its indented path"
fresh
# A file that fails by its exit status alone, with a line that only looks like
# the marker from a distance: the runner reads a FAIL: line, not a word.
printf '#!/usr/bin/env bash\necho ran a_test.sh\necho NOFAIL: not the marker\nexit 1\n' >"$repo/test/a_test.sh"
chmod +x "$repo/test/a_test.sh"
run
t_eq "$t_rc" 1
t_eq "$(tail_of 1)" "$(bad_line a_test.sh)"

t_case "a failing file with a FAIL: line gives the tail scripts/verify.sh keeps a FAILED: marker"
fresh
fake_case a_test.sh 1 "a case that failed"
for n in b c d e f; do fake "${n}_test.sh" 0 20; done
for n in g h i j k; do fake "${n}_test.sh" 1 20; done
run
t_eq "$t_rc" 1
# The whole point of the form: the marker is in column 1 of the last $keep
# lines, where a caller that keeps only the end of the log can look for it.
t_contains "$(tail_of "$keep")" "FAILED: test/a_test.sh: a case that failed"

t_case "a case name is cut to 200 characters, so one line cannot grow past the block"
fresh
long=$(printf 'x%.0s' {1..300})
fake_case a_test.sh 1 "a long case $long"
run
t_eq "$t_rc" 1
line=$(tail_of 1)
# 'FAILED: ', the path, ': ', then 200 characters of the case: 'a long case '
# is 12 of them, so 188 x survive the cut.
t_eq "${#line}" 224
t_eq "${line##*case }" "$(printf 'x%.0s' {1..188})"

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
    # No FAIL: line: these entries keep the indented form the cap is measured
    # with, and the block's size is what is at issue here, not its form.
    printf '#!/usr/bin/env bash\necho ran f%02d_test.sh\nexit 1\n' "$i" >"$repo/test/$(printf 'f%02d' "$i")_test.sh"
    chmod +x "$repo/test/$(printf 'f%02d' "$i")_test.sh"
  done
  run
  t_eq "$t_rc" 1
  # The count line and the last names survive the cut; the earliest ones do not.
  # The failing files print no FAIL: line, so their entries keep the indented
  # form this case measures the cap with.
  t_contains "$(tail_of "$keep")" "test.sh: failed checks ($many), not run (0)"
  t_contains "$(tail_of "$keep")" "$(bad_line "$last")"
  t_lacks "$(block)" "test/f01_test.sh"
  t_contains "$(tail_of 1)" "more, in the full log"
  t_fits
fi

# The header of scripts/test.sh says what the lint is called with. Read the
# sentence out of the script itself, as the bound above is read from
# scripts/verify.sh: a header that stops saying the call carries no floor fails
# here, and the manual in host/docs/tools.md stays the one statement of the
# policy - this file is not the authority for the form of a suppression.
header_lint=$(sed -n '/shellcheck over the shell scripts/,/^# *2\./p' "$T_ROOT/scripts/test.sh" | tr '\n#' ' ' | tr -s ' ')

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
  t_lacks "$(block)" "test/f01_test.sh"
  t_fits
fi

# A process that dies of a signal during a run must leave no core file in the
# checkout, because a file the kernel wrote there dirties the tree a later check
# of it reads: git status lists it, and a factory's final verify failed on dozens
# of them. The run puts the core limit of 0 in front of every task, so the kernel
# is never asked to write one, and the recorder's `timeout` and the test file and
# everything that file starts inherit the limit. Whether the kernel would have
# written a file is its own choice, taken with the limit already off, so the
# first case stubs prlimit and pins what the run asks the kernel for: the limit
# on the task's command, and the limit the check itself sees. The cases after it
# take the other halves: a run of the real prlimit under which a dump really is
# written, the run that keeps the dumps and names them, and the run that proves
# the limit held by looking for the files it meant to prevent.
#
# The name the run asks the kernel for, taken from the host's own file: a plain
# name is a file of the directory the tasks run in, and anything else - a program
# the kernel pipes the dump to, a path, a template - is no file of the checkout.
# A host of the last kind cannot be tested for dumps, so the cases that need the
# first say what this host's kernel really does.
core_pattern=$(head -n 1 /proc/sys/kernel/core_pattern 2>/dev/null) || core_pattern=
case $core_pattern in
  '' | */* | *%* | *'|'*) kernel_writes_dumps=0; core_name=core ;;
  *) kernel_writes_dumps=1; core_name=$core_pattern ;;
esac

t_case "every task runs under the core limit of 0, and a helper that crashes leaves the run clean"
fresh
# prlimit is stubbed to record its arguments and to run the command it was
# handed at the limit it was given, so the case reads the ask off the task's
# command and off the limit the check itself sees, neither of which depends on
# what the kernel chooses to write once the limit is already off.
t_stub prlimit 'printf "prlimit %s\n" "$*" >>"$T/calls/prlimit"
while [ "$#" != 0 ] && [ "$1" != -- ]; do shift; done
[ "$#" = 0 ] || shift
ulimit -c 0
exec "$@"'
crashed a_test.sh
fake b_test.sh 0 2
run
t_eq "$t_rc" 0
t_contains "$t_out" "ran a_test.sh part way"
t_contains "$t_out" "ran b_test.sh line 2"
t_contains "$t_out" "test.sh: 2 cases in 2 files"
# The file of the crash is not named as a failure: it died, and the run is not
# the one to say so, but the file still reported its case and the count says so.
t_lacks "$(block)" "test/a_test.sh"
# One call of prlimit per check, each of them the core limit of 0 in front of
# the check it was given, which is the whole of what keeps a dump out of the
# tree. The lint of each file is a task too, so the count covers both kinds.
t_eq "$(printf '%s\n' "$(t_calls prlimit)" | grep -c '^prlimit --core=0 -- ')" 4
t_contains "$(t_calls prlimit)" "bash $repo/test/a_test.sh"
t_contains "$(t_calls prlimit)" "shellcheck -x "
# And the limit is the one the check ran under, which is what the kernel is
# asked not to write a file with: a task that ran at the host's own limit would
# write one whenever its kernel takes dumps.
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^soft=//p' | head -n 1)" 0
t_eq "$(repo_cores)" ""

# The case above stubs prlimit, so it pins the ask and not the effect: the stub
# imposes the limit with its own ulimit. This one runs the real prlimit of the
# host, so what it pins is that the limit reaches the check through the
# recorder's timeout and every process the check starts, which is where a crash
# happens. It needs a kernel that writes its dumps under a plain name; a host
# that pipes them to a program or under a path of its own cannot be tested this
# way, and says so rather than passing without observing anything.
if [ "$kernel_writes_dumps" = 1 ]; then
  t_case "the real prlimit holds the limit of 0 through the recorder's timeout, so a crash leaves no dump"
  fresh
  crashed a_test.sh
  run
  t_eq "$t_rc" 0
  t_contains "$t_out" "ran a_test.sh part way"
  # The check itself saw the limit, not the host's own: that is what prlimit put
  # there, and the grandchildren inherit it from the check.
  t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^soft=//p' | head -n 1)" 0
  # Nothing was asked for, so nothing was written: the checkout the run works in
  # holds no core file of the crash the file caused.
  t_eq "$(repo_cores)" ""
  t_lacks "$(block)" "core files in the checkout"
else
  t_skip "the kernel of this host writes no dump of the working directory: its core_pattern is '$core_pattern'"
fi

# A file of the right name that ends in anything but a pid is not a dump: the
# kernel writes `name` and `name.<pid>` and nothing else. A run that called
# core.txt a crash would fail a checkout for a file a test meant to write. The
# stale dump of the next case is the other half: the run looks for the files it
# meant to prevent, so a limit the host did not apply cannot pass silently.
t_case "only name and name.<pid> are core files, so a file of another spelling is none"
fresh
# Files planted before the run, so the count the block gives is the case's and
# not this host's: of the names the kernel does write, one dump of an earlier
# crash; of the names it does not, a text file, a suffix that is no pid, and a
# directory of the right spelling. Only the dump is a core file, and only the
# dump fails the run. The bare name is asked for in the run after it, where the
# kernel of a host that writes dumps has nothing else to leave beside it.
#
# The name is given to the run as well as planted: the runner reads the host's
# file again inside its own process, and a host whose kernel answers that read
# with another name would scan for a file this case never wrote. Then the run
# finds nothing, prints no core line, and the case fails for the host's choice
# rather than for what the run does with the names it is given. The name of the
# run is a fact of the case, as it is for the TEHAS_TEST_CORES cases below.
echo a dump from before the run >"$repo/${core_name}.4242"
echo text >"$repo/${core_name}.txt"
echo text >"$repo/${core_name}.abc"
mkdir "$repo/${core_name}.9999"
fake a_test.sh 0 2
run_env ${T_ENV[@]+"${T_ENV[@]}"} TEHAS_TEST_CORE_PATTERN="$core_name"
t_eq "$t_rc" 1
t_contains "$t_out" "$core_name.4242 left in the checkout by a program that crashed"
t_contains "$(block)" "1 core files in the checkout"
t_lacks "$t_out" "${core_name}.txt left"
t_lacks "$t_out" "${core_name}.abc left"
t_lacks "$t_out" "${core_name}.9999 left"
# The file itself ran and reported: a dump of another time is not its failure.
t_lacks "$(block)" "test/a_test.sh"

# A run of a host whose limit did not apply still finds the dumps, because the
# run looks for the files it meant to prevent: an acceptance criterion of this
# change is that the tree a later check reads is clean, and a limit nobody
# verified is not that. It is a different question from the opt-in run, so it is
# a case of its own, and it costs one extra read of the root per run.
t_case "a default run looks for the dumps it meant to prevent and fails when one is there"
fresh
# One file, which ends before the run reads the tree: the run takes the listing
# of the checkout at its end, and a crash of its own would add a dump the host's
# kernel chose to write, which is not the case's to name. So the tree holds one
# dump before the run starts, of the bare name the kernel writes first, and the
# run must find that one and nothing else. The run is told that name too, for
# the reason the case above gives: the run reads the host's file itself, and a
# host answering it with another name would look for no file of this checkout.
fake a_test.sh 0 2
echo a dump from a limit that did not apply >"$repo/$core_name"
run_env ${T_ENV[@]+"${T_ENV[@]}"} TEHAS_TEST_CORE_PATTERN="$core_name"
t_eq "$t_rc" 1
t_contains "$t_out" "$core_name left in the checkout by a program that crashed"
t_contains "$(block)" "1 core files in the checkout"
# The file of the crash is not named as a failure beside it: it ran and reported.
t_lacks "$(block)" "test/a_test.sh"


# The three answers of the scan - files found, a directory that could not be
# read, and nothing to ask about - are pinned in the runner's own cases at
# console/xtask/src/run/tests.rs, where each is a fact the case sets. Here the
# scan is pinned by what it names and what it leaves alone: a tree the run cannot
# read is the one thing a scratch run cannot hold still, since the run lists the
# checkout as it starts and reads it again at the end.
t_case "a task runs under the limit with its own command, and one that crashes is not a failure of the run"
fresh
crashed a_test.sh
run
t_eq "$t_rc" 0
t_contains "$t_out" "ran a_test.sh part way"
t_lacks "$(block)" "test/a_test.sh"
# The file ended and said how many cases it ran, so its crash costs the run
# nothing beyond what the kernel was asked for.
t_contains "$t_out" "test.sh: 1 cases in 1 files"

t_case "TEHAS_TEST_CORES keeps the core files, names what the crash left, and fails"
fresh
crashed a_test.sh
# The setting is the one a person sets to see what a crash really leaves, so the
# case makes the dump a fact rather than a property of the host: a file of the
# name the run asks for is written before it starts, and the run must name that
# file and fail on it. Whether this host's kernel also wrote one of its own is
# not asked: the count line is the one thing the case cannot let the host decide.
printf '0.00 0.00 0.00 1/1 1\n' >"$T/quiet.loadavg"
echo a dump from a crash of the run >"$repo/$core_name"
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
run_env TEHAS_TEST_JOBS=1 TMPDIR="$recs" TEHAS_TEST_CORES=1 \
  TEHAS_TEST_CORE_PATTERN="$core_name" TEHAS_TEST_LOADAVG="$T/quiet.loadavg"
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
t_contains "$t_out" "ran a_test.sh part way"
t_contains "$t_out" "TEHAS_TEST_CORES keeps the core files of what this run starts"
t_contains "$t_out" "kernel.core_pattern is $core_name"
# A task is no longer put under the limit, which is the point of the setting, and
# the file the run found is named and fails it: a dump standing in the tree is
# what the setting exists to show.
t_eq "$t_rc" 1
t_contains "$t_out" "$core_name left in the checkout by a program that crashed"
t_contains "$(block)" "core files in the checkout"
# The file of the crash itself is never the one named: it ran and reported.
t_lacks "$(block)" "test/a_test.sh"

t_case "a host whose kernel keeps its dumps elsewhere is asked for no core file"
fresh
# The name of this host's own dumps is planted before the run so the case can
# say what the setting did not do: a run that looked anyway would name this file
# and fail. The point is that no name of a kernel that pipes its dumps elsewhere
# reaches the scan.
echo a dump no run should name >"$repo/$core_name"
crashed a_test.sh
printf '0.00 0.00 0.00 1/1 1\n' >"$T/quiet.loadavg"
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
run_env TEHAS_TEST_JOBS=1 TMPDIR="$recs" TEHAS_TEST_CORES=1 \
  TEHAS_TEST_CORE_PATTERN='|/usr/lib/systemd/coredump %p %s' \
  TEHAS_TEST_LOADAVG="$T/quiet.loadavg"
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
# A program the kernel pipes the dump to, or a path it writes under, is no file
# of the checkout, so the run asks for none and a run of this host passes: not
# even the opt-in scan runs, because there is no name under which a dump of this
# run could stand in the directory the tasks work in. The file the run planted is
# the proof: a scan that ran named it.
t_eq "$t_rc" 0
t_contains "$t_out" "ran a_test.sh part way"
t_lacks "$t_out" "left in the checkout"
t_lacks "$(block)" "core files in the checkout"

# The one line that says a host cannot run the checks at all, in place of a task
# per check that ends 126 and names a record path. prlimit is as much a
# requirement of the runner as cargo or shellcheck, and the run names it missing
# once, the way it names them.
t_case "a host with no prlimit is told once that every task needs it"
fresh
fake a_test.sh 0 2
printf '0.00 0.00 0.00 1/1 1\n' >"$T/quiet.loadavg"
no_prlimit TEHAS_TEST_LOADAVG="$T/quiet.loadavg"
t_eq "$t_rc" 2
t_contains "$t_out" "prlimit is not installed, and every task needs it"
# Nothing was run, so the block that names failed checks is not printed: the run
# stopped where the missing command stops one for a missing cargo.
t_lacks "$t_out" "failed checks"

t_case "TEHAS_TEST_CORES does not need prlimit, since it runs no task under a limit"
fresh
fake a_test.sh 0 2
# The setting is the way a person finds out what a crash really leaves, so the
# host that lacks the tool is still allowed to ask: the run that keeps the dumps
# puts no prlimit in front of anything, so a host without it runs its checks and
# prints its count line like any other. Only the runner's own line is asked for
# to be absent, which is what the case is about.
printf '0.00 0.00 0.00 1/1 1\n' >"$T/quiet.loadavg"
no_prlimit TEHAS_TEST_CORES=1 TEHAS_TEST_CORE_PATTERN="$core_name" \
  TEHAS_TEST_LOADAVG="$T/quiet.loadavg"
t_eq "$t_rc" 0
t_contains "$t_out" "1 cases in 1 files"
t_lacks "$t_out" "prlimit is not installed"

t_case "a test file that runs past its time is stopped and named as timed out"
fresh
fake a_test.sh 0 1
printf '#!/usr/bin/env bash\necho ran b_test.sh part way\nexec sleep 30\n' >"$repo/test/b_test.sh"
began=$SECONDS
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs" TEHAS_TEST_FILE_TIMEOUT=1)
run
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
t_eq "$t_rc" 1
t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
$(bad_line b_test.sh 'timed out after 1s')"
t_eq "$((SECONDS - began < 15))" 1

# A test file's scratch directories go under the run's root, its TMPDIR, so
# what a file killed before its own cleanup leaves goes with the run.
t_case "a test file's TMPDIR is the run's own root, and the root goes when the run ends"
fresh
fake_body a_test.sh "echo \"\$TMPDIR\" >'$T/a.tmpdir'" 'mktemp -d >/dev/null'
run
t_eq "$t_rc" 0
given=$(cat "$T/a.tmpdir")
t_eq "${given%/run.*}" "$recs/tehas-tests"
t_eq "$([ -e "$given" ] && echo there || echo gone)" gone

t_case "a root a killed run left goes when the next run starts, and one a run holds stays"
fresh
roots=$recs/tehas-tests
mkdir -p "$roots/run.dead/tehas-test.left" "$roots/run.live" "$roots/run.made"
: >"$roots/run.dead/lock"
t_hold "$roots/run.live/lock"
fake a_test.sh 0 1
run
t_release
t_eq "$t_rc" 0
t_eq "$([ -e "$roots/run.dead" ] && echo there || echo gone)" gone
t_eq "$([ -e "$roots/run.live" ] && echo there || echo gone)" there
# A root with no lock yet is one a run is still making.
t_eq "$([ -e "$roots/run.made" ] && echo there || echo gone)" there
rm -rf "$roots"

# Each file notes when it starts and ends. Three at a time, they overlap; on a
# host whose load has reached its CPUs, the next waits for the one before.
# The threshold is the host's CPUs, not 1.5 times them: three gates side by
# side on one host used to run past the CPUs before the runner held a file
# back. The load is read as a whole number, so 0.04 on a host of 8 CPUs is
# below the threshold and 8.00 reaches it. The stubbed count is also below
# what cpus() reads, so every case here sizes its pool from TEHAS_TEST_JOBS
# and never from the host.
overlap() {
  fresh
  for n in a b c; do
    printf '#!/usr/bin/env bash\necho "start %s" >>"%s"\nsleep 0.3\necho "end %s" >>"%s"\necho "%s: 1 cases, 0 failed"\n' \
      "$n" "$T/order" "$n" "$T/order" "$n" >"$repo/test/${n}_test.sh"
  done
  rm -f "$T/order"
  T_ENV=(TEHAS_TEST_JOBS=3 TMPDIR="$recs" TEHAS_TEST_LOADAVG="$T/loadavg")
  run
  T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
}
# The runner takes the host's CPUs from nproc, so a stub makes the threshold
# the same on every machine: eight here, where the host may have sixteen.
t_stub nproc 'echo 8'
# The memory the run may use sizes the pool as well as the CPUs: two tasks a
# GiB. TEHAS_TEST_MEMORY_MAX names the file read in place of the cgroup's, and
# no case here sets TEHAS_TEST_JOBS, or the ceiling would not be asked.
pool() { # memory.max's contents, or nothing for a file that does not exist
  fresh
  printf '#!/usr/bin/env bash\necho "a_test.sh: 1 cases, 0 failed"\n' >"$repo/test/a_test.sh"
  rm -f "$T/memory.max"
  [ -z "$1" ] || echo "$1" >"$T/memory.max"
  T_ENV=(TMPDIR="$recs" TEHAS_TEST_MEMORY_MAX="$T/memory.max" TEHAS_TEST_LOADAVG="$T/loadavg")
  run
  T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
}
echo "0.00 0.00 0.00 1/100 1" >"$T/loadavg"

t_case "a memory ceiling too small for the CPUs sizes the pool"
pool $((2 * 1024 * 1024 * 1024))
t_eq "$t_rc" 0
t_contains "$t_out" " 4 at a time"

t_case "a ceiling with room for every CPU leaves the pool at the CPUs"
pool $((96 * 1024 * 1024 * 1024))
t_eq "$t_rc" 0
t_contains "$t_out" " 8 at a time"

t_case "no ceiling leaves the pool at the CPUs"
pool max
t_eq "$t_rc" 0
t_contains "$t_out" " 8 at a time"

t_case "no file to read leaves the pool at the CPUs"
pool ""
t_eq "$t_rc" 0
t_contains "$t_out" " 8 at a time"

t_case "a ceiling below one task still runs one"
pool $((100 * 1024 * 1024))
t_eq "$t_rc" 0
t_contains "$t_out" " 1 at a time"

t_case "an idle host runs the files side by side"
echo "0.00 0.00 0.00 1/100 1" >"$T/loadavg"
overlap
t_eq "$t_rc" 0
t_eq "$(head -n 2 "$T/order" | cut -d' ' -f1 | tr '\n' ' ')" "start start "

t_case "a load just below the host's CPUs still starts them together"
echo "7.90 7.90 7.90 1/100 1" >"$T/loadavg"
overlap
t_eq "$t_rc" 0
t_eq "$(head -n 2 "$T/order" | cut -d' ' -f1 | tr '\n' ' ')" "start start "

t_case "a load at the host's CPUs holds the next file back"
echo "8.00 8.00 8.00 1/100 1" >"$T/loadavg"
overlap
t_eq "$t_rc" 0
t_eq "$(cut -d' ' -f1 "$T/order" | tr '\n' ' ')" "start end start end start end "

t_case "a busy host runs them one after another, and all of them"
echo "99999.00 0.00 0.00 1/100 1" >"$T/loadavg"
overlap
t_eq "$t_rc" 0
t_eq "$(cut -d' ' -f1 "$T/order" | tr '\n' ' ')" "start end start end start end "

t_case "a load that cannot be read holds nothing back"
rm -f "$T/loadavg"
overlap
t_eq "$t_rc" 0
t_eq "$(head -n 2 "$T/order" | cut -d' ' -f1 | tr '\n' ' ')" "start start "

t_case "the run counts the cases of every file and names the slowest"
fresh
printf '#!/usr/bin/env bash\necho "a_test.sh: 3 cases, 0 failed"\n' >"$repo/test/a_test.sh"
printf '#!/usr/bin/env bash\nsleep 1\necho "b_test.sh: 4 cases, 0 failed"\n' >"$repo/test/b_test.sh"
run
t_eq "$t_rc" 0
t_contains "$t_out" "test.sh: 7 cases in 2 files, 1 at a time"
t_eq "$(printf '%s\n' "$t_out" | sed -n '/the slowest files/{n;p}' | sed 's/.* s  //')" "test/b_test.sh"

t_case "TEHAS_TEST_SLOWEST says how many of the slowest files the run names"
fresh
# A ladder of sleeps, so no two files tie: a run of four names the three
# slowest, slowest first, and leaves the fastest out. A tenth of a second is
# enough to separate two files at the runner's tenth-of-a-second resolution.
for n in 1 2 3 4; do
  printf '#!/usr/bin/env bash\nsleep 0.%d\necho "t%s_test.sh: 1 cases, 0 failed"\n' "${n}9" "$n" >"$repo/test/t${n}_test.sh"
  chmod +x "$repo/test/t${n}_test.sh"
done
run_env TEHAS_TEST_SLOWEST=3
t_eq "$t_rc" 0
# The block is everything between its own header and the next one, so the
# count is read from the run's output rather than from a window of its own.
slowest=$(printf '%s\n' "$t_out" | sed -n '/^test\.sh: the slowest files:$/{n;:a;/^    .* s  /{p;n;ba};q}')
t_eq "$(printf '%s\n' "$slowest" | wc -l)" 3
t_eq "$(printf '%s\n' "$slowest" | sed 's/.* s  //')" "test/t4_test.sh
test/t3_test.sh
test/t2_test.sh"

# More than there are files names every one of them, so a measurement run
# reads its whole timing out of one go.
run_env TEHAS_TEST_SLOWEST=9
t_eq "$t_rc" 0
slowest=$(printf '%s\n' "$t_out" | sed -n '/^test\.sh: the slowest files:$/{n;:a;/^    .* s  /{p;n;ba};q}')
t_eq "$(printf '%s\n' "$slowest" | wc -l)" 4
t_eq "$(printf '%s\n' "$slowest" | sed 's/.* s  //')" "test/t4_test.sh
test/t3_test.sh
test/t2_test.sh
test/t1_test.sh"

t_case "the slowest files run first, from the list, the others after in name order"
fresh
for n in a_test.sh work_x_test.sh dispatch_y_test.sh; do
  printf '#!/usr/bin/env bash\necho "%s" >>"$1"\n' "$n" >"$repo/test/$n"
done
# Each file notes itself in the order it ran: the jobs are one at a time.
sed -i "s|\"\$1\"|$T/order|" "$repo"/test/*.sh
rm -f "$T/order"
run
t_eq "$(cat "$T/order" 2>/dev/null | tr '\n' ' ')" "work_x_test.sh dispatch_y_test.sh a_test.sh "

t_case "the lint runs once for each file, and names the file it refused"
fresh
t_stub shellcheck 'echo "$@" >>"$T/linted"
case "$2" in */bad.sh) echo "In $2: refused"; exit 1 ;; esac'
mkdir -p "$repo/lib"
: >"$repo/lib/good.sh"
: >"$repo/lib/bad.sh"
fake a_test.sh 0 1
rm -f "$T/linted"
run
t_eq "$t_rc" 1
t_eq "$(wc -l <"$T/linted")" 3
t_contains "$t_out" "In $repo/lib/bad.sh: refused"
t_contains "$t_out" "shellcheck: failed, 1 of 3 files"

# The lint's severity floor, pinned two ways: what the runner asks the lint for
# (the stub that keeps the other cases off the host's tools records its
# arguments), and what a lint with no floor makes of a file whose only note is an
# info one (the host's shellcheck, which is what knows a note's severity).
#
#   1. the runner's lint calls over the scratch checkout, one for each file: an
#      exact match against the calls the runner builds for the same files, so an
#      added option is refused rather than only the two spellings of the
#      severity flag
#   2. what a lint with no floor makes of a file whose only note is an info one
#      (the host's shellcheck, which is what knows a note's severity)
t_case "the lint runs with no floor, so an info note fails the run"
t_contains "$header_lint" "no -S floor"
fresh
fake a_test.sh 0 2
run
# The calls the runner makes: -x and one of the files its own globs find, with
# nothing else. Built here from the same globs, so an option that changes what the lint
# reports (-S or --severity for a floor, -e or --exclude to drop codes,
# --enable or --source-path to change what is read) makes the two differ. The
# runner passes the paths as it built them, with $root resolved to the checkout.
lintglobs=()
for g in "$repo"/bin/* "$repo"/libexec/* "$repo"/lib/*.sh "$repo"/scripts/*.sh "$repo"/test/*.sh \
  "$repo"/install.sh "$repo"/bootstrap.sh "$repo"/host/env.sh "$repo"/host/shim/gh \
  "$repo"/host/sandbox/build.sh "$repo"/host/sandbox/gh-shim "$repo"/host/login/tehas; do
  [ -f "$g" ] || continue
  case "$g" in */scripts/publish-lib.sh) continue ;; esac
  lintglobs+=("$g")
done
lintcalls=$(t_calls shellcheck)
# One call for each file. The jobs are one at a time, but the order is not the
# point here, so both sides are sorted.
t_eq "$(LC_ALL=C sort <<<"$lintcalls")" "$(printf -- '-x %s\n' "${lintglobs[@]}" | LC_ALL=C sort)"

fresh
mkdir -p "$repo/lib"
# An info note on its own: a literal in single quotes, the false positive that
# a cycle met once. The host's lint is what knows it is an info note.
rm -f "$T/bin/shellcheck"
if ! command -v shellcheck >/dev/null 2>&1; then
  no_lint "the gate's own run covers the severity floor"
else
  # shellcheck disable=SC2016  # the quotes are a literal for the lint to note on, not expanded here
  printf "echo 'a literal \$(not expanded)'\n" >"$repo/lib/literal.sh"
  fake a_test.sh 0 2
  run
  t_eq "$t_rc" 1
  t_contains "$t_out" "SC2016"
  t_eq "$(tail_of 2)" "test.sh: failed checks (1), not run (0)
    shellcheck"
fi

# --- The state store, built once for the test files ------------------------------
# saw_store <name>: a fake test file that prints what it was told of the store.
saw_store() {
  printf '%s\n' '#!/usr/bin/env bash' 'echo "$0 bin=${T_STORE_BIN:-} why=${T_STORE_WHY:-}"' \
    "echo '$1: 1 cases, 0 failed'" >"$repo/test/$1"
  chmod +x "$repo/test/$1"
}
with_console() {
  mkdir -p "$repo/console"
  : >"$repo/console/Cargo.toml"
}

# The stub records its arguments; the environment of its call is what a case
# about a variable has to echo itself, into the same file t_calls reads.
t_case "the store, the attachment checker, tehas-dispatch, tehas-cycle and the console are built once, offline, into the workspace's target with its profile, and named to every test file"
fresh
with_console
t_stub cargo 'prev=""
for a in "$@"; do [ "$prev" != --target-dir ] || td=$a; prev=$a; done
printf "debug=%s\\n" "${CARGO_PROFILE_DEV_DEBUG-absent}" >>"$T/calls/cargo"
mkdir -p "$td/debug" && echo store >"$td/debug/store"'
saw_store a_test.sh
saw_store b_test.sh
run
t_eq "$t_rc" 0
# The line of the environment is the last of the call, so the arguments still
# read as one line. CARGO_TARGET_DIR is the one t_run gives, as CI gives one:
# the gate's cargo test builds there too, with the same profile (#1360).
t_eq "$(t_calls cargo)" "build --locked --offline --quiet --manifest-path $repo/console/Cargo.toml --target-dir $T/target -p store -p attach -p tehas-dispatch -p tehas-cycle -p console
debug=absent"
t_contains "$t_out" "a_test.sh bin=$T/target/debug/store why="$'\n'
t_contains "$t_out" "b_test.sh bin=$T/target/debug/store why="$'\n'

t_case "without CARGO_TARGET_DIR the build is the workspace's console/target, and the second tree of before is removed"
fresh
with_console
mkdir -p "$repo/console/target/shell/debug"
t_stub cargo 'prev=""
for a in "$@"; do [ "$prev" != --target-dir ] || td=$a; prev=$a; done
mkdir -p "$td/debug" && echo store >"$td/debug/store"'
saw_store a_test.sh
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs" CARGO_TARGET_DIR=)
run
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
t_eq "$t_rc" 0
t_contains "$(t_calls cargo)" "--target-dir $repo/console/target -p store"
t_contains "$t_out" "a_test.sh bin=$repo/console/target/debug/store why="$'\n'
t_eq "$(ls "$repo/console/target")" "debug"

t_case "a store that does not build is named to the test files, which still run"
fresh
with_console
t_stub cargo 'echo "error: could not compile store" >&2; exit 101'
saw_store a_test.sh
# A store named by the caller's environment is not the one this run built.
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs" T_STORE_BIN=/elsewhere/store)
run
T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
t_eq "$t_rc" 0
t_contains "$t_out" "a_test.sh bin= why=the store did not build offline: error: could not compile store"

t_case "a store missing crates offline is fetched once and built again"
fresh
with_console
# The first build fails as a cache without the locked crates does; after the
# fetch the build succeeds.
t_stub cargo 'prev=""
for a in "$@"; do [ "$prev" != --target-dir ] || td=$a; prev=$a; done
case "$1" in
  fetch) : >"$T/fetched" ;;
  build) [ -e "$T/fetched" ] || { echo "error: no matching package named signal-hook found" >&2; exit 101; }
    mkdir -p "$td/debug" && echo store >"$td/debug/store" ;;
esac'
saw_store a_test.sh
run
rm -f "$T/fetched"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo | awk '{print $1}' | tr '\n' ' ')" "build fetch build "
t_contains "$(t_calls cargo)" "fetch --locked --quiet --manifest-path $repo/console/Cargo.toml"
t_contains "$t_out" "a_test.sh bin=$T/target/debug/store why="$'\n'

t_case "a checkout without the console, or a host without cargo, builds nothing"
fresh
t_stub cargo
saw_store a_test.sh
run
t_eq "$(t_calls cargo)" ""
t_contains "$t_out" "a_test.sh bin= why=this checkout has no console/Cargo.toml"
fresh
with_console
saw_store a_test.sh
no_shellcheck
t_contains "$t_out" "a_test.sh bin= why=cargo is not installed"

t_case "a checkout with nothing to check is exit 2, not a pass"
fresh
run
t_eq "$t_rc" 2
t_contains "$t_out" "nothing could be checked"
t_lacks "$t_out" "failed checks"

t_case "paths given run only the test files among them and lint only those"
fresh
fake a_test.sh 0
fake b_test.sh 1
mkdir -p "$repo/lib"
printf 'true\n' >"$repo/lib/one.sh"
printf 'true\n' >"$repo/lib/two.sh"
t_stub shellcheck 'exit 0'
t_run "cd '$repo' && bash runner/runner.sh test/a_test.sh ./lib/one.sh host/docs/x.md"
t_eq "$t_rc" 0
t_contains "$t_out" "ran a_test.sh line 1"
t_lacks "$t_out" "b_test.sh"
t_contains "$t_out" "1 files"
t_eq "$(t_calls shellcheck | sort)" "-x $repo/lib/one.sh
-x $repo/test/a_test.sh"

t_case "a test file given that does not exist is exit 2"
fresh
fake a_test.sh 0
t_run "cd '$repo' && bash runner/runner.sh test/gone_test.sh"
t_eq "$t_rc" 2
t_contains "$t_out" "no test file test/gone_test.sh"

t_case "paths given with nothing to test or lint among them pass"
fresh
fake a_test.sh 0
t_run "cd '$repo' && bash runner/runner.sh host/docs/x.md"
t_eq "$t_rc" 0
t_contains "$t_out" "no test file and no linted script among the paths given"
t_lacks "$t_out" "ran a_test.sh"

# What a test file prints, one 'leaked <name>' line for each of the settings it
# can see, so a case names each one instead of pinning the order of the names:
# no line at all is the pass for a file that sourced the harness.
settings_body() {
  printf '%s\n' 'for n in TEHAS_REPO PI_RUKAS_FOLLOW GH_TOKEN; do' \
    '[ -z "${!n-}" ] || printf "leaked %s\n" "$n"' 'done'
}

t_case "a test file that sources the harness sees none of the host's settings"
fresh
# The scratch checkout stands in for this one: the runner runs a test file from
# its root, so that is where the helper has to be for '. test/lib.sh' to find
# it (fresh() removes the directory, hence a call of its own after it).
cp "$T_ROOT/test/lib.sh" "$repo/test/lib.sh" || t_bad "no harness to source"
fake_body a_test.sh '. test/lib.sh' "$(settings_body)"
# Exported by the runner's command, not by this file: lib.sh is sourced at the
# top of this one and took them out of its own shell, so an export of this
# file's would say nothing about what a child inherits.
run_env TEHAS_REPO=leak/leak PI_RUKAS_FOLLOW=commit:abc GH_TOKEN=x
# What the file saw is in its own output, which the runner prints: no 'leaked'
# line at all is the pass, and the case below shows what a run that saw them
# prints.
t_eq "$t_rc" 0
t_lacks "$t_out" "leaked TEHAS_REPO"
t_lacks "$t_out" "leaked PI_RUKAS_FOLLOW"
t_lacks "$t_out" "leaked GH_TOKEN"

t_case "the runner does give a test file that skips the harness all three"
fresh
fake_body a_test.sh "$(settings_body)"
run_env TEHAS_REPO=leak/leak PI_RUKAS_FOLLOW=commit:abc GH_TOKEN=x
t_eq "$t_rc" 0
t_contains "$t_out" "leaked TEHAS_REPO"
t_contains "$t_out" "leaked PI_RUKAS_FOLLOW"
t_contains "$t_out" "leaked GH_TOKEN"

t_case "the manual's Budget: line under scripts/test.sh gives seconds for the whole gate"
# The one place the wall clock of scripts/verify.sh is written down (tools.md,
# under ## scripts/test.sh). The line is read from the manual, not from a run:
# what this case measures is that the manual says a number, and the measurement
# of that number is a run of the gate itself.
tools_md=$T_ROOT/host/docs/tools.md
[ -f "$tools_md" ] || t_bad "$tools_md is not there"
# Only the section of the runner: a Budget: line of another command is none of
# this case's business, and a line outside the section says nothing about this.
budget=$(awk '/^## scripts\/test\.sh$/{inside=1; next} /^## /{inside=0} inside && /^Budget: /' "$tools_md")
if [ -z "$budget" ]; then
  t_bad "no Budget: line under '## scripts/test.sh' of host/docs/tools.md"
else
  t_eq "$(printf '%s\n' "$budget" | wc -l)" 1
  t_contains "$budget" "scripts/verify.sh"
  # The number of seconds is the point of the line: no seconds, no budget.
  seconds=$(printf '%s\n' "$budget" | sed -nE 's/^Budget: ([0-9]+) s\b.*/\1/p' | head -n 1)
  if [ -z "$seconds" ]; then
    t_bad "the Budget: line names no number of seconds: $budget"
  elif [ "$seconds" -lt 60 ]; then
    t_bad "the Budget: line gives $seconds s, under a minute for the whole gate: $budget"
  fi
fi

t_done
