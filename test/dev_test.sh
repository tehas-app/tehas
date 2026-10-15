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
# The clone's own state on the host: what this session writes, and nothing a
# cycle wrote.
state=$devs/main.state
# The product repository, and the host's checkout of it.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

docker_stub='if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
if [ "$1" = image ]; then cat "$T/image.label" 2>/dev/null; exit; fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"'
t_stub docker "$docker_stub"
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# Pi's settings as install.sh leaves them on a host.
mkdir -p "$T/home/.pi/agent"
echo '{"retry":{"provider":{"timeoutMs":600000,"maxRetries":8,"maxRetryDelayMs":60000}}}' >"$T/home/.pi/agent/settings.json"
# A host with the root step done: the slice loaded from its file with 8 GiB,
# so a container's ceiling is 2048m, Docker on the systemd driver and cgroup
# v2, and nothing in the slice.
t_stub systemctl 'cat "$T/slice.show"'
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  slice_state 0.00
}
slice_good
limits="--cgroup-parent --memory --memory-swap --memory-reservation --pids-limit --cpu-shares"
root="TEHAS_CGROUP_ROOT=$T_CGROUP"

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
T_ENV=(GH_TOKEN=test-token "$root")
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
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("$root")
t_run "$dev"
t_eq "$t_rc" 64
t_contains "$t_out" "GH_TOKEN is not set"
t_eq "$(containers)" 0
t_eq "$([ -e "$devs" ] && echo there)" ""

t_case "a checkout of another repository starts no container"
T_ENV=(GH_TOKEN=test-token TERM=xterm-256color COLORTERM=truecolor SECRET_THING=leak "$root")
git -C "$checkout" remote set-url origin https://github.com/acme/other.git
t_run "$dev"
t_eq "$t_rc" 78
t_contains "$t_out" "not of acme/widget"
t_eq "$(containers)" 0
git -C "$checkout" remote set-url origin "$upstream"

t_case "the first run makes the clone at the branch's tip, with the product's origin"
again
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

t_case "the session gets the slice and a cycle's limits, the image and the clone as its directory"
image="tehas/pi-rukas:uid$(id -u)"
t_eq "$(after --cgroup-parent)" "tehas.slice"
t_eq "$(after --memory)" "2048m"
t_eq "$(after --memory-swap)" "2048m"
t_eq "$(after --memory-reservation)" "2g"
t_eq "$(after --pids-limit)" "4096"
t_eq "$(after --cpu-shares)" "1024"
t_lacks "$(session_args)" "--cpus"
t_lacks "$(session_args)" "--cidfile"

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

t_case "the session is told how much room the slice has before it starts"
t_contains "$t_out" "tehas dev: tehas.slice has room for 4 more cycles"
t_eq "$(after -w)" "$clone"
t_eq "$(after --name)" "tehas-dev-main"
t_eq "$(session_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' ')" "$image pi "

t_case "exactly a cycle's mounts, with the clone in place of the workspace"
want="$clone:$clone
$state/runs:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the session's state is its own, made beside the clone and kept between sessions"
t_eq "$(find "$state" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "cache memory runs sessions "
t_lacks "$(after -v)" "pi-ensemble"
t_lacks "$(after -v)" "$T/home/.pi/agent/sessions"
t_lacks "$(after -v)" "$T/home/.vipune"
echo mine >"$state/sessions/one.jsonl"
again
t_run "$dev"
t_eq "$(cat "$state/sessions/one.jsonl")" mine

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
CBM_CACHE_DIR
PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_NO_DOCKER_SOCKET
PI_ENSEMBLE_NO_SSH
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT
TERM"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"
t_eq "$(passed PI_ENSEMBLE_WORKSPACE_ROOT)" "$clone"

t_case "the session's container gets no option beyond the documented ones"
t_eq "$(session_args | unknown_options "$image" "--name $limits -w -v -e" "--rm -it --init")" ""
t_eq "$(session_args | grep -c -x -e --init)" 1
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
t_stub docker '[ "$1" != info ] || { cat "$T/docker.info"; exit; }
exit 7'
t_run "$dev"
t_eq "$t_rc" 7
t_stub docker "$docker_stub"
echo '{"cycles":[],"last":null}' >"$T/home/tehas/live/running.json"

t_case "the room counts what is in the slice: another container at its use, a cycle with no container yet at one reserve"
again
slice_child "$(slice_id b)" $((3 * 1073741824)) 0 2147483648
t_run "$dev"
t_contains "$t_out" "tehas dev: tehas.slice has room for 2 more cycles"
mkdir -p "$T/home/tehas/state/cycles"
exec 9>"$T/home/tehas/state/cycles/12.lock"
flock -n 9
t_run "$dev"
t_contains "$t_out" "tehas dev: tehas.slice has room for 1 more cycles"
# The cycle's container is in the slice: it is counted once, at its floor.
slice_id c >"$T/home/tehas/state/cycles/12.cid"
slice_child "$(slice_id c)" 1000 0 2147483648
t_run "$dev"
t_contains "$t_out" "tehas dev: tehas.slice has room for 1 more cycles"
exec 9>&-
rm -rf "$T/home/tehas/state/cycles"

t_case "a slice under pressure, or one that cannot be read, has no room and the session still starts"
again
slice_good
slice_state 12.00
slice_child "$(slice_id b)" $((5 * 1073741824)) 0 2147483648
t_run "$dev"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas dev: tehas.slice has room for 0 more cycles (it is under memory pressure)"
slice_good
rm -f "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dev"
t_contains "$t_out" "tehas dev: tehas.slice has room for 0 more cycles (its use could not be read: $T_CGROUP/tehas.slice/memory.pressure cannot be read)"
t_eq "$(containers)" 2
slice_good

# --- Without a usable slice ----------------------------------------------------
# refused: 78, and neither a clone nor a container was made.
refused() {
  t_eq "$t_rc" 78
  t_eq "$(containers)" 0
  t_lacks "$(t_calls docker)" "run "
  t_eq "$(find "$devs" -mindepth 1 -maxdepth 1 -name '*zed*' | wc -l)" 0
}
without() { again; slice_good; }

t_case "refuses with 78 without the slice, before any docker run"
without
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$dev zed --author 'Zed Example <zed@example.com>'"
refused
t_contains "$t_out" "tehas dev: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  without
  echo "$answer" >"$T/docker.info"
  t_run "$dev zed"
  refused
  t_contains "$t_out" "tehas dev: "
  t_contains "$t_out" "ocker"
done

t_case "a reserve above the ceiling is refused"
without
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=3g
t_run "$dev zed"
refused
t_contains "$t_out" "tehas dev: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "the twelve words are all present or nothing starts"
# The command beside a slice library that says yes and hands out a short list,
# or a list that names another parent.
short=$T/short
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
ln -s "$dev" "$short/libexec/dev"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --pids-limit 4096 --cpu-shares 1024" ""; do
  without
  printf '%s\n' 'tehas_slice_ok() { return 0; }' "tehas_slice_limits() { TEHAS_SLICE_LIMITS=($words); }" \
    'tehas_slice_room() { echo "4 0 0 0"; }' >"$short/lib/slice.sh"
  t_run "$short/libexec/dev zed"
  refused
  t_contains "$t_out" "tehas dev: the container limits could not be made, not starting"
done
without

# --- An issue's attachments ----------------------------------------------------
A=https://github.com/user-attachments/assets
t_stub gh 'case "$1 $2" in
  "api repos/acme/widget/issues/7") cat "$T/fx/issue.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/issues/7/events?per_page=100") cat "$T/fx/events.json" 2>/dev/null || echo "[]" ;;
esac'
t_stub curl '
out="" url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift ;;
    -H | --max-filesize | --proto | --proto-redir | --max-redirs | --max-time | -w) shift ;;
    https://*) url=$1 ;;
  esac
  shift
done
key=$(printf %s "$url" | tr -c "A-Za-z0-9" _)
[ -f "$T/fx/net/$key" ] || exit 22
cp "$T/fx/net/$key" "$out"
printf application/octet-stream'
net() { # <address> <printf format>
  mkdir -p "$T/fx/net"
  # shellcheck disable=SC2059  # the format carries the bytes of an image
  printf "$2" >"$T/fx/net/$(printf %s "$1" | tr -c 'A-Za-z0-9' _)"
}
issue_is() { mkdir -p "$T/fx"; jq -n --arg body "$1" --arg who "${2:-OWNER}" '{number: 7, body: $body, author_association: $who}' >"$T/fx/issue.json"; }
labelled_by() { jq -n --arg who "$1" '[{event: "labeled", label: {name: "tehas:ready"}, actor: {login: $who}, created_at: "2026-10-11T00:00:00Z"}]' >"$T/fx/events.json"; }
imgs=$devs/main.images
mounts_base="$clone:$clone
$state/runs:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
attach_on() {
  without
  rm -rf "$T/fx" "$imgs" "$devs/main.author"
  t_stub docker "$docker_stub"
  T_ENV=(GH_TOKEN=test-token "$root")
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere
}

t_case "with --issue the session gets the issue's images read-only and the variable that names them"
attach_on
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "Look ![shot]($A/1111)"
printf '%s|1\n' "$(printf 'a%.0s' {1..40})" >"$T/image.label"
t_run "$dev --issue 7"
t_eq "$t_rc" 0
t_eq "$(containers)" 1
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_base" "$imgs:$imgs:ro" | LC_ALL=C sort)"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 1
t_eq "$(passed PI_ENSEMBLE_EXTRA_IMAGE_DIRS)" "$imgs"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_eq "$(session_args | unknown_options "$image" "--name $limits -w -v -e" "--rm -it --init")" ""
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./manifest.json "
t_contains "$t_out" "tehas dev: Tehas gave this session 1 image from the issue text."
rm -f "$T/image.label"
t_lacks "$(session_args)" "/elsewhere"

t_case "without --issue the session gets neither, and an instance's image directory is not passed on"
again
t_run "$dev"
t_eq "$t_rc" 0
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_base" | LC_ALL=C sort)"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 0
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "an outsider's issue gives its images only when an allowed account applied its last tehas:ready"
for who in alice mallory; do
  attach_on
  net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
  issue_is "Look ![shot]($A/1111)" NONE
  labelled_by "$who"
  t_run "$dev --issue 7"
  t_eq "$t_rc" 0
  t_eq "$who $(jq -c '.images | length' "$imgs/manifest.json")" "$who $([ "$who" = alice ] && echo 1 || echo 0)"
done

t_case "with files alone --issue gives the text files and the same one mount"
attach_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=1
net "https://github.com/user-attachments/files/31/spec.md" '# Spec\n'
issue_is "[spec.md](https://github.com/user-attachments/files/31/spec.md)"
t_run "$dev --issue 7"
t_eq "$t_rc" 0
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./file-1.md ./manifest.json "
t_eq "$(after -v | grep -c -x "$imgs:$imgs:ro")" 1

t_case "an image that cannot be fetched starts no session: exit 4"
attach_on
issue_is "Look ![shot]($A/9999)"
t_run "$dev --issue 7"
t_eq "$t_rc" 4
t_eq "$(containers)" 0
t_contains "$t_out" "tehas dev: $A/9999 could not be fetched, not starting a session"
t_eq "$([ -e "$imgs" ] && echo left)" ""

t_case "--issue on an instance that gives no images or files starts nothing: exit 78"
attach_on
t_instance "${T_MINIMAL[@]}"
t_run "$dev --issue 7"
t_eq "$t_rc" 78
t_eq "$(containers)" 0
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "--issue without a number is refused"
for bad in "--issue" "--issue x" "--issue 07" "--issue ''"; do
  attach_on
  t_run "$dev $bad"
  t_eq "$t_rc" 64
  t_eq "$(containers)" 0
done

t_case "no git identity at all starts no container"
: >"$T/home/.gitconfig"
again
t_run "$dev bob"
t_eq "$t_rc" 64
t_contains "$t_out" "user.name"
t_eq "$(containers)" 0

t_done
