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
# scripts/test.sh lints this shim and host/shim/gh with shellcheck -x, which
# takes the shell from the shebang. Here the shell is named by hand: the shim is
# written in sh, and a bash-only construct is what -s sh says something about.
t_run "shellcheck -s sh '$shim'"
t_eq "$t_rc" 0
t_eq "$t_out" ""

# A read-only instance (#608): the shim of a container given TEHAS_READ_ONLY=1.
ro_shim() { # <args...>
  T_ENV=(TEHAS_GH_BIN="$T/bin/gh" TEHAS_GH_RETRY_WAIT=0 TEHAS_READ_ONLY=1 PI_ENSEMBLE_WORKSPACE_ROOT="$T/ws")
  t_run "$shim $* >'$T/shim.out' 2>'$T/shim.err'"
  T_ENV=()
}

t_case "read-only: the reads a cycle makes still reach gh"
for call in "issue view 7 --json body" "pr list --head feature/issue-7" "pr view 12" "api repos/acme/widget/issues/7/comments" \
  "api -X GET repos/acme/widget/pulls -f state=open" "api graphql -f query='query { viewer { login } }'" "auth git-credential" \
  "repo clone acme/widget" "auth setup-git"; do
  stub_reset
  answer 1 "the answer" "" 0
  ro_shim "$call"
  t_eq "$t_rc" 0
  t_eq "$(cat "$T/shim.out")" "the answer"
  t_eq "$(calls)" 1
done

t_case "read-only: a call that may write is not run, and the shim says what it would have done"
for call in "issue comment 7 --body hi" "issue edit 7 --add-label x" "pr create --title t --body b" "pr merge 12" "label create x" \
  "api -X POST repos/acme/widget/issues/7/labels" "api repos/acme/widget/issues/7/comments -f body=hi" \
  "api --method=PATCH repos/acme/widget/issues/7" "api graphql -f query='mutation { addComment }'" "release upload v1 a.tgz" "repo edit"; do
  stub_reset
  ro_shim "$call"
  t_eq "$t_rc" 0
  t_eq "$(seen)" ""
  t_eq "$(cat "$T/shim.out")" ""
  read -r one two _ <<<"$call"
  t_eq "$(cat "$T/shim.err")" "read-only: would gh $one $two"
done

t_case "read-only: a write that reads standard input is refused before it reads it"
stub_reset
T_ENV=(TEHAS_GH_BIN="$T/bin/gh" TEHAS_READ_ONLY=1)
t_run "printf hi | $shim issue comment 7 --body-file - 2>'$T/shim.err'"
T_ENV=()
t_eq "$t_rc" 0
t_eq "$(seen)" ""
t_eq "$(cat "$T/shim.err")" "read-only: would gh issue comment"

t_case "read-only: a refused pr create leaves the pull request's text in the workspace's git directory"
stub_reset
rm -rf "$T/ws"
git init -q "$T/ws"
printf 'The body.\nTwo lines.\n' >"$T/body.md"
ro_shim "pr create --title 'Fix the thing' --head feature/issue-7 --body-file '$T/body.md'"
t_eq "$t_rc" 0
t_eq "$(seen)" ""
t_eq "$(cat "$T/ws/.git/tehas-read-only-pr.md")" "# Fix the thing

Branch: feature/issue-7

The body.
Two lines."
t_eq "$(git -C "$T/ws" status --porcelain)" ""

t_case "without read-only a write goes to gh as before"
stub_reset
answer 1 "done" "" 0
T_ENV=(TEHAS_GH_BIN="$T/bin/gh" TEHAS_READ_ONLY=0)
t_run "$shim issue comment 7 --body hi"
T_ENV=()
t_eq "$(calls)" 1

t_done
