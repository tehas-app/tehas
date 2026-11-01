# shellcheck shell=bash
# The improvement loop's shared parts: the switch, what a cycle ran with, and
# how a whole log is packed for upload. Sourced by libexec/improve and by tehas
# work. The record a cycle leaves for the loop is tehas-cycle's
# (console/cycle/src/improve.rs).
#
#   tehas_improve_on       is the loop switched on for this instance?
#   tehas_improve_dir      print where records wait
#   tehas_improve_settings <file> <concurrent> <docker options>...
#                          write what a cycle ran with, for its record: the
#                          PI_ENSEMBLE_* variables the -e options give the
#                          container, the instance's TEHAS_* settings that
#                          shape a cycle, and how many cycles were running
#   tehas_improve_secret <file>
#                          does the file hold something shaped like a token?
#   tehas_improve_pack <file> <dir> <name> <limit>
#                          gzip <file> into <dir> as <name>.gz, or in numbered
#                          pieces when that would be over <limit> bytes
#
# Source lib/config.sh and call tehas_load first.

# shellcheck source=lib/attachments.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/attachments.sh"
# shellcheck source=lib/store.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/store.sh"

tehas_improve_on() { [ -n "${TEHAS_IMPROVE_REPO:-}" ]; }

tehas_improve_dir() { printf '%s\n' "$TEHAS_HOME/tmp/improve"; }

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

# Pieces are split at line boundaries and compressed one by one, so each opens
# on its own. The number of pieces doubles until every piece fits.
tehas_improve_pack() { # <file> <dir> <name> <limit>
  local file=$1 dir=$2 name=$3 limit=$4 n lines parts i f big
  mkdir -p "$dir" || return 1
  gzip -9 -c "$file" >"$dir/$name.gz" || return 1
  n=$(stat -c %s "$dir/$name.gz")
  [ "$n" -gt "$limit" ] || return 0
  rm -f "$dir/$name.gz"
  n=$((n / limit + 1))
  lines=$(wc -l <"$file")
  parts=$dir/.parts
  while [ "$lines" -ge 2 ]; do
    [ "$n" -le "$lines" ] || n=$lines
    rm -rf "$parts" "$dir/$name".*.gz
    mkdir -p "$parts" || return 1
    split -n "l/$n" -a 5 -d "$file" "$parts/p." || break
    i=0
    big=0
    for f in "$parts"/p.*; do
      [ -s "$f" ] || continue
      i=$((i + 1))
      gzip -9 -c "$f" >"$dir/$name.$i.gz" || big=1
      [ "$(stat -c %s "$dir/$name.$i.gz")" -le "$limit" ] || big=1
    done
    rm -rf "$parts"
    [ "$big" = 1 ] || return 0
    [ "$n" -lt "$lines" ] || break
    n=$((n * 2))
  done
  rm -rf "$parts" "$dir/$name".*.gz
  return 1
}
