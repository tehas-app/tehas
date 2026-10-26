#!/usr/bin/env bash
# Tests for libexec/dispatch: the millwright's labels and its planning answers.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# --- The fixer ---------------------------------------------------------------
# Issue 7 is labelled by the fixer, its body is "Words.", and alice approved
# its last cycle on that body.
fixer_ready() { # [<fixer starts since alice's approval, or - for no approval>]
  local i
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_MILLWRIGHT_LOGIN=tehas-fixer
  echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:millwright-escalated"}]}]' >"$fx/ready.json"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"2026-10-02T00:00:00Z"}]' >"$fx/events-7.json"
  jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Words."}}}}' >"$fx/edit-7.json"
  echo '[]' >"$fx/icomments-7.json"
  mkdir -p "$T/home/tehas/state/cycles"
  [ "${1:-0}" != - ] || return 0
  past 7 alice label 2026-10-01T00:00:00Z
  for ((i = 1; i <= ${1:-0}; i++)); do past 7 tehas-fixer fixer "2026-10-01T0$i:00:00Z"; done
}

t_case "the fixer's tehas:ready on an issue alice approved starts a cycle that rests on her approval"
fixer_ready
store_refuses none 1 none
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(approval '[.by, .how, .fixer_starts, .rests_on.by, .rests_on.at, .rests_on.how, (.rests_on.body_sha256 == .body_sha256)]')" \
  '["tehas-fixer","fixer",1,"alice","2026-10-01T00:00:00Z","label",true]'
t_contains "$t_out" "start #7 (labelled by the fixer by tehas-fixer)"
t_contains "$(t_calls gh)" "authorized by @tehas-fixer's \`tehas:ready\` label as the fixer, on alice's approval of 2026-10-01T00:00:00Z"

t_case "the store is told the millwright's word"
t_contains "$(t_calls store)" "approval-record 7 --at 1790899200 --by tehas-fixer --how millwright "
t_lacks "$(t_calls store)" "--how fixer"

t_case "the start takes the escalation label off with the rest of the earlier outcome"
t_contains "$(t_calls gh)" "--remove-label tehas:ready --remove-label tehas:question --remove-label tehas:millwright-escalated"

t_case "a second fixer start counts on from the first and keeps the person's approval"
fixer_ready 1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(approval '[.how, .fixer_starts, .rests_on.by]')" '["fixer",2,"alice"]'

t_case "at the limit the fixer's label comes off, and the issue says a person decides"
fixer_ready 2
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: tehas:ready from the fixer starts nothing: the fixer has started 2 cycles on it since alice approved it, the limit"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_contains "$(t_calls gh)" "The fixer applied it, and the fixer may not start this issue"
t_lacks "$t_out" "not an allowed account"

t_case "no approval by a person before: the fixer's label starts nothing"
fixer_ready -
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "no person approved an earlier cycle of it"

t_case "a person's approval whose start never ran is no base for the fixer"
fixer_ready -
past 7 alice label 2026-10-01T00:00:00Z --rc 4 --never-ran
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "no person approved an earlier cycle of it"

t_case "a body changed since the person's approval: the fixer's label starts nothing"
fixer_ready
jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Other words."}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "its text is not the text alice approved"

t_case "a busy store leaves the fixer's label for the next poll: no label comes off, nothing is said"
fixer_ready
store_refuses "approval-current 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: tehas:ready from the fixer waits: its approvals on the state store could not be read"
t_lacks "$(t_calls gh)" "--remove-label tehas:ready"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "the fixer's label on the control issue starts nothing"
fixer_ready
echo '[{"number":1,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"2026-10-02T00:00:00Z"}]' >"$fx/events-1.json"
past 1 alice label 2026-10-01T00:00:00Z
cp "$fx/edit-7.json" "$fx/edit-1.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" 'skip #1: the control issue is never worked on (`tehas:ready`)'

t_case "with no fixer on the instance, its account is an outsider like any other"
fixer_ready
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "clear #7: tehas:ready off, applied by 'tehas-fixer' who may not authorise work here"

t_case "a bot that is the fixer stops the poll: exit 78"
fixer_ready
echo '{"login":"tehas-fixer"}' >"$fx/user.json"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-fixer is TEHAS_MILLWRIGHT_LOGIN"
t_eq "$(t_calls fake-start)" ""

t_case "the fixer's marked comment before its label is the retry's context, whatever the comments setting"
fixer_ready
{
  echo '['
  icomment 81 tehas-fixer 2026-10-01T23:55:00Z 2026-10-01T23:55:00Z $'<!-- tehas:millwright issue=7 action=change -->\nRun the gate first.'
  echo ','
  icomment 80 tehas-fixer 2026-10-01T20:00:00Z 2026-10-01T20:00:00Z $'<!-- tehas:millwright issue=7 action=change -->\nAn older decision.'
  echo ','
  icomment 82 mallory 2026-10-01T23:58:00Z 2026-10-01T23:58:00Z $'<!-- tehas:millwright issue=7 action=change -->\nNot the fixer.'
  echo ','
  icomment 83 tehas-fixer 2026-10-01T23:59:00Z 2026-10-02T01:00:00Z $'<!-- tehas:millwright issue=7 action=change -->\nEdited after the label.'
  echo ']'
} >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(approval '.retry_context')" \
  "$(sorted '[{"source":"millwright:81","by":"tehas-fixer","at":"2026-10-01T23:55:00Z","text":"<!-- tehas:millwright issue=7 action=change -->\nRun the gate first."}]')"
t_eq "$(approval 'has("comment_links")')" false

t_case "the old fixer marker is no longer the retry's context"
fixer_ready
{
  echo '['
  icomment 84 tehas-fixer 2026-10-01T23:56:00Z 2026-10-01T23:56:00Z $'<!-- tehas:fixer issue=7 action=retry -->\nTry once more.'
  echo ']'
} >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(approval 'has("retry_context")')" false

t_case "a title changed by an outsider since the person's approval: the fixer's label starts nothing"
fixer_ready
echo '[{"event":"renamed","actor":{"login":"mallory"},"created_at":"2026-10-01T12:00:00Z"},{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"2026-10-02T00:00:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "its title or text was changed since alice approved it"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"

t_case "without such a comment the record has no context"
fixer_ready
t_run "$dispatch"
t_eq "$(approval 'has("retry_context")')" false

# --- The fixer's planning answers --------------------------------------------
# Issue 7 has a planning question open in a session alice started, on an
# instance with a fixer.
fmark='<!-- tehas:millwright issue=7 action=answer -->'
# The session has had as many turns as the last answer on record names.
fixer_planning() { # [<answers on record as JSON> [<who started the session>]]
  planning
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_MILLWRIGHT_LOGIN=tehas-fixer
  t_plan "$T/home/tehas" 7 "$(jq -nc --argjson a "${1:-[]}" --arg by "${2:-alice}" \
    '{by: $by, turns: ([1] + [$a[].turn] | max), options: ["Adopt all", "Change some"], comment: 4001, answers: $a}')"
}
freply() { # <body> [<id>]: one comment by the fixer, as JSON for comments
  jq -nc --arg b "$1" --argjson id "${2:-4003}" '[{databaseId: $id, author: {login: "tehas-fixer"}, body: $b, createdAt: "2026-01-01T02:00:00Z"}]'
}

t_case "a marked reply of the fixer's runs the turn, and the record says the fixer answered"
fixer_planning '[{"turn":2,"by":"tehas-fixer","fixer":true},{"turn":3,"by":"alice","fixer":false}]'
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply "$fmark"$'\nAdopt all, as alice chose in #12.')"
t_run "$dispatch"
t_contains "$(plan_calls)" "--answer $keep/answer.md --comment 4003"
t_eq "$(cat "$keep/answer.md")" 'The fixer replied on the issue:

Adopt all, as alice chose in #12.'
t_contains "$t_out" "planning #7 continues (answer from tehas-fixer, the fixer)"
t_eq "$(rec | jq -c '.answers[-1]')" '{"turn":4,"by":"tehas-fixer","fixer":true}'

t_case "a reply marked as the millwright's runs the turn, and the store is told the millwright answered"
fixer_planning
store_refuses none 1 none
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply '<!-- tehas:millwright issue=7 action=answer -->'$'\nAdopt all.')"
t_run "$dispatch"
t_contains "$(plan_calls)" "--answer $keep/answer.md --comment 4003"
t_contains "$(t_calls store)" "--millwright"
t_lacks "$(t_calls store)" "--fixer"

t_case "a box the fixer ticked runs the turn as its choice"
fixer_planning
comments 7 "$(plan_body ' ' x ' ')" tehas-fixer
t_run "$dispatch"
t_eq "$(cat "$keep/answer.md")" "The fixer chose: Change some"
t_lacks "$(plan_calls)" "--comment"
t_eq "$(rec | jq -c '.answers')" '[{"turn":2,"by":"tehas-fixer","fixer":true}]'

t_case "a person's reply wins over a newer reply of the fixer's, and a person's tick wins too"
fixer_planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(jq -c '[{databaseId: 4003, author: {login: "alice"}, body: "mine", createdAt: "2026-01-01T01:30:00Z"}] + .' <<<"$(freply "$fmark"$'\nAdopt all.' 4004)")"
t_run "$dispatch"
t_eq "$(cat "$keep/answer.md")" 'The operator replied on the issue:

mine'
t_eq "$(rec | jq -c '.answers')" '[{"turn":2,"by":"alice","fixer":false}]'
fixer_planning
comments 7 "$(plan_body ' ' x ' ')" alice "$(freply "$fmark"$'\nAdopt all.')"
t_run "$dispatch"
t_eq "$(cat "$keep/answer.md")" "The operator chose: Change some"

t_case "never two in a row: after the fixer's answer its next one runs no turn, and the issue says so once"
fixer_planning '[{"turn":2,"by":"tehas-fixer","fixer":true}]'
store_refuses none 1 none
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply "$fmark"$'\nAdopt all.')"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "skip #7: the fixer's answer runs no turn: the fixer answered the question before this one; a person answers this one"
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1
t_contains "$(t_calls gh)" "Tehas did not take the fixer's reply as an answer"
t_eq "$(rec | jq -c '.fixer_refused')" '[4003]'
t_contains "$(t_calls store)" "--kind millwright-refused --id 4003"
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$t_out" "the fixer's answer runs no turn"
t_lacks "$(t_calls gh)" "issue comment"

t_case "and a person's answer then runs the turn"
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(jq -c '. + [{databaseId: 4005, author: {login: "alice"}, body: "go", createdAt: "2026-01-01T03:00:00Z"}]' <<<"$(freply "$fmark"$'\nAdopt all.')")"
t_run "$dispatch"
t_contains "$(plan_calls)" "--comment 4005"

t_case "at the limit the fixer's tick runs no turn and is unticked with a note"
fixer_planning '[{"turn":2,"by":"tehas-fixer","fixer":true},{"turn":3,"by":"alice","fixer":false},{"turn":4,"by":"tehas-fixer","fixer":true},{"turn":5,"by":"alice","fixer":false},{"turn":6,"by":"tehas-fixer","fixer":true},{"turn":7,"by":"alice","fixer":false}]'
comments 7 "$(plan_body x ' ' ' ')" tehas-fixer
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "the fixer has answered 3 questions in this session, the limit"
t_contains "$(t_calls gh)" "api -X PATCH"
t_contains "$(t_calls gh)" "The fixer's tick was not taken as an answer"

t_case "a larger limit lets it answer again"
comments 7 "$(plan_body x ' ' ' ')" tehas-fixer
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_MILLWRIGHT_LOGIN=tehas-fixer TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=4
t_run "$dispatch"
t_eq "$(cat "$keep/answer.md")" "The fixer chose: Adopt all"

t_case "the fixer may not end a session: its tick on the way out is undone and the session goes on"
fixer_planning
comments 7 "$(plan_body ' ' ' ' x)" tehas-fixer
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "Planning session ended"
t_contains "$t_out" "it ticked End this planning session, which only a person may"
t_contains "$(t_calls gh)" "api -X PATCH"
t_eq "$(rec_open)" open

t_case "the fixer cannot end a session with an empty marked reply beside its tick on the way out"
fixer_planning
comments 7 "$(plan_body ' ' ' ' x)" tehas-fixer "$(freply "$fmark")"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "Planning session ended"
t_lacks "$(t_calls gh)" "--remove-label tehas:plan"
t_contains "$t_out" "it ticked End this planning session, which only a person may"
t_eq "$(rec_open)" open

t_case "nor with a marked reply that says something beside that tick"
fixer_planning
comments 7 "$(plan_body ' ' ' ' x)" tehas-fixer "$(freply "$fmark"$'\nDone, end it.')"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "it ticked End this planning session, which only a person may"
t_eq "$(rec_open)" open

t_case "a tick the fixer saved after alice edited the question is held for a person, not unticked"
fixer_planning
comments 7 "$(plan_body ' ' x ' ')" tehas-fixer "$(freply "$fmark"$'\nAdopt all.')"
jq '.data.repository.issue.comments.nodes[0].userContentEdits = {totalCount: 2, nodes: [{editor: {login: "alice"}}, {editor: {login: "tehas-fixer"}}]}' \
  "$fx/comments-7.json" >"$fx/c.json" && mv "$fx/c.json" "$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "api -X PATCH"
t_contains "$t_out" "skip #7: the fixer saved the question after another account did, so who ticked it is not known"
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1
t_contains "$(t_calls gh)" "A person ticks the box again or replies"
t_eq "$(rec | jq -c .held)" '[4001]'
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue comment"

t_case "and alice's tick saved again runs the turn as hers"
jq '.data.repository.issue.comments.nodes[0].editor.login = "alice"' "$fx/comments-7.json" >"$fx/c.json" && mv "$fx/c.json" "$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(cat "$keep/answer.md")" "The operator chose: Change some"

t_case "a held tick on a parked cycle's question starts nothing and stays"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_MILLWRIGHT_LOGIN=tehas-fixer
comments 7 "$(park_body x ' ')" tehas-fixer
jq '.data.repository.issue.comments.nodes[0].userContentEdits = {totalCount: 2, nodes: [{editor: {login: "alice"}}, {editor: {login: "tehas-fixer"}}]}' \
  "$fx/comments-7.json" >"$fx/c.json" && mv "$fx/c.json" "$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "api -X PATCH"

t_case "a reply of the fixer's without its marker runs no turn"
fixer_planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply 'Adopt all.')"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "its reply does not start with <!-- tehas:millwright issue=7 action=answer -->"

t_case "in a session started by an account no longer allowed, the fixer answers nothing"
fixer_planning '[]' mallory
comments 7 "$(plan_body ' ' x ' ')" tehas-fixer
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "no person's tehas:plan started this session"

t_case "a dry run says the fixer's reply would be refused and changes nothing"
fixer_planning '[{"turn":2,"by":"tehas-fixer","fixer":true}]'
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply "$fmark"$'\nAdopt all.')"
before=$(rec)
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: the fixer's answer runs no turn"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(rec)" "$before"

t_case "with no fixer on the instance, its account's tick and reply are an outsider's"
fixer_planning
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1
comments 7 "$(plan_body x ' ' ' ')" tehas-fixer "$(freply "$fmark"$'\nAdopt all.')"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "boxes ticked by 'tehas-fixer' are no answer (stranger)"
t_contains "$(t_calls gh)" "may not answer for this factory (tehas-fixer)"

t_case "the fixer's tick on a parked cycle's question is a stranger's"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_MILLWRIGHT_LOGIN=tehas-fixer
comments 7 "$(park_body x ' ')" tehas-fixer
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "boxes ticked by 'tehas-fixer' are no answer (stranger)"

t_done
