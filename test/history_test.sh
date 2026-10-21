#!/usr/bin/env bash
# Tests for libexec/history: the cycles and planning turns that ended, in the
# order they ended, after a cursor, for one issue, with the park fields.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

history="$T_ROOT/libexec/history"
home=$T/home/tehas
cycles=$home/state/cycles
t_stub gh
t_stub git

# The cursors below name these seconds, so a case says what it means.
T0=1760700000
iso() { date -u -d "@$1" +%FT%TZ; }

reset() {
  rm -rf "$T"/calls/* "$T/instance" "$home"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
# work <issue> <epoch of the end> [fields]: one line of a cycle's history, as
# tehasbot cycle writes it - the issue is the file's name, never the line's.
work() {
  local n=$1 e=$2 extra=${3-}
  [ -n "$extra" ] || extra='{}'
  mkdir -p "$cycles"
  jq -cn --arg at "$(iso "$e")" --argjson x "$extra" '$x + {at: $at}' >"$cycles/$n.history.jsonl"
}
# plan <issue> <epoch of the end> [fields]: one line of a planning turn's
# history, as lib/planning.sh writes it.
plan() {
  local n=$1 e=$2 extra=${3-}
  [ -n "$extra" ] || extra='{}'
  mkdir -p "$home/state"
  jq -cn --arg at "$(iso "$e")" --argjson x "$extra" '$x + {at: $at}' >"$home/state/plan-$n.history.jsonl"
}
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
work 12 "$T0" '{"kind":"work","started":1760696400,"rc":10,"status":"handoff","step":"review","cap":"awaiting-human-merge","park_reason":"merge","failure_kind":"review","failure_cause":"findings open","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label"}'
run_json
t_eq "$t_rc" 0
t_eq "$(jq -c 'del(.cycles)' <<<"$t_out")" '{"api":1,"next":"1760700000-12"}'
t_eq "$(jq -c '.cycles[0]' <<<"$t_out")" "{\"cursor\":\"1760700000-12\",\"kind\":\"work\",\"issue\":12,\"ended\":\"$(iso "$T0")\",\"started\":\"$(iso 1760696400)\",\"status\":10,\"outcome\":\"handoff\",\"step\":\"review\",\"cap\":\"awaiting-human-merge\",\"parkReason\":\"merge\",\"failureKind\":\"review\",\"failureCause\":\"findings open\",\"provider\":false,\"parked\":true,\"pr\":\"https://github.com/acme/widget/pull/9\",\"by\":\"alice\",\"how\":\"label\"}"
t_eq "$(jq -r '.cycles[0] | (.status|type), (.parked|type), (.provider|type), (.issue|type), (.turn|type)' <<<"$t_out")" "number
boolean
boolean
number
null"
t_eq "$(jq -r '.api' <<<"$t_out")" 1
t_eq "$(t_calls gh)$(t_calls git)" ""

t_case "a field the line does not have is left out, never null"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json
t_eq "$(jq -c '.cycles[0]' <<<"$t_out")" "{\"cursor\":\"1760700000-12\",\"kind\":\"work\",\"issue\":12,\"ended\":\"$(iso "$T0")\",\"status\":0,\"outcome\":\"merged\"}"
t_eq "$(jq -r '.cycles[0] | has("started"), has("step"), has("cap"), has("parkReason"), has("provider"), has("parked"), has("pr"), has("by")' <<<"$t_out")" "false
false
false
false
false
false
false
false"

t_case "oldest first, across issues, with the cursor of each"
reset
work 12 "$((T0 + 600))" '{"kind":"work","rc":0,"status":"merged"}'
work 7 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 9 "$((T0 - 10))" '{"kind":"work","rc":1,"status":"failed"}'
run_json
t_eq "$(cursors "$t_out")" "1760699990-9	1760700000-7	1760700600-12"
t_eq "$(jq -r .next <<<"$t_out")" 1760700600-12

t_case "two cycles that ended in the same second have an order of their own, and neither is repeated or skipped"
reset
work 3 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 11 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 4 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json
t_eq "$(cursors "$t_out")" "1760700000-3	1760700000-4	1760700000-11"
t_eq "$(jq -r '[.cycles[].cursor] | unique | length' <<<"$t_out")" 3
# A program keeps "next" and asks again: what it has read is never offered
# again, and nothing that ended later is missed.
mid=$(jq -r '.cycles[1].cursor' <<<"$t_out")
last=$(jq -r .next <<<"$t_out")
run_json "--after $mid"
t_eq "$(cursors "$t_out")" "1760700000-11"
run_json "--after $last"
t_eq "$(jq -c '{cycles, next}' <<<"$t_out")" "{\"cycles\":[],\"next\":\"$last\"}"

t_case "--after with a cursor before everything lists all of them once"
reset
for n in 1 2 3; do
  work "$n" "$((T0 + n))" '{"kind":"work","rc":0,"status":"merged"}'
done
run_json "--after $((T0 - 10))-1"
t_eq "$(jq -r '[.cycles[].issue] | @tsv' <<<"$t_out")" "1	2	3"
t_eq "$(jq -r .next <<<"$t_out")" "1760700003-3"

t_case "the cursor of two lines in one second separates them, so a resume takes the later one only"
reset
work 3 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 4 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
run_json "--after 1760700000-3"
t_eq "$(cursors "$t_out")" "1760700000-4"

t_case "--issue lists one issue's lines, from its cycle and its planning turns"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
work 121 "$T0" '{"kind":"work","rc":1,"status":"failed"}'
plan 12 "$((T0 - 60))" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
run_json "--issue 12"
t_eq "$(jq -r '[.cycles[] | "\(.kind):\(.issue)"] | @tsv' <<<"$t_out")" "plan:12	work:12"
t_eq "$(jq -r '[.cycles[].issue] | unique | @tsv' <<<"$t_out")" 12
run_json "--issue 121"
t_eq "$(jq -r '.cycles[0].kind' <<<"$t_out")" work
run_json "--issue 0"
t_eq "$t_rc" 64

t_case "a planning turn is listed the same way, with kind plan"
reset
plan 7 "$((T0 + 300))" '{"kind":"plan","started":1760700000,"rc":0,"status":"finished","turn":3,"by":"alice"}'
run_json
t_eq "$(jq -c '.cycles[0]' <<<"$t_out")" "{\"cursor\":\"1760700300-7\",\"kind\":\"plan\",\"issue\":7,\"ended\":\"$(iso "$((T0 + 300))")\",\"started\":\"$(iso 1760700000)\",\"status\":0,\"outcome\":\"finished\",\"turn\":3,\"by\":\"alice\"}"
t_eq "$(jq -r .next <<<"$t_out")" 1760700300-7

t_case "a refining turn names its label and the issues it filed, as JSON and as text"
reset
plan 7 "$((T0 + 300))" '{"kind":"plan","rc":0,"status":"finished","turn":2,"by":"alice","label":"tehas:refine","filed":["https://github.com/acme/widget/issues/41","not an issue"]}'
run_json
t_eq "$(jq -c '.cycles[0] | [.label, .filed]' <<<"$t_out")" '["tehas:refine",["https://github.com/acme/widget/issues/41"]]'
t_run "$history"
t_contains "$t_out" "plan  #7  finished  turn 2  refine  filed https://github.com/acme/widget/issues/41"

t_case "both kinds together, and a turn whose unit left no readable status"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
plan 12 "$T0" '{"kind":"plan","rc":"not started","status":"not-started","turn":1,"by":"alice"}'
run_json
t_eq "$(jq -r '[.cycles[] | "\(.kind) \(.outcome) \(.status // "none")"] | @tsv' <<<"$t_out")" "plan not-started none	work merged 0"
# The two ended in one second and the issue is the same, so the pair orders
# them by kind only as far as the cursor goes; both are listed once.
t_eq "$(jq -r '.cycles | length' <<<"$t_out")" 2

t_case "a line this release cannot read is left out, and the rest is listed"
reset
mkdir -p "$cycles"
{
  printf '%s\n' '{"kind":"work","rc":0,"status":"merged"}'
  printf '%s\n' 'not json at all'
  printf '%s\n' '[1,2,3]'
  printf '%s\n' '"a string"'
  printf '%s\n' '{"kind":"plan","rc":0,"status":"finished"}'
  printf '%s\n' '{"rc":0,"status":"merged"}'
  printf '%s\n' '{"kind":"work","at":"not a time","rc":0,"status":"merged"}'
  printf '%s\n' '{"kind":"other","at":"2025-10-17T11:20:00Z","rc":0,"status":"merged"}'
} >"$cycles/5.history.jsonl"
jq -cn --arg at "$(iso "$T0")" '{kind: "work", at: $at, rc: 0, status: "merged"}' >"$cycles/5.history.jsonl"
run_json
t_eq "$(jq -r '[.cycles[] | "\(.kind):\(.cursor)"] | @tsv' <<<"$t_out")" "work:1760700000-5"

t_case "a file that is a link, or a name that is no issue's, is not read"
reset
mkdir -p "$cycles" "$home/state"
jq -cn --arg at "$(iso "$T0")" '{kind: "work", at: $at, rc: 0, status: "merged"}' >"$T/planted.jsonl"
ln -s "$T/planted.jsonl" "$cycles/9.history.jsonl"
jq -cn --arg at "$(iso "$T0")" '{kind: "work", at: $at, rc: 0, status: "merged"}' >"$cycles/x.history.jsonl"
mkdir -p "$cycles/8.history.jsonl"
jq -cn --arg at "$(iso "$T0")" '{kind: "plan", at: $at, rc: 0, status: "finished"}' >"$home/state/plan-.history.jsonl"
run_json
t_eq "$(jq -c '.cycles' <<<"$t_out")" "[]"
t_eq "$(jq -r .next <<<"$t_out")" null

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
work 12 "$T0" '{"kind":"work","rc":10,"status":"handoff","step":"review","parked":true,"pr":"https://github.com/acme/widget/pull/9"}'
plan 12 "$((T0 - 60))" '{"kind":"plan","rc":0,"status":"finished","turn":2}'
t_run "$history"
t_eq "$t_rc" 0
t_eq "$t_out" "1760699940-12  plan  #12  finished  turn 2
1760700000-12  work  #12  handoff  rc 10  step review  parked  pr https://github.com/acme/widget/pull/9"
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

# A cursor is what this command printed. One wider than anything it can print
# belongs to no listing, and an empty listing would be read as "nothing has
# ended" by a program that keeps it and asks again.
t_case "a cursor this command could never print is a bad argument"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
for cursor in "9999999999999999-1" "123456789012345-1" "1760700000-1234567890" "-1" "1760700000-1-1" "17607000000000-1"; do
  run_json "--after $cursor"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehas history"
done
# The widest cursor it can print is taken: 10 digits of seconds and 9 of issue.
run_json "--after 9999999999-999999999"
t_eq "$t_rc" 0
t_eq "$(jq -c '{cycles, next}' <<<"$t_out")" '{"cycles":[],"next":"9999999999-999999999"}'

t_case "without settings it is exit 78 and prints no object"
rm -rf "$T/instance"
run_json "2>/dev/null"
t_eq "$t_rc" 78
t_eq "$t_out" ""

t_case "history writes nothing"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
plan 12 "$T0" '{"kind":"plan","rc":0,"status":"finished","turn":1}'
before=$(cd "$home" && find . | LC_ALL=C sort)
run_json
t_eq "$(jq -r '.cycles | length' <<<"$t_out")" 2
t_run "$history"
t_eq "$(cd "$home" && find . | LC_ALL=C sort)" "$before"

t_case "reading takes no lock a running cycle holds"
reset
work 12 "$T0" '{"kind":"work","rc":0,"status":"merged"}'
: >"$cycles/12.lock"
t_run "flock '$cycles/12.lock' '$history' --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.cycles | length' <<<"$t_out")" 1

t_done
