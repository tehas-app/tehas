#!/usr/bin/env bash
# Tests for tehas start: the host's approval of an issue on the state store,
# what it refuses, and --clear. gh is stubbed: it answers the issue from
# $T/issue.json, or fails as GitHub does for a number that is no issue. The
# start the approval leads to is the dispatcher's, and its fixtures test it
# (console/dispatch/tests/fixtures/host_start.json).
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")
start_cmd="$T_ROOT/bin/tehas start"
th=$T/home/tehas
# The time of an approval, as a line prints it.
ISO='[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z'

# gh api repos/<repo>/issues/<n>: $T/issue.json, or the 404 GitHub answers.
gh_stub() {
  t_stub gh 'case "$1 $2" in
  "api repos/acme/widget/issues/12") cat "$T/issue.json" ;;
  "api repos/acme/widget/issues/"*) echo "gh: Not Found (HTTP 404)" >&2; exit 1 ;;
esac'
}
issue() { # <state> [<extra json>]
  printf '{"number": 12, "state": "%s", "title": "A title", "body": "Words."%s}\n' "$1" "${2:-}" >"$T/issue.json"
}

reset() {
  rm -rf "$th" "$T"/calls/*
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  t_store_home "$th"
  gh_stub
  issue open
}

waiting() { t_row approval-current 12 -- '.approval | "\(.how) \(.by) \(.body_sha256)"'; }
matches() { [[ $1 =~ ^$2$ ]] || t_bad "'$1' does not match $2"; }

t_case "an open issue is approved from the host, and nothing starts"
reset
t_run "$start_cmd 12"
t_eq "$t_rc" 0
matches "$t_out" "approved #12 from the host at $ISO: the next poll starts it when there is room and nothing holds it"
t_eq "$(waiting)" "host host $T_WSUM"
t_eq "$(t_row approval-current 12 -- '.current')" null
t_eq "$(t_calls gh)" "api repos/acme/widget/issues/12"

t_case "--json answers with what was recorded"
reset
t_run "$start_cmd 12 --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.api, .issue, .cleared, .approval.by, .approval.how, .approval.body_sha256]' <<<"$t_out")" \
  "[1,12,false,\"host\",\"host\",\"$T_WSUM\"]"
matches "$(jq -r '.approval.at' <<<"$t_out")" "$ISO"
t_eq "$(jq -r '.approval.title_sha256 | length' <<<"$t_out")" 64

t_case "an unknown issue is refused with a clear message and nothing is recorded"
reset
t_run "$start_cmd 99"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas start: acme/widget has no issue #99; nothing was recorded"
t_eq "$(t_row approval-current 99 -- '.approval')" null

t_case "a pull request and a closed issue are refused"
reset
issue open ', "pull_request": {"url": "x"}'
t_run "$start_cmd 12"
t_eq "$t_rc" 1
t_contains "$t_out" "#12 is a pull request"
issue closed
t_run "$start_cmd 12"
t_eq "$t_rc" 1
t_contains "$t_out" "#12 is closed"
t_eq "$(t_row approval-current 12 -- '.approval')" null

t_case "--clear withdraws an approval that waits, and a second --clear has nothing to do"
reset
t_run "$start_cmd 12"
t_eq "$t_rc" 0
t_run "$start_cmd 12 --clear"
t_eq "$t_rc" 0
matches "$t_out" "withdrew the host's approval of #12 of $ISO"
t_eq "$(t_row approval-current 12 -- '.approval')" null
t_run "$start_cmd 12 --clear --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.issue, .cleared, .approval]' <<<"$t_out")" "[12,false,null]"
# --clear asks GitHub for nothing.
t_eq "$(t_calls gh)" "api repos/acme/widget/issues/12"

t_case "--clear leaves an approval of another kind alone"
reset
t_approve "$th" 12 alice label
t_run "$start_cmd 12 --clear"
t_eq "$t_rc" 0
t_contains "$t_out" "#12 has no approval from the host waiting"
t_eq "$(t_row approval-current 12 -- '.approval.how')" label

t_case "a bad argument writes nothing and exits 64"
reset
for args in "" "0" "12 13" "12 --soon" "x"; do
  t_run "$start_cmd $args"
  t_eq "$t_rc" 64
done
t_eq "$(t_calls gh)" ""

t_done
