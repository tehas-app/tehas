#!/usr/bin/env bash
# Tests for tehas plan, one turn of a planning session: a first turn, its
# container and its runner, the issue's images and a reply's attachments.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/plan_lib.sh
. "$(dirname "$0")/plan_lib.sh" || exit 1

t_case "without Pi's settings.json no turn starts and nothing is written"
reset
mv "$T/home/.pi/agent/settings.json" "$T/settings.kept"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 78
t_contains "$t_out" "no $T/home/.pi/agent/settings.json: run install.sh"
t_eq "$(t_calls docker)" ""
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(find "$T/out.json" "$T/keep" -maxdepth 0 2>/dev/null | wc -l)" 0
mv "$T/settings.kept" "$T/home/.pi/agent/settings.json"

t_case "without the console beside it no turn starts: 78, named"
reset
mv "$shim/libexec/console" "$T/console.kept"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: no console at $shim/libexec/console: run install.sh"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
mv "$T/console.kept" "$shim/libexec/console"

t_case "an image whose runner takes no planning turns starts nothing: 78, named, before any clone"
for caps in '{"contract":"0.12.80","features":{"handoffChannel":true}}' '{"features":{"planTurns":false}}' 'not json' ''; do
  reset
  printf '%s\n' "$caps" >"$T/caps.json"
  rm -rf "$T/home/tehas/work"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  t_eq "$t_rc" 78
  t_contains "$t_out" "runs no planning turns (its --capabilities has no features.planTurns)"
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_eq "$(find "$T/home/tehas/work" "$T/out.json" "$T/keep" -maxdepth 0 2>/dev/null | wc -l)" 0
done

t_case "a first turn writes the session, the turn's text and the planner's question with its proposal"
reset
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .session "$T/out.json")" "abc-123"
t_eq "$(jq -r .text "$T/out.json")" "The plan needs answers before it can be drafted (1 question)."
t_eq "$(jq -r .question.question "$T/out.json")" "Which store, SQLite or files?
   Proposed: SQLite"
t_eq "$(jq -c .question.options "$T/out.json")" "[]"
t_eq "$(jq -r 'has("filed")' "$T/out.json")" false

t_case "the approved text is in the workspace as a file git ignores, followed by the issue brief"
t_eq "$(cat "$T/calls/brief.seen")" "$(with_brief "$T/brief.md")"
t_contains "$(cat "$T/calls/brief.seen")" "## How to write the plan: tehas"
t_eq "$(cat "$T/calls/tree.status")" ""

t_case "only the brief and the answer are hidden from git; the rest of .tehas stays visible"
t_contains "$(cat "$T/calls/exclude.seen")" "/.tehas/brief.md"
t_contains "$(cat "$T/calls/exclude.seen")" "/.tehas/answer.md"
t_lacks "$(grep -x '/.tehas/' "$T/calls/exclude.seen")" ".tehas/"
t_contains "$(cat "$T/calls/tree.status.definition")" ".tehas/definition/"

t_case "the workspace is the turn's own, and it is gone when the turn is over"
t_eq "$([ -e "$tree" ] && echo left)" ""
t_eq "$([ -e "$ws" ] && echo made)" ""
t_eq "$([ -e "$state" ] && echo left)" ""

t_case "a first turn starts a session: the title is what to plan, the approved text is the context"
t_eq "$(after "$runner")" plan
t_eq "$(tail_of)" "$image $runner plan Faster search (issue #7) --start --context $(with_brief "$T/brief.md" | tr '\n' ' ')"

# The descriptor is the console's (console/src/plan.rs), its rules pinned
# there; here the turn is pinned to them.
t_case "a first turn asks the console for the descriptor, and a title of six words is handed over with no number"
long="A planned issue title names the work in full: no cut-off suffix"
reset
# The keep directory of the case before this one holds its brief; a first turn
# remakes it, but only once it has the descriptor of this one in hand.
rm -rf "$T/keep"
printf '# %s\n\nMake it fast.\n' "$long" >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan $long --start --context $(with_brief "$T/brief.md" | tr '\n' ' ')"
t_lacks "$(tail_of)" "(issue #7)"

t_case "a first turn plans the Title: line of a brief, so a half of a split is named for its own work"
reset
rm -rf "$T/keep"
printf 'Title: A Rust renderer of the review issue\n\n# The parent\n\nBody.\n' >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
# The context is the approved text, which holds the parent's heading; what
# names the half is the descriptor, before --start.
t_eq "$(tail_of)" "$image $runner plan A Rust renderer of the review issue --start --context $(with_brief "$T/brief.md" | tr '\n' ' ')"

t_case "a brief with no title at all is planned as the work in the brief's file"
reset
rm -rf "$T/keep"
printf '\nMake it fast.\n' >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan the work in .tehas/brief.md (issue #7) --start --context $(with_brief "$T/brief.md" | tr '\n' ' ')"

# These cases run the turn whose outcome they read.

t_case "the runner's exit status is kept for the next turn"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$(cat "$T/keep/exit")" 11

t_case "the planning container gets the mounts of a work cycle, no more"
want="$tree:$ws
$T/home/.pi/agent/ensemble-runs/plan-7:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$state/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro
$T/home/tehas/state/github-session:/run/tehas/github:ro"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the turn's container is given the narrowed token, never the app's"
tok=$(grep -l -x -e /opt/pi-rukas/bin/pi-rukas-headless "$T"/calls/docker.args.* | head -n 1 | sed 's/args/token/')
t_eq "$(cat "$tok")" "ghs_session"

t_case "the turn is given its own cache, memory and sessions, and no shared one"
t_lacks "$(after -v)" "pi-ensemble"
t_lacks "$(after -v)" "$T/home/.pi/agent/sessions"
t_lacks "$(after -v)" "$T/home/.vipune"
t_contains "$(after -e)" "CBM_CACHE_DIR=/home/vscode/.tehas-cache"

t_case "the planning container gets the variables of a work cycle, no more"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$cycle_vars" | LC_ALL=C sort)"

t_case "the instance places no session and sets no plan cache: the session stays in the clone"
reset
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_PLAN_SESSION_DIR=/elsewhere PI_ENSEMBLE_PLAN_CACHE_TTL_MS=60000
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(after -e | grep -c -e '^PI_ENSEMBLE_PLAN_SESSION_DIR' -e '^PI_ENSEMBLE_PLAN_CACHE_TTL_MS')" 0
t_lacks "$(turn_args)" "/elsewhere"

t_case "the planning container gets no option beyond the documented ones"
t_eq "$(turn_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm --init")" ""
t_eq "$(turn_args | count_of --tmpfs)" "1"
t_eq "$(turn_args | grep -c -x -e --init)" 1
t_eq "$(turn_args | grep -c -x -e -i)" 0
t_eq "$(after --name)" "tehas-plan-7"
t_eq "$(after -w)" "$ws"

t_case "nothing that controls the host is reachable, and one turn container ran after the capabilities were asked"
args=$(cat "$T"/calls/docker.args.*)
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "--privileged"
t_lacks "$args" "test-token"
t_lacks "$args" "$checkout"
t_eq "$(grep -l -x -e plan "$T"/calls/docker.args.* | wc -l)" 1
caps_at=$(t_calls docker | grep -n -F -e "run --pull=never --rm --init $image $runner --capabilities" | cut -d: -f1 | head -n 1)
run_at=$(t_calls docker | grep -n -e " $runner plan " | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$caps_at" ] && [ -n "$run_at" ] && [ "$caps_at" -lt "$run_at" ] && echo before)" before

t_case "a container left under the turn's name is removed before the turn starts"
rm_at=$(t_calls docker | grep -n -x -e 'rm -f tehas-plan-7' | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$rm_at" ] && [ -n "$run_at" ] && [ "$rm_at" -lt "$run_at" ] && echo before)" before

t_case "the turn gets the slice, a cycle's limits and no privileges, and the host aliases from the settings"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=1g PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(after --cgroup-parent)" "tehas.slice"
t_eq "$(after --memory)" "2048m"
t_eq "$(after --memory-swap)" "2048m"
t_eq "$(after --memory-reservation)" "1g"
t_eq "$(after --pids-limit)" "4096"
t_eq "$(after --cpu-shares)" "1024"
t_eq "$(after --cpus)" "15.00"
t_eq "$(turn_args | security_line)" "$security_opts "
t_eq "$(turn_args | security_of --cap-add | LC_ALL=C sort)" "CHOWN
FOWNER"
t_eq "$(turn_args | security_of --security-opt)" "no-new-privileges:true"
# One bounded /tmp, as a cycle's container has (console/cycle/tests/work_container.rs),
# read from the function and counted beside security_line.
t_eq "$(turn_args | security_of --tmpfs)" "$security_tmpfs"
t_eq "$(turn_args | count_of --tmpfs)" "1"
# The image's entrypoint drops from root with setpriv, which the dropped
# SETUID and SETGID refuse: the container starts as the host's user instead.
t_eq "$(after --user)" "$(id -u):$(id -g)"
t_lacks "$(turn_args)" "--cidfile"
t_contains "$(turn_args)" "--add-host=models:192.0.2.9"
t_eq "$(turn_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm --init")" ""

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

# A turn has a workspace and a transcripts directory of its own on the host.
# They are mounted at the paths a cycle's are.
# The options without the pairs only a cycle is given: who reports its
# handoff and the time it may take, its deadline and its gate runs at once.
# The cycle's own: its five inputs and its issue relay, which a turn has not.
without_cycle_only() {
  awk 'held != "" {
      own = (held == "-e" && $0 ~ /^PI_ENSEMBLE_(HANDOFF_CHANNEL|HANDOFF_TIMEOUT_MS|CYCLE_DEADLINE|VERIFY_CONCURRENCY|SPAWN_TIMEOUT_MS)=/) ||
        (held == "-v" && $0 ~ /:\/run\/tehas\/relay$/)
      if (own) { held = ""; next }
      print held
    }
    { held = ($0 == "-e" || $0 == "-v") ? $0 : "" } held == "" { print } END { if (held != "") print held }'
}
t_case "docker is given what tehas work gives a cycle's container: only the name, the two directories of its own, the cycle's cid file, the cycle's own five inputs and relay, and what is run differ"
turn_args | options |
  sed -e "s|^$tree:|$ws-7:|" -e 's|/ensemble-runs/plan-7:|/ensemble-runs/issue-7:|' -e 's|widget-plan-7\.state|widget-7.state|' -e 's|/state/github-session:/run/tehas/github|/state/github-cycle:/run/tehas/github|' >"$T/plan.options"
rm -f "$T"/calls/*
# What the image says it supports: every input the container is given.
jq -n '{contracts: {exitCodes: {version: 1}, resultSchema: {version: 5}, ndjson: {stable: {version: 1}}},
  verbs: {work: {flags: ["--restart", "--json", "--timeout"]}},
  inputs: [("SANDBOX_MODE", "WORKSPACE_ROOT", "MAX_WORKSTREAMS", "VERIFY_TIMEOUT_MS", "INTEGRATION_VERIFY_TIMEOUT_MS", "VERIFY_SUITE_TIMEOUT_MS",
    "SUBAGENT_MODEL", "SUBAGENT_PROVIDER", "HANDOFF_CHANNEL", "HANDOFF_TIMEOUT_MS", "CYCLE_DEADLINE", "VERIFY_CONCURRENCY", "SPAWN_TIMEOUT_MS")
    | {name: "PI_ENSEMBLE_\(.)", scope: "cycle"}],
  features: {handoffChannel: true, cycleDeadline: true}}' >"$T/caps.json"
# tehas work reads its approval from the state store: nothing on record, a run
# by hand.
t_store_home "$T/home/tehas"
t_verb "$shim/libexec/work" tehas work
t_renewer "$T/home/tehas"
T_ENV+=(GH_CYCLE_TOKEN=cycle-token)
t_run "$shim/libexec/work 7"
unset 'T_ENV[-1]'
turn_args | options | without_cycle_only >"$T/work.options"
t_eq "$(turn_args | grep -c -e '^PI_ENSEMBLE_HANDOFF_CHANNEL=host$' -e '^PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=480000$' -e '^PI_ENSEMBLE_CYCLE_DEADLINE=' -e '^PI_ENSEMBLE_VERIFY_CONCURRENCY=')" 4
t_eq "$([ -s "$T/work.options" ] && echo recorded)" recorded
t_contains "$(turn_args)" "--cidfile"
# Each -e that names a PI_ENSEMBLE_ variable without a value, sorted in place
# of the run of them: a shell and the console keep the environment in different
# orders, so the names are compared as a set.
env_names_sorted() {
  awk -v sorter='LC_ALL=C sort | tr " " "\n"' '
    function flush() { if (held) { fflush(); close(sorter); held = 0 } }
    want { want = 0
      if ($0 ~ /^PI_ENSEMBLE_[A-Z0-9_]+$/) { print "-e " $0 | sorter; held = 1; next }
      flush(); print "-e"; print; next }
    /^-e$/ { want = 1; next }
    { flush(); print }
    END { if (want) print "-e"; flush() }'
}
t_eq "$(env_names_sorted <"$T/plan.options")" "$(env_names_sorted <"$T/work.options")"
t_eq "$(tail_of)" "$image /opt/pi-rukas/bin/pi-rukas-headless work 7 --timeout 360 --json "

t_case "a first turn names its issue with --issue when the runner takes it, so the issue is no duplicate of itself"
caps_issue() {
  jq -n '{contract: "0.14.0", features: {planTurns: true},
    verbs: {plan: {flags: ["--start", "--context", "--resume", "--answer", "--confirm", "--issue"]}}}' >"$T/caps.json"
}
reset
caps_issue
runner_keeps '{"turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan Faster search (issue #7) --start --context $(with_brief "$T/brief.md" | tr '\n' ' ')--issue 7 "

t_case "a first turn gives the title's own type with --type when the runner takes it, and plans the rest"
# pi-rukas reads no type off a descriptor without chore's word in it and plans
# a feature, so the type the title is taken off goes over by itself.
reset
jq -n '{contract: "0.14.0", features: {planTurns: true},
  verbs: {plan: {flags: ["--start", "--context", "--resume", "--answer", "--confirm", "--type"]}}}' >"$T/caps.json"
runner_keeps '{"turn":1}'
printf '# chore: Give the sync captured_logs a bound of its own\n\nBody.\n' >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan Give the sync captured_logs a bound of its own --start --context $(with_brief "$T/brief.md" | tr '\n' ' ')--type chore "
# A runner that takes no --type finds chore in the words: the type stays there.
reset
printf '# chore: Give the sync captured_logs a bound of its own\n\nBody.\n' >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan chore: Give the sync captured_logs a bound of its own --start --context $(with_brief "$T/brief.md" | tr '\n' ' ')"

t_case "a resumed turn never passes --issue: the session keeps it and the runner refuses it there"
first_turn_ran
caps_issue
printf 'yes\n' >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan --resume abc-123 --answer $ws/.tehas/answer.md "

# --- The issue's images ----------------------------------------------------------
A=https://github.com/user-attachments/assets
t_stub gh '
printf "%s\n" "$*" >>"$T/calls/gh.args"
case "$1 $2" in
  "api repos/acme/widget/issues/7") cat "$T/fx/issue.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/issues/7/events?per_page=100") cat "$T/fx/events.json" 2>/dev/null || echo "[]" ;;
  "issue comment") printf "%s\n" "$*" >>"$T/calls/gh.comments" ;;
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
printf "%s\n" "$url" >>"$T/calls/curl.urls"
[ -f "$T/fx/net/$key" ] || exit 22
cp "$T/fx/net/$key" "$out"
printf application/octet-stream'
net() { # <address> <printf format>
  mkdir -p "$fx/net"
  # shellcheck disable=SC2059  # the format carries the bytes of an image
  printf "$2" >"$fx/net/$(printf %s "$1" | tr -c 'A-Za-z0-9' _)"
}
issue_is() { jq -n --arg body "$1" --arg who "${2:-OWNER}" '{number: 7, body: $body, author_association: $who}' >"$fx/issue.json"; }
images_on() {
  reset
  rm -rf "$fx/net" "$fx/events.json"
  reads_image 1
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
}
# What the sandbox image says about its own pi-rukas, as host/sandbox/build.sh
# writes it: <commit>|<reads>.
img_commit=$(printf 'c%.0s' {1..40})
reads_image() { # <1, 0, or none when the image carries no label>
  rm -f "$T/image.label"
  if [ "$1" != none ]; then printf '%s\n' "$img_commit|$1" >"$T/image.label"; fi
}

t_case "with images on a first turn fetches them into the session's directory and mounts a copy read-only"
images_on
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "Look ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.issue, (.images | map(.file))]' "$T/keep/images/manifest.json")" '[7,["image-1.png"]]'
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$ws.images"
t_eq "$(turn_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm --init")" ""
t_contains "$(cat "$T/calls/gh.comments")" "Tehas gave this planning session 1 image from the issue text."
t_eq "$([ -e "$tree.images" ] && echo left)" ""

t_case "a later turn mounts the same images and fetches nothing"
runner_keeps '{"turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
rm -f "$fx/keeps"
rm -f "$T"/calls/*
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" "$T/calls/gh.args" 2>/dev/null)" ""
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1

t_case "an image that cannot be fetched starts no session: exit 4, said on the issue"
images_on
issue_is "Look ![shot]($A/9999)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_contains "$(cat "$T/calls/gh.comments")" "Tehas started no planning session: $A/9999 could not be fetched."
t_eq "$([ -e "$T/keep/images" ] && echo left)" ""
t_eq "$([ -e "$T/out.json" ] && echo written)" ""

t_case "a first turn that stops before its workspace, in a home with no work directory, leaves that directory marked as tehas's"
images_on
issue_is "Look ![shot]($A/9999)"
rm -rf "$T/home/tehas/work"
t_run "$plan 7 --out $T/out.json --keep $ws-plan-7.keep --brief $T/brief.md"
t_eq "$t_rc" 4
t_eq "$([ -f "$T/home/tehas/work/.tehas-workspaces" ] && echo marked)" marked
rm -rf "$ws-plan-7.keep"

t_case "an outsider's issue gives its images when an allowed account applied tehas:plan, and not otherwise"
for who in alice mallory; do
  images_on
  net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
  issue_is "Look ![shot]($A/1111)" NONE
  jq -n --arg who "$who" '[{event: "labeled", label: {name: "tehas:plan"}, actor: {login: $who}, created_at: "2026-10-11T00:00:00Z"}]' >"$fx/events.json"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  t_eq "$t_rc" 0
  t_eq "$who $(jq -c '.images | length' "$T/keep/images/manifest.json")" "$who $([ "$who" = alice ] && echo 1 || echo 0)"
done

t_case "tehas:refine from an allowed account approves an outsider's images as tehas:plan does"
images_on
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "Look ![shot]($A/1111)" NONE
jq -n '[{event: "labeled", label: {name: "tehas:refine"}, actor: {login: "alice"}, created_at: "2026-10-11T00:00:00Z"}]' >"$fx/events.json"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c '.images | length' "$T/keep/images/manifest.json")" 1

t_case "an instance that still sets descriptions writes no images.md, and the turn gets the same mounts and variables"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_DESCRIBE=1
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "Look ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/images" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./manifest.json "
t_eq "$(jq -r .described "$T/keep/images/manifest.json")" null
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$cycle_vars" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(turn_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm --init")" ""

t_case "an image the sandbox image does not read is said as fetched and not shown"
images_on
reads_image 0
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "Look ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_lacks "$(cat "$T/calls/gh.comments")" "Tehas gave this planning session"
t_contains "$(cat "$T/calls/gh.comments")" "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this planning session. The pi-rukas in the sandbox image does not read them, so no agent was shown one."

t_case "with images off nothing is fetched or mounted, and no instance line names a directory the turn does not have"
reset
t_instance "${T_MINIMAL[@]}" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" "$T/calls/gh.args" 2>/dev/null)" ""
t_eq "$(after -v | grep -c images)" 0
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" 0
t_lacks "$(after -e)" "/elsewhere"
t_lacks "$(cat "$T"/calls/docker.args.*)" "/elsewhere"

t_case "with images off a reply's links are not fetched and the turn is not told the issue, whatever the instance says"
reset
runner_keeps '{"turn":1}'
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_PLAN_ISSUE=99
net "$A/2222" '\x89PNG\r\n\x1a\nPNGDATA'
printf 'The operator replied on the issue:\n\n![b](%s/2222)\n' "$A" >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" 2>/dev/null)" ""
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_PLAN_ISSUE')" 0
t_eq "$(after -v | grep -c images)" 0

# --- A reply's attachments -----------------------------------------------------
reply_is() { printf 'The operator replied on the issue:\n\n%s\n' "$1" >"$T/answer.md"; }
# A session whose first turn took one image from the issue.
first_turn() {
  images_on
  net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
  issue_is "Look ![shot]($A/1111)"
runner_keeps '{"turn":1}'
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  rm -f "$T"/calls/*
}

t_case "a reply with a link adds its image to the session, numbered on, and the turn sees it"
first_turn
net "$A/2222" '\xff\xd8\xff\xe0JPGDATA'
reply_is "Like this ![b]($A/2222), not like ![shot]($A/1111), and see https://example.com/x.png"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 0
t_eq "$(jq -c '.images | map([.file, .link, .source])' "$T/keep/images/manifest.json")" \
  "[[\"image-1.png\",\"$A/1111\",\"body\"],[\"image-2.jpg\",\"$A/2222\",\"answer:4003\"]]"
t_eq "$(cat "$T/calls/curl.urls")" "$A/2222"
t_eq "$(jq -c '.images | map(.file)' "$T/calls/manifest.seen")" '["image-1.png","image-2.jpg"]'
t_eq "$(cd "$T/keep/images" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./image-2.jpg ./manifest.json "

t_case "the answer sent to the agent names the new file"
t_contains "$(cat "$T/calls/answer.seen")" "Like this"
t_contains "$(cat "$T/calls/answer.seen")" "- image-2.jpg, from $A/2222"

t_case "the turn is told the issue, and gets no other mount or variable than a first turn"
t_eq "$(after -e | grep '^PI_ENSEMBLE_PLAN_ISSUE')" "PI_ENSEMBLE_PLAN_ISSUE=7"
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$cycle_vars" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(turn_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm --init")" ""

t_case "a reply over the limits leaves the link out and says so on the issue"
first_turn
for i in 2 3 4 5 6 7 8 9; do net "$A/000$i" '\x89PNG\r\n\x1a\nPNGDATA'; done
reply_is "$(for i in 2 3 4 5 6 7 8 9; do printf '![x](%s/000%s) ' "$A" "$i"; done)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 0
t_eq "$(jq -c '[(.images | length), (.leftOut | map([.link, .reason, .source]))]' "$T/keep/images/manifest.json")" \
  "[8,[[\"$A/0009\",\"More than 8 images.\",\"answer:4003\"]]]"
t_contains "$(cat "$T/calls/gh.comments")" "Tehas left out of your reply:
- $A/0009: More than 8 images."

t_case "with no attachment checker a reply that links nothing runs its turn"
first_turn
mv "$T/home/tehas/libexec/attach" "$T/attach.away"
reply_is "No link, only https://example.com/x.png"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 0
t_contains "$(cat "$T/calls/answer.seen")" "No link"
t_eq "$(cat "$T/calls/gh.comments" 2>/dev/null)" ""

t_case "with no attachment checker a reply that links a file runs no turn, and no host path is said: exit 4"
reply_is "![b]($A/2222)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 4
t_contains "$(cat "$T/calls/gh.comments")" "the issue's files could not be checked (the attachment checker is not installed)"
t_lacks "$(cat "$T/calls/gh.comments")" "$T/"
mv "$T/attach.away" "$T/home/tehas/libexec/attach"

t_case "a reply's link that cannot be fetched runs no turn and leaves the session's images as they were: exit 4"
first_turn
cp "$T/keep/images/manifest.json" "$T/manifest.before"
reply_is "![b]($A/9999)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md --comment 4003"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(cmp "$T/manifest.before" "$T/keep/images/manifest.json" && echo same)" same
t_eq "$(cd "$T/keep" && find . -mindepth 1 -maxdepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./brief.md ./commit ./exit ./images ./plan-sessions "
t_contains "$(cat "$T/calls/gh.comments")" "Tehas ran no planning turn: $A/9999 could not be fetched."

t_case "without --comment a reply's links are not read"
first_turn
net "$A/2222" '\xff\xd8\xff\xe0JPGDATA'
reply_is "![b]($A/2222)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" 2>/dev/null)" ""

for bad in "--brief $T/brief.md --comment 4003" "--session abc-123 --answer $T/brief.md --comment 12a"; do
  t_case "--comment where it does not belong ('$bad') is refused"
  reset
  t_run "$plan 7 --out $T/out.json --keep $T/keep $bad"
  t_eq "$t_rc" 64
done
t_done
