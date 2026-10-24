#!/usr/bin/env bash
# Tests for libexec/cycle: a park by a provider outage or a failure, the marker
# of a park, the park question, and what is reduced before it is posted.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

# --- A cap that a provider outage gives ---------------------------------------
# The state file as pi-rukas leaves it when a step failed for good: the failed
# dispatch, then the cap.
# A handoff at <cap>, and the reason tehas work read of the failure before it.
capped() { # <cap> <failure event as JSON>
  ended handoff handoff feature/issue-7-x
  jq --arg cap "$1" '.issues[0].cap = $cap' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
  jq -n --argjson f "$2" '{roles: [], reason: ($f.providerMessage // $f.errorTail // "")}' >"$fx/progress.json"
}
outage='{"kind":"dispatch-failed-provider","step":"adversarial","at":2,"providerMessage":"Connection error."}'

t_case "a step that failed on a provider error is parked by a provider outage, and asks for one retry"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
capped step-failed:adversarial "$outage"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' >"$fx/work.sh"
# The unit's start, on alice's label: the retry rests on her.
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_approve "$T/home/tehas" 7 alice label
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "Tehas cycle parked by a provider outage: status \`handoff\`"
t_contains "$(t_calls gh)" "cap \`step-failed:adversarial\`: Connection error."
t_contains "$(t_calls gh)" "Tehas starts it again once on its own, when the model endpoint answers."
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_eq "$(park_of '[.reason, .step, .automatic, .retries, .started_by, (.started_at | type), .question]')" \
  '["provider-outage","handoff",true,0,"alice","number",4002]'
t_eq "$(t_row history-show 7 -- '.rows[-1] | [.cap, .provider, .parked] | tostring')" '["step-failed:adversarial",true,true]'
t_eq "$(jq -c '[.cap, .capReason]' "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" \
  '["step-failed:adversarial","Connection error."]'

t_case "a timeout and a 5xx are provider outages too"
for f in '{"kind":"dispatch-failed","at":2,"errorTail":"request timed out after 600s"}' \
  '{"kind":"dispatch-failed","at":2,"errorTail":"502 Bad Gateway"}'; do
  reset
  capped step-failed:develop "$f"
  t_run "$cycle 7"
  t_eq "$(park_of .reason)" provider-outage
done

t_case "an outage of a cycle started by hand owes no retry, and says it waits for a person"
reset
capped step-failed:adversarial "$outage"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle parked by a provider outage"
t_contains "$(t_calls gh)" "It was started by hand, so it waits for a person."
t_lacks "$(t_calls gh)" "starts it again once"
t_eq "$(park_of '[.reason, .automatic, .started_by]')" '["provider-outage",false,null]'

t_case "the retry tehas made on its own, parked by an outage again, waits for a person"
reset
capped step-failed:adversarial "$outage"
# The dispatcher's own retry after an outage is the approval this start uses.
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_approve "$T/home/tehas" 7 alice outage-retry
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle parked by a provider outage"
t_contains "$(t_calls gh)" "It was the retry tehas made on its own after an outage, so it waits for a person."
t_lacks "$(t_calls gh)" "starts it again once"
t_eq "$(park_of '[.reason, .automatic]')" '["provider-outage",false]'
t_eq "$(t_row cycle-show 7 -- .cycle.requeued)" true

t_case "a step that failed for another reason is parked as before, its cap on record and no retry"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
capped step-failed:develop '{"kind":"dispatch-failed","at":2,"errorTail":"3 assertions failed in test/x"}'
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' >"$fx/work.sh"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_lacks "$(t_calls gh)" "provider outage"
t_eq "$(park_of '[.reason, .automatic]')" '["step-failed:develop",false]'
t_eq "$(jq -r .capReason "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" "3 assertions failed in test/x"

t_case "a failed step with no reason is said in the log, and is not taken for an outage"
reset
capped step-failed:develop '{"kind":"dispatch-failed","at":2}'
t_run "$cycle 7"
t_contains "$t_out" "WARN: #7 ended at step-failed:develop and no reason was found in its progress"
t_eq "$(park_of .reason)" step-failed:develop
reset
capped step-failed:develop '{"kind":"dispatch-failed","at":2,"errorTail":"exit 1"}'
t_run "$cycle 7"
t_lacks "$t_out" "no reason was found"

# A result that carries its failure (features.failureReason): tehas work wrote
# the failure into the progress, and its kind is what counts.
failed() { # <cap> <failure kind> <reason>
  ended handoff handoff feature/issue-7-x
  jq --arg cap "$1" '.issues[0].cap = $cap' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
  jq -n --arg k "$2" --arg r "$3" \
    '{roles: [], reason: $r, failure: {kind: $k, step: "develop", role: "developer", cause: ""}, from: {roles: "result", failure: "result"}}' \
    >"$fx/progress.json"
}

t_case "a failure of kind provider is an outage whatever the cap, and asks for one retry"
reset
failed spawn-backstop provider "rate limited"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas cycle parked by a provider outage"
t_contains "$(t_calls gh)" "cap \`spawn-backstop\`: rate limited"
t_eq "$(park_of .reason)" provider-outage
t_eq "$(t_row history-show 7 -- '.rows[-1] | [.cap, .provider] | tostring')" '["spawn-backstop",true]'

t_case "a failure of another kind is no outage, even when its words look like one"
for k in process timeout verify cap something-new; do
  reset
  failed step-failed:develop "$k" "502 Bad Gateway, connection timed out"
  t_run "$cycle 7"
  t_eq "$(park_of .reason)" step-failed:develop
  t_lacks "$(t_calls gh)" "provider outage"
  t_lacks "$t_out" "no reason was found"
done

t_case "the park comment names the result's failure, and not for a cap's own rule"
reset
failed step-failed:develop process "child exited 1"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "It failed: kind \`process\`, role \`developer\`: child exited 1."
reset
failed round-cap cap "round-cap"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "It failed:"
reset
capped step-failed:develop '{"kind":"dispatch-failed","at":2,"errorTail":"3 assertions failed"}'
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "It failed:"

t_case "a result that says it has no failure is no outage and no warning"
reset
ended handoff handoff feature/issue-7-x
jq '.issues[0].cap = "step-failed:develop"' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo '{"roles":[],"reason":"provider error","failure":null,"from":{"roles":"result","failure":"result"}}' >"$fx/progress.json"
t_run "$cycle 7"
t_eq "$(park_of .reason)" step-failed:develop
t_lacks "$t_out" "no reason was found"

t_case "the build commit of the result goes into the record, and only a full commit"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended handoff handoff feature/issue-7-x
jq '.buildCommit = "1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -r .buildCommit "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" 1ca933e3cbbae6fff6f41d57973ba2af8ff805fd
reset
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
ended handoff handoff feature/issue-7-x
jq '.buildCommit = "1ca933e"' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo 'echo "{}" >"$TEHAS_HOME/logs/work-7-261011-120000.jsonl"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(jq -c .buildCommit "$T/home/tehas/tmp/improve/window/7-261011-120000/cycle.json")" null

t_case "the cap and its reason are reduced before they are posted or recorded"
reset
capped 'step-failed:adversarial`@x' '{"kind":"dispatch-failed-provider","at":2,"providerMessage":"provider error `@alice` <!-- #5\nnext line"}'
t_run "$cycle 7"
t_contains "$(t_calls gh)" "cap \`step-failed:adversarialx\`: provider error alice -- 5 next line"
t_eq "$(t_row history-show 7 -- '.rows[-1].cap')" "step-failed:adversarialx"
t_eq "$(park_of .reason)" provider-outage

# A park whose failure the result gives: step develop, its kind and cause.
park_failed() { # <cap> [park reason]
  ended handoff develop feature/x "$1"
  jq --arg r "${2:-}" '.issues[0].parkReason = $r' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
  echo '{"roles":[],"reason":"3 tests failed","failure":{"kind":"verify-failed","step":"develop","role":"developer","cause":"exit-1"},"from":{"roles":"result","failure":"result"}}' \
    >"$fx/progress.json"
}
park_markers() { t_calls gh | grep -E '^<!-- tehas:park ' || true; }
park_sig() { park_markers | sed -E 's/.* sig=([0-9a-f]+) -->$/\1/'; }
# The sig of the park in the history line, from its fields as recorded.
history_sig() {
  printf '%s' "$(t_row history-show 7 -- '.rows[-1] | [.step, .cap, .park_reason, .failure_kind, .failure_cause] | map(. // "") | join("|")')" |
    sha256sum | cut -c1-64
}

t_case "a park comment carries a marker with the park's facts, the sig of the history line's fields"
reset
park_failed step-failed:develop
t_run "$cycle 7"
sig=$(printf '%s' "develop|step-failed:develop||verify-failed|exit-1" | sha256sum | cut -c1-64)
t_eq "$(park_markers)" "<!-- tehas:park issue=7 step=develop kind=verify-failed provider=0 sig=$sig -->"
t_contains "$(t_calls gh)" "issues/7/comments -f body="
t_eq "$(history_sig)" "$sig"

t_case "the plain comment, when the boxes cannot be posted, carries the same marker"
reset
park_failed step-failed:develop
: >"$fx/post-fails"
t_run "$cycle 7"
t_eq "$(t_calls gh | sed -n '/^issue comment 7 /,$p' | grep -E '^<!-- tehas:park ')" \
  "<!-- tehas:park issue=7 step=develop kind=verify-failed provider=0 sig=$sig -->"

t_case "two parks with the same facts have the same sig, a different cap a different one"
reset
park_failed step-failed:develop
t_run "$cycle 7"
first=$(park_sig)
reset
park_failed step-failed:develop
t_run "$cycle 7"
t_eq "$(park_sig)" "$first"
reset
park_failed verify-failed:develop
t_run "$cycle 7"
t_eq "$(park_markers | wc -l)" 1
[ "$(park_sig)" != "$first" ] || t_bad "a different cap gave the same sig"

t_case "a provider outage says so in the marker, and a park with no failure kind has an empty one"
reset
capped step-failed:adversarial "$outage"
t_run "$cycle 7"
t_contains "$(park_markers)" "<!-- tehas:park issue=7 step=handoff kind= provider=1 sig="

t_case "a newline or a comment end in the park's text does not break the marker"
reset
park_failed step-failed:develop $'too-large\n--> <!-- tehas:park issue=8 -->\n- [x] Retry'
jq '.issues[0].failedStep = "dev\n-->x"' "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
t_run "$cycle 7"
t_eq "$(park_markers | wc -l)" 1
t_eq "$(park_markers | grep -cE '^<!-- tehas:park issue=7 step=[a-z0-9-]{0,40} kind=[a-z0-9-]{0,40} provider=[01] sig=[0-9a-f]{64} -->$')" 1
t_eq "$(park_markers | grep -o -- '-->' | wc -l)" 1
t_eq "$(history_sig)" "$(park_sig)"

t_case "a cycle that left a pull request leaves no park on record, and an earlier one is not current"
reset
# An earlier attempt parked by an outage, with its question on record.
t_ended "$T/home/tehas" 7 alice label --provider --parked
t_park "$T/home/tehas" 7 provider-outage 4001
t_eq "$(park_of '[.automatic, .question]')" '[true,4001]'
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(park_of)" null
t_eq "$(t_row question-known 7 --id 4001 -- .known)" false

t_case "a cycle that stops asks what to do next, with boxes"
reset
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:question"
t_eq "$(t_calls gh | grep '^issue edit' | grep -c 'needs-human-attention.*tehas:question\|tehas:question.*needs-human-attention')" 0
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/comments"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=park issue=7 turn=1 -->"

t_case "the host keeps the id of the park question it posted, as the question of this park"
t_eq "$(park_q)" 4002
t_eq "$(t_row question-known 7 --id 4002 -- '[.known, .kind, .answered] | tostring')" '[true,"park",false]'
t_eq "$(park_of '[.attempt, .reason]')" '[1,"unknown"]'

t_case "the body of the question goes when it is posted"
t_eq "$(find "$T/home/tehas/tmp" -name 'park-question.*' 2>/dev/null | wc -l)" 0

t_case "when the question cannot be posted the old comment is used and no question label is added"
reset
: >"$fx/post-fails"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "issue comment 7"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "To retry, apply"
t_lacks "$(t_calls gh | grep '^issue edit')" "tehas:question"
t_eq "$(park_q)" none
# The park is on record all the same: it asks nothing.
t_eq "$(park_of .reason)" unknown

t_case "a park the store would not take asks no question: the plain comment, and no tehas:question"
reset
t_store_answers park 1 'store: busy #7: the database is locked'
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: could not record the park of #7 (busy: store: busy #7: the database is locked), so it asks no question"
t_eq "$(grep -c '^park 7 ' "$T/calls/store")" 3
t_lacks "$(t_calls gh)" "api repos/acme/widget/issues/7/comments"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$(t_calls gh)" "To retry, apply \`tehas:ready\`."
t_lacks "$(t_calls gh)" "- [ ] Retry from a clean start"
t_lacks "$(t_calls gh | grep '^issue edit')" "tehas:question"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_eq "$(park_of)" null
t_eq "$(t_row history-show 7 -- '.rows[-1].parked')" true

t_case "and so does a store that cannot be used"
reset
t_store_answers park 78 'store: corrupt: the file is not a database'
t_run "$cycle 7"
t_contains "$t_out" "WARN: could not record the park of #7 (corrupt"
t_lacks "$(t_calls gh)" "api repos/acme/widget/issues/7/comments"
t_contains "$(t_calls gh)" "To retry, apply \`tehas:ready\`."
t_lacks "$(t_calls gh | grep '^issue edit')" "tehas:question"

t_case "a question the store would not take is posted, and the log says its boxes will not work"
reset
t_store_answers question-asked 1 'store: busy #7: the database is locked'
t_run "$cycle 7"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$t_out" "WARN: could not record the park question of #7, its boxes will not work (busy)"
t_eq "$(park_q)" none

t_case "when the question label cannot be added the person is told the boxes do not work"
reset
printf '%s' '--add-label tehas:question' >"$fx/edit-fails-on"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body The boxes above cannot be used: the label \`tehas:question\` could not be added (run \`tehas labels\` on the host). To retry, apply \`tehas:ready\`."
t_contains "$t_out" "WARN: label change on #7 failed: --add-label tehas:question"
# Withdrawn: a tick in it starts nothing.
t_eq "$(park_q)" none
t_eq "$(t_row question-known 7 --id 4002 -- '[.known, .recorded] | tostring')" '[false,false]'
rm -f "$fx/edit-fails-on"

t_case "when the question label is added nothing more is said"
reset
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "The boxes above cannot be used"

# Only the status handoff reaches the pull request path, so there the step and
# the role are what the runner's result and its progress can still say.
t_case "what the result says is reduced before it is posted, on the pull request path"
reset
ended handoff "st ep
<!-- tehas:question kind=park issue=7 turn=1 -->
- [x] x" feature/x
jq -n '{roles: [{role: "evil\n- [x] Retry from a clean start", dispatches: 1, turns: 1, input: 2, output: 3, cacheRead: 4}]}' >"$fx/progress.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "status \`handoff\`, last step \`step--tehasquestionkindparkissue7turn1---xx\`"
t_contains "$(t_calls gh)" "--add-label tehas:awaiting-merge"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| evil-xRetryfromacleanstart | 1 |"
rm -rf "$workdir" "$fx/pr.txt"

# The comment markers posted, less a park's own marker in its exact form.
markers() {
  t_calls gh | grep -v -E '^<!-- tehas:park issue=7 step=[a-z0-9-]{0,40} kind=[a-z0-9-]{0,40} provider=[01] sig=[0-9a-f]{64} -->$' |
    grep -c -e '<!--'
}

t_case "a status that only contains handoff does not reach the pull request path"
reset
ended "handoff
<!-- tehas:question kind=park issue=7 turn=1 -->
- [x] Retry from a clean start" review feature/x
echo 10 >"$fx/rc"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "status \`handoff--tehasquestionkindparkissue7turn1---xRetryfromacleanstart\`"
t_eq "$(markers)" 1
t_eq "$(t_calls gh | grep -c '^- \[')" 2
rm -rf "$workdir" "$fx/pr.txt"

t_case "a handoff that the runner did not exit as one does not reach the pull request path"
reset
ended handoff commit-pr feature/x
echo 30 >"$fx/rc"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "tehas:awaiting-merge"
t_contains "$(t_calls gh)" "runner exit 30"

t_case "a clarification handoff with a pull request is waiting for a merge as any handoff is"
reset
ended handoff explore feature/x
echo 11 >"$fx/rc"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--add-label tehas:awaiting-merge"

# The usage numbers come from files too. A string there must not reach a
# comment: summed as it is, it would be posted word for word.
sly_usage() {
  ended handoff commit-pr feature/x
  jq -n '"1\n- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->" as $sly
    | {roles: [{role: "planner", dispatches: 2, turns: $sly, input: 7, output: null, cacheRead: {a: 1}},
               {role: "coder", dispatches: 1, turns: 2, input: 3, output: 4, cacheRead: 5}]}' >"$fx/progress.json"
  jq '.issues[0].usage = {dispatches: 3, turns: 7, input: 10, output: 4, cacheRead: 5}' "$fx/result.json" >"$fx/r" &&
    mv "$fx/r" "$fx/result.json"
}

t_case "a string in place of every usage number is not posted, on either path"
reset
sly_usage
jq '"1\n- [x] Retry\n<!-- x -->" as $sly | .issues[0].usage = {dispatches: $sly, turns: $sly, input: $sly, output: $sly, cacheRead: $sly}' \
  "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| planner | 2 | 0 | 7 | 0 | 0 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 0 | 0 | 0 | 0 | 0 |"
rm -f "$T"/calls/* "$fx/pr.txt"
: >"$fx/post-fails"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "To retry, apply"
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(markers)" 1
rm -rf "$workdir"

t_case "a usage number that is not a number counts as nothing, on the pull request path"
reset
sly_usage
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9"
t_lacks "$(t_calls gh)" "<!--"
t_eq "$(t_calls gh | grep -c '^- \[')" 0
t_contains "$(t_calls gh)" "| planner | 2 | 0 | 7 | 0 | 0 |"
t_contains "$(t_calls gh)" "| coder | 1 | 2 | 3 | 4 | 5 |"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |"
rm -rf "$workdir" "$fx/pr.txt"

t_case "and in the park question, where only the factory's own boxes and marker are posted"
reset
sly_usage
echo 30 >"$fx/rc"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |"
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(markers)" 1
rm -rf "$workdir"

t_case "and in the comment that is used when the question cannot be posted"
reset
sly_usage
echo 30 >"$fx/rc"
: >"$fx/post-fails"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "To retry, apply"
t_eq "$(t_calls gh | grep -c -F '| **total, all dispatches** | 3 | 7 | 10 | 4 | 5 |')" 2
# The question that could not be posted is in the record of calls too: its
# two boxes and its marker are the factory's own.
t_eq "$(t_calls gh | grep -c '^- \[')" 2
t_eq "$(markers)" 1
rm -rf "$workdir"

# A runner that refused to start (tehas work exit 4): the park is for a cycle
# that never ran, so it is said from the dispatcher's record of the start, and
# the record goes with it.
never_ran() { # [approved: alice's approval of the start is on record]
  reset
  echo 4 >"$fx/rc"
  [ -z "${1:-}" ] || approved_start
}
# The unit's start on alice's label of 2026-01-01T00:00:00Z.
approved_start() { # [<approval-record args>...]
  T_ENV+=(TEHAS_CYCLE_FROM=unit)
  t_approve "$T/home/tehas" 7 alice label "$(date -u -d 2026-01-01T00:00:00Z +%s)" "$@"
}

t_case "a start that never ran is parked as one that never ran, named from the record"
never_ran approved
t_run "$cycle 7"
t_contains "$(t_calls gh)" "Tehas started no cycle: an image or file the issue links could not be given to it, or the issue could not be read for them (runner exit 4)."
t_contains "$(t_calls gh)" "alice approved this start at 2026-01-01T00:00:00Z"
t_contains "$(t_calls gh)" "- [ ] Retry from a clean start"
t_lacks "$(t_calls gh)" "stopped without a pull request"
t_lacks "$(t_calls gh)" "status \`unknown\`"
t_lacks "$(t_calls gh)" "last step"
t_eq "$(park_of '[.reason, .automatic, .body_sha256]')" '["unknown",false,null]'

t_case "a refresh that never ran names the person's approval it rests on, not the time the poll decided it"
never_ran
t_ended "$T/home/tehas" 7 alice label --status handoff
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_approve "$T/home/tehas" 7 alice refresh "$(date -u -d 2026-10-24T12:00:00Z +%s)" \
  --refresh-pr 50 --refresh-head "$(printf 'a%.0s' {1..40})"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "alice approved this start at 2023-11-14T22:13:20Z"
t_lacks "$(t_calls gh)" "2026-10-24T12:00:00Z"

t_case "the retry after an outage that never ran names no time of approval: the poll decided it"
never_ran
T_ENV+=(TEHAS_CYCLE_FROM=unit)
t_approve "$T/home/tehas" 7 alice outage-retry "$(date -u -d 2026-10-24T12:00:00Z +%s)"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "alice approved this start;"
t_lacks "$(t_calls gh)" "2026-10-24T12:00:00Z"

t_case "a start that never ran leaves no approval record, and its line of history still names how it was authorised"
never_ran approved
t_run "$cycle 7"
t_eq "$(t_row approval-current 7 -- '[.approval, .current] | tostring')" '[null,null]'
t_eq "$(t_row history-show 7 -- '.rows[0] | [.rc, .status, .step, .how, .parked] | tostring')" '[4,"unknown","-","label",true]'

t_case "a cycle that ran keeps its approval record"
reset
approved_start
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(t_row approval-current 7 -- '[.current.by, .current.used_by] | tostring')" '["alice",1]'
rm -rf "$workdir" "$fx/pr.txt"

t_case "a start that never ran with no record on this host says so, and still asks for the label"
never_ran
t_run "$cycle 7"
t_contains "$(t_calls gh)" "No approval of this start is recorded on this host: a fresh \`tehas:ready\` is needed."
t_contains "$(t_calls gh)" "--add-label needs-human-attention"
t_eq "$(park_of '[.attempt, .question]')" '[1,4002]'

# The park carries the body its cycle was approved on, so the dispatcher can
# tell a relabelled start of the same text from one that came after a change.
t_case "the park carries the body of the approval it started on"
reset
approved_start --body-sha 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
echo 30 >"$fx/rc"
t_run "$cycle 7"
t_eq "$(park_of .body_sha256)" \
  "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

t_case "a park of a cycle started by hand names no body, not that of an earlier start"
# The earlier start's approval is still the last one used, and not this one's.
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_CYCLE_RETRY_WAIT=0)
t_run "$cycle 7"
t_eq "$(park_of .body_sha256)" null
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, .cycle.how] | tostring')" '[2,"hand"]'

t_done
