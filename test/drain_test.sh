#!/usr/bin/env bash
# Tests for libexec/drain: set a limit of no new work, and wait for the
# running cycles to end.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

drain="$T_ROOT/libexec/drain"
file=$T/home/tehas/state/limit.json
cycles=$T/home/tehas/state/cycles
t_stub gh

reset() {
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
rm -rf "$TEHAS_INSTANCE_DIR"
t_run "$drain"
t_eq "$t_rc" 78
t_eq "$([ -e "$file" ] && echo there)" ""

t_done
