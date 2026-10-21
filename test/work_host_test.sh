#!/usr/bin/env bash
# Tests for libexec/work: what a cycle does on the host. The workspace of an
# earlier cycle, the issue's lock, the live view, what is kept on record
# and the container's counts of kills and refused starts.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1

# The workspace of an earlier cycle of the issue, which the case below moves
# aside and compares against.
t_run "$work 7"

t_case "what an earlier cycle left in the workspace is gone, and the host runs none of it"
# Everything an agent could leave to be run later: a hook, git configuration
# that names a program, an untracked file, another issue's state file. The state
# file is put under a directory of its own, because the product's gate is a file
# of the cycle's clone: its .pi is the product's, not a leftover.
mkdir -p "$ws/.git/hooks" "$ws/agent-state"
printf '#!/bin/sh\ntouch "%s"\n' "$T/ran-hook" >"$ws/.git/hooks/post-checkout"
cp "$ws/.git/hooks/post-checkout" "$ws/.git/hooks/reference-transaction"
chmod +x "$ws/.git/hooks/post-checkout" "$ws/.git/hooks/reference-transaction"
git -C "$ws" config core.fsmonitor "touch $T/ran-fsmonitor; false"
git -C "$ws" config core.sshCommand "touch $T/ran-ssh"
echo left >"$ws/left-over.txt"
echo '{"pipelineState":{"status":"merged"}}' >"$ws/agent-state/9.json"
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(find "$T" -maxdepth 1 -name 'ran-*' | wc -l)" 0
t_eq "$(find "$ws/left-over.txt" "$ws/agent-state" "$ws/.git/hooks/post-checkout" 2>/dev/null | wc -l)" 0
# The gate of the product is in the clone, as it is in the commit it was made at.
t_eq "$(cat "$ws/.pi/verify-cmd")" "scripts/verify.sh"
t_eq "$(git -C "$ws" config --get core.fsmonitor || echo unset)" unset
t_eq "$(git -C "$ws" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"

t_case "what a stopped cycle left is kept beside the new workspace, not deleted"
t_eq "$(cat "$ws.before-retry/left-over.txt")" left
t_eq "$(git -C "$ws.before-retry" config --get core.fsmonitor)" "touch $T/ran-fsmonitor; false"

t_case "the kept workspace is given to no container"
t_lacks "$(cat "$T"/calls/docker.args.*)" "before-retry"

t_case "a second retry keeps the latest stopped workspace only"
echo second >"$ws/second.txt"
t_run "$work 7"
t_eq "$(cat "$ws.before-retry/second.txt")" second
t_eq "$(find "$ws.before-retry/left-over.txt" 2>/dev/null | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" -maxdepth 1 \( -name widget-7 -o -name 'widget-7.before-retry' \) | wc -l)" 2
t_eq "$(find "$T/home/tehas/work" -maxdepth 1 -name 'widget-7*' | wc -l)" 3 # the two, and <ws>.state
t_eq "$(find "$T" -maxdepth 1 -name 'ran-*' | wc -l)" 0

t_case "a kept workspace that cannot be replaced starts no cycle"
chmod a-w "$ws.before-retry"
mkdir -p "$ws.before-retry/held" 2>/dev/null
rm -f "$T"/calls/docker*
if [ "$(id -u)" != 0 ] && [ -n "$(ls -A "$ws.before-retry")" ]; then
  t_run "$work 7"
  t_eq "$t_rc" 78
  t_contains "$t_out" "could not make the workspace $ws, not starting a cycle"
  t_eq "$(find "$T"/calls -name 'docker.args.*' | wc -l)" 0
fi
chmod u+w "$ws.before-retry"

t_case "a commit the cycle made on its own branch does not reach the next cycle"
git -C "$ws" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m planted
t_run "$work 7"
t_eq "$(git -C "$ws" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_eq "$(git -C "$checkout" rev-parse main)" "$(git -C "$upstream" rev-parse main)"

t_case "a cycle of the same issue that holds its lock is left alone: exit 5, nothing moved, no container"
lockf=$T/home/tehas/state/cycles/7.lock
echo mine >"$ws/held.txt"
rm -f "$T"/calls/docker*
(exec 9>"$lockf" && flock 9 && exec "$nap" 30) &
holder=$!
for _ in $(seq 50); do flock -n "$lockf" true || break; sleep 0.1; done
t_run "$work 7"
kill "$holder" 2>/dev/null
wait "$holder" 2>/dev/null
t_eq "$t_rc" 5
t_contains "$t_out" "tehas work: a cycle for #7 is running, not starting another"
t_eq "$(cat "$ws/held.txt")" mine
t_eq "$(find "$ws.before-retry/held.txt" 2>/dev/null | wc -l)" 0
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_lacks "$(t_calls docker)" "run "

t_case "under tehasbot cycle the lock it holds is handed down as fd 9, and the cycle starts"
t_run "exec 9>'$lockf' && flock -n 9 || exit 99; $work 7"
t_eq "$t_rc" 2
t_contains "$(cycle_args | tr '\n' ' ')" "$runner work 7 --timeout 360 --json"

t_case "a run by hand holds the issue's lock while its container runs, and lets go at the end"
echo "if flock -n '$lockf' true; then echo free; else echo held; fi >\"\$T/lock.seen\"" >"$T/container.sh"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cat "$T/lock.seen")" held
t_eq "$(flock -n "$lockf" echo free)" free
rm -f "$T/container.sh" "$T/lock.seen"

t_case "another issue's workspace and transcripts are left alone"
mkdir -p "$T/home/tehas/work/widget-9" "$T/home/tehas/work/widget-9.state" "$T/home/.pi/agent/ensemble-runs/issue-9"
echo keep >"$T/home/tehas/work/widget-9/notes.txt"
echo keep >"$T/home/tehas/work/widget-9.state/note.json"
echo keep >"$T/home/.pi/agent/ensemble-runs/issue-9/a-1-coder.json"
rm -f "$T"/calls/docker*
t_run "$work 7"
t_eq "$(cat "$T/home/tehas/work/widget-9/notes.txt")" keep
t_eq "$(cat "$T/home/tehas/work/widget-9.state/note.json")" keep
t_eq "$(cat "$T/home/.pi/agent/ensemble-runs/issue-9/a-1-coder.json")" keep
t_lacks "$(cat "$T"/calls/docker.args.*)" "widget-9"
t_lacks "$(cat "$T"/calls/docker.args.*)" "issue-9"

t_case "the cycle's status line is the driver's last notice, published as the console's mask leaves it"
cat >"$T/container.sh" <<'EOF'
cat >&2 <<'LINES'
{"v":1,"kind":"step-started","event":{"step":"develop"}}
{"v":1,"kind":"log","message":"step 3 of the cycle"}
{"v":1,"kind":"log","unstable":true,"message":"not this"}
LINES
EOF
t_stub console 'if [ "$1" = --mask ]; then sed "s/cycle/[masked]/"; fi'
mv "$T/bin/console" "$tree/libexec/console"
t_run "$work 7"
t_eq "$(jq -r .last.statusText "$T/home/tehas/live/running.json")" "step 3 of the [masked]"
t_eq "$(jq -r .last.step "$T/home/tehas/live/running.json")" develop

# Neither command writes to a process through a fifo any more: both run the
# headless runner as the container's command.
t_case "no command writes to Pi through a fifo"
t_eq "$(grep -c '>&4' "$T_ROOT/libexec/work")" 0
t_eq "$(grep -c '>&4' "$T_ROOT/libexec/plan")" 0

t_case "without a console the status line is not published"
mv "$tree/libexec/console" "$T/console.kept"
t_run "$work 7"
t_eq "$(jq -r '.last.statusText, .last.status' "$T/home/tehas/live/running.json" | tr '\n' ,)" ",running,"
t_stub console
mv "$T/bin/console" "$tree/libexec/console"
rm -f "$T/container.sh"

other() { # issue running status updatedAt: the live file of another cycle
  printf '{"issue":%s,"running":%s,"status":"%s","step":"develop","statusText":"","startedAt":%s,"updatedAt":%s,"console":"console-%s.txt"}\n' \
    "$1" "$2" "$3" "$(($4 - 60))" "$4" "$1" >"$live/cycles/$1.json"
}

t_case "the cycle writes a live file of its own, and the view has it as the last one"
t_run "$work 7"
t_eq "$(jq -c '[.issue, .running, .console]' "$live/cycles/7.json")" '[7,false,"console-7.txt"]'
t_eq "$(jq -c '[(.cycles | length), .last.issue]' "$live/running.json")" "[0,7]"

t_case "another cycle that is running is in the view beside it"
other 9 true running "$(date +%s)"
t_run "$work 7"
t_eq "$(jq -c '[.cycles[].issue, .last.issue]' "$live/running.json")" "[9,7]"
t_eq "$(jq -r .cycles[0].console "$live/running.json")" "console-9.txt"

t_case "a finished cycle that is not the last one leaves the view and the disk"
other 5 false handoff 1000
t_run "$work 7"
t_eq "$(find "$live/cycles" -name '*.json' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "7.json 9.json "
t_eq "$(jq -r .last.issue "$live/running.json")" 7

t_case "a cycle that has written nothing for two minutes is no longer shown as running"
other 9 true running "$(($(date +%s) - 300))"
# Cycle 9 counts as ended when it is found, as cycle 7 starts. The container
# waits until it sees cycle 9 marked lost, then for the next second, so cycle 7
# is the one that finished last however slow the host is.
echo 'for _ in $(seq 300); do [ "$(jq -r .status "'"$live"'/cycles/9.json" 2>/dev/null)" = lost ] && break; "$nap" 0.1; done
s=$(date +%s); while [ "$(date +%s)" = "$s" ]; do "$nap" 0.1; done' >"$T/container.sh"
t_run "$work 7"
rm -f "$T/container.sh"
t_eq "$(jq -c '[(.cycles | length), .last.issue]' "$live/running.json")" "[0,7]"
rm -rf "$live/cycles"

t_case "a cycle that is stopped is recorded as interrupted"
# The cycle's call stays alive until docker stop is called, as a container does.
echo "$nap"' 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid"; wait' >"$T/container.sh"
rm -f "$live/cycles/7.json" "$T/sleeper.pid"
# Every wait is bounded. The live file says running only after the trap is set.
t_run "$work 7 >\"\$T/work.out\" 2>&1 & pid=\$!
for _ in \$(seq 100); do [ \"\$(jq -r .running \"$live/cycles/7.json\" 2>/dev/null)\" = true ] && break; sleep 0.1; done
kill -TERM \$pid; wait \$pid; echo rc=\$?"
kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null
t_contains "$t_out" "rc=2"
t_contains "$(t_calls docker)" "stop -t 10 tehas-work-7"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"interrupted"]'
rm -rf "$live/cycles" "$T/container.sh"

t_case "the image's id and the pi-rukas commit of its label are on record, read before the container starts"
fresh
image_id=sha256:$(printf 'e%.0s' {1..64})
echo "$image_id 0123456789abcdef0123456789abcdef01234567" >"$T/image.inspect"
container 'says handoff'
t_run "$work 7"
t_eq "$(cat "$T/home/tehas/state/cycles/7.image")" "$image_id 0123456789abcdef0123456789abcdef01234567"
t_contains "$(t_calls docker)" "image inspect --format {{.Id}} {{index .Config.Labels \"tehas.rukas\"}} tehas/pi-rukas:uid$(id -u)"
at_inspect=$(t_calls docker | grep -n -e '^image inspect' | cut -d: -f1 | head -n 1)
at_run=$(t_calls docker | grep -n -e 'pi-rukas-headless work ' | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$at_inspect" ] && [ -n "$at_run" ] && [ "$at_inspect" -lt "$at_run" ] && echo before)" before

t_case "an image without the label is on record by its id alone"
fresh
echo "$image_id " >"$T/image.inspect"
t_run "$work 7"
t_eq "$(cat "$T/home/tehas/state/cycles/7.image")" "$image_id "

t_case "an answer that is not an image id leaves no record, and one left by an earlier cycle goes"
fresh
echo "not-an-id 0123456 \$(touch x)" >"$T/image.inspect"
echo "$image_id 0123456" >"$T/home/tehas/state/cycles/7.image"
t_run "$work 7"
t_eq "$(find "$T/home/tehas/state/cycles/7.image" 2>/dev/null | wc -l)" 0
echo "$image_id not-a-commit" >"$T/image.inspect"
t_run "$work 7"
t_eq "$(cat "$T/home/tehas/state/cycles/7.image")" "$image_id "
rm -f "$T/image.inspect" "$T/home/tehas/state/cycles/7.image"

t_case "what the container is given of the settings is on record, before it starts, with the cycles running"
fresh
container 'says handoff'
t_run "$work 7"
t_eq "$t_rc" 0
f=$T/home/tehas/state/cycles/7.settings
t_eq "$(jq -r '.container | keys[]' "$f" | LC_ALL=C sort | tr '\n' ' ')" \
  "PI_ENSEMBLE_CYCLE_DEADLINE PI_ENSEMBLE_HANDOFF_CHANNEL PI_ENSEMBLE_HANDOFF_TIMEOUT_MS PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS PI_ENSEMBLE_MAX_WORKSTREAMS PI_ENSEMBLE_SANDBOX_MODE PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER PI_ENSEMBLE_VERIFY_CONCURRENCY PI_ENSEMBLE_VERIFY_TIMEOUT_MS PI_ENSEMBLE_WORKSPACE_ROOT "
t_eq "$(jq -r '.container.PI_ENSEMBLE_SUBAGENT_MODEL' "$f")" test-model
t_eq "$(jq -r '.container.PI_ENSEMBLE_WORKSPACE_ROOT' "$f")" "[withheld]"
# This command holds the issue's lock, so it counts itself.
t_eq "$(jq -r '.concurrentCycles' "$f")" 1
t_lacks "$(cat "$f")" GH_TOKEN
t_lacks "$(cat "$f")" test-token

t_case "the container's oom_kill count is printed and written to the state directory, and is not read from the slice"
fresh
slice_good
slice_child "$cid" 1000 0 max 3
slice_child "$(slice_id b)" 1000 0 max 5
printf '%s\n' "low 0" "high 0" "max 9" "oom 9" "oom_kill 9" >"$T_CGROUP/tehas.slice/memory.events"
# The scope goes with the container: the count is the last one seen.
container "\"\$nap\" 1; rm -rf '$T_CGROUP/tehas.slice/docker-$cid.scope'; \"\$nap\" 0.5"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "tehas work: 3 processes in this cycle's container were killed for memory"
t_eq "$(cat "$oom_file")" 3
t_eq "$(find "$cid_file" 2>/dev/null | wc -l)" 0

t_case "the starts the pids limit refused are printed and written to the state directory"
fresh
slice_good
slice_child "$cid" 1000 0 max
slice_child "$(slice_id b)" 1000 0 max
printf 'max 41\n' >"$T_CGROUP/tehas.slice/docker-$cid.scope/pids.events"
printf 'max 7\n' >"$T_CGROUP/tehas.slice/docker-$(slice_id b).scope/pids.events"
container "\"\$nap\" 1; rm -rf '$T_CGROUP/tehas.slice/docker-$cid.scope'; \"\$nap\" 0.5"
t_run "$work 7"
t_contains "$t_out" "tehas work: this cycle's container reached its limit of processes, 41 starts were refused"
t_eq "$(cat "$forks_file")" 41
t_lacks "$t_out" "killed for memory"

t_case "a container that stayed under the pids limit leaves no count, and one left by an earlier cycle is removed"
fresh
slice_good
echo 9 >"$forks_file"
slice_child "$cid" 1000 0 max
printf 'max 0\n' >"$T_CGROUP/tehas.slice/docker-$cid.scope/pids.events"
container '"$nap" 0.5'
t_run "$work 7"
t_lacks "$t_out" "limit of processes"
t_eq "$(find "$forks_file" 2>/dev/null | wc -l)" 0

t_case "a cid file and a count left by a cycle that was killed outright are not this cycle's"
fresh
slice_good
slice_id b >"$cid_file"
echo 5 >"$oom_file"
slice_child "$(slice_id b)" 1000 0 max 5
container 'touch "$T/calls/ran"'
t_run "$work 7"
t_eq "$(find "$T/calls/ran" 2>/dev/null | wc -l)" 1
t_lacks "$t_out" "killed for memory"
t_eq "$(find "$cid_file" "$oom_file" 2>/dev/null | wc -l)" 0

t_case "a cid file that does not hold a container id reads no cgroup"
fresh
echo "../../tehas.slice" >"$T/cid"
printf '%s\n' "oom_kill 9" >"$T_CGROUP/tehas.slice/memory.events"
container '"$nap" 0.5'
t_run "$work 7"
t_lacks "$t_out" "killed for memory"
t_eq "$(find "$oom_file" 2>/dev/null | wc -l)" 0
slice_good
rm -f "$T/container.sh"

t_done
