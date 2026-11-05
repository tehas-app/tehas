#!/usr/bin/env bash
# Tests for libexec/dispatch: tehas:lean through the real read of a poll. The
# rules are in console/dispatch/tests/fixtures/lean.json.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# Issue 7 parked with its Retry box ticked by alice. Its last start named the
# driver given, and it carries tehas:lean from alice.
lean_parked() { # <input of the last start, as JSON> [<reason of the park>]
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:lean"}]}]' >"$fx/asking.json"
  printf '%s' "$1" >"$T/lean-input.json"
  t_approve "$T/home/tehas" 7 alice label 1700000000 --input-file "$T/lean-input.json"
  t_started "$T/home/tehas" 7
  TEHAS_HOME=$T/home/tehas flock -w 5 "$T/home/tehas/state/cycles/7.lock" "$T_STORE" cycle-end 7 --attempt 1 \
    --status failed --parked >/dev/null || t_bad "the store refused the end of #7"
  t_park "$T/home/tehas" 7 "${2:-failed}" 4001
  label_events 7 tehas:lean=alice
  comments 7 "$(park_body x ' ')" alice
}

t_case "a retry tick reads the record of an issue that is no candidate, and keeps its driver"
lean_parked '{"links":[],"driver":"lean"}'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(approval .driver)" lean

t_case "a retry tick of a classic start stays classic through the same read"
lean_parked '{"links":[]}'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(approval .driver)" null

t_case "a retry tick on the park of a refused lean start waits while tehas:lean is on, and the tick stays"
lean_parked '{"links":[],"driver":"lean"}' lean-refused
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: a retry was ticked on the park of a lean start this tehas cannot run; take tehas:lean off first"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "the same tick starts the cycle on classic once tehas:lean is off"
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
label_events 7 tehas:lean=alice -tehas:lean=alice
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(approval .driver)" null

t_case "a lean start whose body holds no pi-spec block starts nothing and says why once"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:lean"}]}]' >"$fx/ready.json"
label_events 7 tehas:lean=alice
echo '{"title":"Faster search","body":"Make it fast, with no block."}' >"$fx/issue-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
# The ask comes off, so this is said once, and tehas:lean stays: with it off
# the next tehas:ready would run classic in lean's place. The comment goes out
# as `--body-file -`, and the stub of test/lib.sh rewrites that to `--body`
# with the text the poll put on standard input, so the whole sentence below is
# the text a person reads.
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took \`tehas:ready\` off this issue and started nothing: its text holds no \`pi-spec v1\` block for the lean driver to read: it is not there, or it is not the one block of valid JSON lean needs. A person writes that block, and applies \`tehas:ready\` again."
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1

t_case "a lean start whose body holds the block is started"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:lean"}]}]' >"$fx/ready.json"
label_events 7 tehas:lean=alice
jq -n '{title: "Faster search",
        body: "```pi-spec v1\n{\"intent\":\"faster search\"}\n```\n\nMake it fast."}' >"$fx/issue-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(t_calls gh | grep -c '^issue comment 7 ')" 1

t_done
