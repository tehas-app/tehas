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

home=$T/home/tehas
# The bash poll runs health from beside it; the binary is built elsewhere, so
# it is named.
rpoll=". \"$T_ROOT/lib/config.sh\" && tehas_load && TEHAS_HEALTH_CMD=\"$T_ROOT/libexec/health\" \"$T_DISPATCH\" poll"
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

t_case "a dry run logs what the bash poll's dry run logs"
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

t_case "a poll that starts a cycle logs what the bash poll logs, line for line"
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

t_case "a lock nothing used for 30 days is swept, and said as the bash says it"
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

t_case "a closed low gate holds the lowest issues as the bash poll holds them, the gate asked once"
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
# Both halves of one contract, each beside the read that produces it: the Rust
# poll's words come from the read of the queue (console/dispatch/src/read/run.rs,
# gh and gh_all) and the bash poll's from gh_json (libexec/dispatch). The case
# below is the Rust half and runs wherever the Rust poll runs; the bash half has
# a case of its own further down for T_BASH_POLL=1, so the rule is never left to
# one host alone and a t_skip says which half a given run proved.
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

# The same property of the bash poll, whose first read is its look-up of the
# repository and whose words are gh_json's. Under the default run of the gate
# the Rust poll answers instead, and that is what the skip says.
t_case "the bash poll's failed first read ends it with gh_json's own words"
reset
if ! bash_reads; then
  t_skip "this run is the Rust poll's: T_BASH_POLL=1 is the configuration that runs gh_json"
else
  t_gh_fail 1
  mkdir -p "$home/libexec" "$home/state/github"
  printf 'ghs_poll\n' >"$home/state/github/token"
  echo $(($(date +%s) + 3000)) >"$home/state/github/token.expires"
  printf '#!%s\nprintf "%%s\\n" ghs_poll\n' "$T_BASH" >"$home/libexec/github-token"
  chmod +x "$home/libexec/github-token"
  T_ENV+=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089
    "TEHAS_BOT_LOGIN=tehas-app-bot[bot]")
  t_run "$dispatch"
  t_eq "$t_rc" 1
  t_contains "$t_out" "ERROR: gh repo view failed"
  t_contains "$(t_calls gh)" "repo view acme/widget --json nameWithOwner"
  t_eq "$(head -1 "$T/calls/gh")" "repo view acme/widget --json nameWithOwner"
  t_eq "$(cat "$T/gh-n")" 1
  t_lacks "$t_out" "ghs_poll"
  t_lacks "$t_out" "ghs_fresh"
  t_eq "$(started)" ""
  t_eq "$([ -e "$home/live/demand.json" ] && echo written)" ""
fi

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

t_done
