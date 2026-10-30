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
# The cycle clock of tehas work, in $tree/libexec/tehas-cycle. By default it
# keeps the seconds tehas work's own start was marked at and counts on with
# the wall clock, so a case that says nothing about the clock sees a cycle
# measured as it was before the cycle clock: `clock now` gives the seconds
# since the mark and `clock check` decides from them. A case that means the
# cycle clock itself writes $T/clock.now, one reading per call of `clock now`
# in order, and $T/clock.check, the word `clock check` answers with.
t_stub tehas-cycle 'case "$1 $2" in
  "clock mark") date +%s >"$3"; date +%s >"$T/clock.start" ;;
  "clock now")
    if [ -e "$T/clock.now" ]; then
      n=$(cat "$T/clock.n" 2>/dev/null || echo 0); n=$((n + 1))
      echo "$n" >"$T/clock.n"; sed -n "${n}p" "$T/clock.now"
    else
      date +%s
    fi ;;
  "clock check"*)
    if [ -e "$T/clock.check" ]; then cat "$T/clock.check"; exit 0; fi
    limit=0 stall=0 since=0 prev=""
    for a in "$@"; do
      case $prev in --limit) limit=$a ;; --stall) stall=$a ;; --since) since=$a ;; esac
      prev=$a
    done
    now=$(date +%s); start=$(cat "$T/clock.start" 2>/dev/null || echo "$now")
    [ $((now - start)) -le "$limit" ] || { echo over; exit 2; }
    [ $((now - since)) -lt "$stall" ] || { echo stalled; exit 1; }
    echo ok ;;
esac
exit 0'
mv "$T/bin/tehas-cycle" "$tree/libexec/tehas-cycle"
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
# The product names its gate in .pi/verify-cmd, and tehas work parks a cycle of
# a product whose commit has neither that file nor a stack whose gate pi-rukas
# can guess (libexec/work), so a fixture of no gate would start no container at
# all. tehas work reads the gate at the commit the cycle's clone is made at, not
# in the host's checkout, which is the tree a person works in: so the gate is a
# tracked file of the product's one commit, and the checkout has it as a file of
# the branch. The work files that make a product of their own files build it from
# these two, so the gate survives what they replace.
mkdir -p "$upstream/.pi"
printf 'scripts/verify.sh\n' >"$upstream/.pi/verify-cmd"
git -C "$upstream" add -f -- .pi/verify-cmd
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q -m first
git clone -q "$upstream" "$checkout"
# The checkout's own worktree holds something of its own, untracked, so a case
# can say that the host's tree decides nothing: tehas work reads the commit the
# cycle's clone is made at, not this worktree.
printf 'placeholder\n' >"$checkout/one.txt"

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
printf "%s\n" "${GH_TOKEN:-}" >"$T/calls/docker.token"
prev=""
for a in "$@"; do
  if [ "$prev" = --cidfile ]; then
    [ ! -e "$a" ] || { echo "docker: container ID file found" >&2; exit 125; }
    cat "$T/cid" >"$a"
  fi
  prev=$a
done
case " $* " in
  *"pi-rukas-headless work "*)
    [ -e "$T/container.sh" ] || cp "$T/container-default" "$T/container.sh"
    . "$T/container.sh" ;;
  *" push --dry-run "*)
    [ ! -e "$T/push-hangs" ] || exec "$(command -pv sleep)" 30
    [ "${PUSH_RC:-0}" = 0 ] || { echo "docker: could not read from remote repository" >&2; exit "$PUSH_RC"; } ;;
  " stop "*)
    [ ! -e "$T/stop-hangs" ] || exec "$(command -pv sleep)" 30
    kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
  " kill "*) kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
esac'
# The container of a cycle that named none: it says one line of progress and
# ends at once, purely to keep the cycle's stream from being the empty one tehas
# work takes for a container that never came up and writes nothing for
# (libexec/work, the note of a runner that never started). It is put where a case
# that names no container finds it, and taken away again by fresh. A case that
# means that end names its own container and says nothing; a case that reads
# nothing of the cycle names none and should not find one.
container_default='nap=$(command -pv sleep)
progress() { printf "%s\n" "$1" >&2; }
progress cycle
exit 0'
printf '%s\n' "$container_default" >"$T/container-default"
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# What a runner on result schema 4 answers to --capabilities, cut to what the
# tests use.
caps_good() {
  jq -n '{schemaVersion: 2, version: "0.12.80", commit: null,
    contracts: {exitCodes: {version: 1}, resultSchema: {version: 5}, ndjson: {version: 1, stable: {version: 1}}},
    verbs: {work: {flags: ["--restart", "--json", "--timeout"]}},
    inputs: ([("SANDBOX_MODE", "WORKSPACE_ROOT") | {name: "PI_ENSEMBLE_\(.)", scope: "sandbox"}]
      + [("MAX_WORKSTREAMS", "VERIFY_TIMEOUT_MS", "INTEGRATION_VERIFY_TIMEOUT_MS", "SUBAGENT_MODEL",
          "SUBAGENT_PROVIDER", "MODEL_<ROLE>", "PROVIDER_<ROLE>", "EXTRA_IMAGE_DIRS", "COMMENT_RETENTION",
          "HANDOFF_CHANNEL", "HANDOFF_TIMEOUT_MS", "CYCLE_DEADLINE", "VERIFY_CONCURRENCY",
          "SPAWN_TIMEOUT_MS")
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
# The kernel log, as journalctl -k -o cat prints it, from a file a case writes.
t_stub journalctl 'cat "$T/kernel.log"'
slice_good
env_ok=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")

runner=/opt/pi-rukas/bin/pi-rukas-headless
# The arguments of the docker call that starts the cycle.
cycle_args() { grep -l -x -e 'pi-rukas-headless' -e "$runner" "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
# The arguments of the docker call that checks the push before the cycle.
preflight_args() { grep -l -x -e '--entrypoint' "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
# The security options and the readers of a docker line come from
# test/docker_args.sh, which takes the list from tehas_sandbox_security itself,
# so the cycle, the turn, the session and the push check are all pinned against
# one source and not against a copy kept in step by hand.
# shellcheck source=test/docker_args.sh
. "$(dirname "$0")/docker_args.sh" || exit 1
# The options of lib/slice.sh and of tehas_sandbox_security, by name, for the
# allow-list of unknown_options: one pair of strings for every suite of a
# cycle's container, so an option added to either is allow-listed in all of them
# at once, and one taken away from the function is refused by every test.
limits="--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares"
docker_opts="$limits --cap-drop --cap-add --security-opt --tmpfs"
# Values that follow a flag, e.g. every -v or every -e.
after() { cycle_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }

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
PI_ENSEMBLE_SPAWN_TIMEOUT_MS
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
# to standard output, "progress <line>" one line of its NDJSON. With no
# argument the case means a container that does nothing of note: one that says
# one line of progress and ends at once, purely to keep the cycle's stream from
# being the empty one tehas work takes for a container that never came up and
# writes nothing for. A case of that end names its own container and says nothing.
# The file of what the container does is this case's own: fresh puts the default
# back, so a body never outlives the case that wrote it.
container() {
  # What the container of this case does. The error file of its log goes first,
  # so that a case reads only what its own run wrote there: a log is named to the
  # second, so a case can be given the name a neighbour used, and the file of
  # that name would read as this cycle's own. The path is held in three words of
  # its own, because this body is a string of the stub of docker and a glob of
  # the whole path would be answered too early to name the cycle's log.
  # shellcheck disable=SC2016  # the log name is known only when the run starts
  local body=${1:-}
  [ -n "$body" ] || body='progress cycle; exit 0'
  printf '%s\n' 'nap=$(command -pv sleep)' \
    'c=$T/home/tehas/logs; b=7; rm -f "$c"/work-$b-*.jsonl.err' \
    'says2() { echo "{\"schemaVersion\":5,\"buildCommit\":null,\"exitCode\":10,\"issues\":[{\"issue\":7,\"status\":\"$1\",\"cap\":\"${2:-}\",\"failedStep\":\"develop\",\"branch\":\"feature/issue-7-x\",\"steps\":[{\"step\":\"explore\"},{\"step\":\"develop\"}],\"usage\":{\"input\":5,\"output\":2,\"cacheRead\":1,\"turns\":3,\"dispatches\":2,\"byRole\":{}},\"failure\":${3:-null},\"exitCode\":10}]}"; }' \
    'says() { says2 "$1" "${2:-}"; }' \
    'progress() { printf "%s\n" "$1" >&2; }' \
    "$body" >"$T/container.sh"
}
# Alive until docker stop is called, then gone with 137, as a container is.
hold='"$nap" 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid"'
gone='wait; exit 137'
# A log is named to the second, so a cycle of the case before can share this
# case's name: its files go first, the body of the container the case before
# named among them (container puts the default back, and the file beside it,
# container-default, holds that default for every case that names none).
fresh() {
  local e t_keep=()
  rm -rf "$T"/calls/* "$live/cycles" "$T/sleeper.pid" "$T/container.sh" "$T"/home/tehas/logs/work-7-*
  t_store_home "$T/home/tehas"
  # A fresh store has no start, so no attempt of one (under_cycle).
  for e in "${T_ENV[@]}"; do [[ $e == TEHAS_CYCLE_ATTEMPT=* ]] || t_keep+=("$e"); done
  T_ENV=("${t_keep[@]}")
}
# No case leaves the old file of a record that is on the store.
t_after='t_no_old_state "$T/home/tehas"'

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
# issue_is <body> [association] [title]: the answer for issue 7 as REST gives
# it. A start is held to an answer that names an open issue, so the fixture
# carries the state the real answer carries. With <title> it carries a title
# too, which is what a start that binds one compares.
issue_is() {
  jq -n --arg body "$1" --arg who "${2:-OWNER}" --arg title "${3:-}" '{number: 7, state: "open", body: $body,
    author_association: $who} + (if $title == "" then {} else {title: $title} end)' >"$fx/issue.json"
}
images_on() {
  rm -rf "$fx" "$T"/calls/* "$T/home/tehas/work" "$T/home/tehas/state/cycles/7.manifest.json"
  # The body of the container of the case before, if it named one: fresh clears
  # it too, and the two helpers clear the rest of the host in their own way.
  rm -f "$T/container.sh"
  t_store_home "$T/home/tehas"
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
    "$T/home/tehas/state/cycles/7.manifest.json"
  # What the container of this cycle does is a file a case writes; it goes with
  # the rest of the host here, as fresh does for the cases that use it.
  rm -f "$T/container.sh"
  t_store_home "$T/home/tehas"
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
kept=$T/home/tehas/state/cycles/7.manifest.json
# The approval of the start, as the dispatcher records it on the state store,
# kept here in the shape tehas_approval_view gives it back (the shape the
# record file had). record_put puts it on a fresh store with a start that used
# it, so a run of tehas work on its own finds it as the latest start's. A
# fixer's or a refresh's rests on an earlier start of the person's approval in
# rests_on.
record_json=""
record_put() { t_record "$T/home/tehas" 7 "$record_json"; }
# under_cycle: a run as tehasbot cycle makes it by hand: its start on record,
# and tehas work given its attempt.
under_cycle() {
  local e t_keep=()
  t_started "$T/home/tehas" 7 --by-hand
  for e in "${T_ENV[@]}"; do [[ $e == TEHAS_CYCLE_ATTEMPT=* ]] || t_keep+=("$e"); done
  T_ENV=("${t_keep[@]}" "TEHAS_CYCLE_ATTEMPT=$(t_row cycle-show 7 -- .cycle.attempt)")
}
# record_edit <jq filter> [<jq args>...]: change the approval and put it again.
record_edit() {
  record_json=$(jq -c "$@" <<<"$record_json") || t_bad "record_edit: $*"
  record_put
}
# record_is <by> <body it approved> <link>...: the approval of a start. The
# hash is tehas_body_sha256's own, the one the dispatcher records and tehas work
# compares, so a body that differs only in line ends differs in nothing.
record_is() {
  local by=$1 body=$2
  shift 2
  record_json=$(jq -c -n --arg by "$by" --arg sum "$(jq -n --arg body "$body" '{body: $body}' | tehas_body_sha256)" '{at: "2026-10-14T00:00:00Z", by: $by,
    how: "label", body_sha256: $sum, links: $ARGS.positional}' --args "$@")
  record_put
}
# record_binds_title <title>: add the title's hash to the approval record_is
# made, as the dispatcher records it next to the body's.
record_binds_title() {
  record_edit --arg sum "$(jq -n --arg t "$1" '{title: $t}' | tehas_title_sha256)" '.title_sha256 = $sum'
}

# Most cases run with this instance and environment, and the state store
# with nothing on record.
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
T_ENV=("${env_ok[@]}")
t_store_home "$T/home/tehas"
