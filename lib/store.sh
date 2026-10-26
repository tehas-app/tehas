# shellcheck shell=bash
# The state store, for the shell commands: $TEHAS_HOME/libexec/store
# (console/store). One call runs one verb. Sourcing this file does nothing, and
# no function here exits or logs: lib/ is sourced by login shells, and the
# caller says what went wrong in its own voice.
#
#   tehas_store <verb> [args...]
#       Runs the store once with fd 8 closed, stdin from /dev/null, bounded by
#       timeout -k 2 $tehas_store_bound (20 s). Sets:
#         tehas_store_out   the one JSON line, on 0
#         tehas_store_word  the store's word on failure (busy, conflict,
#                           unapproved, used, missing, corrupt, ...), or ours:
#                           timeout, absent, garbled
#         tehas_store_line  the store's stderr line on failure, cut to 300 bytes
#       Returns the store's status, or ours:
#         0   answered: one line, .api == 1, .ok == true, .verb == <verb>
#         1   refused, busy, conflict, or timeout (word timeout)
#         64  usage: a bug in the caller
#         78  unusable: the store's 78, a binary that is missing or not
#             executable (word absent), or an answer of another shape: a 0
#             whose line is not the shape above, a 1 without the store's word,
#             or a status the store never gives (word garbled)
#   tehas_store_get <jq filter>   jq -r of $tehas_store_out; only after a 0
#   tehas_store_try <tries> <verb> [args...]
#       tehas_store again while the word is busy or timeout, 1 s apart
#   tehas_store_looked            0 when the last call answered (returned 0)
#   tehas_store_ready             0 when the binary is executable and
#                                 state/tehas.db is a regular file and not a
#                                 link: two tests, no process
#   tehas_cycle_attempt <issue>   sets tehas_cycle_attempt_is to the attempt on
#                                 record, 0 when none. Returns 1 when it could
#                                 not look, and the value is then empty. Call
#                                 it in the caller's shell, never in $(...), so
#                                 tehas_store_word stays readable
#   tehas_approval_view <issue> [<attempt>]
#       The approval of the issue's latest start, in the shape .approved.json
#       had, in a new 0600 file under $TEHAS_HOME/tmp; prints its path, and the
#       caller removes it. With <attempt>, only the approval that start used.
#       Returns 0 with a path, 1 when there is none on record, 2 when the store
#       could not answer (and prints nothing).
#   plan_view <issue>
#       The issue's open planning session, in the shape its record file had
#       before the store (tehas_plan_shape), in a new 0600 file under
#       $TEHAS_HOME/tmp; prints its path, the caller removes it. Returns 0
#       with a path, 1 when none is open, 2 when the store could not answer.
#   tehas_plan_shape              a jq filter: of plan-show's answer, the
#                                 session as its record file had it: session,
#                                 turns, options, pending, failed, running,
#                                 startedAt (ISO), comment (the id asked for),
#                                 lost, by, label, answers, filed, and the
#                                 notes fixer_refused and held, absent if none
#   tehas_settings_shape          a jq filter: of a cycle as cycle-show answers
#                                 tehas_improve_settings writes them:
#                                 {container, host, concurrentCycles}; null when
#                                 the cycle has none
#   tehas_history_shape           a jq filter: of history-show's answer, each
#                                 cycle row as the line of history it was before
#                                 the store, one object per line: at and started
#                                 as times, pr as the pull request's address.
#                                 Needs $repo. tehas_history_line defines
#                                 history_line, the same of one row
#
# The rule for every reader: read a field only after a 0, and read JSON null as
# "none on record". After 1, 64 or 78 never use an empty value: take the branch
# that fails closed.

tehas_store_out=""
tehas_store_word=""
tehas_store_line=""
tehas_store_rc=78
tehas_cycle_attempt_is=""

# tehas_store_said <rc> <word> <line>: what a failed call sets. The line is
# for a log: no control bytes, at most 300 bytes.
tehas_store_said() {
  local LC_ALL=C line=$3
  line=${line//[[:cntrl:]]/?}
  tehas_store_rc=$1
  tehas_store_word=$2
  tehas_store_line=${line:0:300}
  return "$1"
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_store() { # <verb> [args...]
  local verb=${1:-} bin=${TEHAS_HOME:-}/libexec/store out rc first word=""
  tehas_store_out=""
  tehas_store_word=""
  tehas_store_line=""
  if [ -z "${TEHAS_HOME:-}" ] || [ ! -f "$bin" ] || [ ! -x "$bin" ]; then
    tehas_store_said 78 absent "store: absent: $bin is not an executable file"
    return
  fi
  out=$(TEHAS_HOME=$TEHAS_HOME timeout -k 2 "${tehas_store_bound:-20}" "$bin" "$@" </dev/null 8>&- 2>&1)
  rc=$?
  case $rc in
    0)
      # One line, and the answer to this verb: anything else is not trusted.
      if [[ $out != *$'\n'* ]] &&
        jq -e --arg v "$verb" '.api == 1 and .ok == true and .verb == $v' <<<"$out" >/dev/null 2>&1; then
        tehas_store_out=$out
        tehas_store_rc=0
        return 0
      fi
      tehas_store_said 78 garbled "store: garbled: $verb answered 0 without its answer"
      return
      ;;
    124 | 137)
      tehas_store_said 1 timeout "store: timeout: $verb did not answer in ${tehas_store_bound:-20} s"
      return
      ;;
    126 | 127)
      tehas_store_said 78 absent "store: absent: $bin could not be run"
      return
      ;;
    1 | 64 | 78) ;;
    *)
      tehas_store_said 78 garbled "store: garbled: $verb exited $rc"
      return
      ;;
  esac
  first=${out%%$'\n'*}
  [[ $first =~ ^store:\ ([a-z][a-z_-]*)(:|\ |$) ]] && word=${BASH_REMATCH[1]}
  # A refusal always names its word; the caller's branch hangs on it.
  if [ "$rc" = 1 ] && [ -z "$word" ]; then
    tehas_store_said 78 garbled "store: garbled: $verb refused without a word: $first"
    return
  fi
  tehas_store_said "$rc" "$word" "$first"
}

tehas_store_get() { # <jq filter>
  [ "$tehas_store_rc" = 0 ] || return 1
  jq -r "$1" <<<"$tehas_store_out"
}

tehas_store_try() { # <tries> <verb> [args...]
  local tries=$1 i=1 rc
  shift
  if ! [[ $tries =~ ^[1-9][0-9]?$ ]]; then
    tehas_store_said 64 usage "store: usage: tehas_store_try takes a count of tries, not $tries"
    return
  fi
  while :; do
    tehas_store "$@"
    rc=$?
    [ "$rc" = 1 ] && [ "$i" -lt "$tries" ] || return "$rc"
    case $tehas_store_word in busy | timeout) ;; *) return "$rc" ;; esac
    i=$((i + 1))
    sleep 1
  done
}

tehas_store_looked() { [ "$tehas_store_rc" = 0 ]; }

tehas_store_ready() {
  local bin=${TEHAS_HOME:-}/libexec/store db=${TEHAS_HOME:-}/state/tehas.db
  [ -n "${TEHAS_HOME:-}" ] && [ -f "$bin" ] && [ -x "$bin" ] && [ -f "$db" ] && [ ! -L "$db" ]
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_cycle_attempt() { # <issue>
  local got
  tehas_cycle_attempt_is=""
  tehas_store cycle-show "$1" || return 1
  got=$(tehas_store_get '.cycle.attempt // 0') || return 1
  [[ $got =~ ^[0-9]{1,9}$ ]] || return 1
  tehas_cycle_attempt_is=$got
}

# The approval as the dispatcher wrote .approved.json: the inputs it gave the
# cycle, and for a millwright's or a refresh start the approval it rests on and
# the starts counted on that. A refresh names the person's approval it rests on,
# as given_at did: its own row's at is when the dispatcher decided (D5). The line
# below is #686's alias: the store writes new words, this shape the old.
# shellcheck disable=SC2016  # a jq program
tehas_approval_shape='def iso: if type == "number" then todate else error("not a time") end;
  def person: if . == null then null else {at: (.at | iso), by, how, body_sha256} end;
  def mw: if . == "millwright" then "fixer" else . end; # part 5 of #686
  (.millwright_starts? // .fixer_starts? // 0) as $starts | .current as $c
  | if $c == null or ($want != null and $c.used_by != $want) then null
    elif ($c | type) != "object" then error("not an approval")
    else ($c.input // {}) as $in | (.rests_on | person) as $base | ($c.how | mw) as $how
    | {at: ($c.at | iso), by: $c.by, how: $how, body_sha256: $c.body_sha256, links: ($in.links // [])}
      + (if $c.title_sha256 == null then {} else {title_sha256: $c.title_sha256} end)
      + (if $in | has("comment_links") then {comment_links: $in.comment_links} else {} end)
      + (if ($in.retry_context // []) | length == 0 then {} else {retry_context: $in.retry_context} end)
      + (if $how == "fixer" then {rests_on: $base, fixer_starts: $starts} else {} end)
      + (if $how == "refresh" then {rests_on: $base, fixer_starts: $starts, refreshes, refresh_of: $in.refresh_of}
           + (if $base == null then {} else {at: $base.at, by: $base.by} end)
         else {} end)
    end'

tehas_approval_view() { # <issue> [<attempt>]
  local want=null dir=${TEHAS_HOME:-}/tmp view file
  # An empty attempt is a caller's mistake, not a wish for any start's approval.
  if [ "$#" -ge 2 ]; then
    want=$2
    [[ $want =~ ^[1-9][0-9]{0,8}$ ]] || return 2
  fi
  tehas_store approval-current "$1" || return 2
  view=$(jq -c --argjson want "$want" "$tehas_approval_shape" <<<"$tehas_store_out" 2>/dev/null) || return 2
  [ -n "$view" ] || return 2
  [ "$view" != null ] || return 1
  [ -e "$dir" ] || mkdir -m 700 "$dir" 2>/dev/null
  [ -d "$dir" ] && [ ! -L "$dir" ] || return 2
  file=$(mktemp "$dir/approval.XXXXXX" 2>/dev/null) || return 2
  printf '%s\n' "$view" >"$file" 2>/dev/null || {
    rm -f "$file"
    return 2
  }
  printf '%s\n' "$file"
}

# The planning record as its file had it before the store, and #686's alias: the
# refused comments go under either name.
# shellcheck disable=SC2016,SC2034  # a jq program, read by the caller
tehas_plan_shape='.planning | if type != "object" then null else
  ((.notes.millwright_refused // []) + (.notes.fixer_refused // [])) as $refused | # part 5 of #686
  {session: (.session // ""), turns, options: (.options // []), pending, failed, running}
  + (if .started_at == null then {} else {startedAt: (.started_at | todate)} end)
  + (if .comment_id == null then {} else {comment: .comment_id} end)
  + (if .lost == true then {lost: true} else {} end)
  + (if .by == null then {} else {by} end)
  + (if .label == null then {} else {label} end)
  + {answers: ((.answers // []) | map(if .millwright != null then {turn, by, fixer: .millwright} else . end))}
  + (if (.filed // []) == [] then {} else {filed} end)
  + (if $refused == [] then {} else {fixer_refused: $refused} end)
  + (if (.notes.held // []) == [] then {} else {held: .notes.held} end) end'

plan_view() { # <issue>
  local dir=${TEHAS_HOME:-}/tmp view file
  tehas_store plan-show "$1" || return 2
  [ "$(jq -r '.planning | if type == "object" and .ended_at == null then "open" else "none" end' <<<"$tehas_store_out" 2>/dev/null)" = open ] ||
    return 1
  view=$(jq -c "$tehas_plan_shape" <<<"$tehas_store_out" 2>/dev/null) || return 2
  [ -n "$view" ] && [ "$view" != null ] || return 2
  [ -e "$dir" ] || mkdir -m 700 "$dir" 2>/dev/null
  [ -d "$dir" ] && [ ! -L "$dir" ] || return 2
  file=$(mktemp "$dir/plan.XXXXXX" 2>/dev/null) || return 2
  printf '%s\n' "$view" >"$file" 2>/dev/null || {
    rm -f "$file"
    return 2
  }
  printf '%s\n' "$file"
}

# The prefix of a setting's name says which half it came from (cycle-image).
# shellcheck disable=SC2016,SC2034  # a jq program, read by the caller
tehas_settings_shape='if (.settings | type) != "object" or (.settings | length) == 0 then null
  else .settings as $s
  | {container: ($s | with_entries(select(.key | startswith("PI_ENSEMBLE_")))),
     host: ($s | with_entries(select(.key | startswith("TEHAS_")))),
     concurrentCycles: ($s.concurrentCycles | if . == null then null else (tonumber? // null) end)} end'

# shellcheck disable=SC2016,SC2034  # a jq program, read by the caller
tehas_history_line='def history_line:
  {at: (.at | todate), started: .started_at, rc, status, step, cap, park_reason, failure_kind,
   failure_cause, provider, parked,
   pr: (if .pr == null then null else "https://github.com/\($repo)/pull/\(.pr)" end), by, how}
  | with_entries(select(.value != null and .value != ""));'
# shellcheck disable=SC2034  # read by the caller
tehas_history_shape="$tehas_history_line"' .rows[] | select(.kind == "cycle") | history_line'
