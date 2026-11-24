#!/usr/bin/env bash
# Tests for tehas dev that stay in shell: an issue's attachments, whose gh
# stub stands for console/src/attach.rs, and the lock of a session. The rest,
# the clone, its author, what its container is given and the slice, is
# console/tests/dev.rs. A stub docker records its arguments, one per line, and
# exits at once, so no session runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'

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

# One record per call: t_stub writes the body into a script that stays on disk,
# so $$ in it is the stub's own pid and is the same on every call. A counter file
# gives each call a file of its own, which is what lets containers, session_args
# and the age of a mount source be read against the call that made them.
docker_stub='if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
if [ "$1" = image ]; then cat "$T/image.label" 2>/dev/null; exit; fi
n=0
[ -e "$T/docker.n" ] && n=$(<"$T/docker.n")
n=$((n + 1))
printf "%s\n" "$n" >"$T/docker.n"
printf "%s\n" "$@" >"$T/calls/docker.args.$n"
printf "%s\n" "${GH_TOKEN:-}" >"$T/calls/docker.token.$n"'
t_stub docker "$docker_stub"
# The command beside the console, as a host has it: the session is the console's
# (console/src/dev.rs), and it makes the clone's state, its build output and the
# lock of the session, so a run that has no console refuses to start rather than
# starting a session without its lock.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
t_front "$T/tree"
# shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
cp "$T_CYCLE" "$T/tree/libexec/tehas-cycle"
dev_cmd="$T/tree/bin/tehas dev"
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# Pi's settings as install.sh leaves them on a host.
# An issue's attachments are read by the real attachment checker.
t_attach_home "$T/home/tehas"
t_ids "$T/home/tehas"
mkdir -p "$T/home/.pi/agent"
echo '{"retry":{"provider":{"timeoutMs":600000,"maxRetries":8,"maxRetryDelayMs":60000}}}' >"$T/home/.pi/agent/settings.json"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"
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
limits="--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares"
# The security options and the readers of a docker line come from
# test/docker_args.sh, which holds the list as a literal, so the cycle, the
# turn, the session and the push check are all pinned against one list, and not
# against the function under test. docker_opts is the
# allow-list of unknown_options: the slice's options and the security ones, by
# name, so an option added to either is allow-listed in all of them at once.
# shellcheck source=test/docker_args.sh
. "$(dirname "$0")/docker_args.sh" || exit 1
docker_opts="$limits --cap-drop --cap-add --security-opt --tmpfs"
root="TEHAS_CGROUP_ROOT=$T_CGROUP"
# The container path of the session's cargo build output: the mount, the
# variable and the manual name it.
target_container=/home/vscode/.tehas-target

# The arguments of the docker call that starts the session.
session_args() { cat "$T"/calls/docker.args.* 2>/dev/null; }
containers() { find "$T/calls" -name 'docker.args.*' | wc -l; }
# Values that follow a flag, e.g. every -v or every -e.
after() { session_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
# The value of one variable passed as NAME=value.
passed() { after -e | sed -n "s/^$1=//p"; }
# unknown_options, count_of and the readers of the security options come from
# test/docker_args.sh, sourced above: one implementation for all four containers.
again() { rm -f "$T"/calls/* "$T/docker.n"; }

# The cases before an issue's attachments reach no GitHub and are Rust tests
# in console/tests/dev.rs. What the cases below read of them: the image, the
# session's variables by name, and a run with the slice as it was.
image="tehas/pi-rukas:uid$(id -u)"
want="CARGO_TARGET_DIR
COLORTERM
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
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT
TERM"
without() { again; slice_good; }

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
$state/target:$target_container
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$state/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro
$T/home/tehas/state/github-session:/run/tehas/github:ro"
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
t_run "$dev_cmd --issue 7"
t_eq "$t_rc" 0
t_eq "$(containers)" 1
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_base" "$imgs:$imgs:ro" | LC_ALL=C sort)"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 1
t_eq "$(passed PI_ENSEMBLE_EXTRA_IMAGE_DIRS)" "$imgs"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_eq "$(session_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm -it --init")" ""
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./manifest.json "
t_contains "$t_out" "tehas dev: Tehas gave this session 1 image from the issue text."
rm -f "$T/image.label"
t_lacks "$(session_args)" "/elsewhere"

t_case "without --issue the session gets neither, and an instance's image directory is not passed on"
again
t_run "$dev_cmd"
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
  t_run "$dev_cmd --issue 7"
  t_eq "$t_rc" 0
  t_eq "$who $(jq -c '.images | length' "$imgs/manifest.json")" "$who $([ "$who" = alice ] && echo 1 || echo 0)"
done

t_case "an image that cannot be fetched starts no session: exit 4"
attach_on
issue_is "Look ![shot]($A/9999)"
t_run "$dev_cmd --issue 7"
t_eq "$t_rc" 4
t_eq "$(containers)" 0
t_contains "$t_out" "tehas dev: $A/9999 could not be fetched, not starting a session"
t_eq "$([ -e "$imgs" ] && echo left)" ""

t_case "--issue on an instance that gives no images starts nothing: exit 78"
attach_on
t_instance "${T_MINIMAL[@]}"
t_run "$dev_cmd --issue 7"
t_eq "$t_rc" 78
t_eq "$(containers)" 0
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "with --issue a fault stops before GitHub is called and before anything is written"
nothing_fetched() {
  t_eq "$t_rc" "$1"
  t_eq "$(containers)" 0
  t_eq "$(t_calls gh)$(t_calls curl)" ""
  t_eq "$(find "$devs" -maxdepth 1 -name '*.images' | wc -l)" 0
}
attach_on
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$dev_cmd --issue 7"
nothing_fetched 78
t_contains "$t_out" "tehas dev: no slice file on this host (sudo ./bootstrap.sh slice)"
attach_on
slice_good
mkdir -p "$devs/notes"
echo keep >"$devs/notes/file.txt"
t_run "$dev_cmd notes --issue 7"
nothing_fetched 78
t_contains "$t_out" "not a clone"
rm -rf "$devs/notes"
attach_on
echo 'not an author' >"$devs/main.author"
t_run "$dev_cmd --issue 7"
nothing_fetched 64
t_contains "$t_out" "main.author"
rm -f "$devs/main.author"

t_case "--issue without a number is refused"
for bad in "--issue" "--issue x" "--issue 07" "--issue ''"; do
  attach_on
  t_run "$dev_cmd $bad"
  t_eq "$t_rc" 64
  t_eq "$(containers)" 0
done

t_case "no git identity at all starts no container"
: >"$T/home/.gitconfig"
again
t_run "$dev_cmd bob"
t_eq "$t_rc" 64
t_contains "$t_out" "user.name"
t_eq "$(containers)" 0

t_case "a clone whose lock another holds waits for it and then starts its session"
again
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
# The case before this one took the git identity away, and the command asks for
# it before it asks for the lock.
T_ENV=()
slice_good
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
mkdir -p "$T/home/tehas/state/dev"
lock=$T/home/tehas/state/dev/erin.lock
# The session's container says whether the lock is held while it runs.
t_stub docker "$docker_stub
flock -n '$lock' true || echo held >\"\$T/calls/docker.lock\""
# What the session meets is a lock that comes free while it waits: freed once
# the command says it is waiting, not after a guess at how long that takes.
t_hold "$lock"
(n=0
  until grep -qs "waiting" "$T/dev.err" || [ $((n += 1)) -gt 1200 ]; do "$(command -pv sleep)" 0.05; done
  kill "$t_holder") &
freed=$!
T_ENV=(GH_TOKEN=test-token "$root")
t_run "$dev_cmd erin 2>\"\$T/dev.err\""
wait "$freed"
t_eq "$t_rc" 0
t_contains "$(cat "$T/dev.err")" "is held"
t_eq "$(containers)" 1
t_contains "$(session_args)" "$devs/erin.state/target:$target_container"
t_eq "$(find "$devs/erin.state/target" -mindepth 1 | wc -l)" 0
# The lock of the session is the clone's own, named by its clone, held for the
# session's life, and beside the locks of the cycles rather than among them.
t_eq "$(cat "$T/calls/docker.lock")" held
t_eq "$([ -e "$T/home/tehas/state/cycles/erin.lock" ] && echo there)" ""
t_release
t_stub docker "$docker_stub"
rm -rf "$devs/erin.state" "$devs/erin.author" "$devs/erin"

t_case "the build output the cleaner took during the wait is made again, and docker is not given the lock"
again
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=()
slice_good
t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
if [ "$1" = image ]; then cat "$T/image.label" 2>/dev/null; exit; fi
[ ! -e /proc/self/fd/9 ] || echo open >"$T/calls/docker.fd9"
printf "%s\n" "$@" >"$T/calls/docker.args.1"'
lock=$T/home/tehas/state/dev/frank.lock
# The order the case is about: the command waits on the lock, the cleaner takes
# the build output away during that wait, and the lock comes free afterwards.
# Each of the three is told by a marker rather than by a count of looks: the
# lock t_hold has taken, the target the command made, and the target the case
# removed. A wait that reads the directory before the command got there is the
# timing this case used to guess at, and on a loaded host it is the reason the
# run was met with no build output to make again and refused 78. The removal
# can also land between the command's create and its own look at the directory,
# before the wait: that is no refusal either (#2477).
t_hold "$lock"
(n=0
  until [ -d "$devs/frank.state/target" ] || [ $((n += 1)) -gt 600 ]; do "$(command -pv sleep)" 0.05; done
  rmdir "$devs/frank.state/target"; kill "$t_holder") &
freed=$!
T_ENV=(GH_TOKEN=test-token "$root")
t_run "$dev_cmd frank"
wait "$freed"
t_eq "$t_rc" 0
t_eq "$(containers)" 1
t_eq "$(stat -c %u "$devs/frank.state/target")" "$(id -u)"
t_eq "$(cat "$T/calls/docker.fd9" 2>/dev/null)" ""
T_ENV=()
t_release
t_stub docker "$docker_stub"
rm -rf "$devs/frank.state" "$devs/frank.author" "$devs/frank"

t_case "a lock that is not free in five minutes starts no session and says who holds it"
again
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=()
slice_good
# The holder is the sleep t_hold leaves behind, so the line names that account:
# the person running the other session.
mkdir -p "$T/home/tehas/state/dev"
t_hold "$T/home/tehas/state/dev/erin.lock"
sleep_holder=$(ps -o user= -p "$t_holder" | tr -d ' ')
# The case waits one second of the five minutes the command asks.
T_ENV=(TEHAS_DEV_LOCK_WAIT=1 GH_TOKEN=test-token "$root")
t_run "$dev_cmd erin"
t_eq "$t_rc" 78
t_eq "$(containers)" 0
t_lacks "$(t_calls docker)" "run "
t_contains "$t_out" "$T/home/tehas/state/dev/erin.lock is held"
t_contains "$t_out" "waiting 1 seconds for it"
# fuser and ps are no part of what the gate needs. Without them the line stops
# at "is held".
if command -v fuser >/dev/null && command -v ps >/dev/null; then
  t_contains "$t_out" "is held by $sleep_holder"
  t_lacks "$t_out" "console"
fi
t_eq "$([ -e "$devs/erin" ] && echo there)" ""
# The refusal neither makes the clone nor takes what an earlier session of it
# left: the lock is asked for before the tree is made, and nothing is removed on
# the way to the refusal.
mkdir -p "$devs/erin"
mkdir -p "$devs/erin.state/target/debug"
echo built >"$devs/erin.state/target/debug/console"
T_ENV=(TEHAS_DEV_LOCK_WAIT=1 GH_TOKEN=test-token "$root")
t_run "$dev_cmd erin"
t_eq "$t_rc" 78
t_eq "$([ -d "$devs/erin/.git" ] && echo clone)" ""
t_eq "$(cat "$devs/erin.state/target/debug/console")" built
t_release
rm -f "$T/home/tehas/state/dev/erin.lock"
t_stub docker "$docker_stub"
rm -rf "$devs/erin.state" "$devs/erin"

t_done
