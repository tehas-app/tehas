# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # snippets and stub bodies are expanded later; the parts read the variables
# What the tests of libexec/cycle share: test/cycle_*_test.sh source this. One
# cycle for one issue against a stub gh, and what is reported about it.
# shellcheck source=test/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

cycle="$T_ROOT/libexec/cycle"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
workdir=$T/home/tehas/work/widget-7
# How a cycle names that directory in a comment: in ~ form, as HOME is $T/home.
kept_here="~${workdir#"$T/home"}"
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
  "pr edit") [ ! -e "$T/fx/pr-edit-fails" ] || exit 1 ;;
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
# Without a progress of its own a case gets one with nothing in it, as tehas
# work writes for a log that has no stable line.
t_stub fake-work 'for f in result progress; do
  [ ! -e "$T/fx/$f.json" ] || cp "$T/fx/$f.json" "$TEHAS_HOME/state/cycles/7.$f.json"
done
[ -e "$T/fx/progress.json" ] || [ -e "$T/fx/no-progress" ] ||
  echo "{\"roles\":[],\"reason\":\"\",\"reviewHeld\":false}" >"$TEHAS_HOME/state/cycles/7.progress.json"
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
# A handoff's cap is the one of a pull request that waits only for a merge
# unless a case names another; "-" is none.
ended() { # status step [branch] [cap]
  local cap=${4:-}
  [ -n "$cap" ] || [ "$1" != handoff ] || cap=awaiting-human-merge
  [ "$cap" != - ] || cap=""
  mkdir -p "$workdir"
  jq -n --arg status "$1" --arg step "$2" --arg branch "${3:-}" --arg cap "$cap" \
    '{schemaVersion: 3, issues: [{issue: 7, status: $status, failedStep: $step, branch: $branch}
      + (if $cap == "" then {} else {cap: $cap} end)]}' >"$fx/result.json"
  case "$1" in merged) echo 0 ;; handoff) echo 10 ;; aborted) echo 20 ;; interrupted) echo 21 ;; *) echo 30 ;; esac >"$fx/rc"
}
# The bot's pull request on the branch was merged.
merged_pr() { echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"; echo MERGED >"$fx/pr-state"; }
