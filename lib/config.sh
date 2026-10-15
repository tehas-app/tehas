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
#     hooks TEHAS_WORK_CMD, TEHAS_STATUS_CMD, TEHAS_INDEX_CMD, TEHAS_PLAN_CMD,
#     TEHAS_CYCLE_START_CMD, TEHAS_CYCLE_STOP_CMD, TEHAS_CYCLE_START_WAIT,
#     TEHAS_CGROUP_ROOT, TEHAS_PREFLIGHT_WAIT, TEHAS_STOP_WAIT, TEHAS_FETCH_WAIT,
#     TEHAS_CONSOLE_WAIT, TEHAS_CYCLE_RETRY_WAIT and TEHAS_CYCLE_STOP_GH_WAIT
#     are kept.
#   - Loading runs no git command, makes no network call and writes nothing.
#   - TEHAS_IMPROVE_REPO switches the improvement loop on. Unset is valid and
#     means there is none. TEHAS_IMPROVE_RUKAS_REPO needs it.
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
    TEHAS_REPO | TEHAS_IMPROVE_REPO | TEHAS_IMPROVE_RUKAS_REPO)
      [[ $v =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || { echo "expected owner/name"; return 1; }
      ;;
    TEHAS_CONTROL_ISSUE | TEHAS_MAX_AWAITING | TEHAS_IMPROVE_MAX_OPEN | TEHAS_WORK_TIMEOUT | TEHAS_WORK_STALL | TEHAS_CONCURRENCY | TEHAS_TICKET_CONCURRENCY | \
      TEHAS_PLAN_MAX_TURNS | TEHAS_PLAN_TIMEOUT | TEHAS_VERIFY_TIMEOUT)
      _tehas_is_count "$v" || { echo "expected a positive whole number"; return 1; }
      ;;
    TEHAS_ALLOWED_LABELERS)
      # Exactly one space between logins and none around them: the dispatcher
      # matches " login " inside " list ", and a stray space would make an
      # empty name match.
      [[ $v =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\ [A-Za-z0-9][A-Za-z0-9-]*)*$ ]] ||
        { echo "expected GitHub logins separated by single spaces"; return 1; }
      ;;
    TEHAS_INDEXING | TEHAS_ANALYTICS_COOKIE | TEHAS_WORK_IMAGES | TEHAS_WORK_DESCRIBE | TEHAS_WORK_FILES | TEHAS_WORK_COMMENTS | TEHAS_TOP)
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
    TEHAS_CYCLE_RESERVE)
      # The shape lib/slice.sh reads at every start: no leading zero, at most
      # twelve digits of bytes or nine with a unit. Docker refuses a memory
      # value under 6m.
      if ! [[ $v =~ ^([1-9][0-9]{0,11}[bB]?|[1-9][0-9]{0,8}[kmgKMG])$ ]] ||
        ! mb=$(_tehas_mem_mb "$v" 2>/dev/null) || [ "$mb" -lt 6 ]; then
        echo "expected a size of at least 6m, such as 512m or 2g"
        return 1
      fi
      ;;
    TEHAS_HOST_ADDR)
      [[ $v =~ ^[A-Za-z0-9.:-]+$ ]] || { echo "expected an address or host name"; return 1; }
      ;;
    TEHAS_PROTOLAB_URL)
      # The dashboard puts a ticket's number straight after it.
      [[ $v =~ ^https?://[A-Za-z0-9.-]+(:[0-9]+)?$ ]] ||
        { echo "expected an address such as https://lab.example, with no path and no slash at the end"; return 1; }
      ;;
    TEHAS_DOMAIN)
      # A name is made by putting uat-<n>. or lab-<n>. in front of it.
      [[ $v =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$ ]] ||
        { echo "expected a domain such as example.org, in lower case, with no scheme and no path"; return 1; }
      ;;
    TEHAS_PREVIEWS)
      [[ $v =~ ^[01]$ ]] || { echo "expected 0 or 1"; return 1; }
      ;;
    TEHAS_PREVIEW_MAX)
      [[ $v =~ ^[1-9][0-9]?$ ]] || { echo "expected a number from 1 to 99"; return 1; }
      ;;
    TEHAS_PREVIEW_MIN_FREE)
      if ! mb=$(_tehas_mem_mb "$v") || [ "$mb" -le 0 ]; then
        echo "expected a size such as 512m or 5g"
        return 1
      fi
      ;;
    PI_ENSEMBLE_SUBAGENT_MODEL | PI_ENSEMBLE_SUBAGENT_PROVIDER | TEHAS_BRANCH)
      [[ $v =~ ^[^[:space:]]+$ ]] || { echo "expected one word"; return 1; }
      ;;
  esac
  return 0
}

tehas_load() {
  local dir file v reason
  : "${TEHAS_HOME:=$HOME/tehas}"
  export TEHAS_HOME
  dir=$(tehas_instance_dir)
  file=$dir/instance.env
  [ -r "$file" ] || { tehas_err "no instance.env in $dir (copy instances/example and fill it in)"; return 78; }

  # TEHAS_ENERGY and TEHAS_ENERGY_URL are host choices in ~/.config/tehas/env,
  # which env.sh reads before this.
  for v in $(compgen -v); do
    case "$v" in
      TEHAS_HOME | TEHAS_INSTANCE_DIR | TEHAS_WORK_CMD | TEHAS_STATUS_CMD | TEHAS_INDEX_CMD | TEHAS_PLAN_CMD | \
        TEHAS_CYCLE_START_CMD | TEHAS_CYCLE_STOP_CMD | TEHAS_CYCLE_START_WAIT | TEHAS_CGROUP_ROOT | \
        TEHAS_PREFLIGHT_WAIT | TEHAS_STOP_WAIT | TEHAS_FETCH_WAIT | TEHAS_CONSOLE_WAIT | \
        TEHAS_CYCLE_RETRY_WAIT | TEHAS_CYCLE_STOP_GH_WAIT | TEHAS_ENERGY | TEHAS_ENERGY_URL) ;;
      TEHAS_* | PI_ENSEMBLE_*) unset "$v" ;;
    esac
  done
  export TEHAS_INSTANCE_DIR=$dir

  set -a
  # shellcheck disable=SC1090
  . "$file" || { set +a; tehas_err "$file could not be read as shell"; return 78; }
  set +a

  # Database clones are not part of this version. An instance that still asks
  # for them is refused, not run without them.
  if [ "${TEHAS_CLONES:-0}" != 0 ]; then
    tehas_err "TEHAS_CLONES is set and this version of tehas has no database clones (see $file)"
    return 78
  fi
  # The budget of an earlier version. The limit around every container is the
  # slice on the host now, and nothing here divides anything, so a budget line
  # would be a setting that does nothing. Refused, not ignored.
  if [ -n "${TEHAS_WORK_CPUS+x}" ]; then
    tehas_err "TEHAS_WORK_CPUS is set and this version of tehas has no CPU budget to set: the limit around all containers is /etc/systemd/system/tehas.slice (see $file)"
    return 78
  fi
  if [ -n "${TEHAS_WORK_MEMORY+x}" ]; then
    tehas_err "TEHAS_WORK_MEMORY is set and this version of tehas has no memory budget to set: the limit around all containers is /etc/systemd/system/tehas.slice (see $file)"
    return 78
  fi

  # Derived and optional settings.
  if [ -n "${TEHAS_REPO:-}" ]; then
    export TEHAS_WORK_REPO=${TEHAS_WORK_REPO:-$TEHAS_HOME/repos/${TEHAS_REPO##*/}}
    # Where a cycle works, as its container sees it. On the host the tree is
    # $TEHAS_WORK_DIR-<issue>, a fresh clone that tehas work makes for each
    # cycle and the only tree that cycle's container can write. Not a setting.
    export TEHAS_WORK_DIR=$TEHAS_HOME/work/${TEHAS_REPO##*/}
  fi
  export TEHAS_CONCURRENCY=${TEHAS_CONCURRENCY:-4}
  # The most issues worked on side by side, each in a cycle of its own. A
  # ceiling: the dispatcher starts fewer while the slice has no room. By
  # default one per CPU, less one for the host. The only thing read of the
  # host here.
  v=$(_tehas_host_cpus)
  export TEHAS_TICKET_CONCURRENCY=${TEHAS_TICKET_CONCURRENCY:-$((v > 1 ? v - 1 : 1))}
  # The memory one container is given as its floor, and what the dispatcher
  # counts for a container when it asks whether another fits. Whether it fits
  # this host is lib/slice.sh's question, asked when a container is started.
  export TEHAS_CYCLE_RESERVE=${TEHAS_CYCLE_RESERVE:-2g}
  export TEHAS_INDEXING=${TEHAS_INDEXING:-0}
  # Whether the host's load and processes are published for the dashboard, by
  # `tehasbot top`. Off unless the instance says so: the dashboard can be on the
  # internet, and this names the processes of the host. It governs the published
  # file only; `tehas top` in a terminal works whatever it says.
  export TEHAS_TOP=${TEHAS_TOP:-0}
  # Visitor analytics on the dashboard and the manual. Off unless the instance
  # names a script and a site.
  export TEHAS_ANALYTICS_SRC=${TEHAS_ANALYTICS_SRC:-}
  export TEHAS_ANALYTICS_SITE=${TEHAS_ANALYTICS_SITE:-}
  export TEHAS_ANALYTICS_COOKIE=${TEHAS_ANALYTICS_COOKIE:-0}
  # The repository the improvement loop files its findings in. Unset, there is
  # no loop: nothing is recorded of a cycle and tehasbot improve does nothing.
  export TEHAS_IMPROVE_REPO=${TEHAS_IMPROVE_REPO:-}
  # Where the loop files what only a change in pi-rukas can fix. Unset, those
  # findings go with the rest.
  export TEHAS_IMPROVE_RUKAS_REPO=${TEHAS_IMPROVE_RUKAS_REPO:-}
  # How many of the loop's issues may be open in one repository before it
  # files no more there. What it files is a model's reading of text anyone
  # may have written, so nobody reading must not mean an endless stack.
  export TEHAS_IMPROVE_MAX_OPEN=${TEHAS_IMPROVE_MAX_OPEN:-10}
  export TEHAS_HOST_ADDR=${TEHAS_HOST_ADDR:-127.0.0.1}
  # Room for every cycle and two pull requests that wait for a review. A fixed
  # number would leave slots that can never be used. A concurrency that is not
  # a number is refused below.
  v=3
  ! _tehas_is_count "$TEHAS_TICKET_CONCURRENCY" || v=$((TEHAS_TICKET_CONCURRENCY + 2))
  export TEHAS_MAX_AWAITING=${TEHAS_MAX_AWAITING:-$v}
  export TEHAS_WORK_TIMEOUT=${TEHAS_WORK_TIMEOUT:-21600}
  # Seconds a cycle may go without adding to any of its logs before it is
  # stopped as stalled. Longer than a run of the gate, which may print nothing.
  export TEHAS_WORK_STALL=${TEHAS_WORK_STALL:-3600}
  # Whether a cycle is given the images its issue links. Off unless the
  # instance says so: it is one more thing a container is given, and it needs
  # a model that reads images.
  export TEHAS_WORK_IMAGES=${TEHAS_WORK_IMAGES:-0}
  # Whether the model describes those images in words before a cycle or a
  # planning session starts. One request per image to the model endpoint.
  export TEHAS_WORK_DESCRIBE=${TEHAS_WORK_DESCRIBE:-0}
  # Whether a cycle is given the text files its issue links: md, txt, csv and
  # the like. Off unless the instance says so. It needs no model that reads
  # images.
  export TEHAS_WORK_FILES=${TEHAS_WORK_FILES:-0}
  # Whether the links in comments by allowed accounts, written before the
  # approval, are taken too. Off unless the instance says so.
  export TEHAS_WORK_COMMENTS=${TEHAS_WORK_COMMENTS:-0}
  # A planning session: how many turns it may take, and how long one turn may.
  export TEHAS_PLAN_MAX_TURNS=${TEHAS_PLAN_MAX_TURNS:-20}
  export TEHAS_PLAN_TIMEOUT=${TEHAS_PLAN_TIMEOUT:-3600}
  # Previews of pull requests and the names of the edge. Off, and no names,
  # unless the instance says so.
  export TEHAS_DOMAIN=${TEHAS_DOMAIN:-}
  export TEHAS_PREVIEWS=${TEHAS_PREVIEWS:-0}
  export TEHAS_PREVIEW_MAX=${TEHAS_PREVIEW_MAX:-5}
  export TEHAS_PREVIEW_MIN_FREE=${TEHAS_PREVIEW_MIN_FREE:-5g}
  export PI_ENSEMBLE_IMAGE=${PI_ENSEMBLE_IMAGE:-tehas/pi-rukas:uid$(id -u)}
  export PI_ENSEMBLE_HOST_ALIASES=${PI_ENSEMBLE_HOST_ALIASES:-}
  # How many workstreams one cycle may run side by side, inside its one
  # container. pi-rukas reads the second name.
  export PI_ENSEMBLE_MAX_WORKSTREAMS=$TEHAS_CONCURRENCY
  # Seconds one run of the product's gate may take inside a cycle. pi-rukas
  # reads milliseconds under two names, one for a workstream's gate and one for
  # the gate on the combined tree, and gives the first ten minutes when it is
  # told nothing. A gate that is refused below is not passed on.
  export TEHAS_VERIFY_TIMEOUT=${TEHAS_VERIFY_TIMEOUT:-1800}
  if _tehas_is_count "$TEHAS_VERIFY_TIMEOUT"; then
    export PI_ENSEMBLE_VERIFY_TIMEOUT_MS=$((TEHAS_VERIFY_TIMEOUT * 1000))
    export PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS=$PI_ENSEMBLE_VERIFY_TIMEOUT_MS
  fi
  # Agents get neither the host Docker socket nor SSH keys. Not a setting.
  export PI_ENSEMBLE_NO_DOCKER_SOCKET=1 PI_ENSEMBLE_NO_SSH=1

  for v in TEHAS_INDEXING TEHAS_WORK_IMAGES TEHAS_WORK_DESCRIBE TEHAS_WORK_FILES TEHAS_WORK_COMMENTS TEHAS_ANALYTICS_COOKIE TEHAS_TOP TEHAS_CONCURRENCY TEHAS_TICKET_CONCURRENCY TEHAS_HOST_ADDR TEHAS_MAX_AWAITING TEHAS_WORK_TIMEOUT TEHAS_WORK_STALL TEHAS_VERIFY_TIMEOUT TEHAS_PLAN_MAX_TURNS TEHAS_PLAN_TIMEOUT \
    TEHAS_PREVIEWS TEHAS_PREVIEW_MAX TEHAS_PREVIEW_MIN_FREE TEHAS_CYCLE_RESERVE; do
    reason=$(_tehas_valid "$v" "${!v}") || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  for v in TEHAS_BRANCH TEHAS_DOMAIN TEHAS_PROTOLAB_URL; do
    [ -n "${!v:-}" ] || continue
    reason=$(_tehas_valid "$v" "${!v}") || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  for v in TEHAS_ANALYTICS_SRC TEHAS_ANALYTICS_SITE TEHAS_IMPROVE_REPO TEHAS_IMPROVE_RUKAS_REPO TEHAS_IMPROVE_MAX_OPEN; do
    [ -n "${!v}" ] || continue
    reason=$(_tehas_valid "$v" "${!v}") || { tehas_err "$v is not valid: $reason (see $file)"; return 78; }
  done
  if [ -n "$TEHAS_ANALYTICS_SRC" ] && [ -z "$TEHAS_ANALYTICS_SITE" ]; then
    tehas_err "TEHAS_ANALYTICS_SITE is not set and TEHAS_ANALYTICS_SRC is (see $file)"
    return 78
  fi
  if [ -n "$TEHAS_IMPROVE_RUKAS_REPO" ] && [ -z "$TEHAS_IMPROVE_REPO" ]; then
    tehas_err "TEHAS_IMPROVE_REPO is not set and TEHAS_IMPROVE_RUKAS_REPO is (see $file)"
    return 78
  fi
  if [ "$TEHAS_WORK_DESCRIBE" = 1 ] && [ "$TEHAS_WORK_IMAGES" != 1 ]; then
    tehas_err "TEHAS_WORK_DESCRIBE is 1 and TEHAS_WORK_IMAGES is not: there are no images to describe (see $file)"
    return 78
  fi
  if [ "$TEHAS_WORK_COMMENTS" = 1 ] && [ "$TEHAS_WORK_IMAGES" != 1 ] && [ "$TEHAS_WORK_FILES" != 1 ]; then
    tehas_err "TEHAS_WORK_COMMENTS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is: nothing is taken from comments (see $file)"
    return 78
  fi
  if [ "$TEHAS_WORK_STALL" -le "$TEHAS_VERIFY_TIMEOUT" ]; then
    tehas_err "TEHAS_WORK_STALL ($TEHAS_WORK_STALL) is not longer than TEHAS_VERIFY_TIMEOUT ($TEHAS_VERIFY_TIMEOUT): a gate that prints nothing would be stopped as stalled (see $file)"
    return 78
  fi
  # tehas-cycle@.service stops a cycle after 8 hours (RuntimeMaxSec), and a
  # unit cannot read this file. A longer limit would have systemd stop every
  # long cycle; the hour between is for the stop and the report.
  if [ "$TEHAS_WORK_TIMEOUT" -gt 25200 ]; then
    tehas_err "TEHAS_WORK_TIMEOUT ($TEHAS_WORK_TIMEOUT) is more than 25200: tehas-cycle@.service stops a cycle after 8 hours, and the stop and the report need the hour after the limit (see $file)"
    return 78
  fi
  # tehas-dispatch.service stops a poll after 3 hours (TimeoutStartSec). A
  # poll runs one planning turn or refreshes the index, and calls GitHub.
  if [ "$TEHAS_PLAN_TIMEOUT" -gt 7200 ]; then
    tehas_err "TEHAS_PLAN_TIMEOUT ($TEHAS_PLAN_TIMEOUT) is more than 7200: tehas-dispatch.service stops a poll after 3 hours, and a poll needs time around its planning turn (see $file)"
    return 78
  fi
  if [ "$TEHAS_PREVIEWS" = 1 ] && [ -z "$TEHAS_DOMAIN" ]; then
    tehas_err "TEHAS_PREVIEWS is 1 and TEHAS_DOMAIN is not set: a preview has no address without it (see $file)"
    return 78
  fi
  export TEHAS_PREVIEW_MIN_FREE_MB
  TEHAS_PREVIEW_MIN_FREE_MB=$(_tehas_mem_mb "$TEHAS_PREVIEW_MIN_FREE")
  if [ -n "$TEHAS_DOMAIN" ] && [ -z "${TEHAS_PROTOLAB_URL:-}" ]; then
    export TEHAS_PROTOLAB_URL=https://lab.$TEHAS_DOMAIN
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

# tehas_edge_name <kind> <number>: the host name of a preview (uat) or of a
# Protolab ticket (lab) under TEHAS_DOMAIN. It is the one definition of such a
# name. The jq programs of libexec/protolab and libexec/preview put the same
# name together themselves, because jq cannot call a shell function, and
# test/protolab_test.sh and test/preview_test.sh pin that theirs is this one.
# Fails without a domain, for another kind, and for anything but a number of
# one to nine digits that does not start with a zero.
tehas_edge_name() {
  [ -n "${TEHAS_DOMAIN:-}" ] || return 1
  case "${1:-}" in uat | lab) ;; *) return 1 ;; esac
  [[ ${2:-} =~ ^[1-9][0-9]{0,8}$ ]] || return 1
  printf '%s-%s.%s\n' "$1" "$2" "$TEHAS_DOMAIN"
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
