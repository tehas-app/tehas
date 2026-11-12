#!/usr/bin/env bash
# Tests for tehasbot dispatch: the containers and locks a poll clears, a start
# that does not come up, the approval record it leaves, the analytics sample a
# start takes, and the labels a poll cannot change.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1
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

t_case "an answer that is no list of issues dispatches nothing"
reset
echo '{"oops":1}' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 1
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
T_ENV=(TEHAS_CYCLE_START_CMD=fake-late TEHAS_CYCLE_START_WAIT=1 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index TEHAS_INDEX_START_CMD=fake-index-start "$slice_env")
t_run "$dispatch"
kill "$(<"$T/late.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #7 holds its lock but did not say so in 1s; taking it as started"
t_lacks "$(t_calls gh)" "needs-human-attention"

t_case "the start of an earlier cycle on record does not count"
reset
: >"$fx/start-silent"
t_started "$T/home/tehas" 7 --by-hand
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"
t_eq "$(t_row cycle-show 7 -- .cycle.attempt)" 1
# The approval this poll recorded is taken off again: a unit that comes up
# late finds none to use.
t_eq "$(t_row approval-current 7 -- '[.approval, .current] | tostring')" "[null,null]"

# undo_busy <times>: the store answers the undo of an approval busy for its
# first <times> calls, and runs the real store for everything else.
undo_busy() {
  local bin=$T/home/tehas/libexec/store
  rm -f "$bin" "$T/undo.n"
  printf '%s\n' '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$*\" >>$(t_q "$T/calls/store")" \
    'case " $* " in *" --undo "*)' \
    "  n=\$((\$(cat $(t_q "$T/undo.n") 2>/dev/null || echo 0) + 1)); echo \"\$n\" >$(t_q "$T/undo.n")" \
    "  if [ \"\$n\" -le $1 ]; then echo 'store: busy: the database is locked' >&2; exit 1; fi ;;" \
    'esac' \
    "exec $(t_q "$T_STORE") \"\$@\"" >"$bin"
  chmod 755 "$bin"
}

t_case "a busy store is asked again to take back the approval of a start that did not come up"
reset
: >"$fx/start-silent"
undo_busy 1
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
t_stub fake-stop
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls store | grep -c -- '--undo')" 2
t_eq "$(t_row approval-current 7 -- '.approval | tostring')" null
t_eq "$(t_calls fake-stop)" ""

t_case "when the approval of a start that did not come up stays on record, the unit is stopped"
reset
: >"$fx/start-silent"
undo_busy 9
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
t_stub fake-stop
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls store | grep -c -- '--undo')" 3
t_contains "$t_out" "WARN: could not take the approval of #7 off the record (busy)"
t_eq "$(t_calls fake-stop)" 7
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that refuses its number after its start still counts as started"
reset
: >"$fx/start-refused"
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$t_out" "did not start"
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, (.cycle.ended_at != null), .cycle.end_reason] | tostring')" '[1,true,null]'

t_case "a cycle that ended at once still counts as started"
reset
# fake-start writes the marker and is gone: no lock is held when the poll looks.
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$t_out" "did not start"

# The analytics program, where install.sh puts it. Each call notes what the
# poll had done when it came: the approval record, the start, the tidy, and
# how many times the status was refreshed.
analytics_stub() { # [<bash body>]
  t_stub_analytics 'printf "%s:%s%s%s%s\n" "$*" \
  "$("$T/store-bin" approval-current 7 | jq -e ".approval != null or .current != null" >/dev/null && echo " approved")" \
  "$([ -e "$T/calls/fake-start" ] && echo " started")" \
  "$([ -e "$T/calls/fake-tidy" ] && echo " tidied")" \
  " status=$(wc -l <"$T/calls/fake-status" 2>/dev/null || echo 0)" >>"$T/calls/analytics.moment"
'"${1:-}"
}

t_case "a start records ready once the approval is written and started once the cycle runs; the poll ends with a sample and a prune"
reset
analytics_stub
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls analytics)" "event 7 ready
event 7 started
sample
prune"
t_eq "$(<"$T/calls/analytics.moment")" "event 7 ready: approved status=1
event 7 started: approved started status=1
sample: approved started tidied status=3
prune: approved started tidied status=3"
t_lacks "$t_out" "analytics"

t_case "ready is recorded at the time tehas:ready was accepted, as the approval record says it"
reset
analytics_stub
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(approval .at)" "2026-01-01T00:00:00Z"
t_eq "$(t_calls analytics)" "event 7 ready --at 1767225600
event 7 started
sample
prune"

t_case "an approval GitHub gave no time for is on record at the poll's, and ready has none"
reset
analytics_stub
before=$(date +%s)
t_run "$dispatch"
t_eq "$(($(t_row approval-current 7 -- .current.at) >= before))" 1
t_eq "$(t_calls analytics | head -n 1)" "event 7 ready"

t_case "a cycle that does not start is ready and not started"
reset
analytics_stub
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls analytics)" "event 7 ready
sample
prune"

t_case "a poll that starts nothing, a held poll and a paused poll take no sample; a poll GitHub fails still does"
reset
analytics_stub
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" "prune"
reset
analytics_stub
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_calls analytics)" "prune"
reset
analytics_stub
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_calls analytics)" "prune"
reset
# The poll's read of GitHub fails before any write, so nothing marks the queue
# changed: the sample of it would say what the poll never read, so it is the
# next poll's.
analytics_stub
rm -f "$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls analytics)" "prune"

t_case "a dry run and TEHAS_ANALYTICS=off call no analytics"
reset
analytics_stub
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" ""
reset
analytics_stub
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_ANALYTICS=off
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls analytics)" ""

t_case "a failing analytics is a warning, and the poll starts its cycle and ends as without it"
reset
analytics_stub 'exit 70'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "WARN analytics event: exit 70"
t_contains "$t_out" "WARN analytics sample: exit 70"
t_contains "$t_out" "WARN analytics prune: exit 70"
t_eq "$(grep -c "WARN analytics" <<<"$t_out")" 4
reset
analytics_stub 'exit 70'
rm -f "$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1

t_case "an analytics that hangs is cut off, and the poll goes on"
reset
analytics_stub 'exec sleep 30'
T_ENV+=(TEHAS_ANALYTICS_WAIT=1)
before=$(date +%s)
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(($(date +%s) - before < 20))" 1
t_eq "$(t_calls fake-start)" "7"
t_eq "$(grep -c "WARN analytics .*: exit 124" <<<"$t_out")" 4

# --- The approval and the start on the state store ------------------------------
t_case "a state store that is not installed stops the poll at 78 before any call to GitHub"
reset
rm -f "$T/home/tehas/libexec/store"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

t_case "a label that cannot be changed takes the approval off the record again, and the last start's stays current (A2)"
reset
past 7 bob label 2026-01-01T00:00:00Z
printf '%s\n' "--add-label tehas:running" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not mark #7 as running, not starting a cycle"
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_row approval-current 7 -- '[.approval, .current.by] | tostring')" '[null,"bob"]'
t_eq "$(t_row cycle-show 7 -- .cycle.attempt)" 1

t_case "an attempt on record that cannot be read before the label changes no label (D2)"
reset
t_store_answers cycle-show 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: #7: the store could not say which attempt is on record (busy), not starting a cycle"
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_row approval-current 7 -- '[.approval, .current] | tostring')" '[null,null]'

t_case "a running list that cannot be read ends the poll before it acts on any cycle (D2)"
reset
hold 9
t_store_answers cycle-show 1 'store: busy #0: the database is locked'
t_run "$dispatch"
release
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the state store could not list the running cycles (busy)"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""


t_done
