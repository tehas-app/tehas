#!/usr/bin/env bash
# Tests for tehasbot dispatch: tehas:blocked, the demand written for tehas
# status, and live/dispatch.json.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# --- Holding the queue: tehas:blocked ---------------------------------------
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

t_case "an open blocked-by link holds a ready issue, and the slot goes to the next one"
reset
queue 7 8
echo '{"number":7,"state":"open","issue_dependencies_summary":{"blocked_by":1}}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "8 "
t_contains "$(t_calls gh)" "issueDependenciesSummary { blockedBy }"
t_contains "$t_out" "wait #7: a blocked-by link holds it: 1 blocker is open"

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
slots 1
queue "7 tehas:blocked"
label_events 7 tehas:blocked=tehas-bot
notes 7 "$(bnote 10 tehas-bot '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" "7 "
t_eq "$(t_row cycle-show 7 -- .cycle.started_by)" alice
t_eq "$(approval '"\(.by) \(.at)"')" "alice 2026-10-01T00:00:00Z"
t_lacks "$t_out" "authorized by @tehas-bot"

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
slots 1
queue "7 tehas:blocked"
label_events 7 tehas:blocked=tehas-bot
notes 7 "$(bnote 10 tehas-bot '<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: #12 is open"

# A GitHub App as the fixer: its label events are REST's, <slug>[bot], and its
# comments GraphQL's, <slug>. Either form of the setting is that one account.
{
  t_case "the app as the fixer: its tehas:blocked and its comment count"
  reset
  slots 1
  queue "7 tehas:blocked"
  label_events 7 'tehas:blocked=tehas-bot[bot]'
  notes 7 "$(bnote 10 tehas-bot '<!-- tehas:blocked by=12 -->')"
  states 12=OPEN
  t_run "$dispatch"
  t_eq "$(started)" ""
  t_contains "$t_out" "wait #7: tehas:blocked holds it: #12 is open"

  t_case "the app as the fixer: a comment by a name like its own names no blockers"
  reset
  slots 1
  queue "7 tehas:blocked"
  label_events 7 'tehas:blocked=tehas-bot[bot]'
  notes 7 "$(bnote 10 tehas-bot2 '<!-- tehas:blocked by=12 -->')"
  states 12=OPEN
  t_run "$dispatch"
  t_eq "$(started)" ""
  t_contains "$t_out" "wait #7: tehas:blocked holds it: no comment of an allowed account or the millwright names its blockers"
}

t_case "tehas:blocked from an outsider is ignored, and its blockers are not asked about"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=mallory
notes 7 "$(bnote 10 mallory '<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch"
t_eq "$(started)" "7 "
t_contains "$t_out" "ignore #7: tehas:blocked was applied by 'mallory', not an allowed account"

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

t_case "with only an outsider's blocker comment the label holds, with no list"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 11 mallory '<!-- tehas:blocked by=13 -->')"
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: no comment of an allowed account or the millwright names its blockers"

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

# alone <issue> [<label>...]: the issue carries tehas:blocked and no tehas:ready.
alone() {
  local n=$1
  shift
  echo '[]' >"$fx/ready.json"
  jq -nc --argjson n "$n" '[{number: $n, labels: ([{name: "tehas:blocked"}] + [$ARGS.positional[] | {name: .}])}]' \
    --args "$@" >"$fx/blocked.json"
}
# blocked_by <issue> <login>: tehas:blocked by that account is the issue's only label event.
blocked_by() {
  jq -nc --arg w "$2" '[{event: "labeled", label: {name: "tehas:blocked"}, actor: {login: $w}, created_at: "2026-10-01T00:01:00Z"}]' \
    >"$fx/events-$1.json"
}

t_case "tehas:blocked alone starts the issue once its blockers close, and comes off"
reset
alone 7
blocked_by 7 alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:blocked"
t_lacks "$(t_calls gh)" "--remove-label tehas:ready"
t_contains "$(t_calls gh)" "authorized by @alice's \`tehas:blocked\` label"
t_eq "$(approval '"\(.by) \(.at)"')" "alice 2026-10-01T00:01:00Z"

t_case "tehas:blocked alone waits while a blocker is open, and changes nothing"
reset
alone 7
blocked_by 7 alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=OPEN
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: tehas:blocked holds it: #12 is open"
t_lacks "$(t_calls gh)" "issue edit 7 "

t_case "tehas:blocked alone from an outsider starts nothing"
reset
alone 7
blocked_by 7 mallory
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "tehas:blocked was applied by 'mallory', not an allowed account"
t_lacks "$(t_calls gh)" "issue edit 7 "

t_case "tehas:blocked alone from the fixer holds the issue but does not start it"
reset
slots 1
alone 7
blocked_by 7 tehas-bot
notes 7 "$(bnote 10 tehas-bot '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" ""
t_lacks "$(t_calls gh)" "issue edit 7 "

t_case "tehas:blocked alone on a body an outsider edited since starts nothing"
reset
alone 7
blocked_by 7 alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-10-02T00:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit.json"
t_run "$dispatch"
t_eq "$(started)" ""

t_case "tehas:blocked alone on an issue in another flow is not asked about"
reset
alone 7 tehas:awaiting-merge
blocked_by 7 alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" ""

t_case "tehas:blocked alone with an open pull request that closes it comes off"
reset
alone 7
blocked_by 7 alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
jq -nc '{data: {repository: {issue: {closedByPullRequestsReferences: {nodes: [{url: "https://github.com/acme/widget/pull/90", state: "OPEN"}]}}}}}' >"$fx/prs-7.json"
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:blocked"
t_lacks "$(t_calls gh)" "--remove-label tehas:ready"

t_case "tehas:ready beside tehas:blocked: the start takes both off"
reset
queue "7 tehas:blocked"
label_events 7 tehas:blocked=alice
notes 7 "$(bnote 10 alice '<!-- tehas:blocked by=12 -->')"
states 12=CLOSED
t_run "$dispatch"
t_eq "$(started)" "7 "
t_contains "$(t_calls gh)" "--remove-label tehas:ready --remove-label tehas:blocked"

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
# The one dispatch case that reads health's output, so it runs the real
# command rather than the stub the lib gives every other case.
real_health
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
t_plan "$T/home/tehas" 9 '{"turns":1,"comment":1}'
slots 2
hold 8
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{waiting, oldest}' "$demand_file")" '{"waiting":1,"oldest":"2026-10-17T10:00:00Z"}'

t_case "demand: the fixer's label waits like a person's"
reset
slots 1
ready_at 7 tehas-bot 2026-10-17T10:00:00Z
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

t_case "demand: a dry run writes nothing"
reset
hold 5
t_run "$dispatch --dry-run"
release
t_eq "$([ -e "$demand_file" ] && echo there)" ""

# --- The fault of a sandbox that cannot push --------------------------------
# The host file Part 1's start writes (console/cycle/src/prepare.rs), read by
# the poll. A case writes it with its own time, so nothing here reads the clock
# of the host: the poll's `now` is the real one. `at` cannot be one minute in
# the future -- the reader holds what it cannot date -- and ten minutes would
# hold the wrong way on a runner that stops there, so a fresh fault is asked to
# be one minute old and an old one an hour.
push_fault() { # <json>: what to write where the start that failed leaves it
  mkdir -p "$T/home/tehas/state"
  printf '%s\n' "$1" >"$T/home/tehas/state/push-fault.json"
}

# The fault of a minute ago, in the shape Part 1 writes it: well inside the
# ten minutes the poll holds on, and far enough from the next poll's clock.
push_fault_now() {
  push_fault "$(jq -nc --arg at "$(date -u -d '-1 minute' +%Y-%m-%dT%H:%M:%SZ)" \
    '{at: $at, login: "tehas-bot", line: "remote: Permission to acme/widget denied to tehas-bot"}')"
}

# The fault of an hour ago: past the wait, so the poll starts as it does today.
push_fault_old() {
  push_fault "$(jq -nc --arg at "$(date -u -d '-1 hour' +%Y-%m-%dT%H:%M:%SZ)" \
    '{at: $at, login: "tehas-bot", line: "remote: Permission to acme/widget denied to tehas-bot"}')"
}

t_case "a fresh push fault starts no cycle, and the log says why once"
reset
slots 3
ready 7 8
push_fault_now
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the sandbox cannot push: remote: Permission to acme/widget denied to tehas-bot (tehas-bot)"
t_eq "$(grep -c 'the sandbox cannot push' <<<"$t_out")" 1
t_eq "$(jq -r .why "$dispatch_json")" lane

t_case "a push fault older than ten minutes starts the queue as it does today"
reset
slots 3
ready 7 8
push_fault_old
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_lacks "$t_out" "the sandbox cannot push"
t_eq "$(jq -r .why "$dispatch_json")" ""

t_case "with no fault on record the queue starts, and nothing is said of it"
reset
slots 3
ready 7 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_lacks "$t_out" "the sandbox cannot push"

t_case "a fault that cannot be read still holds, with a line of the host's own"
reset
ready 7
mkdir -p "$T/home/tehas/state"
printf 'not json\n' >"$T/home/tehas/state/push-fault.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the sandbox cannot push: the sandbox cannot push to origin"

t_done
