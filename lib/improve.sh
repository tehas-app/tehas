# shellcheck shell=bash
# The improvement loop's shared parts: what a cycle ran with.
# Sourced by tehas work. The record a cycle leaves for the loop is tehas-cycle's
# (console/cycle/src/improve.rs).
#
#   tehas_improve_settings <file> <concurrent> <docker options>...
#                          write what a cycle ran with, for its record: the
#                          PI_ENSEMBLE_* variables the -e options give the
#                          container, the instance's TEHAS_* settings that
#                          shape a cycle, and how many cycles were running
#   tehas_improve_secret <file>
#                          does the file hold something shaped like a token?
#
# Source lib/config.sh and call tehas_load first.

# shellcheck source=lib/attachments.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/attachments.sh"
# shellcheck source=lib/store.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/store.sh"

# The TEHAS_* settings a record carries: those that shape a cycle. Not every one:
# the analyst's words are filed in public, and a host's addresses and the
# analytics settings are none of its business.
_tehas_improve_host='^TEHAS_(WORK_(TIMEOUT|STALL|CPUS|MEMORY|IMAGES|FILES|COMMENTS|DESCRIBE)|CONCURRENCY|TICKET_CONCURRENCY|MAX_AWAITING|CYCLE_RESERVE|VERIFY_TIMEOUT|CHILD_TIMEOUT|PREFLIGHT_WAIT|STOP_WAIT|CONSOLE_WAIT|BRANCH)$'

# Only a value of a safe shape goes in: a number, a boolean, or a short word
# of [A-Za-z0-9_.,:=-]. Everything else is [withheld], and so is any value whose
# name says it may be a secret or a host.
_tehas_improve_value() { # <name> <value>
  local v=$2
  case "$1" in
    *KEY* | *TOKEN* | *SECRET* | *PASS* | *PWD* | *COOKIE* | *AUTH* | *CRED* | *BEARER* | *PRIVATE* | \
      *HOST* | *URL* | *ADDR* | *ENDPOINT* | *DOMAIN* | *PROXY*)
      echo "[withheld]"
      return
      ;;
  esac
  if [[ $v =~ ^[A-Za-z0-9_.,:=-]{0,64}$ ]] && ! [[ $v =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}(:[0-9]+)?$ ]] &&
    ! [[ $v =~ [A-Za-z0-9]{16,} ]] && ! [[ $v =~ (^|[^A-Za-z0-9])eyJ ]] &&
    ! [[ $v =~ [A-Za-z0-9-]\.[A-Za-z] ]] && ! [[ $v =~ :[0-9]+$ ]] &&
    ! tehas_improve_secret /dev/stdin <<<"$v"; then
    printf '%s' "$v"
  else
    echo "[withheld]"
  fi
}

tehas_improve_settings() { # <file> <concurrent> <docker options>...
  local out=$1 n=$2 a next=0 name value
  local -a pairs=() host=()
  shift 2
  [[ $n =~ ^(0|[1-9][0-9]{0,4})$ ]] || n=null
  for a in "$@"; do
    if [ "$next" = 1 ]; then
      next=0
      name=${a%%=*}
      [[ $name =~ ^PI_ENSEMBLE_[A-Z0-9_]+$ ]] || continue
      if [ "$name" = "$a" ]; then value=${!name-}; else value=${a#*=}; fi
      pairs+=("$name" "$(_tehas_improve_value "$name" "$value")")
    elif [ "$a" = -e ]; then
      next=1
    fi
  done
  while IFS= read -r name; do
    host+=("$name" "$(_tehas_improve_value "$name" "${!name-}")")
  done < <(compgen -v | grep -E "$_tehas_improve_host" | sort)
  jq -n --argjson n "$n" --argjson split "${#pairs[@]}" '
    def obj: [range(0; length; 2) as $i | {key: .[$i], value: .[$i + 1]}] | from_entries;
    {container: ($ARGS.positional[0:$split] | obj), host: ($ARGS.positional[$split:] | obj),
     concurrentCycles: $n}' --args "${pairs[@]}" "${host[@]}" >"$out.new" && mv "$out.new" "$out" && return 0
  rm -f "$out.new"
  return 1
}

# Logs should hold no secret. This is for the day one does: a log that matches
# is not uploaded.
tehas_improve_secret() { # <file>
  grep -E -q -e 'gh[pousr]_[A-Za-z0-9]{36,}' -e 'github_pat_[A-Za-z0-9_]{22,}' \
    -e '(^|[^A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}' -e 'xox[abprs]-[A-Za-z0-9-]{10,}' -e 'AKIA[0-9A-Z]{16}' \
    -e '-----BEGIN [A-Z ]*PRIVATE KEY-----' "$1"
}
