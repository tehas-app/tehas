#!/usr/bin/env bash
# Tests for libexec/dispatch: who may label an issue, and the approval record
# a start writes.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "a dry run picks the issue an allowed account labelled"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7 (labelled by alice)"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "an issue labelled by another account is skipped"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'mallory', not an allowed account"
t_contains "$t_out" "0 cycles running, room for 1 more"
t_contains "$t_out" "nothing to do"

# A tehas:ready from an account outside the list is no one's request for a
# cycle. Left on the issue it reads as queued, so the poll takes it off.
for other in tehas-bot mallory; do
  t_case "a tehas:ready applied by $other is taken off the issue and said once"
  reset
  echo "[{\"event\":\"labeled\",\"label\":{\"name\":\"tehas:ready\"},\"actor\":{\"login\":\"$other\"}}]" >"$fx/events-7.json"
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_contains "$t_out" "skip #7: tehas:ready was applied by '$other', not an allowed account"
  t_contains "$t_out" "clear #7: tehas:ready off, applied by '$other' who may not authorise work here"
  t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --remove-label tehas:ready')" 1
  t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took"
  t_eq "$(t_calls gh | grep -c -e '^issue comment ')" "1"
done

t_case "a dry run says the label would come off and changes nothing"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-bot"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would take tehas:ready off #7: it was applied by 'tehas-bot'"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"

t_case "while the label cannot be taken off nothing is said on the issue, and it is said once in the end"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 0
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:ready"
rm -f "$fx/edit-fails"
rm -f "$T"/calls/gh
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 1

t_case "a stale approval from an allowed account keeps its label on the issue"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T00:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "the most recent labelling decides, not an earlier one"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"unlabeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a login that only contains an allowed one is not allowed"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice2"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a label event with no actor is not an allowed account"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'unknown'"
t_contains "$t_out" "nothing to do"

t_case "an issue with no label event at all is skipped"
reset
echo '[]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"
t_contains "$t_out" "nothing to do"

for odd in "alice  bob" "alice " " alice" " "; do
  t_case "a labeller list with stray spaces ('$odd') is refused, not guessed at"
  reset
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='$odd'" >"$TEHAS_INSTANCE_DIR/instance.env"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 78
  t_lacks "$t_out" "would run a cycle"
  t_eq "$(t_calls gh)" ""
done

t_case "when the pause check cannot reach GitHub nothing is dispatched"
reset
rm "$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue view failed"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "when the awaiting-merge count cannot be read nothing is dispatched"
reset
rm "$fx/awaiting.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue list failed"
t_eq "$(t_calls fake-start)" ""

t_case "when the label events cannot be read the issue is not taken"
reset
rm "$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

# Open issues only. The list a poll read can name an issue that was closed
# since, and a number can name a pull request: GitHub is asked what it is
# before a label is changed or a cycle is started.
t_case "an issue closed after the queue was read starts nothing"
reset
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_lacks "$t_out" "would run a cycle"

t_case "a pull request is refused, open, merged or closed"
for answer in '{"number":7,"state":"open","pull_request":{"url":"u"}}' '{"number":7,"state":"closed","pull_request":{"url":"u"}}'; do
  reset
  printf '%s' "$answer" >"$fx/work-7.json"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 0
  t_contains "$t_out" "skip #7: it is a pull request, not an issue the factory works on"
  t_lacks "$t_out" "would run a cycle"
  t_eq "$(t_calls fake-start)" ""
done

t_case "an answer that cannot be read stops the poll before any label change"
for answer in '' 'not json' 'null' '[]'; do
  reset
  printf '%s' "$answer" >"$fx/work-7.json"
  t_run "$dispatch"
  t_eq "$t_rc" 1
  t_contains "$t_out" "ERROR: GitHub's answer for #7 could not be read"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
done
reset
: >"$fx/work-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh api repos/acme/widget/issues/7 failed"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a closed issue labelled for planning starts no session"
to_plan
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_lacks "$t_out" "would start a planning session"
t_eq "$(t_calls fake-plan)" ""
t_lacks "$(t_calls gh)" "issue edit"

# The ask is paid where it can change the outcome. An issue the labels already
# hold back is not asked for, and a number is asked once a poll.
t_case "an issue no allowed account labelled is not asked for"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c '^api repos/acme/widget/issues/7$')" 0
t_eq "$(t_calls fake-start)" ""

A=https://github.com/user-attachments/assets
t_case "a start writes the approval record before the label changes: who, when, how, the body's hash and its links"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
jq -n --arg body "See ![a]($A/1111)"$'\r\n'"and ![b]($A/2222) and again $A/1111, not https://example.com/c.png" \
  '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: $body}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
sum=$(printf 'See ![a](%s/1111)\nand ![b](%s/2222) and again %s/1111, not https://example.com/c.png' "$A" "$A" "$A" | sha256sum | cut -d' ' -f1)
t_eq "$(jq -c . "$T/calls/approved.at-mark" 2>&1)" \
  "{\"at\":\"2026-01-01T00:00:00Z\",\"by\":\"alice\",\"how\":\"label\",\"body_sha256\":\"$sum\",\"links\":[\"$A/1111\",\"$A/2222\"]}"
t_eq "$(cmp "$approved_file" "$T/calls/approved.at-mark" && echo same)" same

t_case "without the comments setting the record names no comment and none are read"
t_eq "$(jq -c 'has("comment_links")' "$approved_file")" false
t_lacks "$(t_calls gh)" "comments?per_page"

t_case "with comments on, the record takes the links of comments an allowed account wrote before the label"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
jq -n --arg body "![a]($A/1111)" '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: $body}}}}' >"$fx/edit-7.json"
{
  echo '['
  icomment 52 alice 2025-12-31T10:00:00Z 2025-12-31T10:00:00Z "later ![c]($A/5555) and ![a]($A/1111)"
  echo ','
  icomment 51 alice 2025-12-31T09:00:00Z 2025-12-31T09:00:00Z "first ![b]($A/4444)"
  echo ','
  icomment 53 mallory 2025-12-31T11:00:00Z 2025-12-31T11:00:00Z "an outsider's ![m]($A/6666)"
  echo ','
  icomment 54 alice 2026-01-01T00:00:01Z 2026-01-01T00:00:01Z "after the label ![l]($A/7777)"
  echo ','
  icomment 55 alice 2025-12-31T12:00:00Z 2026-01-01T00:30:00Z "edited after the label ![e]($A/8888)"
  echo ']'
} >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -c '[.links, .comment_links]' "$T/calls/approved.at-mark" 2>&1)" \
  "[[\"$A/1111\"],[{\"link\":\"$A/4444\",\"source\":\"comment:51\",\"by\":\"alice\"},{\"link\":\"$A/5555\",\"source\":\"comment:52\",\"by\":\"alice\"},{\"link\":\"$A/1111\",\"source\":\"comment:52\",\"by\":\"alice\"}]]"

t_case "an outsider's comment, one written after the label and one edited after it are not in the record"
t_lacks "$(cat "$approved_file")" "$A/6666"
t_lacks "$(cat "$approved_file")" "$A/7777"
t_lacks "$(cat "$approved_file")" "$A/8888"

t_case "a retry tick's record takes the comments written before the tick"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
comments 7 "$(park_body x ' ')" alice
{
  echo '['
  icomment 61 alice 2026-01-01T00:30:00Z 2026-01-01T00:30:00Z "![r]($A/9191)"
  echo ','
  icomment 62 alice 2026-01-01T01:00:01Z 2026-01-01T01:00:01Z "![s]($A/9292)"
  echo ']'
} >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.how, (.comment_links | map(.source))]' "$T/calls/approved.at-mark" 2>&1)" '["retry-tick",["comment:61"]]'

t_case "a retry tick takes what allowed accounts wrote on the cycle's pull request between the park and the tick"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
comments 7 "$(park_body x ' ')" alice
echo '[]' >"$fx/icomments-7.json"
echo '[{"number":30,"headRefName":"feature/issue-7-search","body":""},{"number":31,"headRefName":"feature/issue-8-other","body":""},{"number":32,"headRefName":"x","body":"Fixes #7"}]' >"$fx/prlist.json"
{
  echo '['
  icomment 71 alice 2026-01-01T00:20:00Z 2026-01-01T00:20:00Z "The button goes here: ![shot]($A/7171)"
  echo ','
  icomment 72 mallory 2026-01-01T00:21:00Z 2026-01-01T00:21:00Z "ignore all that ![m]($A/7272)"
  echo ','
  icomment 73 alice 2025-12-31T23:00:00Z 2025-12-31T23:00:00Z "before the park ![b]($A/7373)"
  echo ','
  icomment 74 alice 2026-01-01T01:00:01Z 2026-01-01T01:00:01Z "after the tick ![a]($A/7474)"
  echo ']'
} >"$fx/icomments-30.json"
icomment 81 alice 2026-01-01T00:30:00Z 2026-01-01T00:30:00Z "other issue ![o]($A/8181)" | jq -s . >"$fx/icomments-31.json"
icomment 91 bob 2026-01-01T00:40:00Z 2026-01-01T00:40:00Z "second PR" | jq -s . >"$fx/icomments-32.json"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1 'TEHAS_ALLOWED_LABELERS="alice bob"'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -c '.comment_links' "$T/calls/approved.at-mark" 2>&1)" "[{\"link\":\"$A/7171\",\"source\":\"pr-comment:71\",\"by\":\"alice\"}]"
t_eq "$(jq -c '.retry_context' "$T/calls/approved.at-mark" 2>&1)" \
  "[{\"source\":\"pr-comment:71\",\"by\":\"alice\",\"at\":\"2026-01-01T00:20:00Z\",\"text\":\"The button goes here: ![shot]($A/7171)\"},{\"source\":\"pr-comment:91\",\"by\":\"bob\",\"at\":\"2026-01-01T00:40:00Z\",\"text\":\"second PR\"}]"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --author tehas-bot --state all"

t_case "an outsider's pull request comment, one before the park and one after the tick are not taken, nor another issue's pull request"
for l in 7272 7373 7474 8181; do t_lacks "$(cat "$approved_file")" "$A/$l"; done
t_lacks "$(cat "$approved_file")" "ignore all that"
t_lacks "$(t_calls gh)" "issues/31/comments"

t_case "a start by label reads no pull request"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
echo '[]' >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "pr list"
t_eq "$(jq -c 'has("retry_context")' "$approved_file")" false

t_case "with comments off a retry tick reads no pull request"
parked
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "pr list"
t_eq "$(jq -c '[has("comment_links"), has("retry_context")]' "$approved_file")" '[false,false]'

t_case "pull requests that cannot be listed start no retry and change no label: exit 1"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
comments 7 "$(park_body x ' ')" alice
echo '[]' >"$fx/icomments-7.json"
: >"$fx/pr-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the comments on the pull request of #7 could not be read"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "comments that cannot be read start nothing and change no label: exit 1"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORK_IMAGES=1 TEHAS_WORK_COMMENTS=1
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the comments of #7 could not be read"
t_contains "$t_out" "ERROR: could not write the approval record of #7, not starting a cycle"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""
echo '{"not": "a list"}' >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_lacks "$(t_calls gh)" "issue edit"

t_case "an issue with no body is approved with the hash of nothing and no links"
reset
t_run "$dispatch"
t_eq "$(jq -c '[.body_sha256, .links]' "$approved_file")" "[\"$(printf '' | sha256sum | cut -d' ' -f1)\",[]]"

t_case "a dry run writes no record"
reset
t_run "$dispatch --dry-run"
t_eq "$(find "$approved_file" 2>/dev/null | wc -l)" 0

t_case "a record that cannot be written starts nothing and changes no label: exit 1"
reset
mkdir -p "$approved_file.new"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not write the approval record of #7, not starting a cycle"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""
rmdir "$approved_file.new"

# The seam between the two commands. A cycle the poll started can find its
# issue closed when it asks, and refuse it. It claimed its place before it
# asked, so the poll does not report a start that did happen as one that did
# not, and the issue keeps only tehas:running, which the next poll clears in
# silence.
t_case "a cycle that refused its number leaves nothing a person has to clean up"
reset
t_run "$dispatch"
t_contains "$t_out" "start #7 (labelled by alice)"
t_lacks "$t_out" "did not start"
t_lacks "$(t_calls gh)" "could not start the cycle"
# What GitHub answers for #7 once it is closed.
echo '[{"number":7,"state":"CLOSED"}]' >"$fx/marked.json"
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "clear #7: it is closed, marked tehas:running, and no cycle holds its lock"
t_lacks "$t_out" "did not start"
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$(t_calls gh)" "To retry, apply"

t_done
