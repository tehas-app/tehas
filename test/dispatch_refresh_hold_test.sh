#!/usr/bin/env bash
# Tests for libexec/dispatch: what holds a refresh, the refresh lane, and the
# tehas:refreshing mark.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# --- Refreshes (rule 4b) -------------------------------------------------------
# Issue 7 waits for a merge of pull request 50, which conflicts with main. Its
# body is "Words.", which alice approved when she labelled its last cycle, and
# tehasbot refresh asked for a refresh. The image takes --continue-pr
# (features.continuePr).
refresh_dir=$T/home/tehas/state/refresh
rhead=$(printf 'a%.0s' {1..40})
rbase=$(printf 'c%.0s' {1..40})
# pull <jq changes>: pull request 50 as GitHub answers for it now.
pull() {
  jq -n --arg head "$rhead" --arg base "$rbase" '{data: {repository: {pullRequest: {state: "OPEN", number: 50,
    url: "https://github.com/acme/widget/pull/50", isDraft: false, isCrossRepository: false,
    headRefOid: $head, baseRefName: "main", baseRefOid: $base, mergeable: "CONFLICTING",
    author: {login: "tehas-bot"}, headRepository: {nameWithOwner: "acme/widget"}, labels: {nodes: []},
    closingIssuesReferences: {totalCount: 1, nodes: [{number: 7, state: "OPEN",
      labels: {nodes: [{name: "tehas:awaiting-merge"}]},
      closedByPullRequestsReferences: {nodes: [{number: 50, state: "OPEN"}]}}]}}}}}' |
    jq "{data: {repository: {pullRequest: (.data.repository.pullRequest | ${1:-.})}}}" >"$fx/pull-50.json"
}
# issue7 <label>...: what GitHub says issue 7 carries now.
# shellcheck disable=SC2120  # the cases here pass no labels
issue7() {
  printf '%s\n' tehas:awaiting-merge "$@" | jq -R '{name: .}' | jq -s '{number: 7, state: "open", labels: .}' >"$fx/work-7.json"
}
# shellcheck disable=SC2120  # the cases here pass no settings
refresh_setup() { # [<instance settings>...]
  reset
  rm -rf "$refresh_dir" "$fx"/pull-* "$fx"/refreshing.json "$fx"/caps.json "$fx"/mark-fails "$fx"/mark-fails-times "$fx"/unmark-fails "$fx"/label-missing
  slots 2 TEHAS_REFRESH_CONFLICTS=1 "$@"
  echo '[]' >"$fx/ready.json"
  echo '[]' >"$fx/events-7.json"
  echo '[{"number":7}]' >"$fx/awaiting.json"
  jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Words."}}}}' >"$fx/edit-7.json"
  echo '[]' >"$fx/icomments-7.json"
  issue7
  pull
  echo '{"verbs":{"work":{"flags":["--json","--timeout","--restart","--continue-pr"]}},"features":{"continuePr":true}}' >"$fx/caps.json"
  mkdir -p "$refresh_dir" "$T/home/tehas/state/cycles"
  jq -n --arg h "$rhead" --arg b "$rbase" '{at: "2026-10-17T09:00:00Z", pr: 50, head_sha: $h, base_sha: $b}' >"$refresh_dir/7.json"
  # alice's label started the last cycle, which waits for a merge.
  past 7 alice label 2026-10-01T00:00:00Z
}
# Refreshes on alice's approval that ran and ended, the last one parked when
# asked.
refresh_record() { # <refreshes> [parked]
  local i a parked
  for ((i = 1; i <= $1; i++)); do
    a=$(t_row cycle-show 7 -- '.cycle.attempt // 0')
    t_approve "$T/home/tehas" 7 alice refresh $(($(date -u -d 2026-10-02T00:00:00Z +%s) + i)) \
      --refresh-pr 50 --refresh-head "$rhead"
    t_started "$T/home/tehas" 7
    parked=()
    [ "$i" != "$1" ] || [ "${2:-}" != parked ] || parked=(--parked)
    TEHAS_HOME=$T/home/tehas flock -w 5 "$T/home/tehas/state/cycles/7.lock" "$T_STORE" cycle-end 7 --attempt $((a + 1)) \
      --status handoff "${parked[@]}" >/dev/null || t_bad "refresh_record: no end"
  done
}
# A running refresh: an open cycle the state store says started as one, on
# alice's approval of an earlier cycle.
running_refresh() { # <issue>
  past "$1" alice label 2026-09-30T00:00:00Z
  t_approve "$T/home/tehas" "$1" alice refresh "$(date -u -d 2026-10-01T00:00:00Z +%s)" \
    --refresh-pr 50 --refresh-head "$rhead"
  t_started "$T/home/tehas" "$1"
  hold "$1"
  # A running cycle's issue is marked running, as on a host: the queue the
  # Rust poll reads holds every open issue, so one not on it is closed.
  jq -c --argjson n "$1" '. + [{number: $n, labels: [{name: "tehas:running"}]}]' "$fx/marked.json" >"$fx/marked.json.new" &&
    mv "$fx/marked.json.new" "$fx/marked.json"
}
# The REST call of rule 4b that takes the mark off a pull request.
unmark_call="api -X DELETE repos/acme/widget/issues/50/labels/tehas:refreshing"

t_case "at its cap a refresh is held and said on the issue once"
refresh_setup
refresh_record 2
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "drop the refresh of #7: it was refreshed 2 times since alice approved it, the limit"
t_eq "$(t_calls gh | grep -c 'issue comment 7 .*does not refresh')" 1
jq -n --arg h "$rhead" --arg b "$rbase" '{at: "2026-10-17T10:00:00Z", pr: 50, head_sha: $h, base_sha: $b}' >"$refresh_dir/7.json"
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c 'issue comment 7 .*does not refresh')" 1

t_case "TEHAS_REFRESH_MAX sets the cap"
refresh_setup
slots 2 TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_MAX=3
refresh_record 2
t_run "$dispatch"
t_eq "$(started)" "7 "

t_case "after a refresh that parked a refresh is held and said on the issue once"
refresh_setup
refresh_record 1 parked
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "a refresh of it since alice approved it parked"
t_eq "$(t_calls gh | grep -c 'issue comment 7 .*does not refresh')" 1
jq -n --arg h "$rhead" --arg b "$rbase" '{at: "2026-10-17T10:00:00Z", pr: 50, head_sha: $h, base_sha: $b}' >"$refresh_dir/7.json"
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c 'issue comment 7 .*does not refresh')" 1

t_case "a park of a cycle that was no refresh does not hold one"
refresh_setup
t_store_db "$T/home/tehas"
past 7 alice label 2026-10-01T00:00:00Z --parked
t_run "$dispatch"
t_eq "$(started)" "7 "

t_case "the pause flag holds a refresh"
refresh_setup
pause_flag '{"by":"alice","at":"2026-10-17T00:00:00Z","reason":""}'
t_run "$dispatch"
t_eq "$(started)" ""
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "with TEHAS_REFRESH_SLOTS=0 full slots hold a refresh"
refresh_setup
slots 1 TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_SLOTS=0
hold 9
t_run "$dispatch"
release
t_eq "$(started)" ""
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "the limit holds a refresh"
refresh_setup
limit 0
t_run "$dispatch"
t_eq "$(started)" ""

t_case "the awaiting-merge cap does not hold a refresh, and holds the ready issues"
refresh_setup
slots 2 TEHAS_REFRESH_CONFLICTS=1 TEHAS_MAX_AWAITING=2
echo '[{"number":7},{"number":20},{"number":21}]' >"$fx/awaiting.json"
queue 3
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "

t_case "refreshes go after tehas:urgent and before tehas:next"
refresh_setup
slots 4 TEHAS_REFRESH_CONFLICTS=1
queue 3 "8 tehas:urgent" "9 tehas:next"
label_events 8 tehas:urgent=alice
label_events 9 tehas:next=alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "8 7 9 3 "

t_case "with TEHAS_REFRESH_SLOTS=0 a refresh takes a slot before tehas:next does"
refresh_setup
slots 1 TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_SLOTS=0
queue "9 tehas:next"
label_events 9 tehas:next=alice
t_run "$dispatch"
t_eq "$(started)" "7 "

# --- The refresh lane ----------------------------------------------------------
# The slice of reset holds four containers: a quarter of it is a container's
# ceiling.

t_case "the lane: with every cycle slot busy a refresh starts, and the hold says the lane"
refresh_setup
slots 3 TEHAS_REFRESH_CONFLICTS=1
hold 9; hold 10; hold 11
t_run "$dispatch"
release
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_contains "$t_out" "holding: 3 cycles are running (limit 3); the refresh lane has 1 of 1 free"
t_eq "$(jq -c '{free, refreshRoom}' "$dispatch_json")" '{"free":0,"refreshRoom":1}'

t_case "the lane and a free slot: the refresh and a ready issue both start"
refresh_setup
slots 2 TEHAS_REFRESH_CONFLICTS=1
queue 20
hold 9
t_run "$dispatch"
release
t_eq "$(started)" "7 20 "

t_case "a running refresh fills the lane: the next one waits and a cycle slot does not take it"
refresh_setup
slots 3 TEHAS_REFRESH_CONFLICTS=1
running_refresh 8
t_run "$dispatch"
release
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the refresh lane is full (1 of 1)"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "a running refresh does not count against TEHAS_TICKET_CONCURRENCY"
refresh_setup
slots 1 TEHAS_REFRESH_CONFLICTS=1
rm "$refresh_dir/7.json"
queue 20
running_refresh 8
t_run "$dispatch"
release
t_eq "$(started)" "20 "

t_case "TEHAS_REFRESH_SLOTS=2 runs two refreshes side by side"
refresh_setup
slots 2 TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_SLOTS=2
hold 9; hold 10
running_refresh 8
t_run "$dispatch"
release
t_eq "$(started)" "7 "

t_case "new cycles leave the lane's room in the slice: three of four start, and the poll says why"
refresh_setup
slots 4 TEHAS_REFRESH_CONFLICTS=1
rm "$refresh_dir/7.json"
queue 20 21 22 23
t_run "$dispatch"
t_eq "$(started)" "20 21 22 "
t_contains "$t_out" "holding: 3 of 4 free slots start: tehas.slice has room for 4 more and the refresh lane keeps 1"

t_case "with three cycles in a slice of four the lane keeps the last room from a ready issue"
refresh_setup
slots 4 TEHAS_REFRESH_CONFLICTS=1
rm "$refresh_dir/7.json"
queue 20
hold 9; hold 10; hold 11
t_run "$dispatch"
release
t_eq "$(started)" ""
t_contains "$t_out" "holding: tehas.slice has room for 1 more and the refresh lane keeps 1"
t_eq "$(jq -r .why "$dispatch_json")" lane

t_case "a full slice holds a refresh and a ready issue alike"
refresh_setup
slots 4 TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_SLOTS=2
queue 20
hold 9; hold 10; hold 11; hold 12
t_run "$dispatch"
release
t_eq "$(started)" ""
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "with refreshes off the lane keeps nothing: four of four start"
refresh_setup
slots 4
rm "$refresh_dir/7.json"
queue 20 21 22 23
t_run "$dispatch"
t_eq "$(started)" "20 21 22 23 "
t_eq "$(jq -c 'has("refreshRoom")' "$dispatch_json")" false

t_case "a dry run says what a refresh would start, and changes nothing"
refresh_setup
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7 (refresh by alice)"
t_eq "$(started)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "pr edit"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

# The reaper of rule 2: the pull requests marked tehas:refreshing.
marked_refreshing() { echo '[{"number":50,"labels":[{"name":"tehas:refreshing"}]}]' >"$fx/refreshing.json"; }

t_case "a leftover tehas:refreshing is taken off a pull request whose issue has no cycle"
refresh_setup
rm -f "$refresh_dir/7.json"
refresh_record 1
marked_refreshing
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "$unmark_call"
t_contains "$t_out" "clear pull request #50: tehas:refreshing off, no cycle runs on #7"

t_case "the leftover is cleared while the factory is paused"
refresh_setup
rm -f "$refresh_dir/7.json"
refresh_record 1
marked_refreshing
pause_flag '{"by":"alice","at":"2026-10-17T00:00:00Z","reason":""}'
t_run "$dispatch"
t_contains "$(t_calls gh)" "$unmark_call"

t_case "the mark of a running refresh stays when the bot applied it"
refresh_setup
rm -f "$refresh_dir/7.json"
refresh_record 1
marked_refreshing
echo '[{"event":"labeled","label":{"name":"tehas:refreshing"},"actor":{"login":"tehas-bot"},"created_at":"2026-10-17T00:00:00Z"}]' >"$fx/events-50.json"
hold 7
t_run "$dispatch"
release
t_lacks "$(t_calls gh)" "--remove-label tehas:refreshing"

t_case "a tehas:refreshing applied by anyone else is taken off, also while a refresh runs"
refresh_setup
rm -f "$refresh_dir/7.json"
refresh_record 1
marked_refreshing
echo '[{"event":"labeled","label":{"name":"tehas:refreshing"},"actor":{"login":"alice"},"created_at":"2026-10-17T00:00:00Z"}]' >"$fx/events-50.json"
hold 7
t_run "$dispatch"
release
t_contains "$(t_calls gh)" "$unmark_call"
t_contains "$t_out" "it was applied by 'alice', not by tehas"

t_case "a tehas:refreshing on a pull request no refresh of this host names is taken off"
refresh_setup
rm -f "$refresh_dir/7.json"
marked_refreshing
t_run "$dispatch"
t_contains "$t_out" "clear pull request #50: tehas:refreshing off, no refresh on this host names it"

t_case "a host that never refreshed does not ask for the mark"
reset
t_run "$dispatch"
t_lacks "$(t_calls gh)" "tehas:refreshing"

t_case "a history of refreshes the store cannot read clears no mark"
refresh_setup
rm -f "$refresh_dir/7.json"
refresh_record 1
marked_refreshing
store_refuses "history-show --kind cycle --how refresh" 1 'store: busy: the database is locked'
t_run "$dispatch"
t_lacks "$(t_calls gh)" "$unmark_call"
t_contains "$t_out" "WARN: the history of refreshes could not be read (busy)"

t_case "an approval the store cannot read clears no mark"
refresh_setup
rm -f "$refresh_dir/7.json"
refresh_record 1
marked_refreshing
store_refuses "approval-current 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_lacks "$(t_calls gh)" "$unmark_call"
t_contains "$t_out" "WARN: the approval of #7 could not be read (busy)"

t_done
