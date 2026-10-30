# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # snippets and stub bodies are expanded later; the parts read the variables
# What the tests of libexec/cycle share: test/cycle_*_test.sh source this. One
# cycle for one issue against a stub gh, and what is reported about it.
# The part has sourced test/lib.sh before this.

cycle="$T_ROOT/libexec/cycle"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
workdir=$T/home/tehas/work/widget-7
# How a cycle names that directory in a comment: in ~ form, as HOME is $T/home.
kept_here="~${workdir#"$T/home"}"
state=$T/home/tehas/state/cycles
# park_q: the question of #7's current park on record, none without one.
park_q() { t_row park-show 7 -- '.park.question // "none"'; }
# park_of [<jq of the park>]: #7's current park on record.
park_of() { t_row park-show 7 -- ".park | ${1:-.} | tostring"; }

t_stub gh '
# nap names the real sleep: found before the words of a call are named, so
# that the substitution is answered when t_stub writes the stub and not in the
# middle of the case below.
# gh-hangs stands for a GitHub that does not answer. The word sleep is asked
# for once here, in a plain assignment, where its substitution is harmless.
nap=$(command -pv sleep)
[ ! -e "$T/fx/gh-hangs" ] || exec "$nap" 30
# rest_verb, rest_path: the verb and the path of a call of the form
# gh api -X verb path ... Their values reach this body only through these two
# names, and no case pattern below may name them: the case reads $1 and $2,
# which hold the two words of the call that t_stub fills in itself.
rest_verb=$3
rest_path=$4
# fails <fixture> [<fixture of a countdown>]: exit 1 when the first names an
# outage, or while the second holds a count above 0, which it lowers.
fails() {
  local left
  if [ -s "$T/fx/${2:-none}" ]; then
    left=$(cat "$T/fx/$2")
    [ "$left" -gt 0 ] || return 0
    echo $((left - 1)) >"$T/fx/$2"
    exit 1
  fi
  [ ! -e "$T/fx/$1" ] || exit 1
}
# not_found: say what GitHub says for a path it does not have, and fail.
not_found() { echo "message: Not Found" >&2; exit 1; }
# api_rest <verb> <path-glob>: a call of gh api -X that changes one label of a
# pull request, answered as GitHub answers it. Called with the two words, so
# that no pattern of the case below has to name them.
api_rest() {
  [ "$rest_verb" = "$1" ] || return 1
  case "$rest_path" in
    repos/acme/widget/issues/"$2") return 0 ;;
    *) return 1 ;;
  esac
}
case "$1 $2" in
  # The label of a pull request, through the REST issues endpoint: a mark adds
  # it and an unmark takes it off. The verb and the path are compared by
  # api_rest above, because a pattern of this case may name only $1 and $2,
  # which hold just "api" and "-X". A mark fails while $fx/mark-fails-times
  # counts down, or at once when that file is there but holds nothing, so a
  # case can say the first two marks fail and the third works. An unmark fails
  # while $fx/unmark-fails-times counts down, or at once when
  # $fx/unmark-fails is there, which is what the three tries of the cycle use.
  # $fx/label-missing makes either answer 404, as GitHub does for a label the
  # repository has not got.
  "api -X")
    if api_rest POST "*/labels"; then
      [ ! -e "$T/fx/label-missing" ] || not_found
      fails mark-fails mark-fails-times
    elif api_rest DELETE "*/labels/*"; then
      [ ! -e "$T/fx/label-missing" ] || not_found
      fails unmark-fails unmark-fails-times
    # The body of the issue, written. $fx/patch-fails makes it fail.
    elif api_rest PATCH 7; then
      fails patch-fails
    else
      exit 1
    fi ;;
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
# them, and so is the note of a container that never started, which a case names
# with nostart <bytes>. The runner exits as the result says unless work.sh exits
# first.
# Without a progress of its own a case gets one with nothing in it, as tehas
# work writes for a log that has no stable line.
nostart() { # <bytes>: the note tehas work leaves for a container that wrote no
# result and no line of its stream. The bytes of the file are what the case says,
# so a case can name a note the reader must not believe: nothing in it is
# checked, bounded or completed here, and an empty argument leaves an empty file.
  printf '%s\n' "${1:-}" >"$fx/nostart"
}
t_stub fake-work 'for f in result progress; do
  [ ! -e "$T/fx/$f.json" ] || cp "$T/fx/$f.json" "$TEHAS_HOME/state/cycles/7.$f.json"
done
[ -e "$T/fx/progress.json" ] || [ -e "$T/fx/no-progress" ] ||
  echo "{\"roles\":[],\"reason\":\"\",\"reviewHeld\":false}" >"$TEHAS_HOME/state/cycles/7.progress.json"
[ -e "$T/fx/nostart" ] && cat "$T/fx/nostart" >"$TEHAS_HOME/state/cycles/7.nostart"
[ ! -e "$T/fx/work.sh" ] || . "$T/fx/work.sh"
# The body of a case may name the exit of tehas work itself, which is not
# that of the runner inside it: it writes the file of that exit instead.
exit "$(cat "$T/fx/rc" 2>/dev/null || echo 0)"
'
t_stub fake-status
# The poll a cycle starts when it ends.
t_stub systemctl

# No case leaves the old file of a record that is on the store.
t_after='t_no_old_state "$T/home/tehas"'
reset() {
  rm -rf "$T"/calls/* "${fx:?}"/* "$state" "$workdir" "$T/home/tehas/work" "$T/home/tehas/logs" "$T/home/tehas/tmp" \
    "$T/home/tehas/libexec"
  # A case writes a log of an earlier run beside the one of this cycle, and
  # does so before tehas work has run: the directory is made here so the file
  # has somewhere to go.
  mkdir -p "$T/home/tehas/logs"
  # The state store a cycle runs, with nothing on record.
  t_store_home "$T/home/tehas"
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
    '{schemaVersion: 5, issues: [{issue: 7, status: $status, failedStep: $step, branch: $branch}
      + (if $cap == "" then {} else {cap: $cap} end)]}' >"$fx/result.json"
  case "$1" in merged) echo 0 ;; handoff) echo 10 ;; aborted) echo 20 ;; interrupted) echo 21 ;; *) echo 30 ;; esac >"$fx/rc"
}
# The bot's pull request on the branch was merged.
merged_pr() { echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"; echo MERGED >"$fx/pr-state"; }
# The runner of a cycle that died: it writes no result and no progress of its
# own, leaves the file of the runner's own status as tehas work leaves it, and
# leaves a run log whose stable step-started lines name the steps it reached,
# one per argument. Its exit is the runner's, 134 for a panic.
died() { # [step...] ; words: rc=<n> the exit of the container (134 for a
# panic, 137 for a kill), work=<n> the exit of tehas work itself [2, or 137 for
# a container it saw killed], status=<word> the end tehas work read of the
# container (killed, for a container it saw killed).
  local rc=134 steps=() a word="" wrk=""
  for a in "$@"; do
    case "$a" in
      rc=*) rc=${a#rc=} ;;
      work=*) wrk=${a#work=} ;;
      status=*) word=${a#status=} ;;
      *) steps+=("$a") ;;
    esac
  done
  # What the runner of a cycle that died leaves behind: a result file with
  # nothing in it, a progress of its own (which is what says tehas work got as
  # far as the log of this cycle), the log of the run, and the runner's own
  # status as tehas work records it. The cycle removes these only after it
  # takes its lock, so the runner makes them. What each of the three files is
  # proof of is written in lib/cycles.sh, which
  # is where a change to this setup has to agree with the code. tehas work
  # itself exits 2 for a runner that died however it died: the comment's word
  # for the runner is the of this file, not that 2. A case may name tehas work's
  # own exit; the kill of a container is the 137 tehas work exits with.
  [ -n "$wrk" ] || { [ "$word" != killed ] || wrk=137; }
  [ -n "$wrk" ] || wrk=2
  echo "$wrk" >"$fx/rc"
  # The word of tehas work for how the container ended: the one file of the
  # cycle that says an end at all when the runner wrote no result.
  [ -z "$word" ] || printf '%s\n' "$word" >"$fx/status"
  # The steps are a file the body reads line by line, so a step's name is
  # carried by no quoting of this function at all. The branch of the push is
  # said as the step right after commit-pr, which is how the branch of a run
  # whose result was never written is named at all.
  { printf '%s\n' "${steps[@]}"; } >"$fx/steps"
  cat >"$fx/work.sh" <<'EOS'
mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
[ -s "$T/fx/status" ] && cat "$T/fx/status" >"$TEHAS_HOME/state/cycles/7.status"
printf "%s\n" '@RC@' >"$TEHAS_HOME/state/cycles/7.runner_exit"
# The name tehas work gives its log: the UTC time of its start, to the second.
log=$TEHAS_HOME/logs/work-7-$(date -u +%y%m%d-%H%M%S).jsonl
: >"$log"
while IFS= read -r step; do
  printf '{"v":1,"kind":"step-started","issue":7,"event":{"step":"%s"}}\n' "$step" \
    >>"$log"
done <"$T/fx/steps"
cp "$log" "$TEHAS_HOME/logs/dispatch-work-7.log"
EOS
  # The one word of the case that this function decides, and nothing else.
  sed -i "s|'@RC@'|'$rc'|" "$fx/work.sh"
  mkdir -p "$workdir"
}
# A runner that wrote its result but whose entry carries no usage at all, with
# the rows the progress did record. The totals of the table are then those rows
# added up.
ended_no_usage_with_rows() {
  ended handoff commit-pr feature/issue-7-x
  jq '.issues[0] |= del(.usage)' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
  jq -n '{roles: [{role: "planner", dispatches: 2, turns: 3, input: 7, output: 1, cacheRead: 0},
                  {role: "coder", dispatches: 1, turns: 2, input: 3, output: 4, cacheRead: 5}]}' \
    >"$fx/progress.json"
}
# The same, but one row carries a word in place of one of its counts: that row
# is the word of a container, and nothing of it is summed.
ended_no_usage_with_a_sly_row() {
  ended_no_usage_with_rows
  jq -n '"1\n- [x] Retry\n<!-- x -->" as $sly
    | {roles: [{role: "planner", dispatches: 2, turns: $sly, input: 7, output: null, cacheRead: {a: 1}},
               {role: "coder", dispatches: 1, turns: 2, input: 3, output: 4, cacheRead: 5}]}' \
    >"$fx/progress.json"
}
# The bot's open pull request of the bot on the branch, as GitHub would answer.
open_pr() { echo "${1:-https://github.com/acme/widget/pull/347}" >"$fx/pr.txt"; echo OPEN >"$fx/pr-state"; }
