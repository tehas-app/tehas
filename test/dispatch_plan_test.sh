#!/usr/bin/env bash
# Tests for tehasbot dispatch: planning sessions, their turns and answers.
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
t_lacks "$(plan_calls)" "--kind refine"
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

t_case "a turn that hands back a plan for its issue writes it there and ends the session, said once"
to_plan
jq -n '{session: "abc-123", text: "This issue now holds the plan.\n\n> # Faster search", question: null,
  update: {title: "feat: faster search", body: "## Outcome\n\nSearch returns results faster."}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --title feat: faster search --body ## Outcome

Search returns results faster."
t_contains "$(t_calls gh)" "--remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan --remove-label needs-human-attention"
t_eq "$(t_calls gh | grep -c 'This issue now holds the plan.')" 1
t_lacks "$(t_calls gh)" "tehas:question kind=plan"
t_eq "$(rec_open)" none
t_eq "$([ -e "$record.out" ] && echo left)" ""

t_case "a plan that cannot be written onto its issue is written by the next poll, and the session ends then"
to_plan
jq -n '{session: "abc-123", text: "This issue now holds the plan.", question: null,
  update: {title: "feat: faster search", body: "Search returns results faster."}}' >"$fx/plan-result.json"
echo "--title feat: faster search" >"$fx/edit-fails-on"
t_run "$dispatch"
t_contains "$t_out" "WARN: the plan could not be written onto #7; the next poll tries again"
t_lacks "$(t_calls gh)" "This issue now holds the plan."
t_eq "$(rec_open)" open
# The label came off when the turn started.
echo '[]' >"$fx/plans.json"
rm -f "$fx/edit-fails-on" "$T/calls/gh"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --title feat: faster search --body Search returns results faster."
t_eq "$(t_calls gh | grep -c 'This issue now holds the plan.')" 1
t_eq "$(rec_open)" none

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
brief=$(tr '\n' ' ' <"$T_ROOT/lib/refine-brief.md")
t_contains "$brief" "a single unattended \`/work\` cycle finishes"
t_contains "$brief" "plan the first part only, the one the others build on"
t_contains "$brief" "under \"Later parts\""
t_eq "$([ "$(wc -c <"$T_ROOT/lib/refine-brief.md")" -lt 4000 ] && echo short)" short

t_case "the issue brief, which every session is given, asks for a plan grounded in the code, with exact paths"
brief=$(tr '\n' ' ' <"$T_ROOT/lib/issue-brief.md")
t_contains "$brief" "Ground it in the code first"
t_contains "$brief" "[paths: \`a\`, \`b\`]"
t_contains "$brief" "\`.github/\` or \`.pi/\`"
t_contains "$brief" "Classify the plan in its title, as a conventional-commit type"
t_contains "$brief" "Keep a type the    request's title already has."
t_contains "$brief" "no inventory of the request"
t_eq "$([ "$(wc -c <"$T_ROOT/lib/issue-brief.md")" -lt 4000 ] && echo short)" short

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
t_front "$short"
# The Rust poll finds the brief beside its own program, as on a host.
cp "$T_DISPATCH" "$short/libexec/tehas-dispatch"
T_ENV_ALWAYS=()
t_run "$short/bin/tehasbot dispatch"
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

t_done
