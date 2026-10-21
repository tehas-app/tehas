#!/usr/bin/env bash
# Tests for lib/headless.sh: what tehas reads of pi-rukas's headless runner.
# The fixtures in test/fixtures/headless-0.12.80 are what the v0.12.80 runner
# printed in a throwaway container with no model and no token: its
# --capabilities answer, and the NDJSON progress and --json result of
# `work 7`, which ended in a handoff. test/fixtures/headless-1ca933e is the
# same from the image of pi-rukas main at 1ca933e (still version 0.12.80),
# whose --capabilities lists family members and features.failureReason and
# features.usageByRole, and whose result carries failure and usage.byRole.
# test/fixtures/headless-316134e is the same from the image of pi-rukas main at
# 316134e (still version 0.12.80), the first on result schema 4.
# test/fixtures/headless-c5ddc0a is the same from the v0.14.0 release (c5ddc0a),
# the first on result schema 5: its result is the one tehas reads. The ones
# before it are kept for the progress lines and for the refusal. One default in
# each answer from main named a private address and is replaced by a
# documentation one.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=lib/cycles.sh
. "$T_ROOT/lib/cycles.sh"
# shellcheck source=lib/headless.sh
. "$T_ROOT/lib/headless.sh"
fx=$T_ROOT/test/fixtures/headless-0.12.80
caps=$(cat "$fx/capabilities.json")
nx=$T_ROOT/test/fixtures/headless-1ca933e
ncaps=$(cat "$nx/capabilities.json")
sx=$T_ROOT/test/fixtures/headless-c5ddc0a
scaps=$(cat "$sx/capabilities.json")
# What tehas work gives a cycle's container, of PI_ENSEMBLE_* names.
given="PI_ENSEMBLE_SANDBOX_MODE PI_ENSEMBLE_WORKSPACE_ROOT PI_ENSEMBLE_MAX_WORKSTREAMS PI_ENSEMBLE_VERIFY_TIMEOUT_MS
  PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER"

t_case "the runner of v0.14.0, on result schema 5, supports what tehas gives a cycle"
t_eq "$(tehas_headless_unsupported "$scaps" "--timeout 360 --json --restart" "$given PI_ENSEMBLE_EXTRA_IMAGE_DIRS")" ""

t_case "a runner on result schema 3 is refused for that alone"
t_eq "$(tehas_headless_unsupported "$caps" "--timeout 360 --json --restart" "$given PI_ENSEMBLE_EXTRA_IMAGE_DIRS")" \
  "result schema version 3, not 5"
t_eq "$(tehas_headless_unsupported "$ncaps" "--timeout 360 --json --restart" "$given PI_ENSEMBLE_EXTRA_IMAGE_DIRS")" \
  "result schema version 3, not 5"

t_case "a role's model and provider are inputs of their family"
t_eq "$(tehas_headless_unsupported "$scaps" "--json" "PI_ENSEMBLE_MODEL_DEVELOPER PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST")" ""

t_case "a name the build does not read, one it sets on its own children and one of its wrapper are each named"
t_eq "$(tehas_headless_unsupported "$scaps" "--json" "PI_ENSEMBLE_NOT_A_LEVER PI_ENSEMBLE_SUBAGENT_MODE PI_ENSEMBLE_NO_SSH")" \
  "the input PI_ENSEMBLE_NOT_A_LEVER
the input PI_ENSEMBLE_SUBAGENT_MODE
the input PI_ENSEMBLE_NO_SSH"

t_case "only the flags are checked, not their values"
t_eq "$(tehas_headless_unsupported "$scaps" "--timeout 360 --json --bogus" "")" "the flag --bogus"

t_case "each contract version is checked"
t_eq "$(tehas_headless_unsupported "$(jq -c '.contracts.exitCodes.version = 2 | .contracts.resultSchema.version = 6 | .contracts.ndjson.stable.version = 9' <<<"$scaps")" "" "")" \
  "exit codes version 2, not 1
result schema version 6, not 5
stable NDJSON version 9, not 1"
t_contains "$(tehas_headless_unsupported '{}' "" "")" "exit codes version null"

t_case "an answer that is not an object is no answer"
for bad in "" "not json" "[]" "7" "null"; do
  tehas_headless_unsupported "$bad" "" "" >/dev/null
  t_eq "$?" 1
done

t_case "a name in the answer is a pattern only of name characters"
t_eq "$(tehas_headless_unsupported '{"contracts":{"exitCodes":{"version":1},"resultSchema":{"version":5},"ndjson":{"stable":{"version":1}}},
  "inputs":[{"name":"PI_ENSEMBLE_.*","scope":"cycle"},{"name":"PI_ENSEMBLE_A|PI_ENSEMBLE_B","scope":"cycle"}]}' "" "PI_ENSEMBLE_A")" \
  "the input PI_ENSEMBLE_A"

t_case "what is said of the answer is plain characters"
t_eq "$(tehas_headless_unsupported '{"contracts":{"exitCodes":{"version":"1\u001b[31m;x|y"}}}' "" "" | head -n 1)" \
  "exit codes version 131mxy, not 1"

t_case "a family that lists its members takes only those"
t_eq "$(tehas_headless_unsupported "$scaps" "--json" "PI_ENSEMBLE_MODEL_DEVELOPER PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST PI_ENSEMBLE_MODEL_LENS_SECURITY PI_ENSEMBLE_TOKEN_BUDGET_OPS")" ""
t_eq "$(tehas_headless_unsupported "$scaps" "--json" "PI_ENSEMBLE_MODEL_REVIEWER PI_ENSEMBLE_MODEL_LENS_STYLE PI_ENSEMBLE_MODEL_DEVELOPERX")" \
  "the input PI_ENSEMBLE_MODEL_REVIEWER
the input PI_ENSEMBLE_MODEL_LENS_STYLE
the input PI_ENSEMBLE_MODEL_DEVELOPERX"
# A family that lists no members, as v0.12.80's did: any name characters.
t_eq "$(tehas_headless_unsupported "$(jq -c 'del(.inputs[] | select(.name == "PI_ENSEMBLE_MODEL_<ROLE>") | .members)' <<<"$scaps")" \
  "--json" "PI_ENSEMBLE_MODEL_REVIEWER")" ""

t_case "a member is only of name characters"
t_eq "$(tehas_headless_unsupported '{"contracts":{"exitCodes":{"version":1},"resultSchema":{"version":5},"ndjson":{"stable":{"version":1}}},
  "inputs":[{"name":"PI_ENSEMBLE_MODEL_<ROLE>","scope":"cycle","members":["A|.*","OPS"]}]}' "" "PI_ENSEMBLE_MODEL_A PI_ENSEMBLE_MODEL_X PI_ENSEMBLE_MODEL_OPS")" \
  "the input PI_ENSEMBLE_MODEL_A
the input PI_ENSEMBLE_MODEL_X"

t_case "an image reads issue images when it lists PI_ENSEMBLE_ISSUE_IMAGES for manifest format 1"
t_eq "$(tehas_headless_reads_images "$scaps")" 1
t_eq "$(tehas_headless_reads_images "$(jq -c '(.inputs[] | select(.name == "PI_ENSEMBLE_ISSUE_IMAGES") | .manifestVersion) = 2' <<<"$scaps")")" 0
t_eq "$(tehas_headless_reads_images "$(jq -c 'del(.inputs[] | select(.name == "PI_ENSEMBLE_ISSUE_IMAGES"))' <<<"$scaps")")" 0
for bad in "" "not json" "[]"; do t_eq "$(tehas_headless_reads_images "$bad")" 0; done

t_case "where the image says its result carries them, usage per role and the failure come from the result"
jq '.issues[0].usage.byRole = {ops: {input: 0, output: 0, cacheRead: 0, turns: 0, dispatches: 1}}' "$sx/result.json" >"$T/r.json"
p=$(tehas_headless_progress "$sx/progress.jsonl" "$scaps" "$T/r.json" 7)
t_eq "$(jq -c .roles <<<"$p")" '[{"role":"ops","dispatches":1,"turns":0,"input":0,"output":0,"cacheRead":0}]'
t_eq "$(jq -c .failure <<<"$p")" '{"kind":"cap","step":"explore","role":"","cause":""}'
t_eq "$(jq -r .reason <<<"$p")" explore-bodies-empty
t_eq "$(jq -c .from <<<"$p")" '{"roles":"result","failure":"result"}'

t_case "a provider failure gives its kind and the provider's own message"
jq '.issues[0].failure = {kind: "provider", step: "develop", role: "developer", cause: "rate-limited:429",
  message: "The provider refused", providerMessage: "429 Too Many Requests"}
  | .issues[0].usage.byRole = {developer: {input: 5, output: 2, cacheRead: 1, turns: 3, dispatches: 2}, "a b": {dispatches: "x"}, bad: 7}' \
  "$sx/result.json" >"$T/r.json"
p=$(tehas_headless_progress "$sx/progress.jsonl" "$scaps" "$T/r.json" 7)
t_eq "$(jq -c .failure <<<"$p")" '{"kind":"provider","step":"develop","role":"developer","cause":"rate-limited:429"}'
t_eq "$(jq -r .reason <<<"$p")" "429 Too Many Requests"
t_eq "$(jq -c .roles <<<"$p")" '[{"role":"developer","dispatches":2,"turns":3,"input":5,"output":2,"cacheRead":1},{"role":"ab","dispatches":0,"turns":0,"input":0,"output":0,"cacheRead":0}]'

t_case "a result with no failure, where the image says it would carry one, has none"
jq 'del(.issues[0].failure)' "$sx/result.json" >"$T/r.json"
p=$(tehas_headless_progress "$sx/progress.jsonl" "$scaps" "$T/r.json" 7)
t_eq "$(jq -c '[.failure, .reason, .from.failure]' <<<"$p")" '[null,"","result"]'

t_case "an image that does not say so (v0.12.80) gives the progress lines, whatever its result holds"
p=$(tehas_headless_progress "$nx/progress.jsonl" "$caps" "$sx/result.json" 7)
t_eq "$(jq -c '[.roles, .failure, .from]' <<<"$p")" '[[],null,{"roles":"progress","failure":"progress"}]'
t_contains "$(jq -r .reason <<<"$p")" "cancelled (abort signal)"
jq '.issues[0].usage.byRole = {ops: {dispatches: 1}}' "$sx/result.json" >"$T/r.json"
p=$(tehas_headless_progress "$nx/progress.jsonl" "$(jq -c 'del(.features.failureReason)' <<<"$scaps")" "$T/r.json" 7)
t_eq "$(jq -c '[.from, (.roles | length)]' <<<"$p")" '[{"roles":"result","failure":"progress"},1]'

t_case "no result, another issue, a schema 3 result or a result that is a link gives the progress lines"
for args in "$T/missing.json 7" "$sx/result.json 8" "$nx/result.json 7"; do
  # shellcheck disable=SC2086  # the file and the issue, two arguments
  t_eq "$(tehas_headless_progress "$nx/progress.jsonl" "$scaps" $args | jq -c .from)" '{"roles":"progress","failure":"progress"}'
done
ln -s "$sx/result.json" "$T/link.json"
t_eq "$(tehas_headless_progress "$nx/progress.jsonl" "$scaps" "$T/link.json" 7 | jq -c .from)" '{"roles":"progress","failure":"progress"}'
t_eq "$(tehas_headless_progress /dev/null "not json" /dev/null 7 | jq -c '[.roles, .reason, .failure]')" '[[],"",null]'

t_case "the live line is the last stable step and the driver's last notice, without its marker line"
t_eq "$(tehas_headless_now "$fx/progress.jsonl" | head -n 1)" handoff
t_eq "$(tehas_headless_now "$fx/progress.jsonl" | sed -n 2p)" "⚠ pi-rukas /work for issue #7 — HANDOFF DISPATCH INCOMPLETE"
t_eq "$(tehas_headless_now /dev/null | tr '\n' ,)" ",,"

# The log of a run whose runner died: one line per stable step-started line, in
# the order the runner wrote them. A file of them, so a case names the steps.
steps_log() { # <name> <step...>: the path of a log of those steps
  local f=$T/$1.jsonl
  shift
  : >"$f"
  local s
  for s in "$@"; do
    printf '{"v":1,"kind":"step-started","issue":7,"event":{"step":"%s"}}\n' "$s" >>"$f"
  done
  printf '%s\n' "$f"
}
run_of() { # <issue> <step...>: the three lines the helper answers of that run
  local f
  f=$(steps_log run "$@" | tail -n 1)
  tehas_headless_run "$f" "$1" | tr '\n' '|'
}

t_case "the run's last step, its pushed branch, and whether its log was read whole"
# The step of the last step-started line, and the step right after commit-pr as
# the branch, which is this issue's own and so is said.
t_eq "$(run_of 7 explore plan develop commit-pr feature/issue-7-checks-instance)" \
  "feature/issue-7-checks-instance|feature/issue-7-checks-instance|whole|"
# No commit-pr at all: the step of the run stands, and GitHub is asked nothing.
t_eq "$(run_of 7 explore plan develop)" "develop||whole|"
# A run whose steps stop at commit-pr pushed nothing, so its last step is no
# branch of a push.
t_eq "$(run_of 7 explore develop commit-pr)" "commit-pr||whole|"
t_case "the branch of the LAST commit-pr, so a run that pushed twice ends on its last push"
t_eq "$(run_of 7 commit-pr feature/issue-7-first develop commit-pr feature/issue-7-second)" \
  "feature/issue-7-second|feature/issue-7-second|whole|"
t_case "a branch of another issue, one of no slug, and a step that is no branch"
t_eq "$(run_of 7 commit-pr feature/issue-8-other)" "feature/issue-8-other||whole|"
t_eq "$(run_of 7 commit-pr develop)" "develop||whole|"
t_case "a step name over the bound of 100 characters is no step of a run"
# The bound is the one tehas_headless_step_chars holds, and it is wider than the
# grammar of tehas_branch_pattern in one way and narrower in another. A name of
# 101 characters is no step at all, whatever its shape, so the run says only its
# commit-pr. A name of the most characters a step may be, but with a slug past
# 80, is a step and no branch of this issue, so GitHub is asked about nothing. A
# name of the widest branch the grammar takes is both at once.
t_eq "$(run_of 7 commit-pr "$(printf 'x%.0s' {1..101})")" "commit-pr||whole|"
t_eq "$(run_of 7 commit-pr "$(printf 'z%.0s' {1..100})")" \
  "$(printf 'z%.0s' {1..100})||whole|"
t_eq "$(run_of 7 commit-pr "feature/issue-7-$(printf 'y%.0s' {1..84})")" \
  "feature/issue-7-$(printf 'y%.0s' {1..84})||whole|"
t_eq "$(run_of 7 commit-pr "feature/issue-7-$(printf 'y%.0s' {1..80})")" \
  "feature/issue-7-$(printf 'y%.0s' {1..80})|feature/issue-7-$(printf 'y%.0s' {1..80})|whole|"
t_case "an empty log, an unreadable one and no issue number say nothing"
: >"$T/empty.jsonl"
t_eq "$(tehas_headless_run "$T/empty.jsonl" 7 | tr '\n' '|')" "||whole|"
# Not a number: no issue's branch is asked about, and nothing of the run is
# said for it. A missing file is answered the same way, and said of in the log.
t_eq "$(tehas_headless_run "$T/empty.jsonl" abc; echo "rc=$?")" "rc=1"
t_eq "$(tehas_headless_run "$T/nothing.jsonl" 7; echo "rc=$?")" "rc=1"
ln -s "$fx/progress.jsonl" "$T/run-link.jsonl"
t_eq "$(tehas_headless_run "$T/run-link.jsonl" 7; echo "rc=$?")" "rc=1"
t_case "a log bigger than the 200 KB tehas reads says it was read from its end"
python3 -c 'import json,sys
with open(sys.argv[1],"w") as f:
    for _ in range(2500):
        f.write(json.dumps({"v":1,"kind":"log","message":"y"*100})+"\n")
    for s in ["commit-pr","feature/issue-7-late"]:
        f.write(json.dumps({"v":1,"kind":"step-started","event":{"step":s}})+"\n")' "$T/big.jsonl"
t_eq "$(tehas_headless_run "$T/big.jsonl" 7 | tr '\n' '|')" \
  "feature/issue-7-late|feature/issue-7-late|truncated|"

t_case "the progress of the fixture has no completed dispatch and the reason of its one failed dispatch"
t_eq "$(tehas_headless_progress "$fx/progress.jsonl" | jq -c .roles)" "[]"
t_contains "$(tehas_headless_progress "$fx/progress.jsonl" | jq -r .reason)" "cancelled (abort signal)"

t_case "a cap's evidence is the reason before a failed dispatch's"
printf '%s\n' '{"v":1,"kind":"dispatch-failed","unstable":true,"event":{"errorTail":"exit 1"}}' \
  '{"v":1,"kind":"cap-hit","event":{"cap":"x","evidence":"git says no"}}' >"$T/p.jsonl"
t_eq "$(tehas_headless_progress "$T/p.jsonl" | jq -r .reason)" "git says no"
t_eq "$(tehas_headless_progress /dev/null | jq -c '[.roles, .reason, .reviewHeld]')" '[[],"",false]'

t_case "a stable cap-hit that ends the review on its round cap and goes on to ci holds the review"
t_eq "$(tehas_headless_progress "$fx/progress.jsonl" | jq -c .reviewHeld)" false
printf '%s\n' '{"v":1,"kind":"cap-hit","key":"7:1:a","event":{"cap":"round-cap","nextStep":"ci"}}' >"$T/p.jsonl"
t_eq "$(tehas_headless_progress "$T/p.jsonl" | jq -c .reviewHeld)" true
printf '%s\n' '{"v":1,"kind":"cap-hit","key":"7:1:a","event":{"cap":"round-cap","nextStep":"handoff"}}' \
  '{"v":1,"kind":"cap-hit","unstable":true,"event":{"cap":"round-cap","nextStep":"ci"}}' >"$T/p.jsonl"
t_eq "$(tehas_headless_progress "$T/p.jsonl" | jq -c .reviewHeld)" false

t_case "the features this command relies on are checked, and only those asked for"
t_eq "$(tehas_headless_unsupported "$scaps" "--json" "" "handoffChannel cycleDeadline restartClearsAttention")" ""
t_eq "$(tehas_headless_unsupported "$(jq -c 'del(.features.handoffChannel) | .features.cycleDeadline = "yes"' <<<"$scaps")" "--json" "" \
  "handoffChannel cycleDeadline restartClearsAttention")" "the feature handoffChannel
the feature cycleDeadline"
t_eq "$(tehas_headless_unsupported "$(jq -c 'del(.features)' <<<"$scaps")" "--json" "" "")" ""
t_eq "$(tehas_headless_unsupported "$(jq -c '.features = [1]' <<<"$scaps")" "--json" "" "handoffChannel")" "the feature handoffChannel"

t_case "a workaround expires at the release that fixes it"
t_eq "$(tehas_headless_expired "$scaps" "")" ""
t_eq "$(tehas_headless_expired "$scaps" "PI_ENSEMBLE_COMMENT_RETENTION@0.12.80 TEHAS_WORK_IMAGES@0.12.81 PI_ENSEMBLE_X@0.9.100")" \
  "PI_ENSEMBLE_COMMENT_RETENTION works around a pi-rukas defect fixed in 0.12.80, and the image's pi-rukas is 0.14.0
TEHAS_WORK_IMAGES works around a pi-rukas defect fixed in 0.12.81, and the image's pi-rukas is 0.14.0
PI_ENSEMBLE_X works around a pi-rukas defect fixed in 0.9.100, and the image's pi-rukas is 0.14.0"
t_eq "$(tehas_headless_expired "$(jq -c '.version = "0.13.1"' <<<"$scaps")" "TEHAS_WORK_IMAGES@0.12.81")" \
  "TEHAS_WORK_IMAGES works around a pi-rukas defect fixed in 0.12.81, and the image's pi-rukas is 0.13.1"
for bad in '.version = null' '.version = "0.12.80; rm"' 'del(.version)'; do
  t_contains "$(tehas_headless_expired "$(jq -c "$bad" <<<"$scaps")" "TEHAS_WORK_IMAGES@0.12.81")" "version could not be read"
done

t_case "the outcome carries the handoff's body as rendered and an intent park's reason as a word"
t_contains "$(tehas_headless_outcome "$sx/result.json" 7 | jq -r .handoffBody)" "<!-- pi-rukas:handoff issue=7 cap=explore-bodies-empty -->
## "
jq '.issues[0].parkReason = "too-large\n<b>" | .issues[0].handoff.body = 7' "$sx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq -c '[.parkReason, .handoffBody]')" '["too-largeb",""]'
jq --arg b "$(head -c 40000 /dev/zero | tr '\0' x)" '.issues[0].handoff.body = $b' "$sx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq '.handoffBody | length')" 30000

t_case "the outcome of the fixture is its handoff"
t_eq "$(tehas_headless_outcome "$sx/result.json" 7 | jq -c '[.status, .cap, .failedStep, .lastStep, .exitCode, .resumable, .buildVersion, .usage.dispatches]')" \
  '["handoff","explore-bodies-empty","explore","handoff",10,false,"0.14.0",0]'
t_eq "$(tehas_headless_outcome "$sx/result.json" 7 | jq -r .buildCommit)" c5ddc0ab9de62edeaac948263d7f8a8f5b123066

t_case "another issue, another schema or no file is no outcome"
tehas_headless_outcome "$sx/result.json" 8 >/dev/null
t_eq "$?" 4
for v in 2 3 4 6; do
  jq --argjson v "$v" '.schemaVersion = $v' "$sx/result.json" >"$T/r.json"
  t_eq "$(tehas_headless_outcome "$T/r.json" 7)" ""
done
t_eq "$(tehas_headless_outcome "$fx/result.json" 7)" ""
t_eq "$(tehas_headless_outcome "$nx/result.json" 7)" ""
t_eq "$(tehas_headless_outcome "$T/missing.json" 7)" ""

t_case "an exit code that is missing or not a number is null, not 0"
jq 'del(.issues[0].exitCode)' "$sx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq -c .exitCode)" null
jq '.issues[0].exitCode = "0"' "$sx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq -c .exitCode)" null

t_case "a schema 5 result with no issue entry is error only, and one with entries is not"
echo '{"schemaVersion":5,"issues":[],"error":{"code":"refused"}}' >"$T/r.json"
tehas_headless_error_only "$T/r.json"
t_eq "$?" 0
echo '{"schemaVersion":5,"error":{"code":"refused"}}' >"$T/r.json"
tehas_headless_error_only "$T/r.json"
t_eq "$?" 0
tehas_headless_error_only "$sx/result.json"
t_eq "$?" 1
for bad in '{"schemaVersion":3,"issues":[]}' '{"schemaVersion":5,"issues":{}}' '[]' 'not json'; do
  echo "$bad" >"$T/r.json"
  tehas_headless_error_only "$T/r.json"
  t_eq "$?" 1
done

t_case "the outcome's text is plain characters and its numbers are numbers"
jq '.issues[0].status = "handoff\n<!-- x -->" | .issues[0].branch = "a b;`c`" | .issues[0].usage.input = "9; rm"' "$sx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq -c '[.status, .branch, .usage.input]')" '["handoff--x--","abc",0]'

t_done
