#!/usr/bin/env bash
# Tests for libexec/cycle: how it calls the analytics program, and when it stays
# silent about it.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1
# The analytics program, where install.sh puts it. Each call notes how many
# history lines the issue had and whether the status was refreshed yet, and a
# run keeps its facts.
analytics_stub() { # [<bash body>]
  t_stub_analytics 'printf "%s %s\n" "$("$T/store-bin" history-show 7 | jq ".rows | length")" \
  "$([ -e "$T/calls/fake-status" ] && echo late || echo early)" >>"$T/calls/analytics.moment"
[ "$1 $3" != "run --facts" ] || cp "$4" "$T/calls/analytics.facts"
'"${1:-}"
}
# The calls, without the facts file's name, which is new each time.
analytics_calls() { t_calls analytics | sed 's/ --facts [^ ]*//'; }

t_case "the run in analytics carries when its image was built, as docker says of the id on record"
reset
analytics_stub
image_id=sha256:$(printf 'e%.0s' {1..64})
echo 2026-10-29T14:44:16.123456789Z >"$fx/image.label"
echo '"$T/store-bin" cycle-image 7 --attempt "$TEHAS_CYCLE_ATTEMPT" --image '"$image_id"' >/dev/null; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(analytics_calls | head -n 1)" "run 7 --image $image_id --image-built 2026-10-29T14:44:16.123456789Z --oom 0 --forks 0"
t_contains "$(t_calls docker)" "image inspect --format {{.Created}} $image_id"
rm -f "$fx/image.label"

t_case "the cycle records its run and its end in analytics after the history line, with what it read and removed"
reset
analytics_stub
image_id=sha256:$(printf 'e%.0s' {1..64})
echo '"$T/store-bin" cycle-image 7 --attempt "$TEHAS_CYCLE_ATTEMPT" --image '"$image_id"' --rukas 0123456789abcdef0123456789abcdef01234567 >/dev/null
echo 3 >"$TEHAS_HOME/state/cycles/7.oom"; echo 2 >"$TEHAS_HOME/state/cycles/7.forks"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(analytics_calls)" "run 7 --image $image_id --rukas-image 0123456789abcdef0123456789abcdef01234567 --oom 3 --forks 2
event 7 ended"
t_eq "$(cat "$T/calls/analytics.moment")" "1 early
1 early"
t_eq "$(jq -c '[.line.rc, .line.status, .line.how, .line.parked, .attempt, .rukas]' "$T/calls/analytics.facts")" \
  '[2,"unknown","hand",true,1,"0123456789abcdef0123456789abcdef01234567"]'
t_eq "$(t_row history-show 7 -- '.rows[0] | [.rc, .status, .how, .parked] | tostring')" '[2,"unknown","hand",true]'
t_lacks "$t_out" "analytics"

t_case "an option with no value is left out of the run"
reset
analytics_stub
ended handoff commit-pr feature/x
t_run "$cycle 7"
t_eq "$(analytics_calls)" "run 7 --oom 0 --forks 0
event 7 ended"

t_case "a cycle stopped by a signal is recorded too"
reset
analytics_stub
echo 'kill -TERM "$PPID"; sleep 1; exit 143' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(analytics_calls)" "run 7 --oom 0 --forks 0
event 7 ended"

t_case "a start that never ran gives analytics the approval it used, though the record goes with the report"
reset
T_ENV+=(TEHAS_CYCLE_FROM=unit)
analytics_stub
t_approve "$T/home/tehas" 7 alice label "$(date -u -d 2026-01-01T00:00:00Z +%s)"
echo 'exit 4' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.approval.at, .approval.how]' "$T/calls/analytics.facts")" '["2026-01-01T00:00:00Z","label"]'
t_eq "$(t_row approval-current 7 -- '.current | tostring')" null

t_case "with TEHAS_ANALYTICS off the cycle calls no analytics"
reset
analytics_stub
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" ""

t_case "a failing analytics is a warning, and the cycle is reported as without it"
reset
ended handoff commit-pr feature/x
t_run "$cycle 7"
without=$(t_calls gh)
reset
ended handoff commit-pr feature/x
analytics_stub 'exit 70'
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN analytics run: exit 70"
t_contains "$t_out" "WARN analytics event: exit 70"
t_contains "$t_out" "done #7 rc=10 status=handoff"
t_eq "$(t_calls gh)" "$without"
t_eq "$(t_calls fake-status | wc -l)" 1
t_eq "$(t_row history-show 7 -- '.rows | length')" 1

t_case "an analytics that hangs is cut off, and the cycle is reported"
reset
analytics_stub 'exec sleep 30'
T_ENV+=(TEHAS_ANALYTICS_WAIT=1)
before=$(date +%s)
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(($(date +%s) - before < 15))" 1
# GNU timeout reports the TERM as 124; uutils reports the kill that follows
# as 137.
for sub in run event; do
  [[ $t_out =~ "WARN analytics $sub: exit "(124|137) ]] || t_bad "no time limit warning for $sub in: $t_out"
done
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "a host without the analytics program is silent"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_lacks "$t_out" "analytics"

t_done
