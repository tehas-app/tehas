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

t_case "a call to a file writes what the program prints there and returns 0"
rm -f "$T"/calls/*
t_stub_analytics 'echo "{\"report\":1}"'
T_ENV=(TEHAS_ANALYTICS=on)
t_run "$call; tehas_analytics_to \"\$T/out\" report --json; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(cat "$T/out")" '{"report":1}'
t_eq "$(t_calls analytics)" "report --json"

t_case "a call to a file that fails or hangs is one WARN line and returns 1"
t_stub_analytics 'echo partial; exit 70'
t_run "$call; tehas_analytics_to \"\$T/out\" report --json; echo rc=\$?"
t_contains "$t_out" "WARN analytics report: exit 70"
t_contains "$t_out" "rc=1"
t_stub_analytics 'exec sleep 30'
T_ENV=(TEHAS_ANALYTICS=on TEHAS_ANALYTICS_WAIT=1)
t_run "$call; tehas_analytics_to \"\$T/out\" report --json; echo rc=\$?"
t_contains "$t_out" "WARN analytics report: exit 124"
t_contains "$t_out" "rc=1"

t_case "a call to a file with analytics off or not installed returns 2 and writes nothing"
rm -f "$T/out" "$T"/calls/*
t_stub_analytics
T_ENV=(TEHAS_ANALYTICS=off)
t_run "$call; tehas_analytics_to \"\$T/out\" report --json; echo rc=\$?"
t_eq "$t_out" "rc=2"
rm -f "$bin"
T_ENV=(TEHAS_ANALYTICS=on)
t_run "$call; tehas_analytics_to \"\$T/out\" report --json; echo rc=\$?"
t_eq "$t_out" "rc=2"
t_eq "$(t_calls analytics)" ""
t_eq "$([ -e "$T/out" ] && echo there || echo none)" none
T_ENV=()

# A stub that keeps the facts it was given and how they were kept.
facts_stub() {
  t_stub_analytics 'while [ "$#" -gt 0 ] && [ "$1" != --facts ]; do shift; done
cp "$2" "$T/calls/facts.json"; stat -c %a "$2" >"$T/calls/facts.mode"; echo "$2" >"$T/calls/facts.path"'
}
cycles=$T/home/tehas/state/cycles
mkdir -p "$cycles"
now=$(date +%s)

t_case "a run's facts are the last line of its history, the count, the approval, the settings and the commit"
rm -f "$T"/calls/*
facts_stub
T_ENV=(TEHAS_ANALYTICS=on)
{
  jq -n -c --argjson s $((now - 900)) '{at: "2026-10-24T10:00:00Z", started: $s, rc: 10, status: "handoff"}'
  echo 'not json'
  jq -n -c --argjson s $((now - 600)) '{at: "2026-10-24T11:00:00Z", started: $s, rc: 0, status: "merged", how: "label", pr: "https://github.com/o/r/pull/3"}'
} >"$cycles/7.history.jsonl"
echo '{"at": "2026-10-24T10:59:00Z", "by": "alice", "how": "retry-tick", "links": ["x"]}' >"$cycles/7.approved.json"
echo '{"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "m"}, "host": {"TEHAS_WORK_TIMEOUT": "60"}, "concurrentCycles": 2, "other": 1}' >"$cycles/7.settings"
printf 'abcdef1\nmore\n' >"$cycles/7.rukas"
t_run "$call; tehas_analytics_run 7 --oom 0 --forks 1; echo rc=\$?"
t_eq "$t_out" "rc=0"
facts=$(cat "$T/calls/facts.path")
t_eq "$(t_calls analytics)" "run 7 --facts $facts --oom 0 --forks 1"
t_eq "$(jq -c . "$T/calls/facts.json")" "$(jq -n -c --argjson s $((now - 600)) '{line: {at: "2026-10-24T11:00:00Z", started: $s, rc: 0, status: "merged", how: "label", pr: "https://github.com/o/r/pull/3"},
  attempt: 2, approval: {at: "2026-10-24T10:59:00Z", how: "retry-tick"},
  settings: {container: {PI_ENSEMBLE_SUBAGENT_MODEL: "m"}, host: {TEHAS_WORK_TIMEOUT: "60"}, concurrentCycles: 2},
  rukas: "abcdef1"}')"
t_eq "$(cat "$T/calls/facts.mode")" 600
t_eq "$([ -e "$facts" ] && echo there || echo gone)" gone

t_case "a fact whose file is missing, a link, stale or not JSON is null"
rm -f "$T"/calls/*
rm -f "$cycles/7.rukas"
mv "$cycles/7.approved.json" "$T/approved.json"
ln -s "$T/approved.json" "$cycles/7.approved.json"
touch -d @$((now - 700)) "$cycles/7.settings"
t_run "$call; tehas_analytics_run 7; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(jq -c '[.attempt, .approval, .settings, .rukas, .line.status]' "$T/calls/facts.json")" '[2,null,null,null,"merged"]'
rm -f "$cycles/7.approved.json"
echo '{"at": 1' >"$cycles/7.approved.json"
echo '{"container": {}} {"container": {}}' >"$cycles/7.settings"
t_run "$call; tehas_analytics_run 7"
t_eq "$(jq -c '[.approval, .settings]' "$T/calls/facts.json")" '[null,null]'
mv "$cycles/7.history.jsonl" "$T/history.jsonl"
ln -s "$T/history.jsonl" "$cycles/7.history.jsonl"
t_run "$call; tehas_analytics_run 7"
t_eq "$(jq -c '[.line, .attempt]' "$T/calls/facts.json")" '[null,0]'
rm -f "$cycles/7.history.jsonl"
t_run "$call; tehas_analytics_run 7; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(jq -c '[.line, .attempt, .approval, .settings, .rukas]' "$T/calls/facts.json")" '[null,0,null,null,null]'
t_eq "$(find "$T/home/tehas/tmp" -type f | wc -l)" 0

t_case "a history line longer than a command's argument may be is given whole"
rm -f "$T"/calls/*
jq -n -c --argjson s "$now" '{started: $s, rc: 2, status: "failed", failure_cause: ("x" * 200000)}' >"$cycles/7.history.jsonl"
t_run "$call; tehas_analytics_run 7; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(jq -c '[(.line.failure_cause | length), .attempt]' "$T/calls/facts.json")" '[200000,1]'
rm -f "$cycles/7.history.jsonl"

t_case "a planning turn's facts are its record as it is"
rm -f "$T"/calls/*
echo '{"session": "s", "turns": 1, "running": true, "startedAt": "2026-10-24T11:00:00Z", "answers": []}' >"$T/home/tehas/state/plan-7.json"
t_run "$call; tehas_analytics_plan 7 --rc 0 --rukas-image abcdef1; echo rc=\$?"
t_eq "$t_out" "rc=0"
facts=$(cat "$T/calls/facts.path")
t_eq "$(t_calls analytics)" "plan 7 --facts $facts --rc 0 --rukas-image abcdef1"
t_eq "$(jq -c . "$T/calls/facts.json")" '{"session":"s","turns":1,"running":true,"startedAt":"2026-10-24T11:00:00Z","answers":[]}'
t_eq "$(cat "$T/calls/facts.mode")" 600
t_eq "$([ -e "$facts" ] && echo there || echo gone)" gone
mv "$T/home/tehas/state/plan-7.json" "$T/plan-7.json"
ln -s "$T/plan-7.json" "$T/home/tehas/state/plan-7.json"
t_run "$call; tehas_analytics_plan 7 --rc 0"
t_eq "$(cat "$T/calls/facts.json")" null
rm -f "$T/home/tehas/state/plan-7.json"
t_run "$call; tehas_analytics_plan 7 --rc 0"
t_eq "$(cat "$T/calls/facts.json")" null

t_case "facts that cannot be written are one WARN line, and the program is not called"
rm -f "$T"/calls/*
rm -rf "$T/home/tehas/tmp"
: >"$T/home/tehas/tmp"
t_run "$call; tehas_analytics_run 7; tehas_analytics_plan 7 --rc 0; echo rc=\$?"
t_contains "$t_out" "WARN analytics run: no facts written"
t_contains "$t_out" "WARN analytics plan: no facts written"
t_contains "$t_out" "rc=0"
t_eq "$(t_calls analytics)" ""
rm -f "$T/home/tehas/tmp"

t_case "with TEHAS_ANALYTICS off, or no program, no facts are made and nothing is called"
rm -rf "$T"/calls/* "$T/home/tehas/tmp"
facts_stub
T_ENV=(TEHAS_ANALYTICS=off)
t_run "$call; tehas_analytics_run 7; tehas_analytics_plan 7 --rc 0; echo rc=\$?"
t_eq "$t_out" "rc=0"
rm -f "$bin"
T_ENV=(TEHAS_ANALYTICS=on)
t_run "$call; tehas_analytics_run 7; tehas_analytics_plan 7 --rc 0; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(t_calls analytics)" ""
t_eq "$([ -e "$T/home/tehas/tmp" ] && echo made || echo none)" none
T_ENV=()

t_case "sourcing it defines and runs nothing else"
rm -f "$T"/calls/*
t_run '. "$T_ROOT/lib/analytics.sh"; declare -F | wc -l; ls "$T/calls" | wc -l'
t_eq "$t_out" "8
0"

t_done
