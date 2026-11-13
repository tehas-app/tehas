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
  "api graphql")
    case "$*" in
      *number=*) [ ! -e "$T/fx/pr-fails" ] || exit 1; cat "$T/fx/pr.json" ;;
      *) exit 1 ;;
    esac ;;
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
# one <node>: the answer to the read of that one pull request, open.
one() { jq -c '{data: {repository: {pullRequest: (. + {state: "OPEN"})}}}' <<<"$1" >"$fx/pr.json"; }

reset() {
  rm -rf "$T"/calls/* "$T/home/tehas" "${fx:?}"/*
  t_instance "${T_MINIMAL[@]}"
  t_ids "$T/home/tehas"
}
# written: the files a run left on the host, not the app's kept token and login
# or the ids the case recorded.
written() { find "$T/home/tehas" -type f ! -path '*/state/github*' ! -name account-ids 2>/dev/null | wc -l; }

t_case "with no --pr N it is refused with 64 and calls and writes nothing"
reset
t_run "$refresh"
t_eq "$t_rc" 64
t_contains "$t_out" "tehasbot refresh: --pr takes the number of one pull request"
t_eq "$(t_calls gh)" ""
t_eq "$(written)" 0

for kind in "a fork|.isCrossRepository = true" \
  "a branch of another repository|.headRepository.nameWithOwner = \"else/widget\"" \
  "a pull request with no draft state|.isDraft = null" \
  "one closing two issues|.closingIssuesReferences.totalCount = 2 | .closingIssuesReferences.nodes += [.closingIssuesReferences.nodes[0] | .number = 9]" \
  "one whose issue has two open pull requests|.closingIssuesReferences.nodes[0].closedByPullRequestsReferences.nodes += [{number: 52, state: \"OPEN\"}]" \
  "one whose issue does not wait for a merge|.closingIssuesReferences.nodes[0].labels.nodes = []" \
  "one whose issue is closed|.closingIssuesReferences.nodes[0].state = \"CLOSED\"" \
  "one whose issue has a cycle running|.closingIssuesReferences.nodes[0].labels.nodes += [{name: \"tehas:running\"}]" \
  "one whose handed-off issue has a cycle running|.closingIssuesReferences.nodes[0].labels.nodes = [{name: \"needs-human-attention\"}, {name: \"tehas:running\"}]" \
  "one that closes the control issue|.closingIssuesReferences.nodes[0].number = 1"; do
  t_case "--pr on ${kind%%|*} exits 3 and writes nothing"
  reset
  one "$(pr 50 7 "${kind#*|}")"
  t_run "$refresh --pr 50"
  t_eq "$t_rc" 3
  t_eq "$(requests)" ""
done

t_case "--pr on a draft's conflicting pull request writes its request"
reset
one "$(pr 50 7 '.isDraft = true')"
t_run "$refresh --pr 50"
t_eq "$t_rc" 0
t_eq "$(jq -r .pr "$dir/7.json")" 50

t_case "--pr on a handed-off issue's conflicting pull request writes its request"
reset
one "$(pr 50 7 '.closingIssuesReferences.nodes[0].labels.nodes = [{name: "needs-human-attention"}]')"
t_run "$refresh --pr 50"
t_eq "$t_rc" 0
t_eq "$(jq -r .pr "$dir/7.json")" 50

t_case "--pr on a moved head gets a new time"
reset
mkdir -p "$dir"
jq -n --arg h "$head_a" '{at: "2026-01-01T00:00:00Z", pr: 50, head_sha: $h, base_sha: "x"}' >"$dir/7.json"
one "$(pr 50 7 ".headRefOid = \"$head_b\"")"
t_run "$refresh --pr 50"
t_eq "$t_rc" 0
t_lacks "$(jq -r .at "$dir/7.json")" "2026-01-01"
t_eq "$(jq -r .head_sha "$dir/7.json")" "$head_b"

t_case "an unknown argument is refused with 64, and --help answers"
reset
t_run "$refresh --now"
t_eq "$t_rc" 64
t_eq "$(t_calls gh)" ""
t_run "$refresh --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehasbot refresh"

t_case "--help and an unknown argument need no settings"
reset
t_no_instance
t_run "$refresh --now"
t_eq "$t_rc" 64
t_contains "$t_out" "tehasbot refresh: unknown argument: --now"
t_run "$refresh --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"

# What --help prints is the text file of the dispatcher's own package, which is
# the one `console update sources` counts in its sum. An edit of the help is the
# only way to change it, and such an edit changes this line on purpose: the last
# was the end of the timed scan.
t_case "the refresh text is the words it has"
t_eq "$(sha256sum <"$T_ROOT/console/dispatch/src/usage/refresh.txt" | cut -d' ' -f1)" \
  "ac705b0e376d4bae1b83bc3d139769bda5c02a3ca5a5341a37a80fbbcbccfe59"

t_case "--help prints the refresh text word for word"
reset
t_run "$refresh --help"
t_eq "$t_out" "$(cat "$T_ROOT/console/dispatch/src/usage/refresh.txt")"

t_case "a setting that is missing stops it with 78 before any call or write"
reset
t_instance
t_run "$refresh --pr 50"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""
t_eq "$(requests)" ""
t_run "TEHAS_REPO=acme/widget $T_DISPATCH refresh --pr 50"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""
t_eq "$(written)" 0

t_case "the bot is the login the app kept, and GraphQL's login without [bot] is the bot's"
reset
printf 'tehas-app-bot[bot]\n' >"$T/home/tehas/state/github/login"
t_ids "$T/home/tehas" tehas-app-bot
one "$(pr 50 7 '.author.login = "tehas-app-bot"')"
t_run "$refresh --pr 50"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "api user"
t_eq "$(requests)" "7.json"
one "$(pr 51 8 '.author.login = "someone"')"
t_run "$refresh --pr 51"
t_eq "$t_rc" 3
t_eq "$(requests)" "7.json"

t_case "--pr writes the request of that pull request, asked"
reset
one "$(pr 50 7)"
t_run "$refresh --pr 50"
t_eq "$t_rc" 0
t_eq "$(jq -c '{pr, head_sha, base_sha, asked}' "$dir/7.json")" \
  "{\"pr\":50,\"head_sha\":\"$head_a\",\"base_sha\":\"$base_c\",\"asked\":\"pr\"}"
t_eq "$(jq -r 'keys_unsorted | join(" ")' "$dir/7.json")" "at pr head_sha base_sha asked"
t_contains "$(t_calls gh)" "number=50"
t_contains "$(cat "$logfile")" "asked for pull request #50: wrote the request of #7"

t_case "--pr keeps the time of a request for the same head, and writes UNKNOWN"
reset
mkdir -p "$dir"
jq -n --arg h "$head_a" '{at: "2026-01-01T00:00:00Z", pr: 50, head_sha: $h, base_sha: "x"}' >"$dir/7.json"
one "$(pr 50 7 '.mergeable = "UNKNOWN"')"
t_run "$refresh --pr 50"
t_eq "$t_rc" 0
t_eq "$(jq -r .at "$dir/7.json")" "2026-01-01T00:00:00Z"
t_eq "$(jq -r .asked "$dir/7.json")" "pr"

t_case "--pr leaves every other request as it was"
reset
mkdir -p "$dir"
echo '{"pr":60}' >"$dir/9.json"
echo '{}' >"$dir/notes.json"
one "$(pr 50 7)"
t_run "$refresh --pr 50"
t_eq "$t_rc" 0
t_eq "$(requests | tr '\n' ' ')" "7.json 9.json notes.json "
t_eq "$(cat "$dir/9.json")" '{"pr":60}'

for kind in "MERGEABLE|.mergeable = \"MERGEABLE\"|it does not conflict" \
  "another account's|.author.login = \"alice\"|it is not the bot's" \
  "a fork|.isCrossRepository = true|its branch is not in"; do
  t_case "--pr on ${kind%%|*} exits 3, writes nothing and says why"
  reset
  rest=${kind#*|}
  one "$(pr 50 7 "${rest%%|*}")"
  t_run "$refresh --pr 50"
  t_eq "$t_rc" 3
  t_eq "$(requests)" ""
  t_contains "$t_out" "${rest#*|}"
  t_contains "$(cat "$logfile")" "asked for pull request #50: not refreshed: ${rest#*|}"
done

t_case "--pr whose GitHub read fails, or names no pull request, exits 1 and writes nothing"
for how in fails null; do
  reset
  case $how in
    fails) : >"$fx/pr-fails" ;;
    null) echo '{"data":{"repository":{"pullRequest":null}}}' >"$fx/pr.json" ;;
  esac
  t_run "$refresh --pr 50"
  t_eq "$how $t_rc" "$how 1"
  t_eq "$how $(requests)" "$how "
  t_contains "$t_out" "ERROR"
done

t_case "--pr without a number above 0 is refused with 64 and calls nothing"
reset
for args in "--pr 0" "--pr x" "--pr" "--pr 50 51"; do
  t_run "$refresh $args"
  t_eq "$args $t_rc" "$args 64"
done
t_eq "$(t_calls gh)" ""
t_eq "$(requests)" ""

t_case "refresh --help names --pr N and exit 3"
reset
t_run "$refresh --help"
t_contains "$t_out" "tehasbot refresh --pr N"
t_contains "$t_out" "; 3 N is not"

t_done
