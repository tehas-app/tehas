#!/usr/bin/env bash
# Tests for what outsource adds to the factory's state: tehasbot status writes
# live/contractors.json with the feature on and takes it away with it off, and
# tehas status and its --json show the contractors only while the file is
# there. A test build turns the feature on (TEHAS_TEST_FEATURES).
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/status_lib.sh
. "$(dirname "$0")/status_lib.sh" || exit 1
home=$T/home/tehas
store() { TEHAS_HOME=$home "$T_STORE" "$@"; }

t_case "off: status writes no contractors file, and takes a stray one away"
reset
t_run "$status"
t_eq "$t_rc" 0
t_eq "$([ -e "$live/contractors.json" ] && echo there)" ""
echo '{"contractors":[]}' >"$live/contractors.json"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$([ -e "$live/contractors.json" ] && echo there)" ""
t_run "$summary"
t_eq "$(grep -c '^outsource:' <<<"$t_out")" 0
t_run "$summary --json"
t_eq "$(jq 'has("contractors")' <<<"$t_out")" false

t_case "on in a test build: status writes the contractors, with no key"
reset
rm -f "$home"/state/tehas.db*
store init --outsource --version v0.30.0 --commit cb6912a --instance example --repo acme/widget >/dev/null ||
  t_bad "the store did not take the outsource tables"
store contractor-add --name box1 --key "$(printf '%064d' 7)" >/dev/null
store contractor-add --name box2 --key "$(printf '%064d' 8)" >/dev/null
store contractor-seen --name box1 >/dev/null
store contractor-seen --name box2 >/dev/null
cycle true running develop
one 13 true running review
store job-offer --issue 12 --contractor box1 >/dev/null || t_bad "no job for #12"
t_instance "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=on
T_ENV=(TEHAS_TEST_FEATURES=outsource)
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.api, (.contractors | map([.name, .status, .issue]))]' "$live/contractors.json")" \
  '[1,[["box1","working",12],["box2","idle",null]]]'
t_eq "$(jq -c '.counts | [.working, .idle, .outsourced]' "$live/contractors.json")" '[1,1,1]'
t_eq "$(grep -c "$(printf '%064d' 7)" "$live/contractors.json")" 0
t_eq "$(jq -r '.contractors[0].lastSeenAt | test("^[0-9-]{10}T[0-9:]{8}Z$")' "$live/contractors.json")" true

t_case "on: tehas status says where each running cycle is built and prints the outsource line"
t_run "$summary"
t_contains "$t_out" "cycle     #12 running, step develop"
t_eq "$(grep '^cycle     #12' <<<"$t_out" | grep -c ', outsourced to box1$')" 1
t_eq "$(grep '^cycle     #13' <<<"$t_out" | grep -c ', in-house$')" 1
t_contains "$t_out" "outsource: 1 idle, 1 working, 0 resting; 1 cycle(s) outsourced"

t_case "on: tehas status --json has contractors, and running still holds the outsourced cycle"
t_run "$summary --json"
t_eq "$(jq -c '.contractors | [.working, .idle, .outsourced, (.list | map(.name))]' <<<"$t_out")" '[1,1,1,["box1","box2"]]'
t_eq "$(jq -c '[.running[].issue] | sort' <<<"$t_out")" '[12,13]'
t_eq "$(jq -r --slurpfile schema "$T_ROOT/api/status.schema.json" -f "$T_ROOT/test/schema.jq" <<<"$t_out")" ""

t_case "switched off again: the next status takes the file away and the readers say nothing of it"
t_instance "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=off
t_run "$status"
t_eq "$t_rc" 0
t_eq "$([ -e "$live/contractors.json" ] && echo there)" ""
t_run "$summary"
t_eq "$(grep -c 'outsource\|in-house' <<<"$t_out")" 0

t_done
