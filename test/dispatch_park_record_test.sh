#!/usr/bin/env bash
# Tests for libexec/dispatch: a retry tick that is not taken, and a question on
# record that this host does not know or cannot read.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

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
# The bash poll's list and its read of the issue disagree; the Rust poll's one
# page has no closed issue on it to skip.
bash_reads && t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
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
if bash_reads; then
  t_eq "$(grep -c 'skip #7: it is closed, and a closed issue is never taken' <<<"$t_out")" 1
  t_eq "$(t_calls gh | grep -c '^api repos/acme/widget/issues/7$')" 1
fi
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
  # The Rust poll reads the comments of every asking issue with its queue.
  bash_reads && t_lacks "$(t_calls gh)" "comments(last"
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

t_done
