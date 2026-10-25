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

# Where a scratch run keeps each test file's record: one run, one directory, so
# a fake test file can name its own record. The pattern is left for the fake
# file to expand, with the directory quoted and the pattern not.
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
# as a failed run naming the command, not as a block that looks wrong.
no_shellcheck() {
  local c p
  rm -rf "$nobin"
  mkdir -p "$nobin"
  for c in bash cat dirname mkdir mktemp nproc rm sed tail timeout; do
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
  t_skip "no shellcheck on this host: the gate's own run covers a real lint"
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
$(fail_line a_test.sh)
    skipped: shellcheck is not installed"

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

# Each file notes when it starts and ends. Three at a time, they overlap; on a
# host whose load is past its CPUs, the next waits for the one before.
overlap() {
  fresh
  for n in a b c; do
    printf '#!/usr/bin/env bash\necho "start %s" >>"%s"\nsleep 0.3\necho "end %s" >>"%s"\n' \
      "$n" "$T/order" "$n" "$T/order" >"$repo/test/${n}_test.sh"
  done
  rm -f "$T/order"
  T_ENV=(TEHAS_TEST_JOBS=3 TMPDIR="$recs" TEHAS_TEST_LOADAVG="$T/loadavg")
  run
  T_ENV=(TEHAS_TEST_JOBS=1 TMPDIR="$recs")
}
t_case "an idle host runs the files side by side"
echo "0.00 0.00 0.00 1/100 1" >"$T/loadavg"
overlap
t_eq "$t_rc" 0
t_eq "$(head -n 2 "$T/order" | cut -d' ' -f1 | tr '\n' ' ')" "start start "

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
  "$repo"/install.sh "$repo"/bootstrap.sh "$repo"/host/env.sh "$repo"/host/sandbox/build.sh \
  "$repo"/host/login/tehas; do
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
  t_skip "no shellcheck on this host: the gate's own run covers the severity floor"
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
  printf '%s\n' '#!/usr/bin/env bash' 'echo "$0 bin=${T_STORE_BIN:-} why=${T_STORE_WHY:-}"' >"$repo/test/$1"
  chmod +x "$repo/test/$1"
}
with_console() {
  mkdir -p "$repo/console"
  : >"$repo/console/Cargo.toml"
}

t_case "the store is built once, offline, into a target of its own, and named to every test file"
fresh
with_console
t_stub cargo 'prev=""
for a in "$@"; do [ "$prev" != --target-dir ] || td=$a; prev=$a; done
mkdir -p "$td/debug" && echo store >"$td/debug/store"'
saw_store a_test.sh
saw_store b_test.sh
run
t_eq "$t_rc" 0
t_eq "$(t_calls cargo)" "build --locked --offline --quiet --manifest-path $repo/console/Cargo.toml --target-dir $repo/console/target/shell -p store"
t_contains "$t_out" "a_test.sh bin=$repo/console/target/shell/debug/store why="$'\n'
t_contains "$t_out" "b_test.sh bin=$repo/console/target/shell/debug/store why="$'\n'

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

t_done
