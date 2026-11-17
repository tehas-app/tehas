#!/usr/bin/env bash
# Tests for tehas analytics: what it asks of the
# analytics program, and what it refuses. The program is a stub here; its
# Rust tests (console/analytics/tests/command.rs) run this command against
# the real one.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

cmd="$T_ROOT/bin/tehas analytics"
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")
# Nothing it does may reach GitHub, Docker or git.
for tool in gh docker git curl; do t_stub "$tool" 'exit 1'; done
outside() { printf '%s' "$(t_calls gh)$(t_calls docker)$(t_calls git)$(t_calls curl)"; }

reset() {
  rm -rf "$T"/calls/* "$T/home/tehas"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}

t_case "report runs the program's report with TEHAS_HOME and prints what it prints"
reset
t_stub_analytics 'echo "$TEHAS_HOME" >"$T/calls/analytics.home"; echo "Last 10 days"'
t_run "$cmd report"
t_eq "$t_rc" 0
t_eq "$t_out" "Last 10 days"
t_eq "$(t_calls analytics)" "report"
t_eq "$(cat "$T/calls/analytics.home")" "$T/home/tehas"
t_eq "$(outside)" ""

t_case "copy passes --to, the file and --json on, and prints the program's object"
reset
t_stub_analytics 'echo "{\"api\":1,\"schema\":1,\"runs\":0}"'
t_run "$cmd copy --to \"\$T/copy.db\" --json"
t_eq "$t_rc" 0
t_eq "$t_out" '{"api":1,"schema":1,"runs":0}'
t_eq "$(t_calls analytics)" "copy --to $T/copy.db --json"
t_run "$cmd copy --to copy.db"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics | tail -n 1)" "copy --to copy.db"
t_run "$cmd copy --json --to copy.db"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics | tail -n 1)" "copy --json --to copy.db"
t_eq "$(outside)" ""

t_case "export, the name before, is 64 and names copy --to, before the settings are read"
reset
t_no_instance
t_stub_analytics
for args in "export copy.db" "export copy.db --json"; do
  t_run "$cmd $args"
  t_eq "$t_rc" 64
  t_contains "$t_out" "tehas analytics copy --to <file>"
done
t_eq "$(t_calls analytics)" ""

t_case "a failure of the program is exit 1: a file that exists, a database that cannot be read"
reset
t_stub_analytics 'echo "analytics: copy.db exists already" >&2; exit 73'
t_run "$cmd copy --to copy.db --json"
t_eq "$t_rc" 1
t_contains "$t_out" "exists already"
t_stub_analytics 'echo "analytics: the report failed" >&2; exit 70'
t_run "$cmd report"
t_eq "$t_rc" 1
t_contains "$t_out" "the report failed"

t_case "the program's 0, 64, 78 and 141 are the command's"
for rc in 0 64 78 141; do
  reset
  t_stub_analytics "exit $rc"
  t_run "$cmd report"
  t_eq "$t_rc" "$rc"
done

t_case "with TEHAS_ANALYTICS=off it says so, exits 78 and asks the program nothing"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_stub_analytics
before=$(cd "$T/home" && find . | LC_ALL=C sort)
for args in report "copy --to copy.db --json"; do
  t_run "$cmd $args"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_ANALYTICS=off"
done
t_eq "$(t_calls analytics)" ""
t_eq "$(cd "$T/home" && find . | LC_ALL=C sort)" "$before"
t_eq "$([ -e copy.db ] && echo written)" ""
t_eq "$(outside)" ""

t_case "a host without the program is exit 1 and says so"
reset
t_run "$cmd report"
t_eq "$t_rc" 1
t_contains "$t_out" "is not installed"

t_case "missing or malformed settings are exit 78 and ask the program nothing"
reset
t_stub_analytics
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=sometimes
t_run "$cmd report"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ANALYTICS"
t_no_instance
t_run "$cmd copy --to copy.db --json"
t_eq "$t_rc" 78
t_eq "$(t_calls analytics)" ""

t_case "a bad argument is exit 64 with the usage, before the settings are read"
reset
t_instance
t_stub_analytics
for args in "" "report --json" "report now" "copy" "copy a" "copy --to" "copy --to ''" \
  "copy --to --json" "copy --to -o" "copy --to a --yaml" "copy --to a --json now" "prune" "run 7" "sample"; do
  t_run "$cmd $args"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehas analytics"
done
t_eq "$(t_calls analytics)" ""

t_case "it writes nothing of its own"
reset
t_stub_analytics
before=$(cd "$T/home" && find . | LC_ALL=C sort)
t_run "$cmd report"
t_eq "$(cd "$T/home" && find . | LC_ALL=C sort)" "$before"

t_done
