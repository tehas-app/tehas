#!/usr/bin/env bash
# Tests for libexec/work: a cycle stopped at its time limit, as stalled or
# by TERM, and the bounds on the docker stop, the console and the push check.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1

t_case "a runner that does not stop at its own limit is stopped after the grace, and not reported as killed"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=1
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
container "progress '{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"lens-review\"}}'; $hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 1s passed"
t_contains "$t_out" "status=timeout"
t_contains "$t_out" "step=lens-review cap=wall-clock"
t_lacks "$t_out" "was killed"
t_contains "$(t_calls docker)" "stop -t 20 tehas-work-7"
t_eq "$(cat "$T/home/tehas/state/cycles/7.timed")" "1 tehas lens-review"

t_case "a runner stopped at the time limit that writes interrupted is a timeout, exit 2"
fresh
# Its start on record, so its heartbeat is published.
under_cycle
container "trap 'says interrupted; exit 21' TERM; \"\$nap\" 60 & echo \$! >\"\$T/sleeper.pid\"; wait; wait"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "status=timeout"
t_contains "$t_out" "cap=wall-clock"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"timeout"]'
t_eq "$(cat "$T/home/tehas/state/cycles/7.timed")" "1 tehas -"

# Grace enough that the runner ends before tehas would stop it.
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=30)
t_case "a runner that stops itself at the time limit with a result that names no end is a timeout, its own exit kept"
fresh
container '"$nap" 1.2; says running; exit 30'
t_run "$work 7"
t_eq "$t_rc" 30
t_contains "$t_out" "status=timeout"
t_lacks "$t_out" "time limit of 1s passed"
t_eq "$(cat "$T/home/tehas/state/cycles/7.timed")" "1 runner develop"

t_case "a runner that stops itself at the time limit with no result is a timeout at the last step it started"
fresh
container "progress '{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"adversarial\"}}'; \"\$nap\" 1.2; exit 30"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "status=timeout"
t_eq "$(cat "$T/home/tehas/state/cycles/7.timed")" "1 runner adversarial"

t_case "a runner that ends at the time limit on a cap of its own, or with an end, is not a timeout"
for end in 'says handoff deadline; exit 10' 'says merged; exit 0'; do
  fresh
  rm -f "$T/home/tehas/state/cycles/7.timed"
  container "\"\$nap\" 1.2; $end"
  t_run "$work 7"
  t_lacks "$t_out" "status=timeout"
  t_eq "$(find "$T/home/tehas/state/cycles/7.timed" 2>/dev/null | wc -l)" 0
done
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "a cycle none of whose logs change is stopped as stalled before its time limit"
fresh
# Its start on record, so its heartbeat is published.
under_cycle
rm -f "$T/home/tehas/state/cycles/7.stalled"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_STALL=2 TEHAS_VERIFY_TIMEOUT=1
container "$hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "none of the cycle's logs changed for 2s, stopping it as stalled"
t_contains "$t_out" "status=stalled"
t_lacks "$t_out" "time limit"
t_lacks "$t_out" "was killed"
t_contains "$(t_calls docker)" "stop -t 20 tehas-work-7"
t_eq "$(cat "$T/home/tehas/state/cycles/7.stalled")" "2
-"
t_contains "$t_out" "status=stalled step=- cap=stall"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"stalled"]'

t_case "a stalled runner that writes its result as it is stopped is still stalled"
fresh
container "trap 'says interrupted; exit 21' TERM; \"\$nap\" 60 & echo \$! >\"\$T/sleeper.pid\"; wait; wait"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "status=stalled"

t_case "a cycle whose agents keep writing is not stalled"
fresh
rm -f "$T/home/tehas/state/cycles/7.stalled"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_STALL=2 TEHAS_VERIFY_TIMEOUT=1 TEHAS_WORK_TIMEOUT=5
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
container "$hold; ( while kill -0 \"\$(cat \"\$T/sleeper.pid\")\" 2>/dev/null; do date >>\"$runs/agent.jsonl\"; \"\$nap\" 0.3; done ) & $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 5s passed"
t_lacks "$t_out" "stalled"
t_eq "$(find "$T/home/tehas/state/cycles/7.stalled" 2>/dev/null | wc -l)" 0
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "progress the runner writes keeps a cycle from being stalled"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_STALL=2 TEHAS_VERIFY_TIMEOUT=1 TEHAS_WORK_TIMEOUT=5
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
container "$hold; ( while kill -0 \"\$(cat \"\$T/sleeper.pid\")\" 2>/dev/null; do progress '{\"v\":1,\"kind\":\"log\",\"message\":\"x\"}'; \"\$nap\" 0.3; done ) & $gone"
t_run "$work 7"
t_contains "$t_out" "time limit of 5s passed"
t_lacks "$t_out" "stalled"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "TERM stops the container, and the runner's result says interrupted"
fresh
# Its start on record, so its heartbeat is published.
under_cycle
container "trap 'says interrupted; exit 21' TERM; \"\$nap\" 60 & echo \$! >\"\$T/sleeper.pid\"; wait; wait"
t_run "$work 7 >\"\$T/work.out\" 2>&1 & pid=\$!
for _ in \$(seq 100); do [ -e \"\$T/sleeper.pid\" ] && [ \"\$(jq -r .running \"$live/cycles/7.json\" 2>/dev/null)\" = true ] && break; \"\$(command -pv sleep)\" 0.1; done
kill -TERM \$pid; wait \$pid; echo rc=\$?"
kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null
t_contains "$(t_calls docker)" "stop -t 10 tehas-work-7"
t_lacks "$(cat "$T/work.out")" "was killed"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"interrupted"]'

t_case "TERM with no result gives interrupted"
fresh
# Its start on record, so its heartbeat is published.
under_cycle
container "$hold; $gone"
t_run "$work 7 >\"\$T/work.out\" 2>&1 & pid=\$!
for _ in \$(seq 100); do [ -e \"\$T/sleeper.pid\" ] && [ \"\$(jq -r .running \"$live/cycles/7.json\" 2>/dev/null)\" = true ] && break; \"\$(command -pv sleep)\" 0.1; done
kill -TERM \$pid; wait \$pid; echo rc=\$?"
kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null
t_contains "$t_out" "rc=2"
t_lacks "$(cat "$T/work.out")" "was killed"
t_contains "$(cat "$T/work.out")" "status=interrupted"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"interrupted"]'

t_case "a TERM as soon as the container starts is caught: interrupted, and the container stopped"
fresh
container "\"\$nap\" 60 >/dev/null 2>&1 & echo \$! >\"\$T/sleeper.pid\"; kill -TERM \$PPID; $gone"
t_run "$work 7"
kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null
t_eq "$t_rc" 2
t_contains "$t_out" "status=interrupted"
t_contains "$(t_calls docker)" "stop -t 10 tehas-work-7"

t_case "a container left under the issue's name is removed before the cycle's starts"
rm_at=$(t_calls docker | grep -n -x -e 'rm -f tehas-work-7' | cut -d: -f1 | head -n 1)
run_at=$(t_calls docker | grep -n -e 'pi-rukas-headless work ' | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$rm_at" ] && [ -n "$run_at" ] && [ "$rm_at" -lt "$run_at" ] && echo before)" before

t_case "a docker stop that does not end is given up after its bound, said, and the container killed"
fresh
: >"$T/stop-hangs"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=1
T_ENV=("${env_ok[@]}" TEHAS_STOP_WAIT=1 TEHAS_WORK_GRACE=0)
container "$hold; says handoff; $gone"
t_run "$work 7"
rm -f "$T/stop-hangs"
t_eq "$t_rc" 2
t_contains "$t_out" "tehas work: docker stop of tehas-work-7 did not end within 1s, killing it"
t_contains "$(t_calls docker)" "kill tehas-work-7"
t_contains "$t_out" "status=handoff"

t_case "a stop that ends in time kills nothing"
fresh
container "$hold; says handoff; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "status=handoff"
t_lacks "$(t_calls docker)" "kill "
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "a console that does not stop on TERM is killed after its bound, and said"
fresh
# exec keeps the ignored TERM and leaves no child behind holding the lock.
t_stub console '[ "$1" = --mask ] && exit 0; trap "" TERM; exec "$(command -pv sleep)" 30'
mv "$T/bin/console" "$tree/libexec/console"
T_ENV=("${env_ok[@]}" TEHAS_CONSOLE_WAIT=1)
container 'says handoff; exit 10'
start=$(date +%s)
t_run "$work 7"
t_eq "$t_rc" 10
t_contains "$t_out" "tehas work: the console did not stop within 1s, killing it"
t_contains "$t_out" "status=handoff"
[ "$(($(date +%s) - start))" -lt 20 ] || t_bad "the console was waited on for $(($(date +%s) - start))s"
t_stub console
mv "$T/bin/console" "$tree/libexec/console"

t_case "a console that stops in time is not killed"
fresh
container 'says handoff; exit 10'
t_run "$work 7"
t_lacks "$t_out" "the console did not stop"
t_lacks "$t_out" "the console ended"
T_ENV=("${env_ok[@]}")

t_case "a push check that does not end is given up after its bound: exit 3, no cycle"
fresh
: >"$T/push-hangs"
T_ENV=("${env_ok[@]}" TEHAS_PREFLIGHT_WAIT=1)
began=$(date +%s)
t_run "$work 7"
rm -f "$T/push-hangs"
t_eq "$t_rc" 3
t_contains "$t_out" "the push check did not end within 1s"
t_eq "$(cycle_args)" ""
t_eq "$(($(date +%s) - began < 20))" 1
T_ENV=("${env_ok[@]}")

t_done
