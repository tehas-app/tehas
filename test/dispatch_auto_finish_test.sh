#!/usr/bin/env bash
# Tests for tehasbot dispatch: the poll finishes a cycle whose push or pull
# request failed on GitHub's side, from the record the cycle left
# (test/cycle_end_failed_test.sh), with tehas-cycle finish stubbed. The rules
# of the tries are the Rust tests of console/dispatch/src/auto_finish.rs.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# The stand-in for tehas-cycle finish: the URL, or the one line of a refusal
# when $fx/finish-says holds it.
t_stub fake-finish 'if [ -s "$T/fx/finish-says" ]; then
  echo "tehas-cycle: finish: #$2: $(cat "$T/fx/finish-says")" >&2; exit 1; fi
echo "https://github.com/acme/widget/pull/90"'

record=$T/home/tehas/state/cycles/7.end-failed.json
# Issue 7 is parked, and its last cycle's <step> failed a minute ago with
# <why>, GitHub's fault or not.
stopped() { # <step> <github> <why>
  reset
  T_ENV+=(TEHAS_FINISH_CMD=fake-finish)
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1
  rm -f "$fx/finish-says"
  echo '[]' >"$fx/ready.json"
  echo '{"number":7,"state":"open","labels":[{"name":"needs-human-attention"}]}' >"$fx/work-7.json"
  mkdir -p "${record%/*}"
  jq -nc --arg step "$1" --argjson github "$2" --arg why "$3" --argjson at "$(($(date +%s) - 60))" \
    '{issue: 7, attempt: 1, step: $step, github: $github, why: $why, at: $at, tries: [], stopped: "", said: false}' >"$record"
}
# Eleven minutes pass: the failure and every try are moved back.
later() {
  jq -c '.at -= 660 | .tries |= map(. - 660)' "$record" >"$record.new" && mv "$record.new" "$record"
}
finishes() { t_calls fake-finish | grep -c '^finish 7 ' || true; }

t_case "a push that failed 502 once is finished on the next poll"
stopped push true "HTTP 502"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-finish)" "finish 7 --home $T/home/tehas --repo acme/widget --default main --reason The cycle stopped because its push failed on GitHub's side (HTTP 502). The poll finished it from the kept workspace on its own, try 1 of 3."
t_contains "$(t_calls gh)" "<!-- tehas:finish issue=7 result=opened by=poll -->"
t_contains "$(t_calls gh)" "https://github.com/acme/widget/pull/90"
t_contains "$t_out" "auto-finish #7: opened https://github.com/acme/widget/pull/90, try 1 of 3: its push failed on GitHub's side (HTTP 502)"
t_eq "$(ls "$record" 2>/dev/null)" ""
t_eq "$(t_calls fake-start)" ""

t_case "a pull request that fails 429 three times stops after 3 tries and is left for the ask"
stopped pr true "HTTP 429"
echo "could not open the draft: HTTP 429: Too Many Requests" >"$fx/finish-says"
t_run "$dispatch"
t_contains "$t_out" "auto-finish #7: try 1 of 3 failed on GitHub's side: could not open the draft: HTTP 429"
t_eq "$(jq '.tries | length' "$record")" 1
# Too soon for the second.
t_run "$dispatch"
t_eq "$(finishes)" 1
later
t_run "$dispatch"
t_eq "$(finishes)" 2
t_lacks "$(t_calls gh)" "result=stopped"
later
t_run "$dispatch"
t_eq "$(finishes)" 3
t_eq "$(jq -c '[(.tries | length), .said, (.stopped | startswith("3 tries failed"))]' "$record")" '[3,true,true]'
t_eq "$(t_calls gh | grep -c 'result=stopped by=poll')" 1
t_contains "$(t_calls gh)" "It waits for \`tehas:finish\`"
t_contains "$t_out" "auto-finish #7: stopped: 3 tries failed"
later
t_run "$dispatch"
t_eq "$(finishes)" 3
t_eq "$(t_calls gh | grep -c 'result=stopped by=poll')" 1

t_case "a 422 is not retried"
stopped pr false "HTTP 422"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-finish)" ""
t_lacks "$(t_calls gh)" "tehas:finish issue=7"
t_eq "$(jq -c '[.tries, .stopped]' "$record")" '[[],""]'

t_case "a finish already asked for is not run twice"
stopped push true "HTTP 502"
echo '{"number":7,"state":"open","labels":[{"name":"needs-human-attention"},{"name":"tehas:finish"}]}' >"$fx/work-7.json"
jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Words.", title: "Seven"}}}}' >"$fx/edit-7.json"
jq -n '[{event: "labeled", label: {name: "tehas:finish"}, actor: {login: "alice"}, created_at: "2026-10-02T00:00:00Z"}]' >"$fx/events-7.json"
jq -n '[{id: 71, user: {login: "alice"}, created_at: "2026-10-01T23:59:00Z", updated_at: "2026-10-01T23:59:00Z",
  body: "<!-- tehas:millwright issue=7 action=finish -->\nGitHub failed the push."}]' >"$fx/icomments-7.json"
past 7 alice label 2026-10-01T00:00:00Z
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(finishes)" 1
t_contains "$t_out" "finish #7: opened https://github.com/acme/widget/pull/90, asked by alice (label)"
t_lacks "$t_out" "auto-finish #7"
t_eq "$(ls "$record" 2>/dev/null)" ""

t_case "an issue a person took on again has its record removed and nothing run"
stopped push true "HTTP 502"
echo '{"number":7,"state":"closed","labels":[{"name":"needs-human-attention"}]}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-finish)" ""
t_contains "$t_out" "auto-finish #7: dropped: the issue is closed"
t_eq "$(ls "$record" 2>/dev/null)" ""

t_done
