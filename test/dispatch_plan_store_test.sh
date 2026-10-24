#!/usr/bin/env bash
# Tests for libexec/dispatch: the planning session on the state store. What
# survives a turn (B8), which question an answer counts in (F5), an open
# session holding its issue out of other work (A13), a session replaced (B4),
# and a store that does not answer (D2).
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# store_back: the real store again, on the same database.
store_back() {
  rm -f "$T/home/tehas/libexec/store"
  ln "$T_STORE" "$T/home/tehas/libexec/store"
}
# The question a poll of this run posts, and the turn it names.
posted_turn() { t_calls gh | grep -c "<!-- tehas:question kind=plan issue=7 turn=$1 -->"; }

# --- B8 ----------------------------------------------------------------------
t_case "B8: the brief, the session and what it filed survive a failed turn"
planning
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all","Change some"],"comment":4001,"filed":["https://github.com/acme/widget/issues/41"]}'
comments 7 "$(plan_body ' ' x ' ')" alice
echo '{"session":"abc-123"}' >"$fx/plan-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "This planning turn did not finish"
t_eq "$(rec | jq -c '[.session, .turns, .failed, .filed]')" '["abc-123",2,true,["https://github.com/acme/widget/issues/41"]]'
t_eq "$(rec_brief | head -n 1)" "# Faster search"
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1] | [.turn, .outcome, .rc, .session] | tostring')" '[2,"failed",2,"abc-123"]'

t_case "B8: a turn that exits 0 with no session in its outcome is a failed turn of the session on record"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
echo '{"text":"Two ways.","question":null}' >"$fx/plan-result.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it exited with status 0."
t_eq "$(rec | jq -c '[.session, .turns, .failed, .running, .options]')" '["abc-123",2,true,false,[]]'
t_eq "$(rec_open)" open

t_case "B8: a session pi-rukas lost ends with outcome lost, and stays open until its labels come off"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
echo "--remove-label tehas:question --remove-label tehas:planning" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1] | [.turn, .outcome] | tostring')" '[2,"lost"]'
t_eq "$(rec | jq -c '[.lost, .running]')" '[true,true]'
t_eq "$(rec_open)" open
rm -f "$T"/calls/* "$fx/edit-fails-on" "$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "pi-rukas has no record of it on"
t_eq "$(rec_open)" none
# One end on record for the one turn.
t_eq "$(t_row history-show 7 --kind plan -- '[.rows[] | select(.turn == 2)] | length')" 1

# --- F5 ----------------------------------------------------------------------
t_case "F5: once a turn begins, the question it answered is answered on record, and a tick in it runs no second turn"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
echo 7 >"$fx/plan-runs"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls fake-plan-start)" "--session abc-123"
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .answered] | tostring')" '[false,true]'
rm -f "$fx/plan-runs" "$T"/calls/*
# The turn is still on record as running: the same tick is read again.
t_plan_running=$(rec | jq -r .running)
t_eq "$t_plan_running" true
lock_fd=""
exec {lock_fd}>>"$record.lock"
flock -n "$lock_fd"
t_run "$dispatch"
exec {lock_fd}>&-
t_eq "$(t_calls fake-plan-start)" ""
t_contains "$t_out" "skip #7: the question answered is not the one this session is waiting on"
t_eq "$(rec | jq -r .turns)" 1

t_case "F5: a reply after a question the session no longer has on record runs no turn"
planning
TEHAS_HOME=$T/home/tehas "$T_STORE" question-answered 7 --id 4001 --withdraw >/dev/null
comments 7 "$(plan_body ' ' ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"go on","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "skip #7: the question answered is not the one this session is waiting on"

t_case "F5: a question whose record could not be read is neither answered nor unticked"
planning
comments 7 "$(plan_body x x ' ')" alice
store_refuses "plan-show 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "skip #7: its question on record could not be read (busy)"
t_lacks "$(t_calls gh)" "api -X PATCH"
store_refuses "question-known 7" 78 'store: corrupt: the file is not a database'
t_run "$dispatch"
t_eq "$t_rc" 78
t_lacks "$(t_calls gh)" "api -X PATCH"

# --- A13 ---------------------------------------------------------------------
t_case "A13: a turn running on record is not begun again by a new tehas:plan, and the poll starts nothing"
to_plan
echo '[{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-8.json"
t_plan "$T/home/tehas" 7 '{"turns":0,"running":true}'
# The first turn was cut off, and its issue cannot be handed to a person yet.
printf '%s' '--add-label needs-human-attention' >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: its planning session could not begin: store: running"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:planning"
t_eq "$(t_calls fake-start)" ""
t_eq "$(rec | jq -r .running)" true
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-7.brief.*' | wc -l)" 0

t_case "A13: open sessions that cannot be read hold every start: busy is exit 1, a store that cannot be used 78"
for answer in "1 store: busy #0: the database is locked" "78 store: corrupt: the file is not a database"; do
  reset
  store_refuses "plan-show --open" "${answer%% *}" "${answer#* }"
  t_run "$dispatch"
  t_eq "$t_rc" "${answer%% *}"
  t_contains "$t_out" "ERROR: the state store could not list the open planning sessions"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
done

t_case "A13: when the open sessions cannot be read again after a planning write, no issue starts"
reset
# #9's session is over: its issue no longer asks, so the poll ends it, and
# reads the open sessions again before #7 could start.
t_plan "$T/home/tehas" 9 '{"turns":1,"comment":4009}'
bin=$T/home/tehas/libexec/store
rm -f "$bin"
printf '%s\n' '#!/usr/bin/env bash' \
  "if [ \"\$*\" = 'plan-show --open' ] && [ -e $(t_q "$T/calls/open-read") ]; then" \
  "  echo 'store: busy #0: the database is locked' >&2; exit 1" \
  'fi' \
  "[ \"\$*\" != 'plan-show --open' ] || : >$(t_q "$T/calls/open-read")" \
  "exec $(t_q "$T_STORE") \"\$@\"" >"$bin"
chmod 755 "$bin"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_eq "$(t_row plan-show 9 -- '.planning.ended_at | type')" number

t_case "the keep directory a turn is given is beside the workspace, not in the state directory"
to_plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "--keep $T/home/tehas/work/widget-plan-7.keep "
t_lacks "$(plan_calls)" "/state/plan-7.json.keep"
t_eq "$(find "$T/home/tehas/state" -name '*.keep' | wc -l)" 0

# --- B4 ----------------------------------------------------------------------
t_case "B4: a new tehas:plan replaces a session with its answers, what it filed and its notes"
to_plan
# The session waits for an answer nobody has given.
echo '[{"number":7}]' >"$fx/asking.json"
comments 7 "$(plan_body ' ' ' ' ' ' 2)" ""
t_plan "$T/home/tehas" 7 '{"turns":2,"options":["Adopt all"],"comment":4001,"answers":[{"turn":2,"by":"bob","fixer":false}],"filed":["https://github.com/acme/widget/issues/41"],"held":[4001],"fixer_refused":[4003]}'
t_eq "$(rec | jq -c '[.held, .fixer_refused, (.filed | length), (.answers | length)]')" '[[4001],[4003],1,1]'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(rec | jq -c '[.turns, .answers, .filed, .held, .fixer_refused, .comment]')" '[1,[],null,null,null,4002]'
# The question of the session it replaced is off the record.
t_eq "$(t_row question-known 7 --id 4001 -- '.recorded')" false

t_case "B4: a session ended and then asked for again starts with nothing of the old one"
planning
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all"],"comment":4001,"filed":["https://github.com/acme/widget/issues/41"]}'
TEHAS_HOME=$T/home/tehas "$T_STORE" plan-end 7 --close >/dev/null
echo '[]' >"$fx/asking.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
echo '{"title":"Faster search","body":"Make it fast."}' >"$fx/issue-7.json"
t_run "$dispatch"
t_eq "$(rec | jq -c '[.turns, .filed, .answers]')" '[1,null,[]]'

# --- D2 ----------------------------------------------------------------------
t_case "D2: an end of a turn the store will not take keeps the exit status, and the next poll posts it once"
planning
echo '[]' >"$fx/asking.json"
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all","Change some"],"running":true}'
cp "$fx/plan-result.json" "$record.out"
echo 0 >"$record.rc"
store_refuses "plan-end 7 --turn 2" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the end of the planning turn of #7 could not be recorded (busy); the next poll tries again"
t_eq "$(cat "$record.rc" 2>/dev/null)" 0
t_eq "$(posted_turn 2)" 0
t_eq "$(rec | jq -r .running)" true
store_back
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(posted_turn 2)" 1
t_eq "$([ -e "$record.rc" ] && echo left)" ""
t_eq "$(rec | jq -c '[.turns, .running, .pending]')" '[2,false,false]'
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(posted_turn 2)" 0

t_case "D2: an answer whose turn the store will not begin marks nothing and leaves the answer to the next poll"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
store_refuses "plan-answer 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not write the record of #7, not starting a turn (busy)"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:planning"
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-answer.*' | wc -l)" 0
store_back
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123 --answer $keep/answer.md"

t_done
