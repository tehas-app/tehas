#!/usr/bin/env bash
# Tests for libexec/dispatch: the control issue is never work.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

control_queue() { # label...: the control issue in the queue, ready, carrying these labels
  local l list='{"name":"tehas:ready"}'
  for l in "$@"; do list="$list,{\"name\":\"$l\"}"; done
  echo '[{"number":1,"labels":['"$list"']}]' >"$fx/ready.json"
}

control_parked() {
  echo '[]' >"$fx/ready.json"
  echo '[{"number":1,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
  mkdir -p "$T/home/tehas/state"
  echo 4001 >"$T/home/tehas/state/park-1.id"
  comments 1 "$(printf 'Tehas cycle stopped.\n\n- [%s] Retry from a clean start\n- [%s] Leave it parked\n\nTick one box.\n\n<!-- tehas:question kind=park issue=1 turn=1 -->\n' x ' ')" alice
}

ready_at() { # <issue> <login> <time>: tehas:ready applied by <login> at <time>
  jq -nc --arg w "$2" --arg at "$3" '[{event: "labeled", label: {name: "tehas:ready"}, actor: {login: $w}, created_at: $at}]' >"$fx/events-$1.json"
}

t_case "the control issue labelled tehas:ready starts no cycle or changes"
reset
control_queue tehas:ready
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" 'skip #1: the control issue is never worked on (`tehas:ready`)'

t_case "a dry run says the control issue is not worked and changes nothing"
reset
control_queue tehas:ready
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" 'skip #1: the control issue is never worked on (`tehas:ready`)'
t_lacks "$(t_calls gh)" "issue edit"

t_case "a stranger's tehas:ready on the control issue is left alone"
reset
control_queue tehas:ready
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-1.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$t_out" "not an allowed account"
t_contains "$t_out" 'skip #1: the control issue is never worked on (`tehas:ready`)'

t_case "the ready control issue is not counted as demand"
reset
ready 1 7
ready_at 1 alice 2026-10-17T09:00:00Z
ready_at 7 alice 2026-10-17T10:00:00Z
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{waiting, oldest}' "$demand_file")" '{"waiting":1,"oldest":"2026-10-17T10:00:00Z"}'

t_case "the ready control issue takes no slot from a real issue"
reset
slots 2
ready 7 8 1
t_run "$dispatch --dry-run"
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/.*would run a cycle for #\([0-9]*\).*/\1/p' | tr '\n' ' ')" "7 8 "
t_contains "$t_out" 'skip #1: the control issue is never worked on (`tehas:ready`)'

t_case "tehas:plan on the control issue starts no planning turn"
reset
echo '[]' >"$fx/ready.json"
echo '[{"number":1,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" 'skip #1: the control issue is never worked on (`tehas:plan`)'

t_case "a plan on the control issue does not read its text"
reset
echo '[]' >"$fx/ready.json"
echo '[{"number":1,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
rm -f "$fx/issue-1.json"
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -e '^issue view 1 --repo acme/widget --json title,body')" "0"
t_contains "$t_out" 'skip #1: the control issue is never worked on (`tehas:plan`)'

t_case "a tick on the control issue's parked question starts nothing and stays ticked"
reset
control_parked
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_contains "$t_out" 'skip #1: the control issue is never worked on (its question was answered)'

t_case "an outage retry does not start on the control issue"
reset
echo '[]' >"$fx/ready.json"
mkdir -p "$T/home/tehas/state/cycles"
jq -cn '{retry: true, started: 1767225600}' >"$T/home/tehas/state/cycles/1.park"
echo alice >"$T/home/tehas/state/cycles/1.by"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_contains "$t_out" 'skip #1: the control issue is never worked on (a cycle its provider outage parked asked to start again)'

t_case "the control issue gets at most one skip line per poll"
reset
control_queue tehas:ready
control_parked
echo '[{"number":1,"labels":[{"name":"tehas:ready"},{"name":"tehas:plan"}]}]' >"$fx/plans.json"
slots 2
t_run "$dispatch --dry-run"
t_eq "$(grep -c 'the control issue is never worked on' <<<"$t_out")" 1

t_done
