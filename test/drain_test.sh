#!/usr/bin/env bash
# Tests for tehas drain: set a limit of no new work, and wait for the
# running cycles to end.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# The limit is read and written by the console (console flags).
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")

drain="$T_ROOT/bin/tehas drain"
file=$T/home/tehas/state/limit.json
cycles=$T/home/tehas/state/cycles
home=$T/home/tehas
t_stub gh

# A poll of the dispatcher: state/dispatch.lock, taken and held to the end of the
# poll as libexec/dispatch holds it, so drain waits for it.
#
# poll_hold <cycle-after> <hold> [<issue>]: a poll that starts <issue>'s cycle
# <cycle-after> seconds in and ends <hold> seconds in. With no issue it starts
# nothing. <cycle-after> is past the limit drain has written by then: the limit
# holds only a poll that has yet to read it, so a cycle started after the write is
# one drain can miss, and only the take of the lock counts it. The cycle takes the
# issue's lock in its own descriptor and outlives the poll, as a cycle does
# (libexec/cycle does the same with its 9), so drain finds it once the poll is
# gone. The poll takes the dispatcher's lock the same way, in a 9 of its own: a
# poll started here is a child of the shell that waits for it, and a lock it held
# in an 8 would be a lock every drain of every later case inherits, which is the
# one thing that makes a wait for a poll answer that no poll is running. The poll
# ends by itself, so a drain that waits for it is timed by the poll rather than
# hanging on one that outlives the case.
poll_pid=""
poll_hold() { # <cycle-after> <hold> [<issue>]
  local after=$1 hold=$2 n=${3:-}
  # A cycle of the factory holds the issue's lock till it ends: the sleeper is
  # that, in its own descriptor. Without an issue the poll starts nothing, which
  # the poll's own branch says, not a `true &` that would take the whole list with
  # it and let the poll end before the start was even tried.
  printf '#!%s\nexec 9>>"%s" && flock 9\nsleep %s\nif [ -n "%s" ]; then\n  exec 9>"%s" && flock 9 && exec sleep 60\nfi\nsleep %s\n' \
    "$T_BASH" "$home/state/dispatch.lock" "$after" "$n" "$cycles/${n:-0}.lock" "$hold" >"$T/bin/held-poll"
  chmod +x "$T/bin/held-poll"
  env -i PATH="$T/bin:$PATH" HOME="$T/home" TEHAS_HOME="$home" T="$T" \
    TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR" \
    held-poll >/dev/null 2>&1 &
  poll_pid=$!
  while flock -n "$home/state/dispatch.lock" true; do sleep 0.02; done
}
poll_end() {
  [ -n "$poll_pid" ] || return 0
  kill "$poll_pid" 2>/dev/null
  wait "$poll_pid" 2>/dev/null
  poll_pid=""
}

reset() {
  poll_end
  rm -rf "$T"/calls/* "$T/home/tehas"
  mkdir -p "$cycles"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
# A running cycle: its lock, held by a sleeper for <seconds>.
running() { # <issue> <seconds>
  flock -n "$cycles/$1.lock" sleep "$2" &
  sleep 0.2
}

t_case "nothing running: the limit is set, and drain is done at once"
reset
t_run "$drain"
t_eq "$t_rc" 0
t_contains "$t_out" "limit     0 cycles at once, planning off, until cleared: draining"
t_contains "$t_out" "drained: no cycle is running"
t_eq "$(jq -c '{v, cycles, plan, until, reason}' "$file")" '{"v":1,"cycles":0,"plan":false,"until":null,"reason":"draining"}'
t_eq "$(t_calls gh)" ""

t_case "--json: one object with the running cycles and the limit"
reset
t_run "$drain --json"
t_eq "$t_rc" 0
t_eq "$t_out" '{"api":1,"drained":true,"running":[],"limit":{"state":"ok","cycles":0,"plan":false,"reason":"draining"}}'

t_case "a cycle still running without a wait: exit 1, and it is named"
reset
running 7 5
t_run "$drain --json"
t_eq "$t_rc" 1
t_eq "$(jq -c '[.drained, .running]' <<<"$t_out")" '[false,[7]]'
t_run "$drain"
t_eq "$t_rc" 1
t_contains "$t_out" "still running: #7"
wait

t_case "a wait that ends with a cycle running is exit 1"
reset
running 7 5
start=$(date +%s)
t_run "$drain --wait 1"
t_eq "$t_rc" 1
t_eq "$(( $(date +%s) - start < 5 ))" 1
wait

t_case "a cycle that ends during the wait: exit 0"
reset
running 7 1
t_run "$drain --wait 20 --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.drained, .running]' <<<"$t_out")" '[true,[]]'
wait

t_case "drain replaces a limit with an expiry by one that lasts until cleared"
reset
jq -nc '{v: 1, cycles: 3, plan: true, until: "2036-01-01T00:00:00Z", reason: "industry"}' >"$file"
t_run "$drain"
t_eq "$(jq -c '{cycles, plan, until, reason}' "$file")" '{"cycles":0,"plan":false,"until":null,"reason":"draining"}'

t_case "bad arguments are 64 and write nothing"
for args in "--wait" "--wait x" "--wait -1" "--wait 1 --wait 2" "--json --json" "now" "--bogus"; do
  reset
  t_run "$drain $args"
  [ "$t_rc" = 64 ] || t_bad "'$args' exited $t_rc, not 64"
  [ ! -e "$file" ] || t_bad "'$args' wrote $file"
done
t_contains "$t_out" "usage: tehas drain"

t_case "without an instance nothing is written, exit 78"
reset
t_no_instance
t_run "$drain"
t_eq "$t_rc" 78
t_eq "$([ -e "$file" ] && echo there)" ""

# The race of #795, from both sides. The limit holds only a poll that has yet to
# read it, so drain takes the dispatcher's lock before it counts.
t_case "a cycle a running poll starts after the limit was written: drain counts it"
reset
# The poll outlives this drain, so the drain cannot have counted a cycle of a
# poll it had already let go: the take of the lock is what waits for it.
poll_hold 0.4 30 7
t_run "$drain --wait 4 --json"
poll_end
t_eq "$t_rc" 1
t_eq "$(jq -c '[.drained, .running]' <<<"$t_out")" '[false,[7]]'

t_case "a poll that starts nothing: drain waits for it, then answers no cycle"
reset
poll_hold 0 1
start=$(date +%s)
t_run "$drain --wait 4 --json"
poll_end
t_eq "$t_rc" 0
t_eq "$t_out" '{"api":1,"drained":true,"running":[],"limit":{"state":"ok","cycles":0,"plan":false,"reason":"draining"}}'
# It waited for the poll, which held the lock a second, and not for its --wait.
t_eq "$(( $(date +%s) - start < 4 ))" 1
poll_end

t_done
