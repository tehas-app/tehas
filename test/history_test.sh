#!/usr/bin/env bash
# Tests for tehas history: the cycles and planning turns that ended, in the
# order they were written, after a cursor, for one issue, with the park fields.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

history="$T_ROOT/bin/tehas history"
t_console || t_bad "no console: $t_console_why"
home=$T/home/tehas
cycles=$home/state/cycles
t_stub gh
t_stub git

# The cursors below name these seconds, so a case says what it means.
T0=1760700000
iso() { date -u -d "@$1" +%FT%TZ; }

reset() {
  rm -rf "$T"/calls/* "$T/instance" "$home"
  T_ENV=(TEHAS_CONSOLE="$T_CONSOLE")
  t_instance "${T_MINIMAL[@]}"
  t_store_home "$home"
}
# work <issue> <epoch of the end> [fields]: one end of a cycle on the state
# store, as tehasbot cycle records it (t_history_end).
work() { t_history_end "$home" "$@"; }
# plan <issue> <epoch of the end> [fields]: the end of a first planning turn
# on the state store, as the dispatcher records it (t_plan_end).
plan() { t_plan_end "$home" "$@"; }
# run_json [args]: tehas history --json with the arguments given. Each argument
# is one word of the command line, so a cursor reaches the command whole.
run_json() {
  local cmd="$history --json" a
  for a in "$@"; do
    # shellcheck disable=SC2086  # a caller passes the flag and its value as one argument
    cmd="$cmd $a"
  done
  t_run "$cmd"
}
# cursors: the cursor of each cycle listed, in the order it was listed.
cursors() { jq -r '[.cycles[].cursor] | @tsv' <<<"$1"; }

t_case "one cycle: the object, its fields and their types"
reset
work 12 "$T0" '{"kind":"work","started":1760696400,"rc":10,"status":"handoff","step":"review","cap":"awaiting-human-merge","park_reason":"merge","failure_kind":"review","failure_cause":"findings-open","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label"}'
run_json
t_eq "$t_rc" 0
t_eq "$(jq -c 'del(.cycles)' <<<"$t_out")" '{"api":1,"next":"1760700000-12-1"}'
t_eq "$(jq -c '.cycles[0]' <<<"$t_out")" "{\"cursor\":\"1760700000-12-1\",\"kind\":\"work\",\"issue\":12,\"ended\":\"$(iso "$T0")\",\"started\":\"$(iso 1760696400)\",\"status\":10,\"outcome\":\"handoff\",\"step\":\"review\",\"cap\":\"awaiting-human-merge\",\"parkReason\":\"merge\",\"failureKind\":\"review\",\"failureCause\":\"findings-open\",\"provider\":false,\"parked\":true,\"pr\":\"https://github.com/acme/widget/pull/9\",\"by\":\"alice\",\"how\":\"label\"}"
t_eq "$(jq -r '.cycles[0] | (.status|type), (.parked|type), (.provider|type), (.issue|type), (.turn|type)' <<<"$t_out")" "number
boolean
boolean
number
null"
t_eq "$(jq -r '.api' <<<"$t_out")" 1
t_eq "$(t_calls gh)$(t_calls git)" ""

t_case "the object is printed byte for byte, keys in their order"
reset
work 12 "$T0" '{"kind":"work","started":1760696400,"rc":10,"status":"handoff","step":"review","cap":"awaiting-human-merge","park_reason":"merge","failure_kind":"review","failure_cause":"findings-open","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label"}'
plan 7 "$((T0 + 300))" '{"kind":"plan","started":1760700000,"rc":0,"status":"finished","turn":1,"by":"alice","label":"tehas:refine","filed":["https://github.com/acme/widget/issues/41"],"auto":true}'
run_json
t_eq "$t_rc" 0
t_eq "$t_out" "{\"api\":1,\"cycles\":[{\"cursor\":\"1760700000-12-1\",\"kind\":\"work\",\"issue\":12,\"ended\":\"$(iso "$T0")\",\"started\":\"$(iso 1760696400)\",\"status\":10,\"outcome\":\"handoff\",\"step\":\"review\",\"cap\":\"awaiting-human-merge\",\"parkReason\":\"merge\",\"failureKind\":\"review\",\"failureCause\":\"findings-open\",\"provider\":false,\"parked\":true,\"pr\":\"https://github.com/acme/widget/pull/9\",\"by\":\"alice\",\"how\":\"label\"},{\"cursor\":\"1760700300-7-2\",\"kind\":\"plan\",\"issue\":7,\"ended\":\"$(iso "$((T0 + 300))")\",\"started\":\"$(iso 1760700000)\",\"status\":0,\"outcome\":\"finished\",\"turn\":1,\"by\":\"alice\",\"label\":\"tehas:refine\",\"auto\":true,\"filed\":[\"https://github.com/acme/widget/issues/41\"]}],\"next\":\"1760700300-7-2\"}"

t_case "a field the line does not have is left out, never null"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json
t_eq "$(jq -c '.cycles[0]' <<<"$t_out")" "{\"cursor\":\"1760700000-12-1\",\"kind\":\"work\",\"issue\":12,\"ended\":\"$(iso "$T0")\",\"started\":\"$(iso $((T0 - 60)))\",\"status\":0,\"outcome\":\"merged\",\"provider\":false,\"parked\":false,\"how\":\"hand\"}"
t_eq "$(jq -r '.cycles[0] | has("step"), has("cap"), has("parkReason"), has("pr"), has("by")' <<<"$t_out")" "false
false
false
false
false"

t_case "oldest first, across issues, with the cursor of each"
reset
work 9 "$((T0 - 10))" '{"kind":"work","rc":1,"status":"failed"}'
work 7 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 12 "$((T0 + 600))" '{"kind":"work","rc":0,"status":"merged"}'
run_json
t_eq "$(cursors "$t_out")" "1760699990-9-1	1760700000-7-2	1760700600-12-3"
t_eq "$(jq -r .next <<<"$t_out")" 1760700600-12-3

t_case "lines that ended in the same second are listed in the order they were written, and none is repeated or skipped"
reset
work 3 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 11 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 4 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json
t_eq "$(cursors "$t_out")" "1760700000-3-1	1760700000-11-2	1760700000-4-3"
t_eq "$(jq -r '[.cycles[].cursor] | unique | length' <<<"$t_out")" 3
# A program keeps "next" and asks again: what it has read is never offered
# again, and nothing that ended later is missed.
mid=$(jq -r '.cycles[1].cursor' <<<"$t_out")
last=$(jq -r .next <<<"$t_out")
run_json "--after $mid"
t_eq "$(cursors "$t_out")" "1760700000-4-3"
run_json "--after $last"
t_eq "$(jq -c '{cycles, next}' <<<"$t_out")" "{\"cycles\":[],\"next\":\"$last\"}"

t_case "--after with a cursor before everything lists all of them once"
reset
for n in 1 2 3; do
  work "$n" "$((T0 + n))" '{"kind":"work","rc":0,"status":"merged"}'
done
run_json "--after $((T0 - 10))-1"
t_eq "$(jq -r '[.cycles[].issue] | @tsv' <<<"$t_out")" "1	2	3"
t_eq "$(jq -r .next <<<"$t_out")" "1760700003-3-3"

t_case "an old cursor, without the place, is read as before: on the seconds, then the issue"
reset
work 3 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 4 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json "--after 1760700000-3"
t_eq "$(cursors "$t_out")" "1760700000-4-2"
t_eq "$(jq -r .next <<<"$t_out")" "1760700000-4-2"

# The old cursor named the seconds and the issue only: a program that read #12
# kept 1760700000-12, and #7, written a moment later for the same second, was
# never listed to it.
t_case "a line written after a read, for the same second and a lower issue, is listed by the next read"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json
next=$(jq -r .next <<<"$t_out")
work 7 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json "--after $next"
t_eq "$(cursors "$t_out")" "1760700000-7-2"

t_case "a cycle and a planning turn of one issue in one second each have a cursor of their own"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
plan 12 "$T0" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
run_json
t_eq "$(cursors "$t_out")" "1760700000-12-1	1760700000-12-2"
run_json "--after 1760700000-12-1"
t_eq "$(jq -r '[.cycles[].kind] | @tsv' <<<"$t_out")" plan

t_case "--issue lists one issue's lines, from its cycle and its planning turns"
reset
plan 12 "$((T0 - 60))" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 121 "$T0" '{"kind":"work","rc":1,"status":"failed"}'
run_json "--issue 12"
t_eq "$(jq -r '[.cycles[] | "\(.kind):\(.issue)"] | @tsv' <<<"$t_out")" "plan:12	work:12"
t_eq "$(jq -r '[.cycles[].issue] | unique | @tsv' <<<"$t_out")" 12
run_json "--issue 121"
t_eq "$(jq -r '.cycles[0].kind' <<<"$t_out")" work
run_json "--issue 0"
t_eq "$t_rc" 64

t_case "a planning turn is listed the same way, with kind plan"
reset
plan 7 "$((T0 + 300))" '{"kind":"plan","started":1760700000,"rc":0,"status":"finished","turn":1,"by":"alice"}'
run_json
t_eq "$(jq -c '.cycles[0]' <<<"$t_out")" "{\"cursor\":\"1760700300-7-1\",\"kind\":\"plan\",\"issue\":7,\"ended\":\"$(iso "$((T0 + 300))")\",\"started\":\"$(iso 1760700000)\",\"status\":0,\"outcome\":\"finished\",\"turn\":1,\"by\":\"alice\",\"label\":\"tehas:plan\"}"
t_eq "$(jq -r .next <<<"$t_out")" 1760700300-7-1

t_case "a refining turn names its label and the issues it filed, as JSON and as text"
reset
plan 7 "$((T0 + 300))" '{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice","label":"tehas:refine","filed":["https://github.com/acme/widget/issues/41","not an issue"]}'
run_json
t_eq "$(jq -c '.cycles[0] | [.label, .filed]' <<<"$t_out")" '["tehas:refine",["https://github.com/acme/widget/issues/41"]]'
t_run "$history"
t_contains "$t_out" "plan  #7  finished  turn 1  refine  filed https://github.com/acme/widget/issues/41"

t_case "a turn of a refine the poll started on its own says auto, and a labelled one leaves it out"
reset
plan 7 "$((T0 + 300))" '{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice","label":"tehas:refine","auto":true}'
plan 8 "$((T0 + 400))" '{"kind":"plan","rc":0,"status":"finished","turn":1,"by":"alice","label":"tehas:refine"}'
run_json
t_eq "$(jq -c '[.cycles[] | [.issue, .auto]]' <<<"$t_out")" '[[7,true],[8,null]]'
t_run "$history"
t_contains "$t_out" "plan  #7  finished  turn 1  refine (auto)"
t_lacks "$t_out" "#8  finished  turn 1  refine (auto)"

t_case "both kinds together, and a turn whose unit left no readable status"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
plan 12 "$T0" '{"kind":"plan","status":"not-started","turn":1,"by":"alice"}'
run_json
t_eq "$(jq -r '[.cycles[] | "\(.kind) \(.outcome) \(.status // "none")"] | @tsv' <<<"$t_out")" "work merged 0	plan not-started none"

t_case "the files the planning history was kept in before the store are not read"
reset
mkdir -p "$cycles" "$home/state"
jq -cn --arg at "$(iso "$T0")" '{kind: "plan", at: $at, rc: 0, status: "finished"}' >"$T/planted.jsonl"
cp "$T/planted.jsonl" "$cycles/plan-9.history.jsonl"
cp "$T/planted.jsonl" "$home/state/plan-6.history.jsonl"
run_json
t_eq "$(jq -c '.cycles' <<<"$t_out")" "[]"
t_eq "$(jq -r .next <<<"$t_out")" null

t_case "a park of a run of parks shows its count, as JSON and as text"
reset
parked='{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true}'
# Two parks of #12 that stopped exactly alike: the second is number 1 of the run,
# because the end before it is the park that started it. #11 stopped the same way
# but on an issue of its own, which no run of #12 counts.
work 12 "$T0" "$parked"
work 12 "$((T0 + 60))" "$parked"
work 11 "$((T0 + 120))" "$parked"
run_json
t_eq "$(jq -c '[.cycles[] | [.issue, .parked, .repeat]]' <<<"$t_out")" '[[12,true,null],[12,true,1],[11,true,null]]'
t_run "$history"
t_contains "$t_out" "  work  #12  failed  rc 10  step develop  parked  repeat 1"
t_lacks "$t_out" "#11  failed  rc 10  step develop  parked  repeat"
t_lacks "$t_out" "1760700000-12-1  work  #12  failed  rc 10  step develop  parked  repeat"

t_case "what breaks a run of parks, and what does not"
reset
parked='{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true}'
# A merge between two parks alike breaks the run: the park after it is a first
# park, whatever came before the merge.
work 12 "$T0" "$parked"
work 12 "$((T0 + 60))" '{"kind":"work","rc":0,"status":"merged"}'
work 12 "$((T0 + 120))" "$parked"
run_json
t_eq "$(jq -c '[.cycles[] | .repeat]' <<<"$t_out")" '[null,null,null]'
# A park of a provider outage is neither a repeat nor the end a run is counted
# from: it has its own rule of retries. A park that is not one is not either.
reset
work 12 "$T0" "$parked"
work 12 "$((T0 + 60))" '{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true,"provider":true}'
work 12 "$((T0 + 120))" "$parked"
run_json
t_eq "$(jq -c '[.cycles[] | .repeat]' <<<"$t_out")" '[null,null,1]'
reset
work 12 "$T0" '{"kind":"work","rc":10,"status":"handoff"}'
work 12 "$((T0 + 60))" "$parked"
run_json
t_eq "$(jq -c '[.cycles[] | .repeat]' <<<"$t_out")" '[null,null]'

t_case "a park of another issue is no part of the run"
reset
parked='{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true}'
work 12 "$T0" "$parked"
work 11 "$((T0 + 60))" "$parked"
work 12 "$((T0 + 120))" "$parked"
run_json
t_eq "$(jq -c '[.cycles[] | [.issue, .repeat]]' <<<"$t_out")" '[[12,null],[11,null],[12,1]]'

t_case "a line of before the count was kept says nothing of a repeat"
reset
work 12 "$T0" '{"kind":"work","rc":10,"status":"failed","step":"develop","parked":true}'
run_json
t_eq "$(jq -r '.cycles[0] | has("repeat")' <<<"$t_out")" false
t_run "$history"
t_eq "$t_out" "1760700000-12-1  work  #12  failed  rc 10  step develop  parked"

t_case "the count counts the parks the listing holds, and none besides"
reset
# The run begins with a park the listing does not hold: the two it holds are 0
# and 1, the first of them a park the reader cannot know repeats anything.
parked='{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true}'
work 12 "$T0" "$parked"
work 12 "$((T0 + 60))" "$parked"
work 12 "$((T0 + 120))" "$parked"
run_json --after 1760700000-12-1
t_eq "$(jq -c '[.cycles[] | .repeat]' <<<"$t_out")" '[null,1]'

t_case "a repeat is a whole number of a cycle line only"
reset
work 12 "$T0" '{"kind":"work","rc":10,"status":"failed","parked":true,"repeat":false}'
work 11 "$((T0 + 60))" '{"kind":"plan","rc":0,"status":"finished","turn":1,"repeat":2}'
run_json
t_eq "$(jq -r '[.cycles[] | has("repeat")] | @tsv' <<<"$t_out")" "false	false"
t_case "nothing to list: an empty list and no cursor"
reset
run_json
t_eq "$t_rc" 0
t_eq "$t_out" '{"api":1,"cycles":[],"next":null}'

t_case "--json prints one object and nothing else"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json "2>/dev/null"
t_eq "$(jq -s length <<<"$t_out")" 1

t_case "without --json, one line each, oldest first, and the plan's turn"
reset
plan 12 "$((T0 - 60))" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
work 12 "$T0" '{"kind":"work","rc":10,"status":"handoff","step":"review","parked":true,"pr":"https://github.com/acme/widget/pull/9"}'
t_run "$history"
t_eq "$t_rc" 0
t_eq "$t_out" "1760699940-12-1  plan  #12  finished  turn 1
1760700000-12-2  work  #12  handoff  rc 10  step review  parked  pr https://github.com/acme/widget/pull/9"
t_case "a factory with no history of its own says nothing and exits 0"
reset
t_run "$history"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "an argument it does not know is exit 64 and reads nothing"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
for arg in "--json --json" "--after" "--after 12" "--after 1760700000-0" "--after 1760700000-" "--issue" "--issue 7 --issue 8" "--jsn" "--after x-1" "--after -1-1" "7"; do
  t_run "$history $arg"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehas history"
done

t_case "a word that is not text is a bad argument too, and --help among others is one"
reset
t_run "$history --after \$'\\xff'"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas history"
t_run "$history --json --help"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas history"

t_case "its text is the same for help and for --help alone"
reset
t_run "$T_ROOT/bin/tehas help history"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage: tehas history [--json] [--after CURSOR] [--issue N]"
text=$t_out
t_run "$history --help"
t_eq "$t_rc" 0
t_eq "$t_out" "$text"

# A cursor is what this command printed. One wider than anything it can print
# belongs to no listing, and an empty listing would be read as "nothing has
# ended" by a program that keeps it and asks again.
t_case "a cursor this command could never print is a bad argument"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
for cursor in "9999999999999999-1" "123456789012345-1" "1760700000-1234567890" "-1" "1760700000-1-0" "1760700000-1-1234567890123456" "1760700000-1-" "1760700000-1-1-1" "17607000000000-1"; do
  run_json "--after $cursor"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehas history"
done
# The widest cursor it can print is taken: 10 digits of seconds, 9 of issue and
# 15 of place.
run_json "--after 9999999999-999999999-999999999999999"
t_eq "$t_rc" 0
t_eq "$(jq -c '{cycles, next}' <<<"$t_out")" '{"cycles":[],"next":"9999999999-999999999-999999999999999"}'

t_case "a store that cannot answer prints nothing: 1 when it is busy, 78 when it cannot be used or is not there"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
plan 12 "$T0" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
t_store_answers history-show 1 'store: busy #0: the database is locked'
run_json "2>/dev/null"
t_eq "$t_rc" 1
t_eq "$t_out" ""
t_store_answers history-show 78 'store: corrupt: the file is not a database'
run_json "2>/dev/null"
t_eq "$t_rc" 78
t_eq "$t_out" ""
rm -f "$home/libexec/store"
t_run "$history"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"
t_lacks "$t_out" "plan"

t_case "without settings it is exit 78 and prints no object"
rm -rf "$T/instance"
run_json "2>/dev/null"
t_eq "$t_rc" 78
t_eq "$t_out" ""

t_case "a bad argument is 64 whatever the settings hold"
rm -rf "$T/instance"
run_json "--issue 0 2>/dev/null"
t_eq "$t_rc" 64

t_case "without a repository to name its pull requests it is exit 78 and reads nothing"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged","pr":"https://github.com/acme/widget/pull/9"}'
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_store_answers history-show 0 '{"api":1,"ok":true,"verb":"history-show","rows":[]}'
t_run "$history --json"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls store)" ""

t_case "without a console there is nothing to run: exit 78"
reset
T_ENV=(TEHAS_CONSOLE="$T/no-console")
t_run "$history --json"
t_eq "$t_rc" 78
t_contains "$t_out" "no console at $T/no-console"

t_case "history writes nothing"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
plan 12 "$T0" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
# The store's own files of its log (-wal, -shm) are SQLite's, not a write.
files() { (cd "$home" && find . ! -name 'tehas.db-wal' ! -name 'tehas.db-shm' | LC_ALL=C sort); }
before=$(files)
sum=$(t_row history-show -- '.rows | tostring')
run_json
t_eq "$(jq -r '.cycles | length' <<<"$t_out")" 2
t_run "$history"
t_eq "$(files)" "$before"
t_eq "$(t_row history-show -- '.rows | tostring')" "$sum"

t_case "reading takes no lock a running cycle holds"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
: >"$cycles/12.lock"
t_run "flock '$cycles/12.lock' $history --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.cycles | length' <<<"$t_out")" 1

t_done
