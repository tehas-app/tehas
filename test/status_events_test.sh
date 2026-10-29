#!/usr/bin/env bash
# Tests for the issue events libexec/status records in the factory's
# analytics: awaiting, stopped, merged and closed, each when the snapshot it
# writes differs from the one before.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# The command as a host has it: beside the console, which decides what the read
# of what closed an issue means (console/src/shipped.rs) and is this checkout's
# own build. The copy is a real file and not a symlink, because the command finds
# its siblings - health, analytics, store - beside itself by the directory it was
# run from, and a symlink there would name the checkout instead.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
shim=$T/tree
mkdir -p "$shim/libexec"
cp "$T_ROOT/libexec/status" "$shim/libexec/status"
ln -s "$T_ROOT/lib" "$shim/lib"
cp "$T_CONSOLE" "$shim/libexec/console"
for sibling in health summary analytics; do
  ln -s "$T_ROOT/libexec/$sibling" "$shim/libexec/$sibling"
done
status=$shim/libexec/status
live=$T/home/tehas/live

t_stub gh '
case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) echo "[]" ;;
  "issue view"*) echo "" ;;
  "issue edit"*) ;;
  "api repos/"*) echo "[]" ;;
  # The read of the epics: no open issue.
  "api graphql"*subIssues*) echo "{\"data\":{\"repository\":{\"issues\":{\"nodes\":[]}}}}" ;;
  "api graphql"*) cat "$T/shipped.json" ;;
esac'
now=$(date +%s)
ago() { date -u -d "@$((now - $1))" +%FT%TZ; } # <seconds>
issue() { # number label [state [closed seconds ago]]
  local closed=null
  [ -z "${4:-}" ] || closed="\"$(ago "$4")\""
  printf '{"number":%s,"title":"t","url":"u","state":"%s","labels":[{"name":"%s"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":%s}' \
    "$1" "${3:-OPEN}" "$2" "$closed"
}
issues() { local IFS=,; echo "[$*]" >"$T/issues.json"; }
merged() { # number closed-seconds-ago: an issue a merged pull request closed
  printf '{"number":%s,"title":"t","url":"u","updatedAt":"%s","closedAt":"%s","labels":{"nodes":[{"name":"tehas:shipped"}]},"timelineItems":{"nodes":[{"closer":{"merged":true}}]}}' \
    "$1" "$(ago "$2")" "$(ago "$2")"
}
shipped() { local IFS=,; printf '{"data":{"search":{"nodes":[%s]}}}' "$*" >"$T/shipped.json"; }
reset() {
  rm -rf "$T"/calls/* "$live" "$T/home/tehas/state" "$T/home/tehas/libexec"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  # The state store the health file reads the planning turns from, with
  # today's backup.
  t_store_home "$T/home/tehas"
  TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null || t_bad "reset: the store made no backup"
  issues "$(issue 3 tehas:ready)" "$(issue 5 tehas:awaiting-merge)"
  shipped
  t_stub_analytics '[ "$1" != report ] || echo "{\"report\":1}"
    [ "$1" != sessions ] || { mkdir -p "$3" && chmod 700 "$3" && echo "{\"api\":1,\"days\":30}" >"$3/sessions.jsonl"; }
    [ "$1" != outcomes ] || { mkdir -p "$3" && chmod 700 "$3" && echo "{\"api\":1,\"by\":\"version\"}" >"$3/outcomes.jsonl"; }'
}
# The calls that record events: every run also asks for the report, the
# sessions and the outcomes (status_analytics_test.sh).
# The two mcp/ files are asked for with --out and its directory, whose name
# holds this run's scratch directory: cut the line, not the whole call.
events() { t_calls analytics | grep -Ev '^(orders$|report --json|(sessions|outcomes) --out )' || true; }
# The snapshot of the run before, written by status itself, then the calls
# of the next run only.
again() { rm -f "$T/calls/analytics"; t_run "$status"; }

t_case "a first run, with no snapshot before it, records no awaiting or stopped"
reset
issues "$(issue 5 tehas:awaiting-merge)" "$(issue 6 needs-human-attention)"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r '.awaiting | length' "$live/status.json")" 1
t_eq "$(events)" ""

t_case "an issue that is newly awaiting or newly stopped is recorded once, at the time of the run"
reset
t_run "$status"
issues "$(issue 3 tehas:awaiting-merge)" "$(issue 5 tehas:awaiting-merge)" "$(issue 6 needs-human-attention)"
again
t_eq "$t_rc" 0
t_eq "$(events | sort)" "event 3 awaiting
event 6 stopped"
t_eq "$t_out" ""

t_case "an unchanged snapshot records nothing"
again
t_eq "$(events)" ""

t_case "a merge is recorded at the time the issue was closed, the first time it is completed"
reset
t_run "$status"
shipped "$(merged 20 3600)"
again
t_eq "$(events)" "event 20 merged --at $((now - 3600))"
shipped "$(merged 21 60)" "$(merged 20 3600)"
again
t_eq "$(events)" "event 21 merged --at $((now - 60))"

t_case "a first run records the merges of the last ten days, with their times, and none older"
reset
shipped "$(merged 20 3600)" "$(merged 19 $((11 * 86400)))"
t_run "$status"
t_eq "$(events)" "event 20 merged --at $((now - 3600))"

t_case "an issue of the snapshot before that is closed and not merged is recorded as closed, at its time"
reset
issues "$(issue 3 tehas:ready)" "$(issue 6 needs-human-attention)" "$(issue 8 tehas:running)"
t_run "$status"
issues "$(issue 3 tehas:ready CLOSED 120)" "$(issue 6 needs-human-attention CLOSED 60)" "$(issue 8 tehas:running)" \
  "$(issue 9 tehas:ready CLOSED 30)"
shipped "$(merged 6 60)"
again
t_eq "$(events | sort)" "event 3 closed --at $((now - 120))
event 6 merged --at $((now - 60))"
again
t_eq "$(events)" ""

t_case "an issue that was awaiting a merge and is closed, not yet completed, is not recorded as closed"
reset
issues "$(issue 5 tehas:awaiting-merge)" "$(issue 6 needs-human-attention)"
t_run "$status"
# 6 is awaiting and stopped at once, as a handoff with a pull request can be.
issues "$(issue 5 tehas:awaiting-merge)" "$(issue 6 needs-human-attention)" "$(issue 6 tehas:awaiting-merge)"
again
issues "$(issue 5 tehas:awaiting-merge CLOSED 60)" "$(issue 6 needs-human-attention CLOSED 60)"
again
t_eq "$(events)" ""
shipped "$(merged 5 60)" "$(merged 6 60)"
again
t_eq "$(events | sort)" "event 5 merged --at $((now - 60))
event 6 merged --at $((now - 60))"

t_case "an issue that was awaiting a merge and is closed, not yet completed, is not recorded as closed"
reset
issues "$(issue 5 tehas:awaiting-merge)" "$(issue 6 needs-human-attention)"
t_run "$status"
# 6 is awaiting and stopped at once, as a handoff with a pull request can be.
issues "$(issue 5 tehas:awaiting-merge)" "$(issue 6 needs-human-attention)" "$(issue 6 tehas:awaiting-merge)"
again
issues "$(issue 5 tehas:awaiting-merge CLOSED 60)" "$(issue 6 needs-human-attention CLOSED 60)"
again
t_eq "$(events)" ""
shipped "$(merged 5 60)" "$(merged 6 60)"
again
t_eq "$(events | sort)" "event 5 merged --at $((now - 60))
event 6 merged --at $((now - 60))"

t_case "a first run records the closed issues it is shown, with their times"
reset
issues "$(issue 3 tehas:ready CLOSED 120)" "$(issue 1 tehas:ready CLOSED 60)" "$(issue 4 tehas:ready CLOSED $((11 * 86400)))"
t_run "$status"
t_eq "$(events)" "event 3 closed --at $((now - 120))"

t_case "a snapshot before more than an hour old counts as none"
reset
t_run "$status"
jq --arg at "$(ago 3601)" '.generatedAt = $at' "$live/status.json" >"$T/old.json" && mv "$T/old.json" "$live/status.json"
issues "$(issue 3 tehas:awaiting-merge)" "$(issue 5 tehas:awaiting-merge)"
again
t_eq "$(events)" ""

t_case "a snapshot before that is not one counts as none"
reset
mkdir -p "$live"
echo 'not json' >"$live/status.json"
issues "$(issue 3 tehas:awaiting-merge)"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r '.awaiting | length' "$live/status.json")" 1
t_eq "$(events)" ""

t_case "the control issue is never recorded"
reset
t_run "$status"
issues "$(issue 1 tehas:awaiting-merge)" "$(issue 1 needs-human-attention)"
again
t_eq "$(events)" ""

t_case "with TEHAS_ANALYTICS off nothing is recorded"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_run "$status"
issues "$(issue 3 tehas:awaiting-merge)"
shipped "$(merged 20 60)"
again
t_eq "$t_rc" 0
t_eq "$(jq -r '.awaiting | length' "$live/status.json")" 1
t_eq "$(events)" ""

t_case "a snapshot that is not written records nothing"
reset
issues "$(issue 3 tehas:ready)"
t_run "$status"
issues "$(issue 3 tehas:awaiting-merge)"
echo '{"errors": [{"message": "rate limited"}]}' >"$T/shipped.json"
again
t_eq "$t_rc" 0
t_eq "$(jq -r '.awaiting | length' "$live/status.json")" 0
t_eq "$(events)" ""

t_case "a failing analytics is a warning on standard error, and the snapshot is written"
reset
t_run "$status"
t_stub_analytics 'exit 70'
issues "$(issue 3 tehas:awaiting-merge)"
again
t_eq "$t_rc" 0
t_contains "$t_out" "WARN analytics event: exit 70"
t_eq "$(jq -r '.awaiting | length' "$live/status.json")" 1
t_eq "$(jq -r .status "$live/health.json")" pass

t_case "an analytics that hangs costs at most three calls' time, said once"
reset
nodes=()
for n in 20 21 22 23 24 25 26 27 28 29; do nodes+=("$(merged "$n" 60)"); done
shipped "${nodes[@]}"
t_stub_analytics 'exec sleep 30'
T_ENV=(TEHAS_ANALYTICS_WAIT=1)
t_run "$status"
t_eq "$t_rc" 0
# Each call is cut at the 1 s limit, so three calls is three calls' time. A
# count, not a clock: the clock fails on a loaded runner (#655).
t_eq "$(grep -c 'WARN analytics event: exit' <<<"$t_out")" 3
t_contains "$t_out" "WARN analytics event: exit 124"
t_contains "$t_out" "WARN analytics: out of time, the other events of this run are left out"
t_eq "$(grep -c 'out of time' <<<"$t_out")" 1
t_eq "$(jq -r '.completed | length' "$live/status.json")" 10

t_done
