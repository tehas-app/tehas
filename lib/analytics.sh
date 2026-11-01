# shellcheck shell=bash
# The calls of the analytics program, $TEHAS_HOME/libexec/analytics, which
# keeps the factory's 365 days of runs, issue events and samples in
# state/analytics.db. Sourced by libexec/cycle and libexec/status, and written
# for them: it uses their log().
#
#   tehas_analytics <subcommand> [<argument>...]
#       Run the program with TEHAS_HOME, for at most tehas_analytics_limit
#       seconds. Always returns 0: analytics never changes how a cycle or a
#       poll ends. A failure, the time limit included, is one line in the
#       caller's log, "WARN analytics <subcommand>: exit N": 124 at the
#       limit, 137 a second later for a program that ignores TERM, as what
#       libexec/cycle starts during its report does. Does nothing unless
#       TEHAS_ANALYTICS is on, and silently where the program is not installed.
#
#   tehas_analytics_to <file> <subcommand> [<argument>...]
#       The same call, with what the program prints written to <file>, for
#       the subcommands that answer: report and sessions. The same time limit
#       and WARN line. Returns 0 when the program ended with 0, 1 when it failed
#       (<file> then holds what it printed, if anything), and 2, writing nothing,
#       where tehas_analytics would not call it. A caller of sessions or outcomes
#       passes --out and a directory, and /dev/null for <file>.
#
#   tehas_analytics_approval <issue> <attempt>
#       Print {at, how, driver} of the approval that attempt used, or null. Read
#       before cycle-end, which removes the record of a start that never ran.
#
#   tehas_analytics_run <issue> <attempt> <line> <approval> [<option>...]
#       tehas_analytics run <issue> --facts <file> [<option>...], the facts of
#       the run that ended written to <file> for the call and removed after
#       it: {line, attempt, approval, settings, rukas}. <line> is the line of
#       history libexec/cycle made of the end, a JSON object, and <attempt> the
#       attempt the state store gave the start. The settings and pi-rukas commit
#       are from its record on the state store (lib/store.sh, which the caller
#       sources). What the store cannot answer, or has none of, is a null fact,
#       as is a line that is not an object.
#
#       It returns 0. Facts are built only where tehas_analytics would call
#       the program. <file> is a new 0600 file, $TEHAS_HOME/tmp/analytics-facts.*;
#       one a killed caller left is libexec/clean's. Facts
#       that cannot be written there are one line "WARN analytics
#       <subcommand>: no facts written", and the program is not called.
#
#   tehas_analytics_wait
#       Print the seconds a call may take.
#
# What the program says on standard error goes to the caller's. Its standard
# input is not the caller's, nor is its output but through tehas_analytics_to.
# From the environment, for tests: TEHAS_ANALYTICS_WAIT, seconds in place of
# the limit.

# Short: a stopped cycle has TimeoutStopSec for its whole report, and two
# calls are part of it.
tehas_analytics_limit=5

tehas_analytics_on() {
  local bin=$TEHAS_HOME/libexec/analytics
  [ "${TEHAS_ANALYTICS:-}" = on ] && [ -f "$bin" ] && [ -x "$bin" ]
}

tehas_analytics() {
  local bin=$TEHAS_HOME/libexec/analytics wait rc
  tehas_analytics_on || return 0
  wait=$(tehas_analytics_wait)
  TEHAS_HOME=$TEHAS_HOME timeout -k 1 "$wait" "$bin" "$@" </dev/null >/dev/null
  rc=$?
  [ "$rc" = 0 ] || log "WARN analytics ${1:-}: exit $rc"
  return 0
}

tehas_analytics_to() {
  local file=$1 bin=$TEHAS_HOME/libexec/analytics wait rc
  shift
  tehas_analytics_on || return 2
  wait=$(tehas_analytics_wait)
  TEHAS_HOME=$TEHAS_HOME timeout -k 1 "$wait" "$bin" "$@" </dev/null >"$file"
  rc=$?
  [ "$rc" = 0 ] && return 0
  log "WARN analytics ${1:-}: exit $rc"
  return 1
}

tehas_analytics_wait() {
  local wait=${TEHAS_ANALYTICS_WAIT:-}
  [[ $wait =~ ^[1-9][0-9]{0,2}$ ]] || wait=$tehas_analytics_limit
  printf '%s\n' "$wait"
}

# tehas_analytics_facts <subcommand> <issue> <facts> [<option>...]
tehas_analytics_facts() {
  local subcommand=$1 issue=$2 facts=$3 dir=$TEHAS_HOME/tmp file
  shift 3
  [ -e "$dir" ] || mkdir -m 700 "$dir" 2>/dev/null
  if [ ! -d "$dir" ] || [ -L "$dir" ] || ! file=$(mktemp "$dir/analytics-facts.XXXXXX" 2>/dev/null); then
    log "WARN analytics $subcommand: no facts written"
    return 0
  fi
  if ! printf '%s\n' "$facts" >"$file" 2>/dev/null; then
    rm -f "$file"
    log "WARN analytics $subcommand: no facts written"
    return 0
  fi
  tehas_analytics "$subcommand" "$issue" --facts "$file" "$@"
  rm -f "$file"
  return 0
}

tehas_analytics_approval() { # <issue> <attempt>
  local view approval=""
  if [[ $2 =~ ^[1-9][0-9]{0,8}$ ]] && tehas_analytics_on && view=$(tehas_approval_view "$1" "$2"); then
    approval=$(jq -c '{at, how, driver}' "$view" 2>/dev/null)
    rm -f "$view"
  fi
  printf '%s\n' "${approval:-null}"
}

tehas_analytics_run() {
  local issue=$1 attempt=$2 line=$3 approval=$4 settings="" rukas="" facts
  shift 4
  tehas_analytics_on || return 0
  [[ $attempt =~ ^[1-9][0-9]{0,8}$ ]] || attempt=null
  line=$(jq -c 'objects' <<<"$line" 2>/dev/null) || line=""
  # shellcheck disable=SC2154  # lib/store.sh, which the caller sources, sets it
  if [ "$attempt" != null ] && tehas_store settings-view "$issue" --attempt "$attempt"; then
    settings=$(jq -c .view <<<"$tehas_store_out" 2>/dev/null) || settings=""
    rukas=$(jq -c .rukas_commit <<<"$tehas_store_out" 2>/dev/null) || rukas=""
  fi
  # Through standard input: a history line may be longer than one argument
  # of a command may be.
  facts=$(printf '%s\n' "${line:-null}" "$attempt" "${approval:-null}" "${settings:-null}" "${rukas:-null}" |
    jq -s -c '{line: .[0], attempt: .[1], approval: .[2], settings: .[3], rukas: .[4]}' 2>/dev/null) ||
    facts=""
  [ -n "$facts" ] || facts=null
  tehas_analytics_facts run "$issue" "$facts" "$@"
}
