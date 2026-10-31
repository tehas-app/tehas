#!/usr/bin/env bash
# Tests for libexec/cycle: a stop or a hangup while it runs, and the calls to
# GitHub it makes again when they are refused.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

t_case "a stop is passed on to the runner, and the cycle is still reported"
reset
# The runner says when its trap is set, so the signal cannot arrive before it.
echo 'sleep 30 & s=$!
trap "kill \$s; : >\"\$T/calls/work.term\"; exit 2" TERM
: >"$T/calls/work.ready"
wait "$s"' >"$fx/work.sh"
# The wait is bounded: a cycle that never starts its runner must fail this
# case, not hang the gate.
t_run "$cycle 7 & pid=\$!
for _ in \$(seq 100); do [ -e \"\$T/calls/work.ready\" ] && break; sleep 0.1; done
kill -TERM \$pid; wait \$pid; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_eq "$(find "$T/calls/work.term" | wc -l)" 1
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "(runner exit 2)"

t_case "a stop that arrives while the report is made does not cut the report short"
reset
: >"$fx/edit-slow"
# The first relabel takes a second and says when it starts. The wait is bounded.
t_run "$cycle 7 & pid=\$!
for _ in \$(seq 100); do [ -e \"\$T/calls/edit.started\" ] && break; sleep 0.1; done
kill -TERM \$pid; wait \$pid; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_contains "$(t_calls gh)" "issue edit 7"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:question"

t_case "a stop while GitHub does not answer is reported with one bounded try a call, and no status refresh"
reset
T_ENV+=(TEHAS_CYCLE_STOP_GH_WAIT=1)
ended running develop feature/x
# GitHub stops answering once the cycle runs.
echo 'sleep 30 & s=$!
trap "kill \$s; exit 2" TERM
: >"$T/fx/gh-hangs"
: >"$T/calls/work.ready"
wait "$s"' >"$fx/work.sh"
t_run "$cycle 7 & pid=\$!
for _ in \$(seq 100); do [ -e \"\$T/calls/work.ready\" ] && break; sleep 0.1; done
start=\$(date +%s); kill -TERM \$pid; wait \$pid; echo rc=\$? seconds=\$((\$(date +%s) - start))"
t_contains "$t_out" "rc=0"
# Four calls, each cut after a second. Three tries of each, or the
# default bound of 20 seconds, would take far longer. The report ignores TERM,
# so a bound that only sent TERM would not cut a call.
secs=$(sed -n 's/.*seconds=\([0-9]*\).*/\1/p' <<<"$t_out")
t_eq "$([ -n "$secs" ] && [ "$secs" -le 15 ] && echo bounded)" bounded
t_eq "$(t_calls gh | grep -c -e '--remove-label tehas:running')" 1
t_eq "$(t_calls gh | grep -c -e '^pr list ')" 1
t_eq "$(t_calls fake-status)" ""
t_contains "$t_out" "WARN: label change on #7 failed"

t_case "a cycle that was not stopped still refreshes the status and tries three times"
reset
echo "--remove-label tehas:running" >"$fx/edit-fails-on"
t_run "$cycle 7"
t_eq "$(t_calls gh | grep -c -e '--remove-label tehas:running')" 3
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "a hangup is a stop too: the cycle is still reported"
reset
echo 'sleep 30 & s=$!
trap "kill \$s; : >\"\$T/calls/work.term\"; exit 2" TERM
: >"$T/calls/work.ready"
wait "$s"' >"$fx/work.sh"
# A gate started under nohup, as an agent's runner may start it, ignores HUP,
# and bash cannot trap a signal ignored at its start: the cycle gets HUP back.
t_run "env --default-signal=HUP $cycle 7 & pid=\$!
for _ in \$(seq 100); do [ -e \"\$T/calls/work.ready\" ] && break; sleep 0.1; done
kill -HUP \$pid; wait \$pid; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_eq "$(find "$T/calls/work.term" | wc -l)" 1
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a label change that GitHub refuses once is made again"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 1 >"$fx/edit-fails-times"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -e '--remove-label tehas:running --add-label tehas:awaiting-merge')" 2
t_lacks "$t_out" "WARN: label change"
t_contains "$(t_calls gh)" "Pull request waiting for a human"

t_case "after three refusals it gives up, says so in the log and still reports"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "--remove-label tehas:running" >"$fx/edit-fails-on"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -e '--remove-label tehas:running')" 3
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "Pull request waiting for a human"

for st in failed aborted unknown running; do
  t_case "a run that ended as $st is not said to wait for a merge, though a pull request is open"
  reset
  ended "$st" review feature/x
  echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
  t_run "$cycle 7"
  t_eq "$t_rc" 0
  t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
  t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
  t_contains "$(t_calls gh)" "Its branch has an open pull request: https://github.com/acme/widget/pull/9"
done

t_case "a comment that GitHub refuses once is posted on the second try"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 1 >"$fx/comment-fails-times"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -e '^issue comment 7 ')" 2
t_lacks "$t_out" "WARN: comment on #7 failed"
t_contains "$(t_calls gh)" "Pull request waiting for a human"
t_eq "$(t_row cycle-show 7 -- '(.cycle.ended_at != null)')" true

t_case "after three refusals the comment is said once, and the cycle still ends its record"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
: >"$fx/comment-fails"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -e '^issue comment 7 ')" 3
t_eq "$(grep -c -e 'WARN: comment on #7 failed' <<<"$t_out")" 1
t_eq "$(t_row cycle-show 7 -- '(.cycle.ended_at != null)')" true

t_case "a list of pull requests that GitHub refuses once is asked for again"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 1 >"$fx/pr-fails-times"
t_run "$cycle 7"
t_eq "$(t_calls gh | grep -c -e '^pr list ')" 2
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"

t_case "after three refusals the cycle is parked as one whose pull request could not be looked up"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 3 >"$fx/pr-fails-times"
t_run "$cycle 7"
t_eq "$(t_calls gh | grep -c -e '^pr list ')" 3
t_contains "$t_out" "WARN: the pull requests of feature/x could not be read"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "its pull request could not be looked up: GitHub did not list the pull requests of the branch \`feature/x\`"
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"

t_case "the branch whose pull request could not be looked up stays on record"
t_eq "$(t_row cycle-show 7 -- .cycle.branch)" feature/x

t_case "the branch is on record with every end, and an earlier cycle's is not the next one's"
ended handoff commit-pr feature/y
t_run "$cycle 7"
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, .cycle.branch] | tostring')" '[2,"feature/y"]'
t_contains "$(t_calls gh)" "Pull request waiting for a human"
rm -f "$fx/result.json"
t_run "$cycle 7"
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, .cycle.branch] | tostring')" '[3,null]'

t_case "a branch name the container wrote is reduced before it is recorded"
reset
ended handoff commit-pr 'feature/x`;@y$z'
echo 3 >"$fx/pr-fails-times"
t_run "$cycle 7"
t_eq "$(t_row cycle-show 7 -- .cycle.branch)" feature/xyz
t_contains "$(t_calls gh)" "the branch \`feature/xyz\`"

t_case "when the label cannot be changed the comment says so"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "--remove-label tehas:running" >"$fx/edit-fails-on"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Pull request waiting for a human: https://github.com/acme/widget/pull/9 The label of this issue could not be changed"

t_case "and a comment after a label change that worked does not"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "could not be changed"

t_case "the question label is tried three times"
reset
printf '%s' '--add-label tehas:question' >"$fx/edit-fails-on"
t_run "$cycle 7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:question')" 3
t_contains "$(t_calls gh)" "The boxes above cannot be used"

t_case "a cycle that parks takes the build output of its kept workspace as it ends"
reset
ended failed develop
t_run "$cycle 7"
t_contains "$(t_calls fake-console)" "clean ended $workdir target node_modules"

t_case "and a failure to do so is a warning"
reset
ended failed develop
t_stub fake-console 'exit 1'
t_run "$cycle 7"
t_contains "$t_out" "was not removed"
t_stub fake-console

t_done
