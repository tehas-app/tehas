# shellcheck shell=bash
# The calls of the analytics program, $TEHAS_HOME/libexec/analytics, which
# keeps the factory's ten days of runs, issue events and samples in
# state/analytics.db. Sourced by libexec/cycle, libexec/dispatch and
# libexec/status, and written for them: it uses their log().
#
#   tehas_analytics <subcommand> [<argument>...]
#       Run the program with TEHAS_HOME, for at most tehas_analytics_limit
#       seconds. Always returns 0: analytics never changes how a cycle or a
#       poll ends. A failure, the time limit included, is one line in the
#       caller's log, "WARN analytics <subcommand>: exit N": 124 at the
#       limit, 137 a second later for a program that ignores TERM, as what
#       libexec/cycle starts during its report does. Does nothing
#       unless TEHAS_ANALYTICS is on, and nothing, silently, where the
#       program is not installed.
#
#   tehas_analytics_wait
#       Print the seconds a call may take.
#
# What the program says on standard error goes to the caller's. Its standard
# input and output are not the caller's.
# From the environment, for tests: TEHAS_ANALYTICS_WAIT, seconds in place of
# the limit.

# Short: a stopped cycle has TimeoutStopSec for its whole report, and two
# calls are part of it.
tehas_analytics_limit=5

tehas_analytics() {
  local bin=$TEHAS_HOME/libexec/analytics wait rc
  [ "${TEHAS_ANALYTICS:-}" = on ] || return 0
  [ -f "$bin" ] && [ -x "$bin" ] || return 0
  wait=$(tehas_analytics_wait)
  TEHAS_HOME=$TEHAS_HOME timeout -k 1 "$wait" "$bin" "$@" </dev/null >/dev/null
  rc=$?
  [ "$rc" = 0 ] || log "WARN analytics ${1:-}: exit $rc"
  return 0
}

tehas_analytics_wait() {
  local wait=${TEHAS_ANALYTICS_WAIT:-}
  [[ $wait =~ ^[1-9][0-9]{0,2}$ ]] || wait=$tehas_analytics_limit
  printf '%s\n' "$wait"
}
