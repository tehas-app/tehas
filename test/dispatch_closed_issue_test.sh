#!/usr/bin/env bash
# Issue #2214: a planning turn whose issue was closed is dropped.
# shellcheck disable=SC2016
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# The shape of a planning turn that ended and waits to be posted: a session on
# record whose turn is done and pending its write to GitHub, and an empty queue,
# so the poll of the case has one thing to do - post that turn (or drop it, when
# its issue has been closed). A turn on a running or a closed-out session is not
# scheduled for the post, so the record is set pending here rather than driven
# through a turn that the setup poll would post away first.
turn_pending() { # [<the comment of the last turn, or "">]: the question posted
  reset
  echo '[]' >"$fx/ready.json"
  echo '[]' >"$fx/plans.json"
  echo '[]' >"$fx/asking.json"
  t_plan "$T/home/tehas" 7 '{"session":"abc-123","turns":1,"pending":true,"comment":null}'
  # The outcome the turn handed back, waiting where the post reads it: without
  # it the post of an open issue has nothing to write and holds.
  cp "$fx/plan-result.json" "$record.out"
  echo 0 >"$record.rc"
  rm -f "$T"/calls/*
}

t_case "a turn that ends after its issue was closed posts nothing and its session ends"
turn_pending
echo '{"state":"CLOSED"}' >"$fx/state-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
# Nothing of the turn reached GitHub: no outcome, no boxes, no question label.
t_lacks "$(t_calls gh)" "kind=plan issue=7 turn=1"
t_lacks "$(t_calls gh)" "--add-label tehas:question"
# One line says the turn was dropped, and the session is over.
t_contains "$t_out" "planning #7 dropped: the issue is closed"
t_eq "$(rec_open)" "none"

t_case "a waiting turn of a closed issue is dropped with no line of its own"
turn_pending
# The question of the first turn went out and the issue carries its label: what
# remains is the write of the plan's boxes, which a closed issue does not take.
echo '[{"number":7,"labels":[{"name":"tehas:question"}]}]' >"$fx/asking.json"
comments_of 7 4001 "$(plan_body ' ' x ' ')" tehas-bot
echo '{"state":"CLOSED"}' >"$fx/state-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "issue edit 7 --repo acme/widget --body"
t_contains "$t_out" "planning #7 dropped: the issue is closed"
t_eq "$(rec_open)" "none"

t_case "a turn that ends while its issue is open posts as before"
turn_pending
echo '{"state":"OPEN"}' >"$fx/state-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_lacks "$t_out" "dropped: the issue is closed"

t_done
