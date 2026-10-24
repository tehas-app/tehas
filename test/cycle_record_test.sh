#!/usr/bin/env bash
# Tests for libexec/cycle: the improvement record it leaves, a killed, stalled
# or timed out runner, the poll it starts, and the counts it reads.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

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
# What tehas work puts on the cycle's record as the container starts.
image_work() { # <image> [<pi-rukas commit>]
  printf '%s\n' 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' \
    "\"\$T/store-bin\" cycle-image 7 --attempt \"\$TEHAS_CYCLE_ATTEMPT\" --image $1 ${2:+--rukas $2} >/dev/null; exit 2" >"$fx/work.sh"
}
image_work "$image_id" 0123456789abcdef0123456789abcdef01234567
t_run "$cycle 7"
t_eq "$(jq -c '[.image, .rukas]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  "[\"$image_id\",\"0123456789abcdef0123456789abcdef01234567\"]"
t_eq "$(t_row cycle-show 7 -- .cycle.image)" "$image_id"

t_case "without an image on record the record falls back, and an earlier cycle's is not this cycle's"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
printf 'c%.0s' {1..40} >"$fx/image.label"
image_work "$image_id" 0123456789abcdef0123456789abcdef01234567
t_run "$cycle 7"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.image, .rukas]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  "[null,\"$(printf 'c%.0s' {1..40})\"]"
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, .cycle.image] | tostring')" '[2,null]'

t_case "an image that is no image id is not on record"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
printf 'c%.0s' {1..40} >"$fx/image.label"
image_work "sha256:short"
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
t_eq "$(park_of .reason)" stall
t_contains "$(t_calls gh)" 'cap `stall`'
t_eq "$(find "$state/7.stalled" 2>/dev/null | wc -l)" 0

# The three cases below are the shape of the two cycles of tehas-org/tehas#110:
# a container that ended with nothing in its record, no step and no cap. The
# note tehas work leaves for that end is on record as tehas's own cap, and the
# step of such a cycle names no step of the runner. The word is the one below,
# and it is not a word of the runner's vocabulary, nor one the analytics reader
# reads a planning turn's rc for.
t_case "a container that ended before the driver wrote an event is on record as runner-never-started, with its exit status, and no step"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
rm -f "$fx/result.json"
nostart 1
echo 'mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"; echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; cat "$T/fx/nostart" >"$TEHAS_HOME/state/cycles/7.nostart"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.status, .step, .cap, .capReason]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  '["unknown","no-step","runner-never-started","the container ended with the exit status 1 and wrote no line of its stream, so the driver wrote no event and no step started"]'
t_eq "$(park_of .reason)" runner-never-started
t_contains "$(t_calls gh)" 'tehas stopped the cycle: the cap `runner-never-started`.'
t_contains "$(t_calls gh)" "no step started, the container of the cycle ended before the driver wrote an event"
t_eq "$(find "$state/7.nostart" 2>/dev/null | wc -l)" 0

# The memory kills of such a cycle are told once, from the oom file, as for every
# other end: the note holds no count of them, so one number has one home. The oom
# file is one the cycle removes when it takes its lock, so the runner of the case
# is the one that leaves it, as tehas work does.
t_case "the memory kills of a runner that never started are said once, from the oom file"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
rm -f "$fx/result.json"
nostart 1
echo 'mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"; echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; cat "$T/fx/nostart" >"$TEHAS_HOME/state/cycles/7.nostart"; echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -r '.capReason' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  "the container ended with the exit status 1 and wrote no line of its stream, so the driver wrote no event and no step started"
t_contains "$(t_calls gh)" "3 processes were killed for memory in its container."
t_lacks "$(t_calls gh)" "processes in its container were killed for memory"
t_eq "$(find "$state/7.nostart" "$state/7.oom" 2>/dev/null | wc -l)" 0

t_case "the note of a runner that never started is read for nothing when the cycle wrote a result, and one of an earlier cycle is not this cycle's"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
# The note of an earlier cycle, when tehas work of this one died mid-cycle and
# wrote none of its own: it is gone before the report reads anything, and the
# result this cycle wrote has the last word for the step and the cap.
mkdir -p "$state"
nostart 1
cp "$fx/nostart" "$state/7.nostart"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 10' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.status, .step, .cap]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  '["handoff","commit-pr","awaiting-human-merge"]'
t_eq "$(find "$state/7.nostart" 2>/dev/null | wc -l)" 0

# A note the reader cannot believe is no note: the cycle is reported the way it
# was before the note existed, with the step read from its run log, and no word
# of the cap. Each shape below is one a truncated or misplaced write can leave.
for note in "" "x" "0" "-1" "1000" "01"; do
  t_case "a note that holds '$note' is no note: the cycle is reported as one that died part way through"
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
  rm -f "$fx/result.json"
  nostart "$note"
  # A watched run that wrote no result, made the way the helper makes one: the
  # runner_exit file says the run was watched, the progress summary says the log
  # beside it belongs to this cycle, and the log names the steps it reached.
  # Without those the step is read from no log at all, which is the other half
  # of what this case must not be mistaken for.
  died explore
  echo 'mkdir -p "$TEHAS_HOME/state/cycles"; cat "$T/fx/nostart" >"$TEHAS_HOME/state/cycles/7.nostart"; exit 2' >>"$fx/work.sh"
  t_run "$cycle 7"
  # The log the report reads is named for the second of the cycle's start, which
  # is the moment the case takes it: the helper names it so, and the record of
  # the cycle carries that same name. A fixed stamp would look for a record of a
  # run that never was.
  t_eq "$(jq -c '[.step, .cap]' "$T/home/tehas/tmp/improve/window/$(ls "$T/home/tehas/tmp/improve/window")/cycle.json")" '["explore",null]'
  t_lacks "$(t_calls gh)" 'runner-never-started'
  t_eq "$(find "$state/7.nostart" 2>/dev/null | wc -l)" 0
done

# The second field is read for nothing: the oom file is the one account of the
# memory kills, so a note whose count the reader would not believe is still the
# note of a runner that never came up. Its digits are reduced where they are
# used, not read for what they say.
t_case "a second field of the note is read for nothing: the cap stands and the count of the oom file is the one said"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
rm -f "$fx/result.json"
nostart "1 007"
echo 'mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"; echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; cat "$T/fx/nostart" >"$TEHAS_HOME/state/cycles/7.nostart"; echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.step, .cap]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" '["no-step","runner-never-started"]'
t_contains "$(t_calls gh)" "3 processes were killed for memory in its container."

# A note that is a link is no note, as every other file of the cycle's: the
# report reads nothing through it.
t_case "a note that is a link is not read"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
rm -f "$fx/result.json"
echo 1 >"$T/elsewhere"
mkdir -p "$state"
ln -sf "$T/elsewhere" "$state/7.nostart"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.step, .cap]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" '["-",null]'
t_lacks "$(t_calls gh)" "runner-never-started"
t_eq "$(cat "$T/elsewhere")" 1

t_case "a cycle tehas stopped at its time limit with no cap from the runner is on record as wall-clock"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended running develop -
echo '{}' >"$fx/progress.json"
jq '.issues[0].handoff = {channel: "host", body: "<!-- pi-rukas:handoff issue=7 cap=none -->\n## Cap hit"}' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo 21600 tehas develop >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(park_of .reason)" wall-clock
t_contains "$(t_calls gh)" 'cap `wall-clock`'
t_contains "$(t_calls gh)" 'pi-rukas:handoff issue=7 cap=wall-clock -->'
t_eq "$(jq -c '[.status, .step, .cap, .capReason]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  '["timeout","develop","wall-clock","tehas work stopped the container at the time limit of 21600s, step develop in flight"]'

t_case "a runner that stopped itself at the time limit with no result is on record as wall-clock at the step it was in, not running"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
rm -f "$fx/result.json"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo 21600 runner adversarial >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$t_out" "status=timeout step=adversarial"
t_contains "$(t_calls gh)" "It reached its time limit of 360 min at step \`adversarial\`, and the runner stopped there."
t_lacks "$(t_calls gh)" "did not stop on its own"
t_eq "$(jq -c '[.status, .step, .cap, .capReason]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  '["timeout","adversarial","wall-clock","the cycle reached its time limit, TEHAS_WORK_TIMEOUT=21600s, at step adversarial"]'

t_case "a runner that stopped itself at the time limit with a result that names no end is on record as wall-clock"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended running commit-pr -
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo 21600 runner commit-pr >"$TEHAS_HOME/state/cycles/7.timed"; exit 30' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.status, .step, .cap, .rc]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" '["timeout","commit-pr","wall-clock",30]'
t_eq "$(park_of .reason)" wall-clock

t_case "the step in the time limit's file is read as a word"
reset
rm -f "$fx/result.json"
echo 'echo "21600 runner \`rm\`;x" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$t_out" "status=timeout step=- "

t_case "a stop at the time limit names the step tehas work saw in flight when the result names none"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended running -
printf "%s\n" 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo "21600 tehas adversarial" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_eq "$(jq -c '[.status, .step, .cap]' "$rec")" '["timeout","adversarial","wall-clock"]'
t_eq "$(jq -r .capReason "$rec")" "tehas work stopped the container at the time limit of 21600s, step adversarial in flight"
t_contains "$(t_calls gh)" "It ran past its time limit of 360 min at step \`adversarial\`"
t_eq "$(find "$state/7.timed" 2>/dev/null | wc -l)" 0

t_case "a runner stopped mid-step at the time limit is on record with tehas's cap, its own last cap in the reason"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended interrupted develop - verify-failed:develop
echo '{"reason": "fence violated"}' >"$fx/progress.json"
printf "%s\n" 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo "21600 tehas develop" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_eq "$(jq -c '[.status, .step, .cap]' "$rec")" '["timeout","develop","wall-clock"]'
t_eq "$(jq -r .capReason "$rec")" "tehas work stopped the container at the time limit of 21600s, step develop in flight; the runner had named the cap verify-failed:develop; fence violated"
t_eq "$(park_of .reason)" wall-clock
t_contains "$(t_calls gh)" 'tehas stopped the cycle: the cap `wall-clock`.'
t_lacks "$(t_calls gh)" 'pi-rukas stopped on the cap'

t_case "a stall names the step in flight and is tehas's cap over a runner's mid-step cap"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended interrupted - - step-failed:lens-fix
printf "%s\n" 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; printf "3600\nlens-fix\n" >"$TEHAS_HOME/state/cycles/7.stalled"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_eq "$(jq -c '[.status, .step, .cap]' "$rec")" '["stalled","lens-fix","stall"]'
t_contains "$(jq -r .capReason "$rec")" "tehas work stopped the container when no log changed for 3600s, step lens-fix in flight; the runner had named the cap step-failed:lens-fix"
t_contains "$(t_calls gh)" "None of its logs changed for 60 min at step \`lens-fix\`"

t_case "a cap the runner named is kept when tehas also stopped the cycle"
reset
ended handoff develop feature/x deadline
echo 'echo 21600 >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(park_of .reason)" deadline
t_contains "$(t_calls gh)" 'pi-rukas stopped on the cap `deadline`.'

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

# The marker tehas stop leaves: one line of seconds and an account, read as the
# two files of tehas work are read, and said wherever the end of the cycle is
# said. No new cap: the stop is no end tehas work gives a cap or a status for.
t_case "a cycle a person stopped is on record with the stop in its reason, and no cap of its own"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended interrupted develop - deadline
echo '{"reason": "waiting on the fence"}' >"$fx/progress.json"
at=$(date +%s)
printf "%s\n" 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo "'"$at"' alice" >"$TEHAS_HOME/state/cycles/7.stopped"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
# The cap of the runner is kept: a person's stop is not a cap, so nothing here
# replaces the end the cycle itself named.
t_eq "$(jq -c '[.status, .step, .cap]' "$rec")" '["interrupted","develop","deadline"]'
t_contains "$(t_calls gh)" 'A person stopped it with `tehas stop` (alice, at '
t_contains "$(t_calls gh)" 'pi-rukas stopped on the cap `deadline`.'
t_contains "$(t_calls gh)" 'A person stopped it with `tehas stop` (alice, at '
# The marker is this cycle's name: it is read once and gone.
t_eq "$(find "$state/7.stopped" 2>/dev/null | wc -l)" 0

t_case "a person's stop with no cap of its own says the stop in the reason and leaves the cap empty"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended interrupted develop
# A cycle a person stopped: the record names no cap of its own, and its
# reason says who asked and when. The marker is written from a file of the
# case, so no quoting of the case reaches the bytes the cycle reads.
at=$(date +%s)
printf '%s %s\n' "$at" bob.smith_1-2 >"$fx/marker-line"
printf '%s\n' 'cat "$T/fx/marker-line" >"$TEHAS_HOME/state/cycles/7.stopped"; echo '{}' >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 137' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_eq "$(jq -c '[.status, .step, .cap]' "$rec")" '["interrupted","develop",null]'
t_eq "$(jq -r .capReason "$rec")" "tehas stop ended the cycle, run by bob.smith_1-2 at $(date -u -d "@$at" +%FT%TZ), step develop in flight"
t_eq "$(find "$state/7.stopped" 2>/dev/null | wc -l)" 0

t_case "the marker says nothing beside a time limit or a stall: tehas's own cap stands"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended running develop -
echo '{}' >"$fx/progress.json"
at=$(date +%s)
printf "%s\n" 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo 21600 tehas develop >"$TEHAS_HOME/state/cycles/7.timed"; echo "'"$at"' alice" >"$TEHAS_HOME/state/cycles/7.stopped"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_eq "$(jq -c '[.status, .cap]' "$rec")" '["timeout","wall-clock"]'
t_contains "$(jq -r .capReason "$rec")" "tehas work stopped the container at the time limit of 21600s"
t_lacks "$(jq -r .capReason "$rec")" "tehas stop ended the cycle"

t_case "a marker that holds no line, or one of the two fields, is no marker, and is gone"
reset
ended running develop
mkdir -p "$state"
for bad in '' 'not-a-time alice' '1792000000' '   ' '1792000000 alice extra'; do
  printf '%s\n' "$bad" >"$state/7.stopped"
  t_run "$cycle 7"
  t_lacks "$(t_calls gh)" "A person stopped it"
  t_eq "$(find "$state/7.stopped" 2>/dev/null | wc -l)" 0
done

t_case "a marker that is a directory, or a link, is not read, and the cycle is reported as it is"
reset
ended interrupted develop
mkdir -p "$state/7.stopped"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "A person stopped it"
t_contains "$t_out" "status=interrupted"
rmdir "$state/7.stopped"
echo "$(date +%s) alice" >"$T/elsewhere"
ln -sf "$T/elsewhere" "$state/7.stopped"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "A person stopped it"
t_eq "$([ -e "$T/elsewhere" ] && echo kept)" "kept"

t_case "a name that is not one a comment can carry is cut, and the stop is still said"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended interrupted develop
# A name of no account: the marker holds one word, and the cycle says the
# stop without a name rather than saying none, so the record still tells a
# reader that a person ended the cycle. The marker is written from a file of
# the case, so no quoting of the case reaches the bytes the cycle reads.
at=$(date +%s)
printf '%s\n' "$at" >"$fx/marker-line"
printf '%s\n' 'cat "$T/fx/marker-line" >"$TEHAS_HOME/state/cycles/7.stopped"; echo '{}' >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_contains "$(jq -r .capReason "$rec")" "tehas stop ended the cycle, run by name not recorded at "
t_contains "$(t_calls gh)" "A person stopped it with \`tehas stop\` (name not recorded, at "
t_eq "$(find "$state/7.stopped" 2>/dev/null | wc -l)" 0

t_case "a stall file left by an earlier cycle is not this cycle's"
reset
ended running develop
mkdir -p "$state"
echo 3600 >"$state/7.stalled"
t_run "$cycle 7"
t_contains "$t_out" "status=running"
t_lacks "$(t_calls gh)" "None of its logs changed"

t_case "the pi-rukas a cycle starts with is on its record, and nothing writes state/rukas-proven"
reset
rukas=$(printf 'c%.0s' {1..40})
echo "$rukas" >"$fx/image.label"
ended merged commit-pr feature/x
merged_pr
# On record before tehas work runs.
echo '"$T/store-bin" cycle-show 7 | jq -r .cycle.rukas_commit >"$T/calls/rukas.at-work"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(cat "$T/calls/rukas.at-work")" "$rukas"
t_eq "$(t_row cycle-show 7 -- .cycle.rukas_commit)" "$rukas"
rm -f "$fx/work.sh"
ended handoff commit-pr feature/x
echo OPEN >"$fx/pr-state"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Pull request waiting for a human"
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0

t_case "an end the store will not take is an ERROR with its arguments, the cycle is still reported, and the next start replaces the open row"
reset
ended handoff commit-pr feature/x
t_store_answers cycle-end 1 'store: busy #7: the database is locked'
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$t_out" "ERROR: the end of #7 is not on record (busy: store: busy #7: the database is locked): cycle-end 7 --attempt 1 --status handoff"
t_eq "$(t_calls store | grep -c '^cycle-end')" 5
t_eq "$(t_calls fake-status | wc -l)" 1
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, .cycle.ended_at] | tostring')" '[1,null]'
rm -f "$T/home/tehas/libexec/store"
cp "$T_STORE" "$T/home/tehas/libexec/store"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, (.cycle.ended_at != null)] | tostring')" '[2,true]'

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
t_eq "$(find "$T/home/tehas/tmp" -type f 2>/dev/null | wc -l)" 0

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

# The analytics program, where install.sh puts it. Each call notes how many
# history lines the issue had and whether the status was refreshed yet, and a
# run keeps its facts.
analytics_stub() { # [<bash body>]
  t_stub_analytics 'printf "%s %s\n" "$("$T/store-bin" history-show 7 | jq ".rows | length")" \
  "$([ -e "$T/calls/fake-status" ] && echo late || echo early)" >>"$T/calls/analytics.moment"
[ "$1 $3" != "run --facts" ] || cp "$4" "$T/calls/analytics.facts"
'"${1:-}"
}
# The calls, without the facts file's name, which is new each time.
analytics_calls() { t_calls analytics | sed 's/ --facts [^ ]*//'; }

t_case "the cycle records its run and its end in analytics after the history line, with what it read and removed"
reset
analytics_stub
image_id=sha256:$(printf 'e%.0s' {1..64})
echo '"$T/store-bin" cycle-image 7 --attempt "$TEHAS_CYCLE_ATTEMPT" --image '"$image_id"' --rukas 0123456789abcdef0123456789abcdef01234567 >/dev/null
echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; echo 2 >"$TEHAS_HOME/state/cycles/7.forks"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(analytics_calls)" "run 7 --image $image_id --rukas-image 0123456789abcdef0123456789abcdef01234567 --oom 3 --forks 2
event 7 ended"
t_eq "$(cat "$T/calls/analytics.moment")" "1 early
1 early"
t_eq "$(jq -c '[.line.rc, .line.status, .line.how, .line.parked, .attempt, .rukas]' "$T/calls/analytics.facts")" \
  '[2,"unknown","hand",true,1,"0123456789abcdef0123456789abcdef01234567"]'
t_eq "$(t_row history-show 7 -- '.rows[0] | [.rc, .status, .how, .parked] | tostring')" '[2,"unknown","hand",true]'
t_lacks "$t_out" "analytics"

t_case "an option with no value is left out of the run"
reset
analytics_stub
ended handoff commit-pr feature/x
t_run "$cycle 7"
t_eq "$(analytics_calls)" "run 7 --oom 0 --forks 0
event 7 ended"

t_case "a cycle stopped by a signal is recorded too"
reset
analytics_stub
echo 'kill -TERM "$PPID"; sleep 1; exit 143' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(analytics_calls)" "run 7 --oom 0 --forks 0
event 7 ended"

t_case "with TEHAS_ANALYTICS off the cycle calls no analytics"
reset
analytics_stub
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" ""

t_case "a failing analytics is a warning, and the cycle is reported as without it"
reset
ended handoff commit-pr feature/x
t_run "$cycle 7"
without=$(t_calls gh)
reset
ended handoff commit-pr feature/x
analytics_stub 'exit 70'
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN analytics run: exit 70"
t_contains "$t_out" "WARN analytics event: exit 70"
t_contains "$t_out" "done #7 rc=10 status=handoff"
t_eq "$(t_calls gh)" "$without"
t_eq "$(t_calls fake-status | wc -l)" 1
t_eq "$(t_row history-show 7 -- '.rows | length')" 1

t_case "an analytics that hangs is cut off, and the cycle is reported"
reset
analytics_stub 'exec sleep 30'
T_ENV+=(TEHAS_ANALYTICS_WAIT=1)
before=$(date +%s)
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(($(date +%s) - before < 15))" 1
# GNU timeout reports the TERM as 124; uutils reports the kill that follows
# as 137.
for sub in run event; do
  [[ $t_out =~ "WARN analytics $sub: exit "(124|137) ]] || t_bad "no time limit warning for $sub in: $t_out"
done
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "a host without the analytics program is silent"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_lacks "$t_out" "analytics"

# Outside the tests nothing names the two scripts: the cycle runs the ones in
# its own directory, whatever PATH holds.
t_case "without the test seams it runs the scripts beside it, not names on PATH"
reset
T_ENV=()
tree=$T/tree
mkdir -p "$tree/libexec" "$tree/lib"
cp "$cycle" "$tree/libexec/cycle"
cp "$T_ROOT"/lib/*.sh "$tree/lib/"
for name in work status; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
for name in tehas tehasbot tehas-work tehas-status; do t_stub "$name"; done
t_run "$tree/libexec/cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
t_eq "$(t_calls fake-status | wc -l)" 1
for name in tehas tehasbot tehas-work tehas-status; do t_eq "$(t_calls "$name")" ""; done

t_done
