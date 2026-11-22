#!/usr/bin/env bash
# Tests for tehas answer: the answer a program gives for a person to the
# question on record of an issue, recorded on the state store, and what it
# refuses (#2782). It reads nothing of GitHub. What the poll does with the
# answer is the dispatcher's, and its tests hold it
# (console/dispatch/tests/dispatch_answer.rs).
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")
answer_cmd="$T_ROOT/bin/tehas answer"
th=$T/home/tehas
ISO='[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z'

reset() {
  rm -rf "$th" "$T"/calls/*
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  t_store_home "$th"
}

# #12 stopped, its question 4001 on record.
stopped() {
  t_ended "$th" 12 alice
  t_park "$th" 12 failed 4001
}

# #12 in planning, its question 4002 waiting with two options.
planning() {
  t_plan "$th" 12 '{"turns": 1, "comment": 4002, "options": ["Adopt all", "Update this issue"]}'
}

waiting() { t_row question-known 12 --id "$1" -- '.answer | if . == null then "none" else "\(.choice) \(.option) \(.by)" end'; }
matches() { [[ $1 =~ ^$2$ ]] || t_bad "'$1' does not match $2"; }

t_case "a retry of a stop is recorded for the account, and nothing starts"
reset
stopped
t_run "$answer_cmd 12 retry --by alice"
t_eq "$t_rc" 0
matches "$t_out" "answered the park question of #12 for alice at $ISO: retry; the next poll acts on it"
t_eq "$(waiting 4001)" "retry null alice"
t_eq "$(t_calls gh)" ""

t_case "--json answers with what was recorded, and a second answer takes the first one's place"
reset
stopped
t_run "$answer_cmd 12 park --by alice"
t_run "$answer_cmd 12 retry-text --by alice --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.api, .issue, .question.id, .question.kind, .answer.choice, .answer.by, .answer.how, .replaced]' <<<"$t_out")" \
  '[1,12,4001,"park","retry-text","alice","answer",true]'
matches "$(jq -r '.answer.at' <<<"$t_out")" "$ISO"
t_eq "$(waiting 4001)" "retry-text null alice"

t_case "a planning question takes an option, update, end or words"
reset
planning
t_run "$answer_cmd 12 option:1 --by alice"
t_eq "$t_rc" 0
t_contains "$t_out" ": option 1;"
t_eq "$(waiting 4002)" "option 1 alice"
t_run "$answer_cmd 12 update --by alice"
t_eq "$(waiting 4002)" "update 2 alice"
t_run "$answer_cmd 12 end --by alice"
t_eq "$(waiting 4002)" "end null alice"
printf 'Split it in two.\n' >"$T/reply.md"
t_run "cd '$T' && $answer_cmd 12 --text reply.md --by alice"
t_eq "$t_rc" 0
t_eq "$(t_row question-known 12 --id 4002 -- '.answer.text')" "Split it in two."

t_case "an account that may not authorise work is refused with 78, and nothing is recorded"
reset
stopped
t_run "$answer_cmd 12 retry --by mallory"
t_eq "$t_rc" 78
t_contains "$t_out" "mallory may not authorise work for this factory"
t_eq "$(waiting 4001)" none

t_case "no question of that kind waiting, or a choice not offered, is 3"
reset
t_run "$answer_cmd 12 retry --by alice"
t_eq "$t_rc" 3
t_contains "$t_out" "no stop waits for an answer"
stopped
t_run "$answer_cmd 12 end --by alice"
t_eq "$t_rc" 3
t_contains "$t_out" "no planning question waits"
t_eq "$(waiting 4001)" none
reset
planning
t_run "$answer_cmd 12 option:3 --by alice"
t_eq "$t_rc" 3
t_contains "$t_out" "the question offers 2 options, not 3"
t_eq "$(waiting 4002)" none

t_case "a bad argument writes nothing and exits 64"
reset
stopped
for args in "" "12" "12 retry" "0 retry --by alice" "12 tick --by alice" "12 option:0 --by alice" \
  "12 retry --text f --by alice" "12 retry --by 'al ice'" "12 retry end --by alice"; do
  t_run "$answer_cmd $args"
  t_eq "$t_rc" 64
done
t_eq "$(waiting 4001)" none

t_done
