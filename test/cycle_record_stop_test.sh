#!/usr/bin/env bash
# Tests for libexec/cycle: a cycle stopped at a limit or by a person: the time
# limit, the stall, the signal, the caps it parks under, and the notes it reads.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1
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

t_case "a cycle pi-rukas ended at a child's run bound is on record as child-bound, not aborted"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended aborted develop feature/issue-7-x child-timeout:develop
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; exit 20' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c '[.status, .step, .rc, .cap]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  '["child-bound","develop",20,"child-timeout:develop"]'

t_case "a cycle a signal stopped inside a gate is on record with the cap signal and its gate closed"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended interrupted develop
echo '{}' >"$fx/progress.json"
# The stream and the note as tehas work leaves them for a TERM in the gate of
# step develop: the gate's start, the line that closes it, and the signal.
jq -nc '{v: 1, kind: "verify-started", issue: 7, event: {kind: "verify-started", step: "develop", command: "make test"}}' >"$fx/stream"
jq -nc '{v: 1, kind: "verify-aborted", issue: 7, by: "tehas", event: {kind: "verify-aborted", step: "develop", command: "make test", status: "aborted", signal: "TERM"}}' >>"$fx/stream"
echo 'cp "$T/fx/stream" "$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo TERM develop >"$TEHAS_HOME/state/cycles/7.signal"; exit 21' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000
t_eq "$(jq -c '[.status, .step, .cap, .capReason]' "$rec/cycle.json")" \
  '["interrupted","develop","signal","SIGTERM came to tehas work and it stopped the container, step develop in flight"]'
t_eq "$(jq -c 'select(.kind == "verify-aborted") | [.event.step, .event.status]' "$rec/stream.jsonl")" '["develop","aborted"]'
t_contains "$(t_calls gh)" 'tehas stopped the cycle: the cap `signal`.'
# The note is read once and gone.
t_eq "$(find "$state/7.signal" 2>/dev/null | wc -l)" 0

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

# #1207: the seconds of a stop file and its step come of one read, so a first
# word that is no count of seconds is no stop and no step. The report of before
# read the file twice and the two reads disagreed: the cycle was said stalled
# with a step of the same unreadable file.
t_case "a stop file of no count of seconds is no stalled status and no stalled step"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended aborted develop
printf "%s\n" 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; printf "nope\nstall-step\n" >"$TEHAS_HOME/state/cycles/7.stalled"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_eq "$(jq -c '[.status, .step, .cap]' "$rec")" '["aborted","develop",null]'
t_lacks "$t_out" "status=stalled"
t_lacks "$(t_calls gh)" "stall-step"
t_lacks "$(t_calls gh)" "None of its logs changed"
t_eq "$(find "$state/7.stalled" 2>/dev/null | wc -l)" 0

t_case "a stop file of no count of seconds is no timeout and no step of the time limit"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended aborted develop
printf "%s\n" 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; echo "nope tehas timed-step" >"$TEHAS_HOME/state/cycles/7.timed"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
rec=$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json
t_eq "$(jq -c '[.status, .step, .cap]' "$rec")" '["aborted","develop",null]'
t_lacks "$t_out" "status=timeout"
t_lacks "$(t_calls gh)" "timed-step"
t_eq "$(find "$state/7.timed" 2>/dev/null | wc -l)" 0

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

t_done
