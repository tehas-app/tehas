#!/usr/bin/env bash
# Tests for libexec/dispatch: starting a cycle, the poll's own lock, issues
# marked as running with no cycle, left containers and start markers.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh"

t_case "the cycle is started for the issue and told the labeller in a comment"
reset
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "Tehas cycle started on"
t_contains "$(t_calls gh)" "authorized by @alice"

t_case "the poll's lock is not passed on to the cycle it starts"
t_eq "$(find "$T/calls/start.fd8" 2>/dev/null | wc -l)" 0

t_case "the status is refreshed before the poll and after the start"
t_eq "$(t_calls fake-status | wc -l)" 2

t_case "for one issue: the running label, then the start comment, then the start"
reset
# fake-order notes what the gh calls held when the start was asked for.
t_stub fake-order 'grep -q -e "--add-label tehas:running" "$T/calls/gh" && : >"$T/calls/order.label"
grep -q "^issue comment $1 " "$T/calls/gh" && : >"$T/calls/order.comment"
: >"$TEHAS_HOME/state/cycles/$1.taken"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-order TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index "$slice_env")
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls/order.label" "$T/calls/order.comment" 2>/dev/null | wc -l)" 2
label_at=$(grep -n -e "--add-label tehas:running" "$T/calls/gh" | head -n 1 | cut -d: -f1)
comment_at=$(grep -n "^issue comment 7 " "$T/calls/gh" | head -n 1 | cut -d: -f1)
t_eq "$([ "$label_at" -lt "$comment_at" ] && echo before)" "before"

t_case "the poll does not wait for the cycle: it returns while the cycle lives, with its own lock free"
reset
# The cycle outlives the poll and holds its own lock. It must not hold the
# poll's, so the next poll can run. Its output is detached from the test's pipe.
t_stub fake-live '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 8 ) >/dev/null 2>&1 &
echo $! >"$T/live.pid"
: >"$TEHAS_HOME/state/cycles/$1.taken"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-live TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index "$slice_env")
t_run "$dispatch"
alive=no
kill -0 "$(cat "$T/live.pid")" 2>/dev/null && alive=yes
free=$(flock -n "$T/home/tehas/state/dispatch.lock" echo free)
kill "$(cat "$T/live.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_eq "$alive" yes
t_eq "$free" free

t_case "the poll's lock is not handed to the index"
reset
t_run "$dispatch"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(find "$T/calls/index.fd8" 2>/dev/null | wc -l)" 0

t_case "a real poll records when it took the lock"
t_eq "$(($(date +%s) - $(cat "$T/home/tehas/state/dispatch.taken") < 60))" 1

t_case "a poll that finds the lock held for longer than a turn may take says so once, and exits 0"
reset
taken_file=$T/home/tehas/state/dispatch.taken
rm -f "$T/home/tehas/state/dispatch.stuck"
echo $(($(date +%s) - 6000)) >"$taken_file"
exec {lockfd}>>"$T/home/tehas/state/dispatch.lock"
flock -n "$lockfd"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the dispatcher's lock has been held since"
t_contains "$t_out" "more than 5400s: a poll or tehas index hangs, and no poll runs until it ends"
t_eq "$(t_calls gh)" ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "a poll that finds the lock held for a short while says nothing"
stamp=$(($(date +%s) - 60))
echo "$stamp" >"$taken_file"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "another poll, or tehas index, holds the dispatcher's lock"
t_lacks "$t_out" "WARN"

t_case "a poll that finds the lock held does not change when it was taken"
t_eq "$(cat "$taken_file")" "$stamp"

t_case "a poll that finds the lock held still takes the ended cycle's request, so the path unit does not start poll after poll"
again=$T/home/tehas/state/dispatch.again
: >"$again"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$again" 2>/dev/null | wc -l)" 0
exec {lockfd}>&-

t_case "a poll takes the request an ended cycle left, and a dry run leaves it"
reset
: >"$again"
t_run "$dispatch --dry-run"
t_eq "$(find "$again" 2>/dev/null | wc -l)" 1
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$again" 2>/dev/null | wc -l)" 0

t_case "while a cycle is running nothing is started"
reset
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 cycles are running (limit 1)"
release

t_case "an issue marked as running with no cycle behind it is handed to a human"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "park #9: it is marked tehas:running and no cycle holds its lock"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "issue comment 9 --repo acme/widget --body Tehas found this issue marked \`tehas:running\` with no cycle behind it"
t_contains "$(t_calls gh)" "logs/dispatch-work-9.log"
t_contains "$(t_calls gh)" "To retry, apply \`tehas:ready\`. If it is on the issue already, nothing more is needed."
t_lacks "$(t_calls gh)" "remove \`needs-human-attention\`"

t_case "and the poll goes on to the queue, and starts nothing for that issue"
t_eq "$(t_calls fake-start)" "7"

t_case "an issue whose cycle ended after the list was read is not parked"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"number":9,"state":"open","labels":[{"name":"tehas:awaiting-merge"}]}' >"$fx/work-9.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #9: tehas:running is gone from it since the list was read"
t_lacks "$(t_calls gh)" "issue edit 9"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"

t_case "the issue is read again before it is parked, and a failed read ends the poll"
reset
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/work-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/work-fails"

t_case "a closed issue marked as running with no cycle loses the mark and is not parked"
reset
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 9 --repo acme/widget --remove-label tehas:running')" 1
t_lacks "$(t_calls gh)" "issue comment 9"
t_contains "$t_out" "clear #9: it is closed, marked tehas:running, and no cycle holds its lock"

t_case "a closed issue whose cycle still holds its lock is left alone"
reset
slots 2
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
hold 9
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit 9"
release

t_case "closed issues are asked for too"
reset
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue list --repo acme/widget --state all --label tehas:running"

t_case "a lock file nobody holds does not make it a running cycle"
reset
echo '[{"number":9}]' >"$fx/marked.json"
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/9.lock"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"

t_case "an issue whose cycle holds its lock is left alone"
reset
slots 2
echo '[{"number":9}]' >"$fx/marked.json"
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "issue edit 9"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"
release

t_case "it is found while the slots are full too"
reset
echo '[{"number":8},{"number":9}]' >"$fx/marked.json"
hold 8
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit 8"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_eq "$(t_calls fake-start)" ""
release

t_case "when its label cannot be changed nothing is said on it, and the poll goes on"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo "issue edit 9" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: label change on #9 failed"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"

t_case "a dry run says so and changes nothing"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would park #9: it is marked tehas:running and no cycle holds its lock"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls docker)" "rm "

t_case "a container left by a cycle whose lock nobody holds is removed, and that is logged"
reset
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "rm -f tehas-work-9"
t_contains "$t_out" "removed the container tehas-work-9: no cycle holds the lock of #9"

t_case "with no container left the removal says nothing"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_contains "$(t_calls docker)" "rm -f tehas-work-9"
t_lacks "$t_out" "removed the container"

t_case "a closed issue's left container goes too"
reset
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
t_run "$dispatch"
t_contains "$t_out" "removed the container tehas-work-9"

t_case "the container of a cycle that holds its lock is not touched"
reset
slots 2
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
hold 9
t_run "$dispatch"
t_lacks "$(t_calls docker)" "rm "
release

t_case "a paused factory still clears it"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_contains "$t_out" "park #9: it is marked tehas:running and no cycle holds its lock"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"

t_case "when the issues marked as running cannot be read nothing is dispatched"
reset
rm -f "$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue list failed"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "an answer that is no list of issues dispatches nothing"
reset
echo '{"oops":1}' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: unreadable list of issues marked as running"
t_eq "$(t_calls fake-start)" ""

t_case "the lock file of a cycle that ended holds nothing back"
reset
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/9.lock"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a cycle that cannot be started is handed to a human, exit 1"
reset
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start the cycle"

t_case "a cycle that never says it holds its lock is handed to a human"
reset
: >"$fx/start-silent"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that holds its lock but wrote no marker in time is taken as started"
reset
# The holder keeps the cycle's lock for a few seconds with its output detached,
# so the stub returns at once and the test's pipe is not held open.
t_stub fake-late '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 5 ) >/dev/null 2>&1 &
echo $! >"$T/late.pid"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-late TEHAS_CYCLE_START_WAIT=1 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index "$slice_env")
t_run "$dispatch"
kill "$(cat "$T/late.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #7 holds its lock but did not say so in 1s; taking it as started"
t_lacks "$(t_calls gh)" "needs-human-attention"

t_case "a start marker left by an earlier cycle does not count"
reset
: >"$fx/start-silent"
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/7.taken"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"

t_case "a cycle that ended at once still counts as started"
reset
# fake-start writes the marker and is gone: no lock is held when the poll looks.
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$t_out" "did not start"

t_done
