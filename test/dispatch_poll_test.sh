#!/usr/bin/env bash
# Tests for `tehas-dispatch poll` (console/dispatch/src/poll.rs): one poll of
# the queue in Rust, run as libexec/dispatch runs it once its settings are
# loaded. Its frame: the dispatcher's lock, the checks before any call, what
# it publishes and the end of every poll that held the lock.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_dispatch || t_bad "no tehas-dispatch: $t_dispatch_why"
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_health || t_bad "no console: $t_console_why"

home=$T/home/tehas
# The binary is built elsewhere, so the health beside it is named.
rpoll=". \"$T_ROOT/lib/config.sh\" && tehas_load && TEHAS_HEALTH_CMD=\"$T_HEALTH\" \"$T_DISPATCH\" poll"
untimed() { sed -E 's/^[0-9T:-]+Z //' <<<"$1"; }

t_case "a poll that finds the lock held changes nothing"
reset
mkdir -p "$home/state"
exec 7>>"$home/state/dispatch.lock"
flock -n 7
t_run "$rpoll"
exec 7>&-
t_eq "$t_rc" 0
t_eq "$(t_calls gh)" ""
t_eq "$([ -e "$home/state/dispatch.taken" ] && echo taken)" ""

t_case "a lock held for longer than half an hour is said once"
reset
mkdir -p "$home/state"
echo $((EPOCHSECONDS - 4000)) >"$home/state/dispatch.taken"
exec 7>>"$home/state/dispatch.lock"
flock -n 7
t_run "$rpoll"
t_contains "$t_out" "WARN: the dispatcher's lock has been held since"
t_run "$rpoll"
exec 7>&-
t_lacks "$t_out" "WARN: the dispatcher's lock"

t_case "a store that is not installed, or records of an older tehas, end the poll with 78"
reset
rm -f "$home/libexec/store"
t_run "$rpoll"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"
reset
mkdir -p "$home/state/cycles"
: >"$home/state/cycles/7.approved.json"
t_run "$rpoll"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: records of an older tehas at $home/state/cycles/7.approved.json; run install.sh again"
rm -f "$home/state/cycles/7.approved.json"

t_case "a dry run logs what a poll would do"
reset
echo '[]' >"$fx/ready.json"
t_run "$dispatch --dry-run"
b=$(untimed "$t_out")
t_run "$rpoll --dry-run"
t_eq "$(untimed "$t_out")" "$b"
t_eq "$t_rc" 0

t_case "a poll that starts a cycle: journaled, its children without the poll's lock, its end recorded"
reset
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$t_out" "start #7 (labelled by alice)"
t_eq "$([ -e "$T/calls/fake-start" ] && echo started)" started
t_eq "$([ -e "$T/calls/start.fd8" ] && echo inherited)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running"
# Every write of the start was journaled and settled: none is left open.
t_eq "$(TEHAS_HOME=$home "$home/libexec/store" journal-open | jq -c .actions)" "[]"
t_eq "$(TEHAS_HOME=$home "$home/libexec/store" poll-show | jq -r '.last != null')" true
t_eq "$([ -e "$T/calls/tidy.after-start" ] && echo tidied)" tidied
t_eq "$(jq -r '.time | type' "$home/live/dispatch.json" 2>/dev/null)" string

t_case "label events GitHub left out of the page are read one issue at a time"
# GitHub's page can answer an empty timeline for an issue that has labels,
# without an error. Read as it is, an allowed account's label has nobody
# behind it, and the poll would take it off.
reset
: >"$fx/page-drops-events"
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$t_out" "start #7 (labelled by alice)"
t_lacks "$t_out" "'unknown'"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/events?per_page=100 --paginate"
rm -f "$fx/page-drops-events"

t_case "the queue's page asks for the size the dropped-page guard rests on"
# `first:` is not a tuning number: a page over GitHub's budget for nested
# items comes back with its timelines cut short and no error, which is the
# failure #928 was. The number lives in a query string no fixture reads, so
# the words of the call are what pin it. The page's variable is named in
# single quotes: it is GraphQL's, not the case's.
reset
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" 'issues(states: OPEN, first: 25, after: $cursor)'

t_case "the page gave no timelines, and the issue a cycle holds is not re-read"
# The world above re-read every issue whose labels and timeline disagree. A
# running cycle's issue carries the bot's own tehas:running on every poll, and
# nothing of the rules reads that mark's timeline - the park of a mark whose
# cycle is gone reads the lock file and a fresh label read - so guarding it
# would spend one paginated read per running issue per poll. This is the other
# direction of the case above: here the poll must NOT make that call. #7 keeps
# its approval's timeline, so the issue named is one carrying the mark alone.
reset
: >"$fx/page-drops-events"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
hold 9
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issues/7/events?per_page=100 --paginate"
t_lacks "$(t_calls gh)" "issues/9/events?per_page=100 --paginate"
release
rm -f "$fx/page-drops-events"

t_case "a poll that starts a cycle logs its lines, line for line"
reset
t_run "$dispatch"
b=$(untimed "$t_out")
reset
t_run "$rpoll"
t_eq "$(untimed "$t_out")" "$b"

t_case "a poll takes the ask for it, and a dry run leaves it"
reset
echo '[]' >"$fx/ready.json"
mkdir -p "$home/state"
: >"$home/state/dispatch.again"
t_run "$rpoll --dry-run"
t_eq "$([ -e "$home/state/dispatch.again" ] && echo kept)" kept
t_run "$rpoll"
t_eq "$([ -e "$home/state/dispatch.again" ] && echo kept)" ""

t_case "a lock nothing used for 30 days is swept, and said"
reset
echo '[]' >"$fx/ready.json"
mkdir -p "$home/state/cycles"
: >"$home/state/cycles/5.lock"
touch -d '40 days ago' "$home/state/cycles/5.lock"
t_run "$rpoll --dry-run"
t_contains "$t_out" "would remove the lock of #5: unused for 30 days"
t_eq "$([ -e "$home/state/cycles/5.lock" ] && echo kept)" kept
t_run "$rpoll"
t_contains "$t_out" "removed the lock of #5: unused for 30 days"
t_eq "$([ -e "$home/state/cycles/5.lock" ] && echo kept)" ""

t_case "a real poll writes its demand with its time, and its cost counts its start"
reset
t_run "$rpoll"
t_eq "$(jq -r '.time | type' "$home/live/demand.json" 2>/dev/null)" string
t_eq "$(jq -r .waiting "$home/live/demand.json" 2>/dev/null)" 1
t_eq "$(TEHAS_HOME=$home "$home/libexec/store" poll-show | jq -r .last.starts)" 1

t_case "a closed low gate holds the lowest issues, the gate asked once"
gate_once() {
  t_stub gate-run "echo ran >>\"$T/calls/gate-runs\"
exit 1"
  rm -f "$home/state/low-gate.held"
  slots 3 "TEHAS_LOW_GATE=$T/bin/gate-run"
  queue "7 tehas:urgent" "8 tehas:next" 11 12
  label_events 7 tehas:urgent=alice
  label_events 8 tehas:next=alice
}
reset
gate_once
t_run "$dispatch"
b=$(untimed "$t_out")
t_eq "$(t_calls gate-runs | wc -l)" 1
reset
gate_once
t_run "$rpoll"
t_eq "$(untimed "$t_out")" "$b"
t_eq "$(t_calls gate-runs | wc -l)" 1
t_eq "$(started)" "7 8 "
t_eq "$([ -e "$home/state/low-gate.held" ] && echo held)" held
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"

t_case "a read the poll cannot make ends the poll with gh's own words"
reset
# The words come from the read of the queue (console/dispatch/src/read/run.rs,
# gh and gh_all).
t_gh_fail 1
# A host of the GitHub App, so the poll takes a token before its first call and
# hands it to every gh child as TEHAS_GH_TOKEN. The kept token is ghs_poll and a
# token the token program makes is ghs_fresh<n>, and neither is a word the poll
# may print: its lines go to state/dispatch.last and logs/dispatch.log, which
# tehasbot clean keeps, so a failure that echoed gh's own error through would
# keep a token in a log. This is the first case to send gh's stderr into them.
mkdir -p "$home/libexec" "$home/state/github"
printf 'ghs_poll\n' >"$home/state/github/token"
echo $(($(date +%s) + 3000)) >"$home/state/github/token.expires"
printf '#!%s\nprintf "%%s\\n" ghs_poll\n' "$T_BASH" >"$home/libexec/github-token"
chmod +x "$home/libexec/github-token"
T_ENV+=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089
  "TEHAS_BOT_LOGIN=tehas-app-bot[bot]")
t_run "$rpoll"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh api graphql failed: HTTP 502"
# The outage was the call the case armed: the page of the queue is the poll's
# first read, the stub was run for it once, and its count stands at one. Without
# these the case reads as a pass on a poll that never met the outage at all.
t_contains "$(t_calls gh)" "api graphql"
t_eq "$(head -1 "$T/calls/gh" | cut -d' ' -f1-2)" "api graphql"
t_eq "$(cat "$T/gh-n")" 1
t_lacks "$t_out" "ghs_poll"
t_lacks "$t_out" "ghs_fresh"
t_lacks "$t_out" "TEHAS_GH_TOKEN"
t_eq "$(started)" ""
t_eq "$([ -e "$T/calls/tidy.after-start" ] && echo tidied)" ""
t_eq "$([ -e "$home/live/demand.json" ] && echo written)" ""

t_case "a write a dead poll left in the journal is settled first, and decided again"
reset
st="$home/libexec/store"
at=$(TEHAS_HOME=$home "$st" poll-begin | jq -r .at)
TEHAS_HOME=$home "$st" journal-record --poll "$at" --kind label-add --issue 7 \
  --args '{"gh":["issue","edit","7","--repo","acme/widget","--add-label","tehas:running"]}' --before "$at" >/dev/null
t_run "$rpoll"
t_contains "$t_out" "journal: action 1 (label-add on #7"
t_contains "$t_out" "did not happen (it was never attempted); this poll decides it again"
t_contains "$t_out" "start #7 (labelled by alice)"
t_eq "$(TEHAS_HOME=$home "$st" journal-open | jq -c .actions)" "[]"

# The poll's own GraphQL page holds the open pull requests, as this list: the
# author as GraphQL names an app, with no [bot].
pulls() { # one "<number>|<title>|<head>|<author>|<label>" per argument
  printf '%s\n' "$@" | jq -Rn '[inputs | split("|") | {number: (.[0] | tonumber), title: .[1], headRefName: .[2],
    author: {login: .[3]}, labels: {nodes: ([.[4] // empty | select(. != "")] | map({name: .}))}}]' >"$fx/pulls.json"
}
issue_with() { # <number> <label>
  jq -n --argjson n "$1" --arg l "$2" '{number: $n, state: "open", labels: [{name: $l}]}' >"$fx/work-$1.json"
}
title_of() { # <pull request> <title>: the line of the write
  printf 'api -X PATCH repos/acme/widget/pulls/%s -f title=%s' "$1" "$2"
}
titles() { t_console || t_bad "no console: $t_console_why"; T_ENV+=(TEHAS_CONSOLE_CMD="$T_CONSOLE"); }

t_case "a poll gives each factory pull request the prefix of its state"
reset
titles
echo '[]' >"$fx/ready.json"
issue_with 11 tehas:running
issue_with 12 needs-human-attention
issue_with 13 tehas:awaiting-merge
pulls '31|chore(work): running|feature/issue-11-a|tehas-bot' \
  '32|chore(work): handed off|feature/issue-12-b|tehas-bot' \
  '33|STUCK: chore(work): awaiting|feature/issue-13-c|tehas-bot'
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "$(title_of 31 'WIP: chore(work): running')"
t_contains "$(t_calls gh)" "$(title_of 32 'STUCK: chore(work): handed off')"
t_contains "$(t_calls gh)" "$(title_of 33 'chore(work): awaiting')"

t_case "tehas:refreshing on the pull request is WIP, and WIP wins over STUCK"
reset
titles
echo '[]' >"$fx/ready.json"
issue_with 12 needs-human-attention
jq -n '{number: 13, state: "open", labels: [{name: "tehas:running"}, {name: "needs-human-attention"}]}' >"$fx/work-13.json"
pulls '32|handed off|feature/issue-12-b|tehas-bot|tehas:refreshing' \
  '33|STUCK: both|feature/issue-13-c|tehas-bot'
t_run "$rpoll"
t_contains "$(t_calls gh)" "$(title_of 32 'WIP: handed off')"
t_contains "$(t_calls gh)" "$(title_of 33 'WIP: both')"

t_case "the bot's login is matched in the form REST gives it"
reset
titles
echo '[]' >"$fx/ready.json"
echo '{"login":"tehas-bot[bot]"}' >"$fx/user.json"
issue_with 11 tehas:running
pulls '31|a change|feature/issue-11-a|tehas-bot'
t_run "$rpoll"
t_contains "$(t_calls gh)" "$(title_of 31 'WIP: a change')"

t_case "a person's pull request gets no call, and a title already right gets no write"
reset
titles
echo '[]' >"$fx/ready.json"
issue_with 11 tehas:running
pulls '31|WIP: right already|feature/issue-11-a|tehas-bot' \
  '32|a person, on a factory branch|feature/issue-11-b|alice' \
  '33|the bot, on no factory branch|renovate/x|tehas-bot'
t_run "$rpoll"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "pulls/3"

t_case "a poll with no factory pull request makes no extra call"
reset
titles
echo '[]' >"$fx/ready.json"
t_run "$rpoll"
without=$(t_calls gh | wc -l)
reset
titles
echo '[]' >"$fx/ready.json"
pulls '32|a person|feature/issue-11-b|alice'
t_run "$rpoll"
t_eq "$(t_calls gh | wc -l)" "$without"

t_case "an issue that is not open is read once, and a failed read leaves the title alone"
reset
titles
echo '[]' >"$fx/ready.json"
echo '{"number":21,"state":"closed","labels":[]}' >"$fx/work-21.json"
: >"$fx/work-22-fails"
pulls '41|STUCK: closed|feature/issue-21-a|tehas-bot' '42|STUCK: unread|feature/issue-22-b|tehas-bot'
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "$(title_of 41 'closed')"
t_lacks "$(t_calls gh)" "pulls/42"
t_contains "$t_out" "WARN: the title of pull request #42 was not kept"

t_case "a failed write is a warning and holds nothing"
reset
titles
issue_with 11 tehas:running
pulls '31|a change|feature/issue-11-a|tehas-bot'
: >"$fx/patch-fails"
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the title of pull request #31 was not kept"
t_contains "$t_out" "start #7 (labelled by alice)"

t_case "a dry run writes no title"
reset
titles
issue_with 11 tehas:running
pulls '31|a change|feature/issue-11-a|tehas-bot'
t_run "$rpoll --dry-run"
t_lacks "$(t_calls gh)" "pulls/31"

t_done
