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

t_stub gh '
case "$1 $2" in
  "pr list") cat "$T/fx/pr.txt" 2>/dev/null ;;
  "issue edit") [ ! -e "$T/fx/edit-slow" ] || { : >"$T/calls/edit.started"; sleep 1; } ;;
esac'
# What the runner does is a file a case writes; without one it ends at once.
t_stub fake-work '[ ! -e "$T/fx/work.sh" ] || . "$T/fx/work.sh"'
t_stub fake-status

reset() {
  rm -rf "$T"/calls/* "${fx:?}"/* "$state" "$workdir" "$T/home/tehas/work" "$T/home/tehas/logs" "$T/home/tehas/tmp"
  T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status)
  t_instance "${T_MINIMAL[@]}"
}
# The state file a cycle leaves in its workspace.
ended() { # status step [branch]
  mkdir -p "$workdir/.pi/work-state"
  printf '{"pipelineState":{"status":"%s","currentStep":"%s","branchName":"%s"}}\n' "$1" "$2" "${3:-}" \
    >"$workdir/.pi/work-state/7.json"
}

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
t_run "$cycle 7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_lacks "$(t_calls gh)" "--add-label"
t_contains "$(t_calls gh)" "The change was merged."

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

t_case "so is the workspace of a merged cycle"
reset
ended merged "done" feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "the workspace of a cycle that stopped is kept for a person to read"
reset
ended aborted develop feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1

t_case "a directory tehas work did not mark is never removed"
reset
ended merged "done" feature/x
rm -f "$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$t_out" "WARN: $T/home/tehas/work is not marked by tehas work, leaving the workspace $workdir"

t_case "a workspace that cannot be removed is said so"
reset
ended merged "done" feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
chmod a-w "$T/home/tehas/work"
t_run "$cycle 7"
chmod u+w "$T/home/tehas/work"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1
t_contains "$t_out" "WARN: could not remove the workspace $workdir"

t_case "another issue's workspace is left alone"
reset
ended merged "done" feature/x
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
t_contains "$(t_calls gh)" "issue comment 7"

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
cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/improve.sh" "$tree/lib/"
for name in work status; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
for name in tehas tehasbot tehas-work tehas-status; do t_stub "$name"; done
t_run "$tree/libexec/cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
t_eq "$(t_calls fake-status | wc -l)" 1
for name in tehas tehasbot tehas-work tehas-status; do t_eq "$(t_calls "$name")" ""; done

t_done
