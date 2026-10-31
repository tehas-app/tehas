#!/usr/bin/env bash
# Tests for libexec/work: what the cycle's container is given, its mounts,
# variables, limits and console, and that nothing starts without a usable
# slice.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1

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

t_case "the cycle gets the slice, a ceiling of a quarter of it, no swap, a floor, a pids limit, a CPU weight and no privileges"
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
t_eq "$(after --cpus)" "15.00"
# Every capability of Docker's default list is taken away, and only what a
# gate needs is given back: see tehas_sandbox_security.
t_eq "$(cycle_args | security_line)" "$security_opts "
t_eq "$(cycle_args | security_of --cap-drop | LC_ALL=C sort)" "ALL"
t_eq "$(cycle_args | security_of --cap-add | LC_ALL=C sort)" "CHOWN
FOWNER"
t_eq "$(cycle_args | security_of --security-opt)" "no-new-privileges:true"
# One bounded /tmp of its own: the tmpfs counts against the container's memory,
# so a log written in a loop ends the cycle instead of filling the host's disk.
# It stays exec for the product's own gate; see tehas_sandbox_security. The two
# fixtures below are written the way the docker stub records a call: one
# argument per line. The equals form docker also accepts counts as a mount too,
# so an unbounded, a noexec or a second mount written that way fails the count
# instead of passing it unseen; and a second mount of another path is a second
# flag wherever it stands, the one drift security_line's contiguous match
# cannot see, so the whole line must hold exactly one.
t_eq "$(cycle_args | security_of --tmpfs)" "$security_tmpfs"
# Half of --memory 2048m, so a full /tmp is ENOSPC and not an OOM kill.
t_eq "$(cycle_args | security_of --tmpfs)" "/tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"
t_eq "$(cycle_args | count_of --tmpfs)" "1"
t_eq "$(printf 'run\n--tmpfs=/tmp\n' | count_of --tmpfs)" "1"
t_eq "$(printf 'run\n--tmpfs\n/tmp:rw\n--tmpfs\n/dev/shm\n' | count_of --tmpfs)" "2"
# The image's entrypoint drops from root with setpriv, which the dropped
# SETUID and SETGID refuse: the container starts as the host's user instead.
t_eq "$(after --user)" "$(id -u):$(id -g)"

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

t_case "the cid file is gone when the cycle is over, and no count of kills is left"
t_eq "$(find "$cid_file" "$oom_file" 2>/dev/null | wc -l)" 0
t_lacks "$t_out" "killed"

t_case "the image and the working directory come from the settings"
t_contains "$(cycle_args | tr '\n' ' ')" "tehas/pi-rukas:uid$(id -u) $runner work 7 --timeout 360 --json"
t_eq "$(after -w)" "$inside"

t_case "the console is the program beside this one, given the cycle's log, the transcripts and its file"
t_contains "$(t_calls console)" "--progress $T/home/tehas/logs/work-7-"
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
t_contains "$(cycle_args | tr '\n' ' ')" "$runner work 7 --timeout 360 --json"
t_stub console
mv "$T/bin/console" "$tree/libexec/console"
t_run "$work 7"

t_case "the console masks the host aliases, the model host and the bot's email, not names under 4 characters"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=gpu1:192.0.2.9,gb:192.0.2.10,,v6host:2001:db8::1
mkdir -p "$T/home/.pi/agent"
echo '{"providers":{"other":{"baseUrl":"http://elsewhere.lan/v1"},"test-provider":{"baseUrl":"http://models.lan:8086/v1"}}}' \
  >"$T/home/.pi/agent/models.json"
echo 'echo "{\"v\":1,\"kind\":\"log\",\"message\":\"working\"}" >&2' >"$T/container.sh"
rm -f "$T/calls/console"
t_run "$work 7"
given=$(t_calls console | grep -e '^--progress ')
t_contains "$given" " --private gpu1 --private 192.0.2.9 --private 192.0.2.10 --private v6host --private 2001:db8::1 --private models.lan --private bot@example.com"
t_lacks "$given" "--private gb "
t_lacks "$given" "elsewhere"
t_lacks "$(t_calls console)" "--mask"
t_contains "$(t_calls console | grep -e '^--progress ')" "--now $T/home/tehas/live/console-7.txt.now"
t_lacks "$(t_calls console)" "test-token"

# The step and the notice of the status line come from the two lines the
# console beside the log writes (console/src/now.rs), and not from a pass over
# the log by this command. The stub writes the file as the console does.
t_case "the status line is read from the console's now file, and a stale one is taken away"
container 'progress "{\"v\":1,\"kind\":\"log\",\"message\":\"a notice\"}"; exit 0'
# The stub takes the names of the two lines from the files a case writes, so
# one case can say what the console of this cycle kept.
cat >"$tree/libexec/console" <<'NOW'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/console"
now=
prev=
for a in "$@"; do
  [ "$prev" = --now ] && now=$a
  prev=$a
done
[ -n "$now" ] || exit 0
[ -e "$T/now.step" ] || exit 0
printf '%s\n%s\n' "$(cat "$T/now.step")" "$(cat "$T/now.text")" >"$now.new"
mv "$now.new" "$now"
exit 0
NOW
chmod +x "$tree/libexec/console"
# The file of a cycle before this one, which the start takes away.
echo stale >"$live/console-7.txt.now"
printf develop >"$T/now.step"
printf 'step 3 of the cycle' >"$T/now.text"
rm -f "$T"/calls/*
t_run "$work 7"
t_contains "$(t_calls console | grep -e '^--progress ')" "--now $live/console-7.txt.now"
t_eq "$(cat "$live/console-7.txt.now")" "develop
step 3 of the cycle"
t_eq "$(find "$live" -maxdepth 1 -name '*.now.new' | wc -l)" 0

# Without the file the step is starting and the notice empty: tehas status of
# a cycle whose console never came up says so and does not stop.
cat >"$tree/libexec/console" <<'NOW'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/console"
exit 0
NOW
chmod +x "$tree/libexec/console"
echo stale >"$live/console-7.txt.now"
rm -f "$T"/calls/* "$T/now.step"
t_run "$work 7"
t_lacks "$(t_calls store)" "--step develop"
t_eq "$(find "$live" -maxdepth 1 -name 'console-7.txt.now' | wc -l)" 0
rm -f "$T/now.text"

t_case "the console masks the host of a role's own provider too"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_DEVELOPER=big PI_ENSEMBLE_PROVIDER_DEVELOPER=other
rm -f "$T/calls/console"
t_run "$work 7"
t_contains "$(t_calls console | grep -e '^--progress ')" "--private models.lan --private elsewhere.lan --private bot@example.com"

t_case "without a baseUrl or aliases the console is given the bot's email only"
t_instance "${T_MINIMAL[@]}"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"
rm -f "$T/calls/console"
t_run "$work 7"
t_eq "$(t_calls console | grep -e '^--progress ' | grep -o -e '--private .*')" "--private bot@example.com"
rm -f "$T/container.sh"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

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
$state/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
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
PI_ENSEMBLE_CYCLE_DEADLINE
PI_ENSEMBLE_HANDOFF_CHANNEL
PI_ENSEMBLE_HANDOFF_TIMEOUT_MS
PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SPAWN_TIMEOUT_MS
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_CONCURRENCY
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the child bound is tehas', not the instance's, and takes its default when the case names none"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_SPAWN_TIMEOUT_MS=60000 TEHAS_CHILD_TIMEOUT=5400
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_SPAWN_TIMEOUT_MS')" 1
t_eq "$(after -e | grep '^PI_ENSEMBLE_SPAWN_TIMEOUT_MS')" "PI_ENSEMBLE_SPAWN_TIMEOUT_MS=5400000"
t_instance "${T_MINIMAL[@]}"
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$(after -e | sed -n 's/^PI_ENSEMBLE_SPAWN_TIMEOUT_MS=//p')" "7200000"

t_case "a role's own model is passed in under the names pi-rukas reads, and nothing else is"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" \
  "$(printf '%s\n' "$want" PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST | LC_ALL=C sort)"
t_eq "$(cycle_args | grep -c -e '^PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST$' -e '^PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST$')" 2

t_case "the tokens models.json names are passed in by name, and the cycle is on the endpoint picked at its start"
t_instance "${T_MINIMAL[@]}"
jq -n '{providers: {"test-provider": {baseUrl: "https://main.test/v1", apiKey: "$TEHAS_MODEL_KEY_MAIN",
  backupBaseUrl: "https://fleet.test/v1", backupApiKey: "$TEHAS_MODEL_KEY_FLEET", models: [{id: "test-model"}]}}}' \
  >"$T/home/.pi/agent/models.json"
t_stub curl 'case "$*" in *fleet.test*) echo "{\"data\":[{\"id\":\"test-model\"}]}" ;; *) exit 7 ;; esac'
T_ENV=("${env_ok[@]}" TEHAS_MODEL_KEY_MAIN=main-secret TEHAS_MODEL_KEY_FLEET=fleet-secret)
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" TEHAS_MODEL_KEY_FLEET TEHAS_MODEL_KEY_MAIN | LC_ALL=C sort)"
t_lacks "$(cycle_args)" "secret"
t_eq "$(jq -c '.providers."test-provider" | [.baseUrl, .apiKey, has("backupBaseUrl")]' "$state/models.json")" \
  '["https://fleet.test/v1","$TEHAS_MODEL_KEY_FLEET",false]'
t_contains "$t_out" "tehas work: model endpoint: provider test-provider is on its backup"
t_contains "$(t_calls console | grep -e '^--progress ')" "--private main.test --private fleet.test"

t_case "a token models.json names and the host has not set stops the cycle before Docker: exit 78"
T_ENV=("${env_ok[@]}" TEHAS_MODEL_KEY_MAIN=main-secret)
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "models.json names TEHAS_MODEL_KEY_FLEET and it is not set"
t_eq "$(t_calls docker)$(t_calls curl)" ""
rm -f "$T/bin/curl"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"
T_ENV=("${env_ok[@]}")

t_case "without models.json nothing runs: exit 78 and no docker call"
rm -f "$T/home/.pi/agent/models.json" "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "no $T/home/.pi/agent/models.json: run install.sh"
t_eq "$(t_calls docker)" ""
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"

t_case "a role that pi-rukas does not know stops the cycle before Docker"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_REVIEWER=small-model
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "PI_ENSEMBLE_MODEL_REVIEWER names no role"
t_eq "$(t_calls docker)" ""
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
rm -f "$T"/calls/*
t_run "$work 7"

t_case "the cycle's container gets no option beyond the documented ones"
image="tehas/pi-rukas:uid$(id -u)"
t_eq "$(cycle_args | unknown_options "$image" \
  "--name $docker_opts --user --cidfile -w -v -e" "--rm --init")" ""
t_eq "$(cycle_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' ')" "$image $runner work 7 --timeout 360 --json "

# The runner is no init: without one, a process that outlives its parent is never
# collected and counts against the pids limit until the container ends.
t_case "the cycle's container has an init"
t_eq "$(cycle_args | grep -c -x -e --init)" 1

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
t_contains "$t_out" "tehas work: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m, tehas.slice's MemoryMax over TEHAS_SLICE_CONTAINERS=4); the host needs about 16 GiB"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "the fourteen words are all present or nothing starts"
# The command beside a slice library that says yes and hands out a short list,
# or a list that names another parent.
short=$T/short
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
ln -s "$T_ROOT/libexec/work" "$short/libexec/work"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" \
  "--memory 2048m --cgroup-parent tehas.slice --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" ""; do
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

t_case "with a GitHub App the container is given the app's token, not a user's"
rm -f "$T"/calls/*
cp "$TEHAS_INSTANCE_DIR/instance.env" "$T/instance.env.before"
printf '%s\n' TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]" >>"$TEHAS_INSTANCE_DIR/instance.env"
mkdir -p "$T/home/tehas/state/github"
printf 'ghs_app\n' >"$T/home/tehas/state/github/token"
echo $(($(date +%s) + 3000)) >"$T/home/tehas/state/github/token.expires"
T_ENV=("TEHAS_CGROUP_ROOT=$T_CGROUP")
t_run "$work 7"
t_eq "$(cat "$T/calls/docker.token" 2>/dev/null)" "ghs_app"
run_args=$(cycle_args)
t_contains "$run_args" "$T/home/tehas/state/github:/run/tehas/github:ro"
t_lacks "$run_args" "github-app.pem"
cp "$T/instance.env.before" "$TEHAS_INSTANCE_DIR/instance.env"
rm -f "$T/home/tehas/state/github/token" "$T/home/tehas/state/github/token.expires"

t_case "without a GitHub App the container is given no token directory"
rm -f "$T"/calls/*
T_ENV=("${env_ok[@]}")
t_run "$work 7"
run_args=$(cat "$T"/calls/docker.args.* 2>/dev/null)
t_contains "$run_args" "pi-rukas-headless"
t_lacks "$run_args" "/run/tehas/github"

t_case "a scratch disk takes the place of the tmpfs, at the same path, and for this cycle alone"
t_console || t_bad "no console: $t_console_why"
rm -f "$T"/calls/docker*
mkdir -p "$T/scratch"
t_instance "${T_MINIMAL[@]}" TEHAS_SCRATCH_DIR="$T/scratch"
T_ENV+=(TEHAS_CONSOLE_CMD="$T_CONSOLE")
t_run "$work 7"
t_eq "$(cycle_args | count_of --tmpfs)" "0"
t_contains "$(cycle_args)" "$T/scratch/widget-7:/tmp"
t_eq "$(stat -c %a "$T/scratch/widget-7")" "1777"
t_eq "$(after --user)" "$(id -u):$(id -g)"

t_case "a scratch disk that is not there starts no container"
rm -rf "$T/scratch" "$T"/calls/docker*
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cycle_args)" ""
t_eq "$(preflight_args)" ""

t_case "a record that says driver lean is refused before anything moves: 78, no container, and no classic run"
t_instance "${T_MINIMAL[@]}"
T_ENV=("${env_ok[@]}")
rm -rf "$T/home/tehas/work"
mkdir -p "$T/home/tehas/work"
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$T/home/tehas/work/widget-7"
echo keep >"$T/home/tehas/work/widget-7/notes.txt"
record_is alice "Words only."
record_edit '.driver = "lean"'
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "this tehas cannot run the lean driver yet, not starting a cycle; take tehas:lean off #7 to run it on classic"
t_eq "$([ -f "$T/home/tehas/state/cycles/7.lean-refused" ] && echo left)" left
t_lacks "$(t_calls docker)" "run "
t_eq "$(cycle_args)" ""
t_eq "$(preflight_args)" ""
t_eq "$(t_calls gh)" ""
t_eq "$(cat "$T/home/tehas/work/widget-7/notes.txt")" keep
t_eq "$(find "$T/home/tehas/work" -maxdepth 1 -name '*.before-retry' | wc -l)" 0
rm -rf "$T/home/tehas/work"

t_case "the watch loop stops a cycle by the cycle clock, not the wall clock"
fresh
# The wall clock stands still and the cycle clock goes forward past the stall
# of 60 s while the limit of 600 s is still ahead: the cycle ends on its own,
# neither stalled nor timed out. On the wall clock of before the change, where
# the pass that finds a newer log set the stall's mark to the wall's now, this
# same pass was the one the loop stopped the cycle on.
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=600 TEHAS_CHILD_TIMEOUT=600 TEHAS_WORK_STALL=60 TEHAS_VERIFY_TIMEOUT=1
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
printf '%s\n' 1000 1010 1020 1030 1040 1050 >"$T/clock.now"
container 'says handoff; exit 10'
t_run "$work 7"
t_eq "$t_rc" 10
t_lacks "$t_out" "time limit of 600s passed"
t_lacks "$t_out" "stopping it as stalled"
rm -f "$T/clock.now" "$T/clock.n"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "logs that stand still do not move the stall's mark, and the mark is a clock reading"
fresh
rm -f "$T/clock.calls"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=600 TEHAS_CHILD_TIMEOUT=600 TEHAS_WORK_STALL=60 TEHAS_VERIFY_TIMEOUT=1
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
seq 5000 10 5200 >"$T/clock.now"
printf 'ok\n' >"$T/clock.check"
container '"$nap" 1; exit 10'
t_run "$work 7"
since=$(sed -n 's/.*--since \([0-9]*\).*/\1/p' "$T/clock.calls" | sort -u)
t_eq "$(wc -l <"$T/clock.calls" | awk '{print ($1 >= 3)}')" "1"
t_eq "$(echo "$since" | wc -l | awk '{print ($1 <= 2)}')" "1"
t_eq "$(echo "$since" | awk '$1 > 6000' | wc -l | tr -d ' ')" "0"
rm -f "$T/clock.check" "$T/clock.now" "$T/clock.n" "$T/clock.calls"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "the cycle clock saying stalled stops the cycle as stalled"
fresh
rm -f "$T/home/tehas/state/cycles/7.stalled"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=600 TEHAS_CHILD_TIMEOUT=600 TEHAS_WORK_STALL=60 TEHAS_VERIFY_TIMEOUT=1
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
printf 'stalled\n' >"$T/clock.check"
container "$hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "none of the cycle's logs changed for 60s, stopping it as stalled"
t_contains "$t_out" "status=stalled"
t_lacks "$t_out" "time limit"
rm -f "$T/clock.check"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "the cycle clock saying over stops the cycle at its time limit"
fresh
rm -f "$T/home/tehas/state/cycles/7.timed"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=600 TEHAS_CHILD_TIMEOUT=600 TEHAS_WORK_STALL=60 TEHAS_VERIFY_TIMEOUT=1
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
printf 'over\n' >"$T/clock.check"
container "$hold; $gone"
t_run "$work 7"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 600s passed"
t_contains "$t_out" "status=timeout"
t_lacks "$t_out" "stalled"
rm -f "$T/clock.check"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "a cycle clock that answers unknown stops nothing: the cycle ends on its own"
fresh
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=600 TEHAS_CHILD_TIMEOUT=600 TEHAS_WORK_STALL=60 TEHAS_VERIFY_TIMEOUT=1
T_ENV=("${env_ok[@]}" TEHAS_WORK_GRACE=0)
printf 'unknown\n' >"$T/clock.check"
container 'says handoff; exit 10'
t_run "$work 7"
t_eq "$t_rc" 10
t_lacks "$t_out" "time limit of 600s passed"
t_lacks "$t_out" "stopping it as stalled"
t_lacks "$t_out" "status=timeout"
rm -f "$T/clock.check"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")

t_case "the container is given a deadline of the wall clock, and the mark is the cycle's"
fresh
t_run "$work 7"
t_eq "$(after -e | sed -n 's/^PI_ENSEMBLE_CYCLE_DEADLINE=//p' | awk '{print ($1 > 0 && $1 % 1000 == 0)}')" "1"
t_eq "$(cat "$T/home/tehas/state/cycles/7.clock" | tr -cd 0-9 | wc -c | tr -d ' ' | awk '{print ($1 > 0)}')" "1"

t_done
