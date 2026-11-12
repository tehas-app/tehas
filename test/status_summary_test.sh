#!/usr/bin/env bash
# Tests for what tehas status prints of the health file tehasbot status wrote:
# the lines of the summary, the object of --json, the holds on a factory and
# tehas:all-hands on the control issue.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/status_lib.sh
. "$(dirname "$0")/status_lib.sh" || exit 1
t_case "summary: a health file older than ten minutes is not believed, exit 1"
reset
t_run "$status"
jq --arg t "$(date -u -d '-3 hours' +%FT%TZ)" '.time = $t' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   stale: the dispatcher last reported 3 h ago"
t_contains "$t_out" "It said pass: Idle, taking work."
t_contains "$t_out" "queue     2 in line"

t_case "summary: nine minutes old is still believed"
jq --arg t "$(date -u -d '-9 minutes' +%FT%TZ)" '.time = $t' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Idle, taking work. (9 min ago)"

t_case "summary: a time that cannot be read counts as old"
jq '.time = "yesterday"' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   stale: the dispatcher last reported at an unknown time"

t_case "summary: no snapshot yet is a failure with the reason"
reset
t_run "$health; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: No queue snapshot yet"
t_contains "$t_out" "queue     no snapshot yet"

t_case "summary: no health file is exit 1 with one line, and nothing is written"
reset
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas status: no health file at $live/health.json yet"
t_eq "$([ -e "$live" ] && echo there)" ""

t_case "summary: a health file that is not JSON is the same"
reset
mkdir -p "$live"
echo "not json" >"$live/health.json"
t_run "$summary"
t_eq "$t_rc" 1
t_contains "$t_out" "no health file"

t_case "summary reads nothing but the health file: no settings, no gh, no write"
reset
t_run "$status"
rm -f "$T"/calls/*
t_instance 'echo "settings were read" >&2; exit 9'
before=$(cd "$live" && cksum ./*)
t_run "$summary"
t_eq "$t_rc" 0
t_lacks "$t_out" "settings were read"
t_eq "$(t_calls gh)" ""
t_eq "$(cd "$live" && cksum ./*)" "$before"

t_case "summary takes no argument but --json"
reset
t_run "$summary --jsn"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas status [--json]"
t_run "$summary --json --json"
t_eq "$t_rc" 64

# What the dispatcher's last poll wrote: the demand, and what held cycles back.
demand() { # <waiting> [<seconds since the oldest was accepted> [<age of the file>]]
  mkdir -p "$live"
  jq -n --argjson w "$1" --arg at "$(date -u -d "-${3:-30} seconds" +%FT%TZ)" \
    --arg oldest "$(date -u -d "-${2:-0} seconds" +%FT%TZ)" \
    '{time: $at, waiting: $w} + (if $w > 0 then {oldest: $oldest} else {} end)' >"$live/demand.json"
}
# The types status --json promises, for every object it prints.
shape='(.api == 1) and (.time | type) == "string" and (.age | type) == "number" and (.stale | type) == "boolean"
  and (.state | IN("pass", "warn", "fail")) and (.reasons | type) == "array" and (.reasons | all(type == "string"))
  and (.paused | (.local | type) == "boolean" and (.label | type) == "boolean")
  and (.allHands | type) == "boolean" and (.limit.state | type) == "string"
  and (.running | type) == "array"
  and (.running | all((.issue | type) == "number" and (.since | type) == "string"))
  and (.planning | type) == "array"
  and (.planning | all((.issue | type) == "number" and (.turn | type) == "number"))
  and (.demand | . == null or ((.waiting | type) == "number"))
  and (.hold | . == null or ((.why | type) == "string" and (.text | type) == "string"))
  and (.room | . == null or type == "object")'

t_case "status --json: a fresh idle factory, one object of the promised shape"
reset
demand 0
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -s length <<<"$t_out")" 1
t_eq "$(jq "$shape" <<<"$t_out")" true
t_eq "$(jq -c '{state, stale, reasons, paused, allHands, limit, running, demand, hold}' <<<"$t_out")" \
  '{"state":"pass","stale":false,"reasons":[],"paused":{"local":false,"label":false},"allHands":false,"limit":{"state":"none"},"running":[],"demand":{"waiting":0},"hold":null}'
t_eq "$(jq -r '.age < 60' <<<"$t_out")" true

t_case "status --json: a read-only instance says so, the snapshot lists what its starts left, and no other instance has the key"
reset
demand 0
t_run "$status && $summary --json"
t_eq "$(jq -c '.readOnly' <<<"$t_out")" null
t_eq "$(jq -c '.readOnly' "$live/status.json")" null
reset
demand 0
t_instance "${T_MINIMAL[@]}" TEHAS_READ_ONLY=1
# The workspace of #7, beside TEHAS_WORK_DIR, the default <home>/work/widget.
mkdir -p "$T/home/tehas/state/read-only" "$T/home/tehas/work/widget-7/.git"
printf '{"issue":7,"how":"cycle","at":"2026-11-06T10:00:00Z"}\n' >"$T/home/tehas/state/read-only/7.json"
printf '# Fix the thing\n\nBranch: feature/issue-7\n\nThe body.\n' >"$T/home/tehas/work/widget-7/.git/tehas-read-only-pr.md"
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq "$shape" <<<"$t_out")" true
t_eq "$(jq -c '.readOnly' <<<"$t_out")" true
t_eq "$(jq -cS '.readOnly.starts[0] | {issue, how, pr}' "$live/status.json")" \
  '{"how":"cycle","issue":7,"pr":{"body":"The body.","branch":"feature/issue-7","title":"Fix the thing"}}'
t_eq "$(jq -c '.readOnly.on' "$live/status.json")" true
rm -rf "$T/home/tehas/work"

t_case "status --json: the demand, with how long the oldest has waited"
reset
demand 2 5400
t_run "$status && $summary --json"
t_eq "$(jq -r .demand.waiting <<<"$t_out")" 2
t_eq "$(jq -r '.demand.oldestWait | . >= 5400 and . < 5460' <<<"$t_out")" true

t_case "status: a hold on refreshes is in the demand and in the text"
reset
demand 0
jq '. + {refreshHold: "the pi-rukas in the sandbox image does not take --continue-pr"}' "$live/demand.json" >"$live/demand.new" &&
  mv "$live/demand.new" "$live/demand.json"
t_run "$status && $summary --json"
t_eq "$(jq -r .demand.refreshHold <<<"$t_out")" "the pi-rukas in the sandbox image does not take --continue-pr"
t_eq "$(jq "$shape" <<<"$t_out")" true
t_run "$status && $summary"
t_contains "$t_out" "refresh   held: the pi-rukas in the sandbox image does not take --continue-pr"

t_case "status: issues tehas:blocked holds by the label alone are in the demand and in the text"
reset
demand 0
jq '. + {labelAlone: [7, 9]}' "$live/demand.json" >"$live/demand.new" &&
  mv "$live/demand.new" "$live/demand.json"
t_run "$status && $summary --json"
t_eq "$(jq -c .demand.labelAlone <<<"$t_out")" "[7,9]"
t_eq "$(jq "$shape" <<<"$t_out")" true
t_run "$status && $summary"
t_contains "$t_out" "blocked   by the label alone, no blockers named: #7, #9."

t_case "status: no hold on refreshes, no line and no field"
reset
demand 0
t_run "$status && $summary"
t_lacks "$t_out" "refresh   "
t_run "$summary --json"
t_eq "$(jq -c '.demand | has("refreshHold")' <<<"$t_out")" false
t_eq "$(jq -c '.demand | has("labelAlone")' <<<"$t_out")" false

t_case "status --json: no demand from the last ten minutes is null"
reset
demand 2 5400 700
t_run "$status && $summary --json"
t_eq "$(jq -c .demand <<<"$t_out")" null
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status --json: a busy factory lists its running cycles"
reset
cycle true running develop
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.running | map({issue, step})' <<<"$t_out")" '[{"issue":12,"step":"develop"}]'
t_eq "$(jq -r '.running[0].since | fromdateiso8601 | type' <<<"$t_out")" number
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status --json: a held factory says why, and the room under each cap"
reset
held count "holding: 1 cycles are running (limit 1)" 0 3
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"count","text":"holding: 1 cycles are running (limit 1)"}'
t_eq "$(jq -c .room <<<"$t_out")" '{"cycles":0,"maxAwaiting":3,"slice":3}'
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status: an unknown gate is the hold, in the text and in --json"
# A gate whose runs finished without saying they passed holds the same way as
# a red branch, under a word of its own: the one tells a person to fix the
# branch, the other to name the right check.
reset
demand 0
held unknown-gate 'holding: the check "build / verify (linux)" on main at 0123456789ab finished without saying it passed: cycles start' 1 3
t_run "$status && $summary"
t_contains "$t_out" 'slice     holding: the check "build / verify (linux)" on main at 0123456789ab'
t_run "$status && $summary --json"
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"unknown-gate","text":"holding: the check \"build / verify (linux)\" on main at 0123456789ab finished without saying it passed: cycles start"}'

t_case "status: a red branch is the hold, in the text and in --json"
reset
held red "holding: main is red at 0123456789ab" 1 3
t_run "$status && $summary"
t_contains "$t_out" "slice     holding: main is red at 0123456789ab ("
t_run "$status && $summary --json"
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"red","text":"holding: main is red at 0123456789ab"}'

t_case "status --json: room the last poll did not read is left out"
reset
held "" "" 1 null
t_run "$status && $summary --json"
t_eq "$(jq -c .hold <<<"$t_out")" null
t_eq "$(jq -c .room <<<"$t_out")" '{"cycles":1,"slice":3}'

t_case "status --json: a paused factory, by the flag or by the label, is held by the pause"
reset
mkdir -p "$T/home/tehas/state"
jq -nc '{by: "boss", at: "2026-10-15T10:00:00Z", reason: "waiting for the gate"}' >"$T/home/tehas/state/pause.json"
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.state, .paused, .hold.why]' <<<"$t_out")" '["warn",{"local":true,"label":false},"pause"]'
t_contains "$(jq -r .hold.text <<<"$t_out")" "waiting for the gate"
t_lacks "$t_out" "boss"
t_eq "$(jq -r '.reasons | length' <<<"$t_out")" 1
t_eq "$(jq "$shape" <<<"$t_out")" true
reset
echo true >"$T/paused"
t_run "$status && $summary --json"
t_eq "$(jq -c '[.paused, .hold.why]' <<<"$t_out")" '[{"local":false,"label":true},"pause"]'

t_case "status --json: a failed cycle is exit 1, as the text is"
reset
cycle false aborted develop
t_run "$status && $summary --json"
t_eq "$t_rc" 1
t_eq "$(jq -r .state <<<"$t_out")" fail
t_contains "$(jq -r '.reasons[0]' <<<"$t_out")" "#12 ended as aborted"

t_case "status --json: a stale health file is exit 1 and says so"
reset
t_run "$status"
jq --arg t "$(date -u -d '-3 hours' +%FT%TZ)" '.time = $t' "$live/health.json" >"$live/health.json.old"
mv "$live/health.json.old" "$live/health.json"
t_run "$summary --json"
t_eq "$t_rc" 1
t_eq "$(jq -c '[.stale, .age >= 10800]' <<<"$t_out")" '[true,true]'
t_eq "$(jq "$shape" <<<"$t_out")" true

t_case "status --json: no health file is exit 1 with nothing on standard output"
reset
t_run "$summary --json 2>/dev/null"
t_eq "$t_rc" 1
t_eq "$t_out" ""

t_case "status --json reads nothing but the health file: no settings, no gh, no write"
reset
demand 1 60
t_run "$status"
rm -f "$T"/calls/*
t_instance 'echo "settings were read" >&2; exit 9'
before=$(cd "$live" && cksum ./*)
t_run "$summary --json"
t_eq "$t_rc" 0
t_lacks "$t_out" "settings were read"
t_eq "$(t_calls gh)" ""
t_eq "$(cd "$live" && cksum ./*)" "$before"

# tehas:all-hands on the control issue: reported, and nothing else.
all_hands() { # <login or ""> [labeled|unlabeled]: the label's last event, and the label on the issue
  [ "${2:-labeled}" = unlabeled ] || : >"$T/allhands"
  jq -nc --arg w "$1" --arg e "${2:-labeled}" '[{event: $e, label: {name: "tehas:all-hands"},
    actor: (if $w == "" then null else {login: $w} end), created_at: "2026-10-17T10:00:00Z"}]' >"$T/events.json"
}

t_case "all-hands: applied by an allowed account, it is reported and changes nothing else"
reset
all_hands alice
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .allHands "$live/status.json")" true
t_eq "$(jq -r .checks.allHands "$live/health.json")" true
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "all-hands tehas:all-hands is on the control issue"
t_run "$summary --json"
t_eq "$(jq -c '[.allHands, .state, .hold]' <<<"$t_out")" '[true,"pass",null]'

t_case "all-hands: applied by anyone else it is ignored, and said once"
reset
all_hands mallory
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" false
t_contains "$t_out" "tehas:all-hands on #1 was applied by 'mallory', not an allowed account: ignored"
t_run "$status"
t_eq "$t_out" ""
t_run "$summary"
t_lacks "$t_out" "all-hands"

t_case "all-hands: not on the issue is off, whatever its last event says"
reset
all_hands alice unlabeled
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" false
reset
all_hands alice
rm "$T/allhands"
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" false

t_case "all-hands: on the issue with events that cannot be read is unknown, and left out of --json"
reset
all_hands alice
rm "$T/events.json"
t_run "$status"
t_contains "$t_out" "WARN status: gh api repos/acme/widget/issues/1/events?per_page=100 failed"
t_run "$summary --json"
t_eq "$(jq -r .allHands "$live/status.json")" null
t_eq "$(jq -c 'has("allHands")' <<<"$t_out")" false
reset
: >"$T/control-down"
t_run "$status"
t_eq "$(jq -r .allHands "$live/status.json")" null

t_case "all-hands: a snapshot older than ten minutes does not say it"
reset
all_hands alice
t_run "$status"
jq --arg t "$(date -u -d '-20 minutes' +%FT%TZ)" '.generatedAt = $t' "$live/status.json" >"$live/status.json.old"
mv "$live/status.json.old" "$live/status.json"
t_run "$health"
t_eq "$(jq -r .checks.allHands "$live/health.json")" null

t_case "all-hands: a paused factory still reports it"
reset
all_hands alice
echo true >"$T/paused"
t_run "$status && $summary --json"
t_eq "$(jq -c '[.allHands, .paused.label]' <<<"$t_out")" '[true,true]'


t_done
