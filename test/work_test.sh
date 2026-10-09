#!/usr/bin/env bash
# Tests for bin/tehas-work: what the cycle's container is given. A stub docker
# records its arguments, one per line, and exits at once, so no cycle runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

work="$T_ROOT/bin/tehas-work"
checkout=$T/home/tehas/repos/widget
git init -q "$checkout"
git -C "$checkout" remote add origin https://github.com/acme/widget.git

t_stub docker 'printf "%s\n" "$@" >"$T/calls/docker.args.$$"'
t_stub deno
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"

# The arguments of the docker call that starts the cycle.
cycle_args() { grep -l -x -e '--mode' "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
# Values that follow a flag, e.g. every -v or every -e.
after() { cycle_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }

t_case "an invalid instance starts no container"
t_instance TEHAS_CONTROL_ISSUE=1
T_ENV=(GH_TOKEN=test-token)
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""

t_case "the cycle gets the limits from the settings"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_CPUS=1 TEHAS_WORK_MEMORY=512m PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
t_run "$work 7"
t_eq "$t_rc" 2 # the stub ends at once, so the cycle counts as died
t_eq "$(after --cpus)" "1"
t_eq "$(after --memory)" "512m"

t_case "the image and the working directory come from the settings"
t_contains "$(cycle_args | tr '\n' ' ')" "tehas/pi-rukas:uid$(id -u) pi --mode rpc --approve"
t_eq "$(after -w)" "$checkout"

t_case "exactly the documented paths are mounted"
want="$checkout:$checkout
$T/home/.pi/agent/ensemble-runs:/home/vscode/.pi/agent/ensemble-runs
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/sessions:/home/vscode/.pi/agent/sessions
$T/home/.vipune:/home/vscode/.vipune
pi-ensemble-bun-cache:/home/vscode/.bun
pi-ensemble-cache:/home/vscode/.cache"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

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
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_NO_DOCKER_SOCKET
PI_ENSEMBLE_NO_SSH
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_WORKSPACE_ROOT"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

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
T_ENV=()
t_run "$work 7"
t_eq "$t_rc" 64
t_contains "$t_out" "GH_TOKEN is not set"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_done
