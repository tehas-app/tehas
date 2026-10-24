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
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --brief $T/home/tehas/tmp/plan-7.brief."
t_eq "$(rec_brief)" '# Faster search

Make it fast.'
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:plan"
t_contains "$(t_calls gh)" "authorized by @alice's \`tehas:plan\` label"
t_contains "$(t_calls gh)" "- [ ] Adopt all"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(rec | jq -c 'del(.startedAt)')" '{"session":"abc-123","turns":1,"options":["Adopt all","Change some"],"pending":false,"failed":false,"running":false,"comment":4002,"by":"alice","label":"tehas:plan","answers":[]}'
t_eq "$([ -e "$record.out" ] && echo left)" ""
# The brief was the turn's until it ended; the record keeps the text.
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-7.brief.*' | wc -l)" 0
t_eq "$(t_calls fake-start)" ""

t_case "an issue with no body is planned from its title"
to_plan
echo '{"title":"Faster search","body":null}' >"$fx/issue-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"
t_eq "$(rec_brief | head -n 1)" "# Faster search"

t_case "a refine label from an allowed account starts a session with the refine brief after the issue's text"
to_refine
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --brief $T/home/tehas/tmp/plan-7.brief."
t_eq "$(rec_brief | head -n 3)" '# Faster search

Make it fast.'
t_contains "$(rec_brief)" "---

$(cat "$T_ROOT/lib/refine-brief.md")"
t_eq "$(rec_brief | tail -n 1)" "This request is issue #7 itself. The issue you file replaces #7: #7 is not a duplicate of this plan."
t_contains "$t_out" "start planning #7 (labelled by alice, to refine it)"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:refine"
t_lacks "$(t_calls gh)" "--remove-label tehas:plan "
t_contains "$(t_calls gh)" "authorized by @alice's \`tehas:refine\` label"

t_case "a refining session keeps its label on record, in each line of history, and in the analytics how"
t_eq "$(rec | jq -r .label)" "tehas:refine"
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1].label')" "tehas:refine"

# A person who applies the label to a parked issue asks for a session, as
# tehas:ready there starts a cycle. tehas:question is no bar to the ask, and
# the start takes it off, so the park's boxes answer for nothing after it.
t_case "tehas:refine on a parked issue starts a session with the refine brief and takes tehas:question off"
parked_plan refine
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --brief $T/home/tehas/tmp/plan-7.brief."
t_contains "$(rec_brief)" "---

$(cat "$T_ROOT/lib/refine-brief.md")"
t_contains "$t_out" "start planning #7 (labelled by alice, to refine it)"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:refine --remove-label tehas:question --remove-label needs-human-attention"
# A retry that was not ticked answers for nothing, and the park's id goes with
# the label: the boxes of the old park must not answer for the new session.
t_eq "$(park_q)" none
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1].label')" "tehas:refine"

t_case "tehas:plan on a parked issue starts a session from the issue's text, and needs-human-attention is the cycle's to clear"
parked_plan plan
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --brief $T/home/tehas/tmp/plan-7.brief."
t_eq "$(rec_brief | head -n 3)" '# Faster search

Make it fast.'
t_lacks "$(rec_brief)" "$(head -c 40 "$T_ROOT/lib/refine-brief.md")"
t_lacks "$(rec_brief)" "is not a duplicate of this plan"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:plan --remove-label tehas:question --remove-label needs-human-attention"
t_contains "$(t_calls gh)" "authorized by @alice's \`tehas:plan\` label"
t_eq "$(park_q)" none

t_case "a dry run of a session asked for on a parked issue says so and changes nothing"
parked_plan refine
t_run "$dispatch --dry-run"
t_contains "$t_out" "would start a planning session for #7 (labelled by alice, to refine it)"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"
t_eq "$(park_q)" 4001

t_case "a session whose start the store will not take does not start, and its park's question stays"
parked_plan refine
store_refuses "plan-begin 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not write the record of #7, not starting a turn (busy)"
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-7.brief.*' | wc -l)" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(park_q)" 4001

# tehas:running stands for a cycle this poll has no part in stopping; a session
# of its own is what tehas:planning holds off.
for bar in tehas:running tehas:planning; do
  t_case "an issue that carries $bar is not planned, and nothing of it is changed"
  parked_plan refine
  jq -n --arg l "$bar" '[{number: 7, labels: [{name: "needs-human-attention"}, {name: "tehas:question"},
    {name: "tehas:refine"}, {name: $l}]}]' >"$fx/asking.json"
  jq -n --arg l "$bar" '[{number: 7, labels: [{name: "tehas:refine"}, {name: $l}]}]' >"$fx/refines.json"
  t_run "$dispatch"
  t_eq "$(plan_calls)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "issue comment"
  t_eq "$(park_q)" 4001
done

# tehas:question excludes none of them; the park's boxes are read by rule 4
# alone, which runs first and ends the poll, so a ticked retry still gets its
# cycle and its tick is undone with the start. The ask for a session is left for
# the poll after the cycle, as tehas:ready is.
t_case "a ticked retry of a parked issue starts its cycle, whatever planning label it carries"
parked_plan refine
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" 7
t_eq "$(plan_calls)" ""
t_eq "$(park_q)" none
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question"
t_lacks "$(t_calls gh)" "--remove-label tehas:refine"
# The tick is undone with the start, so it cannot fire again later.
t_contains "$(t_calls gh)" "api -X PATCH repos/acme/widget/issues/comments/4001"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
# The ask for a session waits for the poll after the cycle, as tehas:ready does
# behind a running cycle: its label stays, and with it the question.
t_lacks "$(t_calls gh)" "--remove-label tehas:refine"

t_case "a refining turn that files an issue notes it in the ledger of refined issues; a planning one does not"
to_refine
plan_files https://github.com/acme/widget/issues/41
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.source, .filed, .url]' "$T/home/tehas/state/refined.jsonl")" '[7,41,"https://github.com/acme/widget/issues/41"]'
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1].filed | tostring')" '["https://github.com/acme/widget/issues/41"]'
rm -f "$T/home/tehas/state/refined.jsonl"
to_plan
plan_files https://github.com/acme/widget/issues/42
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/home/tehas/state/refined.jsonl" ] && echo written)" ""
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1].label')" "tehas:plan"

t_case "the ledger of refined issues keeps its last 500 lines, as the ledger of sizes does"
to_refine
ledger=$T/home/tehas/state/refined.jsonl
rm -f "$ledger"
for i in $(seq 1 500); do printf '{"at":"x","source":0,"filed":%s,"url":"u"}\n' "$i" >>"$ledger"; done
t_eq "$(wc -l <"$ledger")" 500
plan_files https://github.com/acme/widget/issues/41
t_run "$dispatch"
t_eq "$t_rc" 0
# The new line is on it, and the file is no longer than the 500 it keeps.
t_eq "$(wc -l <"$ledger")" 500
t_eq "$(jq -sc '[.[].filed] | [first, last]' "$ledger")" "[2,41]"
rm -f "$ledger"

t_case "the refine brief asks for one part sized for one cycle, grounded in the code, with exact paths"
brief=$(cat "$T_ROOT/lib/refine-brief.md")
t_contains "$brief" "Ground it in the code first"
t_contains "$brief" "part only, the one the others build on"
t_contains "$brief" "[paths: \`a\`, \`b\`]"
t_contains "$brief" "\`.github/\` or \`.pi/\`"
t_contains "$brief" "Classify the part in its title, as a conventional-commit type"
t_contains "$brief" "Keep a type the
   request's title already has."
t_eq "$([ "$(wc -c <"$T_ROOT/lib/refine-brief.md")" -lt 4000 ] && echo short)" short

t_case "a refine label from a stranger starts nothing and is taken off the issue"
to_refine
echo '[{"event":"labeled","label":{"name":"tehas:refine"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "skip #7: tehas:refine was applied by 'mallory', not an allowed account"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --remove-label tehas:refine')" 1

t_case "an issue with both labels is planned, not refined"
to_refine
echo '[{"number":7,"labels":[{"name":"tehas:plan"},{"name":"tehas:refine"}]}]' >"$fx/plans.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"},{"name":"tehas:refine"}]}]' >"$fx/refines.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}},{"event":"labeled","label":{"name":"tehas:refine"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(plan_calls | grep -c -e '--brief')" 1
t_lacks "$(rec_brief)" "tehas refine"
t_lacks "$(rec_brief)" "is not a duplicate of this plan"
t_contains "$(t_calls gh)" "--remove-label tehas:plan"

t_case "without the refine brief on the host no session starts, and it is logged"
to_refine
short=$T/norefine
rm -rf "$short"
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
cp "$T_ROOT"/libexec/* "$short/libexec/"
t_run "$short/libexec/dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "ERROR: no refine brief at $short/libexec/../lib/refine-brief.md, not refining #7"

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
t_eq "$(jq -c '.planning | [.session, .turns, .options, .pending, .failed, .running, .comment_id, .answers]' "$T/calls/record.at-turn")" '["abc-123",1,["Adopt all","Change some"],false,false,true,null,[{"turn":2,"by":"alice","fixer":false}]]'
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
  {databaseId: 4002, author: {login: "tehas-bot"}, createdAt: "2026-01-01T00:00:00Z", lastEditedAt: null, editor: null,
   body: "This planning turn did not finish.\n\n- [ ] End this planning session\n\n<!-- tehas:question kind=plan issue=7 turn=1 -->"},
  {databaseId: 4003, author: {login: "alice"}, body: "go on", createdAt: "2026-01-01T02:00:00Z"}]}}}}}' >"$fx/comments-7.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --session abc-123 --answer $keep/answer.md --comment 4003"
t_eq "$(cat "$keep/answer.md")" "The operator replied on the issue:

go on"

t_case "a session id that is not a string in a failed turn's file is no session"
to_plan
echo '{"session":{"a":1}}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "could not start"
t_eq "$(rec_open)" none

t_case "a first turn whose session id is longer than the store keeps is no session, and nothing is asked"
to_plan
jq -n --arg s "$(printf 'a%.0s' {1..200})" '{session: $s, text: "Two ways.", question: {question: "Adopt?", options: [{label: "Adopt all"}]}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "could not start"
t_lacks "$(t_calls gh)" "kind=plan issue=7"
t_eq "$(rec_open)" none

t_case "line ends in what a turn names are no words of the store's command: the bad address is not filed"
to_plan
jq -n '{session: "abc-123", text: "Two ways.",
  question: {question: "Adopt?", options: [{label: "Adopt\nall --filed x"}, {label: "Change some"}]},
  filed: ["https://github.com/acme\nwidget/issues/41", "https://github.com/acme/widget/issues/42\n",
    "https://github.com/acme/widget/issues/43"]}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(rec | jq -c '[.session, .turns, .running, .options, .filed]')" '["abc-123",1,false,["Adopt all --filed x","Change some"],["https://github.com/acme/widget/issues/43"]]'
t_eq "$([ -e "$record.rc" ] && echo kept)" ""

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
before=$(rec)
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_eq "$(rec)" "$before"
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
# the program reads it: its facts.
plan_analytics() { # [<bash body>]
  t_stub_analytics '[ "$1 $3" != "plan --facts" ] || cp "$4" "$T/calls/record.at-analytics" 2>/dev/null
'"${1:-}"
}
# The calls, without the facts file's name, which is new each time.
plan_analytics_calls() { t_calls analytics | grep '^plan' | sed 's/ --facts [^ ]*//' || true; }

t_case "a planning turn that ends is recorded in analytics, from its record before it is rewritten"
to_plan
rm -f "$fx/image.label"
plan_analytics
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0"
t_eq "$(jq -c '[.running, .turns, (.startedAt | type), .by, .label]' "$T/calls/record.at-analytics")" '[true,0,"string","alice","tehas:plan"]'
t_eq "$(rec | jq -c .turns)" 1

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
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all","Change some"],"running":true,"startedAt":"2026-01-01T00:00:00Z"}'
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
# The start comment names the minute: two runs either side of one differ
# only there.
gh_calls() { t_calls gh | sed -E 's/ at [0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2} UTC/ at <time> UTC/'; }
t_run "$dispatch"
without_gh=$(gh_calls)
without_record=$(rec | jq -c 'del(.startedAt)')
for body in 'exit 70' 'exec sleep 30'; do
  to_plan
  plan_analytics "[ \"\$1\" != plan ] || $body"
  T_ENV+=(TEHAS_ANALYTICS_WAIT=1)
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_contains "$t_out" "WARN analytics plan: exit"
  t_eq "$(gh_calls)" "$without_gh"
  t_eq "$(rec | jq -c 'del(.startedAt)')" "$without_record"
done

t_done
