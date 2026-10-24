#!/usr/bin/env bash
# Tests for libexec/cycle: what it refuses to start, its lock and the notes it
# leaves.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

for bad in "" "abc" "07" "0" "7 8" "7x" "-7"; do
  t_case "'$bad' is not an issue: nothing runs and nothing is written"
  reset
  t_run "$cycle $bad"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehasbot cycle <issue>"
  t_eq "$(t_calls gh)$(t_calls fake-work)" ""
  t_eq "$(find "$state" 2>/dev/null | wc -l)" 0
done

t_case "an invalid instance runs nothing, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$cycle 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

# The factory works on open issues only. What the number is decides it, and it
# is asked before a label is changed, a comment is posted or a container starts.
t_case "a closed issue is asked for and refused: no cycle, no label, no comment"
for answer in '{"number":7,"state":"closed"}' '{"number":7,"state":"CLOSED"}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-7.json"
  t_run "$cycle 7"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #7: the issue is closed"
  t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "issue comment"
  # It still said it came up. The dispatcher waits for this start on record
  # to know its unit ran; a cycle that refuses its number after that is not a
  # start that did not happen. Its record is ended, as a refusal.
  t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, (.cycle.ended_at != null), .cycle.end_reason] | tostring')" '[1,true,null]'
  t_eq "$(t_row history-show 7 -- '.rows | length')" 0
done

t_case "the number of a pull request is refused, open, merged or closed"
for answer in '{"number":9,"state":"open","pull_request":{"url":"u"}}' '{"number":9,"state":"closed","pull_request":{"url":"u"}}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-9.json"
  t_run "$cycle 9"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #9: it is a pull request, not an issue the factory works on"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_eq "$(t_row cycle-show 9 -- '[.cycle.attempt, (.cycle.ended_at != null)] | tostring')" '[1,true]'
done

t_case "an answer that cannot be read is a refusal, not a guess"
for answer in '' 'not json' 'null' '[]' '{"message":"Not Found"}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-7.json"
  t_run "$cycle 7"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #7: GitHub's answer for it could not be read"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
done

# GitHub could not be reached at all: three tries, then a refusal that says the
# ask failed, which is not the same as an answer that is unreadable.
reset
: >"$fx/issue-fails"
t_run "$cycle 7"
t_eq "$t_rc" 65
t_contains "$t_out" "refuse #7: GitHub could not be asked what the number is"
t_lacks "$t_out" "could not be read"
t_eq "$(grep -c '^api repos/acme/widget/issues/7$' <<<"$(t_calls gh)")" 3
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, (.cycle.ended_at != null)] | tostring')" '[1,true]'

# A call that only failed is retried, as every other call here is: the cycle
# the dispatcher already authorized still starts.
reset
echo 2 >"$fx/issue-fails-times"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(grep -c '^api repos/acme/widget/issues/7$' <<<"$(t_calls gh)")" 3
t_eq "$(t_calls fake-work)" "7"
t_lacks "$t_out" "refuse #7"

t_case "an open issue is asked for first, before its cycle starts"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | head -1)" "api repos/acme/widget/issues/7"
t_eq "$(t_calls fake-work)" "7"

# --restart is the answer to a handoff: pi-rukas's attention gate lets it
# through and takes needs-human-attention off. tehas removes no such label.
t_case "a cycle restarts only an issue that carries needs-human-attention"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
rm -f "$T"/calls/*
t_run "$cycle 7"
t_eq "$(t_calls fake-work)" "7"
rm -f "$T"/calls/*
echo '{"number":7,"state":"open","labels":[{"name":"tehas:running"},{"name":"needs-human-attention"}]}' >"$fx/issue-7.json"
t_run "$cycle 7"
t_eq "$(t_calls fake-work)" "7 --restart"
t_lacks "$(t_calls gh | grep -e '--remove-label needs-human-attention')" "needs-human-attention"

t_case "it says that it holds its lock, and lets go of it at the end"
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, (.cycle.ended_at != null)] | tostring')" '[3,true]'
t_eq "$(flock -n "$state/7.lock" echo free)" "free"

# A run leaves its lock and the carriers kept to the next start beside it;
# its record and its line of history are on the store.
t_case "a cycle leaves its lock and the suffixed notes, and its history on the store"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(find "$state" -maxdepth 1 ! -name cycles -printf '%P\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "7.lock 7.park 7.progress.json "
t_eq "$(t_row history-show 7 -- '[.rows[] | [.attempt, .status, .how]] | tostring')" '[[1,"unknown","hand"]]'

t_case "a second cycle for the same issue runs nothing, exit 3"
reset
mkdir -p "$state"
exec 9>"$state/7.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle for #7 is already running"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(t_row cycle-show 7 -- .cycle)" null
exec 9>&-

# --- The start on the state store ---------------------------------------------
t_case "a cycle run by hand is on record as one, uses no approval, and runs work with its attempt"
reset
t_approve "$T/home/tehas" 7 alice label
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_row cycle-show 7 -- '[.cycle.how, .cycle.started_by, .cycle.requeued] | tostring')" '["hand",null,false]'
# The approval stays for the start it was recorded for.
t_eq "$(t_row approval-current 7 -- '[.approval.by, .current] | tostring')" '["alice",null]'
t_eq "$(t_row history-show 7 -- '.rows[0] | [.by, .how] | tostring')" '[null,"hand"]'

t_case "work is given the attempt of the start, and nothing new reaches it"
reset
printf '%s\n' 'printf "%s\n" "${TEHAS_CYCLE_ATTEMPT:-none}" >"$T/calls/attempt"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(cat "$T/calls/attempt")" 1
t_run "$cycle 7"
t_eq "$(cat "$T/calls/attempt")" 2

t_case "from the unit a start uses the approval the dispatcher recorded, and the account is on record"
reset
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_approve "$T/home/tehas" 7 alice label
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
t_eq "$(t_row cycle-show 7 -- '[.cycle.how, .cycle.started_by] | tostring')" '["label","alice"]'
t_eq "$(t_row approval-current 7 -- '[.approval, .current.used_by] | tostring')" '[null,1]'
t_eq "$(t_row history-show 7 -- '.rows[0] | [.by, .how] | tostring')" '["alice","label"]'

t_case "systemd's INVOCATION_ID is a start from the unit too"
reset
T_ENV+=(INVOCATION_ID=0123456789abcdef)
t_run "$cycle 7"
t_eq "$t_rc" 77
t_eq "$(t_calls fake-work)" ""

t_case "from the unit, no approval waiting starts nothing: exit 77 before any label, comment or call"
reset
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_run "$cycle 7"
t_eq "$t_rc" 77
t_contains "$t_out" "refuse #7: the state store refused the start: store: unapproved #7"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(t_row cycle-show 7 -- .cycle)" null
# A used approval is no approval for a second start (A3).
t_approve "$T/home/tehas" 7 alice label
t_run "$cycle 7"
t_eq "$t_rc" 0
rm -f "$T"/calls/*
t_run "$cycle 7"
t_eq "$t_rc" 77
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(t_row cycle-show 7 -- .cycle.attempt)" 1

t_case "a busy store is asked three times, then the cycle exits 75 and changes nothing"
reset
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_approve "$T/home/tehas" 7 alice label
t_store_answers cycle-start 1 'store: busy #7: the database is locked'
t_run "$cycle 7"
t_eq "$t_rc" 75
t_eq "$(grep -c '^cycle-start' <<<"$(t_calls store)")" 3
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(t_row approval-current 7 -- '[.approval.by, .current] | tostring')" '["alice",null]'

t_case "a store that cannot be used, or is not installed, stops the cycle at 78 before GitHub is asked"
reset
t_store_answers cycle-start 78 'store: corrupt: the file is not a database'
t_run "$cycle 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
reset
rm -f "$T/home/tehas/libexec/store"
t_run "$cycle 7"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(find "$state" -name '7.lock' 2>/dev/null | wc -l)" 0

t_case "the account on record is the approval's, whatever is approved after the start"
reset
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_approve "$T/home/tehas" 7 alice label
# While the cycle runs, a poll records bob's approval of a later label.
printf '%s\n' 'TEHAS_HOME=$T/home/tehas "$T/store-bin" approval-record 7 --at 1760000100 --by bob --how label --label tehas:ready --body-sha "$(printf a%.0s {1..64})" >/dev/null' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_row cycle-show 7 -- .cycle.started_by)" alice
t_eq "$(t_row history-show 7 -- '.rows[0].by')" alice
t_eq "$(t_row approval-current 7 -- '[.approval.by, .current.by] | tostring')" '["bob","alice"]'

t_case "a cycle for another issue is not held back"
reset
mkdir -p "$state"
exec 9>"$state/8.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
exec 9>&-

t_done
