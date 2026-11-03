#!/usr/bin/env bash
# Tests for tehasbot refresh: which of the bot's pull requests get a refresh
# request, against a stub gh.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# shellcheck disable=SC2154  # t_dispatch_why is set by t_dispatch (test/lib.sh)
t_dispatch || t_bad "no tehas-dispatch: $t_dispatch_why"
t_console || t_bad "no console: $t_console_why"
mkdir -p "$(dirname "$T_DISPATCH")/../bin"
cp "$T_ROOT/bin/tehasbot" "$(dirname "$T_DISPATCH")/../bin/tehasbot"
cp "$T_CONSOLE" "$(dirname "$T_DISPATCH")/console"
refresh="$(dirname "$T_DISPATCH")/../bin/tehasbot refresh"
fx=$T/fx
dir=$T/home/tehas/state/refresh
logfile=$T/home/tehas/logs/refresh.log
mkdir -p "$fx"
head_a=$(printf 'a%.0s' {1..40})
head_b=$(printf 'b%.0s' {1..40})
base_c=$(printf 'c%.0s' {1..40})

t_stub gh '
case "$1 $2" in
  "api user") [ ! -e "$T/fx/user-fails" ] || exit 1; echo '"'"'{"login":"tehas-bot"}'"'"' ;;
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
# The mix the counting cases share: one conflict, one UNKNOWN and one MERGEABLE
# of the bot's that qualify, and three that are counted nowhere - another
# account's, a fork, and a second MERGEABLE that keeps the page wider than the
# counts, so `read N` is four and not six.
mixed_prs() {
  prs "$(pr 50 7)" "$(pr 51 8 '.mergeable = "UNKNOWN"')" "$(pr 52 9 '.mergeable = "MERGEABLE"')" \
    "$(pr 53 10 '.author.login = "alice"')" "$(pr 54 11 '.mergeable = "MERGEABLE"')" \
    "$(pr 55 12 '.isCrossRepository = true')"
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
t_contains "$(cat "$logfile")" "read 1 pull requests: 1 conflict, 0 unknown, 0 not ours to refresh, wrote 1 requests"

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

t_case "MERGEABLE and UNKNOWN get no request, and each is counted"
reset
prs "$(pr 50 7 '.mergeable = "MERGEABLE"')" "$(pr 51 8 '.mergeable = "UNKNOWN"')"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(requests)" ""
t_contains "$(cat "$logfile")" "read 2 pull requests: 0 conflict, 1 unknown, 1 not ours to refresh, wrote 0 requests"

t_case "a mix of conflicting, unknown and skipped pull requests says each count"
reset
mixed_prs
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$(requests)" "7.json"
t_contains "$(cat "$logfile")" "read 4 pull requests: 1 conflict, 1 unknown, 2 not ours to refresh, wrote 1 requests"

t_case "--json answers the same counts as one object, and still writes the line"
reset
mixed_prs
t_run "$refresh --json"
t_eq "$t_rc" 0
# The object, in the words the object itself has, so a key cannot be renamed
# or dropped without a case failing; jq -S leaves the object unchanged.
t_eq "$(echo "$t_out" | jq -Sc .)" '{"conflict":1,"read":4,"skipped":2,"unknown":1,"wrote":1}'
t_eq "$(wc -l <"$logfile")" 1
t_contains "$(cat "$logfile")" "read 4 pull requests: 1 conflict, 1 unknown, 2 not ours to refresh, wrote 1 requests"
# The page is wider than the counts: three of the six are not ours to refresh.
t_eq "$(jq '.data.repository.pullRequests.nodes | length' "$fx/prs.json")" 6

t_case "the counts are the bot's that qualify, not the page the run read"
reset
# Six open pull requests, none of them ours to refresh, so none of them is
# counted: the page has six nodes and the line says none. The node that would
# conflict is another account's.
prs "$(pr 60 20 '.author.login = "alice"')" "$(pr 61 21 '.author.login = "alice"')" \
  "$(pr 62 22 '.isCrossRepository = true | .mergeable = "CONFLICTING"')" \
  "$(pr 63 23 '.isDraft = true | .mergeable = "CONFLICTING" | .closingIssuesReferences.nodes[0].labels.nodes = [{name: "tehas:running"}]')" \
  "$(pr 64 24 '.author.login = "alice" | .mergeable = "CONFLICTING"')" \
  "$(pr 65 25 '.isCrossRepository = true | .mergeable = "CONFLICTING"')"
t_run "$refresh --json"
t_eq "$t_rc" 0
t_eq "$t_out" '{"conflict":0,"read":0,"skipped":0,"unknown":0,"wrote":0}'

t_case "--json answers nothing when there was no run"
reset
mixed_prs
t_run "$refresh --json"
t_eq "$t_rc" 0
rm -rf "$T"/calls/*
# Inside the interval there was no run, so there is no object to read.
t_run "$refresh --json"
t_eq "$t_rc" 0
t_eq "$t_out" ""
# And none with the setting off, which exits before the sweep.
t_instance "${T_MINIMAL[@]}" TEHAS_REFRESH_CONFLICTS=0
t_run "$refresh --json"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "without --json nothing is printed on standard output"
reset
prs "$(pr 50 7)"
t_run "$refresh"
t_eq "$t_rc" 0
t_eq "$t_out" ""

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

t_case "an unknown argument is refused with 64 and calls nothing, and --help answers"
reset
for bad in --now "--json --now" "--now --json" "--json --json"; do
  t_run "$refresh $bad"
  t_eq "$t_rc" 64
  t_eq "$(t_calls gh)" ""
done
t_run "$refresh --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehasbot refresh"

t_case "--help and an unknown argument need no settings"
reset
t_no_instance
t_run "$refresh --now"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehasbot refresh [--json]: unknown argument: --now"
t_run "$refresh --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
# The dispatcher's own verb, which is what the unit's program runs: the same
# refusal of the same word, and its own text beside it.
t_run "$T_DISPATCH refresh --now"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --now"
t_contains "$t_out" "Usage:"
t_run "$T_DISPATCH refresh --json --json"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --json"

t_case "a setting that is missing stops it with 78 before any call or write"
reset
t_instance TEHAS_REFRESH_CONFLICTS=1
t_run "$refresh"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""
t_eq "$(requests)" ""
t_run "TEHAS_REPO=acme/widget $T_DISPATCH refresh"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -type f 2>/dev/null | wc -l)" 0

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
