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

t_case "a retry tick writes the approval record with the tick's time, before the label changes"
t_eq "$(at_mark '[.by, .how, .at, .comment_id]')" '["alice","retry-tick","2026-01-01T01:00:00Z",4001]'

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
comments 7 "$(park_body x x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "api -X PATCH"
t_contains "$(t_calls gh)" "Tick one"

t_case "leave it parked takes the question away and keeps the issue parked"
parked
comments 7 "$(park_body ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
t_lacks "$(t_calls gh)" "needs-human-attention"
t_eq "$(park_q)" none
# Answered by the person who ticked it, and on record as such.
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .recorded, .answered] | tostring')" '[false,true,true]'

t_case "leave it parked keeps the question on record while its label cannot be removed"
parked
comments 7 "$(park_body ' ' x)" alice
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$(park_q)" 4001
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:question"
rm -f "$fx/edit-fails"

t_case "a leave it parked whose answer the store would not take is said in the log"
parked
comments 7 "$(park_body ' ' x)" alice
store_refuses "question-answered 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
t_contains "$t_out" "WARN: could not record the answer to the question of #7 (busy)"

t_case "a dry run of leave it parked changes nothing"
parked
comments 7 "$(park_body ' ' x)" alice
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

t_case "a question on one issue does not hold up a ready issue behind it"
reset
echo '[{"number":5}]' >"$fx/asking.json"
jq -n --arg b "$(park_body ' ' ' ')" '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "tehas-bot"}, body: $b, createdAt: "t", lastEditedAt: null, editor: null}]}}}}}' >"$fx/comments-5.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a retry tick on an issue whose cycle was an epic starts nothing, and the tick stays"
parked
comments 7 "$(park_body x ' ')" alice
jq -cn '{number: 7, state: "open", title: "Faster search", body: "Make it fast.",
  labels: [{name: "needs-human-attention"}, {name: "tehas:question"}],
  sub_issues_summary: {total: 13, completed: 0}}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: it is an epic with 13 sub-issues open, and an epic is never started"
# Nothing of the issue is changed: no label of tehas's on it is theirs to take,
# and the tick stays for a person to deal with.
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "issue comment 7"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(park_q)" 4001

t_case "and a dry run of that tick says the same and changes nothing"
parked
comments 7 "$(park_body x ' ')" alice
jq -cn '{number: 7, state: "open", title: "Faster search", body: "Make it fast.",
  labels: [{name: "needs-human-attention"}, {name: "tehas:question"}],
  sub_issues_summary: {total: 13, completed: 0}}' >"$fx/work-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: it is an epic with 13 sub-issues open, and an epic is never started"
t_lacks "$t_out" "would run a cycle"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"
t_eq "$(park_q)" 4001

t_case "a retry tick on an issue whose sub-issues are all closed starts the cycle"
parked
comments 7 "$(park_body x ' ')" alice
jq -cn '{number: 7, state: "open", title: "Faster search", body: "Make it fast.",
  labels: [{name: "needs-human-attention"}, {name: "tehas:question"}],
  sub_issues_summary: {total: 13, completed: 13}}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "it is an epic"

t_case "a retry tick is used up once the issue is marked"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
patch_line=$(t_calls gh | grep -n 'api -X PATCH' | head -1 | cut -d: -f1)
start_line=$(t_calls gh | grep -n -e '--add-label tehas:running' | head -1 | cut -d: -f1)
t_eq "$([ -n "$patch_line" ] && [ -n "$start_line" ] && [ "$start_line" -lt "$patch_line" ] && echo after)" after

t_case "when the issue cannot be marked the tick stays, and the next poll starts the retry"
parked
comments 7 "$(park_body x ' ')" alice
printf '%s' '--add-label tehas:running' >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not mark #7 as running, not starting a cycle"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(park_q)" 4001
rm -f "$fx/edit-fails-on" "$T"/calls/*
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"

t_case "when the tick cannot be undone the cycle still starts, and the tick starts nothing more"
parked
comments 7 "$(park_body x ' ')" alice
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "WARN: could not untick the retry on #7; its question is no longer on record, so it starts nothing more"
t_eq "$(park_q)" none

t_case "a start whose question cannot be taken off the record starts nothing, and its tick and question stay"
parked
comments 7 "$(park_body x ' ')" alice
store_refuses "question-answered 7 --id 4001 --withdraw" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: #7: the question of its park could not be taken off the record (busy), not starting a cycle"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(park_q)" 4001
t_eq "$(t_row approval-current 7 -- '.approval')" null

t_case "a withdraw that timed out after it was done puts the question back, and the next poll starts the retry"
parked
comments 7 "$(park_body x ' ')" alice
bin=$T/home/tehas/libexec/store
rm -f "$bin"
printf '%s\n' '#!/usr/bin/env bash' \
  "printf '%s\\n' \"\$*\" >>$(t_q "$T/calls/store")" \
  'if [ "${1:-}" = question-answered ]; then' \
  "  $(t_q "$T_STORE") \"\$@\" >/dev/null 2>&1; exit 124" \
  'fi' \
  "exec $(t_q "$T_STORE") \"\$@\"" >"$bin"
chmod 755 "$bin"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "could not be taken off the record (timeout)"
t_eq "$(t_calls fake-start)" ""
t_eq "$(park_q)" 4001
rm -f "$bin" "$T"/calls/*
cp "$T_STORE" "$bin"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"

t_case "a retry the store would not spend once the issue is marked is a warning, and the cycle starts"
parked
comments 7 "$(park_body x ' ')" alice
store_refuses "park-retry 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: #7: the retry of its park could not be spent (busy)"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(park_q)" none

t_case "a retry ticked on an issue that is not parked starts nothing"
for have in "tehas:question" "needs-human-attention tehas:question tehas:running" "needs-human-attention tehas:question tehas:awaiting-merge"; do
  parked
  comments 7 "$(park_body x ' ')" alice
  jq -n --arg l "$have" '[{number: 7, labels: ($l | split(" ") | map({name: .}))}]' >"$fx/asking.json"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  # A tick left in place would fire later, when the labels change.
  t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
  t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
  t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Retry was ticked, but this issue is not parked, so nothing was started."
  t_contains "$t_out" "skip #7: a retry was ticked but the issue is not parked"
  t_eq "$(park_q)" 4001
done

t_case "when that tick cannot be undone, nothing is said"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"tehas:question"}]}]' >"$fx/asking.json"
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "could not untick the question on #7"

t_case "a dry run leaves a retry tick on an issue that is not parked alone"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"tehas:question"}]}]' >"$fx/asking.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: a retry was ticked but the issue is not parked"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a retry tick on an issue closed since its cycle stopped starts nothing"
parked
comments 7 "$(park_body x ' ')" alice
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(park_q)" 4001

t_case "and so does a tehas:ready applied to that closed issue, said once"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:ready"}]}]' >"$fx/asking.json"
ready 7
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(grep -c 'skip #7: it is closed, and a closed issue is never taken' <<<"$t_out")" 1
t_eq "$(t_calls gh | grep -c '^api repos/acme/widget/issues/7$')" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"

# A question is the factory's only if this host has its id on record. The
# agent in a container holds the bot's token: it can post a comment with a
# marker and apply labels, but it cannot write the host's state directory.
unrecorded() { # issue 9, never parked by this host, made to look parked
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":9,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
  cp "$fx/events-7.json" "$fx/events-9.json"
  comments 9 "$(printf 'Please confirm you have seen this.\n\n- [x] Retry from a clean start\n\n<!-- tehas:question kind=park issue=3 turn=1 -->')" alice
}

t_case "a bot comment with a park marker that the host has no record of starts nothing"
unrecorded
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "issue comment 9 --repo acme/widget --body This question is not one the factory is waiting on. To retry, apply \`tehas:ready\`."
t_eq "$(park_q 9)" none

t_case "and when its box cannot be unticked nothing starts and nothing is said"
unrecorded
: >"$fx/patch-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "could not untick the question on #9"

t_case "a dry run changes nothing on a question the host has no record of"
unrecorded
t_run "$dispatch --dry-run"
t_lacks "$t_out" "would run a cycle"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "issue edit"

t_case "an unticked question the host has no record of is left alone"
unrecorded
comments 9 "$(park_body ' ' ' ')" ""
t_run "$dispatch"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a question on record of another issue is no record for this one"
unrecorded
# 4001 is the question of #7's park. The tick on #9 is in a comment of that id.
t_ended "$T/home/tehas" 7 alice label --parked
t_park "$T/home/tehas" 7 failed 4001
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "This question is not one the factory is waiting on."
t_eq "$(park_q)" 4001

t_case "a tick in a question taken off the record starts nothing"
# As the cycle leaves it when the label tehas:question could not be added.
parked
comments 7 "$(park_body x ' ')" alice
TEHAS_HOME=$T/home/tehas "$T_STORE" question-answered 7 --id 4001 --withdraw >/dev/null
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: a box was ticked in a question this host has no record of"
t_contains "$(t_calls gh)" "This question is not one the factory is waiting on."

t_case "a question of an earlier attempt is not honoured after a new start"
parked
comments 7 "$(park_body x ' ')" alice
# A later cycle of #7, by hand, that ended without a park.
t_ended "$T/home/tehas" 7 -
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: a box was ticked in a question this host has no record of"
t_eq "$(t_row question-known 7 --id 4001 -- '[.known, .recorded] | tostring')" '[false,true]'

t_case "a question on record that could not be read skips the issue: no tick counts and none is undone"
for refused in "park-show 7" "question-known 7"; do
  parked
  # A stranger's tick, which a poll that read the record would undo.
  comments 7 "$(park_body x ' ')" mallory
  store_refuses "$refused" 1 'store: busy #7: the database is locked'
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_contains "$t_out" "skip #7: its question on record could not be read (busy)"
  t_lacks "$(t_calls gh)" "api -X PATCH"
  t_lacks "$(t_calls gh)" "issue comment"
  t_lacks "$(t_calls gh)" "comments(last"
done

t_case "a store that cannot be used for the question on record stops the poll at 78, and nothing is undone"
parked
comments 7 "$(park_body x ' ')" mallory
store_refuses "park-show 7" 78 'store: corrupt: the file is not a database'
t_run "$dispatch"
t_eq "$t_rc" 78
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the state store cannot be used: store: corrupt"
t_lacks "$(t_calls gh)" "api -X PATCH"

t_case "a tick in another bot comment than the question on record starts nothing"
parked
comments 7 "$(park_body ' ' ' ')" "" "$(jq -nc --arg b "$(park_body x ' ')" '[{databaseId: 4005, author: {login: "tehas-bot"}, body: $b,
  createdAt: "2026-01-01T02:00:00Z", lastEditedAt: "2026-01-01T03:00:00Z", editor: {login: "alice"}}]')"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"

t_case "a question on record whose marker names another issue starts nothing"
parked
comments 7 "$(park_body x ' ' | sed 's/issue=7/issue=3/')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: the question on record does not belong to this issue"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a park id that names a planning question starts nothing"
parked
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(plan_calls)" ""
t_contains "$t_out" "skip #7: the question on record does not belong to this issue"
t_lacks "$(t_calls gh)" "api -X"

t_case "a question on record that is not among the comments read is said in the log"
parked
jq -n '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 9, author: {login: "alice"}, body: "hm", createdAt: "t", lastEditedAt: null, editor: null}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its question is not among the last 100 comments"

t_case "retry is the ticked line's text, not its place"
swapped() { # <leave box> <retry box>
  printf 'Tehas cycle stopped.\n\n- [%s] Leave it parked\n- [%s] Retry from a clean start\n\nTick one box.\n\n<!-- tehas:question kind=park issue=7 turn=1 -->\n' "$1" "$2"
}
parked
comments 7 "$(swapped x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question"
parked
comments 7 "$(swapped ' ' x)" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"


# --- A relabelled start of an issue whose last cycle parked --------------------
# The text the poll reads for #7, in the one answer that carries the edit
# record: unchanged_since keeps the body from it for the approval record, and
# park_holds compares the hash of it with the park's.
issue_body() { # <body>
  jq -cn --arg b "$1" '{data: {repository: {issue: {lastEditedAt: null, editor: null,
    body: $b}}}}' >"$fx/edit-7.json"
}

# The park of the cycle that just stopped, on record: alice's label started it
# on "Words.", the body its approval bound, and it parked on the reason given
# [intent-park], or by hand when the second word is -. Issue 7 carries
# tehas:ready again, by alice, with the body given.
parked_again() { # <body of the issue> [<park reason> [<- for a cycle by hand>]]
  parked "${2:-intent-park}" "${3:-alice}"
  # The endpoint answers: the hold of no resources is asked at rule 3, before a
  # candidate is read, so without it no start is weighed at all. curl is asked
  # for real unless mesh_curl stands in for it, and models.json is the host's
  # own file, so this follows parked, which resets the instance.
  mesh '{"data":[{"id":"test-model"}]}'
  mesh_curl
  comments 7 "$(park_body ' ' ' ')" ""
  # The body and the title come in the same answer as the edit record, so this
  # fixture is the text the poll reads. An answer with no title binds none.
  issue_body "$1"
  jq -cn --arg b "$1" '{number: 7, state: "open", body: $b,
    labels: [{name: "needs-human-attention"}, {name: "tehas:question"}, {name: "tehas:ready"}]}' \
    >"$fx/work-7.json"
  echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' \
    >"$fx/ready.json"
}

# What a poll decided, from the poll that starts here. t_calls reads one file
# per stub that every t_run of the case appends to, so a second poll of one
# case reads the first poll's calls too: the lines of each poll are cut off with
# this before the call that reads them.
new_poll() { rm -f "$T"/calls/*; }

# A park that holds keeps the labels, the park and the tick of its boxes.
holds() { # <body>: one poll on an issue parked on that text
  new_poll
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit 7"
  t_eq "$(park_of '[.attempt, .retries, .question]')" '[1,0,4001]'
}

t_case "a relabelled start on the body its last cycle parked on is held, said once on the issue"
parked_again "Words."
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Not started: the last cycle parked on \`intent-park\` and the issue text has not changed since. Edit the body, or tick Retry in the park comment."
t_eq "$(park_of '.told_at != null')" true
t_eq "$(grep -c '^issue comment 7' <<<"$(t_calls gh)")" 1

t_case "and the next poll of the same park only logs, with no comment"
parked_again "Words."
holds "Words."
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "a dry run of the hold says it and changes nothing, not its park"
parked_again "Words."
new_poll
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "issue comment 7"
t_lacks "$(t_calls gh)" "api -X"
t_eq "$(park_of .told_at)" null

t_case "a hold whose tell the store would not take tells it again on the next poll, and says so"
parked_again "Words."
store_refuses "park-told 7" 1 'store: busy #7: the database is locked'
holds "Words."
t_contains "$t_out" "WARN: could not record the hold on the park of #7 (busy)"
holds "Words."
t_eq "$(grep -c '^issue comment 7' <<<"$(t_calls gh)")" 1
t_eq "$(park_of .told_at)" null

t_case "a park that could not be read holds the start for this poll"
parked_again "Words, and more."
store_refuses "park-show 7" 1 'store: busy #7: the database is locked'
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its park on record could not be read (busy)"
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a park a start has spent holds nothing"
parked_again "Words."
TEHAS_HOME=$T/home/tehas "$T_STORE" park-retry 7 >/dev/null
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"

# The same hold asks nothing of an urgent or tehas:next issue either: the hold
# is of the poll, where its kind is settled, not of the kind itself.
t_case "an urgent issue parked on its own body is held too"
parked_again "Words."
label_events 7 "tehas:urgent=alice"
holds "Words."

t_case "a changed body starts, and the park of the held go is not current after the start"
parked_again "Words, and more."
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"
t_eq "$(park_of)" null

t_case "a ticked retry on the body its last cycle parked on starts all the same"
parked_again "Words."
comments 7 "$(park_body x ' ')" alice
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"
t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"
t_eq "$(park_of)" null

# outage_parked [<who started the cycle: alice, carol, or - for by hand>]:
# #7's last cycle parked by a provider outage, on "Words.". Only rule 4a asks
# for it: no label of a person's queues it.
outage_parked() {
  parked_again "Words." provider-outage "${1:-alice}"
  echo '[]' >"$fx/ready.json"
  echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}' >"$fx/issue-7.json"
  new_poll
}

t_case "a park of a provider outage is retried as today, on its own body"
# Rule 4a's own path, on an issue whose park names its body: the hold of 4c is
# what would refuse this start, and the outage is what leaves it to 4a. The
# start rests on the account that started the parked cycle, and the issue
# carries no tehas:ready for the label's path.
outage_parked
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "start #7 (requeued by alice)"
t_lacks "$t_out" "its last cycle parked on"
t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"
t_eq "$(park_of)" null
t_eq "$(at_mark '[.by, .how] | tostring')" '["alice","outage-retry"]'
t_eq "$(t_row cycle-show 7 -- '[.cycle.started_by, .cycle.requeued] | tostring')" '["alice",true]'

t_case "the retry rests on who started the parked cycle, not on a later approval of another account"
outage_parked
# bob's approval, later and never used: the retry is alice's.
t_approve "$T/home/tehas" 7 bob label 1767300000
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "start #7 (requeued by alice)"
t_eq "$(at_mark .by)" alice

t_case "a retry whose account may no longer label spends it, and starts nothing"
outage_parked carol
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its retry after a provider outage is not started; it waits for a person"
t_eq "$(park_of '[.retries, .automatic]')" '[1,false]'
new_poll
t_run "$dispatch"
t_lacks "$t_out" "its retry after a provider outage"

t_case "a dry run of that leaves the retry owed"
outage_parked carol
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its retry after a provider outage is not started; it waits for a person"
t_eq "$(park_of '[.retries, .automatic]')" '[0,true]'

t_case "one retry after an outage, then a person: the retry's own outage park owes none"
outage_parked
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
# The retry's cycle (attempt 2, requeued) is parked by an outage too.
TEHAS_HOME=$T/home/tehas flock -w 5 "$cycles_dir/7.lock" "$T_STORE" cycle-end 7 --attempt 2 --status handoff \
  --provider --parked >/dev/null
t_park "$T/home/tehas" 7 provider-outage 4009
t_eq "$(park_of '[.attempt, .automatic]')" '[2,false]'
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$t_out" "its retry after a provider outage"

t_case "an outage park of a cycle started by hand owes no retry"
outage_parked -
t_eq "$(park_of .automatic)" false
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$t_out" "provider outage"

t_case "a retry whose unit never comes up leaves no retry behind"
outage_parked
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(park_of '[.retries, .automatic]')" '[1,false]'
rm -f "$fx/start-fails"
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "a retry after an outage whose issue cannot be marked is still owed, and the next poll starts it"
outage_parked
printf '%s' '--add-label tehas:running' >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not mark #7 as running, not starting a cycle"
t_eq "$(t_calls fake-start)" ""
t_eq "$(park_of '[.retries, .automatic]')" '[0,true]'
rm -f "$fx/edit-fails-on" "$T"/calls/*
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(park_of)" null

t_case "a busy store keeps the retry after an outage for the next poll: nothing starts, the park stays"
outage_parked
store_refuses "park-show --automatic" 1 'store: busy: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the state store could not list the parks owed a retry (busy)"
t_eq "$(park_of '[.retries, .automatic]')" '[0,true]'
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a store that cannot be used for the retry after an outage stops the poll at 78"
outage_parked
store_refuses "park-show --automatic" 78 'store: corrupt: the file is not a database'
t_run "$dispatch"
t_eq "$t_rc" 78
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the state store could not list the parks owed a retry (corrupt)"
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a park of a cycle started by hand names no body and starts as today"
# Nothing bound a body for a run by hand: no reason to hold.
parked_again "Words." intent-park -
t_eq "$(park_of .body_sha256)" null
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"
t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"

t_done
