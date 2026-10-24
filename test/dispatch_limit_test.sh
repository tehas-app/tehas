#!/usr/bin/env bash
# Tests for libexec/dispatch: the limit tehas limit writes.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# --- The limit (tehas limit) --------------------------------------------------
t_case "limit: cycles 0, 1 and above the slots, with 0, 1 and 2 running"
for c in 0 1 3; do
  for r in 0 1 2; do
    reset
    slots 2
    ready 7 8 9
    limit "$c"
    [ "$r" -lt 1 ] || hold 5
    [ "$r" -lt 2 ] || hold 6
    t_run "$dispatch"
    release
    cap=$((c < 2 ? c : 2))
    want=$((cap - r))
    [ "$want" -ge 0 ] || want=0
    got=$(t_calls fake-start | wc -l)
    [ "$got" = "$want" ] || t_bad "cycles $c with $r running started $got, not $want"
    [ "$t_rc" = 0 ] || t_bad "cycles $c with $r running exited $t_rc"
  done
done

t_case "limit: a limit that holds every slot says so, by the word limit"
reset
slots 2
limit 1 true "" "held for alpha"
hold 5
t_run "$dispatch"
release
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -r .why "$dispatch_json")" limit
t_eq "$(jq -r .hold "$dispatch_json")" "holding: the limit allows 1 cycle at once and 1 is running: held for alpha"
t_eq "$(jq -c '{free, limitRoom}' "$dispatch_json")" '{"free":1,"limitRoom":0}'
t_contains "$t_out" "holding: the limit allows 1 cycle at once"

t_case "limit: fewer start than the slots allow, and the limit is the reason"
reset
slots 3
ready 7 8 9
limit 1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -r .why "$dispatch_json")" limit
t_eq "$(jq -r .hold "$dispatch_json")" "holding: 1 of 3 free slots start: the limit allows 1 cycle at once"

t_case "limit: no limit leaves the room as it was, and writes no room under it"
reset
slots 2
ready 7 8
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_eq "$(jq -c 'has("limitRoom")' "$dispatch_json")" false

t_case "limit: expired, unreadable or of another version, nothing starts and the hold says why"
for kind in expired unreadable version; do
  reset
  slots 2
  ready 7
  case "$kind" in
    expired) limit 3 true 2020-01-01T00:00:00Z ;;
    unreadable) mkdir -p "${limit_file%/*}"; echo 'not a limit' >"$limit_file" ;;
    version) mkdir -p "${limit_file%/*}"; echo '{"v":2,"cycles":3,"plan":true,"until":null}' >"$limit_file" ;;
  esac
  t_run "$dispatch"
  [ "$t_rc" = 0 ] || t_bad "$kind: exit $t_rc"
  [ -z "$(t_calls fake-start)" ] || t_bad "$kind: a cycle started"
  [ "$(jq -r .why "$dispatch_json")" = limit ] || t_bad "$kind: why is $(jq -r .why "$dispatch_json")"
  case "$kind" in
    expired) t_contains "$(jq -r .hold "$dispatch_json")" "holding: the limit expired at 2020-01-01T00:00:00Z: nothing starts" ;;
    *) t_contains "$(jq -r .hold "$dispatch_json")" "holding: the limit cannot be read: nothing starts" ;;
  esac
done

t_case "limit: an expired limit still lets the poll clean up"
reset
limit 3 true 2020-01-01T00:00:00Z
echo '[{"number":9,"labels":[{"name":"tehas:running"}]}]' >"$fx/marked.json"
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget"

t_case "limit: a paused factory stays paused under any limit"
reset
slots 2
limit 3
mkdir -p "$T/home/tehas/state"
jq -nc '{by: "boss", at: "2026-10-15T10:00:00Z", reason: "weekend"}' >"$T/home/tehas/state/pause.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "limit: planning off holds a new planning session, not an answer"
to_plan
limit 3 false
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:planning"
planning
limit 3 false
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123 --answer $keep/answer.md"

t_case "limit: an expired limit holds an answer too, and a retry tick"
planning
limit 3 true 2020-01-01T00:00:00Z
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
parked
limit 3 true 2020-01-01T00:00:00Z
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "limit: cycles 0 holds a retry tick, planning on lets a session start"
parked
limit 0
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
to_plan
limit 0
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out"

t_done
