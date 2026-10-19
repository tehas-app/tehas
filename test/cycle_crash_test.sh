#!/usr/bin/env bash
# Tests for libexec/cycle: a cycle whose runner died and wrote no result. Its
# step comes from the step-started lines of its run log, its pull request from
# GitHub, and the report is a handoff whose review did not finish - not a park
# that offers a clean retry. The runner's own status is named, not tehas work's.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

# The branch of #7 as pi-rukas names it, which the log of a run that reached
# commit-pr names as the step right after it. tehas work writes no branch file,
# so a branch named as an argument to died() is a file left behind, not one this
# cycle wrote.
BRANCH=feature/issue-7-install-sh-checks-instance-env

t_case "a runner that died after commit-pr is reported with its pull request, not as a park with a clean retry"
reset
died explore plan develop commit-pr "$BRANCH"
open_pr https://github.com/acme/widget/pull/347
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head $BRANCH --author @me --state all"
t_contains "$(t_calls gh)" "Tehas cycle handed off with its pull request open and its review unfinished"
t_contains "$(t_calls gh)" "last step \`$BRANCH\` read from its run log"
t_contains "$(t_calls gh)" "https://github.com/acme/widget/pull/347"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
# Not a park's question: no boxes, and nothing on record for the poll to read.
t_lacks "$(t_calls gh)" "Retry from a clean start"
t_lacks "$(t_calls gh)" "tehas:question"
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_eq "$(find "$T/home/tehas/state/park-7.id" 2>/dev/null | wc -l)" 0
t_eq "$(find "$state/7.park" 2>/dev/null | wc -l)" 0

t_case "the runner's own status is named, and the poll's line keeps the status of tehas work"
reset
died explore plan develop commit-pr "$BRANCH" rc=134
open_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "(runner exit 134)"
t_contains "$t_out" "done #7 rc=2 "

t_case "with no pull request on the branch the cycle is parked as any other that left none"
reset
died explore plan develop commit-pr "$BRANCH"
echo "[]" >"$fx/pr.json"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head $BRANCH --author @me --state all"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_lacks "$(t_calls gh)" "handed off with its pull request open"

t_case "a merged pull request of the bot on that branch ends the cycle as merged, not as a handoff"
reset
died explore plan develop commit-pr "$BRANCH"
merged_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle ended: its pull request https://github.com/acme/widget/pull/9 was merged while the cycle ran"
t_lacks "$(t_calls gh)" "review unfinished"

t_case "a closed pull request of the bot on that branch is said as one that was closed"
reset
died explore plan develop commit-pr "$BRANCH"
echo "https://github.com/acme/widget/pull/347" >"$fx/pr.txt"
echo CLOSED >"$fx/pr-state"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "was closed without being merged"
t_lacks "$(t_calls gh)" "review unfinished"

t_case "a runner that died before commit-pr asks GitHub about no branch and is parked"
reset
died explore plan develop
t_run "$cycle 7"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "pr list --repo"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" 'last step `develop` read from its run log'

t_case "a step after commit-pr that is no branch of this issue asks GitHub nothing"
reset
# pi-rukas names the branch it pushed as the step of the push, so the one step
# right after commit-pr is the branch. A step name of any other shape is a step,
# not a branch, and asks GitHub nothing.
died explore plan develop commit-pr lens-review
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "pr list --repo"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"

t_case "a pull request of the bot that names another issue is said, not taken as this cycle's"
reset
died explore plan develop commit-pr "$BRANCH"
# The stub answers with a branch other than the one asked for, and a body that
# names another issue: the bot's, on this branch, but of #8.
echo 8 >"$fx/pr-body-issue"
jq -n '[{url: "https://github.com/acme/widget/pull/347", state: "OPEN",
         body: "Work on another thing", headRefName: "feature/issue-8-another-branch"}]' >"$fx/pr.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "does not name this issue"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_lacks "$(t_calls gh)" "review unfinished"

t_case "a run whose steps stop at commit-pr names no branch"
reset
# The push is the step after commit-pr. A runner that died in commit-pr itself
# left nothing pushed, so GitHub is asked about no branch of its own.
died explore plan develop commit-pr
open_pr
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "pr list --repo"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"

t_case "the runner's own status is named, and the poll's line keeps the status of tehas work"
reset
died explore plan develop commit-pr "$BRANCH" rc=134
open_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "(runner exit 134)"
t_contains "$t_out" "done #7 rc=2 "

t_case "GitHub refusing the list is said as the unknown it is, with the branch on record"
reset
died explore plan develop commit-pr "$BRANCH"
echo 3 >"$fx/pr-fails-times"
t_run "$cycle 7"
t_contains "$t_out" "WARN: the pull requests of $BRANCH could not be read"
t_contains "$(t_calls gh)" "its pull request could not be looked up"
t_eq "$(cat "$state/7.branch")" "$BRANCH"

t_case "the step of a cycle with no log at all stays a dash"
reset
died
t_run "$cycle 7"
t_contains "$t_out" "done #7 rc=2 status=unknown step=-"

t_case "a run whose tehas work wrote nothing at all is read as a cycle that started no container"
reset
died explore plan develop commit-pr "$BRANCH"
# No runner_exit and no progress: tehas work stopped before it watched a
# container, so the log of an earlier run of the issue is not read for this one.
cat >"$fx/work.sh" <<'EOS'
mkdir -p "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/logs/work-7-261017-012100.jsonl"
printf '{"v":1,"kind":"step-started","issue":7,"event":{"step":"lied-step"}}\n' \
  >>"$TEHAS_HOME/logs/work-7-261017-012100.jsonl"
exit 2
EOS
open_pr
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "lied-step"
t_lacks "$(t_calls gh)" "read from its run log"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_lacks "$(t_calls gh)" "pr list --repo"

t_case "a result that was written names the step: nothing of a log is read for it"
reset
ended handoff commit-pr feature/issue-7-x review-incomplete
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 'echo "{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"lied-step\"}}" >"$TEHAS_HOME/logs/work-7-261018-012100.jsonl"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "lied-step"
t_lacks "$(t_calls gh)" "read from its run log"

t_case "a runner killed after commit-pr is reported with its pull request, as any other that died"
reset
died explore plan develop commit-pr "$BRANCH" rc=137 status=killed
open_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle handed off with its pull request open and its review unfinished"
t_contains "$(t_calls gh)" "killed"
t_lacks "$(t_calls gh)" "Retry from a clean start"

t_case "a runner killed before commit-pr is reported as killed and parked"
reset
died explore plan develop rc=137 status=killed
open_pr
t_run "$cycle 7"
t_contains "$t_out" "status=killed"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"

t_case "the line of history of a runner that died names the step read from its log and its pull request"
reset
died explore plan develop commit-pr "$BRANCH"
open_pr https://github.com/acme/widget/pull/347
t_run "$cycle 7"
# The exit of tehas work is the one the line carries, as for every other end of
# a cycle: tehas work exits 2 for a runner that died however it died, and what
# the runner itself ended with is in the comment, not the line.
t_eq "$(jq -c '[.status, .step, .rc, .pr, .parked]' "$state/7.history.jsonl")" \
  "[\"handoff\",\"$BRANCH\",2,\"https://github.com/acme/widget/pull/347\",false]"

t_case "the runner's own status file is not left for the next cycle of the issue"
reset
died explore plan develop commit-pr "$BRANCH" rc=134 work=2
open_pr
t_run "$cycle 7"
# tehas work writes the file for the report; the cycle reads it and lets it
# go, so the next cycle of the issue does not report this runner's end.
t_eq "$(find "$state/7.runner_exit" 2>/dev/null | wc -l)" 0
t_eq "$(find "$state/7.status" 2>/dev/null | wc -l)" 0

t_case "a runner_exit and a status of an earlier cycle are not this cycle's end"
reset
# The two files are cleared when the cycle takes its lock, before tehas work
# starts, so neither reaches the report of a later cycle: the words here are the
# earlier run's, and its log names a step this cycle never reached. The files go
# where tehas work leaves them, because this cycle starts no tehas work.
mkdir -p "$state" "$T/home/tehas/logs"
printf '134\n' >"$state/7.runner_exit"
printf 'killed\n' >"$state/7.status"
: >"$state/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$state/7.progress.json"
printf '%s\n' '{"v":1,"kind":"step-started","issue":7,"event":{"step":"lied-step"}}' \
  >"$T/home/tehas/logs/work-7-261019-012100.jsonl"
mkdir -p "$workdir"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "lied-step"
t_lacks "$(t_calls gh)" "read from its run log"
t_lacks "$(t_calls gh)" "(runner exit 134)"
t_lacks "$(t_calls gh)" "pr list --repo"
t_eq "$(find "$state/7.runner_exit" 2>/dev/null | wc -l)" 0
t_eq "$(find "$state/7.status" 2>/dev/null | wc -l)" 0

t_case "of several run logs of the issue only the newest is read"
reset
died explore plan develop commit-pr "$BRANCH"
# tehas work names the log for the UTC time of its start to the second, so the
# file of an earlier run has the earlier name and its steps are not this
# cycle's. A name a day old is earlier than any log of this second.
printf '%s\n' '{"v":1,"kind":"step-started","issue":7,"event":{"step":"lied-step"}}' \
  >"$T/home/tehas/logs/work-7-$(date -u -d yesterday +%y%m%d-%H%M%S).jsonl"
open_pr
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "lied-step"
t_contains "$(t_calls gh)" "last step \`$BRANCH\` read from its run log"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head $BRANCH --author @me --state all"

t_case "the table of a run whose result carries no usage totals the rows it did write"
reset
ended handoff commit-pr feature/issue-7-x
ended_no_usage_with_rows
open_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| planner | 2 | 3 | 7 | 1 | 0 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 3 | 5 | 10 | 5 | 5 |"

t_case "a row with a word in place of one of its counts is not summed into the totals"
reset
ended handoff commit-pr feature/issue-7-x
ended_no_usage_with_a_sly_row
open_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| planner | 2 | 0 | 7 | 0 | 0 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 0 | 0 | 0 | 0 | 0 |"

t_done
