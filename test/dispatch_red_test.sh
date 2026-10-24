#!/usr/bin/env bash
# Tests for libexec/dispatch: no new cycle while the check TEHAS_BRANCH_CHECK
# names failed on the head commit of the branch.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

sha=0123456789abcdef0123456789abcdef01234567
checks() { # <status> [<conclusion>]: the one run of the check on the head
  jq -nc --arg sha "$sha" --arg s "$1" --arg c "${2:-}" \
    '{total_count: 1, check_runs: [{name: "verify", head_sha: $sha, status: $s,
      conclusion: (if $c == "" then null else $c end)}]}' >"$fx/checks.json"
}
red_instance() { slots 2 TEHAS_BRANCH_CHECK=verify; }
check_calls() { t_calls gh | grep -c 'commits/main/check-runs'; }

t_case "red: a failed check on the head starts nothing and says so"
reset
red_instance
ready 7 8
checks completed failure
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -r .why "$dispatch_json")" red
t_eq "$(jq -r .hold "$dispatch_json")" "holding: main is red at 0123456789ab"
t_contains "$t_out" "holding: main is red at 0123456789ab"
t_eq "$(check_calls)" 1
t_contains "$(t_calls gh)" "api repos/acme/widget/commits/main/check-runs?check_name=verify&filter=latest"
t_lacks "$(t_calls gh)" "--add-label tehas:running"

t_case "red: a timed-out or a startup failure is red too"
for c in timed_out startup_failure; do
  reset
  red_instance
  checks completed "$c"
  t_run "$dispatch"
  [ -z "$(t_calls fake-start)" ] || t_bad "$c: a cycle started"
  [ "$(jq -r .why "$dispatch_json")" = red ] || t_bad "$c: why is $(jq -r .why "$dispatch_json")"
done

t_case "red: said on every poll while issues wait"
reset
red_instance
checks completed failure
t_run "$dispatch"
rm -f "$T"/calls/*
t_run "$dispatch"
t_contains "$t_out" "holding: main is red at 0123456789ab"
t_eq "$(jq -r .why "$dispatch_json")" red

t_case "red: passed, pending, cancelled or skipped, the cycles start as before"
for st in "completed success" "in_progress" "queued" "completed cancelled" "completed skipped" "completed neutral"; do
  reset
  red_instance
  ready 7 8
  # shellcheck disable=SC2086  # the status and the conclusion as two words
  checks $st
  t_run "$dispatch"
  [ "$(started)" = "7 8 " ] || t_bad "$st: started '$(started)'"
  [ "$(jq -r .why "$dispatch_json")" = "" ] || t_bad "$st: why is $(jq -r .why "$dispatch_json")"
done

t_case "red: a pending check is said once, with why the poll could not tell"
reset
red_instance
checks in_progress
t_run "$dispatch"
t_contains "$t_out" "the check verify on main at 0123456789ab has not finished: cycles start"
rm -f "$T"/calls/*
t_run "$dispatch"
t_lacks "$t_out" "has not finished"

t_case "red: no such check on the head, the cycles start and the log says why"
reset
red_instance
echo '{"total_count":0,"check_runs":[]}' >"$fx/checks.json"
t_run "$dispatch"
t_eq "$(started)" "7 "
t_contains "$t_out" "main has no check named verify on its head commit: cycles start"

t_case "red: a call that fails, hangs or answers nonsense starts cycles and says why"
for kind in fails hangs nonsense badsha; do
  reset
  red_instance
  case "$kind" in
    fails) : >"$fx/checks-fails" ;;
    hangs) : >"$fx/checks-hangs" ;;
    nonsense) echo 'not json' >"$fx/checks.json" ;;
    badsha) echo '{"check_runs":[{"head_sha":"<b>","status":"completed","conclusion":"failure"}]}' >"$fx/checks.json" ;;
  esac
  T_ENV+=(TEHAS_CHECK_WAIT=1)
  t_run "$dispatch"
  [ "$t_rc" = 0 ] || t_bad "$kind: exit $t_rc"
  [ "$(started)" = "7 " ] || t_bad "$kind: started '$(started)'"
  case "$t_out" in
    *"the check verify on main could not be read: cycles start"*) ;;
    *) t_bad "$kind: the log does not say why: $t_out" ;;
  esac
done

t_case "red: no check named, GitHub is not asked"
reset
slots 2
checks completed failure
t_run "$dispatch"
t_eq "$(started)" "7 "
t_eq "$(check_calls)" 0

t_case "red: a retry tick waits while the branch is red"
parked
red_instance
checks completed failure
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "red: a planning session still starts, and an answer still runs a turn"
to_plan
red_instance
checks completed failure
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out"
planning
red_instance
checks completed failure
comments 7 "$(plan_body ' ' x ' ')" alice
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123 --answer $keep/answer.md"

t_case "red: a running cycle goes on"
reset
red_instance
echo '[{"number":5,"labels":[{"name":"tehas:running"}]}]' >"$fx/marked.json"
hold 5
checks completed failure
t_run "$dispatch"
release
t_lacks "$(t_calls gh)" "issue edit 5"
t_eq "$(jq -r .why "$dispatch_json")" red

t_case "red: green again, the hold is released"
reset
red_instance
checks completed failure
t_run "$dispatch"
checks completed success
rm -f "$T"/calls/*
t_run "$dispatch"
t_contains "$t_out" "released: cycles may start again"
t_eq "$(started)" "7 "

t_case "red: a dry run says it and starts nothing"
reset
red_instance
checks completed failure
t_run "$dispatch" --dry-run
t_contains "$t_out" "holding: main is red at 0123456789ab"
t_lacks "$t_out" "would run a cycle"

t_done
