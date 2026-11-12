#!/usr/bin/env bash
# Tests for the gh shim's write log: the rate-limit back-off. A write GitHub
# refused for a rate limit is sent once more and holds up no other call. The
# real gh is a stub (t_stub) that answers what GH_OUT, GH_ERR and GH_RC name:
# no network, and no GitHub asked of anything.
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/shim_writes_lib.sh
. "$(dirname "$0")/shim_writes_lib.sh" || exit 1
t_case "a call GitHub rate limited twice keeps gh's status and stderr of the second, and both lines end 429"
reset
T_ENV=(GH_OUT="gh issue comment 12 -R o/r --body secret" GH_ERR="HTTP 429: Too Many Requests" GH_RC=1 TEHAS_GH_RETRY_WAIT=0)
shim issue comment 12 -R o/r --body secret
t_eq "$t_rc" 1
t_eq "$t_out" "gh: rate limited, trying once more in 0 s
gh issue comment 12 -R o/r --body secret
HTTP 429: Too Many Requests"
t_eq "$(t_calls gh | wc -l)" 2
t_eq "$(lines | wc -l)" 2
t_eq "$(lines | cut -f5,6 | sort -u)" "1	429"

# Back-off (#649 part 2): a write GitHub refused for a rate limit is sent once
# more after TEHAS_GH_RETRY_WAIT seconds, and only that call.
t_case "a write refused with 429 once is sent again and succeeds, with the second answer only"
reset
t_gh_fail 1 1 "HTTP 429: Too Many Requests"
T_ENV=(TEHAS_GH_RETRY_WAIT=0)
shim_split issue comment 12 -R o/r --body secret
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue comment 12 -R o/r --body secret"
t_eq "$t_err" "gh: rate limited, trying once more in 0 s"
t_eq "$(t_calls gh | wc -l)" 2
t_eq "$(lines | cut -f5,6)" "1	429
0	-"

t_case "a 403 that names the rate limit on its next line is sent again"
reset
T_ENV=(GH_ERR="HTTP 403: Forbidden
API rate limit exceeded for installation" GH_RC=1 TEHAS_GH_RETRY_WAIT=0)
shim api -X POST repos/o/r/issues/3/labels -f 'labels[]=x'
t_eq "$t_rc" 1
t_eq "$(t_calls gh | wc -l)" 2

t_case "a refusal that is no rate limit is not sent again"
reset
T_ENV=(GH_ERR="HTTP 404: Not Found" GH_RC=1 TEHAS_GH_RETRY_WAIT=0)
shim issue comment 3 -R o/r --body x
t_eq "$t_rc" 1
t_eq "$t_out" "gh issue comment 3 -R o/r --body x
HTTP 404: Not Found"
t_eq "$(t_calls gh | wc -l)" 1
T_ENV=(GH_ERR="HTTP 403: Resource not accessible by integration" GH_RC=1 TEHAS_GH_RETRY_WAIT=0)
shim issue comment 3 -R o/r --body x
t_eq "$(t_calls gh | wc -l)" 2

t_case "a write that reads standard input is sent once, even after a 429"
reset
T_ENV=(GH_ERR="HTTP 429: Too Many Requests" GH_RC=1 TEHAS_GH_RETRY_WAIT=0)
t_run "echo words | $T/home/tehas/shim/gh issue comment 3 -R o/r --body-file -"
t_eq "$t_rc" 1
t_lacks "$t_out" "rate limited"
t_eq "$(t_calls gh | wc -l)" 1
t_eq "$(field 6)" "429"

# A write at a terminal (#649 review): gh's output stays the terminal, so gh
# can still prompt, and the call is sent once. script(1) gives the shim a
# terminal for its input and output; the stub says what its stdout is.
t_case "a write at a terminal keeps the terminal as gh's output and is sent once, even after a 429"
if command -v script >/dev/null; then
  reset
  T_ENV=(GH_ERR="HTTP 429: Too Many Requests" GH_RC=1 TEHAS_GH_RETRY_WAIT=0 SHOW_TTY=1)
  t_run "script -qec $(printf '%q' "$T/home/tehas/shim/gh pr create -R o/r") /dev/null </dev/null"
  t_contains "$t_out" stdout-tty
  t_lacks "$t_out" "rate limited"
  t_eq "$(t_calls gh | wc -l)" 1
  t_eq "$(field 6)" "429"
  # The same write with its output a pipe is held back and sent again.
  reset
  T_ENV=(GH_ERR="HTTP 429: Too Many Requests" GH_RC=1 TEHAS_GH_RETRY_WAIT=0 SHOW_TTY=1)
  shim pr create -R o/r
  t_contains "$t_out" stdout-pipe
  t_contains "$t_out" "rate limited"
  t_eq "$(t_calls gh | wc -l)" 2
else
  t_skip "no script(1)"
fi

t_case "a write that waits out a rate limit holds up no other call"
reset
# Call 1 is refused and its shim waits a minute before the second attempt. A
# write and a read made meanwhile, in processes of their own, answer at once.
t_gh_fail 1 1 "HTTP 429: Too Many Requests"
(
  T_ENV=(TEHAS_GH_RETRY_WAIT=60)
  t_run "$T/home/tehas/shim/gh issue comment 777 -R o/r --body waits"
) >/dev/null 2>&1 &
waiter=$!
for _ in $(seq 600); do
  [ "$(lines | wc -l)" -lt 1 ] || break
  sleep 0.1
done
t_eq "$(lines | wc -l)" 1
T_ENV=()
shim issue comment 12 -R o/r --body other
t_eq "$t_rc" 0
shim issue view 12 -R o/r
t_eq "$t_rc" 0
t_eq "$(lines | cut -f4,6)" "777	429
12	-"
# The waiting call is still in its wait: it has made one attempt only.
t_eq "$(t_calls gh | grep -c 777)" 1
pkill -f "issue comment 777" 2>/dev/null
wait "$waiter" 2>/dev/null

t_done
