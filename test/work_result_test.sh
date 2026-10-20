#!/usr/bin/env bash
# Tests for libexec/work: how the runner's end is reported, from its exit
# status, its result and its progress.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1

t_case "the runner's exit status is the command's when it wrote its result"
for code in 0 10 11 20 21 30 40; do
  fresh
  container "says handoff; exit $code"
  t_run "$work 7"
  t_eq "$t_rc" "$code"
  t_contains "$t_out" "status=handoff step=develop cap=- runner_exit=$code"
done

t_case "a runner that ends without a result is exit 2, whatever its status"
fresh
container 'exit 10'
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "status=running"

t_case "a result without an exit code of its own is no success: exit 2"
for code in 1 137; do
  fresh
  container "says merged | jq -c 'del(.exitCode, .issues[0].exitCode)'; exit $code"
  t_run "$work 7"
  t_eq "$t_rc" 2
done

t_case "a result with no issue entry keeps the runner's own exit: a refusal is 40, and is no kill"
for code in 30 40; do
  fresh
  container "echo '{\"schemaVersion\":5,\"exitCode\":$code,\"issues\":[],\"error\":{\"code\":\"refused\"}}'; exit $code"
  t_run "$work 7"
  t_eq "$t_rc" "$code"
done
fresh
container "echo '{\"schemaVersion\":5,\"issues\":[]}'; exit 137"
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "was killed"

t_case "a result of another schema, or for another issue, is no result"
fresh
container 'says merged | sed "s/\"schemaVersion\":5/\"schemaVersion\":3/"; exit 0'
t_run "$work 7"
t_eq "$t_rc" 2
fresh
container 'says merged | sed "s/\"issue\":7/\"issue\":8/"; exit 0'
t_run "$work 7"
t_eq "$t_rc" 2

t_case "the result is kept for tehasbot cycle and beside the log, and the cap is said"
fresh
container 'says handoff verify-failed:develop; exit 10'
t_run "$work 7"
t_eq "$t_rc" 10
t_eq "$(jq -r '.issues[0].cap' "$result_file")" verify-failed:develop
cycle_log=$(printf '%s\n' "$t_out" | sed -n 's/^tehas work: issue 7 started, log //p')
t_eq "$(jq -r '.issues[0].cap' "$cycle_log.result.json")" verify-failed:develop
t_contains "$t_out" "status=handoff step=develop cap=verify-failed:develop runner_exit=10"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"handoff"]'

t_case "a cycle that ended at a cap with nothing in its error stream leaves a line naming the cap there"
fresh
container 'says handoff verify-failed:develop; exit 10'
t_run "$work 7"
t_eq "$t_rc" 10
cycle_log=$(printf '%s\n' "$t_out" | sed -n 's/^tehas work: issue 7 started, log //p')
t_eq "$(cat "$cycle_log.err")" "tehas work: issue 7 ended at the cap verify-failed:develop and $(basename "$cycle_log").err holds nothing: the driver wrote no diagnostic for the refusal, so the cap is all the record says about why the step was refused"

t_case "a cycle that ended with no cap leaves its error stream as it is"
fresh
# A log is named by the second it started in: the case above may have left one
# of the same name.
rm -f "$T"/home/tehas/logs/work-7-*
container 'says handoff; exit 10'
t_run "$work 7"
t_eq "$t_rc" 10
cycle_log=$(printf '%s\n' "$t_out" | sed -n 's/^tehas work: issue 7 started, log //p')
t_eq "$(find "$cycle_log.err" 2>/dev/null | wc -l)" 0

t_case "a result left by an earlier cycle is gone before the container starts"
fresh
echo '{"schemaVersion":5,"issues":[{"issue":7,"status":"merged"}]}' >"$result_file"
container 'exit 0'
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cat "$result_file")" ""

t_case "the progress gives the usage per role of the stable dispatch lines, each key once, and the last cap's reason"
fresh
done_line() { printf '{"v":1,"kind":"dispatch-completed","key":"%s","event":{"role":"%s","ok":true,"usage":{"turns":2,"input":10,"output":4,"cacheRead":1}}}' "$1" "$2"; }
container "progress '$(done_line 7:1:1 developer)'; progress '$(done_line 7:1:1 developer)'; progress '$(done_line 7:2:1 developer)'
progress '$(done_line 7:3:1 'ops x;')'
progress '{\"v\":1,\"kind\":\"dispatch-completed\",\"unstable\":true,\"key\":\"7:4:1\",\"event\":{\"role\":\"liar\",\"usage\":{\"turns\":99}}}'
progress 'not json'
progress '{\"v\":1,\"kind\":\"dispatch-failed-provider\",\"unstable\":true,\"event\":{\"providerMessage\":\"502 Bad Gateway\"}}'
says handoff step-failed:develop; exit 10"
t_run "$work 7"
t_eq "$(jq -c '.roles' "$progress_file")" '[{"role":"developer","dispatches":2,"turns":4,"input":20,"output":8,"cacheRead":2},{"role":"opsx","dispatches":1,"turns":2,"input":10,"output":4,"cacheRead":1}]'
t_eq "$(jq -r '.reason' "$progress_file")" "502 Bad Gateway"

t_case "where the image says its result carries them, the usage per role and the failure come from the result"
fresh
jq '.features += {failureReason: true, usageByRole: true}' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
container "progress '$(done_line 7:1:1 liar)'
progress '{\"v\":1,\"kind\":\"dispatch-failed-provider\",\"unstable\":true,\"event\":{\"providerMessage\":\"from the progress\"}}'
echo '{\"schemaVersion\":5,\"buildCommit\":null,\"exitCode\":10,\"issues\":[{\"issue\":7,\"status\":\"handoff\",\"cap\":\"step-failed:develop\",\"failure\":{\"kind\":\"provider\",\"message\":\"m\",\"providerMessage\":\"503 from the result\"},\"usage\":{\"dispatches\":1,\"byRole\":{\"developer\":{\"input\":3,\"output\":1,\"cacheRead\":0,\"turns\":2,\"dispatches\":1}}},\"exitCode\":10}]}'
exit 10"
t_run "$work 7"
t_eq "$t_rc" 10
t_eq "$(jq -c '.roles' "$progress_file")" '[{"role":"developer","dispatches":1,"turns":2,"input":3,"output":1,"cacheRead":0}]'
t_eq "$(jq -c '[.reason, .failure.kind, .from]' "$progress_file")" '["503 from the result","provider",{"roles":"result","failure":"result"}]'
caps_good

t_case "a container that exits 137 with no result is reported as killed and the command exits 137"
fresh
container 'exit 137'
t_run "$work 7"
t_eq "$t_rc" 137
t_contains "$t_out" "tehas work: the container was killed (137) while the cycle was running: out of memory, or stopped from outside"
t_contains "$t_out" "status=killed"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"killed"]'
t_lacks "$(t_calls docker)" "stop "

t_case "a 137 after the runner wrote its result is the result's end, by the result's exit code"
fresh
container 'says handoff; exit 137'
t_run "$work 7"
t_eq "$t_rc" 10
t_contains "$t_out" "status=handoff"
t_lacks "$t_out" "was killed"

t_case "a result that says killed is the runner's word, not a kill: no kill line"
fresh
container 'says killed; exit 30'
t_run "$work 7"
t_eq "$t_rc" 30
t_contains "$t_out" "status=killed"
t_lacks "$t_out" "was killed"

t_case "the container's input is nothing: no fifo, no terminal"
fresh
container 'ls -l /proc/$$/fd/ >"$T/calls/container.fds" 2>&1; says handoff; exit 10'
t_run "$work 7"
t_eq "$t_rc" 10
t_contains "$(grep ' 0 -> ' "$T/calls/container.fds")" "/dev/null"

t_case "the runner is given the time limit in whole minutes, at least one"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=61
t_run "$work 7"
t_contains "$(cycle_args | tr '\n' ' ')" "work 7 --timeout 2 --json"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=1
fresh
t_run "$work 7"
t_contains "$(cycle_args | tr '\n' ' ')" "work 7 --timeout 1 --json"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "a restart is passed to the runner as its flag"
fresh
t_run "$work 7 --restart"
t_contains "$(cycle_args | tr '\n' ' ')" "work 7 --timeout 360 --json --restart"

t_done
