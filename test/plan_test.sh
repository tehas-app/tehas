#!/usr/bin/env bash
# Tests for libexec/plan: one turn of a planning session. A stub docker
# records its arguments and answers as pi-rukas's headless runner does: one
# JSON document on stdout and an exit status, so no model runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'

plan="$T_ROOT/libexec/plan"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
# The directory on the host behind it, for issue 7, and the turn's own state.
tree=$ws-plan-7
state=$tree.state
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"
# One case runs tehas work, which parks a cycle of a product whose commit names
# no gate and holds no stack it can guess a gate from (libexec/work). The gate
# is read at the commit the clone is made at, so it is in a commit here, unlike
# the fixture of the work files, which reads the checkout too. The work files
# hold the same gate their way.
mkdir -p "$upstream/.pi"
printf 'scripts/verify.sh\n' >"$upstream/.pi/verify-cmd"
git -C "$upstream" add -f -- .pi/verify-cmd
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q -m gate
git -C "$checkout" fetch -q "$upstream" +refs/heads/main:refs/remotes/origin/main
git -C "$checkout" checkout -q main


# The turn's workspace is gone when the turn is over, so the container notes
# what it was handed there. What the runner answers is $T/fx/doc.json, with
# the exit status in $T/fx/doc.rc; what it keeps of the session, when a case
# says, is written by $T/fx/keeps.
runner=/opt/pi-rukas/bin/pi-rukas-headless
t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
# tehas plan and tehas work ask the image what it supports.
case " $* " in *" --capabilities "*) cat "$T/caps.json" 2>/dev/null; exit ;; esac
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
case " $* " in
  *" stop "*) : >"$T/calls/stopped" ;;
  *" /opt/pi-rukas/bin/pi-rukas-headless plan "*)
  ws=$T/home/tehas/work/widget-plan-7
  cat "$ws/.tehas/brief.md" >"$T/calls/brief.seen" 2>/dev/null
  cat "$ws/.tehas/answer.md" >"$T/calls/answer.seen" 2>/dev/null
  git -C "$ws" status --porcelain >"$T/calls/tree.status" 2>&1
  cp "$ws/.git/info/exclude" "$T/calls/exclude.seen" 2>/dev/null
  mkdir -p "$ws/.tehas/definition" && echo "{}" >"$ws/.tehas/definition/app.json"
  git -C "$ws" status --porcelain -uall >"$T/calls/tree.status.definition" 2>&1
  git -C "$ws" rev-parse HEAD >"$T/calls/head.seen" 2>&1
  cat "$ws.images/manifest.json" >"$T/calls/manifest.seen" 2>/dev/null
  (cd "$ws/.git/pi-plan-sessions" 2>/dev/null && find . -type f | LC_ALL=C sort) >"$T/calls/sessions.given"
  cat "$ws/.git/pi-plan-sessions/abc-123/session.json" >"$T/calls/sessions.first" 2>/dev/null
  [ ! -e "$T/fx/keeps" ] || . "$T/fx/keeps"
  [ ! -e "$T/fx/live" ] || . "$T/fx/live"
  cat "$T/fx/doc.json" 2>/dev/null
  exit "$(cat "$T/fx/doc.rc" 2>/dev/null || echo 11)" ;;
  *" image inspect "*) cat "$T/image.label" 2>/dev/null ;;
esac'
# The command looks at the container once a second. Here a look costs no time.
t_stub sleep
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# Pi's settings as install.sh leaves them on a host.
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
limits="--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares"
# The security options and the readers of a docker line come from
# test/docker_args.sh, which takes the list from tehas_sandbox_security itself,
# so the cycle, the turn, the session and the push check are all pinned against
# one source and not against a copy kept in step by hand. docker_opts is the
# allow-list of unknown_options: the slice's options and the security ones, by
# name, so an option added to either is allow-listed in all of them at once.
# shellcheck source=test/docker_args.sh
. "$(dirname "$0")/docker_args.sh" || exit 1
docker_opts="$limits --cap-drop --cap-add --security-opt --tmpfs"

turn_args() { grep -l -x -e /opt/pi-rukas/bin/pi-rukas-headless "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { turn_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
image="tehas/pi-rukas:uid$(id -u)"
tail_of() { turn_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }
# unknown_options, count_of and the readers of the security options come from
# test/docker_args.sh, sourced above: one implementation for all four containers.
# What docker is given before the image, one argument per line, without the
# container's name and without the cid file, which only a cycle has.
options() {
  awk -v image="$image" '$0 == image { exit } $0 == "--cidfile" { skip = 1; next } skip { skip = 0; next }
    prev == "--name" { $0 = "<name>" } { print; prev = $0 }'
}


# What the runner answers. turn_doc <exit status> <document>.
turn_doc() { printf '%s\n' "$2" >"$fx/doc.json"; printf '%s\n' "$1" >"$fx/doc.rc"; }
# A turn that asked one question, as pi-rukas numbers them.
asked() {
  turn_doc 11 '{"subcommand":"plan","mode":"turn","exitCode":11,"status":"question","session":{"id":"abc-123","turn":1},
    "text":"The plan needs answers before it can be drafted (1 question).",
    "questions":[{"id":"q1","question":"Which store, SQLite or files?","options":[],"resolution":"SQLite"}]}'
}
# A turn that drafted a plan.
shown() {
  turn_doc 12 '{"subcommand":"plan","mode":"turn","exitCode":12,"status":"shown","session":{"id":"abc-123","turn":2},
    "text":"Drafted plan: Faster search. It has 1 open point: answer them, ask for changes, or confirm to file it as it is.",
    "title":"Faster search","spec":"## Context\n\nSearch is slow.\n\n## Acceptance criteria\n\n- p95 under 100 ms","reviewedDigest":"d1",
    "questions":[{"id":"q1","question":"Index on the title too?","options":[],"resolution":"Yes, both"}]}'
}
# What the runner keeps of the session in the clone's git directory, as it
# would leave it: runner_keeps <the text of its record>.
runner_keeps() {
  cat >"$fx/keeps" <<EOF
mkdir -p "\$ws/.git/pi-plan-sessions/abc-123"
printf '%s\n' '$1' >"\$ws/.git/pi-plan-sessions/abc-123/session.json"
EOF
}
caps_plan() { jq -n '{contract: "0.14.0", features: {planTurns: true}}' >"$T/caps.json"; }
reset() {
  rm -rf "$T"/calls/* "$T/out.json" "$fx/live" "$fx/keeps" "$T/keep"
  asked
  caps_plan
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  printf '# Faster search\n\nMake it fast.\n' >"$T/brief.md"
}
# A session whose first turn ran and kept its record.
first_turn_ran() {
  reset
  runner_keeps '{"turn":1}'
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  rm -f "$fx/keeps" "$T"/calls/*
}

for bad in "" "7" "x --out $T/out.json --keep $T/keep --brief $T/brief.md" "7 --out $T/out.json --keep $T/keep" \
  "7 --out $T/out.json --brief $T/brief.md" "7 --keep $T/keep --brief $T/brief.md" \
  "7 --out $T/out.json --keep $T/keep --brief $T/brief.md --session abc" "7 --out $T/out.json --keep $T/keep --session abc" \
  "7 --out $T/out.json --keep $T/keep --brief $T/missing.md" "7 --out $T/out.json --keep $T/keep --session 'a b;c' --answer $T/brief.md" \
  "7 --out $T/out.json --keep $T/keep --brief $T/brief.md --verbose"; do
  t_case "a call that is not a turn ('$bad') is refused before anything runs"
  reset
  t_run "$plan $bad"
  t_eq "$t_rc" 64
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
done

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

t_case "the approved text is in the workspace as a file git ignores"
t_eq "$(cat "$T/calls/brief.seen")" "$(cat "$T/brief.md")"
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
t_eq "$(tail_of)" "$image $runner plan Plan issue #7: Faster search --start --context # Faster search  Make it fast. "

t_case "the runner's exit status is kept for the next turn"
t_eq "$(cat "$T/keep/exit")" 11

t_case "the planning container gets the mounts of a work cycle, no more"
want="$tree:$ws
$T/home/.pi/agent/ensemble-runs/plan-7:/home/vscode/.pi/agent/ensemble-runs
$state/cache:/home/vscode/.tehas-cache
$state/memory:/home/vscode/.vipune
$state/sessions:/home/vscode/.pi/agent/sessions
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$state/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the turn is given its own cache, memory and sessions, and no shared one"
t_lacks "$(after -v)" "pi-ensemble"
t_lacks "$(after -v)" "$T/home/.pi/agent/sessions"
t_lacks "$(after -v)" "$T/home/.vipune"
t_contains "$(after -e)" "CBM_CACHE_DIR=/home/vscode/.tehas-cache"

t_case "the planning container gets the variables of a work cycle, no more"
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
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

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
caps_at=$(t_calls docker | grep -n -F -e "run --rm --init $image $runner --capabilities" | cut -d: -f1 | head -n 1)
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
t_eq "$(after --cpus)" "3.75"
t_eq "$(turn_args | security_line)" "$security_opts "
t_eq "$(turn_args | security_of --cap-add | LC_ALL=C sort)" "CHOWN
FOWNER"
t_eq "$(turn_args | security_of --security-opt)" "no-new-privileges:true"
# One bounded /tmp, as a cycle's container has: see test/work_test.sh. The
# mount is named by reading it from the function, not copied here. The whole
# line is counted beside security_line, so a second mount outside the block it
# matches fails, and the equals form docker also accepts counts as a mount too.
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
without_cycle_only() {
  awk 'held { if ($0 !~ /^PI_ENSEMBLE_(HANDOFF_CHANNEL|HANDOFF_TIMEOUT_MS|CYCLE_DEADLINE|VERIFY_CONCURRENCY)=/) print "-e"; else { held = 0; next } }
    { held = ($0 == "-e") } !held { print } END { if (held) print "-e" }'
}
t_case "docker is given what tehas work gives a cycle's container: only the name, the two directories of its own, the cycle's cid file, the cycle's own four inputs and what is run differ"
turn_args | options |
  sed -e "s|^$tree:|$ws-7:|" -e 's|/ensemble-runs/plan-7:|/ensemble-runs/issue-7:|' -e 's|widget-plan-7\.state|widget-7.state|' >"$T/plan.options"
rm -f "$T"/calls/*
# What the image says it supports: every input the container is given.
jq -n '{contracts: {exitCodes: {version: 1}, resultSchema: {version: 5}, ndjson: {stable: {version: 1}}},
  verbs: {work: {flags: ["--restart", "--json", "--timeout"]}},
  inputs: [("SANDBOX_MODE", "WORKSPACE_ROOT", "MAX_WORKSTREAMS", "VERIFY_TIMEOUT_MS", "INTEGRATION_VERIFY_TIMEOUT_MS",
    "SUBAGENT_MODEL", "SUBAGENT_PROVIDER", "HANDOFF_CHANNEL", "HANDOFF_TIMEOUT_MS", "CYCLE_DEADLINE", "VERIFY_CONCURRENCY")
    | {name: "PI_ENSEMBLE_\(.)", scope: "cycle"}],
  features: {handoffChannel: true, cycleDeadline: true}}' >"$T/caps.json"
t_run "$T_ROOT/libexec/work 7"
turn_args | options | without_cycle_only >"$T/work.options"
t_eq "$(turn_args | grep -c -e '^PI_ENSEMBLE_HANDOFF_CHANNEL=host$' -e '^PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=480000$' -e '^PI_ENSEMBLE_CYCLE_DEADLINE=' -e '^PI_ENSEMBLE_VERIFY_CONCURRENCY=')" 4
t_eq "$([ -s "$T/work.options" ] && echo recorded)" recorded
t_contains "$(turn_args)" "--cidfile"
t_eq "$(cat "$T/plan.options")" "$(cat "$T/work.options")"
t_eq "$(tail_of)" "$image /opt/pi-rukas/bin/pi-rukas-headless work 7 --timeout 360 --json "

# --- Later turns and what the runner answers --------------------------------------

t_case "a later turn resumes the session and gives the answer as it was written, as a file in the workspace"
first_turn_ran
printf '%s\n' '- "Adopt all", but keep `tagged-at` open' '' "second line with 'quotes' and \$HOME" >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan --resume abc-123 --answer $ws/.tehas/answer.md "
t_eq "$(cat "$T/calls/answer.seen")" "$(cat "$T/answer.md")"
t_eq "$(cat "$T/calls/tree.status")" ""
t_eq "$([ -e "$ws/.tehas/brief.md" ] && echo present)" ""

t_case "a drafted plan is posted with its title and spec, its open points with the proposals, and File this plan"
reset
shown
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" "Drafted plan: Faster search. It has 1 open point: answer them, ask for changes, or confirm to file it as it is.

### Faster search

## Context

Search is slow.

## Acceptance criteria

- p95 under 100 ms"
t_eq "$(jq -r .question.question "$T/out.json")" "Open points, with the planner's proposals:

1. Index on the title too?
   Proposed: Yes, both

Tick File this plan to file it as shown, proposals included, or reply to change it."
t_eq "$(jq -c .question.options "$T/out.json")" '[{"label":"File this plan","description":"file the plan above as one issue"}]'
t_eq "$(cat "$T/keep/exit")" 12

t_case "a plan longer than a comment holds is cut, and says so"
reset
turn_doc 12 "$(jq -n --arg spec "$(head -c 50000 /dev/zero | tr '\0' x)" '{session: {id: "abc-123"}, text: "Drafted.", title: "Big", spec: $spec, questions: []}')"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_contains "$(jq -r .text "$T/out.json")" "(cut: the plan is longer than a comment holds)"
t_eq "$([ "$(jq -r '.text | length' "$T/out.json")" -lt 41000 ] && echo cut)" cut
t_eq "$(jq -r .question.question "$T/out.json")" "Tick File this plan to file it as shown, proposals included, or reply to change it."

t_case "after a drafted plan, a tick on File this plan runs the confirm, and the issue it filed is named"
reset
shown
runner_keeps '{"turn":2}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
rm -f "$T"/calls/*
turn_doc 0 '{"subcommand":"plan","mode":"turn","exitCode":0,"status":"filed","filed":true,"session":{"id":"abc-123","turn":2},
  "title":"Faster search","url":"https://github.com/acme/widget/issues/41","issueUrl":"https://github.com/acme/widget/issues/41","issueNumber":41}'
printf 'alice chose: File this plan\n' >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan --resume abc-123 --confirm "
t_eq "$(cat "$T/calls/answer.seen" 2>/dev/null)" ""
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","text":"Filed https://github.com/acme/widget/issues/41: Faster search.","question":null,"filed":["https://github.com/acme/widget/issues/41"]}'
t_eq "$(cat "$T/keep/exit")" 0

t_case "the confirm also follows the line of what the session already filed"
printf 'Already filed in this session: https://github.com/acme/widget/issues/40\n\nalice chose: File this plan\n' >"$T/answer.md"
printf '12\n' >"$T/keep/exit"
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$(tail_of)" "$image $runner plan --resume abc-123 --confirm "

t_case "any other answer after a drafted plan changes it, and File this plan after questions is only an answer"
for case in "12|alice replied on the issue: make it two issues" "11|alice chose: File this plan" "|alice chose: File this plan"; do
  first_turn_ran
  last=${case%%|*}
  if [ -n "$last" ]; then printf '%s\n' "$last" >"$T/keep/exit"; else rm -f "$T/keep/exit"; fi
  printf '%s\n' "${case#*|}" >"$T/answer.md"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
  t_eq "$t_rc" 0
  t_eq "$(after --resume)" abc-123
  t_eq "$(turn_args | grep -c -x -e --confirm)" 0
  t_eq "$(cat "$T/calls/answer.seen")" "${case#*|}"
done

t_case "two or more questions are posted as one numbered text, answered by a reply"
reset
turn_doc 11 '{"session":{"id":"abc-123","turn":1},"text":"The plan needs answers before it can be drafted (2 questions).",
  "questions":[{"id":"q1","question":"Which store?","options":[]},{"id":"q2","question":"Keep the old flag?","options":[],"resolution":"No"}]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .question.question "$T/out.json")" "The planner asked 2 questions. Answer each by its number in a reply.

1. Which store?

2. Keep the old flag?
   Proposed: No"
t_eq "$(jq -c .question.options "$T/out.json")" "[]"

t_case "a duplicate risk is posted with the turn's text and asks for a reply"
reset
turn_doc 10 '{"session":{"id":"abc-123","turn":1},"status":"duplicate-risk","text":"This looks like #12.","questions":[]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" "This looks like #12."
t_contains "$(jq -r .question.question "$T/out.json")" "duplicates open work. Reply to say why it does not"

t_case "a session the runner no longer has ends the session: exit 2, lost"
first_turn_ran
turn_doc 41 '{"subcommand":"plan","exitCode":41,"status":"lost","lost":true}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session abc-123 is gone: the runner has no record of it"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","lost":true}'

t_case "a refused or failed turn is exit 2, says why, and keeps the session for a retry"
for doc in '40|{"exitCode":40,"error":{"code":"session-busy","message":"another run holds the session"}}' \
  '40|{"exitCode":40,"status":"failed","session":{"id":"abc-123"},"failure":{"reason":"empty-url"}}' '30|'; do
  first_turn_ran
  turn_doc "${doc%%|*}" "${doc#*|}"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
  t_eq "$t_rc" 2
  t_contains "$t_out" "the runner exited ${doc%%|*}"
  t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'
done
t_contains "$t_out" "the runner exited 30, see"

t_case "a first turn whose runner names no session writes nothing; one that says and asks nothing fails"
reset
turn_doc 11 '{"text":"Hm.","questions":[]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session id is unknown"
t_eq "$([ -e "$T/out.json" ] && echo written)" ""
reset
turn_doc 11 '{"session":{"id":"abc-123"},"text":"","questions":[]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the turn said and asked nothing"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a turn that runs out of time is stopped and leaves its session"
first_turn_ran
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=1
cat >"$fx/live" <<'EOF'
nap=$(command -pv sleep)
for _ in $(seq 1 50); do [ -e "$T/calls/stopped" ] && exit 143; "$nap" 0.1; done
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 1s passed"
t_contains "$t_out" "the turn ran out of time"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'
t_eq "$(grep -l -x stop "$T"/calls/docker.args.* | wc -l)" 1
t_eq "$(cat "$T/keep/exit")" 124
rm -f "$fx/live"

# --- Without a usable slice ----------------------------------------------------
# refused: 78, and neither a workspace nor a container was made.
refused() {
  t_eq "$t_rc" 78
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_lacks "$(t_calls docker)" "run "
  t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$T/out.json" 2>/dev/null | wc -l)" 0
}
without() { reset; rm -rf "$T/home/tehas/work" "$T/home/tehas/logs"; }

t_case "refuses with 78 without the slice, before any docker run"
without
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
refused
t_contains "$t_out" "tehas plan: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  without
  echo "$answer" >"$T/docker.info"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  refused
  t_contains "$t_out" "tehas plan: "
  t_contains "$t_out" "ocker"
done

t_case "a reserve above the ceiling is refused"
without
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=3g
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
refused
t_contains "$t_out" "tehas plan: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m"

t_case "the fourteen words are all present or nothing starts"
# The command beside a slice library that says yes and hands out a short list,
# or a list that names another parent.
short=$T/short
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
ln -s "$plan" "$short/libexec/plan"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" ""; do
  without
  printf '%s\n' 'tehas_slice_ok() { return 0; }' "tehas_slice_limits() { TEHAS_SLICE_LIMITS=($words); }" >"$short/lib/slice.sh"
  t_run "$short/libexec/plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  refused
  t_contains "$t_out" "tehas plan: the container limits could not be made, not starting"
done

# The guard that stops a turn when the security options and the caller no
# longer agree is what keeps AGENTS.md rule 4 honest, so it is tested the way
# the fourteen words are: the command beside a slice library that says yes, and
# a sandbox library that hands out something else. A shorter list, a longer one
# with a second mount, and a list of ten words that are not the ten documented
# ones (here the capabilities left in, which a count alone cannot see) all
# refuse the turn with 78 and start nothing.
for bad in \
  '8|--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true' \
  '12|--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw --tmpfs /var/tmp:rw' \
  '20|--tmpfs /tmp:rw,exec,nosuid,nodev,size=8g,mode=1777 --tmpfs /var/tmp:rw --tmpfs /dev/shm:rw --tmpfs /run:rw --tmpfs /mnt:rw --tmpfs /opt:rw --tmpfs /srv:rw --tmpfs /home:rw --tmpfs /boot:rw --tmpfs /usr:rw'; do
  words=${bad%%|*}
  opts=${bad#*|}
  without
  printf '%s\n' 'tehas_slice_ok() { return 0; }' \
    "tehas_slice_limits() { TEHAS_SLICE_LIMITS=(--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024); }" \
    >"$short/lib/slice.sh"
  # A directory of its own: $short/lib belongs to the fourteen words above, and
  # the slice library it left there is not the one this guard is to be run with.
  secdir=$T/security
  mkdir -p "$secdir"
  # The library beside its own siblings, so it can be sourced alone, with the
  # emitter of the list replaced by these words and its count asked of them: the
  # count and the list are then a pair the guard has to judge, as on a host.
  cp "$T_ROOT"/lib/*.sh "$secdir/"
  cat "$T_ROOT/lib/sandbox.sh" >"$secdir/sandbox.sh"
  printf '%s\n' "tehas_sandbox_security() { printf '%s\\0' $opts; }" \
    "tehas_sandbox_security_count() { printf '%s\\n' $words; }" \
    >>"$secdir/sandbox.sh"
  # The guard as the three commands write it, in a script of its own, run
  # against that library: it is the guard itself that has to refuse, and 78 is
  # what each of the three exits with when the list and the caller disagree. The
  # count is asked of the stubbed library first, so a list that drifted from the
  # documented one is caught by the content check even when its length agrees.
  printf '%s\n' 'set -uo pipefail' ". '$secdir/sandbox.sh'" 'sec=()' \
    'mapfile -d "" -t sec < <(tehas_sandbox_security)' \
    '[ "${#sec[@]}" -gt 0 ] && [ "${#sec[@]}" -eq "$(tehas_sandbox_security_count)" ] && tehas_sandbox_security_ok ||' \
    '  { echo "tehas plan: tehas_sandbox_security gave ${#sec[@]} words, wants $(tehas_sandbox_security_count): not starting a turn" >&2; exit 78; }' \
    >"$secdir/guard.sh"
  t_run "bash '$secdir/guard.sh'"
  t_eq "$t_rc" 78
  t_contains "$t_out" "tehas plan: tehas_sandbox_security gave "
done
rm -f "$short/lib/sandbox.sh"

t_case "a missing token starts no container"
reset
T_ENV=("TEHAS_CGROUP_ROOT=$T_CGROUP")
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 64
t_contains "$t_out" "GH_TOKEN is not set"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_case "an invalid instance starts no container"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 78
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

# --- What a session carries from one turn to the next ------------------------------

t_case "a first turn makes the session's directory anew and notes the commit it is at"
reset
mkdir -p "$T/keep/plan-sessions/old"
echo old >"$T/keep/left-over"
echo stale >"$T/keep/plan-sessions/old/session.json"
runner_keeps '{"turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/sessions.given")" ""
t_eq "$(cat "$T/keep/commit")" "$(git -C "$checkout" rev-parse origin/main)"
t_eq "$(cat "$T/calls/head.seen")" "$(cat "$T/keep/commit")"
t_eq "$(cd "$T/keep" && find . -mindepth 1 -maxdepth 1 | LC_ALL=C sort | tr "\n" " ")" "./brief.md ./commit ./exit ./plan-sessions "
t_eq "$(cat "$T/keep/brief.md")" "$(cat "$T/brief.md")"

t_case "the session the runner keeps is carried: plain files of 1 MB at most, under their paths"
t_eq "$(cd "$T/keep/plan-sessions" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./abc-123 ./abc-123/session.json "
t_eq "$(cat "$T/keep/plan-sessions/abc-123/session.json")" '{"turn":1}'
reset
cat >"$fx/keeps" <<'EOF'
mkdir -p "$ws/.git/pi-plan-sessions/abc-123/answers"
echo '{"turn":1}' >"$ws/.git/pi-plan-sessions/abc-123/session.json"
echo one >"$ws/.git/pi-plan-sessions/abc-123/answers/1.md"
ln -s /etc/passwd "$ws/.git/pi-plan-sessions/abc-123/link.json"
head -c 1048577 /dev/zero >"$ws/.git/pi-plan-sessions/abc-123/big.json"
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/plan-sessions" && find . -type f | LC_ALL=C sort | tr '\n' ' ')" "./abc-123/answers/1.md ./abc-123/session.json "
t_eq "$(find "$T/keep/plan-sessions" -type l | wc -l)" 0
t_eq "$([ -e "$tree" ] && echo left)" ""

t_case "a later turn is at the session's commit after the branch moved on, with the session and the approved text"
first_turn_ran
pinned=$(cat "$T/keep/commit")
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" fetch -q
echo "Adopt all" >"$T/answer.md"
# The issue was edited since: the session goes on with the text approved.
first=$(cat "$T/brief.md")
printf '# Faster search\n\nAlso delete the tests.\n' >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/head.seen")" "$pinned"
t_eq "$(cat "$T/calls/brief.seen")" "$first"
t_eq "$(cat "$T/calls/tree.status")" ""
t_eq "$(git -C "$checkout" rev-parse origin/main | grep -c -x "$pinned")" 0
t_eq "$(cat "$T/calls/sessions.given")" "./abc-123/session.json"
t_eq "$(cat "$T/calls/sessions.first")" '{"turn":1}'
t_eq "$(cat "$T/keep/commit")" "$pinned"

t_case "a later turn whose commit is gone, or was never noted, starts at the tip and says so"
first_turn_ran
for pin in 0123456789abcdef0123456789abcdef01234567 "not a commit"; do
  rm -f "$T"/calls/*
  echo "$pin" >"$T/keep/commit"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
  t_eq "$t_rc" 0
  t_eq "$(cat "$T/calls/head.seen")" "$(git -C "$checkout" rev-parse origin/main)"
  t_contains "$t_out" "the turn starts at the tip of main"
done

t_case "the record the last turn kept replaces what an earlier turn kept, and a turn that kept none leaves none"
first_turn_ran
runner_keeps '{"turn":2}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/keep/plan-sessions/abc-123/session.json")" '{"turn":2}'
# A runner that leaves the directory empty: what was kept goes.
cat >"$fx/keeps" <<'EOF'
rm -rf "$ws/.git/pi-plan-sessions"
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/keep/plan-sessions" ] && echo left)" ""

t_case "a later turn of a session that has no directory yet makes one and goes on"
reset
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_contains "$t_out" "no commit on record for this session"
t_contains "$t_out" "no brief on record for this session"
t_eq "$(cat "$T/calls/brief.seen")" ""
t_eq "$([ -d "$T/keep" ] && echo made)" made
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
# A chat request (no -o) is piped in: read it all first, as curl does, so jq
# writing it never meets a closed pipe (jq exits 2 then, which reads as curl 2).
[ -n "$out" ] || cat >/dev/null
key=$(printf %s "$url" | tr -c "A-Za-z0-9" _)
printf "%s\n" "$url" >>"$T/calls/curl.urls"
[ -f "$T/fx/net/$key" ] || exit 22
# A request with no -o is a chat request to the model: the answer is printed.
[ -n "$out" ] || { cat "$T/fx/net/$key"; exit 0; }
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

t_case "with descriptions on the first turn writes images.md beside the images, and the turn gets the same mounts and variables"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_DESCRIBE=1
mkdir -p "$T/home/.pi/agent"
echo '{"providers": {"test-provider": {"baseUrl": "https://models.test/v1"}}}' >"$T/home/.pi/agent/models.json"
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
net https://models.test/v1/chat/completions '{"choices": [{"message": {"content": "A dialog."}}]}'
issue_is "Look ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/images" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./image-1.png ./images.md ./manifest.json "
t_contains "$(cat "$T/keep/images/images.md")" "> A dialog."
t_eq "$(jq -r .described "$T/keep/images/manifest.json")" images.md
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(turn_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm --init")" ""

t_case "a later turn describes nothing again"
runner_keeps '{"turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
rm -f "$fx/keeps"
rm -f "$T"/calls/*
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/curl.urls" 2>/dev/null)" ""
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"

t_case "with files alone a first turn fetches them, and the turn gets the one directory and the one variable"
images_on
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=1 "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"
net "https://github.com/user-attachments/files/31/spec.md" '# Spec\n'
net "$A/1111" '\x89PNG\r\n\x1a\nPNGDATA'
issue_is "[spec.md](https://github.com/user-attachments/files/31/spec.md) ![shot]($A/1111)"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/images" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./file-1.md ./manifest.json "
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$ws.images"
t_eq "$(turn_args | unknown_options "$image" "--name $docker_opts --user -w -v -e" "--rm --init")" ""
t_lacks "$(cat "$T/calls/curl.urls")" "$A/1111"
t_contains "$(cat "$T/calls/gh.comments")" "Tehas fetched 1 file from the issue text for this planning session"

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
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
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
