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
# requests: the request files there are, one name a line.
requests() { find "$dir" -name '*.json' -printf '%f\n' 2>/dev/null | sort; }
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
rm "$dir/last-run"
t_run "$refresh"
t_lacks "$(jq -r .at "$dir/7.json")" "2026-01-01"
t_eq "$(jq -r .head_sha "$dir/7.json")" "$head_b"

t_case "MERGEABLE and UNKNOWN get no request"
reset
prs "$(pr 50 7 '.mergeable = "MERGEABLE"')" "$(pr 51 8 '.mergeable = "UNKNOWN"')"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(requests)" ""
t_contains "$(cat "$logfile")" "read 2 pull requests, 0 conflict, wrote 0 requests"

t_case "UNKNOWN keeps a request it had; MERGEABLE removes it"
reset
mkdir -p "$dir"
echo '{}' >"$dir/7.json"
echo '{}' >"$dir/8.json"
prs "$(pr 50 7 '.mergeable = "UNKNOWN"')" "$(pr 51 8 '.mergeable = "MERGEABLE"')"
t_run "$refresh"
t_eq "$(requests)" "7.json"

for kind in "a fork|.isCrossRepository = true" \
  "a branch of another repository|.headRepository.nameWithOwner = \"else/widget\"" \
  "a pull request with no draft state|.isDraft = null" \
  "another account's|.author.login = \"alice\"" \
  "one closing two issues|.closingIssuesReferences.totalCount = 2 | .closingIssuesReferences.nodes += [.closingIssuesReferences.nodes[0] | .number = 9]" \
  "one whose issue has two open pull requests|.closingIssuesReferences.nodes[0].closedByPullRequestsReferences.nodes += [{number: 52, state: \"OPEN\"}]" \
  "one whose issue does not wait for a merge|.closingIssuesReferences.nodes[0].labels.nodes = []" \
  "one whose issue is closed|.closingIssuesReferences.nodes[0].state = \"CLOSED\"" \
  "one whose issue has a cycle running|.closingIssuesReferences.nodes[0].labels.nodes += [{name: \"tehas:running\"}]" \
  "one whose handed-off issue has a cycle running|.closingIssuesReferences.nodes[0].labels.nodes = [{name: \"needs-human-attention\"}, {name: \"tehas:running\"}]" \
  "one that closes the control issue|.closingIssuesReferences.nodes[0].number = 1"; do
  t_case "${kind%%|*} gets no request, and one it had is removed"
  reset
  mkdir -p "$dir"
  echo '{}' >"$dir/7.json"
  prs "$(pr 50 7 "${kind#*|}")"
  t_run "$refresh"
  t_eq "$t_rc" 0
  t_eq "$(requests)" ""
done

t_case "a draft's conflicting pull request gets a request, and one it had is kept"
reset
mkdir -p "$dir"
prs "$(pr 50 7 '.isDraft = true')"
jq -n '{pr: 51, head_sha: "x"}' >"$dir/7.json"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(requests)" "7.json"
t_eq "$(jq -r .pr "$dir/7.json")" 50

t_case "a handed-off issue's conflicting pull request gets a request"
reset
prs "$(pr 50 7 '.closingIssuesReferences.nodes[0].labels.nodes = [{name: "needs-human-attention"}]')"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(requests)" "7.json"
t_eq "$(jq -r .pr "$dir/7.json")" 50

t_case "a request whose pull request is gone is removed; a stray file is left"
reset
mkdir -p "$dir"
echo '{}' >"$dir/7.json"
echo '{}' >"$dir/notes.json"
t_run "$refresh"
t_eq "$(requests)" "notes.json"

for what in user list; do
  t_case "a failed GitHub call ($what) writes nothing and exits 1"
  reset
  mkdir -p "$dir"
  echo '{"pr":50}' >"$dir/7.json"
  prs "$(pr 51 8)"
  : >"$fx/$what-fails"
  t_run "$refresh"
  t_eq "$t_rc" 1
  t_eq "$(requests)" "7.json"
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

t_case "a run inside TEHAS_REFRESH_EVERY minutes of the last calls nothing; one after it runs and says so"
reset
prs "$(pr 50 7)"
mkdir -p "$dir"
now=$(date +%s)
echo $((now - 9 * 60)) >"$dir/last-run"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(t_calls gh)" ""
t_eq "$(requests)" ""
t_eq "$(cat "$dir/last-run")" "$((now - 9 * 60))"
echo $((now - 10 * 60)) >"$dir/last-run"
t_run "$refresh"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "graphql"
t_eq "$(requests)" "7.json"
last=$(cat "$dir/last-run")
t_eq "$((last >= now))" 1

t_case "the interval is the instance's: with TEHAS_REFRESH_EVERY=1 a run 2 minutes after the last goes ahead"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_EVERY=1
prs "$(pr 50 7)"
mkdir -p "$dir"
echo $(($(date +%s) - 120)) >"$dir/last-run"
t_run "$refresh"
t_eq "$(requests)" "7.json"

t_case "the first run, a stamp that cannot be read and one from the future all run"
for stamp in none junk future; do
  reset
  prs "$(pr 50 7)"
  mkdir -p "$dir"
  case "$stamp" in
    junk) echo soon >"$dir/last-run" ;;
    future) echo $(($(date +%s) + 3600)) >"$dir/last-run" ;;
  esac
  t_run "$refresh"
  t_eq "$stamp $(requests)" "$stamp 7.json"
  t_eq "$stamp $(grep -cE '^[0-9]+$' "$dir/last-run")" "$stamp 1"
done

t_case "a failed run counts: the next one waits for the interval"
reset
prs "$(pr 50 7)"
: >"$fx/list-fails"
t_run "$refresh"
t_eq "$t_rc" 1
rm "$fx/list-fails"
rm -rf "$T"/calls/*
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(t_calls gh)" ""

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
