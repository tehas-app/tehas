#!/usr/bin/env bash
# Tests for lib/headless.sh: what tehas reads of pi-rukas's headless runner.
# The fixtures in test/fixtures/headless-0.12.80 are what the v0.12.80 runner
# printed in a throwaway container with no model and no token: its
# --capabilities answer, and the NDJSON progress and --json result of
# `work 7`, which ended in a handoff.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"
# shellcheck source=lib/headless.sh
. "$T_ROOT/lib/headless.sh"
fx=$T_ROOT/test/fixtures/headless-0.12.80
caps=$(cat "$fx/capabilities.json")
# What tehas work gives a cycle's container, of PI_ENSEMBLE_* names.
given="PI_ENSEMBLE_SANDBOX_MODE PI_ENSEMBLE_WORKSPACE_ROOT PI_ENSEMBLE_MAX_WORKSTREAMS PI_ENSEMBLE_VERIFY_TIMEOUT_MS
  PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER"

t_case "the v0.12.80 runner supports what tehas gives a cycle"
t_eq "$(tehas_headless_unsupported "$caps" "--timeout 360 --json --restart" "$given PI_ENSEMBLE_EXTRA_IMAGE_DIRS")" ""

t_case "a role's model and provider are inputs of their family"
t_eq "$(tehas_headless_unsupported "$caps" "--json" "PI_ENSEMBLE_MODEL_DEVELOPER PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST")" ""

t_case "a name the build does not read, one it sets on its own children and one of its wrapper are each named"
t_eq "$(tehas_headless_unsupported "$caps" "--json" "PI_ENSEMBLE_NOT_A_LEVER PI_ENSEMBLE_SUBAGENT_MODE PI_ENSEMBLE_NO_SSH")" \
  "the input PI_ENSEMBLE_NOT_A_LEVER
the input PI_ENSEMBLE_SUBAGENT_MODE
the input PI_ENSEMBLE_NO_SSH"

t_case "only the flags are checked, not their values"
t_eq "$(tehas_headless_unsupported "$caps" "--timeout 360 --json --bogus" "")" "the flag --bogus"

t_case "each contract version is checked"
t_eq "$(tehas_headless_unsupported "$(jq -c '.contracts.exitCodes.version = 2 | .contracts.resultSchema.version = 4 | .contracts.ndjson.stable.version = 9' <<<"$caps")" "" "")" \
  "exit codes version 2, not 1
result schema version 4, not 3
stable NDJSON version 9, not 1"
t_contains "$(tehas_headless_unsupported '{}' "" "")" "exit codes version null"

t_case "an answer that is not an object is no answer"
for bad in "" "not json" "[]" "7" "null"; do
  tehas_headless_unsupported "$bad" "" "" >/dev/null
  t_eq "$?" 1
done

t_case "a name in the answer is a pattern only of name characters"
t_eq "$(tehas_headless_unsupported '{"contracts":{"exitCodes":{"version":1},"resultSchema":{"version":3},"ndjson":{"stable":{"version":1}}},
  "inputs":[{"name":"PI_ENSEMBLE_.*","scope":"cycle"},{"name":"PI_ENSEMBLE_A|PI_ENSEMBLE_B","scope":"cycle"}]}' "" "PI_ENSEMBLE_A")" \
  "the input PI_ENSEMBLE_A"

t_case "what is said of the answer is plain characters"
t_eq "$(tehas_headless_unsupported '{"contracts":{"exitCodes":{"version":"1\u001b[31m;x|y"}}}' "" "" | head -n 1)" \
  "exit codes version 131mxy, not 1"

t_case "the live line is the last stable step and the driver's last notice, without its marker line"
t_eq "$(tehas_headless_now "$fx/progress.jsonl" | head -n 1)" handoff
t_eq "$(tehas_headless_now "$fx/progress.jsonl" | sed -n 2p)" "⚠ pi-rukas /work for issue #7 — HANDOFF DISPATCH INCOMPLETE"
t_eq "$(tehas_headless_now /dev/null | tr '\n' ,)" ",,"

t_case "the progress of the fixture has no completed dispatch and the reason of its one failed dispatch"
t_eq "$(tehas_headless_progress "$fx/progress.jsonl" | jq -c .roles)" "[]"
t_contains "$(tehas_headless_progress "$fx/progress.jsonl" | jq -r .reason)" "cancelled (abort signal)"

t_case "a cap's evidence is the reason before a failed dispatch's"
printf '%s\n' '{"v":1,"kind":"dispatch-failed","unstable":true,"event":{"errorTail":"exit 1"}}' \
  '{"v":1,"kind":"cap-hit","event":{"cap":"x","evidence":"git says no"}}' >"$T/p.jsonl"
t_eq "$(tehas_headless_progress "$T/p.jsonl" | jq -r .reason)" "git says no"
t_eq "$(tehas_headless_progress /dev/null)" '{"roles":[],"reason":""}'

t_case "the outcome of the fixture is its handoff"
t_eq "$(tehas_headless_outcome "$fx/result.json" 7 | jq -c '[.status, .cap, .failedStep, .lastStep, .exitCode, .resumable, .buildVersion, .usage.dispatches]')" \
  '["handoff","explore-bodies-empty","explore","handoff",10,false,"0.12.80",1]'
t_eq "$(tehas_headless_outcome "$fx/result.json" 7 | jq -r .buildCommit)" 1dc61e7dae8c442c8d6e4005a0ceb59a824ccf11

t_case "another issue, another schema or no file is no outcome"
tehas_headless_outcome "$fx/result.json" 8 >/dev/null
t_eq "$?" 4
jq '.schemaVersion = 2' "$fx/result.json" >"$T/r.json"
tehas_headless_outcome "$T/r.json" 7 >/dev/null || t_eq nothing nothing
t_eq "$(tehas_headless_outcome "$T/r.json" 7)" ""
t_eq "$(tehas_headless_outcome "$T/missing.json" 7)" ""

t_case "an exit code that is missing or not a number is null, not 0"
jq 'del(.issues[0].exitCode)' "$fx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq -c .exitCode)" null
jq '.issues[0].exitCode = "0"' "$fx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq -c .exitCode)" null

t_case "a schema 3 result with no issue entry is error only, and one with entries is not"
echo '{"schemaVersion":3,"issues":[],"error":{"code":"refused"}}' >"$T/r.json"
tehas_headless_error_only "$T/r.json"
t_eq "$?" 0
echo '{"schemaVersion":3,"error":{"code":"refused"}}' >"$T/r.json"
tehas_headless_error_only "$T/r.json"
t_eq "$?" 0
tehas_headless_error_only "$fx/result.json"
t_eq "$?" 1
for bad in '{"schemaVersion":2,"issues":[]}' '{"schemaVersion":3,"issues":{}}' '[]' 'not json'; do
  echo "$bad" >"$T/r.json"
  tehas_headless_error_only "$T/r.json"
  t_eq "$?" 1
done

t_case "the outcome's text is plain characters and its numbers are numbers"
jq '.issues[0].status = "handoff\n<!-- x -->" | .issues[0].branch = "a b;`c`" | .issues[0].usage.input = "9; rm"' "$fx/result.json" >"$T/r.json"
t_eq "$(tehas_headless_outcome "$T/r.json" 7 | jq -c '[.status, .branch, .usage.input]')" '["handoff--x--","abc",0]'

t_done
