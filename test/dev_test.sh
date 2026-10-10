#!/usr/bin/env bash
# Tests for libexec/dev: the clone a manual session works in, whose name its
# commits carry, and what its container is given. A stub docker records its
# arguments, one per line, and exits at once, so no session runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

dev="$T_ROOT/libexec/dev"
checkout=$T/home/tehas/repos/widget
devs=$T/home/tehas/dev
clone=$devs/main
# The product repository, and the host's checkout of it.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

t_stub docker 'printf "%s\n" "$@" >"$T/calls/docker.args.$$"'
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"

# The arguments of the docker call that starts the session.
session_args() { cat "$T"/calls/docker.args.* 2>/dev/null; }
containers() { find "$T/calls" -name 'docker.args.*' | wc -l; }
# Values that follow a flag, e.g. every -v or every -e.
after() { session_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
# The value of one variable passed as NAME=value.
passed() { after -e | sed -n "s/^$1=//p"; }
# Every option that is not one this command is known to pass, up to the image
# name, read the way docker reads them. See test/work_test.sh.
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
again() { rm -f "$T"/calls/*; }

t_case "--help prints the usage with no instance and starts nothing"
T_ENV=()
t_run "$dev --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage: tehas dev"
t_eq "$(containers)" 0

t_case "an unknown argument starts no container and makes no clone"
t_instance "${T_MINIMAL[@]}"
T_ENV=(GH_TOKEN=test-token)
t_run "$dev --resume"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --resume"
t_run "$dev main other"
t_eq "$t_rc" 64
t_eq "$(containers)" 0
t_eq "$([ -e "$devs" ] && echo there)" ""

t_case "a name that is not one plain word is refused"
for bad in ../repos/widget a/b a.author -x ''; do
  t_run "$dev '$bad'"
  t_eq "$t_rc" 64
done
t_eq "$(containers)" 0
t_eq "$([ -e "$devs" ] && echo there)" ""

t_case "a malformed author is refused and not stored"
for bad in 'Alice' 'Alice alice@example.com' '<alice@example.com>' 'Alice <alice>' 'A <a@b> <c@d>' ''; do
  t_run "$dev main --author '$bad'"
  t_eq "$t_rc" 64
done
t_run "$dev main --author"
t_eq "$t_rc" 64
t_eq "$(containers)" 0
t_eq "$([ -e "$devs" ] && echo there)" ""

t_case "an invalid instance starts no container"
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$dev"
t_eq "$t_rc" 78
t_eq "$(containers)" 0
t_eq "$([ -e "$devs" ] && echo there)" ""

t_case "a missing token starts no container"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_CPUS=1 TEHAS_WORK_MEMORY=512m PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=()
t_run "$dev"
t_eq "$t_rc" 64
t_contains "$t_out" "GH_TOKEN is not set"
t_eq "$(containers)" 0
t_eq "$([ -e "$devs" ] && echo there)" ""

t_case "a checkout of another repository starts no container"
T_ENV=(GH_TOKEN=test-token TERM=xterm-256color COLORTERM=truecolor SECRET_THING=leak)
git -C "$checkout" remote set-url origin https://github.com/acme/other.git
t_run "$dev"
t_eq "$t_rc" 78
t_contains "$t_out" "not of acme/widget"
t_eq "$(containers)" 0
git -C "$checkout" remote set-url origin "$upstream"

t_case "the first run makes the clone at the branch's tip, with the product's origin"
t_run "$dev"
t_eq "$t_rc" 0
t_eq "$(containers)" 1
t_eq "$(git -C "$clone" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_eq "$(git -C "$clone" branch --show-current)" main
t_eq "$(git -C "$clone" config --get remote.origin.url)" "$(git -C "$checkout" config --get remote.origin.url)"
t_eq "$(git -C "$clone" symbolic-ref --short refs/remotes/origin/HEAD)" origin/main

t_case "without an author the bot's identity is used, and the command says so"
t_contains "$t_out" "Test Bot"
t_eq "$(passed GIT_CONFIG_VALUE_2)" "Test Bot"
t_eq "$(passed GIT_CONFIG_VALUE_3)" "bot@example.com"

t_case "the session gets the limits, the image and the clone as its directory"
image="tehas/pi-rukas:uid$(id -u)"
t_eq "$(after --cpus)" "1"
t_eq "$(after --memory)" "512m"
t_eq "$(after -w)" "$clone"
t_eq "$(after --name)" "tehas-dev-main"
t_eq "$(session_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' ')" "$image pi "

t_case "exactly a cycle's mounts, with the clone in place of the workspace"
want="$clone:$clone
$T/home/.pi/agent/ensemble-runs:/home/vscode/.pi/agent/ensemble-runs
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/sessions:/home/vscode/.pi/agent/sessions
$T/home/.vipune:/home/vscode/.vipune
pi-ensemble-bun-cache:/home/vscode/.bun
pi-ensemble-cache:/home/vscode/.cache"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "exactly a cycle's variables, plus the terminal's two"
want="COLORTERM
GH_TOKEN
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
PI_ENSEMBLE_WORKSPACE_ROOT
TERM"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"
t_eq "$(passed PI_ENSEMBLE_WORKSPACE_ROOT)" "$clone"

t_case "the session's container gets no option beyond the documented ones"
t_eq "$(session_args | unknown_options "$image" "--name --cpus --memory -w -v -e" "--rm -it")" ""
t_contains "$(session_args)" "--add-host=models:192.0.2.9"

t_case "the token and the rest of the environment are not written into the arguments"
t_lacks "$(session_args)" "test-token"
t_lacks "$(session_args)" "leak"
t_lacks "$(session_args)" "xterm-256color"

t_case "nothing that controls the host is reachable"
args=$(session_args)
t_lacks "$args" "$checkout"
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config"
t_lacks "$args" "--privileged"

t_case "a later run keeps what is in the clone, and the host runs none of it"
# Everything an agent could leave to be run later, and a person's own work.
head_before=$(git -C "$clone" rev-parse HEAD)
echo mine >"$clone/in-progress.txt"
mkdir -p "$clone/.git/hooks"
printf '#!/bin/sh\ntouch "%s"\n' "$T/ran-hook" >"$clone/.git/hooks/post-checkout"
cp "$clone/.git/hooks/post-checkout" "$clone/.git/hooks/reference-transaction"
chmod +x "$clone/.git/hooks/post-checkout" "$clone/.git/hooks/reference-transaction"
printf '[core]\n\tfsmonitor = touch %s; false\n\tsshCommand = touch %s\n' "$T/ran-fsmonitor" "$T/ran-ssh" >>"$clone/.git/config"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" fetch -q origin
again
t_run "$dev"
t_eq "$t_rc" 0
t_eq "$(containers)" 1
t_eq "$(cat "$clone/in-progress.txt")" mine
t_eq "$(find "$T" -maxdepth 1 -name 'ran-*' | wc -l)" 0
t_eq "$(GIT_CONFIG_NOSYSTEM=1 git -C "$clone" -c core.fsmonitor=false rev-parse HEAD)" "$head_before"
rm -f "$T"/ran-*

t_case "an author is stored next to the clone, not in it, and used"
again
t_run "$dev main --author 'Alice Example <alice@example.com>'"
t_eq "$t_rc" 0
t_eq "$(cat "$devs/main.author")" "Alice Example <alice@example.com>"
t_eq "$(passed GIT_CONFIG_VALUE_2)" "Alice Example"
t_eq "$(passed GIT_CONFIG_VALUE_3)" "alice@example.com"
t_lacks "$t_out" "Test Bot"
t_eq "$(find "$clone" -name '*.author' | wc -l)" 0

t_case "the stored author is used on the next run without the option"
again
t_run "$dev"
t_eq "$(passed GIT_CONFIG_VALUE_2)" "Alice Example"
t_eq "$(passed GIT_CONFIG_VALUE_3)" "alice@example.com"

t_case "a stored author that is not Name <email> starts no container"
echo 'not an author' >"$devs/main.author"
again
t_run "$dev"
t_eq "$t_rc" 64
t_contains "$t_out" "main.author"
t_eq "$(containers)" 0
echo 'Alice Example <alice@example.com>' >"$devs/main.author"

t_case "a second name is a second clone with its own author"
again
t_run "$dev bob"
t_eq "$t_rc" 0
t_eq "$(after -w)" "$devs/bob"
t_eq "$(after --name)" "tehas-dev-bob"
t_eq "$(passed GIT_CONFIG_VALUE_2)" "Test Bot"
t_eq "$(git -C "$devs/bob" rev-parse HEAD)" "$(git -C "$checkout" rev-parse origin/main)"

t_case "a clone that could not be finished is not left to pass for one"
real_git=$(command -v git)
t_stub git "case \" \$* \" in *' fetch '*) exit 1 ;; esac; exec $real_git \"\$@\""
again
t_run "$dev carol"
t_eq "$t_rc" 78
t_contains "$t_out" "could not make the clone"
t_eq "$(containers)" 0
t_eq "$(find "$devs" -mindepth 1 -maxdepth 1 -name '*carol*' | wc -l)" 0
rm -f "$T/bin/git"
again
t_run "$dev carol"
t_eq "$t_rc" 0
t_eq "$(git -C "$devs/carol" rev-parse HEAD)" "$(git -C "$checkout" rev-parse origin/main)"
t_eq "$(find "$devs" -mindepth 1 -maxdepth 1 -name '.*' | wc -l)" 0

t_case "the arguments after -- reach pi"
again
t_run "$dev main -- -r --model 'a b'"
t_eq "$(session_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' '|')" "$image|pi|-r|--model|a b|"
again
t_run "$dev -- -r"
t_eq "$(after -w)" "$clone"
t_eq "$(session_args | tail -n 2 | tr '\n' ' ')" "pi -r "

t_case "a directory that is not a clone is left alone"
mkdir -p "$devs/notes"
echo keep >"$devs/notes/file.txt"
again
t_run "$dev notes"
t_eq "$t_rc" 78
t_contains "$t_out" "not a clone"
t_eq "$(cat "$devs/notes/file.txt")" keep
t_eq "$(containers)" 0

t_case "a running cycle is announced, and the session starts"
mkdir -p "$T/home/tehas/live"
echo '{"cycles":[{"issue":12,"running":true}],"last":null}' >"$T/home/tehas/live/running.json"
again
t_run "$dev"
t_eq "$t_rc" 0
t_contains "$t_out" "a cycle is running on issue 12"
t_eq "$(containers)" 1
echo '{"cycles":[],"last":{"issue":12,"running":false}}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_lacks "$t_out" "issue 12"

t_case "several running cycles are all named"
echo '{"cycles":[{"issue":12,"running":true},{"issue":14,"running":true}],"last":null}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_contains "$t_out" "cycles are running on issues 12, 14"

t_case "the one object an earlier version wrote is still read"
echo '{"issue":12,"running":true}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_contains "$t_out" "a cycle is running on issue 12"
echo '{"issue":12,"running":false}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_lacks "$t_out" "issue 12"

t_case "the session's exit status is passed on"
t_stub docker 'exit 7'
t_run "$dev"
t_eq "$t_rc" 7

t_case "no git identity at all starts no container"
t_stub docker 'printf "%s\n" "$@" >"$T/calls/docker.args.$$"'
: >"$T/home/.gitconfig"
again
t_run "$dev bob"
t_eq "$t_rc" 64
t_contains "$t_out" "user.name"
t_eq "$(containers)" 0

t_done
