#!/usr/bin/env bash
# Tests for tehasbot dispatch: starting a planning turn, its record, and the
# turns that fail or are cut off.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1
t_case "a session does not start when the issue cannot be marked as planning"
to_plan
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "ERROR: could not mark #7 as planning, not starting a turn"
t_eq "$(rec)" null
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-7.brief.*' | wc -l)" 0
rm -f "$fx/edit-fails"

t_case "the turn is on record as running before the issue is marked"
to_plan
t_run "$dispatch"
t_eq "$(jq -c '.planning | [.session, .turns, .options, .pending, .failed, .running, .by, .label, .comment_id]' "$T/calls/record.at-mark")" '[null,0,[],false,false,true,"alice","tehas:plan",null]'
t_eq "$(jq -r '.planning.started_at | type' "$T/calls/record.at-mark")" number

t_case "a session does not start when its record cannot be written, and the issue is not marked"
for answer in "1 store: busy #7: the database is locked" "78 store: corrupt: the file is not a database"; do
  to_plan
  store_refuses "plan-begin 7" "${answer%% *}" "${answer#* }"
  t_run "$dispatch"
  t_eq "$t_rc" "${answer%% *}"
  case ${answer%% *} in
    1) t_contains "$t_out" "ERROR: could not write the record of #7, not starting a turn (busy)" ;;
    *) t_contains "$t_out" "ERROR: the state store cannot be used: store: corrupt" ;;
  esac
  t_eq "$(plan_calls)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "issue comment"
done

t_case "a later turn that cannot be marked leaves the record as it was, still answerable"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
before=$(rec | jq -c 'del(.startedAt)')
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(plan_calls)" ""
t_eq "$(rec | jq -c 'del(.startedAt)')" "$before"
t_eq "$(jq -r .planning.running "$T/calls/record.at-mark")" true
# The question is open again: the answer it had is taken back.
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .answered] | tostring')" '[true,false]'
rm -f "$T"/calls/* "$fx/edit-fails"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --session abc-123 --answer $keep/answer.md"

t_case "while a later turn runs the record names no question"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_eq "$(jq -c '.planning | [.session, .turns, .options, .pending, .failed, .running, .comment_id, .answers]' "$T/calls/record.at-turn")" '["abc-123",1,["Adopt all","Change some"],false,false,true,null,[{"turn":2,"by":"alice","millwright":false,"fixer":false}]]'
t_eq "$(jq -r '.planning.started_at | type' "$T/calls/record.at-turn")" number
# The question answered is answered on record by whoever answered it.
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .answered] | tostring')" '[false,true]'

t_case "who started a session and who answered each turn stay on record after the turn"
planning
t_plan "$T/home/tehas" 7 '{"by":"alice","turns":2,"options":["Adopt all","Change some"],"comment":4001,"answers":[{"turn":2,"by":"bob","fixer":false}]}'
comments 7 "$(plan_body ' ' x ' ' 2)" alice
t_run "$dispatch"
t_eq "$(rec | jq -c '[.turns, .by, .answers]')" '[3,"alice",[{"turn":2,"by":"bob","fixer":false},{"turn":3,"by":"alice","fixer":false}]]'

t_case "a turn that fails keeps them too"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
echo '{"session":"abc-123"}' >"$fx/plan-fails"
t_run "$dispatch"
t_eq "$(rec | jq -c '[.failed, .by, .answers]')" '[true,"alice",[{"turn":2,"by":"alice","fixer":false}]]'
rm -f "$fx/plan-fails"

t_case "a turn cut off before the issue was marked is reported like any cut-off turn"
planning
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all","Change some"],"running":true}'
# The mark never happened: the issue still asks, and the old answer is there.
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it was cut off"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention"
t_eq "$(rec | jq -c '[.session, .turns, .options, .pending, .failed, .running, .comment]')" '["abc-123",2,[],false,true,false,4002]'
t_eq "$(plan_calls)" ""

t_case "a first turn cut off before the issue was marked ends visibly and takes the plan label off"
to_plan
t_plan "$T/home/tehas" 7 '{"turns":0,"running":true}'
# What GitHub answers once the label is off.
echo '[]' >"$fx/plans.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:plan --remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$(rec_open)" none
t_eq "$(plan_calls)" ""

t_case "a first turn that fails parks the issue and leaves no session open"
to_plan
: >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$(rec_open)" none
t_eq "$([ -e "$keep" ] && echo left)" ""

# The session ends, but stays readable until the next tehas:plan of the issue,
# so the archive of a turn that made no session is not the line of history
# alone. export_test.sh checks what tehas export does with it.
t_case "a first turn that fails leaves its ended session readable for an export"
to_plan
: >"$fx/plan-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_row plan-show 7 -- '.planning | [.session, .turns, .running, (.ended_at | type), .brief != null] | tostring')" '[null,1,false,"number",true]'
t_eq "$(t_row history-show 7 --kind plan -- '[.rows[] | [.turn, .outcome, .rc]] | tostring')" '[[1,"failed",2]]'

t_case "a turn of a session that went on keeps the session open on record"
to_plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(rec | jq -r .session)" abc-123
t_eq "$(rec_open)" open

t_case "a first turn that fails after its session came into being keeps the session"
to_plan
echo '{"session":"abc-123"}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it exited with status 2."
t_contains "$(t_calls gh)" "Reply in a comment to run the turn again"
t_lacks "$(t_calls gh)" "could not start"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention"
t_eq "$(rec | jq -c 'del(.startedAt)')" '{"session":"abc-123","turns":1,"options":[],"pending":false,"failed":true,"running":false,"comment":4002,"by":"alice","label":"tehas:plan","answers":[]}'

t_case "and a reply runs the turn again in that session"
rm -f "$T"/calls/* "$fx/plan-fails"
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
echo '[]' >"$fx/plans.json"
jq -n '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 4002, author: {login: "tehas-bot", databaseId: ("tehas-bot" | explode | add)}, createdAt: "2026-01-01T00:00:00Z", lastEditedAt: null, editor: null,
   body: "This planning turn did not finish.\n\n- [ ] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=1 -->"},
  {databaseId: 4003, author: {login: "alice"}, body: "go on", createdAt: "2026-01-01T02:00:00Z"}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --session abc-123 --answer $keep/answer.md --comment 4003"
t_eq "$(cat "$keep/answer.md")" "The operator replied on the issue:

go on"

t_done
