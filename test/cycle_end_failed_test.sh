#!/usr/bin/env bash
# Tests for libexec/cycle: the record of a cycle whose last GitHub step failed,
# state/cycles/<issue>.end-failed.json, which says the step and whether the
# failure was GitHub's. The poll that finishes it is
# test/dispatch_auto_finish_test.sh.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

record=$state/7.end-failed.json
# A cycle that stopped in commit-pr with no pull request, on <message>.
in_commit_pr() { # <message>
  ended handoff commit-pr feature/issue-7 step-failed:commit-pr
  jq --arg m "$1" '.issues[0].failure = {kind: "tooling", step: "commit-pr", message: $m}' \
    "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
}
said() { jq -c '[.step, .github, .why, .tries, .stopped]' "$record"; }

t_case "a push that failed 502 is on record as GitHub's"
reset
in_commit_pr "git push origin feature/issue-7: fatal: unable to access: The requested URL returned error: 502"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(said)" '["push",true,"returned error: 502",[],""]'
t_eq "$(jq .issue "$record")" 7
t_contains "$t_out" "#7: its push failed on GitHub's side (returned error: 502): on record"

t_case "a pull request refused 422 is on record as the request's own"
reset
in_commit_pr "gh pr create: HTTP 422: Validation Failed"
t_run "$cycle 7"
t_eq "$(said)" '["pr",false,"HTTP 422",[],""]'

t_case "a step that failed for anything but GitHub is no record, and the record an earlier cycle left goes"
reset
mkdir -p "$state"
echo '{"issue":7,"attempt":1,"step":"push","github":true,"why":"HTTP 502","at":1,"tries":[],"stopped":"","said":false}' >"$record"
in_commit_pr "cargo test failed in other.rs"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(ls "$record" 2>/dev/null)" ""

t_case "a handoff comment that failed 503 is on record as GitHub's"
reset
ended handoff develop feature/issue-7 verify-timeout:develop
jq '.issues[0].handoff = {channel: "host", body: "<!-- pi-rukas:handoff issue=7 cap=verify-timeout -->\nNot pushed."}' \
  "$fx/result.json" >"$fx/r" && mv "$fx/r" "$fx/result.json"
# The park's question and the comment in its place both fail.
: >"$fx/post-fails"
echo "HTTP 503: Service Unavailable" >"$fx/comment-fails"
t_run "$cycle 7"
t_contains "$t_out" "WARN: the handoff of #7 reached no comment"
t_eq "$(said)" '["handoff",true,"HTTP 503",[],""]'

t_done
