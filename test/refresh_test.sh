#!/usr/bin/env bash
# Tests for libexec/refresh: which of the bot's pull requests get a refresh
# request, against a stub gh.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

refresh="$T_ROOT/libexec/refresh"
fx=$T/fx
dir=$T/home/tehas/state/refresh
logfile=$T/home/tehas/logs/refresh.log
mkdir -p "$fx"
head_a=$(printf 'a%.0s' {1..40})
head_b=$(printf 'b%.0s' {1..40})
base_c=$(printf 'c%.0s' {1..40})

t_stub gh '
case "$1 $2" in
  "api user") [ ! -e "$T/fx/user-fails" ] || exit 1; echo tehas-bot ;;
  "api graphql") [ ! -e "$T/fx/list-fails" ] || exit 1; cat "$T/fx/prs.json" ;;
  *) exit 1 ;;
esac'

# pr <number> <issue> [jq changes]: one open pull request node of the bot that
# closes the issue, which carries tehas:awaiting-merge and has no other open
# pull request. It conflicts unless the changes say otherwise.
pr() {
  jq -n -c --argjson n "$1" --argjson i "$2" --arg head "$head_a" --arg base "$base_c" '
    {number: $n, url: "https://github.com/acme/widget/pull/\($n)", isDraft: false, isCrossRepository: false,
     headRefOid: $head, baseRefName: "main", baseRefOid: $base, mergeable: "CONFLICTING",
     author: {login: "tehas-bot"}, headRepository: {nameWithOwner: "acme/widget"}, labels: {nodes: []},
     closingIssuesReferences: {totalCount: 1, nodes: [{number: $i, state: "OPEN",
       labels: {nodes: [{name: "tehas:awaiting-merge"}]},
       closedByPullRequestsReferences: {nodes: [{number: $n, state: "OPEN"}]}}]}}' | jq -c "${3:-.}"
}
prs() { printf '%s\n' "$@" | jq -s '{data: {repository: {pullRequests: {nodes: .}}}}' >"$fx/prs.json"; }

reset() {
  rm -rf "$T"/calls/* "$T/home/tehas" "${fx:?}"/*
  t_instance "${T_MINIMAL[@]}" TEHAS_REFRESH_CONFLICTS=1
  prs
}

t_case "with the setting off it exits 0 and calls nothing"
reset
t_instance "${T_MINIMAL[@]}"
prs "$(pr 50 7)"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -type f 2>/dev/null | wc -l)" 0

t_case "a conflicting pull request gets a request, and the run says what it read"
reset
prs "$(pr 50 7)"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(jq -c '{pr, head_sha, base_sha}' "$dir/7.json")" "{\"pr\":50,\"head_sha\":\"$head_a\",\"base_sha\":\"$base_c\"}"
t_contains "$(jq -r .at "$dir/7.json")" "T"
t_contains "$(cat "$logfile")" "read 1 pull requests, 1 conflict, wrote 1 requests"

t_case "a request for the same head keeps its time; a moved head gets a new one"
reset
prs "$(pr 50 7)"
mkdir -p "$dir"
jq -n --arg h "$head_a" '{at: "2026-01-01T00:00:00Z", pr: 50, head_sha: $h, base_sha: "x"}' >"$dir/7.json"
t_run "$refresh"
t_eq "$(jq -r .at "$dir/7.json")" "2026-01-01T00:00:00Z"
prs "$(pr 50 7 ".headRefOid = \"$head_b\"")"
t_run "$refresh"
t_lacks "$(jq -r .at "$dir/7.json")" "2026-01-01"
t_eq "$(jq -r .head_sha "$dir/7.json")" "$head_b"

t_case "MERGEABLE and UNKNOWN get no request"
reset
prs "$(pr 50 7 '.mergeable = "MERGEABLE"')" "$(pr 51 8 '.mergeable = "UNKNOWN"')"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(find "$dir" -type f | wc -l)" 0
t_contains "$(cat "$logfile")" "read 2 pull requests, 0 conflict, wrote 0 requests"

t_case "UNKNOWN keeps a request it had; MERGEABLE removes it"
reset
mkdir -p "$dir"
echo '{}' >"$dir/7.json"
echo '{}' >"$dir/8.json"
prs "$(pr 50 7 '.mergeable = "UNKNOWN"')" "$(pr 51 8 '.mergeable = "MERGEABLE"')"
t_run "$refresh"
t_eq "$(ls "$dir")" "7.json"

for kind in "a fork|.isCrossRepository = true" \
  "a branch of another repository|.headRepository.nameWithOwner = \"else/widget\"" \
  "a draft|.isDraft = true" \
  "another account's|.author.login = \"alice\"" \
  "one closing two issues|.closingIssuesReferences.totalCount = 2 | .closingIssuesReferences.nodes += [.closingIssuesReferences.nodes[0] | .number = 9]" \
  "one whose issue has two open pull requests|.closingIssuesReferences.nodes[0].closedByPullRequestsReferences.nodes += [{number: 52, state: \"OPEN\"}]" \
  "one whose issue does not wait for a merge|.closingIssuesReferences.nodes[0].labels.nodes = []" \
  "one whose issue is closed|.closingIssuesReferences.nodes[0].state = \"CLOSED\"" \
  "one that closes the control issue|.closingIssuesReferences.nodes[0].number = 1"; do
  t_case "${kind%%|*} gets no request, and one it had is removed"
  reset
  mkdir -p "$dir"
  echo '{}' >"$dir/7.json"
  prs "$(pr 50 7 "${kind#*|}")"
  t_run "$refresh"
  t_eq "$t_rc" 0
  t_eq "$(find "$dir" -type f | wc -l)" 0
done

t_case "a request whose pull request is gone is removed; a stray file is left"
reset
mkdir -p "$dir"
echo '{}' >"$dir/7.json"
echo '{}' >"$dir/notes.json"
t_run "$refresh"
t_eq "$(ls "$dir")" "notes.json"

for what in user list; do
  t_case "a failed GitHub call ($what) writes nothing and exits 1"
  reset
  mkdir -p "$dir"
  echo '{"pr":50}' >"$dir/7.json"
  prs "$(pr 51 8)"
  : >"$fx/$what-fails"
  t_run "$refresh"
  t_eq "$t_rc" 1
  t_eq "$(ls "$dir")" "7.json"
  t_eq "$(cat "$dir/7.json")" '{"pr":50}'
  t_contains "$t_out" "ERROR"
done

t_case "an answer that is not a list writes nothing and exits 1"
reset
mkdir -p "$dir"
echo '{"pr":50}' >"$dir/7.json"
echo '{"data":{}}' >"$fx/prs.json"
t_run "$refresh"
t_eq "$t_rc" 1
t_eq "$(cat "$dir/7.json")" '{"pr":50}'

t_case "an unknown argument is refused with 64, and --help answers"
reset
t_run "$refresh --now"
t_eq "$t_rc" 64
t_eq "$(t_calls gh)" ""
t_run "$refresh --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehasbot refresh"

t_case "with a GitHub App the bot is TEHAS_BOT_LOGIN, and GraphQL's login without [bot] is the bot's"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_REFRESH_CONFLICTS=1 TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]"
touch "$fx/user-fails"
prs "$(pr 50 7 '.author.login = "tehas-app-bot"')" "$(pr 51 8 '.author.login = "someone"')"
t_run "$refresh"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "api user"
t_eq "$(requests)" "7.json"

t_done
