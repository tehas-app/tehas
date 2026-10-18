#!/usr/bin/env bash
# Tests for lib/analytics.sh: how the commands call the analytics program.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

call='log() { printf "%s %s\n" "$(date -u +%FT%TZ)" "$*"; }
. "$T_ROOT/lib/analytics.sh"'
bin=$T/home/tehas/libexec/analytics

t_case "a call runs the program with its arguments and TEHAS_HOME, and says nothing"
t_stub_analytics 'echo "$TEHAS_HOME" >"$T/calls/analytics.home"; echo printed'
T_ENV=(TEHAS_ANALYTICS=on)
t_run "$call; tehas_analytics event 7 ready; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(t_calls analytics)" "event 7 ready"
t_eq "$(cat "$T/calls/analytics.home")" "$T/home/tehas"

t_case "a failing program is one WARN line with its exit, and the call still returns 0"
rm -f "$T"/calls/*
t_stub_analytics 'echo "analytics: the database could not be written" >&2; exit 70'
t_run "$call; tehas_analytics run 7 --oom 0; echo rc=\$?"
t_contains "$t_out" "WARN analytics run: exit 70"
t_contains "$t_out" "analytics: the database could not be written"
t_contains "$t_out" "rc=0"
t_eq "$(grep -c WARN <<<"$t_out")" 1

t_case "a program that hangs is cut off at the time limit"
rm -f "$T"/calls/*
t_stub_analytics 'exec sleep 30'
T_ENV=(TEHAS_ANALYTICS=on TEHAS_ANALYTICS_WAIT=1)
before=$(date +%s)
t_run "$call; tehas_analytics sample; echo rc=\$?"
t_eq "$(($(date +%s) - before < 10))" 1
t_contains "$t_out" "WARN analytics sample: exit 124"
t_contains "$t_out" "rc=0"

t_case "the limit is a few seconds, and a wait that is not a number does not change it"
t_eq "$(sed -n 's/^tehas_analytics_limit=//p' "$T_ROOT/lib/analytics.sh")" 5
t_stub timeout 'echo "$1 $2 $3" >"$T/calls/timeout.args"'
T_ENV=(TEHAS_ANALYTICS=on TEHAS_ANALYTICS_WAIT=soon)
t_run "$call; tehas_analytics prune"
t_eq "$(cat "$T/calls/timeout.args")" "-k 1 5"
rm -f "$T/bin/timeout"

t_case "with TEHAS_ANALYTICS off, or not set at all, the program is not called"
rm -f "$T"/calls/*
t_stub_analytics
for T_ENV in TEHAS_ANALYTICS=off TEHAS_ANALYTICS=; do
  t_run "$call; tehas_analytics event 7 started; echo rc=\$?"
  t_eq "$t_out" "rc=0"
done
T_ENV=()
t_eq "$(t_calls analytics)" ""

t_case "a host without the program is silent"
rm -f "$bin" "$T"/calls/*
T_ENV=(TEHAS_ANALYTICS=on)
t_run "$call; tehas_analytics sample; echo rc=\$?"
t_eq "$t_out" "rc=0"
mkdir -p "$bin"
t_run "$call; tehas_analytics sample; echo rc=\$?"
t_eq "$t_out" "rc=0"
rmdir "$bin"

t_case "sourcing it defines and runs nothing else"
t_run '. "$T_ROOT/lib/analytics.sh"; declare -F | wc -l; ls "$T/calls" | wc -l'
t_eq "$t_out" "1
0"

t_done
