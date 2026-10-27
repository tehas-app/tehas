#!/usr/bin/env bash
# Tests for libexec/dispatch: the question on a parked issue, retry and leave
# it parked.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "an unanswered question starts nothing"
parked
comments 7 "$(park_body ' ' ' ')" ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a retry ticked by an allowed account starts a cycle"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question"
# The cycle's --restart takes it off, through pi-rukas.
t_lacks "$(t_calls gh)" "--remove-label needs-human-attention"
t_contains "$(t_calls gh)" "authorized by @alice's retry tick"
t_contains "$t_out" "start #7 (retry ticked by alice)"

t_case "a retry tick takes tehas:ready and the escalation label off"
parked
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:ready"},{"name":"tehas:millwright-escalated"}]}]' >"$fx/asking.json"
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "--remove-label tehas:question --remove-label tehas:ready --remove-label tehas:millwright-escalated"

t_case "a retry tick writes the approval record with the tick's time, before the label changes"
t_eq "$(at_mark '[.by, .how, .at, .comment_id] | tostring')" '["alice","retry-tick","2026-01-01T01:00:00Z",4001]'

t_case "the question that was answered is used up by the start"
t_eq "$(park_q)" none
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .recorded] | tostring')" '[false,false]'

t_case "a tick counts once: the next poll finds its question off the record and starts nothing"
parked
comments 7 "$(park_body x ' ')" alice
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: a box was ticked in a question this host has no record of"
t_eq "$(t_row cycle-show 7 -- .cycle.attempt)" 2

t_case "a retry waits while every slot is taken, and the tick stays"
parked
comments 7 "$(park_body x ' ')" alice
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(park_q)" 4001
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 cycles are running (limit 1)"
release

t_case "a retry whose cycle does not start is handed back, exit 1"
parked
comments 7 "$(park_body x ' ')" alice
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas could not start the cycle for this issue"
# The start took the question off the record and spent the park's retry before
# it marked the issue, and its unit never came up: the park asks for nothing.
t_eq "$(park_of '[.question, .retries]')" '[null,1]'

t_case "the comments are read a hundred at a time"
t_contains "$(t_calls gh)" "comments(last: 100)"

t_case "a dry run says what the tick would do and changes nothing"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7 (retry ticked by alice)"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(park_q)" 4001

t_case "a retry ticked by the bot or a stranger starts nothing and is undone"
for other in tehas-bot mallory ""; do
  parked
  comments 7 "$(park_body x ' ')" "$other"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
  t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
  t_contains "$(t_calls gh)" "may not answer for this factory"
done

t_case "when a refused tick cannot be undone, the note is not repeated every poll"
parked
comments 7 "$(park_body x ' ')" mallory
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_lacks "$(t_calls gh)" "may not answer for this factory"
t_contains "$t_out" "could not untick the question on #7"

t_case "both boxes ticked is no choice: nothing starts, the boxes are emptied, one note"
parked
comments 7 "$(park_body x ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_contains "$(t_calls gh)" "Tick one"

# The third box is a retry as the first one is: the same rules, and only the
# record is smaller.
t_case "the issue-alone tick by an allowed account starts a cycle with how retry-tick"
parked
comments 7 "$(park_body ' ' x ' ')" alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(at_mark '[.by, .how, .at, .comment_id] | tostring')" '["alice","retry-tick","2026-01-01T01:00:00Z",4001]'
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question"
t_contains "$(t_calls gh)" "authorized by @alice's retry tick"
t_contains "$t_out" "start #7 (retry ticked by alice)"
t_eq "$(park_q)" none

t_case "the issue-alone tick by the bot or a stranger starts nothing and is undone"
for other in tehas-bot mallory ""; do
  parked
  comments 7 "$(park_body ' ' x ' ')" "$other"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
  t_contains "$(t_calls gh)" "- [ ] Retry from the issue alone"
  t_contains "$(t_calls gh)" "may not answer for this factory"
done

t_case "both retry boxes ticked is no choice: nothing starts and the boxes are emptied"
parked
comments 7 "$(park_body x x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_contains "$(t_calls gh)" "Tick one"

t_case "a retry tick of the issue alone waits while every slot is taken, and the tick stays"
parked
comments 7 "$(park_body ' ' x ' ')" alice
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(park_q)" 4001
release

t_case "a dry run says what the issue-alone tick would do and changes nothing"
parked
comments 7 "$(park_body ' ' x ' ')" alice
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7 (retry ticked by alice)"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(park_q)" 4001

t_case "the issue-alone tick does not approve text changed after it"
parked
comments 7 "$(park_body ' ' x ' ')" alice
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-01T03:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "its text was changed by 'mallory' after alice ticked retry; untick the box and tick it again to approve it"

t_case "the pause label stops an issue-alone tick too"
parked
comments 7 "$(park_body ' ' x ' ')" alice
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "a park comment of the old two boxes still starts a clean start when it is ticked"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(at_mark .how)" 'retry-tick'

t_case "and its leave box still takes the question away"
parked
comments 7 "$(park_body ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
t_eq "$(park_q)" none

t_case "leave it parked takes the question away and keeps the issue parked"
parked
comments 7 "$(park_body ' ' ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
t_lacks "$(t_calls gh)" "needs-human-attention"
t_eq "$(park_q)" none
# Answered by the person who ticked it, and on record as such.
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .recorded, .answered] | tostring')" '[false,true,true]'

t_case "leave it parked keeps the question on record while its label cannot be removed"
parked
comments 7 "$(park_body ' ' ' ' x)" alice
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$(park_q)" 4001
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:question"
rm -f "$fx/edit-fails"

t_case "a leave it parked whose answer the store would not take is said in the log"
parked
comments 7 "$(park_body ' ' ' ' x)" alice
store_refuses "question-answered 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
t_contains "$t_out" "WARN: could not record the answer to the question of #7 (busy)"

t_case "a dry run of leave it parked changes nothing"
parked
comments 7 "$(park_body ' ' ' ' x)" alice
t_run "$dispatch --dry-run"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(park_q)" 4001

t_case "a comment from an allowed account on a parked issue is discussion, not an answer"
parked
comments 7 "$(park_body ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"retry please","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "and it does not hide a tick made after it"
parked
comments 7 "$(park_body x ' ')" alice '[{"databaseId":4003,"author":{"login":"alice"},"body":"hm","createdAt":"2026-01-01T00:30:00Z"}]'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a retry does not approve text that somebody else changed after the tick"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-01T03:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "its text was changed by 'mallory' after alice ticked retry; untick the box and tick it again to approve it"

t_case "a retry waits while too many pull requests await a merge"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":1},{"number":2},{"number":3}]' >"$fx/awaiting.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "the pause label stops a retry too"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "a marker in somebody else's comment is not a question"
parked
jq -n --arg b "$(park_body x ' ')" '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "mallory"}, body: $b, createdAt: "t", lastEditedAt: "t", editor: {login: "alice"}}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "when the comments cannot be read nothing is decided"
parked
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""

t_done
