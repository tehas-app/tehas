#!/usr/bin/env bash
# Tests for libexec/plan: one turn of a planning session. A stub docker
# records its arguments, keeps the first two lines it is sent and answers with
# a recorded event stream, so no model runs.
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
# One case runs tehas work, which parks a cycle of a product that names no gate
# and holds no stack it can guess a gate from (libexec/work). The gate is a file
# of the two worktrees, in no commit: the work files hold the same pair.
mkdir -p "$upstream/.pi" "$checkout/.pi"
printf 'scripts/verify.sh\n' >"$upstream/.pi/verify-cmd"
printf 'scripts/verify.sh\n' >"$checkout/.pi/verify-cmd"

# The turn's workspace is gone when the turn is over, so the container notes
# what it was handed there.
t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
# tehas work asks the image what it supports; one case runs it.
case " $* " in *" --capabilities "*) cat "$T/caps.json" 2>/dev/null; exit ;; esac
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
case " $* " in *" --mode "*)
  ws=$T/home/tehas/work/widget-plan-7
  cat "$ws/.tehas/brief.md" >"$T/calls/brief.seen" 2>/dev/null
  git -C "$ws" status --porcelain >"$T/calls/tree.status" 2>&1
  cp "$ws/.git/info/exclude" "$T/calls/exclude.seen" 2>/dev/null
  mkdir -p "$ws/.tehas/definition" && echo "{}" >"$ws/.tehas/definition/app.json"
  git -C "$ws" status --porcelain -uall >"$T/calls/tree.status.definition" 2>&1
  git -C "$ws" rev-parse HEAD >"$T/calls/head.seen" 2>&1
  ls -A "$ws/.git/pi-plan-cache" >"$T/calls/cache.seen" 2>/dev/null
  cat "$ws.images/manifest.json" >"$T/calls/manifest.seen" 2>/dev/null
  # What pi-rukas does before it reads its plan cache, and what filing needs.
  if [ -e "$T/fx/fetch" ]; then
    git -C "$ws" fetch -q origin --end-of-options main >"$T/calls/fetch.err" 2>&1
    git -C "$ws" rev-parse refs/remotes/origin/main >"$T/calls/origin.seen" 2>&1
    git -C "$ws" push -q origin HEAD:refs/heads/plan-push >"$T/calls/push.err" 2>&1
    echo "$?" >"$T/calls/push.rc"
  fi
  cat "$ws/.git/pi-plan-cache/"*.json >"$T/calls/cache.read" 2>/dev/null
  # What the agent leaves in the plan cache, when a case says.
  [ ! -e "$T/fx/cache" ] || . "$T/fx/cache"
  # The host directory the container has at /home/vscode/.pi/agent/sessions,
  # read from the mounts of this call: the state directory of the turn, made
  # anew for it, so what it holds was put there by tehas. The id is the
  # argument after --session, empty for a first turn. A later turn has the
  # file only when tehas carried the sessions of the last turn across.
  container=/home/vscode/.pi/agent/sessions
  mount=$(grep -F -e "$container" "$T/calls/docker.args.$$" | head -n 1)
  mount_at=:$container
  pstate=${mount%$mount_at}
  sid=$(grep -F -A 1 -x -- --session "$T/calls/docker.args.$$" |
    grep -v -F -x -- --session | head -n 1)
  # What the turn was given to reopen, then what its agent leaves there: the
  # text of the session, when a case says, as the agent would leave it.
  ls -A "$pstate" >"$T/calls/sessions.given" 2>/dev/null
  head -n 1 "$pstate/$sid.jsonl" >"$T/calls/sessions.first" 2>/dev/null
  cat "$pstate/nested/part.json" >"$T/calls/sessions.nested" 2>/dev/null
  # What the agent of this turn writes, when a case says the container wrote
  # anything at all: the file of the session, a directory beside it, or both
  # or neither. The state directory belongs to the turn alone, so the stub
  # makes it anew first: what it finds there was carried across by tehas.
  if [ -e "$T/fx/session.any" ]; then
    rm -rf "$pstate"
    mkdir -p "$pstate"
    sid=$(cat "$T/fx/session.id")
    [ ! -e "$T/fx/session.text" ] || cp "$T/fx/session.text" "$pstate/$sid.jsonl"
    if [ -e "$T/fx/session.part" ]; then
      mkdir -p "$pstate/nested"
      cp "$T/fx/session.part" "$pstate/nested/part.json"
    fi
    ls -A "$pstate" >"$T/calls/sessions.wrote" 2>/dev/null
    # A turn whose agent wrote nothing leaves an empty directory, which tehas
    # keeps as nothing: what an earlier turn left is then gone.
    keep_dir=$pstate
    ls -A "$pstate" | head -n 1 | grep -q . || rm -rf "$keep_dir"
  fi
  # Pi has a session only when its file is under the directory it was given,
  # so a later turn with nothing to reopen is told of one with no messages. A
  # case that wants the session itself gone, whatever the directory holds,
  # says so with $T/fx/no_session.
  messages=6
  if [ -n "$sid" ]; then
    [ -e "$T/fx/no_session" ] && messages=0
    [ -e "$pstate/$sid.jsonl" ] || messages=0
  fi
  IFS= read -r line
  # A later turn first asks what is in the session it opened.
  case "$line" in *"\"id\":\"opened\""*)
    printf "%s\n" "$line" >"$T/calls/docker.opened"
    if [ -e "$T/fx/opened" ]; then . "$T/fx/opened"
    else echo "{\"id\":\"opened\",\"type\":\"response\",\"command\":\"get_state\",\"success\":true,\"data\":{\"sessionId\":\"abc-123\",\"messageCount\":$messages}}"; fi
    IFS= read -r -t 5 line ;;
  esac
  printf "%s\n" "$line" >"$T/calls/docker.stdin"
  # The second line may never come: do not wait for it for ever.
  IFS= read -r -t 5 line; printf "%s\n" "$line" >"$T/calls/docker.stdin2"
  # A case that needs a container that lives for a while writes its part here.
  if [ -e "$T/fx/live" ]; then . "$T/fx/live"; else cat "$T/fx/turn.jsonl" 2>/dev/null; fi ;;
  *" image inspect "*) cat "$T/image.label" 2>/dev/null ;;
esac'
# The command looks at the log once a second. Here a look costs no time, so a
# container that lives for a second is looked at many times.
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
# The security options of lib/sandbox.sh, the same eight words a cycle is given
# (test/work_lib.sh holds the same pair for the cycle's tests).
security_opts="--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true"
# The words after one option, read the way docker reads them.
security_of() { # <flag> (reads the arguments on standard input)
  awk -v flag="$1" 'prev == flag { print } { prev = $0 }'
}
# The same eight words, in order, as one line; empty when given another way.
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

turn_args() { grep -l -x -e '--mode' -e /opt/pi-rukas/bin/pi-rukas-headless "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { turn_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
image="tehas/pi-rukas:uid$(id -u)"
tail_of() { turn_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }
# Every option that is not one this command is known to pass, up to the image
# name, read the way docker reads them. See test/work_lib.sh.
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
# What docker is given before the image, one argument per line, without the
# container's name and without the cid file, which only a cycle has.
options() {
  awk -v image="$image" '$0 == image { exit } $0 == "--cidfile" { skip = 1; next } skip { skip = 0; next }
    prev == "--name" { $0 = "<name>" } { print; prev = $0 }'
}

# A turn that asked a question with the tool, then settled.
asked() {
  cat >"$fx/turn.jsonl" <<'EOF'
{"type":"agent_start"}
{"type":"tool_execution_end","toolCallId":"c1","toolName":"question","result":{"content":[],"details":{"question":"Adopt my resolutions?","options":["Adopt all","Change some"],"answer":null,"deferred":true,"optionDetails":[{"label":"Adopt all","description":"Apply all five"},{"label":"Change some","description":""}]}},"isError":false}
{"type":"agent_settled"}
{"id":"state","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123"}}
{"id":"text","type":"response","command":"get_last_assistant_text","success":true,"data":{"text":"Two ways.\nPick one."}}
EOF
}
# What the agent of a turn leaves in the sessions directory of its turn: the
# host directory behind /home/vscode/.pi/agent/sessions, which tehas keeps for
# the next turn. The case writes the text of the session; the stub copies it
# in under the id of the turn, as the agent would leave it. The second form
# leaves the file alone, as a container that wrote nothing does.
agent_leaves() { # <the id of the session> <what it holds>
  : >"$fx/session.any"
  printf '%s\n' "$2" >"$fx/session.text"
  printf '%s\n' '{"part":2}' >"$fx/session.part"
  printf '%s\n' "$1" >"$fx/session.id"
}
# A container whose agent wrote no session at all.
agent_leaves_nothing() {
  : >"$fx/session.any"
  rm -f "$fx/session.text" "$fx/session.part" "$fx/session.id"
}
reset() {
  rm -rf "$T"/calls/* "$T/out.json" "$fx/live" "$fx/cache" "$fx/fetch" "$fx/opened" \
    "$fx/session.any" "$fx/session.text" "$fx/session.part" "$fx/session.id" \
    "$fx/no_session" "$T/keep"
  asked
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  printf '# Faster search\n\nMake it fast.\n' >"$T/brief.md"
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

t_case "a first turn writes the session, the message and the question"
reset
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .session "$T/out.json")" "abc-123"
t_eq "$(jq -r .text "$T/out.json")" "Two ways.
Pick one."
t_eq "$(jq -r .question.question "$T/out.json")" "Adopt my resolutions?"
t_eq "$(jq -c '.question.options' "$T/out.json")" '[{"label":"Adopt all","description":"Apply all five"},{"label":"Change some","description":""}]'

t_case "the approved text is in the workspace as a file git ignores, and the prompt points at it"
t_eq "$(cat "$T/calls/brief.seen")" "$(cat "$T/brief.md")"
t_eq "$(cat "$T/calls/tree.status")" ""

t_case "only the brief is hidden from git; the rest of .tehas stays visible"
t_contains "$(cat "$T/calls/exclude.seen")" "/.tehas/brief.md"
t_lacks "$(grep -x '.tehas/' "$T/calls/exclude.seen")" ".tehas/"
t_contains "$(cat "$T/calls/tree.status.definition")" ".tehas/definition/"

t_case "the workspace is the turn's own, and it is gone when the turn is over"
t_eq "$([ -e "$tree" ] && echo left)" ""
t_eq "$([ -e "$ws" ] && echo made)" ""
t_eq "$([ -e "$state" ] && echo left)" ""
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "/plan the work described in the file .tehas/brief.md"
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "issue #7"

t_case "the prompt leaves the labels to a person"
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "Do not add or remove any tehas:* label or needs-human-attention on any issue: a person labels."
t_eq "$(jq -r .type "$T/calls/docker.stdin")" "prompt"

t_case "the session is asked for right after the prompt, so a turn that fails late still has it"
t_eq "$(jq -c . "$T/calls/docker.stdin2")" '{"id":"state","type":"get_state"}'

t_case "a first turn starts a new session"
t_eq "$(tail_of)" "$image pi --mode rpc --approve "

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
PI_ENSEMBLE_PLAN_CACHE_TTL_MS
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the plan cache never expires, whatever the instance says"
t_eq "$(after -e | grep '^PI_ENSEMBLE_PLAN_CACHE_TTL_MS')" "PI_ENSEMBLE_PLAN_CACHE_TTL_MS=0"
reset
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_PLAN_CACHE_TTL_MS=60000
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_PLAN_CACHE_TTL_MS')" 1
t_eq "$(after -e | grep '^PI_ENSEMBLE_PLAN_CACHE_TTL_MS')" "PI_ENSEMBLE_PLAN_CACHE_TTL_MS=0"
reset
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"

t_case "the planning container gets no option beyond the documented ones"
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""
t_eq "$(turn_args | grep -c -x -e --init)" 1
t_eq "$(after --name)" "tehas-plan-7"
t_eq "$(after -w)" "$ws"

t_case "nothing that controls the host is reachable, and only one container ran"
args=$(cat "$T"/calls/docker.args.*)
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "--privileged"
t_lacks "$args" "test-token"
t_lacks "$args" "$checkout"
t_eq "$(grep -l -x -e run "$T"/calls/docker.args.* | wc -l)" 1

t_case "a container left under the turn's name is removed before the turn starts"
rm_at=$(t_calls docker | grep -n -x -e 'rm -f tehas-plan-7' | cut -d: -f1 | head -n 1)
run_at=$(t_calls docker | grep -n -e ' --mode ' | cut -d: -f1 | head -n 1)
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
# The image's entrypoint drops from root with setpriv, which the dropped
# SETUID and SETGID refuse: the container starts as the host's user instead.
t_eq "$(after --user)" "$(id -u):$(id -g)"
t_lacks "$(turn_args)" "--cidfile"
t_contains "$(turn_args)" "--add-host=models:192.0.2.9"
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

# The options without the one pair a planning turn adds.
without_expiry() {
  awk 'held { if ($0 != "PI_ENSEMBLE_PLAN_CACHE_TTL_MS=0") print "-e"; else { held = 0; next } }
    { held = ($0 == "-e") } !held { print } END { if (held) print "-e" }'
}
# A turn has a workspace and a transcripts directory of its own on the host.
# They are mounted at the paths a cycle's are.
# The options without the pairs only a cycle is given: who reports its
# handoff and the time it may take, its deadline and its gate runs at once.
without_cycle_only() {
  awk 'held { if ($0 !~ /^PI_ENSEMBLE_(HANDOFF_CHANNEL|HANDOFF_TIMEOUT_MS|CYCLE_DEADLINE|VERIFY_CONCURRENCY)=/) print "-e"; else { held = 0; next } }
    { held = ($0 == "-e") } !held { print } END { if (held) print "-e" }'
}
t_case "docker is given what tehas work gives a cycle's container: only the name, the two directories of its own, the plan cache's expiry, the cycle's cid file, the cycle's own four inputs, the input and what is run differ"
turn_args | options | without_expiry | grep -v -x -e '-i' |
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

t_case "a later turn reopens the session and sends the answer as it was written"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
printf '%s\n' '- "Adopt all", but keep `tagged-at` open' '' "second line with 'quotes' and \$HOME" >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image pi --mode rpc --approve --session abc-123 "
t_eq "$(jq -r .message "$T/calls/docker.stdin")" "$(cat "$T/answer.md")"
t_eq "$([ -e "$ws/.tehas/brief.md" ] && echo present)" ""

t_case "a later turn asks what is in the session before it sends the answer"
t_eq "$(jq -c . "$T/calls/docker.opened")" '{"id":"opened","type":"get_state"}'
t_eq "$(jq -c . "$T/calls/docker.stdin2")" '{"id":"state","type":"get_state"}'

t_case "a first turn does not ask"
reset
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/calls/docker.opened" ] && echo asked)" ""

t_case "a later turn whose session Pi opened empty sends no answer and says the session is lost"
reset
cat >"$fx/opened" <<'EOF'
echo '{"id":"opened","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123","messageCount":0}}'
IFS= read -r -t 1 line && printf "%s\n" "$line" >"$T/calls/docker.stdin"
exit 0
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session abc-123 is gone: Pi opened it with no messages, so the answer was not sent"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","lost":true}'
t_eq "$(cat "$T/calls/docker.stdin" 2>/dev/null)" ""
# The container ended by itself; closing Pi's input is all a gone one needs.
t_eq "$(grep -l -x stop "$T"/calls/docker.args.* | wc -l)" 0
t_eq "$([ -e "$tree" ] && echo left)" ""

t_case "a later turn whose container never says what is in the session sends no answer"
reset
echo 'exit 0' >"$fx/opened"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "Pi did not say what is in session abc-123, so the answer was not sent"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a Pi that gives no count goes on and says so"
reset
cat >"$fx/opened" <<'EOF'
echo '{"id":"opened","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123"}}'
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_contains "$t_out" "Pi did not say how many messages session abc-123 has, going on"
t_eq "$(jq -r .message "$T/calls/docker.stdin")" "$(cat "$T/brief.md")"

t_case "a turn that only wrote a message has no question"
reset
grep -v tool_execution_end "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .question "$T/out.json")" "null"

t_case "a turn that filed two issues names both in the outcome"
reset
cat >>"$fx/turn.jsonl" <<'EOF'
{"type":"tool_execution_end","toolCallId":"b1","toolName":"bash","result":{"content":[{"type":"text","text":"Creating issue in acme/widget\nhttps://github.com/acme/widget/issues/42\n"}],"details":{}},"isError":false}
{"type":"tool_execution_end","toolCallId":"b2","toolName":"bash","result":{"content":[{"type":"text","text":"https://github.com/acme/widget/issues/43"}],"details":{}},"isError":false}
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c .filed "$T/out.json")" '["https://github.com/acme/widget/issues/42","https://github.com/acme/widget/issues/43"]'

t_case "what a turn filed is what gh printed: another tool's output, a failed call and a number quoted in prose are none"
reset
cat >>"$fx/turn.jsonl" <<'EOF'
{"type":"tool_execution_end","toolCallId":"b3","toolName":"bash","result":{"content":[{"type":"text","text":"https://github.com/acme/widget/issues/44"}],"details":{}},"isError":true}
{"type":"tool_execution_end","toolCallId":"b4","toolName":"bash","result":{"content":[{"type":"text","text":"File 3 is https://github.com/acme/widget/issues/45, as the brief says."}],"details":{}},"isError":false}
{"type":"tool_execution_end","toolCallId":"b5","toolName":"read","result":{"content":[{"type":"text","text":"https://github.com/acme/widget/issues/46"}],"details":{}},"isError":false}
{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"I will file https://github.com/acme/widget/issues/47 next."}]}}
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r 'has("filed")' "$T/out.json")" "false"

t_case "a turn that filed nothing writes no filed key"
t_eq "$(jq -r 'has("filed")' "$T/out.json")" "false"

t_case "two open questions in one turn are both posted, numbered, and answered by a reply"
reset
cat >>"$fx/turn.jsonl" <<'EOF'
{"type":"tool_execution_end","toolCallId":"c2","toolName":"question","result":{"content":[],"details":{"question":"Which store?","options":["Disk","Memory"],"answer":null,"deferred":true,"optionDetails":[{"label":"Disk","description":"Slow\nand safe"},{"label":"Memory","description":""}]}},"isError":false}
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the turn asked 2 questions; they are posted as one, to be answered by a reply"
t_eq "$(jq -r .question.question "$T/out.json")" "The agent asked 2 questions. Answer each by its number in a reply.

1. Adopt my resolutions?
   - Adopt all: Apply all five
   - Change some

2. Which store?
   - Disk: Slow and safe
   - Memory"
t_eq "$(jq -c .question.options "$T/out.json")" "[]"

t_case "a question from an earlier, interactive answer is not asked again"
reset
sed -i 's/"deferred":true,//' "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$(jq -r .question "$T/out.json")" "null"

t_case "a turn that asked with the tool and wrote nothing still counts"
reset
sed -i 's/"data":{"text":"Two ways.\\nPick one."}/"data":{"text":null}/' "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" ""
t_eq "$(jq -r .question.question "$T/out.json")" "Adopt my resolutions?"

t_case "a turn that never settled is a failure and leaves only its session"
reset
grep -v agent_settled "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a first turn that never settled and never named its session writes nothing"
reset
grep -v -e agent_settled -e get_state "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_eq "$([ -e "$T/out.json" ] && echo written)" ""

t_case "a settled first turn whose session is unknown is a failure and writes nothing"
reset
grep -v get_state "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session id is unknown"
t_eq "$([ -e "$T/out.json" ] && echo written)" ""

t_case "a later turn that never settled leaves the session it was given"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
grep -v -e agent_settled -e get_state "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a run that started again after settling has not settled"
reset
echo '{"type":"agent_start"}' >>"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a turn with neither a message nor a question is a failure"
reset
printf '%s\n' '{"type":"agent_start"}' '{"type":"agent_settled"}' \
  '{"type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123"}}' \
  '{"type":"response","command":"get_last_assistant_text","success":true,"data":{"text":null}}' >"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "said and asked nothing"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a line that is not JSON in the stream is skipped, not fatal"
reset
sed -i '2i not json at all' "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .session "$T/out.json")" "abc-123"

t_case "a very long log gives the same outcome"
reset
{
  for ((i = 0; i < 20000; i++)); do printf '{"type":"message_update","delta":"word %s"}\n' "$i"; done
  cat "$fx/turn.jsonl"
} >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.session, .text, .question.question, (.question.options | length)]' "$T/out.json")" '["abc-123","Two ways.\nPick one.","Adopt my resolutions?",2]'

t_case "a start buried in a very long log after the settle still means not settled"
reset
{
  cat "$fx/turn.jsonl"
  echo '{"type":"agent_start"}'
  for ((i = 0; i < 20000; i++)); do printf '{"type":"message_update","delta":"word %s"}\n' "$i"; done
} >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2

t_case "a last line with no line end is still read"
reset
grep -v get_last_assistant_text "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
printf '%s' '{"id":"text","type":"response","command":"get_last_assistant_text","success":true,"data":{"text":"Cut short"}}' >>"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" "Cut short"
reset
grep -v -e agent_settled -e '"response"' "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
printf '%s' '{"type":"agent_settled"}' >>"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_contains "$t_out" "the session id is unknown"

# A container that stays alive: it writes its events over time, one of them
# in two halves, and answers what it is asked once the turn is taken as over.
# The stub sleep above is not used for its pauses.
live() { # <what the container does after its first events>
  cat >"$fx/live" <<EOF
nap=\$(command -pv sleep)
echo '{"id":"state","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123"}}'
echo '{"type":"agent_start"}'
"\$nap" 0.4
$1
EOF
}

t_case "a turn is followed while the container runs, and asked for its outcome once it is quiet"
reset
# If the turn is never taken as over, the time limit ends it and the stub
# stops waiting: the case fails, it does not hang.
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=20
live 'printf "%s" "{\"type\":\"agent_set"
"$nap" 0.4
echo "tled\"}"
while IFS= read -r -t 30 ask; do
  printf "%s\n" "$ask" >>"$T/calls/docker.asked"
  case "$ask" in
    *get_state*) echo "{\"type\":\"response\",\"command\":\"get_state\",\"success\":true,\"data\":{\"sessionId\":\"abc-123\"}}" ;;
    *get_last_assistant_text*) echo "{\"type\":\"response\",\"command\":\"get_last_assistant_text\",\"success\":true,\"data\":{\"text\":\"Done live.\"}}"; exit 0 ;;
  esac
done'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","text":"Done live.","question":null}'
t_eq "$(jq -r .type "$T/calls/docker.asked" | tr '\n' ' ')" "get_state get_last_assistant_text "
t_eq "$([ "$(wc -l <"$T/calls/sleep")" -ge 20 ] && echo looked)" looked

t_case "once the outcome is read the session is ended by closing Pi's input"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=20
live 'echo "{\"type\":\"agent_settled\"}"
while IFS= read -r -t 30 ask; do
  case "$ask" in
    *get_last_assistant_text*) echo "{\"type\":\"response\",\"command\":\"get_last_assistant_text\",\"success\":true,\"data\":{\"text\":\"Done.\"}}"; break ;;
  esac
done
timeout 5 cat >/dev/null && : >"$T/calls/eof"'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls/eof" 2>/dev/null | wc -l)" 1

t_case "a first turn that runs out of time is stopped and leaves its session"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=1
live '"$nap" 3'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 1s passed"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'
t_eq "$(grep -l -x stop "$T"/calls/docker.args.* | wc -l)" 1

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

# --- Background jobs --------------------------------------------------------------
# The agent dispatches a subagent and ends its turn; the subagent's report
# starts the next run. ok_after: what the container does once the report is in.
answer_live='while IFS= read -r -t 30 ask; do
  printf "%s\n" "$ask" >>"$T/calls/docker.asked"
  case "$ask" in
    *get_state*) echo "{\"type\":\"response\",\"command\":\"get_state\",\"success\":true,\"data\":{\"sessionId\":\"abc-123\"}}" ;;
    *get_last_assistant_text*) echo "{\"type\":\"response\",\"command\":\"get_last_assistant_text\",\"success\":true,\"data\":{\"text\":\"Done after the jobs.\"}}"; exit 0 ;;
  esac
done'
job() { # <jobId|batchId> <id>
  printf '{"type":"tool_execution_end","toolCallId":"c","toolName":"dispatch","result":{"content":[{"type":"text","text":"Dispatched async job %s. End your turn."}],"details":{"%s":"%s","async":true}},"isError":false}\n' "$2" "$1" "$2"
}
report() { # <job|batch> <id>
  printf '{"type":"message_end","message":{"role":"user","content":[{"type":"text","text":"[ensemble:async] Subagent `explore` (%s %s) ok — 3 turns, 2m01s"}]}}\n' "$1" "$2"
}
job jobId j1-abc >"$fx/job1"
job batchId b2-def >"$fx/job2"
report job j1-abc >"$fx/report1"
report batch b2-def >"$fx/report2"

t_case "a turn that settles while its jobs are out is not over until they report and it settles again"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=20
live 'cat "$T/fx/job1" "$T/fx/job2"
echo "{\"type\":\"agent_settled\"}"
"$nap" 1.5
# Anything asked of the container by now was asked too early.
if IFS= read -r -t 0.1 early; then printf "%s\n" "$early" >"$T/calls/docker.early"; fi
cat "$T/fx/report1"
echo "{\"type\":\"agent_start\"}"
echo "{\"type\":\"agent_settled\"}"
"$nap" 1.5
if IFS= read -r -t 0.1 early; then printf "%s\n" "$early" >"$T/calls/docker.early"; fi
cat "$T/fx/report2"
echo "{\"type\":\"agent_start\"}"
echo "{\"type\":\"agent_settled\"}"
'"$answer_live"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/docker.early" 2>/dev/null)" ""
t_eq "$(jq -r .text "$T/out.json")" "Done after the jobs."

t_case "a job that never reports holds the turn until the time limit, and a quote of a report is no report"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=2
live 'cat "$T/fx/job1"
printf "%s\n" "{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"[ensemble:async] Subagent (job j1-abc) ok\"}]}}"
echo "{\"type\":\"agent_settled\"}"
"$nap" 4'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 2s passed"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

# --- What a session carries from one turn to the next ------------------------------

t_case "a first turn makes the session's directory anew and notes the commit it is at"
reset
mkdir -p "$T/keep/pi-plan-cache"
echo old >"$T/keep/left-over"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/keep/commit")" "$(git -C "$checkout" rev-parse origin/main)"
t_eq "$(cat "$T/calls/head.seen")" "$(cat "$T/keep/commit")"
t_eq "$(ls -A "$T/keep")" "brief.md
commit"
t_eq "$(cat "$T/keep/brief.md")" "$(cat "$T/brief.md")"

t_case "the plan cache the agent leaves is kept: plain .json files and the .filed and .claim markers only"
reset
cat >"$fx/cache" <<'EOF'
mkdir -p "$ws/.git/pi-plan-cache/sub"
echo '{"spec":"one"}' >"$ws/.git/pi-plan-cache/a1.json"
echo '{"spec":"sub"}' >"$ws/.git/pi-plan-cache/sub/b.json"
echo claim >"$ws/.git/pi-plan-cache/a1.claim"
echo filed >"$ws/.git/pi-plan-cache/a1.filed"
echo other >"$ws/.git/pi-plan-cache/a1.json.tmp"
ln -s /etc/passwd "$ws/.git/pi-plan-cache/link.json"
ln -s /etc/passwd "$ws/.git/pi-plan-cache/link.filed"
head -c 1048577 /dev/zero >"$ws/.git/pi-plan-cache/big.json"
head -c 1048577 /dev/zero >"$ws/.git/pi-plan-cache/big.claim"
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(find "$T/keep/pi-plan-cache" -mindepth 1 -printf '%f\n' | sort | tr '\n' ' ')" "a1.claim a1.filed a1.json "
t_eq "$(cat "$T/keep/pi-plan-cache/a1.json")" '{"spec":"one"}'
t_eq "$(cat "$T/keep/pi-plan-cache/a1.filed")" "filed"
t_eq "$([ -e "$tree" ] && echo left)" ""

t_case "a later turn is at the session's commit after the branch moved on, with the cache it left and the approved text"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
pinned=$(cat "$T/keep/commit")
rm -f "$fx/cache" "$T"/calls/*
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
t_eq "$(tr '\n' ' ' <"$T/calls/cache.seen")" "a1.claim a1.filed a1.json "
t_eq "$(cat "$T/calls/cache.read")" '{"spec":"one"}'
t_eq "$(cat "$T/keep/commit")" "$pinned"

t_case "a fetch inside a later turn leaves origin/main at the session's commit, and a push still works"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
pinned=$(cat "$T/keep/commit")
rm -f "$T"/calls/*
: >"$fx/fetch"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/fetch.err")" ""
t_eq "$(cat "$T/calls/origin.seen")" "$pinned"
t_eq "$(cat "$T/calls/push.rc")" 0
t_eq "$(git -C "$upstream" rev-parse plan-push)" "$pinned"
rm -f "$fx/fetch"

t_case "a later turn whose commit is gone, or was never noted, starts at the tip and says so"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
for pin in 0123456789abcdef0123456789abcdef01234567 "not a commit"; do
  rm -f "$T"/calls/*
  echo "$pin" >"$T/keep/commit"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
  t_eq "$t_rc" 0
  t_eq "$(cat "$T/calls/head.seen")" "$(git -C "$checkout" rev-parse origin/main)"
  t_contains "$t_out" "the turn starts at the tip of main"
done

t_case "a first turn keeps no sessions and leaves no sessions of another session"
reset
mkdir -p "$T/keep/sessions"
echo kept >"$T/keep/sessions/abc-123.jsonl"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/sessions.given" 2>/dev/null | tr '\n' ' ')" ""
t_eq "$(cat "$T/calls/sessions.wrote" 2>/dev/null | tr '\n' ' ')" ""
t_eq "$([ -e "$T/keep/sessions/abc-123.jsonl" ] && echo kept)" ""

t_case "the sessions a turn's agent wrote are kept, and the next turn is seeded from them"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
# What the agent wrote under the state directory of the turn is gone with the
# turn; only the copy in the session's own directory stays.
t_eq "$([ -e "$state" ] && echo left)" ""
t_eq "$(cat "$T/calls/sessions.wrote" | sort | tr '\n' ' ')" "abc-123.jsonl nested "
t_eq "$(cd "$T/keep/sessions" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./abc-123.jsonl ./nested ./nested/part.json "
t_eq "$(cat "$T/keep/sessions/abc-123.jsonl")" '{"session":"abc-123","turn":1}'
# The next turn finds them where it has Pi's sessions: the file and the
# directory the agent of the last turn wrote.
agent_leaves_nothing
rm -f "$T"/calls/*
printf 'Adopt all\n' >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/sessions.given" | sort | tr '\n' ' ')" "abc-123.jsonl nested "
t_eq "$(cat "$T/calls/sessions.first")" '{"session":"abc-123","turn":1}'
t_eq "$(cat "$T/calls/sessions.nested")" '{"part":2}'

t_case "the sessions an earlier turn left are replaced by what the last turn wrote"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
# This turn leaves the file of the session and nothing beside it.
printf '%s\n' '{"session":"abc-123","turn":2}' >"$fx/session.text"
rm -f "$fx/session.part" "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/sessions" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./abc-123.jsonl "
t_eq "$(cat "$T/keep/sessions/abc-123.jsonl")" '{"session":"abc-123","turn":2}'

t_case "a turn whose container wrote no session keeps none, and what an earlier turn left goes"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/keep/sessions" ] && echo left)" ""

t_case "a later turn whose sessions were never kept finds nothing to reopen and sends no answer"
reset
# tehas carried no sessions into the turn, so the stub answers as Pi does for
# a session it made anew: no messages, so no answer is sent.
: >"$fx/no_session"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session abc-123 is gone: Pi opened it with no messages, so the answer was not sent"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","lost":true}'
t_eq "$(cat "$T/calls/docker.stdin" 2>/dev/null)" ""
t_eq "$([ -e "$T/keep/sessions" ] && echo left)" ""

t_case "a turn reads none of an earlier turn's log of the same second"
reset
: >"$fx/no_session"
# The log is named by the second. Left as an earlier turn left it, it holds
# that turn's answer to how many messages the session has. A slow id holds the
# container's start, which is when the log was emptied before.
real_id=$(command -v id)
t_stub id "sleep 2; exec $real_id \"\$@\""
now=$(date +%s)
for s in 0 1 2 3; do
  printf '%s\n' '{"id":"opened","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123","messageCount":6}}' \
    >"$T/home/tehas/logs/plan-7-$(date -u -d "@$((now + s))" +%y%m%d-%H%M%S).jsonl"
done
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
rm -f "$T/bin/id"
t_eq "$t_rc" 2
t_contains "$t_out" "the session abc-123 is gone"

t_case "a later turn of a session that has no directory yet makes one and goes on"
reset
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
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
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""
t_contains "$(cat "$T/calls/gh.comments")" "Tehas gave this planning session 1 image from the issue text."
t_eq "$([ -e "$tree.images" ] && echo left)" ""

t_case "a later turn mounts the same images and fetches nothing"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
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
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""

t_case "a later turn describes nothing again"
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
agent_leaves_nothing
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
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""
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
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
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
agent_leaves abc-123 '{"session":"abc-123","turn":1}'
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
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "Like this"
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "- image-2.jpg, from $A/2222"

t_case "the turn is told the issue, and gets no other mount or variable than a first turn"
t_eq "$(after -e | grep '^PI_ENSEMBLE_PLAN_ISSUE')" "PI_ENSEMBLE_PLAN_ISSUE=7"
t_eq "$(after -v | grep -c images)" 1
t_eq "$(after -v | grep -c -x "$tree.images:$ws.images:ro")" 1
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS PI_ENSEMBLE_PLAN_ISSUE | LC_ALL=C sort)"
t_eq "$(turn_args | unknown_options "$image" "--name $limits --cap-drop --cap-add --security-opt --user -w -v -e" "--rm -i --init")" ""

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
t_eq "$(cd "$T/keep" && find . -mindepth 1 -maxdepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./brief.md ./commit ./images ./sessions "
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
