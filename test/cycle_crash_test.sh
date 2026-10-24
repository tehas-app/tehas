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

t_case "GitHub refusing the list is said as the unknown it is, with the branch on record"
reset
died explore plan develop commit-pr "$BRANCH"
echo 3 >"$fx/pr-fails-times"
t_run "$cycle 7"
t_contains "$t_out" "WARN: the pull requests of $BRANCH could not be read"
t_contains "$(t_calls gh)" "its pull request could not be looked up"
t_eq "$(t_row cycle-show 7 -- .cycle.branch)" "$BRANCH"

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
# The step on record is cleaned as every step is, to [A-Za-z0-9_.-] and 40
# bytes; the comment names it as the log does.
t_eq "$(t_row history-show 7 -- '.rows[0] | [.status, .step, .rc, .pr, .parked] | tostring')" \
  "[\"handoff\",\"$(tr -cd 'A-Za-z0-9_.-' <<<"$BRANCH" | head -c 40)\",2,347,false]"

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
# The three files are cleared when the cycle takes its lock, before tehas work
# starts, so none reaches the report of a later cycle: the words here are the
# earlier run's, and its log names a step this cycle never reached. The files go
# where tehas work leaves them, because this cycle starts no tehas work. The
# summary of the run is the file that says a log may be read at all, so it is
# seeded too: a log of the earlier run is no log of this one, and no pull
# request is asked for on the strength of its branch.
mkdir -p "$state" "$T/home/tehas/logs"
printf '134\n' >"$state/7.runner_exit"
printf 'killed\n' >"$state/7.status"
: >"$state/7.result.json"
printf '{"roles":[],"reason":"","reviewHeld":false}\n' >"$state/7.progress.json"
touch -d '2 days ago' "$state/7.progress.json"
printf '%s\n' '{"v":1,"kind":"step-started","issue":7,"event":{"step":"lied-step"}}' \
  >"$T/home/tehas/logs/work-7-261019-012100.jsonl"
printf '%s\n' '{"v":1,"kind":"step-started","issue":7,"event":{"step":"feature/issue-7-from-an-earlier-run"}}' \
  >>"$T/home/tehas/logs/work-7-261019-012100.jsonl"
mkdir -p "$workdir"
open_pr
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "lied-step"
t_lacks "$(t_calls gh)" "read from its run log"
t_lacks "$(t_calls gh)" "(runner exit 134)"
t_lacks "$(t_calls gh)" "pr list --repo"
t_lacks "$(t_calls gh)" "handed off with its pull request open"
t_eq "$(find "$state/7.runner_exit" 2>/dev/null | wc -l)" 0
t_eq "$(find "$state/7.status" 2>/dev/null | wc -l)" 0

# The same three files left by a cycle that did report, and a progress file of
# this cycle's own, are read as they stand: the point of the case above is the
# summary's date, not its presence.

t_case "of several run logs of the issue only the newest is read"
reset
died explore plan develop commit-pr "$BRANCH"
# tehas work names the log for the UTC time of its start to the second, so the
# file of an earlier run has the earlier name and its steps are not this
# cycle's. Two earlier ones, one of them a day old: the newest name decides,
# and no step of the others is said.
printf '%s\n' '{"v":1,"kind":"step-started","issue":7,"event":{"step":"lied-step"}}' \
  >"$T/home/tehas/logs/work-7-$(date -u -d yesterday +%y%m%d-%H%M%S).jsonl"
printf '%s\n' '{"v":1,"kind":"step-started","issue":7,"event":{"step":"another-lied-step"}}' \
  >"$T/home/tehas/logs/work-7-$(date -u -d '2 days ago' +%y%m%d-%H%M%S).jsonl"
open_pr
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "lied-step"
t_lacks "$(t_calls gh)" "another-lied-step"
t_contains "$(t_calls gh)" "last step \`$BRANCH\` read from its run log"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head $BRANCH --author @me --state all"

t_case "a step name over the bound of 100 characters is no step of a run"
reset
# The one bound a step name of a log carries: tehas_headless_step_chars holds
# it at 100 characters, so a name past it is not a step at all and says neither
# the step of the cycle nor its branch. The step before it is what the run
# reached.
long=$(printf 'x%.0s' {1..101})
died commit-pr "$long"
open_pr
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "$long"
t_lacks "$(t_calls gh)" "pr list --repo"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"

t_case "the step tehas work saw in flight is said as its own word, not the runner's"
reset
# A step of tehas work's timed file stands in where the result names none, and
# the comment says who saw it: `tehas work saw in flight`, never `read from its
# run log`, which is the runner's own word from its log. The run wrote a log of
# steps it never got to, so a cycle that reads only the host's file says none
# of them.
died explore plan develop timed-step rc=2 work=2 status=-
cat >"$fx/work.sh" <<'EOS'
mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
printf '3600 runner timed-step\n' >"$TEHAS_HOME/state/cycles/7.timed"
printf '%s\n' '3600' >"$TEHAS_HOME/state/cycles/7.runner_exit"
log=$TEHAS_HOME/logs/work-7-$(date -u +%y%m%d-%H%M%S).jsonl
printf '{"v":1,"kind":"step-started","event":{"step":"a-later-step"}}\n' >"$log"
exit 2
EOS
t_run "$cycle 7"
t_contains "$(t_calls gh)" "last step \`timed-step\` tehas work saw in flight"
t_lacks "$(t_calls gh)" "read from its run log"
t_lacks "$(t_calls gh)" "a-later-step"

t_case "the step of a stalled cycle is said as what tehas work saw"
reset
cat >"$fx/work.sh" <<'EOS'
mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
printf '%s\n%s\n' 1800 stall-step >"$TEHAS_HOME/state/cycles/7.stalled"
printf '%s\n' '3600' >"$TEHAS_HOME/state/cycles/7.runner_exit"
exit 2
EOS
t_run "$cycle 7"
t_contains "$(t_calls gh)" "last step \`stall-step\` tehas work saw in flight"
t_lacks "$(t_calls gh)" "read from its run log"

t_case "a run log past the 200 KB tehas reads is not taken for a clean handoff"
reset
# The window is the whole of what tehas reads of a log, so the commit-pr line
# of a big run may have gone at the cut and the branch of the run and the pull
# request GitHub lists may then name two things. Such a cycle is not answered as
# the handoff of a log read whole: it is parked with the pull request named, and
# told to look for one before it is retried.
died explore plan develop commit-pr "$BRANCH"
open_pr
cat >"$fx/work.sh" <<'EOS'
mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
printf '%s\n' '134' >"$TEHAS_HOME/state/cycles/7.runner_exit"
log=$TEHAS_HOME/logs/work-7-$(date -u +%y%m%d-%H%M%S).jsonl
# Past 200 KB of lines the report ignores, then the steps of the run: the push
# of this run, and the name of its branch, are the first thing the cut removes.
head -c 210000 /dev/zero | tr '\0' 'y' | fold -w 100 | sed 's/.*/{"v":1,"kind":"log","message":"&"}/' >"$log"
for step in explore plan develop commit-pr feature/issue-7-installed; do
  printf '{"v":1,"kind":"step-started","event":{"step":"%s"}}\n' "$step" >>"$log"
done
exit 2
EOS
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "Its branch has an open pull request: https://github.com/acme/widget/pull/347"
t_lacks "$(t_calls gh)" "handed off with its pull request open"
t_lacks "$(t_calls gh)" "its review unfinished"

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
