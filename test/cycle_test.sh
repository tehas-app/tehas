#!/usr/bin/env bash
# Tests for libexec/cycle against a stub gh: one cycle for one issue, and what
# is reported about it.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

cycle="$T_ROOT/libexec/cycle"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
workdir=$T/home/tehas/work/widget-7
state=$T/home/tehas/state/cycles
park_id=$T/home/tehas/state/park-7.id

t_stub gh '
# A GitHub that does not answer, once a case writes gh-hangs.
[ ! -e "$T/fx/gh-hangs" ] || exec "$(command -pv sleep)" 30
case "$1 $2" in
  "pr list")
    # The first lists fail, as many as pr-fails-times says.
    if [ -s "$T/fx/pr-fails-times" ]; then
      left=$(cat "$T/fx/pr-fails-times")
      if [ "$left" -gt 0 ]; then echo $((left - 1)) >"$T/fx/pr-fails-times"; exit 1; fi
    fi
    # pr.json is the whole answer. pr.txt is the address of one pull request
    # of the bot on the branch asked for, in the state pr-state says [OPEN],
    # whose body is pr-body [Fixes #7].
    if [ -e "$T/fx/pr.json" ]; then cat "$T/fx/pr.json"
    elif [ -e "$T/fx/pr.txt" ]; then
      head=$(printf "%s\n" "$@" | sed -n "/^--head$/{n;p;}")
      jq -n --arg url "$(cat "$T/fx/pr.txt")" --arg head "$head" \
        --arg state "$(cat "$T/fx/pr-state" 2>/dev/null || echo OPEN)" \
        --arg body "$(cat "$T/fx/pr-body" 2>/dev/null || echo "Fixes #7")" \
        "[{url: \$url, state: \$state, body: \$body, headRefName: \$head}]"
    else echo "[]"; fi ;;
  "issue edit") [ ! -e "$T/fx/edit-slow" ] || { : >"$T/calls/edit.started"; sleep 1; }
    # The first edits fail, as many as edit-fails-times says.
    if [ -s "$T/fx/edit-fails-times" ]; then
      left=$(cat "$T/fx/edit-fails-times")
      if [ "$left" -gt 0 ]; then echo $((left - 1)) >"$T/fx/edit-fails-times"; exit 1; fi
    fi
    # An edit whose arguments contain the text in edit-fails-on fails.
    if [ -s "$T/fx/edit-fails-on" ]; then
      case "$*" in *"$(cat "$T/fx/edit-fails-on")"*) exit 1 ;; esac
    fi ;;
  # The park question, posted through the API so that its id comes back.
  "api repos/acme/widget/issues/"*"/comments") [ ! -e "$T/fx/post-fails" ] || exit 1; echo 4002 ;;
  # What the number is. An open issue, unless a case writes the fixture.
  "api repos/acme/widget/issues/"[0-9]*)
    n=${2#repos/acme/widget/issues/}
    case "$n" in *[!0-9]*) exit 1 ;; esac
    # The first asks fail, as many as issue-fails-times says.
    if [ -s "$T/fx/issue-fails-times" ]; then
      left=$(cat "$T/fx/issue-fails-times")
      if [ "$left" -gt 0 ]; then echo $((left - 1)) >"$T/fx/issue-fails-times"; exit 1; fi
    fi
    [ ! -e "$T/fx/issue-fails" ] || exit 1
    if [ -e "$T/fx/issue-$n.json" ]; then cat "$T/fx/issue-$n.json"; else echo "{\"number\":$n,\"state\":\"open\"}"; fi ;;
esac'
# The sandbox image has no label unless a case writes one: no commit is then
# on record.
t_stub docker '[ -e "$T/fx/image.label" ] || exit 1; cat "$T/fx/image.label"'
# What the runner does is a file a case writes; without one it ends at once.
# A result and a progress summary a case wrote are put where tehas work leaves
# them, and the runner exits as the result says unless work.sh exits first.
t_stub fake-work 'for f in result progress; do
  [ ! -e "$T/fx/$f.json" ] || cp "$T/fx/$f.json" "$TEHAS_HOME/state/cycles/7.$f.json"
done
[ ! -e "$T/fx/work.sh" ] || . "$T/fx/work.sh"
exit "$(cat "$T/fx/rc" 2>/dev/null || echo 0)"'
t_stub fake-status
# The poll a cycle starts when it ends.
t_stub systemctl

reset() {
  rm -rf "$T"/calls/* "${fx:?}"/* "$state" "$workdir" "$T/home/tehas/work" "$T/home/tehas/logs" "$T/home/tehas/tmp" \
    "$T"/home/tehas/state/park-*
  T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_CYCLE_RETRY_WAIT=0)
  t_instance "${T_MINIMAL[@]}"
}
# The runner's result for the cycle, and the exit status that goes with it.
ended() { # status step [branch]
  mkdir -p "$workdir"
  jq -n --arg status "$1" --arg step "$2" --arg branch "${3:-}" \
    '{schemaVersion: 3, issues: [{issue: 7, status: $status, failedStep: $step, branch: $branch}]}' >"$fx/result.json"
  case "$1" in merged) echo 0 ;; handoff) echo 10 ;; aborted) echo 20 ;; interrupted) echo 21 ;; *) echo 30 ;; esac >"$fx/rc"
}
# The bot's pull request on the branch was merged.
merged_pr() { echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"; echo MERGED >"$fx/pr-state"; }

for bad in "" "abc" "07" "0" "7 8" "7x" "-7"; do
  t_case "'$bad' is not an issue: nothing runs and nothing is written"
  reset
  t_run "$cycle $bad"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehasbot cycle <issue>"
  t_eq "$(t_calls gh)$(t_calls fake-work)" ""
  t_eq "$(find "$state" 2>/dev/null | wc -l)" 0
done

t_case "an invalid instance runs nothing, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$cycle 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

# The factory works on open issues only. What the number is decides it, and it
# is asked before a label is changed, a comment is posted or a container starts.
t_case "a closed issue is asked for and refused: no cycle, no label, no comment"
for answer in '{"number":7,"state":"closed"}' '{"number":7,"state":"CLOSED"}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-7.json"
  t_run "$cycle 7"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #7: the issue is closed"
  t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "issue comment"
  # It still said it came up. The dispatcher waits for this file to know its
  # unit ran; a cycle that refuses its number after that is not a start that
  # did not happen.
  t_eq "$([ -e "$state/7.taken" ] && echo taken)" taken
done

t_case "the number of a pull request is refused, open, merged or closed"
for answer in '{"number":9,"state":"open","pull_request":{"url":"u"}}' '{"number":9,"state":"closed","pull_request":{"url":"u"}}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-9.json"
  t_run "$cycle 9"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #9: it is a pull request, not an issue the factory works on"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_eq "$([ -e "$state/9.taken" ] && echo taken)" taken
done

t_case "an answer that cannot be read is a refusal, not a guess"
for answer in '' 'not json' 'null' '[]' '{"message":"Not Found"}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-7.json"
  t_run "$cycle 7"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #7: GitHub's answer for it could not be read"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
done

# GitHub could not be reached at all: three tries, then a refusal that says the
# ask failed, which is not the same as an answer that is unreadable.
reset
: >"$fx/issue-fails"
t_run "$cycle 7"
t_eq "$t_rc" 65
t_contains "$t_out" "refuse #7: GitHub could not be asked what the number is"
t_lacks "$t_out" "could not be read"
t_eq "$(grep -c '^api repos/acme/widget/issues/7$' <<<"$(t_calls gh)")" 3
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$([ -e "$state/7.taken" ] && echo taken)" taken

# A call that only failed is retried, as every other call here is: the cycle
# the dispatcher already authorized still starts.
reset
echo 2 >"$fx/issue-fails-times"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(grep -c '^api repos/acme/widget/issues/7$' <<<"$(t_calls gh)")" 3
t_eq "$(t_calls fake-work)" "7"
t_lacks "$t_out" "refuse #7"

t_case "an open issue is asked for first, before its cycle starts"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | head -1)" "api repos/acme/widget/issues/7"
t_eq "$(t_calls fake-work)" "7"

t_case "a first cycle starts plainly, a later one for the same issue restarts"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
rm -f "$T"/calls/*
t_run "$cycle 7"
t_eq "$(t_calls fake-work)" "7 --restart"

t_case "it says that it holds its lock, and lets go of it at the end"
t_eq "$(find "$state/7.taken" | wc -l)" 1
t_eq "$(flock -n "$state/7.lock" echo free)" "free"

t_case "the runner's output is in the issue's log"
reset
echo 'echo "from the runner"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(cat "$T/home/tehas/logs/dispatch-work-7.log")" "from the runner"

t_case "a cycle that ends without a pull request is handed to a human"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$t_out" "done #7 rc=0 status=unknown"
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "the runner's exit status is in the report"
reset
echo 'exit 3' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "(runner exit 3)"

t_case "a runner that started no cycle over an image (exit 4) is parked with that reason"
reset
echo 'exit 4' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "Tehas started no cycle: an image or file the issue links could not be given to it, or the issue could not be read for them (runner exit 4)."
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"

t_case "the issue's images are gone when the cycle is over, however it ended"
for end in "handoff commit-pr" "aborted develop"; do
  reset
  # shellcheck disable=SC2086  # two words: a status and a step
  ended $end feature/x
  : >"$T/home/tehas/work/.tehas-workspaces"
  mkdir -p "$workdir.images"
  echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
  t_run "$cycle 7"
  t_eq "$(find "$workdir.images" -maxdepth 0 2>/dev/null | wc -l)" 0
done

t_case "an images directory under a directory tehas work did not mark is left alone"
reset
ended merged "done" feature/x
merged_pr
mkdir -p "$workdir.images"
t_run "$cycle 7"
t_eq "$(find "$workdir.images" -maxdepth 0 | wc -l)" 1

t_case "the cycle's own state directory is gone when the cycle is over, however it ended"
for end in "handoff commit-pr" "aborted develop"; do
  reset
  # shellcheck disable=SC2086  # two words: a status and a step
  ended $end feature/x
  : >"$T/home/tehas/work/.tehas-workspaces"
  mkdir -p "$workdir.state/cache" "$workdir.state/memory"
  echo planted >"$workdir.state/memory/note.json"
  [ -s "$workdir.state/memory/note.json" ] || t_bad "the cycle's state contents were not planted"
  echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
  t_run "$cycle 7"
  t_eq "$(find "$workdir.state" -maxdepth 0 2>/dev/null | wc -l)" 0
done

t_case "a state directory under a directory tehas work did not mark is left alone"
reset
ended merged "done" feature/x
mkdir -p "$workdir.state/cache"
t_run "$cycle 7"
t_eq "$(find "$workdir.state" -maxdepth 0 | wc -l)" 1

t_case "a cycle that ends with a pull request waits for a human and is reported on it"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head feature/x --author @me"
t_contains "$(t_calls gh)" "No token usage was recorded for this cycle."

t_case "the token usage per role is posted on the pull request, with the result's totals"
reset
ended handoff commit-pr feature/x
jq '.issues[0].usage = {turns: 4, input: 35, output: 11, cacheRead: 1, dispatches: 3}' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo '{"roles":[{"role":"developer","dispatches":2,"turns":3,"input":30,"output":10,"cacheRead":1}],"reason":""}' >"$fx/progress.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| developer | 2 | 3 | 30 | 10 | 1 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 3 | 4 | 35 | 11 | 1 |"

t_case "a merged cycle only loses its running label"
reset
ended merged "done" feature/x
merged_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_lacks "$(t_calls gh)" "--add-label"
t_contains "$(t_calls gh)" "The change was merged: https://github.com/acme/widget/pull/9"

t_case "a merged state with no merged pull request is a claim: parked, and nothing is removed"
reset
ended merged "done" feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$workdir.before-retry"
rukas=$(printf 'e%.0s' {1..40})
echo "$rukas" >"$fx/image.label"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head feature/x --author @me --state all"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" 'Tehas cycle stopped: its state says `merged`, but the bot has no merged pull request for this issue on `feature/x`'
t_lacks "$(t_calls gh)" "The change was merged"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 | wc -l)" 2
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0
t_eq "$(cat "$park_id")" 4002
rm -f "$fx/image.label"

t_case "a merged state whose branch has only an open pull request is parked, and the comment names it"
reset
ended merged "done" feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_contains "$(t_calls gh)" "Its branch has an open pull request: https://github.com/acme/widget/pull/9"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"

t_case "a merged pull request of another issue does not make a merge"
reset
ended merged "done" feature/issue-9-other
merged_pr
echo "Fixes #9" >"$fx/pr-body"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_contains "$(t_calls gh)" "has a pull request of the bot that does not name this issue"
t_lacks "$(t_calls gh)" "The change was merged"

t_case "a handoff whose branch is another issue's pull request is parked, and nothing is posted on it"
reset
ended handoff commit-pr feature/issue-9-other
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "Fixes #9 and mentions #7" >"$fx/pr-body"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_lacks "$(t_calls gh)" "pr comment"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_contains "$(t_calls gh)" "The branch \`feature/issue-9-other\` has a pull request of the bot that does not name this issue"
t_lacks "$(t_calls gh)" "Its branch has an open pull request"

t_case "a pull request whose head is the issue's branch names it, whatever its body says"
reset
ended handoff commit-pr feature/issue-7-the-change
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "No keyword here." >"$fx/pr-body"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"

t_case "a body that closes a longer number does not name this issue"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo "Closes #70" >"$fx/pr-body"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"

t_case "a result that is a link is not read"
reset
mkdir -p "$T/elsewhere"
jq -n '{schemaVersion: 3, issues: [{issue: 7, status: "merged", failedStep: "done", branch: "feature/x"}]}' >"$T/elsewhere/7.json"
echo 'ln -sf "$T/elsewhere/7.json" "$TEHAS_HOME/state/cycles/7.result.json"' >"$fx/work.sh"
merged_pr
t_run "$cycle 7"
t_contains "$t_out" "WARN: the result of #7 is a link, it is not read"
t_contains "$(t_calls gh)" 'status `unknown`'
t_lacks "$(t_calls gh)" "pr list"
t_lacks "$(t_calls gh)" "The change was merged"
rm -rf "$T/elsewhere"

t_case "a state file the cycle left in its workspace is not read: only the runner's result is"
reset
mkdir -p "$workdir/.pi/work-state"
echo '{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}' >"$workdir/.pi/work-state/7.json"
merged_pr
t_run "$cycle 7"
t_contains "$(t_calls gh)" 'status `unknown`'
t_lacks "$(t_calls gh)" "pr list"

t_case "a result of another schema, or of another issue, is no result"
for change in '.schemaVersion = 2' '.issues[0].issue = 8'; do
  reset
  ended merged "done" feature/x
  jq "$change" "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
  merged_pr
  t_run "$cycle 7"
  t_contains "$(t_calls gh)" 'status `unknown`'
done

t_case "a result left by an earlier cycle is not this cycle's"
reset
mkdir -p "$state"
jq -n '{schemaVersion: 3, issues: [{issue: 7, status: "merged", branch: "feature/x"}]}' >"$state/7.result.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" 'status `unknown`'
t_eq "$(find "$state/7.result.json" 2>/dev/null | wc -l)" 0

t_case "a pull request a person merged while the cycle ran ends it as merged: no park, no question"
reset
ended running handoff feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
rukas=$(printf 'e%.0s' {1..40})
echo "$rukas" >"$fx/image.label"
echo 'exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep '^issue edit')" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_contains "$(t_calls gh)" 'Tehas cycle ended: its pull request https://github.com/acme/widget/pull/9 was merged while the cycle ran (last step `handoff`'
t_contains "$(t_calls gh)" "runner exit 2"
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_lacks "$(t_calls gh)" "issues/7/comments"
t_eq "$(find "$park_id" 2>/dev/null | wc -l)" 0
t_contains "$t_out" "done #7 rc=2 status=merged"
t_contains "$t_out" "pr=https://github.com/acme/widget/pull/9"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0
rm -f "$fx/image.label"

t_case "a pull request closed without a merge parks the issue without the retry boxes"
reset
ended running review feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo CLOSED >"$fx/pr-state"
: >"$T/home/tehas/work/.tehas-workspaces"
echo 'exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas cycle stopped: its pull request https://github.com/acme/widget/pull/9 was closed without being merged"
t_contains "$(t_calls gh)" 'To retry, apply `tehas:ready`.'
t_lacks "$(t_calls gh)" "issues/7/comments"
t_lacks "$(t_calls gh)" "tehas:question"
t_eq "$(find "$park_id" 2>/dev/null | wc -l)" 0
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1

t_case "an open pull request beside a closed one is the cycle's"
reset
ended handoff commit-pr feature/x
jq -n '[{url: "https://github.com/acme/widget/pull/8", state: "CLOSED", body: "Fixes #7", headRefName: "feature/x"},
        {url: "https://github.com/acme/widget/pull/9", state: "OPEN", body: "Fixes #7", headRefName: "feature/x"}]' >"$fx/pr.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "Pull request waiting for a human: https://github.com/acme/widget/pull/9"

t_case "a state file in the product checkout is not the cycle's: the container never writes there"
reset
mkdir -p "$checkout/.pi/work-state"
echo '{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}' >"$checkout/.pi/work-state/7.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
rm -rf "$checkout"

t_case "the workspace of a cycle that left a pull request is removed"
reset
ended handoff commit-pr feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "and with it the one kept from an earlier stopped cycle of the issue"
reset
ended handoff commit-pr feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$workdir.before-retry"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "so is the workspace of a merged cycle"
reset
ended merged "done" feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "the workspace of a cycle that stopped is kept for a person to read"
reset
ended aborted develop feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1

t_case "and so is the one kept from the cycle before it"
reset
ended aborted develop feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$workdir.before-retry"
t_run "$cycle 7"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 | wc -l)" 2

t_case "a directory tehas work did not mark is never removed"
reset
ended merged "done" feature/x
merged_pr
rm -f "$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$t_out" "WARN: $T/home/tehas/work is not marked by tehas work, leaving the workspace $workdir"

t_case "a workspace that cannot be removed is said so"
reset
ended merged "done" feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
chmod a-w "$T/home/tehas/work"
t_run "$cycle 7"
chmod u+w "$T/home/tehas/work"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$t_out" "WARN: could not remove the workspace $workdir"

t_case "another issue's workspace is left alone"
reset
ended merged "done" feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$T/home/tehas/work/widget-9"
t_run "$cycle 7"
t_eq "$(find "$T/home/tehas/work/widget-9" -maxdepth 0 | wc -l)" 1
rm -rf "$T/home/tehas/work"

t_case "a second cycle for the same issue runs nothing, exit 3"
reset
mkdir -p "$state"
exec 9>"$state/7.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle for #7 is already running"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(find "$state/7.taken" 2>/dev/null | wc -l)" 0
exec 9>&-

t_case "a cycle for another issue is not held back"
reset
mkdir -p "$state"
exec 9>"$state/8.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
exec 9>&-

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
t_run "$cycle 7 & pid=\$!
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
t_eq "$(cat "$state/7.branch")" feature/x

t_case "a list that was read leaves no branch on record, and one left by an earlier cycle goes"
mkdir -p "$state"
echo feature/old >"$state/7.branch"
ended handoff commit-pr feature/x
t_run "$cycle 7"
t_eq "$(find "$state/7.branch" 2>/dev/null | wc -l)" 0
t_contains "$(t_calls gh)" "Pull request waiting for a human"

t_case "a branch name the container wrote is reduced before it is recorded"
reset
ended handoff commit-pr 'feature/x`;@y$z'
echo 3 >"$fx/pr-fails-times"
t_run "$cycle 7"
t_eq "$(cat "$state/7.branch")" feature/xyz
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

t_case "with the improvement loop on, a cycle that ended leaves its record"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 'echo "{\"type\":\"agent_start\"}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 0' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
rec=$T/home/tehas/tmp/improve/window/7-261011-120000
t_eq "$(jq -c '[.issue, .status, .step, .rc, .pr, .log]' "$rec/cycle.json")" \
  '[7,"handoff","commit-pr",0,"https://github.com/acme/widget/pull/9","work-7-261011-120000.jsonl"]'
t_eq "$(cat "$rec/stream.jsonl")" '{"type":"agent_start"}'
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "the record names the image and the pi-rukas commit tehas work saw as the container started"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
printf 'c%.0s' {1..40} >"$fx/image.label"
image_id=sha256:$(printf 'e%.0s' {1..64})
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"
echo "'"$image_id"' 0123456789abcdef0123456789abcdef01234567" >"$TEHAS_HOME/state/cycles/7.image"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.image, .rukas]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  "[\"$image_id\",\"0123456789abcdef0123456789abcdef01234567\"]"
t_eq "$(find "$state/7.image" 2>/dev/null | wc -l)" 0

t_case "without that file the record falls back, and one left by an earlier cycle is not this cycle's"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
printf 'c%.0s' {1..40} >"$fx/image.label"
mkdir -p "$state"
echo "$image_id 0123456789abcdef0123456789abcdef01234567" >"$state/7.image"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.image, .rukas]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  "[null,\"$(printf 'c%.0s' {1..40})\"]"

t_case "an image file that is a link, or does not hold an id, is not read"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
printf 'c%.0s' {1..40} >"$fx/image.label"
echo "$image_id 0123456789abcdef0123456789abcdef01234567" >"$T/elsewhere"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"
ln -sf "$T/elsewhere" "$TEHAS_HOME/state/cycles/7.image"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '.image' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" null
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"
echo "sha256:short \`x\` 0123456" >"$TEHAS_HOME/state/cycles/7.image"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.image, .rukas]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" "[null,\"$(printf 'c%.0s' {1..40})\"]"
rm -f "$fx/image.label"

t_case "the record names the runner's exit when there is no pull request"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.status, .rc, .pr]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" '["unknown",2,null]'

t_case "runner exit 137 with no end state is status killed in the comment and the record"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 137' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" 'Tehas cycle stopped without a pull request: status `killed`'
t_contains "$(t_calls gh)" "(runner exit 137)"
t_lacks "$(t_calls gh)" "killed for memory"
t_contains "$t_out" "done #7 rc=137 status=killed"
t_eq "$(jq -c '[.status, .rc, .pr, .oomKills]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" '["killed",137,null,0]'

t_case "runner exit 137 with a state that still says running is killed too"
reset
ended running develop
echo 'exit 137' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" 'status `killed`, last step `develop`'
t_contains "$(t_calls gh)" "--add-label needs-human-attention"

t_case "runner exit 137 with a state that names an end keeps that status, and is still parked"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 'exit 137' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" 'status `handoff`'
t_lacks "$(t_calls gh)" 'status `killed`'
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"

t_case "another exit with no end state is not called killed"
reset
echo 'exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "killed"
t_contains "$t_out" "status=unknown"

t_case "the oom count file is read as digits, said in the comment, put in the record and removed"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; exit 137' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "3 processes were killed for memory in its container."
t_contains "$t_out" "oom_kills=3"
t_eq "$(jq -c '[.status, .rc, .oomKills]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" '["killed",137,3]'
t_eq "$(find "$state/7.oom" 2>/dev/null | wc -l)" 0

t_case "a stalled cycle is reported as stalled with how long nothing moved, parked, and the file removed"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended running lens-fix
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo 3600 >"$TEHAS_HOME/state/cycles/7.stalled"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$t_out" "status=stalled"
t_contains "$(t_calls gh)" "None of its logs changed for 60 min at step \`lens-fix\`"
t_contains "$(t_calls gh)" "needs-human-attention"
t_eq "$(jq -c '[.status, .rc]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" '["stalled",2]'
t_eq "$(find "$state/7.stalled" 2>/dev/null | wc -l)" 0

t_case "a stall file beside a state that names an end leaves the end as it is"
reset
ended aborted develop
echo 'echo 3600 >"$TEHAS_HOME/state/cycles/7.stalled"; exit 1' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$t_out" "status=aborted"

t_case "a runner tehas stopped past its time limit is reported as a timeout and parked"
reset
ended interrupted develop
echo 'echo 21600 >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "status \`timeout\`, last step \`develop\`"
t_contains "$(t_calls gh)" "It ran past its time limit of 360 min at step \`develop\`"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_eq "$(find "$state/7.timed" 2>/dev/null | wc -l)" 0

t_case "a stall file that is not digits is ignored"
reset
ended running develop
echo 'echo "60; rm -rf /" >"$TEHAS_HOME/state/cycles/7.stalled"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$t_out" "status=running"
t_lacks "$(t_calls gh)" "None of its logs changed"

t_case "a stall file left by an earlier cycle is not this cycle's"
reset
ended running develop
mkdir -p "$state"
echo 3600 >"$state/7.stalled"
t_run "$cycle 7"
t_contains "$t_out" "status=running"
t_lacks "$(t_calls gh)" "None of its logs changed"

t_case "the pi-rukas a cycle starts with is on record beside its lock, and nothing writes state/rukas-proven"
reset
rukas=$(printf 'c%.0s' {1..40})
echo "$rukas" >"$fx/image.label"
ended merged commit-pr feature/x
merged_pr
t_run "$cycle 7"
t_eq "$(cat "$state/7.rukas")" "$rukas"
ended handoff commit-pr feature/x
echo OPEN >"$fx/pr-state"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Pull request waiting for a human"
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0

t_case "the end of a cycle starts one poll of the dispatcher"
reset
t_run "$cycle 7"
t_eq "$(t_calls systemctl)" "--user start --no-block tehas-dispatch.service"

t_case "a poll that cannot be started is a warning, and the cycle is still reported"
reset
t_stub systemctl 'exit 1'
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: could not start a poll of the dispatcher"
t_contains "$t_out" "done #7"
t_stub systemctl

t_case "a handoff whose pull request merged before the cycle ended is reported with it"
reset
ended handoff handoff feature/x
merged_pr
t_run "$cycle 7"
t_eq "$(t_calls gh | grep '^issue edit')" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_contains "$(t_calls gh)" "https://github.com/acme/widget/pull/9"
t_contains "$t_out" "status=merged"
t_contains "$t_out" "pr=https://github.com/acme/widget/pull/9"

t_case "a cycle the dispatcher stopped because its issue was closed only loses its running label"
reset
ended running adversarial feature/x
echo 'mkdir -p "$TEHAS_HOME/state/cycles"; echo closed >"$TEHAS_HOME/state/cycles/7.done"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep '^issue edit')" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_contains "$(t_calls gh)" "Tehas stopped the cycle: this issue was closed while it ran"
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$(t_calls gh)" "issues/7/comments"
t_contains "$t_out" "status=closed"
t_eq "$(find "$state/7.done" 2>/dev/null | wc -l)" 0

t_case "a reason the dispatcher left for an earlier cycle is not this cycle's"
reset
mkdir -p "$state"
echo closed >"$state/7.done"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_lacks "$(t_calls gh)" "this issue was closed"

t_case "a merged pull request wins over a closed issue"
reset
ended running adversarial feature/x
merged_pr
echo 'mkdir -p "$TEHAS_HOME/state/cycles"; echo closed >"$TEHAS_HOME/state/cycles/7.done"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "was merged while the cycle ran"
t_contains "$t_out" "status=merged"

t_case "a cycle that left a pull request says the count too"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo 'echo 2 >"$TEHAS_HOME/state/cycles/7.oom"' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Pull request waiting for a human: https://github.com/acme/widget/pull/9 2 processes were killed for memory in its container."
t_contains "$(t_calls gh)" "--add-label tehas:awaiting-merge"

t_case "the refused starts are read as digits, said in the comment and the log, and the file is removed"
reset
echo 'echo 41 >"$TEHAS_HOME/state/cycles/7.forks"' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Its container reached its limit of processes and 41 starts of a process were refused: what the cycle ran after that may not have run."
t_contains "$t_out" "forks_refused=41"
t_eq "$(find "$state/7.forks" 2>/dev/null | wc -l)" 0

t_case "a count of refused starts that is not digits, or is a link, counts as nothing"
for bad in 'many' '4; reboot' '-4' '04' ''; do
  reset
  printf '%s\n' "$bad" >"$fx/forks"
  echo 'cp "$T/fx/forks" "$TEHAS_HOME/state/cycles/7.forks"' >"$fx/work.sh"
  t_run "$cycle 7"
  t_lacks "$(t_calls gh)" "limit of processes"
  t_lacks "$(t_calls gh)" "reboot"
  t_contains "$t_out" "forks_refused=0"
done
reset
echo 4 >"$fx/elsewhere"
echo 'ln -s "$T/fx/elsewhere" "$TEHAS_HOME/state/cycles/7.forks"' >"$fx/work.sh"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "limit of processes"
t_eq "$(find "$state/7.forks" 2>/dev/null | wc -l)" 0

t_case "a count of refused starts left by an earlier cycle of the issue is not this cycle's"
reset
mkdir -p "$state"
echo 6 >"$state/7.forks"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "limit of processes"
t_contains "$t_out" "forks_refused=0"

t_case "a count file with letters counts as nothing"
for bad in 'three' '3 processes' '3; reboot' '`id`' '-3' '03' '12345678901234567890' ''; do
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
  printf '%s\n' "$bad" >"$fx/oom"
  echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; cp "$T/fx/oom" "$TEHAS_HOME/state/cycles/7.oom"; exit 2' >"$fx/work.sh"
  t_run "$cycle 7"
  t_lacks "$(t_calls gh)" "killed for memory"
  t_lacks "$(t_calls gh)" "reboot"
  t_contains "$t_out" "oom_kills=0"
  t_eq "$(jq -c .oomKills "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" 0
  t_eq "$(find "$state/7.oom" 2>/dev/null | wc -l)" 0
done

t_case "a count file that is a link is not read"
reset
echo 4 >"$fx/elsewhere"
echo 'ln -s "$T/fx/elsewhere" "$TEHAS_HOME/state/cycles/7.oom"' >"$fx/work.sh"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "killed for memory"
t_eq "$(cat "$fx/elsewhere")" 4
t_eq "$(find "$state/7.oom" 2>/dev/null | wc -l)" 0

t_case "the appended log is not where the count comes from"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"
echo "tehas work: 5 processes in this cycle'"'"'s container were killed for memory"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(cat "$T/home/tehas/logs/dispatch-work-7.log")" "5 processes"
t_lacks "$(t_calls gh)" "killed for memory"
t_eq "$(jq -c .oomKills "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" 0

t_case "a count left by an earlier cycle of the issue is not this cycle's"
reset
mkdir -p "$state"
echo 6 >"$state/7.oom"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "killed for memory"
t_contains "$t_out" "oom_kills=0"

t_case "with the loop off a cycle leaves no record"
reset
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(find "$T/home/tehas/tmp" 2>/dev/null | wc -l)" 0

t_case "a record that cannot be written changes nothing about the cycle's report"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$t_out" "improve: no log of issue 7"
t_contains "$t_out" "done #7 rc=0 status=unknown"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_eq "$(t_calls fake-status | wc -l)" 1
t_eq "$(find "$T/home/tehas/tmp/improve/window" -mindepth 1 2>/dev/null | wc -l)" 0

# Outside the tests nothing names the two scripts: the cycle runs the ones in
# its own directory, whatever PATH holds.
t_case "without the test seams it runs the scripts beside it, not names on PATH"
reset
T_ENV=()
tree=$T/tree
mkdir -p "$tree/libexec" "$tree/lib"
cp "$cycle" "$tree/libexec/cycle"
cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/cycles.sh" "$T_ROOT/lib/improve.sh" "$tree/lib/"
for name in work status; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
for name in tehas tehasbot tehas-work tehas-status; do t_stub "$name"; done
t_run "$tree/libexec/cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
t_eq "$(t_calls fake-status | wc -l)" 1
for name in tehas tehasbot tehas-work tehas-status; do t_eq "$(t_calls "$name")" ""; done

# --- A cap that a provider outage gives ---------------------------------------
# The state file as pi-rukas leaves it when a step failed for good: the failed
# dispatch, then the cap.
# A handoff at <cap>, and the reason tehas work read of the failure before it.
capped() { # <cap> <failure event as JSON>
  ended handoff handoff feature/issue-7-x
  jq --arg cap "$1" '.issues[0].cap = $cap' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
  jq -n --argjson f "$2" '{roles: [], reason: ($f.providerMessage // $f.errorTail // "")}' >"$fx/progress.json"
}
outage='{"kind":"dispatch-failed-provider","step":"adversarial","at":2,"providerMessage":"Connection error."}'

t_case "a step that failed on a provider error is parked by a provider outage, and asks for one retry"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
capped step-failed:adversarial "$outage"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "Tehas cycle parked by a provider outage: status \`handoff\`"
t_contains "$(t_calls gh)" "cap \`step-failed:adversarial\`: Connection error."
t_contains "$(t_calls gh)" "Tehas starts it again once on its own, when the model endpoint answers."
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_eq "$(jq -c '[.cap, .reason, .provider, .retry, (.started | type)]' "$state/7.park")" \
  '["step-failed:adversarial","Connection error.",true,true,"number"]'
t_eq "$(jq -c '[.cap, .capReason]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  '["step-failed:adversarial","Connection error."]'

t_case "a timeout and a 5xx are provider outages too"
for f in '{"kind":"dispatch-failed","at":2,"errorTail":"request timed out after 600s"}' \
  '{"kind":"dispatch-failed","at":2,"errorTail":"502 Bad Gateway"}'; do
  reset
  capped step-failed:develop "$f"
  t_run "$cycle 7"
  t_eq "$(jq -r .provider "$state/7.park")" true
done

t_case "the retry tehas made on its own, parked by an outage again, waits for a person"
reset
capped step-failed:adversarial "$outage"
mkdir -p "$state"
: >"$state/7.requeued"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle parked by a provider outage"
t_contains "$(t_calls gh)" "It was the retry tehas made on its own after an outage, so it waits for a person."
t_lacks "$(t_calls gh)" "starts it again once"
t_eq "$(jq -c '[.provider, .retry]' "$state/7.park")" '[true,false]'
t_eq "$(find "$state/7.requeued" 2>/dev/null | wc -l)" 0

t_case "a step that failed for another reason is parked as before, its cap on record and no retry"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
capped step-failed:develop '{"kind":"dispatch-failed","at":2,"errorTail":"3 assertions failed in test/x"}'
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_lacks "$(t_calls gh)" "provider outage"
t_eq "$(jq -c '[.cap, .provider, .retry]' "$state/7.park")" '["step-failed:develop",false,false]'
t_eq "$(jq -r .capReason "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" "3 assertions failed in test/x"

t_case "a failed step with no reason is said in the log, and is not taken for an outage"
reset
capped step-failed:develop '{"kind":"dispatch-failed","at":2}'
t_run "$cycle 7"
t_contains "$t_out" "WARN: #7 ended at step-failed:develop and no reason was found in its progress"
t_eq "$(jq -r .provider "$state/7.park")" false
reset
capped step-failed:develop '{"kind":"dispatch-failed","at":2,"errorTail":"exit 1"}'
t_run "$cycle 7"
t_lacks "$t_out" "no reason was found"

t_case "the cap and its reason are reduced before they are posted or recorded"
reset
capped 'step-failed:adversarial`@x' '{"kind":"dispatch-failed-provider","at":2,"providerMessage":"provider error `@alice` <!-- #5\nnext line"}'
t_run "$cycle 7"
t_contains "$(t_calls gh)" "cap \`step-failed:adversarialx\`: provider error alice -- 5 next line"
t_eq "$(jq -c '[.cap, .reason]' "$state/7.park")" '["step-failed:adversarialx","provider error alice -- 5 next line"]'

t_case "a cycle that left a pull request leaves no park on record, and an earlier one is gone"
reset
mkdir -p "$state"
echo '{"retry":true}' >"$state/7.park"
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(find "$state/7.park" 2>/dev/null | wc -l)" 0

t_case "a cycle that stops asks what to do next, with boxes"
reset
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:question"
t_eq "$(t_calls gh | grep '^issue edit' | grep -c 'needs-human-attention.*tehas:question\|tehas:question.*needs-human-attention')" 0
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/comments"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=park issue=7 turn=1 -->"

t_case "the host keeps the id of the park question it posted"
t_eq "$(cat "$park_id")" 4002
t_eq "$([ -e "$T/home/tehas/state/park-7.json" ] && echo left)" ""

t_case "when the question cannot be posted the old comment is used and no question label is added"
reset
: >"$fx/post-fails"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "issue comment 7"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "To retry, apply"
t_lacks "$(t_calls gh | grep '^issue edit')" "tehas:question"
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "when the question label cannot be added the person is told the boxes do not work"
reset
printf '%s' '--add-label tehas:question' >"$fx/edit-fails-on"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body The boxes above cannot be used: the label \`tehas:question\` could not be added (run \`tehas labels\` on the host). To retry, apply \`tehas:ready\`."
t_contains "$t_out" "WARN: label change on #7 failed: --add-label tehas:question"
t_eq "$([ -e "$park_id" ] && echo kept)" ""
rm -f "$fx/edit-fails-on"

t_case "when the question label is added nothing more is said"
reset
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "The boxes above cannot be used"

# Only the status handoff reaches the pull request path, so there the step and
# the role are what the runner's result and its progress can still say.
t_case "what the result says is reduced before it is posted, on the pull request path"
reset
ended handoff "st ep
<!-- tehas:question kind=park issue=7 turn=1 -->
- [x] x" feature/x
jq -n '{roles: [{role: "evil\n- [x] Retry from a clean start", dispatches: 1, turns: 1, input: 2, output: 3, cacheRead: 4}]}' >"$fx/progress.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "status \`handoff\`, last step \`step--tehasquestionkindparkissue7turn1---xx\`"
t_contains "$(t_calls gh)" "--add-label tehas:awaiting-merge"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| evil-xRetryfromacleanstart | 1 |"
rm -rf "$workdir" "$fx/pr.txt"

t_case "a status that only contains handoff does not reach the pull request path"
reset
ended "handoff
<!-- tehas:question kind=park issue=7 turn=1 -->
- [x] Retry from a clean start" review feature/x
echo 10 >"$fx/rc"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "status \`handoff--tehasquestionkindparkissue7turn1---xRetryfromacleanstart\`"
t_eq "$(t_calls gh | grep -c -e '<!--')" 1
t_eq "$(t_calls gh | grep -c '^- \[')" 2
rm -rf "$workdir" "$fx/pr.txt"

t_case "a handoff that the runner did not exit as one does not reach the pull request path"
reset
ended handoff commit-pr feature/x
echo 30 >"$fx/rc"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "runner exit 30"

t_case "a clarification handoff with a pull request is waiting for a merge as any handoff is"
reset
ended handoff explore feature/x
echo 11 >"$fx/rc"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label tehas:awaiting-merge"

# The usage numbers come from files too. A string there must not reach a
# comment: summed as it is, it would be posted word for word.
sly_usage() {
  ended handoff commit-pr feature/x
  jq -n '"1\n- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->" as $sly
    | {roles: [{role: "planner", dispatches: 2, turns: $sly, input: 7, output: null, cacheRead: {a: 1}},
               {role: "coder", dispatches: 1, turns: 2, input: 3, output: 4, cacheRead: 5}]}' >"$fx/progress.json"
  jq '.issues[0].usage = {dispatches: 3, turns: 7, input: 10, output: 4, cacheRead: 5}' "$fx/result.json" >"$fx/r" &&
    mv "$fx/r" "$fx/result.json"
}

t_case "a string in place of every usage number is not posted, on either path"
reset
sly_usage
jq '"1\n- [x] Retry\n<!-- x -->" as $sly | .issues[0].usage = {dispatches: $sly, turns: $sly, input: $sly, output: $sly, cacheRead: $sly}' \
  "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| planner | 2 | 0 | 7 | 0 | 0 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 0 | 0 | 0 | 0 | 0 |"
rm -f "$T"/calls/* "$fx/pr.txt"
: >"$fx/post-fails"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "To retry, apply"
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(t_calls gh | grep -c -e '<!--')" 1
rm -rf "$workdir"

t_case "a usage number that is not a number counts as nothing, on the pull request path"
reset
sly_usage
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| planner | 2 | 0 | 7 | 0 | 0 |"
t_contains "$(t_calls gh)" "| coder | 1 | 2 | 3 | 4 | 5 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |"
rm -rf "$workdir" "$fx/pr.txt"

t_case "and in the park question, where only the factory's own boxes and marker are posted"
reset
sly_usage
echo 30 >"$fx/rc"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |"
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(t_calls gh | grep -c -e '<!--')" 1
rm -rf "$workdir"

t_case "and in the comment that is used when the question cannot be posted"
reset
sly_usage
echo 30 >"$fx/rc"
: >"$fx/post-fails"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "To retry, apply"
t_eq "$(t_calls gh | grep -c -F '| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |')" 2
# The question that could not be posted is in the record of calls too: its
# two boxes and its marker are the factory's own.
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(t_calls gh | grep -c -e '<!--')" 1
rm -rf "$workdir"

t_done
