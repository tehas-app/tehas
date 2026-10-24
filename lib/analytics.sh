# shellcheck shell=bash
# The calls of the analytics program, $TEHAS_HOME/libexec/analytics, which
# keeps the factory's 365 days of runs, issue events and samples in
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
#   tehas_analytics_to <file> <subcommand> [<argument>...]
#       The same call, with what the program prints written to <file>, for
#       the subcommands that answer: report and sessions. The same time limit and the same
#       WARN line. Returns 0 when the program ended with 0, 1 when it failed
#       (<file> then holds what it printed, if anything), and 2, writing
#       nothing, where tehas_analytics would not call it: TEHAS_ANALYTICS is
#       not on or the program is not installed.
#
#   tehas_analytics_run <issue> [<option>...]
#       tehas_analytics run <issue> --facts <file> [<option>...], the facts of
#       the run that ended last written to <file> for the call and removed
#       after it: {line, attempt, approval, settings, rukas}. From
#       state/cycles as libexec/cycle and libexec/work leave them: the last
#       line of <issue>.history.jsonl and how many lines it has, {at, how} of
#       .approved.json, the .settings written since the run started, and the
#       first line of .rukas. A file that is missing, a link, over its bound or
#       not JSON is a null fact, as the program read it before.
#
#   tehas_analytics_plan <issue> [<option>...]
#       tehas_analytics plan <issue> --facts <file> [<option>...], the facts
#       being the planning record state/plan-<issue>.json as it is, null where
#       it cannot be read.
#
#       Both return 0. Facts are built only where tehas_analytics would call
#       the program. <file> is a new 0600 file under $TEHAS_HOME/tmp; facts
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

# The largest host file a fact is read from, as the program bounds its own.
tehas_analytics_max=$((16 << 20))

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

# tehas_analytics_read <file> [<bytes>]: the file's text, for a regular file
# that is not a link and is no bigger than <bytes>; 1 otherwise.
tehas_analytics_read() {
  local max=${2:-$tehas_analytics_max} size
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  size=$(stat -c %s -- "$1" 2>/dev/null) || return 1
  [ "$size" -le "$max" ] || return 1
  head -c "$max" -- "$1"
}

# tehas_analytics_facts <subcommand> <issue> <facts> [<option>...]
tehas_analytics_facts() {
  local subcommand=$1 issue=$2 facts=$3 dir=$TEHAS_HOME/tmp file
  shift 3
  [ -e "$dir" ] || mkdir -m 700 "$dir" 2>/dev/null
  if [ ! -d "$dir" ] || [ -L "$dir" ] || ! file=$(mktemp "$dir/facts.XXXXXX" 2>/dev/null); then
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

tehas_analytics_run() {
  local issue=$1 cycles=$TEHAS_HOME/state/cycles history approval=null settings=null rukas=null started mtime facts
  shift
  tehas_analytics_on || return 0
  history=$(tehas_analytics_read "$cycles/$issue.history.jsonl" |
    jq -R -s -c '[split("\n")[] | fromjson? | objects] | {line: last, attempt: length}' 2>/dev/null) ||
    history=""
  [ -n "$history" ] || history='{"line": null, "attempt": 0}'
  approval=$(tehas_analytics_read "$cycles/$issue.approved.json" |
    jq -s -c 'if length == 1 and (.[0] | type) == "object" then .[0] | {at, how} else null end' 2>/dev/null) ||
    approval=null
  # Only a snapshot of this run: libexec/work writes it as the run starts.
  started=$(jq -r '.line.started | numbers' <<<"$history" 2>/dev/null)
  mtime=$(stat -c %Y -- "$cycles/$issue.settings" 2>/dev/null) || mtime=""
  if [[ $started =~ ^[0-9]+$ ]] && [[ $mtime =~ ^[0-9]+$ ]] && [ "$mtime" -ge $((started - 5)) ]; then
    settings=$(tehas_analytics_read "$cycles/$issue.settings" |
      jq -s -c 'if length == 1 and (.[0] | type) == "object" then .[0] | {container, host, concurrentCycles} else null end' 2>/dev/null) ||
      settings=null
  fi
  rukas=$(tehas_analytics_read "$cycles/$issue.rukas" 4096 | jq -R -s -c 'split("\n")[0]' 2>/dev/null) || rukas=null
  [ -n "$approval" ] || approval=null
  [ -n "$settings" ] || settings=null
  [ -n "$rukas" ] || rukas=null
  # Through standard input: a history line may be longer than one argument
  # of a command may be.
  facts=$(printf '%s\n' "$history" "$approval" "$settings" "$rukas" |
    jq -s -c '{line: .[0].line, attempt: .[0].attempt, approval: .[1], settings: .[2], rukas: .[3]}' 2>/dev/null) ||
    facts=""
  [ -n "$facts" ] || facts=null
  tehas_analytics_facts run "$issue" "$facts" "$@"
}

tehas_analytics_plan() {
  local issue=$1 record
  shift
  tehas_analytics_on || return 0
  record=$(tehas_analytics_read "$TEHAS_HOME/state/plan-$issue.json" |
    jq -s -c 'if length == 1 then .[0] else null end' 2>/dev/null) || record=null
  [ -n "$record" ] || record=null
  tehas_analytics_facts plan "$issue" "$record" "$@"
}
