#!/usr/bin/env bash
# Tests for libexec/dispatch: the question on a parked issue, retry and leave
# it parked.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh"

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
t_eq "$(jq -c '[.by, .how, .at]' "$T/calls/approved.at-mark" 2>&1)" '["alice","retry-tick","2026-01-01T01:00:00Z"]'

t_case "the question that was answered is used up by the start"
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "a retry waits while every slot is taken, and the tick stays"
parked
comments 7 "$(park_body x ' ')" alice
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(cat "$park_id")" 4001
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
t_eq "$(cat "$park_id")" 4001

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
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "leave it parked keeps the question on record while its label cannot be removed"
parked
comments 7 "$(park_body ' ' x)" alice
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$(cat "$park_id")" 4001
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:question"
rm -f "$fx/edit-fails"

t_case "a dry run of leave it parked changes nothing"
parked
comments 7 "$(park_body ' ' x)" alice
t_run "$dispatch --dry-run"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(cat "$park_id")" 4001

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
t_eq "$(cat "$park_id")" 4001
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
t_eq "$([ -e "$park_id" ] && echo kept)" ""

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
  t_eq "$(cat "$park_id")" 4001
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
t_eq "$(cat "$park_id")" 4001

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
t_eq "$(find "$T/home/tehas/state" -name 'park-*' | wc -l)" 0

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

t_case "a park id of another issue is no record for this one"
unrecorded
mkdir -p "$T/home/tehas/state"
echo 4001 >"$park_id"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "This question is not one the factory is waiting on."

t_case "a park id file that holds no number is no record"
for junk in "" "4001 x" "abc"; do
  parked
  comments 7 "$(park_body x ' ')" alice
  printf '%s\n' "$junk" >"$park_id"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_contains "$(t_calls gh)" "This question is not one the factory is waiting on."
done

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
