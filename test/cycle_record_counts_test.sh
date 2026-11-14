#!/usr/bin/env bash
# Tests for libexec/cycle: the counts a cycle reads from the files of its
# container - the memory kills and the refused starts - and what it says of them.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1
t_case "the refused starts are read as digits, said in the comment and the log, and the file is removed"
reset
echo 'echo 41 >"$TEHAS_HOME/state/cycles/7.forks"' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Its container reached its limit of processes (\`--pids-limit\`, which follows \`TEHAS_TICKET_CONCURRENCY\` and \`TEHAS_SLICE_CONTAINERS\`) and 41 starts of a process were refused: what the cycle ran after that may not have run."
t_contains "$t_out" "forks_refused=41"
t_eq "$(find "$state/7.forks" 2>/dev/null | wc -l)" 0

t_case "the pids limit on the second line of the count is named with the settings it follows"
reset
printf '21\n2048\n' >"$fx/forks"
echo 'cp "$T/fx/forks" "$TEHAS_HOME/state/cycles/7.forks"' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Its container reached its limit of 2048 processes (\`--pids-limit\`, which follows"
t_contains "$t_out" "forks_refused=21"

t_case "a count of refused starts that is not digits, or is a link, counts as nothing"
for bad in 'many' '4; reboot' '-4' '04' ''; do
  reset
  printf '%s\n' "$bad" >"$fx/forks"
  echo 'cp "$T/fx/forks" "$TEHAS_HOME/state/cycles/7.forks"' >"$fx/work.sh"
  t_run "$cycle 7"
  t_lacks "$(t_calls gh)" "limit of processes"
  t_lacks "$(t_calls gh)" "reboot"
  t_contains "$t_out" "forks_refused=0"
done
reset
echo 4 >"$fx/elsewhere"
echo 'ln -s "$T/fx/elsewhere" "$TEHAS_HOME/state/cycles/7.forks"' >"$fx/work.sh"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "limit of processes"
t_eq "$(find "$state/7.forks" 2>/dev/null | wc -l)" 0

t_case "a count of refused starts left by an earlier cycle of the issue is not this cycle's"
reset
mkdir -p "$state"
echo 6 >"$state/7.forks"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "limit of processes"
t_contains "$t_out" "forks_refused=0"

t_case "a count file with letters counts as nothing"
for bad in 'three' '3 processes' '3; reboot' '`id`' '-3' '03' '12345678901234567890' ''; do
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
  printf '%s\n' "$bad" >"$fx/oom"
  echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"; cp "$T/fx/oom" "$TEHAS_HOME/state/cycles/7.oom"; exit 2' >"$fx/work.sh"
  t_run "$cycle 7"
  t_lacks "$(t_calls gh)" "killed for memory"
  t_lacks "$(t_calls gh)" "reboot"
  t_contains "$t_out" "oom_kills=0"
  t_eq "$(jq -c .oomKills "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" 0
  t_eq "$(find "$state/7.oom" 2>/dev/null | wc -l)" 0
done

t_case "a count file that is a link is not read"
reset
echo 4 >"$fx/elsewhere"
echo 'ln -s "$T/fx/elsewhere" "$TEHAS_HOME/state/cycles/7.oom"' >"$fx/work.sh"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "killed for memory"
t_eq "$(cat "$fx/elsewhere")" 4
t_eq "$(find "$state/7.oom" 2>/dev/null | wc -l)" 0

t_case "the appended log is not where the count comes from"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"
echo "tehas work: 5 processes in this cycle'"'"'s container were killed for memory"; exit 2' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(cat "$T/home/tehas/logs/dispatch-work-7.log")" "5 processes"
t_lacks "$(t_calls gh)" "killed for memory"
t_eq "$(jq -c .oomKills "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" 0

t_case "a count left by an earlier cycle of the issue is not this cycle's"
reset
mkdir -p "$state"
echo 6 >"$state/7.oom"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "killed for memory"
t_contains "$t_out" "oom_kills=0"

t_done
