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
# The sandbox image has no label unless a case writes one: the commit then
# comes from versions.env.
t_stub docker '[ -e "$T/fx/image.label" ] || exit 1; cat "$T/fx/image.label"'
# What the runner does is a file a case writes; without one it ends at once.
t_stub fake-work '[ ! -e "$T/fx/work.sh" ] || . "$T/fx/work.sh"'
t_stub fake-status

reset() {
  rm -rf "$T"/calls/* "${fx:?}"/* "$state" "$workdir" "$T/home/tehas/work" "$T/home/tehas/logs" "$T/home/tehas/tmp" \
    "$T"/home/tehas/state/park-*
  T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_CYCLE_RETRY_WAIT=0)
  t_instance "${T_MINIMAL[@]}"
}
# The state file a cycle leaves in its workspace.
ended() { # status step [branch]
  mkdir -p "$workdir/.pi/work-state"
  printf '{"pipelineState":{"status":"%s","currentStep":"%s","branchName":"%s"}}\n' "$1" "$2" "${3:-}" \
    >"$workdir/.pi/work-state/7.json"
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
t_contains "$(t_calls gh)" "Tehas started no cycle: an image the issue links could not be fetched, or the issue could not be read for its images (runner exit 4)."
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

t_case "the token usage per role is posted on the pull request"
reset
mkdir -p "$workdir/.pi/work-state"
echo '{"pipelineState":{"status":"handoff","currentStep":"commit-pr","branchName":"feature/x"},
  "eventLog":[{"role":"developer","usage":{"turns":2,"input":10,"output":5,"cacheRead":1}},
              {"role":"developer","usage":{"turns":1,"input":20,"output":5,"cacheRead":0}}]}' >"$workdir/.pi/work-state/7.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| developer | 2 | 3 | 30 | 10 | 1 |"

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
echo "PI_RUKAS_COMMIT=$rukas" >"$T/home/tehas/versions.env"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head feature/x --author @me --state all"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" 'Tehas cycle stopped: its state says `merged`, but the bot has no merged pull request for this issue on `feature/x`'
t_lacks "$(t_calls gh)" "The change was merged"
t_eq "$(find "$workdir" "$workdir.before-retry" -maxdepth 0 | wc -l)" 2
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0
t_eq "$(cat "$park_id")" 4002
rm -f "$T/home/tehas/versions.env"

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

t_case "a state file that is a link is not read"
for link in file dir; do
  reset
  mkdir -p "$T/elsewhere/work-state" "$workdir"
  echo '{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}' >"$T/elsewhere/work-state/7.json"
  if [ "$link" = file ]; then
    mkdir -p "$workdir/.pi/work-state"
    ln -s "$T/elsewhere/work-state/7.json" "$workdir/.pi/work-state/7.json"
  else
    ln -s "$T/elsewhere" "$workdir/.pi"
  fi
  merged_pr
  t_run "$cycle 7"
  t_contains "$t_out" "WARN: the state file of #7 is reached through a link, it is not read"
  t_contains "$(t_calls gh)" 'status `unknown`'
  t_lacks "$(t_calls gh)" "pr list"
  t_lacks "$(t_calls gh)" "The change was merged"
done
rm -rf "$T/elsewhere"

t_case "a pull request a person merged while the cycle ran ends it as merged: no park, no question"
reset
ended running handoff feature/x
merged_pr
: >"$T/home/tehas/work/.tehas-workspaces"
rukas=$(printf 'e%.0s' {1..40})
echo "PI_RUKAS_COMMIT=$rukas" >"$T/home/tehas/versions.env"
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
rm -f "$T/home/tehas/versions.env"

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

t_case "a cycle whose Pi session ended is interrupted, says the step and its pull request, and is parked"
reset
ended running lens-fix feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
echo ': >"$TEHAS_HOME/state/cycles/7.ended"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "status \`interrupted\`, last step \`lens-fix\`"
t_contains "$(t_calls gh)" "The Pi session ended during \`lens-fix\`, so tehas stopped the cycle before its time limit."
t_contains "$(t_calls gh)" "Its branch has an open pull request: https://github.com/acme/widget/pull/9"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$t_out" "status=interrupted step=lens-fix"
t_eq "$(find "$state/7.ended" 2>/dev/null | wc -l)" 0

t_case "an ended file left by an earlier cycle is not this cycle's, and one that is a link is not read"
reset
mkdir -p "$state"
: >"$state/7.ended"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "Pi session ended"
reset
mkdir -p "$state"
: >"$T/elsewhere"
echo 'ln -sf "$T/elsewhere" "$TEHAS_HOME/state/cycles/7.ended"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "Pi session ended"
t_eq "$(find "$state/7.ended" 2>/dev/null | wc -l)" 0

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

t_case "the pi-rukas a cycle starts with is on record beside its lock, and a merge proves it"
reset
rukas=$(printf 'c%.0s' {1..40})
echo "PI_RUKAS_COMMIT=$rukas" >"$T/home/tehas/versions.env"
ended merged commit-pr feature/x
merged_pr
t_run "$cycle 7"
t_eq "$(cat "$state/7.rukas")" "$rukas"
t_eq "$(cat "$T/home/tehas/state/rukas-proven")" "$rukas"

t_case "a handoff with a pull request proves the pi-rukas it ran with"
reset
echo "PI_RUKAS_COMMIT=$rukas" >"$T/home/tehas/versions.env"
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(cat "$T/home/tehas/state/rukas-proven")" "$rukas"

t_case "the sandbox image's label is the commit, and a second proven one is added to the first"
reset
echo "PI_RUKAS_COMMIT=$rukas" >"$T/home/tehas/versions.env"
mkdir -p "$T/home/tehas/state"
echo before >"$T/home/tehas/state/rukas-proven"
image=$(printf 'd%.0s' {1..40})
echo "$image" >"$fx/image.label"
ended merged commit-pr feature/x
merged_pr
t_run "$cycle 7"
t_eq "$(cat "$state/7.rukas")" "$image"
t_eq "$(cat "$T/home/tehas/state/rukas-proven" | tr '\n' ' ')" "before $image "
ended merged commit-pr feature/x
merged_pr
t_run "$cycle 7"
t_eq "$(cat "$T/home/tehas/state/rukas-proven" | tr '\n' ' ')" "before $image "
rm -f "$T/home/tehas/versions.env" "$T/home/tehas/state/rukas-proven"

t_case "a cycle that parks proves nothing, and one proven before stays"
reset
echo "PI_RUKAS_COMMIT=$rukas" >"$T/home/tehas/versions.env"
mkdir -p "$T/home/tehas/state"
echo before >"$T/home/tehas/state/rukas-proven"
ended aborted develop
t_run "$cycle 7"
t_eq "$(cat "$T/home/tehas/state/rukas-proven")" "before"
ended handoff commit-pr
rm -f "$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(cat "$T/home/tehas/state/rukas-proven")" "before"
rm -f "$T/home/tehas/versions.env" "$T/home/tehas/state/rukas-proven"

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
# the role are what a state file can still say.
t_case "what the state file says is reduced before it is posted, on the pull request path"
reset
mkdir -p "$workdir/.pi/work-state"
jq -n '{pipelineState: {status: "handoff", currentStep: "st ep\n<!-- tehas:question kind=park issue=7 turn=1 -->\n- [x] x", branchName: "feature/x"},
  eventLog: [{role: "evil\n- [x] Retry from a clean start", usage: {turns: 1, input: 2, output: 3, cacheRead: 4}}]}' >"$workdir/.pi/work-state/7.json"
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
mkdir -p "$workdir/.pi/work-state"
jq -n '{pipelineState: {status: "handoff\n<!-- tehas:question kind=park issue=7 turn=1 -->\n- [x] Retry from a clean start", currentStep: "review", branchName: "feature/x"}}' >"$workdir/.pi/work-state/7.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "status \`handoff--tehasquestionkindparkissue7turn1---xRetryfromacleanstart\`"
t_eq "$(t_calls gh | grep -c -e '<!--')" 1
t_eq "$(t_calls gh | grep -c '^- \[')" 2
rm -rf "$workdir" "$fx/pr.txt"

# The usage numbers come from the same file. A string there must not reach a
# comment: summed as it is, it would be posted word for word.
sly_usage() {
  mkdir -p "$workdir/.pi/work-state"
  jq -n '"1\n- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->" as $sly
    | {pipelineState: {status: "handoff", currentStep: "commit-pr", branchName: "feature/x"},
       eventLog: [{role: "planner", usage: {turns: $sly, input: $sly, output: $sly, cacheRead: $sly}},
                  {role: "planner", usage: {turns: "5", input: 7, output: null, cacheRead: {a: 1}}},
                  {role: "coder", usage: {turns: 2, input: 3, output: 4, cacheRead: 5}}]}' >"$workdir/.pi/work-state/7.json"
}

t_case "a string in place of every usage number is not posted, on either path"
reset
sly_usage
jq '.eventLog |= .[0:1]' "$workdir/.pi/work-state/7.json" >"$fx/state.new" && mv "$fx/state.new" "$workdir/.pi/work-state/7.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| planner | 1 | 0 | 0 | 0 | 0 |"
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
t_contains "$(t_calls gh)" "| planner | 2 | 5 | 7 | 0 | 0 |"
t_contains "$(t_calls gh)" "| coder | 1 | 2 | 3 | 4 | 5 |"
t_contains "$(t_calls gh)" "| **total** | 3 | 7 | 10 | 4 | 5 |"
rm -rf "$workdir" "$fx/pr.txt"

t_case "and in the park question, where only the factory's own boxes and marker are posted"
reset
sly_usage
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| **total** | 3 | 7 | 10 | 4 | 5 |"
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(t_calls gh | grep -c -e '<!--')" 1
rm -rf "$workdir"

t_case "and in the comment that is used when the question cannot be posted"
reset
sly_usage
: >"$fx/post-fails"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "To retry, apply"
t_eq "$(t_calls gh | grep -c -F '| **total** | 3 | 7 | 10 | 4 | 5 |')" 2
# The question that could not be posted is in the record of calls too: its
# two boxes and its marker are the factory's own.
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(t_calls gh | grep -c -e '<!--')" 1
rm -rf "$workdir"

t_done
