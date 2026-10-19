#!/usr/bin/env bash
# Tests for libexec/dispatch: tehas:next and tehas:blocked, the gate of the
# lowest issues, the demand written for tehas status, and live/dispatch.json.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# --- Ordering the queue: tehas:next and tehas:blocked -----------------------
# bnote <id 10-59> <author> <body> [<editor>]: one comment, written at second <id>.
bnote() {
  jq -nc --argjson id "$1" --arg a "$2" --arg b "$3" --arg e "${4:-}" '{databaseId: $id, author: {login: $a}, body: $b,
    createdAt: "2026-10-01T01:00:\($id)Z", lastEditedAt: (if $e == "" then null else "2026-10-01T02:00:00Z" end),
    editor: (if $e == "" then null else {login: $e} end)}'
}
notes() { # <issue> <comment>...
  local n=$1
  shift
  printf '%s\n' "$@" | jq -s '{data: {repository: {issue: {comments: {nodes: .}}}}}' >"$fx/comments-$n.json"
}
states() { # <number>=<state>...: the blockers as GitHub gives them; an empty state is a number it does not know
  printf '%s\n' "$@" | jq -Rn '{data: {repository: (reduce (inputs | split("=")) as $p ({};
    .["b\($p[0])"] = (if $p[1] == "" then null else {state: $p[1]} end)))}}' >"$fx/blockers.json"
}

t_case "tehas:next from an allowed account comes after urgent and before lower numbers"
reset
slots 3
queue 3 "8 tehas:urgent" "9 tehas:next"
label_events 8 tehas:urgent=alice
label_events 9 tehas:next=alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "8 9 3 "

t_case "tehas:next from the fixer counts"
reset
slots 2 TEHAS_FIXER_LOGIN=tehas-fixer
queue 3 "9 tehas:next"
label_events 9 tehas:next=tehas-fixer
t_run "$dispatch"
t_eq "$(started)" "9 3 "

t_case "tehas:next from an outsider is ignored, and the poll says so"
reset
slots 2 TEHAS_FIXER_LOGIN=tehas-fixer
queue 3 "9 tehas:next"
label_events 9 tehas:next=alice tehas:next=mallory
t_run "$dispatch"
t_eq "$(started)" "3 9 "
t_contains "$t_out" "ignore #9: tehas:next was applied by 'mallory', not an allowed account"

t_case "tehas:next taken off since it was applied is ignored"
reset
slots 2
queue 3 "9 tehas:next"
label_events 9 tehas:next=alice -tehas:next=alice
t_run "$dispatch"
t_eq "$(started)" "3 9 "
t_contains "$t_out" "ignore #9: tehas:next was taken off by 'alice' since it was applied"

t_case "with no fixer on the instance, the fixer account's tehas:next is an outsider's"
reset
slots 2
queue 3 "9 tehas:next"
label_events 9 tehas:next=tehas-fixer
t_run "$dispatch"
t_eq "$(started)" "3 9 "
t_contains "$t_out" "ignore #9: tehas:next was applied by 'tehas-fixer', not an allowed account"

t_case "a blocked issue waits while its blocker is open, and the slot goes to the next one"
reset
queue "7 tehas:blocked" 8
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice $'Blocked by #12.\n\n<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "8 "
t_contains "$t_out" "wait #7: tehas:blocked holds it: #12 is open"
t_lacks "$(t_calls gh)" "issue edit 7 "

t_case "a blocked issue starts once every blocker it names is closed"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12,34 -->')"
states 12=CLOSED 34=CLOSED
t_run "$dispatch"
t_eq "$(started)" "7 "
t_contains "$(t_calls gh)" "b12: issueOrPullRequest(number: 12)"
t_contains "$(t_calls gh)" "b34: issueOrPullRequest(number: 34)"

t_case "a blocked issue that starts is recorded as the ready label's, not the blocked label's"
reset
slots 1 TEHAS_FIXER_LOGIN=tehas-fixer
queue "7 tehas:blocked"
label_events 7 tehas:blocked=tehas-fixer
notes 7 "$(bnote 10 tehas-fixer '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" "7 "
t_eq "$(cat "$cycles_dir/7.by")" alice
t_eq "$(jq -r '"\(.by) \(.at)"' "$approved_file")" "alice 2026-10-01T00:00:00Z"
t_lacks "$t_out" "authorized by @tehas-fixer"

t_case "one open blocker of two holds the issue"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12,34 -->')"
states 12=CLOSED 34=OPEN
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: #34 is open"

t_case "the fixer's tehas:blocked and its comment count"
reset
slots 1 TEHAS_FIXER_LOGIN=tehas-fixer
queue "7 tehas:blocked"
label_events 7 tehas:blocked=tehas-fixer
notes 7 "$(bnote 10 tehas-fixer '<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: #12 is open"

t_case "tehas:blocked from an outsider is ignored, and its blockers are not asked about"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=mallory
notes 7 "$(bnote 10 mallory '<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch"
t_eq "$(started)" "7 "
t_contains "$t_out" "ignore #7: tehas:blocked was applied by 'mallory', not an allowed account"
t_lacks "$(t_calls gh)" "issueOrPullRequest"

t_case "with no fixer on the instance, the fixer account's tehas:blocked is an outsider's"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=tehas-fixer
notes 7 "$(bnote 10 tehas-fixer '<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch"
t_eq "$(started)" "7 "
t_contains "$t_out" "ignore #7: tehas:blocked was applied by 'tehas-fixer', not an allowed account"

t_case "tehas:blocked taken off since it was applied holds nothing"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice -tehas:blocked=alice
t_run "$dispatch"
t_eq "$(started)" "7 "
t_contains "$t_out" "ignore #7: tehas:blocked was taken off by 'alice' since it was applied"

t_case "a blocker comment of an outsider is ignored"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')" "$(bnote 11 mallory '<!-- tehas:blocked by=13 -->')"
states 12=CLOSED 13=OPEN
t_run "$dispatch"
t_eq "$(started)" "7 "
t_lacks "$(t_calls gh)" "b13:"

t_case "with only an outsider's blocker comment the label holds, with no list"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 11 mallory '<!-- tehas:blocked by=13 -->')"
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: no comment of an allowed account or the fixer names its blockers"

t_case "the newest blocker comment decides"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 11 alice '<!-- tehas:blocked by=34 -->')" "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=OPEN 34=CLOSED
t_run "$dispatch"
t_eq "$(started)" "7 "

t_case "a malformed blocker comment holds the issue"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=twelve -->')"
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: the newest comment that names its blockers cannot be read"

t_case "a blocker comment edited by another account holds the issue"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->' mallory)"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: the newest comment that names its blockers was edited by 'mallory'"

t_case "a blocker comment edited by its author counts"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->' alice)"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" "7 "

t_case "a blocker GitHub gives no state for holds the issue"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12,99 -->')"
states 12=CLOSED 99=
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: the state of #99 could not be read"

t_case "blockers whose states cannot be read at all hold the issue, and the poll goes on"
reset
queue "7 tehas:blocked" 8
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
: >"$fx/blockers-fail"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "8 "
t_contains "$t_out" "wait #7: tehas:blocked holds it: the state of #12 could not be read"

t_case "a dry run says what holds a blocked issue"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "wait #7: tehas:blocked holds it: #12 is open"
t_lacks "$t_out" "would run a cycle for #7"

# The demand, written on every poll for tehas status ($demand_file).
ready_at() { # <issue> <login> <time>: tehas:ready applied by <login> at <time>
  jq -nc --arg w "$2" --arg at "$3" '[{event: "labeled", label: {name: "tehas:ready"}, actor: {login: $w}, created_at: $at}]' >"$fx/events-$1.json"
}

t_case "demand: a poll with no room counts the waiting issues and the oldest acceptance"
reset
ready 7 8
ready_at 7 alice 2026-10-17T10:00:00Z
ready_at 8 alice 2026-10-17T09:00:00Z
hold 5
t_run "$dispatch"
release
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -c '{waiting, oldest}' "$demand_file")" '{"waiting":2,"oldest":"2026-10-17T09:00:00Z"}'
t_eq "$(jq -r '.time | fromdateiso8601 | type' "$demand_file")" number

t_case "demand: the health file carries it after the poll"
reset
ready_at 7 alice 2026-10-17T10:00:00Z
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '.checks.demand | {waiting, oldest}' "$T/home/tehas/live/health.json")" '{"waiting":1,"oldest":"2026-10-17T10:00:00Z"}'

t_case "demand: a renamed repository is not asked for it"
reset
echo '{"nameWithOwner":"acme/gadget"}' >"$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$([ -e "$demand_file" ] && echo there)" ""

t_case "demand: a poll paused by the flag or the label still writes it"
reset
ready_at 7 alice 2026-10-17T10:00:00Z
mkdir -p "$T/home/tehas/state"
jq -nc '{by: "boss", at: "2026-10-15T10:00:00Z", reason: "weekend"}' >"$T/home/tehas/state/pause.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -c '{waiting, oldest}' "$demand_file")" '{"waiting":1,"oldest":"2026-10-17T10:00:00Z"}'
reset
ready_at 7 alice 2026-10-17T10:00:00Z
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -r .waiting "$demand_file")" 1

t_case "demand: a stranger's label, a running cycle and a planning session do not wait"
reset
ready 7 8 9 10
ready_at 7 mallory 2026-10-17T08:00:00Z
ready_at 8 alice 2026-10-17T08:30:00Z
ready_at 9 alice 2026-10-17T09:00:00Z
ready_at 10 alice 2026-10-17T10:00:00Z
mkdir -p "$T/home/tehas/state"
echo '{"comment":1}' >"$T/home/tehas/state/plan-9.json"
slots 2
hold 8
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{waiting, oldest}' "$demand_file")" '{"waiting":1,"oldest":"2026-10-17T10:00:00Z"}'

t_case "demand: the fixer's label waits like a person's"
reset
slots 1 TEHAS_FIXER_LOGIN=tehas-fixer
ready_at 7 tehas-fixer 2026-10-17T10:00:00Z
hold 5
t_run "$dispatch"
release
t_eq "$(jq -r .waiting "$demand_file")" 1

t_case "demand: nothing waiting is a count of none and no oldest"
reset
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(jq -c '{waiting, has: has("oldest")}' "$demand_file")" '{"waiting":0,"has":false}'

t_case "demand: events that cannot be read leave no count, and do not end a paused poll"
reset
ready 7
rm -f "$fx/events-7.json"
mkdir -p "$T/home/tehas/live" "$T/home/tehas/state"
echo '{"time":"2026-10-17T10:00:00Z","waiting":3}' >"$demand_file"
jq -nc '{by: "boss", at: "2026-10-15T10:00:00Z", reason: "weekend"}' >"$T/home/tehas/state/pause.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$([ -e "$demand_file" ] && echo there)" ""

t_case "demand: a dry run writes nothing"
reset
hold 5
t_run "$dispatch --dry-run"
release
t_eq "$([ -e "$demand_file" ] && echo there)" ""

t_case "the slots and the room under the review cap reach live/dispatch.json"
reset
slots 3
echo '[{"number":20},{"number":21}]' >"$fx/awaiting.json"
echo '[]' >"$fx/ready.json"
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{free, awaitRoom}' "$dispatch_json")" '{"free":2,"awaitRoom":2}'
reset
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{free, awaitRoom}' "$dispatch_json")" '{"free":0,"awaitRoom":null}'

# --- The gate of the lowest issues (TEHAS_LOW_GATE) -------------------------
# The gate is an instance's own command. The stub answers <status> and writes
# one line for every run, so a case can say how often the poll asked it. The
# paths are written into the stub: the poll hands the gate nothing of its
# environment but PATH, so a stub cannot name them through a variable.
gate_answer() { # <status>: what the gate answers from now on
  t_stub gate-run "echo ran >>\"$T/calls/gate-runs\"
exit $1"
}
gate_runs() { t_calls gate-runs | wc -l; }
gate_held=$T/home/tehas/state/low-gate.held
gate_log=$T/home/tehas/logs/low-gate.log
gate_there() { [ -e "$gate_held" ] && echo there; }
# gate_path: the instance names the stub, and the case puts the stub there or
# leaves it out. It also clears the held file, which reset leaves alone, and
# sets the line to give to slots.
gate_path() {
  rm -f "$gate_held"
  gate_setting=TEHAS_LOW_GATE=$T/bin/gate-run
}
# gate <status>: the instance names a stub that answers <status>.
gate() {
  gate_answer "$1"
  gate_path
}
# gate_up <seconds up> [ignore]: a gate that stays up instead of answering.
# With ignore it takes no notice of being asked to end at the end of its seconds.
gate_up() { # <seconds to stay up> [ignore]
  local body="exec sleep $1"
  if [ "${2:-}" = ignore ]; then
    body=$(printf 'trap "" TERM\n%s' "$body")
  fi
  t_stub gate-run "$body"
  gate_path
}

t_case "with no gate on the instance, the lowest issues start as they did before it"
reset
slots 3
queue 7 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 "
t_eq "$(gate_runs)" 0
t_lacks "$t_out" "gate is closed"

t_case "an open gate starts the lowest issues, and answers once for three of them"
reset
gate 0
slots 3 "$gate_setting"
queue 7 8 11
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 11 "
t_eq "$(gate_runs)" 1
t_lacks "$t_out" "gate is closed"
t_eq "$(gate_there)" ""

t_case "a closed gate holds the lowest issues while an urgent and a tehas:next issue start"
reset
gate 1
slots 3 "$gate_setting"
queue "7 tehas:urgent" "8 tehas:next" 11 12
label_events 7 tehas:urgent=alice
label_events 8 tehas:next=alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 "
t_contains "$t_out" "wait #11: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "wait #12: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_eq "$(gate_runs)" 1
t_lacks "$(t_calls gh)" "issue edit 11 "
t_lacks "$(t_calls gh)" "issue edit 12 "

t_case "a gate the host does not have holds the lowest issues, and the poll ends 0"
reset
gate_path
rm -f "$T/bin/gate-run"
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 127)"

t_case "a gate that is not executable holds the lowest issues, and the poll ends 0"
reset
gate_path
rm -f "$T/bin/gate-run"
: >"$T/bin/gate-run"
chmod 644 "$T/bin/gate-run"
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 126)"

t_case "the gate answers once a poll, whatever the number of issues it holds"
reset
gate 1
slots 4 "$gate_setting"
queue 7 8 11 12
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_eq "$(gate_runs)" 1
t_contains "$t_out" "wait #12: the low-priority gate is closed (exit 1)"

t_case "a poll with urgent and tehas:next issues only never asks the gate"
reset
gate 1
slots 3 "$gate_setting"
queue "7 tehas:urgent" "8 tehas:next"
label_events 7 tehas:urgent=alice
label_events 8 tehas:next=alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 "
t_eq "$(gate_runs)" 0
t_lacks "$t_out" "low issues"

t_case "a hold is said once when it begins and once when it ends, with its file between them"
reset
gate 1
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_eq "$(gate_there)" "there"
# The marker says only that low issues are held. Each poll's answer is in the
# log line, so the file carries nothing that could go stale.
t_eq "$(cat "$gate_held")" ""
t_run "$dispatch"
t_lacks "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_eq "$(gate_there)" "there"
t_eq "$(gate_runs)" 2
gate_answer 0
t_run "$dispatch"
t_contains "$t_out" "low issues may start: TEHAS_LOW_GATE answered 0"
t_lacks "$t_out" "low issues wait"
t_eq "$(gate_there)" ""
t_eq "$(started)" "7 "

t_case "a dry run says the hold and leaves no held file behind"
reset
gate 1
slots 2 "$gate_setting"
queue 7
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_eq "$(gate_there)" ""
t_eq "$(t_calls fake-start)" ""

t_case "a dry run says the end of a hold a real poll left, and leaves the file"
reset
gate 0
slots 2 "$gate_setting"
queue 7
: >"$gate_held"
t_run "$dispatch --dry-run"
t_contains "$t_out" "low issues may start: TEHAS_LOW_GATE answered 0"
t_eq "$(gate_there)" "there"

t_case "a hold is said once although the gate answers differently while it lasts"
reset
gate 1
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
gate_answer 127
t_run "$dispatch"
t_lacks "$t_out" "low issues wait"
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 127)"
t_eq "$(gate_there)" "there"
t_eq "$(cat "$gate_held")" ""

# With the bound of a minute the same gate would have answered 0 and issue 7
# would have started, so this case is also the one that says the seconds come
# from TEHAS_LOW_GATE_WAIT.
t_case "a gate that outlives its seconds answers as a closed gate"
reset
gate_up 20
slots 2 "$gate_setting" TEHAS_LOW_GATE_WAIT=1
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 124)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 124"
t_eq "$(gate_there)" "there"

# It was asked to end at the second and killed ten seconds after that: the run
# ends by a signal, which is 128 plus 9.
t_case "a gate that ignores the end of its seconds is killed, and holds them too"
reset
gate_up 30 ignore
slots 2 "$gate_setting" TEHAS_LOW_GATE_WAIT=1
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 137)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 137"

t_case "the gate is asked with nothing of the poll's environment but PATH"
reset
t_stub gate-run "env >\"$T/calls/gate.env\"
exit 0"
gate_path
# What the host's own environment holds: the bot's token and the model
# endpoints' keys. The gate is the instance's command, not a party to the poll.
T_ENV+=(GH_TOKEN=gh-secret-token TEHAS_MODEL_KEY_TEST=key-for-the-model)
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
gate_env=$(cat "$T/calls/gate.env")
t_contains "$gate_env" "PATH="
t_lacks "$gate_env" "gh-secret-token"
t_lacks "$gate_env" "key-for-the-model"
t_lacks "$gate_env" "TEHAS_REPO"
t_lacks "$gate_env" "TEHAS_LOW_GATE_WAIT"

t_case "what the gate prints is kept where the person can read it, and read by nothing else"
reset
t_stub gate-run 'echo "the price feed would not answer"
exit 3'
gate_path
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 3)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 3"
t_lacks "$t_out" "the price feed would not answer"
t_eq "$(cat "$gate_log")" "the price feed would not answer"
# Every ask rewrites it: the file holds the last ask's words, not a day of them.
t_run "$dispatch"
t_eq "$(cat "$gate_log")" "the price feed would not answer"
t_eq "$(wc -l <"$gate_log")" 1

t_case "a closed gate holds the rest of the lowest issues with no call about them"
reset
gate 1
slots 4 "$gate_setting"
queue 7 8 11 12
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_eq "$(gate_runs)" 1
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "wait #12: the low-priority gate is closed (exit 1)"
# The first issue of the kind paid the checks of the rules above and asked; the
# rest are held by that answer alone. The per-issue calls of those checks name
# the issue in the question this poll asks GitHub.
t_contains "$(t_calls gh)" "number=7"
t_eq "$(t_calls gh | grep -c -e number=8 -e number=11 -e number=12)" 0

t_case "a held marker that is not a regular file is neither read nor written"
reset
gate 1
mkdir -p "$T/home/tehas/state"
echo kept >"$T/home/keep"
ln -s "$T/home/keep" "$gate_held"
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "low-gate.held is not a regular file"
t_eq "$(cat "$T/home/keep")" "kept"

t_done
