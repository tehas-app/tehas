#!/usr/bin/env bash
# Tests for libexec/dispatch: refreshes of pull requests that conflict, how one
# starts and what drops it.
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
issue7() {
  printf '%s\n' tehas:awaiting-merge "$@" | jq -R '{name: .}' | jq -s '{number: 7, state: "open", labels: .}' >"$fx/work-7.json"
}
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
# A refresh started nothing, and its request is gone.
refresh_dropped() { # <why in the log>
  t_eq "$t_rc" 0
  t_eq "$(started)" ""
  t_lacks "$(t_calls gh)" "--add-label tehas:refreshing"
  t_contains "$t_out" "drop the refresh of #7: $1"
  t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 0
}
# A running refresh: an open cycle the state store says started as one, on
# alice's approval of an earlier cycle.
running_refresh() { # <issue>
  past "$1" alice label 2026-09-30T00:00:00Z
  t_approve "$T/home/tehas" "$1" alice refresh "$(date -u -d 2026-10-01T00:00:00Z +%s)" \
    --refresh-pr 50 --refresh-head "$rhead"
  t_started "$T/home/tehas" "$1"
  hold "$1"
}
# The REST call of rule 4b that marks a pull request, and the one that takes
# the mark off.
mark_call="api -X POST repos/acme/widget/issues/50/labels -f labels[]=tehas:refreshing"
unmark_call="api -X DELETE repos/acme/widget/issues/50/labels/tehas:refreshing"
# The two calls of a start that change a label, the mark of the pull request
# first and the mark of the issue second. What the poll reads before them is
# nothing to a case about what comes after, so it is not counted.
label_pair() { t_calls gh | sed -n '/^api -X POST/p;/^issue edit 7/p'; }
# The reaper of rule 2: the pull requests marked tehas:refreshing.
marked_refreshing() { echo '[{"number":50,"labels":[{"name":"tehas:refreshing"}]}]' >"$fx/refreshing.json"; }

# What the poll counted as refreshes of #7 in the record it wrote: "none"
# when it wrote none at all, which is what a start that never ran leaves.
record_refreshes() {
  local r
  r=$(approval '.refreshes // "none"')
  printf '%s\n' "$r"
}

t_case "a refresh starts: the pull request is marked first, then the issue, and the request goes"
refresh_setup
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_contains "$t_out" "start #7 (refresh by alice)"
edits=$(label_pair)
t_contains "$edits" "$mark_call"
t_contains "$edits" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:awaiting-merge"
t_eq "$(head -n 1 <<<"$edits")" "$mark_call"
t_eq "$(wc -l <<<"$edits")" 2
t_lacks "$(t_calls gh)" "pr edit"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 0

t_case "the start comment says it is a refresh, names the pull request and whose approval it rests on"
t_contains "$(t_calls gh)" "Tehas refresh started on"
t_contains "$(t_calls gh)" "https://github.com/acme/widget/pull/50 conflicts with \`main\`"
t_contains "$(t_calls gh)" "It rests on @alice's approval of 2026-10-01T00:00:00Z."

t_case "the approval record of a refresh: by the person, rests_on, refreshes, refresh_of, and a fixed retry_context"
t_eq "$(approval "[.by, .how, .at, .body_sha256 == \"$WSUM\", .rests_on.by, .rests_on.how, .refreshes, .fixer_starts, .refresh_of.pr, .refresh_of.head_sha == \"$rhead\", .refresh_of.base_sha == \"$rbase\", .refresh_of.branch]")" \
  '["alice","refresh","2026-10-01T00:00:00Z",true,"alice","label",1,0,50,true,true,"main"]'
t_eq "$(approval '[.retry_context[] | {source, by}]')" '[{"source":"refresh","by":"tehas"}]'
t_eq "$(approval '.retry_context[0].text')" \
  'This pull request conflicts with `main`. Merge `main` into its branch, resolve the conflicts so the change still does what the issue asks, run the gate, and push to the same branch. Do no other work.'

t_case "the record was on disk before the issue was marked, and after the mark"
t_eq "$(at_mark .how)" refresh
# On record at the time the poll decided it: the time of the approval it rests
# on is the key of a row that started a cycle already.
t_eq "$(($(t_row approval-current 7 -- .current.at) > $(date -u -d 2026-10-01T00:00:00Z +%s)))" 1
t_eq "$(at_mark '[.refresh_pr, .refresh_head]')" "[50,\"$rhead\"]"
# The mark of the pull request comes before the mark of the issue, so the
# record that says refresh is on disk between them: no poll leaves one whose
# record says refresh and whose issue is not marked.
t_eq "$(head -n 1 <<<"$(label_pair)")" "$mark_call"
t_contains "$(tail -n 1 <<<"$(label_pair)")" "--add-label tehas:running"

t_case "a mark that fails starts nothing, marks no issue, and leaves no refresh in the count"
refresh_setup
: >"$fx/mark-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(started)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "pr edit"
t_contains "$t_out" "could not mark pull request #50 as refreshing"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1
t_eq "$(record_refreshes)" none

t_case "the approval record of the last start is put back, so failed starts spend no refresh"
refresh_setup
refresh_record 1
: >"$fx/mark-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(record_refreshes)" 1
: >"$fx/edit-fails-on"
printf '%s\n' "--add-label tehas:running" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(started)" ""
t_eq "$(record_refreshes)" 1
rm -f "$fx/mark-fails" "$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(started)" "7 "
t_eq "$(record_refreshes)" 2

t_case "the first two marks fail and the third starts the refresh: the failures counted for nothing"
refresh_setup
echo 2 >"$fx/mark-fails-times"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(started)" ""
t_contains "$t_out" "could not mark pull request #50 as refreshing"
t_eq "$(record_refreshes)" none
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(started)" ""
t_eq "$(record_refreshes)" none
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(record_refreshes)" 1
# Three marks were asked for: the two that failed and the one that worked.
t_eq "$(grep -c "^api -X POST" <<<"$(t_calls gh)")" 3
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 0

t_case "the reaper clears a mark its cycle could not take off"
refresh_setup
rm -f "$refresh_dir/7.json"
refresh_record 1
marked_refreshing
: >"$fx/unmark-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: label change on pull request #50 failed: --remove-label tehas:refreshing"
: >"$fx/refreshing.json"
rm -f "$fx/unmark-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "$unmark_call"

t_case "a refresh after a refresh counts on from it"
refresh_setup
refresh_record 1
t_run "$dispatch"
t_eq "$(started)" "7 "
t_eq "$(approval '[.refreshes, .rests_on.by]')" '[2,"alice"]'

t_case "a refresh of a fixer's start rests on the person's approval and keeps the fixer's count"
refresh_setup TEHAS_MILLWRIGHT_LOGIN=tehas-fixer
past 7 tehas-fixer fixer 2026-10-02T00:00:00Z
t_run "$dispatch"
t_eq "$(started)" "7 "
t_eq "$(approval '[.by, .how, .rests_on.by, .fixer_starts, .refreshes]')" '["alice","refresh","alice",1,1]'

t_case "a start whose pull request label fails starts nothing and marks no issue"
refresh_setup
: >"$fx/mark-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(started)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "pr edit"
t_contains "$t_out" "could not mark pull request #50 as refreshing"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "a planning session open on the issue drops the refresh (A13)"
refresh_setup
# The session waits on its question, which nobody has answered.
t_plan "$T/home/tehas" 7 '{"turns":1,"comment":4001}'
echo '[{"number":7}]' >"$fx/asking.json"
comments 7 "$(plan_body ' ' ' ' ' ')" ""
t_run "$dispatch"
refresh_dropped "a planning session is open on it"

t_case "open planning sessions that cannot be read again keep the refresh waiting: the request stays"
refresh_setup
open_read_fails_again
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the open planning sessions could not be read (busy)"
t_lacks "$t_out" "drop the refresh of #7"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "with the setting off a request starts nothing"
refresh_setup
slots 2
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "hold refreshes: TEHAS_REFRESH_CONFLICTS is not 1"

t_case "an image without --continue-pr holds refreshes: said once, and in tehas status's demand"
refresh_setup
echo '{"verbs":{"work":{"flags":["--json","--timeout","--restart"]}}}' >"$fx/caps.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "hold refreshes: the pi-rukas in the sandbox image"
t_contains "$(jq -r .refreshHold "$demand_file")" "does not take --continue-pr"
t_eq "$(jq -r .waiting "$demand_file")" 0
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1
t_run "$dispatch"
t_lacks "$t_out" "hold refreshes"
t_contains "$(jq -r .refreshHold "$demand_file")" "does not take --continue-pr"

t_case "the flag alone is not enough: an answer without features.continuePr holds refreshes"
refresh_setup
echo '{"verbs":{"work":{"flags":["--json","--timeout","--restart","--continue-pr"]}}}' >"$fx/caps.json"
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "hold refreshes: the pi-rukas in the sandbox image"

t_case "an image that cannot be asked holds refreshes too"
refresh_setup
rm -f "$fx/caps.json"
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "hold refreshes"

t_case "a refresh that waits for room counts in the demand"
refresh_setup
slots 1 TEHAS_REFRESH_CONFLICTS=1
running_refresh 9
t_run "$dispatch"
release
t_eq "$(started)" ""
t_eq "$(jq -c '{waiting, oldest, refreshHold}' "$demand_file")" '{"waiting":1,"oldest":"2026-10-17T09:00:00Z","refreshHold":null}'
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "a changed body drops the refresh"
refresh_setup
jq -n '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: "Other words."}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
refresh_dropped "its text is not the text alice approved"

t_case "a body edited by someone else since the approval drops the refresh"
refresh_setup
jq -n '{data: {repository: {issue: {lastEditedAt: "2026-10-05T00:00:00Z", editor: {login: "mallory"}, body: "Words."}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
refresh_dropped "its title or text was changed since alice approved it"

t_case "a changed title drops the refresh"
refresh_setup
echo '[{"event":"renamed","actor":{"login":"mallory"},"created_at":"2026-10-05T00:00:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch"
refresh_dropped "its title or text was changed since alice approved it"

t_case "a moved head drops the refresh"
refresh_setup
pull ".headRefOid = \"$(printf 'b%.0s' {1..40})\""
t_run "$dispatch"
refresh_dropped "pull request #50 moved since the request"

t_case "a pull request that no longer conflicts drops the refresh"
refresh_setup
pull '.mergeable = "MERGEABLE"'
t_run "$dispatch"
refresh_dropped "pull request #50 no longer conflicts"

t_case "a pull request whose state GitHub has not computed waits"
refresh_setup
pull '.mergeable = "UNKNOWN"'
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: GitHub has not said yet"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

for kind in "a closed pull request|.state = \"CLOSED\"|is not open" \
  "a pull request of another account|.author.login = \"alice\"|is not the bot" \
  "a pull request with no draft state|.isDraft = null|its draft state cannot be read" \
  "a second open pull request|.closingIssuesReferences.nodes[0].closedByPullRequestsReferences.nodes += [{number: 51, state: \"OPEN\"}]|not the one open pull request"; do
  t_case "${kind%%|*} drops the refresh"
  refresh_setup
  rest=${kind#*|}
  pull "${rest%%|*}"
  t_run "$dispatch"
  refresh_dropped "pull request #50"
  t_contains "$t_out" "${rest#*|}"
done

t_case "a running cycle drops the refresh"
refresh_setup
hold 7
t_run "$dispatch"
release
refresh_dropped "a cycle runs on it"

for l in tehas:fixer-escalated tehas:millwright-escalated tehas:running tehas:plan; do
  t_case "$l on the issue drops the refresh"
  refresh_setup
  issue7 "$l"
  t_run "$dispatch"
  refresh_dropped "it carries $l"
done

t_case "an issue that no longer waits for a merge drops the refresh"
refresh_setup
echo '{"number":7,"state":"open","labels":[]}' >"$fx/work-7.json"
t_run "$dispatch"
refresh_dropped "it carries neither tehas:awaiting-merge nor needs-human-attention"

# handed_off [<label>...]: issue 7 ended as a handoff, its pull request open.
handed_off() {
  printf '%s\n' needs-human-attention "$@" | jq -R '{name: .}' | jq -s '{number: 7, state: "open", labels: .}' >"$fx/work-7.json"
  pull '.closingIssuesReferences.nodes[0].labels.nodes = [{name: "needs-human-attention"}]'
  echo '[]' >"$fx/awaiting.json"
}

t_case "a handed-off issue's pull request is refreshed: the issue is marked running, with no label to take off"
refresh_setup
handed_off
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
edits=$(label_pair)
t_contains "$edits" "$mark_call"
t_contains "$edits" "issue edit 7 --repo acme/widget --add-label tehas:running"
t_lacks "$edits" "--remove-label"
t_eq "$(approval .how)" refresh

t_case "a draft pull request is refreshed, and it stays a draft"
refresh_setup
pull '.isDraft = true'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_lacks "$(t_calls gh)" "pr edit"

t_case "a handed-off issue that carries tehas:question is refreshed, and the start takes it off"
refresh_setup
handed_off tehas:question
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
edits=$(label_pair)
t_contains "$edits" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question"
t_eq "$(wc -l <<<"$edits")" 2

t_case "an issue that carries tehas:awaiting-merge and tehas:question has both taken off"
refresh_setup
issue7 tehas:question
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_contains "$(label_pair)" "--remove-label tehas:awaiting-merge --remove-label tehas:question"

t_case "a handed-off issue that carries tehas:ready drops the refresh: the fresh cycle is the person's"
refresh_setup
slots 1 TEHAS_REFRESH_CONFLICTS=1
handed_off tehas:ready
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
refresh_dropped "it was handed off and carries tehas:ready"

t_case "a handed-off issue whose pull request says tehas:running drops the refresh"
refresh_setup
handed_off
pull '.closingIssuesReferences.nodes[0].labels.nodes = [{name: "needs-human-attention"}, {name: "tehas:running"}]'
t_run "$dispatch"
t_eq "$(started)" ""

t_case "no person's approval on record drops the refresh"
refresh_setup
t_store_db "$T/home/tehas"
t_run "$dispatch"
refresh_dropped "this host has no approval of a person's"

t_case "an approval by an account no longer allowed drops the refresh"
refresh_setup
t_store_db "$T/home/tehas"
past 7 bob label 2026-10-01T00:00:00Z
t_run "$dispatch"
refresh_dropped "this host has no approval of a person's"

t_case "an approval the store cannot read keeps the refresh waiting: the request stays"
refresh_setup
store_refuses "approval-current 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: its approval could not be read"
t_lacks "$t_out" "drop the refresh of #7"
t_lacks "$(t_calls gh)" "$mark_call"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "a history the store cannot read keeps the refresh waiting: the request stays"
refresh_setup
store_refuses "history-show 7" 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: its history could not be read"
t_lacks "$t_out" "drop the refresh of #7"
t_lacks "$(t_calls gh)" "$mark_call"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_case "a pull request that carries tehas:refreshing waits"
refresh_setup
pull '.labels.nodes = [{name: "tehas:refreshing"}]'
t_run "$dispatch"
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: pull request #50 carries tehas:refreshing"
t_eq "$(find "$refresh_dir" -name '7.json' | wc -l)" 1

t_done
