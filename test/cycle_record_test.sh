#!/usr/bin/env bash
# Tests for libexec/cycle: the improvement record it leaves, a killed, stalled
# or timed out runner, the poll it starts, and the counts it reads.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh"

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

t_case "the end of a cycle starts one poll of the dispatcher, and leaves the request for the path unit"
reset
rm -f "$T/home/tehas/state/dispatch.again"
t_run "$cycle 7"
t_eq "$(t_calls systemctl)" "--user start --no-block tehas-dispatch.service"
t_eq "$(find "$T/home/tehas/state/dispatch.again" 2>/dev/null | wc -l)" 1

t_case "the path unit watches for the file the cycle leaves, and starts a poll"
t_eq "$(sed -n 's/^PathExists=//p' "$T_ROOT/host/systemd/tehas-dispatch.path")" "%h/tehas/state/dispatch.again"
t_eq "$(sed -n 's/^Unit=//p' "$T_ROOT/host/systemd/tehas-dispatch.path")" "tehas-dispatch.service"

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

t_done
