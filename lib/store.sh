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
#   tehas_store_ready             0 when the binary is executable and
#                                 state/tehas.db is a regular file and not a
#                                 link: two tests, no process
#   tehas_approval_view <issue> [<attempt>]
#       The store's approval-view of the issue's latest start in a new 0600 file
#       under $TEHAS_HOME/tmp; prints its path, and the caller removes it. With
#       <attempt>, only the approval that start used. Returns 0 with a path, 1
#       when there is none on record, 2 when the store could not answer (and
#       prints nothing).
#
# The rule for every reader: read a field only after a 0, and read JSON null as
# "none on record". After 1, 64 or 78 never use an empty value: take the branch
# that fails closed.

tehas_store_out=""
tehas_store_word=""
tehas_store_line=""
tehas_store_rc=78

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

tehas_store_ready() {
  local bin=${TEHAS_HOME:-}/libexec/store db=${TEHAS_HOME:-}/state/tehas.db
  [ -n "${TEHAS_HOME:-}" ] && [ -f "$bin" ] && [ -x "$bin" ] && [ -f "$db" ] && [ ! -L "$db" ]
}

# The view of the last answer in a new file, as tehas_approval_view says.
tehas_view_file() { # <name>
  local dir=${TEHAS_HOME:-}/tmp view file
  view=$(jq -c .view <<<"$tehas_store_out" 2>/dev/null) || return 2
  [ -n "$view" ] || return 2
  [ "$view" != null ] || return 1
  [ -e "$dir" ] || mkdir -m 700 "$dir" 2>/dev/null
  [ -d "$dir" ] && [ ! -L "$dir" ] || return 2
  file=$(mktemp "$dir/$1.XXXXXX" 2>/dev/null) || return 2
  printf '%s\n' "$view" >"$file" 2>/dev/null || {
    rm -f "$file"
    return 2
  }
  printf '%s\n' "$file"
}

tehas_approval_view() { # <issue> [<attempt>]
  tehas_store approval-view "$1" ${2+--attempt "$2"} || return 2
  tehas_view_file approval
}

