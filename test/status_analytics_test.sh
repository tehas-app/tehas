#!/usr/bin/env bash
# Tests for live/analytics.json, the ten-day report libexec/status writes from
# `analytics report --json` for the dashboard: its keys, and what happens when
# analytics is off, missing or failing.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

status="$T_ROOT/libexec/status"
live=$T/home/tehas/live
file=$live/analytics.json

t_stub gh '
[ ! -e "$T/github-down" ] || exit 1
case "$*" in
  "issue list"*) echo "[]" ;;
  "issue view"*) echo "" ;;
  "api repos/"*) echo "[]" ;;
  "api graphql"*) echo "{\"data\":{\"search\":{\"nodes\":[]}}}" ;;
esac'

# A report with every key the program prints, the optional planned and
# energy among them, and two it never prints: neither reaches the page.
cat >"$T/report.json" <<'EOF'
{"report":1,"from":1792022400,"to":1792843200,"shipped":{"n":3},
 "planned":{"n":6,"inLine":5,"planning":1,"trend":[]},
 "runs":{"n":6,"pr":1,"merged":1,"handoff":1,"parked":0,"failed":2,"refused":1,"stopped":0},
 "planningTurns":{"n":1},"handoffRate":{"n":6,"share":0.167},
 "firstPassYield":{"n":2,"share":0.5},"leadTime":{"n":2,"median":43400,"p90":83600},
 "cycleTime":{"n":6,"median":750,"p90":1800},"machineTime":{"n":6,"seconds":4800,"perShipped":1600},
 "tokens":{"n":4,"total":6650,"input":6100,"output":500,"cacheRead":50,"perShipped":2217},
 "availability":{"n":5,"seconds":734400,"share":0.002},"energy":{"n":4,"kwh":0.075,"perShipped":0.025},
 "daily":[],"stopReasons":{"n":0,"top":[]},"stations":{"n":0,"steps":[]},"versions":[],
 "refine":{"sessions":2,"n":1,"started":1,"shipped":0,"shippedShare":0.0,
  "leadTime":{"n":0},"others":{"n":3,"started":2,"shipped":1,"shippedShare":0.333}},
 "sizes":{"none":{"n":0,"shipped":0,"leadTime":{"n":0}},
  "1-2":{"n":2,"shipped":1,"leadTime":{"n":1,"median":7200}},
  "3-4":{"n":0,"shipped":0,"leadTime":{"n":0}},
  "5+":{"n":1,"shipped":0,"leadTime":{"n":0}}},
 "failureCause":"a cause that may not be public","issues":[12,13]}
EOF
keys="availability,cycleTime,daily,energy,firstPassYield,from,handoffRate,leadTime,machineTime,planned,planningTurns,refine,report,runs,shipped,stations,stopReasons,to,tokens,versions"

reset() {
  rm -rf "$T"/calls/* "$live" "$T/home/tehas/mcp" "$T/home/tehas/libexec" "$T/github-down"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  t_stub_analytics '[ "$1" != report ] || cat "$T/report.json"
    [ "$1" != sessions ] || cat "$T/sessions.jsonl"'
}
there() { [ -e "$file" ] && echo there || echo gone; }
snap=$T/home/tehas/mcp/sessions.jsonl
snap_there() { [ -e "$snap" ] && echo there || echo gone; }
snap_aged() { touch -d "@$(($(date +%s) - $1))" "$snap"; } # <seconds>
printf '%s\n%s\n' '{"api":1,"days":30,"generatedAt":"2026-10-21T00:00:00Z","sessions":1}' \
  '{"id":"a"}' >"$T/sessions.jsonl"
aged() { touch -d "@$(($(date +%s) - $1))" "$file"; } # <seconds>

t_case "the report is written with the keys that may be published and no other"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(t_calls analytics)" "report --json
sessions"
t_eq "$(jq -r 'keys | join(",")' "$file")" "$keys"
# The size figures belong to the terminal, not the page: the report has the
# key, and the publisher's allowlist drops it. The manual says so; this holds
# it true.
t_eq "$(jq -e 'has("sizes")' "$T/report.json")" "true"
t_eq "$(jq -e 'has("sizes")' "$file")" "false"
t_lacks "$file" "may not be public"
t_eq "$(jq -c .runs "$file")" "$(jq -c .runs "$T/report.json")"
t_eq "$(jq -c .refine "$file")" "$(jq -c .refine "$T/report.json")"
t_eq "$(wc -l <"$file")" 1
t_eq "$(find "$live" -name '*.new' | wc -l)" 0
# Readable by the docs server, as status.json is.
t_eq "$(stat -c %a "$file")" "$(stat -c %a "$live/status.json")"

t_case "the report goes through names of this run: a cycle's end and a poll may run status at once"
reset
t_stub_analytics '[ "$1" != report ] || { readlink /proc/$$/fd/1 >"$T/report-to"; cat "$T/report.json"; }'
mkdir -p "$live"
echo partial >"$file.1.raw.new"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r 'keys | join(",")' "$file")" "$keys"
to=$(cat "$T/report-to")
[[ $to =~ ^$live/analytics\.json\.[0-9]+\.raw\.new$ ]] || t_bad "the report went to $to"
t_eq "$(cat "$file.1.raw.new")" partial

t_case "the report is asked for after the run's events, so it holds them"
reset
t_run "$status"
echo '[{"number":5,"title":"t","url":"u","state":"OPEN","labels":[{"name":"tehas:awaiting-merge"}],"updatedAt":"2026-01-01T00:00:00Z","closedAt":null}]' >"$T/issues.json"
t_stub gh '
case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) echo "[]" ;;
  "issue view"*) echo "" ;;
  "api repos/"*) echo "[]" ;;
  "api graphql"*) echo "{\"data\":{\"search\":{\"nodes\":[]}}}" ;;
esac'
rm -f "$T/calls/analytics"
t_run "$status"
t_eq "$(t_calls analytics)" "event 5 awaiting
report --json
sessions"

t_case "a run that ends early because GitHub does not answer still writes the report"
reset
touch "$T/github-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(there)" there
t_eq "$(jq -r .report "$file")" 1

t_case "TEHAS_ANALYTICS=off removes the file and asks for no report"
reset
t_run "$status"
t_eq "$(there)" there
rm -f "$T/calls/analytics"
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(there)" gone
t_eq "$(t_calls analytics)" ""

t_case "a host without the program has no file"
reset
t_run "$status"
rm -f "$T/home/tehas/libexec/analytics"
t_run "$status"
t_eq "$t_out" ""
t_eq "$(there)" gone

t_case "a failed report keeps a file younger than ten minutes and removes an older one"
reset
t_run "$status"
keep=$(cat "$file")
t_stub_analytics '[ "$1" != report ] || { echo "{\"report\":1,\"half"; exit 70; }'
aged 540
t_run "$status"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN analytics report: exit 70"
t_eq "$(cat "$file")" "$keep"
t_eq "$(find "$live" -name '*.new' | wc -l)" 0
aged 600
t_run "$status"
t_eq "$(there)" gone

t_case "a report that is not one is a warning and is not published"
reset
t_run "$status"
for printed in 'not json' '{"report":2}' '[1,2]' ''; do
  printf '%s\n' "$printed" >"$T/report.json"
  aged 60
  t_run "$status"
  t_eq "$t_rc" 0
  t_contains "$t_out" "WARN analytics report: not a report"
  t_eq "$(jq -r 'keys | join(",")' "$file")" "$keys"
done
aged 601
t_run "$status"
t_eq "$(there)" gone

t_case "a report that hangs costs one time limit"
reset
t_stub_analytics '[ "$1" != report ] || exec sleep 30'
T_ENV=(TEHAS_ANALYTICS_WAIT=1)
before=$(date +%s)
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(($(date +%s) - before < 5))" 1
t_contains "$t_out" "WARN analytics report: exit 124"
t_eq "$(there)" gone

cat >"$T/report.json" <<'EOF2'
{"report":1,"from":1,"to":2}
EOF2

t_case "the sessions snapshot is private: mode 600 in a 700 directory, never under live/"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(cat "$snap")" "$(cat "$T/sessions.jsonl")"
t_eq "$(stat -c %a "$snap")" 600
t_eq "$(stat -c %a "${snap%/*}")" 700
t_eq "$(find "${snap%/*}" -name '*.new' | wc -l)" 0
t_eq "$(find "$live" -iname '*session*' | wc -l)" 0

t_case "the snapshot goes through names of this run"
reset
t_stub_analytics '[ "$1" != sessions ] || { readlink /proc/$$/fd/1 >"$T/sessions-to"; cat "$T/sessions.jsonl"; }'
t_run "$status"
to=$(cat "$T/sessions-to")
[[ $to =~ ^${snap%/*}/sessions\.jsonl\.[0-9]+\.raw\.new$ ]] || t_bad "the snapshot went to $to"

t_case "TEHAS_ANALYTICS=off removes the snapshot"
reset
t_run "$status"
t_eq "$(snap_there)" there
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_run "$status"
t_eq "$(snap_there)" gone

t_case "a failed sessions run keeps a snapshot younger than ten minutes and removes an older one"
reset
t_run "$status"
keep=$(cat "$snap")
t_stub_analytics '[ "$1" != sessions ] || { echo "{\"api\":1,\"half"; exit 70; }
  [ "$1" != report ] || cat "$T/report.json"'
snap_aged 540
t_run "$status"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN analytics sessions: exit 70"
t_eq "$(cat "$snap")" "$keep"
t_eq "$(find "${snap%/*}" -name '*.new' | wc -l)" 0
snap_aged 600
t_run "$status"
t_eq "$(snap_there)" gone
t_eq "$(there)" there

t_case "a snapshot without the header is a warning and is not published"
reset
t_run "$status"
for printed in 'not json' '{"api":2,"days":30}' '{"api":1,"days":"x"}' '[1]' ''; do
  printf '%s\n{"id":"b"}\n' "$printed" >"$T/sessions.jsonl"
  snap_aged 60
  t_run "$status"
  t_contains "$t_out" "WARN analytics sessions: not a snapshot"
  t_eq "$(head -n 1 "$snap" | jq -r .api)" 1
  t_eq "$(grep -c '"id":"b"' "$snap" || true)" 0
done

t_case "a failing sessions run does not stop analytics.json"
reset
t_stub_analytics '[ "$1" != sessions ] || exit 70
  [ "$1" != report ] || cat "$T/report.json"'
t_run "$status"
t_eq "$(jq -r .report "$file")" 1
t_eq "$(snap_there)" gone

t_done
