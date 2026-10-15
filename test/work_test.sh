#!/usr/bin/env bash
# Tests for libexec/work: what the cycle's container is given, that nothing
# starts without the slice, and how a killed container is reported. A stub
# docker records its arguments, one per line, and exits at once, so no cycle
# runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

# The script as a host has it: beside the console, a compiled program that a
# checkout does not hold. A stub stands in for it, and it is not on PATH.
tree=$T/tree
mkdir -p "$tree/libexec"
ln -s "$T_ROOT/lib" "$tree/lib"
ln -s "$T_ROOT/libexec/work" "$tree/libexec/work"
t_stub console
mv "$T/bin/console" "$tree/libexec/console"
work="$tree/libexec/work"
checkout=$T/home/tehas/repos/widget
# The workspace of issue 7 on the host, and the path every cycle has it at
# inside its container.
ws=$T/home/tehas/work/widget-7
inside=$T/home/tehas/work/widget
runs=$T/home/.pi/agent/ensemble-runs/issue-7
# The cycle's own state on the host, and the host's directory of the code index.
state=$ws.state
hostindex=$T/home/tehas/state/index
# The product repository, and the host's checkout of it.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

# docker answers the slice's question from a file and, like docker, refuses to
# start a container whose cid file is already there. Only a run is recorded
# one argument per line; what the container does is a file a case writes.
t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
if [ "$1" = image ]; then
  case " $* " in *"{{.Id}}"*) cat "$T/image.inspect" ;; *) cat "$T/image.label" ;; esac 2>/dev/null
  exit
fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
prev=""
for a in "$@"; do
  if [ "$prev" = --cidfile ]; then
    [ ! -e "$a" ] || { echo "docker: container ID file found" >&2; exit 125; }
    cat "$T/cid" >"$a"
  fi
  prev=$a
done
case " $* " in
  *" --mode "*) [ ! -e "$T/container.sh" ] || . "$T/container.sh" ;;
  *" push --dry-run "*)
    [ ! -e "$T/push-hangs" ] || exec "$(command -pv sleep)" 30
    [ "${PUSH_RC:-0}" = 0 ] || { echo "docker: could not read from remote repository" >&2; exit "$PUSH_RC"; } ;;
  " stop "*)
    [ ! -e "$T/stop-hangs" ] || exec "$(command -pv sleep)" 30
    kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
  " kill "*) kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
esac'
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"

# A host with the root step done: the slice loaded from its file with 8 GiB,
# so a container's ceiling is 2048m, Docker on the systemd driver and cgroup
# v2, and nothing in the slice.
cid=$(slice_id a)
cid_file=$T/home/tehas/state/cycles/7.cid
oom_file=$T/home/tehas/state/cycles/7.oom
forks_file=$T/home/tehas/state/cycles/7.forks
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  echo "$cid" >"$T/cid"
  slice_state 0.00
}
t_stub systemctl 'cat "$T/slice.show"'
slice_good
env_ok=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")

# The arguments of the docker call that starts the cycle.
cycle_args() { grep -l -x -e '--mode' "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
# The arguments of the docker call that checks the push before the cycle.
preflight_args() { grep -l -x -e '--entrypoint' "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
# Values that follow a flag, e.g. every -v or every -e.
after() { cycle_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
# Every option of a docker call that is not one this command is known to
# pass, up to the image name. Options are read the way docker reads them: the
# ones that take a value consume the next argument. A mount or a variable
# spelled another way (--volume, --mount, --env, -eNAME), or any other option
# (--network, --cap-add, --user 0:0), shows up here.
unknown_options() { # <image> <options that take a value> <options that take none>
  awk -v image="$1" -v valued=" $2 " -v plain=" $3 " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(plain, " " $0 " ") { next }
    /^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$/ { next }
    { print }'
}

t_case "a work directory tehas did not make is left alone"
t_instance "${T_MINIMAL[@]}"
T_ENV=("${env_ok[@]}")
mkdir -p "$T/home/tehas/work/widget" "$T/home/tehas/work/widget-7"
echo keep >"$T/home/tehas/work/widget/notes.txt"
echo keep >"$T/home/tehas/work/widget-7/notes.txt"
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "was not made by tehas"
t_eq "$(cat "$T/home/tehas/work/widget/notes.txt")" keep
t_eq "$(cat "$T/home/tehas/work/widget-7/notes.txt")" keep
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
rm -rf "$T/home/tehas/work"

t_case "an invalid instance starts no container"
rm -f "$T"/calls/*
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)$(t_calls systemctl)" ""

t_case "the cycle gets the slice, a ceiling of a quarter of it, no swap, a floor, a pids limit and a CPU weight"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 2 # the stub ends at once, so the cycle counts as died
t_eq "$(after --cgroup-parent)" "tehas.slice"
t_eq "$(after --memory)" "2048m"
t_eq "$(after --memory-swap)" "2048m"
t_eq "$(after --memory-reservation)" "2g"
t_eq "$(after --pids-limit)" "4096"
t_eq "$(after --cpu-shares)" "1024"
t_eq "$(after --cidfile)" "$cid_file"
t_lacks "$(cycle_args)" "--cpus"

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

t_case "the cid file is gone when the cycle is over, and no count of kills is left"
t_eq "$(find "$cid_file" "$oom_file" 2>/dev/null | wc -l)" 0
t_lacks "$t_out" "killed"

t_case "the image and the working directory come from the settings"
t_contains "$(cycle_args | tr '\n' ' ')" "tehas/pi-rukas:uid$(id -u) pi --mode rpc --approve"
t_eq "$(after -w)" "$inside"

t_case "the console is the program beside this one, given the cycle's log, the transcripts and its file"
t_contains "$(t_calls console)" "--rpc $T/home/tehas/logs/work-7-"
t_contains "$(t_calls console)" ".jsonl --runs $runs --out $T/home/tehas/live/console-7.txt --since "

t_case "a console that fails is named with its exit status and where it wrote why"
t_lacks "$t_out" "the console ended"
printf '#!/usr/bin/env bash\necho "cannot write" >&2\nexit 1\n' >"$tree/libexec/console"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "tehas work: the console ended with 1, see $T/home/tehas/logs/work-7-"
t_eq "$(cat "$T"/home/tehas/logs/work-7-*.jsonl.console | sort -u)" "cannot write"

t_case "without a console the cycle still runs, and says there is none"
mv "$tree/libexec/console" "$T/console.kept"
rm -f "$T"/calls/docker*
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "tehas work: no console at $tree/libexec/console"
t_lacks "$t_out" "the console ended"
t_contains "$(cycle_args | tr '\n' ' ')" "pi --mode rpc --approve"
t_stub console
mv "$T/bin/console" "$tree/libexec/console"
t_run "$work 7"

t_case "the cycle works in a fresh clone at the branch's tip, with the product's origin"
t_eq "$(git -C "$ws" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_eq "$(git -C "$ws" branch --show-current)" main
t_eq "$(git -C "$ws" config --get remote.origin.url)" "$(git -C "$checkout" config --get remote.origin.url)"
t_eq "$(git -C "$ws" symbolic-ref --short refs/remotes/origin/HEAD)" origin/main

t_case "the product checkout is not mounted anywhere"
t_lacks "$(cat "$T"/calls/docker.args.*)" "$checkout"

t_case "exactly the documented paths are mounted"
want="$ws:$inside
$runs:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the agents are told the path inside the container, not the host's"
t_contains "$(after -e)" "PI_ENSEMBLE_WORKSPACE_ROOT=$inside"
t_lacks "$(after -e)" "widget-7"

t_case "nothing is made on the host at the path inside the container"
t_eq "$(find "$inside" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "the cycle's transcripts directory is its own and is made for it"
t_eq "$(find "$runs" -maxdepth 0 -type d | wc -l)" 1

t_case "the cycle's own state is made beside its workspace, and no shared directory is mounted"
t_eq "$(find "$state" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "cache memory sessions "
t_lacks "$(cat "$T"/calls/docker.args.*)" "pi-ensemble"
t_lacks "$(cat "$T"/calls/docker.args.*)" "$T/home/.pi/agent/sessions"
t_lacks "$(cat "$T"/calls/docker.args.*)" "$T/home/.vipune"

t_case "the cycle is given a copy of the host's code index, and the agents are told where"
mkdir -p "$hostindex/logs"
echo index >"$hostindex/widget.db"
echo cfg >"$hostindex/_config.db"
echo log >"$hostindex/logs/index-1.log"
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$(find "$state/cache" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "_config.db widget.db "
t_eq "$(cat "$state/cache/widget.db")" index
t_eq "$(cat "$hostindex/widget.db")" index
t_eq "$(after -e | grep '^CBM_CACHE_DIR')" "CBM_CACHE_DIR=/home/vscode/.tehas-cache"

t_case "the copy holds the index and not what an interrupted index run left behind"
mkdir -p "$hostindex/logs"
echo left >"$hostindex/widget.db.stage.abc123.tmp.945328.0x200002bc000"
echo gone >"$hostindex/scratch.tmp"
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$(find "$state/cache" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "_config.db widget.db "

t_case "what one cycle wrote in its own state is not what the next cycle of the issue is given"
echo planted >"$state/memory/note.json"
echo planted >"$state/cache/widget.db"
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$(find "$state/memory" -mindepth 1 | wc -l)" 0
t_eq "$(cat "$state/cache/widget.db")" index

t_case "exactly the documented variables are passed in"
want="GH_TOKEN
GIT_CONFIG_COUNT
GIT_CONFIG_KEY_0
GIT_CONFIG_KEY_1
GIT_CONFIG_KEY_2
GIT_CONFIG_KEY_3
GIT_CONFIG_VALUE_0
GIT_CONFIG_VALUE_1
GIT_CONFIG_VALUE_2
GIT_CONFIG_VALUE_3
GIT_TERMINAL_PROMPT
CBM_CACHE_DIR
PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_NO_DOCKER_SOCKET
PI_ENSEMBLE_NO_SSH
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the cycle's container gets no option beyond the documented ones"
image="tehas/pi-rukas:uid$(id -u)"
t_eq "$(cycle_args | unknown_options "$image" \
  "--name --cgroup-parent --memory --memory-swap --memory-reservation --pids-limit --cpu-shares --cidfile -w -v -e" "--rm -i --init")" ""
t_eq "$(cycle_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' ')" "$image pi --mode rpc --approve "

# Pi is no init: without one, a process that outlives its parent is never
# collected and counts against the pids limit until the container ends.
t_case "the cycle's container has an init"
t_eq "$(cycle_args | grep -c -x -e --init)" 1

t_case "the push check's container gets the workspace and the same variables, no more"
t_eq "$(preflight_args | unknown_options "$image" "--user -e -w -v --entrypoint" "--rm")" ""
t_eq "$(preflight_args | awk 'prev == "-v" { print } { prev = $0 }')" "$ws:$inside"
t_eq "$(preflight_args | awk 'prev == "-e" { print } { prev = $0 }' | sed 's/=.*//' | LC_ALL=C sort)" \
  "$(printf '%s\n' "$want" HOME | LC_ALL=C sort)"
t_eq "$(preflight_args | awk 'prev == "--user" { print } { prev = $0 }')" "$(id -u):$(id -g)"

t_case "the token is passed by name, never written into the arguments"
t_lacks "$(cycle_args)" "test-token"

t_case "nothing that controls the host is reachable"
args=$(cat "$T"/calls/docker.args.*)
t_contains "$args" "--mode"
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "--privileged"

t_case "host aliases become --add-host entries"
t_contains "$(cycle_args)" "--add-host=models:192.0.2.9"

t_case "what an earlier cycle left in the workspace is gone, and the host runs none of it"
# Everything an agent could leave to be run later: a hook, git configuration
# that names a program, an untracked file, another issue's state file.
mkdir -p "$ws/.git/hooks" "$ws/.pi/work-state"
printf '#!/bin/sh\ntouch "%s"\n' "$T/ran-hook" >"$ws/.git/hooks/post-checkout"
cp "$ws/.git/hooks/post-checkout" "$ws/.git/hooks/reference-transaction"
chmod +x "$ws/.git/hooks/post-checkout" "$ws/.git/hooks/reference-transaction"
git -C "$ws" config core.fsmonitor "touch $T/ran-fsmonitor; false"
git -C "$ws" config core.sshCommand "touch $T/ran-ssh"
echo left >"$ws/left-over.txt"
echo '{"pipelineState":{"status":"merged"}}' >"$ws/.pi/work-state/9.json"
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(find "$T" -maxdepth 1 -name 'ran-*' | wc -l)" 0
t_eq "$(find "$ws/left-over.txt" "$ws/.pi" "$ws/.git/hooks/post-checkout" 2>/dev/null | wc -l)" 0
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
(exec 9>"$lockf" && flock 9 && exec sleep 30) &
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
t_contains "$(cycle_args | tr '\n' ' ')" "pi --mode rpc --approve"

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

t_case "the cycle's status line is published as the console's mask leaves it"
echo 'echo "{\"statusText\":\"step 3 of the cycle\"}"' >"$T/container.sh"
t_stub console 'if [ "$1" = --mask ]; then sed "s/cycle/[masked]/"; fi'
mv "$T/bin/console" "$tree/libexec/console"
t_run "$work 7"
t_eq "$(jq -r .last.statusText "$T/home/tehas/live/running.json")" "step 3 of the [masked]"

# Once Pi has gone nobody reads the fifo, and a plain write to it ends the
# command by SIGPIPE (exit 141) before it reports. The race cannot be timed in
# a test, so the shape of the one write is pinned.
t_case "every line to Pi goes through send, which ignores SIGPIPE in a subshell"
for f in work plan; do
  t_eq "$(grep -c '>&4' "$T_ROOT/libexec/$f")" 1
  t_contains "$(grep '>&4' "$T_ROOT/libexec/$f")" "(trap '' PIPE;"
done

t_case "without a console the status line is not published"
mv "$tree/libexec/console" "$T/console.kept"
t_run "$work 7"
t_eq "$(jq -r '.last.statusText, .last.status' "$T/home/tehas/live/running.json" | tr '\n' ,)" ",running,"
t_stub console
mv "$T/bin/console" "$tree/libexec/console"
rm -f "$T/container.sh"

live=$T/home/tehas/live
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
t_run "$work 7"
t_eq "$(jq -c '[(.cycles | length), .last.issue]' "$live/running.json")" "[0,7]"
rm -rf "$live/cycles"

t_case "a cycle that is stopped is recorded as interrupted"
# The cycle's call stays alive until docker stop is called, as a container does.
echo 'sleep 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid"; wait' >"$T/container.sh"
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

# --- A container that is killed ----------------------------------------------
# The command sleeps ten seconds between two looks. Here a look costs a tenth
# of a second. What the container does uses the real sleep, $nap.
t_stub sleep '"$(command -pv sleep)" 0.1'
state_file=$ws/.pi/work-state/7.json
# container <what it does>: the cycle's container. "says <status>" writes the
# cycle's state file, a second on, so that it is newer than the cycle's start.
container() {
  printf '%s\n' 'nap=$(command -pv sleep)' "state=$state_file" \
    'says() { "$nap" 1.2; mkdir -p "${state%/*}"; echo "{\"pipelineState\":{\"status\":\"$1\",\"currentStep\":\"develop\"}}" >"$state"; }' \
    "$1" >"$T/container.sh"
}
# Alive until docker stop is called, then gone with 137, as a container is.
hold='"$nap" 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid"'
gone='wait; exit 137'
fresh() { rm -rf "$T"/calls/* "$live/cycles" "$T/sleeper.pid"; }

t_case "a container that exits 137 while the state says running is reported as killed and the command exits 137"
fresh
container 'says running; exit 137'
t_run "$work 7"
t_eq "$t_rc" 137
t_contains "$t_out" "tehas work: the container was killed (137) while the cycle was running: out of memory, or stopped from outside"
t_contains "$t_out" "status=killed"
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"killed"]'
t_lacks "$(t_calls docker)" "stop "

t_case "a container that exits 137 before it wrote a state is killed too"
fresh
container 'exit 137'
t_run "$work 7"
t_eq "$t_rc" 137
t_contains "$t_out" "status=killed"

t_case "a 137 after the state says handoff is a handoff, exit 0"
fresh
container "$hold; says handoff; $gone"
t_run "$work 7"
t_eq "$t_rc" 0
t_contains "$t_out" "status=handoff"
t_lacks "$t_out" "was killed"
t_contains "$(t_calls docker)" "stop -t 20 tehas-work-7"
# The state turns handoff after the loop read it, and the container is gone
# before the loop's next look: the command has to read the state once more
# before it calls the cycle killed. The loop's time check is the one call
# between its read of the state and its next look, so a date stub makes the
# window: the first time the loop has read the state's age (the stat stub
# notes that), date writes the handoff, ends the container and waits until
# it is gone, then tells the time as the real date does.
fresh
container "says running; $hold; $gone"
t_stub stat '[ "$*" != "-c %Y '"$state_file"'" ] || : >"$T/state.read"
exec "$(command -pv stat)" "$@"'
t_stub date 'if [ -e "$T/state.read" ] && [ ! -e "$T/window.done" ]; then
  : >"$T/window.done"
  echo "{\"pipelineState\":{\"status\":\"handoff\",\"currentStep\":\"develop\"}}" >"'"$state_file"'"
  kill "$(cat "$T/sleeper.pid")" 2>/dev/null
  f=$(grep -l -x -e --mode "$T"/calls/docker.args.*); dpid=${f##*.}
  for _ in $(seq 100); do kill -0 "$dpid" 2>/dev/null || break; "$(command -pv sleep)" 0.1; done
fi
exec "$(command -pv date)" "$@"'
t_run "$work 7"
rm -f "$T/bin/stat" "$T/bin/date" "$T/state.read" "$T/window.done"
t_eq "$t_rc" 0
t_contains "$t_out" "status=handoff"
t_lacks "$t_out" "was killed"
t_lacks "$(t_calls docker)" "stop "

t_case "closing the session ends Pi's input: a container that ends on it is not stopped"
fresh
container 'says handoff; cat >/dev/null; : >"$T/calls/eof"; exit 0'
t_run "$work 7"
t_eq "$t_rc" 0
t_contains "$t_out" "status=handoff"
t_eq "$(find "$T/calls/eof" 2>/dev/null | wc -l)" 1
t_lacks "$(t_calls docker)" "stop "

t_case "the container's input is the fifo's read end only, and the console holds neither end"
fresh
container 'ls -l /proc/$$/fd/ >"$T/calls/container.fds" 2>&1; says handoff; cat >/dev/null'
t_stub console '[ "$1" = --mask ] || ls -l /proc/$$/fd/ >>"$T/calls/console.fds" 2>&1'
mv "$T/bin/console" "$tree/libexec/console"
t_run "$work 7"
t_eq "$t_rc" 0
fifo_path=$(sed -n 's/.* 0 -> \(.*\)$/\1/p' "$T/calls/container.fds")
t_contains "$fifo_path" "(deleted)"
t_eq "$(grep -c -F -e "-> $fifo_path" "$T/calls/container.fds")" 1
t_contains "$(grep ' 0 -> ' "$T/calls/container.fds")" "lr-x"
t_eq "$(grep -c -F -e "${fifo_path% (deleted)}" "$T/calls/console.fds")" 0
t_stub console
mv "$T/bin/console" "$tree/libexec/console"

t_case "a 137 after the state says aborted is aborted, exit 1"
fresh
container 'says aborted; exit 137'
t_run "$work 7"
t_eq "$t_rc" 1
t_lacks "$t_out" "was killed"

t_case "a state that says killed is the container's word, not a kill: exit 2 and no kill line"
# The kill is what this command saw of the container, never what the state
# file says: the agent in the container writes that file.
fresh
container "$hold; says killed; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "status=killed"
t_lacks "$t_out" "was killed"
t_contains "$(t_calls docker)" "stop -t 20 tehas-work-7"

t_case "our own stop at the time limit is not reported as killed"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=1
container "$hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 1s passed"
t_contains "$t_out" "status=running"
t_lacks "$t_out" "was killed"
t_contains "$(t_calls docker)" "stop -t 20 tehas-work-7"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "a cycle none of whose logs change is stopped as stalled before its time limit"
fresh
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
t_eq "$(cat "$T/home/tehas/state/cycles/7.stalled")" 2
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"stalled"]'

t_case "a cycle whose agents keep writing is not stalled"
fresh
rm -f "$T/home/tehas/state/cycles/7.stalled"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_STALL=2 TEHAS_VERIFY_TIMEOUT=1 TEHAS_WORK_TIMEOUT=5
container "( for _ in \$(seq 40); do date >>\"$runs/agent.jsonl\"; \"\$nap\" 0.3; done ) & $hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 5s passed"
t_lacks "$t_out" "stalled"
t_eq "$(find "$T/home/tehas/state/cycles/7.stalled" 2>/dev/null | wc -l)" 0
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "a Pi session that ended while the cycle runs stops it at once as interrupted"
fresh
rm -f "$T/home/tehas/state/cycles/7.ended"
# What pi-rukas wrote to the error stream when Pi quit on a real host. The
# container then idles.
container "says running; echo '[pi-rukas] session_shutdown (reason: quit): aborted 2 in-flight async jobs' >&2; $hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "tehas work: the Pi session ended while the cycle was running, stopping it"
t_contains "$t_out" "status=interrupted step=develop"
t_lacks "$t_out" "time limit"
t_lacks "$t_out" "was killed"
t_contains "$(t_calls docker)" "stop -t 20 tehas-work-7"
t_eq "$(find "$T/home/tehas/state/cycles/7.ended" -type f 2>/dev/null | wc -l)" 1
t_eq "$(jq -c '[.running, .status]' "$live/cycles/7.json")" '[false,"interrupted"]'

t_case "a session_shutdown that is not at the start of a line ends nothing, and no ended file is left"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=2
container "says running; echo 'a tool said [pi-rukas] session_shutdown' >&2; $hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 2s passed"
t_lacks "$t_out" "session ended"
t_eq "$(find "$T/home/tehas/state/cycles/7.ended" 2>/dev/null | wc -l)" 0
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "TERM still gives interrupted"
fresh
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
run_at=$(t_calls docker | grep -n -e ' --mode ' | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$rm_at" ] && [ -n "$run_at" ] && [ "$rm_at" -lt "$run_at" ] && echo before)" before

t_case "the image's id and the pi-rukas commit of its label are on record, read before the container starts"
fresh
image_id=sha256:$(printf 'e%.0s' {1..64})
echo "$image_id 0123456789abcdef0123456789abcdef01234567" >"$T/image.inspect"
container 'says handoff'
t_run "$work 7"
t_eq "$(cat "$T/home/tehas/state/cycles/7.image")" "$image_id 0123456789abcdef0123456789abcdef01234567"
t_contains "$(t_calls docker)" "image inspect --format {{.Id}} {{index .Config.Labels \"tehas.rukas\"}} tehas/pi-rukas:uid$(id -u)"
at_inspect=$(t_calls docker | grep -n -e '^image inspect' | cut -d: -f1 | head -n 1)
at_run=$(t_calls docker | grep -n -e ' --mode ' | cut -d: -f1 | head -n 1)
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

t_case "a docker stop that does not end is given up after its bound, said, and the container killed"
fresh
: >"$T/stop-hangs"
T_ENV=("${env_ok[@]}" TEHAS_STOP_WAIT=1)
container "$hold; says handoff; $gone"
t_run "$work 7"
rm -f "$T/stop-hangs"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas work: docker stop of tehas-work-7 did not end within 1s, killing it"
t_contains "$(t_calls docker)" "kill tehas-work-7"
t_contains "$t_out" "status=handoff"

t_case "a stop that ends in time kills nothing"
fresh
container "$hold; says handoff; $gone"
t_run "$work 7"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "kill "
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
rm -f "$T/bin/sleep" "$T/container.sh"

# --- Without a usable slice ----------------------------------------------------
# refused: 78, and neither a clone nor a container was made.
refused() {
  t_eq "$t_rc" 78
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_lacks "$(t_calls docker)" "run "
  t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" 2>/dev/null | wc -l)" 0
}
without() { rm -rf "$T"/calls/* "$T/home/tehas/work" "$T/home/tehas/logs"; slice_good; }

t_case "without the slice no container starts"
without
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$work 7"
refused
t_contains "$t_out" "tehas work: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver is refused the same way"
without
echo "cgroupfs 2" >"$T/docker.info"
t_run "$work 7"
refused
t_contains "$t_out" "tehas work: Docker runs containers with the cgroupfs cgroup driver on cgroup v2, tehas needs systemd on v2"

t_case "cgroup v1 is refused"
without
echo "systemd 1" >"$T/docker.info"
t_run "$work 7"
refused
t_contains "$t_out" "on cgroup v1, tehas needs systemd on v2"

t_case "a systemctl that answers without MemoryMax starts no container"
without
printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active >"$T/slice.show"
t_run "$work 7"
refused
t_contains "$t_out" "tehas work: tehas.slice has no memory limit"

t_case "a systemctl that fails starts no container"
without
t_stub systemctl 'exit 1'
t_run "$work 7"
refused
t_contains "$t_out" "tehas work: systemctl gave no answer"
t_stub systemctl 'cat "$T/slice.show"'

t_case "a cgroup tree without the controllers starts no container"
without
echo "cpuset io" >"$T_CGROUP/cgroup.controllers"
t_run "$work 7"
refused
t_contains "$t_out" "cgroup.controllers does not list cpu, memory and pids"

t_case "a reserve above the ceiling is refused and names both numbers and the smallest host"
without
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=3g
t_run "$work 7"
refused
t_contains "$t_out" "tehas work: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m, a quarter of tehas.slice's MemoryMax); the host needs about 16 GiB"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "the twelve words are all present or nothing starts"
# The command beside a slice library that says yes and hands out a short list,
# or a list that names another parent.
short=$T/short
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
ln -s "$T_ROOT/libexec/work" "$short/libexec/work"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --pids-limit 4096 --cpu-shares 1024" \
  "--memory 2048m --cgroup-parent tehas.slice --memory-swap 2048m --memory-reservation 2g --pids-limit 4096 --cpu-shares 1024" ""; do
  without
  printf '%s\n' 'tehas_slice_ok() { return 0; }' "tehas_slice_limits() { TEHAS_SLICE_LIMITS=($words); }" >"$short/lib/slice.sh"
  t_run "$short/libexec/work 7"
  refused
  t_contains "$t_out" "tehas work: the container limits could not be made, not starting"
done
without

t_case "a checkout of another repository starts no container"
rm -f "$T"/calls/*
git -C "$checkout" remote set-url origin https://github.com/acme/other.git
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "not of acme/widget"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
git -C "$checkout" remote set-url origin https://github.com/acme/widget.git

t_case "a missing token starts no container"
rm -f "$T"/calls/*
T_ENV=("TEHAS_CGROUP_ROOT=$T_CGROUP")
t_run "$work 7"
t_eq "$t_rc" 64
t_contains "$t_out" "GH_TOKEN is not set"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

# --- The issue's images ------------------------------------------------------
# gh answers for the issue and its events from files: no events file is an
# issue nobody labelled. curl hands out $T/fx/net/<address, with _ for every
# other character>; <name>.rc is an exit status to fail with. What arrives on
# standard input (the header) is kept.
fx=$T/fx
A=https://github.com/user-attachments/assets
F=https://github.com/user-attachments/files
imgs=$ws.images
t_stub gh '
case "$1 $2" in
  "api repos/acme/widget/issues/7") cat "$T/fx/issue.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/issues/7/events?per_page=100")
    [ ! -e "$T/fx/events-fail" ] || exit 1
    cat "$T/fx/events.json" 2>/dev/null || echo "[]" ;;
  "issue comment") [ ! -e "$T/fx/comment-fails" ] || exit 1 ;;
esac'
t_stub curl '
out="" url="" max=0 header=0
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift ;;
    --max-filesize) max=$2; shift ;;
    -H) header=1; shift ;;
    https://*) url=$1 ;;
  esac
  shift
done
key=$(printf %s "$url" | tr -c "A-Za-z0-9" _)
[ "$header" = 1 ] && { printf "%s " "$url"; cat; } >>"$T/calls/curl-headers"
[ -f "$T/fx/net/$key.rc" ] && exit "$(cat "$T/fx/net/$key.rc")"
[ -f "$T/fx/net/$key" ] || exit 22
[ "$(stat -c %s "$T/fx/net/$key")" -le "$max" ] || exit 63
cp "$T/fx/net/$key" "$out"
printf application/octet-stream
exit 0'
# net <address> <printf format>: something GitHub would hand out.
net() {
  local key
  key=$(printf %s "$1" | tr -c 'A-Za-z0-9' _)
  # shellcheck disable=SC2059  # the format carries the bytes of an image
  printf "$2" >"$fx/net/$key"
}
png='\x89PNG\r\n\x1a\nPNGDATA'
jpg='\xff\xd8\xff\xe0JPGDATA'
gif='GIF89aGIFDATA'
webp='RIFF\x10\x00\x00\x00WEBPVP8 '
# issue_is <body> [association]
issue_is() {
  jq -n --arg body "$1" --arg who "${2:-OWNER}" '{number: 7, body: $body, author_association: $who}' >"$fx/issue.json"
}
images_on() {
  rm -rf "$fx" "$T"/calls/* "$T/home/tehas/work"
  mkdir -p "$fx/net"
  slice_good
  reads_image 1
  T_ENV=("${env_ok[@]}")
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 "$@"
}
mounts_off="$ws:$inside
$runs:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro"
manifest() { jq -c "$1" "$imgs/manifest.json" 2>&1; }
# What the sandbox image says about its own pi-rukas, as host/sandbox/build.sh
# writes it: <commit>|<reads>. reads_image says it for one case.
img_commit=$(printf 'c%.0s' {1..40})
reads_image() { # <1, 0, or none when the image carries no label>
  rm -f "$T/image.label"
  if [ "$1" != none ]; then printf '%s\n' "$img_commit|$1" >"$T/image.label"; fi
}

t_case "with images off GitHub is not asked, nothing is fetched and no directory is made"
images_on
t_instance "${T_MINIMAL[@]}"
issue_is "![shot]($A/1111)"
net "$A/1111" "$png"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_eq "$(find "$imgs" -maxdepth 0 2>/dev/null | wc -l)" 0
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "a setting that is not 0 or 1 starts nothing and asks nobody"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=yes
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls curl)$(t_calls docker)" ""
t_eq "$(find "$T/home/tehas/work" 2>/dev/null | wc -l)" 0

t_case "with images on the cycle gets one more mount, read-only, and one more variable"
images_on
net "$A/1111" "$png"
net "$F/21/side.JPG" "$jpg"
net "$A/2222" '<svg xmlns="http://www.w3.org/2000/svg"/>'
net "$F/24/drawing.svg" '<svg/>'
net "$A/3333" "$gif"
net "https://user-images.githubusercontent.com/9/old.webp" "$webp"
issue_is "Look: ![shot]($A/1111) and again ![shot]($A/1111)
[side.JPG]($F/21/side.JPG) [all.zip]($F/23/all.zip) [drawing.svg]($F/24/drawing.svg)
<img src=\"$A/2222\"> ![anim]($A/3333) https://user-images.githubusercontent.com/9/old.webp
and https://example.com/not-ours.png"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" "$imgs:$inside.images:ro" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_eq "$(cycle_args | unknown_options "$image" \
  "--name --cgroup-parent --memory --memory-swap --memory-reservation --pids-limit --cpu-shares --cidfile -w -v -e" "--rm -i --init")" ""

t_case "the push check's container is given no image and not the variable"
t_eq "$(preflight_args | awk 'prev == "-v" { print } { prev = $0 }')" "$ws:$inside"
t_eq "$(preflight_args | awk 'prev == "-e" { print } { prev = $0 }' | sed 's/=.*//' | LC_ALL=C sort)" \
  "$(printf '%s\n' "$want" HOME | LC_ALL=C sort)"

t_case "nothing is made on the host at the images' path inside the container"
t_eq "$(find "$inside.images" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "png, jpg, gif and webp are taken in the order written, under names of tehas's own"
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./image-1.png ./image-2.jpg ./image-3.gif ./image-4.webp ./manifest.json "
t_eq "$(manifest '.issue')" 7
t_eq "$(manifest '.images | map([.file, .link, .type, .bytes])')" \
  "[[\"image-1.png\",\"$A/1111\",\"image/png\",15],[\"image-2.jpg\",\"$F/21/side.JPG\",\"image/jpeg\",11],[\"image-3.gif\",\"$A/3333\",\"image/gif\",13],[\"image-4.webp\",\"https://user-images.githubusercontent.com/9/old.webp\",\"image/webp\",16]]"
t_eq "$(cmp "$imgs/image-1.png" "$fx/net/$(printf %s "$A/1111" | tr -c 'A-Za-z0-9' _)" && echo same)" same

t_case "another type is left out and said, and one known by its name is not fetched"
t_eq "$(manifest '.leftOut | map([.link, .reason])')" \
  "[[\"$F/23/all.zip\",\"Not a png, jpg, webp or gif image.\"],[\"$F/24/drawing.svg\",\"Not a png, jpg, webp or gif image.\"],[\"$A/2222\",\"Not a png, jpg, webp or gif image.\"]]"
t_lacks "$(t_calls curl)" "all.zip"
t_lacks "$(t_calls curl)" "drawing.svg"
t_lacks "$(t_calls curl)" "example.com"

t_case "the token goes to github.com on standard input, never on the command line"
t_lacks "$(t_calls curl)" "test-token"
t_contains "$(cat "$T/calls/curl-headers")" "$A/1111 Authorization: Bearer test-token"
t_lacks "$(cat "$T/calls/curl-headers")" "githubusercontent"

t_case "the issue is told how many images the cycle was given and what was left out"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas gave this cycle 4 images from the issue text."
t_contains "$(t_calls gh)" "- $F/23/all.zip: Not a png, jpg, webp or gif image."

t_case "the limit counts all images together: each fetch may take what is left"
t_eq "$(t_calls curl | grep -o -e '--max-filesize [0-9]*' | tr '\n' ' ')" \
  "--max-filesize 10485760 --max-filesize 10485745 --max-filesize 10485734 --max-filesize 10485734 --max-filesize 10485721 "

t_case "an image the sandbox image does not read is said as fetched and not shown, and still mounted"
images_on
reads_image 0
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(after -v)" "$imgs:$inside.images:ro"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_lacks "$(t_calls gh)" "Tehas gave this cycle"
t_contains "$(t_calls gh)" "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this cycle. The pi-rukas in the sandbox image does not read them, so no agent was shown one."

# An image built before the label existed is not proof of anything, and tehas
# does not claim one way or the other.
t_case "a sandbox image that says nothing is not claimed to have shown the images"
images_on
reads_image none
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$(t_calls gh)" "Tehas gave this cycle"
t_contains "$(t_calls gh)" "The sandbox image does not say whether its pi-rukas reads them: rebuild it with $T/home/tehas/sandbox/build.sh, which records the answer."

t_case "a sandbox image that reads them is said to have given them"
images_on
reads_image 1
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas gave this cycle 1 image from the issue text."

t_case "bytes that are not the image the name says are left out"
images_on
net "$F/21/shot.png" '<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>'
net "$F/22/empty.png" ''
issue_is "[shot.png]($F/21/shot.png) [empty.png]($F/22/empty.png)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[(.images | length), (.leftOut | map(.reason))]')" '[0,["Not a png, jpg, webp or gif image.","Not a png, jpg, webp or gif image."]]'
t_eq "$(cd "$imgs" && find . -mindepth 1 | tr '\n' ' ')" "./manifest.json "

t_case "an issue with no image still gets the mount, with an empty list, and no comment"
images_on
issue_is "Words only."
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[.images, .leftOut]')" '[[],[]]'
t_contains "$(after -v)" "$imgs:$inside.images:ro"
t_lacks "$(t_calls gh)" "issue comment"

t_case "an issue by someone outside the repository is given no image, and nothing is fetched"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)" NONE
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(manifest '[.images, (.leftOut | map([.link, .reason]))]')" \
  "[[],[[\"$A/1111\",\"The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account.\"]]]"
t_contains "$(cycle_args | tr '\n' ' ')" "pi --mode rpc --approve"

# labelled_by <login>: the account that applied tehas:ready last.
labelled_by() {
  jq -n --arg who "$1" '[{event: "labeled", label: {name: "tehas:plan"}, actor: {login: "alice"}, created_at: "2026-10-12T00:00:00Z"},
    {event: "labeled", label: {name: "tehas:ready"}, actor: {login: $who}, created_at: "2026-10-11T00:00:00Z"}]' >"$fx/events.json"
}

t_case "an outsider's issue that an allowed account labelled is given its images"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)" NONE
labelled_by alice
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[(.images | map(.link)), .leftOut]')" "[[\"$A/1111\"],[]]"
t_contains "$(after -v)" "$imgs:$inside.images:ro"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/events?per_page=100 --paginate"

t_case "one labelled last by an account outside the list is not, and nothing is fetched"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)" NONE
labelled_by mallory
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(manifest '.images')" "[]"

t_case "events that cannot be read start no cycle: exit 4, said on the issue"
images_on
issue_is "![shot]($A/1111)" NONE
: >"$fx/events-fail"
t_run "$work 7"
t_eq "$t_rc" 4
t_contains "$t_out" "tehas work: the issue's label events could not be read for its images, not starting a cycle"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(t_calls curl)" ""
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas started no cycle: the issue's label events could not be read"

t_case "at most eight images: the ninth is left out and not fetched"
images_on
body=""
for i in 1 2 3 4 5 6 7 8 9; do
  net "$A/000$i" "$png"
  body="$body ![s]($A/000$i)"
done
issue_is "$body"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[(.images | length), (.leftOut | map([.link, .reason]))]')" "[8,[[\"$A/0009\",\"More than 8 images.\"]]]"
t_lacks "$(t_calls curl)" "$A/0009"

t_case "an image over what is left of the limit is left out, and the cycle starts"
images_on
net "$A/1111" "$png"
echo 63 >"$fx/net/$(printf %s "$A/1111" | tr -c 'A-Za-z0-9' _).rc"
issue_is "![big]($A/1111)"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '.leftOut | map(.reason)')" '["Over the 10 MB limit of an issue."]'
t_contains "$(cycle_args | tr '\n' ' ')" "pi --mode rpc --approve"

for rc in 22 6 28; do
  t_case "an image that cannot be fetched (curl $rc) starts no cycle: exit 4, said on the issue"
  images_on
  net "$A/1111" "$png"
  net "$A/2222" "$png"
  echo "$rc" >"$fx/net/$(printf %s "$A/2222" | tr -c 'A-Za-z0-9' _).rc"
  issue_is "![a]($A/1111) ![b]($A/2222)"
  t_run "$work 7"
  t_eq "$t_rc" 4
  t_contains "$t_out" "tehas work: $A/2222 could not be fetched, not starting a cycle"
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_eq "$(find "$imgs" "$ws" -maxdepth 0 2>/dev/null | wc -l)" 0
  t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas started no cycle: $A/2222 could not be fetched"
done

t_case "an issue GitHub does not hand out starts no cycle, and what the last cycle left stays where it is"
images_on
mkdir -p "$T/home/tehas/work" "$ws"
: >"$T/home/tehas/work/.tehas-workspaces"
echo unpushed >"$ws/kept.txt"
t_run "$work 7"
t_eq "$t_rc" 4
t_eq "$(cat "$ws/kept.txt")" unpushed
t_eq "$(find "$ws.before-retry" -maxdepth 0 2>/dev/null | wc -l)" 0
t_contains "$t_out" "tehas work: the issue could not be read for its images, not starting a cycle"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
echo "not json" >"$fx/issue.json"
t_run "$work 7"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_case "a comment that cannot be posted does not stop the cycle"
images_on
net "$A/1111" "$png"
issue_is "![shot]($A/1111)"
: >"$fx/comment-fails"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$(cycle_args | tr '\n' ' ')" "pi --mode rpc --approve"

t_case "the images are remade for every cycle: what an earlier one left is gone"
images_on
mkdir -p "$T/home/tehas/work" "$imgs"
: >"$T/home/tehas/work/.tehas-workspaces"
echo old >"$imgs/image-9.png"
issue_is "Words only."
t_run "$work 7"
t_eq "$(cd "$imgs" && find . -mindepth 1 | tr '\n' ' ')" "./manifest.json "

t_case "a path of the instance's own for the variable is replaced, not added to"
images_on PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere
issue_is "Words only."
t_run "$work 7"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 1
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_lacks "$(cat "$T"/calls/docker.args.*)" "/elsewhere"

t_case "a cycle the push check stopped leaves no copy of the index beside its workspace"
images_on
issue_is "Words only."
rm -rf "$hostindex"
mkdir -p "$hostindex"
echo index >"$hostindex/widget.db"
T_ENV=(PUSH_RC=1 "${env_ok[@]}")
t_run "$work 7"
t_eq "$t_rc" 3
t_contains "$t_out" "cannot push to origin"
t_eq "$(find "$ws" -maxdepth 0 2>/dev/null | wc -l)" 1
t_eq "$(find "$state" -maxdepth 0 2>/dev/null | wc -l)" 0
T_ENV=("${env_ok[@]}")

t_done
