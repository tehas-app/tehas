#!/usr/bin/env bash
# Tests for the three scripts around the factory's state: libexec/status
# writes the queue snapshot and runs libexec/health beside it, and
# libexec/summary prints the health file for `tehas status`.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

status="$T_ROOT/libexec/status"
health="$T_ROOT/libexec/health"
summary="$T_ROOT/libexec/summary"
live=$T/home/tehas/live

t_stub gh '
[ ! -e "$T/gh-down" ] || exit 1
case "$1 $2" in
  "issue list") cat "$T/issues.json" ;;
  "issue view") cat "$T/paused" ;;
esac'
issue() { # number label state
  printf '{"number":%s,"title":"t","url":"u","state":"%s","labels":[{"name":"%s"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":null}' "$1" "${3:-OPEN}" "$2"
}
reset() {
  rm -rf "$T"/calls/* "$T/gh-down" "$live"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  echo "[$(issue 3 tehas:ready),$(issue 4 tehas:ready),$(issue 5 tehas:awaiting-merge),$(issue 6 needs-human-attention)]" >"$T/issues.json"
  echo false >"$T/paused"
}
cycle() { # running status step
  mkdir -p "$live"
  printf '{"issue":12,"running":%s,"status":"%s","step":"%s","startedAt":%s,"updatedAt":%s}\n' \
    "$1" "$2" "$3" "$(($(date +%s) - 600))" "$(($(date +%s) - 180))" >"$live/running.json"
}

t_case "status writes the snapshot and the health file, without a command on PATH"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(jq -r '.queued | length' "$live/status.json")" 2
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r .checks.queue.review "$live/health.json")" 1

t_case "with GitHub down the old snapshot stays and health is still refreshed"
before=$(cksum <"$live/status.json")
rm "$live/health.json"
: >"$T/gh-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(cksum <"$live/status.json")" "$before"
t_eq "$(jq -r .status "$live/health.json")" pass

t_case "status with invalid settings writes nothing, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$status"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""
t_eq "$([ -e "$live" ] && echo there)" ""

t_case "summary: an idle factory in three lines, exit 0"
reset
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 3
t_contains "$t_out" "factory   pass: Idle, taking work. ("
t_contains "$t_out" "s ago)"
t_contains "$t_out" "queue     2 in line, 0 building, 1 in review, 1 stopped"
t_contains "$t_out" "cycle     none yet"

t_case "summary: a running cycle"
reset
cycle true running develop
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #12."
t_contains "$t_out" "cycle     #12 running, step develop, started 10 min ago"

t_case "summary: a parked cycle is not a failure"
reset
cycle false handoff review
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "cycle     #12 handoff at step review, 3 min ago"

t_case "summary: a paused factory says so, exit 0"
reset
echo true >"$T/paused"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   warn: The factory is paused."
t_contains "$t_out" "1 stopped, paused"

t_case "summary: a failed cycle is exit 1, and still printed"
reset
cycle false aborted develop
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: The last cycle failed: #12 ended as aborted."
t_contains "$t_out" "cycle     #12 aborted at step develop"

t_case "summary: no snapshot yet is a failure with the reason"
reset
t_run "$health; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: No queue snapshot yet"
t_contains "$t_out" "queue     no snapshot yet"

t_case "summary: no health file is exit 1 with one line, and nothing is written"
reset
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas status: no health file at $live/health.json yet"
t_eq "$([ -e "$live" ] && echo there)" ""

t_case "summary: a health file that is not JSON is the same"
reset
mkdir -p "$live"
echo "not json" >"$live/health.json"
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "no health file"

t_case "summary reads nothing but the health file: no settings, no gh, no write"
reset
t_run "$status"
rm -f "$T"/calls/*
t_instance 'echo "settings were read" >&2; exit 9'
before=$(cd "$live" && cksum ./*)
t_run "$summary"
t_eq "$t_rc" 0
t_lacks "$t_out" "settings were read"
t_eq "$(t_calls gh)" ""
t_eq "$(cd "$live" && cksum ./*)" "$before"

t_case "summary takes no argument"
reset
t_run "$summary --json"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas status"

t_done
