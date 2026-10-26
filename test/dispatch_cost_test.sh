#!/usr/bin/env bash
# Tests for libexec/dispatch: each poll records what it cost in the state store.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

cost() { t_at "$(date +%s)" poll-show | jq -r "$1"; }
# The calls the stub recorded: a line each, and the lines of a multi-line
# argument after it start with a space.
calls() { t_calls gh | grep -cE '^(repo|api|issue|pr|label) '; }
log=$T/home/tehas/state/github-writes.log

t_case "a poll records its wall and CPU time, its gh calls and the shim's writes"
reset
: >"$log"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(cost '.last.wall_ms > 0')" true
t_eq "$(cost '.last.cpu_ms > 0')" true
t_eq "$(cost .last.gh)" "$(calls)"
writes=$(wc -l <"$log")
t_eq "$([ "$writes" -gt 0 ] && echo some)" some
t_eq "$(cost .last.writes)" "$writes"
t_eq "$(cost .last.kinds.edit)" "$(grep -c $'\tissue edit\t' "$log")"
t_eq "$(cost .hour.polls)" 1
t_contains "$(cost .line)" "poll      last "
t_lacks "$t_out" "WARN: the cost of the poll"

t_case "without a write log the writes are not on record, and the calls still are"
reset
rm -f "$log"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(cost .last.writes)" null
t_eq "$(cost .last.gh)" "$(calls)"

t_case "a dry run records nothing"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(cost .last)" null

t_case "a poll that stops early records its cost too"
reset
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
echo '[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"alice"}}]' >"$fx/events-1.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(cost '.last.gh > 0')" true

t_done
