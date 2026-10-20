# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # stub bodies are expanded when the stub runs; the parts read the variables
# What the tests of libexec/work share: test/work_*_test.sh source this. A
# stub docker records its arguments, one per line, and exits at once, so no
# cycle runs. It answers --capabilities from $T/caps.json.
# The part has sourced test/lib.sh before this.
# The library's own rule for the hash of a body, so that a fixture records what
# the dispatcher would have recorded and not an approximation of it. Sourcing
# attachments.sh runs nothing.
# shellcheck source=lib/attachments.sh
. "$T_ROOT/lib/attachments.sh" || exit 1
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'
# libexec/work sleeps ten seconds between two looks at the container, and
# three for the console at the end. Here each costs a tenth of a second. What
# stands in for the container uses the real sleep, $nap.
t_stub sleep '"$(command -pv sleep)" 0.1'
nap=$(command -pv sleep)

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
case " $* " in *" --capabilities "*)
  printf "%s\n" "$*" >>"$T/calls/docker.caps"
  [ ! -e "$T/caps.json" ] || cat "$T/caps.json"
  exit "${CAPS_RC:-0}" ;;
esac
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
  *"pi-rukas-headless work "*) [ ! -e "$T/container.sh" ] || . "$T/container.sh" ;;
  *" push --dry-run "*)
    [ ! -e "$T/push-hangs" ] || exec "$(command -pv sleep)" 30
    [ "${PUSH_RC:-0}" = 0 ] || { echo "docker: could not read from remote repository" >&2; exit "$PUSH_RC"; } ;;
  " stop "*)
    [ ! -e "$T/stop-hangs" ] || exec "$(command -pv sleep)" 30
    kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
  " kill "*) kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
esac'
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# What a runner on result schema 4 answers to --capabilities, cut to what the
# tests use.
caps_good() {
  jq -n '{schemaVersion: 2, version: "0.12.80", commit: null,
    contracts: {exitCodes: {version: 1}, resultSchema: {version: 4}, ndjson: {version: 1, stable: {version: 1}}},
    verbs: {work: {flags: ["--restart", "--json", "--timeout"]}},
    inputs: ([("SANDBOX_MODE", "WORKSPACE_ROOT") | {name: "PI_ENSEMBLE_\(.)", scope: "sandbox"}]
      + [("MAX_WORKSTREAMS", "VERIFY_TIMEOUT_MS", "INTEGRATION_VERIFY_TIMEOUT_MS", "SUBAGENT_MODEL",
          "SUBAGENT_PROVIDER", "MODEL_<ROLE>", "PROVIDER_<ROLE>", "EXTRA_IMAGE_DIRS", "COMMENT_RETENTION",
          "HANDOFF_CHANNEL", "HANDOFF_TIMEOUT_MS", "CYCLE_DEADLINE", "VERIFY_CONCURRENCY")
         | {name: "PI_ENSEMBLE_\(.)", scope: "cycle"}]
      + [{name: "PI_ENSEMBLE_SUBAGENT_MODE", scope: "internal"}]),
    features: {resultFromMemory: true, cycleDeadline: true, handoffChannel: true, restartClearsAttention: true}}' >"$T/caps.json"
}
caps_good
# The flag a runner that can continue a pull request adds to work.
caps_continue() {
  jq '.verbs.work.flags += ["--continue-pr"]' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
}
# Pi's settings as install.sh leaves them on a host.
mkdir -p "$T/home/.pi/agent"
echo '{"retry":{"maxRetries":10,"baseDelayMs":5000,"maxAgentDelayMs":120000,"provider":{"timeoutMs":600000,"maxRetries":8,"maxRetryDelayMs":60000}}}' >"$T/home/.pi/agent/settings.json"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"

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

runner=/opt/pi-rukas/bin/pi-rukas-headless
# The arguments of the docker call that starts the cycle.
cycle_args() { grep -l -x -e 'pi-rukas-headless' -e "$runner" "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
# The arguments of the docker call that checks the push before the cycle.
preflight_args() { grep -l -x -e '--entrypoint' "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
# The security options of a container, as lib/sandbox.sh prints them: the same
# eight words for a cycle, a planning turn, a session and the push check.
security_opts="--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true"
# How many times an option is given, and with what value: the words after it,
# read the way docker reads them. <flag> is one word, as it stands on the line.
security_of() { # <flag> (reads the arguments on standard input)
  awk -v flag="$1" 'prev == flag { print } { prev = $0 }'
}
# The same eight words, in order, as one line. Empty when a container is given
# them another way or not at all. Reads the arguments on standard input.
security_line() {
  awk '
    { arg[NR] = $0 }
    END {
      want[1] = "--cap-drop"; want[2] = "ALL"
      want[3] = "--cap-add"; want[4] = "CHOWN"
      want[5] = "--cap-add"; want[6] = "FOWNER"
      want[7] = "--security-opt"; want[8] = "no-new-privileges:true"
      for (i = 1; i <= NR; i++) {
        if (seen >= 8) break
        if (arg[i] != want[seen + 1]) continue
        for (j = 1; j <= 8; j++) if (arg[i + j - 1] != want[j]) break
        if (j > 8) { seen = 8; start = i }
      }
      if (seen == 8) for (i = start; i < start + 8; i++) printf "%s ", arg[i]
    }'
}
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

# What the cycle's container is given (AGENTS.md rule 4): the variables, by
# name, and the mounts when it is given nothing of the issue.
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
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_CONCURRENCY
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
mounts_off="$ws:$inside
$runs:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$state/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
image="tehas/pi-rukas:uid$(id -u)"
live=$T/home/tehas/live

result_file=$T/home/tehas/state/cycles/7.result.json
progress_file=$T/home/tehas/state/cycles/7.progress.json
# container <what it does>: the runner. "says <status> [cap]" writes its result
# to standard output, "progress <line>" one line of its NDJSON.
container() {
  printf '%s\n' 'nap=$(command -pv sleep)' \
    'says() { echo "{\"schemaVersion\":4,\"buildCommit\":null,\"exitCode\":10,\"issues\":[{\"issue\":7,\"status\":\"$1\",\"cap\":\"${2:-}\",\"failedStep\":\"develop\",\"branch\":\"feature/issue-7-x\",\"steps\":[{\"step\":\"explore\"},{\"step\":\"develop\"}],\"usage\":{\"input\":5,\"output\":2,\"cacheRead\":1,\"turns\":3,\"dispatches\":2},\"exitCode\":10}]}"; }' \
    'progress() { printf "%s\n" "$1" >&2; }' \
    "$1" >"$T/container.sh"
}
# Alive until docker stop is called, then gone with 137, as a container is.
hold='"$nap" 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid"'
gone='wait; exit 137'
# A log is named to the second, so a cycle of the case before can share this
# case's name: its files go first.
fresh() { rm -rf "$T"/calls/* "$live/cycles" "$T/sleeper.pid" "$T"/home/tehas/logs/work-7-*; }

# stub_github: gh answers for the issue and its events from files, no events
# file is an issue nobody labelled. curl hands out $T/fx/net/<address, with _
# for every other character>; <name>.rc is an exit status to fail with. What
# arrives on standard input (the header) is kept.
fx=$T/fx
A=https://github.com/user-attachments/assets
F=https://github.com/user-attachments/files
imgs=$ws.images
stub_github() {
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
# A request with no -o is a chat request to the model: the answer is printed.
[ -n "$out" ] || { cat "$T/fx/net/$key"; exit 0; }
[ "$(stat -c %s "$T/fx/net/$key")" -le "$max" ] || exit 63
cp "$T/fx/net/$key" "$out"
printf application/octet-stream
exit 0'
}
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
# issue_is <body> [association]: the answer for issue 7 as REST gives it. A
# start is held to an answer that names an open issue, so the fixture carries
# the state the real answer carries.
issue_is() {
  jq -n --arg body "$1" --arg who "${2:-OWNER}" '{number: 7, state: "open", body: $body, author_association: $who}' >"$fx/issue.json"
}
images_on() {
  rm -rf "$fx" "$T"/calls/* "$T/home/tehas/work" "$T/home/tehas/state/cycles/7.manifest.json" "$T/home/tehas/state/cycles/7.approved.json"
  mkdir -p "$fx/net"
  slice_good
  reads_image 1
  T_ENV=("${env_ok[@]}")
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 "$@"
}
# images_off: the same instance with nothing given to the cycle. A test of what
# a start asks GitHub for, with attachments or without them, starts here.
images_off() {
  rm -rf "$fx" "$T"/calls/* "$T/home/tehas/work" "$T/home/tehas/logs" \
    "$T/home/tehas/state/cycles/7.manifest.json" "$T/home/tehas/state/cycles/7.approved.json"
  mkdir -p "$fx/net"
  slice_good
  reads_image 1
  T_ENV=("${env_ok[@]}")
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0
}
manifest() { jq -c "$1" "$imgs/manifest.json" 2>&1; }
# issue_reads: how many times a start asked GitHub for the issue itself. One
# read serves the text check and the attachments of the same start.
issue_reads() { grep -c -x -e 'api repos/acme/widget/issues/7' "$T/calls/gh" 2>/dev/null || true; }
# What the sandbox image says about its own pi-rukas, as host/sandbox/build.sh
# writes it: <commit>|<reads>. reads_image says it for one case.
img_commit=$(printf 'c%.0s' {1..40})
reads_image() { # <1, 0, or none when the image carries no label>
  rm -f "$T/image.label"
  if [ "$1" != none ]; then printf '%s\n' "$img_commit|$1" >"$T/image.label"; fi
}
# labelled_by <login>: the account that applied tehas:ready last.
labelled_by() {
  jq -n --arg who "$1" '[{event: "labeled", label: {name: "tehas:plan"}, actor: {login: "alice"}, created_at: "2026-10-12T00:00:00Z"},
    {event: "labeled", label: {name: "tehas:ready"}, actor: {login: $who}, created_at: "2026-10-11T00:00:00Z"}]' >"$fx/events.json"
}
record=$T/home/tehas/state/cycles/7.approved.json
kept=$T/home/tehas/state/cycles/7.manifest.json
# record_is <by> <body it approved> <link>...: the record of a start. The hash
# is tehas_body_sha256's own, the one the dispatcher records and tehas work
# compares, so a body that differs only in line ends differs in nothing.
record_is() {
  local by=$1 body=$2
  shift 2
  mkdir -p "$(dirname "$record")"
  jq -n --arg by "$by" --arg sum "$(jq -n --arg body "$body" '{body: $body}' | tehas_body_sha256)" '{at: "2026-10-14T00:00:00Z", by: $by,
    how: "label", body_sha256: $sum, links: $ARGS.positional}' --args "$@" >"$record"
}

# Most cases run with this instance and environment.
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")
