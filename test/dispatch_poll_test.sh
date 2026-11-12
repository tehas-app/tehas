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

t_case "a read the poll cannot make ends the poll with gh's own words"
reset
# The words come from the read of the queue (console/dispatch/src/read/run.rs,
# gh and gh_all).
t_gh_fail 1 1 "HTTP 502: Bad Gateway"
t_gh_fail 2 1 "HTTP 502: Bad Gateway"
# The retry of read/retry.rs owes a 5xx the one second process it is given, so
# both answers of the read of the queue are armed above and the poll ends on the
# second of them. The two names below pin the host's shim of host/sandbox/gh-shim,
# whose own minute-long wait is what test/gh_shim_test.sh pins: a 5xx is no rate
# limit to its read, so it neither retries nor waits for this case, and it is told
# which gh to ask. T_ENV is not cleared between cases, so the names go back with
# the case that set them.
T_ENV=()
T_ENV+=(TEHAS_GH_BIN="$T/bin/gh" TEHAS_GH_RETRY_WAIT=0)
# A host of the GitHub App, so the poll takes a token before its first call and
# hands it to every gh child as TEHAS_GH_TOKEN. The kept token is ghs_poll and a
# token the token program makes is ghs_fresh<n>, and neither is a word the poll
# may print: its lines go to logs/dispatch.log, which
# tehasbot clean keeps, so a failure that echoed gh's own error through would
# keep a token in a log. This is the first case to send gh's stderr into them.
mkdir -p "$home/bin" "$home/state/github"
printf 'ghs_poll\n' >"$home/state/github/token"
echo $(($(date +%s) + 3000)) >"$home/state/github/token.expires"
printf '#!%s\n[ "$*" = github-token ] || exit 64\nprintf "%%s\\n" ghs_poll\n' "$T_BASH" >"$home/bin/tehasbot"
chmod +x "$home/bin/tehasbot"
T_ENV+=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089
  "TEHAS_BOT_LOGIN=tehas-app-bot[bot]")
t_run "$rpoll"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh api graphql failed: HTTP 502"
# The outage was the call the case armed, twice over: the page of the queue is
# the poll's first read, and the rule of read/retry.rs owes a 5xx one retry, so
# the read is made twice and both answers are armed. Without these the case reads
# as a pass on a poll that never met the outage at all, or as a pass on a poll that
# gave up after one call and never tried what the rule owes it.
t_contains "$(t_calls gh)" "api graphql"
t_eq "$(head -1 "$T/calls/gh" | cut -d' ' -f1-2)" "api graphql"
t_eq "$(cat "$T/gh-n")" 2
t_lacks "$t_out" "ghs_poll"
t_lacks "$t_out" "ghs_fresh"
t_lacks "$t_out" "TEHAS_GH_TOKEN"
# The failed read is on the host's record that health counts, once.
t_eq "$(cut -f2 "$home/state/github-failures")" "$(grep -o 'ERROR: gh api graphql failed.*' <<<"$t_out" | head -1)"
t_eq "$(wc -l <"$home/state/github-failures")" 1
t_lacks "$(cat "$home/state/github-failures")" "ghs_"
t_eq "$(started)" ""
t_eq "$([ -e "$T/calls/tidy.after-start" ] && echo tidied)" ""
t_eq "$([ -e "$home/live/demand.json" ] && echo written)" ""

t_case "the poll's token reaches the real tidy, status and health: the token program runs once"
reset
# The host's shim, its gh this run's stub, and a token program that counts its
# runs. The real status and health load their settings, and the load keeps the
# name, so only the poll itself asks for a token.
mkdir -p "$home/shim" "$home/bin" "$home/libexec" "$home/state/github"
cp "$T_ROOT/host/shim/gh" "$home/shim/gh"
ln -sf "$T/bin/gh" "$home/bin/gh"
printf 'ghs_poll\n' >"$home/state/github/token"
echo $(($(date +%s) + 3000)) >"$home/state/github/token.expires"
printf '#!%s\n[ "$*" = github-token ] || exit 64\necho run >>%q\nprintf "%%s\\n" ghs_poll\n' "$T_BASH" "$T/token-runs" >"$home/bin/tehasbot"
chmod +x "$home/bin/tehasbot"
rm -f "$T/token-runs"
printf '#!/usr/bin/env bash\nexec %q tidy run\n' "$T_CONSOLE" >"$T/real-tidy"
# shellcheck disable=SC2016  # expanded when the command runs
printf '#!/usr/bin/env bash\nexec "$(dirname "$0")/../bin/tehasbot" status\n' >"$T/health-tree/libexec/status"
chmod +x "$T/real-tidy" "$T/health-tree/libexec/status"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_GITHUB_APP_ID=5452597 \
  TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]"
T_ENV+=(TEHAS_TIDY_CMD="$T/real-tidy" TEHAS_STATUS_CMD="$T/health-tree/libexec/status")
# The shim first on PATH, as host/env.sh puts it.
t_run "PATH=$home/shim:\$PATH; $rpoll"
t_contains "$t_out" "start #7 (labelled by alice)"
t_contains "$(t_calls gh)" "is:closed"
t_eq "$(wc -l <"$T/token-runs")" 1
rm -f "$T/health-tree/libexec/status"

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
