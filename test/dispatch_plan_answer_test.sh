#!/usr/bin/env bash
# Tests for libexec/dispatch: the answers to a planning session. The options a
# turn posts, what it filed, ticks and replies, and how a session ends.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "the record lists the options that were posted, so a box and its option keep the same place"
to_plan
jq -n '{session: "abc-123", text: "Many ways.", question: {question: "Which?", options: [range(30) | {label: "Way \(.)", description: ""}]}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$(rec | jq -c '[(.options | length), .options[0], .options[19]]')" '[20,"Way 0","Way 19"]'
t_eq "$(t_calls gh | grep -c '^- \[ \] Way ')" 20
t_contains "$(t_calls gh)" "Options left out: 10."

t_case "questions posted as one have no option boxes, so a tick answers none of them"
to_plan
jq -n '{session: "abc-123", text: "Two things.", question: {question: "The agent asked 2 questions. Answer each by its number in a reply.\n\n1. Which?\n   - A\n\n2. Where?\n   - B", options: []}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$(rec | jq -c .options)" "[]"
t_contains "$(t_calls gh)" "1. Which?"
t_contains "$(t_calls gh)" "2. Where?"
t_eq "$(t_calls gh | grep -c '^- \[ \] ')" 1
t_contains "$(t_calls gh)" "- [ ] End this planning session"

t_case "a turn that filed issues says so under its words and keeps them on record"
to_plan
plan_files https://github.com/acme/widget/issues/42 https://github.com/acme/widget/issues/43
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -F -e 'Filed in this turn: https://github.com/acme/widget/issues/42, https://github.com/acme/widget/issues/43')" 1
t_eq "$(rec | jq -c .filed)" '["https://github.com/acme/widget/issues/42","https://github.com/acme/widget/issues/43"]'

t_case "the next turn of a session that filed is told not to file them again"
planning
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all","Change some"],"comment":4001,"filed":["https://github.com/acme/widget/issues/42","https://github.com/acme/widget/issues/43"]}'
plan_files
comments 7 "$(plan_filed_body 1 https://github.com/acme/widget/issues/42 https://github.com/acme/widget/issues/43)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" "7 --out $record.out --keep $keep --session abc-123 --answer $keep/answer.md"
t_eq "$(cat "$keep/answer.md")" "Already filed in this session: https://github.com/acme/widget/issues/42, https://github.com/acme/widget/issues/43

The operator chose: Change some"
# The answer on record is the answer as it came; the turn was given it with
# the line on top.
t_eq "$(jq -r .planning.answer "$T/calls/record.at-turn")" "The operator chose: Change some"

t_case "what a later turn files is added to what the session filed before"
planning
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all","Change some"],"comment":4001,"filed":["https://github.com/acme/widget/issues/41"]}'
plan_files https://github.com/acme/widget/issues/43 https://github.com/acme/widget/issues/42
comments 7 "$(plan_filed_body 1 https://github.com/acme/widget/issues/41)" alice
t_run "$dispatch"
t_eq "$(rec | jq -c '.filed | sort')" '["https://github.com/acme/widget/issues/41","https://github.com/acme/widget/issues/42","https://github.com/acme/widget/issues/43"]'
t_eq "$(t_calls gh | grep -c -F -e 'Filed in this turn: https://github.com/acme/widget/issues/42, https://github.com/acme/widget/issues/43')" 1

t_case "a turn that filed nothing names none, and the next answer says nothing of filings"
planning
plan_files
comments 7 "$(plan_filed_body 1)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" "7 --out $record.out --keep $keep --session abc-123 --answer $keep/answer.md"
t_eq "$(cat "$keep/answer.md")" "The operator chose: Change some"
t_lacks "$(t_calls gh)" "Filed in this turn"
rec | jq -e 'has("filed") | not' >/dev/null || t_bad "the record has a filed key"

t_case "of what a turn says it filed, only GitHub issue addresses are kept, each once"
to_plan
plan_files https://github.com/acme/widget/issues/42 https://evil.example/acme/widget/issues/8 https://github.com/acme/widget/issues/42
t_run "$dispatch"
t_eq "$(rec | jq -c .filed)" '["https://github.com/acme/widget/issues/42"]'
rm -f "$T"/calls/*
echo '[{"number":7}]' >"$fx/asking.json"
echo '[]' >"$fx/plans.json"
plan_files
comments_of 7 4002 "$(plan_filed_body 1 https://github.com/acme/widget/issues/42)" alice
t_run "$dispatch"
t_eq "$(head -n 1 "$keep/answer.md")" "Already filed in this session: https://github.com/acme/widget/issues/42"
t_lacks "$(cat "$keep/answer.md")" "evil.example"

t_case "a ticked option goes to the session as the next message"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --session abc-123 --answer $keep/answer.md"
t_eq "$(cat "$keep/answer.md")" "The operator chose: Change some"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=2 -->"
t_eq "$(rec | jq -r .turns)" 2
# The answer's text is on record with who gave it, and nothing is left in tmp.
t_eq "$(t_row history-show 7 --kind plan -- '[.rows[] | .turn] | tostring')" '[1,2]'
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-answer.*' | wc -l)" 0

t_case "a reply goes to the session as it was written"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"Adopt all, but keep `tagged-at` open.\nThen file 3 and 5.","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(cat "$keep/answer.md")" 'The operator replied on the issue:

Adopt all, but keep `tagged-at` open.
Then file 3 and 5.'

t_case "a reply's turn is given the reply's comment id, so its attachments go with it"
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --session abc-123 --answer $keep/answer.md --comment 4003"

t_case "a ticked option passes no comment"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_lacks "$(plan_calls)" "--comment"

t_case "a reply wins over a tick, and the answer says what was ticked"
planning
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice bob'" >"$TEHAS_INSTANCE_DIR/instance.env"
comments 7 "$(plan_body x ' ' ' ')" alice '[{"databaseId":4003,"author":{"login":"bob"},"body":"actually, wait","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(cat "$keep/answer.md")" 'The operator replied on the issue:

actually, wait

A box was also ticked, by alice: Adopt all'
t_contains "$t_out" "planning #7 continues (answer from bob)"

t_case "a reply with the way out ticked runs the turn and says the box was ticked"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice '[{"databaseId":4003,"author":{"login":"alice"},"body":"one more thing","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_contains "$(plan_calls)" "--answer $keep/answer.md"
t_contains "$(cat "$keep/answer.md")" "A box was also ticked, by alice: End this planning session"

t_case "a reply alone says nothing of a box"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"just this","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_lacks "$(cat "$keep/answer.md")" "A box was also ticked"

t_case "an answer on an issue closed since the turn ran runs no turn"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
before=$(rec)
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
# The bash poll's list and its read of the issue disagree here. The Rust
# poll's one page cannot: a closed issue is not on it, and its session ends
# as any session of a closed issue does.
if bash_reads; then
  t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
  t_eq "$(rec)" "$before"
fi
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a stranger's reply and a stranger's tick run no turn"
planning
comments 7 "$(plan_body x ' ' ' ')" mallory '[{"databaseId":4003,"author":{"login":"mallory"},"body":"file everything","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "api -X PATCH"

t_case "the way out ends the session: labels off, record gone, one comment"
planning
mkdir -p "$keep/images"
comments 7 "$(plan_body ' ' ' ' x)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "--remove-label tehas:question"
t_contains "$(t_calls gh)" "Planning session ended by alice."
t_eq "$(rec_open)" none
t_eq "$([ -e "$keep" ] && echo kept)" ""
# The question it ended in is withdrawn with the session.
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .recorded] | tostring')" '[false,false]'

t_case "the turn limit ends the session in place of another turn"
planning
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_PLAN_MAX_TURNS=2
t_plan "$T/home/tehas" 7 '{"turns":2,"options":["Adopt all","Change some"],"comment":4001}'
comments 7 "$(plan_body x ' ' ' ' 2)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "reached its limit of 2 turns"
t_eq "$(rec_open)" none
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-answer.*' | wc -l)" 0

t_case "an answer to a session whose record is gone ends it and says why"
planning
TEHAS_HOME=$T/home/tehas "$T_STORE" plan-end 7 --close >/dev/null
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "The record of this planning session is gone"

t_case "a later turn that fails asks again and keeps the session"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
: >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish"
t_contains "$(t_calls gh)" "--add-label tehas:question --add-label needs-human-attention"
t_eq "$(rec | jq -r '[.session, .turns, .failed] | @tsv')" "abc-123	2	true"

t_case "a later turn whose session pi-rukas no longer has ends the session and says to start again"
planning
mkdir -p "$keep"
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123"
t_contains "$t_out" "planning #7: Pi no longer has session abc-123"
t_contains "$(t_calls gh)" "--remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention"
t_contains "$(t_calls gh)" "pi-rukas has no record of it on"
t_contains "$(t_calls gh)" "Apply \`tehas:plan\` again to start a new session."
t_lacks "$(t_calls gh)" "This planning turn did not finish"
t_lacks "$(t_calls gh)" "tehas:question kind=plan issue=7 turn=2"
t_eq "$(rec_open)" none
t_eq "$([ -e "$keep" ] && echo kept)" ""
# It ended lost, and the turn's line says so.
t_eq "$(t_row plan-show 7 -- '.planning | [.lost, (.ended_at | type)] | tostring')" '[true,"number"]'
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1] | [.turn, .outcome] | tostring')" '[2,"lost"]'

t_case "a refine the poll started on its own hands the issue back when it ends without a plan"
planning
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_PLAN_MAX_TURNS=2
t_plan "$T/home/tehas" 7 '{"turns":2,"options":["Adopt all","Change some"],"comment":4001,"label":"tehas:refine","auto":true}'
comments 7 "$(plan_body x ' ' ' ' 2)" alice
t_run "$dispatch"
t_contains "$(t_calls gh)" "reached its limit of 2 turns"
t_contains "$(t_calls gh)" "--remove-label tehas:plan --add-label needs-human-attention"
t_eq "$(rec_open)" none
planning
t_plan "$T/home/tehas" 7 '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"comment":4001,"label":"tehas:refine","auto":true}'
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "pi-rukas has no record of it on"
t_contains "$(t_calls gh)" "--remove-label tehas:plan --add-label needs-human-attention"
# A refine a label started ends as a plan does.
planning
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_PLAN_MAX_TURNS=2
t_plan "$T/home/tehas" 7 '{"turns":2,"options":["Adopt all","Change some"],"comment":4001,"label":"tehas:refine"}'
comments 7 "$(plan_body x ' ' ' ' 2)" alice
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:plan --remove-label needs-human-attention"

t_case "a lost session whose labels cannot be taken off is ended by the next poll"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
echo "--remove-label tehas:question --remove-label tehas:planning" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(rec | jq -c '[.lost, .running]')" '[true,true]'
t_lacks "$(t_calls gh)" "pi-rukas has no record of it on"
rm -f "$T"/calls/* "$fx/edit-fails-on" "$fx/plan-fails"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "pi-rukas has no record of it on"
t_eq "$(rec_open)" none

t_case "an outcome that could not be posted is posted by the next poll, before anything else"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
: >"$fx/post-fails"
t_run "$dispatch"
t_eq "$(rec | jq -r .pending)" true
t_eq "$([ -s "$record.out" ] && echo kept)" kept
t_lacks "$(t_calls gh)" "--add-label tehas:question"
rm -f "$T"/calls/* "$fx/post-fails"
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=2 -->"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(rec | jq -r .pending)" false

t_case "the record of a session whose issue no longer asks is removed"
planning
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(rec_open)" none
t_lacks "$(t_calls gh)" "issue comment"

t_done
