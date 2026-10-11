# shellcheck shell=bash
# Tehas instance settings. Sourced by every tehas command and by env.sh.
#
# An instance is one host serving one product repository. Its settings are in
# <instance dir>/instance.env, a shell file of KEY=value lines written by the
# operator. It is sourced as written: treat it like a script, keep it writable
# by the tehas user only, and put no secrets in it.
#
#   tehas_load           find the instance, load and check its settings.
#                        Returns 78 with one line on stderr when it cannot. It
#                        never exits, because login shells source it too.
#   tehas_require NAME.. exit 78 unless each named setting is set and valid.
#   tehas_require_instance_settings
#                        exit 78 unless every setting an instance has to set for
#                        itself is set and valid. One list, so no command that
#                        checks it before changing anything has its own copy.
#   tehas_checkout       check the product checkout is a clone of TEHAS_REPO;
#                        returns 78 if not.
#   tehas_branch         print the branch work lands on; returns 78 if unknown.
#
# A command starts with:
#   here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
#   . "$here/../lib/config.sh"
#   tehas_load || exit 78
#   tehas_require TEHAS_REPO ...   (or tehas_require_instance_settings)
#
# Rules:
#   - instance.env is authoritative. Settings of the same name in the
#     environment are dropped; only TEHAS_HOME, TEHAS_INSTANCE_DIR and the test
#     hooks TEHAS_WORK_CMD, TEHAS_STATUS_CMD, TEHAS_INDEX_CMD,
#     TEHAS_CYCLE_START_CMD and TEHAS_CYCLE_START_WAIT are kept.
#   - Loading runs no git command, makes no network call and writes nothing.
#   - TEHAS_IMPROVE_REPO switches the improvement loop on. Unset is valid and
#     means there is none.
#   - Nothing falls back to a project: a setting with no generic default stays
#     unset until tehas_require asks for it.

tehas_err() { echo "tehas: $*" >&2; }

# Where the instance's settings are: the environment, else the one-line file
# ~/.config/tehas/instance-dir, else ~/.config/tehas.
tehas_instance_dir() {
  local pointer=$HOME/.config/tehas/instance-dir dir=""
  if [ -n "${TEHAS_INSTANCE_DIR:-}" ]; then
    dir=$TEHAS_INSTANCE_DIR
  elif [ -r "$pointer" ]; then
    read -r dir <"$pointer" || true
  fi
  printf '%s\n' "${dir:-$HOME/.config/tehas}"
}

_tehas_host_cpus() { nproc 2>/dev/null || echo 1; }
_tehas_host_mem_mb() { awk '/^MemTotal:/ { print int($2 / 1024) }' /proc/meminfo 2>/dev/null || echo 0; }

# A Docker memory value (512m, 8g, a bare number of bytes) in MiB; fails on
# anything else.
_tehas_mem_mb() {
  local v=$1 n
  case "$v" in
    '' | *[!0-9bkmgBKMG]* | [!0-9]*) return 1 ;;
  esac
  n=${v%[bkmgBKMG]}
  case "$n" in '' | *[!0-9]*) return 1 ;; esac
  case "$v" in
    *[gG]) echo $((n * 1024)) ;;
    *[mM]) echo "$n" ;;
    *[kK]) echo $((n / 1024)) ;;
    *) echo $((n / 1048576)) ;;
  esac
}

_tehas_is_count() { case "$1" in '' | *[!0-9]* | 0*) return 1 ;; esac; }

# Print why a value is not acceptable for a setting; return 1 if it is not.
_tehas_valid() {
  local name=$1 v=$2 mb
  case "$name" in
    TEHAS_REPO | TEHAS_IMPROVE_REPO)
      [[ $v =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || { echo "expected owner/name"; return 1; }
      ;;
    TEHAS_CONTROL_ISSUE | TEHAS_MAX_AWAITING | TEHAS_WORK_TIMEOUT | TEHAS_CONCURRENCY | TEHAS_TICKET_CONCURRENCY)
      _tehas_is_count "$v" || { echo "expected a positive whole number"; return 1; }
      ;;
    TEHAS_ALLOWED_LABELERS)
      # Exactly one space between logins and none around them: the dispatcher
      # matches " login " inside " list ", and a stray space would make an
      # empty name match.
      [[ $v =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\ [A-Za-z0-9][A-Za-z0-9-]*)*$ ]] ||
        { echo "expected GitHub logins separated by single spaces"; return 1; }
      ;;
    TEHAS_INDEXING | TEHAS_ANALYTICS_COOKIE)
      case "$v" in 0 | 1) ;; *) echo "expected 0 or 1"; return 1 ;; esac
      ;;
    # Both end up in an HTML attribute of every page, so no quote, space or
    # angle bracket may get through.
    TEHAS_ANALYTICS_SRC)
      [[ $v =~ ^https://[A-Za-z0-9.-]+(:[0-9]+)?/[A-Za-z0-9._/-]*$ ]] ||
        { echo "expected the https address of a script"; return 1; }
      ;;
    TEHAS_ANALYTICS_SITE)
      [[ $v =~ ^[A-Za-z0-9._-]+$ ]] || { echo "expected one word of letters, digits, dots and dashes"; return 1; }
      ;;
    TEHAS_WORK_CPUS)
      if ! [[ $v =~ ^[0-9]+(\.[0-9]+)?$ ]] || ! awk -v v="$v" 'BEGIN { exit !(v > 0) }'; then
        echo "expected a number of CPUs above 0"
        return 1
      fi
      awk -v v="$v" -v max="$(_tehas_host_cpus)" 'BEGIN { exit !(v <= max) }' ||
        { echo "this host has $(_tehas_host_cpus) CPUs"; return 1; }
      ;;
    TEHAS_WORK_MEMORY)
      if ! mb=$(_tehas_mem_mb "$v") || [ "$mb" -le 0 ]; then
        echo "expected a size such as 512m or 8g"
        return 1
      fi
      [ "$mb" -le "$(_tehas_host_mem_mb)" ] || { echo "this host has $(_tehas_host_mem_mb) MiB of memory"; return 1; }
      ;;
    TEHAS_HOST_ADDR)
      [[ $v =~ ^[A-Za-z0-9.:-]+$ ]] || { echo "expected an address or host name"; return 1; }
      ;;
    TEHAS_PROTOLAB_URL)
      # The dashboard puts a ticket's number straight after it.
      [[ $v =~ ^https?://[A-Za-z0-9.-]+(:[0-9]+)?$ ]] ||
        { echo "expected an address such as https://lab.example, with no path and no slash at the end"; return 1; }
      ;;
    PI_ENSEMBLE_SUBAGENT_MODEL | PI_ENSEMBLE_SUBAGENT_PROVIDER | TEHAS_BRANCH)
      [[ $v =~ ^[^[:space:]]+$ ]] || { echo "expected one word"; return 1; }
      ;;
  esac
  return 0
}

tehas_load() {
  local dir file v reason half share
  : "${TEHAS_HOME:=$HOME/tehas}"
  export TEHAS_HOME
  dir=$(tehas_instance_dir)
  file=$dir/instance.env
  [ -r "$file" ] || { tehas_err "no instance.env in $dir (copy instances/example and fill it in)"; return 78; }

  for v in $(compgen -v); do
    case "$v" in
      TEHAS_HOME | TEHAS_INSTANCE_DIR | TEHAS_WORK_CMD | TEHAS_STATUS_CMD | TEHAS_INDEX_CMD | \
        TEHAS_CYCLE_START_CMD | TEHAS_CYCLE_START_WAIT) ;;
      TEHAS_* | PI_ENSEMBLE_*) unset "$v" ;;
    esac
  done
  export TEHAS_INSTANCE_DIR=$dir

  set -a
  # shellcheck disable=SC1090
  . "$file" || { set +a; tehas_err "$file could not be read as shell"; return 78; }
  set +a

  # Previews and database clones are not part of this version. An instance that
  # still asks for them is refused, not run without them.
  for v in TEHAS_PREVIEWS TEHAS_CLONES; do
    if [ "${!v:-0}" != 0 ]; then
      tehas_err "$v is set and this version of tehas has no previews or database clones (see $file)"
      return 78
    fi
  done

  # Derived and optional settings.
  if [ -n "${TEHAS_REPO:-}" ]; then
    export TEHAS_WORK_REPO=${TEHAS_WORK_REPO:-$TEHAS_HOME/repos/${TEHAS_REPO##*/}}
    # Where a cycle works, as its container sees it. On the host the tree is
    # $TEHAS_WORK_DIR-<issue>, a fresh clone that tehas work makes for each
    # cycle and the only tree that cycle's container can write. Not a setting.
    export TEHAS_WORK_DIR=$TEHAS_HOME/work/${TEHAS_REPO##*/}
  fi
  v=$(_tehas_host_cpus)
  export TEHAS_WORK_CPUS=${TEHAS_WORK_CPUS:-$((v > 1 ? v - 1 : 1))}
  half=$(($(_tehas_host_mem_mb) / 2))
  export TEHAS_WORK_MEMORY=${TEHAS_WORK_MEMORY:-${half}m}
  export TEHAS_CONCURRENCY=${TEHAS_CONCURRENCY:-4}
  # How many issues are worked on side by side, each in a cycle of its own.
  export TEHAS_TICKET_CONCURRENCY=${TEHAS_TICKET_CONCURRENCY:-1}
  export TEHAS_INDEXING=${TEHAS_INDEXING:-0}
  # Visitor analytics on the dashboard and the manual. Off unless the instance
  # names a script and a site.
  export TEHAS_ANALYTICS_SRC=${TEHAS_ANALYTICS_SRC:-}
  export TEHAS_ANALYTICS_SITE=${TEHAS_ANALYTICS_SITE:-}
  export TEHAS_ANALYTICS_COOKIE=${TEHAS_ANALYTICS_COOKIE:-0}
  # The repository the improvement loop files its findings in. Unset, there is
  # no loop: nothing is recorded of a cycle and tehasbot improve does nothing.
  export TEHAS_IMPROVE_REPO=${TEHAS_IMPROVE_REPO:-}
  export TEHAS_HOST_ADDR=${TEHAS_HOST_ADDR:-127.0.0.1}
  export TEHAS_MAX_AWAITING=${TEHAS_MAX_AWAITING:-3}
  export TEHAS_WORK_TIMEOUT=${TEHAS_WORK_TIMEOUT:-21600}
  export PI_ENSEMBLE_IMAGE=${PI_ENSEMBLE_IMAGE:-tehas/pi-rukas:uid$(id -u)}
  export PI_ENSEMBLE_HOST_ALIASES=${PI_ENSEMBLE_HOST_ALIASES:-}
  # How many workstreams one cycle may run side by side, inside its one
  # container. pi-rukas reads the second name.
  export PI_ENSEMBLE_MAX_WORKSTREAMS=$TEHAS_CONCURRENCY
  # Agents get neither the host Docker socket nor SSH keys. Not a setting.
  export PI_ENSEMBLE_NO_DOCKER_SOCKET=1 PI_ENSEMBLE_NO_SSH=1

  for v in TEHAS_INDEXING TEHAS_ANALYTICS_COOKIE TEHAS_WORK_CPUS TEHAS_WORK_MEMORY TEHAS_CONCURRENCY \
    TEHAS_TICKET_CONCURRENCY TEHAS_HOST_ADDR TEHAS_MAX_AWAITING TEHAS_WORK_TIMEOUT; do
    reason=$(_tehas_valid "$v" "${!v}") || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  # What one cycle's container gets: the budget divided between the tickets.
  # One ticket at a time gets the budget as written.
  if [ "$TEHAS_TICKET_CONCURRENCY" = 1 ]; then
    export TEHAS_CYCLE_CPUS=$TEHAS_WORK_CPUS TEHAS_CYCLE_MEMORY=$TEHAS_WORK_MEMORY
  else
    share=$(LC_ALL=C awk -v c="$TEHAS_WORK_CPUS" -v n="$TEHAS_TICKET_CONCURRENCY" 'BEGIN { printf "%.2f", int(c * 100 / n) / 100 }')
    if ! LC_ALL=C awk -v s="$share" 'BEGIN { exit !(s >= 1) }'; then
      tehas_err "TEHAS_TICKET_CONCURRENCY=$TEHAS_TICKET_CONCURRENCY leaves each cycle $share CPUs of TEHAS_WORK_CPUS=$TEHAS_WORK_CPUS; a cycle needs at least 1 (see $file)"
      return 78
    fi
    export TEHAS_CYCLE_CPUS=$share
    share=$(($(_tehas_mem_mb "$TEHAS_WORK_MEMORY") / TEHAS_TICKET_CONCURRENCY))
    if [ "$share" -lt 6 ]; then
      tehas_err "TEHAS_TICKET_CONCURRENCY=$TEHAS_TICKET_CONCURRENCY leaves each cycle ${share}m of TEHAS_WORK_MEMORY=$TEHAS_WORK_MEMORY; Docker needs at least 6m (see $file)"
      return 78
    fi
    export TEHAS_CYCLE_MEMORY=${share}m
  fi
  for v in TEHAS_BRANCH TEHAS_PROTOLAB_URL; do
    [ -n "${!v:-}" ] || continue
    reason=$(_tehas_valid "$v" "${!v}") || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  for v in TEHAS_ANALYTICS_SRC TEHAS_ANALYTICS_SITE TEHAS_IMPROVE_REPO; do
    [ -n "${!v}" ] || continue
    reason=$(_tehas_valid "$v" "${!v}") || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  if [ -n "$TEHAS_ANALYTICS_SRC" ] && [ -z "$TEHAS_ANALYTICS_SITE" ]; then
    tehas_err "TEHAS_ANALYTICS_SITE is not set and TEHAS_ANALYTICS_SRC is (see $file)"
    return 78
  fi
  return 0
}

tehas_require() {
  local name reason file=${TEHAS_INSTANCE_DIR:-?}/instance.env
  for name in "$@"; do
    # Only spaces counts as not set.
    [ -n "${!name:-}" ] && [ -n "${!name// /}" ] || { tehas_err "$name is not set (see $file)"; exit 78; }
    reason=$(_tehas_valid "$name" "${!name}") || { tehas_err "$name is not valid: $reason (see $file)"; exit 78; }
  done
}

# The settings an instance has to set itself, because nothing here can give
# them a sensible default. install.sh, tehas-dispatch and tehas update each
# check this one list before they call GitHub, Docker, git or write a file.
tehas_require_instance_settings() {
  tehas_require TEHAS_REPO TEHAS_CONTROL_ISSUE TEHAS_ALLOWED_LABELERS \
    PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER
}

# The product checkout must exist and be a clone of TEHAS_REPO: issues are
# polled from one and the work happens in the other, so they have to be the
# same repository. Returns 78 otherwise. Reads the checkout's configuration
# only; nothing is fetched.
tehas_checkout() {
  local dir=${TEHAS_WORK_REPO:-} url
  if [ -z "$dir" ] || [ ! -d "$dir/.git" ]; then
    tehas_err "no product checkout at ${dir:-(unset)}: clone $TEHAS_REPO there, or set TEHAS_WORK_REPO in ${TEHAS_INSTANCE_DIR:-?}/instance.env"
    return 78
  fi
  url=$(git -C "$dir" config --get remote.origin.url 2>/dev/null) || url=""
  url=${url%/}
  url=${url%.git}
  case "$url" in
    */"$TEHAS_REPO" | *:"$TEHAS_REPO") return 0 ;;
  esac
  tehas_err "$dir is a checkout of ${url:-nothing}, not of $TEHAS_REPO"
  return 78
}

# The branch cycles start from and pull requests target. Read locally: the
# checkout's origin/HEAD is set by install.sh, never looked up here.
tehas_branch() {
  local b=""
  if [ -n "${TEHAS_BRANCH:-}" ]; then
    printf '%s\n' "$TEHAS_BRANCH"
    return 0
  fi
  b=$(git -C "${TEHAS_WORK_REPO:-/nonexistent}" symbolic-ref -q --short refs/remotes/origin/HEAD 2>/dev/null) || b=""
  b=${b#origin/}
  if [ -z "$b" ]; then
    tehas_err "the default branch of ${TEHAS_WORK_REPO:-the product checkout} is unknown: run install.sh, or set TEHAS_BRANCH in ${TEHAS_INSTANCE_DIR:-?}/instance.env"
    return 78
  fi
  printf '%s\n' "$b"
}
