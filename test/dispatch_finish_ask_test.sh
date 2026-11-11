#!/usr/bin/env bash
# Tests for tehasbot dispatch: the finish step, an ask on the issue with a
# marked comment and tehas:finish, run by a real poll with tehas-cycle stubbed.
# The rules of each ask are the Rust tests of console/dispatch/src/finish.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# The stand-in for tehas-cycle finish: its words recorded, then the URL, or
# the one line of a refusal when $fx/finish-refuses holds it.
t_stub fake-finish 'if [ -s "$T/fx/finish-refuses" ]; then
  echo "tehas-cycle: finish: #$2: $(cat "$T/fx/finish-refuses")" >&2; exit 1; fi
echo "https://github.com/acme/widget/pull/90"'

# Issue 7 carries tehas:finish from <who>, and <who> wrote the marked comment
# a minute before. Its body is "Words.", and alice approved a cycle of it.
asked() { # <who>
  reset
  T_ENV+=(TEHAS_FINISH_CMD=fake-finish)
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1
  rm -f "$fx/finish-refuses"
  echo '[]' >"$fx/ready.json"
  echo '{"number":7,"state":"open","labels":[{"name":"tehas:finish"}]}' >"$fx/work-7.json"
  jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Words.", title: "Seven"}}}}' >"$fx/edit-7.json"
  jq -n --arg who "$1" '[{event: "labeled", label: {name: "tehas:finish"}, actor: {login: $who}, created_at: "2026-10-02T00:00:00Z"}]' >"$fx/events-7.json"
  jq -n --arg who "$1" '[{id: 71, user: {login: $who}, created_at: "2026-10-01T23:59:00Z", updated_at: "2026-10-01T23:59:00Z",
    body: "<!-- tehas:millwright issue=7 action=finish -->\nThe flaky test in other.rs failed, not the change."}]' >"$fx/icomments-7.json"
  past 7 alice label 2026-10-01T00:00:00Z
}

t_case "an allowed ask runs the finish, takes the label off and comments with the URL"
asked alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-finish)" "finish 7 --home $T/home/tehas --repo acme/widget --default main --reason The flaky test in other.rs failed, not the change."
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:finish"
t_contains "$(t_calls gh)" "<!-- tehas:finish issue=7 result=opened -->"
t_contains "$(t_calls gh)" "https://github.com/acme/widget/pull/90"
t_contains "$t_out" "finish #7: opened https://github.com/acme/widget/pull/90, asked by alice (label): The flaky test in other.rs failed, not the change."
t_eq "$(t_calls fake-start)" ""

t_case "the millwright asks within its grant"
asked tehas-bot
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-finish)" "finish 7 "
t_contains "$t_out" "asked by tehas-bot (millwright)"

t_case "a refusal of tehas-cycle finish takes the label off and comments with its line"
asked alice
echo "no workspace is kept for it" >"$fx/finish-refuses"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "--remove-label tehas:finish"
t_contains "$(t_calls gh)" "<!-- tehas:finish issue=7 result=refused -->"
t_contains "$(t_calls gh)" "finished nothing: no workspace is kept for it."
t_contains "$t_out" "finish #7: refused, asked by alice (label): no workspace is kept for it"

t_case "a rule not met takes the label off and runs nothing"
asked tehas-bot
jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Other words.", title: "Seven"}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-finish)" ""
t_contains "$(t_calls gh)" "--remove-label tehas:finish"
t_contains "$(t_calls gh)" "the millwright may not ask it: its text is not the text alice approved"

t_case "an ask from an account that is not allowed is ignored"
asked mallory
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-finish)" ""
t_lacks "$(t_calls gh)" "tehas:finish"
t_contains "$t_out" "ignore #7: tehas:finish was applied by 'mallory', not an allowed account"

t_case "a paused factory runs no finish"
asked alice
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(t_calls fake-finish)" ""
t_lacks "$(t_calls gh)" "tehas:finish"

t_case "a dry run runs no finish"
asked alice
t_run "$dispatch --dry-run"
t_eq "$(t_calls fake-finish)" ""
t_lacks "$(t_calls gh)" "--remove-label tehas:finish"

t_done
