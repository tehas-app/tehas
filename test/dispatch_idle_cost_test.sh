#!/usr/bin/env bash
# Tests for libexec/dispatch: the cost of a poll that starts nothing, on a host
# the size of a working one. The fixture has 500 old lock files, 50 shipped
# issues and 5 blocked ones; the poll starts no cycle. Two figures are pinned:
# it makes no write to GitHub, and it starts fewer processes than the ceiling
# below, counted by the PATH shim of test/dispatch_procs_lib.sh. A change that
# raises either has to change this test, which is the point of it.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1
# shellcheck source=test/dispatch_procs_lib.sh
. "$(dirname "$0")/dispatch_procs_lib.sh" || exit 1

# The most processes one idle poll may start. Measured on the runner of
# 2026-10-26 over the fixture below, three runs of it, each the same figure:
# 405 calls. Of those, 11 are the poll's reads of GitHub through the stub gh,
# 36 its 12 calls of the state store (the store, the timeout that bounds it and
# the jq that reads its answer, three each), and the other 358 the small
# commands of the poll, of libexec/status and of libexec/tidy: the jq that
# shapes each answer, the date of each log line, the mkdir of each live file
# and the df of the disk hold. A fifth of room, so a poll that starts one
# command more per issue still fits - and one that pays a gh call or a process
# per lock file, of which this fixture has 500, does not. That is the shape of
# cost this ceiling keeps out: nothing here grows with what the host holds.
PROCS_MAX=480
log=$T/home/tehas/state/github-writes.log

# The world of a host that has been running: <locks> lock files of issues this
# host once ran, dated nothing so no rule has a reason to open them beyond
# counting them; <shipped> closed issues that carry tehas:shipped, which
# libexec/tidy searches for once a poll and labels nothing; <blocked> open
# issues the blocked label holds, which the poll reads with the ready ones and
# starts none of. One issue waits, tehas:ready, and the control issue is
# paused, so the poll starts nothing at all.
world() { # <locks> <shipped> <blocked>
  local locks=$1 shipped=$2 blocked=$3 n list=""
  mkdir -p "$T/home/tehas/state/cycles"
  for ((n = 1000; n < 1000 + locks; n++)); do
    : >"$T/home/tehas/state/cycles/$n.lock"
  done
  for ((n = 1; n <= shipped; n++)); do
    list+=$(jq -nc --argjson n "$n" '{number: $n, state: "CLOSED",
      labels: [{name: "tehas:shipped"}]}')$'\n'
  done
  jq -s . <<<"$list" >"$fx/closed.json"
  for ((n = 100; n < 100 + blocked; n++)); do
    list+=$(jq -nc --argjson n "$n" '{number: $n,
      labels: [{name: "tehas:blocked"}]}')$'\n'
  done
  jq -s . <<<"$(echo '[{"number":7,"labels":[{"name":"tehas:ready"}]}]'; printf '%s\n' "$list" | tail -n "$blocked")" >"$fx/ready.json"
}

# One poll of that world, run as the poll really runs: the real libexec/status
# for the queue and the real libexec/tidy for the closed issues, both of which
# the other dispatch tests stand in for. The control issue is paused, so the
# poll ends before its queue and starts nothing.
idle_poll() {
  echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
  echo '[{"event":"labeled","label":{"name":"tehas:pause"},"actor":{"login":"alice"}}]' >"$fx/events-1.json"
  T_ENV=(TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2
    TEHAS_STATUS_CMD="$T_ROOT/libexec/status" TEHAS_TIDY_CMD="$T_ROOT/libexec/tidy"
    TEHAS_INDEX_CMD=fake-index TEHAS_INDEX_START_CMD=fake-index-start
    TEHAS_PLAN_CMD=fake-plan TEHAS_PLAN_START_CMD=fake-plan-start "$slice_env")
  t_procs_reset
  t_run "$dispatch"
}

t_case "an idle poll of a full host makes no write to GitHub"
reset
world 500 50 5
rm -f "$log"
idle_poll
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
# Every call is a read: no comment, no label, no edit, and no line of the
# host's write log, which is the figure the poll's own record keeps.
t_eq "$(t_calls gh | grep -cE '(^| )(issue edit|issue comment|api -X|api POST|label) ' || true)" 0
t_eq "$(t_writes)" ""
# The poll said so to the store too: 0 starts and no writes on its row.
t_eq "$(t_at "$(date +%s)" poll-show | jq -r '.last.starts')" 0
t_eq "$(t_at "$(date +%s)" poll-show | jq -r '.last.writes')" null

t_case "an idle poll of a full host starts fewer processes than the ceiling"
reset
world 500 50 5
rm -f "$log"
idle_poll
procs=$(t_procs)
# The gh calls of the stub and the store's calls are counted by their stubs,
# not by the PATH shim, so both go into the figure the ceiling is about.
procs=$((procs + $(t_gh_reads)))
if [ "$procs" -gt "$PROCS_MAX" ]; then
  t_bad "$procs processes, over the ceiling of $PROCS_MAX: $(t_procs_list | head -n 5 | tr '\n' ';')"
fi
[ -n "${TEHAS_TEST_PROCS:-}" ] &&
  printf 'processes: %s\n%s\n' "$procs" "$(t_procs_list | head -n 20)" >&2
t_done
