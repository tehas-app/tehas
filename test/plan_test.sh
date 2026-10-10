#!/usr/bin/env bash
# Tests for bin/tehas-plan: one turn of a planning session. A stub docker
# records its arguments, keeps the first line it is sent and answers with a
# recorded event stream, so no model runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

plan="$T_ROOT/bin/tehas-plan"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

t_stub docker 'printf "%s\n" "$@" >"$T/calls/docker.args.$$"
case " $* " in *" --mode "*) head -n 1 >"$T/calls/docker.stdin"; cat "$T/fx/turn.jsonl" 2>/dev/null ;; esac'
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"

turn_args() { grep -l -x -e '--mode' "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { turn_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
image="tehas/pi-rukas:uid$(id -u)"
tail_of() { turn_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }

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
  rm -f "$T"/calls/* "$T/out.json"
  asked
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(GH_TOKEN=test-token)
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
t_eq "$(cat "$ws/.tehas/brief.md")" "$(cat "$T/brief.md")"
t_eq "$(git -C "$ws" status --porcelain)" ""
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "/plan the work described in the file .tehas/brief.md"
t_contains "$(jq -r .message "$T/calls/docker.stdin")" "issue #7"
t_eq "$(jq -r .type "$T/calls/docker.stdin")" "prompt"

t_case "a first turn starts a new session"
t_eq "$(tail_of)" "$image pi --mode rpc --approve "

t_case "the planning container gets the mounts of a work cycle, no more"
want="$ws:$ws
$T/home/.pi/agent/ensemble-runs:/home/vscode/.pi/agent/ensemble-runs
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
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_NO_DOCKER_SOCKET
PI_ENSEMBLE_NO_SSH
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_WORKSPACE_ROOT"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "nothing that controls the host is reachable, and only one container ran"
args=$(cat "$T"/calls/docker.args.*)
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "--privileged"
t_lacks "$args" "test-token"
t_lacks "$args" "$checkout"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 1

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

t_case "a turn that never settled is a failure and writes no outcome"
reset
grep -v agent_settled "$fx/turn.jsonl" >"$fx/turn.new" && mv "$fx/turn.new" "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2
t_eq "$([ -e "$T/out.json" ] && echo written)" ""

t_case "a run that started again after settling has not settled"
reset
echo '{"type":"agent_start"}' >>"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2

t_case "a turn with neither a message nor a question is a failure"
reset
printf '%s\n' '{"type":"agent_start"}' '{"type":"agent_settled"}' \
  '{"type":"response","command":"get_state","success":true,"data":{"sessionId":"abc-123"}}' \
  '{"type":"response","command":"get_last_assistant_text","success":true,"data":{"text":null}}' >"$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 2

t_case "a line that is not JSON in the stream is skipped, not fatal"
reset
sed -i '2i not json at all' "$fx/turn.jsonl"
t_run "$plan 7 --out $T/out.json --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .session "$T/out.json")" "abc-123"

t_case "a missing token starts no container"
reset
T_ENV=()
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
