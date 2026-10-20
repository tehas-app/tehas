#!/usr/bin/env bash
# Tests for libexec/plan: a turn's session and answer, what tehas reads
# out of a container that keeps running, and the jobs a turn waits for.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/plan_lib.sh
. "$(dirname "$0")/plan_lib.sh" || exit 1

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

t_done
