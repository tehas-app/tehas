#!/usr/bin/env bash
# Tests for host/sandbox/gh-shim: the gh of a cycle's container, which reads the
# GitHub App's token and tries a rate-limited call once more. The real gh is a
# stub named by TEHAS_GH_BIN, and TEHAS_GH_RETRY_WAIT=0 keeps a case from
# waiting. The stub keeps no state but a counter file: a case writes the answer
# of each of its calls to a file of its own, and the stub runs the file of its
# own call number, which prints what the case gave and ends with that status.
#
# t_run folds the stdout and the stderr of a run together, so a case cannot read
# the two streams of the shim apart from it. It runs the shim with its stdout and
# its stderr sent to files of their own, so one reading answers what gh gave and
# the other what the shim itself said, and t_run still gives the status.
# shellcheck disable=SC2016  # the bodies written here run in the stub, not here
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

shim="$T_ROOT/host/sandbox/gh-shim"
token=/run/tehas/github/token

# The answer of the nth call, as a script the stub reads: what gh printed on
# stdout, what it printed on stderr, and the status it ended with. The text of an
# answer is printed by printf of the stub, held between double quotes, so a $ of
# a value is left as it stands until the case that wants it filled in says so,
# and \n of an answer stands for the newline that puts a 403's rate limit on a
# line of its own, as GitHub does.
answer() { # <call> <out> <err> <rc>
  local out=${2//\//\\}
  out=${out//\$}
  local err=${3//\//\\}
  err=${err//\$}
  {
    printf '%s\n' "printf %s \"$out\""
    printf '%s\n' "printf %s \"$err\" >&2"
    printf '%s\n' "exit $4"
  } >"$T/answers/$1.sh"
}
# A case starts here: no answer of the case before, and the counter at nought.
stub_reset() {
  rm -rf "$T"/calls/* "$T/answers" "$T/n" "$T/shim.out" "$T/shim.err"
  mkdir -p "$T/answers"
  : >"$T/n"
  cat >"$T/bin/gh" <<'S'
#!/usr/bin/env bash
n=$(($(cat "$T/n") + 1)); echo "$n" >"$T/n"
for a in "$@"; do printf "%s " "$a" >>"$T/calls/gh"; done
printf "\n" >>"$T/calls/gh"
# The answer is a script of the case, read here, so an answer may print itself
# with a command that fills a $ of a value in.
. "$T/answers/$n.sh"
S
  chmod +x "$T/bin/gh"
}
# How many times the stub was called: its own count, which no answer moves.
calls() { cat "$T/n" 2>/dev/null || echo 0; }
# What the stub was called with, one call per line.
seen() { cat "$T/calls/gh" 2>/dev/null || true; }
# The shim, run with its two streams sent to files of their own.
run_shim() { # <args...>
  T_ENV=(TEHAS_GH_BIN="$T/bin/gh" TEHAS_GH_RETRY_WAIT=0)
  t_run "$shim $* >'$T/shim.out' 2>'$T/shim.err'"
  T_ENV=()
}
retry="gh-shim: rate limited, trying once more in 0 s"

t_case "a call that succeeds is made once, and its output and status come back"
stub_reset
answer 1 "the issue" "" 0
run_shim "pr view 7"
t_eq "$t_rc" 0
t_eq "$(cat "$T/shim.out")" "the issue"
t_eq "$(cat "$T/shim.err")" ""
t_eq "$(calls)" 1

t_case "HTTP 429 then success is called twice and answers the second call"
stub_reset
answer 1 "" "HTTP 429: Too Many Requests" 1
answer 2 "the issue" "" 0
run_shim "pr comment 7 --body hi"
t_eq "$t_rc" 0
t_eq "$(cat "$T/shim.out")" "the issue"
t_eq "$(cat "$T/shim.err")" "$retry"
t_eq "$(calls)" 2
t_eq "$(seen)" "pr comment 7 --body hi 
pr comment 7 --body hi "

t_case "HTTP 429 every time is called twice and answers the second call's status"
stub_reset
answer 1 "" "HTTP 429: Too Many Requests" 1
answer 2 "" "HTTP 429: Too Many Requests" 1
run_shim "pr view 7"
t_eq "$t_rc" 1
t_eq "$(cat "$T/shim.out")" ""
t_eq "$(cat "$T/shim.err")" "$retry
HTTP 429: Too Many Requests"
t_eq "$(calls)" 2

t_case "a 403 whose rate limit sits on the next line is retried"
stub_reset
answer 1 "" "HTTP 403: Forbidden\nAPI rate limit exceeded for 203.0.113.5." 1
answer 2 "the issue" "" 0
run_shim "api repos/acme/widget"
t_eq "$t_rc" 0
t_contains "$(cat "$T/shim.err")" "$retry"
t_eq "$(calls)" 2

t_case "a failure that is no rate limit is made once and answered as it was"
stub_reset
answer 1 "" "HTTP 404: Not Found" 1
run_shim "pr view 7"
t_eq "$t_rc" 1
t_eq "$(cat "$T/shim.out")" ""
t_eq "$(cat "$T/shim.err")" "HTTP 404: Not Found"
t_eq "$(calls)" 1

t_case "the words of the rate limit are read across lines and in any case"
stub_reset
answer 1 "" "HTTP 429 RATE limit\nexceeded" 1
answer 2 "the issue" "" 0
run_shim "pr view 7"
t_eq "$t_rc" 0
t_eq "$(calls)" 2

t_case "a call that reads standard input is not retried"
stub_reset
answer 1 "" "HTTP 429: Too Many Requests" 1
answer 2 "the issue" "" 0
T_ENV=(TEHAS_GH_BIN="$T/bin/gh" TEHAS_GH_RETRY_WAIT=0)
t_run "printf hi | $shim pr comment 7 --body-file - >'$T/shim.out' 2>'$T/shim.err'"
T_ENV=()
t_eq "$t_rc" 1
t_eq "$(cat "$T/shim.out")" ""
t_eq "$(cat "$T/shim.err")" "HTTP 429: Too Many Requests"
t_eq "$(calls)" 1
t_lacks "$(cat "$T/shim.err")" "trying once more"

t_case "the shim leaves no temporary file of its own"
stub_reset
answer 1 "" "HTTP 429: Too Many Requests" 1
answer 2 "the issue" "" 0
T_ENV=(TEHAS_GH_BIN="$T/bin/gh" TEHAS_GH_RETRY_WAIT=0)
t_run "$shim pr view 7 >/dev/null 2>&1
  ls \${TMPDIR:-/tmp} | grep -c '^gh-shim' || true"
T_ENV=()
t_eq "$t_out" "0"

t_case "gh is called with the token the container holds"
if [ ! -r "$token" ]; then
  t_skip "no token readable at $token: the container's case cannot be made here"
else
  stub_reset
  # The one answer whose text is named as a $ of a value rather than written
  # out: the printf of the stub fills it in, so gh is seen with the token of the
  # container, which is what a real gh holds.
  printf '%s\n' 'printf %s "token=[$GH_TOKEN]"' >"$T/answers/1.sh"
  printf '%s\n' 'printf %s "" >&2' >>"$T/answers/1.sh"
  printf '%s\n' 'exit 0' >>"$T/answers/1.sh"
  run_shim "pr view 7"
  t_eq "$(cat "$T/shim.out")" "token=[$(cat "$token")]"
fi

t_case "gh's own status comes back from a call that is not a rate limit"
stub_reset
answer 1 "" "a refusal" 7
run_shim "pr view 7"
t_eq "$t_rc" 7

t_case "shellcheck -s sh reads the shim"
# scripts/test.sh lints the shell scripts of bin/, lib/, libexec/ and test/, and
# this one is none of them, so the shim is linted here as the sh it is.
if ! command -v shellcheck >/dev/null 2>&1; then
  t_skip "no shellcheck on this host: the gate's own run covers a real lint"
else
  t_run "shellcheck -s sh '$shim'"
  t_eq "$t_rc" 0
  t_eq "$t_out" ""
fi

t_done
