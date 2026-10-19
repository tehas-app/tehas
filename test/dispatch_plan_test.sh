#!/usr/bin/env bash
# Tests for libexec/dispatch: planning sessions, their turns and answers.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "a plan label from an allowed account starts a session from the issue's text"
to_plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --brief $record.brief"
t_eq "$(cat "$record.brief")" '# Faster search

Make it fast.'
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:plan"
t_contains "$(t_calls gh)" "authorized by @alice's \`tehas:plan\` label"
t_contains "$(t_calls gh)" "- [ ] Adopt all"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(jq -c . "$record")" '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"running":false,"by":"alice","comment":4002}'
t_eq "$([ -e "$record.out" ] && echo left)" ""
t_eq "$(t_calls fake-start)" ""

t_case "an issue with no body is planned from its title"
to_plan
echo '{"title":"Faster search","body":null}' >"$fx/issue-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"
t_eq "$(head -n 1 "$record.brief")" "# Faster search"

t_case "a plan label from the bot or a stranger starts nothing and is taken off the issue"
for other in tehas-bot mallory; do
  to_plan
  echo "[{\"event\":\"labeled\",\"label\":{\"name\":\"tehas:plan\"},\"actor\":{\"login\":\"$other\"}}]" >"$fx/events-7.json"
  t_run "$dispatch"
  t_eq "$(plan_calls)" ""
  t_contains "$t_out" "skip #7: tehas:plan was applied by '$other', not an allowed account"
  t_contains "$t_out" "clear #7: tehas:plan off, applied by '$other' who may not authorise work here"
  t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --remove-label tehas:plan')" 1
  t_eq "$(t_calls gh | grep -c -e '^issue comment ')" 1
done

t_case "a dry run leaves a stranger's plan label where it is"
to_plan
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would take tehas:plan off #7: it was applied by 'mallory'"
t_lacks "$(t_calls gh)" "issue edit"

t_case "text changed after the plan label by somebody else is not planned"
to_plan
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T00:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""

t_case "a dry run says a session would start and changes nothing"
to_plan
t_run "$dispatch --dry-run"
t_contains "$t_out" "would start a planning session for #7 (labelled by alice)"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "planning is not held back by pull requests that await a merge"
to_plan
echo '[{"number":1},{"number":2},{"number":3}]' >"$fx/awaiting.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"

t_case "the poll's lock is not handed to a planning turn"
t_eq "$(find "$T/calls/plan.fd8" 2>/dev/null | wc -l)" 0

t_case "the pause label stops planning"
to_plan
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""

t_case "a session does not start when the issue cannot be marked as planning"
to_plan
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "ERROR: could not mark #7 as planning, not starting a turn"
t_eq "$([ -e "$record" ] && echo kept)" ""
rm -f "$fx/edit-fails"

t_case "the turn is on record as running before the issue is marked"
to_plan
t_run "$dispatch"
t_eq "$(jq -c 'del(.startedAt)' "$T/calls/record.at-mark")" '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true,"by":"alice"}'
t_eq "$(jq -r '.startedAt | test("^[0-9-]+T[0-9:]+Z$")' "$T/calls/record.at-mark")" true

t_case "a session does not start when its record cannot be written, and the issue is not marked"
to_plan
mkdir -p "$record.new"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not write the record of #7, not starting a turn"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
rm -rf "$record.new"

t_case "a later turn that cannot be marked leaves the record as it was, still answerable"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
before=$(cat "$record")
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(plan_calls)" ""
t_eq "$(cat "$record")" "$before"
t_eq "$(jq -r .running "$T/calls/record.at-mark")" true
rm -f "$T"/calls/* "$fx/edit-fails"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"

t_case "while a later turn runs the record names no question"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_eq "$(jq -c 'del(.startedAt)' "$T/calls/record.at-turn")" '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"running":true,"answers":[{"turn":2,"by":"alice","fixer":false}]}'
t_eq "$(jq -r '.startedAt | test("^[0-9-]+T[0-9:]+Z$")' "$T/calls/record.at-turn")" true

t_case "who started a session and who answered each turn stay on record after the turn"
planning
jq '.by = "alice" | .answers = [{"turn": 1, "by": "bob", "fixer": false}]' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_eq "$(jq -c '[.turns, .by, .answers]' "$record")" '[2,"alice",[{"turn":1,"by":"bob","fixer":false},{"turn":2,"by":"alice","fixer":false}]]'

t_case "a turn that fails keeps them too"
planning
jq '.by = "alice"' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body ' ' x ' ')" alice
: >"$fx/plan-fails"
echo '{"session":"abc-123"}' >"$fx/plan-fails"
t_run "$dispatch"
t_eq "$(jq -c '[.failed, .by, .answers]' "$record")" '[true,"alice",[{"turn":2,"by":"alice","fixer":false}]]'
rm -f "$fx/plan-fails"

t_case "answers on record that are not a list stay as they are"
planning
jq '.answers = "lots"' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(plan_calls)" "--answer $record.answer"
t_eq "$(jq -c .answers "$record")" '"lots"'

t_case "a turn cut off before the issue was marked is reported like any cut-off turn"
planning
jq '.running = true | del(.comment)' "$record" >"$record.new" && mv "$record.new" "$record"
# The mark never happened: the issue still asks, and the old answer is there.
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it was cut off"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention"
t_eq "$(jq -c . "$record")" '{"session":"abc-123","turns":2,"options":[],"pending":false,"failed":true,"running":false,"comment":4002}'
t_eq "$(plan_calls)" ""

t_case "a first turn cut off before the issue was marked ends visibly and takes the plan label off"
to_plan
mkdir -p "$T/home/tehas/state"
echo '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}' >"$record"
# What GitHub answers once the label is off.
echo '[]' >"$fx/plans.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:plan --remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""
t_eq "$(plan_calls)" ""

t_case "a first turn that fails parks the issue and keeps no record"
to_plan
: >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""

# The record goes, and with it the session file a person's export of this turn
# would read. lib/planning.sh leaves a copy of the record as the turn left it,
# under a name of its own, so the archive of a turn that made no session is not
# the line of history alone. tehas export_test.sh checks what tehas export does
# with that name; here is the turn that leaves it.
t_case "a first turn that fails leaves the copy of the record an export reads"
to_plan
: >"$fx/plan-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(jq -c '{session,turns,running}' "$T/home/tehas/state/plan-7.turn")" '{"session":"","turns":0,"running":true}'
t_eq "$([ -e "$record" ] && echo kept)" ""

# A turn of a session that went on leaves no copy of its own: the live record is
# the whole session, so there is nothing else to take.
t_case "a turn of a session that went on leaves no copy of the record"
to_plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/home/tehas/state/plan-7.turn" ] && echo left)" ""
t_eq "$(jq -r .session "$record")" abc-123

t_case "a first turn that fails after its session came into being keeps the session"
to_plan
echo '{"session":"abc-123"}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it exited with status 2."
t_contains "$(t_calls gh)" "Reply in a comment to run the turn again"
t_lacks "$(t_calls gh)" "could not start"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention"
t_eq "$(jq -c . "$record")" '{"session":"abc-123","turns":1,"options":[],"pending":false,"failed":true,"running":false,"by":"alice","comment":4002}'

t_case "and a reply runs the turn again in that session"
rm -f "$T"/calls/* "$fx/plan-fails"
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
echo '[]' >"$fx/plans.json"
jq -n '{data: {repository: {issue: {comments: {nodes: [
  {databaseId: 4002, author: {login: "tehas-bot"}, createdAt: "2026-01-01T00:00:00Z", lastEditedAt: null, editor: null,
   body: "This planning turn did not finish.\n\n- [ ] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=1 -->"},
  {databaseId: 4003, author: {login: "alice"}, body: "go on", createdAt: "2026-01-01T02:00:00Z"}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"

t_case "a session id that is not a string in a failed turn's file is no session"
to_plan
echo '{"session":{"a":1}}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "the record lists the options that were posted, so a box and its option keep the same place"
to_plan
jq -n '{session: "abc-123", text: "Many ways.", question: {question: "Which?", options: [range(30) | {label: "Way \(.)", description: ""}]}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$(jq -c '[(.options | length), .options[0], .options[19]]' "$record")" '[20,"Way 0","Way 19"]'
t_eq "$(t_calls gh | grep -c '^- \[ \] Way ')" 20
t_contains "$(t_calls gh)" "Options left out: 10."

t_case "questions posted as one have no option boxes, so a tick answers none of them"
to_plan
jq -n '{session: "abc-123", text: "Two things.", question: {question: "The agent asked 2 questions. Answer each by its number in a reply.\n\n1. Which?\n   - A\n\n2. Where?\n   - B", options: []}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$(jq -c .options "$record")" "[]"
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
t_eq "$(jq -c .filed "$record")" '["https://github.com/acme/widget/issues/42","https://github.com/acme/widget/issues/43"]'

t_case "the next turn of a session that filed is told not to file them again"
planning
jq '.filed = ["https://github.com/acme/widget/issues/42", "https://github.com/acme/widget/issues/43"]' "$record" >"$record.new" && mv "$record.new" "$record"
plan_files
comments 7 "$(plan_filed_body 1 https://github.com/acme/widget/issues/42 https://github.com/acme/widget/issues/43)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer.new"
t_eq "$(cat "$record.answer.new")" "Already filed in this session: https://github.com/acme/widget/issues/42, https://github.com/acme/widget/issues/43

The operator chose: Change some"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"

t_case "what a later turn files is added to what the session filed before"
planning
jq '.filed = ["https://github.com/acme/widget/issues/41"]' "$record" >"$record.new" && mv "$record.new" "$record"
plan_files https://github.com/acme/widget/issues/43 https://github.com/acme/widget/issues/42
comments 7 "$(plan_filed_body 1 https://github.com/acme/widget/issues/41)" alice
t_run "$dispatch"
t_eq "$(jq -c .filed "$record")" '["https://github.com/acme/widget/issues/41","https://github.com/acme/widget/issues/42","https://github.com/acme/widget/issues/43"]'
t_eq "$(t_calls gh | grep -c -F -e 'Filed in this turn: https://github.com/acme/widget/issues/42, https://github.com/acme/widget/issues/43')" 1

t_case "a turn that filed nothing names none, and the next answer says nothing of filings"
planning
plan_files
comments 7 "$(plan_filed_body 1)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"
t_lacks "$(t_calls gh)" "Filed in this turn"
jq -e 'has("filed") | not' "$record" >/dev/null || t_bad "the record has a filed key"

t_case "of what the record says was filed, only GitHub issue addresses are named, each once"
planning
jq '.filed = ["https://github.com/acme/widget/issues/42", 7, "https://evil.example/acme/widget/issues/8", "https://github.com/acme/widget/issues/42"]' "$record" >"$record.new" && mv "$record.new" "$record"
plan_files
comments 7 "$(plan_filed_body 1 https://github.com/acme/widget/issues/42)" alice
t_run "$dispatch"
t_eq "$(head -n 1 "$record.answer.new")" "Already filed in this session: https://github.com/acme/widget/issues/42"
t_lacks "$(cat "$record.answer.new")" "evil.example"

t_case "a ticked option goes to the session as the next message"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=2 -->"
t_eq "$(jq -r .turns "$record")" 2

t_case "a reply goes to the session as it was written"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"Adopt all, but keep `tagged-at` open.\nThen file 3 and 5.","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(cat "$record.answer")" 'The operator replied on the issue:

Adopt all, but keep `tagged-at` open.
Then file 3 and 5.'

t_case "a reply's turn is given the reply's comment id, so its attachments go with it"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer --comment 4003"

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
t_eq "$(cat "$record.answer")" 'The operator replied on the issue:

actually, wait

A box was also ticked, by alice: Adopt all'
t_contains "$t_out" "planning #7 continues (answer from bob)"

t_case "a reply with the way out ticked runs the turn and says the box was ticked"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice '[{"databaseId":4003,"author":{"login":"alice"},"body":"one more thing","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_contains "$(plan_calls)" "--answer $record.answer"
t_contains "$(cat "$record.answer")" "A box was also ticked, by alice: End this planning session"

t_case "a reply alone says nothing of a box"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" "" '[{"databaseId":4003,"author":{"login":"alice"},"body":"just this","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_lacks "$(cat "$record.answer")" "A box was also ticked"

t_case "an answer on an issue closed since the turn ran runs no turn"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
before=$(cat "$record")
echo '{"number":7,"state":"closed"}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: it is closed, and a closed issue is never taken"
t_eq "$(plan_calls)" ""
t_eq "$(cat "$record")" "$before"
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a stranger's reply and a stranger's tick run no turn"
planning
comments 7 "$(plan_body x ' ' ' ')" mallory '[{"databaseId":4003,"author":{"login":"mallory"},"body":"file everything","createdAt":"2026-01-01T02:00:00Z"}]'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "api -X PATCH"

t_case "the way out ends the session: labels off, record gone, one comment"
planning
mkdir -p "$record.keep/images"
comments 7 "$(plan_body ' ' ' ' x)" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "--remove-label tehas:question"
t_contains "$(t_calls gh)" "Planning session ended by alice."
t_eq "$([ -e "$record" ] && echo kept)" ""
t_eq "$([ -e "$record.keep" ] && echo kept)" ""

t_case "the turn limit ends the session in place of another turn"
planning
jq '.turns = 20' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "reached its limit of 20 turns"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "an answer to a session whose record is gone ends it and says why"
planning
rm "$record"
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
t_eq "$(jq -r '[.session, .turns, .failed] | @tsv' "$record")" "abc-123	2	true"

t_case "a later turn whose Pi session is gone ends the session and says to start again"
planning
mkdir -p "$record.keep"
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123"
t_contains "$t_out" "planning #7: Pi no longer has session abc-123"
t_contains "$(t_calls gh)" "--remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention"
t_contains "$(t_calls gh)" "its conversation is no longer on"
t_contains "$(t_calls gh)" "Apply \`tehas:plan\` again to start a new session."
t_lacks "$(t_calls gh)" "This planning turn did not finish"
t_lacks "$(t_calls gh)" "tehas:question kind=plan issue=7 turn=2"
t_eq "$([ -e "$record" ] || [ -e "$record.keep" ] && echo kept)" ""

t_case "a lost session whose labels cannot be taken off is ended by the next poll"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
echo '{"session":"abc-123","lost":true}' >"$fx/plan-fails"
echo "--remove-label tehas:question --remove-label tehas:planning" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(jq -c '[.lost, .running]' "$record")" '[true,true]'
t_lacks "$(t_calls gh)" "its conversation is no longer on"
rm -f "$T"/calls/* "$fx/edit-fails-on" "$fx/plan-fails"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "its conversation is no longer on"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "an outcome that could not be posted is posted by the next poll, before anything else"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
: >"$fx/post-fails"
t_run "$dispatch"
t_eq "$(jq -r .pending "$record")" true
t_eq "$([ -s "$record.out" ] && echo kept)" kept
t_lacks "$(t_calls gh)" "--add-label tehas:question"
rm -f "$T"/calls/* "$fx/post-fails"
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=2 -->"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(jq -r .pending "$record")" false

t_case "the record of a session whose issue no longer asks is removed"
planning
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$([ -e "$record" ] && echo kept)" ""
t_lacks "$(t_calls gh)" "issue comment"

t_case "an answered question is served before a new plan label and before a ready issue"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
echo '[{"number":8,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"number":9,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out"
t_lacks "$(plan_calls)" "8 --out"
t_eq "$(t_calls fake-start)" ""

# One issue is in one flow at a time.
t_case "tehas:ready on an issue with an open planning session starts no cycle"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]}]' >"$fx/ready.json"
before=$(cat "$record")
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_eq "$(cat "$record")" "$before"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_lacks "$t_out" "would run a cycle"

t_case "tehas:ready on an issue whose planning turn is marked as running starts no cycle"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:planning"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"

t_case "an issue with a planning session does not hold up the ready issue behind it"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]},{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
cp "$fx/events-7.json" "$fx/events-8.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
rm -f "$fx/events-8.json"

# The analytics program, where install.sh puts it. A call notes the record as
# the program would read it.
plan_analytics() { # [<bash body>]
  t_stub_analytics '[ "$1" != plan ] || cp "$TEHAS_HOME/state/plan-7.json" "$T/calls/record.at-analytics" 2>/dev/null
'"${1:-}"
}
plan_analytics_calls() { t_calls analytics | grep '^plan' || true; }

t_case "a planning turn that ends is recorded in analytics, from its record before it is rewritten"
to_plan
rm -f "$fx/image.label"
plan_analytics
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0"
t_eq "$(jq -c '[.running, .turns, (.startedAt | type)]' "$T/calls/record.at-analytics")" '[true,0,"string"]'
t_eq "$(jq -c .turns "$record")" 1

t_case "with the pi-rukas commit of the image's label"
to_plan
echo 'abcdef1|1' >"$fx/image.label"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0 --rukas-image abcdef1"
rm -f "$fx/image.label"

t_case "a turn that fails and a turn cut off are recorded with how they ended"
to_plan
: >"$fx/plan-fails"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc 2"
rm -f "$fx/plan-fails"
planning
jq '.running = true | .startedAt = "2026-01-01T00:00:00Z" | del(.comment)' "$record" >"$record.new" && mv "$record.new" "$record"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc interrupted"

t_case "with TEHAS_ANALYTICS off a planning turn is not recorded"
to_plan
plan_analytics
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" ""
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"

t_case "a failing or hanging analytics changes nothing in the turn"
to_plan
t_run "$dispatch"
without_gh=$(t_calls gh)
without_record=$(jq -c . "$record")
for body in 'exit 70' 'exec sleep 30'; do
  to_plan
  plan_analytics "[ \"\$1\" != plan ] || $body"
  T_ENV+=(TEHAS_ANALYTICS_WAIT=1)
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_contains "$t_out" "WARN analytics plan: exit"
  t_eq "$(t_calls gh)" "$without_gh"
  t_eq "$(jq -c . "$record")" "$without_record"
done

t_done
