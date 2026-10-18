#!/usr/bin/env bash
# Tests for libexec/dispatch: the fixer's labels and its planning answers.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# --- The fixer ---------------------------------------------------------------
# Issue 7 is labelled by the fixer, its body is "Words.", and alice approved
# its last cycle on that body.
fixer_ready() { # [<approval record of the last start>]
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_FIXER_LOGIN=tehas-fixer
  echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:fixer-escalated"}]}]' >"$fx/ready.json"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"2026-10-02T00:00:00Z"}]' >"$fx/events-7.json"
  jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Words."}}}}' >"$fx/edit-7.json"
  echo '[]' >"$fx/icomments-7.json"
  mkdir -p "$T/home/tehas/state/cycles"
  printf '%s\n' "${1:-{\"at\":\"2026-10-01T00:00:00Z\",\"by\":\"alice\",\"how\":\"label\",\"body_sha256\":\"$WSUM\",\"links\":[]\}}" >"$approved_file"
}

t_case "the fixer's tehas:ready on an issue alice approved starts a cycle that rests on her approval"
fixer_ready
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -c '[.by, .how, .fixer_starts, .rests_on.by, .rests_on.at, .rests_on.how, (.rests_on.body_sha256 == .body_sha256)]' "$approved_file")" \
  '["tehas-fixer","fixer",1,"alice","2026-10-01T00:00:00Z","label",true]'
t_contains "$t_out" "start #7 (labelled by the fixer by tehas-fixer)"
t_contains "$(t_calls gh)" "authorized by @tehas-fixer's \`tehas:ready\` label as the fixer, on alice's approval of 2026-10-01T00:00:00Z"

t_case "the start takes tehas:fixer-escalated off with the rest of the earlier outcome"
t_contains "$(t_calls gh)" "--remove-label tehas:ready --remove-label tehas:question --remove-label tehas:fixer-escalated"

t_case "a second fixer start counts on from the first and keeps the person's approval"
fixer_ready "$(jq -c '.' "$approved_file")"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -c '[.how, .fixer_starts, .rests_on.by]' "$approved_file")" '["fixer",2,"alice"]'

t_case "at the limit the fixer's label comes off, and the issue says a person decides"
fixer_ready "$(jq -c '.fixer_starts = 2' "$approved_file")"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: tehas:ready from the fixer starts nothing: the fixer has started 2 cycles on it since alice approved it, the limit"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_contains "$(t_calls gh)" "The fixer applied it, and the fixer may not start this issue"
t_lacks "$t_out" "not an allowed account"

t_case "no approval by a person before: the fixer's label starts nothing"
fixer_ready
rm -f "$approved_file"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "no person approved an earlier cycle of it"

t_case "a body changed since the person's approval: the fixer's label starts nothing"
fixer_ready
jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Other words."}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "its text is not the text alice approved"

t_case "the fixer's label on the control issue starts nothing"
fixer_ready
echo '[{"number":1,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"2026-10-02T00:00:00Z"}]' >"$fx/events-1.json"
cp "$approved_file" "$T/home/tehas/state/cycles/1.approved.json"
cp "$fx/edit-7.json" "$fx/edit-1.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #1: tehas:ready from the fixer starts nothing: it is the control issue"

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
t_contains "$t_out" "the bot account tehas-fixer is TEHAS_FIXER_LOGIN"
t_eq "$(t_calls fake-start)" ""

t_case "the fixer's marked comment before its label is the retry's context, whatever the comments setting"
fixer_ready
{
  echo '['
  icomment 81 tehas-fixer 2026-10-01T23:55:00Z 2026-10-01T23:55:00Z $'<!-- tehas:fixer issue=7 action=change -->\nRun the gate first.'
  echo ','
  icomment 80 tehas-fixer 2026-10-01T20:00:00Z 2026-10-01T20:00:00Z $'<!-- tehas:fixer issue=7 action=change -->\nAn older decision.'
  echo ','
  icomment 82 mallory 2026-10-01T23:58:00Z 2026-10-01T23:58:00Z $'<!-- tehas:fixer issue=7 action=change -->\nNot the fixer.'
  echo ','
  icomment 83 tehas-fixer 2026-10-01T23:59:00Z 2026-10-02T01:00:00Z $'<!-- tehas:fixer issue=7 action=change -->\nEdited after the label.'
  echo ']'
} >"$fx/icomments-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(jq -c '.retry_context' "$approved_file")" \
  '[{"source":"fixer:81","by":"tehas-fixer","at":"2026-10-01T23:55:00Z","text":"<!-- tehas:fixer issue=7 action=change -->\nRun the gate first."}]'
t_eq "$(jq -c 'has("comment_links")' "$approved_file")" false

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
t_eq "$(jq -c 'has("retry_context")' "$approved_file")" false

# --- The fixer's planning answers --------------------------------------------
# Issue 7 has a planning question open in a session alice started, on an
# instance with a fixer.
fmark='<!-- tehas:fixer issue=7 action=answer -->'
fixer_planning() { # [<answers on record as JSON>]
  planning
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_FIXER_LOGIN=tehas-fixer
  jq --argjson a "${1:-[]}" '.by = "alice" | .answers = $a' "$record" >"$record.new" && mv "$record.new" "$record"
}
freply() { # <body> [<id>]: one comment by the fixer, as JSON for comments
  jq -nc --arg b "$1" --argjson id "${2:-4003}" '[{databaseId: $id, author: {login: "tehas-fixer"}, body: $b, createdAt: "2026-01-01T02:00:00Z"}]'
}

t_case "a marked reply of the fixer's runs the turn, and the record says the fixer answered"
fixer_planning '[{"turn":1,"by":"tehas-fixer","fixer":true},{"turn":2,"by":"alice","fixer":false}]'
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply "$fmark"$'\nAdopt all, as alice chose in #12.')"
t_run "$dispatch"
t_contains "$(plan_calls)" "--answer $record.answer --comment 4003"
t_eq "$(cat "$record.answer")" 'The fixer replied on the issue:

Adopt all, as alice chose in #12.'
t_contains "$t_out" "planning #7 continues (answer from tehas-fixer, the fixer)"
t_eq "$(jq -c '.answers[-1]' "$record")" '{"turn":2,"by":"tehas-fixer","fixer":true}'

t_case "a box the fixer ticked runs the turn as its choice"
fixer_planning
comments 7 "$(plan_body ' ' x ' ')" tehas-fixer
t_run "$dispatch"
t_eq "$(cat "$record.answer")" "The fixer chose: Change some"
t_lacks "$(plan_calls)" "--comment"
t_eq "$(jq -c '.answers' "$record")" '[{"turn":2,"by":"tehas-fixer","fixer":true}]'

t_case "a person's reply wins over a newer reply of the fixer's, and a person's tick wins too"
fixer_planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(jq -c '[{databaseId: 4003, author: {login: "alice"}, body: "mine", createdAt: "2026-01-01T01:30:00Z"}] + .' <<<"$(freply "$fmark"$'\nAdopt all.' 4004)")"
t_run "$dispatch"
t_eq "$(cat "$record.answer")" 'The operator replied on the issue:

mine'
t_eq "$(jq -c '.answers' "$record")" '[{"turn":2,"by":"alice","fixer":false}]'
fixer_planning
comments 7 "$(plan_body ' ' x ' ')" alice "$(freply "$fmark"$'\nAdopt all.')"
t_run "$dispatch"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"

t_case "never two in a row: after the fixer's answer its next one runs no turn, and the issue says so once"
fixer_planning '[{"turn":2,"by":"tehas-fixer","fixer":true}]'
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply "$fmark"$'\nAdopt all.')"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "skip #7: the fixer's answer runs no turn: the fixer answered the question before this one; a person answers this one"
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1
t_contains "$(t_calls gh)" "Tehas did not take the fixer's reply as an answer"
t_eq "$(jq -c '.fixer_refused' "$record")" '[4003]'
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
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_FIXER_LOGIN=tehas-fixer TEHAS_FIXER_MAX_PLAN_ANSWERS=4
t_run "$dispatch"
t_eq "$(cat "$record.answer")" "The fixer chose: Adopt all"

t_case "the fixer may not end a session: its tick on the way out is undone and the session goes on"
fixer_planning
comments 7 "$(plan_body ' ' ' ' x)" tehas-fixer
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "Planning session ended"
t_contains "$t_out" "it ticked End this planning session, which only a person may"
t_contains "$(t_calls gh)" "api -X PATCH"
t_eq "$([ -e "$record" ] && echo kept)" kept

t_case "the fixer cannot end a session with an empty marked reply beside its tick on the way out"
fixer_planning
comments 7 "$(plan_body ' ' ' ' x)" tehas-fixer "$(freply "$fmark")"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "Planning session ended"
t_lacks "$(t_calls gh)" "--remove-label tehas:plan"
t_contains "$t_out" "it ticked End this planning session, which only a person may"
t_eq "$([ -e "$record" ] && echo kept)" kept

t_case "nor with a marked reply that says something beside that tick"
fixer_planning
comments 7 "$(plan_body ' ' ' ' x)" tehas-fixer "$(freply "$fmark"$'\nDone, end it.')"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "it ticked End this planning session, which only a person may"
t_eq "$([ -e "$record" ] && echo kept)" kept

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
t_eq "$(jq -c .held "$record")" '[4001]'
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue comment"

t_case "and alice's tick saved again runs the turn as hers"
jq '.data.repository.issue.comments.nodes[0].editor.login = "alice"' "$fx/comments-7.json" >"$fx/c.json" && mv "$fx/c.json" "$fx/comments-7.json"
t_run "$dispatch"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"

t_case "a held tick on a parked cycle's question starts nothing and stays"
parked
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_FIXER_LOGIN=tehas-fixer
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
t_contains "$t_out" "its reply does not start with <!-- tehas:fixer issue=7 action=answer -->"

t_case "in a session with no starter on record, or one started by an account no longer allowed, the fixer answers nothing"
for rec in 'del(.by)' '.by = "mallory"'; do
  fixer_planning
  jq "$rec" "$record" >"$record.new" && mv "$record.new" "$record"
  comments 7 "$(plan_body ' ' x ' ')" tehas-fixer
  t_run "$dispatch"
  t_eq "$(plan_calls)" ""
  t_contains "$t_out" "no person's tehas:plan started this session"
done

t_case "a dry run says the fixer's reply would be refused and changes nothing"
fixer_planning '[{"turn":2,"by":"tehas-fixer","fixer":true}]'
comments 7 "$(plan_body ' ' ' ' ' ')" "" "$(freply "$fmark"$'\nAdopt all.')"
before=$(cat "$record")
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: the fixer's answer runs no turn"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(cat "$record")" "$before"

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
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_FIXER_LOGIN=tehas-fixer
comments 7 "$(park_body x ' ')" tehas-fixer
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "boxes ticked by 'tehas-fixer' are no answer (stranger)"

t_done
