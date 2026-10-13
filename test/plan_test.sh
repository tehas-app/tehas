#!/usr/bin/env bash
# Tests for libexec/plan: one turn of a planning session. A stub docker
# records its arguments, keeps the first two lines it is sent and answers with
# a recorded event stream, so no model runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

plan="$T_ROOT/libexec/plan"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
# The directory on the host behind it, for issue 7.
tree=$ws-plan-7
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

# The turn's workspace is gone when the turn is over, so the container notes
# what it was handed there.
t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
case " $* " in *" --mode "*)
  ws=$T/home/tehas/work/widget-plan-7
  cat "$ws/.tehas/brief.md" >"$T/calls/brief.seen" 2>/dev/null
  git -C "$ws" status --porcelain >"$T/calls/tree.status" 2>&1
  IFS= read -r line; printf "%s\n" "$line" >"$T/calls/docker.stdin"
  # The second line may never come: do not wait for it for ever.
  IFS= read -r -t 5 line; printf "%s\n" "$line" >"$T/calls/docker.stdin2"
  # A case that needs a container that lives for a while writes its part here.
  if [ -e "$T/fx/live" ]; then . "$T/fx/live"; else cat "$T/fx/turn.jsonl" 2>/dev/null; fi ;;
esac'
# The command looks at the log once a second. Here a look costs no time, so a
# container that lives for a second is looked at many times.
t_stub sleep
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# A host with the root step done: the slice loaded from its file with 8 GiB,
# so a container's ceiling is 2048m, Docker on the systemd driver and cgroup
# v2, and nothing in the slice.
t_stub systemctl 'cat "$T/slice.show"'
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  slice_state 0.00
}
limits="--cgroup-parent --memory --memory-swap --memory-reservation --pids-limit --cpu-shares"

turn_args() { grep -l -x -e '--mode' "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { turn_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
image="tehas/pi-rukas:uid$(id -u)"
tail_of() { turn_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }
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
reset() {
  rm -f "$T"/calls/* "$T/out.json" "$fx/live"
  asked
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  printf '# Faster search\n\nMake it fast.\n' >"$T/brief.md"
}

for bad in "" "7" "x --out $T/out.json --brief $T/brief.md" "7 --out $T/out.json" \
  "7 --out $T/out.json --brief $T/brief.md --session abc" "7 --out $T/out.json --session abc" \
  "7 --out $T/out.json --brief $T/missing.md" "7 --out $T/out.json --session 'a b;c' --answer $T/brief.md" \
  "7 --out $T/out.json --brief $T/brief.md --verbose"; do
  t_case "a call that is not a turn ('$bad') is refused before anything runs"
  reset
  t_run "$plan $bad"
  t_eq "$t_rc" 64
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
done

t_case "a first turn writes the session, the message and the question"
reset
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .session "$T/out.json")" "abc-123"
t_eq "$(jq -r .text "$T/out.json")" "Two ways.
Pick one."
t_eq "$(jq -r .question.question "$T/out.json")" "Adopt my resolutions?"
t_eq "$(jq -c '.question.options' "$T/out.json")" '[{"label":"Adopt all","description":"Apply all five"},{"label":"Change some","description":""}]'

t_case "the approved text is in the workspace as a file git ignores, and the prompt points at it"
t_eq "$(cat "$T/calls/brief.seen")" "$(cat "$T/brief.md")"
t_eq "$(cat "$T/calls/tree.status")" ""

t_case "the workspace is the turn's own, and it is gone when the turn is over"
t_eq "$([ -e "$tree" ] && echo left)" ""
t_eq "$([ -e "$ws" ] && echo made)" ""
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "/plan the work described in the file .tehas/brief.md"
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "issue #7"
t_eq "$(jq -r .type "$T/calls/docker.stdin")" "prompt"

t_case "the session is asked for right after the prompt, so a turn that fails late still has it"
t_eq "$(jq -c . "$T/calls/docker.stdin2")" '{"id":"state","type":"get_state"}'

t_case "a first turn starts a new session"
t_eq "$(tail_of)" "$image pi --mode rpc --approve "

t_case "the planning container gets the mounts of a work cycle, no more"
want="$tree:$ws
$T/home/.pi/agent/ensemble-runs/plan-7:/home/vscode/.pi/agent/ensemble-runs
$T/home/.pi/agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro
$T/home/.pi/agent/sessions:/home/vscode/.pi/agent/sessions
$T/home/.vipune:/home/vscode/.vipune
pi-ensemble-bun-cache:/home/vscode/.bun
pi-ensemble-cache:/home/vscode/.cache"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

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
PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_NO_DOCKER_SOCKET
PI_ENSEMBLE_NO_SSH
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "the planning container gets no option beyond the documented ones"
t_eq "$(turn_args | unknown_options "$image" "--name $limits -w -v -e" "--rm -i --init")" ""
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
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 1

t_case "the turn gets the slice and a cycle's limits, and the host aliases from the settings"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=1g PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(after --cgroup-parent)" "tehas.slice"
t_eq "$(after --memory)" "2048m"
t_eq "$(after --memory-swap)" "2048m"
t_eq "$(after --memory-reservation)" "1g"
t_eq "$(after --pids-limit)" "4096"
t_eq "$(after --cpu-shares)" "1024"
t_lacks "$(turn_args)" "--cpus"
t_lacks "$(turn_args)" "--cidfile"
t_contains "$(turn_args)" "--add-host=models:192.0.2.9"
t_eq "$(turn_args | unknown_options "$image" "--name $limits -w -v -e" "--rm -i --init")" ""

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

# A turn has a workspace and a transcripts directory of its own on the host.
# They are mounted at the paths a cycle's are.
t_case "docker is given what tehas work gives a cycle's container: only the name, the two directories of its own, the cycle's cid file and what pi is told differ"
turn_args | options | sed -e "s|^$tree:|$ws-7:|" -e 's|/ensemble-runs/plan-7:|/ensemble-runs/issue-7:|' >"$T/plan.options"
rm -f "$T"/calls/*
t_run "$T_ROOT/libexec/work 7"
turn_args | options >"$T/work.options"
t_eq "$([ -s "$T/work.options" ] && echo recorded)" recorded
t_contains "$(turn_args)" "--cidfile"
t_eq "$(cat "$T/plan.options")" "$(cat "$T/work.options")"
t_eq "$(tail_of)" "$image pi --mode rpc --approve "

t_case "a later turn reopens the session and sends the answer as it was written"
reset
printf '%s\n' '- "Adopt all", but keep `tagged-at` open' '' "second line with 'quotes' and \$HOME" >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image pi --mode rpc --approve --session abc-123 "
t_eq "$(jq -r .message "$T/calls/docker.stdin")" "$(cat "$T/answer.md")"
t_eq "$([ -e "$ws/.tehas/brief.md" ] && echo present)" ""

t_case "a turn that only wrote a message has no question"
reset
grep -v tool_execution_end "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .question "$T/out.json")" "null"

t_case "a question from an earlier, interactive answer is not asked again"
reset
sed -i 's/"deferred":true,//' "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$(jq -r .question "$T/out.json")" "null"

t_case "a turn that asked with the tool and wrote nothing still counts"
reset
sed -i 's/"data":{"text":"Two ways.\\nPick one."}/"data":{"text":null}/' "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" ""
t_eq "$(jq -r .question.question "$T/out.json")" "Adopt my resolutions?"

t_case "a turn that never settled is a failure and leaves only its session"
reset
grep -v agent_settled "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a first turn that never settled and never named its session writes nothing"
reset
grep -v -e agent_settled -e get_state "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2
t_eq "$([ -e "$T/out.json" ] && echo written)" ""

t_case "a settled first turn whose session is unknown is a failure and writes nothing"
reset
grep -v get_state "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session id is unknown"
t_eq "$([ -e "$T/out.json" ] && echo written)" ""

t_case "a later turn that never settled leaves the session it was given"
reset
grep -v -e agent_settled -e get_state "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a run that started again after settling has not settled"
reset
echo '{"type":"agent_start"}' >>"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a turn with neither a message nor a question is a failure"
reset
printf '%s\n' '{"type":"agent_start"}' '{"type":"agent_settled"}' \
  '{"type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123"}}' \
  '{"type":"response","command":"get_last_assistant_text","success":true,"data":{"text":null}}' >"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "said and asked nothing"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123"}'

t_case "a line that is not JSON in the stream is skipped, not fatal"
reset
sed -i '2i not json at all' "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .session "$T/out.json")" "abc-123"

t_case "a very long log gives the same outcome"
reset
{
  for ((i = 0; i < 20000; i++)); do printf '{"type":"message_update","delta":"word %s"}\n' "$i"; done
  cat "$fx/turn.jsonl"
} >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.session, .text, .question.question, (.question.options | length)]' "$T/out.json")" '["abc-123","Two ways.\nPick one.","Adopt my resolutions?",2]'

t_case "a start buried in a very long log after the settle still means not settled"
reset
{
  cat "$fx/turn.jsonl"
  echo '{"type":"agent_start"}'
  for ((i = 0; i < 20000; i++)); do printf '{"type":"message_update","delta":"word %s"}\n' "$i"; done
} >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2

t_case "a last line with no line end is still read"
reset
grep -v get_last_assistant_text "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
printf '%s' '{"id":"text","type":"response","command":"get_last_assistant_text","success":true,"data":{"text":"Cut short"}}' >>"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" "Cut short"
reset
grep -v -e agent_settled -e '"response"' "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
printf '%s' '{"type":"agent_settled"}' >>"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
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
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","text":"Done live.","question":null}'
t_eq "$(jq -r .type "$T/calls/docker.asked" | tr '\n' ' ')" "get_state get_last_assistant_text "
t_eq "$([ "$(wc -l <"$T/calls/sleep")" -ge 20 ] && echo looked)" looked

t_case "a first turn that runs out of time is stopped and leaves its session"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=1
live '"$nap" 3'
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
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
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
refused
t_contains "$t_out" "tehas plan: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  without
  echo "$answer" >"$T/docker.info"
  t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
  refused
  t_contains "$t_out" "tehas plan: "
  t_contains "$t_out" "ocker"
done

t_case "a reserve above the ceiling is refused"
without
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=3g
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
refused
t_contains "$t_out" "tehas plan: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m"

t_case "the twelve words are all present or nothing starts"
# The command beside a slice library that says yes and hands out a short list,
# or a list that names another parent.
short=$T/short
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
ln -s "$plan" "$short/libexec/plan"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --pids-limit 4096 --cpu-shares 1024" ""; do
  without
  printf '%s\n' 'tehas_slice_ok() { return 0; }' "tehas_slice_limits() { TEHAS_SLICE_LIMITS=($words); }" >"$short/lib/slice.sh"
  t_run "$short/libexec/plan 7 --out $T/out.json --brief $T/brief.md"
  refused
  t_contains "$t_out" "tehas plan: the container limits could not be made, not starting"
done

t_case "a missing token starts no container"
reset
T_ENV=("TEHAS_CGROUP_ROOT=$T_CGROUP")
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 64
t_contains "$t_out" "GH_TOKEN is not set"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_case "an invalid instance starts no container"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 78
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_done
